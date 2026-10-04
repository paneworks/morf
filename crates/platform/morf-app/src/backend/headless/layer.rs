//! How big a layer surface is on a headless output, and where it sits: what
//! a compositor decides from its anchors, margins and the output under it.

use crate::{LayerAnchors, ShellLayer};

/// A layer surface's extent along one axis: the output's when anchored to
/// both edges and asking for no size, what it asked for otherwise, and its
/// content's (at most the output's) when it asked for nothing.
pub fn layer_extent(near: bool, far: bool, asked: u32, output: u32, content: u32) -> u32 {
    // Layer shell: a size of zero on an axis anchored at both ends is the
    // output's extent; a size given is kept, centred between the anchors,
    // as a compositor does (a layer that forgets `height = 0` shows as a
    // 32 px band on screen, and must show as one here too).
    if near && far && asked == 0 {
        return output;
    }
    if asked == 0 {
        return content.min(output).max(1);
    }
    asked
}

/// Where a layer surface of `size` sits on an output, from its anchors and
/// margins (top, right, bottom, left): centred on an axis it is anchored to
/// neither or both ends of.
pub fn layer_position(
    anchors: LayerAnchors,
    margins: (i32, i32, i32, i32),
    size: (u32, u32),
    output: (u32, u32),
) -> (i32, i32) {
    let (top, right, bottom, left) = margins;
    let along = |near: bool, far: bool, size: u32, full: u32, before: i32, after: i32| {
        let free = i64::from(full) - i64::from(size);
        let at = match (near, far) {
            (true, false) => i64::from(before),
            (false, true) => free - i64::from(after),
            _ => free / 2,
        };
        at.clamp(0, free.max(0)) as i32
    };
    (
        along(anchors.left, anchors.right, size.0, output.0, left, right),
        along(anchors.top, anchors.bottom, size.1, output.1, top, bottom),
    )
}

/// A layer's place in the stack, bottom first.
pub fn layer_stack(layer: ShellLayer) -> u8 {
    crate::placement::layer_rank(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_anchored_at_both_ends_stretches_only_when_it_asks_for_no_size() {
        assert_eq!(layer_extent(true, true, 0, 1080, 40), 1080);
        // The 32 px a layer gets by default stays 32 px, as on a compositor.
        assert_eq!(layer_extent(true, true, 32, 1080, 40), 32);
        assert_eq!(layer_extent(true, false, 0, 1080, 40), 40);
        assert_eq!(layer_extent(false, false, 300, 1080, 40), 300);
    }

    #[test]
    fn a_layer_sits_at_its_margin_from_the_one_edge_it_is_anchored_to() {
        let bottom = LayerAnchors { top: false, right: false, bottom: true, left: false };
        assert_eq!(layer_position(bottom, (0, 0, 10, 0), (100, 40), (1000, 500)), (450, 450));
    }
}
