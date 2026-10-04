//! Text and layout direction: `layout_direction = "ltr" | "rtl"` on any
//! node, "" (the default) taking its parent's, the root the process's --
//! the locale's unless told otherwise.
//!
//! In a right-to-left subtree the layout mirrors what is laid out relative
//! to the parent's sides: anchored children, the packing of rows and grids,
//! an inset's margins and a flex row's run, and a text's left or right
//! alignment. An explicit `x` stays where it is, as in Qt's layout
//! mirroring: a drawing positioned by number is the drawing's own business.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::{NodeHandle, Scene};

static DEFAULT_RTL: AtomicBool = AtomicBool::new(false);

/// The direction a subtree with none of its own takes.
pub fn set_default_rtl(rtl: bool) {
    DEFAULT_RTL.store(rtl, Ordering::Relaxed);
}

pub fn default_rtl() -> bool {
    DEFAULT_RTL.load(Ordering::Relaxed)
}

/// Whether a locale (`ar_EG.UTF-8`, `he`, `fa_IR`, ...) writes right to left.
pub fn locale_is_rtl(locale: &str) -> bool {
    let language = locale
        .split(['_', '.', '@', '-'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        language.as_str(),
        "ar" | "he"
            | "iw"
            | "fa"
            | "ur"
            | "yi"
            | "ji"
            | "ps"
            | "sd"
            | "ug"
            | "dv"
            | "ckb"
            | "syr"
            | "ku"
    )
}

/// The locale's direction, from `LC_ALL`, `LC_MESSAGES`, `LANG` in that order.
pub fn locale_rtl_from_env() -> bool {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            return locale_is_rtl(&value);
        }
    }
    false
}

impl Scene {
    /// Whether `node` lays out right to left: its own `layout_direction`,
    /// else its nearest ancestor's, else the process's default.
    pub fn is_rtl(&self, node: NodeHandle) -> bool {
        let mut current = Some(node);
        while let Some(n) = current {
            match self.string_value(n, "layout_direction") {
                Ok("rtl") => return true,
                Ok("ltr") => return false,
                _ => {}
            }
            current = self.parent(n).ok().flatten();
        }
        default_rtl()
    }
}

impl Scene {
    /// A text's `horizontal_alignment` as it is drawn: left and right
    /// swapped in a right-to-left subtree, so text keeps to its start.
    pub fn directed_alignment(&self, node: NodeHandle) -> Result<&str, crate::SceneError> {
        let value = self.string_value(node, "horizontal_alignment")?;
        if !matches!(value, "left" | "right") || !self.is_rtl(node) {
            return Ok(value);
        }
        Ok(if value == "left" { "right" } else { "left" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_and_inheritance() {
        assert!(
            locale_is_rtl("ar_EG.UTF-8") && locale_is_rtl("he") && !locale_is_rtl("en_US.UTF-8")
        );
        let mut scene = Scene::new();
        let root = scene.create(crate::Element::Item);
        let child = scene.create(crate::Element::Item);
        scene.reparent(child, Some(root)).unwrap();
        assert!(!scene.is_rtl(child));
        scene
            .assign(root, "layout_direction", crate::Value::String("rtl".into()))
            .unwrap();
        assert!(scene.is_rtl(child));
        scene
            .assign(
                child,
                "layout_direction",
                crate::Value::String("ltr".into()),
            )
            .unwrap();
        assert!(!scene.is_rtl(child));
    }
}
