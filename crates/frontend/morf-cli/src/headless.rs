//! A configuration run with no compositor: what `morf check`, `morf render`
//! and `morf test` share.
//!
//! The shell's loop is a compositor's frame callbacks, its configure events
//! and its input. Here all three are stood in for: a surface is as big as its
//! `morf.surface` settings make it on a screen of a given size, a frame is a
//! call, and a click is a `Event` handed to the same pointer path the
//! shell uses. Nothing connects to Wayland -- there is no client to connect
//! with -- and time is a virtual clock that moves only when asked, so the
//! same run fires the same timers every time.

use std::path::{Path, PathBuf};
use std::time::Duration;

use morf_layout::{Layout, Size};
use morf_lua::{Limits, LogEntry, LogLevel, Runtime};
use morf_scene::NodeHandle;
use morf_text::TextSystem;
use morf_app::WindowId;

use crate::config::LoadPolicy;
use crate::headless_surfaces::headless_screens;
use crate::supervisor::{execute_config_on, lua_screen, lua_screens, store_outputs};
use crate::surfaces::{PointerInput, primary_surface_root};

/// One frame of a 60 Hz output, which is what time advances by between
/// animation ticks.
pub(crate) const FRAME: Duration = Duration::from_millis(16);

/// Layout passes a frame allows the bindings that read the layout.
const SETTLE_PASSES: usize = 8;

/// What to load, and onto what.
#[derive(Clone, Debug)]
pub(crate) struct LoadOptions {
    /// The configuration's path; its folder is where `require` looks.
    pub(crate) path: PathBuf,
    /// The source to run instead of reading `path`, for a configuration
    /// written inline in a test.
    pub(crate) source: Option<Vec<u8>>,
    /// Every screen's logical size.
    pub(crate) size: (u32, u32),
    /// How many screens `morf.screens` lists.
    pub(crate) screens: usize,
    /// Which of them this runtime draws to, from zero.
    pub(crate) screen_index: usize,
    /// The integer scale screens report.
    pub(crate) scale: i32,
    /// The configuration's own arguments, what follows `--`.
    pub(crate) args: Vec<String>,
    pub(crate) policy: LoadPolicy,
    /// Lua run before the configuration, in the same runtime.
    pub(crate) prelude: Option<String>,
}

impl LoadOptions {
    pub(crate) fn new(path: PathBuf) -> Self {
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
        }
    }
}

/// A configuration that did not load, and what it said before it stopped.
#[derive(Debug)]
pub(crate) struct LoadFailure {
    pub(crate) error: String,
    pub(crate) logs: Vec<LogEntry>,
}

/// One surface the configuration asked for.
pub(crate) struct Surface {
    pub(crate) role: WindowId,
    /// `primary`, `popup`, `floating` or `layer`.
    pub(crate) kind: &'static str,
    /// Its namespace, or a floating window's title.
    pub(crate) name: String,
    /// The window surface's id; nothing for the primary.
    pub(crate) id: Option<u64>,
    pub(crate) root: NodeHandle,
    /// Logical size.
    pub(crate) size: (u32, u32),
    /// Where a layer surface sits on its screen; a popup's anchor point;
    /// nothing useful for a floating window, which the compositor places.
    pub(crate) position: (i32, i32),
    pub(crate) visible: bool,
    /// Where a layer-shell compositor stacks it: 0 background, 1 bottom,
    /// 2 top, 3 overlay. `screen` composes surfaces in this order.
    pub(crate) stack: u8,
    pub(crate) blend: String,
    pub(crate) layout: Option<Layout>,
    /// The tree revision and size `layout` was computed for.
    pub(crate) laid: Option<(u64, (u32, u32))>,
    /// Whether the last layout's bindings agreed with it.
    pub(crate) stable: bool,
    /// How a person names it, unique among the surfaces.
    pub(crate) label: String,
}

impl Surface {
    /// How a person names it: `primary`, `popup:3`, `layer:impasto-desk`,
    /// with `#id` after it when two surfaces would otherwise share it.
    pub(crate) fn label(&self) -> String {
        self.label.clone()
    }

    /// The label before it is made unique.
    pub(crate) fn plain_label(&self) -> String {
        match (self.kind, self.id) {
            ("primary", _) => "primary".to_owned(),
            (kind, Some(id)) if self.name.is_empty() => format!("{kind}:{id}"),
            (kind, _) => format!("{kind}:{}", self.name),
        }
    }

    /// Whether `wanted` -- an index, `primary`, a kind, a name, or a label --
    /// names this surface.
    pub(crate) fn is_named(&self, wanted: &str) -> bool {
        wanted == self.label
            || wanted == self.plain_label()
            || wanted == self.name
            || wanted == self.kind
            || self
                .id
                .is_some_and(|id| wanted == format!("{}:{id}", self.kind))
    }
}

/// A configuration running with nothing under it.
pub(crate) struct Headless {
    pub(crate) runtime: Runtime,
    pub(crate) text: TextSystem,
    pub(crate) screen: (u32, u32),
    pub(crate) surfaces: Vec<Surface>,
    pub(crate) input: PointerInput,
    /// Every log line since load, in order.
    pub(crate) logs: Vec<LogEntry>,
    /// Problems the runner itself found: layouts that failed, surfaces that
    /// could not be worked out.
    pub(crate) problems: Vec<String>,
    /// The virtual clock's reading, and when the last frame was.
    now: Duration,
    last_frame: Duration,
    /// Where the pointer is, for a wheel or a release with no position.
    pub(crate) pointer: Option<(WindowId, f64, f64)>,
    /// The surface a button was last pressed on: the one a compositor gives
    /// the keyboard to, and where keys go when a test names no surface.
    pub(crate) keyboard: Option<WindowId>,
    /// Loaded with no screen: nothing it declares is mapped.
    pub(crate) outputless: bool,
}

impl Headless {
    /// Loads a configuration onto one headless screen, on a virtual clock.
    pub(crate) fn load(options: &LoadOptions) -> Result<Self, LoadFailure> {
        let failure = |error: String| LoadFailure {
            error,
            logs: Vec::new(),
        };
        let screens = headless_screens(options.screens, options.size, options.scale);
        // No screen at all is the shell with every output gone: the
        // configuration runs as the outputless runtime does (outputless.rs).
        let outputless = options.screens == 0;
        let own = match outputless {
            true => None,
            false => Some(screens.get(options.screen_index).cloned().ok_or_else(|| {
                failure(format!("there is no screen {}", options.screen_index + 1))
            })?),
        };
        // `execute_config` gives `morf.screens` the recorded outputs, the way
        // a worker is given the compositor's.
        store_outputs(&screens);
        let (limits, warnings) = Limits::from_env();
        let mut runtime = match &own {
            Some(own) => Runtime::for_screen(limits, lua_screen(own)),
            None => Runtime::new(limits),
        };
        runtime.use_virtual_clock();
        runtime.set_arguments(options.args.clone());
        let mut capabilities = vec![("headless".to_owned(), "true".to_owned())];
        if outputless {
            capabilities.push(("outputless".to_owned(), "true".to_owned()));
        }
        runtime.set_capabilities(&capabilities);
        for warning in warnings {
            runtime.warn(warning);
        }
        if let Some(prelude) = &options.prelude {
            runtime
                .execute("=morf-headless", prelude.as_bytes())
                .map_err(|error| failure(format!("headless prelude: {error}")))?;
        }
        let source = match &options.source {
            Some(source) => source.clone(),
            None => std::fs::read(&options.path).map_err(|error| {
                failure(format!(
                    "could not read {}: {error}",
                    options.path.display()
                ))
            })?,
        };
        if let Err(error) = execute_config_on(
            &mut runtime,
            &options.path,
            &source,
            options.policy,
            // Its own list rather than the recorded one another run may be
            // writing at the same moment.
            &lua_screens(&screens),
        ) {
            return Err(LoadFailure {
                error,
                logs: runtime.take_logs(),
            });
        }
        if outputless && !runtime.layer_surface_config().outputless {
            return Err(LoadFailure {
                error: "the configuration does not run without an output \
                        (morf.surface.outputless is not set)"
                    .to_owned(),
                logs: runtime.take_logs(),
            });
        }
        // The shell hands a configuration the time of day once a second.
        let _ = runtime.update_clock(crate::paint::clock_text());
        let mut headless = Self {
            runtime,
            text: TextSystem::new(),
            screen: options.size,
            surfaces: Vec::new(),
            input: PointerInput::default(),
            logs: Vec::new(),
            problems: Vec::new(),
            now: Duration::ZERO,
            last_frame: Duration::ZERO,
            pointer: None,
            keyboard: None,
            outputless,
        };
        // The first frame, at time zero: what the shell draws before any
        // time has passed.
        headless.frame(Duration::ZERO);
        Ok(headless)
    }

    /// The virtual time since load.
    pub(crate) fn now(&self) -> Duration {
        self.now
    }

    /// Lays out every surface at its size, visible or not: a hidden panel is
    /// laid out too, so its problems are found before it is opened.
    pub(crate) fn layout_all(&mut self) {
        self.refresh_surfaces();
        for index in 0..self.surfaces.len() {
            let (root, size, label) = {
                let surface = &self.surfaces[index];
                (surface.root, surface.size, surface.label())
            };
            let available = Size {
                width: f64::from(size.0),
                height: f64::from(size.1),
            };
            // As the shell's cache: a tree whose revision has not moved, at
            // the size it was laid out at, lays out the same again.
            let revision = self.runtime.scene().layout_revision_of(root);
            if self.surfaces[index].layout.is_some()
                && self.surfaces[index].laid == Some((revision, size))
            {
                if let Some(layout) = &self.surfaces[index].layout {
                    self.runtime.sync_text_inputs(layout, &mut self.text);
                    self.runtime.observe_stretch(layout);
                }
                continue;
            }
            match self
                .runtime
                .settle_layout(root, available, &mut self.text, SETTLE_PASSES)
            {
                Ok(settled) => {
                    self.runtime.lint_layout(&settled.layout, root);
                    self.runtime
                        .sync_text_inputs(&settled.layout, &mut self.text);
                    self.runtime.observe_stretch(&settled.layout);
                    let revision = self.runtime.scene().layout_revision_of(root);
                    let surface = &mut self.surfaces[index];
                    surface.stable = settled.stable;
                    surface.layout = Some(settled.layout);
                    surface.laid = Some((revision, size));
                }
                Err(error) => {
                    let problem = format!("{label}: layout: {error}");
                    if !self.problems.contains(&problem) {
                        self.problems.push(problem);
                    }
                }
            }
        }
    }

    /// One frame: services and timers, `delta` of animation, and a layout.
    pub(crate) fn frame(&mut self, delta: Duration) {
        self.runtime.poll_services();
        if let Err(error) = self.runtime.tick_animations(delta) {
            self.problems.push(format!("animation: {error}"));
        }
        self.apply_transitions();
        self.runtime.take_window_surface_change();
        self.runtime.take_layer_surface_change();
        self.layout_all();
        // A node first asked for its `contains_pointer` is answered where
        // the pointer is, now that it is laid out; if that changed what a
        // binding drew, the surfaces are laid out again.
        let layouts = crate::headless_input::Layouts(&self.surfaces);
        if crate::surface_pointer::answer_new_containment(&mut self.runtime, &self.input, &layouts)
        {
            self.layout_all();
        }
        // Twice: the poll is what turns a layout's lint into log lines.
        self.runtime.poll_services();
        self.logs.extend(self.runtime.take_logs());
    }

    /// Moves nodes a configuration asked to reparent with a transition, as
    /// the shell does before a paint.
    fn apply_transitions(&mut self) {
        let transitions = self.runtime.take_parent_transitions();
        if transitions.is_empty() {
            return;
        }
        let Ok(root) = primary_surface_root(&self.runtime) else {
            return;
        };
        let available = Size {
            width: f64::from(self.screen.0),
            height: f64::from(self.screen.1),
        };
        for transition in transitions {
            if let Err(error) = Layout::transition_reparent(
                &mut self.runtime.scene_mut(),
                &mut self.text,
                morf_layout::ReparentTransition {
                    root,
                    node: transition.node,
                    new_parent: transition.parent,
                    anchors: transition.anchors,
                    available,
                    behavior: transition.behavior,
                },
            ) {
                self.problems.push(format!("reparent: {error}"));
            }
        }
    }

    /// Advances the virtual clock by `by`: a frame every [`FRAME`], and a
    /// stop at every timer that comes due in between, so a timer fires at
    /// its own time and not the next frame's.
    ///
    /// `real` spreads that much wall time over the advance, for a
    /// configuration whose answers come from processes and buses that run
    /// on the wall clock whatever this one says.
    pub(crate) fn advance(&mut self, by: Duration, real: Duration) {
        let end = self.now + by;
        let frames = (by.as_millis() / FRAME.as_millis()).max(1) as u32;
        let nap = real / frames;
        while self.now < end {
            let next_frame = self.last_frame + FRAME;
            let mut target = end.min(next_frame);
            if let Some(deadline) = self.runtime.next_virtual_deadline()
                && deadline > self.now
            {
                target = target.min(deadline);
            }
            // A deadline at or before now has fired already; step on.
            if target <= self.now {
                target = end.min(next_frame);
            }
            self.runtime.advance_virtual_clock(target - self.now);
            self.now = target;
            if target == next_frame || target == end {
                if !nap.is_zero() {
                    std::thread::sleep(nap);
                }
                let delta = self.now - self.last_frame;
                self.last_frame = self.now;
                self.frame(delta);
            } else {
                self.runtime.poll_services();
                self.logs.extend(self.runtime.take_logs());
            }
        }
    }

    /// Advances a frame at a time until nothing moves -- no animation
    /// running, no layout still converging, no service with news -- or
    /// `limit` has passed. Returns the virtual time it took.
    pub(crate) fn settle(&mut self, limit: Duration) -> Duration {
        let started = self.now;
        let mut quiet = 0;
        while self.now - started < limit {
            let revision = self.runtime.scene().layout_revision();
            self.runtime.advance_virtual_clock(FRAME);
            self.now += FRAME;
            let changed = self.runtime.poll_services();
            let frame = self.runtime.tick_animations(FRAME);
            self.last_frame = self.now;
            self.apply_transitions();
            self.runtime.take_window_surface_change();
            self.runtime.take_layer_surface_change();
            self.layout_all();
            self.runtime.poll_services();
            self.logs.extend(self.runtime.take_logs());
            let moving = changed
                || frame
                    .as_ref()
                    .is_ok_and(|frame| frame.active || frame.changed > 0)
                || self.runtime.scene().layout_revision() != revision
                || self.surfaces.iter().any(|surface| !surface.stable);
            quiet = if moving { 0 } else { quiet + 1 };
            // Two quiet frames: one can be the frame a change lands in.
            if quiet >= 2 {
                break;
            }
        }
        self.now - started
    }

    /// Log lines at `level` or above.
    pub(crate) fn logs_at(&self, level: LogLevel) -> impl Iterator<Item = &LogEntry> {
        self.logs.iter().filter(move |entry| entry.level >= level)
    }
}

/// Resolves a path given on a command line or in a spec: as written when it
/// exists, else against `base`.
pub(crate) fn resolve(path: &str, base: Option<&Path>) -> PathBuf {
    let direct = PathBuf::from(path);
    if direct.is_absolute() {
        return direct;
    }
    if let Some(base) = base {
        let beside = base.join(path);
        if beside.exists() {
            return beside;
        }
    }
    direct
}
