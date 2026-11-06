//! Shared domain models for `mq-meta` (Vanilla manifests, rules, asset index, and modloader profiles).

use std::collections::BTreeMap;

use mq_core::{ModLoaderKind, Sha1Digest};
use serde::{Deserialize, Serialize};

/// Summary entry in Mojang's `version_manifest_v2.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub release_type: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default, rename = "releaseTime")]
    pub release_time: Option<String>,
}

/// Top-level `version_manifest_v2.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    #[serde(default)]
    pub snapshot: String,
}

/// Target environment used to evaluate Minecraft OS/arch/feature rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleContext {
    pub os_name: String,
    pub os_arch: String,
    pub features: BTreeMap<String, bool>,
}

impl Default for RuleContext {
    fn default() -> Self {
        Self::linux_x86_64()
    }
}

impl RuleContext {
    pub fn linux_x86_64() -> Self {
        Self {
            os_name: "linux".to_string(),
            os_arch: "x86_64".to_string(),
            features: BTreeMap::new(),
        }
    }
}

/// Required Java runtime version metadata from `version.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVersionSpec {
    #[serde(default = "default_java_component")]
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

fn default_java_component() -> String {
    "java-runtime-gamma".to_string()
}

/// Resolved artifact download metadata (deterministic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDownload {
    pub path: String,
    pub url: String,
    pub sha1: Option<Sha1Digest>,
    pub size: u64,
}

/// Resolved library entry in deterministic manifest order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLibrary {
    /// Full Maven coordinate (`group:artifact:version[:classifier]`).
    pub name: String,
    /// Primary JAR artifact (if present).
    pub artifact: Option<ArtifactDownload>,
    /// Native Linux classifier artifact (if applicable on Linux).
    pub native_linux: Option<ArtifactDownload>,
    /// Patterns to exclude when extracting natives (e.g. `META-INF/`).
    pub extract_exclude: Vec<String>,
    /// Whether this library goes onto the JVM `-cp` classpath.
    pub include_in_classpath: bool,
}

impl ResolvedLibrary {
    /// Returns `(group, artifact, classifier)` key used for deterministic loader override deduplication.
    pub fn module_key(&self) -> (String, String, Option<String>) {
        // Maven coordinate format: group:artifact:version[:classifier] (ignoring @ext suffix)
        let clean = self.name.split('@').next().unwrap_or(&self.name);
        let parts: Vec<&str> = clean.split(':').collect();
        let group = parts.first().copied().unwrap_or("").to_string();
        let artifact = parts.get(1).copied().unwrap_or("").to_string();
        let classifier = parts.get(3).map(|s| (*s).to_string());
        (group, artifact, classifier)
    }
}

/// Asset index pointer in `version.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndexPointer {
    pub id: String,
    pub sha1: Option<Sha1Digest>,
    pub size: u64,
    #[serde(default, rename = "totalSize")]
    pub total_size: u64,
    pub url: String,
}

/// Fully resolved, deterministic Minecraft + Modloader version specification (Lesson E8).
/// Wrapped in `Arc<ResolvedVersion>` when shared across worker pools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedVersion {
    pub id: String,
    pub mc_version: String,
    pub loader: ModLoaderKind,
    pub loader_version: Option<String>,
    pub inherits_from: Option<String>,
    pub main_class: String,
    pub java_version: JavaVersionSpec,
    pub asset_index: AssetIndexPointer,
    pub client_jar: ArtifactDownload,
    /// Ordered list of libraries applicable to the target OS/arch.
    pub libraries: Vec<ResolvedLibrary>,
    /// Ordered JVM arguments (with `${...}` placeholders preserved for `mq-launch`).
    pub jvm_arguments: Vec<String>,
    /// Ordered game arguments (with `${...}` placeholders preserved for `mq-launch`).
    pub game_arguments: Vec<String>,
}

/// Single asset object in `assets/indexes/<id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObject {
    /// Logical asset name (e.g. `"minecraft/sounds/ambient/cave/cave1.ogg"`).
    pub name: String,
    pub hash: Sha1Digest,
    pub size: u64,
}

impl AssetObject {
    /// Relative path inside `<assets>/objects/<2-hex>/<40-hex>`.
    pub fn relative_object_path(&self) -> String {
        let hex = self.hash.to_hex();
        let prefix = &hex[..2];
        format!("{prefix}/{hex}")
    }

    /// Download URL from base resources URL (e.g. `https://resources.download.minecraft.net`).
    pub fn download_url(&self, base_resources_url: &str) -> String {
        let base = base_resources_url.trim_end_matches('/');
        format!("{base}/{}", self.relative_object_path())
    }
}

/// Deterministic parsed Asset Index (sorted by logical name for reproducible iteration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    /// Deduplicated and deterministically ordered list of asset objects.
    pub objects: Vec<AssetObject>,
}
