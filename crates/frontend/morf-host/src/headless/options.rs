//! What a headless load takes: the configuration, the screens it sees,
//! and how its imported modules are held.

use std::path::PathBuf;

use crate::process::supervisor::LoadPolicy;

/// What to load, and onto what.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// The configuration's path; its folder is where `require` looks.
    pub path: PathBuf,
    /// The source to run instead of reading `path`, for a configuration
    /// written inline in a test.
    pub source: Option<Vec<u8>>,
    /// Every screen's logical size.
    pub size: (u32, u32),
    /// How many screens `morf.screens` lists.
    pub screens: usize,
    /// Which of them this runtime draws to, from zero.
    pub screen_index: usize,
    /// The integer scale screens report.
    pub scale: i32,
    /// The configuration's own arguments, what follows `--`.
    pub args: Vec<String>,
    pub policy: LoadPolicy,
    /// Lua run before the configuration, in the same runtime.
    pub prelude: Option<String>,
    /// Paint with the GPU, offscreen, rather than only lay out.
    pub gpu: bool,
    /// How imported modules are held back while they load.
    pub permissions: morf_lua::PermissionMode,
    /// Module roots searched after the configuration's folder.
    pub runtime_path: Vec<PathBuf>,
    /// Where approvals are read from, instead of the shell's state folder.
    pub approvals: Option<PathBuf>,
}

impl LoadOptions {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            source: None,
            size: (1920, 1080),
            screens: 1,
            screen_index: 0,
            scale: 1,
            args: Vec::new(),
            policy: LoadPolicy::default(),
            prelude: None,
            gpu: false,
            permissions: morf_lua::PermissionMode::default(),
            runtime_path: Vec::new(),
            approvals: None,
        }
    }
}
