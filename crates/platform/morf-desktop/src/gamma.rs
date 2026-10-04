//! `wlr-gamma-control-unstable-v1`: an output's colour ramps, for a night
//! light — warmer, dimmer, or with another gamma.
//!
//! The ramps are computed here from a colour temperature, the way
//! wlsunset and hyprsunset do: the white point of a black body at that
//! temperature (normalised so 6500 K is neutral), times a brightness, through
//! a gamma curve. The compositor puts an output's own ramps back the moment
//! its control is destroyed, so a reset is dropping the control, and a shell
//! that exits or crashes — its connection closing — restores every output
//! it touched without having to.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;

use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_output;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

use crate::{Desktop, DesktopState};

/// What an output's ramps should do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GammaSettings {
    /// Colour temperature in kelvin; 6500 is neutral.
    pub temperature: f64,
    /// 0 (black) to 1 (unchanged).
    pub brightness: f64,
    /// The curve's exponent; 1 is linear.
    pub gamma: f64,
}

impl Default for GammaSettings {
    fn default() -> Self {
        Self {
            temperature: NEUTRAL,
            brightness: 1.0,
            gamma: 1.0,
        }
    }
}

/// The temperature that leaves colours as they are.
pub const NEUTRAL: f64 = 6500.0;
/// The range of temperatures the approximation below holds for.
pub const TEMPERATURE_RANGE: (f64, f64) = (1000.0, 25000.0);

/// A black body's colour at `kelvin`, each channel 0..1, before
/// normalising. Tanner Helland's fit to the CIE 1964 data, which is what
/// most night-light tools use.
fn black_body(kelvin: f64) -> [f64; 3] {
    let t = kelvin.clamp(TEMPERATURE_RANGE.0, TEMPERATURE_RANGE.1) / 100.0;
    let red = if t <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)
    };
    let green = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    [red, green, blue].map(|channel| (channel / 255.0).clamp(0.0, 1.0))
}

/// The white point at `kelvin`: 1, 1, 1 at 6500 K, and the brightest
/// channel at 1 at any other.
pub fn white_point(kelvin: f64) -> [f64; 3] {
    let raw = black_body(kelvin);
    let neutral = black_body(NEUTRAL);
    let scaled: [f64; 3] = std::array::from_fn(|index| raw[index] / neutral[index]);
    let most = scaled
        .iter()
        .copied()
        .fold(f64::MIN, f64::max)
        .max(f64::EPSILON);
    scaled.map(|channel| (channel / most).clamp(0.0, 1.0))
}

/// The three ramps of `size` entries each, red then green then blue, as
/// `set_gamma` wants them.
pub fn ramps(size: usize, settings: GammaSettings) -> Vec<u16> {
    let white = white_point(settings.temperature);
    let brightness = settings.brightness.clamp(0.0, 1.0);
    let gamma = if settings.gamma.is_finite() && settings.gamma > 0.0 {
        settings.gamma
    } else {
        1.0
    };
    let mut out = Vec::with_capacity(size * 3);
    for channel in white {
        for index in 0..size {
            let value = if size > 1 {
                index as f64 / (size - 1) as f64
            } else {
                1.0
            };
            let level = (value * channel * brightness).powf(1.0 / gamma);
            out.push((level.clamp(0.0, 1.0) * f64::from(u16::MAX)).round() as u16);
        }
    }
    out
}

/// One output's control, and what it is to show once the compositor says
/// how long its ramps are.
pub(crate) struct GammaControl {
    pub(crate) output: wl_output::WlOutput,
    pub(crate) control: ZwlrGammaControlV1,
    size: Option<u32>,
    wanted: GammaSettings,
    applied: Option<GammaSettings>,
}

#[derive(Default)]
pub(crate) struct GammaState {
    pub(crate) manager: Option<ZwlrGammaControlManagerV1>,
    pub(crate) controls: Vec<GammaControl>,
    /// Outputs whose control the compositor refused (another client holds
    /// it, or the output has no ramps), by name, waiting to be reported.
    pub(crate) failures: Vec<String>,
}

impl GammaState {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<DesktopState>) -> Self {
        Self {
            manager: globals.bind(qh, 1..=1, ()).ok(),
            ..Self::default()
        }
    }

    /// Lets go of a control whose output went away.
    pub(crate) fn forget(&mut self, output: &wl_output::WlOutput) {
        self.controls.retain(|control| {
            let keep = control.output != *output;
            if !keep {
                control.control.destroy();
            }
            keep
        });
    }

    fn apply(control: &mut GammaControl) {
        let Some(size) = control.size else {
            return;
        };
        if control.applied == Some(control.wanted) || size == 0 {
            return;
        }
        let table = ramps(size as usize, control.wanted);
        let bytes: Vec<u8> = table.iter().flat_map(|value| value.to_ne_bytes()).collect();
        let written = memfd_create("morf-gamma", MemfdFlags::CLOEXEC)
            .map_err(std::io::Error::from)
            .map(File::from)
            .and_then(|mut file| {
                file.write_all(&bytes)?;
                file.seek(SeekFrom::Start(0))?;
                Ok(file)
            });
        if let Ok(file) = written {
            control.control.set_gamma(file.as_fd());
            control.applied = Some(control.wanted);
        }
    }
}

impl Desktop {
    /// Whether the compositor offers gamma control.
    pub fn supports_gamma_control(&self) -> bool {
        self.state.gamma.manager.is_some()
    }

    /// Sets the ramps of the named output, or this shell's own, or all.
    pub fn set_gamma(
        &mut self,
        output: Option<&str>,
        settings: GammaSettings,
    ) -> Result<(), String> {
        let Some(manager) = self.state.gamma.manager.clone() else {
            return Err("this compositor has no gamma control".to_owned());
        };
        let qh = self.handle();
        for output in self.state.targets(output)? {
            let gamma = &mut self.state.gamma;
            let index = match gamma
                .controls
                .iter()
                .position(|control| control.output == output)
            {
                Some(index) => index,
                None => {
                    let control = manager.get_gamma_control(&output, &qh, output.clone());
                    gamma.controls.push(GammaControl {
                        output,
                        control,
                        size: None,
                        wanted: settings,
                        applied: None,
                    });
                    gamma.controls.len() - 1
                }
            };
            let control = &mut gamma.controls[index];
            control.wanted = settings;
            GammaState::apply(control);
        }
        Ok(())
    }

    /// Gives the named output (or this shell's, or every one) its own ramps
    /// back.
    pub fn reset_gamma(&mut self, output: Option<&str>) -> Result<(), String> {
        let targets = match output {
            Some(_) => self.state.targets(output)?,
            // A reset without a name puts back everything this shell changed.
            None => self
                .state
                .gamma
                .controls
                .iter()
                .map(|control| control.output.clone())
                .collect(),
        };
        self.state.gamma.controls.retain(|control| {
            let keep = !targets.contains(&control.output);
            if !keep {
                control.control.destroy();
            }
            keep
        });
        Ok(())
    }

    /// Names of outputs whose gamma control was refused since the last call.
    pub fn take_gamma_failures(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.gamma.failures)
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for DesktopState {
    fn event(
        _: &mut Self,
        _: &ZwlrGammaControlManagerV1,
        _: <ZwlrGammaControlManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, wl_output::WlOutput> for DesktopState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        output: &wl_output::WlOutput,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(index) = state
            .gamma
            .controls
            .iter()
            .position(|control| control.control == *proxy)
        else {
            return;
        };
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                let control = &mut state.gamma.controls[index];
                control.size = Some(size);
                GammaState::apply(control);
            }
            zwlr_gamma_control_v1::Event::Failed => {
                let control = state.gamma.controls.remove(index);
                control.control.destroy();
                let name = state.output_name(output);
                state.gamma.failures.push(name);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_the_identity_ramp() {
        let table = ramps(256, GammaSettings::default());
        assert_eq!(table.len(), 768);
        for channel in 0..3 {
            assert_eq!(table[channel * 256], 0);
            assert_eq!(table[channel * 256 + 255], u16::MAX);
            let middle = table[channel * 256 + 128];
            assert!((i64::from(middle) - 128 * 257).abs() <= 1, "{middle}");
        }
    }

    #[test]
    fn warm_keeps_red_and_takes_blue() {
        let white = white_point(3000.0);
        assert_eq!(white[0], 1.0);
        assert!(white[1] > 0.6 && white[1] < 0.8, "{white:?}");
        assert!(white[2] > 0.3 && white[2] < 0.55, "{white:?}");
        // Warmer is monotonically less blue.
        let warmer = white_point(2000.0);
        assert!(warmer[2] < white[2] && warmer[1] < white[1]);
        // Cooler than neutral takes red instead, and blue stays whole.
        let cool = white_point(10000.0);
        assert_eq!(cool[2], 1.0);
        assert!(cool[0] < 1.0);
        // Out of range is clamped rather than refused.
        assert_eq!(white_point(1.0), white_point(TEMPERATURE_RANGE.0));
    }

    #[test]
    fn brightness_and_gamma_shape_the_curve() {
        let dim = ramps(
            3,
            GammaSettings {
                brightness: 0.5,
                ..GammaSettings::default()
            },
        );
        assert_eq!(dim[2], (0.5 * 65535.0f64).round() as u16);
        let curved = ramps(
            3,
            GammaSettings {
                gamma: 2.0,
                ..GammaSettings::default()
            },
        );
        // 0.5 through an exponent of 1/2 is about 0.707.
        assert_eq!(curved[1], (0.5f64.sqrt() * 65535.0).round() as u16);
        assert_eq!(ramps(1, GammaSettings::default()), vec![u16::MAX; 3]);
        // A nonsense gamma is treated as linear rather than producing NaN.
        let odd = ramps(
            3,
            GammaSettings {
                gamma: 0.0,
                ..GammaSettings::default()
            },
        );
        assert_eq!(odd, ramps(3, GammaSettings::default()));
    }
}
