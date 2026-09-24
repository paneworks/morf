//! `HttpTask` against a server on loopback: no network, real sockets.

use crate::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

/// A tiny HTTP/1.1 server with a few fixed routes; returns its base URL.
fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            thread::spawn(move || {
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
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if name == "content-length" {
                length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let header = |name: &str| {
        headers
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let (status, extra, reply): (&str, String, Vec<u8>) = match path.as_str() {
        "/hello" => ("200 OK", "X-Test: yes\r\n".into(), b"hello".to_vec()),
        "/echo" => (
            "200 OK",
            "Content-Type: application/json\r\n".into(),
            format!(
                "{{\"method\":\"{method}\",\"token\":\"{}\",\"type\":\"{}\",\"body\":{}}}",
                header("x-token"),
                header("content-type"),
                serde_json::to_string(&String::from_utf8_lossy(&body)).unwrap(),
            )
            .into_bytes(),
        ),
        "/missing" => ("404 Not Found", String::new(), b"nope".to_vec()),
        "/redirect" => ("302 Found", "Location: /hello\r\n".into(), Vec::new()),
        "/slow" => {
            thread::sleep(Duration::from_secs(3));
            ("200 OK", String::new(), b"late".to_vec())
        }
        "/big" => ("200 OK", String::new(), vec![b'x'; 256 * 1024]),
        _ => ("400 Bad Request", String::new(), Vec::new()),
    };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        reply.len()
    )?;
    stream.write_all(&reply)?;
    stream.flush()
}

fn run(request: HttpRequest) -> Result<HttpResponse, String> {
    HttpTask::start(request)
        .wait(Duration::from_secs(10))
        .expect("the request finished in time")
}

#[test]
fn a_get_returns_status_headers_and_body() {
    let base = serve();
    let response = run(HttpRequest::get(format!("{base}/hello"))).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "x-test" && value == "yes"),
        "header names come back lowercased: {:?}",
        response.headers
    );
}

#[test]
fn a_post_carries_its_method_headers_and_body() {
    let base = serve();
    let mut request = HttpRequest::get(format!("{base}/echo"));
    request.method = "post".into();
    request.headers = vec![
        ("X-Token".into(), "abc".into()),
        ("Content-Type".into(), "text/plain".into()),
    ];
    request.body = Some(b"payload".to_vec());
    let response = run(request).unwrap();
    let echoed: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(echoed["method"], "POST");
    assert_eq!(echoed["token"], "abc");
    assert_eq!(echoed["type"], "text/plain");
    assert_eq!(echoed["body"], "payload");
}

#[test]
fn an_error_status_is_a_response_not_a_failure() {
    let base = serve();
    let response = run(HttpRequest::get(format!("{base}/missing"))).unwrap();
    assert_eq!(response.status, 404);
    assert_eq!(response.body, b"nope");
}

#[test]
fn a_redirect_is_followed_and_the_final_url_reported() {
    let base = serve();
    let response = run(HttpRequest::get(format!("{base}/redirect"))).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.url, format!("{base}/hello"));
}

#[test]
fn a_slow_server_times_out() {
    let base = serve();
    let mut request = HttpRequest::get(format!("{base}/slow"));
    request.timeout = Duration::from_millis(200);
    let error = run(request).unwrap_err();
    assert!(error.contains("timed out"), "{error}");
}

#[test]
fn a_body_over_the_limit_is_refused() {
    let base = serve();
    let mut request = HttpRequest::get(format!("{base}/big"));
    request.max_bytes = 1024;
    let error = run(request).unwrap_err();
    assert!(error.contains("exceeds"), "{error}");
    let mut request = HttpRequest::get(format!("{base}/big"));
    request.max_bytes = 256 * 1024;
    assert_eq!(run(request).unwrap().body.len(), 256 * 1024);
}

#[test]
fn a_bad_url_is_an_error_not_a_panic() {
    for url in ["not a url", "ftp://example.com/x", "http://", ""] {
        let error = run(HttpRequest::get(url)).unwrap_err();
        assert!(!error.is_empty(), "{url}");
    }
    let refused = run(HttpRequest::get("http://127.0.0.1:1/")).unwrap_err();
    assert!(!refused.is_empty());
}

#[test]
fn a_cancelled_request_says_so() {
    let base = serve();
    let mut task = HttpTask::start(HttpRequest::get(format!("{base}/slow")));
    task.cancel();
    let outcome = task.wait(Duration::from_secs(10)).expect("an answer");
    assert_eq!(outcome.unwrap_err(), "cancelled");
    assert!(task.poll().is_none(), "the answer comes once");
}

#[test]
fn many_requests_share_the_bounded_pool() {
    let base = serve();
    let mut tasks: Vec<_> = (0..40)
        .map(|_| HttpTask::start(HttpRequest::get(format!("{base}/hello"))))
        .collect();
    for task in &mut tasks {
        let response = task.wait(Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(response.body, b"hello");
    }
}

#[test]
fn url_encoding_keeps_unreserved_and_escapes_the_rest() {
    assert_eq!(url_encode(b"a b&c=d/~_.-"), "a%20b%26c%3Dd%2F~_.-");
    assert_eq!(url_encode("é".as_bytes()), "%C3%A9");
    assert_eq!(url_decode(b"a%20b+c%zz%4"), b"a b c%zz%4");
}

#[test]
fn an_answer_rings_the_loop() {
    // The shell's loop sleeps with no timeout when nothing is due; it sees
    // an answer because the worker rings its alarm.
    let base = serve();
    let wake = Wake::new().unwrap();
    let mut task = HttpTask::start(HttpRequest::get(format!("{base}/hello")));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        wake.drain();
        if let Some(outcome) = task.poll() {
            assert_eq!(outcome.unwrap().body, b"hello");
            break;
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(wake.wait(left), "the answer rang the loop");
    }
}
