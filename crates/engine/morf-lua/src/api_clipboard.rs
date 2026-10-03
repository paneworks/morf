//! `morf.clipboard` and `morf.drag`: what is on offer, and taking it.
//!
//! Two sources of offers share one shape. The clipboard announces a new offer
//! every time anything is copied, anywhere; a drag announces one when it comes
//! over a `ui.DropArea`. Either way a configuration is handed a table naming
//! the offer and listing its types, and nothing has been read yet — a
//! clipboard history that only keeps text should not pay for every image
//! somebody copies. `offer:read(mime, callback)` fetches one type, off the
//! loop, and calls back with the bytes.

use luna::{Callback, CallbackReturn, Closure, Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime_input::EventPoint;
use crate::{scene_bindings::*, state::*, surface_types::*};

/// The most bytes one `set` may hand the compositor.
const MAX_SET_BYTES: usize = 32 * 1024 * 1024;
/// How many reads may wait for an answer at once.
const MAX_PENDING_READS: usize = 16;
/// How many types a drag out may offer.
const MAX_DRAG_TYPES: usize = 16;

pub(crate) fn install_clipboard_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let set_state = Rc::clone(&state);
    // `set(data, mime)` or `set(data, { mime = ..., primary = true })`. With
    // no type the data is text, and is offered under every name text goes by.
    let clipboard_set = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (data, options): (LuaValue, LuaValue) = stack.consume(ctx)?;
        let LuaValue::String(data) = data else {
            return Err(HostError("clipboard.set wants a string".into()).into());
        };
        let data = data.as_bytes().to_vec();
        if data.len() > MAX_SET_BYTES {
            return Err(HostError("clipboard data limit reached".into()).into());
        }
        let (mime, primary) = match options {
            LuaValue::Nil => (None, false),
            LuaValue::String(mime) => (Some(mime.display_lossy().to_string()), false),
            LuaValue::Table(options) => {
                let mime = match options.get_value(ctx, "mime") {
                    LuaValue::Nil => None,
                    LuaValue::String(mime) => Some(mime.display_lossy().to_string()),
                    _ => return Err(HostError("clipboard mime must be a string".into()).into()),
                };
                let primary = matches!(options.get_value(ctx, "primary"), LuaValue::Boolean(true));
                (mime, primary)
            }
            _ => {
                return Err(HostError(
                    "clipboard.set options must be a mime string or a table".into(),
                )
                .into());
            }
        };
        if mime
            .as_deref()
            .is_some_and(|mime| mime.is_empty() || mime.len() > 256)
        {
            return Err(HostError("clipboard mime must be 1..256 bytes".into()).into());
        }
        let mut state = set_state.borrow_mut();
        if state.clipboard_requests.len() >= 64 {
            return Err(HostError("clipboard request limit reached".into()).into());
        }
        state.clipboard_requests.push(ClipboardRequest {
            data,
            mime,
            primary,
        });
        Ok(CallbackReturn::Return)
    });
    let subscribe_state = Rc::clone(&state);
    // The older, text-only view: called with the clipboard's text whenever
    // it changes while the shell holds the keyboard.
    let clipboard_subscribe = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let callback: Closure = stack.consume(ctx)?;
        let mut state = subscribe_state.borrow_mut();
        if state.clipboard_callbacks.len() >= 64 {
            return Err(HostError("clipboard callback limit reached".into()).into());
        }
        state.clipboard_callbacks.push(ctx.stash(callback));
        Ok(CallbackReturn::Return)
    });
    let watch_state = Rc::clone(&state);
    // `watch(function(offer) end, { primary = true })`: every change, focused
    // or not, through data control. `offer` is nil when the clipboard was
    // cleared.
    let clipboard_watch = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (callback, options): (Closure, Option<Table>) = stack.consume(ctx)?;
        let primary = options.is_some_and(|options| {
            matches!(options.get_value(ctx, "primary"), LuaValue::Boolean(true))
        });
        let mut state = watch_state.borrow_mut();
        if state.clipboard_watchers.len() >= 64 {
            return Err(HostError("clipboard watch limit reached".into()).into());
        }
        state
            .clipboard_watchers
            .push((ctx.stash(callback), primary));
        Ok(CallbackReturn::Return)
    });
    let supported_state = Rc::clone(&state);
    // `supported()` — data control is here; `supported("primary")` — and it
    // carries the primary selection. Known once the shell has connected, so
    // false while the configuration is first loading.
    let clipboard_supported = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let what: Option<String> = stack.consume(ctx)?;
        let name = match what.as_deref() {
            None => "data_control",
            Some("primary") => "primary_selection",
            Some(other) => {
                return Err(HostError(format!("clipboard.supported: unknown `{other}`")).into());
            }
        };
        stack.replace(ctx, capability(&supported_state.borrow(), name));
        Ok(CallbackReturn::Return)
    });
    let clipboard = Table::new(&ctx);
    clipboard.set_field(ctx, "set", clipboard_set);
    clipboard.set_field(ctx, "subscribe", clipboard_subscribe);
    clipboard.set_field(ctx, "watch", clipboard_watch);
    clipboard.set_field(ctx, "supported", clipboard_supported);
    morf.set_field(ctx, "clipboard", clipboard);

    let drag_state = Rc::clone(&state);
    // `start({ text =, uris =, paths =, data = { [mime] = bytes } }, done)`:
    // a drag out of the shell. Only from a pointer press — the compositor
    // ties a drag to the button that began it.
    let drag_start = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (payload, done): (Table, Option<Closure>) = stack.consume(ctx)?;
        let request = drag_request(ctx, payload).map_err(HostError)?;
        let mut state = drag_state.borrow_mut();
        if state.drag_requests.len() >= 4 {
            return Err(HostError("drag request limit reached".into()).into());
        }
        state.drag_requests.push(request);
        if let Some(done) = done {
            state.drag_end_callbacks.push(ctx.stash(done));
        }
        Ok(CallbackReturn::Return)
    });
    let drag_supported_state = Rc::clone(&state);
    let drag_supported = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        stack.replace(
            ctx,
            capability(&drag_supported_state.borrow(), "drag_and_drop"),
        );
        Ok(CallbackReturn::Return)
    });
    let drag = Table::new(&ctx);
    drag.set_field(ctx, "start", drag_start);
    drag.set_field(ctx, "supported", drag_supported);
    morf.set_field(ctx, "drag", drag);
}

fn capability(state: &ReactiveState, name: &str) -> bool {
    state
        .capabilities
        .iter()
        .any(|(key, value)| key == name && value == "true")
}

fn string_list<'gc>(
    ctx: Context<'gc>,
    value: LuaValue<'gc>,
    what: &str,
) -> Result<Vec<String>, String> {
    match value {
        LuaValue::Nil => Ok(Vec::new()),
        LuaValue::String(text) => Ok(vec![text.display_lossy().to_string()]),
        LuaValue::Table(table) => {
            let mut items = Vec::new();
            for index in 1..=4096 {
                match table.get_value(ctx, index) {
                    LuaValue::Nil => break,
                    LuaValue::String(text) => items.push(text.display_lossy().to_string()),
                    _ => return Err(format!("drag {what} must be strings")),
                }
            }
            Ok(items)
        }
        _ => Err(format!("drag {what} must be a string or a list of strings")),
    }
}

fn drag_request<'gc>(ctx: Context<'gc>, payload: Table<'gc>) -> Result<DragRequest, String> {
    let text = match payload.get_value(ctx, "text") {
        LuaValue::Nil => None,
        LuaValue::String(text) => Some(text.display_lossy().to_string()),
        _ => return Err("drag text must be a string".to_owned()),
    };
    let uris = string_list(ctx, payload.get_value(ctx, "uris"), "uris")?;
    let paths = string_list(ctx, payload.get_value(ctx, "paths"), "paths")?;
    let mut data = Vec::new();
    let mut total = text.as_ref().map_or(0, String::len);
    match payload.get_value(ctx, "data") {
        LuaValue::Nil => {}
        LuaValue::Table(table) => {
            for (mime, bytes) in table.iter(ctx) {
                let (LuaValue::String(mime), LuaValue::String(bytes)) = (mime, bytes) else {
                    return Err("drag data must map mime strings to byte strings".to_owned());
                };
                total += bytes.as_bytes().len();
                data.push((mime.display_lossy().to_string(), bytes.as_bytes().to_vec()));
                if data.len() > MAX_DRAG_TYPES {
                    return Err("drag offers too many types".to_owned());
                }
            }
        }
        _ => return Err("drag data must be a table".to_owned()),
    }
    if total > MAX_SET_BYTES {
        return Err("drag data limit reached".to_owned());
    }
    if text.is_none() && uris.is_empty() && paths.is_empty() && data.is_empty() {
        return Err("a drag must carry something: text, uris, paths or data".to_owned());
    }
    // Deterministic order for the host; a Lua table has none.
    data.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(DragRequest {
        text,
        uris,
        paths,
        data,
    })
}

/// Builds the table a clipboard watcher or a drop handler is given.
///
/// `read` is bound to the offer here, so `offer:read(mime, cb)` needs nothing
/// from the table but its method.
pub(crate) fn offer_table<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    offer: &OfferDescription,
    primary: Option<bool>,
    point: Option<EventPoint>,
) -> Table<'gc> {
    let table = Table::new(&ctx);
    table.set_field(ctx, "id", offer.id as i64);
    let mime_types = Table::new(&ctx);
    for (index, mime) in offer.mime_types.iter().enumerate() {
        let _ = mime_types.set(ctx, index as i64 + 1, ctx.intern(mime.as_bytes()));
    }
    table.set_field(ctx, "mime_types", mime_types);
    if let Some(primary) = primary {
        table.set_field(ctx, "primary", primary);
    }
    if let Some(accepted) = &offer.accepted {
        table.set_field(ctx, "accepted", ctx.intern(accepted.as_bytes()));
    }
    if !offer.uris.is_empty() {
        let uris = Table::new(&ctx);
        for (index, uri) in offer.uris.iter().enumerate() {
            let _ = uris.set(ctx, index as i64 + 1, ctx.intern(uri.as_bytes()));
        }
        table.set_field(ctx, "uris", uris);
        let paths = Table::new(&ctx);
        for (index, path) in offer.paths.iter().enumerate() {
            let _ = paths.set(ctx, index as i64 + 1, ctx.intern(path.as_bytes()));
        }
        table.set_field(ctx, "paths", paths);
    }
    if let Some(text) = &offer.text {
        table.set_field(ctx, "text", ctx.intern(text.as_bytes()));
    }
    if let Some(point) = point {
        table.set_field(ctx, "x", point.local_x);
        table.set_field(ctx, "y", point.local_y);
        table.set_field(ctx, "surface_x", point.surface_x);
        table.set_field(ctx, "surface_y", point.surface_y);
    }
    let read_state = Rc::clone(state);
    let offer_id = offer.id;
    let read = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (receiver, mime, callback): (LuaValue, String, Closure) = stack.consume(ctx)?;
        if !matches!(receiver, LuaValue::Table(_)) {
            return Err(HostError("use offer:read(mime, callback)".into()).into());
        }
        if mime.is_empty() || mime.len() > 256 {
            return Err(HostError("offer mime must be 1..256 bytes".into()).into());
        }
        let mut state = read_state.borrow_mut();
        if state.offer_read_callbacks.len() >= MAX_PENDING_READS {
            return Err(HostError("offer read limit reached".into()).into());
        }
        state.next_offer_read = state.next_offer_read.wrapping_add(1);
        let id = state.next_offer_read;
        state.offer_reads.push(OfferReadRequest {
            id,
            offer: offer_id,
            mime,
        });
        state.offer_read_callbacks.insert(id, ctx.stash(callback));
        Ok(CallbackReturn::Return)
    });
    table.set_field(ctx, "read", read);
    table
}
