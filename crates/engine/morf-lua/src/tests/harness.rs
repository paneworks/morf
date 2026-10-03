//! What a runner that is not a shell asks of a runtime: a virtual clock, a
//! command line of its own, and host functions.

use std::rc::Rc;
use std::time::Duration;

use super::*;
use morf_value::IpcTable;

fn integer(runtime: &mut Runtime, verb: &str) -> i64 {
    match runtime.call_ipc(verb, &[]).unwrap().as_slice() {
        [IpcValue::Integer(value)] => *value,
        other => panic!("{verb} answered {other:?}"),
    }
}

#[test]
fn timers_on_a_virtual_clock_fire_when_it_is_advanced_and_not_before() {
    let mut runtime = Runtime::default();
    runtime.use_virtual_clock();
    runtime
        .execute(
            "virtual-timers.lua",
            br#"
                local once, every, node = 0, 0, 0
                morf.ipc.once = function() return once end
                morf.ipc.every = function() return every end
                morf.ipc.node = function() return node end
                morf.timer(100, function() once = once + 1 end, false)
                morf.timer(30, function() every = every + 1 end)
                require("morf.ui").Timer {
                    interval = 50, running = true,
                    on_triggered = function() node = node + 1 end,
                }
            "#,
        )
        .unwrap();
    assert_eq!(runtime.virtual_clock(), Some(Duration::ZERO));
    assert_eq!(
        runtime.next_virtual_deadline(),
        Some(Duration::from_millis(30))
    );
    // However long the wall clock waits, nothing is due.
    std::thread::sleep(Duration::from_millis(120));
    runtime.poll_services();
    assert_eq!(integer(&mut runtime, "once"), 0);
    assert_eq!(integer(&mut runtime, "every"), 0);
    // Stepped deadline by deadline, as a runner does.
    while runtime.virtual_clock().unwrap() < Duration::from_millis(100) {
        let next = runtime.next_virtual_deadline().unwrap();
        let now = runtime.virtual_clock().unwrap();
        runtime.advance_virtual_clock(next.saturating_sub(now));
        runtime.poll_services();
    }
    assert_eq!(integer(&mut runtime, "once"), 1);
    assert_eq!(integer(&mut runtime, "every"), 3);
    assert_eq!(integer(&mut runtime, "node"), 1);
    // A long jump fires a repeating timer once, as the wall timer would.
    runtime.advance_virtual_clock(Duration::from_secs(1));
    runtime.poll_services();
    assert_eq!(integer(&mut runtime, "every"), 4);
}

#[test]
fn a_runtime_without_a_virtual_clock_keeps_the_wall_one() {
    let mut runtime = Runtime::default();
    runtime.advance_virtual_clock(Duration::from_secs(5));
    assert_eq!(runtime.virtual_clock(), None);
    assert_eq!(runtime.next_virtual_deadline(), None);
}

#[test]
fn a_runner_gives_a_configuration_arguments_of_its_own() {
    let mut runtime = Runtime::default();
    runtime.set_arguments(vec!["--size".into(), "3".into(), "lock".into()]);
    runtime
        .execute(
            "arguments.lua",
            br#"
                assert(morf.args[3] == "lock", tostring(morf.args[3]))
                assert(morf.options.size == "3", tostring(morf.options.size))
                assert(morf.operands[1] == "lock")
            "#,
        )
        .unwrap();
}

#[test]
fn a_host_function_takes_and_returns_tables() {
    let mut runtime = Runtime::default();
    runtime.register_host_function(
        "__host",
        "sum",
        Rc::new(|arguments| {
            let Some(IpcValue::Table(table)) = arguments.first() else {
                return Err("sum wants a list".to_owned());
            };
            let IpcTable::List(items) = &**table else {
                return Err("sum wants a list".to_owned());
            };
            let total = items
                .iter()
                .map(|item| match item {
                    IpcValue::Integer(value) => *value,
                    _ => 0,
                })
                .sum::<i64>();
            Ok(vec![
                IpcValue::Integer(total),
                IpcValue::String("done".into()),
            ])
        }),
    );
    runtime
        .execute(
            "host.lua",
            br#"
                local total, word = __host.sum({ 1, 2, 3 })
                assert(total == 6 and word == "done")
                local ok, err = pcall(__host.sum, "no")
                assert(not ok and tostring(err):find("sum wants a list"))
            "#,
        )
        .unwrap();
}

#[test]
fn a_missing_module_is_blamed_on_the_line_that_required_it() {
    let mut runtime = Runtime::default();
    let error = runtime
        .execute(
            "needs.lua",
            b"local a = 1\nlocal b = require('no.such.module')",
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("needs.lua:2:"), "{error}");
}

#[test]
fn a_node_can_carry_an_id() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "ids.lua",
            br#"
                local node = morf.ui.Item { id = "panel" }
                assert(node.id == "panel")
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    assert_eq!(runtime.scene().string_value(root, "id").unwrap(), "panel");
}
