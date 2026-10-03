use std::path::Path;

use crate::image_cache::{ImageError, decode_path, normalize_source, source_dimensions};

/// Decoded straight-alpha RGBA pixels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageData {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Row-major RGBA8 pixels.
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Extracts up to `2^depth` prevalent opaque colors from an image.
pub fn quantize_colors(
    source: impl AsRef<Path>,
    depth: u8,
    crop: Option<ImageRect>,
    rescale_size: u32,
) -> Result<Vec<[u8; 4]>, ImageError> {
    if depth > 8 || rescale_size > 512 {
        return Err(ImageError::InvalidSize);
    }
    let source = normalize_source(source.as_ref())?;
    let (intrinsic_width, intrinsic_height) = source_dimensions(&source)?;
    if intrinsic_width == 0
        || intrinsic_height == 0
        || u64::from(intrinsic_width) * u64::from(intrinsic_height) > 16_777_216
    {
        return Err(ImageError::InvalidSize);
    }
    let target = if rescale_size == 0 {
        (intrinsic_width, intrinsic_height)
    } else if intrinsic_width >= intrinsic_height {
        (
            rescale_size,
            (u64::from(intrinsic_height) * u64::from(rescale_size) / u64::from(intrinsic_width))
                .max(1) as u32,
        )
    } else {
        (
            (u64::from(intrinsic_width) * u64::from(rescale_size) / u64::from(intrinsic_height))
                .max(1) as u32,
            rescale_size,
        )
    };
    let image = decode_path(&source, target.0, target.1)?;
    let crop = crop.map(|crop| ImageRect {
        x: (u64::from(crop.x) * u64::from(target.0) / u64::from(intrinsic_width)) as u32,
        y: (u64::from(crop.y) * u64::from(target.1) / u64::from(intrinsic_height)) as u32,
        width: (u64::from(crop.width) * u64::from(target.0) / u64::from(intrinsic_width)).max(1)
            as u32,
        height: (u64::from(crop.height) * u64::from(target.1) / u64::from(intrinsic_height)).max(1)
            as u32,
    });
    quantize_image(&image, depth, crop)
}

pub(crate) fn quantize_image(
    image: &ImageData,
    depth: u8,
    crop: Option<ImageRect>,
) -> Result<Vec<[u8; 4]>, ImageError> {
    let crop = crop.unwrap_or(ImageRect {
        x: 0,
        y: 0,
        width: image.width,
        height: image.height,
    });
    let right = crop.x.saturating_add(crop.width).min(image.width);
    let bottom = crop.y.saturating_add(crop.height).min(image.height);
    if crop.x >= right || crop.y >= bottom {
        return Err(ImageError::InvalidSize);
    }
    let mut pixels = Vec::with_capacity(((right - crop.x) * (bottom - crop.y)) as usize);
    for y in crop.y..bottom {
        for x in crop.x..right {
            let offset = ((y * image.width + x) * 4) as usize;
            let pixel = &image.rgba[offset..offset + 4];
            if pixel[3] != 0 {
                pixels.push([pixel[0], pixel[1], pixel[2], pixel[3]]);
            }
        }
    }
    if pixels.is_empty() {
        return Ok(Vec::new());
    }
    let mut buckets = vec![pixels];
    for _ in 0..depth {
        let mut next = Vec::with_capacity(buckets.len() * 2);
        for mut bucket in buckets {
            if bucket.len() < 2 {
                next.push(bucket);
                continue;
            }
            let channel = widest_channel(&bucket);
            bucket.sort_unstable_by_key(|pixel| pixel[channel]);
            let second = bucket.split_off(bucket.len() / 2);
            next.push(bucket);
            next.push(second);
        }
        buckets = next;
    }
    Ok(buckets
        .into_iter()
        .filter(|bucket| !bucket.is_empty())
        .map(|bucket| {
            let mut sums = [0_u64; 4];
            for pixel in &bucket {
                for channel in 0..4 {
                    sums[channel] += u64::from(pixel[channel]);
                }
            }
            let length = bucket.len() as u64;
            [
                (sums[0] / length) as u8,
                (sums[1] / length) as u8,
                (sums[2] / length) as u8,
                (sums[3] / length) as u8,
            ]
        })
        .collect())
}

fn widest_channel(pixels: &[[u8; 4]]) -> usize {
    let mut minimum = [u8::MAX; 3];
    let mut maximum = [u8::MIN; 3];
    for pixel in pixels {
        for channel in 0..3 {
            minimum[channel] = minimum[channel].min(pixel[channel]);
            maximum[channel] = maximum[channel].max(pixel[channel]);
        }
    }
    (0..3)
        .max_by_key(|channel| maximum[*channel] - minimum[*channel])
        .unwrap_or(0)
}

/// One colour of a palette, and how much of the picture it covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaletteEntry {
    /// The colour, straight RGBA.
    pub rgba: [u8; 4],
    /// Its share of the picture's visible pixels, 0..1.
    pub fraction: f64,
}

/// Up to `count` dominant colours of an image, most common first.
///
/// Median cut alone — what [`quantize_colors`] does — splits the pixels into
/// buckets of *equal* size, so every colour it names covers the same share of
/// the picture and "dominant" means nothing. Its buckets are good starting
/// points, though, so they seed a few rounds of k-means, and each colour then
/// owns the pixels actually nearest to it. That count is what the order and
/// the fractions come from.
///
/// Twice as many seeds as colours asked for, then the least common dropped:
/// a small accent colour survives when it is distinct, rather than being
/// averaged into its neighbour because the split happened to fall there.
pub fn palette_of(image: &ImageData, count: usize) -> Vec<PaletteEntry> {
    let count = count.clamp(1, 64);
    let pixels: Vec<[u8; 4]> = image
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[3] >= 128)
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .collect();
    if pixels.is_empty() {
        return Vec::new();
    }
    let depth = (count * 2).next_power_of_two().trailing_zeros().min(8) as u8;
    let mut centres: Vec<[f64; 3]> = quantize_image(image, depth, None)
        .unwrap_or_default()
        .into_iter()
        .map(|colour| [colour[0].into(), colour[1].into(), colour[2].into()])
        .collect();
    if centres.is_empty() {
        return Vec::new();
    }
    let mut owners = vec![0usize; pixels.len()];
    for _ in 0..6 {
        let mut sums = vec![[0.0f64; 4]; centres.len()];
        for (pixel, owner) in pixels.iter().zip(owners.iter_mut()) {
            *owner = nearest(&centres, pixel);
            let sum = &mut sums[*owner];
            sum[0] += f64::from(pixel[0]);
            sum[1] += f64::from(pixel[1]);
            sum[2] += f64::from(pixel[2]);
            sum[3] += 1.0;
        }
        for (centre, sum) in centres.iter_mut().zip(&sums) {
            if sum[3] > 0.0 {
                *centre = [sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3]];
            }
        }
    }
    let mut population = vec![0usize; centres.len()];
    for owner in &owners {
        population[*owner] += 1;
    }
    let total = pixels.len() as f64;
    let mut entries: Vec<PaletteEntry> = centres
        .iter()
        .zip(&population)
        .filter(|(_, count)| **count > 0)
        .map(|(centre, count)| PaletteEntry {
            rgba: [
                centre[0].round() as u8,
                centre[1].round() as u8,
                centre[2].round() as u8,
                255,
            ],
            fraction: *count as f64 / total,
        })
        .collect();
    entries.sort_by(|a, b| b.fraction.total_cmp(&a.fraction));
    entries.truncate(count);
    entries
}

fn nearest(centres: &[[f64; 3]], pixel: &[u8; 4]) -> usize {
    let mut best = (0, f64::MAX);
    for (index, centre) in centres.iter().enumerate() {
        let distance = (centre[0] - f64::from(pixel[0])).powi(2)
            + (centre[1] - f64::from(pixel[1])).powi(2)
            + (centre[2] - f64::from(pixel[2])).powi(2);
        if distance < best.1 {
            best = (index, distance);
        }
    }
    best.0
}
