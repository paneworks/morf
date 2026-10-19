// Where a configuration is found: named shells and their parts, links, and
// the roots its modules are searched in.

use morf_host::process::supervisor::runtimepath_roots;
use std::fs;
use std::path::PathBuf;

use crate::*;

#[test]
fn clean_policy_keeps_only_the_config_root() {
    let config = PathBuf::from("/tmp/morf-clean/shell.lua");

    assert_eq!(
        runtimepath_roots(&config, false),
        [PathBuf::from("/tmp/morf-clean")]
    );
}

#[test]
fn a_linked_configuration_runs_from_where_it_really_is() {
    // `make apply` makes `~/.config/morf/default` a link to a named shell:
    // its modules are beside the file linked to, not beside the link.
    let root = std::env::temp_dir().join(format!("morf-linked-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("eva/shell")).unwrap();
    fs::write(root.join("eva/shell/init.lua"), "").unwrap();
    std::os::unix::fs::symlink("eva", root.join("default")).unwrap();
    let real = fs::canonicalize(root.join("eva/shell/init.lua")).unwrap();
    let linked = crate::config::followed(root.join("default/shell/init.lua"));
    assert_eq!(linked, real);
    assert_eq!(
        runtimepath_roots(&linked, false),
        vec![real.parent().unwrap().to_path_buf()]
    );
    // One that is not there stays as given, for the error to name it.
    assert_eq!(
        crate::config::followed(PathBuf::from("no-such.lua")),
        PathBuf::from("no-such.lua")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_named_shell_has_parts() {
    let root = std::env::temp_dir().join(format!("morf-parts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let previous_home = std::env::var_os("XDG_CONFIG_HOME");
    let previous_dirs = std::env::var_os("XDG_CONFIG_DIRS");
    let system = root.join("system");
    // SAFETY: this is the only test that reads the XDG config directories.
    // Keep an installed system theme from becoming part of this fixture.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &root);
        std::env::set_var("XDG_CONFIG_DIRS", &system);
    }
    let morf = root.join("morf");
    let named = |name: &str| crate::config::named_config_path(name);
    assert_eq!(named("eva").unwrap(), morf.join("eva/shell/init.lua"));
    assert_eq!(named("eva/lock").unwrap(), morf.join("eva/lock/init.lua"));
    assert_eq!(named("eva/greet").unwrap(), morf.join("eva/greet/init.lua"));
    assert!(named("eva/bar").is_err());
    assert!(named("a/b/c").is_err());
    assert!(named("..").is_err());
    for part in ["shell", "lock", "greet"] {
        let args = [part, "-c", "eva", "--", "window", "preview"].map(std::ffi::OsString::from);
        let Command::Run(path, _, operands, _) = parse_command(&args).unwrap() else {
            panic!("expected role run")
        };
        assert_eq!(path, morf.join(format!("eva/{part}/init.lua")));
        assert_eq!(operands, ["window", "preview"]);
    }
    assert!(parse_command(&["lock", "-c", "eva/greet"].map(std::ffi::OsString::from)).is_err());
    assert!(parse_command(&["greet", "unexpected"].map(std::ffi::OsString::from)).is_err());
    let Command::Run(path, _, _, _) = parse_command(&["lock".into()]).unwrap() else {
        panic!("expected lock")
    };
    assert_eq!(path, morf.join("default/lock/init.lua"));
    // The old layout, NAME/shell.lua, is still found.
    fs::create_dir_all(morf.join("old")).unwrap();
    fs::write(morf.join("old/shell.lua"), "").unwrap();
    assert_eq!(named("old").unwrap(), morf.join("old/shell.lua"));
    // Bare: the default's shell, unless the old shell.lua is there.
    assert_eq!(
        crate::config::default_config_path().unwrap(),
        morf.join("default/shell/init.lua")
    );
    fs::write(morf.join("shell.lua"), "").unwrap();
    assert_eq!(
        crate::config::default_config_path().unwrap(),
        morf.join("shell.lua")
    );
    // A greeter with an empty home uses system configuration; user parts
    // still override it. Relative XDG search entries are never loaded.
    fs::create_dir_all(system.join("morf/eva/greet")).unwrap();
    fs::write(system.join("morf/eva/greet/init.lua"), "").unwrap();
    unsafe { std::env::set_var("XDG_CONFIG_DIRS", format!("relative:{}", system.display())) };
    assert_eq!(
        named("eva/greet").unwrap(),
        system.join("morf/eva/greet/init.lua")
    );
    fs::create_dir_all(morf.join("eva/greet")).unwrap();
    fs::write(morf.join("eva/greet/init.lua"), "").unwrap();
    assert_eq!(named("eva/greet").unwrap(), morf.join("eva/greet/init.lua"));
    unsafe {
        match previous_dirs {
            Some(value) => std::env::set_var("XDG_CONFIG_DIRS", value),
            None => std::env::remove_var("XDG_CONFIG_DIRS"),
        }
        match previous_home {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
    let _ = fs::remove_dir_all(&root);
}
