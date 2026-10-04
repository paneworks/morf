use super::*;

// `morf.screens`: the compositor's whole output list, own output first.

fn output(name: &str, x: i32, width: i32, height: i32) -> Screen {
    Screen {
        id: 1,
        name: name.to_owned(),
        position: Some((x, 0)),
        width: Some(width),
        height: Some(height),
        scale: 1,
        transform: "normal".to_owned(),
        ..Screen::default()
    }
}

#[test]
fn screens_list_every_output_with_the_instance_own_output_first() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("DP-2", 1920, 2560, 1440));

    runtime.set_screens(&[
        output("eDP-1", 0, 1920, 1080),
        output("DP-2", 1920, 2560, 1440),
        output("HDMI-A-1", 4480, 1280, 1024),
    ]);

    runtime
        .execute(
            "screens.lua",
            br#"
                local screens = morf.screens
                assert(#screens == 3)
                assert(screens[1].name == "DP-2")
                assert(screens[2].name == "eDP-1")
                assert(screens[3].name == "HDMI-A-1")
                assert(screens[1].x == 1920 and screens[1].width == 2560)
                assert(screens[2].x == 0 and screens[2].height == 1080)
                assert(screens[3].scale == 1 and screens[3].transform == "normal")
                -- `Workspace.qml`'s barOnRight, with no compositor query.
                local main = screens[2]
                local own = screens[1]
                local main_centre = main.x + main.width / 2
                local own_centre = own.x + own.width / 2
                assert(own_centre > main_centre)
            "#,
        )
        .unwrap();
}

#[test]
fn unplugging_an_output_drops_it_from_the_screen_list() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("DP-2", 1920, 2560, 1440));
    runtime.set_screens(&[
        output("eDP-1", 0, 1920, 1080),
        output("DP-2", 1920, 2560, 1440),
    ]);

    runtime.set_screens(&[output("eDP-1", 0, 1920, 1080)]);

    runtime
        .execute(
            "unplugged.lua",
            br#"
                -- The instance keeps its own output at index 1 even once the
                -- compositor stops reporting it; the supervisor is what stops
                -- this worker.
                assert(#morf.screens == 2)
                assert(morf.screens[1].name == "DP-2")
                assert(morf.screens[2].name == "eDP-1")
                assert(morf.screens[3] == nil)
            "#,
        )
        .unwrap();
}

#[test]
fn a_runtime_with_no_compositor_keeps_the_screens_it_was_built_with() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("eDP-1", 0, 1920, 1080));

    runtime.set_screens(&[]);

    runtime
        .execute(
            "no-outputs.lua",
            br#"
                assert(#morf.screens == 1)
                assert(morf.screens[1].name == "eDP-1")
            "#,
        )
        .unwrap();
}

#[test]
fn a_runtime_owning_the_desktop_drops_disconnected_outputs_and_moves_controls() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("DP-2", 1920, 2560, 1440));
    runtime.replace_screens(&[
        output("eDP-1", 0, 1920, 1080),
        output("DP-2", 1920, 2560, 1440),
    ]);
    runtime.execute("desktop.lua",br#"
        local selected=morf.signal("lock.output","DP-2")
        morf.effect("lock.outputs",function()
            morf.screens_revision()
            for _,output in ipairs(morf.screens) do if output.name==selected:get() then return end end
            selected:set((morf.screens[1] or {}).name or "")
        end)
        morf.ipc.selected=function() return selected:get(),#morf.screens end
    "#).unwrap();
    runtime.replace_screens(&[output("eDP-1", 0, 1920, 1080)]);
    assert_eq!(
        runtime.call_ipc("selected", &[]).unwrap(),
        vec![IpcValue::String("eDP-1".into()), IpcValue::Integer(1)]
    );
    runtime.replace_screens(&[]);
    assert_eq!(
        runtime.call_ipc("selected", &[]).unwrap(),
        vec![IpcValue::String("".into()), IpcValue::Integer(0)]
    );
    let mut portrait = output("DP-3", -1080, 1080, 1920);
    portrait.scale = 2;
    portrait.transform = "90".into();
    runtime.replace_screens(&[portrait]);
    assert_eq!(
        runtime.call_ipc("selected", &[]).unwrap(),
        vec![IpcValue::String("DP-3".into()), IpcValue::Integer(1)]
    );
    runtime
        .execute(
            "reconnected.lua",
            br#"
        assert(morf.screens[1].width==1080 and morf.screens[1].height==1920)
        assert(morf.screens[1].scale==2 and morf.screens[1].transform=="90")
    "#,
        )
        .unwrap();
}

#[test]
fn outputs_the_compositor_left_unnamed_stay_separate_entries() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("", 0, 1920, 1080));

    runtime.set_screens(&[output("", 0, 1920, 1080), output("", 1920, 2560, 1440)]);

    runtime
        .execute(
            "unnamed.lua",
            br#"
                -- Nothing addresses a nameless output, but it still occupies
                -- the desktop, so it may not collapse into its neighbour.
                assert(#morf.screens == 2)
                assert(morf.screens[1].width == 1920)
                assert(morf.screens[2].width == 2560)
            "#,
        )
        .unwrap();
}

#[test]
fn an_effect_on_the_screens_revision_runs_when_outputs_change() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("DP-2", 1920, 2560, 1440));
    runtime
        .execute(
            "revision.lua",
            br#"
                runs, seen = 0, {}
                morf.effect("screens", function()
                    morf.screens_revision()
                    runs = runs + 1
                    seen[runs] = #morf.screens
                end)
            "#,
        )
        .unwrap();
    let runs = |runtime: &mut Runtime| {
        runtime.call_ipc("runs", &[]).ok();
        runtime
            .execute("check.lua", b"morf.ipc.runs = function() return runs end")
            .unwrap();
        match runtime.call_ipc("runs", &[]).unwrap().as_slice() {
            [IpcValue::Integer(count)] => *count,
            [IpcValue::Number(count)] => *count as i64,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(runs(&mut runtime), 1);
    // A second monitor arrives.
    let two = [
        output("DP-2", 1920, 2560, 1440),
        output("eDP-1", 0, 1920, 1080),
    ];
    runtime.set_screens(&two);
    assert_eq!(runs(&mut runtime), 2);
    // The same list again is not a change.
    runtime.set_screens(&two);
    assert_eq!(runs(&mut runtime), 2);
    // A rescale is.
    let mut rescaled = two.clone();
    rescaled[1].scale = 2;
    runtime.set_screens(&rescaled);
    assert_eq!(runs(&mut runtime), 3);
    runtime
        .execute(
            "seen.lua",
            br#"assert(seen[2] == 2 and morf.screens_revision() == 2)"#,
        )
        .unwrap();
}

#[test]
fn primary_is_a_tracked_read_and_its_callbacks_hear_each_change_once() {
    let mut runtime = Runtime::for_screen(Limits::default(), output("DP-2", 0, 800, 600));
    assert!(
        runtime.is_primary(),
        "a runtime nobody told otherwise is primary"
    );
    runtime
        .execute(
            "primary.lua",
            br#"
                assert(morf.primary() == true)
                heard = {}
                runs = 0
                morf.on_primary(function(primary) heard[#heard + 1] = tostring(primary) end)
                morf.effect("follow primary", function() morf.primary() runs = runs + 1 end)
                morf.ipc.state = function()
                    return tostring(morf.primary()) .. " " .. runs .. " " .. table.concat(heard, ",")
                end
            "#,
        )
        .unwrap();
    let state = |runtime: &mut Runtime| runtime.call_ipc("state", &[]).unwrap();
    assert_eq!(
        state(&mut runtime),
        vec![IpcValue::String("true 1 ".into())]
    );
    assert!(runtime.set_primary(false));
    assert!(!runtime.is_primary());
    assert_eq!(
        state(&mut runtime),
        vec![IpcValue::String("false 2 false".into())]
    );
    // The same answer again is no change.
    assert!(!runtime.set_primary(false));
    assert!(runtime.set_primary(true));
    assert_eq!(
        state(&mut runtime),
        vec![IpcValue::String("true 3 false,true".into())]
    );
}
