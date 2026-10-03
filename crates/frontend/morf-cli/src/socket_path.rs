//! A socket path that fits in `sockaddr_un`.
//!
//! A Unix socket's address is at most 107 bytes (`sun_path` is 108 with its
//! NUL), so `$XDG_RUNTIME_DIR/morf/<display>.sock` under a long runtime
//! directory -- a sandbox's, a test harness's -- could not be bound at all,
//! and the shell failed to start over a path. When the full path would not
//! fit, the socket goes to a private directory of the user's under `/tmp`
//! instead, named by the display and a hash of the full path, so every
//! runtime directory and display still has its own. Server and client both
//! come here, so `morf ipc` finds the socket the shell bound.

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

/// The longest path a Unix socket can be bound at, in bytes.
pub(crate) const SOCKET_PATH_MAX: usize = 107;

/// Where sockets go when their proper path is too long.
pub(crate) fn fallback_dir() -> PathBuf {
    PathBuf::from(format!("/tmp/morf-{}", uid()))
}

/// `full` if it fits, else a short path in `fallback`, whose directory is
/// made private to this user (and refused if someone else owns it).
pub(crate) fn fitting_socket_path(
    full: PathBuf,
    display: &str,
    fallback: &Path,
) -> Result<PathBuf, String> {
    if full.as_os_str().len() <= SOCKET_PATH_MAX {
        return Ok(full);
    }
    let hash = fnv1a(full.as_os_str().as_encoded_bytes());
    let named = fallback.join(format!("{display}-{:08x}.sock", hash as u32));
    let short = if named.as_os_str().len() <= SOCKET_PATH_MAX {
        named
    } else {
        fallback.join(format!("{hash:016x}.sock"))
    };
    if short.as_os_str().len() > SOCKET_PATH_MAX {
        return Err(format!(
            "no socket path fits in {SOCKET_PATH_MAX} bytes: {}",
            full.display()
        ));
    }
    private_dir(fallback)
        .map_err(|error| format!("could not use {}: {error}", fallback.display()))?;
    Ok(short)
}

/// Makes `dir` a directory only this user can enter, or checks that it is
/// one: `/tmp` is shared, and a directory someone else made there first
/// must not be where the shell's control socket lives.
fn private_dir(dir: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || metadata.uid() != uid() || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "not a private directory of this user",
        ));
    }
    Ok(())
}

fn uid() -> u32 {
    // SAFETY: getuid cannot fail and touches no memory.
    unsafe { libc::getuid() }
}

pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        // `/tmp` itself rather than `$TMPDIR`, which may be as long as the
        // paths under test.
        let dir = PathBuf::from(format!("/tmp/ms-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_path_that_fits_is_kept() {
        let full = PathBuf::from("/run/user/1000/morf/wayland-1.sock");
        let fallback = scratch("keep");
        assert_eq!(
            fitting_socket_path(full.clone(), "wayland-1", &fallback).unwrap(),
            full
        );
        // Nothing was made for it.
        assert!(!fallback.exists());
    }

    #[test]
    fn a_path_too_long_moves_to_a_private_short_one() {
        let long = PathBuf::from(format!("/{}/morf/wayland-1.sock", "r".repeat(120)));
        let other = PathBuf::from(format!("/{}/morf/wayland-1.sock", "s".repeat(120)));
        let fallback = scratch("short");
        let path = fitting_socket_path(long.clone(), "wayland-1", &fallback).unwrap();
        assert!(path.as_os_str().len() <= SOCKET_PATH_MAX);
        assert!(path.starts_with(&fallback));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wayland-1-")
        );
        // The same full path is the same socket, for server and client alike;
        // another runtime directory is another socket.
        assert_eq!(
            fitting_socket_path(long, "wayland-1", &fallback).unwrap(),
            path
        );
        assert_ne!(
            fitting_socket_path(other, "wayland-1", &fallback).unwrap(),
            path
        );
        let mode = fs::metadata(&fallback).unwrap().mode();
        assert_eq!(mode & 0o777, 0o700);
        // It can actually be bound.
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        drop(listener);
        fs::remove_dir_all(&fallback).unwrap();
    }

    #[test]
    fn a_shared_fallback_directory_is_refused() {
        let long = PathBuf::from(format!("/{}/morf/wayland-1.sock", "r".repeat(120)));
        let fallback = scratch("shared");
        fs::create_dir(&fallback).unwrap();
        fs::set_permissions(
            &fallback,
            std::os::unix::fs::PermissionsExt::from_mode(0o777),
        )
        .unwrap();
        let error = fitting_socket_path(long, "wayland-1", &fallback).unwrap_err();
        assert!(error.contains("not a private directory"), "{error}");
        fs::remove_dir_all(&fallback).unwrap();
    }
}
