//! Fixed-size cryptographic digest types, mod-loader identifiers, and strong IDs.

use std::borrow::Borrow;
use std::fmt;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::{MqError, MqResult};

/// Fixed-size 20-byte SHA-1 digest used for CAS addressing and Mojang/Modrinth verification.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha1Digest([u8; 20]);

/// Alias matching architecture documentation terminology.
pub type Sha1Hash = Sha1Digest;

impl Sha1Digest {
    /// Byte length of a binary SHA-1 digest.
    pub const BYTE_LEN: usize = 20;
    /// Character length of a hex-encoded SHA-1 digest.
    pub const HEX_LEN: usize = 40;

    /// Constructs a [`Sha1Digest`] from a raw 20-byte array.
    #[inline]
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Borrows the underlying 20-byte array.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Parses a 40-character hexadecimal string (case-insensitive) into a [`Sha1Digest`].
    pub fn from_hex(hex_str: &str) -> MqResult<Self> {
        let mut bytes = [0u8; 20];
        hex::decode_to_slice(hex_str, &mut bytes).map_err(|err| {
            MqError::Metadata(format!("invalid SHA-1 hex digest '{hex_str}': {err}"))
        })?;
        Ok(Self(bytes))
    }

    /// Formats the digest as a 40-character lowercase hexadecimal string.
    #[inline]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Returns the 2-character lowercase hex shard prefix (`00`..`ff`) for CAS/assets layout.
    #[inline]
    pub fn shard_prefix(&self) -> String {
        hex::encode(&self.0[..1])
    }
}

impl From<[u8; 20]> for Sha1Digest {
    #[inline]
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl From<Sha1Digest> for [u8; 20] {
    #[inline]
    fn from(digest: Sha1Digest) -> Self {
        digest.0
    }
}

impl AsRef<[u8]> for Sha1Digest {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Sha1Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; 40];
        hex::encode_to_slice(self.0, &mut buf).expect("40-byte stack buffer fits 20-byte digest");
        let s = std::str::from_utf8(&buf).expect("hex output is valid ASCII");
        f.write_str(s)
    }
}

impl fmt::Debug for Sha1Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha1Digest({self})")
    }
}

impl FromStr for Sha1Digest {
    type Err = MqError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl Serialize for Sha1Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buf = [0u8; 40];
        hex::encode_to_slice(self.0, &mut buf).expect("40-byte stack buffer fits 20-byte digest");
        let s = std::str::from_utf8(&buf).expect("hex output is valid ASCII");
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for Sha1Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Sha1Visitor;

        impl de::Visitor<'_> for Sha1Visitor {
            type Value = Sha1Digest;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a 40-character hexadecimal SHA-1 digest")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Sha1Digest::from_hex(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Sha1Visitor)
    }
}

/// Fixed-size 64-byte SHA-512 digest used for Modrinth `.mrpack` and mod verification.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha512Digest([u8; 64]);

/// Alias matching architecture documentation terminology.
pub type Sha512Hash = Sha512Digest;

impl Sha512Digest {
    /// Byte length of a binary SHA-512 digest.
    pub const BYTE_LEN: usize = 64;
    /// Character length of a hex-encoded SHA-512 digest.
    pub const HEX_LEN: usize = 128;

    /// Constructs a [`Sha512Digest`] from a raw 64-byte array.
    #[inline]
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Borrows the underlying 64-byte array.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Parses a 128-character hexadecimal string (case-insensitive) into a [`Sha512Digest`].
    pub fn from_hex(hex_str: &str) -> MqResult<Self> {
        let mut bytes = [0u8; 64];
        hex::decode_to_slice(hex_str, &mut bytes).map_err(|err| {
            MqError::Metadata(format!("invalid SHA-512 hex digest '{hex_str}': {err}"))
        })?;
        Ok(Self(bytes))
    }

    /// Formats the digest as a 128-character lowercase hexadecimal string.
    #[inline]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl From<[u8; 64]> for Sha512Digest {
    #[inline]
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

impl From<Sha512Digest> for [u8; 64] {
    #[inline]
    fn from(digest: Sha512Digest) -> Self {
        digest.0
    }
}

impl AsRef<[u8]> for Sha512Digest {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Sha512Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; 128];
        hex::encode_to_slice(self.0, &mut buf).expect("128-byte stack buffer fits 64-byte digest");
        let s = std::str::from_utf8(&buf).expect("hex output is valid ASCII");
        f.write_str(s)
    }
}

impl fmt::Debug for Sha512Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha512Digest({self})")
    }
}

impl FromStr for Sha512Digest {
    type Err = MqError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl Serialize for Sha512Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buf = [0u8; 128];
        hex::encode_to_slice(self.0, &mut buf).expect("128-byte stack buffer fits 64-byte digest");
        let s = std::str::from_utf8(&buf).expect("hex output is valid ASCII");
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for Sha512Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Sha512Visitor;

        impl de::Visitor<'_> for Sha512Visitor {
            type Value = Sha512Digest;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a 128-character hexadecimal SHA-512 digest")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Sha512Digest::from_hex(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Sha512Visitor)
    }
}

/// Supported Minecraft mod loader families.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModLoaderKind {
    #[default]
    #[serde(alias = "Vanilla", alias = "VANILLA", alias = "minecraft")]
    Vanilla,
    #[serde(alias = "Fabric", alias = "FABRIC", alias = "fabric-loader")]
    Fabric,
    #[serde(alias = "Quilt", alias = "QUILT", alias = "quilt-loader")]
    Quilt,
    #[serde(alias = "Forge", alias = "FORGE")]
    Forge,
    #[serde(
        alias = "NeoForge",
        alias = "NEOFORGE",
        alias = "neo-forge",
        alias = "neo_forge"
    )]
    NeoForge,
}

impl ModLoaderKind {
    /// Returns the canonical lowercase identifier string for this mod loader.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }
}

impl fmt::Display for ModLoaderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ModLoaderKind {
    type Err = MqError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vanilla" | "minecraft" | "none" => Ok(Self::Vanilla),
            "fabric" | "fabric-loader" => Ok(Self::Fabric),
            "quilt" | "quilt-loader" => Ok(Self::Quilt),
            "forge" => Ok(Self::Forge),
            "neoforge" | "neo-forge" | "neo_forge" => Ok(Self::NeoForge),
            other => Err(MqError::Metadata(format!(
                "unsupported mod loader '{other}' (expected vanilla, fabric, quilt, forge, or neoforge)"
            ))),
        }
    }
}

/// Validated filesystem-safe slug identifying a game instance (`[a-z0-9._-]+`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(String);

impl InstanceId {
    /// Validates and constructs an [`InstanceId`], rejecting empty strings, traversal
    /// segments, or path separators.
    pub fn new(raw: impl Into<String>) -> MqResult<Self> {
        let s = raw.into();
        if !Self::is_valid_slug(&s) {
            return Err(MqError::Metadata(format!(
                "invalid instance identifier '{s}' (allowed characters: ASCII alphanumeric, '-', '_', '.')"
            )));
        }
        Ok(Self(s))
    }

    /// Converts an arbitrary display name into a deterministic, filesystem-safe slug.
    pub fn sanitize_slug(display_name: &str) -> Self {
        let mut slug = String::with_capacity(display_name.len());
        let mut prev_dash = false;
        for ch in display_name.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if matches!(ch, '-' | '_' | '.') && !slug.is_empty() {
                slug.push(ch);
                prev_dash = ch == '-';
            } else if !prev_dash && !slug.is_empty() {
                slug.push('-');
                prev_dash = true;
            }
        }
        while slug.ends_with(['-', '.']) {
            slug.pop();
        }
        if slug.is_empty() || slug == "." || slug == ".." {
            slug = "instance".to_owned();
        }
        Self(slug)
    }

    /// Returns `true` if `s` is a safe single-component instance directory slug.
    pub fn is_valid_slug(s: &str) -> bool {
        !s.is_empty()
            && s.len() <= 128
            && s != "."
            && s != ".."
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    }

    /// Borrows the underlying slug string.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for InstanceId {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for InstanceId {
    #[inline]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for InstanceId {
    type Err = MqError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Strongly typed UUID wrapper identifying a stored player account profile.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(Uuid);

impl AccountId {
    /// Generates a random v4 [`AccountId`].
    #[inline]
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wraps an existing [`Uuid`].
    #[inline]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner [`Uuid`].
    #[inline]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AccountId {
    #[inline]
    fn default() -> Self {
        Self::new_v4()
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for AccountId {
    type Err = MqError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(s)
            .map_err(|err| MqError::Metadata(format!("invalid account UUID '{s}': {err}")))?;
        Ok(Self(uuid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_digest_hex_and_serde_roundtrip() {
        let upper_hex = "A94A8FE5CCB19BA61C4C0873D391E987982FBBD3";
        let lower_hex = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";

        let d1 = Sha1Digest::from_hex(upper_hex).expect("parse uppercase sha1");
        let d2: Sha1Digest = lower_hex.parse().expect("parse lowercase sha1");
        assert_eq!(d1, d2);
        assert_eq!(d1.to_hex(), lower_hex);
        assert_eq!(d1.to_string(), lower_hex);
        assert_eq!(d1.shard_prefix(), "a9");

        let from_raw = Sha1Digest::from_bytes(*d1.as_bytes());
        assert_eq!(from_raw, d1);

        let json = serde_json::to_string(&d1).expect("serialize sha1");
        assert_eq!(json, format!("\"{lower_hex}\""));
        let decoded: Sha1Digest = serde_json::from_str(&json).expect("deserialize sha1");
        assert_eq!(decoded, d1);

        assert!(Sha1Digest::from_hex("deadbeef").is_err());
        assert!(Sha1Digest::from_hex("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    }

    #[test]
    fn sha512_digest_hex_and_serde_roundtrip() {
        let bytes = [0x5au8; 64];
        let expected_hex = "5a".repeat(64);
        let d = Sha512Digest::from_bytes(bytes);
        assert_eq!(d.to_hex(), expected_hex);
        assert_eq!(d.to_string(), expected_hex);

        let parsed = Sha512Digest::from_hex(&expected_hex.to_uppercase()).expect("parse sha512");
        assert_eq!(parsed, d);
        assert_eq!(parsed.as_bytes(), &bytes);

        let json = serde_json::to_string(&d).expect("serialize sha512");
        let decoded: Sha512Digest = serde_json::from_str(&json).expect("deserialize sha512");
        assert_eq!(decoded, d);

        assert!(Sha512Digest::from_hex("5a").is_err());
    }

    #[test]
    fn mod_loader_kind_parse_display_and_serde() {
        let cases = [
            ("vanilla", ModLoaderKind::Vanilla),
            ("Fabric", ModLoaderKind::Fabric),
            ("QUILT", ModLoaderKind::Quilt),
            ("forge", ModLoaderKind::Forge),
            ("NeoForge", ModLoaderKind::NeoForge),
            ("neo-forge", ModLoaderKind::NeoForge),
        ];

        for (raw, expected) in cases {
            let parsed: ModLoaderKind = raw.parse().expect("parse loader");
            assert_eq!(parsed, expected);
            let json = serde_json::to_string(&parsed).expect("serialize loader");
            assert_eq!(json, format!("\"{}\"", expected.as_str()));
            let roundtrip: ModLoaderKind = serde_json::from_str(&json).expect("deserialize loader");
            assert_eq!(roundtrip, expected);
        }

        assert!("liteloader".parse::<ModLoaderKind>().is_err());
    }

    #[test]
    fn instance_id_validation_and_slug_sanitization() {
        assert!(InstanceId::new("fabric-1.21.1").is_ok());
        assert!(InstanceId::new("../escape").is_err());
        assert!(InstanceId::new(".hidden").is_err());
        assert!(InstanceId::new("a/b").is_err());
        assert!(InstanceId::new("").is_err());

        let slug = InstanceId::sanitize_slug("  Fabulously Optimized 1.21!  ");
        assert_eq!(slug.as_str(), "fabulously-optimized-1.21");
    }
}
