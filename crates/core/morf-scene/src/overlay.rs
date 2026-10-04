//! Where an overlay goes: beside the node that opened it, flipped to the
//! other side when its own side has no room, and shifted along to stay on
//! the surface -- or centred, with no node to go beside.

/// A box on a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Which side of its anchor an overlay sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
    /// Over the anchor's centre, or the surface's with no anchor.
    Center,
}

/// How an overlay lines up along its side: its start edge with the anchor's,
/// centred on it, or its end edge with the anchor's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// A side and an alignment: `"bottom-start"`, `"right"`, `"center"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub side: Side,
    pub align: Align,
}

impl Placement {
    /// Parses `"bottom"`, `"top-end"`, `"left-start"`, `"center"`.
    pub fn parse(text: &str) -> Option<Self> {
        let (side, align) = text.split_once('-').unwrap_or((text, "center"));
        let side = match side {
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            "left" => Side::Left,
            "right" => Side::Right,
            "center" => Side::Center,
            _ => return None,
        };
        let align = match align {
            "start" => Align::Start,
            "center" => Align::Center,
            "end" => Align::End,
            _ => return None,
        };
        Some(Self { side, align })
    }
}

fn along(start: f64, length: f64, size: f64, align: Align) -> f64 {
    match align {
        Align::Start => start,
        Align::Center => start + (length - size) / 2.0,
        Align::End => start + length - size,
    }
}

/// Keeps `[at, at + size]` inside `[low, high]`, or at `low` when it cannot
/// fit at all.
fn shift(at: f64, size: f64, low: f64, high: f64) -> f64 {
    at.min(high - size).max(low)
}

/// Where an overlay of `size` goes: beside `anchor` by `placement`, `gap`
/// away, inside `surface` less `margin`. Returns its top left, and the
/// side it ended on.
pub fn place(
    anchor: Option<Bounds>,
    size: (f64, f64),
    surface: Bounds,
    placement: Placement,
    gap: f64,
    margin: f64,
) -> ((f64, f64), Side) {
    let (width, height) = size;
    let (left, top) = (surface.x + margin, surface.y + margin);
    let (right, bottom) = (
        surface.x + surface.width - margin,
        surface.y + surface.height - margin,
    );
    let Some(a) = anchor else {
        let x = surface.x + (surface.width - width) / 2.0;
        let y = surface.y + (surface.height - height) / 2.0;
        return (
            (shift(x, width, left, right), shift(y, height, top, bottom)),
            Side::Center,
        );
    };
    // Room on each side of the anchor.
    let room = |side: Side| match side {
        Side::Top => a.y - gap - top,
        Side::Bottom => bottom - (a.y + a.height + gap),
        Side::Left => a.x - gap - left,
        Side::Right => right - (a.x + a.width + gap),
        Side::Center => f64::INFINITY,
    };
    let need = |side: Side| match side {
        Side::Top | Side::Bottom => height,
        Side::Left | Side::Right => width,
        Side::Center => 0.0,
    };
    let opposite = |side: Side| match side {
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
        Side::Left => Side::Right,
        Side::Right => Side::Left,
        Side::Center => Side::Center,
    };
    let mut side = placement.side;
    if room(side) < need(side) && room(opposite(side)) > room(side) {
        side = opposite(side);
    }
    let (x, y) = match side {
        Side::Top => (
            along(a.x, a.width, width, placement.align),
            a.y - gap - height,
        ),
        Side::Bottom => (
            along(a.x, a.width, width, placement.align),
            a.y + a.height + gap,
        ),
        Side::Left => (
            a.x - gap - width,
            along(a.y, a.height, height, placement.align),
        ),
        Side::Right => (
            a.x + a.width + gap,
            along(a.y, a.height, height, placement.align),
        ),
        Side::Center => (
            a.x + (a.width - width) / 2.0,
            a.y + (a.height - height) / 2.0,
        ),
    };
    (
        (shift(x, width, left, right), shift(y, height, top, bottom)),
        side,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 400.0,
        height: 300.0,
    };

    fn placement(text: &str) -> Placement {
        Placement::parse(text).unwrap()
    }

    #[test]
    fn sits_below_its_anchor_lined_up_at_the_start() {
        let anchor = Bounds {
            x: 50.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        };
        let (at, side) = place(
            Some(anchor),
            (120.0, 60.0),
            SURFACE,
            placement("bottom-start"),
            4.0,
            8.0,
        );
        assert_eq!((at, side), ((50.0, 54.0), Side::Bottom));
    }

    #[test]
    fn flips_when_its_side_has_no_room_and_shifts_onto_the_surface() {
        let anchor = Bounds {
            x: 340.0,
            y: 260.0,
            width: 50.0,
            height: 30.0,
        };
        let (at, side) = place(
            Some(anchor),
            (120.0, 60.0),
            SURFACE,
            placement("bottom-start"),
            4.0,
            8.0,
        );
        assert_eq!(side, Side::Top);
        assert_eq!(at, (272.0, 196.0));
    }

    #[test]
    fn centres_with_no_anchor() {
        let (at, side) = place(None, (100.0, 50.0), SURFACE, placement("bottom"), 4.0, 8.0);
        assert_eq!((at, side), ((150.0, 125.0), Side::Center));
    }
}
