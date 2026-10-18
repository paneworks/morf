// Window surfaces against the primary one: whose root is whose, and whether
// a child window shows through its parents.

use morf_app::physical_size;
use morf_host::host::popups::window_surface_effectively_visible;
use morf_host::host::surfaces::primary_surface_root;
use morf_lua::Runtime;
use std::collections::{HashMap, HashSet};

#[test]
fn primary_root_excludes_registered_window_roots() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-roots.lua",
            br#"
                    local ui = require("morf.ui")
                    local window = require("morf.window")
                    local primary = ui.Item {}
                    local popup = ui.Item {}
                    window.popup { root = popup, width = 20, height = 10 }
                "#,
        )
        .unwrap();
    let primary = primary_surface_root(&runtime).unwrap();
    assert_eq!(runtime.scene().roots()[0], primary);
    assert_eq!(physical_size((101, 31), 150), (127, 39));
}

#[test]
fn child_window_visibility_follows_parent_chain() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "window-parents.lua",
            br#"
                    local ui = require("morf.ui")
                    local window = require("morf.window")
                    local parent = window.floating {
                      root = ui.Item {}, visible = false,
                    }
                    local child = window.floating {
                      root = ui.Item {}, visible = true, parent = parent,
                    }
                    window.popup {
                      root = ui.Item {}, visible = true, parent = child,
                    }
                "#,
        )
        .unwrap();
    let surfaces = runtime.window_surface_configs();
    let by_id = surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<HashMap<_, _>>();

    assert!(!window_surface_effectively_visible(
        2,
        &by_id,
        &mut HashSet::new()
    ));
    runtime.set_window_surface_visible(0, true);
    let surfaces = runtime.window_surface_configs();
    let by_id = surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<HashMap<_, _>>();
    assert!(window_surface_effectively_visible(
        2,
        &by_id,
        &mut HashSet::new()
    ));
}
