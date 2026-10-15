use super::*;
use std::io::{BufRead, BufReader};
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(format!(
            "/tmp/morf-provider-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn bind(&self) -> Provider {
        // SAFETY: read-only current credential query; only model compositor
        // identity is used by these private-socket protocol tests.
        Provider::bind_at(
            &self.0,
            PeerIdentity {
                pid: std::process::id(),
                uid: unsafe { libc::geteuid() },
            },
        )
        .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn query(client: &mut BufReader<UnixStream>) -> serde_json::Value {
    client
        .get_mut()
        .write_all(b"{\"version\":1,\"op\":\"state\"}\n")
        .unwrap();
    let mut reply = String::new();
    client.read_line(&mut reply).unwrap();
    serde_json::from_str(&reply).unwrap()
}
fn evidence() -> Shared {
    Arc::new((
        Mutex::new(Evidence {
            epoch: "a".repeat(32),
            phase: Phase::Pending,
            compositor: PeerIdentity { pid: 42, uid: 1000 },
            start: 123,
            observed: false,
            released: false,
        }),
        Condvar::new(),
    ))
}
#[test]
fn native_phases_cannot_be_set_or_released_by_socket_callers() {
    let state = evidence();
    let read = br#"{"version":1,"op":"state"}"#;
    let sample: serde_json::Value =
        serde_json::from_slice(&request(&state, 1000, read).unwrap()).unwrap();
    assert_eq!(sample["phase"], "pending");
    assert_eq!(sample["revision"], 1);
    assert!(state.0.lock().unwrap().observed);
    assert!(
        request(
            &state,
            1000,
            br#"{"version":1,"op":"state","phase":"locked"}"#
        )
        .is_none()
    );
    let release = serde_json::to_vec(
        &serde_json::json!({"version":1,"op":"release","epoch":"a".repeat(32),"revision":3}),
    )
    .unwrap();
    assert!(request(&state, 0, &release).is_none());
    state.0.lock().unwrap().phase = Phase::Locked;
    let sample: serde_json::Value =
        serde_json::from_slice(&request(&state, 0, read).unwrap()).unwrap();
    assert_eq!(sample["phase"], "locked");
    assert_eq!(sample["revision"], 2);
    assert!(state.0.lock().unwrap().observed);
    state.0.lock().unwrap().phase = Phase::UnlockDispatched;
    assert!(request(&state, 1000, &release).is_none());
    assert!(request(&state, 0, &release).is_some());
    assert!(state.0.lock().unwrap().released);
    assert!(request(&state, 0, br#"{"version":2,"op":"state"}"#).is_none());
}

#[test]
fn persistent_socket_reports_only_native_transitions_and_retires_its_own_path() {
    let scratch = Scratch::new();
    let provider = scratch.bind();
    let path = provider._socket.path.clone();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    let client = UnixStream::connect(&path).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut client = BufReader::new(client);
    let pending = query(&mut client);
    assert_eq!(pending["phase"], "pending");
    provider.unlock_dispatched();
    assert_eq!(query(&mut client)["phase"], "pending");
    provider.locked();
    let locked = query(&mut client);
    assert_eq!(locked["phase"], "locked");
    assert_eq!(locked["revision"], 2);
    assert_eq!(locked["compositor_pid"], std::process::id());
    assert_eq!(
        locked["compositor_start"],
        birth(std::process::id()).unwrap()
    );
    assert_eq!(locked["epoch"], pending["epoch"]);
    provider.unlock_dispatched();
    provider.locked();
    assert_eq!(query(&mut client)["phase"], "unlock_dispatched");
    let before = Instant::now();
    drop(provider);
    assert!(before.elapsed() < Duration::from_secs(1));
    assert!(!path.exists());
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).unwrap(), 0);
}

#[test]
fn malformed_and_oversized_connections_do_not_change_evidence_or_block_teardown() {
    let scratch = Scratch::new();
    let provider = scratch.bind();
    for bytes in [
        b"{\"version\":1,\"op\":\"state\",\"phase\":\"locked\"}\n".to_vec(),
        vec![b'x'; MAX_REQUEST + 1],
    ] {
        let mut client = UnixStream::connect(&provider._socket.path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        client.write_all(&bytes).unwrap();
        let mut reply = [0];
        match client.read(&mut reply) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::NotConnected
                ) => {}
            other => panic!("malformed peer was not promptly closed: {other:?}"),
        }
    }
    let mut idle = UnixStream::connect(&provider._socket.path).unwrap();
    idle.write_all(b"{").unwrap();
    assert_eq!(provider.shared.0.lock().unwrap().phase, Phase::Pending);
    let before = Instant::now();
    drop(provider);
    assert!(before.elapsed() < Duration::from_secs(1));
}

#[test]
fn invalid_runtime_or_process_binding_is_refused_and_replaced_paths_are_preserved() {
    let scratch = Scratch::new();
    let mut identity = PeerIdentity {
        pid: std::process::id(),
        uid: unsafe { libc::geteuid() },
    };
    identity.pid = u32::MAX;
    assert!(Provider::bind_at(&scratch.0, identity).is_err());
    assert!(!scratch.0.join("morf").exists());
    fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o755)).unwrap();
    identity.pid = std::process::id();
    assert!(Provider::bind_at(&scratch.0, identity).is_err());
    fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o700)).unwrap();
    let provider = scratch.bind();
    let path = provider._socket.path.clone();
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"replacement").unwrap();
    drop(provider);
    assert_eq!(fs::read(path).unwrap(), b"replacement");
}

#[test]
fn exact_root_release_completes_retention_and_stale_or_foreign_release_is_refused() {
    let scratch = Scratch::new();
    let provider = scratch.bind();
    provider.locked();
    provider.unlock_dispatched();
    let epoch = provider.shared.0.lock().unwrap().epoch.clone();
    let release = |epoch: &str, revision| {
        serde_json::to_vec(
            &serde_json::json!({"version":1,"op":"release","epoch":epoch,"revision":revision}),
        )
        .unwrap()
    };
    assert!(request(&provider.shared, 0, &release("stale", 3)).is_none());
    assert!(request(&provider.shared, 0, &release(&epoch, 2)).is_none());
    assert!(request(&provider.shared, 1000, &release(&epoch, 3)).is_none());
    request(&provider.shared, 0, br#"{"version":1,"op":"state"}"#).unwrap();
    request(&provider.shared, 0, &release(&epoch, 3)).unwrap();
    let before = Instant::now();
    provider.finish();
    assert!(before.elapsed() < Duration::from_millis(100));
}

#[test]
fn connection_capacity_and_partial_requests_cannot_hold_the_lock_loop_or_teardown() {
    let scratch = Scratch::new();
    let provider = scratch.bind();
    let mut clients = Vec::new();
    for _ in 0..MAX_PEERS {
        let stream = UnixStream::connect(&provider._socket.path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut client = BufReader::new(stream);
        query(&mut client);
        clients.push(client);
    }
    let mut extra = UnixStream::connect(&provider._socket.path).unwrap();
    extra
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut byte = [0];
    assert_eq!(extra.read(&mut byte).unwrap(), 0);
    let before = Instant::now();
    provider.locked();
    provider.unlock_dispatched();
    assert!(before.elapsed() < Duration::from_millis(100));
    drop(provider);
    assert!(before.elapsed() < Duration::from_secs(1));
}
