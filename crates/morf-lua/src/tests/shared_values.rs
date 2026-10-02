//! `morf.shared`: one copy writes, the others read it on their next turn.

use super::*;

fn answer(runtime: &mut Runtime, verb: &str) -> Vec<IpcValue> {
    runtime.call_ipc(verb, &[]).unwrap()
}

#[test]
fn a_write_in_one_runtime_reaches_the_others_and_their_bindings() {
    let source = br#"
        local morf = require("morf")
        local cpu = morf.shared("test.shared.cpu", { busy = 0 })
        local seen = 0
        morf.effect("test.shared.follow", function() seen = cpu:get().busy end)
        morf.ipc.write = function() cpu:set { busy = 42 } return true end
        morf.ipc.seen = function() return seen end
    "#;
    let mut primary = Runtime::default();
    primary.execute("shared-a.lua", source).unwrap();
    let mut other = Runtime::default();
    other.execute("shared-b.lua", source).unwrap();
    assert_eq!(answer(&mut other, "seen"), vec![IpcValue::Integer(0)]);
    answer(&mut primary, "write");
    primary.poll_services();
    other.poll_services();
    assert_eq!(answer(&mut other, "seen"), vec![IpcValue::Integer(42)]);
    // A copy that starts later begins from what was written, not the default.
    let mut late = Runtime::default();
    late.execute("shared-c.lua", source).unwrap();
    assert_eq!(answer(&mut late, "seen"), vec![IpcValue::Integer(42)]);
}
