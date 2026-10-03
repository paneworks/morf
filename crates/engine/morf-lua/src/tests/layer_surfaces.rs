use super::*;

#[test]
fn configured_layer_surfaces_carry_their_own_settings() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layers.lua",
            br##"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local shell = ui.Item {}
                local edge = window.layer {
                    root = ui.Rect { color = "#111111" },
                    visible = true,
                    namespace = "border-top",
                    width = 0,
                    height = 6,
                    anchors = { top = true, left = true, right = true },
                    margin_top = -6,
                    layer = "overlay",
                    keyboard_focus = "none",
                }
                local corner = window.layer {
                    root = ui.Item {},
                    namespace = "border-corner",
                    width = 24,
                    height = 24,
                    updates_enabled = false,
                }
                assert(edge:kind() == "layer" and edge:visible())
                assert(corner:kind() == "layer" and not corner:visible())
                assert(not corner:updates_enabled())
                assert(edge:parent_id() == nil)
                assert(corner:size().width == 24)
                corner:open()
                edge:close()
                assert(corner:visible() and not edge:visible())
            "##,
        )
        .unwrap();

    let surfaces = runtime.window_surface_configs();
    assert_eq!(surfaces.len(), 2);
    assert!(!surfaces[0].visible);
    assert!(surfaces[1].visible);
    let WindowSurfaceKind::Layer(edge) = &surfaces[0].kind else {
        panic!("first surface was not a layer surface");
    };
    assert_eq!(edge.namespace, "border-top");
    assert_eq!((edge.width, edge.height), (0, 6));
    assert_eq!(edge.margin_top, -6);
    assert_eq!(edge.layer, "overlay");
    assert_eq!(edge.keyboard_focus, "none");
    assert!(edge.anchors.top && edge.anchors.left && edge.anchors.right);
    assert!(!edge.anchors.bottom);
    // Decoration drawn outside the usable area must not claim space by default.
    assert_eq!(edge.exclusive_zone, 0);
    assert_eq!(edge.reserve, SurfaceReserve::default());
}

#[test]
fn layer_surface_settings_reject_unknown_and_shell_only_keys() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layer-errors.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local root = ui.Item {}
                assert(not pcall(window.layer, { root = root, nonsense = 1 }))
                assert(not pcall(window.layer, { root = root, layer = "middle" }))
                -- Zero is compositor-sized on both axes now, so what is
                -- rejected is a size a surface cannot be.
                assert(not pcall(window.layer, { root = root, height = 20000 }))
                assert(not pcall(window.layer, { root = root, width = 20000 }))
                assert(not pcall(window.layer, { root = root, reserve = { top = 4 } }))
                assert(not pcall(window.layer, { width = 10 }))
            "#,
        )
        .unwrap();
    assert!(runtime.window_surface_configs().is_empty());
}

#[test]
fn every_surface_names_the_space_it_blends_in() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "blend.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                assert(morf.surface.blend == "linear")
                morf.surface.blend = "srgb"
                assert(morf.surface.blend == "srgb")
                assert(not pcall(function() morf.surface.blend = "gamma" end))
                assert(not pcall(function() morf.surface.blend = true end))
                window.layer { root = ui.Item {}, blend = "srgb" }
                window.popup { root = ui.Item {}, blend = "srgb" }
                window.floating { root = ui.Item {} }
                assert(not pcall(window.floating, { root = ui.Item {}, blend = "cmyk" }))
            "#,
        )
        .unwrap();
    assert_eq!(runtime.layer_surface_config().blend, "srgb");
    let blends: Vec<_> = runtime
        .window_surface_configs()
        .into_iter()
        .map(|surface| match surface.kind {
            WindowSurfaceKind::Layer(config) => config.blend,
            WindowSurfaceKind::Popup(config) => config.blend,
            WindowSurfaceKind::Floating(config) => config.blend,
        })
        .collect();
    assert_eq!(blends, ["srgb", "srgb", "linear"]);
}

#[test]
fn shell_surface_reserve_is_native_and_typed() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "reserve.lua",
            br#"
                local morf = require("morf")
                assert(morf.surface.reserve.top == 0)
                morf.surface.reserve = { top = 12, bottom = 8 }
                assert(morf.surface.reserve.top == 12)
                assert(morf.surface.reserve.right == 0)
                assert(morf.surface.reserve.bottom == 8)
                assert(not pcall(function() morf.surface.reserve = { top = -1 } end))
                assert(not pcall(function() morf.surface.reserve = { middle = 1 } end))
                assert(not pcall(function() morf.surface.reserve = 4 end))
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime.layer_surface_config().reserve,
        SurfaceReserve {
            top: 12,
            right: 0,
            bottom: 8,
            left: 0,
        }
    );
}

#[test]
fn a_layer_surface_root_is_not_a_scene_orphan() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layer-root.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                ui.Item {}
                window.layer { root = ui.Item {}, height = 4 }
            "#,
        )
        .unwrap();

    let surfaces = runtime.window_surface_configs();
    assert_eq!(surfaces.len(), 1);
    let roots = runtime.scene().roots();
    assert_eq!(roots.len(), 2);
    assert!(roots.contains(&surfaces[0].root));
}

#[test]
fn shell_surface_geometry_reports_only_real_changes() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "geometry.lua",
            br#"
                local morf = require("morf")
                morf.surface.margin_left = 200
            "#,
        )
        .unwrap();

    assert!(runtime.take_layer_surface_change());
    assert!(!runtime.take_layer_surface_change());

    // Lua re-runs a binding whenever anything it reads moves, so an assignment
    // that writes back the value already there must not reconfigure a surface.
    runtime
        .execute(
            "unchanged.lua",
            br#"
                local morf = require("morf")
                morf.surface.margin_left = 200
            "#,
        )
        .unwrap();
    assert!(!runtime.take_layer_surface_change());
}

#[test]
fn shell_surface_geometry_accepts_interpolated_numbers() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "animated.lua",
            br#"
                local morf = require("morf")
                morf.surface.margin_left = 120.6
                morf.surface.margin_top = -3.5
                morf.surface.width = 640.4
                assert(not pcall(function() morf.surface.margin_left = "12" end))
            "#,
        )
        .unwrap();

    let config = runtime.layer_surface_config();
    assert!(runtime.take_layer_surface_change());
    // A slide animation assigns a float every frame; rounding is what keeps the
    // margin animatable instead of raising an error mid-transition.
    assert_eq!(config.margin_left, 121);
    assert_eq!(config.margin_top, -4);
    assert_eq!(config.width, 640);
}

#[test]
fn a_surface_can_claim_to_be_opaque() {
    // A hint the compositor uses to skip blending a few hundred thousand
    // pixels a frame. Off by default, because a bar with one transparent
    // corner that claims otherwise draws garbage in it -- so it is a claim the
    // configuration makes, and reads back, rather than one the engine infers.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "opaque.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local before = morf.surface.opaque
                morf.surface.opaque = true
                ui.Text { text = tostring(before) .. "," .. tostring(morf.surface.opaque) }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "false,true"
    );
    assert!(runtime.layer_surface_config().opaque);
    assert!(
        runtime.take_layer_surface_change(),
        "and the claim reaches the compositor on the next frame"
    );
}

#[test]
fn an_exclusive_zone_can_be_automatic() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "auto-zone.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                morf.surface.exclusive_zone = "auto"
                local first = morf.surface.exclusive_zone
                morf.surface.exclusive_zone = 12
                ui.Text { text = tostring(first) .. "," .. tostring(morf.surface.exclusive_zone) }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "auto,12",
        "the mode reads back as itself, and a number takes it out of the mode"
    );
    assert!(!runtime.layer_surface_config().exclusive_auto);
}

#[test]
fn a_layer_handle_changes_its_settings_at_runtime() {
    // A dock that slides away or a panel that moves to another edge needs
    // anchors, margins, the zone, the layer and focus to change after the
    // surface exists; the handle used to offer only `size()`.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layer_runtime.lua",
            br##"
                local morf = require("morf")
                local ui = require("morf.ui")
                local window = require("morf.window")
                local dock = window.layer {
                    root = ui.Item {},
                    visible = true,
                    namespace = "dock",
                    width = 400,
                    height = 64,
                    anchors = { bottom = true },
                }
                assert(dock.namespace == "dock")
                assert(dock.margin_bottom == 0 and dock.layer == "top")
                assert(dock:kind() == "layer")
                morf.ipc.slide = function(margin) dock.margin_bottom = margin end
                morf.ipc.move = function()
                    dock:configure {
                        anchors = { left = true, top = true, bottom = true },
                        exclusive_zone = 64,
                        layer = "overlay",
                        keyboard_focus = "exclusive",
                        mask = { x = 0, y = 0, width = 10, height = 10 },
                        margin_left = 4.4,
                    }
                end
                morf.ipc.bad = function() dock:configure { margin_top = 3, layer = "sky" } end
                morf.ipc.rename = function() dock.namespace = "other" end
                morf.ipc.read = function() return dock.margin_bottom, dock.layer end
            "##,
        )
        .unwrap();
    runtime.take_window_surface_change();
    let layer = |runtime: &Runtime| {
        let WindowSurfaceKind::Layer(config) = runtime.window_surface_configs()[0].kind.clone()
        else {
            panic!("not a layer surface");
        };
        config
    };

    runtime
        .call_ipc("slide", &[IpcValue::Integer(-40)])
        .unwrap();
    assert!(runtime.take_window_surface_change());
    assert_eq!(layer(&runtime).margin_bottom, -40);
    // The same value again is no change: an animation re-assigning its
    // resting value must not reconfigure the surface every frame.
    runtime
        .call_ipc("slide", &[IpcValue::Integer(-40)])
        .unwrap();
    assert!(!runtime.take_window_surface_change());

    runtime.call_ipc("move", &[]).unwrap();
    assert!(runtime.take_window_surface_change());
    let config = layer(&runtime);
    assert!(config.anchors.left && config.anchors.top && config.anchors.bottom);
    assert!(!config.anchors.right);
    assert_eq!(config.exclusive_zone, 64);
    assert_eq!(config.layer, "overlay");
    assert_eq!(config.keyboard_focus, "exclusive");
    assert_eq!(config.margin_left, 4);
    assert!(config.input_regions.is_some());
    assert_eq!(
        runtime.call_ipc("read", &[]).unwrap(),
        [IpcValue::Integer(-40), IpcValue::String("overlay".into())]
    );

    // A bad setting in a batch leaves the surface as it was.
    assert!(runtime.call_ipc("bad", &[]).is_err());
    assert_eq!(layer(&runtime).margin_top, 0);
    assert!(!runtime.take_window_surface_change());
    assert!(runtime.call_ipc("rename", &[]).is_err());
    assert_eq!(layer(&runtime).namespace, "dock");
}
