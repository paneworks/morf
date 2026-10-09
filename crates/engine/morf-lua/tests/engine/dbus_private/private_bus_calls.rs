//! Calls that answer later, subscriptions that end, and descriptors that go
//! round, over the private bus.

use super::*;

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_calls_answer_later_and_subscriptions_end() {
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let name = format!("org.morf.test.v2.p{}", std::process::id());
    let other = format!("{name}.other");
    let _server = Server::start(&name);
    let body = format!(
        r#"
        local NAME, OTHER, PATH, IFACE = "{name}", "{other}", "{PATH}", "{INTERFACE}"
        local eq = fake.eq
        local dbus = morf.dbus
        local p = dbus.proxy("session", NAME, PATH, IFACE, 2000)
        local got, pings, other_pings, owners = {{}}, {{}}, {{}}, {{}}
        local handle, other_handle, owner_handle, other_service
        local function record(list)
            return function(body, info)
                list[#list + 1] = {{ body = body, sender = info.sender, member = info.member,
                    path = info.path, interface = info.interface, arguments = info.arguments }}
            end
        end
        fake.steps({{
            function()
                -- Any number of arguments, spelled either way.
                local r = p:call_with("Echo", "x", "y", "z")
                eq(#r, 3, "three arguments went")
                eq(r[3], "z", "in order")
                eq(p:call_with("Echo", {{ "a", 2, true }})[2], 2, "the list spelling still works")
                eq(p:call("Echo", 7, 8)[2], 8, "call takes arguments too")
                eq(p:call_with("Echo", "one"), "one", "one argument is itself")
                -- Typed empties.
                eq(p:call_with("Signature", {{ signature = "a{{sv}}", value = {{}} }}), "a{{sv}}", "an empty map")
                eq(p:call_with("Signature", {{ signature = "aay", value = {{}} }}), "aay", "an empty list of lists")
                eq(p:call_with("Signature", {{ signature = "a{{uu}}", value = {{ [2]=3, [4]=10 }} }}),
                    "a{{uu}}", "sparse numeric dictionary keys")
                local retries=p:call("UnlockRetries")[1]
                eq(retries[2],3,"SIM PIN retries retain numeric keys")
                eq(retries[4],10,"SIM PUK retries retain numeric keys")
                eq(p:call_with("Signature", "s", {{ signature = "a{{sv}}", value = {{}} }},
                    {{ signature = "u", value = 3 }}), "sa{{sv}}u", "three typed arguments")
                local ok = pcall(p.call_with, p, "Signature", {{ ssids = {{ signature = "aay", value = {{}} }} }})
                eq(ok, false, "an untyped map as an argument is still refused")

                -- Calls that answer later.
                local same_turn = true
                assert(dbus.call_async("session", NAME, PATH, IFACE, "Echo", {{ "a", 2, true }},
                    {{ timeout_ms = 2000 }}, function(ok, reply)
                        got.module = {{ ok = ok, reply = reply, same_turn = same_turn }}
                    end))
                assert(p:call_async("Echo", "x", "y", function(ok, reply)
                    got.proxy = {{ ok = ok, reply = reply }}
                end))
                assert(dbus.call_async("session", NAME, PATH, IFACE, "Signature",
                    {{ "s", {{ signature = "a{{sv}}", value = {{}} }} }}, function(ok, reply)
                        got.typed = reply
                    end))
                assert(dbus.call_async("session", "org.morf.test.Nobody", "/", "org.morf.Nobody", "X",
                    nil, function(ok, err) got.nobody = {{ ok = ok, err = err }} end))
                assert(dbus.call_async("session", NAME, PATH, IFACE, "Never", nil,
                    {{ timeout_ms = 200 }}, function(ok, err) got.never = {{ ok = ok, err = err }} end))
                same_turn = false

                -- Questions for the bus.
                eq(dbus.name_has_owner("session", NAME), true, "owned")
                eq(dbus.name_has_owner("session", "org.morf.test.Nobody"), false, "not owned")
                eq(dbus.name_owner("session", "org.morf.test.Nobody"), nil, "nobody's")
                local listed = false
                for _, each in ipairs(dbus.list_names("session")) do
                    if each == NAME then listed = true end
                end
                eq(listed, true, "listed")

                -- Subscriptions.
                handle = p:subscribe("Ping", record(pings))
                owner_handle = dbus.on_name_owner_changed("session", OTHER, function(old, new, name)
                    owners[#owners + 1] = {{ old = old, new = new, name = name }}
                end)
            end,
            function()
                eq(got.module.ok, true, "answered")
                eq(got.module.same_turn, false, "on a later turn")
                eq(got.module.reply[1], "a", "first")
                eq(got.module.reply[3], true, "third")
                eq(got.proxy.reply[2], "y", "the proxy's")
                eq(got.typed, "sa{{sv}}", "typed arguments, asynchronously")
                eq(got.nobody.ok, false, "nobody answers")
                assert(type(got.nobody.err) == "string", "with a reason")
                -- A second service on the same path, subscribed too: each
                -- subscription hears only its own sender.
                other_service = assert(dbus.serve("session", OTHER, PATH, false))
                other_handle = dbus.proxy("session", OTHER, PATH, IFACE):subscribe("Ping", record(other_pings))
                other_service:emit(PATH, IFACE, "Ping", "impostor")
                p:call_with("Emit", "hello")
            end,
            function()
                eq(got.never.ok, false, "never answered")
                assert(got.never.err:find("Timeout"), got.never.err)
                eq(#pings, 1, "one ping for the first service")
                eq(pings[1].body, "hello", "its own")
                eq(pings[1].sender, dbus.name_owner("session", NAME), "sent by its owner")
                eq(pings[1].member, "Ping", "member")
                eq(pings[1].path, PATH, "path")
                eq(pings[1].interface, IFACE, "interface")
                eq(pings[1].arguments, "hello", "arguments")
                eq(#other_pings, 1, "one for the second")
                eq(other_pings[1].body, "impostor", "and it was the second's")
                eq(#owners, 1, "the second name arrived")
                eq(owners[1].old, "", "from nobody")
                eq(owners[1].name, OTHER, "named")
                eq(owners[1].new:sub(1, 1), ":", "to a unique name")
                eq(handle:active(), true, "active")
                eq(handle:close(), true, "closed")
                eq(handle:close(), false, "once")
                eq(handle:unsubscribe(), false, "by either name")
                eq(handle:active(), false, "and inactive")
                p:call_with("Emit", "after")
                other_handle:close()
                other_service:close()
            end,
            function()
                eq(#pings, 1, "nothing after close")
                eq(#owners, 2, "the second name left")
                eq(owners[2].new, "", "to nobody")
                owner_handle:close()
            end,
        }}, done)
        "#
    );
    assert_eq!(run_with_fake("test-dbus-private", &body), "ok");
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_a_descriptor_goes_round_and_closes() {
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let name = format!("org.morf.test.fd.p{}", std::process::id());
    let _server = Server::start(&name);
    let body = format!(
        r#"
        local eq = fake.eq
        local p = morf.dbus.proxy("session", "{name}", "{PATH}", "{INTERFACE}", 2000)
        fake.steps({{
            function()
                local fd = p:call("Open")
                eq(type(fd), "userdata", "an opaque handle")
                eq(fd:is_open(), true, "open")
                p:call_with("Take", fd)
                p:call_with("Take", {{ signature = "h", value = fd }})
                eq(fd:close(), true, "closed")
                eq(fd:close(), false, "once")
                eq(fd:is_open(), false, "and says so")
                local ok, err = pcall(p.call_with, p, "Take", fd)
                eq(ok, false, "a closed handle cannot be sent")
                assert(tostring(err):find("closed"), tostring(err))
                -- The same, answered later.
                assert(p:call_async("Open", function(ok, handle)
                    assert(ok, handle)
                    handle:close()
                end))
            end,
            function() end,
        }}, done)
        "#
    );
    assert_eq!(run_with_fake("test-dbus-fd", &body), "ok");
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_descriptor_bytes_arrive_through_the_handle() {
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let name = format!("org.morf.test.fdbytes.p{}", std::process::id());
    let server = Server::start(&name);
    let body = format!(
        r#"
        local p = morf.dbus.proxy("session", "{name}", "{PATH}", "{INTERFACE}", 2000)
        fake.steps({{
            function()
                local fd = p:call("Open")
                p:call_with("Take", fd)
                p:call_with("Take", {{ signature = "h", value = fd }})
                fd:close()
            end,
        }}, done)
        "#
    );
    assert_eq!(run_with_fake("test-dbus-fd-bytes", &body), "ok");
    let peer = server.peer.lock().unwrap().take().expect("opened");
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut rest = Vec::new();
    let read = (&peer).read_to_end(&mut rest);
    // `read_to_end` returns only at end of file, so an `Ok` is also the proof
    // that every copy of the descriptor — the handle's, the messages' — was
    // closed once the handle was.
    assert_eq!(
        rest, b"xx",
        "both writes went through the handed-out socket"
    );
    assert!(read.is_ok(), "and then it closed: {read:?}");
}

#[test]
#[ignore = "runs only inside dbus-run-session, started by the test above"]
fn private_bus_a_runtime_takes_its_subscriptions_with_it() {
    if std::env::var(PRIVATE_BUS).is_err() {
        return;
    }
    let before = morf_io::subscription_count(Bus::Session);
    {
        let mut runtime = Runtime::default();
        runtime
            .execute(
                "subscriptions.lua",
                br#"
                local morf = require("morf")
                local bus = morf.dbus.proxy("session", "org.freedesktop.DBus",
                    "/org/freedesktop/DBus", "org.freedesktop.DBus")
                bus:subscribe("NameOwnerChanged", function() end)
                bus:subscribe("NameAcquired", function() end)
                morf.dbus.on_name_owner_changed("session", "org.morf.test.Nobody", function() end)
                -- In flight as the runtime goes: answered into nothing.
                morf.dbus.call_async("session", "org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus", "ListNames", nil, function() error("never delivered") end)
                local closed = bus:subscribe("NameLost", function() end)
                assert(closed:close())
                "#,
            )
            .unwrap();
        // At least: a runtime may subscribe on its own behalf (the desktop
        // portal's settings), and those go with it too.
        assert!(
            morf_io::subscription_count(Bus::Session) >= before + 3,
            "three live, the closed one gone"
        );
    }
    assert_eq!(
        morf_io::subscription_count(Bus::Session),
        before,
        "a runtime dropped on reload takes every subscription with it"
    );
}
