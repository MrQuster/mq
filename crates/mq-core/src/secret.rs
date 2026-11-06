//! Redacted secret wrapper and log/text scrubbing utilities.

use std::fmt;
use std::sync::atomic::{compiler_fence, Ordering};

use serde::{Deserialize, Serialize};

/// Redaction placeholder emitted in `Debug`, `Display`, and scrubbed log output.
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

/// Minimum character count for a secret to be scrubbed by [`redact_text`].
const MIN_REDACT_CHARS: usize = 4;

/// A string wrapper for sensitive values (passwords, session tokens, API keys)
/// that always formats as `"[REDACTED]"` in `Debug` and `Display` and zeroes
/// its backing UTF-8 buffer on drop.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    /// Wraps a sensitive string value.
    #[inline]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicitly exposes the underlying secret slice for authorized consumers
    /// (such as HTTP `Authorization` headers or JVM launch arguments).
    #[inline]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// Returns `true` if the secret contains zero bytes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the byte length of the underlying secret.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<String> for SecretString {
    #[inline]
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretString {
    #[inline]
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_PLACEHOLDER)
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_PLACEHOLDER)
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        // SAFETY: Overwriting UTF-8 bytes with 0x00 (ASCII NUL) preserves valid UTF-8
        // invariants. Volatile byte writes followed by a compiler fence prevent the
        // optimizer from eliminating the zeroization before freeing heap memory.
        unsafe {
            for byte in self.0.as_bytes_mut() {
                std::ptr::write_volatile(byte, 0);
            }
        }
        compiler_fence(Ordering::SeqCst);
        self.0.clear();
    }
}

#[derive(Clone, Copy)]
enum Segment<'a> {
    Raw(&'a str),
    Redacted,
}

/// Scrubs any secret substring of length `>= 4` characters from `input`,
/// replacing matches with `"[REDACTED]"`.
///
/// Secrets are matched longest-first so shared prefixes cannot leak trailing
/// characters, and already-redacted spans are never re-matched.
pub fn redact_text(input: &str, secrets: &[&str]) -> String {
    if input.is_empty() || secrets.is_empty() {
        return input.to_owned();
    }

    let mut eligible: Vec<&str> = secrets
        .iter()
        .copied()
        .filter(|s| s.chars().count() >= MIN_REDACT_CHARS)
        .collect();

    if eligible.is_empty() {
        return input.to_owned();
    }

    // Longest secrets first so prefix secrets do not partially consume longer ones.
    eligible.sort_unstable_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    eligible.dedup();

    let mut segments = vec![Segment::Raw(input)];
    let mut next_segments = Vec::new();

    for pat in eligible {
        next_segments.clear();
        for seg in &segments {
            match *seg {
                Segment::Redacted => next_segments.push(Segment::Redacted),
                Segment::Raw(chunk) => {
                    if !chunk.contains(pat) {
                        next_segments.push(Segment::Raw(chunk));
                    } else {
                        for (idx, part) in chunk.split(pat).enumerate() {
                            if idx > 0 {
                                next_segments.push(Segment::Redacted);
                            }
                            if !part.is_empty() {
                                next_segments.push(Segment::Raw(part));
                            }
                        }
                    }
                }
            }
        }
        std::mem::swap(&mut segments, &mut next_segments);
    }

    let mut out = String::with_capacity(input.len());
    for seg in segments {
        match seg {
            Segment::Raw(s) => out.push_str(s),
            Segment::Redacted => out.push_str(REDACTED_PLACEHOLDER),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_string_never_leaks_in_debug_or_display() {
        let raw = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.super_secret_token";
        let secret = SecretString::new(raw);

        assert_eq!(secret.expose_secret(), raw);
        assert!(!secret.is_empty());
        assert_eq!(secret.len(), raw.len());

        let dbg = format!("{secret:?}");
        let pretty_dbg = format!("{secret:#?}");
        let disp = format!("{secret}");

        assert_eq!(dbg, "[REDACTED]");
        assert_eq!(pretty_dbg, "[REDACTED]");
        assert_eq!(disp, "[REDACTED]");
        assert!(!dbg.contains("super_secret"));
        assert!(!disp.contains("eyJ"));
    }

    #[test]
    fn secret_string_serde_roundtrip_and_drop() {
        let secret = SecretString::from("cf_api_key_998877");
        let json = serde_json::to_string(&secret).expect("serialize secret");
        assert_eq!(json, "\"cf_api_key_998877\"");

        let restored: SecretString = serde_json::from_str(&json).expect("deserialize secret");
        assert_eq!(restored, secret);
        assert_eq!(restored.expose_secret(), "cf_api_key_998877");
        drop(restored);
    }

    #[test]
    fn redact_text_scrubs_secrets_longest_first_and_ignores_short_tokens() {
        let line = "--accessToken abcd123456 --uuid 123 --user abc --other abcd";
        let scrubbed = redact_text(line, &["", "abc", "123", "abcd", "abcd123456"]);
        assert_eq!(
            scrubbed,
            "--accessToken [REDACTED] --uuid 123 --user abc --other [REDACTED]"
        );
    }

    #[test]
    fn redact_text_does_not_corrupt_placeholder_when_secret_matches_redacted() {
        let line = "Bearer very_sensitive_session_token_xyz";
        let scrubbed = redact_text(line, &["very_sensitive_session_token_xyz", "REDACTED"]);
        assert_eq!(scrubbed, "Bearer [REDACTED]");
    }
}
