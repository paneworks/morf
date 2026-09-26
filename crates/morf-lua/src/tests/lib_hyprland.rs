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
        .join("../../examples/hyprland-test.lua")
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
            H = require("lib.hyprland")
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
fn hyprland_library_fills_state_from_the_request_socket() {
    let instance = fake_hyprland("state");
    let mut runtime = Runtime::default();
    start(&mut runtime, &instance);

    wait_for(
        &mut runtime,
        r#"
            local s = H.state
            assert(s.connected)
            assert(s.monitors:len() == 2, "monitors")
            assert(s.workspaces:len() == 3, "workspaces")
            assert(s.clients:len() == 2, "clients")
            assert(s.keyboard_layout == "English (US)", "layout")
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                local s = H.state
                assert(s.focused_monitor == "DP-1")
                assert(s.active_workspace.id == 1 and s.active_workspace.name == "1")
                assert(s.keyboard == "kb")
                assert(s.submap == "")
                assert(s.fullscreen == false)
                -- Sorted by id, whatever order the compositor answered in.
                assert(s.workspaces:get(1).id == 1 and s.workspaces:get(3).id == 3)
                assert(s.workspaces:get(1).active and not s.workspaces:get(2).active)
                assert(s.workspaces:get(3).visible, "shown on the other monitor")
                assert(s.monitors:get(1).name == "eDP-1" and s.monitors:get(1).active_workspace == 3)
                local term = H.client("f00d")
                assert(term and term.floating and term.xwayland and term.width == 800)
                assert(#H.workspace_windows(1) == 2 and #H.workspace_windows(2) == 0)
                assert(H.occupied(1) and not H.occupied(2))
                assert(H.monitor_workspace("eDP-1") == 3 and H.monitor_workspace() == 1)
                assert(H.monitor("DP-1").scale == 1 and H.workspace(3).monitor == "eDP-1")
                assert(#H.snapshot().clients == 2)
            "#,
        )
        .unwrap();
}

#[test]
fn hyprland_library_follows_events_and_refetches_what_they_touch() {
    let instance = fake_hyprland("events");
    let fake = Arc::clone(&instance.fake);
    let mut runtime = Runtime::default();
    start(&mut runtime, &instance);
    wait_for(&mut runtime, "assert(H.state.clients:len() == 2)");
    let clients_before = fake.count("j/clients");
    let devices_before = fake.count("j/devices");

    // The compositor has moved on: a third window on workspace 2, now shown.
    fake.set(
        "j/monitors all",
        &MONITORS.replace(r#""id": 1, "name": "1""#, r#""id": 2, "name": "2""#),
    );
    fake.set(
        "j/workspaces",
        &WORKSPACES.replace(
            r#""id": 2, "name": "2", "monitor": "DP-1", "monitorID": 1, "windows": 0"#,
            r#""id": 2, "name": "2", "monitor": "DP-1", "monitorID": 1, "windows": 1"#,
        ),
    );
    fake.set(
        "j/clients",
        &CLIENTS.replace(
            "\n]",
            r#",
  {"address": "0xbeef", "at": [0, 0], "size": [100, 100], "workspace": {"id": 2, "name": "2"},
   "class": "kitty", "title": "a, title", "monitor": 1}
]"#,
        ),
    );
    let overlong = "x".repeat(70 * 1024);
    fake.send(&format!(
        "workspace>>2\nworkspacev2>>2,2\ngarbage without separator\n>>no name\n\
         {overlong}\nactivewindow>>kitty,a, title\nactivewindowv2>>beef\n\
         openwindow>>beef,2,kitty,a, title\nmovewindowv2>>beef,2,2\n\
         submap>>resize\nactivelayout>>video-bus,Russian\nactivelayout>>kb,German\n\
         moveworkspacev2>>4,odd, name,DP-1\n"
    ));
    wait_for(
        &mut runtime,
        r#"
            local s = H.state
            assert(s.clients:len() == 3)
            assert(H.occupied(2))
            assert(s.active_workspace.id == 2)
            assert(seen.open)
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                local s = H.state
                assert(seen.ws == 1 and seen.ws_id == 2, "ws " .. seen.ws)
                assert(seen.open[1] == "0xbeef" and seen.open[2] == "2", "open")
                assert(seen.open[3] == "kitty" and seen.open[4] == "a, title", "open title")
                assert(s.active_window.address == "0xbeef", "address " .. s.active_window.address)
                assert(s.active_window.class == "kitty" and s.active_window.title == "a, title", "class " .. s.active_window.class .. "/" .. s.active_window.title)
                assert(H.client("0xbeef").active and not H.client("0xcafe").active, "active flags")
                assert(s.submap == "resize", "submap " .. s.submap)
                -- Only the main keyboard's layout is the one shown.
                assert(s.keyboard_layout == "German", "layout " .. s.keyboard_layout)
                assert(#H.workspace_windows(2) == 1, "windows")
            "#,
        )
        .unwrap();
    // Four window events in one burst, one or two fetches of the windows --
    // not four -- and none of the devices, which nothing touched.
    let clients_fetched = fake.count("j/clients") - clients_before;
    assert!(
        (1..=2).contains(&clients_fetched),
        "clients fetched {clients_fetched} times"
    );
    assert_eq!(fake.count("j/devices"), devices_before);

    // A handle's `off` stops that handler and no other.
    runtime
        .execute(
            "check.lua",
            b"workspace_sub:off(); stars_before = seen.stars",
        )
        .unwrap();
    // Back on workspace 1, which now has a fullscreen window: the fake's
    // answers agree with the events, as the compositor's would.
    fake.set("j/monitors all", MONITORS);
    fake.set(
        "j/workspaces",
        &WORKSPACES.replacen(r#""hasfullscreen": false"#, r#""hasfullscreen": true"#, 1),
    );
    fake.send("workspacev2>>1,1\nurgent>>cafe\nfullscreen>>1\n");
    wait_for(
        &mut runtime,
        r#"
            assert(H.state.urgent == "0xcafe")
            assert(H.state.fullscreen == true)
            assert(seen.stars >= stars_before + 3)
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                assert(seen.ws == 1, "unsubscribed handler ran")
                assert(H.client("cafe").urgent)
            "#,
        )
        .unwrap();
    fake.send("activewindowv2>>cafe\n");
    wait_for(
        &mut runtime,
        r#"assert(H.state.urgent == "" and not H.client("cafe").urgent)"#,
    );
}

#[test]
fn hyprland_library_sends_commands_and_batches() {
    let instance = fake_hyprland("commands");
    let fake = Arc::clone(&instance.fake);
    let mut runtime = Runtime::default();
    start(&mut runtime, &instance);
    wait_for(&mut runtime, "assert(H.state.monitors:len() == 2)");

    runtime
        .execute(
            "check.lua",
            br#"
                answers = {}
                H.dispatch("workspace", 3, function(ok, reply) answers.dispatch = { ok, reply } end)
                H.keyword("general:gaps_out", 8, function(ok) answers.keyword = ok end)
                H.eval("hl.dispatch(hl.dsp.focus({ workspace = 3 }))",
                    function(reply) answers.eval = reply end)
                H.reload(true, function(ok) answers.reload = ok end)
                H.cursor_position(function(x, y) answers.cursor = { x, y } end)
                H.getoption("general:gaps_out", function(option) answers.option = option end)
                H.options({ "general:gaps_out", "nope" }, function(map) answers.options = map end)
                H.version(function(version) answers.version = version end)
                H.json("nonsense", function(value, err) answers.bad = { value, err } end)
                H.batch({ "j/cursorpos", "j/submap" }, function(replies)
                    answers.batch = replies
                end)
                assert(H.batch({ "a;b" }, function(r, err) answers.refused = err end) == false)
            "#,
        )
        .unwrap();
    wait_for(
        &mut runtime,
        r#"
            assert(answers.dispatch and answers.keyword ~= nil and answers.eval)
            assert(answers.reload ~= nil and answers.cursor and answers.option)
            assert(answers.options and answers.version and answers.bad and answers.batch)
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                assert(answers.dispatch[1] == true and answers.dispatch[2] == "ok")
                assert(answers.keyword == true and answers.reload == true)
                assert(answers.eval:find("evaluated hl.dispatch", 1, true))
                assert(answers.cursor[1] == 12 and answers.cursor[2] == 34)
                assert(answers.option.css == "10 10 10 10")
                assert(answers.options["general:gaps_out"].set == true)
                assert(answers.options["nope"] == nil)
                assert(answers.version.version == "0.56.2")
                assert(answers.bad[1] == nil and answers.bad[2] == "unknown request")
                assert(#answers.batch == 2 and answers.batch[2] == '"default"')
                assert(answers.batch[1]:find('"x": 12', 1, true))
                assert(answers.refused == "batched command contains ';'")
            "#,
        )
        .unwrap();
    let requests = fake.requests.lock().unwrap().clone();
    for expected in [
        "/dispatch workspace 3",
        "/keyword general:gaps_out 8",
        "/eval hl.dispatch(hl.dsp.focus({ workspace = 3 }))",
        "/reload config-only",
        "[[BATCH]]j/cursorpos;j/submap",
        "j/nonsense",
    ] {
        assert!(
            requests.iter().any(|seen| seen == expected),
            "{expected} was not sent: {requests:?}"
        );
    }
}

#[test]
fn hyprland_library_reconnects_when_the_stream_drops() {
    let instance = fake_hyprland("reconnect");
    let fake = Arc::clone(&instance.fake);
    let mut runtime = Runtime::default();
    start(&mut runtime, &instance);
    wait_for(
        &mut runtime,
        "assert(H.state.connected and seen.connects == 1)",
    );
    let monitors_before = fake.count("j/monitors all");
    // The client hears it connected once the kernel queued the connection;
    // the fake holds the stream only once its accept loop has taken it. A
    // hang-up before that has nothing to hang up.
    let deadline = Instant::now() + Duration::from_secs(5);
    while fake.events.lock().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the fake never took the stream");
        thread::sleep(Duration::from_millis(1));
    }

    fake.hang_up();
    wait_for(
        &mut runtime,
        "assert(seen.disconnects == 1 and seen.connects == 2 and H.state.connected)",
    );
    assert_eq!(fake.accepts.load(Ordering::SeqCst), 2);
    // A reconnect asks for everything again: what happened meanwhile is not
    // known.
    let deadline = Instant::now() + Duration::from_secs(5);
    while fake.count("j/monitors all") == monitors_before {
        assert!(Instant::now() < deadline, "no refetch after reconnecting");
        runtime.poll_services();
        thread::sleep(Duration::from_millis(3));
    }
    fake.send("submap>>after\n");
    wait_for(&mut runtime, r#"assert(H.state.submap == "after")"#);
}

#[test]
fn hyprland_library_is_harmless_without_hyprland() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &examples_script(),
            br#"
                local H = require("lib.hyprland")
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

#[test]
fn hyprland_library_parses_event_fields() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &examples_script(),
            br##"
                local H = require("lib.hyprland")
                H.start { signature = "" }
                local function same(a, b) assert(a == b, tostring(a) .. " ~= " .. tostring(b)) end
                local id, name, monitor = H.parse_event("moveworkspacev2", "3,a,b,DP-1")
                same(id, 3); same(name, "a,b"); same(monitor, "DP-1")
                name, monitor = H.parse_event("activespecial", "special:x,eDP-1")
                same(name, "special:x"); same(monitor, "eDP-1")
                local address, floating = H.parse_event("changefloatingmode", "abc,1")
                same(address, "0xabc"); same(floating, true)
                same(H.parse_event("activewindowv2", ","), "")
                same(H.parse_event("activewindowv2", ""), "")
                local open, members = H.parse_event("togglegroup", "1,a,b")
                same(open, true); same(members[2], "0xb")
                id, name = H.parse_event("workspacev2", "not a number")
                same(id, nil); same(name, "")
                local _, _, description = H.parse_event("monitoraddedv2", "1,DP-2,Big, bright")
                same(description, "Big, bright")
                same(H.parse_event("somethingnew", "raw,data"), "raw,data")
                same(select("#", H.parse_event("configreloaded", "")), 0)
            "##,
        )
        .unwrap();
}

/// Reads the session's own Hyprland, never writing to it: `j/` queries and
/// the event stream only. Run by hand with `--ignored` under Hyprland to check
/// the parsing against real answers.
#[test]
#[ignore]
fn hyprland_library_reads_the_live_compositor() {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_err() {
        return;
    }
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &examples_script(),
            br#"
                H = require("lib.hyprland")
                assert(H.start { poll_ms = 10 })
                live = {}
                H.version(function(v) live.version = v end)
                H.layers(function(v) live.layers = v end)
                H.binds(function(v) live.binds = v end)
                H.cursor_position(function(x, y) live.cursor = { x, y } end)
                H.getoption("general:gaps_out", function(v) live.gaps = v end)
            "#,
        )
        .unwrap();
    wait_for(
        &mut runtime,
        r#"
            local s = H.state
            assert(s.monitors:len() > 0 and s.workspaces:len() > 0)
            assert(s.focused_monitor ~= "" and s.active_workspace.id ~= 0)
            assert(s.keyboard_layout ~= "")
            assert(live.version and live.layers and live.binds and live.cursor and live.gaps)
        "#,
    );
    runtime
        .execute(
            "check.lua",
            br#"
                local s = H.state
                local lines = { "version " .. tostring(live.version.version),
                    "focused " .. s.focused_monitor .. " ws " .. s.active_workspace.id
                        .. " (" .. s.active_workspace.name .. ")",
                    "layout " .. s.keyboard .. ": " .. s.keyboard_layout,
                    "active " .. s.active_window.address .. " " .. s.active_window.class,
                    "cursor " .. tostring(live.cursor[1]) .. "," .. tostring(live.cursor[2]),
                    "binds " .. #live.binds }
                for i = 1, s.monitors:len() do
                    local m = s.monitors:get(i)
                    lines[#lines + 1] = "monitor " .. m.name .. " shows " .. m.active_workspace
                end
                for i = 1, s.workspaces:len() do
                    local w = s.workspaces:get(i)
                    lines[#lines + 1] = "workspace " .. w.id .. " " .. w.name .. " on "
                        .. w.monitor .. " windows " .. w.windows
                        .. " listed " .. #H.workspace_windows(w.id)
                end
                report = table.concat(lines, "\n")
            "#,
        )
        .unwrap();
    // Surfaced as an error only because an error is what reaches the test.
    let report = runtime
        .execute("report.lua", b"H.stop(); error(report, 0)")
        .unwrap_err();
    eprintln!("{report}");
}
