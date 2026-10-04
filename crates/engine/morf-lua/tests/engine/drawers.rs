use morf_lua::*;
use std::time::Duration;

// `stretch`, `track`, `transform_matrix` and a field's `blend_profile` as a
// configuration writes them.

fn lay_out(runtime: &Runtime, root: morf_scene::NodeHandle) -> morf_layout::Layout {
    morf_layout::Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 400.0,
            height: 300.0,
        },
        &mut super::NoText,
    )
    .unwrap()
}

#[test]
fn a_panel_declares_its_stretch_and_a_layer_follows_it() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "drawer.lua",
            br#"
                local ui = require("morf.ui")
                local panel = ui.Item {
                    id = "panel", x = 20, y = 0, width = 120, height = 80,
                    stretch = { stiffness = 300, damping = 14, scale = 0.2 },
                    behavior = { translate_y = { duration = 400 } },
                }
                local shape = ui.SdfShape { shape = "box", track = panel }
                local root = ui.Item {
                    width = 400, height = 300,
                    ui.Sdf { anchors = { fill = true }, blend = 12, blend_profile = "circular", shape },
                    panel,
                }
                assert(shape.track.id == "panel", "track reads back as the node")
                assert(panel.stretch.stiffness == 300, "stretch reads back")
                assert(not pcall(function() shape.track = 4 end), "a number is not a node")
                assert(not pcall(function() panel.track = shape end), "only a field layer tracks")
                _G.panel, _G.shape = panel, shape
                morf.ipc.open = function() panel.translate_y = 200 end
                morf.ipc.untrack = function() shape.track = nil; panel.stretch = false end
            "#,
        )
        .unwrap();
    let scene = runtime.scene();
    let root = scene.roots()[0];
    let panel = scene.children(root).unwrap()[1];
    let shape = scene.children(scene.children(root).unwrap()[0]).unwrap()[0];
    assert_eq!(scene.track(shape), Some(panel));
    assert!(scene.stretch(panel).is_some());
    drop(scene);

    let layout = lay_out(&runtime, root);
    runtime.observe_stretch(&layout);
    runtime
        .scene_mut()
        .assign(panel, "translate_y", 200.0)
        .unwrap();
    for _ in 0..8 {
        runtime.tick_animations(Duration::from_millis(16)).unwrap();
        let layout = lay_out(&runtime, root);
        runtime.observe_stretch(&layout);
    }
    let deformation = runtime
        .scene()
        .deformation(panel)
        .expect("sliding stretches");
    assert!(
        deformation[3] > 1.0,
        "taller while it slides down: {deformation:?}"
    );

    runtime.call_ipc("untrack", &[]).unwrap();
    assert_eq!(runtime.scene().track(shape), None);
    assert_eq!(runtime.scene().stretch(panel), None);
    assert_eq!(runtime.scene().deformation(panel), None);
}

#[test]
fn a_transform_matrix_and_a_blend_profile_are_checked_where_they_are_written() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "bad.lua",
            br#"
                local ui = require("morf.ui")
                ui.Item { transform_matrix = { 1, 0, 0 } }
            "#,
        )
        .unwrap_err();
    assert!(error.to_string().contains("six numbers"), "{error}");
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "bad.lua",
            br#"
                local ui = require("morf.ui")
                ui.Sdf { blend_profile = "cubic" }
            "#,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("quadratic or circular"),
        "{error}"
    );
}
