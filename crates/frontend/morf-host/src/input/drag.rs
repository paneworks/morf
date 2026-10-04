//! Offers on their way to the configuration: the clipboard, drags, reads.
//!
//! The clipboard needs nothing but the runtime. A drag needs the layout too —
//! which `DropArea` is under it decides whether it is accepted at all — and
//! the client, which has to be told that answer while the drag is still
//! moving, and told when a drop is done with.

use morf_lua::{ClipboardRequest, EventPoint, OfferDescription, Runtime};
use morf_scene::NodeHandle;
use morf_app::mime::{
    TEXT_MIMES, URI_LIST_MIME, accept_mime, encode_uri_list, path_to_uri, uri_to_path,
};
use morf_app::{Event, LayerClient, OfferInfo, WindowId};
use morf_desktop::Desktop;
use std::sync::Arc;

use crate::surfaces::*;

/// A drag from elsewhere, as far as the shell has followed it.
pub struct DragFollow {
    pub surface: WindowId,
    pub offer: OfferInfo,
    /// The `DropArea` under it, and the type that area accepted.
    pub target: Option<(NodeHandle, Option<String>)>,
}

fn description(offer: &OfferInfo) -> OfferDescription {
    OfferDescription {
        id: offer.id,
        mime_types: offer.mime_types.clone(),
        ..OfferDescription::default()
    }
}

/// Handles the events this module owns, and hands every other one back.
pub fn handle_data_event(
    runtime: &mut Runtime,
    client: &mut LayerClient,
    desktop: &mut Desktop,
    state: &mut SurfaceEventState,
    event: Event,
) -> Result<Result<bool, String>, Event> {
    let repaint = match event {
        Event::OfferRead { request_id, result } => {
            runtime.dispatch_offer_read(request_id, result)
        }
        Event::DragSourceEnded { dropped } => runtime.dispatch_drag_ended(dropped),
        Event::DragEnter {
            surface,
            x,
            y,
            offer,
        } => {
            let mut repaint = leave_target(runtime, state);
            state.drag = Some(DragFollow {
                surface,
                offer,
                target: None,
            });
            repaint |= match follow_drag(runtime, client, state, x, y) {
                Ok(repaint) => repaint,
                Err(error) => return Ok(Err(error)),
            };
            repaint
        }
        Event::DragMotion { surface, x, y } => {
            if state
                .drag
                .as_ref()
                .is_none_or(|drag| drag.surface != surface)
            {
                return Ok(Ok(false));
            }
            match follow_drag(runtime, client, state, x, y) {
                Ok(repaint) => repaint,
                Err(error) => return Ok(Err(error)),
            }
        }
        Event::DragLeave { .. } => {
            let repaint = leave_target(runtime, state);
            state.drag = None;
            repaint
        }
        Event::Drop {
            surface,
            x,
            y,
            drop,
        } => {
            let mut repaint = false;
            let target = state
                .drag
                .as_ref()
                .filter(|drag| drag.surface == surface && drag.offer.id == drop.offer.id)
                .and_then(|drag| drag.target.clone());
            if let Some((node, accepted)) = target
                && accepted.is_some()
            {
                let local = surface_layout(
                    surface,
                    &state.layout,
                    &state.popup_surfaces,
                    &state.floating_surfaces,
                    &state.layer_surfaces,
                )
                .map(|layout| layout.local_point(&runtime.scene(), node, x, y))
                .unwrap_or((x, y));
                let paths = drop
                    .uris
                    .iter()
                    .filter_map(|uri| uri_to_path(uri))
                    .collect();
                let dropped = OfferDescription {
                    id: drop.offer.id,
                    mime_types: drop.offer.mime_types.clone(),
                    accepted: drop.accepted.clone().or(accepted),
                    uris: drop.uris.clone(),
                    paths,
                    text: drop.text.clone(),
                };
                repaint |= runtime.dispatch_dropped(node, EventPoint::new((x, y), local), &dropped);
            }
            repaint |= leave_target(runtime, state);
            state.drag = None;
            // Reads the drop handler asked for go out before `finish`, which
            // is the last moment the offer may still be read.
            apply_offer_reads(runtime, client, desktop);
            client.finish_drop();
            repaint
        }
        other => return Err(other),
    };
    Ok(Ok(repaint))
}

/// Re-hit-tests a moving drag and tells the areas it crossed.
fn follow_drag(
    runtime: &mut Runtime,
    client: &mut LayerClient,
    state: &mut SurfaceEventState,
    x: f64,
    y: f64,
) -> Result<bool, String> {
    let Some(drag) = &state.drag else {
        return Ok(false);
    };
    let hit = match surface_layout(
        drag.surface,
        &state.layout,
        &state.popup_surfaces,
        &state.floating_surfaces,
        &state.layer_surfaces,
    ) {
        Some(layout) => layout
            .drop_hit_test(&runtime.scene(), x, y)
            .map_err(|error| error.to_string())?,
        None => None,
    };
    let mut repaint = false;
    let current = drag.target.as_ref().map(|(node, _)| *node);
    if hit.map(|hit| hit.node) != current {
        repaint |= leave_target(runtime, state);
        if let Some(hit) = hit
            && let Some(drag) = &mut state.drag
        {
            let accepted = accept_mime(&runtime.drop_area_keys(hit.node), &drag.offer.mime_types);
            drag.target = Some((hit.node, accepted.clone()));
            let mut offer = description(&drag.offer);
            offer.accepted = accepted;
            repaint |= runtime.dispatch_drag_entered(
                hit.node,
                EventPoint::new((x, y), (hit.local_x, hit.local_y)),
                &offer,
            );
        }
    }
    if let Some(hit) = hit {
        repaint |= runtime.dispatch_drag_moved(
            hit.node,
            EventPoint::new((x, y), (hit.local_x, hit.local_y)),
        );
    }
    let accepted = state
        .drag
        .as_ref()
        .and_then(|drag| drag.target.as_ref())
        .and_then(|(_, accepted)| accepted.clone());
    client.accept_drag(accepted.as_deref());
    Ok(repaint)
}

/// Tells the area under the drag that it left, if there was one.
fn leave_target(runtime: &mut Runtime, state: &mut SurfaceEventState) -> bool {
    match state.drag.as_mut().and_then(|drag| drag.target.take()) {
        Some((node, _)) => runtime.dispatch_drag_exited(node),
        None => false,
    }
}

/// Sends every read the configuration asked for.
/// Starts the offer reads the configuration asked for, each on whichever
/// side announced its offer: a drag's is the window client's, a selection's
/// the desktop's.
pub fn apply_offer_reads(
    runtime: &mut Runtime,
    client: &mut LayerClient,
    desktop: &mut Desktop,
) {
    for read in runtime.take_offer_reads() {
        if desktop.owns_offer(read.offer) {
            desktop.read_offer(read.id, read.offer, &read.mime);
        } else {
            client.read_offer(read.id, read.offer, &read.mime);
        }
    }
}

/// What one clipboard request offers: each type with its bytes.
pub fn clipboard_payload(request: ClipboardRequest) -> Vec<(String, Arc<Vec<u8>>)> {
    let bytes = Arc::new(request.data);
    match request.mime {
        Some(mime) => vec![(mime, bytes)],
        None => TEXT_MIMES
            .iter()
            .map(|mime| ((*mime).to_owned(), Arc::clone(&bytes)))
            .collect(),
    }
}

/// Starts the drags out the configuration asked for; the last one wins.
pub fn apply_drag_requests(runtime: &mut Runtime, client: &mut LayerClient) {
    let Some(request) = runtime.take_drag_requests().pop() else {
        return;
    };
    let mut data = Vec::new();
    let mut uris = request.uris;
    uris.extend(request.paths.iter().map(|path| path_to_uri(path)));
    if !uris.is_empty() {
        data.push((
            URI_LIST_MIME.to_owned(),
            Arc::new(encode_uri_list(&uris).into_bytes()),
        ));
    }
    if let Some(text) = request.text {
        let bytes = Arc::new(text.into_bytes());
        data.extend(
            TEXT_MIMES
                .iter()
                .map(|mime| ((*mime).to_owned(), Arc::clone(&bytes))),
        );
    }
    data.extend(
        request
            .data
            .into_iter()
            .map(|(mime, bytes)| (mime, Arc::new(bytes))),
    );
    if !client.start_drag(data) {
        // No press to start it from: said now, rather than never.
        runtime.dispatch_drag_ended(false);
    }
}
