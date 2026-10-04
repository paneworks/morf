//! `morf.screencopy`: asking the compositor for a picture of the screen.
//!
//! Split from the host API at the line gate. Two ways to ask -- an output,
//! or one window by the identifier `morf.windows` reported -- one way to let
//! a picture go, since nothing collects a published capture on its own, and
//! one way to put a picture straight into a file.

use luna::{Callback, CallbackReturn, Closure, Context, Table, Value as LuaValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::api_image_ops::{callback_of, format_of, quality_of, region_of, source_of};
use crate::image_jobs::CaptureSave;
use crate::{scene_bindings::HostError, state::*, surface_types::*};

/// How many captures may be waiting on the compositor at once.
const MAX_PENDING: usize = 4;

/// Captures in flight, whether a callback or a file is waiting on each.
fn pending(state: &ReactiveState) -> usize {
    state.requests.screencopy_callbacks.len()
        + state
            .screencopy_saves
            .keys()
            .filter(|id| !state.requests.screencopy_callbacks.contains_key(id))
            .count()
}

pub(crate) fn install_screencopy_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    let screencopy_state = Rc::clone(&state);
    // `capture(include_cursor, handler, options)`: `options.gpu` asks for
    // the picture to stay on the GPU, which is the difference between a
    // thumbnail that costs two copies of the screen and one that costs none;
    // `options.output` names the output, where there is more than one.
    let screencopy_capture = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (include_cursor, callback, options): (bool, Closure, Option<Table>) =
            stack.consume(ctx)?;
        let (gpu, name, output) = capture_options(ctx, options);
        let mut state = screencopy_state.borrow_mut();
        if pending(&state) >= MAX_PENDING {
            return Err(HostError("screencopy request limit reached".into()).into());
        }
        let id = state.requests.next_screencopy;
        state.requests.next_screencopy = state.requests.next_screencopy.wrapping_add(1);
        state.requests.screencopy_requests.push(ScreencopyRequest {
            id,
            include_cursor,
            window: None,
            gpu,
            name: name.clone(),
            output,
        });
        if let Some(name) = name {
            state.requests.screencopy_names.insert(id, name);
        }
        state
            .requests
            .screencopy_callbacks
            .insert(id, crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    let window_state = Rc::clone(&state);
    // `capture_window(identifier, handler)` — the same frame, the same handler
    // shape, one window instead of the whole output. Separate from `capture`
    // rather than an extra argument to it because the two can fail for
    // different reasons and a configuration wants to know which.
    let screencopy_window = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (identifier, callback, options): (String, Closure, Option<Table>) =
            stack.consume(ctx)?;
        let (gpu, name, _) = capture_options(ctx, options);
        let mut state = window_state.borrow_mut();
        if pending(&state) >= MAX_PENDING {
            return Err(HostError("screencopy request limit reached".into()).into());
        }
        let id = state.requests.next_screencopy;
        state.requests.next_screencopy = state.requests.next_screencopy.wrapping_add(1);
        state.requests.screencopy_requests.push(ScreencopyRequest {
            id,
            include_cursor: false,
            window: Some(identifier),
            gpu,
            name: name.clone(),
            output: None,
        });
        if let Some(name) = name {
            state.requests.screencopy_names.insert(id, name);
        }
        state
            .requests
            .screencopy_callbacks
            .insert(id, crate::vm::handler_store::register(ctx.stash(callback)));
        Ok(CallbackReturn::Return)
    });
    // `release(source)`: the picture is as large as the screen, on the GPU
    // or in memory, and nothing collects it on its own -- only the
    // configuration knows when the window it showed has gone.
    let release_state = Rc::clone(&state);
    let screencopy_release = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let source: String = stack.consume(ctx)?;
        release_state
            .borrow_mut()
            .requests
            .screencopy_releases
            .push(source);
        Ok(CallbackReturn::Return)
    });
    // `save { path, output?, window?, region?, include_cursor?, format?,
    // quality?, on_done? }`: a screenshot, to a file. The same request as
    // `capture`, but the pixels go to an image worker to be cut and encoded
    // instead of into Lua as a string a screen large — and nothing is
    // published for `ui.Image`, so there is nothing to release afterwards.
    //
    // `region` is in the capture's own pixels, which on a scaled output are
    // physical pixels, not layout units. `on_done(true, { width, height,
    // format, path })` or `on_done(false, message)`.
    let save_state = Rc::clone(&state);
    let screencopy_save = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Table = stack.consume(ctx)?;
        let path = source_of(options.get_value(ctx, "path"), "screencopy.save path")?;
        let format = format_of(options.get_value(ctx, "format"), &path)?;
        let quality = quality_of(options.get_value(ctx, "quality"))?;
        let region = region_of(ctx, options.get_value(ctx, "region"))?;
        let callback = callback_of(options.get_value(ctx, "on_done"), "on_done")?;
        let include_cursor = match options.get_value(ctx, "include_cursor") {
            LuaValue::Nil => false,
            LuaValue::Boolean(value) => value,
            _ => return Err(HostError("include_cursor must be boolean".into()).into()),
        };
        let text = |field: &str| match options.get_value(ctx, field) {
            LuaValue::Nil => Ok(None),
            LuaValue::String(value) => Ok(Some(value.display_lossy().to_string())),
            _ => Err(HostError(format!(
                "screencopy.save {field} must be a string"
            ))),
        };
        let output = text("output")?;
        let window = text("window")?;
        let mut state = save_state.borrow_mut();
        if pending(&state) >= MAX_PENDING {
            return Err(HostError("screencopy request limit reached".into()).into());
        }
        let id = state.requests.next_screencopy;
        state.requests.next_screencopy = state.requests.next_screencopy.wrapping_add(1);
        state.requests.screencopy_requests.push(ScreencopyRequest {
            id,
            include_cursor: include_cursor && window.is_none(),
            window,
            gpu: false,
            name: None,
            output,
        });
        state.screencopy_saves.insert(
            id,
            CaptureSave {
                path,
                region,
                format,
                quality,
            },
        );
        if let Some(callback) = callback {
            state
                .requests
                .screencopy_callbacks
                .insert(id, crate::vm::handler_store::register(ctx.stash(callback)));
        }
        Ok(CallbackReturn::Return)
    });
    let screencopy = Table::new(&ctx);
    screencopy.set_field(ctx, "capture", screencopy_capture);
    screencopy.set_field(ctx, "capture_window", screencopy_window);
    screencopy.set_field(ctx, "release", screencopy_release);
    screencopy.set_field(ctx, "save", screencopy_save);
    morf.set_field(ctx, "screencopy", screencopy);
}

/// What a capture's `options` table asks for: the GPU, a name, an output.
fn capture_options<'gc>(
    ctx: Context<'gc>,
    options: Option<Table<'gc>>,
) -> (bool, Option<String>, Option<String>) {
    let Some(options) = options else {
        return (false, None, None);
    };
    let gpu = matches!(options.get_value(ctx, "gpu"), LuaValue::Boolean(true));
    let text = |field: &str| match options.get_value(ctx, field) {
        LuaValue::String(value) => Some(value.display_lossy().to_string()),
        _ => None,
    };
    (gpu, text("name"), text("output"))
}
