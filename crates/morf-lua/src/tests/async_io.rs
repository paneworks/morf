//! `morf.spawn`, `morf.run`, `morf.connect` and `morf.request_socket`: output
//! arriving as callbacks, pumped through `poll_services` as the loop does.
//!
//! Only real programs with argv, never a shell: `printf`, `cat`, `sleep`,
//! `false`, `env`, `head`.

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use super::*;

/// Pumps the runtime until `check` runs without error.
fn wait_for(runtime: &mut Runtime, check: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        runtime.poll_services();
        let error = match runtime.execute("check.lua", check.as_bytes()) {
            Ok(()) => return,
            Err(error) => error,
        };
        if Instant::now() > deadline {
            panic!("never became true: {check}\nlast error: {error}");
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn start(source: &str) -> Runtime {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "io.lua",
            format!("morf = require(\"morf\")\n{source}").as_bytes(),
        )
        .unwrap();
    runtime
}

fn quiet(runtime: &mut Runtime) {
    let logs = runtime.take_logs();
    assert!(
        logs.iter().all(|entry| !entry.message.contains("I/O")),
        "{logs:?}"
    );
}

#[test]
fn spawn_delivers_lines_then_the_exit() {
    let mut runtime = start(
        r#"
        lines, exited = {}, nil
        child = morf.spawn {
            command = { "printf", "one\ntwo\n\nlast" },
            on_stdout = function(line) lines[#lines + 1] = line end,
            on_exit = function(code, signal, timed_out)
                exited = { code = code, signal = signal, timed_out = timed_out, seen = #lines }
            end,
        }
        assert(child:running() and child:pid() > 0)
        "#,
    );
    wait_for(
        &mut runtime,
        r#"
        assert(exited, "no exit yet")
        assert(exited.code == 0 and exited.signal == nil and exited.timed_out == false)
        assert(exited.seen == 4, "the exit comes after every line")
        assert(table.concat(lines, "|") == "one|two||last", table.concat(lines, "|"))
        assert(not child:running())
        "#,
    );
    quiet(&mut runtime);
}

#[test]
fn spawn_writes_stdin_and_reads_it_back_in_chunks() {
    let mut runtime = start(
        r#"
        got, done = "", false
        cat = morf.spawn {
            command = { "cat" }, stdin = "pipe", lines = false,
            on_stdout = function(chunk) got = got .. chunk end,
            on_exit = function() done = true end,
        }
        assert(cat:write("hello "))
        assert(cat:write("world\n"))
        cat:close_stdin()
        "#,
    );
    wait_for(
        &mut runtime,
        r#"assert(done and got == "hello world\n", got)"#,
    );
}

#[test]
fn run_collects_output_codes_and_failures() {
    let mut runtime = start(
        r#"
        results = {}
        morf.run({ "printf", "%s-%s", "a", "b" }, function(r) results.printf = r end)
        morf.run({ "false" }, function(r) results.fail = r end)
        morf.run({ "cat" }, { stdin = "from stdin" }, function(r) results.cat = r end)
        morf.run({ "/nonexistent/program" }, function(r) results.missing = r end)
        -- Started from inside a callback: no pool, no pre-made views.
        morf.run({ "printf", "outer" }, function(r)
            morf.run({ "printf", r.stdout .. "+inner" }, function(n) results.nested = n end)
        end)
        "#,
    );
    wait_for(
        &mut runtime,
        r#"
        local r = results
        assert(r.printf and r.fail and r.cat and r.missing and r.nested, "waiting")
        assert(r.printf.ok and r.printf.code == 0 and r.printf.stdout == "a-b")
        assert(not r.fail.ok and r.fail.code == 1 and r.fail.stdout == "")
        assert(r.cat.stdout == "from stdin")
        assert(not r.missing.ok and r.missing.code == nil and r.missing.error:find("nonexistent"))
        assert(r.nested.stdout == "outer+inner")
        "#,
    );
}

#[test]
fn run_times_out_and_kill_ends_a_child() {
    let mut runtime = start(
        r#"
        results = {}
        morf.run({ "sleep", "30" }, { timeout_ms = 50 }, function(r) results.slow = r end)
        sleeper = morf.spawn {
            command = { "sleep", "30" },
            on_exit = function(code, signal) results.killed = { code = code, signal = signal } end,
        }
        assert(sleeper:kill("KILL") == true)
        "#,
    );
    wait_for(
        &mut runtime,
        r#"
        assert(results.slow and results.killed, "waiting")
        assert(results.slow.timed_out and not results.slow.ok and results.slow.signal == 15)
        assert(results.killed.code == nil and results.killed.signal == 9)
        assert(sleeper:kill() == false, "nothing left to kill")
        "#,
    );
}

#[test]
fn spawn_strips_ld_library_path_unless_given() {
    // SAFETY: a variable only these tests read.
    unsafe { std::env::set_var("LD_LIBRARY_PATH", "/nix/store/wrapped") };
    let mut runtime = start(
        r#"
        results = {}
        morf.run({ "env" }, function(r) results.plain = r.stdout end)
        morf.run({ "env" }, { env = { LD_LIBRARY_PATH = "/mine", MORF_X = "1" } },
            function(r) results.given = r.stdout end)
        morf.run({ "env" }, { clear_env = true, env = { ONLY = "this" } },
            function(r) results.clear = r.stdout end)
        "#,
    );
    wait_for(
        &mut runtime,
        r#"
        local r = results
        assert(r.plain and r.given and r.clear, "waiting")
        assert(not r.plain:find("LD_LIBRARY_PATH="), r.plain)
        assert(r.plain:find("\nPATH=") or r.plain:find("^PATH="))
        assert(r.given:find("LD_LIBRARY_PATH=/mine\n", 1, true) and r.given:find("MORF_X=1", 1, true))
        assert(r.clear == "ONLY=this\n", r.clear)
        "#,
    );
}

#[test]
fn many_lines_arrive_in_order_across_turns() {
    let mut runtime = start(
        r#"
        count, ordered, done = 0, true, false
        -- head over a pipe from printf would need a shell; seq is argv too.
        morf.spawn {
            command = { "seq", "1", "5000" },
            on_stdout = function(line)
                count = count + 1
                if tonumber(line) ~= count then ordered = false end
            end,
            on_exit = function() done = true end,
        }
        "#,
    );
    // One turn hands a handle at most a batch, so this takes several.
    runtime.poll_services();
    wait_for(
        &mut runtime,
        r#"assert(done and count == 5000 and ordered, tostring(count))"#,
    );
}

#[test]
fn huge_output_is_bounded() {
    let mut runtime = start(
        r#"
        results = {}
        morf.run({ "head", "-c", "20000000", "/dev/zero" }, { max_output = 1000 },
            function(r) results.capped = r end)
        seen = 0
        morf.spawn {
            command = { "head", "-c", "3000000", "/dev/zero" }, lines = false,
            max_output = 5000,
            on_stdout = function(chunk) seen = seen + #chunk end,
            on_exit = function() results.stream = seen end,
        }
        "#,
    );
    wait_for(
        &mut runtime,
        r#"
        assert(results.capped and results.stream, "waiting")
        assert(#results.capped.stdout == 1000 and results.capped.truncated and results.capped.code == 0)
        assert(results.stream == 5000, tostring(results.stream))
        "#,
    );
}

#[test]
fn a_closed_handle_calls_back_no_more() {
    let mut runtime = start(
        r#"
        calls = 0
        child = morf.spawn {
            command = { "seq", "1", "100000" },
            on_stdout = function() calls = calls + 1; child:close() end,
            on_exit = function() calls = calls + 1000 end,
        }
        "#,
    );
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        runtime.poll_services();
        thread::sleep(Duration::from_millis(2));
    }
    runtime
        .execute("check.lua", b"assert(calls == 1, tostring(calls))")
        .unwrap();
}

#[test]
fn children_die_with_the_runtime() {
    let runtime = start(
        r#"
        child = morf.spawn { command = { "sleep", "30" } }
        kept = morf.spawn { command = { "sleep", "30" }, detached = true }
        require("morf.ui").Text { text = child:pid() .. " " .. kept:pid() }
        "#,
    );
    let root = runtime.scene().roots()[0];
    let text = runtime
        .scene()
        .string_value(root, "text")
        .unwrap()
        .to_owned();
    let pids = text
        .split(' ')
        .map(|pid| pid.parse::<i32>().unwrap())
        .collect::<Vec<_>>();
    let (pid, kept) = (pids[0], pids[1]);
    assert_eq!(unsafe { libc_kill(pid, 0) }, 0);
    drop(runtime);
    assert_eq!(unsafe { libc_kill(pid, 0) }, -1, "killed and reaped");
    assert_eq!(
        unsafe { libc_kill(kept, 0) },
        0,
        "a detached child lives on"
    );
    unsafe { libc_kill(kept, 9) };
}

unsafe extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, signal: i32) -> i32;
}

fn socket_path(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("morf-async-io-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn connect_reads_lines_and_chunks_sends_and_sees_the_close() {
    let path = socket_path("lines");
    let listener = UnixListener::bind(&path).unwrap();
    thread::spawn(move || {
        for round in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut hello = [0u8; 6];
            stream.read_exact(&mut hello).unwrap();
            assert_eq!(&hello, b"hello\n");
            stream
                .write_all(format!("a>>{round}\nb>>two\ntail").as_bytes())
                .unwrap();
        }
    });
    let mut runtime = start(&format!(
        r#"
        events = {{}}
        local function note(text) events[#events + 1] = text end
        lines = morf.connect {{
            path = {path:?},
            on_connect = function() note("connect"); lines:send("hello\n") end,
            on_line = function(line) note(line) end,
            on_close = function(reason) note("close " .. reason) end,
        }}
        "#
    ));
    wait_for(
        &mut runtime,
        r#"
        assert(table.concat(events, "|") == "connect|a>>0|b>>two|tail|close eof", table.concat(events, "|"))
        assert(not lines:connected())
        "#,
    );
    // A reconnect is just another connect, from a callback or anywhere.
    runtime
        .execute(
            "again.lua",
            format!(
                r#"
                chunks = ""
                again = morf.connect {{
                    path = {path:?},
                    on_connect = function() again:send("hello\n") end,
                    on_data = function(chunk) chunks = chunks .. chunk end,
                    on_close = function(reason) closed = reason end,
                }}
                "#
            )
            .as_bytes(),
        )
        .unwrap();
    wait_for(
        &mut runtime,
        r#"assert(closed == "eof" and chunks == "a>>1\nb>>two\ntail", chunks)"#,
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn request_socket_answers_refuses_and_times_out() {
    let path = socket_path("request");
    let listener = UnixListener::bind(&path).unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut request = vec![0u8; 256];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            match request.as_str() {
                // Answers nothing and holds the connection open.
                "silent" => {
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(2));
                        drop(stream);
                    });
                }
                "big" => {
                    let _ = stream.write_all(&vec![b'x'; 10_000]);
                }
                _ => {
                    let _ = stream.write_all(format!("answer to {request}").as_bytes());
                }
            }
        }
    });
    let missing = socket_path("missing");
    let mut runtime = start(&format!(
        r#"
        got = {{}}
        morf.request_socket({path:?}, "j/monitors", function(reply, err) got.ok = {{ reply, err }} end)
        morf.request_socket({path:?}, "silent", function(reply, err) got.slow = {{ reply, err }} end,
            {{ timeout_ms = 60 }})
        morf.request_socket({path:?}, "big", function(reply, err) got.big = {{ reply, err }} end,
            {{ max_bytes = 100 }})
        morf.request_socket({missing:?}, "x", function(reply, err) got.missing = {{ reply, err }} end)
        "#
    ));
    wait_for(
        &mut runtime,
        r#"
        assert(got.ok and got.slow and got.big and got.missing, "waiting")
        assert(got.ok[1] == "answer to j/monitors" and got.ok[2] == nil)
        assert(got.slow[1] == nil and got.slow[2] == "timed out", tostring(got.slow[2]))
        assert(got.big[1] == nil and got.big[2]:find("exceeds"))
        assert(got.missing[1] == nil and type(got.missing[2]) == "string")
        "#,
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn malformed_calls_raise() {
    let mut runtime = Runtime::default();
    for source in [
        "require('morf').spawn { command = {} }",
        "require('morf').spawn { command = 'printf hi' }",
        "require('morf').connect { on_line = function() end }",
        "require('morf').connect { path = '/x', on_line = function() end, on_data = function() end }",
        "require('morf').run({ 'printf' }, 5)",
    ] {
        assert!(
            runtime.execute("bad.lua", source.as_bytes()).is_err(),
            "{source}"
        );
    }
}

/// The panacea example's `proc` module, now a thin layer over `morf.run`
/// and `morf.spawn` with no pool and no tick.
#[test]
fn panacea_proc_runs_and_streams_without_a_tick() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/panacea/proc-test.lua")
        .to_string_lossy()
        .into_owned();
    let mut runtime = Runtime::default();
    runtime
        .execute(
            &script,
            br#"
            local proc = require("proc")
            got = {}
            for index = 1, 20 do
                proc.exec({ "printf", "%d", tostring(index) }, function(out, ok)
                    if ok then got[#got + 1] = tonumber(out) end
                end)
            end
            proc.exec({ "false" }, function(out, ok) failed = not ok end)
            proc.exec({ "/nonexistent/program" }, function(out, ok) missing = (out == "" and not ok) end)
            lines = {}
            proc.stream({ "printf", "a\nb\n" }, function(line) lines[#lines + 1] = line end,
                { retry_ms = 20 })
            "#,
        )
        .unwrap();
    wait_for(
        &mut runtime,
        r#"
        assert(#got == 20 and failed and missing, #got)
        -- Printed, exited, and started again after its retry.
        assert(#lines >= 4 and lines[1] == "a" and lines[4] == "b", #lines)
        "#,
    );
}
