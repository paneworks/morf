//! HTTP(S) requests, run where a slow server cannot stall a frame.
//!
//! A request is handed to a small pool of worker threads and answered through
//! a channel, with [`crate::wake_all`] poking the loop when the answer lands,
//! the same way a child's output or a D-Bus signal arrives. Nothing here
//! blocks the caller: [`HttpTask::start`] queues and returns, and
//! [`HttpTask::poll`] only looks.
//!
//! Everything is bounded, because the other end is someone else's server: a
//! body larger than the request allows is refused rather than buffered, the
//! whole exchange — connect, redirects, body — shares one deadline, and at
//! most [`MAX_WORKERS`] requests are on the wire at once, the rest waiting
//! their turn. TLS is rustls with the webpki root set compiled in, so the
//! binary does not pick up a dependency on the system's OpenSSL or its
//! certificate store.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// Requests on the wire at once, across every runtime in the process.
pub const MAX_WORKERS: usize = 16;
/// How long an idle worker waits for more work before its thread exits.
const WORKER_IDLE: Duration = Duration::from_secs(30);
/// Redirects followed before the answer is "too many redirects".
pub const MAX_REDIRECTS: u32 = 10;
/// The largest body any request may ask for.
pub const MAX_BODY_LIMIT: usize = 64 * 1024 * 1024;
/// The longest deadline any request may ask for.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(120);

/// One request, as the caller wants it made.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// For the whole exchange; clamped to [`MAX_TIMEOUT`].
    pub timeout: Duration,
    /// The largest body accepted, after decompression; clamped to
    /// [`MAX_BODY_LIMIT`].
    pub max_bytes: usize,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET".to_owned(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout: Duration::from_secs(15),
            max_bytes: 8 * 1024 * 1024,
        }
    }
}

/// What the server said. Any status is a response, 404 and 500 included;
/// only failing to get one at all is an error.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    /// Lowercased names; a repeated header is joined with `, `.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Where the answer came from, after redirects.
    pub url: String,
}

type Outcome = Result<HttpResponse, String>;

struct Job {
    request: HttpRequest,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::Sender<Outcome>,
}

struct Pool {
    queue: VecDeque<Job>,
    workers: usize,
    idle: usize,
}

static POOL: Mutex<Pool> = Mutex::new(Pool {
    queue: VecDeque::new(),
    workers: 0,
    idle: 0,
});
static WORK: Condvar = Condvar::new();

/// A request in flight. Dropping it cancels the request.
pub struct HttpTask {
    cancelled: Arc<AtomicBool>,
    reply: mpsc::Receiver<Outcome>,
    finished: bool,
}

impl HttpTask {
    /// Queues the request and returns at once.
    pub fn start(request: HttpRequest) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (reply, receiver) = mpsc::channel();
        let job = Job {
            request,
            cancelled: Arc::clone(&cancelled),
            reply,
        };
        let spawn = {
            let mut pool = POOL.lock().unwrap_or_else(|error| error.into_inner());
            pool.queue.push_back(job);
            // Only grow when nobody is free to take it; a pool that is
            // already waiting for work is told below instead.
            let spawn = pool.idle < pool.queue.len() && pool.workers < MAX_WORKERS;
            if spawn {
                pool.workers += 1;
            }
            spawn
        };
        if spawn
            && thread::Builder::new()
                .name("morf-http".into())
                .spawn(worker)
                .is_err()
        {
            POOL.lock()
                .unwrap_or_else(|error| error.into_inner())
                .workers -= 1;
        }
        WORK.notify_one();
        Self {
            cancelled,
            reply: receiver,
            finished: false,
        }
    }

    /// The answer, once there is one; `None` while the request is running.
    ///
    /// Returns the answer once. A worker that died without answering reads as
    /// an error rather than as a request that never finishes.
    pub fn poll(&mut self) -> Option<Outcome> {
        self.wait(Duration::ZERO)
    }

    /// Like [`HttpTask::poll`], but waits up to `timeout` for the answer.
    pub fn wait(&mut self, timeout: Duration) -> Option<Outcome> {
        if self.finished {
            return None;
        }
        let outcome = match self.reply.recv_timeout(timeout) {
            Ok(outcome) => outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => return None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Also the cancelled case, when the worker had not started:
                // it drops the job, and with it the sender, unanswered.
                if self.cancelled.load(Ordering::Relaxed) {
                    Err("cancelled".to_owned())
                } else {
                    Err("request failed without an answer".to_owned())
                }
            }
        };
        self.finished = true;
        Some(outcome)
    }

    /// Stops the request: a queued one never starts, a running one stops
    /// reading at its next chunk. Its answer, if one still comes, says
    /// `cancelled`. A request still waiting on the server for its headers
    /// holds its worker until the deadline, which is why that is bounded.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for HttpTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn worker() {
    loop {
        let job = {
            let mut pool = POOL.lock().unwrap_or_else(|error| error.into_inner());
            loop {
                if let Some(job) = pool.queue.pop_front() {
                    break job;
                }
                pool.idle += 1;
                let (next, timeout) = WORK
                    .wait_timeout(pool, WORKER_IDLE)
                    .unwrap_or_else(|error| error.into_inner());
                pool = next;
                pool.idle -= 1;
                if timeout.timed_out() && pool.queue.is_empty() {
                    pool.workers -= 1;
                    return;
                }
            }
        };
        if job.cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let outcome = perform(&job.request, &job.cancelled);
        // A receiver that is gone belongs to a runtime that was reloaded or
        // torn down; the answer has nobody to go to and is dropped here.
        if job.reply.send(outcome).is_ok() {
            crate::wake_all();
        }
    }
}

/// The shared agent: one connection pool for every request, so a config that
/// polls the same API every minute reuses its connection.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(MAX_REDIRECTS)
            .user_agent(concat!("morf/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .new_agent()
    })
}

fn perform(request: &HttpRequest, cancelled: &AtomicBool) -> Outcome {
    let timeout = request.timeout.clamp(Duration::from_millis(1), MAX_TIMEOUT);
    let max_bytes = request.max_bytes.min(MAX_BODY_LIMIT);
    let deadline = Instant::now() + timeout;
    let method = ureq::http::Method::from_bytes(request.method.to_ascii_uppercase().as_bytes())
        .map_err(|_| format!("invalid method {:?}", request.method))?;
    let uri: ureq::http::Uri = request
        .url
        .parse()
        .map_err(|error| format!("invalid url: {error}"))?;
    match uri.scheme_str() {
        Some("http") | Some("https") => {}
        _ => return Err("url must start with http:// or https://".to_owned()),
    }
    if uri.host().is_none() {
        return Err("url has no host".to_owned());
    }
    let mut builder = ureq::http::Request::builder().method(method).uri(uri);
    for (name, value) in &request.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let body = request.body.clone().unwrap_or_default();
    let http_request = builder
        .body(body)
        .map_err(|error| format!("invalid request: {error}"))?;
    let agent = agent();
    let http_request = agent
        .configure_request(http_request)
        .timeout_global(Some(timeout))
        .build();
    let mut response = agent.run(http_request).map_err(describe)?;
    let url = {
        use ureq::ResponseExt;
        response.get_uri().to_string()
    };
    let status = response.status().as_u16();
    let mut headers: Vec<(String, String)> = Vec::new();
    for (name, value) in response.headers() {
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        match headers.iter_mut().find(|(known, _)| known == name.as_str()) {
            Some((_, joined)) => {
                joined.push_str(", ");
                joined.push_str(&value);
            }
            None => headers.push((name.as_str().to_owned(), value)),
        }
    }
    // Refused before a byte is read when the server says up front it is too
    // big; otherwise counted as it arrives, after decompression, so neither a
    // lying length nor a small gzip of a huge body gets past the limit.
    if let Some(length) = response.body().content_length()
        && length > max_bytes as u64
        && !response.headers().contains_key("content-encoding")
    {
        return Err(format!("body exceeds {max_bytes} bytes"));
    }
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut body = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled".to_owned());
        }
        if Instant::now() >= deadline {
            return Err("timed out".to_owned());
        }
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => return Err(describe_io(error)),
        };
        if body.len() + read > max_bytes {
            return Err(format!("body exceeds {max_bytes} bytes"));
        }
        body.extend_from_slice(&chunk[..read]);
    }
    Ok(HttpResponse {
        status,
        headers,
        body,
        url,
    })
}

/// A failure, said the way a configuration would log it.
fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(_) => "timed out".to_owned(),
        ureq::Error::TooManyRedirects => "too many redirects".to_owned(),
        ureq::Error::Io(error) => describe_io(error),
        error => error.to_string(),
    }
}

fn describe_io(error: std::io::Error) -> String {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        return "timed out".to_owned();
    }
    // ureq wraps its own errors in io::Error when they surface through a
    // body reader; unwrap them so a timeout reads as one.
    match error.downcast::<ureq::Error>() {
        Ok(error) => describe(error),
        Err(error) => error.to_string(),
    }
}

/// Percent-encodes everything but the RFC 3986 unreserved characters, which
/// is right for both a query value and a path segment.
pub fn url_encode(text: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Decodes `%XX` escapes, and `+` as a space as forms write it.
pub fn url_decode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let byte = text[index];
        if byte == b'%'
            && let Some(value) = text
                .get(index + 1..index + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            index += 3;
            continue;
        }
        out.push(if byte == b'+' { b' ' } else { byte });
        index += 1;
    }
    out
}
