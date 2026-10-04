//! The event socket: events followed and what they touch refetched, a
//! dropped stream reconnected, and event fields parsed.

use super::*;

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
fn hyprland_library_parses_event_fields() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &examples_script(),
            br##"
                local H = require("lib.integrations.hyprland")
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
