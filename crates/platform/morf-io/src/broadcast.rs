//! One screen's shell calling a verb on every screen's, through the shell's
//! own IPC socket.
//!
//! Each output runs its own copy of the configuration; some things exist
//! once per session and are wanted wherever the person is, so a copy can
//! say something to all of them, itself included, through the same door a
//! `morf ipc call` comes in by. The calls go out from one thread of their
//! own, in the order they were made, and are not waited for: the
//! supervisor waits on every output's answer, this one's included, and an
//! output blocked on its own broadcast would answer nobody.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, mpsc};

use crate::{IpcRequest, IpcValue};

/// Most arguments one broadcast carries.
pub const MAX_BROADCAST_ARGUMENTS: usize = 32;

/// The way to the sending thread, once the shell's socket is known.
static OUTBOX: OnceLock<Mutex<mpsc::Sender<IpcRequest>>> = OnceLock::new();

/// Names the shell's IPC socket, once it is bound, and starts the thread
/// that sends through it. Until then, and in a process that has none,
/// [`broadcast`] answers `false`.
pub fn set_shell_socket(path: PathBuf) {
    let (sender, requests) = mpsc::channel::<IpcRequest>();
    let started = std::thread::Builder::new()
        .name("morf-broadcast".into())
        .spawn(move || {
            for request in requests {
                let _ = crate::ipc_call(&path, &request);
            }
        });
    if started.is_ok() {
        let _ = OUTBOX.set(Mutex::new(sender));
    }
}

/// Refuses more than [`MAX_BROADCAST_ARGUMENTS`] arguments.
pub fn check_broadcast_arguments(count: usize) -> Result<(), String> {
    if count > MAX_BROADCAST_ARGUMENTS {
        return Err(format!(
            "broadcast takes at most {MAX_BROADCAST_ARGUMENTS} arguments"
        ));
    }
    Ok(())
}

/// Queues `target(args...)` for every output; false when there is no shell
/// socket to send through (a headless test, a lock screen), so the caller
/// can do the thing itself.
pub fn broadcast(target: String, args: Vec<IpcValue>) -> bool {
    OUTBOX.get().is_some_and(|outbox| {
        outbox
            .lock()
            .is_ok_and(|outbox| outbox.send(IpcRequest::Call { target, args }).is_ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_capped() {
        assert!(check_broadcast_arguments(MAX_BROADCAST_ARGUMENTS).is_ok());
        assert_eq!(
            check_broadcast_arguments(MAX_BROADCAST_ARGUMENTS + 1).unwrap_err(),
            "broadcast takes at most 32 arguments"
        );
    }
}
