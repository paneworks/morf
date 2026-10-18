//! Deep output on the GPU: what reaches a ten-bit or half-float buffer keeps
//! the steps eight bits would merge.

use morf_layout::Geometry;
use morf_scene::Color;

use super::depth::{OutputDepth, PresentFormat};
use super::readback::half_to_f32;
use super::tests::test_quad;
use super::*;
use crate::*;

/// Four one-pixel columns, the grey of each a step of 1/1023 apart from the
/// next: eight bits cannot tell them apart.
pub(crate) const STEPS: [u32; 4] = [512, 513, 514, 515];

pub(crate) fn columns(scene: &mut morf_scene::Scene) -> DrawList {
    let commands = STEPS
        .iter()
        .enumerate()
        .map(|(column, step)| {
            let value = *step as f32 / 1023.0;
            let color = Color {
                red: value,
                green: value,
                blue: value,
                alpha: 1.0,
            };
            let node = scene.create(morf_scene::Element::Rect);
            let mut quad = test_quad(node, color, Color::rgba8(0, 0, 0, 0), 0.0);
            if let DrawCommand::Quad { bounds, .. } = &mut quad {
                *bounds = Geometry {
                    x: column as f64,
                    y: 0.0,
                    width: 1.0,
                    height: 4.0,
                };
            }
            quad
        })
        .collect();
    DrawList {
        commands,
        layers: Vec::new(),
    }
}

/// The red channel of each pixel of a row of `XBGR2101010` words.
pub(crate) fn ten_bit_reds(row: &[u8]) -> Vec<u32> {
    row.as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes(*word) & 0x3ff)
        .collect()
}

fn whole() -> [DamageRect; 1] {
    [DamageRect {
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    }]
}

fn deep_backend(blend: BlendSpace, opaque: bool) -> WgpuBackend {
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    backend.set_blend(blend);
    backend.present_into_ring(2);
    backend.set_opaque_surface(opaque);
    assert!(backend.set_output_depth(OutputDepth::Ten));
    assert_eq!(backend.output_depth(), OutputDepth::Ten);
    backend
}

#[test]
#[ignore = "requires a GPU adapter"]
fn an_opaque_surface_presents_ten_bits_a_channel() {
    require_gpu!();
    for blend in [BlendSpace::Srgb, BlendSpace::Linear] {
        let mut backend = deep_backend(blend, true);
        assert_eq!(backend.present, PresentFormat::Ten);
        let mut scene = morf_scene::Scene::new();
        backend.render(&columns(&mut scene), &whole(), 120).unwrap();
        let index = backend.ring_presented().expect("a buffer was drawn");
        let reds = ten_bit_reds(&backend.ring_bytes(index)[..16]);
        for (red, step) in reds.iter().zip(STEPS) {
            assert!(red.abs_diff(step) <= 1, "{blend:?}: {reds:?} for {STEPS:?}");
        }
        let mut distinct = reds.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "{blend:?}: steps merged: {reds:?}");
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn a_translucent_surface_presents_half_floats_with_alpha() {
    require_gpu!();
    let mut backend = deep_backend(BlendSpace::Linear, false);
    assert_eq!(backend.present, PresentFormat::Half);
    let mut scene = morf_scene::Scene::new();
    backend.render(&columns(&mut scene), &whole(), 120).unwrap();
    let index = backend.ring_presented().expect("a buffer was drawn");
    let bytes = backend.ring_bytes(index);
    let halves: Vec<f32> = bytes[..32]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|half| half_to_f32(u16::from_le_bytes(*half)))
        .collect();
    for (pixel, step) in halves.as_chunks::<4>().0.iter().zip(STEPS) {
        let expected = step as f32 / 1023.0;
        assert!((pixel[0] - expected).abs() < 1.0 / 1023.0, "{pixel:?}");
        assert_eq!(pixel[3], 1.0);
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn eight_bits_stay_eight_bits() {
    require_gpu!();
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    backend.present_into_ring(2);
    assert!(!backend.set_output_depth(OutputDepth::Eight));
    assert_eq!(backend.present, PresentFormat::Eight);
    assert_eq!(
        backend.texture().format(),
        wgpu::TextureFormat::Rgba8UnormSrgb
    );
}
