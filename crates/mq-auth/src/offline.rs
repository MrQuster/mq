//! Offline authentication provider and vanilla-compatible UUID v3 generation.
//!
//! Computes the exact UUID produced by vanilla Minecraft's offline mode:
//! `java.util.UUID.nameUUIDFromBytes(("OfflinePlayer:" + username).getBytes(StandardCharsets.UTF_8))`.

use md5::{Digest, Md5};
use mq_core::{AccountId, MqError, MqResult, SecretString};
use uuid::Uuid;

use crate::models::{AccountKind, AccountProfile};
use crate::skin::generate_offline_geometric_avatar;

/// Prefix prepended to the player name before hashing in vanilla Minecraft offline mode.
pub const OFFLINE_UUID_PREFIX: &str = "OfflinePlayer:";

/// Minimum allowed length for a valid Minecraft offline username.
pub const MIN_USERNAME_LEN: usize = 3;

/// Maximum allowed length for a valid Minecraft offline username.
pub const MAX_USERNAME_LEN: usize = 16;

/// Validates a Minecraft offline username against `^[a-zA-Z0-9_]{3,16}$`.
///
/// Returns the borrowed username slice if valid, or [`MqError::InvalidUsername`] otherwise.
pub fn validate_offline_username(username: &str) -> MqResult<&str> {
    let len = username.len();
    if !(MIN_USERNAME_LEN..=MAX_USERNAME_LEN).contains(&len) {
        return Err(MqError::InvalidUsername(format!(
            "username '{username}' must be between {MIN_USERNAME_LEN} and {MAX_USERNAME_LEN} characters long (got {len})"
        )));
    }

    if !username
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(MqError::InvalidUsername(format!(
            "username '{username}' contains invalid characters (only ASCII alphanumeric and '_' are allowed)"
        )));
    }

    Ok(username)
}

/// Computes the deterministic vanilla Minecraft offline UUID v3 for `username`.
///
/// Matches `java.util.UUID.nameUUIDFromBytes(("OfflinePlayer:" + username).getBytes(StandardCharsets.UTF_8))`:
/// 1. Computes the 16-byte MD5 digest of `"OfflinePlayer:" + username`.
/// 2. Sets the RFC 4122 version 3 bits: `bytes[6] = (bytes[6] & 0x0f) | 0x30`.
/// 3. Sets the RFC 4122 IETF variant bits: `bytes[8] = (bytes[8] & 0x3f) | 0x80`.
pub fn compute_offline_uuid(username: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(OFFLINE_UUID_PREFIX.as_bytes());
    hasher.update(username.as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();

    // Set version to 3 (name-based MD5) per RFC 4122 section 4.3.
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    // Set variant to IETF RFC 4122 (10xx_xxxx).
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes)
}

/// Creates a complete [`AccountProfile`] for an offline player account.
///
/// Validates `username`, computes the deterministic offline UUID v3, and generates
/// a deterministic 8x8 geometric pixel avatar for the sidebar UI.
pub fn create_offline_account(username: &str) -> MqResult<AccountProfile> {
    let valid_username = validate_offline_username(username)?;
    let uuid = compute_offline_uuid(valid_username);
    // Derive a deterministic account ID from the lowercase username so case variations
    // map to the same canonical offline account slot.
    let canonical_id_uuid =
        compute_offline_uuid(&format!("offline-{}", valid_username.to_ascii_lowercase()));
    let avatar = generate_offline_geometric_avatar(valid_username);

    Ok(AccountProfile {
        id: AccountId::from_uuid(canonical_id_uuid),
        kind: AccountKind::Offline,
        username: valid_username.to_owned(),
        uuid,
        access_token: SecretString::new("0"),
        refresh_token: None,
        client_token: None,
        expires_at: None,
        skin_url: None,
        avatar_head: Some(avatar),
    })
}

#[cfg(test)]
mod tests {
    use uuid::Version;

    use super::*;
    use crate::models::SkinHeadAvatar;

    #[test]
    fn offline_uuid_matches_java_name_uuid_from_bytes_vectors() {
        // Exact vector from Java:
        // UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(StandardCharsets.UTF_8))
        let notch = compute_offline_uuid("Notch");
        assert_eq!(
            notch.hyphenated().to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(
            notch.simple().to_string(),
            "b50ad385829d3141a2167e7d7539ba7f"
        );
        assert_eq!(notch.get_version(), Some(Version::Md5));
        assert_eq!(notch.get_version_num(), 3);

        // Verify RFC 4122 version (0x30) and variant (0x80) bitmasks on "Mr_Quster" and "Steve".
        let quster = compute_offline_uuid("Mr_Quster");
        let quster_bytes = quster.as_bytes();
        assert_eq!(quster_bytes[6] & 0xf0, 0x30, "version must be 3");
        assert_eq!(quster_bytes[8] & 0xc0, 0x80, "variant must be RFC 4122");
        assert_eq!(quster, compute_offline_uuid("Mr_Quster"));

        // Verify exact match against raw MD5 with manual bit masking for "Mr_Quster".
        let mut raw_md5: [u8; 16] = Md5::digest(b"OfflinePlayer:Mr_Quster").into();
        raw_md5[6] = (raw_md5[6] & 0x0f) | 0x30;
        raw_md5[8] = (raw_md5[8] & 0x3f) | 0x80;
        assert_eq!(quster.as_bytes(), &raw_md5);

        // Case sensitivity: vanilla OfflinePlayer:<name> is case-sensitive.
        let notch_lower = compute_offline_uuid("notch");
        assert_ne!(notch, notch_lower);
    }

    #[test]
    fn username_validation_enforces_length_and_charset_boundaries() {
        // Valid boundaries (3 chars, 16 chars, alphanumeric + underscore).
        assert_eq!(validate_offline_username("abc").unwrap(), "abc");
        assert_eq!(validate_offline_username("Notch").unwrap(), "Notch");
        assert_eq!(validate_offline_username("Mr_Quster").unwrap(), "Mr_Quster");
        assert_eq!(
            validate_offline_username("Player_123456789").unwrap(),
            "Player_123456789"
        );
        assert_eq!(validate_offline_username("___").unwrap(), "___");

        // Invalid lengths (< 3 or > 16).
        for invalid_len in ["", "a", "ab", "12345678901234567", "abcdefghijklmnopqrst"] {
            let err = validate_offline_username(invalid_len).unwrap_err();
            assert!(
                matches!(err, MqError::InvalidUsername(_)),
                "expected InvalidUsername for {invalid_len:?}, got {err:?}"
            );
        }

        // Invalid characters (spaces, hyphens, dots, non-ASCII).
        for invalid_chars in [
            "Mr-Quster",
            "Mr.Quster",
            "Mr Quster",
            "Notch!",
            "Игрок",
            "user@ely",
            "abc\ndef",
        ] {
            let err = validate_offline_username(invalid_chars).unwrap_err();
            assert!(
                matches!(err, MqError::InvalidUsername(_)),
                "expected InvalidUsername for {invalid_chars:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn create_offline_account_populates_profile_and_geometric_avatar() {
        let profile = create_offline_account("Notch").expect("create Notch offline account");
        assert_eq!(profile.kind, AccountKind::Offline);
        assert_eq!(profile.username, "Notch");
        assert_eq!(
            profile.uuid_hyphenated(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(profile.uuid_simple(), "b50ad385829d3141a2167e7d7539ba7f");
        assert_eq!(profile.access_token.expose_secret(), "0");
        assert!(profile.refresh_token.is_none());
        assert!(profile.client_token.is_none());
        assert!(profile.expires_at.is_none());
        assert!(profile.skin_url.is_none());

        let avatar = profile.avatar_head.expect("offline avatar must be present");
        assert_eq!(avatar.width, SkinHeadAvatar::SIZE);
        assert_eq!(avatar.height, SkinHeadAvatar::SIZE);
        assert_eq!(avatar.rgba.len(), SkinHeadAvatar::BYTE_LEN);

        // Case-insensitive account slot ID stability.
        let lower_profile = create_offline_account("notch").expect("create lowercase account");
        assert_eq!(profile.id, lower_profile.id);
    }
}
