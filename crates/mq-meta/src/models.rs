//! Shared domain models for `mq-meta` (Vanilla manifests, rules, asset index, and modloader profiles).

use std::collections::BTreeMap;

use mq_core::{ModLoaderKind, Sha1Digest};
use serde::{Deserialize, Serialize};

/// Summary entry in Mojang's `version_manifest_v2.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifestEntry {
    pub id: String,
    #[serde(default, rename = "type")]
    pub release_type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default, rename = "releaseTime")]
    pub release_time: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default, rename = "complianceLevel")]
    pub compliance_level: Option<i32>,
}

impl VersionManifestEntry {
    pub fn is_release(&self) -> bool {
        self.release_type.eq_ignore_ascii_case("release")
    }

    pub fn is_snapshot(&self) -> bool {
        self.release_type.eq_ignore_ascii_case("snapshot")
    }

    pub fn is_old_beta(&self) -> bool {
        self.release_type.eq_ignore_ascii_case("old_beta")
    }

    pub fn is_old_alpha(&self) -> bool {
        self.release_type.eq_ignore_ascii_case("old_alpha")
    }
}

/// Top-level `version_manifest_v2.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    #[serde(default)]
    pub versions: Vec<VersionManifestEntry>,
}

impl VersionManifest {
    /// Sorts versions chronologically by `releaseTime` descending (newest first).
    pub fn sort_by_release_time_desc(&mut self) {
        self.versions.sort_by(|a, b| {
            let ta = a
                .release_time
                .as_deref()
                .or(a.time.as_deref())
                .unwrap_or("");
            let tb = b
                .release_time
                .as_deref()
                .or(b.time.as_deref())
                .unwrap_or("");
            tb.cmp(ta)
        });
    }

    /// Sorts versions chronologically by `releaseTime` ascending (oldest first).
    pub fn sort_by_release_time_asc(&mut self) {
        self.versions.sort_by(|a, b| {
            let ta = a
                .release_time
                .as_deref()
                .or(a.time.as_deref())
                .unwrap_or("");
            let tb = b
                .release_time
                .as_deref()
                .or(b.time.as_deref())
                .unwrap_or("");
            ta.cmp(tb)
        });
    }

    /// Filters versions by release type (`"release"`, `"snapshot"`, `"old_beta"`, `"old_alpha"`).
    pub fn filter_by_type(&self, release_type: &str) -> Vec<&VersionManifestEntry> {
        self.versions
            .iter()
            .filter(|v| v.release_type.eq_ignore_ascii_case(release_type))
            .collect()
    }
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
    pub os_version: Option<String>,
    pub features: BTreeMap<String, bool>,
}

impl Default for RuleContext {
    fn default() -> Self {
        Self::current()
    }
}

impl RuleContext {
    pub fn linux_x86_64() -> Self {
        Self {
            os_name: "linux".to_string(),
            os_arch: "x86_64".to_string(),
            os_version: None,
            features: BTreeMap::new(),
        }
    }

    pub fn linux_arm64() -> Self {
        Self {
            os_name: "linux".to_string(),
            os_arch: "arm64".to_string(),
            os_version: None,
            features: BTreeMap::new(),
        }
    }

    pub fn windows_x86_64() -> Self {
        Self {
            os_name: "windows".to_string(),
            os_arch: "x86_64".to_string(),
            os_version: Some("10.0".to_string()),
            features: BTreeMap::new(),
        }
    }

    pub fn windows_arm64() -> Self {
        Self {
            os_name: "windows".to_string(),
            os_arch: "arm64".to_string(),
            os_version: Some("10.0".to_string()),
            features: BTreeMap::new(),
        }
    }

    pub fn macos_arm64() -> Self {
        Self {
            os_name: "osx".to_string(),
            os_arch: "arm64".to_string(),
            os_version: None,
            features: BTreeMap::new(),
        }
    }

    pub fn macos_x86_64() -> Self {
        Self {
            os_name: "osx".to_string(),
            os_arch: "x86_64".to_string(),
            os_version: None,
            features: BTreeMap::new(),
        }
    }

    pub fn current() -> Self {
        let os = mq_core::platform::os_name();
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "arm64",
            "x86" => "x86",
            other => other,
        };
        let os_version = if os == "windows" {
            Some("10.0".to_string())
        } else {
            None
        };
        Self {
            os_name: os.to_string(),
            os_arch: arch.to_string(),
            os_version,
            features: BTreeMap::new(),
        }
    }
}

/// Required Java runtime version metadata from `version.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVersionSpec {
    #[serde(default = "default_java_component")]
    pub component: String,
    #[serde(default, rename = "majorVersion")]
    pub major_version: u32,
}

fn default_java_component() -> String {
    "java-runtime-gamma".to_string()
}

/// Resolved artifact download metadata (deterministic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ArtifactDownload {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AssetIndexPointer {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub size: u64,
    #[serde(default, rename = "totalSize")]
    pub total_size: u64,
    #[serde(default)]
    pub url: String,
}

/// Resolved client logging configuration from `version.json` (`logging.client`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub client: Option<LoggingClientConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LoggingClientConfig {
    /// JVM argument template (e.g. `"-Dlog4j.configurationFile=${path}"`).
    #[serde(default)]
    pub argument: String,
    /// Downloadable logging configuration file.
    #[serde(default)]
    pub file: ArtifactDownload,
    /// Type of logging config (e.g. `"log4j2-xml"`).
    #[serde(default, rename = "type")]
    pub logging_type: String,
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
    /// Client logging configuration (`logging.client`), if present.
    #[serde(default)]
    pub logging: Option<LoggingConfig>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetIndex {
    pub id: String,
    pub is_virtual: bool,
    pub map_to_resources: bool,
    /// Deduplicated and deterministically ordered list of asset objects.
    pub objects: Vec<AssetObject>,
}
