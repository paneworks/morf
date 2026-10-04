//! A runtime on the private bus: preferences that never wait for the portal,
//! signals and replies that ring the loop, and names given back on the way
//! out.

use super::*;

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_preferences_never_wait_for_the_portal() {
    // A shell used to read the settings portal with blocking calls while it
    // was being built: a portal that had to be activated, or a slow one, kept
    // the whole shell off the screen. The fields now start at their defaults,
    // an absent portal is never called (a call would activate it), and one
    // that arrives later is read when its name does.
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    let started = std::time::Instant::now();
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "prefers.lua",
            br#"
            local morf = require("morf")
            morf.ipc.scheme = function() return morf.prefers.color_scheme end
            "#,
        )
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "no portal, no wait: {:?}",
        started.elapsed()
    );
    let scheme = |runtime: &mut Runtime| match runtime.call_ipc("scheme", &[]).unwrap().as_slice() {
        [IpcValue::String(value)] => value.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(scheme(&mut runtime), "none", "the default, at once");

    let emit_now = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let portal = {
        let emit_now = Arc::clone(&emit_now);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let (mut service, _) =
                DbusService::own(Bus::Session, PORTAL, PORTAL_PATH, false).expect("owned");
            let typed = |signature: &str, value: DbusValue| DbusValue::Typed {
                signature: signature.to_owned(),
                value: Box::new(value),
            };
            let mut emitted = false;
            while !stop.load(Ordering::Relaxed) {
                if !emitted && emit_now.load(Ordering::Relaxed) {
                    emitted = true;
                    service
                        .emit(
                            PORTAL_PATH,
                            "org.freedesktop.portal.Settings",
                            "SettingChanged",
                            &DbusValue::List(vec![
                                DbusValue::String("org.freedesktop.appearance".to_owned()),
                                DbusValue::String("color-scheme".to_owned()),
                                typed("v", typed("u", DbusValue::Integer(2))),
                            ]),
                        )
                        .unwrap();
                }
                let Some(call) = service.next_call(Duration::from_millis(10)) else {
                    continue;
                };
                let DbusValue::List(arguments) = &call.arguments else {
                    continue;
                };
                if call.member == "ReadOne"
                    && arguments.get(1) == Some(&DbusValue::String("color-scheme".to_owned()))
                {
                    service
                        .reply(call.id, &typed("v", typed("u", DbusValue::Integer(1))))
                        .unwrap();
                } else {
                    service
                        .reply_error(call.id, "org.freedesktop.portal.Error.NotFound", "not set")
                        .unwrap();
                }
            }
        })
    };
    let wait_for = |runtime: &mut Runtime, wanted: &str| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while scheme(runtime) != wanted && std::time::Instant::now() < deadline {
            runtime.poll_services();
            thread::sleep(Duration::from_millis(2));
        }
        scheme(runtime)
    };
    assert_eq!(
        wait_for(&mut runtime, "dark"),
        "dark",
        "read when the portal arrived"
    );
    emit_now.store(true, Ordering::Relaxed);
    assert_eq!(
        wait_for(&mut runtime, "light"),
        "light",
        "and followed after"
    );
    stop.store(true, Ordering::Relaxed);
    portal.join().unwrap();
}

/// How long until something rang a loop's alarm, if within `most`: the
/// shell's loop, reduced to its alarm and a poll.
fn rung_within(wake: &morf_io::Wake, most: Duration) -> Option<Duration> {
    let started = std::time::Instant::now();
    wake.wait(most).then(|| started.elapsed())
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_signals_and_replies_ring_the_loop() {
    // The shell's loop sleeps with no timeout when nothing is due: a signal
    // or an answer from the bus is only seen because its arrival rings the
    // alarm.
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let name = format!("org.morf.test.v2.w{}", std::process::id());
    let _server = Server::start(&name);
    let wake = morf_io::Wake::new().unwrap();
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "wake.lua",
            format!(
                r#"
                local p = morf.dbus.proxy("session", "{name}", "{PATH}", "{INTERFACE}", 2000)
                local answered, pinged = false, false
                p:subscribe("Ping", function() pinged = true end)
                morf.ipc.ask = function()
                    assert(p:call_async("Echo", "x", function(ok) answered = ok end))
                end
                morf.ipc.ping = function() p:call_with("Emit", "hello") end
                morf.ipc.answered = function() return answered end
                morf.ipc.pinged = function() return pinged end
                "#
            )
            .as_bytes(),
        )
        .unwrap();
    let flag = |runtime: &mut Runtime, verb: &str| {
        matches!(
            runtime.call_ipc(verb, &[]).unwrap().as_slice(),
            [IpcValue::Boolean(true)]
        )
    };

    // A reply.
    runtime.poll_services();
    wake.drain();
    runtime.call_ipc("ask", &[]).unwrap();
    let took = rung_within(&wake, Duration::from_secs(5)).expect("the reply rang the loop");
    assert!(took < Duration::from_secs(1), "at once: {took:?}");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !flag(&mut runtime, "answered") && std::time::Instant::now() < deadline {
        wake.drain();
        runtime.poll_services();
        if !flag(&mut runtime, "answered") {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            rung_within(&wake, left).expect("the reply rang the loop");
        }
    }
    assert!(
        flag(&mut runtime, "answered"),
        "and it was there to collect"
    );

    // A signal.
    runtime.poll_services();
    wake.drain();
    runtime.call_ipc("ping", &[]).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !flag(&mut runtime, "pinged") && std::time::Instant::now() < deadline {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        rung_within(&wake, left).expect("the signal rang the loop");
        wake.drain();
        runtime.poll_services();
    }
    assert!(flag(&mut runtime, "pinged"), "the signal was delivered");
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_a_runtime_gives_its_names_back_before_another_takes_them() {
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let name = format!("org.morf.test.primary.p{}", std::process::id());
    // Held in a global, so the collector would never let it go by itself.
    let serve = format!(
        r#"
        held, outcome = morf.dbus.serve("session", "{name}", "/org/morf/test/primary", false)
        morf.ipc.outcome = function() return outcome end
        "#
    );
    let outcome = |runtime: &mut Runtime| runtime.call_ipc("outcome", &[]).unwrap();
    let owned = vec![IpcValue::String("owned".to_owned())];
    let taken = vec![IpcValue::String("taken".to_owned())];
    let mut first = Runtime::default();
    first.execute("first.lua", serve.as_bytes()).unwrap();
    assert_eq!(outcome(&mut first), owned);
    let mut second = Runtime::default();
    second.execute("second.lua", serve.as_bytes()).unwrap();
    assert_eq!(outcome(&mut second), taken, "one owner at a time");
    // Handing the duty over: the first gives its names back, and by the time
    // that returns the bus has them free.
    first.release_bus_names();
    let mut third = Runtime::default();
    third.execute("third.lua", serve.as_bytes()).unwrap();
    assert_eq!(outcome(&mut third), owned);
    // A runtime that ends does the same, whatever still refers to the name.
    drop(third);
    let mut fourth = Runtime::default();
    fourth.execute("fourth.lua", serve.as_bytes()).unwrap();
    assert_eq!(outcome(&mut fourth), owned);
}
