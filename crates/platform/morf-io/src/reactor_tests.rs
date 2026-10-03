use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crate::*;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// Every event for `id` up to and including its last, crediting as it goes.
fn collect(reactor: &Reactor, handle: &IoHandle) -> Vec<IoEvent> {
    let control = reactor.control();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        let Some(event) = reactor.next_timeout(Duration::from_millis(50)) else {
            continue;
        };
        if event.id() != handle.id() {
            continue;
        }
        control.credit(handle, event.weight());
        let last = event.is_final();
        events.push(event);
        if last {
            return events;
        }
    }
    panic!("no final event; saw {events:?}");
}

fn stdout_lines(events: &[IoEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            IoEvent::Stdout(_, bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect()
}

fn exit_of(events: &[IoEvent]) -> (Option<i32>, Option<i32>, bool, bool) {
    match events.last() {
        Some(IoEvent::Exit {
            code,
            signal,
            timed_out,
            truncated,
            ..
        }) => (*code, *signal, *timed_out, *truncated),
        other => panic!("not an exit: {other:?}"),
    }
}

#[test]
fn line_splitter_cuts_long_lines_and_keeps_the_tail() {
    let mut splitter = LineSplitter::new(4);
    let mut lines = Vec::new();
    splitter.push(b"ab\ncdefgh\nij", |line| lines.push(line));
    splitter.push(b"\n\nklmnop", |line| lines.push(line));
    splitter.push(b"qr\nst", |line| lines.push(line));
    assert_eq!(
        lines,
        [
            b"ab".to_vec(),
            b"cdef".to_vec(),
            b"ij".to_vec(),
            Vec::new(),
            b"klmn".to_vec()
        ]
    );
    assert_eq!(splitter.finish(), Some(b"st".to_vec()));
    assert_eq!(splitter.finish(), None);
}

#[test]
fn reactor_delivers_lines_and_the_exit_after_them() {
    let reactor = Reactor::new().unwrap();
    let handle = reactor
        .spawn(SpawnOptions::new(argv(&["printf", "one\ntwo\nthree"])))
        .unwrap();
    assert!(handle.pid().is_some());
    let events = collect(&reactor, &handle);
    assert_eq!(stdout_lines(&events), ["one", "two", "three"]);
    assert_eq!(exit_of(&events), (Some(0), None, false, false));
}

#[test]
fn reactor_writes_stdin_and_reads_it_back() {
    let reactor = Reactor::new().unwrap();
    let mut options = SpawnOptions::new(argv(&["cat"]));
    options.stdin = StdinMode::Pipe;
    options.lines = false;
    let handle = reactor.spawn(options).unwrap();
    let control = reactor.control();
    control.write(&handle, b"hello ".to_vec()).unwrap();
    control.write(&handle, b"world".to_vec()).unwrap();
    control.close_stdin(&handle);
    let events = collect(&reactor, &handle);
    assert_eq!(stdout_lines(&events).concat(), "hello world");
    assert_eq!(exit_of(&events).0, Some(0));
}

#[test]
fn reactor_reports_failure_and_timeouts() {
    let reactor = Reactor::new().unwrap();
    let failed = reactor.spawn(SpawnOptions::new(argv(&["false"]))).unwrap();
    assert_eq!(exit_of(&collect(&reactor, &failed)).0, Some(1));

    let mut options = SpawnOptions::new(argv(&["sleep", "30"]));
    options.timeout = Some(Duration::from_millis(100));
    let started = Instant::now();
    let slow = reactor.spawn(options).unwrap();
    let (code, signal, timed_out, _) = exit_of(&collect(&reactor, &slow));
    assert_eq!((code, signal, timed_out), (None, Some(libc::SIGTERM), true));
    assert!(started.elapsed() < Duration::from_secs(5));

    assert!(
        reactor
            .spawn(SpawnOptions::new(argv(&["/nonexistent/program"])))
            .is_err()
    );
}

#[test]
fn reactor_signals_a_child() {
    let reactor = Reactor::new().unwrap();
    let handle = reactor
        .spawn(SpawnOptions::new(argv(&["sleep", "30"])))
        .unwrap();
    reactor.control().signal(&handle, libc::SIGKILL);
    let (_, signal, timed_out, _) = exit_of(&collect(&reactor, &handle));
    assert_eq!((signal, timed_out), (Some(libc::SIGKILL), false));
}

#[test]
fn reactor_bounds_output_and_keeps_reading_through_backpressure() {
    let reactor = Reactor::new().unwrap();
    // Four MiB of zeroes, far past the high-water mark: the reactor must
    // pause, be credited, resume, and still see the end.
    let mut options = SpawnOptions::new(argv(&["head", "-c", "4194304", "/dev/zero"]));
    options.lines = false;
    let whole = reactor.spawn(options.clone()).unwrap();
    let events = collect(&reactor, &whole);
    let total: usize = events
        .iter()
        .map(|event| match event {
            IoEvent::Stdout(_, bytes) => bytes.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(total, 4 * 1024 * 1024);
    assert_eq!(exit_of(&events), (Some(0), None, false, false));

    options.max_output = Some(1000);
    let capped = reactor.spawn(options).unwrap();
    let events = collect(&reactor, &capped);
    assert_eq!(stdout_lines(&events).concat().len(), 1000);
    assert!(exit_of(&events).3, "truncated");
}

#[test]
fn reactor_strips_ld_library_path_unless_asked() {
    // SAFETY: a test that only sets a variable no other test reads.
    unsafe { std::env::set_var("LD_LIBRARY_PATH", "/nix/store/wrapper-libs") };
    let reactor = Reactor::new().unwrap();
    let plain = reactor.spawn(SpawnOptions::new(argv(&["env"]))).unwrap();
    let lines = stdout_lines(&collect(&reactor, &plain));
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("LD_LIBRARY_PATH="))
    );
    assert!(lines.iter().any(|line| line.starts_with("PATH=")));

    let mut options = SpawnOptions::new(argv(&["env"]));
    options
        .environment
        .insert("LD_LIBRARY_PATH".into(), "/mine".into());
    options.environment.insert("MORF_TEST".into(), "yes".into());
    let asked = reactor.spawn(options).unwrap();
    let lines = stdout_lines(&collect(&reactor, &asked));
    assert!(lines.iter().any(|line| line == "LD_LIBRARY_PATH=/mine"));
    assert!(lines.iter().any(|line| line == "MORF_TEST=yes"));
}

#[test]
fn reactor_kills_and_reaps_children_when_dropped() {
    let reactor = Reactor::new().unwrap();
    let handle = reactor
        .spawn(SpawnOptions::new(argv(&["sleep", "30"])))
        .unwrap();
    let pid = handle.pid().unwrap() as i32;
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    drop(reactor);
    // Killed and reaped: not even a zombie is left to signal.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

fn socket_path(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("morf-reactor-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn reactor_connects_reads_lines_and_sees_the_close() {
    let path = socket_path("lines");
    let listener = UnixListener::bind(&path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut hello = [0u8; 5];
        stream.read_exact(&mut hello).unwrap();
        assert_eq!(&hello, b"hello");
        stream.write_all(b"a>>1\nb>>2\npartial").unwrap();
    });
    let reactor = Reactor::new().unwrap();
    let mut options = ConnectOptions::new(Endpoint::Unix(path.clone()));
    options.lines = true;
    let handle = reactor.connect(options);
    reactor.control().write(&handle, b"hello".to_vec()).unwrap();
    let events = collect(&reactor, &handle);
    server.join().unwrap();
    let id = handle.id();
    assert_eq!(
        events,
        [
            IoEvent::Connected(id),
            IoEvent::Data(id, b"a>>1".to_vec()),
            IoEvent::Data(id, b"b>>2".to_vec()),
            IoEvent::Data(id, b"partial".to_vec()),
            IoEvent::Closed {
                id,
                reason: CloseReason::Eof
            },
        ]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reactor_reports_refused_and_timed_out_connections() {
    let reactor = Reactor::new().unwrap();
    let missing = reactor.connect(ConnectOptions::new(Endpoint::Unix(socket_path("none"))));
    match collect(&reactor, &missing).as_slice() {
        [
            IoEvent::Closed {
                reason: CloseReason::Error(_),
                ..
            },
        ] => {}
        other => panic!("{other:?}"),
    }

    // Accepted but never answered: the request's deadline ends it.
    let path = socket_path("silent");
    let _listener = UnixListener::bind(&path).unwrap();
    let mut options = ConnectOptions::new(Endpoint::Unix(path.clone()));
    options.deadline = Some(Duration::from_millis(80));
    options.greeting = b"j/monitors".to_vec();
    let silent = reactor.connect(options);
    let events = collect(&reactor, &silent);
    assert_eq!(
        events.last(),
        Some(&IoEvent::Closed {
            id: silent.id(),
            reason: CloseReason::TimedOut
        })
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reactor_speaks_tcp_to_a_numeric_host() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(b"pong").unwrap();
    });
    let reactor = Reactor::new().unwrap();
    let handle = reactor.connect(ConnectOptions::new(Endpoint::Tcp {
        host: "127.0.0.1".into(),
        port,
    }));
    let events = collect(&reactor, &handle);
    server.join().unwrap();
    assert_eq!(events[0], IoEvent::Connected(handle.id()));
    assert!(events.contains(&IoEvent::Data(handle.id(), b"pong".to_vec())));
}

#[test]
fn a_childs_output_rings_the_loop() {
    let reactor = Reactor::new().unwrap();
    let wake = Wake::new().unwrap();
    wake.drain();
    let handle = reactor
        .spawn(SpawnOptions::new(argv(&["printf", "one"])))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut finished = false;
    while !finished {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(wake.wait(left), "the child's events rang the loop");
        wake.drain();
        while let Some(event) = reactor.next_timeout(Duration::ZERO) {
            if event.id() == handle.id() {
                reactor.control().credit(&handle, event.weight());
                finished |= event.is_final();
            }
        }
    }
}
