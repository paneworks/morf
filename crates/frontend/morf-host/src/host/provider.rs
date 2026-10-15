//! Native shell connection evidence. This reports connection/configuration
//! lifetime, never Lua-declared geometry, authentication or action authority.
use crate::process::native::{Endpoint, SocketPath, birth, peer, server};
use std::{
    io::{self, Write},
    os::{fd::BorrowedFd, unix::net::UnixStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub struct Provider {
    _socket: SocketPath,
    configured: Arc<AtomicBool>,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
}
impl Provider {
    /// `output` must come from the actual LayerClient's own_output snapshot.
    pub fn bind(connection: BorrowedFd<'_>, output: &str) -> io::Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is unset"))?;
        Self::bind_at(Path::new(&runtime), connection, output)
    }
    fn bind_at(runtime: &Path, connection: BorrowedFd<'_>, output: &str) -> io::Result<Self> {
        if output.is_empty() || output.len() > 256 || output.chars().any(char::is_control) {
            return Err(io::Error::other("invalid native output name"));
        }
        let compositor = peer(connection)?;
        let start = birth(compositor.pid)?;
        let Endpoint {
            socket,
            listener,
            epoch,
            owner,
        } = Endpoint::bind(runtime, "shell")?;
        if compositor.uid != owner {
            return Err(io::Error::other("native shell compositor owner differs"));
        }
        let configured = Arc::new(AtomicBool::new(false));
        let ready = configured.clone();
        let output = output.to_owned();
        let (wake, wait) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wait.set_nonblocking(true)?;
        let thread = std::thread::Builder::new()
            .name("morf-shell-provider".into())
            .spawn(move || {
                server::serve(listener, wait, owner, move |_, bytes| {
                    let request: serde_json::Value = serde_json::from_slice(bytes).ok()?;
                    if request.as_object()?.len() != 2
                        || request["version"] != 1
                        || request["op"] != "state"
                    {
                        return None;
                    }
                    let configured = ready.load(Ordering::Acquire);
                    let mut response = serde_json::to_vec(&serde_json::json!({
                        "version":1,"role":"shell","epoch":epoch,
                        "revision":if configured {2} else {1},
                        "phase":if configured {"configured"} else {"pending"},
                        "compositor_pid":compositor.pid,"compositor_start":start,
                        "compositor_uid":compositor.uid,"output":output,
                    }))
                    .ok()?;
                    response.push(b'\n');
                    Some(response)
                });
            })?;
        Ok(Self {
            _socket: socket,
            configured,
            wake,
            thread: Some(thread),
        })
    }
    /// Native host startup completed its first configured surface/frame.
    /// This is not compositor presentation acknowledgement or region evidence.
    pub fn configured(&self) {
        self.configured.store(true, Ordering::Release);
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.wake.write_all(&[1]);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader},
        os::{
            fd::AsFd,
            unix::fs::{DirBuilderExt, MetadataExt},
        },
        time::Duration,
    };

    #[test]
    fn shell_endpoint_uses_actual_socket_credentials_and_only_native_configuration() {
        let path = std::path::PathBuf::from(format!("/tmp/msp-{}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let (connection, _compositor) = UnixStream::pair().unwrap();
        let provider = Provider::bind_at(&path, connection.as_fd(), "HEADLESS-1").unwrap();
        let socket = provider._socket.path.clone();
        assert_eq!(fs::metadata(&socket).unwrap().mode() & 0o777, 0o600);
        let mut client = BufReader::new(UnixStream::connect(&socket).unwrap());
        client
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let query = |client: &mut BufReader<UnixStream>| {
            client
                .get_mut()
                .write_all(b"{\"version\":1,\"op\":\"state\"}\n")
                .unwrap();
            let mut response = String::new();
            client.read_line(&mut response).unwrap();
            serde_json::from_str::<serde_json::Value>(&response).unwrap()
        };
        let pending = query(&mut client);
        assert_eq!(pending["phase"], "pending");
        assert_eq!(pending["role"], "shell");
        assert_eq!(pending["compositor_pid"], std::process::id());
        assert_eq!(
            pending["compositor_start"],
            birth(std::process::id()).unwrap()
        );
        assert_eq!(pending["output"], "HEADLESS-1");
        provider.configured();
        let ready = query(&mut client);
        assert_eq!(ready["phase"], "configured");
        assert_eq!(ready["revision"], 2);
        assert_eq!(ready["epoch"], pending["epoch"]);
        client
            .get_mut()
            .write_all(b"{\"version\":1,\"op\":\"state\",\"phase\":\"configured\"}\n")
            .unwrap();
        let mut response = String::new();
        assert_eq!(client.read_line(&mut response).unwrap(), 0);
        drop(provider);
        assert!(!socket.exists());
        fs::remove_dir_all(path).unwrap();
    }
}
