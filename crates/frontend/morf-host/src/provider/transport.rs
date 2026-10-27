//! Private native provider transport, shared by lock and shell evidence.
use std::{
    fs::{self, File},
    io::{self, Read},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd},
        unix::{
            fs::{DirBuilderExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub(crate) mod server;
pub(crate) const MAX_PEERS: usize = 8;
pub(crate) const MAX_REQUEST: usize = 1024;
const IDLE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub(crate) struct PeerIdentity {
    pub pid: u32,
    pub uid: u32,
}
pub(crate) fn peer(fd: BorrowedFd<'_>) -> io::Result<PeerIdentity> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut size = std::mem::size_of_val(&credentials) as libc::socklen_t;
    // SAFETY: initialized exact-size credentials buffer and a borrowed socket.
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
pub(crate) fn birth(pid: u32) -> io::Result<u64> {
    fs::read_to_string(format!("/proc/{pid}/stat"))?
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| io::Error::other("missing compositor process birth"))
}
pub(crate) struct SocketPath {
    pub path: PathBuf,
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
pub(crate) struct Endpoint {
    pub socket: SocketPath,
    pub listener: UnixListener,
    pub epoch: String,
    pub owner: u32,
}
impl Endpoint {
    pub fn bind(runtime: &Path, role: &str) -> io::Result<Self> {
        if !matches!(role, "lock" | "shell") {
            return Err(io::Error::other("unsupported native provider role"));
        }
        // SAFETY: read-only current process credential query.
        let owner = unsafe { libc::geteuid() };
        let metadata = fs::symlink_metadata(runtime)?;
        if !metadata.is_dir() || metadata.uid() != owner || metadata.mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "native provider requires the owning private runtime",
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
                "native provider directory is not owned and protected",
            ));
        }
        let mut random = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let epoch: String = random.iter().map(|value| format!("{value:02x}")).collect();
        let path = directory.join(format!(
            "{}-{epoch}-{role}-provider.sock",
            std::process::id()
        ));
        let listener = UnixListener::bind(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        let socket = SocketPath {
            path,
            identity: (metadata.dev(), metadata.ino()),
        };
        fs::set_permissions(&socket.path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            socket,
            listener,
            epoch,
            owner,
        })
    }
}
