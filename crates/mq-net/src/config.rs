#![deny(clippy::await_holding_lock)]
//! Configuration types, download specifications, outcomes, and deterministic jitter backoff
//! calculations for the `mq-net` streaming HTTP engine.

use std::path::PathBuf;
use std::time::Duration;

use mq_core::{Sha1Digest, Sha512Digest};

/// Canonical `User-Agent` header sent on all launcher HTTP requests.
pub const USER_AGENT: &str = "mq-launcher/0.1.0 (archlinux; rust)";

/// Default maximum concurrent HTTP connections (`Semaphore` bound).
pub const DEFAULT_MAX_CONNECTIONS: usize = 32;

/// Default maximum retry attempts for transient network or HTTP 408/429/5xx failures.
pub const DEFAULT_MAX_RETRIES: u32 = 4;

/// Default base exponential backoff delay in milliseconds.
pub const DEFAULT_BASE_BACKOFF_MS: u64 = 50;

/// Default TCP/TLS connection establishment timeout (`10s`).
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default per-chunk socket read timeout (`30s`).
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum response size for `fetch_json` (32 MiB).
pub const DEFAULT_MAX_JSON_BYTES: usize = 32 * 1024 * 1024;

/// Maximum backoff cap in milliseconds (10 seconds).
const MAX_BACKOFF_MS: u64 = 10_000;

/// Configuration parameters for [`crate::NetEngine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetConfig {
    /// Maximum concurrent HTTP connections enforced via `Arc<tokio::sync::Semaphore>`.
    pub max_connections: usize,
    /// Maximum number of retries on transient failures (`0` disables retries).
    pub max_retries: u32,
    /// Base delay in milliseconds for exponential backoff with jitter.
    pub base_backoff_ms: u64,
    /// Timeout for establishing an HTTP/TLS connection.
    pub connect_timeout: Duration,
    /// Timeout for individual socket read operations.
    pub read_timeout: Duration,
    /// `User-Agent` string sent with every request.
    pub user_agent: String,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_MAX_CONNECTIONS,
            max_retries: DEFAULT_MAX_RETRIES,
            base_backoff_ms: DEFAULT_BASE_BACKOFF_MS,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            user_agent: USER_AGENT.to_owned(),
        }
    }
}

impl NetConfig {
    /// Creates a new [`NetConfig`] with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum number of concurrent HTTP connections (clamped to `>= 1`).
    #[must_use]
    pub fn with_max_connections(mut self, max_connections: usize) -> Self {
        self.max_connections = max_connections.max(1);
        self
    }

    /// Sets the maximum number of retry attempts for transient errors.
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the base backoff duration in milliseconds.
    #[must_use]
    pub fn with_base_backoff_ms(mut self, base_backoff_ms: u64) -> Self {
        self.base_backoff_ms = base_backoff_ms;
        self
    }

    /// Sets the connection establishment timeout.
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the per-read socket timeout.
    #[must_use]
    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// Sets a custom `User-Agent` header value.
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }
}

/// Specification for downloading and verifying a single artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    /// Logical identifier (e.g., library Maven coordinate, asset hash, or mod filename).
    pub id: String,
    /// Ordered list of URLs to attempt (primary URL followed by optional fallback mirrors).
    pub urls: Vec<String>,
    /// Final destination path on disk once verified.
    pub dest_path: PathBuf,
    /// Optional expected SHA-1 digest verified incrementally during streaming.
    pub expected_sha1: Option<Sha1Digest>,
    /// Optional expected SHA-512 digest verified incrementally during streaming.
    pub expected_sha512: Option<Sha512Digest>,
    /// Optional expected size in bytes.
    pub expected_size: Option<u64>,
}

impl DownloadSpec {
    /// Creates a [`DownloadSpec`] with a single primary URL and no digest constraints.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        url: impl Into<String>,
        dest_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            urls: vec![url.into()],
            dest_path: dest_path.into(),
            expected_sha1: None,
            expected_sha512: None,
            expected_size: None,
        }
    }

    /// Creates a [`DownloadSpec`] with an explicit list of primary + fallback mirror URLs.
    #[must_use]
    pub fn with_urls(
        id: impl Into<String>,
        urls: Vec<String>,
        dest_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            urls,
            dest_path: dest_path.into(),
            expected_sha1: None,
            expected_sha512: None,
            expected_size: None,
        }
    }

    /// Appends a fallback mirror URL.
    #[must_use]
    pub fn with_fallback_url(mut self, url: impl Into<String>) -> Self {
        self.urls.push(url.into());
        self
    }

    /// Sets the expected SHA-1 digest.
    #[must_use]
    pub fn with_sha1(mut self, sha1: Sha1Digest) -> Self {
        self.expected_sha1 = Some(sha1);
        self
    }

    /// Sets the expected SHA-512 digest.
    #[must_use]
    pub fn with_sha512(mut self, sha512: Sha512Digest) -> Self {
        self.expected_sha512 = Some(sha512);
        self
    }

    /// Sets the expected file size in bytes.
    #[must_use]
    pub fn with_expected_size(mut self, size: u64) -> Self {
        self.expected_size = Some(size);
        self
    }

    /// Returns `true` if this download specification specifies additional fallback mirror URLs.
    #[must_use]
    pub fn has_mirrors(&self) -> bool {
        self.urls.len() > 1
    }

    /// Returns whether fallback mirrors can be safely used for this download.
    /// Under MQ integrity rules, fallback mirrors are ONLY permitted if an official expected SHA-1 digest
    /// is present to verify untrusted or third-party mirrors.
    #[must_use]
    pub fn allows_mirrors(&self) -> bool {
        self.expected_sha1.is_some()
    }
}

/// Verified outcome of a single completed download operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// Logical identifier matching [`DownloadSpec::id`].
    pub id: String,
    /// Final destination path where the verified file resides.
    pub dest_path: PathBuf,
    /// Total verified size of the destination file in bytes.
    pub bytes_written: u64,
    /// Computed and verified SHA-1 digest of the complete file.
    pub sha1: Sha1Digest,
    /// Computed and verified SHA-512 digest (present if requested or enabled).
    pub sha512: Option<Sha512Digest>,
    /// Byte offset from which HTTP `206 Partial Content` resumed (`0` if downloaded from start).
    pub resumed_from_offset: u64,
    /// `true` if this call was satisfied by waiting on an in-flight leader or existing verified file.
    pub deduplicated: bool,
}

/// Parses the HTTP `Retry-After` header value (either integer seconds or RFC 2822 HTTP-date).
///
/// Returns `Some(Duration)` if a valid non-negative delay could be parsed, or `None`.
#[must_use]
pub fn parse_retry_after(header_val: &str) -> Option<Duration> {
    let trimmed = header_val.trim();
    if let Ok(seconds) = trimmed.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(trimmed) {
        let now = chrono::Utc::now();
        let target = date.with_timezone(&chrono::Utc);
        if let Ok(diff) = (target - now).to_std() {
            return Some(diff);
        } else {
            return Some(Duration::ZERO);
        }
    }

    // Also support standard HTTP-date (RFC 7231 / RFC 2616 format: "Sun, 06 Nov 1994 08:49:37 GMT")
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%a, %d %b %Y %H:%M:%S GMT") {
        let target = naive.and_utc();
        let now = chrono::Utc::now();
        if let Ok(diff) = (target - now).to_std() {
            return Some(diff);
        } else {
            return Some(Duration::ZERO);
        }
    }

    None
}

/// Computes exponential backoff with fast deterministic FNV-1a jitter, incorporating
/// any optional server-provided `Retry-After` delay.
///
/// If `retry_after` is provided, the returned duration will be at least `retry_after`,
/// plus a small jitter (up to 20% or 500ms), capped at 60 seconds.
#[must_use]
pub fn compute_backoff_delay_with_retry_after(
    attempt: u32,
    base_backoff_ms: u64,
    seed: &str,
    retry_after: Option<Duration>,
) -> Duration {
    if let Some(ra) = retry_after {
        if !ra.is_zero() {
            let jitter_span = (ra.as_millis() as u64 / 5).clamp(10, 500);
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in seed.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash ^= u64::from(attempt).wrapping_mul(0x9e3779b97f4a7c15);
            let jitter_ms = hash % (jitter_span + 1);
            return ra
                .saturating_add(Duration::from_millis(jitter_ms))
                .min(Duration::from_secs(60));
        }
    }

    if base_backoff_ms == 0 {
        return Duration::ZERO;
    }
    let shift = attempt.min(16);
    let exp_ms = base_backoff_ms
        .saturating_mul(1u64 << shift)
        .min(MAX_BACKOFF_MS);

    let jitter_span = (base_backoff_ms / 2).max(1);
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= u64::from(attempt).wrapping_mul(0x9e3779b97f4a7c15);
    let jitter_ms = hash % (jitter_span + 1);

    Duration::from_millis(exp_ms.saturating_add(jitter_ms).min(MAX_BACKOFF_MS))
}

/// Computes exponential backoff with fast deterministic FNV-1a jitter.
///
/// For `attempt` (0-indexed), base delay scales as `base_backoff_ms * 2^attempt` (capped at 10s),
/// plus deterministic jitter in `0..=max(1, base / 2)` derived from `seed` and `attempt`.
#[must_use]
pub fn compute_backoff_delay(attempt: u32, base_backoff_ms: u64, seed: &str) -> Duration {
    compute_backoff_delay_with_retry_after(attempt, base_backoff_ms, seed, None)
}
