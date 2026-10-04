//! `library/lib/hyprland.lua` against a fake Hyprland.
//!
//! The library is pure Lua over `morf.socket`, so what needs testing is the
//! conversation: that it asks the request socket the right questions, reads
//! the event socket's lines into typed events and fresh state, survives lines
//! a real compositor would never send, and comes back when the stream drops.
//! Two Unix listeners in a temporary `$XDG_RUNTIME_DIR/hypr/<signature>`
//! stand in for the compositor; nothing here touches the session's Hyprland.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

const MONITORS: &str = r#"[
  {"id": 0, "name": "eDP-1", "description": "Panel", "width": 1920, "height": 1080,
   "x": 0, "y": 0, "scale": 1.5, "transform": 0, "refreshRate": 60.0, "focused": false,
   "activeWorkspace": {"id": 3, "name": "3"}, "specialWorkspace": {"id": 0, "name": ""},
   "disabled": false},
  {"id": 1, "name": "DP-1", "description": "Desk", "width": 3840, "height": 2160,
   "x": 1920, "y": 0, "scale": 1, "transform": 0, "refreshRate": 59.99, "focused": true,
   "activeWorkspace": {"id": 1, "name": "1"}, "specialWorkspace": {"id": 0, "name": ""},
   "disabled": false}
]"#;

const WORKSPACES: &str = r#"[
  {"id": 1, "name": "1", "monitor": "DP-1", "monitorID": 1, "windows": 2,
   "hasfullscreen": false, "lastwindow": "0xcafe", "lastwindowtitle": "editor",
   "ispersistent": true},
  {"id": 3, "name": "3", "monitor": "eDP-1", "monitorID": 0, "windows": 0,
   "hasfullscreen": false, "lastwindow": "0x0", "lastwindowtitle": "",
   "ispersistent": false},
  {"id": 2, "name": "2", "monitor": "DP-1", "monitorID": 1, "windows": 0,
   "hasfullscreen": false, "lastwindow": "0x0", "lastwindowtitle": "",
   "ispersistent": false}
]"#;

const CLIENTS: &str = r#"[
  {"address": "0xcafe", "mapped": true, "hidden": false, "at": [10, 20], "size": [800, 600],
   "workspace": {"id": 1, "name": "1"}, "floating": false, "monitor": 1,
   "class": "editor", "title": "editor", "initialClass": "editor", "initialTitle": "editor",
   "pid": 10, "xwayland": false, "pinned": false, "fullscreen": 0, "focusHistoryID": 0},
  {"address": "0xf00d", "mapped": true, "hidden": false, "at": [820, 20], "size": [800, 600],
   "workspace": {"id": 1, "name": "1"}, "floating": true, "monitor": 1,
   "class": "term", "title": "shell", "initialClass": "term", "initialTitle": "shell",
   "pid": 11, "xwayland": true, "pinned": false, "fullscreen": 0, "focusHistoryID": 1}
]"#;

const DEVICES: &str = r#"{"mice": [], "keyboards": [
  {"address": "0x1", "name": "video-bus", "active_keymap": "English (US)", "main": false},
  {"address": "0x2", "name": "kb", "active_keymap": "English (US)", "main": true}
], "tablets": [], "touch": [], "switches": []}"#;

/// What the fake compositor answers and remembers.
struct Fake {
    replies: Mutex<HashMap<String, String>>,
    requests: Mutex<Vec<String>>,
    events: Mutex<Option<UnixStream>>,
    accepts: AtomicUsize,
    stop: AtomicBool,
}

impl Fake {
    fn set(&self, request: &str, reply: &str) {
        self.replies
            .lock()
            .unwrap()
            .insert(request.to_owned(), reply.to_owned());
    }

    fn answer(&self, request: &str) -> String {
        if let Some(batch) = request.strip_prefix("[[BATCH]]") {
            return batch
                .split(';')
                .map(|one| self.answer(one) + "\n\n\n")
                .collect();
        }
        if request.starts_with("/dispatch")
            || request.starts_with("/keyword")
            || request.starts_with("/reload")
        {
            return "ok".to_owned();
        }
        if let Some(chunk) = request.strip_prefix("/eval ") {
            return format!("evaluated {chunk}");
        }
        self.replies
            .lock()
            .unwrap()
            .get(request)
            .cloned()
            .unwrap_or_else(|| "unknown request".to_owned())
    }

    fn count(&self, request: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|seen| seen.as_str() == request)
            .count()
    }

    fn send(&self, lines: &str) {
        let mut events = self.events.lock().unwrap();
        let stream = events.as_mut().expect("the library is listening");
        stream.write_all(lines.as_bytes()).unwrap();
        stream.flush().unwrap();
    }

    /// Drops the event connection, as a compositor that exits does.
    fn hang_up(&self) {
        if let Some(stream) = self.events.lock().unwrap().take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

fn accept_loop(listener: UnixListener, fake: Arc<Fake>, serve: fn(&Fake, UnixStream)) {
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        while !fake.stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    serve(&fake, stream);
                }
                Err(_) => thread::sleep(Duration::from_millis(2)),
            }
        }
    });
}

fn serve_request(fake: &Fake, mut stream: UnixStream) {
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut bytes = vec![0; 64 * 1024];
    let Ok(read) = stream.read(&mut bytes) else {
        return;
    };
    let request = String::from_utf8_lossy(&bytes[..read]).into_owned();
    let reply = fake.answer(&request);
    fake.requests.lock().unwrap().push(request);
    let _ = stream.write_all(reply.as_bytes());
}

fn serve_events(fake: &Fake, stream: UnixStream) {
    fake.accepts.fetch_add(1, Ordering::SeqCst);
    *fake.events.lock().unwrap() = Some(stream);
}

struct Instance {
    root: PathBuf,
    fake: Arc<Fake>,
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.fake.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fake_hyprland(tag: &str) -> Instance {
    let root = std::env::temp_dir().join(format!("morf-hypr-{tag}-{}", std::process::id()));
    let directory = root.join("hypr").join("fake");
    std::fs::create_dir_all(&directory).unwrap();
    let fake = Arc::new(Fake {
        replies: Mutex::new(HashMap::new()),
        requests: Mutex::new(Vec::new()),
        events: Mutex::new(None),
        accepts: AtomicUsize::new(0),
        stop: AtomicBool::new(false),
    });
    fake.set("j/monitors all", MONITORS);
    fake.set("j/workspaces", WORKSPACES);
    fake.set("j/clients", CLIENTS);
    fake.set("j/devices", DEVICES);
    fake.set("j/submap", "\"default\"");
    fake.set("j/cursorpos", "{\"x\": 12, \"y\": 34}");
    fake.set(
        "j/getoption general:gaps_out",
        r#"{"option": "general:gaps_out", "css": "10 10 10 10", "set": true}"#,
    );
    fake.set("j/version", r#"{"version": "0.56.2", "tag": "v0.56.2"}"#);
    accept_loop(
        UnixListener::bind(directory.join(".socket.sock")).unwrap(),
        Arc::clone(&fake),
        serve_request,
    );
    accept_loop(
        UnixListener::bind(directory.join(".socket2.sock")).unwrap(),
        Arc::clone(&fake),
        serve_events,
    );
    Instance { root, fake }
}

fn examples_script() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../library/hyprland-test.lua")
        .to_string_lossy()
        .into_owned()
}

/// Pumps the runtime until `check` runs without error, or fails the test
/// with the last error after a few seconds.
fn wait_for(runtime: &mut Runtime, check: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        runtime.poll_services();
        let error = match runtime.execute("check.lua", check.as_bytes()) {
            Ok(()) => return,
            Err(error) => error,
        };
        if Instant::now() > deadline {
            panic!("never became true: {check}\nlast error: {error}");
        }
        thread::sleep(Duration::from_millis(3));
    }
}

fn start(runtime: &mut Runtime, instance: &Instance) {
    let source = format!(
        r#"
            H = require("lib.integrations.hyprland")
            seen = {{ ws = 0, stars = 0, connects = 0, disconnects = 0 }}
            H.on("openwindow", function(address, workspace, class, title)
                seen.open = {{ address, workspace, class, title }}
            end)
            workspace_sub = H.on("workspacev2", function(id, name)
                seen.ws = seen.ws + 1
                seen.ws_id = id
            end)
            H.on("*", function() seen.stars = seen.stars + 1 end)
            H.on("connected", function() seen.connects = seen.connects + 1 end)
            H.on("disconnected", function() seen.disconnects = seen.disconnects + 1 end)
            assert(H.start {{
                runtime_dir = {:?}, signature = "fake",
                poll_ms = 4, reconnect_min_ms = 8, reconnect_max_ms = 40,
            }})
            assert(H.available())
        "#,
        instance.root.to_string_lossy()
    );
    runtime
        .execute(&examples_script(), source.as_bytes())
        .unwrap();
}

#[test]
fn hyprland_library_is_harmless_without_hyprland() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &examples_script(),
            br#"
                local H = require("lib.integrations.hyprland")
                assert(H.start { signature = "" } == false)
                assert(not H.available())
                local answered = nil
                assert(H.request("j/monitors", function(value, err) answered = err end) == false)
                assert(answered == "unavailable")
                assert(H.dispatch("workspace", 1) == false)
                assert(H.state.monitors:len() == 0 and H.state.clients:len() == 0)
                assert(H.state.connected == false and H.state.active_workspace.id == 0)
                assert(#H.workspace_windows(1) == 0 and H.monitor_workspace() == nil)
                H.stop()
            "#,
        )
        .unwrap();
    runtime.poll_services();
}

mod events;
mod live;
mod requests;
