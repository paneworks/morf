//! Linux-PAM, driven from a shell.
//!
//! A transaction runs on its own thread, because PAM is synchronous and blocks
//! inside modules for as long as they like — a fingerprint module waits for a
//! finger — and a shell cannot stop drawing while it does. What crosses back is
//! a stream of [`PamEvent`]s: every message a module sends, and finally the
//! verdict.
//!
//! Two ways to answer, one transaction. [`PamSession`] hands each prompt to
//! the caller and waits; [`PamAuthenticator`] answers from credentials given
//! up front. They share the conversation callback and everything else, so a
//! fix to one is a fix to both.
//!
//! Secrets travel as [`Zeroizing`] buffers from the caller to the
//! conversation, so every copy is wiped when it is dropped.

use std::error::Error as StdError;
use std::ffi::CString;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::pam_conversation::{Answers, Bridge, PamEvent, PamPrompt};

mod transaction;

/// A secret answer: wiped from memory when dropped.
pub type Secret = Zeroizing<Vec<u8>>;

/// A username and password; the password is wiped when dropped.
pub(crate) struct Credentials {
    pub(crate) username: CString,
    pub(crate) password: Secret,
}

/// PAM transaction failure without credential contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PamError {
    code: Option<i32>,
    message: String,
}

impl PamError {
    pub fn code(&self) -> Option<i32> {
        self.code
    }

    fn plain(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for PamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl StdError for PamError {}

/// The code for a transaction the caller gave up on.
///
/// Reported as its own value rather than as the generic conversation error the
/// module sees, because a caller that cancelled wants to know its cancel took
/// and not that "the conversation failed".
pub const PAM_CANCELLED: i32 = -1;

/// One running PAM transaction, with the person on this end of it.
///
/// Dropping it cancels: the transaction thread gets a conversation error at
/// its next prompt and unwinds through `pam_end`. A module that is blocked in
/// its own wait — a sensor with no finger on it — is not interrupted by this,
/// because PAM has no way to interrupt it; the thread lingers until the module
/// gives up on its own, and then leaves.
pub struct PamSession {
    events: mpsc::Receiver<PamEvent>,
    answers: Option<mpsc::Sender<Secret>>,
    cancelled: Arc<AtomicBool>,
    finished: Option<Result<(), PamError>>,
    /// Whether a prompt has been handed out and not yet answered.
    ///
    /// `respond` is gated on this rather than on the channel being open,
    /// because the channel is open for the whole transaction and an answer
    /// sent while no question is pending would sit in it until the next one --
    /// which is how a password typed at "touch the sensor" ends up as the reply
    /// to "PIN:".
    awaiting: bool,
}

impl PamSession {
    /// Starts a transaction whose prompts the caller will answer.
    ///
    /// `confdir` names a directory of PAM service files to use instead of
    /// `/etc/pam.d`, so a shell can carry its own service definition rather
    /// than depend on one being installed. It needs Linux-PAM 1.4 or later; on
    /// an older libpam the transaction finishes at once with an error saying so.
    pub fn start(service: &str, username: &str, confdir: Option<&str>) -> Self {
        Self::launch(service, username, confdir, None)
    }

    /// Starts a transaction answered from credentials, for the password case.
    pub(crate) fn start_fixed(
        service: &str,
        username: &str,
        password: Secret,
        confdir: Option<&str>,
    ) -> Self {
        Self::launch(service, username, confdir, Some(password))
    }

    fn launch(
        service: &str,
        username: &str,
        confdir: Option<&str>,
        password: Option<Secret>,
    ) -> Self {
        let (events, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut session = Self {
            events: receiver,
            answers: None,
            cancelled: Arc::clone(&cancelled),
            finished: None,
            awaiting: false,
        };
        let prepared = (|| {
            let service = cstring("service", service)?;
            let user = cstring("username", username)?;
            let confdir = confdir.map(|dir| cstring("confdir", dir)).transpose()?;
            let answers = match password {
                Some(password) if password.contains(&0) => {
                    return Err(PamError::plain("password contains a null byte"));
                }
                Some(password) => Answers::Fixed(Credentials {
                    username: user.clone(),
                    password,
                }),
                None => {
                    let (sender, receiver) = mpsc::channel();
                    session.answers = Some(sender);
                    Answers::Interactive(receiver)
                }
            };
            Ok::<_, PamError>((service, user, confdir, answers))
        })();
        match prepared {
            Ok((service, user, confdir, answers)) => {
                let bridge = Bridge {
                    events: events.clone(),
                    answers,
                    cancelled,
                };
                thread::spawn(move || {
                    // The verdict, and the thread's end, ring the loop.
                    let _wake = morf_io::WakeOnDrop;
                    let outcome =
                        transaction::transact(&service, &user, confdir.as_deref(), bridge);
                    let _ = events.send(PamEvent::Finished(outcome));
                });
            }
            // Nothing to run, but the caller is still owed a verdict, and
            // owed it through the same channel it will be reading.
            Err(error) => {
                let _ = events.send(PamEvent::Finished(Err(error)));
                morf_io::wake_all();
            }
        }
        session
    }

    /// The next thing the transaction has to say, within `timeout`.
    ///
    /// After `Finished` there is nothing more, and asking again returns the
    /// same verdict rather than blocking on a thread that has left.
    pub fn next(&mut self, timeout: Duration) -> Option<PamEvent> {
        if let Some(verdict) = &self.finished {
            return Some(PamEvent::Finished(verdict.clone()));
        }
        let event = self.events.recv_timeout(timeout).ok()?;
        match &event {
            PamEvent::Finished(verdict) => self.finished = Some(verdict.clone()),
            PamEvent::Message(PamPrompt::Prompt { .. }) => self.awaiting = true,
            PamEvent::Message(_) => {}
        }
        Some(event)
    }

    /// Answers the prompt the transaction is waiting on.
    ///
    /// `false` when there is nothing to answer: the transaction is fixed,
    /// finished, or cancelled. An answer nobody asked for is not queued for
    /// the next prompt — that is how a stale password ends up sent to a
    /// question about something else.
    pub fn respond(&mut self, answer: Secret) -> bool {
        if !self.awaiting {
            return false;
        }
        let sent = self
            .answers
            .as_ref()
            .is_some_and(|sender| sender.send(answer).is_ok());
        self.awaiting = !sent;
        sent
    }

    /// Gives up. The transaction fails at its next prompt.
    pub fn cancel(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // Dropping the sender wakes a thread blocked on the next answer, and
        // the flag covers a thread that has not asked yet.
        self.answers = None;
        self.awaiting = false;
        if self.finished.is_none() {
            self.finished = Some(Err(PamError {
                code: Some(PAM_CANCELLED),
                message: "authentication cancelled".to_owned(),
            }));
        }
    }
}

/// Synchronous Linux-PAM authentication and account validation.
pub struct PamAuthenticator;

/// Authentication result produced away from the shell event loop.
pub struct PamTask {
    session: PamSession,
}

impl PamTask {
    /// Waits up to the supplied duration for PAM to finish.
    ///
    /// Informational messages are read and dropped on this path, as they were
    /// before the conversation could carry them; a caller that wants to see
    /// them uses a [`PamSession`].
    pub fn wait(&mut self, timeout: Duration) -> Option<Result<(), PamError>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.session.next(remaining)? {
                PamEvent::Finished(verdict) => return Some(verdict),
                PamEvent::Message(_) => {}
            }
        }
    }
}

impl PamAuthenticator {
    /// Authenticates credentials through the named PAM service.
    pub fn authenticate(service: &str, username: &str, password: &str) -> Result<(), PamError> {
        let password = Zeroizing::new(password.as_bytes().to_vec());
        let mut session = PamSession::start_fixed(service, username, password, None);
        loop {
            match session.next(Duration::from_secs(60 * 60)) {
                Some(PamEvent::Finished(verdict)) => return verdict,
                Some(PamEvent::Message(_)) => {}
                None => return Err(PamError::plain("PAM did not answer")),
            }
        }
    }

    /// Starts authentication on a dedicated worker thread.
    pub fn authenticate_async(
        service: &str,
        username: &str,
        password: Secret,
        confdir: Option<&str>,
    ) -> PamTask {
        PamTask {
            session: PamSession::start_fixed(service, username, password, confdir),
        }
    }
}

fn cstring(field: &str, value: &str) -> Result<CString, PamError> {
    CString::new(value).map_err(|_| PamError::plain(format!("{field} contains a null byte")))
}

#[cfg(test)]
mod tests;
