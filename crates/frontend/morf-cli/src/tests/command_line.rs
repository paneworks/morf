// The command line: what morf takes for itself, and what after `--` is the
// configuration's.

use morf_io::IpcRequest;
use morf_io::IpcValue as WireValue;
use std::path::PathBuf;

use crate::*;

#[test]
fn command_parser_exposes_ipc_and_legacy_config_path() {
    let args = ["ipc", "call", "launcher.toggle", "one", "two"].map(std::ffi::OsString::from);
    let Command::Client(IpcRequest::Call { target, args }) = parse_command(&args).unwrap() else {
        panic!("expected IPC call");
    };
    assert_eq!(target, "launcher.toggle");
    assert_eq!(
        args,
        [
            WireValue::String("one".into()),
            WireValue::String("two".into())
        ]
    );

    let args = [std::ffi::OsString::from("custom.lua")];
    let Command::Run(path, policy, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert_eq!(path, PathBuf::from("custom.lua"));
    assert_eq!(policy, LoadPolicy::default());

    let args = ["--no-plugin", "custom.lua"].map(std::ffi::OsString::from);
    let Command::Run(_, policy, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert!(!policy.plugins);
    assert!(policy.external_roots);

    let args = ["--clean", "custom.lua"].map(std::ffi::OsString::from);
    let Command::Run(_, policy, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert!(!policy.plugins);
    assert!(!policy.external_roots);

    // A file named lock remains accessible explicitly; the bare word now
    // selects the lock part of the default configuration.
    let args = ["./lock"].map(std::ffi::OsString::from);
    let Command::Run(path, _, _, _) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert_eq!(path, PathBuf::from("./lock"));

    // Everything after `--` is the configuration's, and nothing before it is.
    let args = ["shell.lua", "--", "-d", "lock"].map(std::ffi::OsString::from);
    let Command::Run(_, _, arguments, daemonize) = parse_command(&args).unwrap() else {
        panic!("expected config path");
    };
    assert_eq!(arguments, vec!["-d", "lock"]);
    assert!(!daemonize);
    let args = ["shell.lua", "--lock"].map(std::ffi::OsString::from);
    let error = parse_command(&args).unwrap_err();
    assert!(error.contains("after `--`"), "{error}");
    // A plain word is no exception: the separator is the rule, not the dash.
    let args = ["greeter.lua", "lock"].map(std::ffi::OsString::from);
    let error = parse_command(&args).unwrap_err();
    assert!(error.contains("`lock`"), "{error}");

    let args = ["log", "--bindings"].map(std::ffi::OsString::from);
    assert!(matches!(
        parse_command(&args).unwrap(),
        Command::Client(IpcRequest::Bindings)
    ));
}

/// Everything after the `--` belongs to the configuration. morf takes what it
/// needs to find the file and stops looking at the separator.
#[test]
fn arguments_after_the_separator_are_the_configurations_own() {
    let args =
        ["--clean", "custom.lua", "--", "--numbers-only", "-n", "5"].map(std::ffi::OsString::from);
    let Command::Run(path, policy, arguments, _) = parse_command(&args).unwrap() else {
        panic!("a configuration to run");
    };
    assert_eq!(path, std::path::PathBuf::from("custom.lua"));
    assert!(!policy.plugins);
    assert_eq!(arguments, ["--numbers-only", "-n", "5"]);
}

/// A word between the configuration and the `--` is nobody's: not one of
/// morf's, and not handed to a file that was never told to expect it.
#[test]
fn a_word_before_the_separator_is_refused() {
    let args = ["custom.lua", "greet", "--", "now"].map(std::ffi::OsString::from);
    let error = parse_command(&args).unwrap_err();
    assert!(error.contains("`greet`"), "{error}");
}

/// The `--` is morf getting out of the way, so a configuration can be asked
/// for its own help rather than morf answering for it.
#[test]
fn a_separator_hands_the_rest_over_untouched() {
    let args = ["custom.lua", "--", "--help"].map(std::ffi::OsString::from);
    let Command::Run(_, _, arguments, _) = parse_command(&args).unwrap() else {
        panic!("a configuration to run");
    };
    assert_eq!(arguments, ["--help"]);
}

/// An unknown option is an unknown option, not a filename. Passing the rest of
/// the line to the configuration must not turn a typo into "could not read
/// `--colour`".
#[test]
fn an_unknown_leading_option_is_still_refused() {
    let args = ["--colour", "red"].map(std::ffi::OsString::from);
    assert!(parse_command(&args).is_err());
}
