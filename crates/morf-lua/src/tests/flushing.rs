use crate::*;

#[test]
fn a_handler_that_writes_three_signals_flushes_once() {
    // Each `set` outside an effect used to flush the whole graph on the spot:
    // three writes in a click handler were three passes over every dirty
    // effect. Inside a handler they are now one flush, when it returns.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "flush.lua",
            br#"
                local morf = require("morf")
                local a = morf.signal("a", 0)
                local b = morf.signal("b", 0)
                local runs = 0
                morf.effect("count", function()
                    a:get()
                    b:get()
                    runs = runs + 1
                end)
                morf.ipc.poke = function()
                    a:set(1)
                    b:set(2)
                    a:set(3)
                end
                morf.ipc.runs = function() return runs end
                morf.ipc.sum = function() return a:get() + b:get() end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(1)]
    );

    runtime.call_ipc("poke", &[]).unwrap();

    assert_eq!(
        runtime.call_ipc("runs", &[]).unwrap(),
        [IpcValue::Integer(2)]
    );
    assert_eq!(
        runtime.call_ipc("sum", &[]).unwrap(),
        [IpcValue::Integer(5)]
    );
}

#[test]
fn a_bare_property_write_in_a_handler_reaches_the_bindings_that_read_it() {
    // `node.text = "b"` marked the bindings reading `node.text` dirty and
    // then nothing flushed them: they waited for the next signal write
    // anywhere, or the once-a-second clock. A handler's return is a flush.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "write.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local source = ui.Text { text = "a" }
                local label = ui.Text { text = function() return source.text .. "!" end }
                morf.ipc.rename = function() source.text = "b" end
            "#,
        )
        .unwrap();
    let label = runtime.scene().roots()[1];
    assert_eq!(runtime.scene().string_value(label, "text").unwrap(), "a!");

    runtime.call_ipc("rename", &[]).unwrap();

    assert_eq!(runtime.scene().string_value(label, "text").unwrap(), "b!");
}

#[test]
fn a_binding_on_layout_width_follows_the_frame() {
    // `node.width` is what a node asked for, and zero for one sized by its
    // parent. `node.layout_width` is what the frame gave it, and a binding
    // that reads it is re-run when a frame moves it.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layout.lua",
            br#"
                local ui = require("morf.ui")
                local box = ui.Item { width = 200, height = 50 }
                local half = ui.Rect { height = 10, width = function()
                    return (box.layout_width or 0) / 2
                end }
                _G.box, _G.half = box, half
            "#,
        )
        .unwrap();
    let (root, half) = {
        let scene = runtime.scene();
        (scene.roots()[0], scene.roots()[1])
    };
    assert_eq!(runtime.scene().number(half, "width").unwrap(), 0.0);

    let layout = morf_layout::Layout::compute(
        &runtime.scene(),
        root,
        morf_layout::Size {
            width: 200.0,
            height: 50.0,
        },
        &mut super::NoText,
    )
    .unwrap();
    runtime.observe_layout(&layout);

    assert_eq!(runtime.scene().number(half, "width").unwrap(), 100.0);
}

#[test]
fn a_lua_layout_container_places_its_children_with_its_own_functions() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layout-fn.lua",
            br#"
                local ui = require("morf.ui")
                -- Right-aligned stack: each child at the right edge, one under
                -- the other, and the container as wide as its widest child.
                ui.Layout {
                    width = 200,
                    measure = function(available, children)
                        local width, height = 0, 0
                        for _, child in ipairs(children) do
                            width = math.max(width, child.width)
                            height = height + child.height
                        end
                        return width, height
                    end,
                    place = function(bounds, children)
                        local placements, y = {}, 0
                        for index, child in ipairs(children) do
                            placements[index] = { x = bounds.width - child.width, y = y }
                            y = y + child.height
                        end
                        return placements
                    end,
                    ui.Rect { width = 50, height = 10 },
                    ui.Rect { width = 80, height = 20 },
                }
            "#,
        )
        .unwrap();
    let (root, first, second) = {
        let scene = runtime.scene();
        let root = scene.roots()[0];
        let children = scene.children(root).unwrap();
        (root, children[0], children[1])
    };

    let layout = runtime
        .compute_layout(
            root,
            morf_layout::Size {
                width: 200.0,
                height: 100.0,
            },
            &mut super::NoText,
        )
        .unwrap();

    assert_eq!(layout.geometry(root).unwrap().height, 100.0);
    assert_eq!(layout.implicit_size(root).unwrap().height, 30.0);
    let first = layout.geometry(first).unwrap();
    assert_eq!((first.x, first.y), (150.0, 0.0));
    let second = layout.geometry(second).unwrap();
    assert_eq!((second.x, second.y), (120.0, 10.0));
}

#[test]
fn a_layout_function_that_writes_to_a_node_is_refused_not_crashed() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layout-write.lua",
            br#"
                local ui = require("morf.ui")
                local box = ui.Rect { width = 10, height = 10 }
                ui.Layout {
                    width = 100,
                    measure = function() box.width = 20; return 100, 10 end,
                    place = function() return {} end,
                    box,
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];

    let error = runtime
        .compute_layout(
            root,
            morf_layout::Size {
                width: 100.0,
                height: 100.0,
            },
            &mut super::NoText,
        )
        .unwrap_err();

    assert!(error.contains("inside a layout function"), "{error}");
}

#[test]
fn a_chain_of_layout_bindings_settles_in_as_many_passes_as_it_is_long() {
    // Each link reads the one before it, which the frame has placed only
    // once the previous pass has flushed: three passes, the last a no-op.
    let source = br#"
        local ui = require("morf.ui")
        local fill = ui.Item { anchors = { fill = true } }
        local middle = ui.Item { width = 10, height = 10, x = function()
            return (fill.layout_width or 0) / 2
        end }
        local beside = ui.Item { width = 10, height = 10, x = function()
            return (middle.layout_width or 0) + (middle.layout_x or 0)
        end }
        ui.Item { width = 300, height = 50, fill, middle, beside }
    "#;
    let size = morf_layout::Size {
        width: 300.0,
        height: 50.0,
    };
    let mut runtime = Runtime::default();
    runtime.execute("settle.lua", source).unwrap();
    let root = *runtime.scene().roots().last().unwrap();
    let settled = runtime
        .settle_layout(root, size, &mut super::NoText, 8)
        .unwrap();
    assert!(settled.stable);
    assert_eq!(settled.passes, 3);
    let beside = runtime.scene().children(root).unwrap()[2];
    assert_eq!(settled.layout.geometry(beside).unwrap().x, 160.0);

    // Bounded: out of passes, it says so and hands back the last.
    let mut runtime = Runtime::default();
    runtime.execute("settle.lua", source).unwrap();
    let root = *runtime.scene().roots().last().unwrap();
    let settled = runtime
        .settle_layout(root, size, &mut super::NoText, 1)
        .unwrap();
    assert!(!settled.stable);
    assert_eq!(settled.passes, 1);
}

#[test]
fn a_binding_on_layout_size_stays_still_while_its_node_only_moves() {
    // A label in a panel that slides moves on every frame and keeps its
    // size; the bindings sizing something to it used to re-run on every one
    // of those frames, one signal standing for the whole rectangle.
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "layout-split.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local sized, placed = 0, 0
                local label = ui.Item { width = 40, height = 10 }
                local slide = ui.Item { x = 0, width = 100, height = 20, label }
                ui.Item { width = 300, height = 50, slide }
                ui.Rect { width = function() sized = sized + 1 return label.layout_width or 0 end }
                ui.Rect { x = function() placed = placed + 1 return label.layout_x or 0 end }
                morf.ipc.slide = function() slide.x = 50 end
                morf.ipc.grow = function() label.width = 60 end
                morf.ipc.counts = function() return sized .. "/" .. placed end
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let size = morf_layout::Size {
        width: 300.0,
        height: 50.0,
    };
    let frame = |runtime: &mut Runtime| {
        let layout =
            morf_layout::Layout::compute(&runtime.scene(), root, size, &mut super::NoText).unwrap();
        runtime.observe_layout(&layout);
    };
    let counts = |runtime: &mut Runtime| runtime.call_ipc("counts", &[]).unwrap();
    frame(&mut runtime);
    assert_eq!(counts(&mut runtime), [IpcValue::String("2/2".into())]);

    runtime.call_ipc("slide", &[]).unwrap();
    frame(&mut runtime);
    assert_eq!(
        counts(&mut runtime),
        [IpcValue::String("2/3".into())],
        "moved, not resized"
    );

    runtime.call_ipc("grow", &[]).unwrap();
    frame(&mut runtime);
    assert_eq!(
        counts(&mut runtime),
        [IpcValue::String("3/3".into())],
        "resized, not moved"
    );
}
