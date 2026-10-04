//! `morf.fs.watch`: filesystem changes as callbacks on the loop, pumped
//! through `poll_services` as the shell's loop pumps them.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use super::*;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("morf-lua-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn start(source: &str) -> Runtime {
    start_with(Limits::default(), source)
}

fn start_with(limits: Limits, source: &str) -> Runtime {
    let mut runtime = Runtime::new(limits);
    runtime
        .execute(
            "watch.lua",
            format!("morf = require(\"morf\")\nseen = {{}}\n{source}").as_bytes(),
        )
        .unwrap();
    runtime
}

/// Pumps the loop until `check` runs without error, for up to three seconds.
fn wait_for(runtime: &mut Runtime, check: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        runtime.poll_services();
        let error = match runtime.execute("check.lua", check.as_bytes()) {
            Ok(()) => return,
            Err(error) => error,
        };
        if Instant::now() > deadline {
            panic!("never became true: {check}\nlast error: {error}");
        }
        thread::sleep(Duration::from_millis(2));
    }
}

/// Pumps the loop for a while, for what must not happen.
fn pump(runtime: &mut Runtime, for_ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(for_ms);
    while Instant::now() < deadline {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(2));
    }
}

const RECORD: &str = r#"
function record(event)
  seen[#seen + 1] = event.kind .. " " .. event.name .. " " .. event.path
end
"#;

#[test]
fn a_file_is_heard_created_changed_and_deleted() {
    let dir = Scratch::new("file");
    let file = dir.path("settings.json");
    let mut runtime = start(&format!(
        "{RECORD}\nw = morf.fs.watch({file:?}, record)\nassert(w:path() == {file:?})"
    ));
    fs::write(&file, "{}").unwrap();
    wait_for(
        &mut runtime,
        &format!("assert(seen[1] == 'created settings.json ' .. {file:?}, seen[1])"),
    );
    pump(&mut runtime, 100);
    runtime.execute("reset.lua", b"seen = {}").unwrap();
    fs::write(&file, "{ }").unwrap();
    wait_for(
        &mut runtime,
        "assert(seen[1] and seen[1]:find('^changed settings.json'), seen[1])",
    );
    fs::remove_file(&file).unwrap();
    wait_for(
        &mut runtime,
        "assert(seen[#seen]:find('^deleted settings.json'), seen[#seen])",
    );
}

#[test]
fn a_burst_in_one_turn_is_one_callback() {
    let dir = Scratch::new("burst");
    let file = dir.path("history");
    fs::write(&file, "").unwrap();
    let mut runtime = start(&format!("{RECORD}\nw = morf.fs.watch({file:?}, record)"));
    for index in 0..100 {
        fs::write(&file, format!("{index}")).unwrap();
    }
    // An editor's save: written beside, renamed over.
    fs::write(dir.path("history.tmp"), "saved").unwrap();
    fs::rename(dir.path("history.tmp"), &file).unwrap();
    thread::sleep(Duration::from_millis(200));
    runtime.poll_services();
    runtime
        .execute(
            "check.lua",
            b"assert(#seen == 1, #seen) assert(seen[1]:find('^changed history'), seen[1])",
        )
        .unwrap();
}

#[test]
fn a_directory_reports_its_entries() {
    let dir = Scratch::new("dir");
    let root = dir.0.to_string_lossy().into_owned();
    let mut runtime = start(&format!(
        "{RECORD}\nw = morf.fs.watch({root:?}, record, {{ recursive = true }})"
    ));
    fs::create_dir(dir.0.join("sub")).unwrap();
    wait_for(
        &mut runtime,
        "assert(seen[1] and seen[1]:find('^created sub '))",
    );
    pump(&mut runtime, 50);
    fs::write(dir.0.join("sub/entry.lua"), "return 1").unwrap();
    wait_for(
        &mut runtime,
        "local found for _, line in ipairs(seen) do found = found or line:find('^created sub/entry.lua ') end assert(found)",
    );
}

#[test]
fn a_callback_that_closes_its_watch_hears_nothing_after() {
    let dir = Scratch::new("self-close");
    let mut runtime = start(&format!(
        "w = morf.fs.watch({:?}, function(event) seen[#seen + 1] = event.name w:close() end)",
        dir.0.to_string_lossy()
    ));
    for name in ["a", "b", "c"] {
        fs::write(dir.0.join(name), "x").unwrap();
    }
    thread::sleep(Duration::from_millis(200));
    pump(&mut runtime, 50);
    runtime
        .execute("check.lua", b"assert(#seen == 1, #seen)")
        .unwrap();
}

#[test]
fn watches_are_bounded() {
    let dir = Scratch::new("bound");
    let mut runtime = start_with(
        Limits {
            watches: 2,
            ..Limits::default()
        },
        "",
    );
    let source = format!(
        r#"
        a = morf.fs.watch({a:?}, function() end)
        b = morf.fs.watch({b:?}, function() end)
        local ok, err = pcall(morf.fs.watch, {c:?}, function() end)
        assert(not ok and tostring(err):find("more than 2 watches"), tostring(err))
        a:close()
        c = morf.fs.watch({c:?}, function() end)
        assert(c)
        "#,
        a = dir.path("a"),
        b = dir.path("b"),
        c = dir.path("c"),
    );
    runtime.execute("bound.lua", source.as_bytes()).unwrap();
}

#[test]
fn malformed_calls_raise() {
    let mut runtime = start("");
    for call in [
        "morf.fs.watch(nil, function() end)",
        "morf.fs.watch('', function() end)",
        "morf.fs.watch('/tmp', 'not a function')",
        "morf.fs.watch('/tmp', function() end, { recursive = 'yes' })",
    ] {
        let source = format!("assert(not pcall(function() {call} end))");
        runtime.execute("raise.lua", source.as_bytes()).unwrap();
    }
}

// Every screen runs the configuration in a runtime of its own, and each may
// watch the same file (a colour tool's scheme): each hears every change.
#[test]
fn several_runtimes_watching_one_file_each_hear_it() {
    let scratch = Scratch::new("shared");
    let file = scratch.path("colors.json");
    fs::write(&file, "one").unwrap();
    let source = format!(
        r#"count = 0
           handle = morf.fs.watch({file:?}, function() count = count + 1 end)"#
    );
    let mut runtimes: Vec<Runtime> = (0..3).map(|_| start(&source)).collect();
    thread::sleep(Duration::from_millis(50));
    fs::write(&file, "two").unwrap();
    for runtime in &mut runtimes {
        wait_for(runtime, "assert(count >= 1)");
    }
}
