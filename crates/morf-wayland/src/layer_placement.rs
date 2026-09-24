//! Where a layer surface lands when there is no layer-shell to put it there.
//!
//! Without `wlr-layer-shell` the primary surface stands in as a fullscreen
//! toplevel and every other layer surface becomes a `wl_subsurface` of it
//! (see `ShellSurface::Subsurface`). The compositor then places nothing: the
//! anchors, margins, size and exclusive zones a configuration asked for have
//! to be turned into a position inside the primary by morf itself. This module
//! is that arithmetic, kept free of protocol objects so it can be tested.
//!
//! The rules are wlroots' (`wlr_scene_layer_surface_v1_configure` and the
//! exclusive-zone helper, which sway and Hyprland share in substance), so a
//! surface lands where a wlroots compositor with layer-shell would put it on an
//! output the size of the primary surface.

use crate::types::{BarConfig, LayerAnchors, ShellLayer};

/// The part of a layer surface's configuration that decides where it goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LayerRequest {
    pub(crate) anchors: LayerAnchors,
    /// Requested width; zero stretches across the bounds.
    pub(crate) width: u32,
    /// Requested height; zero stretches across the bounds.
    pub(crate) height: u32,
    pub(crate) margin_top: i32,
    pub(crate) margin_right: i32,
    pub(crate) margin_bottom: i32,
    pub(crate) margin_left: i32,
    /// Positive reserves an edge, zero respects others' reservations, `-1`
    /// ignores them.
    pub(crate) exclusive_zone: i32,
    pub(crate) layer: ShellLayer,
}

impl LayerRequest {
    pub(crate) fn from_config(config: &BarConfig) -> Self {
        Self {
            anchors: config.anchors,
            width: config.width,
            height: config.height,
            margin_top: config.margin_top,
            margin_right: config.margin_right,
            margin_bottom: config.margin_bottom,
            margin_left: config.margin_left,
            exclusive_zone: config.exclusive_zone,
            layer: config.layer,
        }
    }
}

/// A rectangle in the primary surface's (the stand-in output's) coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Placement {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// A signed working rectangle: an area can shrink past zero while zones are
/// taken out of it, and that must not wrap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Area {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// Stacking rank of a layer, lowest first.
pub(crate) fn layer_rank(layer: ShellLayer) -> u8 {
    match layer {
        ShellLayer::Background => 0,
        ShellLayer::Bottom => 1,
        ShellLayer::Top => 2,
        ShellLayer::Overlay => 3,
    }
}

/// Whether a layer stacks under the shell's own content.
///
/// A subsurface can only go below or above its parent, and the parent is the
/// fullscreen primary, so "background" and "bottom" land under it (visible
/// only where the primary is transparent) and "top" and "overlay" above it.
pub(crate) fn stacks_below_primary(layer: ShellLayer) -> bool {
    layer_rank(layer) < layer_rank(ShellLayer::Top)
}

/// Places one surface inside `bounds`, as wlroots does.
fn place(bounds: Area, request: &LayerRequest) -> Area {
    let anchors = request.anchors;
    let (x, width) = place_axis(
        bounds.x,
        bounds.width,
        request.width,
        anchors.left,
        anchors.right,
        request.margin_left,
        request.margin_right,
    );
    let (y, height) = place_axis(
        bounds.y,
        bounds.height,
        request.height,
        anchors.top,
        anchors.bottom,
        request.margin_top,
        request.margin_bottom,
    );
    Area {
        x,
        y,
        width,
        height,
    }
}

/// One axis of [`place`]: `start` and `end` are the anchors at the low and the
/// high edge, the margins likewise.
fn place_axis(
    origin: i32,
    extent: i32,
    desired: u32,
    start: bool,
    end: bool,
    margin_start: i32,
    margin_end: i32,
) -> (i32, i32) {
    let desired = i32::try_from(desired).unwrap_or(i32::MAX);
    let (mut position, mut size) = if desired == 0 {
        (origin, extent)
    } else if start && end {
        (origin + (extent / 2 - desired / 2), desired)
    } else if start {
        (origin, desired)
    } else if end {
        (origin + (extent - desired), desired)
    } else {
        (origin + (extent / 2 - desired / 2), desired)
    };
    if start && end {
        position += margin_start;
        size -= margin_start + margin_end;
    } else if start {
        position += margin_start;
    } else if end {
        position -= margin_end;
    }
    (position, size)
}

/// Takes a surface's exclusive zone out of the usable area.
///
/// Only a surface anchored to one edge, or to one edge and both of its
/// neighbours, reserves anything; the reservation is the zone plus the margin
/// on that edge.
fn apply_exclusive(usable: &mut Area, request: &LayerRequest) {
    if request.exclusive_zone <= 0 {
        return;
    }
    let a = request.anchors;
    let horizontal = a.left == a.right;
    let vertical = a.top == a.bottom;
    let zone = request.exclusive_zone;
    if a.top && !a.bottom && horizontal {
        let taken = zone + request.margin_top;
        usable.y += taken;
        usable.height -= taken;
    } else if a.bottom && !a.top && horizontal {
        usable.height -= zone + request.margin_bottom;
    } else if a.left && !a.right && vertical {
        let taken = zone + request.margin_left;
        usable.x += taken;
        usable.width -= taken;
    } else if a.right && !a.left && vertical {
        usable.width -= zone + request.margin_right;
    }
}

/// Arranges every surface on an output of `size`, the way layer-shell would.
///
/// `surfaces` is `(id, request)` in creation order. Surfaces that reserve an
/// edge go first, from the overlay layer down, each taking its zone out of the
/// area the next one gets; the rest are then placed from the overlay layer
/// down in what is left, or in the whole output when their zone is `-1`.
/// Sizes that come out below one pixel are clamped to one.
pub(crate) fn arrange(size: (u32, u32), surfaces: &[(u64, LayerRequest)]) -> Vec<(u64, Placement)> {
    let full = Area {
        x: 0,
        y: 0,
        width: i32::try_from(size.0).unwrap_or(i32::MAX),
        height: i32::try_from(size.1).unwrap_or(i32::MAX),
    };
    let mut usable = full;
    let mut placed = Vec::with_capacity(surfaces.len());
    for exclusive in [true, false] {
        for rank in (0..=3).rev() {
            for (id, request) in surfaces {
                if layer_rank(request.layer) != rank || (request.exclusive_zone > 0) != exclusive {
                    continue;
                }
                let bounds = if request.exclusive_zone == -1 {
                    full
                } else {
                    usable
                };
                let area = place(bounds, request);
                apply_exclusive(&mut usable, request);
                placed.push((
                    *id,
                    Placement {
                        x: area.x,
                        y: area.y,
                        width: area.width.max(1) as u32,
                        height: area.height.max(1) as u32,
                    },
                ));
            }
        }
    }
    placed
}

/// The order subsurfaces stack in, bottom first: by layer, then by creation.
///
/// `surfaces` is `(id, layer, sequence)`; the sequence breaks ties within a
/// layer, later on top, as a compositor stacks surfaces mapped later.
pub(crate) fn stacking(surfaces: &[(u64, ShellLayer, u64)]) -> Vec<u64> {
    let mut order = surfaces.to_vec();
    order.sort_by_key(|(_, layer, sequence)| (layer_rank(*layer), *sequence));
    order.into_iter().map(|(id, _, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors(top: bool, right: bool, bottom: bool, left: bool) -> LayerAnchors {
        LayerAnchors {
            top,
            right,
            bottom,
            left,
        }
    }

    fn request(anchors: LayerAnchors, width: u32, height: u32) -> LayerRequest {
        LayerRequest {
            anchors,
            width,
            height,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            exclusive_zone: 0,
            layer: ShellLayer::Top,
        }
    }

    fn one(request: LayerRequest) -> Placement {
        arrange((1000, 800), &[(1, request)])[0].1
    }

    fn at(x: i32, y: i32, width: u32, height: u32) -> Placement {
        Placement {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_dock_anchored_bottom_left_right_sits_bottom_centre() {
        // Impasto's dock: a fixed size, anchored to three edges.
        let dock = request(anchors(false, true, true, true), 60, 68);
        assert_eq!(one(dock), at(470, 732, 60, 68));
    }

    #[test]
    fn each_edge_and_corner_lands_where_layer_shell_puts_it() {
        let cases = [
            (anchors(true, false, false, false), at(450, 0, 100, 50)),
            (anchors(false, false, true, false), at(450, 750, 100, 50)),
            (anchors(false, false, false, true), at(0, 375, 100, 50)),
            (anchors(false, true, false, false), at(900, 375, 100, 50)),
            (anchors(true, false, false, true), at(0, 0, 100, 50)),
            (anchors(true, true, false, false), at(900, 0, 100, 50)),
            (anchors(false, false, true, true), at(0, 750, 100, 50)),
            (anchors(false, true, true, false), at(900, 750, 100, 50)),
            (anchors(false, false, false, false), at(450, 375, 100, 50)),
        ];
        for (edges, expected) in cases {
            assert_eq!(one(request(edges, 100, 50)), expected, "{edges:?}");
        }
    }

    #[test]
    fn margins_push_away_from_the_anchored_edge() {
        let mut corner = request(anchors(false, true, true, false), 100, 50);
        corner.margin_right = 10;
        corner.margin_bottom = 20;
        // Margins on edges it is not anchored to are ignored.
        corner.margin_left = 99;
        corner.margin_top = 99;
        assert_eq!(one(corner), at(890, 730, 100, 50));

        let mut top_left = request(anchors(true, false, false, true), 100, 50);
        top_left.margin_top = 7;
        top_left.margin_left = 9;
        assert_eq!(one(top_left), at(9, 7, 100, 50));
    }

    #[test]
    fn zero_size_with_opposite_anchors_stretches_inside_the_margins() {
        let mut bar = request(anchors(true, true, false, true), 0, 32);
        bar.margin_left = 8;
        bar.margin_right = 12;
        bar.margin_top = 4;
        assert_eq!(one(bar), at(8, 4, 980, 32));

        let full = request(anchors(true, true, true, true), 0, 0);
        assert_eq!(one(full), at(0, 0, 1000, 800));
    }

    #[test]
    fn both_anchors_and_a_size_centre_then_apply_margins_like_wlroots() {
        let mut centred = request(anchors(false, true, false, true), 200, 40);
        centred.margin_left = 10;
        centred.margin_right = 30;
        // Centred at 400, shifted by the left margin and shrunk by both.
        assert_eq!(one(centred), at(410, 380, 160, 40));
    }

    #[test]
    fn exclusive_zones_shrink_what_later_surfaces_get() {
        let mut bar = request(anchors(true, true, false, true), 0, 30);
        bar.exclusive_zone = 30;
        bar.margin_top = 5;
        let mut side = request(anchors(true, false, true, true), 40, 0);
        side.exclusive_zone = 0;
        let mut ignoring = request(anchors(true, false, true, true), 40, 0);
        ignoring.exclusive_zone = -1;
        // The side panel is created first, yet the bar's reservation still
        // applies to it: reservers are arranged before everything else.
        let placed = arrange((1000, 800), &[(1, side), (2, bar), (3, ignoring)]);
        let find = |id| placed.iter().find(|(i, _)| *i == id).unwrap().1;
        assert_eq!(find(2), at(0, 5, 1000, 30));
        assert_eq!(find(1), at(0, 35, 40, 765));
        assert_eq!(find(3), at(0, 0, 40, 800));
    }

    #[test]
    fn a_corner_anchor_reserves_nothing() {
        let mut corner = request(anchors(true, false, false, true), 50, 50);
        corner.exclusive_zone = 50;
        let later = request(anchors(true, true, false, true), 0, 10);
        let placed = arrange((1000, 800), &[(1, corner), (2, later)]);
        assert_eq!(placed[1].1, at(0, 0, 1000, 10));
    }

    #[test]
    fn higher_layers_reserve_first() {
        let mut bottom = request(anchors(true, true, false, true), 0, 20);
        bottom.exclusive_zone = 20;
        bottom.layer = ShellLayer::Bottom;
        let mut overlay = request(anchors(true, true, false, true), 0, 10);
        overlay.exclusive_zone = 10;
        overlay.layer = ShellLayer::Overlay;
        let placed = arrange((1000, 800), &[(1, bottom), (2, overlay)]);
        let find = |id| placed.iter().find(|(i, _)| *i == id).unwrap().1;
        assert_eq!(find(2), at(0, 0, 1000, 10));
        assert_eq!(find(1), at(0, 10, 1000, 20));
    }

    #[test]
    fn a_degenerate_size_is_clamped_to_one_pixel() {
        let mut squeezed = request(anchors(false, true, false, true), 0, 10);
        squeezed.margin_left = 600;
        squeezed.margin_right = 600;
        assert_eq!(one(squeezed).width, 1);
    }

    #[test]
    fn stacking_orders_by_layer_then_creation() {
        let order = stacking(&[
            (1, ShellLayer::Overlay, 1),
            (2, ShellLayer::Background, 5),
            (3, ShellLayer::Top, 3),
            (4, ShellLayer::Top, 2),
            (5, ShellLayer::Bottom, 4),
        ]);
        assert_eq!(order, vec![2, 5, 4, 3, 1]);
        assert!(stacks_below_primary(ShellLayer::Bottom));
        assert!(stacks_below_primary(ShellLayer::Background));
        assert!(!stacks_below_primary(ShellLayer::Top));
        assert!(!stacks_below_primary(ShellLayer::Overlay));
    }
}
