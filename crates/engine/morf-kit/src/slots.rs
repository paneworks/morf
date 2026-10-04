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
    "Canvas",
    "Dock",
    "Transform",
    "Sheet",
    "Roving",
    "Form",
    "Overflow",
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
        // The world's layers, lowest first: a grid, the items (a builder:
        // the configuration's, or a skin's for a kind of item), the wires,
        // the selection's outline and handles, the draft, the band, a
        // crosshair under the pointer, and overlaid controls (zoom buttons,
        // a minimap, a scale).
        // `tab`, `stack` and `floating` are builders: a tab's look, a
        // stack's frame round its tabs and panel, a floating panel's frame.
        // `divider` is a builder too: the handle between a split's parts.
        // The box's ground, its frame, the handles (a builder: one per
        // handle name), the turning handle, a guide while it moves.
        "Transform" => &[
            "background",
            "frame",
            "handle",
            "rotate_handle",
            "guide",
            "content",
        ],
        // `cell` and `header` are builders: a cell's look, a row's or a
        // column's header; `cursor` the current cell's ring, `range` the
        // selected range, `editor` the field an edit happens in.
        "Sheet" => &[
            "background",
            "cell",
            "header",
            "range",
            "cursor",
            "editor",
            "content",
        ],
        // The group's ground and the ring round the member with focus.
        "Roving" => &["background", "indicator", "separator", "content"],
        // A form's ground, its summary of what is wrong, and a field's
        // message (a builder: name, message).
        "Form" => &["background", "summary", "message", "content"],
        // The "more" button and its menu's look.
        "Overflow" => &["background", "more", "menu", "content"],
        "Dock" => &[
            "background",
            "tab",
            "stack",
            "divider",
            "floating",
            "drop_indicator",
            "content",
        ],
        "Canvas" => &[
            "background",
            "grid",
            "content",
            "item",
            "wires",
            "selection",
            "draft",
            "band",
            "crosshair",
            "overlay",
            // A builder: one handle of the selected box (`s.name`: n, ne,
            // e, se, s, sw, w, nw; `s.hovered()`, `s.held()`).
            "grip",
        ],
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
        // `row`, `cell` and `header` are builders: a row's look, a table
        // cell's, a column header's.
        "Collection" => &[
            "background",
            "header",
            "section_header",
            "row",
            "cell",
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
