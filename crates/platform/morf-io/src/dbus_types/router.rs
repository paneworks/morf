//! Signal subscriptions: one match-rule router per bus, shared by every proxy.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;

use zbus::blocking::Connection as DbusConnection;

use crate::dbus_decode::DbusSignal;
use zbus::zvariant::{Structure, Value};

use super::{BUS_NAME, Bus, MAX_ASYNC_CALL_TIMEOUT, ask_owner};

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
    pub(super) connection: DbusConnection,
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

    pub(super) fn subscribe(self: &Arc<Self>, route: Route) -> zbus::Result<DbusSignal> {
        if route.resolves_sender() {
            self.track(&route.sender);
        }
        // The route goes in before the bus is asked for the signal: once the
        // match rule is in, the reader can receive a matching message at any
        // moment, and one that arrived before its route existed was read,
        // matched nothing, and was dropped -- a name that appeared in that
        // window was never heard of again.
        let (tx, events) = mpsc::channel();
        let id = {
            let mut next = self
                .next_id
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            *next = next.wrapping_add(1);
            *next
        };
        let rule = route.rule();
        self.routes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((id, route.clone(), tx));
        // Rules are reference counted by the bus, so two subscriptions to the
        // same signal add it twice and it survives until both have gone.
        let added = rule.and_then(|rule| {
            zbus::blocking::fdo::DBusProxy::new(&self.connection)?
                .add_match_rule(rule)
                .map_err(zbus::Error::from)
        });
        if let Err(error) = added {
            self.routes
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .retain(|(held, _, _)| *held != id);
            if route.resolves_sender() {
                self.untrack(&route.sender);
            }
            return Err(error);
        }
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
pub(super) fn router(bus: Bus) -> zbus::Result<Arc<SignalRouter>> {
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
