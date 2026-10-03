// A surface's cached layout lasts until its own tree changes: a clock
// ticking on the bar does not lay the settings window out again.

use crate::paint::CachedLayout;
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
        layout: Layout::compute(&runtime.scene(), root, size, &mut NoText).unwrap(),
        revision: runtime.scene().layout_revision_of(root),
        size: (400, 300),
        scale_120: 120,
        input: Vec::new(),
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
    let bar = crate::surfaces::primary_surface_root(&runtime).unwrap();
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
