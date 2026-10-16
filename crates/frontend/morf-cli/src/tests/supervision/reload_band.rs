// A failed hot reload through the real host turn: the running configuration
// keeps painting, and the engine shows the failure on its primary surface.

use morf_host::headless::{Headless, LoadOptions};
use std::path::PathBuf;

const BROKEN: &[u8] = b"local x = 1\nlocal y =\n";

fn shell(root: &str) -> String {
    format!(
        r##"
        local ui = require("morf.ui")
        morf.surface.anchors = {{ top = true, left = true, right = true }}
        morf.surface.height = 300
        ui.{root} {{
          id = "root",
          ui.Rect {{ id = "old", width = 40, height = 20, color = "#336699" }},
          ui.Rect {{ id = "second", width = 30, height = 10 }},
        }}
        "##
    )
}

fn load(source: &str) -> Headless {
    let mut options = LoadOptions::new(PathBuf::from("reload-band-test.lua"));
    options.source = Some(source.as_bytes().to_vec());
    options.size = (800, 600);
    Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error))
}

/// Whether the node with `id` was laid out on the primary surface.
fn laid_out(headless: &Headless, id: &str) -> bool {
    let Some(surface) = headless.surfaces.first() else {
        return false;
    };
    let Some(layout) = headless.layout_of(surface) else {
        return false;
    };
    let scene = headless.runtime.scene();
    let mut pending = vec![surface.root];
    while let Some(node) = pending.pop() {
        if scene.string_value(node, "id").ok() == Some(id) {
            return layout.geometry(node).is_some();
        }
        if let Ok(children) = scene.children(node) {
            pending.extend(children.iter().copied());
        }
    }
    false
}

#[test]
fn the_reload_band_spec_passes() {
    use crate::runner_args::{Runner, parse_runner};
    use crate::test_runner::{Tally, run_spec};
    let spec = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/demos/tests/reload_band_spec.lua");
    let args = parse_runner(Runner::Test, &[spec.to_str().unwrap()]).unwrap();
    let (mut number, mut tally) = (0, Tally::default());
    run_spec(&spec, &args, &mut number, &mut tally).unwrap();
    assert_eq!(tally.failed, 0, "{} of {number} failed", tally.failed);
    assert_eq!(tally.passed, number);
    assert!(number >= 16, "{number} tests ran");
}

#[test]
fn the_band_takes_the_pointer_and_a_press_on_the_primary_surface_dismisses_it() {
    use morf_app::{KeyModifiers, PRIMARY_LAYER, WindowId};
    let mut headless = load(&shell("Item"));
    let primary = WindowId::Layer(PRIMARY_LAYER);
    let before = headless.input_region(primary).unwrap_or_default();
    let _ = headless.reload(Some(BROKEN.to_vec()));
    let bounds = headless.reload_band().expect("the band is drawn").bounds;
    assert_eq!((bounds.x, bounds.y, bounds.width), (0.0, 0.0, 800.0));
    let region = headless.input_region(primary).unwrap();
    assert!(
        region
            .iter()
            .any(|rect| (rect.x, rect.y, rect.width) == (0, 0, 800) && rect.height > 0),
        "{region:?}"
    );
    let left = morf_host::headless::input::button("left").unwrap();
    let elsewhere = WindowId::Popup(1);
    let _ = headless.click(elsewhere, (10.0, 10.0), left, KeyModifiers::default());
    assert!(
        headless.runtime.reload_error_shown(),
        "a press on another surface"
    );
    headless
        .click(primary, (10.0, 10.0), left, KeyModifiers::default())
        .unwrap();
    assert!(!headless.runtime.reload_error_shown());
    assert!(headless.reload_band().is_none());
    assert_eq!(headless.input_region(primary).unwrap_or_default(), before);
}

#[test]
fn a_failed_reload_never_stops_a_shell_whatever_its_root_is() {
    for root in ["Item", "Row", "Column", "Grid", "Flex"] {
        let mut headless = load(&shell(root));
        assert!(
            headless.problems.is_empty(),
            "{root}: {:?}",
            headless.problems
        );
        let error = headless.reload(Some(BROKEN.to_vec())).unwrap_err();
        assert!(error.contains("unexpected end"), "{root}: {error}");
        headless.frame(std::time::Duration::from_millis(16));
        assert!(
            headless.problems.is_empty(),
            "{root}: {:?}",
            headless.problems
        );
        assert!(
            laid_out(&headless, "old"),
            "{root}: the old scene still lays out"
        );
        assert!(headless.runtime.reload_error_shown(), "{root}: band shown");
    }
}
