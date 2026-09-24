use super::*;
use crate::pipewire::pod::{self, Pod};

fn sink(id: ObjectId, name: &str, volumes: &[f32]) -> Device {
    Device {
        id,
        name: name.to_owned(),
        description: format!("{name} speakers"),
        kind: DeviceKind::Sink,
        channel_volumes: volumes.to_vec(),
        muted: false,
        icon_name: None,
    }
}

fn source(id: ObjectId, name: &str) -> Device {
    Device {
        kind: DeviceKind::Source,
        ..sink(id, name, &[1.0])
    }
}

fn stream(id: ObjectId, direction: Direction, device: Option<ObjectId>) -> Stream {
    Stream {
        id,
        app_name: "player".into(),
        app_id: None,
        binary: Some("player".into()),
        icon_name: None,
        media_name: Some("a song".into()),
        direction,
        device,
        channel_volumes: vec![1.0, 1.0],
        muted: false,
        pid: Some(42),
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn volume_is_cubic_and_bounded() {
    assert!(close(volume::from_linear(0.125), 0.5));
    assert!(close(volume::to_linear(0.5), 0.125));
    assert!(close(volume::to_linear(2.0), 1.5 * 1.5 * 1.5));
    assert_eq!(volume::to_linear(f32::NAN), 0.0);
    assert_eq!(volume::from_linear(-1.0), 0.0);
    assert!(close(volume::average(&[1.0, 0.125]), 0.75));
    assert_eq!(volume::average(&[]), 0.0);
}

#[test]
fn scaling_keeps_the_balance_between_channels() {
    // Left at 100%, right at 50% (as shown): average 75%.
    let gains = [1.0, 0.125];
    let scaled = volume::scale(&gains, 0.375);
    assert!(close(volume::average(&scaled), 0.375));
    assert!(close(volume::from_linear(scaled[0]), 0.5));
    assert!(close(volume::from_linear(scaled[1]), 0.25));
    // Silence has no balance: every channel comes back level.
    let level = volume::scale(&[0.0, 0.0], 0.5);
    assert!(close(level[0], 0.125) && close(level[1], 0.125));
    // An unknown channel count becomes one gain.
    assert_eq!(volume::scale(&[], 1.0), vec![1.0]);
}

#[test]
fn devices_and_streams_are_kept_by_id() {
    let mut state = AudioState::default();
    assert!(state.apply(Update::Available(true)).available);
    let changes = state.apply(Update::Device(sink(40, "speakers", &[1.0, 1.0])));
    assert!(changes.devices && !changes.defaults);
    state.apply(Update::Device(source(41, "mic")));
    assert_eq!(state.devices(DeviceKind::Sink).count(), 1);
    assert_eq!(state.devices(DeviceKind::Source).count(), 1);
    // The same report twice changes nothing.
    assert!(
        !state
            .apply(Update::Device(sink(40, "speakers", &[1.0, 1.0])))
            .any()
    );
    let changes = state.apply(Update::Stream(stream(70, Direction::Playback, Some(40))));
    assert!(changes.streams);
    assert_eq!(state.stream(70).unwrap().device, Some(40));
    assert!(state.apply(Update::StreamRemoved(70)).streams);
    assert!(!state.apply(Update::StreamRemoved(70)).streams);
    assert!(state.apply(Update::DeviceRemoved(41)).devices);
    assert!(state.device(41).is_none());
}

#[test]
fn defaults_follow_names_whenever_they_arrive() {
    let mut state = AudioState::default();
    state.apply(Update::Available(true));
    // The default is named before its device exists...
    assert!(
        state
            .apply(Update::DefaultSink(Some("hdmi".into())))
            .defaults
    );
    assert!(state.default_device(DeviceKind::Sink).is_none());
    // ...and becomes it when it appears.
    let changes = state.apply(Update::Device(sink(5, "hdmi", &[1.0])));
    assert!(changes.devices && changes.defaults);
    assert_eq!(state.default_device(DeviceKind::Sink).unwrap().id, 5);
    assert!(state.is_default(5));
    state.apply(Update::Device(sink(6, "speakers", &[1.0])));
    assert!(!state.is_default(6));
    // Switching.
    assert!(
        state
            .apply(Update::DefaultSink(Some("speakers".into())))
            .defaults
    );
    assert!(state.is_default(6) && !state.is_default(5));
    // The default going away is a change of default.
    assert!(state.apply(Update::DeviceRemoved(6)).defaults);
    assert!(state.default_device(DeviceKind::Sink).is_none());
    // A volume change on a default is not.
    state.apply(Update::DefaultSink(Some("hdmi".into())));
    assert!(
        !state
            .apply(Update::Device(sink(5, "hdmi", &[0.5])))
            .defaults
    );
}

#[test]
fn losing_the_server_forgets_everything() {
    let mut state = AudioState::default();
    state.apply(Update::Available(true));
    state.apply(Update::Device(sink(1, "a", &[1.0])));
    state.apply(Update::Stream(stream(2, Direction::Record, None)));
    state.apply(Update::DefaultSink(Some("a".into())));
    let changes = state.apply(Update::Available(false));
    assert!(changes.available && changes.devices && changes.streams && changes.defaults);
    assert!(!state.available());
    assert_eq!(state.devices(DeviceKind::Sink).count(), 0);
    assert_eq!(state.streams().count(), 0);
}

#[test]
fn audio_round_trips_through_a_fake_server() {
    let (backend, server) = fake::fake(vec![
        Update::Device(sink(10, "speakers", &[1.0, 0.125])),
        Update::Device(sink(11, "headphones", &[1.0, 1.0])),
        Update::Device(source(12, "mic")),
        Update::DefaultSink(Some("speakers".into())),
        Update::Stream(stream(20, Direction::Playback, Some(10))),
    ]);
    let mut audio = Audio::with_backend(backend);
    assert_eq!(audio.backend(), "fake");
    let poll = audio.poll();
    assert!(poll.changes.available && poll.changes.devices && poll.changes.defaults);
    assert!(audio.state().available());
    assert_eq!(
        audio.state().default_device(DeviceKind::Sink).unwrap().id,
        10
    );

    // Volume keeps balance and comes back as the device.
    assert!(audio.set_volume(10, 0.375));
    let Some(Command::SetVolumes { id: 10, volumes }) = server.commands().pop() else {
        panic!("expected a volume command");
    };
    assert!(close(volume::average(&volumes), 0.375));
    assert!(audio.poll().changes.devices);
    assert!(close(audio.state().device(10).unwrap().volume(), 0.375));

    assert!(audio.set_mute(20, true));
    audio.poll();
    assert!(audio.state().stream(20).unwrap().muted);

    assert!(audio.set_default(11));
    assert!(audio.poll().changes.defaults);
    assert!(audio.state().is_default(11));

    // Playback moves to sinks only.
    assert!(!audio.move_stream(20, 12));
    assert!(audio.move_stream(20, 11));
    audio.poll();
    assert_eq!(audio.state().stream(20).unwrap().device, Some(11));

    // Unknown ids are refused without a command.
    let sent = server.commands().len();
    assert!(!audio.set_volume(99, 1.0));
    assert!(!audio.set_mute(99, true));
    assert!(!audio.set_default(99));
    assert_eq!(server.commands().len(), sent);
}

#[test]
fn levels_collapse_to_one_per_monitor_per_poll() {
    let (backend, server) = fake::fake(Vec::new());
    let mut audio = Audio::with_backend(backend);
    let monitor = audio.monitor(None, 500.0, 4);
    assert_eq!(server.monitors(), vec![monitor]);
    assert!(matches!(
        server.commands().last(),
        Some(Command::StartMonitor { rate_hz, bands: 4, device: None, .. }) if *rate_hz == 120.0
    ));
    server.level(0.9, 0.1, vec![0.0; 4]);
    server.level(0.2, 0.3, vec![1.0; 4]);
    let poll = audio.poll();
    assert_eq!(poll.levels.len(), 1);
    let level = &poll.levels[0];
    assert_eq!(
        (level.monitor, level.left, level.right),
        (monitor, 0.9, 0.3)
    );
    assert_eq!(level.bands, vec![1.0; 4]);
    // Sound that stops is silence, even after a loud reading in the same poll.
    server.level(0.7, 0.7, vec![0.5; 4]);
    server.level(0.0, 0.0, vec![0.0; 4]);
    let poll = audio.poll();
    assert_eq!((poll.levels[0].left, poll.levels[0].right), (0.0, 0.0));
    audio.stop_monitor(monitor);
    assert!(server.monitors().is_empty());
}

#[test]
fn a_meter_falls_silent_when_asked() {
    let mut meter = Meter::new(10.0, 4);
    meter.set_format(1000, 2);
    assert!(meter.push(&[0.8; 150]).is_none());
    let silence = meter.silence();
    assert_eq!((silence.left, silence.right), (0.0, 0.0));
    assert_eq!(silence.bands, vec![0.0; 4]);
    // The half window before it is forgotten too.
    let reading = meter.push(&[0.0; 200]).unwrap();
    assert_eq!((reading.left, reading.right), (0.0, 0.0));
}

#[test]
fn an_unavailable_audio_is_empty_and_quiet() {
    let mut audio = Audio::unavailable();
    assert!(!audio.poll().changes.any());
    assert!(!audio.state().available());
    assert!(!audio.set_volume(1, 0.5));
    let monitor = audio.monitor(None, 30.0, 0);
    audio.stop_monitor(monitor);
}

#[test]
fn a_meter_reads_peaks_per_window() {
    let mut meter = Meter::new(10.0, 0);
    meter.set_format(1000, 2);
    // 100 frames make a window at 10 Hz and 1 kHz.
    let mut frames = vec![0.0f32; 2 * 99];
    frames[10] = -0.5;
    frames[21] = 0.25;
    assert!(meter.push(&frames).is_none());
    let reading = meter.push(&[0.0, 0.0]).expect("the window is full");
    assert_eq!((reading.left, reading.right), (0.5, 0.25));
    assert!(reading.bands.is_empty());
    // Peaks reset after a reading; mono counts for both sides.
    meter.set_format(1000, 1);
    let reading = meter.push(&vec![0.1; 100]).unwrap();
    assert!(close(reading.left, 0.1) && close(reading.right, 0.1));
}

use crate::dsp::Meter;

#[test]
fn bands_find_a_tone_where_it_is() {
    let rate = 48_000;
    let mut meter = Meter::new(10.0, 8);
    meter.set_format(rate, 2);
    let tone = 1_000.0;
    let frames: Vec<f32> = (0..4800)
        .flat_map(|index| {
            let sample = (2.0 * std::f32::consts::PI * tone * index as f32 / rate as f32).sin();
            [sample, sample]
        })
        .collect();
    let reading = meter.push(&frames).unwrap();
    assert_eq!(reading.bands.len(), 8);
    let ranges = dsp::band_ranges(8, rate, 2048);
    let bin = (tone / (rate as f32 / 2048.0)).round() as usize;
    let loudest = reading
        .bands
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    assert!(ranges[loudest].0 <= bin && bin < ranges[loudest].1 + 1);
    assert!(reading.bands[loudest] > 0.9, "{:?}", reading.bands);
    // Far from the tone is well down.
    assert!(reading.bands[7] < 0.5, "{:?}", reading.bands);
}

#[test]
fn band_ranges_cover_every_band() {
    for count in [1, 8, 32, 128] {
        let ranges = dsp::band_ranges(count, 44_100, 2048);
        assert_eq!(ranges.len(), count);
        assert!(
            ranges
                .iter()
                .all(|(start, end)| start < end && *end <= 1024)
        );
    }
}

#[test]
fn pods_round_trip() {
    let route = Pod::object(
        pod::OBJECT_ROUTE,
        13,
        vec![
            (pod::ROUTE_INDEX, Pod::Int(3)),
            (pod::ROUTE_DEVICE, Pod::Int(7)),
            (
                pod::ROUTE_PROPS,
                Pod::object(
                    pod::OBJECT_PROPS,
                    2,
                    vec![
                        (pod::PROP_CHANNEL_VOLUMES, Pod::floats(&[0.5, 0.25])),
                        (pod::PROP_MUTE, Pod::Bool(true)),
                    ],
                ),
            ),
            (pod::ROUTE_SAVE, Pod::Bool(true)),
            (1234, Pod::String("hdmi".into())),
        ],
    );
    let bytes = route.encode().bytes();
    assert_eq!(bytes.len() % 8, 0);
    let decoded = Pod::decode(&bytes).unwrap();
    assert_eq!(decoded, route);
    let props = decoded.property(pod::ROUTE_PROPS).unwrap();
    assert_eq!(
        props
            .property(pod::PROP_CHANNEL_VOLUMES)
            .and_then(Pod::as_floats),
        Some(vec![0.5, 0.25])
    );
    // Cut short, it is refused rather than misread.
    assert!(Pod::decode(&bytes[..bytes.len() - 16]).is_none());
}

#[test]
fn pods_match_libspa_layout() {
    // spa_pod_builder_add_object(SPA_TYPE_OBJECT_Props, SPA_PARAM_Props,
    //     SPA_PROP_mute, SPA_POD_Bool(true)) as libspa lays it out.
    let expected: [u32; 10] = [
        32, 15, // object header: body size (padding included), SPA_TYPE_Object
        0x40002, 2, // SPA_TYPE_OBJECT_Props, SPA_PARAM_Props
        0x10004, 0, // key, flags
        4, 2, // bool header
        1, 0, // value, padding
    ];
    let encoded = Pod::object(
        pod::OBJECT_PROPS,
        2,
        vec![(pod::PROP_MUTE, Pod::Bool(true))],
    )
    .encode()
    .bytes();
    let words: Vec<u32> = encoded
        .chunks(4)
        .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
        .collect();
    assert_eq!(words, expected);
}

#[test]
fn choices_read_as_their_default() {
    // Choice(None) of one Int 48000: type, flags, child header, value.
    let words: [u32; 6] = [16 + 4, 19, 0, 0, 4, 4];
    let mut bytes: Vec<u8> = words.iter().flat_map(|word| word.to_ne_bytes()).collect();
    bytes.extend_from_slice(&48_000i32.to_ne_bytes());
    bytes.extend_from_slice(&[0; 4]);
    assert_eq!(Pod::decode(&bytes), Some(Pod::Int(48_000)));
}

#[test]
fn metadata_names_parse_and_quote() {
    use crate::pipewire::{json_name, json_quote};
    assert_eq!(
        json_name(r#"{ "name": "alsa_output.pci-0000_00_1f.3.analog-stereo" }"#).as_deref(),
        Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
    );
    assert_eq!(json_name(r#"{"name":"a\"bA"}"#).as_deref(), Some("a\"bA"));
    assert_eq!(json_name("{}"), None);
    assert_eq!(json_quote("a\"b\\c"), r#""a\"b\\c""#);
    assert_eq!(
        json_name(&format!("{{ \"name\": {} }}", json_quote("x\"y"))).as_deref(),
        Some("x\"y")
    );
}
