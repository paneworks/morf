//! `morf.audio`: the machine's sound, for volume sliders, mixers and meters.
//!
//! ```lua
//! local audio = morf.audio
//! audio.available()                -- is a sound server reachable
//! audio.sinks, audio.sources       -- list models of devices, keyed by `id`
//! audio.streams                    -- list model of applications playing or recording
//! audio.default_sink()             -- a device row, or nil
//! audio.set_volume(id, 0.5)        -- a device or a stream; 0 to 1.5, as mixers show it
//! audio.set_mute(id, true)
//! audio.set_default(id)
//! audio.move_stream(stream_id, device_id)
//! audio.on_changed(function(what) end)       -- what = { devices, streams, defaults, available }
//! local meter = audio.monitor { device = nil, rate_hz = 30, bands = 16,
//!     on_level = function(left, right, bands) end,
//!     beat = true,                                   -- listen for beats too
//!     on_beat = function(strength) end,              -- 0 to 1, as each lands
//!     on_tempo = function(bpm, confidence) end }     -- as the estimate moves
//! meter.bpm, meter.confidence                        -- the latest estimate, or nil
//! meter:stop()
//! ```
//!
//! Every reader (`available`, `default_sink`, `device`, ...) is tracked, so a
//! binding that calls one follows it. Nothing here names a sound server; the
//! server is whatever `morf_audio::Audio::connect` finds, started the first
//! time a configuration touches `morf.audio`, so a shell that never does
//! never opens a connection. Without a server everything reads empty and
//! every command answers false.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use luna::{
    Callback, CallbackReturn, Closure, Context, Executor, StashedClosure, Table, UserData,
    Value as LuaValue, Variadic,
};
use morf_audio::{Audio, Device, DeviceKind, Stream};
use morf_reactive::SignalId;
use morf_scene::{ListModel, Value as SceneValue};

use crate::{
    reactive_execute::drive_executor,
    scene_bindings::*,
    serialization::scene_to_lua,
    state::*,
    surface_types::*,
    table_menu::{optional_closure, table_bool, table_number},
    types::*,
};

/// How many `on_changed` handlers and monitors one configuration may hold.
const MAX_LISTENERS: usize = 32;
const MAX_MONITORS: usize = 8;

/// What `morf.audio` keeps between frames.
pub(crate) struct AudioHost {
    pub(crate) audio: Option<Audio>,
    /// What starts the audio when first asked; the machine's server unless
    /// a host (a test) said otherwise.
    pub(crate) factory: Option<Box<dyn FnOnce() -> Audio>>,
    pub(crate) sinks: Rc<RefCell<ListModel>>,
    pub(crate) sources: Rc<RefCell<ListModel>>,
    pub(crate) streams: Rc<RefCell<ListModel>>,
    pub(crate) available: SignalId,
    /// Moves on every change, so a reader that depends on any of it reruns.
    pub(crate) revision: SignalId,
    pub(crate) revisions: i64,
    pub(crate) listeners: Vec<(u64, StashedClosure)>,
    pub(crate) monitors: HashMap<u64, MonitorHandlers>,
    pub(crate) next_listener: u64,
}

/// What one `morf.audio.monitor` calls, and the tempo it last heard.
pub(crate) struct MonitorHandlers {
    pub(crate) on_level: Option<StashedClosure>,
    pub(crate) on_beat: Option<StashedClosure>,
    pub(crate) on_tempo: Option<StashedClosure>,
    pub(crate) tempo: Option<(f32, f32)>,
}

impl AudioHost {
    /// The audio, started on first use.
    pub(crate) fn started(&mut self) -> &mut Audio {
        let factory = &mut self.factory;
        self.audio
            .get_or_insert_with(|| factory.take().map_or_else(Audio::connect, |start| start()))
    }
}

/// A volume for Lua: four decimals, so 0.54 reads as 0.54 and not as the
/// float nearest it.
fn tidy(volume: f32) -> f64 {
    (f64::from(volume) * 10_000.0).round() / 10_000.0
}

fn text(value: &Option<String>) -> SceneValue {
    value
        .as_ref()
        .map_or(SceneValue::Nil, |value| SceneValue::String(value.clone()))
}

pub(crate) fn device_row(device: &Device, default: bool) -> SceneValue {
    let mut row = BTreeMap::new();
    row.insert("id".into(), SceneValue::Number(f64::from(device.id)));
    row.insert("name".into(), SceneValue::String(device.name.clone()));
    row.insert(
        "description".into(),
        SceneValue::String(device.description.clone()),
    );
    row.insert("kind".into(), SceneValue::String(device.kind.name().into()));
    row.insert("volume".into(), SceneValue::Number(tidy(device.volume())));
    row.insert(
        "volumes".into(),
        SceneValue::List(
            device
                .channel_volumes
                .iter()
                .map(|gain| SceneValue::Number(tidy(morf_audio::volume::from_linear(*gain))))
                .collect(),
        ),
    );
    row.insert("muted".into(), SceneValue::Bool(device.muted));
    row.insert("default".into(), SceneValue::Bool(default));
    row.insert(
        "channels".into(),
        SceneValue::Number(device.channels() as f64),
    );
    row.insert("icon_name".into(), text(&device.icon_name));
    SceneValue::Map(row)
}

pub(crate) fn stream_row(stream: &Stream) -> SceneValue {
    let mut row = BTreeMap::new();
    row.insert("id".into(), SceneValue::Number(f64::from(stream.id)));
    row.insert(
        "app_name".into(),
        SceneValue::String(stream.app_name.clone()),
    );
    row.insert("app_id".into(), text(&stream.app_id));
    row.insert("binary".into(), text(&stream.binary));
    row.insert("icon_name".into(), text(&stream.icon_name));
    row.insert("media_name".into(), text(&stream.media_name));
    row.insert(
        "direction".into(),
        SceneValue::String(stream.direction.name().into()),
    );
    row.insert(
        "device".into(),
        stream.device.map_or(SceneValue::Nil, |device| {
            SceneValue::Number(f64::from(device))
        }),
    );
    row.insert("volume".into(), SceneValue::Number(tidy(stream.volume())));
    row.insert("muted".into(), SceneValue::Bool(stream.muted));
    row.insert(
        "channels".into(),
        SceneValue::Number(stream.channels() as f64),
    );
    row.insert(
        "pid".into(),
        stream
            .pid
            .map_or(SceneValue::Nil, |pid| SceneValue::Number(f64::from(pid))),
    );
    SceneValue::Map(row)
}

/// A row as a Lua table, with its counts and ids as integers.
fn row_to_lua<'gc>(ctx: Context<'gc>, row: &SceneValue) -> Result<LuaValue<'gc>, String> {
    let value = scene_to_lua(ctx, row)?;
    if let (LuaValue::Table(table), SceneValue::Map(fields)) = (value, row) {
        for key in ["id", "device", "channels", "pid"] {
            if let Some(SceneValue::Number(number)) = fields.get(key) {
                table.set_field(ctx, key, *number as i64);
            }
        }
    }
    Ok(value)
}

/// An object id from Lua: an integer, or a float that is one (a row read
/// back out of a list model).
fn object_id(value: LuaValue<'_>, what: &str) -> Result<u32, HostError> {
    let id = match value {
        LuaValue::Integer(id) => id,
        LuaValue::Number(id) if id.fract() == 0.0 && id.is_finite() => id as i64,
        _ => return Err(HostError(format!("{what} must be an audio object id"))),
    };
    u32::try_from(id).map_err(|_| HostError(format!("{what} `{id}` is not an audio object id")))
}

/// Reads a signal, telling a running binding it depends on it.
fn track(state: &mut ReactiveState, id: SignalId) -> Option<IpcValue> {
    if let Some(active) = &mut state.active {
        active.reads.insert(id);
    }
    state.values.get(&id).cloned()
}

fn host(state: &mut ReactiveState) -> &mut AudioHost {
    state
        .audio
        .as_mut()
        .expect("morf.audio is installed with the runtime")
}

/// Starts the audio if need be and marks the caller as following it.
fn follow(state: &mut ReactiveState) {
    let revision = {
        let host = host(state);
        host.started();
        host.revision
    };
    track(state, revision);
}

fn list_model_userdata<'gc>(
    ctx: Context<'gc>,
    state: &ReactiveState,
    model: &Rc<RefCell<ListModel>>,
) -> Result<UserData<'gc>, HostError> {
    let metatable = state
        .model_metatable
        .clone()
        .ok_or_else(|| HostError("list models are not installed".into()))?;
    let userdata = UserData::new_static(
        &ctx,
        ListModelToken {
            model: Rc::clone(model),
        },
    );
    userdata.set_metatable(ctx, Some(ctx.fetch(&metatable)));
    Ok(userdata)
}

pub(crate) fn install_audio_api<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    morf: Table<'gc>,
) {
    {
        let mut state = state.borrow_mut();
        let graph = state
            .graph
            .as_mut()
            .expect("the graph is not running at install");
        let available = graph.signal("audio.available", IpcValue::Boolean(false));
        let revision = graph.signal("audio.revision", IpcValue::Integer(0));
        state.values.insert(available, IpcValue::Boolean(false));
        state.values.insert(revision, IpcValue::Integer(0));
        state.signals.push(available);
        state.signals.push(revision);
        state.audio = Some(AudioHost {
            audio: None,
            factory: None,
            sinks: Rc::new(RefCell::new(ListModel::default())),
            sources: Rc::new(RefCell::new(ListModel::default())),
            streams: Rc::new(RefCell::new(ListModel::default())),
            available,
            revision,
            revisions: 0,
            listeners: Vec::new(),
            monitors: HashMap::new(),
            next_listener: 1,
        });
    }

    let audio = Table::new(&ctx);
    let function =
        |name: &'static str, callback: Callback<'gc>| audio.set_field(ctx, name, callback);

    function(
        "available",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let mut state = state.borrow_mut();
                follow(&mut state);
                let available = host(&mut state).available;
                let value = track(&mut state, available);
                stack.replace(ctx, matches!(value, Some(IpcValue::Boolean(true))));
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "backend",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let mut state = state.borrow_mut();
                let name = host(&mut state).started().backend();
                stack.replace(ctx, name);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    for (name, kind) in [
        ("default_sink", DeviceKind::Sink),
        ("default_source", DeviceKind::Source),
    ] {
        function(
            name,
            Callback::from_fn(&ctx, {
                let state = Rc::clone(&state);
                move |ctx, _, mut stack| {
                    let mut state = state.borrow_mut();
                    follow(&mut state);
                    let audio = host(&mut state).started();
                    let row = audio
                        .state()
                        .default_device(kind)
                        .map(|device| device_row(device, true));
                    let value = match row {
                        Some(row) => row_to_lua(ctx, &row).map_err(HostError)?,
                        None => LuaValue::Nil,
                    };
                    stack.replace(ctx, value);
                    Ok(CallbackReturn::Return)
                }
            }),
        );
    }

    function(
        "device",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let id = object_id(stack.consume(ctx)?, "device")?;
                let mut state = state.borrow_mut();
                follow(&mut state);
                let audio = host(&mut state).started().state();
                let row = audio
                    .device(id)
                    .map(|device| device_row(device, audio.is_default(id)));
                let value = match row {
                    Some(row) => row_to_lua(ctx, &row).map_err(HostError)?,
                    None => LuaValue::Nil,
                };
                stack.replace(ctx, value);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "stream",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let id = object_id(stack.consume(ctx)?, "stream")?;
                let mut state = state.borrow_mut();
                follow(&mut state);
                let row = host(&mut state)
                    .started()
                    .state()
                    .stream(id)
                    .map(stream_row);
                let value = match row {
                    Some(row) => row_to_lua(ctx, &row).map_err(HostError)?,
                    None => LuaValue::Nil,
                };
                stack.replace(ctx, value);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_volume",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (id, volume): (LuaValue, f64) = stack.consume(ctx)?;
                let id = object_id(id, "set_volume target")?;
                if !volume.is_finite() {
                    return Err(HostError("volume must be a finite number".into()).into());
                }
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_volume(id, volume as f32);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_channel_volumes",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (id, volumes): (LuaValue, Table) = stack.consume(ctx)?;
                let id = object_id(id, "set_channel_volumes target")?;
                let mut list = Vec::new();
                for index in 1..=volumes.length(&ctx) {
                    let volume = match volumes.get_value(ctx, index) {
                        LuaValue::Number(value) => value,
                        LuaValue::Integer(value) => value as f64,
                        _ => {
                            return Err(HostError(
                                "set_channel_volumes takes a list of numbers".into(),
                            )
                            .into());
                        }
                    };
                    if !volume.is_finite() {
                        return Err(HostError("volume must be a finite number".into()).into());
                    }
                    list.push(volume as f32);
                }
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_channel_volumes(id, &list);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_mute",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (id, muted): (LuaValue, bool) = stack.consume(ctx)?;
                let id = object_id(id, "set_mute target")?;
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_mute(id, muted);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "set_default",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let id = object_id(stack.consume(ctx)?, "set_default target")?;
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().set_default(id);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "move_stream",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let (stream, device): (LuaValue, LuaValue) = stack.consume(ctx)?;
                let stream = object_id(stream, "stream")?;
                let device = object_id(device, "device")?;
                let mut state = state.borrow_mut();
                let sent = host(&mut state).started().move_stream(stream, device);
                stack.replace(ctx, sent);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "on_changed",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let callback: Closure = stack.consume(ctx)?;
                let id = {
                    let mut state = state.borrow_mut();
                    let host = host(&mut state);
                    if host.listeners.len() >= MAX_LISTENERS {
                        return Err(
                            HostError("too many morf.audio.on_changed handlers".into()).into()
                        );
                    }
                    host.started();
                    let id = host.next_listener;
                    host.next_listener += 1;
                    host.listeners.push((id, ctx.stash(callback)));
                    id
                };
                let stop = Callback::from_fn(&ctx, {
                    let state = Rc::clone(&state);
                    move |_, _, _| {
                        host(&mut state.borrow_mut())
                            .listeners
                            .retain(|(listener, _)| *listener != id);
                        Ok(CallbackReturn::Return)
                    }
                });
                let handle = Table::new(&ctx);
                handle.set_field(ctx, "stop", stop);
                stack.replace(ctx, handle);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    function(
        "monitor",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let options: Table = stack.consume(ctx)?;
                let beat = table_bool(ctx, options, "beat", false)
                    .map_err(|error| HostError(format!("monitor {error}")))?;
                let handlers = MonitorHandlers {
                    on_level: optional_closure(ctx, options, "on_level").map_err(HostError)?,
                    on_beat: optional_closure(ctx, options, "on_beat").map_err(HostError)?,
                    on_tempo: optional_closure(ctx, options, "on_tempo").map_err(HostError)?,
                    tempo: None,
                };
                if handlers.on_level.is_none() && !beat {
                    return Err(HostError("monitor needs an on_level function".into()).into());
                }
                if !beat && (handlers.on_beat.is_some() || handlers.on_tempo.is_some()) {
                    return Err(
                        HostError("monitor on_beat and on_tempo need beat = true".into()).into(),
                    );
                }
                let device = match options.get_value(ctx, "device") {
                    LuaValue::Nil => None,
                    value => Some(object_id(value, "monitor device")?),
                };
                let rate_hz = table_number(ctx, options, "rate_hz", 30.0).map_err(HostError)?;
                // `delay`: milliseconds, or "device" for as long as the device
                // takes to play what it is given (a Bluetooth headset's 250 ms).
                let delay = match options.get_value(ctx, "delay") {
                    LuaValue::Nil => morf_audio::MonitorDelay::None,
                    LuaValue::Integer(ms) => morf_audio::MonitorDelay::Fixed(ms as f32),
                    LuaValue::Number(ms) => morf_audio::MonitorDelay::Fixed(ms as f32),
                    LuaValue::String(word) if word.as_bytes() == b"device" => morf_audio::MonitorDelay::Device,
                    _ => {
                        return Err(HostError(
                            "monitor delay must be milliseconds or \"device\"".into(),
                        )
                        .into());
                    }
                };
                let bands = table_number(ctx, options, "bands", 0.0).map_err(HostError)?;
                if !(0.0..=morf_audio::dsp::MAX_BANDS as f64).contains(&bands) {
                    return Err(HostError(format!(
                        "monitor bands must be 0 to {}",
                        morf_audio::dsp::MAX_BANDS
                    ))
                    .into());
                }
                let id = {
                    let mut state = state.borrow_mut();
                    let host = host(&mut state);
                    if host.monitors.len() >= MAX_MONITORS {
                        return Err(HostError("too many audio monitors running".into()).into());
                    }
                    let id = host.started().monitor_delayed(
                        device,
                        rate_hz as f32,
                        bands as usize,
                        beat,
                        delay,
                    );
                    host.monitors.insert(id, handlers);
                    id
                };
                let stop = Callback::from_fn(&ctx, {
                    let state = Rc::clone(&state);
                    move |_, _, _| {
                        let mut state = state.borrow_mut();
                        let host = host(&mut state);
                        if host.monitors.remove(&id).is_some() {
                            host.started().stop_monitor(id);
                        }
                        Ok(CallbackReturn::Return)
                    }
                });
                // `bpm` and `confidence` are read from the host as they are
                // asked for, so they are always the latest.
                let tempo = Callback::from_fn(&ctx, {
                    let state = Rc::clone(&state);
                    move |ctx, _, mut stack| {
                        let (_, key): (Table, LuaValue) = stack.consume(ctx)?;
                        let mut state = state.borrow_mut();
                        let tempo = host(&mut state)
                            .monitors
                            .get(&id)
                            .and_then(|handlers| handlers.tempo);
                        let value = match (key, tempo) {
                            (LuaValue::String(key), Some((bpm, confidence))) => {
                                match key.as_bytes() {
                                    b"bpm" => LuaValue::Number(f64::from(bpm)),
                                    b"confidence" => LuaValue::Number(f64::from(confidence)),
                                    _ => LuaValue::Nil,
                                }
                            }
                            _ => LuaValue::Nil,
                        };
                        stack.replace(ctx, value);
                        Ok(CallbackReturn::Return)
                    }
                });
                let handle = Table::new(&ctx);
                handle.set_field(ctx, "stop", stop);
                handle.set_field(ctx, "id", id as i64);
                let metatable = Table::new(&ctx);
                metatable.set_field(ctx, "__index", tempo);
                handle.set_metatable(ctx, Some(metatable));
                stack.replace(ctx, handle);
                Ok(CallbackReturn::Return)
            }
        }),
    );

    // The lists, through `__index`, so reading one is what starts the audio.
    let index = Callback::from_fn(&ctx, {
        let state = Rc::clone(&state);
        move |ctx, _, mut stack| {
            let (_, key): (Table, String) = stack.consume(ctx)?;
            let mut state = state.borrow_mut();
            let model = {
                let host = host(&mut state);
                let model = match key.as_str() {
                    "sinks" => &host.sinks,
                    "sources" => &host.sources,
                    "streams" => &host.streams,
                    _ => {
                        stack.replace(ctx, LuaValue::Nil);
                        return Ok(CallbackReturn::Return);
                    }
                };
                let model = Rc::clone(model);
                host.started();
                model
            };
            let userdata = list_model_userdata(ctx, &state, &model)?;
            stack.replace(ctx, userdata);
            Ok(CallbackReturn::Return)
        }
    });
    let metatable = Table::new(&ctx);
    metatable.set_field(ctx, "__index", index);
    audio.set_metatable(ctx, Some(metatable));
    morf.set_field(ctx, "audio", audio);
}

/// Runs a handler with arguments that may be tables.
pub(crate) fn execute_audio_handler(
    ctx: Context<'_>,
    closure: &StashedClosure,
    args: &[SceneValue],
    limits: Limits,
) -> Result<(), String> {
    let args = args
        .iter()
        .map(|value| scene_to_lua(ctx, value))
        .collect::<Result<Vec<_>, _>>()?;
    let executor = Executor::start(ctx, ctx.fetch(closure).into(), Variadic(args));
    drive_executor(ctx, executor, limits, limits.effect_fuel, "audio handler")?;
    match executor.take_result::<()>(ctx) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}
