//! What the output under a configuration can do, and how big one of morf's
//! pixels is on it.

use morf_value::density::Density;

#[derive(Default)]
pub struct Display {
    /// What this output's compositor and GPU can do, as name = value pairs.
    capabilities: Vec<(String, String)>,
    /// How big one of morf's pixels is, as the configuration said
    /// (`morf.window.density`).
    density: Density,
    /// Whether `density` changed since the host last took it.
    density_changed: bool,
}

impl Display {
    /// What the compositor and GPU can do, as name = value pairs.
    pub fn capabilities(&self) -> &[(String, String)] {
        &self.capabilities
    }

    /// Records what the compositor and GPU can do.
    pub fn set_capabilities(&mut self, capabilities: Vec<(String, String)>) {
        self.capabilities = capabilities;
    }

    /// Whether the capability `name` is there.
    pub fn capable(&self, name: &str) -> bool {
        self.capabilities
            .iter()
            .any(|(key, value)| key == name && value == "true")
    }

    /// How big one of morf's pixels is.
    pub fn density(&self) -> Density {
        self.density
    }

    /// The configuration set the density; whether that changed it.
    pub fn set_density(&mut self, density: Density) -> bool {
        if self.density == density {
            return false;
        }
        self.density = density;
        self.density_changed = true;
        true
    }

    /// The density, when it changed since this was last asked.
    pub fn take_density_change(&mut self) -> Option<Density> {
        std::mem::take(&mut self.density_changed).then_some(self.density)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_density_change_is_taken_once() {
        let mut display = Display::default();
        assert!(!display.set_density(Density::default()));
        assert_eq!(display.take_density_change(), None);
        display.set_capabilities(vec![("gamma_control".into(), "true".into())]);
        assert!(display.capable("gamma_control"));
        assert!(!display.capable("screencopy"));
    }
}
