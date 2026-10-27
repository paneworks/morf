//! A held lock with no compositor: the headless backend grants it at once,
//! and the lock's own event handling (`lock::session`) builds one tree per
//! virtual output, as a compositor's grant does.
//!
//! Nothing is drawn here: each lock surface is laid out every frame its tree
//! or size moved, and `morf render` draws one on demand.

use std::collections::HashMap;
use std::time::Duration;

use morf_app::backend::headless::HeadlessBackend;
use morf_app::{Backend, Output, WindowId};
use morf_layout::Size;
use morf_lua::Runtime;
use morf_text::TextSystem;

use crate::headless::{Headless, SETTLE_PASSES, Surface};
use crate::input::cursor::BackendCursor;
use crate::input::pointer::answer_new_containment;
use crate::lock::outputs::size_lock_root;
use crate::lock::session::{LockSession, LockStep};
use crate::paint::CachedLayout;

/// A lock the headless backend holds.
pub struct HeadlessLock {
    pub session: LockSession,
    pub backend: HeadlessBackend,
    /// The configuration lifted it: its surfaces are gone.
    pub unlocked: bool,
    /// The tree revision and size each surface was last laid out at, and
    /// whether that layout's bindings agreed with it.
    laid: HashMap<usize, (u64, (u32, u32), bool)>,
}

impl HeadlessLock {
    /// Whether a loaded configuration is a lock built per output: one that
    /// asked for the session lock and gave `morf.lock.surface` a builder.
    pub fn wanted(runtime: &Runtime) -> bool {
        runtime.layer_surface_config().session_lock && runtime.has_lock_surface_builder()
    }

    /// Asks the backend for the lock on every one of `screens`.
    pub fn start(runtime: &mut Runtime, screens: Vec<Output>) -> Result<Self, String> {
        let session = LockSession::new(runtime)?;
        let mut backend = HeadlessBackend::new(screens);
        LockSession::request(runtime, &mut backend)?;
        Ok(Self {
            session,
            backend,
            unlocked: false,
            laid: HashMap::new(),
        })
    }

    /// One frame `delta` after the last: frame callbacks, the services, the
    /// events, and a layout of every surface that moved.
    pub fn frame(
        &mut self,
        runtime: &mut Runtime,
        text: &mut TextSystem,
        delta: Duration,
        problems: &mut Vec<String>,
    ) {
        if self.unlocked {
            runtime.poll_services();
            return;
        }
        for index in self.session.indices() {
            self.backend.request_frame(WindowId::Lock(index));
        }
        self.backend.advance(delta);
        runtime.poll_services();
        runtime.take_window_surface_change();
        runtime.take_layer_surface_change();
        if self.session.wants_unlock(runtime) {
            if let Err(error) = self.session.unlock(runtime, &mut self.backend) {
                problems.push(format!("unlock: {error}"));
            }
            self.session.release(runtime);
            self.laid.clear();
            self.unlocked = true;
            return;
        }
        while let Some(event) = self.backend.next_event() {
            let step = self
                .session
                .handle(runtime, &mut BackendCursor(&mut self.backend), event);
            let result = match step {
                Ok(LockStep::Handled(_)) => Ok(()),
                Ok(LockStep::Finished(error)) => Err(error),
                Ok(LockStep::Configure {
                    index,
                    width,
                    height,
                }) => {
                    let screen = self.backend.outputs().get(index).cloned();
                    self.session
                        .configure(runtime, index, screen, (width, height))
                        .map(drop)
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result
                && !problems.contains(&error)
            {
                problems.push(error);
            }
        }
        self.lay_out(runtime, text, problems);
        answer_new_containment(runtime, &self.session.input, &self.session.outputs);
    }

    /// Lays out every lock surface whose tree or size moved since the last.
    fn lay_out(
        &mut self,
        runtime: &mut Runtime,
        text: &mut TextSystem,
        problems: &mut Vec<String>,
    ) {
        for index in self.session.indices() {
            let Some(root) = self.session.root(index) else {
                continue;
            };
            let Some(window) = self.session.window_mut(index) else {
                continue;
            };
            let size = (window.width, window.height);
            if let Err(error) = size_lock_root(runtime, root, size) {
                problems.push(error);
                continue;
            }
            let revision = runtime.scene().layout_revision_of(root);
            if window.layout.is_some()
                && self
                    .laid
                    .get(&index)
                    .is_some_and(|laid| (laid.0, laid.1) == (revision, size))
            {
                continue;
            }
            let available = Size {
                width: f64::from(size.0),
                height: f64::from(size.1),
            };
            match runtime.settle_layout(root, available, text, SETTLE_PASSES) {
                Ok(settled) => {
                    runtime.sync_text_inputs(&settled.layout, text);
                    runtime.observe_stretch(&settled.layout);
                    runtime.observe_layout(&settled.layout);
                    let revision = runtime.scene().layout_revision_of(root);
                    self.laid.insert(index, (revision, size, settled.stable));
                    if let Some(window) = self.session.window_mut(index) {
                        window.layout = Some(CachedLayout::uncached(settled.layout));
                    }
                }
                Err(error) => {
                    let problem = format!("lock surface {index}: layout: {error}");
                    if !problems.contains(&problem) {
                        problems.push(problem);
                    }
                }
            }
        }
    }

    /// The lock's surfaces, as [`Headless::surfaces`] lists them: one per
    /// output, the whole of it, above everything.
    pub fn surfaces(&self, runtime: &Runtime) -> Vec<Surface> {
        let blend = runtime.layer_surface_config().blend.clone();
        self.session
            .indices()
            .into_iter()
            .filter_map(|index| {
                let root = self.session.root(index)?;
                let window = self.session.outputs.by_window(WindowId::Lock(index))?;
                let name = self
                    .backend
                    .outputs()
                    .get(index)
                    .and_then(|output| output.name.clone())
                    .unwrap_or_default();
                Some(Surface {
                    role: WindowId::Lock(index),
                    kind: "lock",
                    name,
                    id: Some(index as u64),
                    root,
                    size: (window.width, window.height),
                    position: (0, 0),
                    visible: true,
                    stack: crate::headless::surfaces::layer_stack("overlay"),
                    blend: blend.clone(),
                    open: true,
                    layout: None,
                    laid: None,
                    stable: self.laid.get(&index).is_none_or(|laid| laid.2),
                    label: String::new(),
                })
            })
            .collect()
    }
}

impl Headless {
    /// The layout a lock surface was last given.
    pub(crate) fn lock_layout(&self, role: WindowId) -> Option<&morf_layout::Layout> {
        self.lock
            .as_ref()?
            .session
            .outputs
            .by_window(role)?
            .layout
            .as_ref()
            .map(|cached| &cached.layout)
    }
}
