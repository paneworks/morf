//! The bus names a runtime owns and who answers the calls on each.
//!
//! A [`DbusService`] owns a name and hands out arriving calls; a runtime may
//! hold up to [`MAX_DBUS_SERVICES`] of them, each with one handler (`C`, kept
//! opaque here). [`DbusHandlers::drain_calls`] takes at most
//! [`MAX_CALLS_PER_FRAME`] calls from each service per turn and pairs them
//! with their handler for the caller to run. Alongside: the small readings
//! of a script's arguments a binding needs — a bus by name, a call id.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::{Bus, DbusCall, DbusService, NameOutcome};

/// How many names one configuration may hold.
///
/// A shell owns a handful — notifications, a tray watcher, its own control
/// interface. A configuration asking for hundreds has a loop in it.
pub const MAX_DBUS_SERVICES: usize = 32;

/// How many calls one service may hand over per turn.
///
/// Bounded per frame, unlike a signal drain: a signal that arrives faster
/// than it is read is the sender's problem; a call that does is ours,
/// because the caller is blocked until it is answered and answering happens
/// after the drain. Taking them all would let one chatty peer hold the
/// frame open.
pub const MAX_CALLS_PER_FRAME: usize = 32;

/// A shared service, as the runtime and the script's handle both hold it.
pub type SharedService = Rc<RefCell<DbusService>>;

/// Each owned name's handler.
pub struct DbusHandlers<C> {
    entries: Vec<(SharedService, C)>,
}

impl<C> Default for DbusHandlers<C> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<C: Clone> DbusHandlers<C> {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Makes `callback` the one handler of `service`. One per service: a
    /// second replaces the first rather than fanning out, because two
    /// handlers answering one call means one replies to a call the other
    /// already answered.
    pub fn set(&mut self, service: &SharedService, callback: C) -> Result<(), String> {
        if self.entries.len() >= MAX_DBUS_SERVICES {
            return Err("D-Bus service limit reached".into());
        }
        let entry = (Rc::clone(service), callback);
        match self
            .entries
            .iter()
            .position(|(known, _)| Rc::ptr_eq(known, service))
        {
            Some(index) => self.entries[index] = entry,
            None => self.entries.push(entry),
        }
        Ok(())
    }

    /// Forgets `service`; with the handle gone too, dropping it gives the
    /// name to whoever is queued behind.
    pub fn remove(&mut self, service: &SharedService) {
        self.entries
            .retain(|(known, _)| !Rc::ptr_eq(known, service));
    }

    /// The calls that arrived, each with its handler; at most
    /// [`MAX_CALLS_PER_FRAME`] per service.
    pub fn drain_calls(&self) -> Vec<(C, DbusCall)> {
        let mut calls = Vec::new();
        for (service, callback) in &self.entries {
            for _ in 0..MAX_CALLS_PER_FRAME {
                let Some(call) = service.borrow_mut().next_call(Duration::ZERO) else {
                    break;
                };
                calls.push((callback.clone(), call));
            }
        }
        calls
    }
}

/// Remembers `service` weakly among the names a runtime gives back when it
/// ends, forgetting those already gone.
pub fn remember_name(names: &mut Vec<Weak<RefCell<DbusService>>>, service: &SharedService) {
    names.retain(|weak| weak.strong_count() > 0);
    names.push(Rc::downgrade(service));
}

/// `"session"` or `"system"`.
pub fn bus_named(name: &str) -> Result<Bus, String> {
    match name {
        "session" => Ok(Bus::Session),
        "system" => Ok(Bus::System),
        _ => Err(format!("unknown D-Bus bus `{name}`")),
    }
}

/// Narrows a script's integer to the call id the service handed out.
///
/// Ids are opaque and only ever come from a call, so anything that is not a
/// positive integer is a configuration replying to something it invented.
pub fn call_id(id: i64) -> Result<u64, String> {
    u64::try_from(id).map_err(|_| format!("`{id}` is not a D-Bus call id"))
}

impl NameOutcome {
    /// `"owned"`, `"taken"` or `"queued"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Taken => "taken",
            Self::Queued => "queued",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_read() {
        assert_eq!(bus_named("session"), Ok(Bus::Session));
        assert_eq!(bus_named("system"), Ok(Bus::System));
        assert!(bus_named("starter").is_err());
        assert_eq!(call_id(7), Ok(7));
        assert_eq!(call_id(-1).unwrap_err(), "`-1` is not a D-Bus call id");
        assert_eq!(NameOutcome::Queued.name(), "queued");
        let handlers = DbusHandlers::<u8>::default();
        assert!(handlers.is_empty());
        assert!(handlers.drain_calls().is_empty());
    }
}
