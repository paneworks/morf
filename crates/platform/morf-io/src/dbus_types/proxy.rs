//! The proxy handle, its values, and the opaque fd that rides in them.

use crate::dbus_decode::basic_value;
use crate::dbus_encode::dbus_argument_value;
use crate::dbus_encode::decode_message_value;
use std::collections::BTreeMap;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use zbus::blocking::Proxy as ZbusProxy;

use crate::dbus_decode::DbusSignal;
use zbus::zvariant::{DynamicDeserialize, DynamicType, OwnedValue, Value};

use super::{
    Bus, DEFAULT_CALL_TIMEOUT, PendingReply, Route, positional_arguments, router,
    shared_connection, spawn_call,
};

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
    /// Dictionaries with non-string keys, such as ModemManager's `a{uu}`
    /// UnlockRetries. Preserve the keys rather than rejecting the whole reply.
    Dictionary(Vec<(DbusValue, DbusValue)>),
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
        let connection = shared_connection(bus, timeout)?;
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
    /// [`super::MAX_CALLS_IN_FLIGHT`] and by this proxy's own call timeout.
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
            DbusValue::List(_) | DbusValue::Map(_) | DbusValue::Dictionary(_) => {
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
