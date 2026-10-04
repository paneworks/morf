//! Frosted glass on a real adapter: it blurs what the surface drew beneath
//! it, matches the CPU reference of the chain, keeps its corners, and blurs
//! again only when what is beneath changes.
//!
//! Run under a Vulkan driver: `nixVulkanIntel cargo test -p morf-render --lib
//! backdrop -- --ignored`.

use morf_layout::{Layout, Size};
use morf_scene::{Element, NodeHandle, Scene};

use super::field_tests::read_frame;
use crate::backdrop::{Plane, backdrop_plan, reference_blur};
use crate::tests::NoText;
use crate::*;

const SIZE: u32 = 64;

struct Desk {
    scene: Scene,
    root: NodeHandle,
    stripes: Vec<NodeHandle>,
    glass: NodeHandle,
    /// A dot drawn over the glass.
    above: NodeHandle,
}

/// Four-pixel black and white stripes across the whole surface, and a
/// transparent 32-square glass with 8-pixel corners at (16, 16) blurring them
/// at `radius`, under a small dot. `wrap` puts the glass inside a parent that
/// makes it a layer, as a panel's rounded clip or fade does.
fn desk(radius: f64, wrap: Option<&str>) -> Desk {
    let mut scene = Scene::new();
    let root = scene.create(Element::Item);
    scene.assign(root, "width", f64::from(SIZE)).unwrap();
    scene.assign(root, "height", f64::from(SIZE)).unwrap();
    let mut stripes = Vec::new();
    for index in 0..SIZE / 4 {
        let stripe = scene.create(Element::Rect);
        scene.assign(stripe, "x", f64::from(index * 4)).unwrap();
        scene.assign(stripe, "width", 4.0).unwrap();
        scene.assign(stripe, "height", f64::from(SIZE)).unwrap();
        let color = if index % 2 == 0 {
            "#000000ff"
        } else {
            "#ffffffff"
        };
        scene.assign(stripe, "color", color).unwrap();
        scene.reparent(stripe, Some(root)).unwrap();
        stripes.push(stripe);
    }
    let parent = match wrap {
        None => root,
        Some(kind) => {
            let parent = scene.create(Element::ClipRect);
            scene.assign(parent, "width", f64::from(SIZE)).unwrap();
            scene.assign(parent, "height", f64::from(SIZE)).unwrap();
            scene.assign(parent, "color", "#00000000").unwrap();
            match kind {
                "clip" => scene.assign(parent, "radius", 2.0).unwrap(),
                _ => scene.assign(parent, "opacity", 0.999).unwrap(),
            }
            scene.reparent(parent, Some(root)).unwrap();
            parent
        }
    };
    let glass = scene.create(Element::Rect);
    for (property, value) in [
        ("x", 16.0),
        ("y", 16.0),
        ("width", 32.0),
        ("height", 32.0),
        ("radius", 8.0),
        ("backdrop_blur", radius),
    ] {
        scene.assign(glass, property, value).unwrap();
    }
    scene.assign(glass, "color", "#00000000").unwrap();
    scene.reparent(glass, Some(parent)).unwrap();
    let above = scene.create(Element::Rect);
    for (property, value) in [("x", 30.0), ("y", 30.0), ("width", 2.0), ("height", 2.0)] {
        scene.assign(above, property, value).unwrap();
    }
    scene.assign(above, "color", "#ff0000ff").unwrap();
    scene.reparent(above, Some(parent)).unwrap();
    Desk {
        scene,
        root,
        stripes,
        glass,
        above,
    }
}

fn list(desk: &Desk) -> DrawList {
    let layout = Layout::compute(
        &desk.scene,
        desk.root,
        Size {
            width: f64::from(SIZE),
            height: f64::from(SIZE),
        },
        &mut NoText,
    )
    .unwrap();
    DrawList::from_scene(&desk.scene, &layout).unwrap()
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * SIZE + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn backend() -> WgpuBackend {
    pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap()
}

fn encode(linear: f32) -> f32 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn glass_blurs_the_stripes_beneath_it_and_nothing_else() {
    let desk = desk(4.0, None);
    let pixels = read_frame(&mut backend(), &list(&desk), SIZE);
    // Inside, the stripes have run together into a grey.
    let middle = at(&pixels, 24, 36);
    assert!(
        (60..=220).contains(&middle[0]),
        "inside the glass is grey: {middle:?}"
    );
    // Outside, and in the corner the radius cuts away, they are untouched.
    for (x, y) in [(2, 2), (18, 60), (16, 16)] {
        let pixel = at(&pixels, x, y);
        assert!(
            pixel[0] < 5 || pixel[0] > 250,
            "({x}, {y}) is a stripe: {pixel:?}"
        );
    }
    // The dot over the glass is drawn over it, not blurred into it.
    assert_eq!(at(&pixels, 30, 30), [255, 0, 0, 255]);
}

#[test]
#[ignore = "requires a GPU adapter"]
fn glass_matches_the_cpu_reference_of_the_chain() {
    let radius = 6.0;
    let desk = desk(radius, None);
    let pixels = read_frame(&mut backend(), &list(&desk), SIZE);
    // The region read beneath: the glass widened by twice the radius, cut
    // at the surface's edges.
    let (left, top) = (16 - 12, 16 - 12);
    let (width, height) = (32 + 24, 32 + 24);
    let mut plane = Plane::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let white = ((x + left) / 4) % 2 == 1;
            plane.pixels[y * width + x] = if white { 1.0 } else { 0.0 };
        }
    }
    let (levels, offset) = backdrop_plan(radius);
    let blurred = reference_blur(&plane, levels, offset);
    let mut worst = 0.0_f32;
    for y in 22..42 {
        for x in 20..44 {
            if (29..33).contains(&x) && (29..33).contains(&y) {
                continue; // the dot
            }
            let u = (x as f32 + 0.5 - left as f32) / width as f32;
            let v = (y as f32 + 0.5 - top as f32) / height as f32;
            let expected = encode(blurred.sample(u, v)) * 255.0;
            let actual = f32::from(at(&pixels, x, y)[0]);
            worst = worst.max((expected - actual).abs());
        }
    }
    // Eight-bit levels between passes and the driver's filtering precision
    // leave a few units; a wrong plan or a wrong region leaves tens.
    assert!(worst <= 6.0, "worst difference from the reference: {worst}");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn glass_inside_a_layer_still_sees_the_surface_beneath() {
    for wrap in ["clip", "fade"] {
        let desk = desk(4.0, Some(wrap));
        let pixels = read_frame(&mut backend(), &list(&desk), SIZE);
        let middle = at(&pixels, 24, 36);
        assert!(
            (60..=220).contains(&middle[0]) && middle[3] == 255,
            "{wrap}: inside the glass is an opaque grey: {middle:?}"
        );
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn glass_blurs_again_only_when_what_is_beneath_changes() {
    let mut desk = desk(4.0, None);
    let mut backend = backend();
    read_frame(&mut backend, &list(&desk), SIZE);
    assert_eq!(backend.backdrop_blurs(), 1, "the first frame blurs");
    read_frame(&mut backend, &list(&desk), SIZE);
    assert_eq!(backend.backdrop_blurs(), 1, "a still frame does not");

    // Something over the glass moves: the glass is drawn again, from what
    // it already has.
    desk.scene.assign(desk.above, "x", 34.0).unwrap();
    let pixels = read_frame(&mut backend, &list(&desk), SIZE);
    assert_eq!(backend.backdrop_blurs(), 1, "a change above does not");
    assert_eq!(at(&pixels, 34, 30), [255, 0, 0, 255]);

    // Something beneath it changes: it blurs again, and shows it.
    let before = at(&pixels, 24, 36);
    for stripe in &desk.stripes {
        desk.scene.assign(*stripe, "color", "#ffffffff").unwrap();
    }
    let pixels = read_frame(&mut backend, &list(&desk), SIZE);
    assert_eq!(backend.backdrop_blurs(), 2, "a change beneath does");
    let after = at(&pixels, 24, 36);
    assert!(after[0] > 250 && before[0] < 220, "{before:?} -> {after:?}");

    // Far outside its reach, nothing it reads has changed.
    desk.scene
        .assign(desk.stripes[15], "color", "#000000ff")
        .unwrap();
    read_frame(&mut backend, &list(&desk), SIZE);
    assert_eq!(
        backend.backdrop_blurs(),
        2,
        "a change out of reach does not"
    );

    // The glass itself moving is a new region.
    desk.scene.assign(desk.glass, "y", 12.0).unwrap();
    read_frame(&mut backend, &list(&desk), SIZE);
    assert_eq!(backend.backdrop_blurs(), 3, "a moved glass does");
}

#[test]
#[ignore = "requires a GPU adapter"]
fn saturation_greys_the_backdrop() {
    let mut desk = desk(2.0, None);
    for (index, stripe) in desk.stripes.iter().enumerate() {
        let color = if index % 2 == 0 {
            "#ff0000ff"
        } else {
            "#cc0000ff"
        };
        desk.scene.assign(*stripe, "color", color).unwrap();
    }
    desk.scene
        .assign(desk.glass, "backdrop_saturation", 0.0)
        .unwrap();
    let pixels = read_frame(&mut backend(), &list(&desk), SIZE);
    let grey = at(&pixels, 24, 36);
    assert!(
        grey[0].abs_diff(grey[1]) <= 2 && grey[1].abs_diff(grey[2]) <= 2,
        "no saturation leaves grey: {grey:?}"
    );
}
