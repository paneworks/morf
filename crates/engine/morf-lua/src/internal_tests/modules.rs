//! Tests of modules that reach the runtime's internals; the rest are in
//! tests/engine/modules/mod.rs.
#![allow(unused_imports)]

use super::*;
use std::fs;

#[test]
fn a_configuration_finds_its_projects_library_above_it() {
    // A shell in a repository, folders deep, requires `lib.x` from the
    // repository's `library/` without a link beside it; one with no project
    // library above it finds none.
    let root = std::env::temp_dir().join(format!("morf-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let deep = root.join("examples/shells/demo/shell");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::create_dir_all(root.join("library/lib")).unwrap();
    std::fs::write(
        root.join("library/lib/greeting.lua"),
        "return 'from the project'",
    )
    .unwrap();
    let config = deep.join("init.lua");
    std::fs::write(&config, "").unwrap();
    let roots = crate::runtimepath_roots(&config, true);
    let library = std::fs::canonicalize(root.join("library")).unwrap();
    assert_eq!(roots[0], deep);
    assert_eq!(
        roots[1], library,
        "the project's library comes next: {roots:?}"
    );
    assert_eq!(
        crate::serialization::load_runtime_module(&roots, "lib.greeting").unwrap(),
        b"return 'from the project'"
    );
    // Asked to look nowhere else, it does not.
    assert_eq!(crate::runtimepath_roots(&config, false), vec![deep]);
    let _ = std::fs::remove_dir_all(&root);
}
