//! `morf.audio` against a sound server that exists only in memory.

use morf_audio::fake::{FakeServer, fake};
use morf_audio::{
    Backend, Command, Control, Device, DeviceKind, Direction, Events, Stream, Update,
};

use super::*;

fn device(id: u32, name: &str, kind: DeviceKind, gain: f32) -> Device {
    Device {
        id,
        name: name.into(),
        description: format!("{name} description"),
        kind,
        channel_volumes: vec![gain, gain],
        muted: false,
        icon_name: Some("audio-card".into()),
    }
}

fn stream(id: u32, device: u32) -> Stream {
    Stream {
        id,
        app_name: "Player".into(),
        app_id: Some("org.example.Player".into()),
        binary: Some("player".into()),
        icon_name: None,
        media_name: Some("A Song".into()),
        direction: Direction::Playback,
        device: Some(device),
        channel_volumes: vec![1.0, 1.0],
        muted: false,
        pid: Some(4242),
    }
}

fn machine() -> Vec<Update> {
    vec![
        Update::Device(device(40, "speakers", DeviceKind::Sink, 0.125)),
        Update::Device(device(41, "headphones", DeviceKind::Sink, 1.0)),
        Update::Device(device(50, "mic", DeviceKind::Source, 1.0)),
        Update::DefaultSink(Some("speakers".into())),
        Update::DefaultSource(Some("mic".into())),
        Update::Stream(stream(70, 40)),
    ]
}

fn runtime_with(server_seed: Vec<Update>) -> (Runtime, FakeServer) {
    let (backend, server) = fake(server_seed);
    let mut runtime = Runtime::default();
    runtime.set_audio_backend(backend);
    (runtime, server)
}

fn root_text(runtime: &Runtime, index: usize) -> String {
    let root = runtime.scene().roots()[index];
    runtime
        .scene()
        .string_value(root, "text")
        .unwrap()
        .to_owned()
}

#[test]
fn bindings_follow_the_default_sink_and_lists_fill() {
    let (mut runtime, server) = runtime_with(machine());
    runtime
        .execute(
            "audio.lua",
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local audio = morf.audio
            ui.Text { text = function()
                local sink = audio.default_sink()
                if not sink then return "none" end
                return sink.description .. " " .. math.floor(sink.volume * 100 + 0.5)
                    .. (sink.muted and " muted" or "")
            end }
            ui.Text { text = function()
                return tostring(audio.available()) .. " " .. audio.sinks:len() .. " "
                    .. audio.sources:len() .. " " .. audio.streams:len()
            end }
            morf.ipc.volume = function(value)
                return tostring(audio.set_volume(audio.default_sink().id, tonumber(value)))
            end
            morf.ipc.balance = function()
                return tostring(audio.set_channel_volumes(40, { 0.25, 1 }))
                    .. " " .. tostring(audio.set_channel_volumes(40, {}))
                    .. " " .. tostring(audio.set_channel_volumes(99, { 1 }))
            end
            morf.ipc.mute = function()
                return tostring(audio.set_mute(40, true))
            end
            morf.ipc.default = function()
                local second = audio.sinks:get(2)
                return tostring(audio.set_default(second.id))
            end
            morf.ipc.row = function()
                local row = audio.sinks:get(1)
                local s = audio.stream(70)
                return row.name .. " " .. row.kind .. " " .. tostring(row.default) .. " "
                    .. math.tointeger(row.channels) .. " " .. s.app_name .. " " .. s.direction .. " "
                    .. math.tointeger(s.device) .. " " .. s.binary .. " " .. s.media_name
            end
            morf.ipc.move = function()
                return tostring(audio.move_stream(70, 50)) .. " " .. tostring(audio.move_stream(70, 41))
            end
            "#,
        )
        .unwrap();
    assert_eq!(root_text(&runtime, 0), "none");
    assert_eq!(root_text(&runtime, 1), "false 0 0 0");
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "speakers description 50");
    assert_eq!(root_text(&runtime, 1), "true 2 1 1");
    assert_eq!(
        runtime.call_ipc("row", &[]).unwrap(),
        vec![IpcValue::String(
            "speakers sink true 2 Player playback 40 player A Song".into()
        )]
    );

    assert_eq!(
        runtime
            .call_ipc("volume", &[IpcValue::String("0.8".into())])
            .unwrap(),
        vec![IpcValue::String("true".into())]
    );
    let Some(Command::SetVolumes { id: 40, volumes }) = server.commands().pop() else {
        panic!("a volume command reached the server");
    };
    assert!((morf_audio::volume::average(&volumes) - 0.8).abs() < 1e-4);
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "speakers description 80");

    // Each channel on its own: left quiet, right full.
    assert_eq!(
        runtime.call_ipc("balance", &[]).unwrap(),
        vec![IpcValue::String("true false false".into())]
    );
    let Some(Command::SetVolumes { id: 40, volumes }) = server.commands().pop() else {
        panic!("a channel volume command reached the server");
    };
    assert_eq!(volumes.len(), 2);
    assert!((morf_audio::volume::from_linear(volumes[0]) - 0.25).abs() < 1e-4);
    assert!((morf_audio::volume::from_linear(volumes[1]) - 1.0).abs() < 1e-4);
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "speakers description 63");
    // The overall level back at 80, the balance kept.
    runtime
        .call_ipc("volume", &[IpcValue::String("0.8".into())])
        .unwrap();
    runtime.poll_services();

    runtime.call_ipc("mute", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "speakers description 80 muted");

    runtime.call_ipc("default", &[]).unwrap();
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "headphones description 100");

    assert_eq!(
        runtime.call_ipc("move", &[]).unwrap(),
        vec![IpcValue::String("false true".into())]
    );

    // The server going away empties everything and says so.
    server.push(Update::Available(false));
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "none");
    assert_eq!(root_text(&runtime, 1), "false 0 0 0");
}

#[test]
fn a_repeater_follows_the_sinks() {
    let (mut runtime, server) = runtime_with(machine());
    runtime
        .execute(
            "mixer.lua",
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            ui.Column {
                ui.Repeater {
                    model = morf.audio.sinks,
                    delegate = function(sink)
                        return ui.Text { text = sink.description }
                    end,
                },
            }
            "#,
        )
        .unwrap();
    runtime.poll_services();
    let column = runtime.scene().roots()[0];
    let count = |runtime: &Runtime| {
        fn texts(runtime: &Runtime, node: morf_scene::NodeHandle, out: &mut Vec<String>) {
            if let Ok(text) = runtime.scene().string_value(node, "text") {
                out.push(text.to_owned());
            }
            for child in runtime.scene().children(node).unwrap_or_default() {
                texts(runtime, *child, out);
            }
        }
        let mut out = Vec::new();
        texts(runtime, column, &mut out);
        out
    };
    assert_eq!(
        count(&runtime),
        vec!["speakers description", "headphones description"]
    );
    server.push(Update::DeviceRemoved(40));
    runtime.poll_services();
    runtime.poll_services();
    assert_eq!(count(&runtime), vec!["headphones description"]);
}

#[test]
fn handlers_hear_changes_and_meters_hear_levels() {
    let (mut runtime, server) = runtime_with(machine());
    runtime
        .execute(
            "meter.lua",
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local changes = morf.signal("changes", "")
            local level = morf.signal("level", "")
            local watcher = morf.audio.on_changed(function(what)
                changes:set(changes:get() .. (what.devices and "d" or "")
                    .. (what.defaults and "D" or "") .. (what.streams and "s" or "") .. ";")
            end)
            local meter = morf.audio.monitor {
                bands = 4, rate_hz = 20,
                on_level = function(left, right, bands)
                    level:set(string.format("%.1f %.1f %d", left, right, bands and #bands or 0))
                end,
            }
            ui.Text { text = function() return changes:get() end }
            ui.Text { text = function() return level:get() end }
            morf.ipc.stop = function() meter:stop(); watcher:stop() end
            "#,
        )
        .unwrap();
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "dDs;");
    assert_eq!(server.monitors().len(), 1);
    server.level(0.5, 0.25, vec![0.1, 0.2, 0.3, 0.4]);
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 1), "0.5 0.2 4");

    server.push(Update::Stream(Stream {
        muted: true,
        ..stream(70, 40)
    }));
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "dDs;s;");

    runtime.call_ipc("stop", &[]).unwrap();
    assert!(server.monitors().is_empty());
    server.level(0.9, 0.9, Vec::new());
    server.push(Update::StreamRemoved(70));
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), "dDs;s;");
    assert_eq!(root_text(&runtime, 1), "0.5 0.2 4");
}

/// A server that never answers: no sound server on this machine.
struct Silent;

struct Nothing;

impl Control for Nothing {
    fn send(&self, _command: Command) {}
}

impl Backend for Silent {
    fn name(&self) -> &'static str {
        "silent"
    }

    fn start(self: Box<Self>, _events: Events) -> Box<dyn Control> {
        Box::new(Nothing)
    }
}

#[test]
fn without_a_server_everything_is_empty_and_nothing_errors() {
    let mut runtime = Runtime::default();
    runtime.set_audio_backend(Silent);
    runtime
        .execute(
            "none.lua",
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local audio = morf.audio
            local meter = audio.monitor { on_level = function() end }
            meter:stop()
            local handle = audio.on_changed(function() end)
            handle:stop()
            ui.Text { text = table.concat({
                tostring(audio.available()), audio.backend(), tostring(audio.default_sink()),
                tostring(audio.default_source()), audio.sinks:len(), audio.streams:len(),
                tostring(audio.set_volume(1, 0.5)), tostring(audio.set_mute(1, true)),
                tostring(audio.set_default(1)), tostring(audio.move_stream(1, 2)),
                tostring(audio.device(1)), tostring(audio.stream(1)),
            }, " ") }
            "#,
        )
        .unwrap();
    runtime.poll_services();
    assert_eq!(
        root_text(&runtime, 0),
        "false silent nil nil 0 0 false false false false nil nil"
    );
}

#[test]
fn malformed_calls_raise() {
    let (mut runtime, _server) = runtime_with(Vec::new());
    runtime
        .execute(
            "bad.lua",
            br#"
            local audio = require("morf").audio
            assert(not pcall(audio.set_volume, "speakers", 0.5))
            assert(not pcall(audio.set_volume, 1, 0 / 0))
            assert(not pcall(audio.set_volume, -1, 0.5))
            assert(not pcall(audio.set_channel_volumes, 1, { "loud" }))
            assert(not pcall(audio.set_channel_volumes, 1, { 0 / 0 }))
            assert(not pcall(audio.monitor, {}))
            assert(not pcall(audio.monitor, { on_level = print, bands = 1000 }))
            assert(not pcall(audio.monitor, { on_level = print, on_beat = print }))
            assert(not pcall(audio.monitor, { beat = "yes", on_beat = print }))
            assert(audio.nonsense == nil)
            "#,
        )
        .unwrap();
}

/// A decaying 1 kHz click every beat of `bpm`, stereo at 48 kHz.
fn click_track(bpm: f32, seconds: f32) -> Vec<f32> {
    let rate = 48_000.0;
    let period = (60.0 / bpm * rate) as usize;
    (0..(seconds * rate) as usize)
        .flat_map(|index| {
            let t = (index % period) as f32 / rate;
            let sample =
                (2.0 * std::f32::consts::PI * 1_000.0 * t).sin() * (-t * 300.0).exp() * 0.8;
            [sample, sample]
        })
        .collect()
}

#[test]
fn a_monitor_hears_beats_and_a_tempo() {
    let (mut runtime, server) = runtime_with(machine());
    runtime
        .execute(
            "beats.lua",
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local beats = morf.signal("beats", 0)
            local tempo = morf.signal("tempo", "")
            meter = morf.audio.monitor {
                beat = true,
                on_beat = function(strength)
                    assert(strength > 0 and strength <= 1)
                    beats:set(beats:get() + 1)
                end,
                on_tempo = function(bpm, confidence)
                    tempo:set(string.format("%d %s", math.floor(bpm + 0.5), confidence > 0.5))
                end,
            }
            assert(meter.bpm == nil and meter.confidence == nil)
            ui.Text { text = function() return tostring(beats:get()) end }
            ui.Text { text = function() return tempo:get() end }
            morf.ipc.bpm = function() return math.floor(meter.bpm + 0.5) end
            morf.ipc.stop = function() meter:stop() end
            "#,
        )
        .unwrap();
    runtime.poll_services();
    for second in click_track(120.0, 10.0).chunks(96_000) {
        server.play(48_000, 2, second);
        runtime.poll_services();
    }
    let beats: usize = root_text(&runtime, 0).parse().unwrap();
    assert!((18..=20).contains(&beats), "{beats} beats");
    assert_eq!(root_text(&runtime, 1), "120 true");
    assert!(matches!(
        runtime.call_ipc("bpm", &[]).unwrap().as_slice(),
        [IpcValue::Integer(120)] | [IpcValue::Number(120.0)]
    ));
    runtime.call_ipc("stop", &[]).unwrap();
    assert!(server.monitors().is_empty());
    server.play(48_000, 2, &click_track(120.0, 2.0));
    runtime.poll_services();
    assert_eq!(root_text(&runtime, 0), beats.to_string());
}

#[test]
fn a_plain_monitor_hears_no_beats() {
    let (mut runtime, server) = runtime_with(machine());
    runtime
        .execute(
            "levels.lua",
            br#"
            local morf = require("morf")
            local ui = require("morf.ui")
            local levels = morf.signal("levels", 0)
            local meter = morf.audio.monitor {
                rate_hz = 10,
                on_level = function() levels:set(levels:get() + 1) end,
            }
            ui.Text { text = function() return tostring(levels:get()) end }
            morf.ipc.bpm = function() return meter.bpm == nil and "none" or "some" end
            "#,
        )
        .unwrap();
    runtime.poll_services();
    for second in click_track(120.0, 4.0).chunks(96_000) {
        server.play(48_000, 2, second);
        runtime.poll_services();
    }
    assert_eq!(root_text(&runtime, 0), "4");
    assert!(matches!(
        runtime.call_ipc("bpm", &[]).unwrap().as_slice(),
        [IpcValue::String(none)] if none == "none"
    ));
}
