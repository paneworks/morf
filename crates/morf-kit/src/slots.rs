//! The slots each archetype offers a skin: named places a skin fills with
//! node trees. `background` and `content` are every control's; each
//! archetype adds its own. A slot a skin leaves empty gets the theme's
//! default, built only when wanted.

/// Every archetype the kit knows, `Control` (the bare base) first.
pub const ARCHETYPES: &[&str] = &[
    "Control",
    "Press",
    "Range",
    "Plane",
    "Selection",
    "Popup",
    "TextField",
    "Scroll",
    "Collection",
    "Disclosure",
    "Drag",
    "Navigation",
    "Shell",
];

/// An archetype's slots, in the order they are stacked: the first lowest.
pub fn slots_of(archetype: &str) -> Option<&'static [&'static str]> {
    Some(match archetype {
        "Control" => &["background", "content"],
        "Press" => &[
            "background",
            "content",
            "indicator",
            "icon",
            "label",
            "badge",
        ],
        "Range" => &[
            "background",
            "track",
            "fill",
            "ticks",
            "handle",
            "second_handle",
            "value_label",
            "increase",
            "decrease",
            "content",
        ],
        "Plane" => &["background", "field", "crosshair", "handle", "content"],
        // `item`, `place` and `container` are builders, not nodes: an
        // entry's look, its area's properties, and what the entries go in.
        "Selection" => &[
            "background",
            "indicator",
            "item",
            "place",
            "container",
            "separator",
            "content",
        ],
        "Popup" => &["dim", "background", "content", "enter", "exit"],
        "TextField" => &[
            "background",
            "field",
            "placeholder",
            "leading",
            "trailing",
            "counter",
            "error",
            "content",
        ],
        "Scroll" => &[
            "background",
            "content",
            "edge_fade",
            "overscroll",
            "scroll_bar_x",
            "scroll_bar_y",
        ],
        "Collection" => &[
            "background",
            "header",
            "section_header",
            "row",
            "placeholder_row",
            "footer",
            "empty",
            "loading",
            "content",
        ],
        "Disclosure" => &["background", "header", "indicator", "content"],
        "Drag" => &["background", "handle", "ghost", "drop_indicator", "content"],
        "Navigation" => &[
            "background",
            "page",
            "transition",
            "back",
            "indicator",
            "content",
        ],
        "Shell" => &[
            "background",
            "header_bar",
            "toolbar_top",
            "banner",
            "sidebar",
            "content",
            "inspector",
            "toolbar_bottom",
            "bottom_bar",
            "toasts",
        ],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_archetype_has_a_background_and_content_or_its_own() {
        for archetype in ARCHETYPES {
            let slots = slots_of(archetype).expect(archetype);
            assert!(
                slots.contains(&"content") || slots.contains(&"page"),
                "{archetype}"
            );
        }
    }
}
