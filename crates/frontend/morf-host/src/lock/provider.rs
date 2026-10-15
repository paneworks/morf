//! Native session-lock evidence for an authenticated input broker. Lua cannot
//! set these phases: the lock loop advances them on actual backend outcomes.
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd},
        unix::{
            fs::{DirBuilderExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

mod server;

const MAX_PEERS: usize = 8;
const MAX_REQUEST: usize = 1024;
const IDLE: Duration = Duration::from_secs(2);
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
#[derive(Clone, Copy, Debug)]
struct PeerIdentity {
    pid: u32,
    uid: u32,
}
fn peer(fd: BorrowedFd<'_>) -> io::Result<PeerIdentity> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut size = std::mem::size_of_val(&credentials) as libc::socklen_t;
    // SAFETY: initialized credentials buffer and its exact length, live socket.
    if unsafe {
        libc::getsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut size,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if size as usize != std::mem::size_of_val(&credentials) || credentials.pid <= 0 {
        return Err(io::Error::other("missing compositor socket credentials"));
    }
    Ok(PeerIdentity {
        pid: credentials.pid as u32,
        uid: credentials.uid,
    })
}
fn birth(pid: u32) -> io::Result<u64> {
    fs::read_to_string(format!("/proc/{pid}/stat"))?
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| io::Error::other("missing compositor process birth"))
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
struct SocketPath {
    path: PathBuf,
    identity: (u64, u64),
}
impl Drop for SocketPath {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            (metadata.dev(), metadata.ino()) == self.identity
                && std::os::unix::fs::FileTypeExt::is_socket(&metadata.file_type())
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
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
        let metadata = fs::symlink_metadata(runtime)?;
        if !metadata.is_dir()
            || metadata.uid() != owner
            || metadata.mode() & 0o077 != 0
            || compositor.uid != owner
        {
            return Err(io::Error::other(
                "native lock provider requires the owning private runtime",
            ));
        }
        let directory = runtime.join("morf");
        match fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
            return Err(io::Error::other(
                "native lock provider directory is not owned and protected",
            ));
        }
        let mut random = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let epoch: String = random.iter().map(|value| format!("{value:02x}")).collect();
        let path = directory.join(format!("{}-{epoch}-lock-provider.sock", std::process::id()));
        let listener = UnixListener::bind(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        let socket = SocketPath {
            path: path.clone(),
            identity: (metadata.dev(), metadata.ino()),
        };
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
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
            .spawn(move || server::serve(listener, wait, state, owner))?;
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
