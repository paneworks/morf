//! The reactor thread's child processes: spawning, stdin, output, and reaping.

use std::os::fd::OwnedFd;
use std::os::unix::process::ExitStatusExt;
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use rustix::event::epoll;
use rustix::io::Errno;

use crate::reactor::{IoEvent, IoId, LineSplitter, OutputMode, Shared, SpawnOptions, StdinMode};

use super::{
    Core, EXIT_GRACE, Outbox, PIDFD, Pending, Proc, READS_PER_WAKE, REAP_POLL, STDERR, STDIN,
    STDOUT, Stream, nonblocking, over_high_water, token, unregister,
};

impl Core {
    // ------------------------------------------------------------ processes

    pub(super) fn add_process(
        &mut self,
        id: IoId,
        shared: Arc<Shared>,
        mut child: Box<Child>,
        options: SpawnOptions,
    ) {
        let now = Instant::now();
        let splitter = || options.lines.then(|| LineSplitter::new(options.max_line));
        let stream = |fd: Option<OwnedFd>, slot| {
            let fd = nonblocking(fd?).ok()?;
            let registered =
                epoll::add(&self.epoll, &fd, token(id, slot), epoll::EventFlags::IN).is_ok();
            Some(Stream {
                fd,
                splitter: splitter(),
                registered,
            })
        };
        let stdout = stream(child.stdout.take().map(OwnedFd::from), STDOUT);
        let stderr = stream(child.stderr.take().map(OwnedFd::from), STDERR);
        debug_assert!(options.stdout != OutputMode::Capture || stdout.is_some());
        let stdin = child
            .stdin
            .take()
            .map(OwnedFd::from)
            .and_then(|fd| nonblocking(fd).ok());
        let pidfd = rustix::process::Pid::from_raw(child.id() as i32)
            .and_then(|pid| {
                rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()).ok()
            })
            .filter(|fd| {
                epoll::add(&self.epoll, fd, token(id, PIDFD), epoll::EventFlags::IN).is_ok()
            });
        let reap_at = pidfd.is_none().then(|| now + REAP_POLL);
        let mut pending = Pending::default();
        let close_stdin_after = match options.stdin {
            StdinMode::Data(bytes) => {
                shared.outgoing.fetch_add(bytes.len(), Ordering::SeqCst);
                pending.push(bytes);
                true
            }
            _ => false,
        };
        self.procs.insert(
            id,
            Proc {
                shared,
                child,
                exit: None,
                pidfd,
                stdout,
                stderr,
                stdin,
                stdin_registered: false,
                pending,
                close_stdin_after,
                output_left: options.max_output,
                truncated: false,
                timeout_at: options.timeout.map(|timeout| now + timeout),
                kill_at: None,
                timed_out: false,
                grace_at: None,
                reap_at,
                paused: false,
                silent: false,
                detached: options.detached,
            },
        );
        self.flush_stdin(id);
    }

    pub(super) fn flush_stdin(&mut self, id: IoId) {
        let Some(proc) = self.procs.get_mut(&id) else {
            return;
        };
        let Some(fd) = proc.stdin.as_ref() else {
            proc.pending.discard(&proc.shared);
            return;
        };
        match proc
            .pending
            .flush(&proc.shared, |bytes| rustix::io::write(fd, bytes))
        {
            Ok(true) => {
                if proc.stdin_registered {
                    unregister(&self.epoll, fd);
                    proc.stdin_registered = false;
                }
                if proc.close_stdin_after {
                    close_stdin(&self.epoll, proc);
                }
            }
            Ok(false) => {
                if !proc.stdin_registered {
                    proc.stdin_registered =
                        epoll::add(&self.epoll, fd, token(id, STDIN), epoll::EventFlags::OUT)
                            .is_ok();
                }
            }
            Err(_) => {
                // The child stopped reading; what it did not take is gone.
                proc.pending.discard(&proc.shared);
                close_stdin(&self.epoll, proc);
            }
        }
    }

    pub(super) fn read_process(&mut self, id: IoId, slot: u64) {
        let Some(proc) = self.procs.get_mut(&id) else {
            return;
        };
        let stderr = slot == STDERR;
        for _ in 0..READS_PER_WAKE {
            let stream = if stderr {
                &mut proc.stderr
            } else {
                &mut proc.stdout
            };
            let Some(open) = stream.as_mut() else {
                break;
            };
            match rustix::io::read(&open.fd, &mut self.buffer[..]) {
                Ok(0) | Err(Errno::PIPE | Errno::IO | Errno::BADF) => {
                    let closed = stream.take().expect("open stream");
                    end_stream(&self.epoll, &mut self.out, id, proc, closed, stderr);
                    break;
                }
                Ok(read) => {
                    deliver(&mut self.out, id, proc, stderr, &self.buffer[..read]);
                    if over_high_water(&proc.shared) {
                        pause_process(&self.epoll, proc);
                        break;
                    }
                }
                Err(Errno::INTR) => {}
                Err(_) => break,
            }
        }
        self.finish_process(id);
    }

    pub(super) fn reap(&mut self, id: IoId) {
        let Some(proc) = self.procs.get_mut(&id) else {
            return;
        };
        match proc.child.try_wait() {
            Ok(Some(status)) => {
                proc.exit = Some((status.code(), status.signal()));
            }
            Ok(None) => return,
            // Reaped by someone else; nothing more will be known.
            Err(_) => proc.exit = Some((None, None)),
        }
        if let Some(fd) = proc.pidfd.take() {
            unregister(&self.epoll, &fd);
        }
        proc.reap_at = None;
        proc.timeout_at = None;
        proc.kill_at = None;
        if proc.stdout.is_some() || proc.stderr.is_some() {
            proc.grace_at = Some(Instant::now() + EXIT_GRACE);
        }
        self.finish_process(id);
    }

    /// Reports and forgets a child that has exited and said everything.
    pub(super) fn finish_process(&mut self, id: IoId) {
        let Some(proc) = self.procs.get(&id) else {
            return;
        };
        let Some((code, signal)) = proc.exit else {
            return;
        };
        if proc.stdout.is_some() || proc.stderr.is_some() {
            return;
        }
        let mut proc = self.procs.remove(&id).expect("present");
        close_stdin(&self.epoll, &mut proc);
        if !proc.silent {
            self.out.emit(
                &proc.shared,
                IoEvent::Exit {
                    id,
                    code,
                    signal,
                    timed_out: proc.timed_out,
                    truncated: proc.truncated,
                },
            );
        }
    }
}

pub(super) fn close_stdin(epoll: &OwnedFd, proc: &mut Proc) {
    if let Some(fd) = proc.stdin.take() {
        if proc.stdin_registered {
            unregister(epoll, &fd);
        }
        proc.stdin_registered = false;
    }
    proc.pending.discard(&proc.shared);
}

fn pause_process(epoll: &OwnedFd, proc: &mut Proc) {
    proc.paused = true;
    for stream in [proc.stdout.as_mut(), proc.stderr.as_mut()]
        .into_iter()
        .flatten()
    {
        if stream.registered {
            unregister(epoll, &stream.fd);
            stream.registered = false;
        }
    }
}

/// Output as the child's options want it: counted against `max_output`,
/// then split into lines or passed on whole.
fn deliver(out: &mut Outbox, id: IoId, proc: &mut Proc, stderr: bool, data: &[u8]) {
    if proc.silent {
        return;
    }
    let data = match proc.output_left.as_mut() {
        Some(left) => {
            let take = data.len().min(*left);
            *left -= take;
            if take < data.len() {
                proc.truncated = true;
            }
            &data[..take]
        }
        None => data,
    };
    if data.is_empty() {
        return;
    }
    let make = |bytes| {
        if stderr {
            IoEvent::Stderr(id, bytes)
        } else {
            IoEvent::Stdout(id, bytes)
        }
    };
    let stream = if stderr {
        proc.stderr.as_mut()
    } else {
        proc.stdout.as_mut()
    };
    match stream.and_then(|stream| stream.splitter.as_mut()) {
        Some(splitter) => {
            let shared = &proc.shared;
            splitter.push(data, |line| out.emit(shared, make(line)));
        }
        None => out.emit(&proc.shared, make(data.to_vec())),
    }
}

/// A stream at its end: its last partial line is delivered and it leaves
/// the set.
pub(super) fn end_stream(
    epoll: &OwnedFd,
    out: &mut Outbox,
    id: IoId,
    proc: &mut Proc,
    mut stream: Stream,
    stderr: bool,
) {
    if stream.registered {
        unregister(epoll, &stream.fd);
    }
    if !proc.silent
        && let Some(line) = stream.splitter.as_mut().and_then(LineSplitter::finish)
    {
        let event = if stderr {
            IoEvent::Stderr(id, line)
        } else {
            IoEvent::Stdout(id, line)
        };
        out.emit(&proc.shared, event);
    }
}
