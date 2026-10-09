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
    run_under_private_bus("dbus_private::private_bus_");
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
                        "UnlockRetries" => service
                            .reply(
                                call.id,
                                &DbusValue::Typed {
                                    signature: "a{uu}".to_owned(),
                                    value: Box::new(DbusValue::Dictionary(vec![
                                        (DbusValue::Unsigned(2), DbusValue::Unsigned(3)),
                                        (DbusValue::Unsigned(4), DbusValue::Unsigned(10)),
                                    ])),
                                },
                            )
                            .unwrap(),
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

mod private_bus_calls;
mod private_bus_runtime;
