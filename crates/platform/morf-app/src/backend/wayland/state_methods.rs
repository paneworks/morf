use smithay_client_toolkit::shm::slot::SlotPool;
use wayland_client::protocol::{wl_output, wl_shm};
use wayland_client::QueueHandle;

use crate::backend::wayland::{helpers::*, state_types::*, surface_types::*};

impl LayerState {
    /// Maps a layer surface with one transparent pixel, once it is configured.
    ///
    /// Only a surface that asked for it and has not been mapped yet is touched,
    /// so this is safe to call from both the request and the configure handler.
    pub(crate) fn attach_blank_buffer(&mut self, id: u64) {
        let Some(record) = self.layers.get(&id) else {
            return;
        };
        if !record.wants_blank || record.blank.is_some() || !record.configured {
            return;
        }
        let color = record.blank_color;
        let Some(shm) = self.shm.as_ref() else {
            return;
        };
        let Ok(mut pool) = SlotPool::new(4, shm) else {
            return;
        };
        let Ok((buffer, canvas)) = pool.create_buffer(1, 1, 4, wl_shm::Format::Argb8888) else {
            return;
        };
        canvas[..4].copy_from_slice(&color);
        let Some(record) = self.layers.get_mut(&id) else {
            return;
        };
        let surface = record.surface.wl_surface();
        if buffer.attach_to(surface).is_err() {
            return;
        }
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
        record.blank = Some((pool, buffer));
    }

    pub(crate) fn refresh_virtual_keyboard(&mut self, qh: &QueueHandle<Self>) {
        if self.virtual_keyboard.is_some() {
            return;
        }
        let Some(manager) = &self.virtual_keyboard_manager else {
            return;
        };
        let Some(seat) = self.seats.seats().next() else {
            return;
        };
        let Some(keymap) = &self.virtual_keyboard_keymap else {
            return;
        };
        let keyboard = manager.create_virtual_keyboard(&seat, qh, ());
        match install_virtual_keymap(&keyboard, keymap) {
            Ok(file) => {
                self.virtual_keyboard_keymap_file = Some(file);
                self.virtual_keyboard = Some(keyboard);
            }
            Err(_) => keyboard.destroy(),
        }
    }

    pub(crate) fn refresh_data_devices(&mut self, qh: &QueueHandle<Self>) {
        let Some(manager) = &self.data_device_manager else {
            return;
        };
        for seat in self.seats.seats() {
            if self
                .data_devices
                .iter()
                .all(|device| device.data().seat() != &seat)
            {
                self.data_devices.push(manager.get_data_device(qh, &seat));
            }
        }
    }

    /// Starts tracking fractional scale for a popup or floating window.
    ///
    /// The same two objects a layer surface gets, kept in `aux_scales` because
    /// those surfaces have no record of their own. A compositor offering
    /// neither protocol leaves both `None`, and the surface stays at 1x --
    /// which is what it did before this existed.
    pub(crate) fn track_aux_scale(
        &mut self,
        role: WindowId,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        qh: &QueueHandle<Self>,
    ) {
        let fractional = self
            .fractional_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(surface, qh, role));
        let viewport = self
            .viewporter
            .as_ref()
            .map(|manager| manager.get_viewport(surface, qh, ()));
        self.aux_scales.insert(
            role,
            AuxSurfaceScale {
                fractional,
                viewport,
                scale_120: 120,
            },
        );
    }

    /// Holds the session awake, or stops holding it.
    ///
    /// The protocol has no "off": an inhibitor exists or it does not, and
    /// destroying it is how the session is released. So this is idempotent by
    /// construction — asking twice for the same state does nothing the second
    /// time, which matters because a configuration is likely to assign this
    /// from a binding that re-runs on every frame.
    pub(crate) fn set_idle_inhibited(&mut self, inhibited: bool, qh: &QueueHandle<Self>) {
        if inhibited == self.idle_inhibitor.is_some() {
            return;
        }
        match self.idle_inhibitor.take() {
            Some(inhibitor) => inhibitor.destroy(),
            None => {
                let Some(manager) = &self.idle_inhibit_manager else {
                    return;
                };
                // Against the shell's own surface, because the protocol scopes
                // inhibition to a surface. Looked up rather than taken through
                // `layer()`, which panics when there is none: a configuration
                // may ask for this before its surface exists, and refusing to
                // inhibit is the right answer there rather than dying.
                let Some(layer) = self.layers.get(&crate::backend::wayland::PRIMARY_LAYER) else {
                    return;
                };
                self.idle_inhibitor =
                    Some(manager.create_inhibitor(layer.surface.wl_surface(), qh, ()));
            }
        }
    }

    pub(crate) fn create_lock_surface(
        &mut self,
        output: wl_output::WlOutput,
        qh: &QueueHandle<Self>,
    ) {
        // Not filtered on `is_locked`: the surfaces go up the moment the lock
        // is asked for, before the compositor confirms it. The protocol says
        // the client "is expected to create lock surfaces for all outputs
        // currently present" and that `locked` "must not be sent until a new
        // 'locked' frame has been presented on all outputs" — so a client that
        // waits for `locked` before making any surface is waiting for
        // something its own surfaces are the condition of.
        let Some(lock) = self.session_lock.clone() else {
            return;
        };
        if self
            .lock_surfaces
            .iter()
            .any(|surface| surface.output == output)
        {
            return;
        }
        let scale = self
            .outputs
            .info(&output)
            .map(|info| info.scale_factor.max(1) as u32)
            .unwrap_or(1);
        let surface = self.compositor.create_surface(qh);
        surface.set_buffer_scale(scale as i32);
        // No commit here. A lock surface is not an xdg surface: the compositor
        // configures it the moment it exists, and a commit before the first
        // buffer is the protocol's `null_buffer` error — Hyprland ends the
        // whole lock on it. The first commit is the first frame.
        let surface = lock.create_lock_surface(surface, &output, qh);
        self.lock_surfaces.push(LockSurface {
            surface,
            output,
            size: (1, 1),
            scale,
            primer: None,
        });
    }

    /// Presents one lock surface's first frame from shared memory: a plain
    /// field of one colour, the size the compositor asked for.
    ///
    /// The protocol wants a locked frame on every output before it will call
    /// the session locked, and a compositor that shows its own "the lock
    /// screen died" screen does so within a second of the request. A GPU
    /// device and a first real frame per output take longer than that, so
    /// this goes up first: a memset, one commit, and the compositor has its
    /// frame while the GPU is still being found.
    pub(crate) fn prime_lock_surface(&mut self, index: usize, color: [u8; 4]) -> bool {
        let Some(shm) = self.shm.as_ref() else {
            return false;
        };
        let Some(record) = self.lock_surfaces.get(index) else {
            return false;
        };
        let width = record.size.0.saturating_mul(record.scale).max(1);
        let height = record.size.1.saturating_mul(record.scale).max(1);
        let stride = width.saturating_mul(4);
        let Ok(mut pool) = SlotPool::new((stride as usize) * (height as usize), shm) else {
            return false;
        };
        let Ok((buffer, canvas)) = pool.create_buffer(
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        ) else {
            return false;
        };
        // ARGB8888 is little-endian: blue first in memory.
        let [red, green, blue, alpha] = color;
        for pixel in canvas.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[blue, green, red, alpha]);
        }
        let Some(record) = self.lock_surfaces.get_mut(index) else {
            return false;
        };
        let surface = record.surface.wl_surface();
        if buffer.attach_to(surface).is_err() {
            return false;
        }
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();
        record.primer = Some((pool, buffer));
        true
    }
}
