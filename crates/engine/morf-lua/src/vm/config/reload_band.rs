//! The engine-owned reload-error band: a failed reload nobody reported is
//! shown on the primary surface, in a tree of the engine's own that no
//! configuration can reach, lay out, clip or take down.

use super::*;
use morf_layout::{Geometry, Layout, Size, TextMeasurer};
use morf_scene::{Color, Element, Value};

/// The surface height the band takes at most.
const BAND_CAP: f64 = 240.0;
/// Space around the band's text: across, and above and below.
const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 6.0;
const FONT_SIZE: f64 = 13.0;
const LINE: f64 = 18.0;
const MAX_LINES: f64 = 8.0;

/// A reload that failed and that no handler of the configuration reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadFailure {
    /// The error, as the reload gave it.
    pub error: String,
    /// The configuration that was being loaded.
    pub path: PathBuf,
    /// The file and line the error points at, when it names a line.
    pub location: Option<(PathBuf, u32)>,
}

impl ReloadFailure {
    pub fn new(error: &str, path: &Path) -> Self {
        Self {
            error: error.to_owned(),
            path: path.to_path_buf(),
            location: error_location(error, path),
        }
    }

    /// What the band says: where, then the error itself.
    pub fn message(&self) -> String {
        let path = self.path.display();
        let place = match &self.location {
            Some((file, line)) if file == &self.path => format!(" at line {line}"),
            Some((file, line)) => format!(" at {}:{line}", file.display()),
            None => String::new(),
        };
        format!(
            "reload of {path} failed{place}; the previous configuration is still running\n{}",
            self.error
        )
    }
}

/// The band as a surface laid it out: its tree, where its box is, and what
/// the layout was made from.
#[derive(Clone, Debug)]
pub struct BandLayout {
    pub root: NodeHandle,
    pub layout: Layout,
    /// The band's own box on the surface: what a press on it lands in.
    pub bounds: Geometry,
    /// The band tree's layout revision and the surface size laid out for.
    pub laid: (u64, (u32, u32)),
}

/// The band's nodes, and what the configuration has been told of it.
pub(crate) struct ReloadBand {
    failure: ReloadFailure,
    root: NodeHandle,
    rect: NodeHandle,
    inset: NodeHandle,
    label: NodeHandle,
    /// How many `on_reload` handlers have been offered the failure.
    offered: usize,
    /// The surface size the band was last fitted to.
    fitted: Option<(u32, u32)>,
}

impl Runtime {
    /// Reports a failed reload: to the configuration's `on_reload`
    /// handlers, or, when none took it (returned true), on the band.
    /// Returns whether the band is shown.
    pub fn report_reload_failure(&mut self, error: &str, path: &Path) -> bool {
        self.clear_reload_error();
        let (offered, reported) = self.dispatch_reload_callbacks(Some(error.to_owned()), 0);
        if reported > 0 {
            return false;
        }
        self.show_reload_band(ReloadFailure::new(error, path), offered);
        self.reload_error_shown()
    }

    /// The failure the band is showing, while it is shown.
    pub fn reload_failure(&self) -> Option<ReloadFailure> {
        let band = self.reload_error.borrow();
        band.as_ref().map(|band| band.failure.clone())
    }

    /// Whether the engine-owned reload-error band is shown.
    pub fn reload_error_shown(&self) -> bool {
        self.reload_band_root().is_some()
    }

    /// The text the band is showing, while it is shown.
    pub fn reload_error_text(&self) -> Option<String> {
        let label = self.reload_error.borrow().as_ref()?.label;
        match self.scene().current(label, "text") {
            Ok(Value::String(text)) => Some(text.clone()),
            _ => None,
        }
    }

    /// The band's root while it is shown: a top-level tree of the engine's
    /// own, which no surface takes as its root.
    pub fn reload_band_root(&self) -> Option<NodeHandle> {
        let root = self.reload_error.borrow().as_ref()?.root;
        self.scene().contains(root).then_some(root)
    }

    /// Takes the band down, if it is up.
    pub fn clear_reload_error(&mut self) {
        let band = self.reload_error.borrow_mut().take();
        if let Some(band) = band
            && self.scene().contains(band.root)
        {
            let _ = self.scene_mut().remove(band.root);
        }
    }

    /// Lays the band out over a surface of `size`, its text wrapped to the
    /// width and its height capped. A band that cannot be laid out is
    /// taken down and its error logged; nothing reaches the caller.
    pub fn lay_out_reload_band(
        &mut self,
        size: (u32, u32),
        text: &mut impl TextMeasurer,
    ) -> Option<BandLayout> {
        let root = self.reload_band_root()?;
        self.fit_reload_band(size);
        let available = Size {
            width: f64::from(size.0),
            height: f64::from(size.1),
        };
        let rect = self.reload_error.borrow().as_ref()?.rect;
        let laid = self
            .compute_layout(root, available, text)
            .and_then(|layout| match layout.geometry(rect) {
                Some(bounds) => Ok((layout, bounds)),
                None => Err("the band was not placed".to_owned()),
            });
        match laid {
            Ok((layout, bounds)) => Some(BandLayout {
                root,
                layout,
                bounds,
                laid: (self.scene().layout_revision_of(root), size),
            }),
            Err(message) => {
                let failure = self.reload_failure().map(|failure| failure.error);
                self.clear_reload_error();
                self.warn(format!(
                    "reload failed: {}; its error band could not be laid out: {message}",
                    failure.unwrap_or_default()
                ));
                None
            }
        }
    }

    /// Offers the shown failure to `on_reload` handlers registered
    /// since it was shown; one that takes it (returns true) reports it,
    /// and the band comes down. Returns whether it did.
    pub(crate) fn offer_reload_failure(&mut self) -> bool {
        let Some((offered, error)) = self
            .reload_error
            .borrow()
            .as_ref()
            .map(|band| (band.offered, band.failure.error.clone()))
        else {
            return false;
        };
        let registered = self.reactive.borrow().lifecycle().reload_callbacks();
        if registered.len() <= offered {
            return false;
        }
        let (offered, reported) = self.dispatch_reload_callbacks(Some(error), offered);
        if reported > 0 {
            self.clear_reload_error();
            return true;
        }
        if let Some(band) = self.reload_error.borrow_mut().as_mut() {
            band.offered = offered;
        }
        false
    }

    fn show_reload_band(&mut self, failure: ReloadFailure, offered: usize) {
        let state = &self.reactive;
        let create = |element| crate::api::ui::scene::create_node(state, element);
        let (root, rect, inset, label) = (
            create(Element::Item),
            create(Element::Rect),
            create(Element::Inset),
            create(Element::Text),
        );
        let white = Color {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 1.0,
        };
        let red = Color {
            red: 0.45,
            green: 0.08,
            blue: 0.08,
            alpha: 0.96,
        };
        let settings = [
            (
                root,
                "id",
                Value::String("morf-reload-error-overlay".to_owned()),
            ),
            (root, "clip", Value::Bool(true)),
            (rect, "id", Value::String("morf-reload-error".to_owned())),
            (rect, "color", Value::Color(red)),
            (rect, "clip", Value::Bool(true)),
            (inset, "margin", Value::Number(PAD_Y)),
            (inset, "left_margin", Value::Number(PAD_X)),
            (inset, "right_margin", Value::Number(PAD_X)),
            (
                label,
                "id",
                Value::String("morf-reload-error-text".to_owned()),
            ),
            (label, "text", Value::String(failure.message())),
            (label, "color", Value::Color(white)),
            (label, "font_size", Value::Number(FONT_SIZE)),
            (label, "wrap", Value::Bool(true)),
        ];
        let mut state = state.borrow_mut();
        let mut problems = Vec::new();
        for (node, property, value) in settings {
            if let Err(message) =
                crate::api::ui::scene::assign_scene_property(&mut state, node, property, value)
            {
                problems.push(format!("{property}: {message}"));
            }
        }
        for (child, parent) in [(label, inset), (inset, rect), (rect, root)] {
            if let Err(message) = state.scene_mut().reparent(child, Some(parent)) {
                problems.push(message.to_string());
            }
        }
        for problem in problems {
            state.log(LogLevel::Warn, format!("reload error band: {problem}"));
        }
        drop(state);
        *self.reload_error.borrow_mut() = Some(ReloadBand {
            failure,
            root,
            rect,
            inset,
            label,
            offered,
            fitted: None,
        });
    }

    /// Sizes the band to a surface: as wide as it, and as many lines as
    /// fit its height up to the cap.
    fn fit_reload_band(&mut self, size: (u32, u32)) {
        let Some((rect, inset, label)) = self
            .reload_error
            .borrow_mut()
            .as_mut()
            .filter(|band| band.fitted != Some(size))
            .map(|band| {
                band.fitted = Some(size);
                (band.rect, band.inset, band.label)
            })
        else {
            return;
        };
        let width = f64::from(size.0);
        let room = f64::from(size.1).min(BAND_CAP) - 2.0 * PAD_Y;
        let lines = (room / LINE).floor().clamp(1.0, MAX_LINES);
        let mut state = self.reactive.borrow_mut();
        for (node, property, value) in [
            (rect, "width", width),
            (inset, "width", width),
            (label, "width", (width - 2.0 * PAD_X).max(1.0)),
            (label, "max_lines", lines),
        ] {
            let value = Value::Number(value);
            if let Err(message) =
                crate::api::ui::scene::assign_scene_property(&mut state, node, property, value)
            {
                state.log(LogLevel::Warn, format!("reload error band: {message}"));
            }
        }
    }
}

/// The file and line an error names: `file.lua:12:` as a runtime error
/// gives it, or `at line 12` as a parse error does, of the file named
/// before it -- or of `path`, when it names none.
pub(crate) fn error_location(error: &str, path: &Path) -> Option<(PathBuf, u32)> {
    let leading_digits = |text: &str| {
        let digits = text.bytes().take_while(u8::is_ascii_digit).count();
        text[..digits].parse::<u32>().ok()
    };
    for (at, _) in error.match_indices(".lua:") {
        if let Some(line) = leading_digits(&error[at + 5..]) {
            return Some((PathBuf::from(file_ending_at(error, at + 4)), line));
        }
    }
    let at = error.find("at line ")?;
    let line = leading_digits(&error[at + 8..])?;
    let file = error[..at]
        .rfind(".lua")
        .map(|end| PathBuf::from(file_ending_at(error, end + 4)))
        .unwrap_or_else(|| path.to_path_buf());
    Some((file, line))
}

/// The path that ends at byte `end` of `text`: back to a space or a quote.
fn file_ending_at(text: &str, end: usize) -> &str {
    let start = text[..end]
        .rfind(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '(' | '['))
        .map_or(0, |at| at + 1);
    &text[start..end]
}

#[cfg(test)]
mod tests;
