//! IPC for a lock process.
//!
//! A lock runs alongside the shell on the same `WAYLAND_DISPLAY`, so it
//! cannot take the shell's socket. It binds `<display>-lock.sock` beside it,
//! which `morf list` shows like any other instance and `morf --lock ipc call
//! ...` (or `morf -i <display>-lock ...`) reaches.

use morf_io::{IpcIncoming, IpcReply, IpcRequest, IpcServer, IpcValue as WireValue};
use morf_lua::{LogEntry, Runtime};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use crate::services::{lua_ipc_value, wire_ipc_value};

/// The lock's socket, and what reaches it.
pub(crate) struct LockIpc {
    // Held for its lifetime: dropping it unbinds the socket.
    _server: IpcServer,
    requests: mpsc::Receiver<IpcIncoming>,
    owner: u32,
    path: PathBuf,
    started: Instant,
}

impl LockIpc {
    /// Binds the lock's socket. Its requests ring the loop's wake, so a call
    /// is answered at once rather than at the next fallback tick.
    pub(crate) fn bind(config: &Path) -> Result<Self, String> {
        let socket = crate::socket_path::lock_socket_path()?;
        let (tx, forwarded) = mpsc::channel();
        let server = IpcServer::bind(&socket, tx).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AddrInUse {
                return format!(
                    "another morf lock is already running on this display (socket {})",
                    socket.display()
                );
            }
            format!(
                "could not bind lock IPC socket {}: {error}",
                socket.display()
            )
        })?;
        let owner = std::fs::metadata(&socket)
            .map_err(|error| format!("could not inspect lock IPC socket: {error}"))?
            .uid();
        let (ring, requests) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(request) = forwarded.recv() {
                if ring.send(request).is_err() {
                    break;
                }
                morf_io::wake_all();
            }
        });
        Ok(Self {
            _server: server,
            requests,
            owner,
            path: config.to_path_buf(),
            started: Instant::now(),
        })
    }

    /// Answers everything waiting. Returns whether a call changed anything.
    pub(crate) fn serve(&self, runtime: &mut Runtime) -> bool {
        let mut repaint = false;
        while let Ok(incoming) = self.requests.try_recv() {
            if incoming.peer.uid != self.owner {
                incoming.reply(IpcReply::refused("peer uid does not own the lock"));
                continue;
            }
            let (reply, changed) =
                answer_lock_request(runtime, &incoming.request, &self.path, self.started);
            repaint |= changed;
            incoming.reply(reply);
        }
        repaint
    }
}

/// One request, answered by the lock's runtime. Returns the reply and
/// whether the runtime may have changed.
pub(crate) fn answer_lock_request(
    runtime: &mut Runtime,
    request: &IpcRequest,
    path: &Path,
    started: Instant,
) -> (IpcReply, bool) {
    match request {
        IpcRequest::Call { target, args } => {
            let args = args.iter().map(lua_ipc_value).collect::<Vec<_>>();
            match runtime.call_ipc(target, &args) {
                Ok(values) => (
                    IpcReply::success(values.iter().map(wire_ipc_value).collect()),
                    true,
                ),
                Err(error) => (IpcReply::refused(error.to_string()), false),
            }
        }
        IpcRequest::Verbs => (strings(runtime.ipc_verbs()), false),
        IpcRequest::Log => (
            strings(runtime.take_logs().iter().map(LogEntry::to_wire).collect()),
            false,
        ),
        IpcRequest::Capabilities => (strings(runtime.capabilities()), false),
        IpcRequest::Bindings => (strings(runtime.binding_dependencies()), false),
        IpcRequest::Info => (
            IpcReply::success(vec![
                WireValue::Integer(i64::from(std::process::id())),
                WireValue::String(path.to_string_lossy().into_owned()),
                WireValue::Integer(started.elapsed().as_secs() as i64),
            ]),
            false,
        ),
        // Killing a lock client does not unlock anything: the compositor
        // keeps the session locked with nobody to answer it. The lock is
        // lifted by its configuration, and only by it.
        IpcRequest::Kill => (
            IpcReply::refused("a lock is lifted by its configuration, not killed"),
            false,
        ),
    }
}

fn strings(lines: Vec<String>) -> IpcReply {
    IpcReply::success(lines.into_iter().map(WireValue::String).collect())
}
