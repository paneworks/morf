//! Peer credentials, ownership fences and payload checks for controller work.
use crate::provider::transport::{PeerIdentity, birth};
use serde_json::Value;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub(super) fn decimal(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    let number = text.parse::<u64>().ok()?;
    (text == number.to_string()).then_some(number)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Fence {
    broker: [u64; 2],
    configuration: u64,
    session: u64,
}
impl Fence {
    pub(super) fn parse(value: &Value) -> Option<Self> {
        if value.as_object()?.len() != 3 || value["broker"].as_array()?.len() != 2 {
            return None;
        }
        let result = Self {
            broker: [value["broker"][0].as_u64()?, value["broker"][1].as_u64()?],
            configuration: value["configuration"].as_u64()?,
            session: value["session"].as_u64()?,
        };
        (result.broker != [0, 0] && result.configuration > 0 && result.session > 0)
            .then_some(result)
    }
    pub(super) fn advances(self, old: Self) -> bool {
        self.broker == old.broker
            && self.configuration >= old.configuration
            && self.session >= old.session
            && self != old
    }
}
pub(super) struct Issuer {
    pid: u32,
    start: u64,
    fd: OwnedFd,
}
impl Issuer {
    pub(super) fn open(peer: PeerIdentity) -> Option<Self> {
        if peer.uid != 0 {
            return None;
        }
        let start = birth(peer.pid).ok()?;
        // SAFETY: fixed syscall and flags; successful return is a new descriptor.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, peer.pid, 0) };
        if raw < 0 {
            return None;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw as i32) };
        if birth(peer.pid).ok() != Some(start) {
            return None;
        }
        Some(Self {
            pid: peer.pid,
            start,
            fd,
        })
    }
    pub(super) fn alive(&self) -> bool {
        let mut poll = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: initialized single live pidfd; a zero-time readiness query.
        unsafe { libc::poll(&mut poll, 1, 0) == 0 }
    }
    pub(super) fn matches(&self, peer: PeerIdentity) -> bool {
        peer.uid == 0
            && peer.pid == self.pid
            && self.alive()
            && birth(peer.pid).ok() == Some(self.start)
    }
}
pub(super) fn hex(value: &Value) -> bool {
    value.as_str().is_some_and(|v| {
        v.len() == 32
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub(super) fn gesture_key(epoch: &Value, gesture: &Value) -> Option<String> {
    (hex(epoch) && hex(gesture))
        .then(|| format!("{}:{}", epoch.as_str().unwrap(), gesture.as_str().unwrap()))
}
pub(super) fn effect_token(token: &str) -> Option<(&str, &str)> {
    let (base, suffix) = token.split_at_checked(token.len().checked_sub(66)?)?;
    let key = suffix.strip_prefix(':')?;
    let (epoch, gesture) = key.split_once(':')?;
    let valid = |s: &str| {
        s.len() == 32
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    (valid(epoch) && valid(gesture)).then_some((base, key))
}
pub(super) fn valid_payload(r: &Value, output: &str) -> bool {
    if r.as_object().is_none_or(|o| o.len() != 11)
        || r["version"] != 1
        || r["output"] != output
        || !hex(&r["epoch"])
        || !hex(&r["gesture"])
    {
        return false;
    }
    let Some(ordinal) = r["ordinal"]
        .as_u64()
        .filter(|v| (1..=i32::MAX as u64).contains(v))
    else {
        return false;
    };
    let Some(phase) = r["phase"].as_str() else {
        return false;
    };
    let Some(action) = r["action"].as_str() else {
        return false;
    };
    let continuous = matches!(
        action,
        "dashboard" | "notifications" | "quick_settings" | "workspace_preview"
    );
    let discrete = continuous && action != "workspace_preview"
        || matches!(
            action,
            "apps" | "web" | "keyboard_show" | "keyboard_hide" | "keyboard_toggle_layout"
        );
    if !(phase == "discrete" && discrete && ordinal == 1
        || phase == "begin" && continuous && ordinal == 1
        || matches!(phase, "update" | "end" | "cancel") && continuous && ordinal > 1)
    {
        return false;
    }
    ["dx", "dy", "vx", "vy"].iter().all(|key| {
        r[key]
            .as_f64()
            .is_some_and(|v| v.is_finite() && v.abs() <= 1_000_000.)
    })
}
