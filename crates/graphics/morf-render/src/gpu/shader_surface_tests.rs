//! Surface shaders against an adapter: a shader that decides its own coverage
//! rather than being clipped to the node's shape.

use crate::gpu::backend_types::ShaderRegistration;
use crate::gpu::field_tests::{alpha_at, field_command, field_layer, read_frame};
use crate::gpu::shader_tests::{SIZE, shaded};
use crate::*;
use morf_shader::{ShaderKind, ShaderSpec};

/// Renders one surface-mode shader, where the shader decides its own coverage.
pub(super) fn surface(body: &str) -> Vec<u8> {
    let spec = ShaderSpec {
        kind: ShaderKind::Surface,
        inputs: ShaderSpec::default_inputs(ShaderKind::Surface),
        params: Vec::new(),
        entry: "fragment".to_owned(),
        textures: Vec::new(),
        data: Vec::new(),
        vertex: false,
    };
    let compiled = morf_shader::compile(body, &spec)
        .unwrap_or_else(|errors| panic!("{}", morf_shader::report("test", &errors)));
    let mut backend = pollster::block_on(WgpuBackend::new(SIZE, SIZE)).unwrap();
    backend
        .register_shader(ShaderRegistration {
            program: compiled.hash,
            wgsl: Some(&compiled.wgsl),
            vertex: None,
            offsets: &[],
            uniform_size: compiled.uniform_size,
            owns_coverage: true,
            effect: false,
            textures: &[],
            data: &[],
        })
        .expect("the generated WGSL compiles");

    let mut scene = morf_scene::Scene::new();
    let node = scene.create(morf_scene::Element::Sdf);
    let mut command = field_command(node, vec![field_layer(8.0, 8.0, 48.0, Shape::Box)]);
    if let DrawCommand::Field { shader, .. } = &mut command {
        *shader = Some(ShaderBinding {
            program: compiled.hash,
            params: Vec::new(),
            data: Vec::new(),
            samples_behind: false,
            owns_coverage: true,
        });
    }
    let list = DrawList {
        commands: vec![command],
        layers: Vec::new(),
    };
    read_frame(&mut backend, &list, SIZE)
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_surface_shader_decides_its_own_coverage() {
    require_gpu!();
    // The difference between the two modes, in one test. The node's own shape
    // is a plain box; the shader carves a disc out of it by returning alpha,
    // and the corners the box would have filled are clear.
    let pixels = surface(
        "function fragment(uv)
           local d = length(uv - vec2(0.5, 0.5))
           local inside = 1.0 - step(0.3, d)
           return vec4(1.0, 0.2, 0.2, inside)
         end",
    );
    assert_eq!(
        alpha_at(&pixels, SIZE, 32, 32),
        255,
        "the middle of the disc is painted",
    );
    assert_eq!(
        alpha_at(&pixels, SIZE, 12, 12),
        0,
        "and the box corner the shader did not claim is clear",
    );
}

#[test]
#[ignore = "requires a GPU adapter"]
pub(crate) fn a_material_shader_cannot_paint_outside_the_shape_but_a_surface_one_can() {
    require_gpu!();
    // The same body under both modes. A material shader is multiplied by the
    // field's coverage, so its alpha of one is still nothing outside the box; a
    // surface shader's is not, so it fills its whole rectangle.
    let body = "function fragment(uv)
                  return vec4(0.2, 0.9, 0.4, 1.0)
                end";
    let material = shaded(body);
    let surfaced = surface(body);
    // Well outside the eight-to-fifty-six box, inside the node's rectangle.
    assert_eq!(alpha_at(&material, SIZE, 3, 3), 0, "the field clipped it");
    assert_eq!(
        alpha_at(&surfaced, SIZE, 3, 3),
        255,
        "and the surface shader was not clipped",
    );
}
