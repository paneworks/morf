//! A configuration run with no compositor: what `morf check`, `morf render`
//! and `morf test` share.
//!
//! The same [`Host`] the shell runs on a compositor, on the headless
//! backend: its virtual outputs configure every window it opens, its seat
//! carries the clicks and keys a test sends, and its clock -- with the
//! runtime's virtual one -- moves only when asked, so the same run fires the
//! same timers and frame callbacks every time. Nothing is drawn: the host
//! lays out, and `morf render` draws a surface on demand.

use std::path::{Path, PathBuf};
use std::time::Duration;

use morf_app::backend::headless::{HeadlessBackend, VirtualSeat, virtual_outputs};
use morf_app::{Backend, WindowId};
use morf_layout::Layout;
use morf_lua::{Limits, LogEntry, LogLevel, Runtime};
use morf_scene::NodeHandle;
use morf_text::TextSystem;

use crate::host::turn::{Host, StartOptions, Turn};
use crate::supervisor::LoadPolicy;
use crate::supervisor::{execute_config_on, lua_screen, lua_screens, store_outputs};
use crate::surfaces::PointerInput;

/// One frame of a 60 Hz output, which is what time advances by between
/// animation ticks.
pub const FRAME: Duration = Duration::from_millis(16);

/// Turns a frame allows: the bindings that read the layout, and the handlers
/// that open windows, converge within them as they do on a compositor over
/// as many turns.
pub(crate) const SETTLE_PASSES: usize = 8;

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
        }
    }
}

/// A configuration that did not load, and what it said before it stopped.
#[derive(Debug)]
pub struct LoadFailure {
    pub error: String,
    pub logs: Vec<LogEntry>,
}

/// One surface the configuration asked for.
pub struct Surface {
    pub role: WindowId,
    /// `primary`, `popup`, `floating` or `layer`.
    pub kind: &'static str,
    /// Its namespace, or a floating window's title.
    pub name: String,
    /// The window surface's id; nothing for the primary.
    pub id: Option<u64>,
    pub root: NodeHandle,
    /// Logical size.
    pub size: (u32, u32),
    /// Where a layer surface sits on its screen; a popup's anchor point;
    /// nothing useful for a floating window, which the compositor places.
    pub position: (i32, i32),
    pub visible: bool,
    /// Where a layer-shell compositor stacks it: 0 background, 1 bottom,
    /// 2 top, 3 overlay. `screen` composes surfaces in this order.
    pub stack: u8,
    pub blend: String,
    /// Whether the host has it open; then its layout is the host's
    /// ([`Headless::layout_of`]), and `layout` is unused.
    pub open: bool,
    /// The layout of a surface the host has not opened, laid out here.
    pub layout: Option<Layout>,
    /// The tree revision and size `layout` was computed for.
    pub laid: Option<(u64, (u32, u32))>,
    /// Whether the last layout's bindings agreed with it.
    pub stable: bool,
    /// How a person names it, unique among the surfaces.
    pub label: String,
}

impl Surface {
    /// How a person names it: `primary`, `popup:3`, `layer:impasto-desk`,
    /// with `#id` after it when two surfaces would otherwise share it.
    pub fn label(&self) -> String {
        self.label.clone()
    }

    /// The label before it is made unique.
    pub fn plain_label(&self) -> String {
        match (self.kind, self.id) {
            ("primary", _) => "primary".to_owned(),
            (kind, Some(id)) if self.name.is_empty() => format!("{kind}:{id}"),
            (kind, _) => format!("{kind}:{}", self.name),
        }
    }

    /// Whether `wanted` -- an index, `primary`, a kind, a name, or a label --
    /// names this surface.
    pub fn is_named(&self, wanted: &str) -> bool {
        wanted == self.label
            || wanted == self.plain_label()
            || wanted == self.name
            || wanted == self.kind
            // `floating`: what a toplevel was called before.
            || (wanted == "floating" && self.kind == "toplevel")
            || self
                .id
                .is_some_and(|id| wanted == format!("{}:{id}", self.kind))
    }
}

/// A configuration running with nothing under it.
pub struct Headless {
    pub runtime: Runtime,
    /// The host and its headless backend; none with no screen (the outputless
    /// runtime) or a configuration that built no surface.
    pub host: Option<Host>,
    /// Lays out what the host has not opened (a hidden panel), so its
    /// problems are found before it is shown; with a host, the host's.
    pub(crate) text: Option<TextSystem>,
    pub screen: (u32, u32),
    pub surfaces: Vec<Surface>,
    /// Every log line since load, in order.
    pub logs: Vec<LogEntry>,
    /// Problems the runner itself found: layouts that failed, surfaces that
    /// could not be worked out.
    pub problems: Vec<String>,
    /// The virtual clock's reading, and when the last frame was.
    now: Duration,
    last_frame: Duration,
    /// Loaded with no screen: nothing it declares is mapped.
    pub outputless: bool,
    /// The layout revision each open surface was last linted at.
    pub(crate) linted: std::collections::HashMap<WindowId, u64>,
    /// The seat and pointer of a run with no host.
    idle_seat: VirtualSeat,
    idle_input: PointerInput,
}

impl Headless {
    /// Loads a configuration onto one headless screen, on a virtual clock.
    pub fn load(options: &LoadOptions) -> Result<Self, LoadFailure> {
        let failure = |error: String| LoadFailure {
            error,
            logs: Vec::new(),
        };
        let screens = virtual_outputs(options.screens, options.size, options.scale);
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
        let mut problems = Vec::new();
        let host = match &own {
            Some(own) if crate::surfaces::primary_surface_root(&runtime).is_ok() => {
                // The output this runtime draws to, first: where its windows go.
                let mut ordered = vec![own.clone()];
                ordered.extend(screens.iter().filter(|screen| screen.id != own.id).cloned());
                let backend = HeadlessBackend::new(ordered);
                match Host::start(
                    &mut runtime,
                    Box::new(backend),
                    None,
                    StartOptions {
                        name: own.name.clone().unwrap_or_default(),
                        gpu: false,
                        publish_capabilities: false,
                        desktop_canvas: false,
                        report_slow: false,
                    },
                ) {
                    Ok(host) => Some(host),
                    Err(error) => {
                        return Err(LoadFailure {
                            error,
                            logs: runtime.take_logs(),
                        });
                    }
                }
            }
            Some(_) => {
                if let Err(error) = crate::surfaces::primary_surface_root(&runtime) {
                    problems.push(error);
                }
                None
            }
            None => None,
        };
        let text = host.is_none().then(TextSystem::new);
        let mut headless = Self {
            runtime,
            host,
            text,
            screen: options.size,
            surfaces: Vec::new(),
            logs: Vec::new(),
            problems,
            now: Duration::ZERO,
            last_frame: Duration::ZERO,
            outputless,
            linted: std::collections::HashMap::new(),
            idle_seat: VirtualSeat::default(),
            idle_input: PointerInput::default(),
        };
        // The first frame, at time zero: what the shell draws before any
        // time has passed.
        headless.frame(Duration::ZERO);
        Ok(headless)
    }

    /// The virtual time since load.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// One frame: the clock's frame callbacks, `delta` after the last, and
    /// as many turns of the host as it wants before it is quiet.
    pub fn frame(&mut self, delta: Duration) {
        match self.host.as_mut() {
            Some(host) => {
                if let Some(backend) = host.backend.as_headless_mut() {
                    // Every frame calls back, whether or not the last paint
                    // asked: the frames are this clock's ticks, each of
                    // `delta`.
                    backend.request_frame(WindowId::Layer(morf_app::PRIMARY_LAYER));
                    backend.advance(delta);
                }
                for _ in 0..SETTLE_PASSES {
                    match host.turn(&mut self.runtime, None) {
                        Ok(Turn::Again) => {}
                        Ok(_) => break,
                        Err(error) => {
                            if !self.problems.contains(&error) {
                                self.problems.push(error);
                            }
                            break;
                        }
                    }
                    if host.settled() {
                        break;
                    }
                }
            }
            // No surface to drive frames: the services, the timers and the
            // animations, as the outputless runtime has them.
            None => {
                self.runtime.poll_services();
                if let Err(error) = self.runtime.tick_animations(delta) {
                    self.problems.push(format!("animation: {error}"));
                }
                self.runtime.take_window_surface_change();
                self.runtime.take_layer_surface_change();
            }
        }
        self.refresh_surfaces();
        self.lint_open();
        self.lay_out_hidden();
        // The poll is what turns a layout's lint into log lines.
        self.runtime.poll_services();
        self.logs.extend(self.runtime.take_logs());
    }

    /// Advances the virtual clock by `by`: a frame every [`FRAME`], and a
    /// stop at every timer that comes due in between, so a timer fires at
    /// its own time and not the next frame's.
    ///
    /// `real` spreads that much wall time over the advance, for a
    /// configuration whose answers come from processes and buses that run
    /// on the wall clock whatever this one says.
    pub fn advance(&mut self, by: Duration, real: Duration) {
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
    /// running, no layout still converging, no turn still owed -- or `limit`
    /// has passed. Returns the virtual time it took.
    pub fn settle(&mut self, limit: Duration) -> Duration {
        let started = self.now;
        let mut quiet = 0;
        while self.now - started < limit {
            let revision = self.runtime.scene().layout_revision();
            self.runtime.advance_virtual_clock(FRAME);
            self.now += FRAME;
            self.last_frame = self.now;
            self.frame(FRAME);
            let moving = self.runtime.has_motion()
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

    /// The seat: where the pointer is, and which surface has the keyboard.
    pub fn seat(&self) -> &VirtualSeat {
        self.host
            .as_ref()
            .and_then(|host| host.backend.as_headless())
            .map_or(&self.idle_seat, HeadlessBackend::seat)
    }

    /// Where the pointer, the buttons and the fingers are, as the host's
    /// pointer path keeps them.
    pub fn input(&self) -> &PointerInput {
        self.host
            .as_ref()
            .map_or(&self.idle_input, |host| &host.state.input)
    }

    /// The text system layouts are measured with.
    pub fn text(&mut self) -> &mut TextSystem {
        match &mut self.host {
            Some(host) => host.state.painter.text(),
            None => self.text.get_or_insert_with(TextSystem::new),
        }
    }

    /// Log lines at `level` or above.
    pub fn logs_at(&self, level: LogLevel) -> impl Iterator<Item = &LogEntry> {
        self.logs.iter().filter(move |entry| entry.level >= level)
    }
}

/// Resolves a path given on a command line or in a spec: as written when it
/// exists, else against `base`.
pub fn resolve(path: &str, base: Option<&Path>) -> PathBuf {
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
