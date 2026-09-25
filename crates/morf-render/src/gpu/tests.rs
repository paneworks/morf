use morf_layout::{Geometry, Transform2D};
use morf_scene::{Color, NodeHandle};

use super::glyphs::{ShelfAllocator, layer_mask_data};
use super::targets::{clamp_scissor, intersect_damage};
use super::textures::texture_placement;
use super::*;
use crate::*;

pub(crate) fn test_quad(
    node: NodeHandle,
    color: Color,
    border_color: Color,
    border_width: f64,
) -> DrawCommand {
    DrawCommand::Quad {
        node,
        bounds: Geometry {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
        },
        transform: Transform2D::IDENTITY,
        clip: None,
        color,
        color_overlay: Color::rgba8(0, 0, 0, 0),
        gradient: None,
        radii: [0.0; 4],
        border_width,
        antialiasing: false,
        border_pixel_aligned: true,
        border_color,
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_offset_x: 0.0,
        shadow_offset_y: 0.0,
        shadow_inner: false,
        shader: None,
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn srgb_target_preserves_hex_colors_and_blends_borders() {
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    let mut scene = morf_scene::Scene::new();
    let background = scene.create(morf_scene::Element::Rect);
    let border = scene.create(morf_scene::Element::Rect);
    let list = DrawList {
        commands: vec![
            test_quad(
                background,
                Color::rgba8(33, 34, 41, 255),
                Color::rgba8(0, 0, 0, 0),
                0.0,
            ),
            test_quad(
                border,
                Color::rgba8(0, 0, 0, 0),
                Color::rgba8(190, 198, 240, 20),
                1.0,
            ),
        ],
        layers: Vec::new(),
    };
    backend
        .render(
            &list,
            &[DamageRect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            }],
            120,
        )
        .unwrap();

    let bytes_per_row = 256;
    let buffer = backend.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("morf color test readback"),
        size: bytes_per_row * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = backend
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("morf color test copy"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &backend.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row as u32),
                rows_per_image: Some(4),
            },
        },
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    backend.queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (send, receive) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        send.send(result).unwrap()
    });
    backend
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let pixels = slice.get_mapped_range().unwrap();

    // The border pixel is a blend, so it is asserted to within one unit: the
    // exact byte depends on how the driver schedules the fragment arithmetic,
    // and it moved by one in green when the shader hook was added without any
    // change to what the shader computes. The centre is a solid fill, and that
    // one stays exact — it is the assertion that actually guards the colour
    // space, since a wrong transfer would move it by far more than one.
    let blended = &pixels[0..4];
    for (channel, expected) in blended.iter().zip([66u8, 69, 84, 255]) {
        assert!(
            channel.abs_diff(expected) <= 1,
            "blended border is {blended:?}, expected about [66, 69, 84, 255]",
        );
    }
    let center = 2 * bytes_per_row as usize + 2 * 4;
    assert_eq!(&pixels[center..center + 4], &[33, 34, 41, 255]);
}

#[test]
pub(crate) fn scissor_is_clamped_to_the_physical_target() {
    assert_eq!(
        clamp_scissor(
            DamageRect {
                x: 8,
                y: 9,
                width: 20,
                height: 20,
            },
            10,
            12,
        ),
        Some((8, 9, 2, 3))
    );
}

#[test]
pub(crate) fn damage_and_clip_scissors_are_intersected() {
    assert_eq!(
        intersect_damage(
            DamageRect {
                x: 0,
                y: 10,
                width: 40,
                height: 20,
            },
            DamageRect {
                x: 20,
                y: 0,
                width: 30,
                height: 20,
            },
        ),
        Some(DamageRect {
            x: 20,
            y: 10,
            width: 20,
            height: 10,
        })
    );
    assert_eq!(
        intersect_damage(
            DamageRect {
                x: 700,
                y: 0,
                width: 40,
                height: 40,
            },
            DamageRect {
                x: 0,
                y: 200,
                width: 100,
                height: 100,
            },
        ),
        None
    );
}

#[test]
pub(crate) fn texture_fit_and_crop_preserve_aspect_ratio() {
    let bounds = Geometry {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 100.0,
    };
    let fit = texture_placement(
        bounds,
        (200, 100),
        ImageFillMode::PreserveAspectFit,
        Transform2D::IDENTITY,
    );
    assert_eq!(
        fit.bounds,
        Geometry {
            x: 10.0,
            y: 45.0,
            width: 100.0,
            height: 50.0
        }
    );
    assert_eq!(fit.uv, [0.0, 0.0, 1.0, 1.0]);

    let crop = texture_placement(
        bounds,
        (200, 100),
        ImageFillMode::PreserveAspectCrop,
        Transform2D::IDENTITY,
    );
    assert_eq!(crop.bounds, bounds);
    assert_eq!(crop.logical_width, 200);
    assert_eq!(crop.logical_height, 100);
    assert_eq!(crop.uv, [0.25, 0.0, 0.5, 1.0]);
}

#[test]
pub(crate) fn layer_mask_data_inverts_the_owner_transform() {
    let mask = LayerMask {
        bounds: Geometry {
            x: 10.0,
            y: 20.0,
            width: 40.0,
            height: 30.0,
        },
        transform: Transform2D::around((10.0, 20.0), 2.0, 0.0),
        radii: [4.0, 5.0, 6.0, 7.0],
    };

    let (enabled, bounds, inverse_0, inverse_1, radii) = layer_mask_data(Some(mask));

    assert_eq!(enabled, 1.0);
    assert_eq!(bounds, [10.0, 20.0, 40.0, 30.0]);
    assert_eq!(inverse_0, [0.5, 0.0, 5.0, 0.0]);
    assert_eq!(inverse_1, [0.0, 0.5, 10.0, 0.0]);
    assert_eq!(radii, [4.0, 5.0, 6.0, 7.0]);
}

/// Renders `list` on a 4×4 target blending in `blend`, and reads the pixel at
/// (1, 1).
fn blended_pixel(blend: BlendSpace, list: &DrawList) -> [u8; 4] {
    let mut backend = pollster::block_on(WgpuBackend::new(4, 4)).unwrap();
    assert!(backend.set_blend(blend) || blend == BlendSpace::Linear);
    assert_eq!(backend.blend(), blend);
    backend
        .render(
            list,
            &[DamageRect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            }],
            120,
        )
        .unwrap();
    let pixels = backend.read_pixels();
    let at = (4 + 1) * 4;
    pixels[at..at + 4].try_into().unwrap()
}

fn close_to(pixel: [u8; 4], expected: [u8; 4]) -> bool {
    pixel
        .iter()
        .zip(expected)
        .all(|(channel, expected)| channel.abs_diff(expected) <= 2)
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn srgb_blending_mixes_encoded_values_as_browsers_do() {
    let mut scene = morf_scene::Scene::new();
    let ground = scene.create(morf_scene::Element::Rect);
    let veil = scene.create(morf_scene::Element::Rect);
    let clear = Color::rgba8(0, 0, 0, 0);
    let black = test_quad(ground, Color::rgba8(0, 0, 0, 255), clear, 0.0);
    // Half white, drawn straight over black.
    let direct = DrawList {
        commands: vec![
            black.clone(),
            test_quad(veil, Color::rgba8(255, 255, 255, 128), clear, 0.0),
        ],
        layers: Vec::new(),
    };
    // Opaque white in a half-opaque layer: the layer's target is composited
    // back without being encoded a second time.
    let layered = DrawList {
        commands: vec![
            black.clone(),
            test_quad(veil, Color::rgba8(255, 255, 255, 255), clear, 0.0),
        ],
        layers: vec![Layer {
            node: veil,
            commands: 1..2,
            parent: None,
            opacity: 128.0 / 255.0,
            blur: 0.0,
            shadow_color: clear,
            shadow_blur: 0.0,
            shadow_offset: [0.0; 2],
            mask: None,
            shader: None,
            bounds: Geometry {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
        }],
    };
    // An opaque colour lands on the same bytes whichever space blends.
    let solid = DrawList {
        commands: vec![test_quad(
            ground,
            Color::rgba8(33, 134, 241, 255),
            clear,
            0.0,
        )],
        layers: Vec::new(),
    };

    for list in [&direct, &layered] {
        let linear = blended_pixel(BlendSpace::Linear, list);
        let srgb = blended_pixel(BlendSpace::Srgb, list);
        assert!(close_to(linear, [188, 188, 188, 255]), "linear: {linear:?}");
        assert!(close_to(srgb, [128, 128, 128, 255]), "srgb: {srgb:?}");
    }
    for blend in [BlendSpace::Linear, BlendSpace::Srgb] {
        let pixel = blended_pixel(blend, &solid);
        assert!(close_to(pixel, [33, 134, 241, 255]), "{blend:?}: {pixel:?}");
    }
}

#[test]
pub(crate) fn glyph_shelves_reserve_padding_and_wrap_rows() {
    let mut allocator = ShelfAllocator::default();

    assert_eq!(allocator.allocate(1022, 10), Some((1, 1)));
    assert_eq!(allocator.allocate(1022, 20), Some((1025, 1)));
    assert_eq!(allocator.allocate(1, 1), Some((1, 23)));
}

#[test]
#[ignore = "requires a GPU adapter"]
fn backends_share_one_device_and_draw_side_by_side() {
    // Opened once for the process: every backend after the first, on any
    // thread, draws with the device the first one opened.
    let first = pollster::block_on(WgpuBackend::new(8, 8)).unwrap();
    let opened = crate::opened_device_count();
    let others = (0..4)
        .map(|index| {
            std::thread::spawn(move || {
                let mut backend = pollster::block_on(WgpuBackend::new(8, 8)).unwrap();
                let shade = 40 * (index + 1);
                let list = DrawList {
                    commands: vec![test_quad(
                        morf_scene::Scene::new().create(morf_scene::Element::Rect),
                        Color::rgba8(shade, 0, 0, 255),
                        Color::rgba8(0, 0, 0, 0),
                        0.0,
                    )],
                    layers: Vec::new(),
                };
                let pixels = crate::gpu::field_tests::read_frame(&mut backend, &list, 8);
                (backend.device.clone(), shade, pixels[0])
            })
        })
        .collect::<Vec<_>>();
    for other in others {
        let (device, shade, red) = other.join().unwrap();
        assert!(device == first.device, "the same device");
        // Each drew its own frame into its own target on the shared queue.
        assert!(
            (i32::from(red) - i32::from(shade)).abs() <= 24,
            "drew its own red: {red} for {shade}"
        );
    }
    assert_eq!(crate::opened_device_count(), opened, "and opened no other");
}
