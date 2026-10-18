//! The kernel's sync files on a dmabuf: its fences exported as one, a fence
//! attached to it, and polling one for its signal.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

pub(super) const DMA_BUF_SYNC_WRITE: u32 = 2;
/// `_IOWR('b', 2, struct dma_buf_export_sync_file)`. Neither libc nor rustix
/// names the sync-file ioctls, so they are encoded with libc's own macros.
const DMA_BUF_IOCTL_EXPORT_SYNC_FILE: libc::Ioctl = libc::_IOWR::<SyncFileArgument>(b'b' as u32, 2);
/// `_IOW('b', 3, struct dma_buf_import_sync_file)`.
const DMA_BUF_IOCTL_IMPORT_SYNC_FILE: libc::Ioctl = libc::_IOW::<SyncFileArgument>(b'b' as u32, 3);

#[repr(C)]
struct SyncFileArgument {
    flags: u32,
    fd: i32,
}

/// The fences a dmabuf carries, as one sync file: with `DMA_BUF_SYNC_WRITE`,
/// every one a writer has to wait for.
pub(super) fn export_sync_file(dmabuf: RawFd, flags: u32) -> Option<OwnedFd> {
    let mut argument = SyncFileArgument { flags, fd: -1 };
    let status = unsafe {
        libc::ioctl(
            dmabuf,
            DMA_BUF_IOCTL_EXPORT_SYNC_FILE,
            &mut argument as *mut SyncFileArgument,
        )
    };
    (status == 0 && argument.fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(argument.fd) })
}

/// Attaches a sync file to a dmabuf as a write fence. Returns whether the
/// kernel took it.
pub(super) fn import_sync_file(dmabuf: RawFd, fence: &OwnedFd) -> bool {
    let mut argument = SyncFileArgument {
        flags: DMA_BUF_SYNC_WRITE,
        fd: fence.as_raw_fd(),
    };
    let status = unsafe {
        libc::ioctl(
            dmabuf,
            DMA_BUF_IOCTL_IMPORT_SYNC_FILE,
            &mut argument as *mut SyncFileArgument,
        )
    };
    status == 0
}

pub(super) fn signalled(fence: &OwnedFd) -> bool {
    poll_fence(fence, 0)
}

pub(super) fn wait_signalled(fence: &OwnedFd, milliseconds: i32) {
    use super::super::present_stalls::{FENCE_TIMEOUTS, FENCE_WAITS, count};
    count(&FENCE_WAITS);
    if !poll_fence(fence, milliseconds) {
        count(&FENCE_TIMEOUTS);
    }
}

/// A sync file polls readable once its fence has signalled.
fn poll_fence(fence: &OwnedFd, milliseconds: i32) -> bool {
    let mut poll = [libc::pollfd {
        fd: fence.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    }];
    unsafe { libc::poll(poll.as_mut_ptr(), 1, milliseconds) > 0 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_sync_file_ioctls_encode_as_the_kernel_defines_them() {
        // `Ioctl` is unsigned on glibc and signed on musl.
        let bits = |ioctl: libc::Ioctl| ioctl as u32;
        assert_eq!(bits(super::DMA_BUF_IOCTL_EXPORT_SYNC_FILE), 0xC008_6202);
        assert_eq!(bits(super::DMA_BUF_IOCTL_IMPORT_SYNC_FILE), 0x4008_6203);
    }
}
