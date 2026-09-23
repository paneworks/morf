use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value as JsonValue;
use zbus::zvariant::{ObjectPath, OwnedValue, Signature, StructureBuilder, Value};

use crate::dbus_decode::dynamic_value;
use crate::dbus_encode::typed_dbus_value;
use crate::ipc::{decode_ipc_reply, decode_ipc_request, encode_ipc_request};
use crate::*;

use crate::dbus_decode::basic_value;
use crate::dbus_encode::dbus_argument_value;

#[test]
fn split_parser_handles_multibyte_delimiters() {
    let mut parser = SplitParser::new(b"--".to_vec());
    assert_eq!(parser.push(b"a-b--c--"), [b"a-b".to_vec(), b"c".to_vec()]);
    assert!(parser.push(b"left|ri").is_empty());
    assert_eq!(parser.set_delimiter(b"|".to_vec()), [b"left".to_vec()]);
    assert_eq!(parser.push(b"ght|"), [b"right".to_vec()]);
    assert_eq!(parser.set_delimiter(Vec::new()), Vec::<Vec<u8>>::new());
    assert_eq!(parser.push(b"raw"), [b"raw".to_vec()]);
}

#[test]
fn file_document_tracks_load_write_and_errors() {
    let path = std::env::temp_dir().join(format!("morf-file-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut file = FileDocument::new(&path, 16);
    assert!(file.preload());
    file.set_preload(false);
    assert!(!file.preload());
    assert!(!file.reload());
    assert_eq!(file.error(), Some(FileViewError::FileNotFound));
    assert!(file.set_data(b"hello"));
    assert!(file.loaded());
    assert!(file.exists());
    assert_eq!(file.text().as_deref(), Some("hello"));
    file.set_atomic_writes(false);
    assert!(!file.atomic_writes());
    assert!(file.set_data(b"world"));
    assert_eq!(std::fs::read(&path).unwrap(), b"world");
    assert!(!file.set_data(b"this value is too large"));
    assert_eq!(file.error(), Some(FileViewError::TooLarge));
    file.set_watch_changes(true).unwrap();
    assert!(file.watch_changes());
    file.set_path("").unwrap();
    assert!(file.watch_changes());
    assert!(!file.loaded());
    assert_eq!(file.next_change(Duration::ZERO), None);
    let next = std::env::temp_dir().join(format!("morf-file-next-{}", std::process::id()));
    std::fs::write(&next, b"next").unwrap();
    file.set_path(&next).unwrap();
    assert!(file.watch_changes());
    assert!(!file.loaded());
    assert!(file.reload());
    assert_eq!(file.text().as_deref(), Some("next"));
    file.set_path(std::env::temp_dir()).unwrap();
    assert!(!file.reload());
    assert_eq!(file.error(), Some(FileViewError::NotAFile));
    file.set_path(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(!file.reload());
    assert_eq!(file.error(), Some(FileViewError::FileNotFound));
    std::fs::remove_file(next).unwrap();
}

#[test]
fn socket_server_accepts_without_blocking() {
    let path = std::env::temp_dir().join(format!("morf-io-server-{}", std::process::id()));
    let server = SocketServer::bind(&path).unwrap();
    assert!(server.try_accept().unwrap().is_none());
    let _client = Socket::connect(&path).unwrap();
    assert!(server.try_accept().unwrap().is_some());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn socket_server_drop_preserves_rebound_path() {
    let path = std::env::temp_dir().join(format!("morf-io-server-rebound-{}", std::process::id()));
    let server = SocketServer::bind(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let replacement = UnixListener::bind(&path).unwrap();
    drop(server);
    assert!(path.exists());
    drop(replacement);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn process_streams_output_and_exit() {
    let mut process = Process::spawn("sh", ["-c", "printf out; printf err >&2"]).unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let status = loop {
        match process.next_event(Duration::from_secs(1)).unwrap() {
            Some(ProcessEvent::Stdout(bytes)) => stdout.extend(bytes),
            Some(ProcessEvent::Stderr(bytes)) => stderr.extend(bytes),
            Some(ProcessEvent::Exit(status)) => break status,
            None if std::time::Instant::now() < deadline => {}
            None => panic!("child did not exit"),
        }
    };
    assert!(status.success());
    assert_eq!(stdout, b"out");
    assert_eq!(stderr, b"err");
}

#[test]
fn process_config_applies_directory_and_environment() {
    let directory = std::env::temp_dir();
    let config = ProcessConfig {
        command: vec![
            "sh".into(),
            "-c".into(),
            "printf '%s:%s' \"$PWD\" \"$MORF_PROCESS_TEST\"".into(),
        ],
        environment: BTreeMap::from([("MORF_PROCESS_TEST".into(), "ok".into())]),
        clear_environment: false,
        working_directory: Some(directory.clone()),
    };
    let mut process = Process::spawn_config(&config).unwrap();
    let mut stdout = Vec::new();
    loop {
        match process.next_event(Duration::from_secs(1)).unwrap() {
            Some(ProcessEvent::Stdout(bytes)) => stdout.extend(bytes),
            Some(ProcessEvent::Exit(status)) => {
                assert!(status.success());
                break;
            }
            Some(ProcessEvent::Stderr(_)) | None => {}
        }
    }
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        format!("{}:ok", directory.display())
    );
}

#[test]
fn file_view_writes_and_watches_atomically() {
    let path = std::env::temp_dir().join(format!("morf-io-{}", std::process::id()));
    fs::write(&path, b"old").unwrap();
    let file = FileView::new(&path);
    let watcher = file.watch().unwrap();
    file.write(b"new").unwrap();
    assert_eq!(file.read().unwrap(), b"new");
    assert!(watcher.next_event(Duration::from_secs(1)).is_some());
    fs::remove_file(path).unwrap();
}

#[test]
fn timer_emits_and_stops() {
    let timer = Timer::every(Duration::from_millis(10)).unwrap();
    assert!(timer.tick(Duration::from_secs(1)));
    drop(timer);
}

#[test]
fn ipc_server_keeps_connections_open_and_reads_peer_identity() {
    let path = std::env::temp_dir().join(format!("morf-ipc-{}", std::process::id()));
    let _ = fs::remove_file(&path);
    let (tx, rx) = mpsc::channel();
    let server = IpcServer::bind(&path, tx).unwrap();
    let responder = thread::spawn(move || {
        for _ in 0..3 {
            let incoming = rx.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(incoming.peer.pid, std::process::id());
            incoming.reply(IpcReply::success(vec![IpcValue::String("ok".into())]));
        }
    });
    let mut stream = UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    for request in [IpcRequest::Verbs, IpcRequest::Log, IpcRequest::Bindings] {
        let mut bytes = encode_ipc_request(&request).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).unwrap();
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        assert_eq!(
            decode_ipc_reply(reply.as_bytes()).unwrap(),
            IpcReply::success(vec![IpcValue::String("ok".into())])
        );
    }
    responder.join().unwrap();
    drop(server);
    assert!(!path.exists());
}

#[test]
fn ipc_server_reclaims_stale_socket_paths() {
    let path = std::env::temp_dir().join(format!("morf-ipc-stale-{}", std::process::id()));
    let _ = fs::remove_file(&path);
    fs::write(&path, b"stale").unwrap();
    let (tx, _rx) = mpsc::channel();
    let server = IpcServer::bind(&path, tx).unwrap();
    assert!(path.exists());
    drop(server);
    assert!(!path.exists());
}

#[test]
fn ipc_wire_refuses_nested_values_and_keeps_result_count() {
    assert!(decode_ipc_request(br#"{"op":"call","target":"x","args":[{}]}"#).is_err());
    let bytes = encode_ipc_reply(&IpcReply::success(vec![
        IpcValue::Integer(7),
        IpcValue::Boolean(true),
    ]))
    .unwrap();
    let value: JsonValue = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["n"], 2);
    assert_eq!(value["result"], serde_json::json!([7, true]));
}

#[test]
fn dbus_argument_lists_build_positional_structures() {
    let arguments = [
        DbusValue::String("device".to_owned()),
        DbusValue::Typed {
            signature: "u".to_owned(),
            value: Box::new(DbusValue::Integer(7)),
        },
        DbusValue::Bool(true),
    ];
    let mut body = StructureBuilder::new();
    for argument in &arguments {
        body = body.append_field(dbus_argument_value(argument).unwrap());
    }
    let body = body.build().unwrap();

    assert_eq!(body.fields().len(), 3);
    assert!(matches!(&body.fields()[0], Value::Str(value) if value.as_str() == "device"));
    assert!(matches!(body.fields()[1], Value::U32(7)));
    assert!(matches!(body.fields()[2], Value::Bool(true)));
}

#[test]
fn explicit_dbus_signatures_build_compound_values() {
    let array_value = DbusValue::List(vec![
        DbusValue::String("one".into()),
        DbusValue::String("two".into()),
    ]);
    let array = typed_dbus_value("as", &array_value).unwrap();
    assert_eq!(array.value_signature().to_string(), "as");

    let map_value = DbusValue::Map(BTreeMap::from([
        ("enabled".into(), DbusValue::Bool(true)),
        (
            "count".into(),
            DbusValue::Typed {
                signature: "u".into(),
                value: Box::new(DbusValue::Integer(7)),
            },
        ),
    ]));
    let map = typed_dbus_value("a{sv}", &map_value).unwrap();
    assert_eq!(map.value_signature().to_string(), "a{sv}");
    let decoded = dynamic_value(&map).unwrap();
    let DbusValue::Map(decoded) = decoded else {
        panic!("D-Bus dictionary did not decode as a map");
    };
    assert_eq!(decoded["enabled"], DbusValue::Bool(true));
    assert_eq!(decoded["count"], DbusValue::Unsigned(7));

    let structure_value = DbusValue::List(vec![
        DbusValue::String("name".into()),
        DbusValue::Integer(-2),
    ]);
    let structure = typed_dbus_value("(si)", &structure_value).unwrap();
    assert_eq!(structure.value_signature().to_string(), "(si)");
}

#[test]
fn stream_collector_controls_publication_and_bounds() {
    let mut delayed = StreamCollector::new(8, true).unwrap();
    assert!(!delayed.push(b"ab").unwrap());
    assert_eq!(delayed.data(), b"");
    assert!(delayed.finish());
    assert_eq!(delayed.text(), "ab");

    let mut live = StreamCollector::new(4, false).unwrap();
    assert!(live.push(b"ab").unwrap());
    assert_eq!(live.data(), b"ab");
    assert!(live.push(b"cde").is_err());
    live.reset();
    assert!(!live.finished());
}

#[test]
fn a_finished_process_reports_its_exit_rather_than_an_empty_poll() {
    // Closing the pipes and being reaped are two different moments. Between
    // them the event channel is disconnected, which makes `recv_timeout` return
    // at once — so a `next_event` that gave up there would neither honour the
    // timeout it was given nor ever report the exit, and a caller polling in a
    // loop would spin a core and then conclude the process was still running.
    let mut process = Process::spawn_config(&ProcessConfig {
        command: vec!["sh".to_owned(), "-c".to_owned(), "printf done".to_owned()],
        ..ProcessConfig::default()
    })
    .unwrap();

    let mut output = Vec::new();
    let mut exited = None;
    for _ in 0..8 {
        match process.next_event(Duration::from_millis(500)).unwrap() {
            Some(ProcessEvent::Stdout(bytes)) => output.extend_from_slice(&bytes),
            Some(ProcessEvent::Exit(status)) => {
                exited = Some(status);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(String::from_utf8_lossy(&output), "done");
    assert!(
        exited.is_some_and(|status| status.success()),
        "the exit is reported, not swallowed"
    );
}

#[test]
fn a_property_holding_an_object_path_is_readable() {
    // Object paths and signatures are scalars like any other, and the decoder
    // beside this one has always handled them. The property decoder probed a
    // hand-written list of types instead and quietly left both off it, so a
    // service exposing `o` — which is most of them — had an unreadable
    // property for no reason anyone chose.
    let path = OwnedValue::try_from(Value::ObjectPath(
        ObjectPath::try_from("/org/example/Player").unwrap(),
    ))
    .unwrap();
    assert_eq!(
        basic_value(&path).unwrap(),
        DbusValue::String("/org/example/Player".to_owned())
    );

    let signature =
        OwnedValue::try_from(Value::Signature(Signature::try_from("a{sv}").unwrap())).unwrap();
    assert_eq!(
        basic_value(&signature).unwrap(),
        DbusValue::String("a{sv}".to_owned())
    );
}

#[test]
fn splitting_on_newlines_drops_the_carriage_return_that_comes_with_them() {
    // A line ending is two characters on one of the two systems that write text
    // files, so the newline delimiter has to account for it — that convention
    // is the entire reason a second, near-identical line parser existed. A
    // parser splitting on anything else has no such convention and must not
    // invent one.
    let mut lines = SplitParser::new(b"\n".to_vec());
    let parts = lines.push(b"alpha\r\nbeta\ngamma");
    assert_eq!(parts, vec![b"alpha".to_vec(), b"beta".to_vec()]);
    assert_eq!(lines.finish(), Some(b"gamma".to_vec()));

    let mut dashes = SplitParser::new(b"--".to_vec());
    assert_eq!(
        dashes.push(b"one\r--two--"),
        vec![b"one\r".to_vec(), b"two".to_vec()]
    );
}

mod fsops_tests {
    use crate::fs as ops;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("morf-fsops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn patterns_match_like_a_shell() {
        assert!(ops::matches("*.jpeg", "tiger.jpeg"));
        assert!(!ops::matches("*.jpeg", "tiger.jpg"));
        assert!(ops::matches("*.{jpg,jpeg,png}", "a.png"));
        assert!(ops::matches("t?ger", "tiger"));
        assert!(ops::matches("[a-c]*", "bengal"));
        assert!(!ops::matches("[!a-c]*", "bengal"));
        assert!(ops::matches("*", ""));
        assert!(ops::matches("a*b*c", "aXXbYYc"));
        assert!(!ops::matches("a*b*c", "aXXbYY"));
        assert!(ops::matches("[", "["));
    }

    #[test]
    fn list_write_copy_rename_and_remove() {
        let dir = scratch("ops");
        ops::write(
            &dir.join("sub/a.txt"),
            b"one",
            ops::WriteOptions {
                parents: true,
                ..Default::default()
            },
        )
        .unwrap();
        ops::write(
            &dir.join("sub/a.txt"),
            b" two",
            ops::WriteOptions {
                append: true,
                ..Default::default()
            },
        )
        .unwrap();
        ops::write(
            &dir.join("b.json"),
            b"{}",
            ops::WriteOptions {
                atomic: true,
                ..Default::default()
            },
        )
        .unwrap();
        ops::write(&dir.join(".hidden"), b"", Default::default()).unwrap();
        assert_eq!(ops::read(&dir.join("sub/a.txt"), 64).unwrap(), b"one two");
        assert!(ops::read(&dir.join("sub/a.txt"), 3).is_err());

        let (flat, truncated) = ops::list(&dir, Default::default()).unwrap();
        assert!(!truncated);
        let names = flat.iter().map(|e| e.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names, ["b.json", "sub"]);
        let (deep, _) = ops::list(
            &dir,
            ops::ListOptions {
                hidden: true,
                depth: 4,
                follow: false,
            },
        )
        .unwrap();
        assert_eq!(deep.len(), 4);
        assert_eq!(
            ops::stat(&dir.join("sub")).unwrap().kind,
            ops::EntryKind::Dir
        );
        assert_eq!(ops::stat(&dir.join("b.json")).unwrap().size, 2);

        assert_eq!(
            ops::copy(&dir.join("sub"), &dir.join("copy"), true).unwrap(),
            7
        );
        ops::rename(&dir.join("copy/a.txt"), &dir.join("copy/c.txt")).unwrap();
        assert!(ops::stat(&dir.join("copy/c.txt")).is_ok());
        assert!(ops::remove(&dir.join("copy"), false).is_err());
        ops::remove(&dir.join("copy"), true).unwrap();
        assert!(ops::stat(&dir.join("copy")).is_err());
        assert!(ops::remove(std::path::Path::new("/"), true).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn globs_walk_segments_and_double_star() {
        let dir = scratch("glob");
        for path in ["w/a.jpg", "w/b.png", "w/deep/c.jpg", "w/.x.jpg", "n/d.txt"] {
            ops::write(
                &dir.join(path),
                b"",
                ops::WriteOptions {
                    parents: true,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let base = dir.to_string_lossy();
        let (flat, _) = ops::glob(&format!("{base}/w/*.{{jpg,png}}"));
        assert_eq!(flat.len(), 2);
        let (deep, _) = ops::glob(&format!("{base}/**/*.jpg"));
        assert_eq!(deep.len(), 2, "{deep:?}");
        let (dots, _) = ops::glob(&format!("{base}/w/.*.jpg"));
        assert_eq!(dots.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn expand_and_normalize_paths() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(ops::expand("~/x"), format!("{home}/x"));
        assert_eq!(ops::expand("${HOME}/y"), format!("{home}/y"));
        assert_eq!(ops::expand("$HOME"), home);
        assert_eq!(
            ops::normalize(std::path::Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert!(ops::user_dir("config").is_some());
        assert!(ops::user_dir("nonsense").is_none());
    }
}
