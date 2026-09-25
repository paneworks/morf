//! Frosted glass inside one surface: the plan a backdrop blur follows, and a
//! CPU reference of the passes the GPU runs.
//!
//! The blur is dual Kawase. The region beneath the glass is halved `levels`
//! times, each halving a five-tap average reaching `offset` texels of the level
//! it reads, and then doubled back up to half the region's resolution with the
//! eight-tap kernel. Work falls by four at every level, so a wide radius costs
//! little more than a narrow one; the final half-resolution picture is
//! stretched over the glass, which a blur cannot tell from full resolution.

/// The deepest a blur halves its region: at 96 logical pixels on a scale-2
/// output this is still one pass over a thirty-second of the region.
pub(crate) const MAX_LEVELS: u32 = 6;

/// How a blur of `radius` physical pixels is done: how many times the region
/// is halved, and how far each pass reaches in texels of the level it reads.
///
/// `radius` is the standard deviation of the Gaussian the result stands in
/// for, as in CSS `blur()`. What each chain spreads an impulse to was
/// measured with [`reference_blur`] ([`SPREAD`], checked by the
/// `backdrop_plan_matches_the_asked_radius` test), so the plan takes the
/// shallowest chain that reaches the radius without stretching its taps past
/// one and a half texels — further, and they start to leave gaps — and reads
/// the reach off the measurements.
pub(crate) fn backdrop_plan(radius: f64) -> (u32, f32) {
    let radius = radius.max(0.0);
    if radius < spread(2, 0) * 0.6 {
        // Below what two levels can do at their shortest reach; one halving
        // is already a blur of about a pixel.
        return (1, 1.0);
    }
    let mut levels = 2;
    while levels < MAX_LEVELS && spread(levels, 2) < radius {
        levels += 1;
    }
    let offset = if radius <= spread(levels, 0) {
        OFFSETS[0]
    } else if radius >= spread(levels, 3) {
        OFFSETS[3]
    } else {
        let step = (0..3)
            .find(|&index| radius <= spread(levels, index + 1))
            .unwrap_or(2);
        let (low, high) = (spread(levels, step), spread(levels, step + 1));
        let fraction = (radius - low) / (high - low);
        OFFSETS[step] + (OFFSETS[step + 1] - OFFSETS[step]) * fraction as f32
    };
    (levels, offset)
}

/// The reaches [`SPREAD`] was measured at, in texels.
const OFFSETS: [f32; 4] = [0.5, 1.0, 1.5, 2.0];

/// The deviation, in pixels of the region, that two, three and four levels
/// spread an impulse to at each of [`OFFSETS`]: measured with
/// [`reference_blur`] on a 513-pixel square. A level deeper doubles it.
const SPREAD: [[f64; 4]; 3] = [
    [3.71, 5.83, 8.22, 11.07],
    [8.05, 12.87, 18.26, 24.52],
    [16.40, 26.31, 37.39, 49.84],
];

/// What `levels` levels spread an impulse to at the `offset`-th reach.
fn spread(levels: u32, offset: usize) -> f64 {
    let levels = levels.max(2);
    let row = (levels as usize - 2).min(2);
    SPREAD[row][offset] * f64::from(1_u32 << (levels - 2 - row as u32))
}

/// The size of each level of a region's chain: level 0 is the region itself.
pub(crate) fn level_sizes(width: u32, height: u32, levels: u32) -> Vec<(u32, u32)> {
    let mut sizes = vec![(width.max(1), height.max(1))];
    for _ in 0..levels {
        let (w, h) = *sizes.last().expect("level 0 is always there");
        sizes.push(((w / 2).max(1), (h / 2).max(1)));
    }
    sizes
}

/// Which level each pass reads and which it writes, in order: down to the
/// deepest level and back up to level 1.
pub(crate) fn pass_order(levels: u32) -> Vec<(usize, usize)> {
    let levels = levels as usize;
    let mut passes: Vec<(usize, usize)> = (0..levels).map(|level| (level, level + 1)).collect();
    passes.extend((1..levels).rev().map(|level| (level + 1, level)));
    passes
}

/// One channel of a picture, for the reference.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plane {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: Vec<f32>,
}

#[cfg(test)]
impl Plane {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0.0; width * height],
        }
    }

    pub(crate) fn at(&self, x: usize, y: usize) -> f32 {
        self.pixels[y * self.width + x]
    }

    /// Bilinear, clamped at the edges, with texel centres at half-integers:
    /// what a linear sampler with clamp-to-edge addressing returns.
    pub(crate) fn sample(&self, u: f32, v: f32) -> f32 {
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        let clamp_x = |value: f32| (value.max(0.0) as usize).min(self.width - 1);
        let clamp_y = |value: f32| (value.max(0.0) as usize).min(self.height - 1);
        let (left, right) = (clamp_x(x0), clamp_x(x0 + 1.0));
        let (top, bottom) = (clamp_y(y0), clamp_y(y0 + 1.0));
        let row = |y: usize| self.at(left, y) * (1.0 - fx) + self.at(right, y) * fx;
        row(top) * (1.0 - fy) + row(bottom) * fy
    }
}

/// One pass of `blur.wgsl`, into a target of `size`.
#[cfg(test)]
pub(crate) fn reference_pass(source: &Plane, size: (usize, usize), offset: f32, up: bool) -> Plane {
    let mut target = Plane::new(size.0, size.1);
    let step_x = offset / source.width as f32;
    let step_y = offset / source.height as f32;
    for y in 0..size.1 {
        for x in 0..size.0 {
            let u = (x as f32 + 0.5) / size.0 as f32;
            let v = (y as f32 + 0.5) / size.1 as f32;
            let at = |dx: f32, dy: f32| source.sample(u + dx * step_x, v + dy * step_y);
            let value = if up {
                (at(-1.0, -1.0)
                    + at(1.0, -1.0)
                    + at(-1.0, 1.0)
                    + at(1.0, 1.0)
                    + (at(-2.0, 0.0) + at(2.0, 0.0) + at(0.0, -2.0) + at(0.0, 2.0)) * 2.0)
                    / 12.0
            } else {
                (at(0.0, 0.0) * 4.0 + at(-1.0, -1.0) + at(1.0, -1.0) + at(-1.0, 1.0) + at(1.0, 1.0))
                    / 8.0
            };
            target.pixels[y * size.0 + x] = value;
        }
    }
    target
}

/// The whole chain on the CPU: what the GPU leaves at level 1 for `plane`.
#[cfg(test)]
pub(crate) fn reference_blur(plane: &Plane, levels: u32, offset: f32) -> Plane {
    let sizes = level_sizes(plane.width as u32, plane.height as u32, levels);
    let mut planes: Vec<Plane> = vec![plane.clone()];
    planes.resize(sizes.len(), Plane::new(1, 1));
    for (from, to) in pass_order(levels) {
        let size = (sizes[to].0 as usize, sizes[to].1 as usize);
        planes[to] = reference_pass(&planes[from], size, offset, to < from);
    }
    planes.swap_remove(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deviation of a blurred impulse along x, in pixels of the region.
    fn impulse_spread(levels: u32, offset: f32) -> f64 {
        let size = 513;
        let mut plane = Plane::new(size, size);
        plane.pixels[(size / 2) * size + size / 2] = 1.0;
        let blurred = reference_blur(&plane, levels, offset);
        // Back at the region's resolution: each texel of level 1 is two
        // pixels wide.
        let scale = size as f64 / blurred.width as f64;
        let (mut total, mut mean, mut square) = (0.0, 0.0, 0.0);
        for y in 0..blurred.height {
            for x in 0..blurred.width {
                let weight = f64::from(blurred.at(x, y));
                let position = (x as f64 + 0.5) * scale;
                total += weight;
                mean += weight * position;
                square += weight * position * position;
            }
        }
        mean /= total;
        (square / total - mean * mean).sqrt()
    }

    #[test]
    fn a_flat_backdrop_stays_flat() {
        let mut plane = Plane::new(40, 30);
        plane.pixels.fill(0.6);
        for levels in 1..=4 {
            let blurred = reference_blur(&plane, levels, 1.5);
            assert_eq!((blurred.width, blurred.height), (20, 15));
            for value in &blurred.pixels {
                assert!((value - 0.6).abs() < 1e-5, "{levels}: {value}");
            }
        }
    }

    #[test]
    fn the_chain_keeps_the_light_it_was_given() {
        // Away from the edges nothing is clamped, so the kernel sums to one
        // and the picture keeps its total brightness at every depth.
        let size = 256;
        let mut plane = Plane::new(size, size);
        plane.pixels[128 * size + 128] = 1.0;
        for levels in 1..=3 {
            let blurred = reference_blur(&plane, levels, 1.0);
            let total: f32 = blurred.pixels.iter().sum();
            // Level 1 has a quarter of the pixels, each worth four.
            assert!((total * 4.0 - 1.0).abs() < 1e-3, "{levels}: {total}");
        }
    }

    #[test]
    fn backdrop_plan_matches_the_asked_radius() {
        // The table the plan is built on, measured again.
        for levels in 2..=3 {
            for (index, offset) in OFFSETS.iter().enumerate() {
                let measured = impulse_spread(levels, *offset);
                let modelled = spread(levels, index);
                assert!(
                    (measured - modelled).abs() / modelled < 0.02,
                    "{levels} levels at {offset} spread {measured:.2}, the table says {modelled:.2}"
                );
            }
        }
        // And what that buys: the plan lands within a tenth of the radius
        // asked for across the range a panel uses.
        for radius in [4.0, 8.0, 12.0, 20.0, 32.0, 48.0] {
            let (levels, offset) = backdrop_plan(radius);
            let measured = impulse_spread(levels, offset);
            assert!(
                (measured - radius).abs() / radius < 0.1,
                "radius {radius}: {levels} levels at {offset} spread {measured:.2}"
            );
        }
    }

    #[test]
    fn a_wider_radius_halves_deeper_rather_than_reaching_further() {
        let (shallow, near) = backdrop_plan(6.0);
        let (deep, far) = backdrop_plan(60.0);
        assert!(deep > shallow);
        assert!(near <= 2.0 && far <= 2.0);
        assert!(backdrop_plan(1e9).0 == MAX_LEVELS);
    }

    #[test]
    fn passes_go_down_and_come_back_to_half_resolution() {
        assert_eq!(pass_order(1), vec![(0, 1)]);
        assert_eq!(pass_order(3), vec![(0, 1), (1, 2), (2, 3), (3, 2), (2, 1)]);
        assert_eq!(
            level_sizes(100, 7, 3),
            vec![(100, 7), (50, 3), (25, 1), (12, 1)]
        );
    }
}
