//! The headless backend: windows on virtual outputs, a seat a script drives,
//! offscreen render targets and a clock that moves only when told.
//!
//! What `morf check`, `morf render` and `morf test` stand on. Nothing
//! connects anywhere, so the same run gives the same events every time: a
//! window is configured the moment it opens, sized as a compositor would
//! size it, and a frame comes only when the clock is advanced.

mod layer;
mod outputs;
mod seat;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

pub use layer::{layer_extent, layer_position, layer_stack};
pub use outputs::virtual_outputs;
pub use seat::VirtualSeat;

use crate::backend::{Backend, Capabilities, RenderTarget, WindowKind};
use crate::{Edge, Event, InputRect, Output, WindowId};

/// One window on the headless backend.
#[derive(Clone, Debug, PartialEq)]
struct HeadlessWindow {
    size: (u32, u32),
}

/// Windows on virtual outputs.
#[derive(Debug, Default)]
pub struct HeadlessBackend {
    outputs: Vec<Output>,
    windows: BTreeMap<WindowKey, HeadlessWindow>,
    events: VecDeque<Event>,
    /// The windows that asked for a frame (asked through `&self`, as a
    /// compositor's frame callback is).
    frames: RefCell<BTreeSet<WindowKey>>,
    /// Where each window takes the pointer; absent is everywhere.
    input_regions: RefCell<BTreeMap<WindowKey, Vec<InputRect>>>,
    /// The clock's reading.
    now: Duration,
    locked: bool,
    seat: VirtualSeat,
    cursor: Option<String>,
}

/// A window id in an order: so frames come out the same way every run.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WindowKey(u8, u64);

impl From<WindowId> for WindowKey {
    fn from(id: WindowId) -> Self {
        match id {
            WindowId::Layer(id) => Self(0, id),
            WindowId::Toplevel(id) => Self(1, id),
            WindowId::Popup(id) => Self(2, id),
            WindowId::Lock(index) => Self(3, index as u64),
        }
    }
}

impl From<WindowKey> for WindowId {
    fn from(WindowKey(kind, id): WindowKey) -> Self {
        match kind {
            0 => Self::Layer(id),
            1 => Self::Toplevel(id),
            2 => Self::Popup(id),
            _ => Self::Lock(id as usize),
        }
    }
}

impl HeadlessBackend {
    /// A backend with these outputs.
    pub fn new(outputs: Vec<Output>) -> Self {
        Self {
            outputs,
            ..Self::default()
        }
    }

    /// The first output's logical size, or nothing at all without one.
    fn output_size(&self) -> (u32, u32) {
        self.outputs
            .first()
            .and_then(|output| output.size)
            .map_or((0, 0), |(width, height)| {
                (width.max(0) as u32, height.max(0) as u32)
            })
    }

    /// Replaces the outputs, telling the host.
    pub fn set_outputs(&mut self, outputs: Vec<Output>) {
        self.outputs = outputs.clone();
        self.events.push_back(Event::Screens(outputs));
    }

    /// The seat, as it stands after the events injected so far.
    pub fn seat(&self) -> &VirtualSeat {
        &self.seat
    }

    /// Hands the host an input event, as the seat would send it.
    pub fn inject(&mut self, event: Event) {
        self.seat.observe(&event);
        self.events.push_back(event);
    }

    /// The pointer shape last asked for.
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    /// The clock's reading.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Moves the clock on by `by`: every window that asked for a frame gets
    /// one, stamped with the new reading.
    pub fn advance(&mut self, by: Duration) {
        self.now += by;
        let time_ms = self.now.as_millis() as u32;
        for key in self.frames.take() {
            if !self.windows.contains_key(&key) {
                continue;
            }
            self.events.push_back(match WindowId::from(key) {
                WindowId::Layer(id) => Event::Frame { id, time_ms },
                WindowId::Toplevel(id) => Event::ToplevelFrame { id, time_ms },
                WindowId::Popup(id) => Event::PopupFrame { id, time_ms },
                WindowId::Lock(index) => Event::SessionLockFrame { index, time_ms },
            });
        }
    }

    fn configure(&mut self, id: WindowId, size: (u32, u32)) {
        let (width, height) = size;
        self.windows.insert(id.into(), HeadlessWindow { size });
        self.events.push_back(match id {
            WindowId::Layer(id) => Event::Configure { id, width, height },
            WindowId::Toplevel(id) => Event::ToplevelConfigure { id, width, height },
            WindowId::Popup(id) => Event::PopupConfigure { id, width, height },
            WindowId::Lock(index) => Event::SessionLockConfigure {
                index,
                width,
                height,
            },
        });
    }

    /// The input region a window was last given.
    pub fn input_region(&self, id: WindowId) -> Option<Vec<InputRect>> {
        self.input_regions.borrow().get(&id.into()).cloned()
    }
}

impl Backend for HeadlessBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            layer_shell: true,
            layer_surfaces: true,
            live_layer_change: true,
            toplevels: true,
            popups: true,
            session_lock: true,
            ..Capabilities::default()
        }
    }

    fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    fn open(&mut self, id: WindowId, kind: WindowKind) -> Result<(), String> {
        let output = self.output_size();
        if output == (0, 0) {
            return Err("there is no output to open a window on".to_owned());
        }
        let size = match (id, kind) {
            (WindowId::Layer(_), WindowKind::Layer(config)) => {
                let anchors = config.anchors;
                (
                    layer_extent(anchors.left, anchors.right, config.width, output.0, 1),
                    layer_extent(anchors.top, anchors.bottom, config.height, output.1, 1),
                )
            }
            (WindowId::Toplevel(_), WindowKind::Toplevel { config, .. }) => {
                (config.width.max(1), config.height.max(1))
            }
            (WindowId::Popup(_), WindowKind::Popup { parent, config }) => {
                if !self.windows.contains_key(&parent.into()) {
                    return Err(format!("the popup's parent {parent:?} is not open"));
                }
                (config.width.max(1), config.height.max(1))
            }
            (id, kind) => return Err(format!("{id:?} cannot be opened as {kind:?}")),
        };
        self.configure(id, size);
        Ok(())
    }

    fn close(&mut self, id: WindowId) {
        self.windows.remove(&id.into());
        self.frames.borrow_mut().remove(&id.into());
        self.input_regions.borrow_mut().remove(&id.into());
    }

    fn logical_size(&self, id: WindowId) -> Option<(u32, u32)> {
        self.windows.get(&id.into()).map(|window| window.size)
    }

    fn scale_120(&self, _id: WindowId) -> u32 {
        self.outputs
            .first()
            .map_or(120, |output| output.scale.max(1) as u32 * 120)
    }

    fn request_frame(&self, id: WindowId) {
        self.frames.borrow_mut().insert(id.into());
    }

    fn commit(&self, _id: WindowId) {}

    fn set_input_region(&self, id: WindowId, region: Option<&[InputRect]>) {
        if !self.windows.contains_key(&id.into()) {
            return;
        }
        let mut regions = self.input_regions.borrow_mut();
        match region {
            Some(region) => regions.insert(id.into(), region.to_vec()),
            None => regions.remove(&id.into()),
        };
    }

    fn start_move(&self, id: WindowId) -> bool {
        matches!(id, WindowId::Toplevel(_)) && self.windows.contains_key(&id.into())
    }

    fn start_resize(&self, id: WindowId, _edge: Edge) -> bool {
        self.start_move(id)
    }

    fn set_cursor(&mut self, shape: &str) -> bool {
        self.cursor = Some(shape.to_owned());
        true
    }

    fn lock(&mut self) -> Result<(), String> {
        if self.locked {
            return Err("the session is already locked".to_owned());
        }
        self.locked = true;
        self.events.push_back(Event::SessionLocked);
        let sizes: Vec<(u32, u32)> = self
            .outputs
            .iter()
            .filter_map(|output| output.size)
            .map(|(width, height)| (width.max(1) as u32, height.max(1) as u32))
            .collect();
        for (index, size) in sizes.into_iter().enumerate() {
            self.configure(WindowId::Lock(index), size);
        }
        Ok(())
    }

    fn unlock(&mut self) -> Result<(), String> {
        if !self.locked {
            return Err("the session is not locked".to_owned());
        }
        self.locked = false;
        self.windows
            .retain(|key, _| !matches!(WindowId::from(*key), WindowId::Lock(_)));
        Ok(())
    }

    fn render_target(&self, id: WindowId) -> Option<RenderTarget> {
        let (width, height) = self.logical_size(id)?;
        let scale = self.scale_120(id);
        Some(RenderTarget::Offscreen {
            width: (width * scale).div_ceil(120),
            height: (height * scale).div_ceil(120),
        })
    }

    fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    fn dispatch(&mut self, _timeout: Option<Duration>) -> Result<bool, String> {
        // Nothing arrives on its own: only what was injected, or what the
        // clock brought.
        Ok(!self.events.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LayerConfig, PopupConfig};

    fn backend() -> HeadlessBackend {
        HeadlessBackend::new(virtual_outputs(2, (1920, 1080), 1))
    }

    #[test]
    fn a_layer_is_configured_as_soon_as_it_opens_and_framed_only_when_the_clock_moves() {
        let mut backend = backend();
        backend
            .open(
                WindowId::Layer(0),
                WindowKind::Layer(LayerConfig::default()),
            )
            .unwrap();
        assert_eq!(
            backend.next_event(),
            Some(Event::Configure {
                id: 0,
                width: 1920,
                height: 32
            })
        );
        backend.request_frame(WindowId::Layer(0));
        assert_eq!(backend.next_event(), None);
        backend.advance(Duration::from_millis(16));
        assert_eq!(
            backend.next_event(),
            Some(Event::Frame { id: 0, time_ms: 16 })
        );
        assert_eq!(backend.next_event(), None);
    }

    #[test]
    fn a_popup_needs_its_parent_open() {
        let mut backend = backend();
        let popup = WindowKind::Popup {
            parent: WindowId::Layer(0),
            config: PopupConfig::default(),
        };
        assert!(backend.open(WindowId::Popup(1), popup.clone()).is_err());
        backend
            .open(
                WindowId::Layer(0),
                WindowKind::Layer(LayerConfig::default()),
            )
            .unwrap();
        backend.open(WindowId::Popup(1), popup).unwrap();
        assert!(backend.logical_size(WindowId::Popup(1)).is_some());
    }

    #[test]
    fn a_lock_puts_a_surface_on_every_output_and_an_unlock_takes_them_away() {
        let mut backend = backend();
        backend.lock().unwrap();
        let events: Vec<Event> = std::iter::from_fn(|| backend.next_event()).collect();
        assert_eq!(events[0], Event::SessionLocked);
        assert_eq!(events.len(), 3);
        assert!(backend.logical_size(WindowId::Lock(1)).is_some());
        backend.unlock().unwrap();
        assert!(backend.logical_size(WindowId::Lock(1)).is_none());
    }
}
