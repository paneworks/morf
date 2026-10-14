//! A shader the compiler accepts but the GPU refuses.

use crate::gpu::backend_types::ShaderRegistration;
use crate::*;
use morf_shader::{ShaderKind, ShaderSpec};

fn registration<'a>(program: u64, wgsl: &'a str, size: u32) -> ShaderRegistration<'a> {
    ShaderRegistration {
        program,
        wgsl: Some(wgsl),
        vertex: None,
        offsets: &[],
        uniform_size: size,
        owns_coverage: false,
        effect: false,
        textures: &[],
        data: &[],
    }
}

#[test]
fn a_pipeline_the_gpu_rejects_is_an_error_not_a_dead_pipeline() {
    require_gpu!();
    let spec = ShaderSpec {
        kind: ShaderKind::Material,
        inputs: ShaderSpec::default_inputs(ShaderKind::Material),
        entry: "fragment".to_owned(),
        ..ShaderSpec::default()
    };
    let compiled = morf_shader::compile(
        "function fragment(uv)
           return vec4(uv, 0.0, 1.0)
         end",
        &spec,
    )
    .expect("the shader compiles");
    // Valid as far as morf-shader can tell; naga refuses the name it reads.
    let broken = format!(
        "{}\nfn morf_unused() -> f32 {{ return morf_undeclared; }}\n",
        compiled.wgsl
    );
    let mut backend = pollster::block_on(WgpuBackend::new(32, 32)).unwrap();
    let error = backend
        .register_shader(registration(compiled.hash, &broken, compiled.uniform_size))
        .expect_err("the GPU refuses the module");
    assert!(matches!(error, GpuError::Pipeline(_)), "{error:?}");
    assert!(error.message().contains("rejected the shader"), "{error}");
    assert!(error.message().contains("morf_undeclared"), "{error}");
    // Nothing was left registered under that program, so a fixed shader
    // takes its place rather than being skipped as already known.
    backend
        .register_shader(registration(
            compiled.hash,
            &compiled.wgsl,
            compiled.uniform_size,
        ))
        .expect("the corrected shader registers");
}
