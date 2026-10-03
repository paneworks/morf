//! Lists this machine's audio devices and streams, then meters the default
//! sink for a moment. Read-only: it changes no volume, mute or default.
//!
//!     cargo run -p morf-audio --example audio_smoke [seconds-of-metering]

use std::time::{Duration, Instant};

use morf_audio::{Audio, DeviceKind};

fn main() {
    let seconds: f64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1.5);
    let mut audio = Audio::connect();
    println!("backend: {}", audio.backend());
    // Started before the server has even answered, as a configuration does.
    let monitor = (seconds > 0.0).then(|| audio.monitor(None, 10.0, 8));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !audio.state().available() && Instant::now() < deadline {
        audio.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    // Let whatever was still arriving arrive.
    std::thread::sleep(Duration::from_millis(200));
    for error in audio.poll().errors {
        println!("error: {error}");
    }
    let state = audio.state();
    if !state.available() {
        println!("no sound server reachable");
        return;
    }
    for kind in [DeviceKind::Sink, DeviceKind::Source] {
        println!("{}s:", kind.name());
        for device in state.devices(kind) {
            println!(
                "  {}{:>4}  {:<40} vol {:>4.0}% {}ch{}  [{}]{}",
                if state.is_default(device.id) {
                    "*"
                } else {
                    " "
                },
                device.id,
                device.description,
                device.volume() * 100.0,
                device.channels(),
                if device.muted { " muted" } else { "" },
                device.name,
                device
                    .icon_name
                    .as_deref()
                    .map(|icon| format!(" icon {icon}"))
                    .unwrap_or_default(),
            );
        }
    }
    println!("streams:");
    for stream in state.streams() {
        println!(
            "  {:>4}  {:<9} {:<20} {:<30} vol {:>4.0}%{}  -> {:?}  bin {:?} icon {:?}",
            stream.id,
            stream.direction.name(),
            stream.app_name,
            stream.media_name.as_deref().unwrap_or(""),
            stream.volume() * 100.0,
            if stream.muted { " muted" } else { "" },
            stream.device,
            stream.binary,
            stream.icon_name,
        );
    }
    let Some(monitor) = monitor else {
        return;
    };
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    let mut readings = 0;
    while Instant::now() < until {
        let poll = audio.poll();
        for error in poll.errors {
            println!("error: {error}");
        }
        for level in poll.levels {
            readings += 1;
            let bands: Vec<String> = level
                .bands
                .iter()
                .map(|band| format!("{:.2}", band))
                .collect();
            println!(
                "level L {:.3} R {:.3} bands [{}]",
                level.left,
                level.right,
                bands.join(" ")
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // The meter's own capture stream is not an application's.
    let own = audio
        .state()
        .streams()
        .filter(|stream| stream.app_name == "morf")
        .count();
    audio.stop_monitor(monitor);
    println!("{readings} readings in {seconds}s; own streams listed: {own}");
}
