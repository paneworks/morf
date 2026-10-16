// A surface's cached layout lasts until its own tree changes: a clock
// ticking on the bar does not lay the settings window out again.

use morf_host::paint::CachedLayout;
use morf_layout::{Layout, Size};
use morf_lua::Runtime;
use morf_scene::NodeHandle;

struct NoText;

impl morf_layout::TextMeasurer for NoText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        _text: &str,
        _family: &str,
        _size: f64,
        _options: morf_layout::TextOptions,
    ) -> Size {
        Size::default()
    }
}

/// Lays `root` out and caches it the way a paint does.
fn cached(runtime: &Runtime, root: NodeHandle) -> CachedLayout {
    let size = Size {
        width: 400.0,
        height: 300.0,
    };
    CachedLayout {
        painted: None,
        layout: Layout::compute(&runtime.scene(), root, size, &mut NoText).unwrap(),
        revision: runtime.scene().layout_revision_of(root),
        size: (400, 300),
        scale_120: 120,
        input: Vec::new(),
        input_paint: u64::MAX,
        backdrop: Vec::new(),
        keyboard_focus: String::new(),
    }
}

fn still_valid(runtime: &Runtime, cache: &CachedLayout, root: NodeHandle) -> bool {
    cache.still_valid(runtime.scene().layout_revision_of(root), (400, 300), 120)
}

#[test]
fn a_change_on_one_surface_keeps_another_surfaces_layout() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "two-surfaces.lua",
            br#"
                local ui = require("morf.ui")
                local window = require("morf.window")
                local clock = morf.signal("clock", "12:00")
                local rows = morf.signal("rows", 2)
                morf.ipc.tick = function(value) clock:set(value) end
                morf.ipc.rows = function(value) rows:set(value) end
                -- The bar: a clock that ticks.
                ui.Row { ui.Text { text = function() return clock:get() end } }
                -- The settings window, whose page reads its own signal.
                window.floating {
                  root = ui.Column {
                    ui.Rect { width = 100, height = function() return rows:get() * 20 end },
                  },
                  visible = true,
                }
            "#,
        )
        .unwrap();
    let bar = morf_host::host::surfaces::primary_surface_root(&runtime).unwrap();
    let settings = runtime.window_surface_configs()[0].root;
    let bar_cache = cached(&runtime, bar);
    let settings_cache = cached(&runtime, settings);

    runtime
        .call_ipc("tick", &[morf_value::IpcValue::String("12:01".into())])
        .unwrap();
    assert!(
        !still_valid(&runtime, &bar_cache, bar),
        "the bar re-lays out"
    );
    assert!(
        still_valid(&runtime, &settings_cache, settings),
        "the settings window keeps its layout"
    );

    runtime
        .call_ipc("rows", &[morf_value::IpcValue::Integer(5)])
        .unwrap();
    assert!(!still_valid(&runtime, &settings_cache, settings));
}

#[test]
fn the_input_region_is_kept_until_where_input_lands_moves() {
    use morf_app::backend::headless::HeadlessBackend;
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "moving-area.lua",
            br##"
                local ui = require("morf.ui")
                local shift = morf.signal("shift", 0)
                local tint = morf.signal("tint", "#ff0000")
                morf.ipc.shift = function(value) shift:set(tonumber(value)) end
                morf.ipc.tint = function(value) tint:set(value) end
                ui.Item {
                  width = 400, height = 300,
                  ui.Rect { width = 20, height = 20, color = function() return tint:get() end },
                  ui.MouseArea {
                    x = 10, y = 10, width = 50, height = 20,
                    translate_x = function() return shift:get() end,
                  },
                }
            "##,
        )
        .unwrap();
    let root = morf_host::host::surfaces::primary_surface_root(&runtime).unwrap();
    let config = runtime.layer_surface_config();
    let client = HeadlessBackend::new(Vec::new());
    let mut cache = cached(&runtime, root);
    let region = |runtime: &Runtime, cache: &CachedLayout, stamp| {
        let scene = runtime.scene();
        morf_host::paint::send_input_region(
            &scene,
            &cache.layout,
            &client,
            0,
            &config,
            Some(cache),
            stamp,
        )
        .unwrap()
    };
    let derived = runtime.scene().input_stamp(root).unwrap();
    let (input, _) = region(&runtime, &cache, None);
    assert_eq!(input[0].x, 10);
    cache.input = input;
    cache.input_paint = derived;

    // A colour is how something is drawn, not where it takes input.
    let painted = runtime.scene().paint_stamp(root).unwrap();
    runtime
        .call_ipc("tint", &[morf_value::IpcValue::String("#00ff00".into())])
        .unwrap();
    assert_ne!(runtime.scene().paint_stamp(root).unwrap(), painted);
    assert_eq!(runtime.scene().input_stamp(root).unwrap(), derived);

    runtime
        .call_ipc("shift", &[morf_value::IpcValue::String("40".into())])
        .unwrap();
    assert!(
        still_valid(&runtime, &cache, root),
        "a translation needs no new layout"
    );
    let moved = runtime.scene().input_stamp(root).unwrap();
    assert_ne!(moved, derived, "a translation moves the tree's input stamp");
    // The stamp it was derived at: kept, without walking the tree.
    assert_eq!(region(&runtime, &cache, Some(derived)).0[0].x, 10);
    // The stamp now: derived again, where the area is drawn.
    let (input, at_now) = region(&runtime, &cache, Some(moved));
    assert_eq!(input[0].x, 50);
    assert_eq!(at_now, moved);
}
