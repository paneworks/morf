//! The gate over every API that spawns, writes, or talks to a bus, a socket
//! or the network: refused with `nil, reason` and one log line while an
//! unapproved imported module loads, and untouched for everyone else.

use super::{Fixture, appears, refusals, stays_absent, touch};

/// Each gated API, called the way the probe calls it.
const CALLS: &[(&str, &str)] = &[
    (
        "morf.process.spawn",
        "morf.process.spawn, { command = { 'touch', MARK } }",
    ),
    ("morf.process.run", "morf.process.run, { 'touch', MARK }"),
    ("morf.process.kill", "morf.process.kill, 999999"),
    ("morf.process.open", "morf.process.open, 'touch', { MARK }"),
    (
        "morf.process.view",
        "morf.process.view, { command = { 'touch', MARK }, running = true }",
    ),
    (
        "morf.process.detach",
        "morf.process.detach, { 'touch', MARK }",
    ),
    (
        "ui.Terminal",
        "require('morf.ui').Terminal, { command = { 'touch', MARK } }",
    ),
    (
        "launch",
        "morf.apps.entries({ OUT }).launch, morf.apps.entries({ OUT }), 'x'",
    ),
    (
        "morf.socket.connect",
        "morf.socket.connect, { path = OUT .. '/socket' }",
    ),
    (
        "morf.socket.request",
        "morf.socket.request, OUT .. '/socket', 'hello'",
    ),
    ("morf.socket.open", "morf.socket.open, OUT .. '/socket'"),
    ("morf.socket.server", "morf.socket.server, OUT .. '/server'"),
    ("morf.broadcast", "morf.broadcast, 'verb'"),
    (
        "morf.http.request",
        "morf.http.request, { url = 'http://127.0.0.1:9/' }",
    ),
    ("morf.http.get", "morf.http.get, 'http://127.0.0.1:9/'"),
    (
        "morf.http.post",
        "morf.http.post, 'http://127.0.0.1:9/', 'x'",
    ),
    (
        "morf.dbus.proxy",
        "morf.dbus.proxy, { bus = 'session', service = 'a.b', path = '/', interface = 'a.b' }",
    ),
    ("morf.dbus.call_async", "morf.dbus.call_async, {}"),
    ("morf.dbus.serve", "morf.dbus.serve, 'session', 'a.b', '/'"),
    (
        "morf.dbus.name_has_owner",
        "morf.dbus.name_has_owner, 'session', 'a.b'",
    ),
    (
        "morf.dbus.name_owner",
        "morf.dbus.name_owner, 'session', 'a.b'",
    ),
    ("morf.dbus.list_names", "morf.dbus.list_names, 'session'"),
    (
        "morf.dbus.on_name_owner_changed",
        "morf.dbus.on_name_owner_changed, 'session', function() end",
    ),
    (
        "morf.status_notifier.subscribe",
        "morf.status_notifier.subscribe, function() end",
    ),
    (
        "morf.pam.authenticate",
        "morf.pam.authenticate, 'login', 'u', 'p', function() end",
    ),
    ("morf.pam.session", "morf.pam.session, 'login', 'u'"),
    ("morf.greetd.connect", "morf.greetd.connect"),
    ("morf.greetd.converse", "morf.greetd.converse, 'u'"),
    ("morf.fs.write", "morf.fs.write, OUT .. '/written', 'x'"),
    ("morf.fs.mkdir", "morf.fs.mkdir, OUT .. '/made'"),
    ("morf.fs.remove", "morf.fs.remove, OUT .. '/keep'"),
    (
        "morf.fs.rename",
        "morf.fs.rename, OUT .. '/keep', OUT .. '/moved'",
    ),
    (
        "morf.fs.copy",
        "morf.fs.copy, OUT .. '/keep', OUT .. '/copied'",
    ),
    (
        "morf.fs.symlink",
        "morf.fs.symlink, OUT .. '/keep', OUT .. '/link'",
    ),
    ("file:write", "HANDLES.file.write, HANDLES.file, 'x'"),
    (
        "file_view:set_text",
        "HANDLES.view.set_text, HANDLES.view, 'x'",
    ),
    (
        "morf.image.process",
        "morf.image.process, { source = OUT .. '/keep', output = OUT .. '/p.png' }",
    ),
    (
        "morf.image.compose",
        "morf.image.compose, { width = 1, height = 1, output = OUT .. '/c.png' }",
    ),
];

fn probe_source() -> String {
    let mut lines = vec![
        "local results = {}".to_owned(),
        "local function try(name, fn, ...)".to_owned(),
        "  local ok, value, why = pcall(fn, ...)".to_owned(),
        "  results[name] = { ok = ok, value = value, why = why }".to_owned(),
        "end".to_owned(),
    ];
    for (name, call) in CALLS {
        lines.push(format!("try('{name}', {call})"));
    }
    lines.push("return results".to_owned());
    lines.join("\n")
}

#[test]
fn every_gated_api_refuses_while_an_unapproved_module_loads() {
    let fixture = Fixture::new("every-api");
    let mark = fixture.out("spawned");
    fixture.write("out/keep", "kept");
    fixture.write("site/probe.lua", &probe_source());
    let mut runtime = fixture.runtime();
    let config = format!(
        r#"
        MARK = "{mark}"
        OUT = "{out}"
        HANDLES = {{ file = morf.fs.file(OUT .. "/handle"), view = morf.fs.view {{ path = OUT .. "/view" }} }}
        local results = require("probe")
        local wrong = {{}}
        for name, result in pairs(results) do
          local refused = result.ok and result.value == nil and type(result.why) == "string"
            and result.why:find("permission", 1, true) and result.why:find("`probe`", 1, true)
          if not refused then
            wrong[#wrong + 1] = name .. ": " .. tostring(result.ok) .. " " .. tostring(result.value)
              .. " " .. tostring(result.why)
          end
        end
        table.sort(wrong)
        assert(#wrong == 0, table.concat(wrong, "\n"))
        COUNT = 0
        for _ in pairs(results) do COUNT = COUNT + 1 end
        assert(COUNT == {count}, "every call was made: " .. COUNT)
        "#,
        mark = mark.display(),
        out = fixture.out("").display(),
        count = CALLS.len(),
    );
    fixture.run(&mut runtime, &config);
    let lines = refusals(&mut runtime);
    assert_eq!(
        lines.len(),
        CALLS.len(),
        "one log line per refusal: {lines:#?}"
    );
    assert!(stays_absent(&mark), "nothing was spawned");
    for name in [
        "written", "made", "moved", "copied", "link", "handle", "view", "i.png",
    ] {
        assert!(!fixture.out(name).exists(), "{name} was not written");
    }
    assert!(fixture.out("keep").exists(), "nothing was removed");
}

#[test]
fn the_configuration_itself_is_not_gated() {
    let fixture = Fixture::new("config-free");
    let mark = fixture.out("config-spawned");
    let mut runtime = fixture.runtime();
    fixture.run(
        &mut runtime,
        &format!(
            r#"
            local child, why = {spawn}
            assert(child ~= nil, tostring(why))
            assert(morf.fs.write("{out}", "mine"))
            "#,
            spawn = touch(&mark),
            out = fixture.out("config-wrote").display(),
        ),
    );
    assert!(appears(&mark));
    assert_eq!(
        std::fs::read_to_string(fixture.out("config-wrote")).unwrap(),
        "mine"
    );
    assert!(refusals(&mut runtime).is_empty());
}

#[test]
fn a_directory_module_may_write_only_inside_its_own_folder() {
    let fixture = Fixture::new("own-folder");
    let inside = fixture.site().join("cache/state.txt");
    let outside = fixture.out("escaped.txt");
    fixture.write(
        "site/cache/init.lua",
        &format!(
            r#"return {{
              inside = {{ morf.fs.write("{inside}", "ok") }},
              outside = {{ morf.fs.write("{outside}", "no") }},
              dotdot = {{ morf.fs.write("{inside_dir}/../../out/dotdot.txt", "no") }},
            }}"#,
            inside = inside.display(),
            outside = outside.display(),
            inside_dir = fixture.site().join("cache").display(),
        ),
    );
    fixture.write(
        "site/single.lua",
        &format!(
            r#"return {{ morf.fs.write("{}", "no") }}"#,
            fixture.site().join("single-cache.txt").display()
        ),
    );
    let mut runtime = fixture.runtime();
    fixture.run(
        &mut runtime,
        r#"
        local cache = require("cache")
        assert(cache.inside[1] == true, tostring(cache.inside[2]))
        assert(cache.outside[1] == nil and cache.outside[2]:find("permission"))
        assert(cache.dotdot[1] == nil, "a path out through .. is outside")
        local single = require("single")
        assert(single[1] == nil, "a single-file module has no folder of its own")
        "#,
    );
    assert!(inside.exists());
    assert!(!outside.exists());
}
