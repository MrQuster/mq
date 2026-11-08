//! Shared domain models for Modrinth API v2, CurseForge Core API v1,
//! recursive dependency resolution, modpack installation, and local `.jar` scanning.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use mq_core::{ModLoaderKind, MqError, MqResult, Sha1Digest, Sha512Digest};
use serde::{Deserialize, Serialize};

/// Mandatory User-Agent header sent on all Modrinth and CurseForge HTTP requests.
pub const MQ_USER_AGENT: &str = concat!(
    "MrQuster/mq/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/MrQuster/mq)"
);

/// Upstream mod ecosystem catalog provider.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CatalogSource {
    #[default]
    #[serde(alias = "Modrinth", alias = "MODRINTH", alias = "mr")]
    Modrinth,
    #[serde(
        alias = "CurseForge",
        alias = "CURSEFORGE",
        alias = "curse-forge",
        alias = "curse_forge",
        alias = "cf"
    )]
    CurseForge,
}

/// Alias for [`CatalogSource`] per unified catalog contract.
pub type SourceId = CatalogSource;

impl CatalogSource {
    /// Returns the canonical lowercase string identifier for this catalog source.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
        }
    }
}

impl fmt::Display for CatalogSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CatalogSource {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "modrinth" | "mr" => Ok(Self::Modrinth),
            "curseforge" | "curse-forge" | "curse_forge" | "cf" => Ok(Self::CurseForge),
            other => Err(MqError::Metadata(format!(
                "unknown catalog source '{other}' (expected 'modrinth' or 'curseforge')"
            ))),
        }
    }
}

/// Origin of an installed content file in `.mq-lock.json`.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum LockSource {
    #[default]
    #[serde(alias = "Modrinth", alias = "MODRINTH", alias = "mr")]
    Modrinth,
    #[serde(
        alias = "CurseForge",
        alias = "CURSEFORGE",
        alias = "curse-forge",
        alias = "curse_forge",
        alias = "cf"
    )]
    CurseForge,
    #[serde(alias = "Manual", alias = "MANUAL", alias = "local")]
    Manual,
}

impl LockSource {
    /// Returns the canonical lowercase string identifier (`"modrinth"`, `"curseforge"`, `"manual"`).
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
            Self::Manual => "manual",
        }
    }

    /// Converts this lock source to a [`CatalogSource`] if it came from a remote catalog.
    #[inline]
    pub const fn as_catalog_source(&self) -> Option<CatalogSource> {
        match self {
            Self::Modrinth => Some(CatalogSource::Modrinth),
            Self::CurseForge => Some(CatalogSource::CurseForge),
            Self::Manual => None,
        }
    }
}

impl From<CatalogSource> for LockSource {
    #[inline]
    fn from(source: CatalogSource) -> Self {
        match source {
            CatalogSource::Modrinth => Self::Modrinth,
            CatalogSource::CurseForge => Self::CurseForge,
        }
    }
}

impl fmt::Display for LockSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LockSource {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "modrinth" | "mr" => Ok(Self::Modrinth),
            "curseforge" | "curse-forge" | "curse_forge" | "cf" => Ok(Self::CurseForge),
            "manual" | "local" => Ok(Self::Manual),
            other => Err(MqError::Metadata(format!(
                "unknown lock source '{other}' (expected 'modrinth', 'curseforge', or 'manual')"
            ))),
        }
    }
}

/// UI and CLI filter for selecting catalog providers (`All`, `Modrinth`, `CurseForge`).
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CatalogSourceFilter {
    #[default]
    #[serde(alias = "All", alias = "ALL")]
    All,
    #[serde(alias = "Modrinth", alias = "MODRINTH", alias = "mr")]
    Modrinth,
    #[serde(
        alias = "CurseForge",
        alias = "CURSEFORGE",
        alias = "curse-forge",
        alias = "curse_forge",
        alias = "cf"
    )]
    CurseForge,
}

impl CatalogSourceFilter {
    /// Returns the canonical lowercase string identifier (`"all"`, `"modrinth"`, `"curseforge"`).
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
        }
    }

    /// Returns `true` if `source` matches this filter.
    #[inline]
    pub const fn includes(&self, source: CatalogSource) -> bool {
        match self {
            Self::All => true,
            Self::Modrinth => matches!(source, CatalogSource::Modrinth),
            Self::CurseForge => matches!(source, CatalogSource::CurseForge),
        }
    }

    /// Returns the single [`CatalogSource`] if this filter is pinned to one provider.
    #[inline]
    pub const fn as_single_source(&self) -> Option<CatalogSource> {
        match self {
            Self::All => None,
            Self::Modrinth => Some(CatalogSource::Modrinth),
            Self::CurseForge => Some(CatalogSource::CurseForge),
        }
    }
}

impl From<CatalogSource> for CatalogSourceFilter {
    #[inline]
    fn from(source: CatalogSource) -> Self {
        match source {
            CatalogSource::Modrinth => Self::Modrinth,
            CatalogSource::CurseForge => Self::CurseForge,
        }
    }
}

impl fmt::Display for CatalogSourceFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CatalogSourceFilter {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "all" | "*" => Ok(Self::All),
            "modrinth" | "mr" => Ok(Self::Modrinth),
            "curseforge" | "curse-forge" | "curse_forge" | "cf" => Ok(Self::CurseForge),
            other => Err(MqError::Metadata(format!(
                "unknown catalog source filter '{other}' (expected 'all', 'modrinth', or 'curseforge')"
            ))),
        }
    }
}

/// Content classification across Modrinth (`project_type`) and CurseForge (`classId`).
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    #[default]
    #[serde(alias = "Mod", alias = "MOD", alias = "mods")]
    Mod,
    #[serde(alias = "Modpack", alias = "MODPACK", alias = "modpacks")]
    Modpack,
    #[serde(
        rename = "resourcepack",
        alias = "ResourcePack",
        alias = "RESOURCEPACK",
        alias = "resource_pack",
        alias = "resource-pack",
        alias = "resourcepacks"
    )]
    ResourcePack,
    #[serde(
        alias = "Shader",
        alias = "SHADER",
        alias = "shaders",
        alias = "shaderpack"
    )]
    Shader,
}

/// Alias for [`ProjectKind`] per unified catalog contract.
pub type ContentKind = ProjectKind;

impl ProjectKind {
    /// Returns the Modrinth API v2 `project_type` facet string (`"mod"`, `"modpack"`, `"resourcepack"`, `"shader"`).
    #[inline]
    pub const fn as_modrinth_project_type(&self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
        }
    }

    /// Parses a Modrinth `project_type` value into a [`ProjectKind`].
    #[inline]
    pub fn from_modrinth_project_type(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mod" | "mods" | "plugin" | "datapack" => Some(Self::Mod),
            "modpack" | "modpacks" => Some(Self::Modpack),
            "resourcepack" | "resource_pack" | "resource-pack" | "resourcepacks" => {
                Some(Self::ResourcePack)
            }
            "shader" | "shaders" | "shaderpack" => Some(Self::Shader),
            _ => None,
        }
    }

    /// Returns the CurseForge Core API v1 `classId` for Minecraft (`gameId = 432`).
    #[inline]
    pub const fn curseforge_class_id(&self) -> u32 {
        match self {
            Self::Mod => 6,
            Self::Modpack => 4471,
            Self::ResourcePack => 12,
            Self::Shader => 6552,
        }
    }

    /// Converts a CurseForge `classId` into a [`ProjectKind`].
    #[inline]
    pub const fn from_curseforge_class_id(class_id: u32) -> Option<Self> {
        match class_id {
            6 => Some(Self::Mod),
            4471 => Some(Self::Modpack),
            12 => Some(Self::ResourcePack),
            6552 => Some(Self::Shader),
            _ => None,
        }
    }

    /// Returns the target subfolder inside an instance directory (`"mods"`, `"resourcepacks"`, `"shaderpacks"`, or `""` for modpacks).
    #[inline]
    pub const fn target_subfolder(&self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::ResourcePack => "resourcepacks",
            Self::Shader => "shaderpacks",
            Self::Modpack => "",
        }
    }
}

impl fmt::Display for ProjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_modrinth_project_type())
    }
}

impl FromStr for ProjectKind {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        Self::from_modrinth_project_type(s).ok_or_else(|| {
            MqError::Metadata(format!(
                "unknown project kind '{s}' (expected 'mod', 'modpack', 'resourcepack', or 'shader')"
            ))
        })
    }
}

/// Catalog search sort order across Modrinth and CurseForge.
#[derive(
    Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SearchSort {
    #[default]
    #[serde(alias = "Relevance", alias = "RELEVANCE", alias = "featured")]
    Relevance,
    #[serde(alias = "Downloads", alias = "DOWNLOADS", alias = "total_downloads")]
    Downloads,
    #[serde(
        alias = "Follows",
        alias = "FOLLOWS",
        alias = "followers",
        alias = "popularity"
    )]
    Follows,
    #[serde(alias = "Newest", alias = "NEWEST", alias = "published")]
    Newest,
    #[serde(alias = "Updated", alias = "UPDATED", alias = "last_updated")]
    Updated,
}

impl SearchSort {
    /// Returns the canonical lowercase string identifier for this sort order.
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

impl fmt::Display for SearchSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SearchSort {
    type Err = MqError;

    fn from_str(s: &str) -> MqResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "relevance" | "featured" => Ok(Self::Relevance),
            "downloads" | "total_downloads" => Ok(Self::Downloads),
            "follows" | "followers" | "popularity" => Ok(Self::Follows),
            "newest" | "published" => Ok(Self::Newest),
            "updated" | "last_updated" => Ok(Self::Updated),
            other => Err(MqError::Metadata(format!(
                "unknown search sort '{other}' (expected 'relevance', 'downloads', 'follows', 'newest', or 'updated')"
            ))),
        }
    }
}

/// Parameters for searching Modrinth or CurseForge catalogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    #[serde(default)]
    pub project_kind: ProjectKind,
    #[serde(default)]
    pub mc_version: Option<String>,
    #[serde(default)]
    pub loader: Option<ModLoaderKind>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub sort: SearchSort,
    #[serde(default = "default_search_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

const fn default_search_limit() -> u32 {
    20
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            project_kind: ProjectKind::Mod,
            mc_version: None,
            loader: None,
            categories: Vec::new(),
            sort: SearchSort::Relevance,
            limit: 20,
            offset: 0,
        }
    }
}

impl SearchQuery {
    /// Constructs a new [`SearchQuery`] for mods with default pagination (`limit = 20`, `offset = 0`).
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    /// Sets the target [`ProjectKind`].
    pub fn with_project_kind(mut self, project_kind: ProjectKind) -> Self {
        self.project_kind = project_kind;
        self
    }

    /// Sets the target Minecraft version filter.
    pub fn with_mc_version(mut self, mc_version: impl Into<String>) -> Self {
        self.mc_version = Some(mc_version.into());
        self
    }

    /// Sets the target mod loader filter.
    pub fn with_loader(mut self, loader: ModLoaderKind) -> Self {
        self.loader = Some(loader);
        self
    }

    /// Appends a single category filter.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        let cat = category.into();
        let trimmed = cat.trim();
        if !trimmed.is_empty() && !self.categories.iter().any(|c| c == trimmed) {
            self.categories.push(trimmed.to_owned());
        }
        self
    }

    /// Sets the category filter list.
    pub fn with_categories<I, S>(mut self, categories: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.categories = categories
            .into_iter()
            .map(Into::into)
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .collect();
        self
    }

    /// Sets the search sort order.
    pub fn with_sort(mut self, sort: SearchSort) -> Self {
        self.sort = sort;
        self
    }

    /// Sets the maximum number of search hits to return.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Sets the pagination offset.
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = offset;
        self
    }
}

/// Normalized catalog search result entry from Modrinth or CurseForge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogHit {
    pub source: CatalogSource,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub project_kind: ProjectKind,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub latest_version: Option<String>,
}

impl CatalogHit {
    /// Converts a summary [`CatalogHit`] into an extended [`ProjectDetails`] card.
    pub fn into_project_details(self) -> ProjectDetails {
        ProjectDetails::from(self)
    }
}

/// Screenshot or gallery image attached to a [`ProjectDetails`] card.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GalleryItem {
    pub url: String,
    #[serde(default)]
    pub raw_url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub featured: bool,
}

/// Extended project details card (`Project`) combining Modrinth and CurseForge metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDetails {
    pub source: CatalogSource,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub body: Option<String>,
    pub author: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub gallery: Vec<GalleryItem>,
    #[serde(default)]
    pub project_kind: ProjectKind,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub client_side: Option<String>,
    #[serde(default)]
    pub server_side: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
}

/// Alias for [`ProjectDetails`] per unified catalog contract.
pub type Project = ProjectDetails;

impl From<CatalogHit> for ProjectDetails {
    fn from(hit: CatalogHit) -> Self {
        Self {
            source: hit.source,
            project_id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            description: hit.description,
            body: None,
            author: hit.author,
            downloads: hit.downloads,
            followers: 0,
            icon_url: hit.icon_url,
            gallery: Vec::new(),
            project_kind: hit.project_kind,
            loaders: hit.loaders,
            game_versions: hit.game_versions,
            categories: Vec::new(),
            latest_version: hit.latest_version,
            license: None,
            client_side: None,
            server_side: None,
            source_url: None,
            website_url: None,
        }
    }
}

impl ProjectDetails {
    /// Extracts the summary [`CatalogHit`] from this [`ProjectDetails`] card.
    pub fn into_catalog_hit(self) -> CatalogHit {
        CatalogHit {
            source: self.source,
            project_id: self.project_id,
            slug: self.slug,
            title: self.title,
            description: self.description,
            author: self.author,
            downloads: self.downloads,
            icon_url: self.icon_url,
            project_kind: self.project_kind,
            loaders: self.loaders,
            game_versions: self.game_versions,
            latest_version: self.latest_version,
        }
    }
}

/// Multi-algorithm cryptographic digest and CurseForge Murmur2 fingerprint bundle.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hashes {
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub sha512: Option<Sha512Digest>,
    #[serde(default)]
    pub md5: Option<String>,
    #[serde(default)]
    pub murmur2_fingerprint: Option<u32>,
}

/// Dependency relationship kind between two mod versions.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

impl DependencyKind {
    /// Parses a Modrinth API v2 `dependency_type` string.
    #[inline]
    pub fn from_modrinth_type(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "required" => Some(Self::Required),
            "optional" => Some(Self::Optional),
            "incompatible" => Some(Self::Incompatible),
            "embedded" => Some(Self::Embedded),
            _ => None,
        }
    }

    /// Converts a CurseForge Core API v1 `FileRelationType` integer (`3 = RequiredDependency`).
    #[inline]
    pub const fn from_curseforge_relation_type(relation_type: u32) -> Option<Self> {
        match relation_type {
            1 | 6 => Some(Self::Embedded),
            2 | 4 => Some(Self::Optional),
            3 => Some(Self::Required),
            5 => Some(Self::Incompatible),
            _ => None,
        }
    }
}

/// Reference to a dependency declared by a [`ProjectVersion`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDependencyRef {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub file_name: Option<String>,
    pub kind: DependencyKind,
}

/// Downloadable artifact file attached to a [`ProjectVersion`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectVersionFile {
    #[serde(default)]
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub sha512: Option<Sha512Digest>,
    #[serde(default)]
    pub primary: bool,
}

/// Alias for [`ProjectVersionFile`] per unified catalog contract.
pub type ContentFile = ProjectVersionFile;

impl ProjectVersionFile {
    /// Returns `true` when automatic download is blocked (`downloadUrl == null` on CurseForge).
    #[inline]
    pub fn is_download_blocked(&self) -> bool {
        self.url.trim().is_empty()
    }

    /// Returns the non-empty download URL if automatic download is allowed.
    #[inline]
    pub fn download_url(&self) -> Option<&str> {
        let trimmed = self.url.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    }
}

/// Descriptor for a CurseForge file whose direct CDN download is blocked (`downloadUrl == null`),
/// requiring manual browser download from `{websiteUrl}/download/{fileId}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedDownload {
    pub project_id: String,
    pub file_id: String,
    #[serde(default)]
    pub project_title: String,
    pub filename: String,
    pub webpage_url: String,
    #[serde(default)]
    pub expected_size: u64,
    #[serde(default)]
    pub expected_sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub expected_fingerprint: Option<u32>,
    pub target_rel_path: String,
}

/// Normalized release version of a project on Modrinth or CurseForge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ProjectVersionFile>,
    pub dependencies: Vec<ModDependencyRef>,
}

impl ProjectVersion {
    /// Returns the primary downloadable file for this version, or the first file if none is marked primary.
    #[inline]
    pub fn primary_file(&self) -> Option<&ProjectVersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
}

/// Concrete download plan item produced by the recursive dependency resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedModDownload {
    pub project_id: String,
    pub version_id: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub sha1: Option<Sha1Digest>,
    pub sha512: Option<Sha512Digest>,
    pub is_dependency: bool,
}

impl ResolvedModDownload {
    /// Returns `true` if this resolved artifact has no direct download URL (`downloadUrl == null`).
    #[inline]
    pub fn is_download_blocked(&self) -> bool {
        self.url.trim().is_empty()
    }
}

/// Metadata and cryptographic digests for a locally installed `.jar` or `.jar.disabled` mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledModEntry {
    pub file_name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub loader: Option<ModLoaderKind>,
    pub dependencies: Vec<String>,
    pub sha1: Sha1Digest,
    pub sha512: Sha512Digest,
    pub size: u64,
}

/// Conflict or missing requirement detected when scanning an instance's `mods/` directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModConflict {
    DuplicateModId {
        mod_id: String,
        files: Vec<String>,
    },
    MissingDependency {
        mod_id: String,
        file_name: String,
        required_dep: String,
    },
}

/// Summary report produced by scanning an instance's local `mods/` folder.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModScanReport {
    pub mods: Vec<InstalledModEntry>,
    pub conflicts: Vec<ModConflict>,
    pub scan_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_kind_target_subfolder_and_ids() {
        assert_eq!(ProjectKind::Mod.target_subfolder(), "mods");
        assert_eq!(
            ProjectKind::ResourcePack.target_subfolder(),
            "resourcepacks"
        );
        assert_eq!(ProjectKind::Shader.target_subfolder(), "shaderpacks");
        assert_eq!(ProjectKind::Modpack.target_subfolder(), "");

        assert_eq!(ProjectKind::Mod.curseforge_class_id(), 6);
        assert_eq!(ProjectKind::Modpack.curseforge_class_id(), 4471);
        assert_eq!(ProjectKind::ResourcePack.curseforge_class_id(), 12);
        assert_eq!(ProjectKind::Shader.curseforge_class_id(), 6552);
    }

    #[test]
    fn lock_source_and_catalog_filter_roundtrip() {
        assert_eq!(
            "modrinth".parse::<LockSource>().unwrap(),
            LockSource::Modrinth
        );
        assert_eq!(
            "curseforge".parse::<LockSource>().unwrap(),
            LockSource::CurseForge
        );
        assert_eq!("manual".parse::<LockSource>().unwrap(), LockSource::Manual);
        assert_eq!(LockSource::Manual.as_catalog_source(), None);
        assert_eq!(
            LockSource::from(CatalogSource::CurseForge),
            LockSource::CurseForge
        );

        assert!(CatalogSourceFilter::All.includes(CatalogSource::Modrinth));
        assert!(CatalogSourceFilter::All.includes(CatalogSource::CurseForge));
        assert!(CatalogSourceFilter::Modrinth.includes(CatalogSource::Modrinth));
        assert!(!CatalogSourceFilter::Modrinth.includes(CatalogSource::CurseForge));
        assert_eq!(
            "cf".parse::<CatalogSourceFilter>().unwrap(),
            CatalogSourceFilter::CurseForge
        );
    }

    #[test]
    fn search_query_serde_defaults_and_builders() {
        let legacy_json = r#"{
            "query": "sodium",
            "project_kind": "mod",
            "mc_version": "26.1",
            "loader": "Fabric",
            "limit": 20,
            "offset": 0
        }"#;
        let parsed: SearchQuery = serde_json::from_str(legacy_json).expect("parse legacy query");
        assert_eq!(parsed.query, "sodium");
        assert_eq!(parsed.mc_version.as_deref(), Some("26.1"));
        assert!(parsed.categories.is_empty());
        assert_eq!(parsed.sort, SearchSort::Relevance);

        let built = SearchQuery::new("iris")
            .with_project_kind(ProjectKind::Shader)
            .with_category("optimization")
            .with_sort(SearchSort::Downloads);
        assert_eq!(built.categories, vec!["optimization"]);
        assert_eq!(built.sort, SearchSort::Downloads);
    }

    #[test]
    fn project_version_file_detects_blocked_download() {
        let blocked = ProjectVersionFile {
            url: String::new(),
            filename: "sodium.jar".into(),
            size: 1024,
            sha1: None,
            sha512: None,
            primary: true,
        };
        assert!(blocked.is_download_blocked());
        assert_eq!(blocked.download_url(), None);

        let allowed = ProjectVersionFile {
            url: "https://cdn.modrinth.com/data/AANobbMI/versions/v1/sodium.jar".into(),
            ..blocked
        };
        assert!(!allowed.is_download_blocked());
        assert!(allowed.download_url().is_some());
    }
}
