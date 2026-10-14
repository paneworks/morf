//! HDR output on the GPU: what an sRGB frame becomes in a PQ ten-bit buffer
//! and in an scRGB half-float one.

use morf_layout::Geometry;
use morf_scene::Color;
use morf_value::present::Encoding;

use super::depth::{OutputDepth, PresentFormat, Transfer};
use super::depth_tests::ten_bit_reds;
use super::hdr::{self, DEFAULT_REFERENCE_WHITE, encode_pq};
use super::readback::half_to_f32;
use super::tests::test_quad;
use super::*;
use crate::*;

/// sRGB mid-grey, SDR white, and a white twice as bright, in linear light.
const LINEAR: [f64; 3] = [0.214_041_140_482_232_55, 1.0, 2.0];

/// A one-pixel column of each of `LINEAR`, then black.
fn columns(scene: &mut morf_scene::Scene) -> DrawList {
    let encoded = |linear: f64| -> f32 {
        // The sRGB curve, carried past 1.0 as `color_array` reads it back.
        (1.055 * linear.powf(1.0 / 2.4) - 0.055) as f32
    };
    let shades = LINEAR.map(encoded).into_iter().chain([0.0]);
    let commands = shades
        .enumerate()
        .map(|(column, value)| {
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

fn whole() -> [DamageRect; 1] {
    [DamageRect {
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    }]
}

fn hdr_backend(opaque: bool, encoding: Encoding) -> WgpuBackend {
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    backend.present_into_ring(2);
    backend.set_opaque_surface(opaque);
    assert!(backend.set_output_depth(OutputDepth::Ten));
    Transfer::force_encoding(&mut backend, encoding);
    backend
}

#[test]
#[ignore = "requires a GPU adapter"]
fn sdr_white_and_grey_land_on_their_pq_codes() {
    require_gpu!();
    let encoding = Encoding::Pq {
        reference_white: DEFAULT_REFERENCE_WHITE,
    };
    let mut backend = hdr_backend(true, encoding);
    assert_eq!(backend.present, PresentFormat::Ten);
    let mut scene = morf_scene::Scene::new();
    backend.render(&columns(&mut scene), &whole(), 120).unwrap();
    let index = backend.ring_presented().expect("a buffer was drawn");
    let reds = ten_bit_reds(&backend.ring_bytes(index)[..16]);
    let expected: Vec<u32> = LINEAR
        .iter()
        .chain(&[0.0])
        .map(|linear| {
            let signal = encode_pq([*linear; 3], f64::from(DEFAULT_REFERENCE_WHITE))[0];
            (signal * 1023.0).round() as u32
        })
        .collect();
    // SDR white at 203 cd/m² is code 594 of 1023, mid-grey about 438.
    assert!(expected[1].abs_diff(594) <= 1 && expected[0].abs_diff(438) <= 1);
    for (red, want) in reds.iter().zip(&expected) {
        assert!(red.abs_diff(*want) <= 2, "{reds:?} for {expected:?}");
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn scrgb_keeps_sdr_white_at_one() {
    require_gpu!();
    let mut backend = hdr_backend(false, Encoding::ExtLinear);
    assert_eq!(backend.present, PresentFormat::Half);
    let mut scene = morf_scene::Scene::new();
    backend.render(&columns(&mut scene), &whole(), 120).unwrap();
    let index = backend.ring_presented().expect("a buffer was drawn");
    let halves: Vec<f32> = backend.ring_bytes(index)[..32]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|half| half_to_f32(u16::from_le_bytes(*half)))
        .collect();
    let pixels = halves.as_chunks::<4>().0;
    for (pixel, want) in pixels.iter().zip(LINEAR.iter().chain(&[0.0])) {
        let want = *want as f32;
        assert!((pixel[0] - want).abs() <= want * 4e-3 + 1e-3, "{pixels:?}");
        assert_eq!(pixel[3], 1.0);
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn without_hdr_the_frame_stays_srgb() {
    require_gpu!();
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    backend.present_into_ring(2);
    backend.hdr = hdr::HdrRequest {
        enabled: true,
        reference_white: 300,
    };
    // No window to describe it to: deep, but sRGB.
    backend.set_output_depth(OutputDepth::Eight);
    assert!(backend.present.deep());
    assert_eq!(backend.encoding, Encoding::Srgb);
    assert_eq!(backend.transfer().mode, 1.0);
}
