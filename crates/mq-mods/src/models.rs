//! Shared domain models for Modrinth API v2, CurseForge Core API v1,
//! recursive dependency resolution, modpack installation, and local `.jar` scanning.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use mq_core::{ModLoaderKind, MqError, MqResult, Sha1Digest, Sha512Digest};
use serde::{Deserialize, Serialize};

/// Mandatory User-Agent header sent on all Modrinth and CurseForge HTTP requests.
pub const MQ_USER_AGENT: &str =
    concat!("mq-launcher/", env!("CARGO_PKG_VERSION"), " (https://github.com/MrQuster/mq)");

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

/// Parameters for searching Modrinth or CurseForge catalogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub project_kind: ProjectKind,
    pub mc_version: Option<String>,
    pub loader: Option<ModLoaderKind>,
    pub limit: u32,
    pub offset: u32,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            project_kind: ProjectKind::Mod,
            mc_version: None,
            loader: None,
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
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha1: Option<Sha1Digest>,
    pub sha512: Option<Sha512Digest>,
    pub primary: bool,
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
