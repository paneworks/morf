//! Tests of core_api.rs that reach the runtime's internals; the rest are in
//! tests/engine/core_api.rs.
#![allow(unused_imports)]

use super::*;
use morf_layout::Layout;
use std::fs;
use std::path::PathBuf;

#[test]
fn transform_example_uses_the_native_watcher() {
    let source = include_bytes!("../../../../../examples/demos/motion/transform.lua");
    let mut runtime = Runtime::default();
    runtime
        .execute("examples/demos/motion/transform.lua", source)
        .unwrap();

    assert_eq!(runtime.scene().roots().len(), 2);
    assert_eq!(runtime.reactive.borrow().transform_watchers.len(), 1);
    assert_eq!(runtime.window_surface_configs().len(), 1);
}
