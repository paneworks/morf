//! The `private_bus_` tests: MPRIS and notifications over a real session
//! bus, run only inside `dbus-run-session` by
//! `services_over_a_private_session_bus`.

use super::*;

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_mpris_round_trip() {
    // Refuses to run on an ambient bus: `--ignored` by hand would otherwise
    // serve a name on whatever session the shell it was typed in belongs to.
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let prefix = format!("org.morf.test.mpris{}.", std::process::id());
    let player_name = format!("{prefix}fake");
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<bool>();
    let server = {
        let stop = Arc::clone(&stop);
        let player_name = player_name.clone();
        thread::spawn(move || {
            let mut runtime = Runtime::default();
            let source = format!(
                r#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local log = morf.signal("player.log", "")
                ui.Text {{ text = function() return log:get() end }}
                local PATH = "/org/mpris/MediaPlayer2"
                local PLAYER = "org.mpris.MediaPlayer2.Player"
                local service, outcome = morf.dbus.serve("session", "{player_name}", PATH, false)
                assert(outcome == "owned", outcome)
                local status = "Paused"
                local function player()
                    return {{
                        PlaybackStatus = status, Rate = 1.0, Volume = 0.5,
                        Position = {{ signature = "x", value = 42000000 }},
                        CanPlay = true, CanPause = true, CanSeek = true, CanControl = true,
                        Metadata = {{
                            ["xesam:title"] = "Wire",
                            ["xesam:artist"] = {{ "One", "Two" }},
                            ["mpris:length"] = {{ signature = "x", value = 180000000 }},
                            ["mpris:trackid"] = {{ signature = "o", value = "/org/morf/track/7" }},
                        }},
                    }}
                end
                service:on_call(function(call)
                    if call.member == "GetAll" then
                        if call.arguments[1] == PLAYER then
                            service:reply(call.id, player())
                        else
                            service:reply(call.id, {{ Identity = "Fake", DesktopEntry = "fake" }})
                        end
                    elseif call.member == "PlayPause" then
                        log:set(log:get() .. "PlayPause;")
                        status = status == "Playing" and "Paused" or "Playing"
                        service:reply(call.id, nil)
                        service:emit(PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged",
                            {{ PLAYER, {{ PlaybackStatus = status }}, {{ signature = "as", value = {{}} }} }})
                    else
                        service:reply_error(call.id, "org.freedesktop.DBus.Error.UnknownMethod", call.member)
                    end
                end)
                "#
            );
            let started = runtime.execute("fake-player.lua", source.as_bytes());
            let _ = ready_tx.send(started.is_ok());
            if started.is_err() {
                return String::new();
            }
            while !stop.load(Ordering::Relaxed) {
                runtime.poll_services();
                thread::sleep(Duration::from_millis(1));
            }
            let root = runtime.scene().roots()[0];
            runtime
                .scene()
                .string_value(root, "text")
                .unwrap()
                .to_owned()
        })
    };
    if !ready_rx.recv().unwrap() {
        stop.store(true, Ordering::Relaxed);
        let _ = server.join();
        return;
    }

    let body = format!(
        r#"
        local media = require("lib.services.mpris").connect({{ prefix = "{prefix}", debounce_ms = 10 }})
        local s = media.state
        local eq = fake.eq
        fake.steps({{
            function()
                eq(s.count, 1, "found by ListNames")
                eq(s.active.identity, "Fake", "root interface read")
                eq(s.active.title, "Wire", "metadata through a variant")
                eq(s.active.artist, "One, Two", "an `as` inside a variant")
                eq(s.active.length, 180.0, "an `x` inside a variant")
                eq(s.active.track_id, "/org/morf/track/7", "an `o` inside a variant")
                eq(s.active.position, 42.0, "position")
                eq(s.active.status, "paused", "status")
                assert(media.play_pause())
            end,
            function() end,
            function()
                eq(s.active.status, "playing", "PropertiesChanged over the bus, re-read")
            end,
        }}, done, 300)
        "#
    );
    let verdict = run_with_fake("test-mpris-bus", &body);
    stop.store(true, Ordering::Relaxed);
    let log = server.join().unwrap();
    assert_eq!(verdict, "ok");
    assert_eq!(log, "PlayPause;", "the button reached the player once");
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_notification_with_an_image_and_a_resident_action() {
    // The notification server owns `org.freedesktop.Notifications` with
    // `replace` -- which on a live session would take the name from the
    // desktop's own daemon. That is why this runs only on a private bus.
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let mut runtime = Runtime::default();
    let path = format!(
        "{}/../../../library/test-notifications-bus.lua",
        env!("CARGO_MANIFEST_DIR")
    );
    runtime
        .execute(
            &path,
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local notifications = require("lib.services.notifications")
            local shown = morf.signal("shown", "waiting")
            ui.Text { text = function() return shown:get() end }
            local server
            server = assert(notifications.serve {
                on_change = function(list)
                    local n = list[1]
                    if not n or shown:get() ~= "waiting" then return end
                    local image = n.image_data
                    local described = table.concat({
                        n.summary, n.image_path, image and (image.width .. "x" .. image.height) or "none",
                        image and tostring(#image.data) or "0", tostring(image and image.has_alpha),
                        tostring(n.urgency), n.category, n.desktop_entry, tostring(n.resident),
                        n.image_source and n.image_source:match("^memory:image/") or "no source",
                    }, "|")
                    morf.timer(1, function()
                        server.invoke(n.id, "open")
                        shown:set(described .. "|after=" .. #server.list)
                    end, false)
                end,
            })
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];

    let caller = thread::spawn(|| {
        use morf_io::DbusValue as V;
        use std::collections::BTreeMap;
        let proxy = morf_io::DbusProxy::connect_with_timeout(
            morf_io::Bus::Session,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            Duration::from_secs(5),
        )
        .expect("a caller can connect");
        let typed = |signature: &str, value: V| V::Typed {
            signature: signature.to_owned(),
            value: Box::new(value),
        };
        let mut hints = BTreeMap::new();
        hints.insert("urgency".to_owned(), typed("y", V::Integer(2)));
        hints.insert("category".to_owned(), V::String("im.received".to_owned()));
        hints.insert("desktop-entry".to_owned(), V::String("chat".to_owned()));
        hints.insert("resident".to_owned(), V::Bool(true));
        hints.insert(
            "image-path".to_owned(),
            V::String("/tmp/face.png".to_owned()),
        );
        // A 2x1 RGBA picture: eight bytes.
        hints.insert(
            "image-data".to_owned(),
            typed(
                "(iiibiiay)",
                V::List(vec![
                    V::Integer(2),
                    V::Integer(1),
                    V::Integer(8),
                    V::Bool(true),
                    V::Integer(8),
                    V::Integer(4),
                    V::List((0..8).map(V::Integer).collect()),
                ]),
            ),
        );
        proxy.call_value_with(
            "Notify",
            &V::List(vec![
                V::String("chat".to_owned()),
                typed("u", V::Integer(0)),
                V::String("chat".to_owned()),
                V::String("hello".to_owned()),
                V::String("body".to_owned()),
                typed(
                    "as",
                    V::List(vec![
                        V::String("open".to_owned()),
                        V::String("Open".to_owned()),
                    ]),
                ),
                typed("a{sv}", V::Map(hints)),
                typed("i", V::Integer(-1)),
            ]),
        )
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime.scene().string_value(root, "text").unwrap() == "waiting"
        && Instant::now() < deadline
    {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(1));
    }
    // One more turn for the timer that invokes the action.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !runtime
        .scene()
        .string_value(root, "text")
        .unwrap()
        .contains("after=")
        && Instant::now() < deadline
    {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(1));
    }
    let reply = caller.join().expect("the caller finished");
    assert!(reply.is_ok(), "Notify was answered: {reply:?}");
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "hello|/tmp/face.png|2x1|8|true|2|im.received|chat|true|memory:image/|after=1",
        "every hint read, and a resident notification outlives its action"
    );
}
