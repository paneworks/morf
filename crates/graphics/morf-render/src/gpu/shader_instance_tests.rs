//! Several nodes wearing one shader, each with its own values, on an adapter.
//!
//! A program's pipeline is shared and its parameters are not. Every buffer
//! write lands before the frame's commands run, so a block shared between
//! nodes draws all of them with whichever was written last: the bug these
//! guard against came out as a row of spectrum bars all the height of the
//! last one.

use crate::gpu::backend_types::ShaderRegistration;
use crate::gpu::field_tests::{alpha_at, field_command, field_layer, read_back, read_frame};
use crate::gpu::shader_tests::{SIZE, channel};
use crate::*;
use morf_layout::Geometry;
use morf_scene::{Color, NodeHandle};
use morf_shader::{ShaderKind, ShaderSpec};

/// Left and right halves of the target, and a point inside each.
const LEFT: (f64, u32) = (0.0, 16);
const RIGHT: (f64, u32) = (32.0, 48);
const MIDDLE: u32 = 16;

/// Compiles a shader with one parameter, `level`, and one data block of four,
/// `values`, and registers it.
fn register(backend: &mut WgpuBackend, kind: ShaderKind, body: &str) -> u64 {
    let spec = ShaderSpec {
        kind,
        inputs: ShaderSpec::default_inputs(kind),
        params: vec![morf_shader::Binding {
            name: "level".to_owned(),
            ty: morf_shader::Type::F32,
        }],
        entry: "fragment".to_owned(),
        textures: Vec::new(),
        data: vec![("values".to_owned(), morf_shader::Type::F32, 4)],
        vertex: false,
    };
    let compiled = morf_shader::compile(body, &spec)
        .unwrap_or_else(|errors| panic!("{}", morf_shader::report("test", &errors)));
    let offsets: Vec<u32> = compiled.params.iter().map(|slot| slot.offset).collect();
    backend
        .register_shader(ShaderRegistration {
            program: compiled.hash,
            wgsl: Some(&compiled.wgsl),
            vertex: None,
            offsets: &offsets,
            uniform_size: compiled.uniform_size,
            owns_coverage: false,
            effect: kind == ShaderKind::Effect,
            textures: &[],
            data: &[("values".to_owned(), 4)],
        })
        .expect("the generated WGSL compiles");
    compiled.hash
}

fn binding(program: u64, level: f32, value: f32, effect: bool) -> ShaderBinding {
    ShaderBinding {
        program,
        params: vec![level],
        data: vec![vec![value, 0.0, 0.0, 0.0]],
        samples_behind: effect,
        owns_coverage: false,
    }
}

/// A square filling most of one half of the target, wearing `shader`.
fn square(node: NodeHandle, left: f64, shader: Option<ShaderBinding>) -> DrawCommand {
    let mut command = field_command(node, vec![field_layer(left + 4.0, 4.0, 24.0, Shape::Box)]);
    if let DrawCommand::Field {
        bounds, shader: on, ..
    } = &mut command
    {
        *bounds = half(left);
        *on = shader;
    }
    command
}

fn half(left: f64) -> Geometry {
    Geometry {
        x: left,
        y: 0.0,
        width: SIZE as f64 / 2.0,
        height: SIZE as f64,
    }
}

/// The damage rectangle over one half.
fn damage(left: f64) -> DamageRect {
    DamageRect {
        x: left as u32,
        y: 0,
        width: SIZE / 2,
        height: SIZE,
    }
}

fn material_frame(program: u64, nodes: [NodeHandle; 2], values: [(f32, f32); 2]) -> DrawList {
    DrawList {
        commands: vec![
            square(
                nodes[0],
                LEFT.0,
                Some(binding(program, values[0].0, values[0].1, false)),
            ),
            square(
                nodes[1],
                RIGHT.0,
                Some(binding(program, values[1].0, values[1].1, false)),
            ),
        ],
        layers: Vec::new(),
    }
}

const MATERIAL: &str = "function fragment(uv, time, resolution, coverage, level)
   return vec4(level, values[0], 0.0, 1.0)
 end";

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn two_nodes_sharing_a_shader_each_draw_with_their_own_values() {
    let mut backend = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    let program = register(&mut backend, ShaderKind::Material, MATERIAL);
    let mut scene = morf_scene::Scene::new();
    let nodes = [
        scene.create(morf_scene::Element::Sdf),
        scene.create(morf_scene::Element::Sdf),
    ];
    // The left node red from its parameter, the right green from its data.
    let list = material_frame(program, nodes, [(1.0, 0.0), (0.0, 1.0)]);
    let pixels = read_frame(&mut backend, &list, SIZE);
    for x in [LEFT.1, RIGHT.1] {
        assert_eq!(alpha_at(&pixels, SIZE, x, MIDDLE), 255, "{x} painted");
    }
    assert_eq!(channel(&pixels, LEFT.1, MIDDLE, 0), 255, "left: its level");
    assert_eq!(channel(&pixels, LEFT.1, MIDDLE, 1), 0, "left: its data");
    assert_eq!(channel(&pixels, RIGHT.1, MIDDLE, 0), 0, "right: its level");
    assert_eq!(channel(&pixels, RIGHT.1, MIDDLE, 1), 255, "right: its data");
    assert_eq!(
        backend.shader_instances.len(),
        2,
        "one set of buffers per node, and no more",
    );

    // The next frame, the same two nodes: their buffers are reused.
    let pixels = read_frame(&mut backend, &list, SIZE);
    assert_eq!(channel(&pixels, LEFT.1, MIDDLE, 0), 255);
    assert_eq!(channel(&pixels, RIGHT.1, MIDDLE, 1), 255);
    assert_eq!(backend.shader_instances.len(), 2);

    // One node gone: its buffers go with it.
    let only = DrawList {
        commands: vec![list.commands[0].clone()],
        layers: Vec::new(),
    };
    read_frame(&mut backend, &only, SIZE);
    assert_eq!(
        backend.shader_instances.len(),
        1,
        "a node that no longer draws keeps nothing",
    );
    // And none at all once nothing wears the shader.
    let bare = DrawList {
        commands: vec![square(nodes[0], LEFT.0, None)],
        layers: Vec::new(),
    };
    read_frame(&mut backend, &bare, SIZE);
    assert!(backend.shader_instances.is_empty());
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_frame_drawn_through_damage_keeps_each_nodes_values() {
    // The shaded node is redrawn wherever there is damage, so a small frame
    // that changes only one of two nodes must still draw that one with its
    // own values — and leave the other as it was.
    let mut backend = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    let program = register(&mut backend, ShaderKind::Material, MATERIAL);
    let mut scene = morf_scene::Scene::new();
    let nodes = [
        scene.create(morf_scene::Element::Sdf),
        scene.create(morf_scene::Element::Sdf),
    ];
    read_frame(
        &mut backend,
        &material_frame(program, nodes, [(1.0, 0.0), (0.0, 1.0)]),
        SIZE,
    );
    // The left node changes; only its half is damaged. It is written first,
    // so a shared block would have drawn it with the right node's values.
    let changed = material_frame(program, nodes, [(0.0, 1.0), (1.0, 0.0)]);
    backend.render(&changed, &[damage(LEFT.0)], 120).unwrap();
    let pixels = read_back(&mut backend, SIZE);
    assert_eq!(
        channel(&pixels, LEFT.1, MIDDLE, 0),
        0,
        "left: its new level"
    );
    assert_eq!(
        channel(&pixels, LEFT.1, MIDDLE, 1),
        255,
        "left: its new data"
    );
    assert_eq!(
        channel(&pixels, RIGHT.1, MIDDLE, 1),
        255,
        "right: outside the damage, still what it was",
    );
    // Now the right half catches up, and the whole matches a frame drawn from
    // nothing.
    backend.render(&changed, &[damage(RIGHT.0)], 120).unwrap();
    let pixels = read_back(&mut backend, SIZE);
    let mut fresh = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    register(&mut fresh, ShaderKind::Material, MATERIAL);
    let expected = read_frame(&mut fresh, &changed, SIZE);
    assert_eq!(pixels, expected, "the damaged frames add up to a whole one");
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn two_layers_sharing_an_effect_each_composite_with_their_own_values() {
    let mut backend = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    let program = register(
        &mut backend,
        ShaderKind::Effect,
        "function fragment(uv, time, resolution, level)
           return vec4(level, values[0], 0.0, texture(uv).a)
         end",
    );
    let mut scene = morf_scene::Scene::new();
    let nodes = [
        scene.create(morf_scene::Element::Sdf),
        scene.create(morf_scene::Element::Sdf),
    ];
    let layer = |index: usize, left: f64, level: f32, value: f32| Layer {
        node: nodes[index],
        commands: index..index + 1,
        parent: None,
        bounds: half(left),
        opacity: 1.0,
        blur: 0.0,
        shadow_color: Color::rgba8(0, 0, 0, 0),
        shadow_blur: 0.0,
        shadow_offset: [0.0, 0.0],
        alpha_mask: None,
        mask_for: None,
        mask: None,
        shader: Some(binding(program, level, value, true)),
    };
    let list = DrawList {
        commands: vec![
            square(nodes[0], LEFT.0, None),
            square(nodes[1], RIGHT.0, None),
        ],
        layers: vec![layer(0, LEFT.0, 1.0, 0.0), layer(1, RIGHT.0, 0.0, 1.0)],
    };
    let pixels = read_frame(&mut backend, &list, SIZE);
    for x in [LEFT.1, RIGHT.1] {
        assert_eq!(alpha_at(&pixels, SIZE, x, MIDDLE), 255, "{x} composited");
    }
    assert_eq!(channel(&pixels, LEFT.1, MIDDLE, 0), 255, "left: its level");
    assert_eq!(channel(&pixels, LEFT.1, MIDDLE, 1), 0, "left: its data");
    assert_eq!(channel(&pixels, RIGHT.1, MIDDLE, 0), 0, "right: its level");
    assert_eq!(channel(&pixels, RIGHT.1, MIDDLE, 1), 255, "right: its data");

    // Through damage too: the left layer changes, and only its half is drawn.
    let mut changed = list.clone();
    changed.layers[0] = layer(0, LEFT.0, 0.0, 1.0);
    backend.render(&changed, &[damage(LEFT.0)], 120).unwrap();
    let pixels = read_back(&mut backend, SIZE);
    assert_eq!(
        channel(&pixels, LEFT.1, MIDDLE, 0),
        0,
        "left: its new level"
    );
    assert_eq!(
        channel(&pixels, LEFT.1, MIDDLE, 1),
        255,
        "left: its new data"
    );
    assert_eq!(
        channel(&pixels, RIGHT.1, MIDDLE, 1),
        255,
        "right: unchanged"
    );
}
