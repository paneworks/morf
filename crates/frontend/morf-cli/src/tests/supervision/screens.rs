use crate::*;
use morf_app::Output;
use morf_host::lock::WorkerCommand;
use morf_host::supervisor::known_outputs;
use morf_host::supervisor::lua_screen;
use morf_host::supervisor::store_outputs;
use morf_host::workers::handle_worker_command;
use morf_lua::{Limits, Runtime};

// The compositor's output list reaching every worker's `morf.screens`.

#[test]
fn a_hotplug_reaches_every_worker_runtime() {
    let screens = [
        Output {
            id: 7,
            name: Some("eDP-1".to_owned()),
            position: Some((0, 0)),
            size: Some((1920, 1080)),
            scale: 1,
            transform: "normal",
            ..Output::default()
        },
        Output {
            id: 9,
            name: Some("DP-2".to_owned()),
            position: Some((1920, 0)),
            size: Some((2560, 1440)),
            scale: 2,
            transform: "normal",
            ..Output::default()
        },
    ];
    // The supervisor only tells the workers when the topology actually moved.
    assert!(store_outputs(&screens));
    assert!(!store_outputs(&screens));
    assert_eq!(
        known_outputs()
            .iter()
            .map(|screen| screen.name.clone())
            .collect::<Vec<_>>(),
        ["eDP-1", "DP-2"]
    );
    let own = lua_screen(&screens[1]);
    let mut runtime = Runtime::for_screen(Limits::default(), own.clone());

    let update = handle_worker_command(
        &mut runtime,
        Some(&own),
        LoadPolicy::default(),
        WorkerCommand::Screens(screens.to_vec()),
    );

    assert!(!update.repaint);
    runtime
        .execute(
            "screens.lua",
            br#"
                assert(#morf.screens == 2)
                assert(morf.screens[1].name == "DP-2")
                assert(morf.screens[1].x == 1920)
                assert(morf.screens[1].device_pixel_ratio == 2)
                assert(morf.screens[2].name == "eDP-1")
                assert(morf.screens[2].width == 1920)
            "#,
        )
        .unwrap();
    // Left as the rest of the suite expects to find it.
    store_outputs(&[]);
}
