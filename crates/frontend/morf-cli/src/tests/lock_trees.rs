// A lock built per output: each output's tree is its own, laid out against
// its own size.

use crate::lock_outputs::{LockOutput, LockTrees, ensure_lock_tree, release_lock_tree};
use morf_lua::Runtime;
use morf_wayland::{ScreenInfo, SurfaceRole};

fn screen(name: &str, width: i32, height: i32) -> ScreenInfo {
    ScreenInfo {
        id: 1,
        name: Some(name.to_owned()),
        size: Some((width, height)),
        scale: 1,
        ..ScreenInfo::default()
    }
}

fn per_output_runtime() -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-per-output.lua",
            br##"
                local ui = require("morf.ui")
                morf.surface.session_lock = true
                morf.lock_surface(function(screen)
                    return ui.Rect {
                        width = screen.width, height = screen.height, color = "#000000",
                        ui.Item {
                            -- A binding on the size, which is the thing a
                            -- shared tree gets wrong on the second screen.
                            x = function() return screen.width / 2 - 50 end,
                            width = 100, height = 20,
                        },
                    }
                end)
            "##,
        )
        .unwrap();
    runtime
}

fn centred_x(runtime: &Runtime, root: morf_scene::NodeHandle) -> f64 {
    let scene = runtime.scene();
    let child = scene.children(root).unwrap()[0];
    scene.number(child, "x").unwrap()
}

#[test]
fn every_output_gets_a_root_sized_by_its_own_screen() {
    let mut runtime = per_output_runtime();
    let trees = LockTrees::of(&runtime).unwrap();
    assert_eq!(trees, LockTrees::PerOutput);
    let mut outputs = vec![LockOutput::default(), LockOutput::default()];
    assert!(
        ensure_lock_tree(
            &mut runtime,
            trees,
            &mut outputs[0],
            0,
            Some(screen("eDP-1", 1920, 1080)),
            (1920, 1080),
        )
        .unwrap()
    );
    assert!(
        ensure_lock_tree(
            &mut runtime,
            trees,
            &mut outputs[1],
            1,
            Some(screen("DP-2", 2560, 1440)),
            (2560, 1440),
        )
        .unwrap()
    );
    let first = trees.root(&outputs, 0).unwrap();
    let second = trees.root(&outputs, 1).unwrap();
    assert_ne!(first, second);
    assert_eq!(runtime.scene().roots().len(), 2);
    assert_eq!(centred_x(&runtime, first), 910.0);
    assert_eq!(centred_x(&runtime, second), 1230.0);
    assert_eq!(runtime.scene().number(second, "width").unwrap(), 2560.0);
    // A key on the second surface goes into the second tree.
    assert_eq!(trees.key_root(&outputs, SurfaceRole::Lock(1)), Some(second));

    // The same size again builds nothing; a new size builds afresh.
    assert!(
        !ensure_lock_tree(
            &mut runtime,
            trees,
            &mut outputs[0],
            0,
            Some(screen("eDP-1", 1920, 1080)),
            (1920, 1080),
        )
        .unwrap()
    );
    ensure_lock_tree(
        &mut runtime,
        trees,
        &mut outputs[0],
        0,
        Some(screen("eDP-1", 1280, 800)),
        (1280, 800),
    )
    .unwrap();
    let rebuilt = trees.root(&outputs, 0).unwrap();
    assert!(!runtime.scene().contains(first));
    assert_eq!(centred_x(&runtime, rebuilt), 590.0);
    assert_eq!(runtime.scene().roots().len(), 2);

    // An output that goes takes its tree with it.
    release_lock_tree(&mut runtime, &mut outputs[1]);
    assert!(!runtime.scene().contains(second));
    assert_eq!(runtime.scene().roots(), [rebuilt]);
}

#[test]
fn a_shared_root_is_still_one_tree_for_every_output() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-shared.lua",
            br##"
                local ui = require("morf.ui")
                morf.surface.session_lock = true
                ui.Rect { width = 10, height = 10, color = "#000000" }
            "##,
        )
        .unwrap();
    let trees = LockTrees::of(&runtime).unwrap();
    let root = runtime.scene().roots()[0];
    assert_eq!(trees, LockTrees::Shared(root));
    let mut output = LockOutput::default();
    assert!(!ensure_lock_tree(&mut runtime, trees, &mut output, 0, None, (800, 600)).unwrap());
    assert_eq!(trees.root(&[output], 0), Some(root));
}

#[test]
fn a_builder_and_a_root_of_its_own_is_refused() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-both.lua",
            br##"
                local ui = require("morf.ui")
                morf.lock_surface(function() return ui.Rect { color = "#000000" } end)
                ui.Rect { color = "#000000" }
            "##,
        )
        .unwrap();
    assert!(LockTrees::of(&runtime).is_err());
}

#[test]
fn a_builder_that_returns_no_rect_is_refused_and_leaves_nothing() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "lock-item.lua",
            br#"
                local ui = require("morf.ui")
                morf.lock_surface(function() return ui.Item {} end)
            "#,
        )
        .unwrap();
    let trees = LockTrees::of(&runtime).unwrap();
    let mut output = LockOutput::default();
    assert!(ensure_lock_tree(&mut runtime, trees, &mut output, 0, None, (800, 600)).is_err());
    assert!(runtime.scene().roots().is_empty());
    assert!(output.root.is_none());
}
