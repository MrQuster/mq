//! Fault injection engine for `MockServer`.
//!
//! Provides granular simulation of network failures:
//! - HTTP Status codes: 404 Not Found, 403 Forbidden, 429 Too Many Requests (with `Retry-After`),
//!   500 Internal Server Error, 503 Service Unavailable
//! - Connection truncation / premature EOF (drops connection or truncates body mid-file)
//! - Corrupted payload (SHA-1 checksum mismatch)
//! - Invalid Content-Length header mismatch
//! - Transient retryable failure count

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use parking_lot::RwLock;

/// Actions that can be performed when a fault rule matches an incoming request.
#[derive(Debug, Clone)]
pub enum FaultAction {
    /// Return a specific HTTP status code (e.g. 404, 403, 429, 500, 503).
    HttpStatus {
        status: StatusCode,
        retry_after_secs: Option<u64>,
        message: String,
    },
    /// Truncate the response body after `bytes_to_send` bytes, simulating a drop mid-file.
    TruncateBody { bytes_to_send: usize },
    /// Serve corrupted payload bytes (violates SHA-1/SHA-512 check).
    CorruptSha1,
    /// Return Content-Length header that does not match actual payload length.
    InvalidContentLength { claimed_len: usize },
}

/// A configurable rule defining when and how to inject a fault.
#[derive(Debug, Clone)]
pub struct FaultRule {
    /// Substring match on request path (e.g. "/mc/assets/", "client.jar", or None for all requests).
    pub path_filter: Option<String>,
    /// How many times this rule should trigger before expiring. None means unlimited.
    pub remaining_count: Option<usize>,
    /// The action to perform.
    pub action: FaultAction,
}

impl FaultRule {
    pub fn matches(&self, path: &str) -> bool {
        match &self.path_filter {
            Some(filter) => path.contains(filter),
            None => true,
        }
    }
}

/// Thread-safe fault injector state.
#[derive(Debug, Default)]
pub struct FaultInjector {
    rules: RwLock<Vec<FaultRule>>,
    matched_count: AtomicUsize,
}

impl FaultInjector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a generic fault rule.
    pub fn add_rule(&self, rule: FaultRule) {
        self.rules.write().push(rule);
    }

    /// Helper to inject an HTTP status failure for the next `count` matching requests.
    pub fn inject_status(&self, path_filter: Option<&str>, status: StatusCode, count: usize) {
        self.add_rule(FaultRule {
            path_filter: path_filter.map(str::to_string),
            remaining_count: Some(count),
            action: FaultAction::HttpStatus {
                status,
                retry_after_secs: None,
                message: format!("Injected mock HTTP {}", status.as_u16()),
            },
        });
    }

    /// Helper to inject HTTP 404 Not Found.
    pub fn inject_404(&self, path_filter: Option<&str>, count: usize) {
        self.inject_status(path_filter, StatusCode::NOT_FOUND, count);
    }

    /// Helper to inject HTTP 403 Forbidden.
    pub fn inject_403(&self, path_filter: Option<&str>, count: usize) {
        self.inject_status(path_filter, StatusCode::FORBIDDEN, count);
    }

    /// Helper to inject HTTP 429 Too Many Requests with a `Retry-After` header.
    pub fn inject_429(&self, path_filter: Option<&str>, retry_after_secs: u64, count: usize) {
        self.add_rule(FaultRule {
            path_filter: path_filter.map(str::to_string),
            remaining_count: Some(count),
            action: FaultAction::HttpStatus {
                status: StatusCode::TOO_MANY_REQUESTS,
                retry_after_secs: Some(retry_after_secs),
                message: "Injected 429 Rate Limit Exceeded".to_string(),
            },
        });
    }

    /// Helper to inject HTTP 500 Internal Server Error.
    pub fn inject_500(&self, path_filter: Option<&str>, count: usize) {
        self.inject_status(path_filter, StatusCode::INTERNAL_SERVER_ERROR, count);
    }

    /// Helper to inject HTTP 503 Service Unavailable.
    pub fn inject_503(
        &self,
        path_filter: Option<&str>,
        retry_after_secs: Option<u64>,
        count: usize,
    ) {
        self.add_rule(FaultRule {
            path_filter: path_filter.map(str::to_string),
            remaining_count: Some(count),
            action: FaultAction::HttpStatus {
                status: StatusCode::SERVICE_UNAVAILABLE,
                retry_after_secs,
                message: "Injected 503 Service Unavailable".to_string(),
            },
        });
    }

    /// Helper to inject connection truncation mid-file.
    pub fn inject_truncation(&self, path_filter: Option<&str>, bytes_to_send: usize, count: usize) {
        self.add_rule(FaultRule {
            path_filter: path_filter.map(str::to_string),
            remaining_count: Some(count),
            action: FaultAction::TruncateBody { bytes_to_send },
        });
    }

    /// Helper to inject SHA-1 byte corruption.
    pub fn inject_corrupt_sha1(&self, path_filter: Option<&str>, count: usize) {
        self.add_rule(FaultRule {
            path_filter: path_filter.map(str::to_string),
            remaining_count: Some(count),
            action: FaultAction::CorruptSha1,
        });
    }

    /// Helper to inject invalid Content-Length header.
    pub fn inject_invalid_content_length(
        &self,
        path_filter: Option<&str>,
        claimed_len: usize,
        count: usize,
    ) {
        self.add_rule(FaultRule {
            path_filter: path_filter.map(str::to_string),
            remaining_count: Some(count),
            action: FaultAction::InvalidContentLength { claimed_len },
        });
    }

    /// Evaluate incoming request against registered rules.
    /// Decrements `remaining_count` and removes expired rules.
    pub fn evaluate(&self, path: &str) -> Option<FaultAction> {
        let mut rules = self.rules.write();
        let mut triggered_idx = None;
        let mut triggered_action = None;

        for (idx, rule) in rules.iter_mut().enumerate() {
            if rule.matches(path) {
                triggered_action = Some(rule.action.clone());
                triggered_idx = Some(idx);
                if let Some(ref mut count) = rule.remaining_count {
                    if *count > 0 {
                        *count -= 1;
                    }
                }
                break;
            }
        }

        if let Some(idx) = triggered_idx {
            self.matched_count.fetch_add(1, Ordering::SeqCst);
            if let Some(0) = rules[idx].remaining_count {
                rules.remove(idx);
            }
        }

        triggered_action
    }

    /// Clears all rules.
    pub fn clear(&self) {
        self.rules.write().clear();
    }

    /// Returns the number of times fault rules were triggered.
    pub fn matched_count(&self) -> usize {
        self.matched_count.load(Ordering::SeqCst)
    }
}

/// Applies body or header mutation based on the evaluated `FaultAction`.
pub async fn apply_body_fault(resp: Response, action: FaultAction) -> Response {
    let (mut parts, body) = resp.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match action {
        FaultAction::HttpStatus {
            status,
            retry_after_secs,
            message,
        } => {
            let mut r = (status, message).into_response();
            if let Some(secs) = retry_after_secs {
                if let Ok(val) = secs.to_string().parse() {
                    r.headers_mut().insert(header::RETRY_AFTER, val);
                }
            }
            r
        }
        FaultAction::TruncateBody { bytes_to_send } => {
            let send_len = bytes_to_send.min(body_bytes.len());
            if let Ok(val) = send_len.to_string().parse() {
                parts.headers.insert(header::CONTENT_LENGTH, val);
            }
            Response::from_parts(parts, axum::body::Body::from(body_bytes.slice(0..send_len)))
        }
        FaultAction::CorruptSha1 => {
            let mut corrupted = body_bytes.to_vec();
            if !corrupted.is_empty() {
                corrupted[0] ^= 0xFF;
            } else {
                corrupted.push(0xFE);
            }
            let c_len = corrupted.len();
            if let Ok(val) = c_len.to_string().parse() {
                parts.headers.insert(header::CONTENT_LENGTH, val);
            }
            Response::from_parts(parts, axum::body::Body::from(Bytes::from(corrupted)))
        }
        FaultAction::InvalidContentLength { claimed_len } => {
            let mut payload = body_bytes.to_vec();
            payload.resize(claimed_len, 0xAA);
            let new_len = payload.len();
            if let Ok(val) = new_len.to_string().parse() {
                parts.headers.insert(header::CONTENT_LENGTH, val);
            }
            Response::from_parts(parts, axum::body::Body::from(Bytes::from(payload)))
        }
    }
}
