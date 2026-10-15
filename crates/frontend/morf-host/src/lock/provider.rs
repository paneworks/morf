//! Native session-lock evidence for an authenticated input broker. Lua cannot
//! set these phases: the lock loop advances them on actual backend outcomes.
use std::{
    io::{self, Write},
    os::{fd::BorrowedFd, unix::net::UnixStream},
    path::Path,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::process::native::{Endpoint, PeerIdentity, SocketPath, birth, peer, server};
#[cfg(test)]
use crate::process::native::{MAX_PEERS, MAX_REQUEST};

const RELEASE_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Pending,
    Locked,
    UnlockDispatched,
}
impl Phase {
    fn revision(self) -> u64 {
        match self {
            Self::Pending => 1,
            Self::Locked => 2,
            Self::UnlockDispatched => 3,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Locked => "locked",
            Self::UnlockDispatched => "unlock_dispatched",
        }
    }
}
struct Evidence {
    epoch: String,
    phase: Phase,
    compositor: PeerIdentity,
    start: u64,
    observed: bool,
    released: bool,
}
type Shared = Arc<(Mutex<Evidence>, Condvar)>;
fn request(shared: &Shared, uid: u32, bytes: &[u8]) -> Option<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    if value["version"] != 1 {
        return None;
    }
    let mut state = shared.0.lock().ok()?;
    match value["op"].as_str()? {
        "state" if object.len() == 2 => {
            state.observed = true;
            let mut response = serde_json::to_vec(&serde_json::json!({
                "version":1,"role":"lock","epoch":state.epoch,
                "revision":state.phase.revision(),"phase":state.phase.name(),
                "compositor_pid":state.compositor.pid,"compositor_start":state.start,
                "compositor_uid":state.compositor.uid,
            }))
            .ok()?;
            response.push(b'\n');
            Some(response)
        }
        "release"
            if uid == 0
                && object.len() == 4
                && state.phase == Phase::UnlockDispatched
                && value["epoch"] == state.epoch
                && value["revision"] == state.phase.revision() =>
        {
            state.released = true;
            shared.1.notify_all();
            Some(Vec::new())
        }
        _ => None,
    }
}

/// A private, bounded provider. Dropping it removes only its own socket inode.
pub struct Provider {
    _socket: SocketPath,
    shared: Shared,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
}
impl Provider {
    /// Use the connected Wayland socket, not an environment-selected PID or a
    /// Lua state field. The broker independently verifies this binding.
    pub fn bind(compositor: BorrowedFd<'_>) -> io::Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is unset"))?;
        Self::bind_at(Path::new(&runtime), peer(compositor)?)
    }
    fn bind_at(runtime: &Path, compositor: PeerIdentity) -> io::Result<Self> {
        // SAFETY: read-only process credential query.
        let owner = unsafe { libc::geteuid() };
        let start = birth(compositor.pid)?;
        if compositor.uid != owner {
            return Err(io::Error::other("native provider compositor owner differs"));
        }
        let Endpoint {
            socket,
            listener,
            epoch,
            owner,
        } = Endpoint::bind(runtime, "lock")?;
        let shared = Arc::new((
            Mutex::new(Evidence {
                epoch,
                phase: Phase::Pending,
                compositor,
                start,
                observed: false,
                released: false,
            }),
            Condvar::new(),
        ));
        let (wake, wait) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wait.set_nonblocking(true)?;
        let state = shared.clone();
        let worker = std::thread::Builder::new()
            .name("morf-lock-provider".into())
            .spawn(move || {
                server::serve(listener, wait, owner, move |uid, bytes| {
                    request(&state, uid, bytes)
                })
            })?;
        Ok(Self {
            _socket: socket,
            shared,
            wake,
            thread: Some(worker),
        })
    }
    pub fn locked(&self) {
        if let Ok(mut state) = self.shared.0.lock()
            && state.phase == Phase::Pending
        {
            state.phase = Phase::Locked;
        }
    }
    /// Call only after the backend accepted and flushed its unlock request.
    pub fn unlock_dispatched(&self) {
        if let Ok(mut state) = self.shared.0.lock()
            && state.phase == Phase::Locked
        {
            state.phase = Phase::UnlockDispatched;
        }
    }
    /// Keep evidence alive briefly only if a permitted observer sampled it.
    /// UI/Wayland resources should be dropped before waiting here.
    pub fn finish(&self) {
        let until = Instant::now() + RELEASE_WAIT;
        let Ok(mut state) = self.shared.0.lock() else {
            return;
        };
        while state.observed && !state.released && state.phase == Phase::UnlockDispatched {
            let Some(remaining) = until.checked_duration_since(Instant::now()) else {
                break;
            };
            match self.shared.1.wait_timeout(state, remaining) {
                Ok((next, _)) => state = next,
                Err(_) => return,
            }
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.wake.write_all(&[1]);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
use std::{
    fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::PathBuf,
};
