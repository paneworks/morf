//! Idle thresholds that count only the person.
//!
//! A media player inhibits idle so the screen stays on through a film. A shell
//! that dims its own bar after a minute of nobody touching anything still
//! wants that minute -- and before this it could not ask, because the only
//! threshold it could register was the inhibitable one.

use super::*;

#[test]
fn an_input_only_threshold_is_its_own_key() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "idle-input.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local which = morf.signal("idle.which", "awake")
                morf.idle.subscribe(60000, function(idle) which:set(idle and "idle" or "awake") end)
                morf.idle.subscribe(60000, function(idle) which:set(idle and "input-idle" or "awake") end, true)
                ui.Text { text = function() return which:get() end }
            "#,
        )
        .unwrap();
    let mut timeouts = runtime.idle_timeouts();
    timeouts.sort_unstable();
    assert_eq!(
        timeouts,
        [(60_000, false), (60_000, true)],
        "the same number of milliseconds is two different requests"
    );
    let root = runtime.scene().roots()[0];
    assert!(runtime.dispatch_idle(60_000, true, true));
    runtime.poll_services();
    assert_eq!(
        runtime.scene().string_value(root, "text").unwrap(),
        "input-idle",
        "input idleness reaches the callback that asked for it, and only that one"
    );
}

#[test]
fn a_threshold_subscribed_or_cancelled_later_is_reported_as_a_change() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "idle-late.lua",
            br#"
                local ui = require("morf.ui")
                local subs = {}
                morf.idle.subscribe(60000, function() end)
                morf.ipc.add = function()
                    subs[#subs + 1] = morf.idle.subscribe(5000, function() end)
                end
                morf.ipc.drop = function() table.remove(subs):cancel() end
                ui.Item {}
            "#,
        )
        .unwrap();
    // Loading asked for one threshold; the loop applies it once.
    assert_eq!(
        runtime.take_idle_timeouts_change(),
        Some(vec![(60_000, false)])
    );
    assert_eq!(runtime.take_idle_timeouts_change(), None);

    runtime.call_ipc("add", &[]).unwrap();
    assert_eq!(
        runtime.take_idle_timeouts_change(),
        Some(vec![(5_000, false), (60_000, false)]),
        "a subscription made after loading reaches the compositor at once"
    );
    // A second callback on a threshold already asked for asks nothing new.
    runtime.call_ipc("add", &[]).unwrap();
    assert_eq!(runtime.take_idle_timeouts_change(), None);
    // Cancelling one of two keeps the threshold; the last takes it away.
    runtime.call_ipc("drop", &[]).unwrap();
    assert_eq!(runtime.take_idle_timeouts_change(), None);
    runtime.call_ipc("drop", &[]).unwrap();
    assert_eq!(
        runtime.take_idle_timeouts_change(),
        Some(vec![(60_000, false)])
    );
    assert!(
        !runtime.dispatch_idle(5_000, false, true),
        "nobody hears it now"
    );
}

// Keeping the session awake can be read back: what was last asked for, and
// the change the host applies.
#[test]
fn an_idle_inhibit_reads_back() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "inhibit.lua",
            br#"
                assert(morf.idle.inhibited() == false)
                morf.idle.inhibit(true)
                assert(morf.idle.inhibited() == true)
            "#,
        )
        .unwrap();
    assert_eq!(runtime.take_idle_inhibit_change(), Some(true));
    runtime
        .execute(
            "off.lua",
            br#"morf.idle.inhibit(false) assert(morf.idle.inhibited() == false)"#,
        )
        .unwrap();
    assert_eq!(runtime.take_idle_inhibit_change(), Some(false));
}
