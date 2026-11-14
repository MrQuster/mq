//! CurseForge Core API v1 client (`https://api.curseforge.com/v1`) with API key enforcement,
//! 4 content classes (`mc-mods`, `modpacks`, `texture-packs`, `shaders`), sorting & category filters,
//! batch queries (`POST /v1/mods`, `POST /v1/mods/files`), HTML description retrieval,
//! CurseForge MurmurHash2 fingerprinting (`POST /v1/fingerprints/432`), and strict compliance
//! with third-party distribution rules (never synthesizing CDN URLs when `downloadUrl` is `null`
//! or `allowModDistribution == false`).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use mq_core::config::ENV_CURSEFORGE_API_KEY;
use mq_core::{
    AppConfig, ModLoaderKind, MqError, MqResult, SecretString, Sha1Digest, ENV_CF_API_KEY,
};
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::debug;
use url::Url;

use crate::models::{
    CatalogHit, CatalogSource, DependencyKind, ModDependencyRef, ProjectKind, ProjectVersion,
    ProjectVersionFile, ResolvedModDownload, SearchQuery, MQ_USER_AGENT,
};
use crate::resolver::{IntoLoaderFilter, IntoMcVersionFilter};

/// Default production base URL for CurseForge Core API v1.
pub const DEFAULT_CURSEFORGE_API_URL: &str = "https://api.curseforge.com/v1";

/// CurseForge `gameId` for Minecraft: Java Edition.
pub const MINECRAFT_GAME_ID: u32 = 432;

/// CurseForge `classId = 6` for Minecraft Mods (`mc-mods`).
pub const CF_CLASS_ID_MODS: u32 = 6;

/// CurseForge `classId = 4471` for Minecraft Modpacks (`modpacks`).
pub const CF_CLASS_ID_MODPACKS: u32 = 4471;

/// CurseForge `classId = 12` for Minecraft Resource Packs (`texture-packs`).
pub const CF_CLASS_ID_RESOURCE_PACKS: u32 = 12;

/// CurseForge `classId = 6552` for Minecraft Shaders (`shaders`).
pub const CF_CLASS_ID_SHADERS: u32 = 6552;

/// CurseForge `ModLoaderType = 0` (`Any`).
pub const CF_MOD_LOADER_ANY: u32 = 0;

/// CurseForge `ModLoaderType = 1` (`Forge`).
pub const CF_MOD_LOADER_FORGE: u32 = 1;

/// CurseForge `ModLoaderType = 2` (`Cauldron`).
pub const CF_MOD_LOADER_CAULDRON: u32 = 2;

/// CurseForge `ModLoaderType = 3` (`LiteLoader`).
pub const CF_MOD_LOADER_LITELOADER: u32 = 3;

/// CurseForge `ModLoaderType = 4` (`Fabric`).
pub const CF_MOD_LOADER_FABRIC: u32 = 4;

/// CurseForge `ModLoaderType = 5` (`Quilt`).
pub const CF_MOD_LOADER_QUILT: u32 = 5;

/// CurseForge `ModLoaderType = 6` (`NeoForge`).
pub const CF_MOD_LOADER_NEOFORGE: u32 = 6;

/// CurseForge `FileRelationType = 1` (`EmbeddedLibrary`).
pub const CF_RELATION_EMBEDDED_LIBRARY: u32 = 1;

/// CurseForge `FileRelationType = 2` (`OptionalDependency`).
pub const CF_RELATION_OPTIONAL_DEPENDENCY: u32 = 2;

/// CurseForge `FileRelationType = 3` (`RequiredDependency`).
pub const CF_RELATION_REQUIRED_DEPENDENCY: u32 = 3;

/// CurseForge `FileRelationType = 4` (`Tool`).
pub const CF_RELATION_TOOL: u32 = 4;

/// CurseForge `FileRelationType = 5` (`Incompatible`).
pub const CF_RELATION_INCOMPATIBLE: u32 = 5;

/// CurseForge `FileRelationType = 6` (`Include`).
pub const CF_RELATION_INCLUDE: u32 = 6;

/// MurmurHash2 seed (`1`) used by CurseForge for 32-bit file fingerprints.
pub const CF_MURMUR2_SEED: u32 = 1;

/// Allowed HTTPS download hosts for CurseForge CDN artifacts.
pub const ALLOWED_CURSEFORGE_DOWNLOAD_HOSTS: &[&str] = &[
    "edge.forgecdn.net",
    "mediafilez.forgecdn.net",
    "media.forgecdn.net",
    "files.forgecdn.net",
];

/// Validates whether `raw_url` points to an allowed CurseForge CDN host over `https://`
/// (or `http://127.0.0.1` / `http://localhost` for local mock servers in integration tests).
pub fn is_allowed_curseforge_download_url(raw_url: &str) -> bool {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return false;
    }
    let Ok(parsed) = Url::parse(trimmed) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.to_ascii_lowercase();
    if parsed.scheme() == "http" {
        return host_lower == "127.0.0.1" || host_lower == "localhost";
    }
    if parsed.scheme() != "https" {
        return false;
    }
    ALLOWED_CURSEFORGE_DOWNLOAD_HOSTS
        .iter()
        .any(|allowed| host_lower == *allowed)
}

/// No-op compatibility stub: CurseForge third-party distribution rules strictly forbid
/// synthesizing `edge.forgecdn.net` fallback URLs when `downloadUrl` is `null` or
/// `allowModDistribution == false`.
///
/// Always returns an empty string (`""`), marking the file as blocked for automatic download.
/// Use [`format_curseforge_file_webpage_url`] or [`build_curseforge_manual_download_url`]
/// to construct the browser download page URL (`{websiteUrl}/download/{fileId}`).
#[inline]
pub fn compute_curseforge_cdn_fallback_url(_file_id: u64, _file_name: &str) -> String {
    String::new()
}

/// Returns the CurseForge URL path section corresponding to a [`ProjectKind`].
#[inline]
pub const fn project_kind_to_cf_url_section(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Mod => "mc-mods",
        ProjectKind::Modpack => "modpacks",
        ProjectKind::ResourcePack => "texture-packs",
        ProjectKind::Shader => "shaders",
    }
}

/// Formats the official CurseForge browser download page URL (`{websiteUrl}/download/{fileId}`)
/// for a file whose direct `downloadUrl` is `null` or restricted (`allowModDistribution == false`).
pub fn format_curseforge_file_webpage_url(website_url: &str, file_id: u64) -> String {
    let base = website_url.trim().trim_end_matches('/');
    format!("{base}/download/{file_id}")
}

/// Builds the official CurseForge manual download page URL (`{websiteUrl}/download/{fileId}`)
/// from optional `website_url`, falling back to `https://www.curseforge.com/minecraft/{section}/{slug}/download/{fileId}`.
pub fn build_curseforge_manual_download_url(
    website_url: Option<&str>,
    slug: Option<&str>,
    project_kind: ProjectKind,
    file_id: u64,
) -> String {
    if let Some(url) = website_url.map(str::trim).filter(|u| !u.is_empty()) {
        return format_curseforge_file_webpage_url(url, file_id);
    }
    let section = project_kind_to_cf_url_section(project_kind);
    let clean_slug = slug
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");
    format!("https://www.curseforge.com/minecraft/{section}/{clean_slug}/download/{file_id}")
}

/// Returns `true` if a CurseForge file is blocked from automatic third-party download
/// (`downloadUrl` is `None`/empty or `allowModDistribution == Some(false)`).
#[inline]
pub fn is_curseforge_file_blocked(
    download_url: Option<&str>,
    allow_mod_distribution: Option<bool>,
) -> bool {
    if allow_mod_distribution == Some(false) {
        return true;
    }
    download_url.map(str::trim).unwrap_or("").is_empty()
}

/// Returns `true` if `b` is one of the 4 ASCII whitespace bytes stripped by CurseForge's
/// MurmurHash2 fingerprint calculation (`9` = `\t`, `10` = `\n`, `13` = `\r`, `32` = `' '`).
#[inline]
pub const fn is_curseforge_whitespace(b: u8) -> bool {
    matches!(b, 9 | 10 | 13 | 32)
}

/// Computes the 32-bit CurseForge `fileFingerprint` using whitespace-stripped MurmurHash2 (`seed = 1`).
///
/// All bytes equal to `9` (`\t`), `10` (`\n`), `13` (`\r`), or `32` (`' '`) are filtered out
/// before computing MurmurHash2 (`m = 0x5bd1e995`, `r = 24`, `seed = 1`).
pub fn compute_curseforge_fingerprint(bytes: &[u8]) -> u32 {
    const M: u32 = 0x5bd1_e995;
    const R: u32 = 24;

    let normalized_len = bytes
        .iter()
        .copied()
        .filter(|&b| !is_curseforge_whitespace(b))
        .count() as u32;

    let mut h: u32 = CF_MURMUR2_SEED ^ normalized_len;
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0usize;

    for &b in bytes {
        if is_curseforge_whitespace(b) {
            continue;
        }
        chunk[chunk_len] = b;
        chunk_len += 1;
        if chunk_len == 4 {
            let mut k = u32::from_le_bytes(chunk);
            k = k.wrapping_mul(M);
            k ^= k >> R;
            k = k.wrapping_mul(M);

            h = h.wrapping_mul(M);
            h ^= k;
            chunk_len = 0;
        }
    }

    match chunk_len {
        3 => {
            h ^= (chunk[2] as u32) << 16;
            h ^= (chunk[1] as u32) << 8;
            h ^= chunk[0] as u32;
            h = h.wrapping_mul(M);
        }
        2 => {
            h ^= (chunk[1] as u32) << 8;
            h ^= chunk[0] as u32;
            h = h.wrapping_mul(M);
        }
        1 => {
            h ^= chunk[0] as u32;
            h = h.wrapping_mul(M);
        }
        _ => {}
    }

    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}

/// Computes the 32-bit CurseForge MurmurHash2 fingerprint for a file on disk.
pub fn compute_curseforge_fingerprint_file(path: &Path) -> MqResult<u32> {
    let bytes = fs::read(path)?;
    Ok(compute_curseforge_fingerprint(&bytes))
}

/// Maps a [`ModLoaderKind`] to CurseForge's `ModLoaderType` numeric enum (`1=Forge, 4=Fabric, 5=Quilt, 6=NeoForge`).
#[inline]
pub const fn mod_loader_to_cf_type(loader: ModLoaderKind) -> Option<u32> {
    match loader {
        ModLoaderKind::Vanilla => None,
        ModLoaderKind::Forge => Some(CF_MOD_LOADER_FORGE),
        ModLoaderKind::Fabric => Some(CF_MOD_LOADER_FABRIC),
        ModLoaderKind::Quilt => Some(CF_MOD_LOADER_QUILT),
        ModLoaderKind::NeoForge => Some(CF_MOD_LOADER_NEOFORGE),
    }
}

/// Maps a CurseForge `ModLoaderType` integer to a canonical loader name.
#[inline]
pub const fn cf_type_to_mod_loader(cf_loader_type: u32) -> Option<&'static str> {
    match cf_loader_type {
        CF_MOD_LOADER_FORGE => Some("forge"),
        CF_MOD_LOADER_FABRIC => Some("fabric"),
        CF_MOD_LOADER_QUILT => Some("quilt"),
        CF_MOD_LOADER_NEOFORGE => Some("neoforge"),
        _ => None,
    }
}

/// Maps a CurseForge `FileRelationType` integer (`1..=6`) to [`DependencyKind`].
#[inline]
pub const fn cf_relation_to_dependency_kind(relation_type: u32) -> Option<DependencyKind> {
    DependencyKind::from_curseforge_relation_type(relation_type)
}

/// Sort field (`sortField` query parameter) for CurseForge `GET /v1/mods/search`.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CurseForgeSortField {
    #[default]
    Featured = 1,
    Popularity = 2,
    LastUpdated = 3,
    Name = 4,
    Author = 5,
    TotalDownloads = 6,
    Category = 7,
    GameVersion = 8,
    EarlyAccess = 9,
    FeaturedReleased = 10,
    ReleasedDate = 11,
    Rating = 12,
}

impl CurseForgeSortField {
    /// Returns the numeric `sortField` value expected by CurseForge Core API v1.
    #[inline]
    pub const fn as_u32(&self) -> u32 {
        *self as u32
    }

    /// Converts a numeric CurseForge `sortField` value into [`CurseForgeSortField`].
    #[inline]
    pub const fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(Self::Featured),
            2 => Some(Self::Popularity),
            3 => Some(Self::LastUpdated),
            4 => Some(Self::Name),
            5 => Some(Self::Author),
            6 => Some(Self::TotalDownloads),
            7 => Some(Self::Category),
            8 => Some(Self::GameVersion),
            9 => Some(Self::EarlyAccess),
            10 => Some(Self::FeaturedReleased),
            11 => Some(Self::ReleasedDate),
            12 => Some(Self::Rating),
            _ => None,
        }
    }
}

impl FromStr for CurseForgeSortField {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "1" | "featured" | "relevance" | "relevant" => Ok(Self::Featured),
            "2" | "popularity" | "popular" | "follows" | "followers" => Ok(Self::Popularity),
            "3" | "lastupdated" | "last_updated" | "updated" | "recent" => Ok(Self::LastUpdated),
            "4" | "name" | "title" => Ok(Self::Name),
            "5" | "author" => Ok(Self::Author),
            "6" | "totaldownloads" | "total_downloads" | "downloads" => Ok(Self::TotalDownloads),
            "7" | "category" => Ok(Self::Category),
            "8" | "gameversion" | "game_version" => Ok(Self::GameVersion),
            "9" | "earlyaccess" | "early_access" => Ok(Self::EarlyAccess),
            "10" | "featuredreleased" | "featured_released" => Ok(Self::FeaturedReleased),
            "11" | "releaseddate" | "released_date" | "newest" | "new" => Ok(Self::ReleasedDate),
            "12" | "rating" => Ok(Self::Rating),
            other => Err(MqError::Metadata(format!(
                "unknown CurseForge sortField '{other}'"
            ))),
        }
    }
}

/// Sort direction (`sortOrder` query parameter: `"asc"` or `"desc"`) for CurseForge search.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CurseForgeSortOrder {
    Asc,
    #[default]
    Desc,
}

impl CurseForgeSortOrder {
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl fmt::Display for CurseForgeSortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Additional CurseForge-specific search parameters (`categoryId`, `categoryIds`, `sortField`, `sortOrder`, `slug`).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurseForgeSearchOptions {
    pub category_id: Option<u32>,
    pub category_ids: Vec<u32>,
    pub sort_field: Option<CurseForgeSortField>,
    pub sort_order: Option<CurseForgeSortOrder>,
    pub slug: Option<String>,
}

impl CurseForgeSearchOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_category_id(mut self, category_id: u32) -> Self {
        self.category_id = Some(category_id);
        self
    }

    pub fn with_category_ids(mut self, category_ids: impl IntoIterator<Item = u32>) -> Self {
        self.category_ids = category_ids.into_iter().collect();
        self
    }

    pub fn with_sort_field(mut self, sort_field: CurseForgeSortField) -> Self {
        self.sort_field = Some(sort_field);
        self
    }

    pub fn with_sort_order(mut self, sort_order: CurseForgeSortOrder) -> Self {
        self.sort_order = Some(sort_order);
        self
    }

    pub fn with_slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }
}

/// Category entry returned by `GET /v1/categories?gameId=432`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeCategory {
    pub id: u32,
    #[serde(default)]
    pub game_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub is_class: Option<bool>,
    #[serde(default)]
    pub parent_category_id: Option<u32>,
}

/// Screenshot / gallery image attached to a CurseForge project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeScreenshot {
    pub id: u64,
    #[serde(default)]
    pub mod_id: Option<u64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub url: String,
}

/// Detailed CurseForge project metadata (`GET /v1/mods/{modId}` or `POST /v1/mods`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurseForgeModDetails {
    pub hit: CatalogHit,
    pub website_url: Option<String>,
    pub wiki_url: Option<String>,
    pub issues_url: Option<String>,
    pub source_url: Option<String>,
    pub allow_mod_distribution: bool,
    pub categories: Vec<CurseForgeCategory>,
    pub screenshots: Vec<CurseForgeScreenshot>,
    pub description_html: Option<String>,
}

impl CurseForgeModDetails {
    /// Builds the browser download page URL (`{websiteUrl}/download/{fileId}`) for a file of this project.
    pub fn file_webpage_url(&self, file_id: u64) -> String {
        build_curseforge_manual_download_url(
            self.website_url.as_deref(),
            Some(&self.hit.slug),
            self.hit.project_kind,
            file_id,
        )
    }
}

/// Structured information for a CurseForge file blocked from automatic third-party download
/// (`downloadUrl == null` or `allowModDistribution == false`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurseForgeBlockedFile {
    pub project_id: u64,
    pub file_id: u64,
    pub project_title: String,
    pub file_name: String,
    pub webpage_url: String,
    pub expected_size: u64,
    pub expected_sha1: Option<Sha1Digest>,
    pub expected_fingerprint: Option<u32>,
    pub target_rel_path: String,
}

/// Exact fingerprint match entry returned by `POST /v1/fingerprints/432`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurseForgeFingerprintMatch {
    pub mod_id: u64,
    pub fingerprint: u32,
    pub file: ProjectVersion,
    pub latest_files: Vec<ProjectVersion>,
}

/// Asynchronous HTTP client for CurseForge Core API v1.
#[derive(Debug, Clone)]
pub struct CurseForgeClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<SecretString>,
}

impl CurseForgeClient {
    /// Creates a [`CurseForgeClient`] pointing at [`DEFAULT_CURSEFORGE_API_URL`].
    pub fn new(api_key: Option<String>) -> MqResult<Self> {
        Self::with_base_url(DEFAULT_CURSEFORGE_API_URL, api_key)
    }

    /// Creates a [`CurseForgeClient`] reading `$MQ_CURSEFORGE_API_KEY` (or legacy `$MQ_CF_API_KEY`)
    /// from the process environment, falling back to the built-in key unless disabled.
    pub fn from_env() -> MqResult<Self> {
        let key = std::env::var(ENV_CURSEFORGE_API_KEY)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var(ENV_CF_API_KEY)
                    .ok()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                if mq_core::config::is_builtin_cf_key_disabled() {
                    None
                } else {
                    Some(mq_core::config::builtin_cf_api_key())
                }
            });
        Self::new(key)
    }

    /// Creates a [`CurseForgeClient`] from [`AppConfig`], honoring user settings first
    /// (`config.curseforge_api_key` / `0600` secret file) and falling back to
    /// `$MQ_CURSEFORGE_API_KEY` / `$MQ_CF_API_KEY`, plus `config.mock_url`.
    pub fn from_config(config: &AppConfig) -> MqResult<Self> {
        let base_url = match config
            .mock_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(mock) => {
                let trimmed = mock.trim_end_matches('/');
                if trimmed.ends_with("/curseforge/v1") || trimmed.ends_with("/v1") {
                    trimmed.to_owned()
                } else {
                    format!("{trimmed}/curseforge/v1")
                }
            }
            None => DEFAULT_CURSEFORGE_API_URL.to_owned(),
        };
        Self::with_base_url(base_url, config.effective_cf_api_key())
    }

    /// Creates a [`CurseForgeClient`] with a custom `base_url` and optional `api_key`.
    pub fn with_base_url(base_url: impl Into<String>, api_key: Option<String>) -> MqResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(MQ_USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| {
                MqError::Network(format!("failed to build CurseForge HTTP client: {err}"))
            })?;
        Ok(Self::with_http_client(http, base_url, api_key))
    }

    /// Creates a [`CurseForgeClient`] wrapping an existing [`reqwest::Client`], `base_url`, and `api_key`.
    pub fn with_http_client(
        http: reqwest::Client,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        let normalized_url = base_url.into().trim().trim_end_matches('/').to_owned();
        let normalized_key = api_key
            .map(|k| k.trim().to_owned())
            .filter(|k| !k.is_empty())
            .map(SecretString::new);

        Self {
            http,
            base_url: if normalized_url.is_empty() {
                DEFAULT_CURSEFORGE_API_URL.to_owned()
            } else {
                normalized_url
            },
            api_key: normalized_key,
        }
    }

    /// Returns the configured CurseForge API v1 base URL without a trailing slash.
    #[inline]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns `true` if a non-empty CurseForge API key is configured.
    #[inline]
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Validates that a non-empty CurseForge API key is present or returns [`MqError::MissingApiKey`].
    pub fn require_api_key(&self) -> MqResult<&str> {
        self.api_key
            .as_ref()
            .map(SecretString::expose_secret)
            .filter(|s| !s.trim().is_empty())
            .ok_or(MqError::MissingApiKey {
                service: "CurseForge",
                env_var: ENV_CF_API_KEY,
            })
    }

    /// Searches CurseForge (`GET /v1/mods/search?gameId=432&classId=...&searchFilter=...&gameVersion=...&modLoaderType=...`).
    pub async fn search(&self, q: &SearchQuery) -> MqResult<Vec<CatalogHit>> {
        self.search_with_options(q, &CurseForgeSearchOptions::default())
            .await
    }

    /// Searches CurseForge with optional category, sort field, sort order, and slug filters.
    pub async fn search_with_options(
        &self,
        q: &SearchQuery,
        opts: &CurseForgeSearchOptions,
    ) -> MqResult<Vec<CatalogHit>> {
        let api_key = self.require_api_key()?;
        let mut url = self.endpoint_url(&["mods", "search"])?;
        let page_size = if q.limit == 0 { 20 } else { q.limit.min(50) };

        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("gameId", &MINECRAFT_GAME_ID.to_string());
            qp.append_pair("classId", &q.project_kind.curseforge_class_id().to_string());
            qp.append_pair("searchFilter", &q.query);
            if let Some(slug) = opts
                .slug
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                qp.append_pair("slug", slug);
            }
            if let Some(mc_ver) = q
                .mc_version
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                qp.append_pair("gameVersion", mc_ver);
            }
            if matches!(q.project_kind, ProjectKind::Mod | ProjectKind::Modpack) {
                if let Some(loader) = q.loader {
                    if let Some(cf_loader) = mod_loader_to_cf_type(loader) {
                        qp.append_pair("modLoaderType", &cf_loader.to_string());
                    }
                }
            }
            if let Some(cat_id) = opts.category_id {
                qp.append_pair("categoryId", &cat_id.to_string());
            }
            if !opts.category_ids.is_empty() {
                let serialized_cats =
                    serde_json::to_string(&opts.category_ids).unwrap_or_else(|_| "[]".to_owned());
                qp.append_pair("categoryIds", &serialized_cats);
            }
            if let Some(sort_field) = opts.sort_field {
                qp.append_pair("sortField", &sort_field.as_u32().to_string());
            }
            if let Some(sort_order) = opts.sort_order {
                qp.append_pair("sortOrder", sort_order.as_str());
            }
            qp.append_pair("pageSize", &page_size.to_string());
            qp.append_pair("index", &q.offset.to_string());
        }

        debug!(url = %url, "searching CurseForge v1 catalog");
        let env: CfEnvelope<Vec<CfModDto>> = self.get_json(url, api_key).await?;
        Ok(env
            .data
            .into_iter()
            .map(|dto| dto.into_catalog_hit(q.project_kind))
            .collect())
    }

    /// Fetches CurseForge categories for Minecraft (`GET /v1/categories?gameId=432&classId=...`).
    pub async fn get_categories(&self, class_id: Option<u32>) -> MqResult<Vec<CurseForgeCategory>> {
        let api_key = self.require_api_key()?;
        let mut url = self.endpoint_url(&["categories"])?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("gameId", &MINECRAFT_GAME_ID.to_string());
            if let Some(cid) = class_id {
                qp.append_pair("classId", &cid.to_string());
            }
        }
        debug!(url = %url, ?class_id, "fetching CurseForge categories");
        let env: CfEnvelope<Vec<CurseForgeCategory>> = self.get_json(url, api_key).await?;
        Ok(env.data)
    }

    /// Fetches a single CurseForge project (`GET /v1/mods/{mod_id}`).
    pub async fn get_mod(&self, mod_id: u64) -> MqResult<CatalogHit> {
        let api_key = self.require_api_key()?;
        let id_str = mod_id.to_string();
        let url = self.endpoint_url(&["mods", &id_str])?;
        debug!(url = %url, mod_id, "fetching CurseForge mod metadata");
        let env: CfEnvelope<CfModDto> = self.get_json(url, api_key).await?;
        Ok(env.data.into_catalog_hit(ProjectKind::Mod))
    }

    /// Fetches detailed metadata for a single CurseForge project (`GET /v1/mods/{mod_id}`),
    /// including `links.websiteUrl`, `allowModDistribution`, `categories`, and `screenshots`.
    pub async fn get_mod_details(&self, mod_id: u64) -> MqResult<CurseForgeModDetails> {
        let api_key = self.require_api_key()?;
        let id_str = mod_id.to_string();
        let url = self.endpoint_url(&["mods", &id_str])?;
        debug!(url = %url, mod_id, "fetching CurseForge mod details");
        let env: CfEnvelope<CfModDto> = self.get_json(url, api_key).await?;
        Ok(env.data.into_mod_details(ProjectKind::Mod))
    }

    /// Fetches the full HTML description of a CurseForge project (`GET /v1/mods/{mod_id}/description`).
    pub async fn get_mod_description(&self, mod_id: u64) -> MqResult<String> {
        let api_key = self.require_api_key()?;
        let id_str = mod_id.to_string();
        let url = self.endpoint_url(&["mods", &id_str, "description"])?;
        debug!(url = %url, mod_id, "fetching CurseForge mod HTML description");
        let env: CfEnvelope<String> = self.get_json(url, api_key).await?;
        Ok(env.data)
    }

    /// Fetches multiple CurseForge projects in a single batch (`POST /v1/mods` with `{"modIds": [...]}`).
    pub async fn get_mods_batch(&self, mod_ids: &[u64]) -> MqResult<Vec<CatalogHit>> {
        let details = self.get_mod_details_batch(mod_ids).await?;
        Ok(details.into_iter().map(|d| d.hit).collect())
    }

    /// Fetches detailed metadata for multiple CurseForge projects in a single batch (`POST /v1/mods`).
    pub async fn get_mod_details_batch(
        &self,
        mod_ids: &[u64],
    ) -> MqResult<Vec<CurseForgeModDetails>> {
        let api_key = self.require_api_key()?;
        if mod_ids.is_empty() {
            return Ok(Vec::new());
        }
        let url = self.endpoint_url(&["mods"])?;
        let body = serde_json::json!({
            "modIds": mod_ids,
        });
        debug!(url = %url, count = mod_ids.len(), "fetching CurseForge mods batch");
        let env: CfEnvelope<Vec<CfModDto>> = self.post_json(url, api_key, &body).await?;
        Ok(env
            .data
            .into_iter()
            .map(|dto| dto.into_mod_details(ProjectKind::Mod))
            .collect())
    }

    /// Fetches compatible files for a CurseForge project (`GET /v1/mods/{mod_id}/files`).
    ///
    /// When `downloadUrl` is `null` (third-party distribution disabled), `ProjectVersionFile::url`
    /// is left empty (`""`) rather than synthesizing a CDN fallback URL.
    pub async fn get_mod_files(
        &self,
        mod_id: u64,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        let api_key = self.require_api_key()?;
        let id_str = mod_id.to_string();
        let mut url = self.endpoint_url(&["mods", &id_str, "files"])?;
        let mc_filter = mc_version.map(str::trim).filter(|s| !s.is_empty());
        let loader_filter = loader.filter(|l| *l != ModLoaderKind::Vanilla);

        {
            let mut qp = url.query_pairs_mut();
            if let Some(ver) = mc_filter {
                qp.append_pair("gameVersion", ver);
            }
            if let Some(ldr) = loader_filter {
                if let Some(cf_loader) = mod_loader_to_cf_type(ldr) {
                    qp.append_pair("modLoaderType", &cf_loader.to_string());
                }
            }
        }

        debug!(url = %url, mod_id, ?mc_filter, ?loader_filter, "fetching CurseForge mod files");
        let env: CfEnvelope<Vec<CfFileDto>> = self.get_json(url, api_key).await?;

        let mut out = Vec::with_capacity(env.data.len());
        for dto in env.data {
            let ver = dto.into_project_version(mod_id)?;
            if let Some(target_mc) = mc_filter {
                if !ver.game_versions.is_empty()
                    && !ver
                        .game_versions
                        .iter()
                        .any(|gv| gv.eq_ignore_ascii_case(target_mc))
                {
                    continue;
                }
            }
            if let Some(target_loader) = loader_filter {
                if !ver.loaders.is_empty()
                    && !ver.loaders.iter().any(|ld| {
                        ld.eq_ignore_ascii_case(target_loader.as_str())
                            || (target_loader == ModLoaderKind::Quilt
                                && ld.eq_ignore_ascii_case("fabric"))
                    })
                {
                    continue;
                }
            }
            out.push(ver);
        }

        Ok(out)
    }

    /// Fetches a specific file for a CurseForge mod (`GET /v1/mods/{mod_id}/files/{file_id}`).
    pub async fn get_mod_file(&self, mod_id: u64, file_id: u64) -> MqResult<ProjectVersion> {
        let api_key = self.require_api_key()?;
        let mod_str = mod_id.to_string();
        let file_str = file_id.to_string();
        let url = self.endpoint_url(&["mods", &mod_str, "files", &file_str])?;
        debug!(url = %url, mod_id, file_id, "fetching CurseForge mod file by id");
        let env: CfEnvelope<CfFileDto> = self.get_json(url, api_key).await?;
        env.data.into_project_version(mod_id)
    }

    /// Fetches multiple CurseForge files in a single batch (`POST /v1/mods/files` with `{"fileIds": [...]}`).
    pub async fn get_files_batch(&self, file_ids: &[u64]) -> MqResult<Vec<ProjectVersion>> {
        let api_key = self.require_api_key()?;
        if file_ids.is_empty() {
            return Ok(Vec::new());
        }
        let url = self.endpoint_url(&["mods", "files"])?;
        let body = serde_json::json!({
            "fileIds": file_ids,
        });
        debug!(url = %url, count = file_ids.len(), "fetching CurseForge files batch");
        let env: CfEnvelope<Vec<CfFileDto>> = self.post_json(url, api_key, &body).await?;
        let mut out = Vec::with_capacity(env.data.len());
        for dto in env.data {
            let fallback_mod_id = dto.mod_id.unwrap_or(0);
            out.push(dto.into_project_version(fallback_mod_id)?);
        }
        Ok(out)
    }

    /// Fetches the download URL for a specific CurseForge mod file (`GET /v1/mods/{mod_id}/files/{file_id}/download-url`).
    pub async fn get_mod_file_download_url(&self, mod_id: u64, file_id: u64) -> MqResult<String> {
        let api_key = self.require_api_key()?;
        let mod_str = mod_id.to_string();
        let file_str = file_id.to_string();
        let url = self.endpoint_url(&["mods", &mod_str, "files", &file_str, "download-url"])?;
        let env: CfEnvelope<Option<String>> = self.get_json(url, api_key).await?;
        Ok(env.data.unwrap_or_default())
    }

    /// Identifies installed files by their 32-bit CurseForge MurmurHash2 fingerprints
    /// (`POST /v1/fingerprints/432` with `{"fingerprints": [...]}`).
    ///
    /// Returns a map from matched `u32` fingerprint to its [`ProjectVersion`].
    pub async fn identify_by_fingerprints(
        &self,
        fingerprints: &[u32],
    ) -> MqResult<BTreeMap<u32, ProjectVersion>> {
        let matches = self.identify_by_fingerprints_detailed(fingerprints).await?;
        let mut out = BTreeMap::new();
        for m in matches {
            out.insert(m.fingerprint, m.file);
        }
        Ok(out)
    }

    /// Identifies installed files by their 32-bit CurseForge MurmurHash2 fingerprints
    /// (`POST /v1/fingerprints/432`), returning full [`CurseForgeFingerprintMatch`] records
    /// with `file` and `latest_files`.
    pub async fn identify_by_fingerprints_detailed(
        &self,
        fingerprints: &[u32],
    ) -> MqResult<Vec<CurseForgeFingerprintMatch>> {
        let api_key = self.require_api_key()?;
        if fingerprints.is_empty() {
            return Ok(Vec::new());
        }
        let game_id_str = MINECRAFT_GAME_ID.to_string();
        let url = self.endpoint_url(&["fingerprints", &game_id_str])?;
        let body = serde_json::json!({
            "fingerprints": fingerprints,
        });
        debug!(url = %url, count = fingerprints.len(), "identifying files by CurseForge Murmur2 fingerprints");
        let env: CfEnvelope<CfFingerprintsResultDto> = self.post_json(url, api_key, &body).await?;

        let mut out = Vec::with_capacity(env.data.exact_matches.len());
        for (idx, m) in env.data.exact_matches.into_iter().enumerate() {
            let mod_id = m.id.or(m.file.mod_id).unwrap_or(0);
            let fp = m
                .file
                .file_fingerprint
                .and_then(|v| u32::try_from(v).ok())
                .or_else(|| env.data.exact_fingerprints.get(idx).copied())
                .or_else(|| fingerprints.get(idx).copied())
                .unwrap_or(0);
            let file = m.file.into_project_version(mod_id)?;
            let mut latest_files = Vec::with_capacity(m.latest_files.len());
            for lf in m.latest_files {
                if let Ok(pv) = lf.into_project_version(mod_id) {
                    latest_files.push(pv);
                }
            }
            out.push(CurseForgeFingerprintMatch {
                mod_id,
                fingerprint: fp,
                file,
                latest_files,
            });
        }

        Ok(out)
    }

    /// Builds a [`CurseForgeBlockedFile`] descriptor for a file whose `downloadUrl` is empty (`null`)
    /// or whose project has `allowModDistribution == false`.
    pub fn build_blocked_file(
        details: &CurseForgeModDetails,
        version: &ProjectVersion,
    ) -> Option<CurseForgeBlockedFile> {
        let file = version.primary_file()?;
        if details.allow_mod_distribution && !file.url.trim().is_empty() {
            return None;
        }
        let project_id = details.hit.project_id.parse::<u64>().unwrap_or(0);
        let file_id = version.id.parse::<u64>().unwrap_or(0);
        let webpage_url = details.file_webpage_url(file_id);
        let subfolder = match details.hit.project_kind {
            ProjectKind::Mod => "mods",
            ProjectKind::ResourcePack => "resourcepacks",
            ProjectKind::Shader => "shaderpacks",
            ProjectKind::Modpack => "",
        };
        let target_rel_path = if subfolder.is_empty() {
            file.filename.clone()
        } else {
            format!("{subfolder}/{}", file.filename)
        };

        Some(CurseForgeBlockedFile {
            project_id,
            file_id,
            project_title: details.hit.title.clone(),
            file_name: file.filename.clone(),
            webpage_url,
            expected_size: file.size,
            expected_sha1: file.sha1,
            expected_fingerprint: None,
            target_rel_path,
        })
    }

    /// Resolves a CurseForge mod and all transitive required dependencies (`relationType == 3`)
    /// into a deterministic download plan.
    pub async fn resolve_mod_with_dependencies<V, L>(
        &self,
        project_id_or_slug: &str,
        mc_version: V,
        loader: L,
    ) -> MqResult<Vec<ResolvedModDownload>>
    where
        V: IntoMcVersionFilter,
        L: IntoLoaderFilter,
    {
        crate::resolver::resolve_mod_with_dependencies(self, project_id_or_slug, mc_version, loader)
            .await
    }

    fn endpoint_url(&self, segments: &[&str]) -> MqResult<Url> {
        let mut url = Url::parse(&self.base_url).map_err(|err| {
            MqError::Metadata(format!(
                "invalid CurseForge base URL '{}': {err}",
                self.base_url
            ))
        })?;
        {
            let mut path_segs = url.path_segments_mut().map_err(|()| {
                MqError::Metadata(format!(
                    "CurseForge base URL '{}' cannot be a base",
                    self.base_url
                ))
            })?;
            path_segs.pop_if_empty();
            for seg in segments {
                path_segs.push(seg);
            }
        }
        Ok(url)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url, api_key: &str) -> MqResult<T> {
        let url_str = url.to_string();
        let resp = self
            .http
            .get(url)
            .header(USER_AGENT, MQ_USER_AGENT)
            .header(ACCEPT, "application/json")
            .header("x-api-key", api_key)
            .send()
            .await
            .map_err(|err| MqError::Network(format!("GET {url_str} failed: {err}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url: url_str,
                body,
            });
        }

        let bytes = resp.bytes().await.map_err(|err| {
            MqError::Network(format!(
                "failed reading response body from {url_str}: {err}"
            ))
        })?;
        serde_json::from_slice(&bytes).map_err(MqError::Json)
    }

    async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: Url,
        api_key: &str,
        body: &B,
    ) -> MqResult<T> {
        let url_str = url.to_string();
        let payload = serde_json::to_vec(body)?;
        let resp = self
            .http
            .post(url)
            .header(USER_AGENT, MQ_USER_AGENT)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .header("x-api-key", api_key)
            .body(payload)
            .send()
            .await
            .map_err(|err| MqError::Network(format!("POST {url_str} failed: {err}")))?;

        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url: url_str,
                body: err_body,
            });
        }

        let bytes = resp.bytes().await.map_err(|err| {
            MqError::Network(format!(
                "failed reading response body from {url_str}: {err}"
            ))
        })?;
        serde_json::from_slice(&bytes).map_err(MqError::Json)
    }
}

#[derive(Debug, Deserialize)]
struct CfEnvelope<T> {
    data: T,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfLinksDto {
    #[serde(default)]
    website_url: Option<String>,
    #[serde(default)]
    wiki_url: Option<String>,
    #[serde(default)]
    issues_url: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfModDto {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    class_id: Option<u32>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    links: Option<CfLinksDto>,
    #[serde(default)]
    allow_mod_distribution: Option<bool>,
    #[serde(default)]
    authors: Vec<CfAuthorDto>,
    #[serde(default)]
    logo: Option<CfLogoDto>,
    #[serde(default)]
    screenshots: Vec<CurseForgeScreenshot>,
    #[serde(default)]
    categories: Vec<CurseForgeCategory>,
    #[serde(default)]
    main_file_id: Option<u64>,
    #[serde(default)]
    latest_files: Vec<CfFileDto>,
    #[serde(default)]
    latest_files_indexes: Vec<CfFileIndexDto>,
}

#[derive(Debug, Deserialize)]
struct CfAuthorDto {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfLogoDto {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFileIndexDto {
    #[serde(default)]
    game_version: String,
    #[serde(default)]
    file_id: u64,
    #[serde(default)]
    mod_loader: Option<u32>,
}

impl CfModDto {
    fn into_mod_details(self, fallback_kind: ProjectKind) -> CurseForgeModDetails {
        let allow_mod_distribution = self.allow_mod_distribution.unwrap_or(true);
        let links = self.links.clone().unwrap_or_default();
        let categories = self.categories.clone();
        let screenshots = self.screenshots.clone();
        let hit = self.into_catalog_hit(fallback_kind);

        CurseForgeModDetails {
            hit,
            website_url: links.website_url.filter(|s| !s.trim().is_empty()),
            wiki_url: links.wiki_url.filter(|s| !s.trim().is_empty()),
            issues_url: links.issues_url.filter(|s| !s.trim().is_empty()),
            source_url: links.source_url.filter(|s| !s.trim().is_empty()),
            allow_mod_distribution,
            categories,
            screenshots,
            description_html: None,
        }
    }

    fn into_catalog_hit(self, fallback_kind: ProjectKind) -> CatalogHit {
        let project_kind = self
            .class_id
            .and_then(ProjectKind::from_curseforge_class_id)
            .unwrap_or(fallback_kind);
        let author = self
            .authors
            .into_iter()
            .map(|a| a.name)
            .filter(|n| !n.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        let mut loaders = Vec::new();
        let mut game_versions = Vec::new();

        for idx in &self.latest_files_indexes {
            if let Some(ldr_code) = idx.mod_loader {
                if let Some(ldr_str) = cf_type_to_mod_loader(ldr_code) {
                    if !loaders.iter().any(|l| l == ldr_str) {
                        loaders.push(ldr_str.to_owned());
                    }
                }
            }
            classify_cf_version_tag(&idx.game_version, &mut game_versions, &mut loaders);
        }

        for file in &self.latest_files {
            for tag in &file.game_versions {
                classify_cf_version_tag(tag, &mut game_versions, &mut loaders);
            }
        }

        let latest_version = self
            .main_file_id
            .filter(|id| *id > 0)
            .map(|id| id.to_string())
            .or_else(|| self.latest_files.first().map(|f| f.id.to_string()))
            .or_else(|| {
                self.latest_files_indexes
                    .first()
                    .filter(|i| i.file_id > 0)
                    .map(|i| i.file_id.to_string())
            });

        let slug = if self.slug.is_empty() {
            self.id.to_string()
        } else {
            self.slug
        };

        CatalogHit {
            source: CatalogSource::CurseForge,
            project_id: self.id.to_string(),
            slug,
            title: self.name,
            description: self.summary,
            author,
            downloads: self.download_count,
            icon_url: self
                .logo
                .and_then(|l| l.url)
                .filter(|u| !u.trim().is_empty()),
            project_kind,
            loaders,
            game_versions,
            latest_version,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFileDto {
    id: u64,
    #[serde(default)]
    mod_id: Option<u64>,
    #[serde(default)]
    display_name: Option<String>,
    file_name: String,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    file_fingerprint: Option<u64>,
    #[serde(default)]
    hashes: Vec<CfHashDto>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<CfDependencyDto>,
}

#[derive(Debug, Deserialize)]
struct CfHashDto {
    #[serde(default)]
    value: String,
    #[serde(default)]
    algo: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfDependencyDto {
    mod_id: u64,
    relation_type: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFingerprintsResultDto {
    #[serde(default)]
    exact_matches: Vec<CfFingerprintMatchDto>,
    #[serde(default)]
    exact_fingerprints: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFingerprintMatchDto {
    #[serde(default)]
    id: Option<u64>,
    file: CfFileDto,
    #[serde(default)]
    latest_files: Vec<CfFileDto>,
}

impl CfFileDto {
    fn into_project_version(self, fallback_mod_id: u64) -> MqResult<ProjectVersion> {
        self.into_project_version_with_distribution(fallback_mod_id, None)
    }

    fn into_project_version_with_distribution(
        self,
        fallback_mod_id: u64,
        allow_mod_distribution: Option<bool>,
    ) -> MqResult<ProjectVersion> {
        let project_id = self.mod_id.unwrap_or(fallback_mod_id).to_string();

        // STRICT RULE: Never synthesize edge.forgecdn.net fallback URLs when downloadUrl is null
        // or when allowModDistribution is false. Leave `url` empty ("") to signal a blocked file.
        let url =
            if is_curseforge_file_blocked(self.download_url.as_deref(), allow_mod_distribution) {
                String::new()
            } else {
                self.download_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .unwrap_or("")
                    .to_owned()
            };

        // In CurseForge API v1, algo == 1 is SHA-1 (algo == 2 is MD5).
        let sha1 = match self
            .hashes
            .iter()
            .find(|h| h.algo == 1 && !h.value.trim().is_empty())
        {
            Some(h) => Some(Sha1Digest::from_hex(h.value.trim())?),
            None => None,
        };

        let mut game_versions = Vec::new();
        let mut loaders = Vec::new();
        for tag in &self.game_versions {
            classify_cf_version_tag(tag, &mut game_versions, &mut loaders);
        }

        let dependencies = self
            .dependencies
            .into_iter()
            .filter_map(|dep| {
                let kind = DependencyKind::from_curseforge_relation_type(dep.relation_type)?;
                Some(ModDependencyRef {
                    project_id: Some(dep.mod_id.to_string()),
                    version_id: None,
                    file_name: None,
                    kind,
                })
            })
            .collect();

        let name = self
            .display_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.file_name.clone());
        let version_number = self.file_name.clone();

        let file = ProjectVersionFile {
            url,
            filename: self.file_name,
            size: self.file_length,
            sha1,
            sha512: None,
            primary: true,
        };

        Ok(ProjectVersion {
            id: self.id.to_string(),
            project_id,
            name,
            version_number,
            game_versions,
            loaders,
            files: vec![file],
            dependencies,
        })
    }
}

fn classify_cf_version_tag(
    raw_tag: &str,
    game_versions: &mut Vec<String>,
    loaders: &mut Vec<String>,
) {
    let trimmed = raw_tag.trim();
    if trimmed.is_empty() {
        return;
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "fabric" | "quilt" | "forge" | "neoforge" => {
            if !loaders.contains(&lower) {
                loaders.push(lower);
            }
        }
        "client" | "server" => {}
        _ if lower.starts_with("java ") => {}
        _ => {
            if !game_versions
                .iter()
                .any(|v| v.eq_ignore_ascii_case(trimmed))
            {
                game_versions.push(trimmed.to_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Debug, Clone)]
    struct RecordedCfRequest {
        request_line: String,
        headers: HashMap<String, String>,
        body: String,
    }

    async fn spawn_mock_cf_server<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedCfRequest>>>)
    where
        F: Fn(&str, &str, &HashMap<String, String>, &str) -> (u16, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock cf listener");
        let addr = listener.local_addr().expect("local addr");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded);
        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let rec = Arc::clone(&recorded_clone);
                let h = Arc::clone(&handler);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16384];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let (header_part, body_part) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
                    let mut lines = header_part.lines();
                    let request_line = lines.next().unwrap_or("").to_owned();
                    let mut headers = HashMap::new();
                    for line in lines {
                        if line.is_empty() {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
                        }
                    }

                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or("GET");
                    let path_and_query = parts.next().unwrap_or("/");
                    let (status, resp_body) = h(method, path_and_query, &headers, body_part);
                    rec.lock().unwrap().push(RecordedCfRequest {
                        request_line: request_line.clone(),
                        headers,
                        body: body_part.to_owned(),
                    });

                    let status_text = if status == 200 { "OK" } else { "Forbidden" };
                    let resp = format!(
                        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                        resp_body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        (format!("http://{addr}/curseforge/v1"), recorded)
    }

    #[tokio::test]
    async fn curseforge_missing_api_key_returns_clear_error_without_network_call() {
        for missing in [None, Some(String::new()), Some("   ".to_owned())] {
            let client =
                CurseForgeClient::with_base_url("http://127.0.0.1:9/curseforge/v1", missing)
                    .expect("build client");
            assert!(!client.has_api_key());

            let search_err = client
                .search(&SearchQuery::new("jei"))
                .await
                .expect_err("search must fail without API key");
            match search_err {
                MqError::MissingApiKey { service, env_var } => {
                    assert_eq!(service, "CurseForge");
                    assert_eq!(env_var, "MQ_CF_API_KEY");
                }
                other => panic!("expected MissingApiKey, got {other:?}"),
            }

            let files_err = client
                .get_mod_files(394468, Some("1.21.1"), Some(ModLoaderKind::Fabric))
                .await
                .expect_err("get_mod_files must fail without API key");
            assert!(matches!(
                files_err,
                MqError::MissingApiKey {
                    service: "CurseForge",
                    env_var: "MQ_CF_API_KEY"
                }
            ));
        }
    }

    #[test]
    fn curseforge_null_download_url_never_synthesizes_cdn_and_builds_manual_webpage_url() {
        // Strict rule: compute_curseforge_cdn_fallback_url must NOT synthesize edge.forgecdn.net
        assert_eq!(
            compute_curseforge_cdn_fallback_url(4572915, "jei-1.21.1-19.8.0.jar"),
            ""
        );
        assert!(is_curseforge_file_blocked(None, Some(true)));
        assert!(is_curseforge_file_blocked(Some(""), Some(true)));
        assert!(is_curseforge_file_blocked(
            Some("https://edge.forgecdn.net/files/4572/915/jei.jar"),
            Some(false)
        ));
        assert!(!is_curseforge_file_blocked(
            Some("https://edge.forgecdn.net/files/4572/915/jei.jar"),
            Some(true)
        ));

        assert_eq!(
            format_curseforge_file_webpage_url(
                "https://www.curseforge.com/minecraft/mc-mods/jei/",
                4572915
            ),
            "https://www.curseforge.com/minecraft/mc-mods/jei/download/4572915"
        );
        assert_eq!(
            build_curseforge_manual_download_url(None, Some("sodium"), ProjectKind::Mod, 4572915),
            "https://www.curseforge.com/minecraft/mc-mods/sodium/download/4572915"
        );
        assert_eq!(
            build_curseforge_manual_download_url(
                None,
                Some("complementary-unbound"),
                ProjectKind::Shader,
                5551234
            ),
            "https://www.curseforge.com/minecraft/shaders/complementary-unbound/download/5551234"
        );
    }

    #[test]
    fn curseforge_class_ids_loader_types_relations_and_allowed_hosts_match_spec() {
        assert_eq!(MINECRAFT_GAME_ID, 432);
        assert_eq!(ProjectKind::Mod.curseforge_class_id(), CF_CLASS_ID_MODS);
        assert_eq!(
            ProjectKind::Modpack.curseforge_class_id(),
            CF_CLASS_ID_MODPACKS
        );
        assert_eq!(
            ProjectKind::ResourcePack.curseforge_class_id(),
            CF_CLASS_ID_RESOURCE_PACKS
        );
        assert_eq!(
            ProjectKind::Shader.curseforge_class_id(),
            CF_CLASS_ID_SHADERS
        );

        assert_eq!(mod_loader_to_cf_type(ModLoaderKind::Forge), Some(1));
        assert_eq!(mod_loader_to_cf_type(ModLoaderKind::Fabric), Some(4));
        assert_eq!(mod_loader_to_cf_type(ModLoaderKind::Quilt), Some(5));
        assert_eq!(mod_loader_to_cf_type(ModLoaderKind::NeoForge), Some(6));
        assert_eq!(mod_loader_to_cf_type(ModLoaderKind::Vanilla), None);

        assert_eq!(
            cf_relation_to_dependency_kind(1),
            Some(DependencyKind::Embedded)
        );
        assert_eq!(
            cf_relation_to_dependency_kind(2),
            Some(DependencyKind::Optional)
        );
        assert_eq!(
            cf_relation_to_dependency_kind(3),
            Some(DependencyKind::Required)
        );
        assert_eq!(
            cf_relation_to_dependency_kind(4),
            Some(DependencyKind::Optional)
        );
        assert_eq!(
            cf_relation_to_dependency_kind(5),
            Some(DependencyKind::Incompatible)
        );
        assert_eq!(
            cf_relation_to_dependency_kind(6),
            Some(DependencyKind::Embedded)
        );

        assert!(is_allowed_curseforge_download_url(
            "https://edge.forgecdn.net/files/4572/915/sodium.jar"
        ));
        assert!(is_allowed_curseforge_download_url(
            "https://mediafilez.forgecdn.net/files/4572/915/sodium.jar"
        ));
        assert!(!is_allowed_curseforge_download_url(
            "http://edge.forgecdn.net/files/4572/915/sodium.jar"
        ));
        assert!(!is_allowed_curseforge_download_url(
            "https://evil.example.com/files/sodium.jar"
        ));
    }

    #[test]
    fn curseforge_murmur2_fingerprint_strips_whitespace_and_matches_seed_1() {
        // Whitespace bytes (9, 10, 13, 32) must be completely ignored
        let raw_clean = b"CurseForge-Mod-Payload-12345";
        let raw_with_ws = b"CurseForge-\tMod-\nPayload-\r12345   \t\r\n";

        let fp_clean = compute_curseforge_fingerprint(raw_clean);
        let fp_with_ws = compute_curseforge_fingerprint(raw_with_ws);
        assert_eq!(fp_clean, fp_with_ws);
        assert_ne!(fp_clean, 0);

        // Empty and whitespace-only inputs both hash 0 normalized bytes with seed 1:
        // h = 1 ^ 0 = 1 -> h *= 0x5bd1e995 -> h ^= h >> 15 == 0x5bd15e36 (1540447798)
        assert_eq!(compute_curseforge_fingerprint(b""), 0x5bd1_5e36);
        assert_eq!(compute_curseforge_fingerprint(b" \t\r\n  \t"), 0x5bd1_5e36);

        // File helper produces identical fingerprint
        let tmp = tempfile::tempdir().expect("tempdir");
        let file_path = tmp.path().join("sample-mod.jar");
        fs::write(&file_path, raw_with_ws).expect("write sample mod");
        assert_eq!(
            compute_curseforge_fingerprint_file(&file_path).expect("fingerprint file"),
            fp_clean
        );
    }

    #[tokio::test]
    async fn curseforge_search_null_download_url_blocked_file_and_sort_categories() {
        let sha1_hex = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";
        let sha1_clone = sha1_hex.to_owned();

        let (base_url, recorded) =
            spawn_mock_cf_server(move |method, path_and_query, headers, _body| {
                assert_eq!(
                    headers.get("x-api-key").map(String::as_str),
                    Some("cf-secret-test-key")
                );
                assert_eq!(
                    headers.get("user-agent").map(String::as_str),
                    Some(MQ_USER_AGENT)
                );

                if method == "GET" && path_and_query.starts_with("/curseforge/v1/mods/search?") {
                    let body = json!({
                        "data": [
                            {
                                "id": 394468,
                                "gameId": 432,
                                "name": "Sodium",
                                "slug": "sodium",
                                "classId": 6,
                                "summary": "Modern rendering engine",
                                "downloadCount": 1500000,
                                "allowModDistribution": false,
                                "links": {
                                    "websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/sodium"
                                },
                                "authors": [{ "name": "jellysquid3" }],
                                "logo": { "url": "https://media.forgecdn.net/avatars/sodium.png" },
                                "mainFileId": 4572915,
                                "latestFiles": [],
                                "latestFilesIndexes": [
                                    {
                                        "gameVersion": "26.1",
                                        "fileId": 4572915,
                                        "modLoader": 4
                                    }
                                ]
                            }
                        ]
                    })
                    .to_string();
                    (200, body)
                } else if method == "GET" && path_and_query == "/curseforge/v1/mods/394468" {
                    let body = json!({
                        "data": {
                            "id": 394468,
                            "gameId": 432,
                            "name": "Sodium",
                            "slug": "sodium",
                            "classId": 6,
                            "summary": "Modern rendering engine",
                            "downloadCount": 1500000,
                            "allowModDistribution": false,
                            "links": {
                                "websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/sodium"
                            },
                            "authors": [{ "name": "jellysquid3" }],
                            "screenshots": [
                                {
                                    "id": 10,
                                    "title": "Preview",
                                    "description": "Fast chunk rendering",
                                    "url": "https://media.forgecdn.net/attachments/10.png"
                                }
                            ]
                        }
                    })
                    .to_string();
                    (200, body)
                } else if method == "GET"
                    && path_and_query.starts_with("/curseforge/v1/mods/394468/files")
                {
                    // Return downloadUrl: null to verify NO edge.forgecdn.net fallback is synthesized!
                    let body = json!({
                        "data": [
                            {
                                "id": 4572915,
                                "modId": 394468,
                                "displayName": "Sodium 0.6.0",
                                "fileName": "sodium-fabric-0.6.0+mc26.1.jar",
                                "fileLength": 987654,
                                "downloadUrl": null,
                                "fileFingerprint": 33445566,
                                "hashes": [
                                    { "value": sha1_clone, "algo": 1 },
                                    { "value": "098f6bcd4621d373cade4e832627b4f6", "algo": 2 }
                                ],
                                "gameVersions": ["26.1", "Fabric", "Client"],
                                "dependencies": [
                                    { "modId": 306612, "relationType": 3 },
                                    { "modId": 999999, "relationType": 2 },
                                    { "modId": 888888, "relationType": 5 },
                                    { "modId": 777777, "relationType": 1 }
                                ]
                            }
                        ]
                    })
                    .to_string();
                    (200, body)
                } else {
                    (404, json!({ "error": "NotFound" }).to_string())
                }
            })
            .await;

        let client =
            CurseForgeClient::with_base_url(&base_url, Some("cf-secret-test-key".to_owned()))
                .expect("build cf client");

        // Ensure Debug redacts API key
        let dbg_str = format!("{client:?}");
        assert!(!dbg_str.contains("cf-secret-test-key"));

        let query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("26.1")
            .with_loader(ModLoaderKind::Fabric);
        let opts = CurseForgeSearchOptions::new()
            .with_category_id(423)
            .with_sort_field(CurseForgeSortField::TotalDownloads)
            .with_sort_order(CurseForgeSortOrder::Desc);

        let hits = client
            .search_with_options(&query, &opts)
            .await
            .expect("cf search_with_options");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, CatalogSource::CurseForge);
        assert_eq!(hits[0].project_id, "394468");
        assert_eq!(hits[0].slug, "sodium");
        assert_eq!(hits[0].loaders, vec!["fabric"]);
        assert_eq!(hits[0].game_versions, vec!["26.1"]);
        assert_eq!(hits[0].latest_version.as_deref(), Some("4572915"));

        let details = client
            .get_mod_details(394468)
            .await
            .expect("cf get_mod_details");
        assert!(!details.allow_mod_distribution);
        assert_eq!(
            details.file_webpage_url(4572915),
            "https://www.curseforge.com/minecraft/mc-mods/sodium/download/4572915"
        );

        let versions = client
            .get_mod_files(394468, Some("26.1"), Some(ModLoaderKind::Fabric))
            .await
            .expect("cf get_mod_files");
        assert_eq!(versions.len(), 1);
        let ver = &versions[0];
        assert_eq!(ver.id, "4572915");
        assert_eq!(ver.project_id, "394468");
        assert_eq!(ver.dependencies.len(), 4);
        assert_eq!(ver.dependencies[0].kind, DependencyKind::Required);
        assert_eq!(ver.dependencies[1].kind, DependencyKind::Optional);
        assert_eq!(ver.dependencies[2].kind, DependencyKind::Incompatible);
        assert_eq!(ver.dependencies[3].kind, DependencyKind::Embedded);

        // File with downloadUrl: null MUST have an empty url ("") and produce a BlockedFile
        let file = ver.primary_file().expect("primary file");
        assert_eq!(file.url, "");
        assert_eq!(file.sha1.unwrap().to_hex(), sha1_hex);

        let blocked =
            CurseForgeClient::build_blocked_file(&details, ver).expect("blocked file descriptor");
        assert_eq!(blocked.project_id, 394468);
        assert_eq!(blocked.file_id, 4572915);
        assert_eq!(
            blocked.webpage_url,
            "https://www.curseforge.com/minecraft/mc-mods/sodium/download/4572915"
        );
        assert_eq!(
            blocked.target_rel_path,
            "mods/sodium-fabric-0.6.0+mc26.1.jar"
        );

        let reqs = recorded.lock().unwrap();
        assert_eq!(
            reqs[0].headers.get("x-api-key").map(String::as_str),
            Some("cf-secret-test-key")
        );
        assert!(reqs[0].body.is_empty());
        let search_url = Url::parse(&format!(
            "http://localhost{}",
            reqs[0].request_line.split_whitespace().nth(1).unwrap()
        ))
        .unwrap();
        let qp: HashMap<String, String> = search_url.query_pairs().into_owned().collect();
        assert_eq!(qp.get("gameId").map(String::as_str), Some("432"));
        assert_eq!(qp.get("classId").map(String::as_str), Some("6"));
        assert_eq!(qp.get("searchFilter").map(String::as_str), Some("sodium"));
        assert_eq!(qp.get("gameVersion").map(String::as_str), Some("26.1"));
        assert_eq!(qp.get("modLoaderType").map(String::as_str), Some("4"));
        assert_eq!(qp.get("categoryId").map(String::as_str), Some("423"));
        assert_eq!(qp.get("sortField").map(String::as_str), Some("6"));
        assert_eq!(qp.get("sortOrder").map(String::as_str), Some("desc"));
    }

    #[tokio::test]
    async fn curseforge_batch_mods_files_description_and_fingerprints_endpoints() {
        let sample_bytes = b"sample-jar-content-for-cf-fingerprint\r\n";
        let expected_fp = compute_curseforge_fingerprint(sample_bytes);

        let (base_url, recorded) =
            spawn_mock_cf_server(move |method, path, _headers, body| match (method, path) {
                ("POST", "/curseforge/v1/mods") => {
                    let parsed: serde_json::Value = serde_json::from_str(body).expect("json body");
                    assert_eq!(parsed["modIds"], json!([394468, 306612]));
                    (
                        200,
                        json!({
                            "data": [
                                { "id": 394468, "name": "Sodium", "slug": "sodium", "classId": 6 },
                                { "id": 306612, "name": "Fabric API", "slug": "fabric-api", "classId": 6 }
                            ]
                        })
                        .to_string(),
                    )
                }
                ("POST", "/curseforge/v1/mods/files") => {
                    let parsed: serde_json::Value = serde_json::from_str(body).expect("json body");
                    assert_eq!(parsed["fileIds"], json!([4572915]));
                    (
                        200,
                        json!({
                            "data": [
                                {
                                    "id": 4572915,
                                    "modId": 394468,
                                    "fileName": "sodium-fabric-0.6.0.jar",
                                    "fileLength": 512000,
                                    "downloadUrl": "https://edge.forgecdn.net/files/4572/915/sodium-fabric-0.6.0.jar",
                                    "gameVersions": ["1.21.1", "Fabric"]
                                }
                            ]
                        })
                        .to_string(),
                    )
                }
                ("GET", "/curseforge/v1/mods/394468/description") => (
                    200,
                    json!({ "data": "<p>Sodium HTML description</p>" }).to_string(),
                ),
                ("POST", "/curseforge/v1/fingerprints/432") => {
                    let parsed: serde_json::Value = serde_json::from_str(body).expect("json body");
                    assert_eq!(parsed["fingerprints"], json!([expected_fp]));
                    (
                        200,
                        json!({
                            "data": {
                                "isCacheBuilt": true,
                                "exactMatches": [
                                    {
                                        "id": 394468,
                                        "file": {
                                            "id": 4572915,
                                            "modId": 394468,
                                            "fileName": "sodium-fabric-0.6.0.jar",
                                            "fileLength": 512000,
                                            "fileFingerprint": expected_fp,
                                            "downloadUrl": "https://edge.forgecdn.net/files/4572/915/sodium-fabric-0.6.0.jar",
                                            "gameVersions": ["1.21.1", "Fabric"]
                                        },
                                        "latestFiles": []
                                    }
                                ],
                                "exactFingerprints": [expected_fp]
                            }
                        })
                        .to_string(),
                    )
                }
                _ => (404, json!({ "error": "NotFound" }).to_string()),
            })
            .await;

        let client =
            CurseForgeClient::with_base_url(&base_url, Some("cf-secret-test-key".to_owned()))
                .expect("build cf client");

        let mods = client
            .get_mods_batch(&[394468, 306612])
            .await
            .expect("get_mods_batch");
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].slug, "sodium");
        assert_eq!(mods[1].slug, "fabric-api");

        let files = client
            .get_files_batch(&[4572915])
            .await
            .expect("get_files_batch");
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].primary_file().unwrap().url,
            "https://edge.forgecdn.net/files/4572/915/sodium-fabric-0.6.0.jar"
        );

        let desc = client
            .get_mod_description(394468)
            .await
            .expect("get_mod_description");
        assert_eq!(desc, "<p>Sodium HTML description</p>");

        let identified = client
            .identify_by_fingerprints(&[expected_fp])
            .await
            .expect("identify_by_fingerprints");
        assert_eq!(identified.len(), 1);
        let matched_ver = identified.get(&expected_fp).expect("matched fingerprint");
        assert_eq!(matched_ver.id, "4572915");
        assert_eq!(matched_ver.project_id, "394468");

        assert_eq!(recorded.lock().unwrap().len(), 4);
    }
}
