//! Shared authentication domain models for `mq-auth` (Offline, Ely.by, Microsoft, and Skin Avatar).

use chrono::{DateTime, Utc};
use mq_core::{AccountId, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported authentication provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Offline,
    ElyBy,
    Microsoft,
}

impl AccountKind {
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Offline => "OFFLINE",
            Self::ElyBy => "ELY.BY",
            Self::Microsoft => "MSA",
        }
    }

    pub fn user_type_arg(&self) -> &'static str {
        match self {
            Self::Offline => "legacy",
            Self::ElyBy => "mojang",
            Self::Microsoft => "msa",
        }
    }
}

/// 8x8 RGBA pixel buffer (`64` pixels * `4` bytes = `256` bytes) representing a rendered 2D skin head
/// (base 8x8 face composited with the 8x8 hat/helmet second layer, ready for Nearest-Neighbor UI scaling).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinHeadAvatar {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA8 pixels (`width * height * 4` bytes, typically `8 * 8 * 4 = 256`).
    pub rgba: Vec<u8>,
}

impl SkinHeadAvatar {
    pub const SIZE: u32 = 8;
    pub const BYTE_LEN: usize = (Self::SIZE as usize) * (Self::SIZE as usize) * 4;
}

/// Stored account profile persisted in `<data_dir>/accounts.json` (mode `0600`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountProfile {
    pub id: AccountId,
    pub kind: AccountKind,
    pub username: String,
    pub uuid: Uuid,
    pub access_token: SecretString,
    #[serde(default)]
    pub refresh_token: Option<SecretString>,
    #[serde(default)]
    pub client_token: Option<SecretString>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub skin_url: Option<String>,
    #[serde(default)]
    pub avatar_head: Option<SkinHeadAvatar>,
}

impl AccountProfile {
    /// Undashed 32-hex UUID string used in Minecraft `--uuid ${auth_uuid}`.
    pub fn uuid_simple(&self) -> String {
        self.uuid.simple().to_string()
    }

    /// Hyphenated 36-char UUID string.
    pub fn uuid_hyphenated(&self) -> String {
        self.uuid.hyphenated().to_string()
    }
}

/// Metadata for `authlib-injector.jar` used by Ely.by accounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthlibInjectorInfo {
    pub build_number: u32,
    pub version: String,
    pub download_url: String,
    pub sha256: String,
}
