use morf_layout::Layout;
use morf_scene::Element;
use std::time::Duration;

use super::*;

#[test]
fn inset_is_native_and_rejects_ambiguous_children() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "inset.lua",
            br#"
                local ui = require("morf.ui")
                ui.Inset {
                    margin = 8,
                    left_margin = 12,
                    ui.Text { text = "content" },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().element(root).unwrap(), Element::Inset);
    assert_eq!(runtime.scene().number(root, "margin").unwrap(), 8.0);
    assert_eq!(runtime.scene().children(root).unwrap().len(), 1);

    let error = Runtime::default()
        .execute(
            "ambiguous-inset.lua",
            br#"
                local ui = require("morf.ui")
                ui.Inset { ui.Item {}, ui.Item {} }
            "#,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Inset accepts at most one child")
    );
}

#[test]
fn clip_rect_is_native_and_clips_by_default() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clip-rect.lua",
            br#"
                local ui = require("morf.ui")
                ui.ClipRect {
                    border_width = 2,
                    content_under_border = true,
                    content_inside_border = false,
                    antialiasing = false,
                    border_pixel_aligned = false,
                    ui.Item {},
                }
            "#,
        )
        .unwrap();

    let root = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().element(root).unwrap(), Element::ClipRect);
    assert!(runtime.scene().bool_value(root, "clip").unwrap());
    assert!(
        !runtime
            .scene()
            .bool_value(root, "content_inside_border")
            .unwrap()
    );
    assert!(
        runtime
            .scene()
            .bool_value(root, "content_under_border")
            .unwrap()
    );
    assert!(!runtime.scene().bool_value(root, "antialiasing").unwrap());
    assert!(
        !runtime
            .scene()
            .bool_value(root, "border_pixel_aligned")
            .unwrap()
    );
}

#[test]
fn transform_watcher_dispatches_after_rendered_geometry_changes() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "transform.lua",
            br#"
                local morf = require("morf")
                local core = require("morf.core")
                local ui = require("morf.ui")
                local calls = morf.signal("transform.calls", 0)
                local child = ui.Item { implicit_width = 20, implicit_height = 10 }
                local root = ui.Item { child }
                local watcher = core.transform_watcher {
                  a = root,
                  b = child,
                  common_parent = root,
                  on_changed = function(revision) calls:set(revision) end,
                }
                morf.ipc["transform.state"] = function()
                  return watcher:revision(), calls:get()
                end
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let child = runtime.scene().children(root).unwrap()[0];
    let layout = Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 50.0,
        },
        &mut NoText,
    )
    .unwrap();
    assert!(!runtime.observe_layout(&layout));

    runtime.scene_mut().assign(child, "x", 12.0).unwrap();
    let layout = Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 50.0,
        },
        &mut NoText,
    )
    .unwrap();
    assert!(runtime.observe_layout(&layout));
    assert!(runtime.poll_services());
    assert_eq!(
        runtime.call_ipc("transform.state", &[]).unwrap(),
        [IpcValue::Integer(1), IpcValue::Integer(1)]
    );

    let error = Runtime::default()
        .execute(
            "invalid-transform.lua",
            br#"
                local core = require("morf.core")
                local ui = require("morf.ui")
                local a = ui.Item {}
                local b = ui.Item {}
                core.transform_watcher { a = a, b = b, common_parent = a }
            "#,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("common_parent must contain both")
    );
}

#[test]
fn cached_layout_still_observes_transforms_without_relaying_out() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "cached-transform.lua",
            br#"
        local ui = require("morf.ui")
        local core = require("morf.core")
        local child = ui.Item { width=20, height=10 }
        local root = ui.Item { child }
        core.transform_watcher { a=root, b=child, common_parent=root,
            on_changed=function() end }
    "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let child = runtime.scene().children(root).unwrap()[0];
    let layout = Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 100.0,
            height: 50.0,
        },
        &mut NoText,
    )
    .unwrap();
    assert!(!runtime.observe_layout(&layout));
    let revision = runtime.scene().layout_revision_of(root);
    runtime
        .scene_mut()
        .assign(child, "translate_x", 12.0)
        .unwrap();
    assert_eq!(runtime.scene().layout_revision_of(root), revision);
    assert!(runtime.observe_layout_with(&layout, false));
    assert!(!runtime.observe_layout_with(&layout, false));
}

#[test]
fn lua_constructs_image_icon_and_field_elements() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "images.lua",
            br#"
                local ui = require("morf.ui")
                ui.Item {
                    ui.Image { source = "/tmp/picture.png", width = 64, height = 32 },
                    ui.Icon { name = "battery", theme = "hicolor", width = 24, height = 24 },
                    ui.Sdf {
                      width = 32, height = 32,
                      ui.SdfShape { width = 32, height = 32, shape = "triangle" },
                    },
                }
            "#,
        )
        .unwrap();

    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();
    assert_eq!(
        runtime.scene().element(children[0]).unwrap(),
        Element::Image
    );
    assert_eq!(runtime.scene().element(children[1]).unwrap(), Element::Icon);
    assert_eq!(runtime.scene().element(children[2]).unwrap(), Element::Sdf);
}

#[test]
fn clock_service_recomputes_text_bindings() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clock.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                ui.Text { text = function() return morf.clock:get() end }
            "#,
        )
        .unwrap();
    runtime.update_clock("12:34:56").unwrap();

    let node = runtime.scene().roots()[0];
    assert_eq!(
        runtime.scene().string_value(node, "text").unwrap(),
        "12:34:56"
    );
}

#[test]
fn pam_callbacks_return_asynchronously_to_lua() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "pam.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local result = morf.signal("pam.result", "pending")
                morf.pam.authenticate("morf\0test", "user", "secret", function(ok, error)
                    result:set(ok and "ok" or error)
                end)
                ui.Text { text = function() return result:get() end }
            "#,
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !runtime.poll_services() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let root = runtime.scene().roots()[0];

    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "service contains a null byte"
    );
}

#[test]
fn failed_pam_authentication_cannot_request_unlock() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "unlock.lua",
            br#"
                local morf = require("morf")
                morf.pam.authenticate_unlock("morf\0test", "user", "secret", function() end)
            "#,
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !runtime.poll_services() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(!runtime.take_session_unlock_request());
}

#[test]
fn a_distance_field_composes_animatable_layers_from_lua() {
    // The point of layers being ordinary nodes: a morph and a blend are just
    // numbers, so they animate through the same behaviors as anything else and
    // need no mechanism of their own.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "field.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local merge = morf.signal("field.merge", 0)
                morf.ipc["field.merge"] = function(value) merge:set(value) end
                ui.Sdf {
                  width = 200,
                  height = 120,
                  fill_color = "#b4e1ea",
                  stroke_color = "#0e1213",
                  stroke_width = 2,
                  softness = 0,
                  ui.SdfShape {
                    x = 10, y = 10, width = 100, height = 100,
                    shape = "circle",
                    morph_to = "star",
                    morph_progress = 0.5,
                    points = 6,
                  },
                  ui.SdfShape {
                    x = 90, y = 10, width = 100, height = 100,
                    shape = "circle",
                    operation = "smooth_union",
                    blend = function() return merge:get() end,
                    behavior = { blend = { duration = 200, easing = "in_out_cubic" } },
                  },
                }
            "##,
        )
        .unwrap();

    let (root, layers) = {
        let scene = runtime.scene();
        let root = scene.roots()[0];
        (root, scene.children(root).unwrap().to_vec())
    };
    assert_eq!(runtime.scene().element(root).unwrap(), Element::Sdf);
    assert_eq!(layers.len(), 2);
    assert_eq!(
        runtime.scene().string_value(layers[0], "morph_to").unwrap(),
        "star"
    );
    assert_eq!(
        runtime.scene().number(layers[0], "morph_progress").unwrap(),
        0.5
    );
    assert_eq!(runtime.scene().number(layers[1], "blend").unwrap(), 0.0);

    // Driving the signal starts the behavior on the second layer's blend, so
    // the two circles merge over 200ms rather than snapping together.
    runtime
        .call_ipc("field.merge", &[IpcValue::Number(40.0)])
        .unwrap();
    runtime
        .tick_animations(std::time::Duration::from_millis(100))
        .unwrap();
    let midway = runtime.scene().number(layers[1], "blend").unwrap();
    assert!(
        midway > 0.0 && midway < 40.0,
        "blend should be easing, got {midway}"
    );
    runtime
        .tick_animations(std::time::Duration::from_millis(200))
        .unwrap();
    assert_eq!(runtime.scene().number(layers[1], "blend").unwrap(), 40.0);
}

// A function assigned to a property after construction is a binding, as in
// the constructor: it follows what it reads, a second one takes the first's
// place, and the checks a constructor makes still hold.
#[test]
fn a_function_assigned_after_construction_is_a_binding() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "bind.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                width = morf.signal("width", 10)
                other = morf.signal("other", 1)
                runs = 0
                node = ui.Rect { width = 5 }
                node.width = function() runs = runs + 1 return width:get() * 2 end
                assert(node.width == 20, "not run at once: " .. tostring(node.width))
                width:set(30)
                assert(node.width == 60, "did not follow: " .. tostring(node.width))
                -- A second binding takes the first's place: the first no
                -- longer runs when what it read changes.
                node.width = function() return other:get() end
                assert(node.width == 1)
                local before = runs
                width:set(40)
                assert(runs == before, "the old binding still ran")
                assert(node.width == 1)
                other:set(7)
                assert(node.width == 7)
                -- The constructor's checks.
                assert(not pcall(function() node.nonsense = function() return 1 end end))
                local area = ui.MouseArea {}
                assert(not pcall(function() area.hovered = function() return true end end))
                -- Inside a handler as well.
                morf.ipc.bind = function()
                    node.height = function() return other:get() + 1 end
                    return node.height
                end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("bind", &[]).unwrap(),
        vec![IpcValue::Number(8.0)]
    );
}
