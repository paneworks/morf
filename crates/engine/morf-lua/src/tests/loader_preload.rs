//! A `ui.Loader` switched on and off: preloading ahead hidden, keeping its
//! item, and waiting for the scene to stop moving.

use std::time::Duration;

use super::*;

/// A `ui.Loader` with the given extra fields, inactive until `loader.open`
/// and again after `loader.close`, counting how often its source runs.
fn switched_loader(fields: &str) -> (Runtime, NodeHandle) {
    let mut runtime = Runtime::default();
    let source = format!(
        r#"
            local ui = require("morf.ui")
            local active = morf.signal("loader.active", false)
            builds = 0
            local loader = ui.Loader {{
              {fields}
              active = function() return active:get() end,
              source = function()
                builds = builds + 1
                return ui.Text {{ text = "panel " .. builds }}
              end,
            }}
            morf.ipc["loader.open"] = function() active:set(true) end
            morf.ipc["loader.close"] = function() active:set(false) end
            morf.ipc["loader.builds"] = function() return builds end
            ui.Item {{ loader }}
        "#
    );
    runtime.execute("switched.lua", source.as_bytes()).unwrap();
    let root = runtime.scene().roots()[0];
    let loader = runtime.scene().children(root).unwrap()[0];
    (runtime, loader)
}

fn builds(runtime: &mut Runtime) -> i64 {
    match runtime.call_ipc("loader.builds", &[]).unwrap().as_slice() {
        [IpcValue::Integer(count)] => *count,
        [IpcValue::Number(count)] => *count as i64,
        other => panic!("builds: {other:?}"),
    }
}

/// The loader's item, as its text and whether it is shown.
fn item(runtime: &Runtime, loader: NodeHandle) -> Option<(String, bool)> {
    let scene = runtime.scene();
    let child = *scene.children(loader).unwrap().first()?;
    Some((
        scene.string_value(child, "text").unwrap().to_owned(),
        scene.bool_value(child, "visible").unwrap(),
    ))
}

#[test]
fn a_preloading_loader_builds_ahead_hidden_and_shows_without_building() {
    let (mut runtime, loader) = switched_loader("preload = true,");
    assert_eq!(item(&runtime, loader), None);
    // Built once, and held: turns that follow leave it be.
    for _ in 0..4 {
        runtime.poll_services();
    }
    // Built while nothing moves, hidden, and not made active by it.
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), false)));
    assert!(!runtime.scene().bool_value(loader, "active").unwrap());
    assert_eq!(builds(&mut runtime), 1);

    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), true)));
    assert_eq!(builds(&mut runtime), 1, "opening built nothing");

    // Closed, it is let go as any Loader's item is, and a fresh one is
    // built ahead of the next open.
    runtime.call_ipc("loader.close", &[]).unwrap();
    for _ in 0..4 {
        runtime.poll_services();
    }
    assert_eq!(item(&runtime, loader), Some(("panel 2".into(), false)));
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 2".into(), true)));
    assert_eq!(builds(&mut runtime), 2);
}

#[test]
fn a_kept_loader_hides_its_item_and_shows_the_same_one_again() {
    let (mut runtime, loader) = switched_loader("keep = true,");
    runtime.poll_services();
    assert_eq!(
        item(&runtime, loader),
        None,
        "nothing built before it is asked"
    );
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    let shown = runtime.scene().children(loader).unwrap()[0];
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), true)));

    runtime.call_ipc("loader.close", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), false)));
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(runtime.scene().children(loader).unwrap()[0], shown);
    assert_eq!(item(&runtime, loader), Some(("panel 1".into(), true)));
    assert_eq!(builds(&mut runtime), 1);
}

#[test]
fn a_preload_waits_for_the_scene_to_stop_moving() {
    let (mut runtime, loader) =
        switched_loader("preload = true, behavior = { x = { duration = 200 } },");
    runtime.poll_services();
    runtime.call_ipc("loader.open", &[]).unwrap();
    runtime.poll_services();
    runtime.call_ipc("loader.close", &[]).unwrap();
    // Something moving when the next one would be built.
    runtime.scene_mut().assign(loader, "x", 100.0).unwrap();
    assert!(runtime.has_motion());
    runtime.poll_services();
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), None, "not while anything moves");
    assert!(matches!(
        runtime.next_deadline(),
        Some((_, crate::DeadlineCause::Preload))
    ));
    for _ in 0..4 {
        runtime.tick_animations(Duration::from_millis(100)).unwrap();
    }
    assert!(!runtime.has_motion(), "the motion is over");
    runtime.poll_services();
    assert_eq!(item(&runtime, loader), Some(("panel 2".into(), false)));
}
