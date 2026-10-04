//! What the person asked their desktop for, read from the settings portal.
//!
//! Five preferences — `color_scheme`, `contrast`, `reduced_motion`,
//! `accent_color` and `scale` — of which the portal answers the first four
//! over D-Bus and keeps them current from its change signal. Nothing here
//! blocks: the portal is asked only once it has an owner, and its answers are
//! collected by [`Portal::poll`] as plain values for the shell to store.

use std::collections::HashSet;
use std::time::Duration;

use morf_io::{Bus, DbusSignal, DbusValue, PendingReply};
use morf_value::{Color, IpcValue};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const INTERFACE: &str = "org.gnome.desktop.interface";

/// Every preference, by name.
pub const PREFERENCES: [&str; 5] = [
    "color_scheme",
    "contrast",
    "reduced_motion",
    "accent_color",
    "scale",
];

/// A variant's payload, however many layers of typing wrap it.
fn plain(value: DbusValue) -> DbusValue {
    match value {
        DbusValue::Typed { value, .. } => plain(*value),
        other => other,
    }
}

fn unsigned(value: &DbusValue) -> Option<u64> {
    match value {
        DbusValue::Unsigned(value) => Some(*value),
        DbusValue::Integer(value) => u64::try_from(*value).ok(),
        DbusValue::Number(value) => Some(*value as u64),
        _ => None,
    }
}

/// What a portal setting means as a preference, or nothing when the key is
/// not one that is followed.
pub fn preference_from_setting(
    namespace: &str,
    key: &str,
    value: DbusValue,
) -> Option<(&'static str, IpcValue)> {
    let value = plain(value);
    match (namespace, key) {
        (APPEARANCE, "color-scheme") => Some((
            "color_scheme",
            IpcValue::String(
                match unsigned(&value)? {
                    1 => "dark",
                    2 => "light",
                    _ => "none",
                }
                .to_owned(),
            ),
        )),
        (APPEARANCE, "contrast") => Some((
            "contrast",
            IpcValue::String(
                if unsigned(&value)? == 1 {
                    "high"
                } else {
                    "none"
                }
                .to_owned(),
            ),
        )),
        (APPEARANCE, "accent-color") => {
            let DbusValue::List(channels) = value else {
                return None;
            };
            let channel = |index: usize| match channels.get(index)? {
                DbusValue::Number(value) => Some(*value),
                _ => None,
            };
            let (red, green, blue) = (channel(0)?, channel(1)?, channel(2)?);
            // Out of range means the desktop has no accent to offer.
            let none = [red, green, blue]
                .iter()
                .any(|value| !(0.0..=1.0).contains(value));
            Some((
                "accent_color",
                if none {
                    IpcValue::Nil
                } else {
                    IpcValue::Color(Color {
                        red: red as f32,
                        green: green as f32,
                        blue: blue as f32,
                        alpha: 1.0,
                    })
                },
            ))
        }
        (INTERFACE, "enable-animations") => match value {
            DbusValue::Bool(enabled) => Some(("reduced_motion", IpcValue::Boolean(!enabled))),
            _ => None,
        },
        _ => None,
    }
}

/// The settings followed, by portal namespace and key.
const SETTINGS_READ: [(&str, &str); 4] = [
    (APPEARANCE, "color-scheme"),
    (APPEARANCE, "contrast"),
    (APPEARANCE, "accent-color"),
    (INTERFACE, "enable-animations"),
];

/// How long a reading may take before it is given up on. Generous, because
/// nothing waits for it: the preferences hold their defaults meanwhile.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Asks the portal for every setting followed, answered later.
///
/// This used to be four blocking calls, made while the runtime was being
/// built — before anything was drawn. A portal that had to be started by
/// activation, or a slow one, held the whole shell off the screen for as long
/// as it took. Now the calls go out and the answers fill in when they come.
pub fn ask_portal() -> Vec<(&'static str, &'static str, PendingReply)> {
    SETTINGS_READ
        .iter()
        .filter_map(|&(namespace, key)| {
            morf_io::call_async(
                Bus::Session,
                PORTAL,
                PORTAL_PATH,
                SETTINGS,
                "ReadOne",
                DbusValue::List(vec![
                    DbusValue::String(namespace.to_owned()),
                    DbusValue::String(key.to_owned()),
                ]),
                READ_TIMEOUT,
            )
            .ok()
            .map(|reply| (namespace, key, reply))
        })
        .collect()
}

/// How the settings portal is followed.
///
/// Nothing here blocks: the portal is asked only once it has an owner — a
/// read of an absent portal would activate it, and activating a portal can
/// take the whole of a call's timeout — and its answers are collected from a
/// poll.
pub struct Portal {
    /// `SettingChanged`, from whoever owns the portal's name.
    pub changes: DbusSignal,
    /// The portal's name changing hands: a portal that starts after the
    /// shell is read when it arrives.
    pub owner: DbusSignal,
    /// Readings asked for and not yet answered: namespace, key, reply.
    pub pending: Vec<(&'static str, &'static str, PendingReply)>,
}

impl Portal {
    /// Starts following the settings portal: its change signal, its name,
    /// and — only if something already owns the name — a first reading.
    /// `None` when there is no session bus.
    ///
    /// Never by activation. Asking the bus whether the name has an owner
    /// cannot start anything; a call to the name would, and a portal started
    /// that way can take the whole of a call's timeout to come up. A portal
    /// that starts later is read when its name arrives.
    pub fn watch() -> Option<Self> {
        let changes = morf_io::subscribe_signal(
            Bus::Session,
            PORTAL,
            PORTAL_PATH,
            SETTINGS,
            "SettingChanged",
        )
        .ok()?;
        let owner = morf_io::subscribe_name_owner_changed(Bus::Session, PORTAL).ok()?;
        let pending = if morf_io::name_has_owner(Bus::Session, PORTAL).unwrap_or(false) {
            ask_portal()
        } else {
            Vec::new()
        };
        Some(Self {
            changes,
            owner,
            pending,
        })
    }

    /// What the portal said since the last poll, as preferences to write.
    /// A first reading of a preference in `overridden` is dropped: the host
    /// set it after the question went out, so the host's is the newer word.
    pub fn poll(&mut self, overridden: &HashSet<&'static str>) -> Vec<(&'static str, IpcValue)> {
        let mut changes = Vec::new();
        // The portal arriving (or changing hands) is a reason to read it; it
        // leaving is not, and what it said last stands.
        let mut arrived = false;
        while let Some(event) = self.owner.next_event(Duration::ZERO) {
            if let Ok(DbusValue::List(parts)) = event.arguments
                && matches!(parts.get(2), Some(DbusValue::String(new)) if !new.is_empty())
            {
                arrived = true;
            }
        }
        if arrived {
            self.pending = ask_portal();
        }
        self.pending.retain(|(namespace, key, reply)| {
            let Some(answer) = reply.try_take() else {
                return true;
            };
            // `ReadOne` has one output, a variant, and a reply decodes as
            // the list of its outputs.
            let answer = answer.map(|value| match value {
                DbusValue::List(mut outputs) if outputs.len() == 1 => outputs.remove(0),
                other => other,
            });
            if let Ok(value) = answer
                && let Some(change) = preference_from_setting(namespace, key, value)
                && !overridden.contains(change.0)
            {
                changes.push(change);
            }
            false
        });
        while let Some(Ok(value)) = self.changes.next_value(Duration::ZERO) {
            let DbusValue::List(parts) = value else {
                continue;
            };
            if let [DbusValue::String(namespace), DbusValue::String(key), value] = parts.as_slice()
                && let Some(change) = preference_from_setting(namespace, key, value.clone())
            {
                changes.push(change);
            }
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(value: DbusValue) -> DbusValue {
        DbusValue::Typed {
            signature: "v".into(),
            value: Box::new(value),
        }
    }

    #[test]
    fn a_colour_scheme_reads_through_its_variants() {
        assert_eq!(
            preference_from_setting(APPEARANCE, "color-scheme", typed(DbusValue::Unsigned(1))),
            Some(("color_scheme", IpcValue::String("dark".into())))
        );
        assert_eq!(
            preference_from_setting(APPEARANCE, "color-scheme", DbusValue::Unsigned(0)),
            Some(("color_scheme", IpcValue::String("none".into())))
        );
    }

    #[test]
    fn an_accent_out_of_range_is_none() {
        let accent = |red| {
            DbusValue::List(vec![
                DbusValue::Number(red),
                DbusValue::Number(0.5),
                DbusValue::Number(0.5),
            ])
        };
        assert_eq!(
            preference_from_setting(APPEARANCE, "accent-color", accent(-1.0)),
            Some(("accent_color", IpcValue::Nil))
        );
        assert!(matches!(
            preference_from_setting(APPEARANCE, "accent-color", accent(0.25)),
            Some(("accent_color", IpcValue::Color(_)))
        ));
    }

    #[test]
    fn animations_off_is_reduced_motion() {
        assert_eq!(
            preference_from_setting(INTERFACE, "enable-animations", DbusValue::Bool(false)),
            Some(("reduced_motion", IpcValue::Boolean(true)))
        );
        assert_eq!(
            preference_from_setting(INTERFACE, "font-name", DbusValue::Bool(false)),
            None
        );
    }
}
