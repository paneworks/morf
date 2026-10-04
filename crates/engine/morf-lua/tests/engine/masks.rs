use morf_lua::*;

// `mask` and `mask_invert` as a configuration writes them: a node, a
// gradient table, or nothing, at construction and afterwards.

#[test]
fn a_node_mask_is_kept_moved_under_its_owner_and_read_back() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "masks.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local dot = ui.Rect { id = "dot", width = 20, height = 20, radius = 10 }
                local panel = ui.Rect {
                    id = "panel", width = 100, height = 60,
                    mask = dot, mask_invert = true,
                    ui.Text { text = "inside" },
                }
                local root = ui.Item { width = 400, height = 300, panel }
                assert(panel.mask.id == "dot", "a node mask reads back as the node")
                assert(panel.mask_invert == true)
                assert(not pcall(function() panel.mask = 4 end), "a number is not a mask")
                _G.panel = panel
                morf.ipc.fade = function()
                    panel.mask = { gradient = { angle = 90, stops = { 0, 1 } } }
                    return panel.mask.gradient.angle
                end
                morf.ipc.again = function()
                    panel.mask = ui.Rect { id = "square" }
                    return panel.mask.id
                end
                morf.ipc.clear = function()
                    panel.mask = nil
                    return panel.mask.gradient == nil
                end
            "#,
        )
        .unwrap();
    let (panel, dot) = {
        let scene = runtime.scene();
        let root = scene.roots()[0];
        let panel = scene.children(root).unwrap()[0];
        let dot = scene.mask(panel).expect("a node mask");
        assert_eq!(scene.parent(dot).unwrap(), Some(panel));
        assert!(scene.bool_value(panel, "mask_invert").unwrap());
        (panel, dot)
    };

    let angle = runtime.call_ipc("fade", &[]).unwrap();
    assert!(
        matches!(
            angle.as_slice(),
            [IpcValue::Integer(90)] | [IpcValue::Number(90.0)]
        ),
        "{angle:?}"
    );
    {
        let scene = runtime.scene();
        assert_eq!(scene.mask(panel), None, "a gradient replaces the node");
        assert!(
            scene.element(dot).is_err(),
            "and the node it replaced is gone"
        );
        let spec = morf_scene::MaskSpec::parse(scene.current(panel, "mask").unwrap())
            .unwrap()
            .expect("a gradient mask");
        assert_eq!(spec.gradient.stops[0].color.alpha, 0.0);
    }

    let id = runtime.call_ipc("again", &[]).unwrap();
    assert_eq!(id, vec![IpcValue::String("square".to_owned())]);
    assert!(runtime.scene().mask(panel).is_some());

    let cleared = runtime.call_ipc("clear", &[]).unwrap();
    assert_eq!(cleared, vec![IpcValue::Boolean(true)]);
    assert_eq!(runtime.scene().mask(panel), None);
}

#[test]
fn a_mask_gradient_can_be_a_binding() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "masks.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local edge = morf.signal("edge", 0.2)
                local list = ui.Item {
                    width = 100, height = 200,
                    mask = function()
                        return { gradient = { stops = { 0, { 1, edge:get() }, { 1, 1 - edge:get() }, 0 } } }
                    end,
                }
                ui.Item { width = 400, height = 300, list }
                morf.ipc.edge = function(value) edge:set(value) end
            "#,
        )
        .unwrap();
    let list = {
        let scene = runtime.scene();
        scene.children(scene.roots()[0]).unwrap()[0]
    };
    let edge = |runtime: &Runtime| {
        let scene = runtime.scene();
        morf_scene::MaskSpec::parse(scene.current(list, "mask").unwrap())
            .unwrap()
            .expect("a mask")
            .gradient
            .stops[1]
            .position
    };
    assert!((edge(&runtime) - 0.2).abs() < 1e-9);
    runtime.call_ipc("edge", &[IpcValue::Number(0.1)]).unwrap();
    assert!((edge(&runtime) - 0.1).abs() < 1e-9);
}
