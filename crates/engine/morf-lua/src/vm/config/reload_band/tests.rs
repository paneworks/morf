use super::*;

struct NoText;

impl TextMeasurer for NoText {
    fn measure(
        &mut self,
        _node: NodeHandle,
        text: &str,
        _family: &str,
        size: f64,
        _options: morf_layout::TextOptions,
    ) -> Size {
        Size {
            width: text.len() as f64 * size / 2.0,
            height: size,
        }
    }
}

fn shell(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime.execute("shell.lua", source.as_bytes()).unwrap();
    runtime
}

fn logged(runtime: &mut Runtime, needle: &str) -> bool {
    let logs = runtime.take_logs();
    logs.iter().any(|entry| entry.message.contains(needle))
}

#[test]
fn a_band_that_cannot_be_laid_out_is_taken_down_and_logged() {
    let mut runtime = shell("morf.ui.Item {}");
    assert!(runtime.report_reload_failure("shell.lua:1: boom", Path::new("shell.lua")));
    assert!(
        runtime
            .lay_out_reload_band((800, 600), &mut NoText)
            .is_some()
    );
    let label = runtime.reload_error.borrow().as_ref().unwrap().label;
    let bogus = Value::String("bogus".to_owned());
    runtime.scene_mut().assign(label, "elide", bogus).unwrap();
    assert!(
        runtime
            .lay_out_reload_band((800, 600), &mut NoText)
            .is_none()
    );
    assert!(!runtime.reload_error_shown());
    assert!(logged(&mut runtime, "could not be laid out"));
}

#[test]
fn a_handler_that_throws_has_not_reported_the_failure() {
    let mut runtime = shell("morf.on_reload_failed(function() error('broke') end)");
    assert!(runtime.report_reload_failure("shell.lua:1: boom", Path::new("shell.lua")));
    assert!(logged(&mut runtime, "broke"));
    let mut runtime = shell("morf.on_reload_failed(function() end)");
    assert!(!runtime.report_reload_failure("shell.lua:1: boom", Path::new("shell.lua")));
    assert!(!runtime.reload_error_shown());
}

#[test]
fn a_handler_registered_while_the_band_is_up_reports_it_and_takes_it_down() {
    let mut runtime = shell(
        "local heard = '' morf.ipc.heard = function() return heard end
         morf.ipc.listen = function()
           morf.on_reload_failed(function(error) heard = error end)
         end",
    );
    assert!(runtime.report_reload_failure("shell.lua:1: boom", Path::new("shell.lua")));
    runtime.poll_services();
    assert!(runtime.reload_error_shown());
    runtime.call_ipc("listen", &[]).unwrap();
    assert!(runtime.poll_services());
    assert!(!runtime.reload_error_shown());
    let heard = runtime.call_ipc("heard", &[]).unwrap();
    assert_eq!(heard, [IpcValue::String("shell.lua:1: boom".to_owned())]);
}

#[test]
fn the_band_wraps_to_the_surface_and_stops_at_its_cap() {
    let mut runtime = shell("morf.ui.Item {}");
    let long = "word ".repeat(2000);
    runtime.report_reload_failure(&long, Path::new("shell.lua"));
    let mut text = morf_text::TextSystem::new();
    let band = runtime.lay_out_reload_band((800, 600), &mut text).unwrap();
    assert_eq!(band.bounds.width, 800.0);
    assert!(
        band.bounds.height > 3.0 * FONT_SIZE,
        "{}",
        band.bounds.height
    );
    assert!(band.bounds.height <= BAND_CAP, "{}", band.bounds.height);
    let label = runtime.reload_error.borrow().as_ref().unwrap().label;
    let label = band.layout.geometry(label).unwrap();
    assert!(
        label.x >= 0.0 && label.x + label.width <= 800.0,
        "{label:?}"
    );
    assert!(label.y + label.height <= band.bounds.height, "{label:?}");
    let bar = runtime.lay_out_reload_band((800, 32), &mut text).unwrap();
    assert!(bar.bounds.height <= 32.0, "{}", bar.bounds.height);
}

#[test]
fn a_runtime_error_names_its_file_and_line() {
    let error = "Lua error: runtime error: /home/me/shell/bar.lua:21: attempt to index";
    assert_eq!(
        error_location(error, Path::new("/home/me/shell/init.lua")),
        Some((PathBuf::from("/home/me/shell/bar.lua"), 21))
    );
}

#[test]
fn a_parse_error_names_its_line_in_the_file_before_it() {
    let error = "could not load Lua: /c/init.lua: runtime error: parse error: \
                 parse error at line 3: unexpected end of token stream";
    let path = Path::new("/c/init.lua");
    assert_eq!(
        error_location(error, path),
        Some((PathBuf::from("/c/init.lua"), 3))
    );
    assert_eq!(
        error_location("parse error at line 7: oops", path),
        Some((path.to_path_buf(), 7))
    );
    assert_eq!(error_location("no line here", path), None);
}

#[test]
fn the_message_says_where_and_what() {
    let failure = ReloadFailure::new("/c/init.lua:4: boom", Path::new("/c/init.lua"));
    let message = failure.message();
    assert!(message.starts_with("reload of /c/init.lua failed at line 4"));
    assert!(message.ends_with("\n/c/init.lua:4: boom"));
    let failure = ReloadFailure::new("/c/mod.lua:9: boom", Path::new("/c/init.lua"));
    assert!(failure.message().contains("failed at /c/mod.lua:9;"));
}
