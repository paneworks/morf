//! `morf.audio.monitor`: levels, bands, beats and tempo from a device, to
//! handlers or straight to a data channel.

use super::*;

/// Installs `monitor` on `audio`.
pub(super) fn install_monitor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    audio: Table<'gc>,
) {
    let function =
        |name: &'static str, callback: Callback<'gc>| audio.set_field(ctx, name, callback);

    function(
        "monitor",
        Callback::from_fn(&ctx, {
            let state = Rc::clone(&state);
            move |ctx, _, mut stack| {
                let options: Table = stack.consume(ctx)?;
                let beat = table_bool(ctx, options, "beat", false)
                    .map_err(|error| HostError(format!("monitor {error}")))?;
                let handlers = Monitor {
                    on_level: optional_closure(ctx, options, "on_level").map_err(HostError)?,
                    on_beat: optional_closure(ctx, options, "on_beat").map_err(HostError)?,
                    on_tempo: optional_closure(ctx, options, "on_tempo").map_err(HostError)?,
                    tempo: None,
                    channel: match options.get_value(ctx, "channel") {
                        LuaValue::Nil => None,
                        LuaValue::Table(handle) => {
                            let id = match handle.get_value(ctx, "id") {
                                LuaValue::Integer(id) => id as u64,
                                LuaValue::Number(id) => id as u64,
                                _ => {
                                    return Err(HostError(
                                        "monitor channel must be a morf.channel".into(),
                                    )
                                    .into());
                                }
                            };
                            let channel = morf_scene::channel_by_id(id)
                                .ok_or_else(|| HostError("monitor channel is gone".into()))?;
                            let filter = match options.get_value(ctx, "spectrum") {
                                LuaValue::Nil => None,
                                LuaValue::Table(o) => Some(
                                    morf_audio::spectrum::Filter::new(
                                        crate::api_audio_spectrum::options(ctx, Some(o))?,
                                    )
                                    .map_err(HostError)?,
                                ),
                                _ => {
                                    return Err(HostError(
                                        "monitor spectrum must be a table".into(),
                                    )
                                    .into());
                                }
                            };
                            Some(MonitorChannel::new(channel, filter))
                        }
                        _ => {
                            return Err(
                                HostError("monitor channel must be a morf.channel".into()).into()
                            );
                        }
                    },
                };
                if handlers.on_level.is_none() && handlers.channel.is_none() && !beat {
                    return Err(HostError(
                        "monitor needs an on_level function or a channel".into(),
                    )
                    .into());
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
                    LuaValue::String(word) if word.as_bytes() == b"device" => {
                        morf_audio::MonitorDelay::Device
                    }
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
                let spec = MonitorSpec {
                    device,
                    rate_hz: rate_hz as f32,
                    bands: bands as usize,
                    beat,
                    delay,
                };
                let id = host(&mut state.borrow_mut())
                    .session
                    .monitor(spec, handlers)
                    .map_err(HostError)?;
                let stop = Callback::from_fn(&ctx, {
                    let state = Rc::clone(&state);
                    move |_, _, _| {
                        host(&mut state.borrow_mut()).session.stop_monitor(id);
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
                        let tempo = host(&mut state).session.tempo(id);
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
}
