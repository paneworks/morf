//! Generic D-Bus method and property client.
//!
//! # File descriptors
//!
//! They cross into a configuration only as something it cannot look through.
//!
//! A D-Bus message can carry an fd, and handing one to a sandboxed Lua VM as a
//! number would hand it whatever that descriptor is attached to — a file
//! outside the config directory, a socket to another service, a device node.
//! This used to be refused outright, in both directions, and the refusal had a
//! cost: logind's `Inhibit` answers with an fd and the lock lasts exactly as
//! long as the fd is open, so a shell could only hold one through a
//! `systemd-inhibit … cat` child process.
//!
//! What arrives now is a [`DbusFd`]: an owned descriptor behind an opaque
//! handle. A configuration can hold it, close it, and hand it back to the bus
//! as an `h` argument — and nothing else. There is no read, no write, no
//! number to pass to anything that would. The set of things a configuration
//! can reach is still an enumerable list; an fd from the bus adds "keep this
//! alive" to it, which is the one thing every fd-returning service on a
//! desktop (inhibitors, leases, portals' handles) actually asks of a client.

mod proxy;
mod router;

use crate::dbus_encode::dbus_argument_value;
use crate::dbus_encode::decode_message_value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use zbus::blocking::Connection as DbusConnection;

use crate::dbus_decode::DbusSignal;
use zbus::zvariant::{Structure, StructureBuilder};

pub use proxy::{DbusFd, DbusProxy, DbusValue};
use router::router;
pub use router::subscription_count;
pub(crate) use router::{Route, SignalRouter};

/// Message bus used by a generic D-Bus proxy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bus {
    Session,
    System,
}

impl Bus {
    /// A connection builder for this bus, on a socket connected here.
    ///
    /// zbus connects a Unix socket on the `blocking` crate's thread pool,
    /// and that pool's thread never leaves once it exists: it wakes every
    /// half second for the life of the process, the one wake an idle shell
    /// could not get rid of. Connecting a local socket takes no time, so it
    /// is done on the calling thread and zbus is handed the stream. An
    /// address this does not understand -- TCP, `unixexec`, a launchd one --
    /// goes to zbus as before.
    pub(crate) fn builder(self) -> zbus::Result<zbus::blocking::connection::Builder<'static>> {
        let address = match self {
            Self::Session => std::env::var("DBUS_SESSION_BUS_ADDRESS").ok().or_else(|| {
                std::env::var("XDG_RUNTIME_DIR")
                    .ok()
                    .map(|dir| format!("unix:path={dir}/bus"))
            }),
            Self::System => Some(
                std::env::var("DBUS_SYSTEM_BUS_ADDRESS")
                    .unwrap_or_else(|_| "unix:path=/var/run/dbus/system_bus_socket".to_owned()),
            ),
        };
        if let Some(stream) = address.as_deref().and_then(connect_local) {
            return Ok(zbus::blocking::connection::Builder::async_io_unix_stream(
                stream,
            ));
        }
        match self {
            Self::Session => zbus::blocking::connection::Builder::session(),
            Self::System => zbus::blocking::connection::Builder::system(),
        }
    }
}

/// The connection proxies on `bus` with calls bounded by `timeout` share.
///
/// zbus bounds calls per connection, and a connection per proxy was what
/// that led to: a socket, an authentication handshake, a unique bus name and
/// an executor thread for every object a configuration talked to -- 53 of
/// them in one shell. Proxies with the same bus and bound now share one; a
/// connection the bus has dropped (the daemon restarted) is replaced by the
/// next proxy that asks.
fn shared_connection(bus: Bus, timeout: Duration) -> zbus::Result<zbus::blocking::Connection> {
    static SHARED: OnceLock<Mutex<Vec<(Bus, Duration, zbus::blocking::Connection)>>> =
        OnceLock::new();
    let shared = SHARED.get_or_init(|| Mutex::new(Vec::new()));
    let mut shared = shared.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(index) = shared
        .iter()
        .position(|(held_bus, held_timeout, _)| *held_bus == bus && *held_timeout == timeout)
    {
        // A connection whose peer is gone answers nothing: replaced, not reused.
        if !is_closed(&shared[index].2) {
            return Ok(shared[index].2.clone());
        }
        let _gone = shared.swap_remove(index);
    }
    let connection = bus.builder()?.method_timeout(timeout).build()?;
    shared.push((bus, timeout, connection.clone()));
    Ok(connection)
}

/// Whether a connection's socket has gone (the bus daemon restarted).
fn is_closed(connection: &zbus::blocking::Connection) -> bool {
    // A cheap local question the executor answers only while it runs.
    zbus::blocking::fdo::DBusProxy::new(connection)
        .and_then(|proxy| proxy.get_id().map_err(zbus::Error::from))
        .is_err()
}

/// Connects the first `unix:path=` or `unix:abstract=` entry of a D-Bus
/// address that answers.
fn connect_local(address: &str) -> Option<std::os::unix::net::UnixStream> {
    use std::os::unix::ffi::OsStrExt;
    for entry in address.split(';') {
        let Some(keys) = entry.strip_prefix("unix:") else {
            continue;
        };
        let mut path = None;
        let mut abstract_name = None;
        for pair in keys.split(',') {
            match pair.split_once('=') {
                Some(("path", value)) => path = unescape_address(value),
                Some(("abstract", value)) => abstract_name = unescape_address(value),
                _ => {}
            }
        }
        let stream = match (path, abstract_name) {
            (Some(path), _) => {
                std::os::unix::net::UnixStream::connect(std::ffi::OsStr::from_bytes(&path))
            }
            (None, Some(name)) => {
                use std::os::linux::net::SocketAddrExt;
                std::os::unix::net::SocketAddr::from_abstract_name(&name)
                    .and_then(|address| std::os::unix::net::UnixStream::connect_addr(&address))
            }
            (None, None) => continue,
        };
        if let Ok(stream) = stream
            && stream.set_nonblocking(true).is_ok()
        {
            return Some(stream);
        }
    }
    None
}

/// Undoes a D-Bus address value's `%xx` escapes.
fn unescape_address(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Some(out)
}

/// How long a call waits for its reply before giving up.
///
/// zbus defaults to twenty-five seconds, which is the right answer for a
/// program that can afford to wait and the wrong one for this: a configuration
/// calls out from a Lua handler, and a Lua handler runs on the thread that
/// paints. A service that hangs would take the shell with it for twenty-five
/// seconds — no repaints, no input — and there is no service worth that.
///
/// A second is far longer than any healthy reply on a session bus and short
/// enough that a bad one costs a stutter rather than a hang.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_millis(1000);

/// A reply that has not arrived yet.
///
/// Answered by [`DbusProxy::get_later`], [`DbusProxy::call_later_with`],
/// [`DbusProxy::call_async`] and [`call_async`]; ask with
/// [`PendingReply::try_take`] from a poll.
pub struct PendingReply {
    rx: std::sync::mpsc::Receiver<Result<DbusValue, String>>,
    /// When to stop waiting, for a call whose connection waits longer than
    /// its caller asked to.
    deadline: Option<Instant>,
}

impl PendingReply {
    /// When the caller stops waiting, if it set a bound: the loop has to
    /// wake then to deliver the timeout, since no thread will ring for it.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// The reply if it is in, `None` while it is not. A reply whose thread
    /// vanished is an error rather than a wait that never ends, and so is one
    /// that missed its deadline.
    pub fn try_take(&self) -> Option<Result<DbusValue, String>> {
        match self.rx.try_recv() {
            Ok(reply) => Some(reply),
            Err(std::sync::mpsc::TryRecvError::Empty) => match self.deadline {
                Some(deadline) if Instant::now() >= deadline => Some(Err(
                    "org.freedesktop.DBus.Error.Timeout: the call was not answered in time"
                        .to_owned(),
                )),
                _ => None,
            },
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(Err("the D-Bus reply was lost".to_owned()))
            }
        }
    }
}

/// How many calls may be waiting on the bus at once, across the process.
///
/// Each waits on a thread of its own, and a thread whose caller stopped
/// waiting at its deadline still waits for the connection's own bound. Some
/// ceiling has to stand between a configuration calling in a loop and the
/// process running out of threads; this one is far above anything a shell
/// does on purpose.
pub const MAX_CALLS_IN_FLIGHT: usize = 128;

/// The longest any asynchronous call may be asked to wait.
///
/// Long enough for a person to confirm a Bluetooth pairing or type a password
/// into a polkit dialog, which is what the long waits are for.
pub const MAX_ASYNC_CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// How long an asynchronous call waits when its caller does not say.
///
/// zbus's own default. Nothing is blocked while it waits, so there is no
/// reason to be as stingy as [`DEFAULT_CALL_TIMEOUT`].
pub const DEFAULT_ASYNC_CALL_TIMEOUT: Duration = Duration::from_secs(25);

static CALLS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Runs `work` on a thread of its own and hands its answer to a poll.
///
/// Refused rather than queued above [`MAX_CALLS_IN_FLIGHT`]: a queue would only
/// move the unbounded part somewhere less visible.
fn spawn_call(
    deadline: Option<Instant>,
    work: impl FnOnce() -> Result<DbusValue, String> + Send + 'static,
) -> Result<PendingReply, String> {
    let taken = CALLS_IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
    if taken >= MAX_CALLS_IN_FLIGHT {
        CALLS_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
        return Err(format!(
            "too many D-Bus calls in flight (the limit is {MAX_CALLS_IN_FLIGHT})"
        ));
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let spawned = thread::Builder::new()
        .name("morf-dbus-call".to_owned())
        .spawn(move || {
            // A receiver that has gone is a caller torn down while it waited,
            // and dropping the answer is exactly right.
            let _ = tx.send(work());
            CALLS_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
            crate::wake_all();
        });
    if let Err(error) = spawned {
        CALLS_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
        return Err(format!("could not start a D-Bus call: {error}"));
    }
    Ok(PendingReply { rx, deadline })
}

/// Calls a method from this process's shared connection to `bus`, without
/// waiting for it.
///
/// Shared with the signal reader: one socket, one handshake, one name on the
/// bus for every asynchronous call and every subscription, and a call that
/// starts something tied to its caller's connection — a BlueZ discovery —
/// lasts as long as the process rather than as long as some proxy.
///
/// `timeout` is clamped to [`MAX_ASYNC_CALL_TIMEOUT`]. `arguments` follows the
/// same convention as [`DbusProxy::call_value_with`]: a list is the positional
/// arguments, anything else is the one argument.
pub fn call_async(
    bus: Bus,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    arguments: DbusValue,
    timeout: Duration,
) -> Result<PendingReply, String> {
    let router = router(bus).map_err(|error| error.to_string())?;
    let connection = router.connection.clone();
    let destination = destination.to_owned();
    let path = path.to_owned();
    let interface = interface.to_owned();
    let method = method.to_owned();
    let timeout = timeout.min(MAX_ASYNC_CALL_TIMEOUT);
    spawn_call(Some(Instant::now() + timeout), move || {
        call_on(
            &connection,
            &destination,
            &path,
            &interface,
            &method,
            &arguments,
        )
    })
}

/// Whether `name` has an owner on `bus` right now.
///
/// Asked of the bus itself, which never activates anything to answer it — so
/// this, unlike reading a property, cannot start the service it asks about.
pub fn name_has_owner(bus: Bus, name: &str) -> Result<bool, String> {
    let router = router(bus).map_err(|error| error.to_string())?;
    let name = zbus::names::BusName::try_from(name).map_err(|error| error.to_string())?;
    let proxy = zbus::blocking::fdo::DBusProxy::new(&router.connection)
        .map_err(|error| error.to_string())?;
    proxy
        .name_has_owner(name)
        .map_err(|error| error.to_string())
}

/// The unique name owning `name`, or `None` when nobody does.
pub fn name_owner(bus: Bus, name: &str) -> Result<Option<String>, String> {
    let router = router(bus).map_err(|error| error.to_string())?;
    ask_owner(&router.connection, name)
}

/// Every name on `bus`, unique ones included.
pub fn list_names(bus: Bus) -> Result<Vec<String>, String> {
    let router = router(bus).map_err(|error| error.to_string())?;
    let proxy = zbus::blocking::fdo::DBusProxy::new(&router.connection)
        .map_err(|error| error.to_string())?;
    let names = proxy.list_names().map_err(|error| error.to_string())?;
    Ok(names.into_iter().map(|name| name.to_string()).collect())
}

/// Hears `NameOwnerChanged` for one name only.
///
/// The bus sends that signal for every name — every client that connects or
/// leaves is one — and a library that wants one service used to hear them all
/// and filter in Lua. The filter is now the match rule's `arg0`, applied by the
/// bus before anything is sent.
pub fn subscribe_name_owner_changed(bus: Bus, name: &str) -> zbus::Result<DbusSignal> {
    router(bus)?.subscribe(Route {
        sender: BUS_NAME.to_owned(),
        path: "/org/freedesktop/DBus".to_owned(),
        interface: BUS_NAME.to_owned(),
        member: "NameOwnerChanged".to_owned(),
        arg0: Some(name.to_owned()),
    })
}

/// Hears one signal from `sender` on the process's shared connection,
/// without a proxy — and so without the connection and handshake a proxy
/// costs, which matters for something every runtime does at startup.
pub fn subscribe_signal(
    bus: Bus,
    sender: &str,
    path: &str,
    interface: &str,
    member: &str,
) -> zbus::Result<DbusSignal> {
    router(bus)?.subscribe(Route {
        sender: sender.to_owned(),
        path: path.to_owned(),
        interface: interface.to_owned(),
        member: member.to_owned(),
        arg0: None,
    })
}

/// The bus daemon's own name, which is also the sender of its signals.
const BUS_NAME: &str = "org.freedesktop.DBus";

/// Asks the bus who owns `name`. `None` is nobody, which is not an error.
fn ask_owner(connection: &DbusConnection, name: &str) -> Result<Option<String>, String> {
    let bus_name = zbus::names::BusName::try_from(name).map_err(|error| error.to_string())?;
    let proxy =
        zbus::blocking::fdo::DBusProxy::new(connection).map_err(|error| error.to_string())?;
    match proxy.get_name_owner(bus_name) {
        Ok(owner) => Ok(Some(owner.to_string())),
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// One method call on `connection`, arguments by the positional convention.
fn call_on(
    connection: &DbusConnection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    arguments: &DbusValue,
) -> Result<DbusValue, String> {
    let message = match positional_arguments(arguments)? {
        Some(body) => {
            connection.call_method(Some(destination), path, Some(interface), method, &body)
        }
        None => connection.call_method(Some(destination), path, Some(interface), method, &()),
    }
    .map_err(|error| error.to_string())?;
    decode_message_value(&message)
}

/// A call's arguments as one message body, `None` for none.
///
/// A list is the positional arguments; anything else is the one argument. A
/// bare map is refused: `a{sv}` is the obvious guess for one, but the caller
/// who meant it can say so, and the one who did not gets an error instead of a
/// call the service rejects for a reason it will not explain.
fn positional_arguments(value: &DbusValue) -> Result<Option<Structure<'_>>, String> {
    let values = match value {
        DbusValue::Nil => return Ok(None),
        DbusValue::List(values) if values.is_empty() => return Ok(None),
        DbusValue::List(values) => values.as_slice(),
        DbusValue::Map(_) | DbusValue::Dictionary(_) => {
            return Err("D-Bus maps need an explicit signature".to_owned());
        }
        other => std::slice::from_ref(other),
    };
    let mut body = StructureBuilder::new();
    for value in values {
        body = body.append_field(dbus_argument_value(value)?);
    }
    body.build().map(Some).map_err(|error| error.to_string())
}
