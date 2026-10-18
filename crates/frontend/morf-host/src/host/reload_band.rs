//! The engine's reload-error band on the primary surface: laid out over the
//! surface on its own, painted over the configuration's scene, and taken
//! down by a press on that surface.

use morf_app::{Event, InputRect, PRIMARY_LAYER, WindowId};
use morf_layout::TextMeasurer;
use morf_lua::{BandLayout, Runtime};
use morf_paint::Overlay;
use morf_value::region::{Region, Shape};

/// The band for a surface of `size`: `previous` while nothing it was laid
/// out from moved, else laid out again; nothing while no band is shown.
pub fn band_for(
    runtime: &mut Runtime,
    previous: Option<&BandLayout>,
    size: (u32, u32),
    text: &mut impl TextMeasurer,
) -> Option<BandLayout> {
    let root = runtime.reload_band_root()?;
    if let Some(previous) = previous
        && previous.root == root
        && previous.laid == (runtime.scene().layout_revision_of(root), size)
    {
        return Some(previous.clone());
    }
    runtime.lay_out_reload_band(size, text)
}

/// The band as the renderer paints it, over everything else.
pub fn band_overlay(band: Option<&BandLayout>) -> Option<Overlay<'_>> {
    band.map(|band| Overlay {
        root: band.root,
        layout: &band.layout,
        conceals: false,
    })
}

/// The band's box, in whole pixels, as the input region takes it.
pub fn band_input(band: Option<&BandLayout>) -> Option<InputRect> {
    let bounds = band?.bounds;
    let (left, top) = (bounds.x.floor() as i32, bounds.y.floor() as i32);
    Some(InputRect {
        x: left,
        y: top,
        width: (bounds.x + bounds.width).ceil() as i32 - left,
        height: (bounds.y + bounds.height).ceil() as i32 - top,
    })
}

/// The band's box as one shape of a configured input mask.
pub fn band_region(rect: InputRect) -> Region {
    Region {
        rect,
        shape: Shape::Box,
        ..Region::default()
    }
}

/// A press on the primary surface takes the band down. Returns `None` when
/// the event did not, and otherwise whether the press landed on the band,
/// which it then goes no further than.
pub fn dismiss_reload_band(
    runtime: &mut Runtime,
    band: Option<&BandLayout>,
    event: &Event,
) -> Option<bool> {
    let Event::PointerButton {
        surface: WindowId::Layer(PRIMARY_LAYER),
        pressed: true,
        x,
        y,
        ..
    } = *event
    else {
        return None;
    };
    if !runtime.reload_error_shown() {
        return None;
    }
    runtime.clear_reload_error();
    let on_band = band.is_some_and(|band| {
        let bounds = band.bounds;
        (bounds.x..bounds.x + bounds.width).contains(&x)
            && (bounds.y..bounds.y + bounds.height).contains(&y)
    });
    Some(on_band)
}
