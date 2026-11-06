//! Unified error hierarchy and result alias for the `mq` launcher workspace.

/// Unified error enum shared across all `mq` crates.
#[derive(Debug, thiserror::Error)]
pub enum MqError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("network error: {0}")]
    Network(String),

    #[error("HTTP {status} for {url}: {body}")]
    HttpStatus {
        status: u16,
        url: String,
        body: String,
    },

    #[error("{algo} hash mismatch for {target}: expected {expected}, got {actual}")]
    HashMismatch {
        target: String,
        algo: &'static str,
        expected: String,
        actual: String,
    },

    #[error("unsafe archive or relative path rejected (zip-slip): {entry}")]
    ZipSlip { entry: String },

    #[error("archive error: {0}")]
    Archive(String),

    #[error("invalid username: {0}")]
    InvalidUsername(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("missing API key for {service} (set ${env_var} or configure it in config.toml)")]
    MissingApiKey {
        service: &'static str,
        env_var: &'static str,
    },

    #[error("missing OAuth client ID (set ${env_var} or configure it in config.toml)")]
    MissingClientId { env_var: &'static str },

    #[error("operation cancelled (epoch {epoch})")]
    Cancelled { epoch: u64 },

    #[error("metadata error: {0}")]
    Metadata(String),

    #[error("dependency resolution error: {0}")]
    DependencyResolution(String),

    #[error("instance not found: {0}")]
    InstanceNotFound(String),

    #[error("instance already exists: {0}")]
    InstanceAlreadyExists(String),

    #[error("Java {required_major}+ runtime not found ({hint})")]
    JavaNotFound { required_major: u32, hint: String },

    #[error("no graphical display detected (neither WAYLAND_DISPLAY nor DISPLAY is set)")]
    NoDisplay,

    #[error("process error: {0}")]
    Process(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl MqError {
    /// Returns `true` if this error represents a cooperative cancellation.
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled { .. })
    }

    /// Returns `true` if the error represents a transient network or HTTP condition
    /// eligible for bounded retry with backoff.
    #[inline]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) => true,
            Self::HttpStatus { status, .. } => {
                *status == 408 || *status == 429 || (500..600).contains(status)
            }
            _ => false,
        }
    }
}

/// Standard `Result` alias using [`MqError`].
pub type MqResult<T> = Result<T, MqError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_variants_format_and_classify_cleanly() {
        let cancel = MqError::Cancelled { epoch: 7 };
        assert!(cancel.is_cancelled());
        assert!(!cancel.is_retryable());
        assert!(cancel.to_string().contains("epoch 7"));

        let transient = MqError::HttpStatus {
            status: 503,
            url: "https://example.invalid/file.jar".into(),
            body: "service unavailable".into(),
        };
        assert!(transient.is_retryable());
        assert!(!transient.is_cancelled());

        let not_found = MqError::HttpStatus {
            status: 404,
            url: "https://example.invalid/missing.jar".into(),
            body: "not found".into(),
        };
        assert!(!not_found.is_retryable());

        let rate_limited = MqError::HttpStatus {
            status: 429,
            url: "https://api.modrinth.com/v2/project/sodium".into(),
            body: "too many requests".into(),
        };
        assert!(rate_limited.is_retryable());

        let mismatch = MqError::HashMismatch {
            target: "client.jar".into(),
            algo: "SHA-1",
            expected: "aaaa".into(),
            actual: "bbbb".into(),
        };
        assert!(mismatch.to_string().contains("SHA-1"));
        assert!(mismatch.to_string().contains("client.jar"));
    }
}
