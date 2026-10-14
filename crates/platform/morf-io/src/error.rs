//! The one error the crate's public functions return.
//!
//! Each kind carries the full message a caller shows, so [`Error`]'s
//! `Display` is exactly the text these functions returned as a `String`;
//! the kind is there for callers that branch on what went wrong.

use std::fmt;
use std::io;

/// What went wrong, with the message to show for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A path, name or peer that does not exist.
    NotFound(String),
    /// The system or the peer refused.
    PermissionDenied(String),
    /// Nothing arrived in time.
    TimedOut(String),
    /// A D-Bus connection, call or reply failed.
    Bus(String),
    /// An HTTP request failed.
    Http(String),
    /// A child could not be spawned, written to or signalled.
    Process(String),
    /// An argument or input was refused before anything ran.
    Invalid(String),
    /// Anything else.
    Other(String),
}

/// The kind of an [`Error`], without its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    TimedOut,
    Bus,
    Http,
    Process,
    Invalid,
    Other,
}

impl ErrorKind {
    /// `"not_found"`, `"permission_denied"`, `"timed_out"`, `"bus"`,
    /// `"http"`, `"process"`, `"invalid"` or `"other"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::TimedOut => "timed_out",
            Self::Bus => "bus",
            Self::Http => "http",
            Self::Process => "process",
            Self::Invalid => "invalid",
            Self::Other => "other",
        }
    }
}

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn bus(message: impl fmt::Display) -> Self {
        Self::Bus(message.to_string())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    /// An I/O failure of a child, keeping the I/O kind where it says more.
    pub fn process(error: io::Error) -> Self {
        match Self::from(error) {
            Self::Other(message) => Self::Process(message),
            known => known,
        }
    }

    /// The same kind of error, told with `message`.
    pub fn with_message(self, message: impl Into<String>) -> Self {
        let message = message.into();
        match self.kind() {
            ErrorKind::NotFound => Self::NotFound(message),
            ErrorKind::PermissionDenied => Self::PermissionDenied(message),
            ErrorKind::TimedOut => Self::TimedOut(message),
            ErrorKind::Bus => Self::Bus(message),
            ErrorKind::Http => Self::Http(message),
            ErrorKind::Process => Self::Process(message),
            ErrorKind::Invalid => Self::Invalid(message),
            ErrorKind::Other => Self::Other(message),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFound(_) => ErrorKind::NotFound,
            Self::PermissionDenied(_) => ErrorKind::PermissionDenied,
            Self::TimedOut(_) => ErrorKind::TimedOut,
            Self::Bus(_) => ErrorKind::Bus,
            Self::Http(_) => ErrorKind::Http,
            Self::Process(_) => ErrorKind::Process,
            Self::Invalid(_) => ErrorKind::Invalid,
            Self::Other(_) => ErrorKind::Other,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(message)
            | Self::PermissionDenied(message)
            | Self::TimedOut(message)
            | Self::Bus(message)
            | Self::Http(message)
            | Self::Process(message)
            | Self::Invalid(message)
            | Self::Other(message) => message,
        }
    }

    pub fn into_message(self) -> String {
        match self {
            Self::NotFound(message)
            | Self::PermissionDenied(message)
            | Self::TimedOut(message)
            | Self::Bus(message)
            | Self::Http(message)
            | Self::Process(message)
            | Self::Invalid(message)
            | Self::Other(message) => message,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        let message = error.to_string();
        match error.kind() {
            io::ErrorKind::NotFound => Self::NotFound(message),
            io::ErrorKind::PermissionDenied => Self::PermissionDenied(message),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Self::TimedOut(message),
            io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => Self::Invalid(message),
            _ => Self::Other(message),
        }
    }
}

impl From<zbus::Error> for Error {
    fn from(error: zbus::Error) -> Self {
        let name = match &error {
            zbus::Error::MethodError(name, ..) => Some(name.to_string()),
            zbus::Error::FDO(error) => Some(zbus::DBusError::name(error.as_ref()).to_string()),
            _ => None,
        };
        let short = name
            .as_deref()
            .and_then(|name| name.strip_prefix("org.freedesktop.DBus.Error."));
        let message = error.to_string();
        match short {
            Some(
                "ServiceUnknown" | "NameHasNoOwner" | "UnknownObject" | "UnknownInterface"
                | "UnknownMethod" | "UnknownProperty",
            ) => Self::NotFound(message),
            Some("AccessDenied" | "AuthFailed" | "InteractiveAuthorizationRequired") => {
                Self::PermissionDenied(message)
            }
            Some("Timeout" | "TimedOut" | "NoReply") => Self::TimedOut(message),
            Some("InvalidArgs" | "InvalidSignature") => Self::Invalid(message),
            _ => Self::Bus(message),
        }
    }
}

impl From<zbus::fdo::Error> for Error {
    fn from(error: zbus::fdo::Error) -> Self {
        Self::from(zbus::Error::FDO(Box::new(error)))
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Other(message.to_owned())
    }
}

impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.into_message()
    }
}

impl PartialEq<str> for Error {
    fn eq(&self, other: &str) -> bool {
        self.message() == other
    }
}

impl PartialEq<&str> for Error {
    fn eq(&self, other: &&str) -> bool {
        self.message() == *other
    }
}
