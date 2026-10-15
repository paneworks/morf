//! Native half of the global-touch recognizer handoff. A persistent marker
//! inhibits restart; only an authenticated release may remove that inhibition.
use morf_app::{Event, WindowId};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(crate) struct Gate {
    enabled: AtomicBool,
    durable: AtomicBool,
    marker: Mutex<Marker>,
}
struct Marker {
    path: PathBuf,
    identity: Option<(u64, u64)>,
}
impl Gate {
    pub fn open_at(runtime: &Path, output: &str) -> io::Result<Arc<Self>> {
        if output.is_empty() || output.len() > 256 || output.chars().any(char::is_control) {
            return Err(io::Error::other("invalid handoff output"));
        }
        // SAFETY: read-only process credential query.
        let owner = unsafe { libc::geteuid() };
        let check = |path: &Path, private: bool| -> io::Result<()> {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_dir()
                || metadata.uid() != owner
                || metadata.mode() & if private { 0o077 } else { 0o022 } != 0
            {
                return Err(io::Error::other(
                    "handoff directory is not owned and protected",
                ));
            }
            Ok(())
        };
        check(runtime, true)?;
        let mut directory = runtime.to_owned();
        // Hex chunks avoid traversal, output-name aliases and NAME_MAX limits.
        let encoded: String = output.bytes().map(|b| format!("{b:02x}")).collect();
        let names = std::iter::once("morf".to_owned())
            .chain(std::iter::once("handoff".into()))
            .chain(
                encoded
                    .as_bytes()
                    .chunks(120)
                    .map(|chunk| format!("o-{}", std::str::from_utf8(chunk).unwrap())),
            );
        for (index, name) in names.enumerate() {
            directory.push(name);
            match fs::DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            check(&directory, index != 0)?;
        }
        let path = directory.join("inhibit");
        let identity = match fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.is_file()
                    && metadata.uid() == owner
                    && metadata.mode() & 0o077 == 0
                    && metadata.nlink() == 1 =>
            {
                Some((metadata.dev(), metadata.ino()))
            }
            Ok(_) => return Err(io::Error::other("invalid handoff inhibition marker")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        Ok(Arc::new(Self {
            enabled: AtomicBool::new(identity.is_none()),
            durable: AtomicBool::new(identity.is_some()),
            marker: Mutex::new(Marker { path, identity }),
        }))
    }
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
    fn inhibit(&self) {
        self.enabled.store(false, Ordering::Release);
        self.durable.store(false, Ordering::Release);
    }
    pub fn persist_inhibition(&self) -> io::Result<()> {
        let mut marker = self.marker.lock().unwrap();
        if marker.identity.is_none() {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&marker.path)?;
            let metadata = file.metadata()?;
            marker.identity = Some((metadata.dev(), metadata.ino()));
            file.write_all(b"native recognizers inhibited\n")?;
            file.sync_all()?;
            File::open(marker.path.parent().unwrap())?.sync_all()?;
        } else {
            let metadata = fs::symlink_metadata(&marker.path)?;
            if !metadata.is_file() || Some((metadata.dev(), metadata.ino())) != marker.identity {
                return Err(io::Error::other("handoff marker was replaced"));
            }
        }
        self.durable.store(true, Ordering::Release);
        Ok(())
    }
    pub fn remove_inhibition(&self) -> io::Result<()> {
        let mut marker = self.marker.lock().unwrap();
        if marker.identity.is_none() && !marker.path.try_exists()? {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(&marker.path)?;
        if !metadata.is_file() || Some((metadata.dev(), metadata.ino())) != marker.identity {
            return Err(io::Error::other("handoff marker was replaced"));
        }
        fs::remove_file(&marker.path)?;
        File::open(marker.path.parent().unwrap())?.sync_all()?;
        marker.identity = None;
        self.durable.store(false, Ordering::Release);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Boundary {
    pub contacts: u32,
    pub neutral: bool,
    pub revision: u64,
}
#[derive(Default)]
pub(crate) struct Contacts {
    active: HashMap<i32, WindowId>,
    tainted: bool,
    exhausted: bool,
    revision: u64,
}
impl Contacts {
    pub fn observe(&mut self, event: &Event) {
        match event {
            Event::TouchDown { id, surface, .. } => {
                if self.active.len() >= 64 || self.active.insert(*id, *surface).is_some() {
                    self.tainted = true;
                }
            }
            Event::TouchUp { id, .. } => {
                if self.active.remove(id).is_none() {
                    self.tainted = true;
                }
            }
            Event::TouchMotion { id, surface, .. } => {
                if self.active.get(id) == Some(surface) {
                    return;
                }
                self.tainted = true;
            }
            Event::TouchCancel => {
                self.active.clear();
                self.tainted = false;
            }
            Event::Closed { id } => {
                self.active
                    .retain(|_, surface| *surface != WindowId::Layer(*id));
            }
            _ => return,
        }
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
        } else {
            self.exhausted = true;
        }
    }
    pub fn snapshot(&self) -> Boundary {
        Boundary {
            contacts: self.active.len() as u32,
            neutral: self.active.is_empty() && !self.tainted && !self.exhausted,
            revision: self.revision,
        }
    }
    pub fn new_connection(&mut self) {
        self.active.clear();
        self.tainted = false;
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
        } else {
            self.exhausted = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Native,
    Quiescing,
    Quiesced,
    Owned,
    Releasing,
    Lost,
}
pub(super) struct State {
    gate: Arc<Gate>,
    phase: Phase,
    generation: u64,
    token: String,
    boundary: Boundary,
    observed: Duration,
    ui_ack: bool,
    driver: String,
    restore_ready: bool,
    restore_started: bool,
    restore_prepared: bool,
}
impl State {
    pub fn permits_controls(&self) -> bool {
        self.phase == Phase::Owned
    }
    pub fn new(gate: Arc<Gate>) -> Self {
        let phase = if gate.enabled() {
            Phase::Native
        } else {
            Phase::Lost
        };
        Self {
            gate,
            phase,
            generation: 0,
            token: String::new(),
            boundary: Boundary {
                contacts: 0,
                neutral: false,
                revision: 0,
            },
            observed: Duration::ZERO,
            ui_ack: false,
            driver: String::new(),
            restore_ready: false,
            restore_started: false,
            restore_prepared: false,
        }
    }
    pub fn lost(&mut self) {
        if self.phase != Phase::Native {
            self.phase = Phase::Lost;
            self.ui_ack = false;
            self.restore_ready = false;
            self.restore_started = false;
            self.restore_prepared = false;
        }
    }
    pub fn synchronize(&mut self, lease_valid: bool) {
        if !lease_valid && !matches!(self.phase, Phase::Native | Phase::Releasing) {
            self.lost();
        }
    }
    pub fn request(
        &mut self,
        op: &str,
        generation: u64,
        released: bool,
        token: &str,
        at: Duration,
    ) -> Result<(), &'static str> {
        if op == "handoff-quiesce" {
            if generation == 0 || generation <= self.generation {
                return Err("stale_ownership");
            }
            if !matches!(self.phase, Phase::Native | Phase::Lost) {
                return Err("busy");
            }
            self.generation = generation;
            self.token = token.to_owned();
            self.ui_ack = false;
            self.phase = Phase::Lost;
            self.gate.inhibit();
            self.phase = Phase::Quiescing;
            return Ok(());
        }
        if generation != self.generation || generation == 0 {
            return Err("stale_ownership");
        }
        if op == "handoff-activate" {
            if self.phase != Phase::Quiesced
                || !self.gate.durable.load(Ordering::Acquire)
                || !self.ui_ack
                || !self.boundary.neutral
                || at < self.observed
                || at - self.observed >= Duration::from_millis(250)
            {
                return Err("not_neutral");
            }
            self.phase = Phase::Owned;
            return Ok(());
        }
        if op == "handoff-release" {
            if self.phase == Phase::Native || !released {
                return Err("release_unconfirmed");
            }
            self.phase = Phase::Releasing;
            self.token = token.to_owned();
            self.ui_ack = false;
            return Ok(());
        }
        Err("invalid_operation")
    }
    pub fn work(&self) -> Option<(String, bool)> {
        (self.phase != Phase::Native && self.token.len() == 32)
            .then(|| (self.token.clone(), self.phase == Phase::Releasing))
    }
    pub fn observe(&mut self, boundary: Boundary, at: Duration) {
        self.boundary = boundary;
        self.observed = at;
        if self.phase == Phase::Quiesced && !boundary.neutral {
            self.phase = Phase::Quiescing;
        }
    }
    pub fn authorizes(&self, token: &str, enabled: bool) -> bool {
        token.len() == 32
            && token == self.token
            && self.phase != Phase::Native
            && enabled == (self.phase == Phase::Releasing)
    }
    pub fn update(&mut self, boundary: Boundary, ack: bool, driver: &str, at: Duration) -> bool {
        let previous = (
            self.phase,
            self.ui_ack,
            self.gate.enabled(),
            self.driver.clone(),
            self.restore_ready,
        );
        self.boundary = boundary;
        self.observed = at;
        self.ui_ack = ack;
        self.driver = driver.to_owned();
        if matches!(self.phase, Phase::Quiescing | Phase::Quiesced) {
            self.phase = if ack && boundary.neutral && self.gate.durable.load(Ordering::Acquire) {
                Phase::Quiesced
            } else {
                Phase::Quiescing
            };
        } else if self.phase == Phase::Owned && !ack {
            self.lost();
        } else if self.phase == Phase::Releasing && ack && boundary.neutral {
            if self.restore_prepared {
                self.phase = Phase::Native;
                self.token.clear();
                self.restore_ready = false;
                self.restore_started = false;
                self.restore_prepared = false;
                self.gate.enabled.store(true, Ordering::Release);
            } else {
                self.restore_ready = true;
            }
        } else if self.phase == Phase::Releasing {
            self.restore_ready = false;
        }
        previous
            != (
                self.phase,
                self.ui_ack,
                self.gate.enabled(),
                self.driver.clone(),
                self.restore_ready,
            )
    }
    pub fn restore_ready(&self) -> bool {
        self.phase == Phase::Releasing && self.restore_ready && !self.restore_started
    }
    pub fn take_restore(&mut self) -> Option<String> {
        if !self.restore_ready() {
            return None;
        }
        self.restore_started = true;
        Some(self.token.clone())
    }
    pub fn prepared_restore(&mut self, token: &str, success: bool) {
        if self.phase != Phase::Releasing || self.token != token {
            return;
        }
        if success {
            self.restore_prepared = true;
        } else {
            self.lost();
        }
    }
    pub fn status(&self) -> Value {
        let phase = match self.phase {
            Phase::Native => "native",
            Phase::Quiescing => "quiescing",
            Phase::Quiesced => "quiesced",
            Phase::Owned => "owned",
            Phase::Releasing => "releasing",
            Phase::Lost => "lost",
        };
        json!({"phase":phase,"ownership_generation":self.generation.to_string(),"global_enabled":self.gate.enabled(),
            "native_neutral":self.boundary.neutral,"contacts":self.boundary.contacts,
            "contact_revision":self.boundary.revision.to_string(),"ui_ack":self.ui_ack,"driver":self.driver,
            "observed_ns":self.observed.as_nanos().to_string(),
            "scope":"global_phone_touch","recognizer_ownership":false})
    }
}

#[cfg(test)]
mod tests;
