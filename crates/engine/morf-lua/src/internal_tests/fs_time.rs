//! Tests of fs_time.rs that reach the runtime's internals; the rest are in
//! tests/engine/fs_time.rs.
#![allow(unused_imports)]

use super::*;

#[test]
fn log_writes_levels_and_stays_bounded() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "log.lua",
            br#"
            morf.log("warn", "settings", "missing", 3)
            morf.log.info("hello")
            assert(not pcall(morf.log, "loud", "x"))
            for i = 1, 2500 do morf.log.debug("line", i) end
            "#,
        )
        .unwrap();
    let logs = runtime.take_logs();
    assert_eq!(logs.len(), crate::state::MAX_LOG_ENTRIES);
    assert_eq!(logs.last().unwrap().message, "line 2500");
    assert!(
        logs.iter()
            .all(|entry| entry.message != "settings missing 3")
    );
    let mut fresh = Runtime::default();
    fresh
        .execute("log2.lua", br#"morf.log("warn", "settings", "missing", 3)"#)
        .unwrap();
    let logs = fresh.take_logs();
    assert!(
        logs.iter()
            .any(|e| e.message == "settings missing 3" && e.level == LogLevel::Warn),
        "{logs:?}"
    );
}
