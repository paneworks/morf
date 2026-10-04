//! Tests of events_animation.rs that reach the runtime's internals; the rest are in
//! tests/engine/events_animation.rs.
#![allow(unused_imports)]

use super::*;
use crate::serialization::lua_to_dbus;
use luna::Table;
use luna::Value as LuaValue;
use morf_io::DbusValue;
use morf_scene::Value as SceneValue;
use std::time::Duration;

#[test]
fn lua_dbus_arguments_preserve_positional_lists() {
    let mut runtime = Runtime::default();
    let value = runtime.lua.enter(|ctx| {
        let arguments = Table::new(&ctx);
        arguments.set(ctx, 1, "device").unwrap();
        let typed = Table::new(&ctx);
        typed.set_field(ctx, "signature", "u");
        typed.set_field(ctx, "value", 7_i64);
        arguments.set(ctx, 2, typed).unwrap();
        arguments.set(ctx, 3, true).unwrap();
        lua_to_dbus(ctx, LuaValue::Table(arguments), 0)
    });

    assert_eq!(
        value.unwrap(),
        DbusValue::List(vec![
            DbusValue::String("device".to_owned()),
            DbusValue::Typed {
                signature: "u".to_owned(),
                value: Box::new(DbusValue::Integer(7)),
            },
            DbusValue::Bool(true),
        ])
    );
}

#[test]
fn pointer_drag_handlers_receive_position_and_displacement() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "drag.lua",
            br#"
                local morf = require("morf")
                local ui = require("morf.ui")
                local status = morf.signal("drag.status", "idle")
                ui.MouseArea {
                  accepted_buttons = { "right" },
                  on_dragged = function(x, y, dx, dy)
                    status:set(string.format("%.0f:%.0f:%.0f:%.0f", x, y, dx, dy))
                  end,
                  ui.Text { text = function() return status:get() end },
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let text = runtime.scene().children(root).unwrap()[0];

    assert!(!runtime.accepts_pointer_button(root, 0x110));
    assert!(runtime.accepts_pointer_button(root, 0x111));
    assert!(runtime.dispatch_pointer_event(
        root,
        UiEvent::Dragged,
        EventPoint::new((20.0, 30.0), (20.0, 30.0)),
        (9.0, 12.0)
    ));
    assert_eq!(
        runtime.scene().string_value(text, "text").unwrap(),
        "20:30:9:12"
    );
}
