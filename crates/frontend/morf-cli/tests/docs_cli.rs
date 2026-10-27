//! `morf docs` run as a program: a reader that closes early ends it with
//! success, and a guide prints only from a bundle stamped with the
//! executable's own version, the package's own bundle first.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MORF: &str = env!("CARGO_BIN_EXE_morf");
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("docs-cli-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A guide bundle under `dir`, stamped `stamp`, saying `body`.
fn bundle(dir: &Path, stamp: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("UI.md"), format!("{body}\n")).unwrap();
    std::fs::write(dir.join("IO.md"), format!("{body}\n")).unwrap();
    std::fs::write(dir.join("VERSION"), format!("{stamp}\n")).unwrap();
}

/// `exe` with a home, data and runtime folders under `root`, a stale guide
/// in `$XDG_DATA_HOME`, and no session or bus.
fn morf(exe: &Path, root: &Path) -> Command {
    let data = root.join("data");
    bundle(
        &data.join("morf/library/docs"),
        "0.0.1-stale",
        "STALE GUIDE",
    );
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::set_permissions(
        &runtime,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let mut command = Command::new(exe);
    command
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", &data)
        .env("XDG_DATA_DIRS", root.join("no-system-data"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("MORF_DEBUG", "no_crash_handler")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY");
    command
}

/// This executable placed at `<prefix>/bin/morf`, away from any checkout.
fn installed(prefix: &Path) -> PathBuf {
    let exe = prefix.join("bin/morf");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    if std::fs::hard_link(MORF, &exe).is_err() {
        std::fs::copy(MORF, &exe).unwrap();
    }
    exe
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn run(command: &mut Command) -> Output {
    command.output().unwrap()
}

#[test]
fn a_reader_that_closes_early_ends_morf_with_success() {
    let root = scratch("closed");
    for args in [
        &["docs"][..],
        &["docs", "--ui"],
        &["--help"],
        &["--version"],
    ] {
        let mut child = morf(Path::new(MORF), &root)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(
            !text(&output.stderr).contains("panicked"),
            "{args:?}: {output:?}"
        );
    }
}

#[test]
fn a_stale_guide_in_the_data_folders_is_passed_over() {
    let root = scratch("stale-passed");
    let output = run(morf(Path::new(MORF), &root).args(["docs", "--ui"]));
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout.lines().next(),
        Some(format!("# morf {VERSION} UI guide").as_str())
    );
    assert!(!stdout.contains("STALE GUIDE"), "a stale guide was printed");
}

#[test]
fn an_unpackaged_build_names_the_bundle_paths_it_looked_at() {
    let root = scratch("unpackaged");
    let exe = installed(&root.join("prefix"));
    let output = run(morf(&exe, &root).args(["docs", "--ui"]));
    let stderr = text(&output.stderr);
    assert!(!output.status.success(), "{output:?}");
    let bundled = root.join("prefix/share/morf/library/docs/UI.md");
    assert!(
        stderr.contains(&format!("{}: not there", bundled.display())),
        "{stderr}"
    );
    assert!(stderr.contains("stamped for morf 0.0.1-stale"), "{stderr}");
    assert!(
        stderr.contains(&format!("this is morf {VERSION}")),
        "{stderr}"
    );
    assert!(output.stdout.is_empty(), "nothing is printed");
}

#[test]
fn the_package_own_bundle_wins_and_its_header_is_the_version() {
    let root = scratch("packaged");
    let prefix = root.join("prefix");
    let exe = installed(&prefix);
    bundle(
        &prefix.join("share/morf/library/docs"),
        VERSION,
        "PACKAGED GUIDE",
    );
    let version = text(&run(morf(&exe, &root).arg("--version")).stdout);
    for (flag, title) in [("--ui", "UI guide"), ("--io", "I/O guide")] {
        let output = run(morf(&exe, &root).args(["docs", flag]));
        let stdout = text(&output.stdout);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            stdout.lines().next(),
            Some(format!("# {} {title}", version.trim()).as_str())
        );
        assert!(
            stdout.contains("PACKAGED GUIDE") && !stdout.contains("STALE GUIDE"),
            "{stdout}"
        );
    }
    let reference = text(&run(morf(&exe, &root).arg("docs")).stdout);
    assert_eq!(
        reference.lines().next(),
        Some(format!("# {} API reference", version.trim()).as_str())
    );
}
