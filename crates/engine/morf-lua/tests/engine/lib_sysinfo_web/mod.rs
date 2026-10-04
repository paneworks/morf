//! The pure-Lua libraries in `library/lib` that watch the machine and the
//! web: sysinfo, weather, github, packages, claude_usage.
//!
//! Each runs inside a real runtime with `library/` as a module root. The
//! machine is a folder of fake /proc and /sys files, the web is a server on
//! loopback serving recorded answers, the package tools are an injected
//! runner, and the transcripts are written by the test. Nothing here needs
//! the network or changes the system.

use std::time::{Duration, Instant};

use super::*;

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../library")
}

/// Runs `source` with `library/` as a module root and pumps the loop until
/// the `seen` text contains `done` (or the deadline passes). Returns the text.
/// Any log line -- a handler that raised or ran out of fuel -- fails the test.
fn run(source: &str, seconds: u64) -> String {
    let mut runtime = Runtime::default();
    runtime.set_module_roots(vec![examples_dir()]);
    let source = format!(
        r#"
        local morf = require("morf")
        local ui = require("morf.ui")
        local seen = morf.signal("test.seen", "")
        local function note(text) seen:set(seen:get() .. tostring(text) .. ";") end
        {source}
        ui.Text {{ text = function() return seen:get() end }}
        "#
    );
    runtime.execute("lib_test.lua", source.as_bytes()).unwrap();
    let root = *runtime.scene().roots().last().unwrap();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        runtime.poll_services();
        let text = runtime
            .scene()
            .string_value(root, "text")
            .unwrap()
            .to_owned();
        let logs = runtime.take_logs();
        assert!(logs.is_empty(), "{logs:?}\n{text}");
        if text.contains("done;") || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("morf-lib-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(root: &std::path::Path, path: &str, text: &str) {
    let path = root.join(path.trim_start_matches('/'));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// What the loopback server answers: a path prefix and a status and body.
type Routes = Vec<(&'static str, u16, String)>;

/// A server on loopback answering each request by the first route whose
/// prefix its path starts with, 404 otherwise. Returns its base URL and the
/// paths (with queries) it was asked for, in order.
fn serve(routes: Routes) -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&hits);
    let routes = Arc::new(routes);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = Arc::clone(&routes);
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.trim().eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = std::io::Read::read_exact(&mut reader, &mut body);
                seen.lock().unwrap().push(if body.is_empty() {
                    path.clone()
                } else {
                    format!("{path} {}", String::from_utf8_lossy(&body))
                });
                let (status, body) = routes
                    .iter()
                    .find(|(prefix, _, _)| path.starts_with(prefix))
                    .map_or((404, String::new()), |(_, status, body)| {
                        (*status, body.clone())
                    });
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.flush();
            });
        }
    });
    (format!("http://{address}"), hits)
}

use std::sync::{Arc, Mutex};

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

mod claude_usage;
mod github;
mod packages;
mod poll;
mod sysinfo;
mod weather;
