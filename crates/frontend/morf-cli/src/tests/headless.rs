// The headless runners: `morf check`, `morf render` and `morf test` drive a
// configuration with no compositor, on a virtual clock.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use morf_lua::{LogEntry, LogLevel};
use morf_value::IpcValue;
use morf_app::{PRIMARY_LAYER, WindowId};

use crate::headless::{Headless, LoadOptions};
use crate::headless_input::{button, keysym, modifiers};
use crate::runner_args::{Runner, parse_runner, parse_size};
use crate::runners::is_error;
use crate::test_runner::{Tally, run_spec};

fn load(source: &str) -> Headless {
    let mut options = LoadOptions::new(PathBuf::from("headless-test.lua"));
    options.source = Some(source.as_bytes().to_vec());
    options.size = (800, 600);
    Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error))
}

fn ask(headless: &mut Headless, verb: &str) -> IpcValue {
    headless.runtime.call_ipc(verb, &[]).unwrap()[0].clone()
}

const COUNTER: &str = r#"
    local ui = require("morf.ui")
    morf.surface.anchors = {}
    morf.surface.width = 200
    morf.surface.height = 100
    local clicks, ticks, typed = 0, 0, ""
    morf.ipc.clicks = function() return clicks end
    morf.ipc.ticks = function() return ticks end
    morf.ipc.typed = function() return typed end
    morf.timer(250, function() ticks = ticks + 1 end)
    ui.Item {
        anchors = { fill = true },
        ui.MouseArea {
            id = "button", x = 10, y = 10, width = 50, height = 20,
            on_clicked = function() clicks = clicks + 1 end,
            on_key_pressed = function(_, text) typed = typed .. (text or "") end,
        },
    }
"#;

#[test]
fn a_surface_is_sized_from_its_settings_on_the_screen() {
    let headless = load(COUNTER);
    let primary = &headless.surfaces[0];
    assert_eq!(primary.kind, "primary");
    assert_eq!(primary.size, (200, 100));
    // No anchors: centred, as a compositor places it.
    assert_eq!(primary.position, (300, 250));
    // The default is a bar along the top, the screen's width.
    let bar = load("morf.ui.Item {}");
    assert_eq!(bar.surfaces[0].size, (800, 32));
    assert_eq!(bar.surfaces[0].position, (0, 0));
}

#[test]
fn a_click_reaches_the_area_under_it_and_nothing_beside_it() {
    let mut headless = load(COUNTER);
    let surface = WindowId::Layer(PRIMARY_LAYER);
    headless.click(surface, (20.0, 20.0), 0x110, Default::default()).unwrap();
    headless.click(surface, (150.0, 80.0), 0x110, Default::default()).unwrap();
    assert_eq!(ask(&mut headless, "clicks"), IpcValue::Integer(1));
}

#[test]
fn keys_go_to_the_focused_node() {
    let mut headless = load(COUNTER);
    let surface = WindowId::Layer(PRIMARY_LAYER);
    for name in ["h", "i"] {
        let (code, text) = keysym(name).unwrap();
        headless
            .key(surface, code, text.as_deref(), Default::default())
            .unwrap();
    }
    assert_eq!(ask(&mut headless, "typed"), IpcValue::String("hi".into()));
}

#[test]
fn timers_run_on_the_virtual_clock_without_waiting_for_it() {
    let mut headless = load(COUNTER);
    let started = Instant::now();
    headless.advance(Duration::from_millis(249), Duration::ZERO);
    assert_eq!(ask(&mut headless, "ticks"), IpcValue::Integer(0));
    headless.advance(Duration::from_millis(1), Duration::ZERO);
    assert_eq!(ask(&mut headless, "ticks"), IpcValue::Integer(1));
    // Ten seconds of the configuration's time: forty ticks, each at its own
    // deadline rather than coalesced into frames.
    headless.advance(Duration::from_secs(10), Duration::ZERO);
    assert_eq!(ask(&mut headless, "ticks"), IpcValue::Integer(41));
    assert_eq!(headless.now(), Duration::from_millis(10_250));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_node_is_found_where_it_was_laid_out() {
    let headless = load(COUNTER);
    let layout = headless.surfaces[0].layout.as_ref().unwrap();
    let scene = headless.runtime.scene();
    let root = headless.surfaces[0].root;
    let button = scene.children(root).unwrap()[0];
    assert_eq!(scene.string_value(button, "id").unwrap(), "button");
    let rect = layout.surface_rect(&scene, button).unwrap();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (10.0, 10.0, 50.0, 20.0)
    );
}

#[test]
fn a_configuration_that_fails_says_where() {
    let mut options = LoadOptions::new(PathBuf::from("broken.lua"));
    options.source = Some(b"local x = nil\nreturn x.y".to_vec());
    let failure = Headless::load(&options).err().expect("a failure");
    assert!(failure.error.contains("broken.lua:2"), "{}", failure.error);
}

#[test]
fn window_surfaces_are_listed_with_their_sizes() {
    let headless = load(
        r#"
        local ui = require("morf.ui")
        local window = require("morf.window")
        ui.Item {}
        window.floating { root = ui.Item {}, width = 300, height = 200, title = "Settings",
                          visible = true }
        window.layer { root = ui.Item {}, namespace = "dock", anchors = { bottom = true },
                       width = 400, height = 50, visible = false }
        "#,
    );
    let labels = headless
        .surfaces
        .iter()
        .map(|surface| (surface.label(), surface.size, surface.visible))
        .collect::<Vec<_>>();
    assert!(
        labels.contains(&("floating:Settings".to_owned(), (300, 200), true)),
        "{labels:?}"
    );
    assert!(labels.contains(&("layer:dock".to_owned(), (400, 50), false)));
    assert_eq!(headless.surface_index(Some("dock")).unwrap(), 2);
    assert!(headless.surface_index(Some("nothing")).is_err());
}

#[test]
fn a_lua_error_in_a_callback_is_an_error_and_a_lint_is_not() {
    let entry = |level, message: &str| LogEntry {
        level,
        at_ms: 0,
        message: message.to_owned(),
    };
    assert!(is_error(&entry(
        LogLevel::Warn,
        "timer callback: runtime error: shell.lua:2: boom"
    )));
    assert!(is_error(&entry(LogLevel::Error, "impasto: x did not load")));
    assert!(!is_error(&entry(
        LogLevel::Warn,
        "lint: Item laid out to nothing and has 1 child that will never be seen"
    )));
    assert!(!is_error(&entry(LogLevel::Warn, "appearance: no portal")));
}

#[test]
fn the_runners_read_their_options() {
    let check = parse_runner(
        Runner::Check,
        &[
            "shell.lua",
            "--size",
            "800x600",
            "--screens",
            "2",
            "--strict",
            "--",
            "lock",
        ],
    )
    .unwrap();
    assert_eq!(check.files, [PathBuf::from("shell.lua")]);
    assert_eq!(
        (check.size, check.screens, check.strict),
        ((800, 600), 2, true)
    );
    assert_eq!(check.args, ["lock"]);
    assert!(!check.isolate);

    let render = parse_runner(
        Runner::Render,
        &[
            "shell.lua",
            "-o",
            "out.png",
            "--scale",
            "2",
            "--after",
            "300",
        ],
    )
    .unwrap();
    assert_eq!(render.output, Some(PathBuf::from("out.png")));
    assert_eq!(
        (render.scale, render.after),
        (2, Duration::from_millis(300))
    );
    assert!(parse_runner(Runner::Render, &["shell.lua"]).is_err());

    let test = parse_runner(Runner::Test, &["a.lua", "b.lua", "--filter", "opens"]).unwrap();
    assert_eq!(test.files.len(), 2);
    assert_eq!(test.filter.as_deref(), Some("opens"));
    // A test does not write to the settings of whoever runs it.
    assert!(test.isolate);
    assert!(parse_runner(Runner::Test, &[]).is_err());
    // An option that belongs to another runner is refused, not ignored.
    assert!(parse_runner(Runner::Check, &["shell.lua", "--filter", "x"]).is_err());
    assert!(parse_runner(Runner::Check, &["shell.lua", "extra"]).is_err());
    assert_eq!(parse_size("1280x720").unwrap(), (1280, 720));
    assert!(parse_size("1280").is_err());
    assert!(parse_size("0x10").is_err());
}

#[test]
fn keys_and_buttons_are_named_as_x_names_them() {
    assert_eq!(keysym("Return").unwrap().0, 0xff0d);
    assert_eq!(keysym("Escape").unwrap(), (0xff1b, None));
    assert_eq!(keysym("F5").unwrap().0, 0xffc2);
    assert_eq!(keysym("a").unwrap(), (0x61, Some("a".to_owned())));
    assert_eq!(keysym("é").unwrap().0, 0xe9);
    assert_eq!(keysym("ж").unwrap().0, 0x0100_0000 + 0x436);
    assert!(keysym("NoSuchKey").is_none());
    let held = modifiers(&["ctrl".to_owned(), "Shift".to_owned()]).unwrap();
    assert!(held.ctrl && held.shift && !held.alt);
    assert!(modifiers(&["hyper".to_owned()]).is_err());
    assert_eq!(button("right").unwrap(), 0x111);
    assert!(button("toe").is_err());
}

/// The runner end to end: a spec that passes, one that fails, one that is
/// skipped, and a filter.
#[test]
fn a_spec_runs_its_tests_one_at_a_time() {
    let root = std::env::temp_dir().join(format!("morf-spec-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("counter.lua"), COUNTER).unwrap();
    let spec = root.join("counter_spec.lua");
    std::fs::write(
        &spec,
        r#"
        local test = morf.test
        test.describe("counter", function()
          test.before_each(function() test.load("counter.lua") end)
          test.it("clicks", function()
            test.click { id = "button" }
            test.eq(test.ipc("clicks"), 1)
          end)
          test.it("ticks", function()
            test.advance(500)
            test.eq(test.ipc("ticks"), 2)
          end)
          test.it("fails", function() test.eq(test.ipc("clicks"), 5) end)
          test.it("stubs", function()
            test.stub_run("definitely-not-a-program", { stdout = "ok" })
            test.source([[
              local out = ""
              morf.ipc.out = function() return out end
              morf.run({ "definitely-not-a-program" }, function(r) out = r.stdout end)
              morf.ui.Item {}
            ]])
            test.settle()
            test.eq(test.ipc("out"), "ok")
          end)
          test.skip("later", "not written")
        end)
        "#,
    )
    .unwrap();
    let mut args = parse_runner(Runner::Test, &[spec.to_str().unwrap()]).unwrap();
    let mut number = 0;
    let mut tally = Tally::default();
    run_spec(&spec, &args, &mut number, &mut tally).unwrap();
    assert_eq!(
        (tally.passed, tally.failed, tally.skipped, number),
        (3, 1, 1, 5)
    );

    args.filter = Some("ticks".to_owned());
    let (mut number, mut tally) = (0, Tally::default());
    run_spec(&spec, &args, &mut number, &mut tally).unwrap();
    assert_eq!((tally.passed, number), (1, 1));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn each_spec_file_starts_from_an_empty_home() {
    let base = std::env::temp_dir().join(format!("morf-empty-home-{}", std::process::id()));
    for folder in ["config", "data", "state", "cache"] {
        let path = base.join(folder).join("app");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("settings.json"), "{\"left\":\"over\"}").unwrap();
    }
    std::fs::create_dir_all(base.join("bus")).unwrap();
    crate::headless_env::empty_home(&base);
    for folder in ["config", "data", "state", "cache"] {
        let path = base.join(folder);
        assert!(path.is_dir(), "{folder} is still there to write into");
        assert_eq!(
            std::fs::read_dir(&path).unwrap().count(),
            0,
            "{folder} is empty"
        );
    }
    assert!(base.join("bus").is_dir(), "the private bus is left alone");
    let _ = std::fs::remove_dir_all(&base);
}

// `--screens 0`, `test.load(path, { screens = 0 })`: the shell once every
// output is gone. Only a configuration that asked runs; it maps nothing and
// its timers and IPC go on.
#[test]
fn with_no_screen_only_a_configuration_that_asked_runs_and_maps_nothing() {
    let mut options = LoadOptions::new(PathBuf::from("headless-test.lua"));
    options.screens = 0;
    options.source = Some(COUNTER.as_bytes().to_vec());
    let refused = Headless::load(&options)
        .err()
        .expect("a drawing-only file is refused");
    assert!(refused.error.contains("morf.surface.outputless"));

    let source = format!(
        "morf.surface.outputless = true\n{COUNTER}\n\
         morf.ipc.screens = function() return #morf.screens end"
    );
    options.source = Some(source.into_bytes());
    let mut headless =
        Headless::load(&options).unwrap_or_else(|failure| panic!("{}", failure.error));
    assert!(headless.surfaces.is_empty());
    assert_eq!(ask(&mut headless, "screens"), IpcValue::Integer(0));
    headless.advance(Duration::from_millis(600), Duration::ZERO);
    assert_eq!(ask(&mut headless, "ticks"), IpcValue::Integer(2));
    assert!(headless.surfaces.is_empty());
}
