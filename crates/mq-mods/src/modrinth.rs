//! Modrinth API v2 client (`https://api.modrinth.com/v2`) supporting all 4 content types
//! (`mod`, `resourcepack`, `shader`, `modpack`), mandatory `User-Agent`, client-side rate
//! limiting (~300 req/min), `429`/`5xx` retries with `Retry-After` / `X-Ratelimit-Reset`,
//! optional disk cache with TTL and `ETag`, batch project/version queries, and hash lookup
//! (`POST /v2/version_files` and `POST /v2/version_files/update`).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mq_core::{AppConfig, ModLoaderKind, MqError, MqResult, Sha1Digest, Sha512Digest};
use reqwest::header::{HeaderMap, ACCEPT, ETAG, IF_NONE_MATCH, RETRY_AFTER, USER_AGENT};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};
use url::Url;

use crate::models::{
    CatalogHit, CatalogSource, DependencyKind, ModDependencyRef, ProjectKind, ProjectVersion,
    ProjectVersionFile, ResolvedModDownload, SearchQuery,
};
use crate::resolver::{IntoLoaderFilter, IntoMcVersionFilter};

/// Default production base URL for Modrinth API v2.
pub const DEFAULT_MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

/// Mandatory Modrinth API v2 `User-Agent` header (`MrQuster/mq/<version> (https://github.com/MrQuster/mq)`).
/// Never includes a personal email address.
pub const MODRINTH_USER_AGENT: &str = concat!(
    "MrQuster/mq/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/MrQuster/mq)"
);

/// Default Modrinth API v2 rate limit (300 requests per minute per IP).
pub const MODRINTH_RATE_LIMIT_PER_MIN: u32 = 300;

/// Default TTL for cached Modrinth `GET` responses on disk (5 minutes).
pub const DEFAULT_MODRINTH_CACHE_TTL: Duration = Duration::from_secs(300);

/// Default maximum retry attempts on HTTP `429` or `5xx` responses.
pub const DEFAULT_MODRINTH_MAX_RETRIES: u32 = 3;

/// Default base exponential backoff delay on transient errors.
pub const DEFAULT_MODRINTH_BASE_BACKOFF: Duration = Duration::from_millis(200);

/// Maximum sleep duration honored for a single `Retry-After` / `X-Ratelimit-Reset` header.
const MAX_RETRY_SLEEP: Duration = Duration::from_secs(30);

/// Allowed HTTPS download hosts for Modrinth artifacts.
pub const ALLOWED_MODRINTH_DOWNLOAD_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "gitlab.com",
];

/// Validates whether `raw_url` points to an allowed Modrinth download host over `https://`
/// (or `http://127.0.0.1` / `http://localhost` for local mock servers in integration tests).
pub fn is_allowed_modrinth_download_url(raw_url: &str) -> bool {
    let Ok(parsed) = Url::parse(raw_url.trim()) else {
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
    ALLOWED_MODRINTH_DOWNLOAD_HOSTS
        .iter()
        .any(|allowed| host_lower == *allowed)
}

/// Sort order (`index` query parameter) for Modrinth `GET /v2/search`.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ModrinthSearchIndex {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

/// Alias for [`ModrinthSearchIndex`].
pub type ModrinthSearchSort = ModrinthSearchIndex;

impl ModrinthSearchIndex {
    /// Returns the canonical Modrinth `index` query parameter string.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

impl fmt::Display for ModrinthSearchIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ModrinthSearchIndex {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "relevance" | "relevant" => Ok(Self::Relevance),
            "downloads" | "popular" | "popularity" => Ok(Self::Downloads),
            "follows" | "followers" => Ok(Self::Follows),
            "newest" | "new" | "created" => Ok(Self::Newest),
            "updated" | "recent" | "modified" => Ok(Self::Updated),
            other => Err(MqError::Metadata(format!(
                "unknown Modrinth search sort index '{other}' (expected relevance, downloads, follows, newest, or updated)"
            ))),
        }
    }
}

/// Environment support level (`client_side` / `server_side`) on Modrinth.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ModrinthSideRequirement {
    Required,
    Optional,
    Unsupported,
    #[default]
    Unknown,
}

impl ModrinthSideRequirement {
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ModrinthSideRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ModrinthSideRequirement {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "required" => Ok(Self::Required),
            "optional" => Ok(Self::Optional),
            "unsupported" => Ok(Self::Unsupported),
            "unknown" | "" => Ok(Self::Unknown),
            other => Err(MqError::Metadata(format!(
                "unknown Modrinth side requirement '{other}'"
            ))),
        }
    }
}

/// Extended search parameters for Modrinth `GET /v2/search`, adding categories,
/// `client_side` / `server_side` environment facets, and `index` sorting to [`SearchQuery`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModrinthSearchParams {
    pub query: SearchQuery,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub client_side: Vec<String>,
    #[serde(default)]
    pub server_side: Vec<String>,
    #[serde(default)]
    pub index: ModrinthSearchIndex,
}

impl Default for ModrinthSearchParams {
    fn default() -> Self {
        Self {
            query: SearchQuery::default(),
            categories: Vec::new(),
            client_side: Vec::new(),
            server_side: Vec::new(),
            index: ModrinthSearchIndex::Relevance,
        }
    }
}

impl From<SearchQuery> for ModrinthSearchParams {
    fn from(query: SearchQuery) -> Self {
        Self::from_query(query)
    }
}

impl ModrinthSearchParams {
    /// Wraps a base [`SearchQuery`] with default Modrinth sorting (`relevance`) and no extra facets.
    pub fn from_query(query: SearchQuery) -> Self {
        Self {
            query,
            ..Self::default()
        }
    }

    /// Appends category filters (ANDed across distinct categories).
    pub fn with_categories<I, S>(mut self, categories: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.categories = categories
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    /// Sets the `client_side` OR-facet values (e.g. `["required", "optional"]`).
    pub fn with_client_side<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.client_side = values
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    /// Sets the `server_side` OR-facet values (e.g. `["required", "optional"]`).
    pub fn with_server_side<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.server_side = values
            .into_iter()
            .map(Into::into)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    /// Sets the Modrinth `index` sort order (`relevance`, `downloads`, `follows`, `newest`, `updated`).
    pub fn with_index(mut self, index: ModrinthSearchIndex) -> Self {
        self.index = index;
        self
    }
}

/// Paginated search response from Modrinth `GET /v2/search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModrinthSearchPage {
    pub hits: Vec<CatalogHit>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u64,
}

/// License metadata attached to a Modrinth project.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModrinthLicense {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// Gallery screenshot entry attached to a Modrinth project.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModrinthGalleryItem {
    pub url: String,
    #[serde(default)]
    pub raw_url: Option<String>,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub ordering: i64,
}

/// Full Modrinth project metadata returned by `GET /v2/project/{id|slug}` or `GET /v2/projects?ids=[...]`,
/// including Markdown `body`, `license`, `gallery`, and `client_side`/`server_side` requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModrinthProjectDetails {
    pub hit: CatalogHit,
    pub id: String,
    pub slug: String,
    pub project_type: ProjectKind,
    pub title: String,
    pub description: String,
    pub body: String,
    pub author: String,
    pub downloads: u64,
    pub followers: u64,
    pub icon_url: Option<String>,
    pub license: Option<ModrinthLicense>,
    pub gallery: Vec<ModrinthGalleryItem>,
    pub client_side: String,
    pub server_side: String,
    pub categories: Vec<String>,
    pub additional_categories: Vec<String>,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub versions: Vec<String>,
    pub source_url: Option<String>,
    pub issues_url: Option<String>,
    pub wiki_url: Option<String>,
    pub discord_url: Option<String>,
}

/// Hash algorithm selector for Modrinth `POST /v2/version_files` and `POST /v2/version_files/update`.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModrinthHashAlgorithm {
    Sha1,
    #[default]
    Sha512,
}

impl ModrinthHashAlgorithm {
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha512 => "sha512",
        }
    }

    /// Detects the hash algorithm from the hex string length (40 chars -> `sha1`, otherwise `sha512`).
    #[inline]
    pub fn detect_from_hex(hash: &str) -> Self {
        if hash.trim().len() == 40 {
            Self::Sha1
        } else {
            Self::Sha512
        }
    }
}

#[derive(Debug)]
struct RateLimiterState {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    min_interval: Option<Duration>,
    last_refill: Instant,
    last_request: Option<Instant>,
    blocked_until: Option<Instant>,
}

impl RateLimiterState {
    fn new(requests_per_minute: u32) -> Self {
        let rpm = requests_per_minute.max(1) as f64;
        Self {
            tokens: rpm,
            capacity: rpm,
            refill_per_sec: rpm / 60.0,
            min_interval: None,
            last_refill: Instant::now(),
            last_request: None,
            blocked_until: None,
        }
    }

    fn acquire_wait(&mut self, now: Instant) -> Option<Duration> {
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
            self.last_refill = now;
        }

        let mut wait = Duration::ZERO;

        if let Some(until) = self.blocked_until {
            if until > now {
                wait = wait.max(until.saturating_duration_since(now));
            } else {
                self.blocked_until = None;
            }
        }

        if let (Some(min_int), Some(last_req)) = (self.min_interval, self.last_request) {
            let next_allowed = last_req + min_int;
            if next_allowed > now {
                wait = wait.max(next_allowed.saturating_duration_since(now));
            }
        }

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
        } else {
            let deficit = 1.0 - self.tokens;
            let token_wait = Duration::from_secs_f64(deficit / self.refill_per_sec.max(0.001));
            self.tokens = 0.0;
            wait = wait.max(token_wait);
        }

        self.last_request = Some(now + wait);
        (!wait.is_zero()).then_some(wait)
    }

    fn observe_ratelimit_headers(&mut self, remaining: Option<u64>, reset_secs: Option<u64>) {
        if let Some(rem) = remaining {
            self.tokens = (rem as f64).min(self.capacity);
            self.last_refill = Instant::now();
            if rem == 0 {
                if let Some(secs) = reset_secs.filter(|s| *s > 0) {
                    let delay = Duration::from_secs(secs).min(MAX_RETRY_SLEEP);
                    self.blocked_until = Some(Instant::now() + delay);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskCacheEntry {
    url: String,
    cached_at_unix_ms: u64,
    #[serde(default)]
    etag: Option<String>,
    body: String,
}

/// Asynchronous HTTP client for Modrinth API v2.
#[derive(Debug, Clone)]
pub struct ModrinthClient {
    http: reqwest::Client,
    base_url: String,
    rate_limiter: Arc<Mutex<RateLimiterState>>,
    cache_dir: Option<PathBuf>,
    cache_ttl: Duration,
    max_retries: u32,
    base_backoff: Duration,
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new().expect("default reqwest::Client for ModrinthClient should initialize")
    }
}

impl ModrinthClient {
    /// Creates a new [`ModrinthClient`] pointing at [`DEFAULT_MODRINTH_API_URL`].
    pub fn new() -> MqResult<Self> {
        Self::with_base_url(DEFAULT_MODRINTH_API_URL)
    }

    /// Creates a [`ModrinthClient`] honoring `config.mock_url` when configured.
    pub fn from_config(config: &AppConfig) -> MqResult<Self> {
        let base_url = match config
            .mock_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(mock) => {
                let trimmed = mock.trim_end_matches('/');
                if trimmed.ends_with("/modrinth/v2") || trimmed.ends_with("/v2") {
                    trimmed.to_owned()
                } else {
                    format!("{trimmed}/modrinth/v2")
                }
            }
            None => DEFAULT_MODRINTH_API_URL.to_owned(),
        };
        Self::with_base_url(base_url)
    }

    /// Creates a new [`ModrinthClient`] with a custom `base_url` (e.g. `"{mock}/modrinth/v2"`).
    pub fn with_base_url(base_url: impl Into<String>) -> MqResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(MODRINTH_USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| {
                MqError::Network(format!("failed to build Modrinth HTTP client: {err}"))
            })?;
        Ok(Self::with_http_client(http, base_url))
    }

    /// Creates a [`ModrinthClient`] wrapping a pre-built [`reqwest::Client`] and `base_url`.
    pub fn with_http_client(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        let normalized = base_url.into().trim().trim_end_matches('/').to_owned();
        Self {
            http,
            base_url: if normalized.is_empty() {
                DEFAULT_MODRINTH_API_URL.to_owned()
            } else {
                normalized
            },
            rate_limiter: Arc::new(Mutex::new(RateLimiterState::new(
                MODRINTH_RATE_LIMIT_PER_MIN,
            ))),
            cache_dir: None,
            cache_ttl: DEFAULT_MODRINTH_CACHE_TTL,
            max_retries: DEFAULT_MODRINTH_MAX_RETRIES,
            base_backoff: DEFAULT_MODRINTH_BASE_BACKOFF,
        }
    }

    /// Configures an optional on-disk `GET` response cache with TTL and `ETag` conditional revalidation.
    pub fn with_disk_cache(mut self, cache_dir: impl Into<PathBuf>, ttl: Duration) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self.cache_ttl = ttl;
        self
    }

    /// Configures the client-side token-bucket rate limit in requests per minute (default: 300 req/min).
    pub fn with_rate_limit_per_minute(self, requests_per_minute: u32) -> Self {
        if let Ok(mut guard) = self.rate_limiter.lock() {
            *guard = RateLimiterState::new(requests_per_minute);
        }
        self
    }

    /// Configures a minimum pacing interval between consecutive requests.
    pub fn with_min_request_interval(self, min_interval: Duration) -> Self {
        if let Ok(mut guard) = self.rate_limiter.lock() {
            guard.min_interval = Some(min_interval);
        }
        self
    }

    /// Configures retry attempts and base backoff duration for `429`/`5xx` responses.
    pub fn with_retry_policy(mut self, max_retries: u32, base_backoff: Duration) -> Self {
        self.max_retries = max_retries;
        self.base_backoff = base_backoff;
        self
    }

    /// Returns the configured Modrinth API v2 base URL without a trailing slash.
    #[inline]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the mandatory Modrinth `User-Agent` header value.
    #[inline]
    pub const fn user_agent() -> &'static str {
        MODRINTH_USER_AGENT
    }

    /// Builds the Modrinth v2 `facets` matrix (`Vec<Vec<String>>`) for a [`SearchQuery`].
    ///
    /// For a Quilt instance when searching mods (`ProjectKind::Mod` + `ModLoaderKind::Quilt`),
    /// includes `["categories:quilt", "categories:fabric"]` so compatible Fabric mods are also found.
    pub fn build_facets(q: &SearchQuery) -> Vec<Vec<String>> {
        let mut facets = vec![vec![format!(
            "project_type:{}",
            q.project_kind.as_modrinth_project_type()
        )]];

        if let Some(mc_ver) = q
            .mc_version
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            facets.push(vec![format!("versions:{mc_ver}")]);
        }

        if let Some(loader) = q.loader.filter(|l| *l != ModLoaderKind::Vanilla) {
            match q.project_kind {
                ProjectKind::Mod => {
                    if loader == ModLoaderKind::Quilt {
                        facets.push(vec![
                            "categories:quilt".to_owned(),
                            "categories:fabric".to_owned(),
                        ]);
                    } else {
                        facets.push(vec![format!("categories:{}", loader.as_str())]);
                    }
                }
                ProjectKind::Modpack => {
                    facets.push(vec![format!("categories:{}", loader.as_str())]);
                }
                ProjectKind::ResourcePack | ProjectKind::Shader => {}
            }
        }

        facets
    }

    /// Builds the Modrinth v2 `facets` matrix (`Vec<Vec<String>>`) including category filters
    /// and `client_side` / `server_side` environment requirements.
    pub fn build_facets_with_options(params: &ModrinthSearchParams) -> Vec<Vec<String>> {
        let mut facets = Self::build_facets(&params.query);

        for cat in &params.categories {
            let trimmed = cat.trim();
            if !trimmed.is_empty() {
                let formatted = if trimmed.starts_with("categories:") {
                    trimmed.to_owned()
                } else {
                    format!("categories:{trimmed}")
                };
                facets.push(vec![formatted]);
            }
        }

        let client_group: Vec<String> = params
            .client_side
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with("client_side:") {
                    s.to_owned()
                } else {
                    format!("client_side:{s}")
                }
            })
            .collect();
        if !client_group.is_empty() {
            facets.push(client_group);
        }

        let server_group: Vec<String> = params
            .server_side
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with("server_side:") {
                    s.to_owned()
                } else {
                    format!("server_side:{s}")
                }
            })
            .collect();
        if !server_group.is_empty() {
            facets.push(server_group);
        }

        facets
    }

    /// Searches the Modrinth catalog (`GET /search`) using query text, pagination, and facets.
    pub async fn search(&self, q: &SearchQuery) -> MqResult<Vec<CatalogHit>> {
        let mut url = self.endpoint_url(&["search"])?;
        let limit = if q.limit == 0 { 20 } else { q.limit.min(100) };
        let facets_json = serde_json::to_string(&Self::build_facets(q))?;

        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("query", &q.query);
            qp.append_pair("limit", &limit.to_string());
            qp.append_pair("offset", &q.offset.to_string());
            qp.append_pair("facets", &facets_json);
        }

        debug!(url = %url, "searching Modrinth v2 catalog");
        let resp: ModrinthSearchResponseDto = self.get_json(url).await?;
        Ok(resp
            .hits
            .into_iter()
            .map(|hit| hit.into_catalog_hit(q.project_kind))
            .collect())
    }

    /// Searches the Modrinth catalog (`GET /search`) with categories, environment facets, and `index` sorting.
    pub async fn search_with_options(
        &self,
        params: &ModrinthSearchParams,
    ) -> MqResult<ModrinthSearchPage> {
        let mut url = self.endpoint_url(&["search"])?;
        let q = &params.query;
        let limit = if q.limit == 0 { 20 } else { q.limit.min(100) };
        let facets_json = serde_json::to_string(&Self::build_facets_with_options(params))?;

        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("query", &q.query);
            qp.append_pair("index", params.index.as_str());
            qp.append_pair("limit", &limit.to_string());
            qp.append_pair("offset", &q.offset.to_string());
            qp.append_pair("facets", &facets_json);
        }

        debug!(url = %url, index = %params.index, "searching Modrinth v2 catalog with options");
        let resp: ModrinthSearchResponseDto = self.get_json(url).await?;
        let hits = resp
            .hits
            .into_iter()
            .map(|hit| hit.into_catalog_hit(q.project_kind))
            .collect();
        Ok(ModrinthSearchPage {
            hits,
            offset: resp.offset.unwrap_or(q.offset),
            limit: resp.limit.unwrap_or(limit),
            total_hits: resp.total_hits.unwrap_or(0),
        })
    }

    /// Fetches project metadata (`GET /project/{id_or_slug}`) as a [`CatalogHit`].
    pub async fn get_project(&self, id_or_slug: &str) -> MqResult<CatalogHit> {
        let details = self.get_project_details(id_or_slug).await?;
        Ok(details.hit)
    }

    /// Fetches full project metadata (`GET /project/{id_or_slug}`), including Markdown `body`,
    /// `license`, `gallery`, and `client_side`/`server_side` fields.
    pub async fn get_project_details(&self, id_or_slug: &str) -> MqResult<ModrinthProjectDetails> {
        let url = self.endpoint_url(&["project", id_or_slug])?;
        debug!(url = %url, id_or_slug, "fetching Modrinth project metadata");
        let dto: ModrinthProjectDto = self.get_json(url).await?;
        Ok(dto.into_project_details())
    }

    /// Fetches multiple projects in a single batch call (`GET /v2/projects?ids=[...]`) as [`CatalogHit`]s.
    pub async fn get_projects_batch(&self, ids: &[String]) -> MqResult<Vec<CatalogHit>> {
        let details = self.get_projects_details_batch(ids).await?;
        Ok(details.into_iter().map(|d| d.hit).collect())
    }

    /// Fetches full details for multiple projects in a single batch call (`GET /v2/projects?ids=[...]`).
    pub async fn get_projects_details_batch(
        &self,
        ids: &[String],
    ) -> MqResult<Vec<ModrinthProjectDetails>> {
        let clean_ids: Vec<&str> = ids
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if clean_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut url = self.endpoint_url(&["projects"])?;
        let ids_json = serde_json::to_string(&clean_ids)?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("ids", &ids_json);
        }

        debug!(url = %url, count = clean_ids.len(), "fetching Modrinth batch projects");
        let dtos: Vec<ModrinthProjectDto> = self.get_json(url).await?;
        Ok(dtos
            .into_iter()
            .map(ModrinthProjectDto::into_project_details)
            .collect())
    }

    /// Checks whether a version's `game_versions` list matches `target_mc` without making
    /// legacy `1.x.y` semver assumptions (supports `26.x` versions such as `26.1`, `26.1.1`, snapshots, etc.).
    pub fn matches_mc_version(game_versions: &[String], target_mc: &str) -> bool {
        let target = target_mc.trim();
        if target.is_empty() {
            return true;
        }
        game_versions.iter().any(|gv| {
            let candidate = gv.trim();
            if candidate.eq_ignore_ascii_case(target) {
                return true;
            }
            // Support wildcard major.minor tags like "26.1.x" or "1.21.x" matching "26.1.1" or "1.21.1".
            if let Some(prefix) = candidate
                .strip_suffix(".x")
                .or_else(|| candidate.strip_suffix(".*"))
            {
                if target.eq_ignore_ascii_case(prefix) {
                    return true;
                }
                if let Some(rest) = target.strip_prefix(prefix) {
                    return rest.starts_with('.');
                }
            }
            false
        })
    }

    /// Checks whether a version's `loaders` list matches `target_loader`.
    /// For `ModLoaderKind::Quilt`, accepts both `"quilt"` and `"fabric"` loaders.
    pub fn matches_loader(version_loaders: &[String], target_loader: ModLoaderKind) -> bool {
        if target_loader == ModLoaderKind::Vanilla {
            return true;
        }
        // Resourcepacks and shaders may omit mod loaders or list shader/datapack loaders ("iris", "optifine", "minecraft", "vanilla").
        let has_any_mod_loader = version_loaders.iter().any(|l| {
            matches!(
                l.trim().to_ascii_lowercase().as_str(),
                "fabric" | "quilt" | "forge" | "neoforge" | "liteloader" | "rift"
            )
        });
        if !has_any_mod_loader && !version_loaders.is_empty() {
            return true;
        }

        match target_loader {
            ModLoaderKind::Quilt => version_loaders.iter().any(|ld| {
                let lower = ld.trim().to_ascii_lowercase();
                lower == "quilt" || lower == "fabric"
            }),
            other => version_loaders
                .iter()
                .any(|ld| ld.trim().eq_ignore_ascii_case(other.as_str())),
        }
    }

    /// Fetches versions for a project (`GET /project/{id_or_slug}/version`), applying both
    /// server query parameters and client-side filtering for `mc_version` (including `26.x`) and `loader`
    /// (including `Quilt` compatibility with `Fabric`).
    pub async fn get_project_versions(
        &self,
        id_or_slug: &str,
        mc_version: Option<&str>,
        loader: Option<ModLoaderKind>,
    ) -> MqResult<Vec<ProjectVersion>> {
        let mut url = self.endpoint_url(&["project", id_or_slug, "version"])?;
        let mc_filter = mc_version.map(str::trim).filter(|s| !s.is_empty());
        let loader_filter = loader.filter(|l| *l != ModLoaderKind::Vanilla);

        {
            let mut qp = url.query_pairs_mut();
            if let Some(ver) = mc_filter {
                let gv_json = serde_json::to_string(&[ver])?;
                qp.append_pair("game_versions", &gv_json);
            }
            if let Some(ldr) = loader_filter {
                let loaders_slice: &[&str] = if ldr == ModLoaderKind::Quilt {
                    &["quilt", "fabric"]
                } else {
                    &[ldr.as_str()]
                };
                let ldr_json = serde_json::to_string(loaders_slice)?;
                qp.append_pair("loaders", &ldr_json);
            }
        }

        debug!(url = %url, id_or_slug, ?mc_filter, ?loader_filter, "fetching Modrinth project versions");
        let raw_versions: Vec<ModrinthVersionDto> = self.get_json(url).await?;

        let mut out = Vec::with_capacity(raw_versions.len());
        for dto in raw_versions {
            let ver = dto.into_project_version()?;
            if let Some(target_mc) = mc_filter {
                if !Self::matches_mc_version(&ver.game_versions, target_mc) {
                    continue;
                }
            }
            if let Some(target_loader) = loader_filter {
                if !Self::matches_loader(&ver.loaders, target_loader) {
                    continue;
                }
            }
            out.push(ver);
        }

        Ok(out)
    }

    /// Fetches a specific version by ID (`GET /version/{version_id}`).
    pub async fn get_version(&self, version_id: &str) -> MqResult<ProjectVersion> {
        let url = self.endpoint_url(&["version", version_id])?;
        debug!(url = %url, version_id, "fetching Modrinth version by id");
        let dto: ModrinthVersionDto = self.get_json(url).await?;
        dto.into_project_version()
    }

    /// Fetches multiple versions in a single batch call (`GET /v2/versions?ids=[...]`).
    pub async fn get_versions_batch(
        &self,
        version_ids: &[String],
    ) -> MqResult<Vec<ProjectVersion>> {
        let clean_ids: Vec<&str> = version_ids
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if clean_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut url = self.endpoint_url(&["versions"])?;
        let ids_json = serde_json::to_string(&clean_ids)?;
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("ids", &ids_json);
        }

        debug!(url = %url, count = clean_ids.len(), "fetching Modrinth batch versions");
        let dtos: Vec<ModrinthVersionDto> = self.get_json(url).await?;
        let mut out = Vec::with_capacity(dtos.len());
        for dto in dtos {
            out.push(dto.into_project_version()?);
        }
        Ok(out)
    }

    /// Identifies installed files by their SHA-512 or SHA-1 hashes via `POST /v2/version_files`.
    ///
    /// Automatically groups hashes by length (40 hex chars -> `sha1`, 128 hex chars -> `sha512`)
    /// and returns a map of `<hash> -> ProjectVersion`.
    pub async fn identify_by_hashes(
        &self,
        hashes: &[String],
    ) -> MqResult<BTreeMap<String, ProjectVersion>> {
        let mut sha512_hashes = Vec::new();
        let mut sha1_hashes = Vec::new();

        for raw in hashes {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            match ModrinthHashAlgorithm::detect_from_hex(trimmed) {
                ModrinthHashAlgorithm::Sha1 => sha1_hashes.push(trimmed.to_owned()),
                ModrinthHashAlgorithm::Sha512 => sha512_hashes.push(trimmed.to_owned()),
            }
        }

        let mut out = BTreeMap::new();
        if !sha512_hashes.is_empty() {
            let part = self
                .identify_by_hashes_with_algorithm(&sha512_hashes, ModrinthHashAlgorithm::Sha512)
                .await?;
            out.extend(part);
        }
        if !sha1_hashes.is_empty() {
            let part = self
                .identify_by_hashes_with_algorithm(&sha1_hashes, ModrinthHashAlgorithm::Sha1)
                .await?;
            out.extend(part);
        }
        Ok(out)
    }

    /// Identifies installed files by hash with an explicit [`ModrinthHashAlgorithm`] (`POST /v2/version_files`).
    pub async fn identify_by_hashes_with_algorithm(
        &self,
        hashes: &[String],
        algorithm: ModrinthHashAlgorithm,
    ) -> MqResult<BTreeMap<String, ProjectVersion>> {
        let clean: Vec<&str> = hashes
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if clean.is_empty() {
            return Ok(BTreeMap::new());
        }

        let url = self.endpoint_url(&["version_files"])?;
        let body = serde_json::json!({
            "hashes": clean,
            "algorithm": algorithm.as_str(),
        });

        debug!(url = %url, algorithm = algorithm.as_str(), count = clean.len(), "identifying Modrinth files by hash");
        let raw_map: BTreeMap<String, ModrinthVersionDto> = self.post_json(url, &body).await?;

        let mut out = BTreeMap::new();
        for (hash, dto) in raw_map {
            if let Ok(pv) = dto.into_project_version() {
                out.insert(hash, pv);
            }
        }
        Ok(out)
    }

    /// Checks for updates for a set of installed files via Modrinth `POST /v2/version_files/update`.
    ///
    /// Automatically selects `"sha512"` or `"sha1"` based on hash length, includes `["quilt", "fabric"]`
    /// for Quilt instances, and maps `<current_hash> -> <latest_compatible_version>`.
    pub async fn check_version_updates(
        &self,
        hashes: &[String],
        loader: ModLoaderKind,
        mc_version: &str,
    ) -> MqResult<BTreeMap<String, ProjectVersion>> {
        let clean: Vec<&str> = hashes
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if clean.is_empty() {
            return Ok(BTreeMap::new());
        }

        let algorithm = ModrinthHashAlgorithm::detect_from_hex(clean[0]);
        let loaders: Vec<&str> = match loader {
            ModLoaderKind::Quilt => vec!["quilt", "fabric"],
            ModLoaderKind::Vanilla => vec!["minecraft", "datapack", "iris", "optifine"],
            other => vec![other.as_str()],
        };

        let url = self.endpoint_url(&["version_files", "update"])?;
        let body = serde_json::json!({
            "hashes": clean,
            "algorithm": algorithm.as_str(),
            "loaders": loaders,
            "game_versions": [mc_version.trim()],
        });

        debug!(url = %url, algorithm = algorithm.as_str(), ?loaders, mc_version, "checking Modrinth version updates");
        let raw_map: BTreeMap<String, ModrinthVersionDto> = self.post_json(url, &body).await?;

        let mut out = BTreeMap::new();
        for (hash, dto) in raw_map {
            if let Ok(pv) = dto.into_project_version() {
                out.insert(hash, pv);
            }
        }

        Ok(out)
    }

    /// Resolves a project and all transitive required dependencies into a deterministic download plan.
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
                "invalid Modrinth base URL '{}': {err}",
                self.base_url
            ))
        })?;
        {
            let mut path_segs = url.path_segments_mut().map_err(|()| {
                MqError::Metadata(format!(
                    "Modrinth base URL '{}' cannot be a base",
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

    async fn wait_for_rate_limit(&self) {
        let delay = {
            let Ok(mut guard) = self.rate_limiter.lock() else {
                return;
            };
            guard.acquire_wait(Instant::now())
        };
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }
    }

    fn record_response_headers(&self, headers: &HeaderMap) {
        let remaining = parse_header_u64(headers, "x-ratelimit-remaining");
        let reset = parse_header_u64(headers, "x-ratelimit-reset");
        if remaining.is_some() || reset.is_some() {
            if let Ok(mut guard) = self.rate_limiter.lock() {
                guard.observe_ratelimit_headers(remaining, reset);
            }
        }
    }

    fn compute_retry_delay(&self, attempt: u32, headers: Option<&HeaderMap>) -> Duration {
        if let Some(hdrs) = headers {
            if let Some(secs) = hdrs
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|v| *v >= 0.0)
            {
                return Duration::from_secs_f64(secs).min(MAX_RETRY_SLEEP);
            }
            if let Some(reset_secs) = parse_header_u64(hdrs, "x-ratelimit-reset") {
                if reset_secs > 0 {
                    return Duration::from_secs(reset_secs).min(MAX_RETRY_SLEEP);
                }
            }
        }

        let shift = attempt.min(6);
        let exp = self.base_backoff.saturating_mul(1u32 << shift);
        let jitter_ms = u64::from((attempt.wrapping_mul(37) % 53) + 7);
        (exp + Duration::from_millis(jitter_ms)).min(MAX_RETRY_SLEEP)
    }

    fn cache_file_path(&self, url_str: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let mut hasher = Sha256::new();
        hasher.update(url_str.as_bytes());
        let digest_hex = hex::encode(hasher.finalize());
        Some(dir.join(format!("{digest_hex}.json")))
    }

    fn read_disk_cache(&self, url_str: &str) -> Option<DiskCacheEntry> {
        let path = self.cache_file_path(url_str)?;
        let raw = fs::read_to_string(path).ok()?;
        let entry: DiskCacheEntry = serde_json::from_str(&raw).ok()?;
        (entry.url == url_str).then_some(entry)
    }

    fn write_disk_cache(&self, url_str: &str, etag: Option<String>, body: &str) {
        let Some(path) = self.cache_file_path(url_str) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let entry = DiskCacheEntry {
            url: url_str.to_owned(),
            cached_at_unix_ms: now_unix_ms(),
            etag,
            body: body.to_owned(),
        };
        if let Ok(serialized) = serde_json::to_string(&entry) {
            let _ = write_file_atomically(&path, serialized.as_bytes());
        }
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> MqResult<T> {
        let url_str = url.to_string();
        let mut conditional_etag: Option<String> = None;
        let mut cached_body_on_304: Option<String> = None;

        if let Some(entry) = self.read_disk_cache(&url_str) {
            let age_ms = now_unix_ms().saturating_sub(entry.cached_at_unix_ms);
            let ttl_ms = u64::try_from(self.cache_ttl.as_millis()).unwrap_or(u64::MAX);
            if age_ms <= ttl_ms {
                if let Ok(parsed) = serde_json::from_str::<T>(&entry.body) {
                    debug!(url = %url_str, age_ms, "Modrinth GET disk cache hit (fresh TTL)");
                    return Ok(parsed);
                }
            }
            if let Some(etag) = entry.etag.filter(|e| !e.trim().is_empty()) {
                conditional_etag = Some(etag);
                cached_body_on_304 = Some(entry.body);
            }
        }

        let mut attempt = 0u32;
        loop {
            self.wait_for_rate_limit().await;

            let mut req = self
                .http
                .get(url.clone())
                .header(USER_AGENT, MODRINTH_USER_AGENT)
                .header(ACCEPT, "application/json");

            if let Some(ref etag) = conditional_etag {
                req = req.header(IF_NONE_MATCH, etag);
            }

            let send_res = req.send().await;
            let resp = match send_res {
                Ok(r) => r,
                Err(err) => {
                    if attempt < self.max_retries {
                        let delay = self.compute_retry_delay(attempt, None);
                        warn!(
                            url = %url_str,
                            attempt,
                            ?delay,
                            error = %err,
                            "transient Modrinth GET network error; retrying"
                        );
                        attempt += 1;
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(MqError::Network(format!("GET {url_str} failed: {err}")));
                }
            };

            self.record_response_headers(resp.headers());
            let status = resp.status();

            if status == StatusCode::NOT_MODIFIED {
                if let Some(ref cached_body) = cached_body_on_304 {
                    self.write_disk_cache(&url_str, conditional_etag.clone(), cached_body);
                    debug!(url = %url_str, "Modrinth GET 304 Not Modified (ETag cache revalidated)");
                    return serde_json::from_str(cached_body).map_err(MqError::Json);
                }
            }

            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < self.max_retries
            {
                let delay = self.compute_retry_delay(attempt, Some(resp.headers()));
                warn!(
                    url = %url_str,
                    status = status.as_u16(),
                    attempt,
                    ?delay,
                    "Modrinth GET returned retryable status; backing off"
                );
                attempt += 1;
                tokio::time::sleep(delay).await;
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(MqError::HttpStatus {
                    status: status.as_u16(),
                    url: url_str,
                    body,
                });
            }

            let response_etag = resp
                .headers()
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty());

            let bytes = resp.bytes().await.map_err(|err| {
                MqError::Network(format!(
                    "failed reading response body from {url_str}: {err}"
                ))
            })?;

            if self.cache_dir.is_some() {
                if let Ok(body_str) = std::str::from_utf8(&bytes) {
                    self.write_disk_cache(&url_str, response_etag, body_str);
                }
            }

            return serde_json::from_slice(&bytes).map_err(MqError::Json);
        }
    }

    async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: Url,
        body: &B,
    ) -> MqResult<T> {
        let url_str = url.to_string();
        let mut attempt = 0u32;

        loop {
            self.wait_for_rate_limit().await;

            let send_res = self
                .http
                .post(url.clone())
                .header(USER_AGENT, MODRINTH_USER_AGENT)
                .header(ACCEPT, "application/json")
                .json(body)
                .send()
                .await;

            let resp = match send_res {
                Ok(r) => r,
                Err(err) => {
                    if attempt < self.max_retries {
                        let delay = self.compute_retry_delay(attempt, None);
                        warn!(
                            url = %url_str,
                            attempt,
                            ?delay,
                            error = %err,
                            "transient Modrinth POST network error; retrying"
                        );
                        attempt += 1;
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(MqError::Network(format!("POST {url_str} failed: {err}")));
                }
            };

            self.record_response_headers(resp.headers());
            let status = resp.status();

            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < self.max_retries
            {
                let delay = self.compute_retry_delay(attempt, Some(resp.headers()));
                warn!(
                    url = %url_str,
                    status = status.as_u16(),
                    attempt,
                    ?delay,
                    "Modrinth POST returned retryable status; backing off"
                );
                attempt += 1;
                tokio::time::sleep(delay).await;
                continue;
            }

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
            return serde_json::from_slice(&bytes).map_err(MqError::Json);
        }
    }
}

fn parse_header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn write_file_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponseDto {
    #[serde(default)]
    hits: Vec<ModrinthSearchHitDto>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    total_hits: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchHitDto {
    #[serde(alias = "id")]
    project_id: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    display_categories: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default, alias = "game_versions")]
    versions: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    latest_version: Option<String>,
}

impl ModrinthSearchHitDto {
    fn into_catalog_hit(self, fallback_kind: ProjectKind) -> CatalogHit {
        let project_kind =
            ProjectKind::from_modrinth_project_type(&self.project_type).unwrap_or(fallback_kind);
        let loaders = if !self.loaders.is_empty() {
            self.loaders
        } else {
            extract_loaders_from_categories(&self.categories, &self.display_categories)
        };
        let slug = if self.slug.is_empty() {
            self.project_id.clone()
        } else {
            self.slug
        };

        CatalogHit {
            source: CatalogSource::Modrinth,
            project_id: self.project_id,
            slug,
            title: self.title,
            description: self.description,
            author: self.author,
            downloads: self.downloads,
            icon_url: self.icon_url.filter(|u| !u.trim().is_empty()),
            project_kind,
            loaders,
            game_versions: self.versions,
            latest_version: self.latest_version.filter(|v| !v.trim().is_empty()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ModrinthLicenseWire {
    Object(ModrinthLicense),
    IdOnly(String),
}

impl ModrinthLicenseWire {
    fn into_license(self) -> Option<ModrinthLicense> {
        match self {
            Self::Object(lic) => {
                if lic.id.trim().is_empty() && lic.name.trim().is_empty() {
                    None
                } else {
                    Some(lic)
                }
            }
            Self::IdOnly(id) => {
                let trimmed = id.trim().to_owned();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(ModrinthLicense {
                        id: trimmed.clone(),
                        name: trimmed,
                        url: None,
                    })
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthProjectDto {
    id: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    project_type: String,
    #[serde(default)]
    team: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    followers: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    license: Option<ModrinthLicenseWire>,
    #[serde(default)]
    gallery: Vec<ModrinthGalleryItem>,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    additional_categories: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    issues_url: Option<String>,
    #[serde(default)]
    wiki_url: Option<String>,
    #[serde(default)]
    discord_url: Option<String>,
}

impl ModrinthProjectDto {
    fn into_project_details(self) -> ModrinthProjectDetails {
        let project_kind =
            ProjectKind::from_modrinth_project_type(&self.project_type).unwrap_or(ProjectKind::Mod);
        let loaders = if !self.loaders.is_empty() {
            self.loaders
        } else {
            extract_loaders_from_categories(&self.categories, &self.additional_categories)
        };
        let latest_version = self.versions.last().cloned();
        let slug = if self.slug.is_empty() {
            self.id.clone()
        } else {
            self.slug
        };
        let icon_url = self.icon_url.filter(|u| !u.trim().is_empty());

        let hit = CatalogHit {
            source: CatalogSource::Modrinth,
            project_id: self.id.clone(),
            slug: slug.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            author: self.team.clone(),
            downloads: self.downloads,
            icon_url: icon_url.clone(),
            project_kind,
            loaders: loaders.clone(),
            game_versions: self.game_versions.clone(),
            latest_version,
        };

        let mut gallery = self.gallery;
        gallery.sort_by_key(|item| item.ordering);

        ModrinthProjectDetails {
            hit,
            id: self.id,
            slug,
            project_type: project_kind,
            title: self.title,
            description: self.description,
            body: self.body,
            author: self.team,
            downloads: self.downloads,
            followers: self.followers,
            icon_url,
            license: self.license.and_then(ModrinthLicenseWire::into_license),
            gallery,
            client_side: if self.client_side.trim().is_empty() {
                "unknown".to_owned()
            } else {
                self.client_side
            },
            server_side: if self.server_side.trim().is_empty() {
                "unknown".to_owned()
            } else {
                self.server_side
            },
            categories: self.categories,
            additional_categories: self.additional_categories,
            loaders,
            game_versions: self.game_versions,
            versions: self.versions,
            source_url: self.source_url.filter(|s| !s.trim().is_empty()),
            issues_url: self.issues_url.filter(|s| !s.trim().is_empty()),
            wiki_url: self.wiki_url.filter(|s| !s.trim().is_empty()),
            discord_url: self.discord_url.filter(|s| !s.trim().is_empty()),
        }
    }
}

fn extract_loaders_from_categories(primary: &[String], secondary: &[String]) -> Vec<String> {
    let mut loaders = Vec::new();
    for cat in primary.iter().chain(secondary.iter()) {
        let lower = cat.trim().to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "fabric" | "quilt" | "forge" | "neoforge" | "iris" | "optifine" | "minecraft"
        ) && !loaders.contains(&lower)
        {
            loaders.push(lower);
        }
    }
    loaders
}

#[derive(Debug, Deserialize)]
struct ModrinthVersionDto {
    id: String,
    project_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    files: Vec<ModrinthFileDto>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependencyDto>,
}

#[derive(Debug, Default, Deserialize)]
struct ModrinthHashesDto {
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    sha512: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFileDto {
    #[serde(default)]
    hashes: ModrinthHashesDto,
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthDependencyDto {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    dependency_type: String,
}

impl ModrinthVersionDto {
    fn into_project_version(self) -> MqResult<ProjectVersion> {
        let has_explicit_primary = self.files.iter().any(|f| f.primary);
        let mut files = Vec::with_capacity(self.files.len());
        for (idx, file_dto) in self.files.into_iter().enumerate() {
            let sha1 = match file_dto
                .hashes
                .sha1
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(hex_str) => Some(Sha1Digest::from_hex(hex_str)?),
                None => None,
            };
            let sha512 = match file_dto
                .hashes
                .sha512
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(hex_str) => Some(Sha512Digest::from_hex(hex_str)?),
                None => None,
            };
            files.push(ProjectVersionFile {
                url: file_dto.url,
                filename: file_dto.filename,
                size: file_dto.size,
                sha1,
                sha512,
                primary: if has_explicit_primary {
                    file_dto.primary
                } else {
                    idx == 0
                },
            });
        }

        let dependencies = self
            .dependencies
            .into_iter()
            .filter_map(|dep| {
                let kind = DependencyKind::from_modrinth_type(&dep.dependency_type)?;
                Some(ModDependencyRef {
                    project_id: dep.project_id.filter(|s| !s.trim().is_empty()),
                    version_id: dep.version_id.filter(|s| !s.trim().is_empty()),
                    file_name: dep.file_name.filter(|s| !s.trim().is_empty()),
                    kind,
                })
            })
            .collect();

        let name = if self.name.is_empty() {
            self.version_number.clone()
        } else {
            self.name
        };

        Ok(ProjectVersion {
            id: self.id,
            project_id: self.project_id,
            name,
            version_number: self.version_number,
            game_versions: self.game_versions,
            loaders: self.loaders,
            files,
            dependencies,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Debug, Clone)]
    struct RecordedRequest {
        request_line: String,
        user_agent: Option<String>,
        if_none_match: Option<String>,
        body: String,
    }

    struct MockResponseSpec {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl MockResponseSpec {
        fn json(status: u16, body: impl Into<String>) -> Self {
            Self {
                status,
                headers: Vec::new(),
                body: body.into(),
            }
        }

        fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.headers.push((key.into(), value.into()));
            self
        }
    }

    async fn spawn_mock_http_server<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedRequest>>>)
    where
        F: Fn(&str) -> (u16, String) + Send + Sync + 'static,
    {
        spawn_mock_http_server_ext(move |req| {
            let path = req.request_line.split_whitespace().nth(1).unwrap_or("/");
            let (status, body) = handler(path);
            MockResponseSpec::json(status, body)
        })
        .await
    }

    async fn spawn_mock_http_server_ext<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedRequest>>>)
    where
        F: Fn(&RecordedRequest) -> MockResponseSpec + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral mock listener");
        let addr = listener.local_addr().expect("local addr");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded);
        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let recorded_req = Arc::clone(&recorded_clone);
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
                    let mut user_agent = None;
                    let mut if_none_match = None;
                    for line in lines {
                        if let Some((k, v)) = line.split_once(':') {
                            let key = k.trim();
                            if key.eq_ignore_ascii_case("user-agent") {
                                user_agent = Some(v.trim().to_owned());
                            } else if key.eq_ignore_ascii_case("if-none-match") {
                                if_none_match = Some(v.trim().to_owned());
                            }
                        }
                    }

                    let rec = RecordedRequest {
                        request_line,
                        user_agent,
                        if_none_match,
                        body: body_part.to_owned(),
                    };
                    let spec = h(&rec);
                    recorded_req.lock().unwrap().push(rec);

                    let status_text = match spec.status {
                        200 => "OK",
                        304 => "Not Modified",
                        429 => "Too Many Requests",
                        500..=599 => "Internal Server Error",
                        _ => "Not Found",
                    };
                    let mut extra_headers = String::new();
                    for (hk, hv) in &spec.headers {
                        extra_headers.push_str(&format!("{hk}: {hv}\r\n"));
                    }
                    let resp = format!(
                        "HTTP/1.1 {} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{}",
                        spec.status,
                        spec.body.len(),
                        spec.body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        (format!("http://{addr}/modrinth/v2"), recorded)
    }

    #[test]
    fn modrinth_user_agent_matches_specification_without_personal_email() {
        assert!(MODRINTH_USER_AGENT.starts_with("MrQuster/mq/"));
        assert!(MODRINTH_USER_AGENT.ends_with(" (https://github.com/MrQuster/mq)"));
        assert!(
            !MODRINTH_USER_AGENT.contains('@'),
            "MODRINTH_USER_AGENT must never include a personal email"
        );
    }

    #[test]
    fn modrinth_facet_builder_formats_all_content_types_quilt_fabric_and_options() {
        let mod_query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("1.21.1")
            .with_loader(ModLoaderKind::Fabric);
        assert_eq!(
            ModrinthClient::build_facets(&mod_query),
            vec![
                vec!["project_type:mod".to_owned()],
                vec!["versions:1.21.1".to_owned()],
                vec!["categories:fabric".to_owned()],
            ]
        );

        // Quilt instance searching mods must include both ["categories:quilt", "categories:fabric"].
        let quilt_mod_query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("26.1")
            .with_loader(ModLoaderKind::Quilt);
        assert_eq!(
            ModrinthClient::build_facets(&quilt_mod_query),
            vec![
                vec!["project_type:mod".to_owned()],
                vec!["versions:26.1".to_owned()],
                vec![
                    "categories:quilt".to_owned(),
                    "categories:fabric".to_owned()
                ],
            ]
        );

        // Shaders and resourcepacks should not emit a mod loader category facet even if loader is set.
        for (kind, expected_type) in [
            (ProjectKind::Shader, "project_type:shader"),
            (ProjectKind::ResourcePack, "project_type:resourcepack"),
        ] {
            let q = SearchQuery::new("complementary")
                .with_project_kind(kind)
                .with_mc_version("1.21.1")
                .with_loader(ModLoaderKind::Fabric);
            assert_eq!(
                ModrinthClient::build_facets(&q),
                vec![
                    vec![expected_type.to_owned()],
                    vec!["versions:1.21.1".to_owned()],
                ]
            );
        }

        // Extended options with categories and client_side/server_side environment facets.
        let params = ModrinthSearchParams::from_query(quilt_mod_query)
            .with_categories(["optimization", "technology"])
            .with_client_side(["required", "optional"])
            .with_server_side(["optional"])
            .with_index(ModrinthSearchIndex::Downloads);
        assert_eq!(
            ModrinthClient::build_facets_with_options(&params),
            vec![
                vec!["project_type:mod".to_owned()],
                vec!["versions:26.1".to_owned()],
                vec![
                    "categories:quilt".to_owned(),
                    "categories:fabric".to_owned()
                ],
                vec!["categories:optimization".to_owned()],
                vec!["categories:technology".to_owned()],
                vec![
                    "client_side:required".to_owned(),
                    "client_side:optional".to_owned()
                ],
                vec!["server_side:optional".to_owned()],
            ]
        );
    }

    #[tokio::test]
    async fn modrinth_search_sends_mandatory_user_agent_and_parses_facets_and_sort_index() {
        let (base_url, recorded) = spawn_mock_http_server(|path_and_query| {
            assert!(path_and_query.starts_with("/modrinth/v2/search?"));
            let body = json!({
                "hits": [
                    {
                        "project_id": "AANobbMI",
                        "project_type": "mod",
                        "slug": "sodium",
                        "author": "jellysquid3",
                        "title": "Sodium",
                        "description": "Modern rendering engine",
                        "categories": ["fabric", "quilt", "optimization"],
                        "versions": ["26.1", "1.21.1"],
                        "downloads": 42500000,
                        "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
                        "latest_version": "ver-sodium-261"
                    }
                ],
                "offset": 0,
                "limit": 10,
                "total_hits": 1
            })
            .to_string();
            (200, body)
        })
        .await;

        let client = ModrinthClient::with_base_url(&base_url).expect("create modrinth client");
        let query = SearchQuery::new("sodium")
            .with_project_kind(ProjectKind::Mod)
            .with_mc_version("26.1")
            .with_loader(ModLoaderKind::Quilt)
            .with_limit(10);
        let params = ModrinthSearchParams::from_query(query)
            .with_categories(["optimization"])
            .with_index(ModrinthSearchIndex::Follows);

        let page = client
            .search_with_options(&params)
            .await
            .expect("modrinth search with options");
        assert_eq!(page.total_hits, 1);
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].source, CatalogSource::Modrinth);
        assert_eq!(page.hits[0].project_id, "AANobbMI");
        assert_eq!(page.hits[0].slug, "sodium");
        assert_eq!(page.hits[0].project_kind, ProjectKind::Mod);
        assert_eq!(page.hits[0].loaders, vec!["fabric", "quilt"]);
        assert_eq!(page.hits[0].game_versions, vec!["26.1", "1.21.1"]);

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].user_agent.as_deref(), Some(MODRINTH_USER_AGENT));

        let parsed_url = Url::parse(&format!(
            "http://localhost{}",
            reqs[0].request_line.split_whitespace().nth(1).unwrap()
        ))
        .expect("parse recorded request URL");
        let qp: std::collections::HashMap<String, String> =
            parsed_url.query_pairs().into_owned().collect();
        assert_eq!(qp.get("query").map(String::as_str), Some("sodium"));
        assert_eq!(qp.get("index").map(String::as_str), Some("follows"));
        assert_eq!(qp.get("limit").map(String::as_str), Some("10"));
        assert_eq!(
            qp.get("facets").map(String::as_str),
            Some("[[\"project_type:mod\"],[\"versions:26.1\"],[\"categories:quilt\",\"categories:fabric\"],[\"categories:optimization\"]]")
        );
    }

    #[tokio::test]
    async fn modrinth_get_project_details_and_batch_parse_body_license_gallery_and_sides() {
        let (base_url, _recorded) = spawn_mock_http_server(|path| {
            if path.starts_with("/modrinth/v2/project/sodium") {
                let body = json!({
                    "id": "AANobbMI",
                    "slug": "sodium",
                    "project_type": "mod",
                    "team": "jellysquid3",
                    "title": "Sodium",
                    "description": "Fast rendering engine",
                    "body": "# Sodium\nFull Markdown description.",
                    "downloads": 50000000,
                    "followers": 120000,
                    "client_side": "required",
                    "server_side": "unsupported",
                    "categories": ["fabric", "quilt", "optimization"],
                    "loaders": ["fabric", "quilt"],
                    "game_versions": ["26.1", "1.21.1"],
                    "versions": ["ver-1", "ver-2"],
                    "license": {
                        "id": "LGPL-3.0-only",
                        "name": "GNU Lesser General Public License v3.0 only",
                        "url": "https://spdx.org/licenses/LGPL-3.0-only.html"
                    },
                    "gallery": [
                        {
                            "url": "https://cdn.modrinth.com/gallery/2.png",
                            "featured": false,
                            "title": "Second",
                            "ordering": 2
                        },
                        {
                            "url": "https://cdn.modrinth.com/gallery/1.png",
                            "featured": true,
                            "title": "First",
                            "ordering": 1
                        }
                    ]
                })
                .to_string();
                (200, body)
            } else if path.starts_with("/modrinth/v2/projects?ids=") {
                let body = json!([
                    {
                        "id": "AANobbMI",
                        "slug": "sodium",
                        "project_type": "mod",
                        "title": "Sodium",
                        "description": "Fast rendering",
                        "body": "Body 1",
                        "license": "LGPL-3.0-only"
                    },
                    {
                        "id": "YL57xq9U",
                        "slug": "iris",
                        "project_type": "shader",
                        "title": "Iris Shaders",
                        "description": "Modern shader pack loader",
                        "body": "Body 2",
                        "license": { "id": "LGPL-3.0", "name": "LGPL 3.0" }
                    }
                ])
                .to_string();
                (200, body)
            } else {
                (404, "{}".to_owned())
            }
        })
        .await;

        let client = ModrinthClient::with_base_url(&base_url).expect("client");
        let details = client
            .get_project_details("sodium")
            .await
            .expect("get_project_details");
        assert_eq!(details.id, "AANobbMI");
        assert_eq!(details.body, "# Sodium\nFull Markdown description.");
        assert_eq!(details.client_side, "required");
        assert_eq!(details.server_side, "unsupported");
        assert_eq!(
            details.license.as_ref().map(|l| l.id.as_str()),
            Some("LGPL-3.0-only")
        );
        assert_eq!(details.gallery.len(), 2);
        assert_eq!(details.gallery[0].title.as_deref(), Some("First"));
        assert_eq!(details.hit.latest_version.as_deref(), Some("ver-2"));

        let batch = client
            .get_projects_batch(&["AANobbMI".to_owned(), "YL57xq9U".to_owned()])
            .await
            .expect("get_projects_batch");
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].project_kind, ProjectKind::Mod);
        assert_eq!(batch[1].project_kind, ProjectKind::Shader);
    }

    #[tokio::test]
    async fn modrinth_get_project_versions_supports_quilt_fabric_and_26x_versions() {
        let sha1_hex = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";
        let sha512_hex = "5a".repeat(64);
        let sha1_clone = sha1_hex.to_owned();
        let sha512_clone = sha512_hex.clone();

        let (base_url, recorded) = spawn_mock_http_server(move |_path| {
            let body = json!([
                {
                    "id": "ver-sodium-261-fabric",
                    "project_id": "AANobbMI",
                    "name": "Sodium 0.7.0",
                    "version_number": "0.7.0+mc26.1.1",
                    "game_versions": ["26.1.1"],
                    "loaders": ["fabric"],
                    "dependencies": [
                        {
                            "project_id": "P7dR8mSH",
                            "version_id": "ver-fapi-261",
                            "dependency_type": "required"
                        }
                    ],
                    "files": [
                        {
                            "hashes": {
                                "sha1": sha1_clone,
                                "sha512": sha512_clone
                            },
                            "url": "https://cdn.modrinth.com/sodium-0.7.0.jar",
                            "filename": "sodium-0.7.0.jar",
                            "primary": true,
                            "size": 102400
                        }
                    ]
                },
                {
                    "id": "ver-sodium-1201-forge",
                    "project_id": "AANobbMI",
                    "name": "Sodium 0.5.0 Forge",
                    "version_number": "0.5.0+mc1.20.1",
                    "game_versions": ["1.20.1"],
                    "loaders": ["forge"],
                    "dependencies": [],
                    "files": []
                }
            ])
            .to_string();
            (200, body)
        })
        .await;

        let client = ModrinthClient::with_base_url(&base_url).expect("client");
        // Requesting for MC "26.1.1" and Quilt loader should match a Fabric version for 26.1.1!
        let versions = client
            .get_project_versions("sodium", Some("26.1.1"), Some(ModLoaderKind::Quilt))
            .await
            .expect("get versions");

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, "ver-sodium-261-fabric");
        assert_eq!(versions[0].dependencies.len(), 1);
        assert_eq!(versions[0].dependencies[0].kind, DependencyKind::Required);
        let file = versions[0].primary_file().expect("primary file");
        assert_eq!(file.filename, "sodium-0.7.0.jar");
        assert_eq!(file.sha1.unwrap().to_hex(), sha1_hex);
        assert_eq!(file.sha512.unwrap().to_hex(), sha512_hex);

        let reqs = recorded.lock().unwrap();
        assert_eq!(reqs[0].user_agent.as_deref(), Some(MODRINTH_USER_AGENT));
    }

    #[tokio::test]
    async fn modrinth_identify_by_hashes_queries_version_files_for_sha512_and_sha1() {
        let sha512_hash = "ab".repeat(64);
        let sha1_hash = "cd".repeat(20);
        let sha512_clone = sha512_hash.clone();
        let sha1_clone = sha1_hash.clone();

        let (base_url, recorded) = spawn_mock_http_server_ext(move |req| {
            assert!(req
                .request_line
                .starts_with("POST /modrinth/v2/version_files "));
            let parsed: serde_json::Value =
                serde_json::from_str(&req.body).expect("parse version_files body");
            let algo = parsed.get("algorithm").and_then(|v| v.as_str()).unwrap();
            let body = if algo == "sha512" {
                json!({
                    sha512_clone.clone(): {
                        "id": "ver-by-sha512",
                        "project_id": "AANobbMI",
                        "name": "Sodium",
                        "version_number": "0.6.0",
                        "game_versions": ["1.21.1"],
                        "loaders": ["fabric"],
                        "files": []
                    }
                })
                .to_string()
            } else {
                json!({
                    sha1_clone.clone(): {
                        "id": "ver-by-sha1",
                        "project_id": "P7dR8mSH",
                        "name": "Fabric API",
                        "version_number": "0.102.0",
                        "game_versions": ["1.21.1"],
                        "loaders": ["fabric"],
                        "files": []
                    }
                })
                .to_string()
            };
            MockResponseSpec::json(200, body)
        })
        .await;

        let client = ModrinthClient::with_base_url(&base_url).expect("client");
        let identified = client
            .identify_by_hashes(&[sha512_hash.clone(), sha1_hash.clone()])
            .await
            .expect("identify_by_hashes");

        assert_eq!(identified.len(), 2);
        assert_eq!(identified[&sha512_hash].id, "ver-by-sha512");
        assert_eq!(identified[&sha1_hash].id, "ver-by-sha1");
        assert_eq!(recorded.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn modrinth_retries_429_and_503_honoring_retry_after_and_caches_with_etag() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let (base_url, recorded) = spawn_mock_http_server_ext(move |req| {
            let call_idx = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if call_idx == 0 {
                MockResponseSpec::json(429, r#"{"error":"rate_limited"}"#)
                    .with_header("Retry-After", "0")
                    .with_header("X-Ratelimit-Remaining", "0")
                    .with_header("X-Ratelimit-Reset", "0")
            } else if call_idx == 1 {
                MockResponseSpec::json(503, r#"{"error":"unavailable"}"#)
            } else if req.if_none_match.as_deref() == Some("\"etag-sodium-v1\"") {
                MockResponseSpec::json(304, "")
            } else {
                let body = json!({
                    "id": "AANobbMI",
                    "slug": "sodium",
                    "project_type": "mod",
                    "title": "Sodium",
                    "description": "Fast renderer"
                })
                .to_string();
                MockResponseSpec::json(200, body).with_header("ETag", "\"etag-sodium-v1\"")
            }
        })
        .await;

        let tmp = TempDir::new().expect("tempdir");
        let client = ModrinthClient::with_base_url(&base_url)
            .expect("client")
            .with_retry_policy(3, Duration::from_millis(10))
            .with_disk_cache(tmp.path(), Duration::from_millis(50));

        // 1st call: experiences 429 -> 503 -> 200 OK and writes to disk cache with ETag.
        let hit1 = client
            .get_project("sodium")
            .await
            .expect("project after retries");
        assert_eq!(hit1.project_id, "AANobbMI");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);

        // 2nd immediate call within TTL: served directly from disk cache without hitting network!
        let hit2 = client
            .get_project("sodium")
            .await
            .expect("project from fresh cache");
        assert_eq!(hit2.project_id, "AANobbMI");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);

        // Wait for TTL to expire, then 3rd call sends If-None-Match and receives 304 Not Modified!
        tokio::time::sleep(Duration::from_millis(70)).await;
        let hit3 = client
            .get_project("sodium")
            .await
            .expect("project after 304 ETag revalidation");
        assert_eq!(hit3.project_id, "AANobbMI");
        assert_eq!(attempts.load(Ordering::SeqCst), 4);

        let reqs = recorded.lock().unwrap();
        assert_eq!(
            reqs.last().unwrap().if_none_match.as_deref(),
            Some("\"etag-sodium-v1\"")
        );
    }
}
