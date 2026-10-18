// Hot reload: which file changes reach the supervisor as a reload.

use morf_host::process::supervisor::lua_snapshot;
use std::fs;

#[test]
fn runtimepath_snapshot_tracks_nested_lua_changes() {
    let root = std::env::temp_dir().join(format!("morf-watch-{}", std::process::id()));
    let module = root.join("lua/plugin/widget.lua");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(&module, b"return 1").unwrap();
    let before = lua_snapshot(std::slice::from_ref(&root));

    fs::write(&module, b"return 200").unwrap();
    let after = lua_snapshot(std::slice::from_ref(&root));

    assert_ne!(before, after);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_file_changes_are_pushed_and_other_files_ignored() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let root = std::env::temp_dir().join(format!("morf-follow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("lua")).unwrap();
    fs::write(root.join("init.lua"), b"return 1").unwrap();
    let enabled = Arc::new(AtomicBool::new(true));
    let (tx, rx) = mpsc::channel();
    let thread_root = root.clone();
    let thread_enabled = Arc::clone(&enabled);
    std::thread::spawn(move || {
        morf_host::process::supervisor::follow_lua_files(&[thread_root], &thread_enabled, |_| {
            tx.send(()).is_ok()
        });
    });
    std::thread::sleep(Duration::from_millis(200));
    // Not a module: looked at, not a reload.
    fs::write(root.join("settings.json"), b"{}").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    // A module in a directory made after the watch began.
    fs::create_dir_all(root.join("lua/new")).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    fs::write(root.join("lua/new/widget.lua"), b"return 2").unwrap();
    assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    // A burst of saves is one reload.
    for index in 0..20 {
        fs::write(root.join("init.lua"), format!("return {index}")).unwrap();
    }
    assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    // Switched off: a change is taken in without a reload, and is not one
    // later either.
    enabled.store(false, std::sync::atomic::Ordering::Release);
    fs::write(root.join("init.lua"), b"return 'off'").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    enabled.store(true, std::sync::atomic::Ordering::Release);
    fs::write(root.join("notes.txt"), b"x").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    fs::remove_dir_all(root).unwrap();
}
