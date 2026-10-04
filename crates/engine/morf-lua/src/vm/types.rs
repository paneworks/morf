use luna::Lua;
use std::cell::RefCell;
use std::error::Error as StdError;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::*;
pub use morf_runtime::log::{LogEntry, LogLevel};
pub use morf_runtime::reactive::ResourceStats;
pub use morf_runtime::screens::Screen;
pub(crate) use morf_runtime::screens::{
    density as screen_density, orientation as screen_orientation,
    primary_orientation as screen_primary_orientation,
};

/// Execution limits applied independently to each loaded chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum VM fuel a chunk may consume.
    pub fuel: u64,
    /// Maximum bytes owned by the Lua state.
    pub memory: usize,
    /// VM fuel granted before the host regains control.
    pub slice_fuel: i32,
    /// Maximum VM fuel granted to one reactive Lua effect.
    pub effect_fuel: u64,
    /// Maximum VM fuel granted to all effects in one recompute pass.
    pub frame_fuel: u64,
    /// Maximum VM fuel granted to one view delegate. A delegate builds a
    /// part of a view -- a row, or a whole panel or board shown on demand --
    /// which is loading's kind of work rather than a handler's, so it gets
    /// a budget nearer loading's: a board of widgets each drawing a face
    /// took most of a handler's million instructions.
    pub delegate_fuel: u64,
    /// Maximum VM fuel granted to loading one module (`require`). A module
    /// builds what it defines -- a whole panel's nodes, a table of
    /// outlines -- which is loading's kind of work, not a handler's.
    pub module_fuel: u64,
    /// Most `ui.Terminal` nodes that may exist at once: each is a program
    /// on a pseudo-terminal and a screen with its history.
    pub terminals: usize,
    /// Most `morf.fs.watch` watches that may be open at once.
    pub watches: usize,
}

impl Default for Limits {
    /// Sized for a whole desktop -- a bar, a desk of widgets, a palette
    /// derived from a painting in one handler -- while a loop that never
    /// ends is still stopped within a frame or two.
    fn default() -> Self {
        Self {
            fuel: 50_000_000,
            memory: 256 * 1024 * 1024,
            slice_fuel: 4_096,
            effect_fuel: 1_000_000,
            frame_fuel: 8_000_000,
            delegate_fuel: 20_000_000,
            module_fuel: 20_000_000,
            terminals: 16,
            watches: 256,
        }
    }
}

impl Limits {
    /// The defaults with any of `MORF_LIMITS` applied: comma-separated
    /// `load=N`, `memory=N` (bytes, or with a `k`/`m`/`g` suffix),
    /// `handler=N`, `frame=N`, `delegate=N` and `module=N` (VM instructions), `terminals=N` and
    /// `watches=N`. An
    /// entry that does not parse is ignored and named in the returned
    /// warnings.
    pub fn from_env() -> (Self, Vec<String>) {
        let mut limits = Self::default();
        let mut warnings = Vec::new();
        let Ok(text) = std::env::var("MORF_LIMITS") else {
            return (limits, warnings);
        };
        for entry in text
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            let Some((key, value)) = entry.split_once('=') else {
                warnings.push(format!("MORF_LIMITS entry `{entry}` is not key=value"));
                continue;
            };
            let value = value.trim().to_ascii_lowercase();
            let (digits, scale) = match value.chars().last() {
                Some('k') => (&value[..value.len() - 1], 1024u64),
                Some('m') => (&value[..value.len() - 1], 1024 * 1024),
                Some('g') => (&value[..value.len() - 1], 1024 * 1024 * 1024),
                _ => (value.as_str(), 1),
            };
            let Some(number) = digits
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(scale))
                .filter(|n| *n > 0)
            else {
                warnings.push(format!(
                    "MORF_LIMITS value `{value}` for {key} is not a positive number"
                ));
                continue;
            };
            match key.trim() {
                "load" => limits.fuel = number,
                "memory" => limits.memory = usize::try_from(number).unwrap_or(usize::MAX),
                "handler" => limits.effect_fuel = number,
                "frame" => limits.frame_fuel = number,
                "delegate" => limits.delegate_fuel = number,
                "module" => limits.module_fuel = number,
                "terminals" => limits.terminals = usize::try_from(number).unwrap_or(usize::MAX),
                "watches" => limits.watches = usize::try_from(number).unwrap_or(usize::MAX),
                other => warnings.push(format!(
                    "MORF_LIMITS key `{other}` is not load, memory, handler, frame, delegate, terminals or watches"
                )),
            }
        }
        (limits, warnings)
    }
}

/// A configuration execution failure.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// The source could not be compiled.
    Load(String),
    /// Execution stopped with a Lua error.
    Runtime(String),
    /// Execution exceeded its instruction budget.
    FuelExhausted { budget: u64 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(message) => write!(f, "could not load Lua: {message}"),
            Self::Runtime(message) => write!(f, "Lua error: {message}"),
            Self::FuelExhausted { budget } => {
                write!(f, "Lua fuel exhausted after {budget} instructions")
            }
        }
    }
}

impl StdError for Error {}

/// The Luna VM owned behind morf's stable runtime boundary.
pub struct Runtime {
    pub(crate) lua: Lua,
    pub(crate) limits: Limits,
    pub(crate) reactive: Rc<RefCell<ReactiveState>>,
    pub(crate) module_roots: Rc<RefCell<Vec<PathBuf>>>,
    /// Lint findings waiting to be logged.
    ///
    /// Its own cell rather than a push straight into the log, because the
    /// lint runs while whoever laid the scene out still holds it borrowed --
    /// and the log lives behind the same borrow. Drained on the next poll.
    pub(crate) lint_queue: RefCell<Vec<(morf_scene::NodeHandle, String, usize)>>,
}

pub use morf_runtime::requests::{ToplevelRequest, WorkspaceRequest};

#[derive(Clone, Copy)]
pub(crate) enum StorageKind {
    Data,
    State,
    Cache,
}

pub(crate) fn shell_storage_dir(shell_root: &Path, kind: StorageKind) -> Result<PathBuf, String> {
    let (variable, fallback) = match kind {
        StorageKind::Data => ("XDG_DATA_HOME", ".local/share"),
        StorageKind::State => ("XDG_STATE_HOME", ".local/state"),
        StorageKind::Cache => ("XDG_CACHE_HOME", ".cache"),
    };
    let base = std::env::var_os(variable)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .ok_or_else(|| format!("{variable} and HOME are unset"))?;
    Ok(base.join("morf").join(shell_storage_key(shell_root)))
}

pub(crate) fn shell_storage_key(shell_root: &Path) -> String {
    let name = shell_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("shell")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let hash = shell_root
        .to_string_lossy()
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("{name}-{hash:016x}")
}

pub(crate) fn launch_time_ms() -> u64 {
    static LAUNCH_TIME: OnceLock<u64> = OnceLock::new();
    *LAUNCH_TIME.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    })
}

pub(crate) fn rooted_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.len() > 4_096 || relative.as_bytes().contains(&0) {
        return Err("relative path is invalid".to_owned());
    }
    let relative = Path::new(relative);
    if relative.is_absolute() {
        return Err("relative path must not be absolute".to_owned());
    }
    Ok(root.join(relative))
}

pub(crate) fn icon_lookup_options(
    name: &str,
    theme: Option<String>,
    size: Option<i64>,
) -> Result<(String, u32), String> {
    if name.is_empty() || name.len() > 512 || name.as_bytes().contains(&0) {
        return Err("icon name is invalid".to_owned());
    }
    let theme = theme
        .or_else(|| std::env::var("MORF_ICON_THEME").ok())
        .unwrap_or_else(|| "hicolor".to_owned());
    if theme.is_empty() || theme.len() > 128 || theme.as_bytes().contains(&0) {
        return Err("icon theme is invalid".to_owned());
    }
    let size = u32::try_from(size.unwrap_or(32))
        .ok()
        .filter(|size| (1..=1_024).contains(size))
        .ok_or_else(|| "icon size must be 1..1024".to_owned())?;
    Ok((theme, size))
}
