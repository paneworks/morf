//! `morf.http`: fetching from the web without stalling a frame.
//!
//! ```lua
//! local handle = morf.http.request {
//!     url = "https://api.example.com/v1/items",
//!     method = "POST",               -- default GET, or POST when a body is given
//!     headers = { authorization = "Bearer ..." },
//!     body = "raw bytes",            -- or
//!     json = { name = "x" },         -- encoded, and content-type set to match
//!     timeout_ms = 15000,            -- whole exchange; at most 120000
//!     max_bytes = 8 * 1024 * 1024,   -- the body refused past this; at most 64 MiB
//!     on_done = function(response) end,
//! }
//! handle:cancel()   -- on_done will not be called; false if it already was
//! handle:done()     -- answered or cancelled
//!
//! morf.http.get(url, function(response) end)
//! morf.http.get(url, { headers = ... }, function(response) end)
//! morf.http.post(url, "raw body", function(response) end)
//! morf.http.post(url, { a = 1 }, { headers = ... }, function(response) end) -- a table is sent as JSON
//! morf.http.url_encode("a b")        --> "a%20b"
//! morf.http.url_decode("a%20b")      --> "a b"
//! morf.http.query { q = "x y", n = 2 } --> "n=2&q=x%20y"
//! ```
//!
//! A response is `{ ok, status, headers, body, url, error, json }`: `ok` is a
//! 2xx status; `headers` has lowercased names, a repeated one joined by `, `; `url` is where the answer came
//! from after redirects; `json()` decodes the body and returns `nil, message`
//! when it is not JSON. Any status is a response — a 404 is `ok = false` with
//! its `status` and `body`. A request that got no response at all (bad URL,
//! refused, timed out, too large, cancelled) is `ok = false`, `status = 0` and
//! `error` saying why (`cancelled` is never seen: a cancelled request does
//! not call back). Those are the failures a configuration should expect,
//! so they arrive in the callback; only a malformed call raises.
//!
//! The request runs on a worker thread and `on_done` runs on the main loop,
//! like every other callback here, so it may touch signals and nodes. The
//! engine holds the callback, not the other way round: a reload drops every
//! request the old configuration made, and their answers go nowhere. At
//! most sixteen requests are on the wire at once across the process, the
//! rest queue; redirects are followed up to ten deep.

use luna::{
    Callback, CallbackReturn, Closure, Context, Executor, Function, StashedTable, StashedUserData,
    Table, UserData, UserRef, Value as LuaValue, Variadic,
};
use morf_io::{HttpRequest, HttpResponse, HttpTask, MAX_BODY_LIMIT, MAX_TIMEOUT};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::{
    Limits, reactive_execute::drive_executor, scene_bindings::*, serialization::*, state::*,
};
use morf_runtime::Handler;

/// Requests one configuration may have outstanding. The pool puts at most
/// sixteen on the wire at once; this caps the queue behind them, so a loop
/// that forgets to wait cannot pile up work without end.
const MAX_PENDING: usize = 64;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_HEADERS: usize = 64;

/// The tags `morf.json` puts on decoded tables, so `response.json()` returns
/// the same kind of value `morf.json.decode` does and it encodes back the same.
#[derive(Clone)]
pub(crate) struct JsonKinds {
    pub(crate) array: StashedTable,
    pub(crate) object: StashedTable,
    pub(crate) null: StashedUserData,
}

/// What a handle and the pending entry share: the one bit of news either
/// side has for the other.
#[derive(Default)]
pub(crate) struct HttpHandleState {
    pub(crate) done: Cell<bool>,
    pub(crate) cancelled: Cell<bool>,
}

/// A request in flight and who is owed its answer.
pub(crate) struct PendingHttp {
    pub(crate) task: HttpTask,
    pub(crate) callback: Option<Handler>,
    pub(crate) handle: Rc<HttpHandleState>,
    pub(crate) url: String,
    pub(crate) json: JsonKinds,
}

struct HttpHandleToken {
    state: Rc<HttpHandleState>,
}

/// Installs `morf.http`.
pub(crate) fn install_http_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
    json: JsonKinds,
) {
    let http = Table::new(&ctx);

    let handle_cancel = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let handle: UserRef<HttpHandleToken> = stack.consume(ctx)?;
        let pending = !handle.state.done.get();
        // The entry is dropped at the next poll, which drops the task and
        // stops the worker; the callback is never run from here on.
        handle.state.cancelled.set(true);
        handle.state.done.set(true);
        stack.replace(ctx, pending);
        Ok(CallbackReturn::Return)
    });
    let handle_done = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let handle: UserRef<HttpHandleToken> = stack.consume(ctx)?;
        stack.replace(ctx, handle.state.done.get());
        Ok(CallbackReturn::Return)
    });
    let methods = Table::new(&ctx);
    methods.set_field(ctx, "cancel", handle_cancel);
    methods.set_field(ctx, "done", handle_done);
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", methods);
    let metatable = ctx.stash(metatable);

    let start = Rc::new(Starter {
        state,
        json,
        metatable,
    });

    let request_start = Rc::clone(&start);
    let request = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let options: Table = stack.consume(ctx)?;
        let url = match options.get_value(ctx, "url") {
            LuaValue::String(url) => url.display_lossy().to_string(),
            _ => return Err(HostError("http request url must be a string".into()).into()),
        };
        let mut request = HttpRequest::get(url);
        apply_options(ctx, &mut request, Some(options))?;
        let callback = function_of(options.get_value(ctx, "on_done"))?;
        stack.replace(ctx, request_start.start(ctx, request, callback)?);
        Ok(CallbackReturn::Return)
    });
    http.set_field(ctx, "request", request);

    let get_start = Rc::clone(&start);
    let get = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (url, options, callback): (String, LuaValue, LuaValue) = stack.consume(ctx)?;
        let (options, callback) = options_and_callback(ctx, options, callback)?;
        let mut request = HttpRequest::get(url);
        apply_options(ctx, &mut request, options)?;
        stack.replace(ctx, get_start.start(ctx, request, callback)?);
        Ok(CallbackReturn::Return)
    });
    http.set_field(ctx, "get", get);

    let post = Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let (url, payload, options, callback): (String, LuaValue, LuaValue, LuaValue) =
            stack.consume(ctx)?;
        let (options, callback) = options_and_callback(ctx, options, callback)?;
        let mut request = HttpRequest::get(url);
        request.method = "POST".into();
        match payload {
            LuaValue::Nil => {}
            LuaValue::String(body) => request.body = Some(body.as_bytes().to_vec()),
            payload => set_json_body(ctx, &mut request, payload)?,
        }
        apply_options(ctx, &mut request, options)?;
        stack.replace(ctx, start.start(ctx, request, callback)?);
        Ok(CallbackReturn::Return)
    });
    http.set_field(ctx, "post", post);

    let url_encode = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let text: luna::String = stack.consume(ctx)?;
        stack.replace(ctx, morf_io::url_encode(text.as_bytes()));
        Ok(CallbackReturn::Return)
    });
    http.set_field(ctx, "url_encode", url_encode);
    let url_decode = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let text: luna::String = stack.consume(ctx)?;
        stack.replace(ctx, ctx.intern(&morf_io::url_decode(text.as_bytes())));
        Ok(CallbackReturn::Return)
    });
    http.set_field(ctx, "url_decode", url_decode);
    let query = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let values: Table = stack.consume(ctx)?;
        stack.replace(ctx, encode_query(ctx, values)?);
        Ok(CallbackReturn::Return)
    });
    http.set_field(ctx, "query", query);

    morf.set_field(ctx, "http", http);
}

/// Every entry point ends here: the request is queued, the callback kept
/// alongside it, and the caller gets a handle.
struct Starter {
    state: Rc<RefCell<ReactiveState>>,
    json: JsonKinds,
    metatable: StashedTable,
}

impl Starter {
    fn start<'gc>(
        &self,
        ctx: Context<'gc>,
        request: HttpRequest,
        callback: Option<Closure<'gc>>,
    ) -> Result<UserData<'gc>, HostError> {
        let mut reactive = self.state.borrow_mut();
        reactive
            .http_requests
            .retain(|entry| !entry.handle.cancelled.get());
        if reactive.http_requests.len() >= MAX_PENDING {
            return Err(HostError(format!(
                "more than {MAX_PENDING} HTTP requests in flight"
            )));
        }
        let handle = Rc::new(HttpHandleState::default());
        reactive.http_requests.push(PendingHttp {
            url: request.url.clone(),
            task: HttpTask::start(request),
            callback: callback
                .map(|callback| crate::vm::handler_store::register(ctx.stash(callback))),
            handle: Rc::clone(&handle),
            json: self.json.clone(),
        });
        let userdata = UserData::new_static(&ctx, HttpHandleToken { state: handle });
        userdata.set_metatable(ctx, Some(ctx.fetch(&self.metatable)));
        Ok(userdata)
    }
}

/// `get(url, callback)` and `get(url, opts, callback)` both read naturally,
/// and so does leaving the callback in `opts.on_done`.
fn options_and_callback<'gc>(
    ctx: Context<'gc>,
    options: LuaValue<'gc>,
    callback: LuaValue<'gc>,
) -> Result<(Option<Table<'gc>>, Option<Closure<'gc>>), HostError> {
    match options {
        LuaValue::Nil => Ok((None, function_of(callback)?)),
        LuaValue::Function(_) => {
            if !matches!(callback, LuaValue::Nil) {
                return Err(HostError("http callback given twice".into()));
            }
            Ok((None, function_of(options)?))
        }
        LuaValue::Table(table) => {
            let callback = match callback {
                LuaValue::Nil => function_of(table.get_value(ctx, "on_done"))?,
                callback => function_of(callback)?,
            };
            Ok((Some(table), callback))
        }
        _ => Err(HostError("http options must be a table".into())),
    }
}

fn function_of(value: LuaValue<'_>) -> Result<Option<Closure<'_>>, HostError> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::Function(Function::Closure(closure)) => Ok(Some(closure)),
        _ => Err(HostError("http callback must be a Lua function".into())),
    }
}

fn apply_options<'gc>(
    ctx: Context<'gc>,
    request: &mut HttpRequest,
    options: Option<Table<'gc>>,
) -> Result<(), HostError> {
    request.timeout = DEFAULT_TIMEOUT;
    request.max_bytes = DEFAULT_MAX_BYTES;
    let Some(options) = options else {
        return Ok(());
    };
    match options.get_value(ctx, "headers") {
        LuaValue::Nil => {}
        LuaValue::Table(headers) => {
            for (name, value) in headers.iter(ctx) {
                let LuaValue::String(name) = name else {
                    return Err(HostError("http header names must be strings".into()));
                };
                let value = match value {
                    LuaValue::String(value) => value.display_lossy().to_string(),
                    LuaValue::Integer(value) => value.to_string(),
                    LuaValue::Number(value) => value.to_string(),
                    _ => return Err(HostError("http header values must be strings".into())),
                };
                let name = name.display_lossy().to_string();
                // A later header of the same name replaces one the call
                // itself set, content-type after `json` in particular.
                request
                    .headers
                    .retain(|(known, _)| !known.eq_ignore_ascii_case(&name));
                request.headers.push((name, value));
                if request.headers.len() > MAX_HEADERS {
                    return Err(HostError(format!("more than {MAX_HEADERS} http headers")));
                }
            }
        }
        _ => return Err(HostError("http headers must be a table".into())),
    }
    let body = options.get_value(ctx, "body");
    let json = options.get_value(ctx, "json");
    match (body, json) {
        (LuaValue::Nil, LuaValue::Nil) => {}
        (LuaValue::String(body), LuaValue::Nil) => {
            request.body = Some(body.as_bytes().to_vec());
        }
        (LuaValue::Nil, json) => set_json_body(ctx, request, json)?,
        (LuaValue::String(_), _) => {
            return Err(HostError(
                "http request takes body or json, not both".into(),
            ));
        }
        _ => return Err(HostError("http body must be a string".into())),
    }
    match options.get_value(ctx, "method") {
        LuaValue::Nil if request.body.is_some() && request.method == "GET" => {
            request.method = "POST".into();
        }
        LuaValue::Nil => {}
        LuaValue::String(method) => {
            let method = method.display_lossy().to_string().to_ascii_uppercase();
            if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_alphabetic()) {
                return Err(HostError(format!("http method {method:?} is not valid")));
            }
            request.method = method;
        }
        _ => return Err(HostError("http method must be a string".into())),
    }
    if let Some(timeout) = positive(ctx, options, "timeout_ms")? {
        request.timeout = Duration::from_millis(timeout).min(MAX_TIMEOUT);
    }
    if let Some(max_bytes) = positive(ctx, options, "max_bytes")? {
        request.max_bytes = usize::try_from(max_bytes)
            .unwrap_or(MAX_BODY_LIMIT)
            .min(MAX_BODY_LIMIT);
    }
    Ok(())
}

fn positive<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
    field: &str,
) -> Result<Option<u64>, HostError> {
    match options.get_value(ctx, field) {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(value) if value > 0 => Ok(Some(value as u64)),
        LuaValue::Number(value) if value.is_finite() && value >= 1.0 => Ok(Some(value as u64)),
        _ => Err(HostError(format!("http {field} must be a positive number"))),
    }
}

fn set_json_body<'gc>(
    ctx: Context<'gc>,
    request: &mut HttpRequest,
    value: LuaValue<'gc>,
) -> Result<(), HostError> {
    let mut entries = 0;
    let value = lua_to_json(ctx, value, 0, &mut entries).map_err(HostError)?;
    let body = serde_json::to_vec(&value).map_err(|error| HostError(error.to_string()))?;
    request.body = Some(body);
    request.headers.push((
        "Content-Type".into(),
        "application/json; charset=utf-8".into(),
    ));
    Ok(())
}

/// `a=1&b=x`, keys sorted so the same table always makes the same URL — and
/// the same cache key. A list value repeats its key, as most APIs expect.
fn encode_query<'gc>(ctx: Context<'gc>, values: Table<'gc>) -> Result<String, HostError> {
    fn scalar(value: LuaValue<'_>) -> Result<Vec<u8>, HostError> {
        match value {
            LuaValue::String(value) => Ok(value.as_bytes().to_vec()),
            LuaValue::Integer(value) => Ok(value.to_string().into_bytes()),
            LuaValue::Number(value) => Ok(value.to_string().into_bytes()),
            LuaValue::Boolean(value) => Ok(value.to_string().into_bytes()),
            _ => Err(HostError("http query values must be scalars".into())),
        }
    }
    let mut pairs = Vec::new();
    for (key, value) in values.iter(ctx) {
        let key = match key {
            LuaValue::String(key) => key.as_bytes().to_vec(),
            LuaValue::Integer(key) => key.to_string().into_bytes(),
            _ => return Err(HostError("http query keys must be strings".into())),
        };
        match value {
            LuaValue::Table(list) => {
                for index in 1..=list.length(&ctx) {
                    pairs.push((key.clone(), scalar(list.get_value(ctx, index))?));
                }
            }
            value => pairs.push((key, scalar(value)?)),
        }
    }
    pairs.sort();
    Ok(pairs
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                morf_io::url_encode(key),
                morf_io::url_encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&"))
}

/// Hands one answer to the callback that asked for it, as a response table.
pub(crate) fn execute_http_handler(
    ctx: Context<'_>,
    closure: &Handler,
    outcome: Result<HttpResponse, String>,
    url: &str,
    json: &JsonKinds,
    limits: Limits,
) -> Result<(), String> {
    let response = Table::new(&ctx);
    let headers = Table::new(&ctx);
    let body = match outcome {
        Ok(answer) => {
            response.set_field(ctx, "ok", (200..300).contains(&answer.status));
            response.set_field(ctx, "status", i64::from(answer.status));
            response.set_field(ctx, "url", answer.url.as_str());
            for (name, value) in &answer.headers {
                headers
                    .set(
                        ctx,
                        ctx.intern(name.as_bytes()),
                        ctx.intern(value.as_bytes()),
                    )
                    .map_err(|error| error.to_string())?;
            }
            ctx.intern(&answer.body)
        }
        Err(error) => {
            response.set_field(ctx, "ok", false);
            response.set_field(ctx, "status", 0i64);
            response.set_field(ctx, "url", url);
            response.set_field(ctx, "error", error.as_str());
            ctx.intern(b"")
        }
    };
    response.set_field(ctx, "headers", headers);
    response.set_field(ctx, "body", body);
    let root = (
        body,
        ctx.fetch(&json.array),
        ctx.fetch(&json.object),
        ctx.fetch(&json.null),
    );
    let decode = Callback::from_fn_with(
        &ctx,
        root,
        |&(body, array, object, null), ctx, _, mut stack| {
            let decoded = serde_json::from_slice::<serde_json::Value>(body.as_bytes())
                .map_err(|error| error.to_string())
                .and_then(|value| {
                    let mut entries = 0;
                    json_to_lua(ctx, &value, array, object, null, 0, &mut entries)
                });
            match decoded {
                Ok(value) => stack.replace(ctx, value),
                Err(error) => stack.replace(ctx, (LuaValue::Nil, error.as_str())),
            }
            Ok(CallbackReturn::Return)
        },
    );
    response.set_field(ctx, "json", decode);
    let executor = Executor::start(
        ctx,
        ctx.fetch(&crate::vm::handler_store::stashed(closure))
            .into(),
        Variadic(vec![LuaValue::Table(response)]),
    );
    drive_executor(ctx, executor, limits, limits.effect_fuel, "handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
