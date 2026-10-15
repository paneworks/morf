//! Root-issued, expiring native controller work. No Lua values cross threads.
use crate::process::native::{PeerIdentity, birth};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::Duration,
};

const QUEUE: usize = 8;
const RECEIPTS: usize = 64;
const LEASE: Duration = Duration::from_secs(1);
const GRANT: Duration = Duration::from_millis(250);

pub(super) fn now() -> Option<Duration> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: initialized output, fixed monotonic clock, no retained pointer.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0
        || time.tv_sec < 0
        || !(0..1_000_000_000).contains(&time.tv_nsec)
    {
        return None;
    }
    Some(Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
}
pub(super) fn nonce() -> Option<String> {
    let mut bytes = [0u8; 16];
    // SAFETY: exact live output buffer; nonblocking entropy request.
    if unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), libc::GRND_NONBLOCK) }
        != bytes.len() as isize
    {
        return None;
    }
    Some(bytes.iter().map(|v| format!("{v:02x}")).collect())
}
fn decimal(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    let number = text.parse::<u64>().ok()?;
    (text == number.to_string()).then_some(number)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Fence {
    broker: [u64; 2],
    configuration: u64,
    session: u64,
}
impl Fence {
    fn parse(value: &Value) -> Option<Self> {
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
    fn advances(self, old: Self) -> bool {
        self.broker == old.broker
            && self.configuration >= old.configuration
            && self.session >= old.session
            && self != old
    }
}
struct Issuer {
    pid: u32,
    start: u64,
    fd: OwnedFd,
}
impl Issuer {
    fn open(peer: PeerIdentity) -> Option<Self> {
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
    fn alive(&self) -> bool {
        let mut poll = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: initialized single live pidfd; a zero-time readiness query.
        unsafe { libc::poll(&mut poll, 1, 0) == 0 }
    }
    fn matches(&self, peer: PeerIdentity) -> bool {
        peer.uid == 0
            && peer.pid == self.pid
            && self.alive()
            && birth(peer.pid).ok() == Some(self.start)
    }
}
pub(super) struct Job {
    pub id: u64,
    pub token: String,
    pub payload: String,
    deadline: Duration,
}
pub(super) struct Controls {
    handoff: Option<super::handoff::State>,
    epoch: String,
    output: String,
    issuer: Option<Issuer>,
    fence: Option<Fence>,
    expires: Duration,
    token: String,
    revoked: bool,
    last_id: u64,
    last_gesture: String,
    pending: VecDeque<Job>,
    results: BTreeMap<u64, Value>,
    generation: u64,
    closed: bool,
}
impl Controls {
    pub fn new(epoch: String, output: String) -> Self {
        Self {
            handoff: None,
            epoch,
            output,
            issuer: None,
            fence: None,
            expires: Duration::ZERO,
            token: String::new(),
            revoked: true,
            last_id: 0,
            last_gesture: String::new(),
            pending: VecDeque::new(),
            results: BTreeMap::new(),
            generation: 1,
            closed: false,
        }
    }
    pub fn with_handoff(
        epoch: String,
        output: String,
        gate: std::sync::Arc<super::handoff::Gate>,
    ) -> Self {
        let mut state = Self::new(epoch, output);
        state.handoff = Some(super::handoff::State::new(gate));
        state
    }
    pub fn handoff_status(&mut self, at: Duration) -> Value {
        let valid = self.valid(&self.token, at);
        if let Some(handoff) = self.handoff.as_mut() {
            handoff.synchronize(valid);
        }
        json!({"version":1,"epoch":self.epoch,"ui_generation":self.generation.to_string(),"handoff":self.handoff.as_ref().map(|h|h.status())})
    }
    pub fn handoff_authorizes(&self, token: &str, enabled: bool) -> bool {
        self.handoff
            .as_ref()
            .is_some_and(|state| state.authorizes(token, enabled))
    }
    pub fn handoff_work(&mut self, at: Duration) -> Option<(String, bool)> {
        let valid = self.valid(&self.token, at);
        let handoff = self.handoff.as_mut()?;
        handoff.synchronize(valid);
        handoff.work()
    }
    pub fn handoff_observe(&mut self, boundary: super::handoff::Boundary, at: Duration) {
        if let Some(state) = self.handoff.as_mut() {
            state.observe(boundary, at);
        }
    }
    pub fn handoff_inhibition_failed(&mut self) -> Value {
        if let Some(state) = self.handoff.as_mut() {
            state.lost();
        }
        self.answer("rejected", Some("inhibition_failed"))
    }
    pub fn handoff_restore_ready(&self) -> bool {
        self.handoff.as_ref().is_some_and(|s| s.restore_ready())
    }
    pub fn handoff_take_restore(&mut self) -> Option<String> {
        self.handoff.as_mut()?.take_restore()
    }
    pub fn handoff_prepared_restore(&mut self, token: &str, success: bool) {
        if let Some(state) = self.handoff.as_mut() {
            state.prepared_restore(token, success);
        }
    }
    pub fn handoff_update(
        &mut self,
        token: &str,
        boundary: super::handoff::Boundary,
        ack: bool,
        driver: &str,
        at: Duration,
    ) -> bool {
        let Some(handoff) = self.handoff.as_mut() else {
            return false;
        };
        if !handoff.work().is_some_and(|(current, _)| current == token) {
            return false;
        }
        handoff.update(boundary, ack, driver, at)
    }
    pub fn valid(&self, token: &str, at: Duration) -> bool {
        !self.closed
            && !self.revoked
            && at < self.expires
            && token == self.token
            && self.issuer.as_ref().is_some_and(Issuer::alive)
    }
    pub fn valid_for_effect(&self, token: &str, at: Duration) -> bool {
        self.valid(token, at)
            && self
                .handoff
                .as_ref()
                .is_none_or(|state| state.permits_controls())
    }
    pub fn invalidate(&mut self) {
        if let Some(handoff) = self.handoff.as_mut() {
            handoff.lost();
        }
        self.revoked = true;
        while let Some(job) = self.pending.pop_front() {
            self.result(job.id, "rejected", Some("retired"));
        }
    }
    pub fn close(&mut self) {
        self.invalidate();
        self.closed = true;
    }
    pub fn reset_ui(&mut self) {
        self.retire_for_reload();
        if let Some(next) = self.generation.checked_add(1) {
            self.generation = next;
        } else {
            self.closed = true;
        }
    }
    pub fn retire_for_reload(&mut self) {
        if let Some(handoff) = self.handoff.as_mut() {
            handoff.lost();
        }
        while let Some(job) = self.pending.pop_front() {
            self.result(job.id, "rejected", Some("ui_replaced"));
        }
        self.token.clear();
        self.expires = Duration::ZERO;
    }
    pub fn capabilities(&self) -> Value {
        json!({"version":1,"epoch":self.epoch,"ui_generation":self.generation.to_string(),"root_only":true,"queue":QUEUE,"receipts":RECEIPTS,"lease_ms":1000,"grant_ms":250,"recognizer_ownership":false})
    }
    fn answer(&self, status: &str, error: Option<&str>) -> Value {
        json!({"version":1,"epoch":self.epoch,"ui_generation":self.generation.to_string(),"status":status,"error":error})
    }
    fn result(&mut self, id: u64, status: &str, error: Option<&str>) {
        if self.results.len() >= RECEIPTS && !self.results.contains_key(&id) {
            self.results.pop_first();
        }
        let mut result = self.answer(status, error);
        result["id"] = json!(id.to_string());
        self.results.insert(id, result);
    }
    pub fn request(
        &mut self,
        peer: PeerIdentity,
        r: &Value,
        at: Duration,
        candidate: &str,
    ) -> Option<Value> {
        if peer.uid != 0 || self.closed || r["version"] != 1 || r["epoch"] != self.epoch {
            return None;
        }
        let fence = Fence::parse(&r["fence"])?;
        let fields = r.as_object()?.len();
        let refused = |s: &Self, error| Some(s.answer("rejected", Some(error)));
        if decimal(&r["ui_generation"]) != Some(self.generation) {
            return refused(self, "ui_replaced");
        }
        if r["op"] == "lease" && fields == 6 {
            let expires = Duration::from_nanos(decimal(&r["expires_ns"])?);
            if expires <= at || expires - at > LEASE {
                return refused(self, "invalid_deadline");
            }
            let same = self
                .issuer
                .as_ref()
                .is_some_and(|issuer| issuer.matches(peer));
            if !same && self.issuer.as_ref().is_some_and(Issuer::alive) {
                return refused(self, "owner_alive");
            }
            let advances = same && self.fence.is_some_and(|old| fence.advances(old));
            if same && self.fence != Some(fence) && !advances {
                return refused(self, "stale_fence");
            }
            if same && self.revoked && !advances {
                return refused(self, "retired");
            }
            let fresh = !same || advances;
            if !same {
                self.issuer = Some(Issuer::open(peer)?);
            }
            if fresh {
                if let Some(handoff) = self.handoff.as_mut() {
                    handoff.lost();
                }
                self.last_id = 0;
                self.last_gesture.clear();
                self.pending.clear();
                self.results.clear();
            }
            if fresh || at >= self.expires {
                self.token = candidate.to_owned();
            }
            self.fence = Some(fence);
            self.expires = expires;
            self.revoked = false;
            return Some(self.answer("accepted", None));
        }
        if self.fence != Some(fence)
            || !self
                .issuer
                .as_ref()
                .is_some_and(|issuer| issuer.matches(peer))
        {
            return refused(self, "stale_fence");
        }
        if matches!(
            r["op"].as_str(),
            Some("handoff-quiesce" | "handoff-activate" | "handoff-release")
        ) {
            if !self.valid(&self.token, at) {
                return refused(self, "expired_lease");
            }
            let op = r["op"].as_str()?;
            if fields != if op == "handoff-release" { 7 } else { 6 } {
                return None;
            }
            let generation = decimal(&r["ownership_generation"])?;
            let Some(handoff) = self.handoff.as_mut() else {
                return refused(self, "handoff_unavailable");
            };
            if let Err(error) = handoff.request(
                op,
                generation,
                r["descriptors_released"] == true,
                candidate,
                at,
            ) {
                return refused(self, error);
            }
            if op == "handoff-release" {
                // Retire queued effects before global recognizers can resume.
                self.revoked = true;
                while let Some(job) = self.pending.pop_front() {
                    self.result(job.id, "rejected", Some("retired"));
                }
            }
            return Some(self.answer("accepted", None));
        }
        if r["op"] == "revoke" && fields == 5 {
            self.invalidate();
            return Some(self.answer("accepted", None));
        }
        if r["op"] == "receipt" && fields == 6 {
            let id = decimal(&r["id"])?;
            return Some(
                self.results
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| self.answer("rejected", Some("retired"))),
            );
        }
        if r["op"] != "submit" || fields != 8 {
            return None;
        }
        let id = decimal(&r["id"])?;
        if id == 0 || id <= self.last_id {
            return refused(self, "duplicate");
        }
        if !self.valid(&self.token, at) {
            return refused(self, "expired_lease");
        }
        if self
            .handoff
            .as_ref()
            .is_some_and(|state| !state.permits_controls())
        {
            return refused(self, "handoff_required");
        }
        let deadline = Duration::from_nanos(decimal(&r["deadline_ns"])?);
        if deadline <= at || deadline - at > GRANT || deadline > self.expires {
            return refused(self, "invalid_deadline");
        }
        let payload = &r["request"];
        if !valid_payload(payload, &self.output) {
            return refused(self, "invalid_request");
        }
        self.last_id = id;
        if self.pending.len() >= QUEUE {
            self.result(id, "rejected", Some("capacity"));
            return self.results.get(&id).cloned();
        }
        if matches!(payload["phase"].as_str(), Some("discrete" | "begin")) {
            let gesture = payload["gesture"].as_str()?;
            if gesture <= self.last_gesture.as_str() {
                self.result(id, "rejected", Some("duplicate_gesture"));
                return self.results.get(&id).cloned();
            }
            self.last_gesture = gesture.to_owned();
        }
        self.pending.push_back(Job {
            id,
            token: self.token.clone(),
            payload: payload.to_string(),
            deadline,
        });
        self.result(id, "queued", None);
        self.results.get(&id).cloned()
    }
    pub fn take(&mut self, at: Duration) -> Option<Job> {
        while let Some(job) = self.pending.pop_front() {
            if at >= job.deadline || !self.valid_for_effect(&job.token, at) {
                self.result(job.id, "rejected", Some("expired"));
            } else {
                return Some(job);
            }
        }
        None
    }
    pub fn complete(&mut self, job: &Job, accepted: bool) {
        if job.token == self.token {
            self.result(
                job.id,
                if accepted { "dispatched" } else { "rejected" },
                if accepted {
                    None
                } else {
                    Some("controller_refused")
                },
            );
        }
    }
}
fn valid_payload(r: &Value, output: &str) -> bool {
    let hex = |value: &Value| {
        value.as_str().is_some_and(|v| {
            v.len() == 32
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    // Positive tests model a root credential using our own live pidfd. Actual
    // SO_PEERCRED rejection is exercised separately on the private endpoint.
    fn root_model() -> PeerIdentity {
        PeerIdentity {
            pid: std::process::id(),
            uid: 0,
        }
    }
    fn state() -> Controls {
        Controls::new("a".repeat(32), "HEADLESS-1".into())
    }
    fn envelope(op: &str) -> Value {
        json!({"version":1,"op":op,"epoch":"a".repeat(32),"ui_generation":"1",
            "fence":{"broker":[1,2],"configuration":1,"session":1}})
    }
    fn lease(state: &mut Controls, at: u64, nonce: &str) -> Value {
        let mut r = envelope("lease");
        r["ui_generation"] = json!(state.generation.to_string());
        r["expires_ns"] = json!(((at + 1000) * 1_000_000).to_string());
        state
            .request(root_model(), &r, Duration::from_millis(at), nonce)
            .unwrap()
    }
    fn submit(state: &mut Controls, id: u64, at: u64) -> Value {
        let mut r = envelope("submit");
        r["ui_generation"] = json!(state.generation.to_string());
        r["id"] = json!(id.to_string());
        r["deadline_ns"] = json!(((at + 100) * 1_000_000).to_string());
        r["request"] = json!({"version":1,"output":"HEADLESS-1","epoch":"b".repeat(32),
            "gesture":format!("{id:032x}"),"ordinal":1,"phase":"discrete","action":"apps",
            "dx":0,"dy":0,"vx":0,"vy":0});
        state
            .request(root_model(), &r, Duration::from_millis(at), "unused")
            .unwrap()
    }

    #[test]
    fn model_queued_work_expires_and_receipts_do_not_reexecute() {
        let mut s = state();
        assert_eq!(lease(&mut s, 0, "first")["status"], "accepted");
        assert_eq!(submit(&mut s, 1, 0)["status"], "queued");
        assert!(s.take(Duration::from_millis(100)).is_none());
        assert_eq!(s.results[&1]["error"], "expired");
        assert_eq!(submit(&mut s, 1, 100)["error"], "duplicate");
        assert_eq!(submit(&mut s, 2, 100)["status"], "queued");
        let job = s.take(Duration::from_millis(101)).unwrap();
        s.complete(&job, true);
        assert_eq!(s.results[&2]["status"], "dispatched");
        assert!(s.take(Duration::from_millis(102)).is_none());
    }

    #[test]
    fn model_revoke_requires_a_new_fence_and_expiry_rotates_guards() {
        let mut s = state();
        lease(&mut s, 0, "first");
        submit(&mut s, 1, 0);
        assert_eq!(
            s.request(root_model(), &envelope("revoke"), Duration::ZERO, "unused")
                .unwrap()["status"],
            "accepted"
        );
        assert!(!s.valid("first", Duration::ZERO));
        assert_eq!(s.results[&1]["error"], "retired");
        assert_eq!(lease(&mut s, 1, "second")["error"], "retired");
        let mut r = envelope("lease");
        r["fence"]["session"] = json!(2);
        r["expires_ns"] = json!("1000000000");
        assert_eq!(
            s.request(root_model(), &r, Duration::ZERO, "second")
                .unwrap()["status"],
            "accepted"
        );
        assert!(!s.valid("first", Duration::ZERO));
        assert!(s.valid("second", Duration::ZERO));
        assert_eq!(lease(&mut s, 0, "third")["error"], "stale_fence");
        let mut s = state();
        lease(&mut s, 0, "first");
        submit(&mut s, 1, 0);
        assert_eq!(lease(&mut s, 1000, "second")["status"], "accepted");
        assert!(!s.valid("first", Duration::from_millis(1000)));
        assert!(s.valid("second", Duration::from_millis(1000)));
        assert!(s.take(Duration::from_millis(1000)).is_none());
        assert_eq!(submit(&mut s, 1, 1000)["error"], "duplicate");
    }

    #[test]
    fn model_ui_reload_retires_queue_and_guards_without_reviving_ids() {
        let mut s = state();
        lease(&mut s, 0, "first");
        submit(&mut s, 1, 0);
        s.reset_ui();
        assert!(!s.valid("first", Duration::ZERO));
        assert_eq!(s.results[&1]["error"], "ui_replaced");
        let mut r = envelope("lease");
        r["expires_ns"] = json!("1000000000");
        assert_eq!(
            s.request(root_model(), &r, Duration::ZERO, "second")
                .unwrap()["error"],
            "ui_replaced"
        );
        assert_eq!(lease(&mut s, 0, "second")["status"], "accepted");
        assert_eq!(submit(&mut s, 1, 0)["error"], "duplicate");
        assert_eq!(s.capabilities()["ui_generation"], "2");
        s.close();
        assert!(!s.valid("second", Duration::ZERO));
        assert!(
            s.request(root_model(), &r, Duration::ZERO, "third")
                .is_none()
        );
    }

    #[test]
    fn model_queue_and_receipt_memory_are_bounded() {
        let mut s = state();
        lease(&mut s, 0, "first");
        for id in 1..=8 {
            assert_eq!(submit(&mut s, id, 0)["status"], "queued");
        }
        for id in 9..=80 {
            assert_eq!(submit(&mut s, id, 0)["error"], "capacity");
        }
        assert_eq!(s.pending.len(), 8);
        assert_eq!(s.results.len(), 64);
        s.invalidate();
        assert!(s.pending.is_empty());
        assert_eq!(s.results.len(), 64);
        assert_eq!(submit(&mut s, 80, 0)["error"], "duplicate");
    }

    #[test]
    fn nonroot_malformed_cross_output_and_overlong_deadlines_are_refused() {
        let mut s = state();
        let mut r = envelope("lease");
        r["expires_ns"] = json!("1000000000");
        assert!(
            s.request(
                PeerIdentity {
                    pid: std::process::id(),
                    uid: 1000
                },
                &r,
                Duration::ZERO,
                "first"
            )
            .is_none()
        );
        r["expires_ns"] = json!("1000000001");
        assert_eq!(
            s.request(root_model(), &r, Duration::ZERO, "first")
                .unwrap()["error"],
            "invalid_deadline"
        );
        lease(&mut s, 0, "first");
        submit(&mut s, 1, 0);
        let mut payload: Value = serde_json::from_str(&s.pending[0].payload).unwrap();
        payload["output"] = json!("other");
        assert!(!valid_payload(&payload, "HEADLESS-1"));
        payload["output"] = json!("HEADLESS-1");
        payload["dx"] = json!(1_000_001);
        assert!(!valid_payload(&payload, "HEADLESS-1"));
        assert!(decimal(&json!("01")).is_none());
        assert!(decimal(&json!(1)).is_none());
    }

    #[test]
    fn model_issuer_death_retires_queued_work_without_waiting_for_expiry() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let peer = PeerIdentity {
            pid: child.id(),
            uid: 0,
        };
        let issuer = Issuer::open(peer).unwrap();
        let mut s = state();
        lease(&mut s, 0, "first");
        submit(&mut s, 1, 0);
        s.issuer = Some(issuer);
        assert!(s.valid("first", Duration::ZERO));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!s.valid("first", Duration::ZERO));
        assert!(s.take(Duration::ZERO).is_none());
        assert_eq!(s.results[&1]["error"], "expired");
    }

    #[test]
    fn model_handoff_orders_quiesce_activation_and_confirmed_release() {
        use std::os::unix::fs::DirBuilderExt;
        let path =
            std::env::temp_dir().join(format!("mhc-{}-{}", std::process::id(), nonce().unwrap()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        let gate = super::super::handoff::Gate::open_at(&path, "HEADLESS-1").unwrap();
        let mut s = Controls::with_handoff("a".repeat(32), "HEADLESS-1".into(), gate.clone());
        lease(&mut s, 0, "lease");
        assert_eq!(submit(&mut s, 1, 0)["error"], "handoff_required");
        let mut quiesce = envelope("handoff-quiesce");
        quiesce["ownership_generation"] = json!("1");
        assert!(
            s.request(
                PeerIdentity {
                    pid: std::process::id(),
                    uid: 1000
                },
                &quiesce,
                Duration::ZERO,
                &"a".repeat(32)
            )
            .is_none()
        );
        assert!(gate.enabled());
        assert_eq!(
            s.request(root_model(), &quiesce, Duration::ZERO, &"a".repeat(32))
                .unwrap()["status"],
            "accepted"
        );
        assert!(!gate.enabled());
        gate.persist_inhibition().unwrap();
        let mut activate = envelope("handoff-activate");
        activate["ownership_generation"] = json!("1");
        assert_eq!(
            s.request(root_model(), &activate, Duration::ZERO, "unused")
                .unwrap()["error"],
            "not_neutral"
        );
        s.handoff_update(
            &"a".repeat(32),
            super::super::handoff::Boundary {
                contacts: 0,
                neutral: true,
                revision: 2,
            },
            true,
            "native",
            Duration::from_millis(1),
        );
        assert_eq!(
            s.request(root_model(), &activate, Duration::from_millis(1), "unused")
                .unwrap()["status"],
            "accepted"
        );
        assert_eq!(submit(&mut s, 1, 1)["status"], "queued");
        let mut release = envelope("handoff-release");
        release["ownership_generation"] = json!("1");
        release["descriptors_released"] = json!(false);
        assert_eq!(
            s.request(root_model(), &release, Duration::from_millis(1), "unused")
                .unwrap()["error"],
            "release_unconfirmed"
        );
        release["descriptors_released"] = json!(true);
        assert_eq!(
            s.request(
                root_model(),
                &release,
                Duration::from_millis(1),
                &"b".repeat(32)
            )
            .unwrap()["status"],
            "accepted"
        );
        assert_eq!(s.results[&1]["error"], "retired");
        assert!(!s.valid("lease", Duration::from_millis(1)));
        assert!(!s.handoff_authorizes(&"a".repeat(32), true));
        s.handoff_update(
            &"b".repeat(32),
            super::super::handoff::Boundary {
                contacts: 1,
                neutral: false,
                revision: 3,
            },
            true,
            "native",
            Duration::from_millis(2),
        );
        assert!(!gate.enabled());
        s.handoff_update(
            &"b".repeat(32),
            super::super::handoff::Boundary {
                contacts: 0,
                neutral: true,
                revision: 4,
            },
            true,
            "native",
            Duration::from_millis(3),
        );
        let restore = s.handoff_take_restore().unwrap();
        s.handoff_prepared_restore(&restore, gate.remove_inhibition().is_ok());
        assert!(!gate.enabled());
        s.handoff_update(
            &"b".repeat(32),
            super::super::handoff::Boundary {
                contacts: 0,
                neutral: true,
                revision: 4,
            },
            true,
            "native",
            Duration::from_millis(3),
        );
        assert!(gate.enabled());
        assert_eq!(lease(&mut s, 3, "new")["error"], "retired");
        drop(s);
        drop(gate);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn model_native_queue_calls_the_real_vm_guard_and_reload_retires_it() {
        use std::os::{
            fd::AsFd,
            unix::{fs::DirBuilderExt, net::UnixStream},
        };
        let directory = std::path::PathBuf::from(format!("/tmp/mcg-{}", std::process::id()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let (connection, _) = UnixStream::pair().unwrap();
        let mut provider =
            super::super::Provider::bind_at(&directory, connection.as_fd(), "HEADLESS-1").unwrap();
        let mut runtime = morf_lua::Runtime::default();
        runtime
            .execute(
                "guard.lua",
                br#"
            local count=0
            morf.ipc["prek-native-control"]=function(payload,token)
                assert(__morf_prek_authorized(token))
                count=count+1
                return {status="accepted"}
            end
            morf.ipc.count=function() return count end
            morf.ipc["prek-native-handoff"]=function(token,enabled)
                assert(__morf_prek_handoff_authorized(token,enabled))
                return {ready=true,enabled=enabled,driver="native"}
            end
        "#,
            )
            .unwrap();
        assert!(!provider.dispatch(&mut runtime, 1));
        {
            let mut state = provider.controls.lock().unwrap();
            // Reuse the real queue and VM, with explicit modeled credentials.
            let at = now().unwrap().as_millis() as u64;
            let mut request = envelope("lease");
            request["epoch"] = json!(state.epoch);
            request["expires_ns"] = json!(((at + 1000) * 1_000_000).to_string());
            assert_eq!(
                state
                    .request(
                        root_model(),
                        &request,
                        Duration::from_millis(at),
                        &"c".repeat(32)
                    )
                    .unwrap()["status"],
                "accepted"
            );
            // The helper has a fixed model epoch; use a temporary matching state
            let handoff = state.handoff.as_mut().unwrap();
            handoff
                .request(
                    "handoff-quiesce",
                    1,
                    false,
                    &"d".repeat(32),
                    Duration::from_millis(at),
                )
                .unwrap();
            provider.gate.persist_inhibition().unwrap();
            handoff.update(
                super::super::handoff::Boundary {
                    contacts: 0,
                    neutral: true,
                    revision: 1,
                },
                true,
                "native",
                Duration::from_millis(at),
            );
            handoff
                .request(
                    "handoff-activate",
                    1,
                    false,
                    "unused",
                    Duration::from_millis(at),
                )
                .unwrap();
            // to produce an otherwise valid typed payload for the real queue.
            let mut fixture = super::tests::state();
            lease(&mut fixture, at, "fixture");
            submit(&mut fixture, 1, at);
            let mut request = envelope("submit");
            request["epoch"] = json!(state.epoch);
            request["id"] = json!("1");
            request["deadline_ns"] = json!(((at + 250) * 1_000_000).to_string());
            request["request"] =
                serde_json::from_str::<Value>(&fixture.pending[0].payload).unwrap();
            assert_eq!(
                state
                    .request(root_model(), &request, Duration::from_millis(at), "unused")
                    .unwrap()["status"],
                "queued"
            );
        }
        assert!(provider.dispatch(&mut runtime, 1));
        assert_eq!(runtime.call_ipc("count", &[]).unwrap()[0].to_json(), 1);
        assert_eq!(
            provider.controls.lock().unwrap().results[&1]["status"],
            "dispatched"
        );
        provider.dispatch(&mut runtime, 2);
        assert!(
            !provider
                .controls
                .lock()
                .unwrap()
                .valid(&"c".repeat(32), now().unwrap())
        );
        assert_eq!(runtime.call_ipc("count", &[]).unwrap()[0].to_json(), 1);
        drop(provider);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
