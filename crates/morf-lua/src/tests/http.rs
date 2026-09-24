//! `morf.http` from a configuration, against a server on loopback.
//!
//! Each test starts its own tiny server, issues requests from Lua, and pumps
//! `poll_services` the way the output loop does until the callbacks have
//! written what they saw into a signal the test can read back.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use super::*;

fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = answer(stream);
            });
        }
    });
    format!("http://{address}")
}

fn answer(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").to_owned();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end().to_owned();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            if name == "content-length" {
                length = value.trim().parse().unwrap_or(0);
            }
            headers.push((name, value.trim().to_owned()));
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let header = |name: &str| {
        headers
            .iter()
            .find(|(known, _)| known == name)
            .map_or(String::new(), |(_, value)| value.clone())
    };
    let (status, reply) = match path.split('?').next().unwrap_or("") {
        "/json" => (
            "200 OK",
            r#"{"temp":21.5,"tags":["a","b"],"none":null}"#.into(),
        ),
        "/echo" => (
            "200 OK",
            serde_json::json!({
                "method": method,
                "path": path,
                "token": header("x-token"),
                "type": header("content-type"),
                "body": String::from_utf8_lossy(&body),
            })
            .to_string(),
        ),
        "/missing" => ("404 Not Found", "gone".to_owned()),
        "/slow" => {
            std::thread::sleep(Duration::from_secs(2));
            ("200 OK", "late".to_owned())
        }
        "/big" => ("200 OK", "x".repeat(64 * 1024)),
        _ => ("400 Bad Request", String::new()),
    };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nX-Served-By: test\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    )?;
    stream.flush()
}

/// Runs `source`, which records into the `seen` signal, and pumps the loop
/// until `seen` holds `want` entries separated by `;`.
fn run(source: &str, want: usize) -> String {
    let mut runtime = Runtime::default();
    let source = format!(
        r#"
        local morf = require("morf")
        local ui = require("morf.ui")
        local seen = morf.signal("http.seen", "")
        local function note(text) seen:set(seen:get() .. text .. ";") end
        {source}
        ui.Text {{ text = function() return seen:get() end }}
        "#
    );
    runtime.execute("http.lua", source.as_bytes()).unwrap();
    let root = runtime.scene().roots()[0];
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        runtime.poll_services();
        let text = runtime
            .scene()
            .string_value(root, "text")
            .unwrap()
            .to_owned();
        if text.matches(';').count() >= want || Instant::now() > deadline {
            let logs = runtime.take_logs();
            assert!(
                logs.iter().all(|entry| !entry.message.contains("http")),
                "{logs:?}"
            );
            return text;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_get_delivers_status_headers_body_and_json() {
    let base = serve();
    let text = run(
        &format!(
            r#"
            local handle = morf.http.get("{base}/json", function(r)
                local data = r.json()
                note(tostring(r.ok) .. " " .. r.status .. " " .. r.headers["x-served-by"])
                note(data.temp .. " " .. data.tags[2] .. " " .. tostring(data.none == morf.json.null))
                note(r.url == "{base}/json" and "url" or r.url)
                note(morf.json.encode(r:json().tags))
            end)
            assert(handle:done() == false)
            "#
        ),
        4,
    );
    assert_eq!(text, r#"true 200 test;21.5 b true;url;["a","b"];"#);
}

#[test]
fn a_post_sends_json_with_headers_and_a_query() {
    let base = serve();
    let text = run(
        &format!(
            r#"
            local q = morf.http.query {{ b = "x y", a = 1, list = {{ "p", "q" }} }}
            assert(q == "a=1&b=x%20y&list=p&list=q", q)
            morf.http.post("{base}/echo?" .. q, {{ name = "morf" }}, {{ headers = {{ ["X-Token"] = "t1" }} }},
                function(r)
                    local e = r.json()
                    note(e.method .. " " .. e.token .. " " .. e.type:gsub(";", ","))
                    note(morf.json.decode(e.body).name .. " " .. e.path)
                end)
            morf.http.request {{
                url = "{base}/echo", method = "put", body = "raw",
                on_done = function(r) note(r.json().method .. " " .. r.json().body) end,
            }}
            "#
        ),
        3,
    );
    assert!(
        text.contains("POST t1 application/json, charset=utf-8;"),
        "{text}"
    );
    assert!(
        text.contains("morf /echo?a=1&b=x%20y&list=p&list=q;"),
        "{text}"
    );
    assert!(text.contains("PUT raw;"), "{text}");
}

#[test]
fn failures_arrive_in_the_response() {
    let base = serve();
    let text = run(
        &format!(
            r#"
            local function show(r)
                local _, err = r.json()
                note(tostring(r.ok) .. " " .. r.status .. " " .. tostring(r.error) .. " " .. tostring(err ~= nil))
            end
            morf.http.get("{base}/missing", show)
            morf.http.get("{base}/slow", {{ timeout_ms = 100 }}, show)
            morf.http.get("{base}/big", {{ max_bytes = 1000 }}, show)
            morf.http.get("not a url", show)
            "#
        ),
        4,
    );
    assert!(text.contains("false 404 nil true;"), "{text}");
    assert!(text.contains("false 0 timed out true;"), "{text}");
    assert!(
        text.contains("false 0 body exceeds 1000 bytes true;"),
        "{text}"
    );
    assert!(text.contains("false 0 invalid url"), "{text}");
}

#[test]
fn a_cancelled_request_never_calls_back() {
    let base = serve();
    let text = run(
        &format!(
            r#"
            local slow = morf.http.get("{base}/slow", function() note("cancelled one ran") end)
            assert(slow:cancel() == true and slow:done() == true)
            assert(slow:cancel() == false)
            morf.http.get("{base}/json", function(r) note("other " .. r.status) end)
            "#
        ),
        1,
    );
    assert_eq!(text, "other 200;");
}

#[test]
fn a_malformed_call_raises() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "http-bad.lua",
            br#"
            assert(not pcall(morf.http.request, { method = "GET" }))
            assert(not pcall(morf.http.request, { url = "http://x", body = "a", json = {} }))
            assert(not pcall(morf.http.request, { url = "http://x", timeout_ms = -1 }))
            assert(not pcall(morf.http.request, { url = "http://x", on_done = 3 }))
            assert(not pcall(morf.http.get, "http://x", 5))
            assert(morf.http.url_encode("a b/\xC3\xA9") == "a%20b%2F%C3%A9")
            assert(morf.http.url_decode("a%20b+c") == "a b c")
            "#,
        )
        .unwrap();
}

#[test]
fn dropping_the_runtime_with_requests_in_flight_is_quiet() {
    // A reload builds a new runtime and drops the old one; its requests are
    // still on the wire. Nothing may call into the dead Lua state.
    let base = serve();
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "http-drop.lua",
            format!(
                r#"for i = 1, 8 do morf.http.get("{base}/slow", function() error("dead") end) end"#
            )
            .as_bytes(),
        )
        .unwrap();
    runtime.poll_services();
    drop(runtime);
    std::thread::sleep(Duration::from_millis(50));
}
