//! `morf.dbus` over a real bus: calls that answer later, subscriptions that
//! end, signals that say who sent them, and descriptors that go round.
//!
//! Every test here serves names and presses buttons, so none of it runs on the
//! person's own session bus: the one ordinary test re-runs the `private_bus_`
//! ones under `dbus-run-session`, exactly as `lib_dbus_services` does, and each
//! of those refuses to run anywhere else.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use morf_io::{Bus, DbusFd, DbusService, DbusValue};

use super::lib_dbus_services::{PRIVATE_BUS, run_under_private_bus, run_with_fake};
use super::*;

const PATH: &str = "/org/morf/test/v2";
const INTERFACE: &str = "org.morf.test.V2";

#[test]
fn dbus_over_a_private_session_bus() {
    run_under_private_bus("tests::dbus_private::private_bus_");
}

/// A service played from a thread: `Echo` answers with its arguments,
/// `Signature` with their wire signature, `Never` is never answered, `Emit`
/// sends `Ping`, `Open` hands out one end of a socket pair (the other end goes
/// to `peer`) and `Take` writes one byte into whatever descriptor it is given.
struct Server {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
    peer: Arc<Mutex<Option<UnixStream>>>,
}

impl Server {
    fn start(name: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peer = Arc::new(Mutex::new(None));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let join = {
            let stop = Arc::clone(&stop);
            let peer = Arc::clone(&peer);
            let name = name.to_owned();
            thread::spawn(move || {
                let (mut service, _) =
                    DbusService::own(Bus::Session, &name, PATH, false).expect("owned");
                ready_tx.send(()).unwrap();
                while !stop.load(Ordering::Relaxed) {
                    let Some(call) = service.next_call(Duration::from_millis(10)) else {
                        continue;
                    };
                    match call.member.as_str() {
                        "Echo" => service.reply(call.id, &call.arguments).unwrap(),
                        "Signature" => service
                            .reply(call.id, &DbusValue::String(call.signature.clone()))
                            .unwrap(),
                        "Never" => {}
                        "Emit" => {
                            service.reply(call.id, &DbusValue::Nil).unwrap();
                            let DbusValue::List(values) = &call.arguments else {
                                continue;
                            };
                            service.emit(PATH, INTERFACE, "Ping", &values[0]).unwrap();
                        }
                        "Open" => {
                            let (ours, theirs) = UnixStream::pair().unwrap();
                            *peer.lock().unwrap() = Some(theirs);
                            let fd = DbusValue::Fd(DbusFd::new(ours.into()));
                            service.reply(call.id, &fd).unwrap();
                        }
                        "Take" => {
                            let DbusValue::List(values) = &call.arguments else {
                                continue;
                            };
                            let Some(DbusValue::Fd(fd)) = values.first() else {
                                service
                                    .reply_error(call.id, "org.morf.test.Error", "not an fd")
                                    .unwrap();
                                continue;
                            };
                            let mut stream =
                                UnixStream::from(fd.as_fd().try_clone_to_owned().unwrap());
                            stream.write_all(b"x").unwrap();
                            drop(stream);
                            service.reply(call.id, &DbusValue::Nil).unwrap();
                        }
                        other => service
                            .reply_error(call.id, "org.freedesktop.DBus.Error.UnknownMethod", other)
                            .unwrap(),
                    }
                }
            })
        };
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the server started");
        Self {
            stop,
            join: Some(join),
            peer,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Whether the other end of a socket pair has been closed.
///
/// Waits a moment for it: a test running beside this one may fork, and the
/// child holds a copy of every descriptor until it execs.
fn peer_closed(peer: &mut UnixStream) -> bool {
    peer.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut byte = [0u8; 1];
    loop {
        if matches!(peer.read(&mut byte), Ok(0)) {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Whether the other end is still open, right now.
fn peer_open(peer: &mut UnixStream) -> bool {
    peer.set_nonblocking(true).unwrap();
    let mut byte = [0u8; 1];
    matches!(peer.read(&mut byte), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
}

/// Hands a descriptor to the configuration as a bus reply would, under `name`.
fn give_fd(runtime: &mut Runtime, name: &str) -> UnixStream {
    let (ours, peer) = UnixStream::pair().unwrap();
    let fd = DbusValue::Fd(DbusFd::new(ours.into()));
    runtime.lua.enter(|ctx| {
        let value = crate::serialization::dbus_value_to_lua(ctx, fd).unwrap();
        ctx.globals().set(ctx, name, value).unwrap();
    });
    peer
}

#[test]
fn a_bus_descriptor_is_closed_when_forgotten_or_reloaded() {
    // An inhibitor's lock lasts as long as its descriptor: one the
    // configuration closes, drops, or leaves behind in a reload must not keep
    // a laptop awake.
    let mut runtime = Runtime::default();
    let mut closed = give_fd(&mut runtime, "closed");
    let mut dropped = give_fd(&mut runtime, "dropped");
    let mut kept = give_fd(&mut runtime, "kept");
    runtime
        .execute(
            "fds.lua",
            br#"
            assert(closed:is_open() and closed:close() and not closed:is_open())
            dropped = nil
            "#,
        )
        .unwrap();
    assert!(peer_closed(&mut closed), "close() closes");
    runtime.lua.gc_collect();
    assert!(peer_closed(&mut dropped), "a collected handle closes");
    assert!(peer_open(&mut kept), "a held one stays open");
    drop(runtime);
    assert!(
        peer_closed(&mut kept),
        "and the runtime going closes the rest"
    );
}

mod private_bus_calls;
mod private_bus_runtime;
