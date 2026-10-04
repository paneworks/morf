//! Keeping a headless run away from the session it was started in.
//!
//! The runners never connect to Wayland -- there is no client to connect
//! with -- but a configuration finds its compositor, its bus and its settings
//! through the environment, and so do the programs it starts. This file is
//! where that environment is cut back, before any thread exists to read it.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The environment variables a configuration, a library or a child finds a
/// running session through.
pub const SESSION_VARIABLES: [&str; 5] = [
    "WAYLAND_DISPLAY",
    "WAYLAND_SOCKET",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "NIRI_SOCKET",
    "SWAYSOCK",
];

/// An address no bus answers on.
const NO_BUS: &str = "unix:path=/nonexistent/morf-no-dbus";

/// Cuts this process off from the session it was started in: no Wayland
/// display, no compositor socket, and -- with `no_dbus` -- no bus either.
///
/// Must run before any thread exists, which is when changing the
/// environment is sound; every runner calls it first thing.
pub fn isolate_from_session(no_dbus: bool) {
    for name in SESSION_VARIABLES {
        // SAFETY: called from `run` before any thread is started.
        unsafe { std::env::remove_var(name) };
    }
    if no_dbus {
        // An address that cannot be connected to fails every bus call at
        // once, the way a machine with no bus would, rather than waiting on
        // one that is there but should not be touched.
        for name in ["DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS"] {
            // SAFETY: as above.
            unsafe { std::env::set_var(name, NO_BUS) };
        }
    }
}

/// A scratch folder for this run, under `$TMPDIR`.
pub fn scratch_dir() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("morf-headless-{}", std::process::id()))
}

/// Points the XDG directories into `base`, so a configuration that writes
/// its settings writes them somewhere that is thrown away.
pub fn isolate_home(base: &Path) -> Result<(), String> {
    // Before the cache moves: fontconfig answers from the person's cache.
    morf_text::warm_font_preferences();
    for (name, folder) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        let path = base.join(folder);
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("could not create {}: {error}", path.display()))?;
        // SAFETY: called from `run` before any thread is started.
        unsafe { std::env::set_var(name, &path) };
    }
    Ok(())
}

/// Empties the folders [`isolate_home`] made, so the next configuration
/// starts from nothing: what one spec file saved (settings, profiles, a
/// history) is not what the next one finds. The variables already point
/// here, so nothing in the environment changes.
pub fn empty_home(base: &Path) {
    for folder in ["config", "data", "state", "cache"] {
        let path = base.join(folder);
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::create_dir_all(&path);
    }
}

/// The configuration a private bus runs with: a session bus that allows
/// anything and knows no services, so nothing is started on its behalf.
fn bus_config(listen: &Path) -> String {
    [
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN""#,
        r#" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">"#,
        "<busconfig>",
        "  <type>session</type>",
        &format!("  <listen>unix:dir={}</listen>", listen.display()),
        "  <auth>EXTERNAL</auth>",
        r#"  <policy context="default">"#,
        r#"    <allow send_destination="*" eavesdrop="true"/>"#,
        r#"    <allow eavesdrop="true"/>"#,
        r#"    <allow own="*"/>"#,
        "  </policy>",
        "</busconfig>",
        "",
    ]
    .join("\n")
}

/// A session bus of the run's own: a `dbus-daemon` that knows no services
/// and lives as long as the run, so a configuration that owns a name or
/// serves an interface does it where nothing else is listening.
pub struct PrivateBus {
    daemon: Child,
}

impl PrivateBus {
    /// Starts the daemon under `scratch` and points this process's session
    /// bus at it; the system bus is made unreachable.
    ///
    /// Before any thread exists, like everything else here.
    pub fn start(scratch: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(scratch)
            .map_err(|error| format!("could not create {}: {error}", scratch.display()))?;
        let config = scratch.join("bus.conf");
        std::fs::write(&config, bus_config(scratch))
            .map_err(|error| format!("could not write {}: {error}", config.display()))?;
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("could not start dbus-daemon for --private-bus: {error}"))?;
        let mut address = String::new();
        let read = daemon
            .stdout
            .take()
            .map(|stdout| std::io::BufReader::new(stdout).read_line(&mut address));
        let address = address.trim().to_owned();
        if !matches!(read, Some(Ok(count)) if count > 0) || address.is_empty() {
            let _ = daemon.kill();
            let _ = daemon.wait();
            return Err("dbus-daemon for --private-bus printed no address".to_owned());
        }
        // SAFETY: called from `run` before any thread is started.
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &address);
            std::env::set_var("DBUS_SYSTEM_BUS_ADDRESS", NO_BUS);
        }
        Ok(Self { daemon })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}
