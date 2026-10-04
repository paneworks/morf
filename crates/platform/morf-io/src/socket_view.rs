//! A socket or a listener named by a path, connected and disconnected as a
//! script says.
//!
//! [`Socket`] and [`SocketServer`] are open endpoints; a [`SocketView`] and a
//! [`ServerView`] keep the path too, so a closed one can be opened again or
//! pointed somewhere else. The path only changes while closed, and what is
//! sent or received in one call is capped at [`MAX_SOCKET_CHUNK`].

use std::io::ErrorKind;
use std::time::Duration;

use crate::{Socket, SocketServer};

/// The most one send or one receive moves.
pub const MAX_SOCKET_CHUNK: usize = 64 * 1024;

/// A receive limit as a script gives it, 1..=[`MAX_SOCKET_CHUNK`].
pub fn receive_limit(maximum: i64) -> Result<usize, String> {
    usize::try_from(maximum)
        .ok()
        .filter(|maximum| (1..=MAX_SOCKET_CHUNK).contains(maximum))
        .ok_or_else(|| "socket receive limit must be 1..65536".to_owned())
}

/// A path, and the connection to it while open.
pub struct SocketView {
    path: String,
    socket: Option<Socket>,
}

impl SocketView {
    /// Connects to `path`.
    pub fn connect(path: String) -> Result<Self, String> {
        let socket = Socket::connect(&path).map_err(|error| error.to_string())?;
        Ok(Self {
            path,
            socket: Some(socket),
        })
    }

    /// A connection a server accepted, with no path to reconnect to.
    pub fn accepted(socket: Socket) -> Self {
        Self {
            path: String::new(),
            socket: Some(socket),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn connected(&self) -> bool {
        self.socket.is_some()
    }

    /// Sends `bytes`; nothing while closed.
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_SOCKET_CHUNK {
            return Err("socket send exceeds 64 KiB".to_owned());
        }
        match self.socket.as_mut() {
            Some(stream) => stream.send(bytes).map_err(|error| error.to_string()),
            None => Ok(()),
        }
    }

    pub fn flush(&mut self) -> Result<(), String> {
        match self.socket.as_mut() {
            Some(stream) => stream.flush().map_err(|error| error.to_string()),
            None => Ok(()),
        }
    }

    /// Up to `maximum` bytes within `timeout`: `None` when closed or when
    /// nothing came in time.
    pub fn receive(&mut self, maximum: usize, timeout: Duration) -> Result<Option<Vec<u8>>, String> {
        let maximum = receive_limit(maximum as i64)?;
        let Some(stream) = self.socket.as_mut() else {
            return Ok(None);
        };
        let mut bytes = vec![0; maximum];
        match stream.receive_timeout(&mut bytes, timeout) {
            Ok(read) => {
                bytes.truncate(read);
                Ok(Some(bytes))
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Ok(None)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// Connects or disconnects; whether it is connected after. Without a
    /// path there is nothing to connect to.
    pub fn set_connected(&mut self, connected: bool) -> Result<bool, String> {
        if connected && self.socket.is_none() {
            if self.path.is_empty() {
                return Ok(false);
            }
            self.socket = Some(Socket::connect(&self.path).map_err(|error| error.to_string())?);
        } else if !connected {
            self.close();
        }
        Ok(self.socket.is_some())
    }

    pub fn close(&mut self) {
        if let Some(stream) = self.socket.take() {
            let _ = stream.shutdown();
        }
    }

    /// Points a closed view at `path`; false while connected.
    pub fn set_path(&mut self, path: String) -> bool {
        if self.socket.is_some() {
            return false;
        }
        self.path = path;
        true
    }
}

/// A path, and the listener on it while active.
pub struct ServerView {
    path: String,
    server: Option<SocketServer>,
}

impl ServerView {
    /// Listens on `path`.
    pub fn bind(path: String) -> Result<Self, String> {
        let server = SocketServer::bind(&path).map_err(|error| error.to_string())?;
        Ok(Self {
            path,
            server: Some(server),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn active(&self) -> bool {
        self.server.is_some()
    }

    /// A waiting connection, if one is waiting and the view is active.
    pub fn accept(&self) -> Result<Option<SocketView>, String> {
        let Some(listener) = self.server.as_ref() else {
            return Ok(None);
        };
        Ok(listener
            .try_accept()
            .map_err(|error| error.to_string())?
            .map(SocketView::accepted))
    }

    /// Starts or stops listening; whether it is listening after.
    pub fn set_active(&mut self, active: bool) -> Result<bool, String> {
        if active && self.server.is_none() {
            if self.path.is_empty() {
                return Ok(false);
            }
            self.server = Some(SocketServer::bind(&self.path).map_err(|error| error.to_string())?);
        } else if !active {
            self.server = None;
        }
        Ok(self.server.is_some())
    }

    pub fn close(&mut self) {
        self.server = None;
    }

    /// Points an inactive view at `path`; false while listening.
    pub fn set_path(&mut self, path: String) -> bool {
        if self.server.is_some() {
            return false;
        }
        self.path = path;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_reconnects_by_path_and_caps_its_chunks() {
        let dir = std::env::temp_dir().join(format!("morf-socket-view-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("s").to_string_lossy().into_owned();
        let mut server = ServerView::bind(path.clone()).unwrap();
        assert!(!server.set_path("elsewhere".into()));
        let mut client = SocketView::connect(path.clone()).unwrap();
        let mut accepted = None;
        for _ in 0..100 {
            if let Some(socket) = server.accept().unwrap() {
                accepted = Some(socket);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut accepted = accepted.expect("a connection");
        assert_eq!(accepted.path(), "");
        client.send(b"hi").unwrap();
        client.flush().unwrap();
        let got = accepted.receive(16, Duration::from_millis(500)).unwrap();
        assert_eq!(got.as_deref(), Some(&b"hi"[..]));
        assert!(client.send(&vec![0; MAX_SOCKET_CHUNK + 1]).is_err());
        assert!(client.receive(0, Duration::ZERO).is_err());

        client.close();
        assert!(!client.connected());
        assert_eq!(client.receive(16, Duration::ZERO).unwrap(), None);
        assert!(client.set_path(path.clone()));
        assert!(client.set_connected(true).unwrap());
        assert!(!accepted.set_connected(false).unwrap());
        assert!(!accepted.set_connected(true).unwrap());

        assert!(!server.set_active(false).unwrap());
        assert!(server.accept().unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
