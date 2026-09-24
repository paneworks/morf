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

use crate::dbus_decode::basic_value;
use crate::dbus_encode::dbus_argument_value;
use crate::dbus_encode::decode_message_value;
use std::collections::{BTreeMap, HashMap};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use zbus::blocking::{Connection as DbusConnection, Proxy as ZbusProxy};

use crate::dbus_decode::DbusSignal;
use zbus::zvariant::{
    DynamicDeserialize, DynamicType, OwnedValue, Structure, StructureBuilder, Value,
};

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
        DbusValue::Map(_) => return Err("D-Bus maps need an explicit signature".to_owned()),
        other => std::slice::from_ref(other),
    };
    let mut body = StructureBuilder::new();
    for value in values {
        body = body.append_field(dbus_argument_value(value)?);
    }
    body.build().map(Some).map_err(|error| error.to_string())
}

/// A file descriptor that arrived over the bus, or is going back onto it.
///
/// Opaque on purpose: see the note at the top of this module. Cloning shares
/// the descriptor rather than duplicating it, and it is closed when the last
/// clone goes.
#[derive(Clone)]
pub struct DbusFd(Arc<OwnedFd>);

impl DbusFd {
    /// Wraps an owned descriptor.
    pub fn new(fd: OwnedFd) -> Self {
        Self(Arc::new(fd))
    }

    /// Borrows the descriptor, to put it into a message.
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl std::fmt::Debug for DbusFd {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "DbusFd({})", self.0.as_raw_fd())
    }
}

impl PartialEq for DbusFd {
    /// The same descriptor, not merely one open on the same file.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Typed generic D-Bus method and property client.
#[derive(Clone, Debug)]
pub struct DbusProxy {
    proxy: ZbusProxy<'static>,
    bus: Bus,
    destination: String,
    path: String,
    interface: String,
}

/// Bounded value transferable through the Lua D-Bus facade.
#[derive(Clone, Debug, PartialEq)]
pub enum DbusValue {
    Nil,
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Number(f64),
    String(String),
    /// A byte array (`ay`): an image's pixels, a file's contents, a
    /// NUL-terminated path. A string of bytes to Lua, not a list of numbers.
    Bytes(Vec<u8>),
    List(Vec<DbusValue>),
    Map(BTreeMap<String, DbusValue>),
    Typed {
        signature: String,
        value: Box<DbusValue>,
    },
    /// A file descriptor (`h`). See [`DbusFd`].
    Fd(DbusFd),
}

impl DbusProxy {
    /// Connects a proxy to one bus object and interface.
    ///
    /// Calls are bounded by [`DEFAULT_CALL_TIMEOUT`]; use
    /// [`Self::connect_with_timeout`] to choose another.
    pub fn connect(
        bus: Bus,
        destination: impl Into<String>,
        path: impl Into<String>,
        interface: impl Into<String>,
    ) -> zbus::Result<Self> {
        Self::connect_with_timeout(bus, destination, path, interface, DEFAULT_CALL_TIMEOUT)
    }

    /// Connects a proxy whose calls give up after `timeout`.
    ///
    /// The bound is on the connection rather than the call, which is where zbus
    /// puts it — so it is per proxy, and a configuration that genuinely needs to
    /// wait on one slow service can say so without slowing every other call it
    /// makes.
    pub fn connect_with_timeout(
        bus: Bus,
        destination: impl Into<String>,
        path: impl Into<String>,
        interface: impl Into<String>,
        timeout: Duration,
    ) -> zbus::Result<Self> {
        let connection = bus.builder()?.method_timeout(timeout).build()?;
        let destination = destination.into();
        let path = path.into();
        let interface = interface.into();
        let proxy = ZbusProxy::new_owned(
            connection,
            destination.clone(),
            path.clone(),
            interface.clone(),
        )?;
        Ok(Self {
            proxy,
            bus,
            destination,
            path,
            interface,
        })
    }

    /// How long this proxy's calls wait before giving up.
    ///
    /// Readable so the bound can be asserted on. A timeout is only observable
    /// by waiting for it, and a test that waits for a real one has to find a
    /// peer willing to accept a call and never answer — which is a harder thing
    /// to arrange than the bug is to prevent.
    pub fn call_timeout(&self) -> Option<Duration> {
        self.proxy.connection().method_timeout()
    }

    /// Returns the connection's unique bus name.
    pub fn unique_name(&self) -> Option<String> {
        self.proxy
            .connection()
            .unique_name()
            .map(ToString::to_string)
    }

    /// Calls one method and deserializes its reply body.
    pub fn call<B, R>(&self, method: &str, body: &B) -> zbus::Result<R>
    where
        B: Serialize + DynamicType,
        R: for<'de> DynamicDeserialize<'de>,
    {
        self.proxy.call(method, body)
    }

    /// Reads one remote property.
    pub fn get_property<T>(&self, property: &str) -> zbus::Result<T>
    where
        T: TryFrom<OwnedValue>,
        T::Error: Into<zbus::Error>,
    {
        self.proxy.get_property(property)
    }

    /// Writes one remote property.
    pub fn set_property<'value, T>(&self, property: &str, value: T) -> zbus::Result<()>
    where
        T: 'value + Into<Value<'value>>,
    {
        Ok(self.proxy.set_property(property, value)?)
    }

    /// Returns the remote object's introspection XML.
    pub fn introspect(&self) -> zbus::Result<String> {
        Ok(self.proxy.introspect()?)
    }

    /// Reads one property for an interpreter-facing facade.
    /// Reads a property without waiting for it.
    ///
    /// The blocking read holds the calling thread for the reply, and when the
    /// reply has to come from *this* process -- a tray host asking a watcher
    /// served from the same configuration -- the thread that would answer is
    /// the one waiting. The read happens on a thread of its own and the answer
    /// is collected later, from a poll.
    pub fn get_later(&self, property: &str) -> PendingReply {
        let proxy = self.clone();
        let property = property.to_owned();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(proxy.get_value(&property));
            crate::wake_all();
        });
        PendingReply { rx, deadline: None }
    }

    /// Calls a method without waiting for it, for the same reason.
    pub fn call_later_with(&self, method: &str, value: DbusValue) -> PendingReply {
        let proxy = self.clone();
        let method = method.to_owned();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(proxy.call_value_with(&method, &value));
            crate::wake_all();
        });
        PendingReply { rx, deadline: None }
    }

    /// Calls a method without waiting for it, bounded by
    /// [`MAX_CALLS_IN_FLIGHT`] and by this proxy's own call timeout.
    ///
    /// The configuration-facing form of [`Self::call_later_with`]: the same
    /// thread and poll, with a ceiling on how many a configuration may start.
    pub fn call_async(&self, method: &str, value: DbusValue) -> Result<PendingReply, String> {
        let proxy = self.clone();
        let method = method.to_owned();
        spawn_call(None, move || proxy.call_value_with(&method, &value))
    }

    pub fn get_value(&self, property: &str) -> Result<DbusValue, String> {
        let value: OwnedValue = self
            .proxy
            .get_property(property)
            .map_err(|error| error.to_string())?;
        basic_value(&value)
    }

    /// Calls a no-argument method returning a supported value.
    pub fn call_value(&self, method: &str) -> Result<DbusValue, String> {
        let message = self
            .proxy
            .call_method(method, &())
            .map_err(|error| error.to_string())?;
        decode_message_value(&message)
    }

    /// Calls a method with one scalar or a list of positional scalar arguments.
    pub fn call_value_with(&self, method: &str, value: &DbusValue) -> Result<DbusValue, String> {
        let message = match positional_arguments(value)? {
            Some(body) => self.proxy.call_method(method, &body),
            None => self.proxy.call_method(method, &()),
        }
        .map_err(|error| error.to_string())?;
        decode_message_value(&message)
    }

    /// Writes one property for an interpreter-facing facade.
    pub fn set_value(&self, property: &str, value: &DbusValue) -> Result<(), String> {
        let result = match value {
            DbusValue::Nil => return Err("D-Bus properties cannot be nil".to_owned()),
            DbusValue::Bool(value) => self.set_property(property, *value),
            DbusValue::Integer(value) => self.set_property(property, *value),
            DbusValue::Unsigned(value) => self.set_property(property, *value),
            DbusValue::Number(value) => self.set_property(property, *value),
            DbusValue::String(value) => self.set_property(property, value.as_str()),
            DbusValue::Typed { .. } | DbusValue::Fd(_) | DbusValue::Bytes(_) => {
                let value = dbus_argument_value(value)?;
                self.set_property(property, value)
            }
            // A list or a map needs its wire type stated, because there is no
            // way to infer one: an empty list could be an array of anything,
            // and a map of numbers could be `a{sd}` or `a{si}`. Reading needs
            // no such guess — the reply carries its own signature — which is
            // why this was asymmetric, and why the answer is to ask rather than
            // to refuse.
            DbusValue::List(_) | DbusValue::Map(_) => {
                return Err(
                    "a compound D-Bus property needs its signature stated: pass `{ signature = \"as\", value = ... }` rather than a bare table"
                        .to_owned(),
                );
            }
        };
        result.map_err(|error| error.to_string())
    }

    /// Subscribes to one signal on a dedicated bus connection.
    pub fn subscribe(&self, signal: impl Into<String>) -> zbus::Result<DbusSignal> {
        // One connection per bus, and one reader thread on it, with every
        // subscription a route rather than a thread of its own.
        //
        // This began as a connection each: a socket, an authentication
        // handshake and a bus name for every signal a configuration watched. A
        // thread each was the obvious next step and is the wrong one — a thread
        // blocked in `next()` cannot be woken, so ending a subscription would
        // mean either closing the shared connection (taking every other
        // subscription with it) or waiting for a message that may never come.
        // A route can simply be removed.
        let router = router(self.bus)?;
        router.subscribe(Route {
            sender: self.destination.clone(),
            path: self.path.clone(),
            interface: self.interface.clone(),
            member: signal.into(),
            arg0: None,
        })
    }
}

/// What a subscription is listening for.
///
/// Matched exactly, on all four, because these rules are built here rather than
/// parsed from a configuration — so the general case D-Bus match syntax allows
/// cannot arise, and reimplementing it to handle rules nobody can write would
/// be reimplementing it badly. `arg0` is the one addition, for the one signal
/// that is useless without it: `NameOwnerChanged`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Route {
    pub(crate) sender: String,
    pub(crate) path: String,
    pub(crate) interface: String,
    pub(crate) member: String,
    pub(crate) arg0: Option<String>,
}

impl Route {
    fn rule(&self) -> zbus::Result<zbus::MatchRule<'static>> {
        let mut builder = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(self.sender.as_str())?
            .path(self.path.as_str())?
            .interface(self.interface.as_str())?
            .member(self.member.as_str())?;
        if let Some(arg0) = &self.arg0 {
            builder = builder.arg(0, arg0.as_str())?;
        }
        Ok(builder.build().to_owned())
    }

    /// Whether the sender has to be looked up to be compared.
    ///
    /// A signal arrives stamped with its sender's *unique* name, and a
    /// subscription usually names a well-known one. The bus daemon's own
    /// signals, and a subscription that named a unique name, compare directly.
    fn resolves_sender(&self) -> bool {
        !self.sender.starts_with(':') && self.sender != BUS_NAME
    }

    /// Whether a message is what this route asked for.
    ///
    /// The sender is compared against the owner of the well-known name the
    /// subscription named, as last reported by the bus. This used to be
    /// skipped, on the grounds that the bus had already applied the rule — and
    /// it had, to the *connection*: two subscriptions to the same path on two
    /// different services share one connection, so every MPRIS player's
    /// `PropertiesChanged` went to every MPRIS player's handler. `owner` is
    /// `None` while that is not known yet, and then the old, lenient answer is
    /// the one given rather than dropping what might be the service's first
    /// signal.
    fn matches(
        &self,
        header: &zbus::message::Header<'_>,
        owner: Option<Option<&str>>,
        arg0: Option<&str>,
    ) -> bool {
        let addressed = header.path().is_some_and(|path| path.as_str() == self.path)
            && header
                .interface()
                .is_some_and(|interface| interface.as_str() == self.interface)
            && header
                .member()
                .is_some_and(|member| member.as_str() == self.member);
        if !addressed {
            return false;
        }
        if let Some(wanted) = &self.arg0
            && arg0 != Some(wanted.as_str())
        {
            return false;
        }
        let sender = header.sender().map(|sender| sender.as_str());
        if !self.resolves_sender() {
            return sender == Some(self.sender.as_str());
        }
        match owner {
            Some(Some(owner)) => sender == Some(owner),
            // Known to have no owner: nothing it sends can be for this route,
            // but a name that just arrived may speak before its
            // `NameOwnerChanged` is read — which cannot happen on one
            // connection, where the bus orders them. So this is a stale
            // signal from a previous owner, and it is dropped.
            Some(None) => false,
            None => true,
        }
    }
}

/// A well-known name some route is about, and who owns it.
struct Tracked {
    /// How many routes name it.
    routes: usize,
    /// Whether `owner` has been learned yet.
    known: bool,
    owner: Option<String>,
}

/// One reader per bus, dealing signals to whoever asked for them.
pub(crate) struct SignalRouter {
    connection: DbusConnection,
    routes: Mutex<Vec<(u64, Route, mpsc::Sender<zbus::Message>)>>,
    /// The owners of the well-known names routes are about.
    ///
    /// Kept current from `NameOwnerChanged`, which this reader hears for each
    /// of them through a rule of its own. Order on one connection is what
    /// makes that sound: the bus sends a name's change of owner before
    /// anything the new owner says, and this reader reads them in that order.
    owners: Mutex<HashMap<String, Tracked>>,
    next_id: Mutex<u64>,
}

/// A rule for one name's `NameOwnerChanged`.
fn owner_rule(name: &str) -> zbus::Result<zbus::MatchRule<'static>> {
    Route {
        sender: BUS_NAME.to_owned(),
        path: "/org/freedesktop/DBus".to_owned(),
        interface: BUS_NAME.to_owned(),
        member: "NameOwnerChanged".to_owned(),
        arg0: Some(name.to_owned()),
    }
    .rule()
}

impl SignalRouter {
    /// The unique bus name this reader's connection holds.
    pub(crate) fn connection_name(&self) -> Option<String> {
        self.connection.unique_name().map(ToString::to_string)
    }

    /// Starts following who owns `name`, for a route that names it.
    ///
    /// No lock is held across a call to the bus: the reader thread takes these
    /// same locks to deliver, and zbus stops reading the socket while a reader
    /// is not keeping up — so a lock held while waiting for a reply can be a
    /// lock the reply is waiting behind.
    fn track(&self, name: &str) {
        let first = {
            let mut owners = self
                .owners
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let entry = owners.entry(name.to_owned()).or_insert(Tracked {
                routes: 0,
                known: false,
                owner: None,
            });
            entry.routes += 1;
            entry.routes == 1
        };
        if !first {
            return;
        }
        if let Ok(rule) = owner_rule(name)
            && let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&self.connection)
        {
            let _ = proxy.add_match_rule(rule);
        }
        // Asked after the rule is in, so a change between the two is heard
        // rather than missed; and only written if the reader has not already
        // heard something newer.
        let Ok(owner) = ask_owner(&self.connection, name) else {
            return;
        };
        let mut owners = self
            .owners
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(entry) = owners.get_mut(name)
            && !entry.known
        {
            entry.known = true;
            entry.owner = owner;
        }
    }

    /// Stops following `name` once no route names it.
    fn untrack(&self, name: &str) {
        let last = {
            let mut owners = self
                .owners
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let Some(entry) = owners.get_mut(name) else {
                return;
            };
            entry.routes = entry.routes.saturating_sub(1);
            let last = entry.routes == 0;
            if last {
                owners.remove(name);
            }
            last
        };
        if last
            && let Ok(rule) = owner_rule(name)
            && let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&self.connection)
        {
            let _ = proxy.remove_match_rule(rule);
        }
    }

    fn subscribe(self: &Arc<Self>, route: Route) -> zbus::Result<DbusSignal> {
        if route.resolves_sender() {
            self.track(&route.sender);
        }
        // Ask the bus to deliver these. Rules are reference counted by the bus,
        // so two subscriptions to the same signal add it twice and it survives
        // until both have gone.
        let added = route.rule().and_then(|rule| {
            zbus::blocking::fdo::DBusProxy::new(&self.connection)?
                .add_match_rule(rule)
                .map_err(zbus::Error::from)
        });
        if let Err(error) = added {
            if route.resolves_sender() {
                self.untrack(&route.sender);
            }
            return Err(error);
        }
        let (tx, events) = mpsc::channel();
        let id = {
            let mut next = self
                .next_id
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            *next = next.wrapping_add(1);
            *next
        };
        self.routes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((id, route, tx));
        Ok(DbusSignal {
            events,
            router: Some(Arc::clone(self)),
            id,
        })
    }

    /// Drops a route and tells the bus to stop sending what only it wanted.
    pub(crate) fn unsubscribe(&self, id: u64) {
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(index) = routes.iter().position(|(held, _, _)| *held == id) else {
            return;
        };
        let (_, route, _) = routes.remove(index);
        drop(routes);
        if let Ok(rule) = route.rule()
            && let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&self.connection)
        {
            let _ = proxy.remove_match_rule(rule);
        }
        if route.resolves_sender() {
            self.untrack(&route.sender);
        }
    }

    /// How many routes are live. For tests: "closing a subscription removes
    /// it" should be something one can count.
    pub(crate) fn route_count(&self) -> usize {
        self.routes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len()
    }

    /// Hands one message to every route that asked for it.
    fn deliver(&self, message: &zbus::Message) {
        let header = message.header();
        if header.message_type() != zbus::message::Type::Signal {
            return;
        }
        // A change of owner is noted before anything is matched, so a route
        // that listens for the signal itself sees the table already current.
        if header
            .sender()
            .is_some_and(|sender| sender.as_str() == BUS_NAME)
            && header
                .member()
                .is_some_and(|member| member.as_str() == "NameOwnerChanged")
            && let Ok((name, _, new_owner)) =
                message.body().deserialize::<(String, String, String)>()
        {
            let mut owners = self
                .owners
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(entry) = owners.get_mut(&name) {
                entry.known = true;
                entry.owner = (!new_owner.is_empty()).then_some(new_owner);
            }
        }
        let owners = self
            .owners
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // A receiver that has gone is a subscription whose owner dropped it
        // without the route being removed — possible if a `DbusSignal` leaked.
        // Clearing them here keeps the list from growing forever.
        let mut delivered = false;
        let arg0 = routes
            .iter()
            .any(|(_, route, _)| route.arg0.is_some())
            .then(|| first_string_argument(message))
            .flatten();
        routes.retain(|(_, route, tx)| {
            let owner = owners
                .get(&route.sender)
                .filter(|entry| entry.known)
                .map(|entry| entry.owner.as_deref());
            if !route.matches(&header, owner, arg0.as_deref()) {
                return true;
            }
            delivered = true;
            tx.send(message.clone()).is_ok()
        });
        drop(routes);
        drop(owners);
        if delivered {
            crate::wake_all();
        }
    }
}

/// The one connection this process holds to `bus`, opened on first use.
///
/// Shared rather than pooled: a connection is a socket, a handshake and a name
/// on the bus, and there is no reason for a process to hold more than one of
/// each. Subscriptions are separated by their match rules, not by their
/// sockets, and asynchronous calls by their serials.
fn router(bus: Bus) -> zbus::Result<Arc<SignalRouter>> {
    static SESSION: OnceLock<Mutex<Option<Arc<SignalRouter>>>> = OnceLock::new();
    static SYSTEM: OnceLock<Mutex<Option<Arc<SignalRouter>>>> = OnceLock::new();
    let slot = match bus {
        Bus::Session => &SESSION,
        Bus::System => &SYSTEM,
    }
    .get_or_init(|| Mutex::new(None));
    // A poisoned lock means a previous caller panicked while connecting, which
    // says nothing about whether connecting works now.
    let mut held = slot.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(router) = held.as_ref() {
        return Ok(Arc::clone(router));
    }
    // The asynchronous calls ride this connection too, and each carries its
    // own deadline; the connection's bound is only the ceiling under them.
    let connection = bus
        .builder()?
        .method_timeout(MAX_ASYNC_CALL_TIMEOUT)
        .build()?;
    let router = Arc::new(SignalRouter {
        connection: connection.clone(),
        routes: Mutex::new(Vec::new()),
        owners: Mutex::new(HashMap::new()),
        next_id: Mutex::new(0),
    });
    // The one reader. It lives as long as the process, which is why nothing
    // has to be able to interrupt it: subscriptions come and go as routes, and
    // this never has to be stopped and restarted.
    let reading = Arc::clone(&router);
    thread::spawn(move || {
        for message in zbus::blocking::MessageIterator::from(connection) {
            let Ok(message) = message else { break };
            reading.deliver(&message);
        }
    });
    *held = Some(Arc::clone(&router));
    Ok(router)
}

/// How many subscriptions are live on `bus`'s shared connection.
///
/// For tests and diagnostics: a subscription that is closed should stop being
/// counted here, and one that leaks keeps a match rule on the bus.
pub fn subscription_count(bus: Bus) -> usize {
    router(bus).map_or(0, |router| router.route_count())
}

/// A signal's first argument, if it is a string: what an `arg0` rule matches.
fn first_string_argument(message: &zbus::Message) -> Option<String> {
    let body = message.body();
    if let Ok(text) = body.deserialize::<String>() {
        return Some(text);
    }
    let fields = body.deserialize::<Structure<'_>>().ok()?;
    match fields.fields().first() {
        Some(Value::Str(text)) => Some(text.to_string()),
        _ => None,
    }
}
