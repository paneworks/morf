use super::*;

fn string(value: &str) -> IpcValue {
    IpcValue::String(value.to_owned())
}

#[test]
fn clipboard_set_takes_text_bytes_and_options() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clipboard-set.lua",
            br#"
                morf.clipboard.set("hello")
                morf.clipboard.set("\137PNG", "image/png")
                morf.clipboard.set("middle", { primary = true })
                assert(not pcall(morf.clipboard.set, 42))
                assert(not pcall(morf.clipboard.set, "x", 7))
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.take_clipboard_requests(),
        [
            ClipboardRequest {
                data: b"hello".to_vec(),
                mime: None,
                primary: false,
            },
            ClipboardRequest {
                data: b"\x89PNG".to_vec(),
                mime: Some("image/png".to_owned()),
                primary: false,
            },
            ClipboardRequest {
                data: b"middle".to_vec(),
                mime: None,
                primary: true,
            },
        ]
    );
}

#[test]
fn clipboard_supported_follows_capabilities() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clipboard-supported.lua",
            br#"
                morf.ipc["supported"] = function()
                    return morf.clipboard.supported(), morf.clipboard.supported("primary"),
                        morf.drag.supported()
                end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.call_ipc("supported", &[]).unwrap(),
        [
            IpcValue::Boolean(false),
            IpcValue::Boolean(false),
            IpcValue::Boolean(false)
        ]
    );
    runtime.set_capabilities(&[
        ("data_control".to_owned(), "true".to_owned()),
        ("primary_selection".to_owned(), "false".to_owned()),
        ("drag_and_drop".to_owned(), "true".to_owned()),
    ]);
    assert_eq!(
        runtime.call_ipc("supported", &[]).unwrap(),
        [
            IpcValue::Boolean(true),
            IpcValue::Boolean(false),
            IpcValue::Boolean(true)
        ]
    );
}

#[test]
fn clipboard_watch_sees_offers_and_reads_them() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clipboard-watch.lua",
            br#"
                local seen = morf.signal("seen", "")
                local got = morf.signal("got", "")
                local primaries = morf.signal("primaries", 0)
                morf.clipboard.watch(function(offer)
                    if not offer then seen:set("cleared") return end
                    seen:set(table.concat(offer.mime_types, ",") .. "@" .. offer.id)
                    assert(offer.primary == false)
                    offer:read("text", function(bytes, err)
                        got:set(bytes or ("error: " .. err))
                    end)
                end)
                morf.clipboard.watch(function(offer)
                    if offer and offer.primary then primaries:set(primaries:get() + 1) end
                end, { primary = true })
                morf.ipc["state"] = function() return seen:get(), got:get(), primaries:get() end
            "#,
        )
        .unwrap();
    assert!(runtime.watches_clipboard());

    assert!(runtime.dispatch_selection(
        false,
        Some(OfferDescription {
            id: 7,
            mime_types: vec!["text/plain".to_owned(), "image/png".to_owned()],
            ..OfferDescription::default()
        }),
    ));
    let reads = runtime.take_offer_reads();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].offer, 7);
    assert_eq!(reads[0].mime, "text");
    assert!(runtime.dispatch_offer_read(reads[0].id, Ok(b"copied".to_vec())));
    // Answered once: the callback is gone after it ran.
    assert!(!runtime.dispatch_offer_read(reads[0].id, Ok(Vec::new())));
    assert_eq!(
        runtime.call_ipc("state", &[]).unwrap(),
        [
            string("text/plain,image/png@7"),
            string("copied"),
            IpcValue::Integer(0)
        ]
    );

    // A failed read reaches the callback as an error, not as silence.
    runtime.dispatch_selection(
        false,
        Some(OfferDescription {
            id: 8,
            mime_types: vec!["text/plain".to_owned()],
            ..OfferDescription::default()
        }),
    );
    let reads = runtime.take_offer_reads();
    runtime.dispatch_offer_read(reads[0].id, Err("the offer is gone".to_owned()));
    assert_eq!(
        runtime.call_ipc("state", &[]).unwrap()[1],
        string("error: the offer is gone")
    );

    // The primary selection reaches only the watcher that asked for it.
    runtime.dispatch_selection(
        true,
        Some(OfferDescription {
            id: 9,
            mime_types: vec!["UTF8_STRING".to_owned()],
            ..OfferDescription::default()
        }),
    );
    assert!(runtime.take_offer_reads().is_empty());
    runtime.dispatch_selection(false, None);
    assert_eq!(
        runtime.call_ipc("state", &[]).unwrap(),
        [
            string("cleared"),
            string("error: the offer is gone"),
            IpcValue::Integer(1)
        ]
    );
}

#[test]
fn offer_reads_are_bounded_and_need_method_syntax() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "clipboard-bounded.lua",
            br#"
                local errors = morf.signal("errors", "")
                morf.clipboard.watch(function(offer)
                    local ok = pcall(offer.read, "text", function() end)
                    assert(not ok)
                    for _ = 1, 16 do offer:read("text", function() end) end
                    local ok, err = pcall(offer.read, offer, "text", function() end)
                    errors:set(tostring(ok) .. " " .. tostring(err))
                end)
                morf.ipc["errors"] = function() return errors:get() end
            "#,
        )
        .unwrap();
    runtime.dispatch_selection(
        false,
        Some(OfferDescription {
            id: 1,
            mime_types: vec!["text/plain".to_owned()],
            ..OfferDescription::default()
        }),
    );
    assert_eq!(runtime.take_offer_reads().len(), 16);
    let errors = runtime.call_ipc("errors", &[]).unwrap();
    let IpcValue::String(errors) = &errors[0] else {
        panic!("{errors:?}");
    };
    assert!(errors.starts_with("false"), "{errors}");
    assert!(errors.contains("offer read limit"), "{errors}");
}

#[test]
fn drop_area_receives_a_drag_and_its_drop() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "drop.lua",
            br#"
                local ui = require("morf.ui")
                local log = morf.signal("log", "")
                local function say(line) log:set(log:get() .. line .. ";") end
                ui.DropArea {
                    width = 100, height = 50,
                    keys = { "files", "text" },
                    on_entered = function(info)
                        say("entered " .. tostring(info.accepted) .. " " .. #info.mime_types
                            .. " @" .. info.x .. "," .. info.y)
                    end,
                    on_moved = function(x, y, sx, sy) say("moved " .. x .. "," .. y .. " " .. sx) end,
                    on_exited = function() say("exited") end,
                    on_dropped = function(drop)
                        say("dropped " .. drop.uris[1] .. " " .. drop.paths[1] .. " "
                            .. tostring(drop.text) .. " " .. drop.accepted)
                        drop:read("image/png", function(bytes) say("png " .. #bytes) end)
                    end,
                }
                morf.ipc["log"] = function() return log:get() end
            "#,
        )
        .unwrap();
    let node = runtime.scene().roots()[0];
    assert_eq!(runtime.drop_area_keys(node), ["files", "text"]);

    let offer = OfferDescription {
        id: 3,
        mime_types: vec!["text/uri-list".to_owned(), "image/png".to_owned()],
        accepted: Some("text/uri-list".to_owned()),
        ..OfferDescription::default()
    };
    let point = EventPoint::new((12.0, 8.0), (2.0, 3.0));
    assert!(runtime.dispatch_drag_entered(node, point, &offer));
    assert!(runtime.dispatch_drag_moved(node, EventPoint::new((13.0, 9.0), (3.0, 4.0))));
    assert!(runtime.dispatch_dropped(
        node,
        point,
        &OfferDescription {
            uris: vec!["file:///tmp/a%20b".to_owned()],
            paths: vec!["/tmp/a b".to_owned()],
            text: Some("file:///tmp/a%20b".to_owned()),
            ..offer.clone()
        },
    ));
    let reads = runtime.take_offer_reads();
    assert_eq!(reads.len(), 1);
    assert_eq!((reads[0].offer, reads[0].mime.as_str()), (3, "image/png"));
    runtime.dispatch_offer_read(reads[0].id, Ok(vec![0; 42]));
    assert!(runtime.dispatch_drag_exited(node));
    assert_eq!(
        runtime.call_ipc("log", &[]).unwrap(),
        [string(
            "entered text/uri-list 2 @2.0,3.0;moved 3.0,4.0 13.0;\
             dropped file:///tmp/a%20b /tmp/a b file:///tmp/a%20b text/uri-list;png 42;exited;"
        )]
    );
}

#[test]
fn drop_area_keys_accept_a_single_string_or_nothing() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "drop-keys.lua",
            br#"
                local ui = require("morf.ui")
                ui.Item {
                    ui.DropArea { keys = "image/*" },
                    ui.DropArea {},
                }
            "#,
        )
        .unwrap();
    let root = runtime.scene().roots()[0];
    let children = runtime.scene().children(root).unwrap().to_vec();
    assert_eq!(runtime.drop_area_keys(children[0]), ["image/*"]);
    assert!(runtime.drop_area_keys(children[1]).is_empty());
    // Nothing bound: nothing dispatched, and no error either.
    assert!(!runtime.dispatch_drag_exited(children[1]));
}

#[test]
fn drag_start_queues_payloads_and_reports_the_end() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "drag.lua",
            br#"
                local ended = morf.signal("ended", "")
                morf.drag.start({
                    text = "hi",
                    paths = { "/tmp/a b" },
                    uris = "https://example.org",
                    data = { ["application/x-thing"] = "\0\1" },
                }, function(dropped) ended:set(tostring(dropped)) end)
                assert(not pcall(morf.drag.start, {}))
                assert(not pcall(morf.drag.start, { data = { [1] = "x" } }))
                morf.ipc["ended"] = function() return ended:get() end
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.take_drag_requests(),
        [DragRequest {
            text: Some("hi".to_owned()),
            uris: vec!["https://example.org".to_owned()],
            paths: vec!["/tmp/a b".to_owned()],
            data: vec![("application/x-thing".to_owned(), vec![0, 1])],
        }]
    );
    assert!(runtime.dispatch_drag_ended(true));
    // Told once.
    assert!(!runtime.dispatch_drag_ended(false));
    assert_eq!(runtime.call_ipc("ended", &[]).unwrap(), [string("true")]);
}
