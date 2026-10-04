//! The system-service libraries in `library/lib`, against fake services.
//!
//! NetworkManager, BlueZ, UPower, MPRIS and logind are pure Lua on top of the
//! engine's generic D-Bus client, and every one of them can change the
//! machine it runs on: turn the radio off, forget a network, suspend. So none
//! of these tests goes near a real service. Most hand the library a stand-in
//! for `morf.dbus` written in Lua (`lib_dbus_fake.lua`), which answers in the
//! shapes the engine's decoder produces and records every argument as the
//! library typed it — the thing a real service would reject if it were wrong.
//! One goes over a real session bus, to a fake player this test owns under a
//! name no real player uses, to pin the decoder's shapes to the fake's.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

const FAKE: &str = include_str!("../lib_dbus_fake.lua");

/// Runs `body` with `fake`, `morf`, `ui` and `done(text)` in scope, beside the
/// examples so `require("lib.x")` finds the libraries, until `done` is called.
pub(super) fn run_with_fake(name: &str, body: &str) -> String {
    let script = format!(
        "local fake = (function()\n{FAKE}\nend)()\n\
         local morf = require(\"morf\")\n\
         local ui = require(\"morf.ui\")\n\
         local verdict = morf.signal(\"verdict\", \"running\")\n\
         local function done(text) verdict:set(text) end\n\
         ui.Text {{ text = function() return verdict:get() end }}\n\
         -- An `ay` as the engine delivers it: a string of bytes.\n\
         local function bytes(s) return s end\n\
         {body}"
    );
    let path = format!("{}/../../../library/{name}.lua", env!("CARGO_MANIFEST_DIR"));
    let mut runtime = Runtime::default();
    runtime
        .execute(&path, script.as_bytes())
        .unwrap_or_else(|error| panic!("{name} failed to load: {error}"));
    let root = runtime.scene().roots()[0];
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let text = runtime
            .scene()
            .string_value(root, "text")
            .unwrap()
            .to_owned();
        if text != "running" || Instant::now() > deadline {
            return text;
        }
        runtime.poll_services();
        thread::sleep(Duration::from_millis(1));
    }
}

/// Set in the child process that runs on a bus of its own.
pub(super) const PRIVATE_BUS: &str = "MORF_TEST_PRIVATE_SESSION_BUS";

/// A session bus that lets its one user do anything and activates nothing.
const PRIVATE_BUS_CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

#[test]
fn services_over_a_private_session_bus() {
    // The fake above answers in the shapes the engine's decoder is believed
    // to produce. The `private_bus_` tests pin that belief over a real bus: a
    // player served from a second runtime, read and pressed by the MPRIS
    // library, and a notification with an image sent to the notification
    // server.
    //
    // Never on the person's own session bus. Serving a name there -- even one
    // no real player uses -- is this test reaching into a live desktop, and
    // the library it drives lists and presses players. So the test re-runs
    // itself under `dbus-run-session`, which starts a bus for the child alone
    // and tears it down after; where that is not installed, it does not run.
    //
    // The bus is configured with no service directories, so nothing on it
    // can be activated: a runtime starting up asks for the desktop portal,
    // and on a stock session bus that would launch one against the private
    // bus for nothing.
    run_under_private_bus("tests::lib_dbus_services::private_bus::private_bus_");
}

/// Re-runs the ignored tests matching `filter` in a child process under
/// `dbus-run-session`, on a bus of its own; see the test above for why.
pub(super) fn run_under_private_bus(filter: &str) {
    let Ok(test_binary) = std::env::current_exe() else {
        return;
    };
    let config = std::env::temp_dir().join(format!(
        "morf-private-bus-{}-{}.conf",
        std::process::id(),
        filter.replace(':', "_")
    ));
    std::fs::write(&config, PRIVATE_BUS_CONFIG).unwrap();
    let status = std::process::Command::new("dbus-run-session")
        .arg(format!("--config-file={}", config.display()))
        .arg("--")
        .arg(test_binary)
        .args([filter, "--ignored", "--test-threads=1"])
        .env(PRIVATE_BUS, "1")
        .status();
    let _ = std::fs::remove_file(&config);
    match status {
        Ok(status) => assert!(status.success(), "the private-bus run failed: {status}"),
        // No `dbus-run-session`: nothing here can make a private bus.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("could not start dbus-run-session: {error}"),
    }
}

mod bluez;
mod logind;
mod mpris;
mod networkmanager;
mod private_bus;
mod upower;
