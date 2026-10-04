//! Tests of fs_watch.rs that reach the runtime's internals; the rest are in
//! tests/engine/fs_watch.rs.
#![allow(unused_imports)]

use super::*;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn a_closed_watch_calls_back_no_more() {
    let dir = Scratch::new("close");
    let file = dir.path("f");
    let mut runtime = start(&format!(
        "{RECORD}\nw = morf.fs.watch({file:?}, record)\nw:close()\nassert(w:closed())"
    ));
    fs::write(&file, "x").unwrap();
    pump(&mut runtime, 200);
    runtime
        .execute("check.lua", b"assert(#seen == 0, #seen)")
        .unwrap();
    assert_eq!(runtime.reactive.borrow().watches.len(), 0);
}

#[test]
fn a_forgotten_handle_is_closed_when_collected() {
    let dir = Scratch::new("gc");
    let file = dir.path("f");
    let mut runtime = start(&format!(
        "{RECORD}\nlost = 0\ndo morf.fs.watch({file:?}, function() lost = lost + 1 end) end\nkept = morf.fs.watch({file:?}, record)"
    ));
    assert_eq!(runtime.reactive.borrow().watches.len(), 2);
    runtime.lua.gc_collect();
    runtime.poll_services();
    assert_eq!(runtime.reactive.borrow().watches.len(), 1);
    fs::write(&file, "x").unwrap();
    wait_for(&mut runtime, "assert(#seen >= 1)");
    pump(&mut runtime, 100);
    runtime
        .execute("check.lua", b"assert(lost == 0, lost)")
        .unwrap();
}

#[test]
fn twenty_idle_watches_cost_no_thread_each() {
    let dir = Scratch::new("twenty");
    let mut source = String::new();
    for index in 0..20 {
        source.push_str(&format!(
            "w{index} = morf.fs.watch({:?}, function() end)\n",
            dir.path(&format!("file-{index}"))
        ));
    }
    let runtime = start(&source);
    assert_eq!(runtime.reactive.borrow().watches.len(), 20);
    assert!(morf_io::watcher_threads() <= 2);
}

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
