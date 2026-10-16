//! Root-issued, expiring native controller work. No Lua values cross threads.
use crate::process::native::PeerIdentity;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Duration,
};

mod checks;
use checks::{Fence, Issuer, decimal, effect_token, gesture_key, valid_payload};

const QUEUE: usize = 8;
const RECEIPTS: usize = 64;
const GESTURES: usize = 4096;
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
    result_keys: BTreeMap<u64, String>,
    cancelled: BTreeSet<String>,
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
            result_keys: BTreeMap::new(),
            cancelled: BTreeSet::new(),
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
        let (base, key) =
            effect_token(token).map_or((token, None), |(base, key)| (base, Some(key)));
        self.valid(base, at)
            && key.is_none_or(|key| !self.cancelled.contains(key))
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
        json!({"version":1,"epoch":self.epoch,"ui_generation":self.generation.to_string(),"root_only":true,"queue":QUEUE,"receipts":RECEIPTS,"lease_ms":1000,"grant_ms":250,"recognizer_ownership":false,"cancel_gesture":true})
    }
    fn answer(&self, status: &str, error: Option<&str>) -> Value {
        json!({"version":1,"epoch":self.epoch,"ui_generation":self.generation.to_string(),"status":status,"error":error})
    }
    fn result(&mut self, id: u64, status: &str, error: Option<&str>) {
        if self.results.len() >= RECEIPTS
            && !self.results.contains_key(&id)
            && let Some((old, _)) = self.results.pop_first()
        {
            self.result_keys.remove(&old);
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
                self.result_keys.clear();
                self.cancelled.clear();
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
        if r["op"] == "cancel-gesture" && fields == 7 {
            let key = gesture_key(&r["gesture_epoch"], &r["gesture"])?;
            if self.cancelled.len() >= GESTURES && !self.cancelled.contains(&key) {
                return refused(self, "capacity");
            }
            self.cancelled.insert(key.clone());
            self.pending
                .retain(|job| effect_token(&job.token).is_none_or(|(_, job_key)| job_key != key));
            let pending = self
                .result_keys
                .iter()
                .filter(|(id, job_key)| {
                    **job_key == key
                        && self
                            .results
                            .get(id)
                            .is_some_and(|result| result["status"] == "queued")
                })
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            for id in pending {
                self.result(id, "rejected", Some("retired"));
            }
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
        let key = gesture_key(&payload["epoch"], &payload["gesture"])?;
        if self.cancelled.contains(&key) {
            return refused(self, "retired");
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
            token: format!("{}:{key}", self.token),
            payload: payload.to_string(),
            deadline,
        });
        self.result_keys.insert(id, key);
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
        let (base, key) = effect_token(&job.token)
            .map_or((job.token.as_str(), None), |(base, key)| (base, Some(key)));
        if base == self.token && key.is_none_or(|key| !self.cancelled.contains(key)) {
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

#[cfg(test)]
mod tests;
