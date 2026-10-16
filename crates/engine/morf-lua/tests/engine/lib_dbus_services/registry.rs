//! `morf.service`: the registry's binder reached from the engine, refused to
//! an unapproved module, and torn down with its owner node or its runtime
//! without leaving a subscription, a timer or a match rule behind.

use std::fs;

use morf_io::Bus;

use super::*;

#[test]
fn an_unapproved_module_is_refused_a_service_at_load_time() {
    let root = std::env::temp_dir().join(format!("morf-service-gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    let imported_dir = root.join("site");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(imported_dir.join("plug")).unwrap();
    fs::write(
        imported_dir.join("plug/media.lua"),
        b"local binding, err = morf.service('media')\nreturn { binding = binding, err = err }",
    )
    .unwrap();
    fs::write(
        imported_dir.join("plug/permissions.lua"),
        b"return { services = true }",
    )
    .unwrap();
    let library = format!("{}/../../../library", env!("CARGO_MANIFEST_DIR"));
    let source = format!(
        "local fake = (function()\n{FAKE}\nend)()\n{}",
        r#"
        local w = require("plug.media")
        assert(w.binding == nil, "an unapproved module binds nothing at load")
        assert(tostring(w.err):find("services", 1, true), tostring(w.err))
        local U = "org.freedesktop.UPower"
        fake.object("system", U, "/org/freedesktop/UPower", U, { OnBattery = true },
            { EnumerateDevices = function() return { {} } end })
        local power = assert(morf.service("power", { dbus = fake.dbus }))
        assert(power.state.on_battery == true, "the configuration's own call is not gated")
        local ok, err = pcall(morf.service, "nonesuch")
        assert(not ok, "an unknown name raises")
        assert(tostring(err) == "unknown service `nonesuch` (registered: media, power)", tostring(err))
        "#
    );
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![
        config_dir.clone(),
        imported_dir.clone(),
        library.into(),
    ]);
    runtime
        .execute(
            &config_dir.join("shell.lua").to_string_lossy(),
            source.as_bytes(),
        )
        .unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_over_a_private_session_bus() {
    run_under_private_bus("lib_dbus_services::registry::private_bus_");
}

/// A UPower stand-in and one MPRIS player, each on one object path, served
/// from a runtime of their own until `stop`.
fn serve(power: &str, player: &str, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    let source = r#"
        local morf = require("morf")
        local ui = require("morf.ui")
        ui.Text { text = "" }
        local function serve(name, path, answer)
            local service, outcome = morf.dbus.serve("session", name, path, false)
            assert(outcome == "owned", outcome)
            service:on_call(function(call)
                local reply = answer(call)
                if reply == false then
                    service:reply_error(call.id, "org.freedesktop.DBus.Error.UnknownMethod", call.member)
                else
                    service:reply(call.id, reply)
                end
            end)
            return service
        end
        held_power = serve("POWER", "/org/freedesktop/UPower", function(call)
            local interface = call.arguments[1]
            if call.member == "GetAll" and interface == "org.freedesktop.UPower" then
                return { OnBattery = true }
            elseif call.member == "GetAll" and interface == "org.freedesktop.UPower.Device" then
                return { Percentage = 55.0, IsPresent = true }
            elseif call.member == "EnumerateDevices" then
                return { signature = "ao", value = {} }
            end
            return false
        end)
        held_player = serve("PLAYER", "/org/mpris/MediaPlayer2", function(call)
            if call.member ~= "GetAll" then return false end
            if call.arguments[1] == "org.mpris.MediaPlayer2.Player" then
                return { PlaybackStatus = "Playing", Rate = 1.0, Position = { signature = "x", value = 0 },
                         Metadata = { ["xesam:title"] = "Loop" } }
            end
            return { Identity = "Fake" }
        end)
    "#
    .replace("POWER", power)
    .replace("PLAYER", player);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let join = thread::spawn(move || {
        let mut runtime = Runtime::default();
        let started = runtime.execute("services.lua", source.as_bytes());
        let _ = ready_tx.send(started.map_err(|error| error.to_string()));
        while !stop.load(Ordering::Relaxed) {
            runtime.poll_services();
            thread::sleep(Duration::from_millis(1));
        }
    });
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the services started")
        .expect("the services were served");
    join
}

/// Polls until `verb` answers true, for at most five seconds.
fn wait_for(runtime: &mut Runtime, verb: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let answer = runtime.call_ipc(verb, &[]).unwrap();
        if matches!(answer.as_slice(), [IpcValue::Boolean(true)]) {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        runtime.poll_services();
        thread::sleep(Duration::from_millis(2));
    }
}

/// A few turns of the loop, for anything a teardown left to deliver.
fn turns(runtime: &mut Runtime) {
    for _ in 0..5 {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_bindings_come_and_go_without_leaving_anything_behind() {
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let id = std::process::id();
    let power = format!("org.morf.test.power{id}");
    let prefix = format!("org.morf.test.player{id}.");
    let stop = Arc::new(AtomicBool::new(false));
    let server = serve(&power, &format!("{prefix}fake"), Arc::clone(&stop));

    let config = r#"
        local morf = require("morf")
        local ui = require("morf.ui")
        local holder = ui.Item { width = 10, height = 10 }
        local held
        local function bind(owner)
            return {
                power = assert(morf.service("power", { bus = "session", service = "POWER", owner = owner,
                    interfaces = { display = { path = "/org/freedesktop/UPower" }, profiles = { any_of = {} } } })),
                media = assert(morf.service("media", { owner = owner,
                    objects = { players = { names = { prefix = "PREFIX", ignore = {} } } } })),
            }
        end
        morf.ipc.bind = function()
            local node = ui.Item {}
            ui.reparent(node, holder)
            held = bind(node)
            held.node = node
        end
        morf.ipc.bind_unowned = function() held = bind(nil) end
        morf.ipc.ready = function()
            local power, media = held.power.state, held.media.state
            return power.available and power.display.percentage == 55 and media.count == 1
                and media.active.playing
        end
        morf.ipc.close = function()
            held.power.close()
            held.media.close()
        end
        morf.ipc.destroy = function()
            ui.destroy(held.node, true)
            local closed = held.power.closed() and held.media.closed()
            held = nil
            return closed
        end
    "#
    .replace("POWER", &power)
    .replace("PREFIX", &prefix);
    let path = format!(
        "{}/../../../library/test-registry-bus.lua",
        env!("CARGO_MANIFEST_DIR")
    );
    let routes_before_runtime = morf_io::subscription_count(Bus::Session);
    let mut runtime = Runtime::default();
    runtime.execute(&path, config.as_bytes()).unwrap();
    turns(&mut runtime);
    let baseline = runtime.resource_counts();
    let routes = morf_io::subscription_count(Bus::Session);
    assert_eq!(baseline.processes, 0);

    for round in 0..20 {
        runtime.call_ipc("bind", &[]).unwrap();
        assert!(
            wait_for(&mut runtime, "ready"),
            "round {round}: both services read"
        );
        let bound = runtime.resource_counts();
        assert_eq!(bound.owned_hooks, baseline.owned_hooks + 2, "round {round}");
        assert!(
            bound.dbus_subscriptions > baseline.dbus_subscriptions,
            "round {round}"
        );
        assert!(
            bound.timers > baseline.timers,
            "round {round}: the position tick"
        );
        assert!(
            morf_io::subscription_count(Bus::Session) > routes,
            "round {round}: routes"
        );
        if round % 2 == 1 {
            runtime.call_ipc("close", &[]).unwrap();
            turns(&mut runtime);
            assert_eq!(runtime.resource_counts(), baseline, "round {round}: closed");
        }
        let closed = runtime.call_ipc("destroy", &[]).unwrap();
        assert_eq!(
            closed,
            vec![IpcValue::Boolean(true)],
            "round {round}: closed with its owner"
        );
        turns(&mut runtime);
        assert_eq!(runtime.resource_counts(), baseline, "round {round}");
        assert_eq!(
            morf_io::subscription_count(Bus::Session),
            routes,
            "round {round}: every match rule given back"
        );
    }

    // A binding nothing owns lives as long as its runtime, and goes with it.
    runtime.call_ipc("bind_unowned", &[]).unwrap();
    assert!(wait_for(&mut runtime, "ready"));
    assert!(morf_io::subscription_count(Bus::Session) > routes);
    drop(runtime);
    assert_eq!(
        morf_io::subscription_count(Bus::Session),
        routes_before_runtime,
        "a runtime that ends takes its bindings' match rules with it"
    );
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
}
