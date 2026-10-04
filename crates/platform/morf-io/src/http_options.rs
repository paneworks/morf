//! A request shaped by a script's options, and the bookkeeping around it.
//!
//! A binding reads the caller's table and hands each option here, where it
//! is checked before anything goes on the wire: a method that is not a word
//! and more than [`MAX_HEADERS`] headers are refused, a deadline past
//! [`MAX_TIMEOUT`] or a body cap past [`MAX_BODY_LIMIT`] is clamped. The
//! handle a caller holds shares one [`HttpHandle`] with the queued entry, and
//! [`check_pending`] keeps the queue behind the worker pool from growing
//! without end.

use std::cell::Cell;
use std::time::Duration;

use crate::{HttpRequest, MAX_BODY_LIMIT, MAX_TIMEOUT};

/// Requests one configuration may have outstanding. The pool puts at most
/// sixteen on the wire at once; this caps the queue behind them, so a loop
/// that forgets to wait cannot pile up work without end.
pub const MAX_HTTP_PENDING: usize = 64;
/// Headers one request may carry.
pub const MAX_HEADERS: usize = 64;

/// Refuses one more request when `in_flight` already fill the queue.
pub fn check_pending(in_flight: usize) -> Result<(), String> {
    if in_flight >= MAX_HTTP_PENDING {
        return Err(format!("more than {MAX_HTTP_PENDING} HTTP requests in flight"));
    }
    Ok(())
}

/// What a handle and the pending entry share: the one bit of news either
/// side has for the other.
#[derive(Debug, Default)]
pub struct HttpHandle {
    pub done: Cell<bool>,
    pub cancelled: Cell<bool>,
}

impl HttpHandle {
    /// Cancels the request; whether it was still waiting for its answer.
    pub fn cancel(&self) -> bool {
        let pending = !self.done.get();
        self.cancelled.set(true);
        self.done.set(true);
        pending
    }
}

impl HttpRequest {
    /// Sets a header, replacing one of the same name (any case) that the
    /// call itself set, content-type after `json` in particular.
    pub fn set_header(&mut self, name: String, value: String) -> Result<(), String> {
        self.headers
            .retain(|(known, _)| !known.eq_ignore_ascii_case(&name));
        self.headers.push((name, value));
        if self.headers.len() > MAX_HEADERS {
            return Err(format!("more than {MAX_HEADERS} http headers"));
        }
        Ok(())
    }

    /// Sends `json` (already encoded) as the body, and says so.
    pub fn set_json_body(&mut self, json: Vec<u8>) {
        self.body = Some(json);
        self.headers.push((
            "Content-Type".into(),
            "application/json; charset=utf-8".into(),
        ));
    }

    /// The method as given (upper-cased), or POST when none is given and a
    /// body is.
    pub fn set_method(&mut self, method: Option<&str>) -> Result<(), String> {
        match method {
            None => {
                if self.body.is_some() && self.method == "GET" {
                    self.method = "POST".into();
                }
            }
            Some(method) => {
                let method = method.to_ascii_uppercase();
                if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_alphabetic()) {
                    return Err(format!("http method {method:?} is not valid"));
                }
                self.method = method;
            }
        }
        Ok(())
    }

    /// The whole exchange's deadline, at most [`MAX_TIMEOUT`].
    pub fn set_timeout_ms(&mut self, millis: u64) {
        self.timeout = Duration::from_millis(millis).min(MAX_TIMEOUT);
    }

    /// The largest body accepted, at most [`MAX_BODY_LIMIT`].
    pub fn set_max_bytes(&mut self, max_bytes: u64) {
        self.max_bytes = usize::try_from(max_bytes)
            .unwrap_or(MAX_BODY_LIMIT)
            .min(MAX_BODY_LIMIT);
    }
}

/// `a=1&b=x` from key and value pairs, sorted so the same pairs always make
/// the same URL — and the same cache key.
pub fn encode_query(mut pairs: Vec<(Vec<u8>, Vec<u8>)>) -> String {
    pairs.sort();
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", crate::url_encode(key), crate::url_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_replace_by_name_and_are_capped() {
        let mut request = HttpRequest::get("http://x");
        request.set_json_body(b"{}".to_vec());
        request
            .set_header("content-type".into(), "text/plain".into())
            .unwrap();
        assert_eq!(request.headers, vec![("content-type".into(), "text/plain".into())]);
        for index in 1..MAX_HEADERS {
            request.set_header(format!("h{index}"), "v".into()).unwrap();
        }
        assert!(request.set_header("one-more".into(), "v".into()).is_err());
    }

    #[test]
    fn method_defaults_to_post_with_a_body_and_must_be_a_word() {
        let mut request = HttpRequest::get("http://x");
        request.set_method(None).unwrap();
        assert_eq!(request.method, "GET");
        request.body = Some(b"x".to_vec());
        request.set_method(None).unwrap();
        assert_eq!(request.method, "POST");
        request.set_method(Some("put")).unwrap();
        assert_eq!(request.method, "PUT");
        assert!(request.set_method(Some("GE T")).is_err());
        assert!(request.set_method(Some("")).is_err());
    }

    #[test]
    fn limits_clamp() {
        let mut request = HttpRequest::get("http://x");
        request.set_timeout_ms(u64::MAX);
        assert_eq!(request.timeout, MAX_TIMEOUT);
        request.set_timeout_ms(500);
        assert_eq!(request.timeout, Duration::from_millis(500));
        request.set_max_bytes(u64::MAX);
        assert_eq!(request.max_bytes, MAX_BODY_LIMIT);
    }

    #[test]
    fn query_is_sorted_and_encoded() {
        let pairs = vec![
            (b"q".to_vec(), b"x y".to_vec()),
            (b"n".to_vec(), b"2".to_vec()),
        ];
        assert_eq!(encode_query(pairs), "n=2&q=x%20y");
    }

    #[test]
    fn handles_cancel_once_and_the_queue_is_capped() {
        let handle = HttpHandle::default();
        assert!(handle.cancel());
        assert!(!handle.cancel());
        assert!(handle.cancelled.get() && handle.done.get());
        assert!(check_pending(MAX_HTTP_PENDING - 1).is_ok());
        assert!(check_pending(MAX_HTTP_PENDING).is_err());
    }
}
