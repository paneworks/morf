//! Tests of dbus_private that reach the runtime's internals; the rest are in
//! tests/engine/dbus_private/mod.rs.
#![allow(unused_imports)]

use super::*;
use morf_io::{Bus, DbusFd, DbusService, DbusValue};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
