//! Fabric, Quilt, Forge, NeoForge and local `inheritsFrom` modloader profile
//! metadata resolution, Maven coordinate artifact mapping, and library duplicate resolution.

use std::collections::{BTreeMap, HashSet};

use mq_core::{ModLoaderKind, MqError, MqResult, Sha1Digest};
use serde::{de, Deserialize, Deserializer, Serialize};
use url::Url;

use crate::assets::ClientLoggingConfig;
use crate::models::{ArtifactDownload, JavaVersionSpec, ResolvedLibrary, RuleContext};

/// Default base URL for the official Fabric Meta v2 API.
pub const FABRIC_META_BASE_URL: &str = "https://meta.fabricmc.net";

/// Default base URL for the official Quilt Meta v3 API.
pub const QUILT_META_BASE_URL: &str = "https://meta.quiltmc.org";

/// Default Maven repository URL for Fabric artifacts.
pub const FABRIC_MAVEN_URL: &str = "https://maven.fabricmc.net";

/// Default Maven repository URL for Quilt artifacts.
pub const QUILT_MAVEN_URL: &str = "https://maven.quiltmc.org/repository/release";

/// Default Maven repository URL for Minecraft Forge artifacts.
pub const FORGE_MAVEN_URL: &str = "https://maven.minecraftforge.net";

/// Default Maven repository URL for NeoForged artifacts.
pub const NEOFORGE_MAVEN_URL: &str = "https://maven.neoforged.net/releases";

/// Fallback Maven repository URL when a profile library omits both `downloads.artifact` and `url`.
pub const DEFAULT_LIBRARIES_BASE_URL: &str = "https://libraries.minecraft.net";

/// Returns the standard default Maven repository base URL for a given modloader kind.
pub fn default_repo_for_loader(loader: ModLoaderKind) -> &'static str {
    match loader {
        ModLoaderKind::Fabric => FABRIC_MAVEN_URL,
        ModLoaderKind::Quilt => QUILT_MAVEN_URL,
        ModLoaderKind::Forge => FORGE_MAVEN_URL,
        ModLoaderKind::NeoForge => NEOFORGE_MAVEN_URL,
        ModLoaderKind::Vanilla => DEFAULT_LIBRARIES_BASE_URL,
    }
}

/// Returns the recommended Maven repository base URL based on Maven coordinate group and loader context.
pub fn default_repo_for_coordinate(coord: &str, loader: ModLoaderKind) -> &'static str {
    let clean = coord.split('@').next().unwrap_or(coord).trim();
    let group = clean.split(':').next().unwrap_or("");
    if group.starts_with("net.fabricmc") {
        FABRIC_MAVEN_URL
    } else if group.starts_with("org.quiltmc") {
        QUILT_MAVEN_URL
    } else if group.starts_with("net.minecraftforge") {
        FORGE_MAVEN_URL
    } else if group.starts_with("net.neoforged") {
        NEOFORGE_MAVEN_URL
    } else {
        default_repo_for_loader(loader)
    }
}

/// Parsed and resolved representation of a modloader `version.json` / `profile/json`
/// ready to be deterministically merged onto a parent [` crate::models::ResolvedVersion `].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderProfile {
    pub id: String,
    pub loader: ModLoaderKind,
    pub loader_version: Option<String>,
    pub inherits_from: Option<String>,
    pub main_class: String,
    pub libraries: Vec<ResolvedLibrary>,
    pub jvm_arguments: Vec<String>,
    pub game_arguments: Vec<String>,
    #[serde(default)]
    pub asset_index: Option<crate::models::AssetIndexPointer>,
    #[serde(default)]
    pub client_jar: Option<ArtifactDownload>,
    #[serde(default)]
    pub java_version: Option<JavaVersionSpec>,
    #[serde(default)]
    pub logging: Option<ClientLoggingConfig>,
}

impl LoaderProfile {
    /// Deterministically merges this child loader profile onto a parent [`crate::models::ResolvedVersion`].
    /// Cascades `asset_index`, `assets`, `downloads` (`client_jar`), `java_version`, `logging`,
    /// and resolves libraries with child precedence.
    pub fn merge_with_parent(
        &self,
        parent: &crate::models::ResolvedVersion,
    ) -> MqResult<crate::models::ResolvedVersion> {
        crate::merge::merge_inherits_from(parent, self)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawLoaderAssetIndexPointer {
    id: String,
    #[serde(default)]
    sha1: Option<Sha1Digest>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default, rename = "totalSize")]
    total_size: Option<u64>,
    #[serde(default)]
    url: Option<String>,
}

/// Raw library entry inside a Fabric, Quilt, Forge, or NeoForge profile JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoaderLibrary {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_sha1")]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub downloads: Option<RawLoaderDownloads>,
    #[serde(default)]
    pub rules: Option<Vec<RawLoaderRule>>,
    #[serde(default)]
    pub extract: Option<RawLoaderExtract>,
    #[serde(default)]
    pub natives: Option<BTreeMap<String, String>>,
}

/// Optional `downloads` block inside a Forge, NeoForge, or Mojang-style profile library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoaderDownloads {
    #[serde(default)]
    pub artifact: Option<RawLoaderArtifact>,
    #[serde(default)]
    pub classifiers: BTreeMap<String, RawLoaderArtifact>,
}

/// Raw download artifact descriptor inside `downloads.artifact` or `downloads.classifiers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoaderArtifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_sha1")]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub size: Option<u64>,
}

/// Optional native archive extraction rules on a profile library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoaderExtract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// OS/arch/feature condition rule on a profile library or argument entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoaderRule {
    pub action: String,
    #[serde(default)]
    pub os: Option<RawLoaderOsRule>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

/// OS matcher inside [`RawLoaderRule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLoaderOsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLoaderProfile {
    id: String,
    #[serde(default, rename = "inheritsFrom")]
    inherits_from: Option<String>,
    #[serde(default, rename = "mainClass")]
    main_class: Option<String>,
    #[serde(default, rename = "javaVersion")]
    java_version: Option<JavaVersionSpec>,
    #[serde(default, rename = "assetIndex")]
    asset_index: Option<RawLoaderAssetIndexPointer>,
    #[serde(default)]
    assets: Option<String>,
    #[serde(default)]
    arguments: Option<RawLoaderArguments>,
    #[serde(default, rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
    #[serde(default)]
    libraries: Vec<RawLoaderLibrary>,
    #[serde(default)]
    downloads: Option<RawLoaderProfileDownloads>,
    #[serde(default)]
    logging: Option<RawLoaderProfileLogging>,
}

#[derive(Debug, Deserialize)]
struct RawLoaderProfileDownloads {
    #[serde(default)]
    client: Option<RawLoaderArtifact>,
}

#[derive(Debug, Deserialize)]
struct RawLoaderProfileLogging {
    #[serde(default)]
    client: Option<ClientLoggingConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct RawLoaderArguments {
    #[serde(default)]
    jvm: Vec<RawArgumentEntry>,
    #[serde(default)]
    game: Vec<RawArgumentEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawArgumentEntry {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<RawLoaderRule>,
        value: RawArgumentValue,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LoaderVersionResponseEntry {
    Nested { loader: NestedLoaderVersion },
    Flat { version: String },
    Plain(String),
}

#[derive(Debug, Deserialize)]
struct NestedLoaderVersion {
    version: String,
}

fn deserialize_optional_sha1<'de, D>(deserializer: D) -> Result<Option<Sha1Digest>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(raw) if raw.trim().is_empty() => Ok(None),
        Some(raw) => Sha1Digest::from_hex(raw.trim())
            .map(Some)
            .map_err(de::Error::custom),
    }
}

fn is_safe_coord_token(token: &str) -> bool {
    !token.is_empty()
        && token != "."
        && token != ".."
        && !token.contains("..")
        && !token.contains('/')
        && !token.contains('\\')
        && !token.contains('\0')
}

fn validate_safe_relative_artifact_path(rel: &str) -> MqResult<String> {
    if rel.is_empty()
        || rel.contains('\0')
        || rel.starts_with('/')
        || rel.starts_with('\\')
        || rel.contains('\\')
    {
        return Err(MqError::ZipSlip {
            entry: rel.to_owned(),
        });
    }

    let bytes = rel.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(MqError::ZipSlip {
            entry: rel.to_owned(),
        });
    }

    let mut clean_segments = Vec::new();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(MqError::ZipSlip {
                entry: rel.to_owned(),
            });
        }
        clean_segments.push(seg);
    }

    Ok(clean_segments.join("/"))
}

/// Resolves a Maven coordinate (`group:artifact:version[:classifier][@ext]`) and an optional
/// Maven repository base URL into a deterministic [`ArtifactDownload`].
///
/// Strips `@jar` (or uses any explicit `@<ext>` suffix) and rejects malformed or path-traversal
/// coordinates.
pub fn maven_coord_to_artifact(
    name: &str,
    base_url: Option<&str>,
    sha1: Option<Sha1Digest>,
    size: u64,
) -> MqResult<ArtifactDownload> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MqError::Metadata("empty Maven coordinate".to_owned()));
    }

    let (coord_part, ext) = match trimmed.split_once('@') {
        Some((coord, raw_ext)) => {
            if !is_safe_coord_token(raw_ext) || raw_ext.contains(':') || raw_ext.contains('@') {
                return Err(MqError::Metadata(format!(
                    "invalid extension in Maven coordinate '{name}'"
                )));
            }
            (coord, raw_ext)
        }
        None => (trimmed, "jar"),
    };

    let parts: Vec<&str> = coord_part.split(':').collect();
    if parts.len() != 3 && parts.len() != 4 {
        return Err(MqError::Metadata(format!(
            "invalid Maven coordinate '{name}': expected group:artifact:version[:classifier]"
        )));
    }

    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();

    if group.split('.').any(|seg| !is_safe_coord_token(seg))
        || !is_safe_coord_token(artifact)
        || !is_safe_coord_token(version)
        || classifier.is_some_and(|c| !is_safe_coord_token(c))
    {
        return Err(MqError::Metadata(format!(
            "unsafe or empty segment in Maven coordinate '{name}'"
        )));
    }

    let group_path = group.replace('.', "/");
    let file_name = match classifier {
        Some(clf) => format!("{artifact}-{version}-{clf}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    let path = format!("{group_path}/{artifact}/{version}/{file_name}");

    let base = base_url
        .map(|u| u.trim().trim_end_matches('/'))
        .filter(|u| !u.is_empty())
        .unwrap_or(DEFAULT_LIBRARIES_BASE_URL);
    let url = format!("{base}/{path}");

    Ok(ArtifactDownload {
        path,
        url,
        sha1,
        size,
    })
}

impl RawLoaderLibrary {
    /// Returns `(group, artifact, classifier)` key matching [`ResolvedLibrary::module_key`].
    pub fn module_key(&self) -> (String, String, Option<String>) {
        let clean = self.name.split('@').next().unwrap_or(&self.name);
        let parts: Vec<&str> = clean.split(':').collect();
        let group = parts.first().copied().unwrap_or("").to_string();
        let artifact = parts.get(1).copied().unwrap_or("").to_string();
        let classifier = parts.get(3).map(|s| (*s).to_string());
        (group, artifact, classifier)
    }

    /// Converts this raw profile library entry into a [`ResolvedLibrary`] with default repository fallback.
    pub fn to_resolved_library(&self) -> MqResult<ResolvedLibrary> {
        self.to_resolved_library_for_loader(ModLoaderKind::Vanilla)
    }

    /// Converts this raw profile library entry into a [`ResolvedLibrary`] using the appropriate loader repository.
    pub fn to_resolved_library_for_loader(
        &self,
        loader: ModLoaderKind,
    ) -> MqResult<ResolvedLibrary> {
        let default_repo = default_repo_for_coordinate(&self.name, loader);
        let effective_repo = match self.url.as_deref().map(str::trim) {
            Some(u) if !u.is_empty() => Some(u),
            Some(_) => None, // preserve empty URL for processor artifacts
            None => Some(default_repo),
        };

        let coord_artifact = maven_coord_to_artifact(
            &self.name,
            effective_repo,
            self.sha1,
            self.size.unwrap_or(0),
        )?;

        let is_native_linux_coord = self.name.contains(":natives-linux")
            || self.name.ends_with(":natives-linux")
            || self.name.contains("natives-linux@");

        let (artifact, native_linux) = match &self.downloads {
            Some(downloads) => {
                let primary = if let Some(raw_art) = &downloads.artifact {
                    Some(resolve_raw_artifact(
                        raw_art,
                        &coord_artifact,
                        effective_repo,
                        self.sha1,
                        self.size,
                    )?)
                } else if !downloads.classifiers.is_empty() {
                    None
                } else {
                    Some(coord_artifact.clone())
                };

                let native_clf = self
                    .natives
                    .as_ref()
                    .and_then(|m| m.get("linux").cloned())
                    .unwrap_or_else(|| "natives-linux".to_string());

                let native = if let Some(raw_nat) = downloads
                    .classifiers
                    .get(&native_clf)
                    .or_else(|| downloads.classifiers.get("natives-linux"))
                {
                    let fallback_native = maven_coord_to_artifact(
                        &format!(
                            "{}:{native_clf}",
                            self.name.split('@').next().unwrap_or(&self.name)
                        ),
                        effective_repo,
                        None,
                        0,
                    )
                    .unwrap_or_else(|_| coord_artifact.clone());
                    Some(resolve_raw_artifact(
                        raw_nat,
                        &fallback_native,
                        effective_repo,
                        None,
                        None,
                    )?)
                } else if is_native_linux_coord {
                    primary.clone()
                } else {
                    None
                };

                (primary, native)
            }
            None => {
                if let Some(natives_map) = &self.natives {
                    let native_clf = natives_map
                        .get("linux")
                        .cloned()
                        .unwrap_or_else(|| "natives-linux".to_string());
                    let clean = self.name.split('@').next().unwrap_or(&self.name);
                    let native_coord = format!("{clean}:{native_clf}");
                    let native_artifact = maven_coord_to_artifact(
                        &native_coord,
                        effective_repo,
                        self.sha1,
                        self.size.unwrap_or(0),
                    )?;
                    (None, Some(native_artifact))
                } else if is_native_linux_coord {
                    (Some(coord_artifact.clone()), Some(coord_artifact))
                } else {
                    (Some(coord_artifact), None)
                }
            }
        };

        let extract_exclude = self
            .extract
            .as_ref()
            .map(|e| e.exclude.clone())
            .unwrap_or_default();

        let include_in_classpath = artifact.is_some() && self.extract.is_none();

        Ok(ResolvedLibrary {
            name: self.name.trim().to_owned(),
            artifact,
            native_linux,
            extract_exclude,
            include_in_classpath,
        })
    }
}

fn resolve_raw_artifact(
    raw_art: &RawLoaderArtifact,
    fallback: &ArtifactDownload,
    repo_base_url: Option<&str>,
    outer_sha1: Option<Sha1Digest>,
    outer_size: Option<u64>,
) -> MqResult<ArtifactDownload> {
    let path = match raw_art.path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => validate_safe_relative_artifact_path(p)?,
        _ => fallback.path.clone(),
    };

    let url = match raw_art.url.as_deref().map(str::trim) {
        Some(u) if !u.is_empty() => u.to_owned(),
        Some(_) => {
            if let Some(base) = repo_base_url
                .map(|b| b.trim().trim_end_matches('/'))
                .filter(|b| !b.is_empty())
            {
                format!("{base}/{path}")
            } else {
                // Preserve empty URL for local-only Forge/NeoForge processor artifacts.
                String::new()
            }
        }
        None => fallback.url.clone(),
    };

    let sha1 = raw_art.sha1.or(outer_sha1).or(fallback.sha1);
    let size = raw_art.size.or(outer_size).unwrap_or(fallback.size);

    Ok(ArtifactDownload {
        path,
        url,
        sha1,
        size,
    })
}

fn rules_allow(rules: &[RawLoaderRule], ctx: &RuleContext) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        let os_matches = match &rule.os {
            None => true,
            Some(os) => {
                let name_ok = os.name.as_deref().is_none_or(|n| n == ctx.os_name);
                let arch_ok = os.arch.as_deref().is_none_or(|a| a == ctx.os_arch);
                name_ok && arch_ok
            }
        };

        let features_match = rule
            .features
            .iter()
            .all(|(key, expected)| ctx.features.get(key).copied().unwrap_or(false) == *expected);

        if os_matches && features_match {
            allowed = rule.action == "allow";
        }
    }

    allowed
}

fn flatten_arguments(entries: &[RawArgumentEntry], ctx: &RuleContext) -> Vec<String> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            RawArgumentEntry::Plain(arg) => out.push(arg.clone()),
            RawArgumentEntry::Conditional { rules, value } => {
                if rules_allow(rules, ctx) {
                    match value {
                        RawArgumentValue::Single(arg) => out.push(arg.clone()),
                        RawArgumentValue::Multiple(args) => out.extend(args.iter().cloned()),
                    }
                }
            }
        }
    }
    out
}

/// Parses a Fabric, Quilt, Forge, or NeoForge profile JSON string into a [`LoaderProfile`].
pub fn parse_loader_profile(
    json: &str,
    loader_kind: ModLoaderKind,
    loader_version: Option<String>,
) -> MqResult<LoaderProfile> {
    let raw: RawLoaderProfile = serde_json::from_str(json)?;

    let id = raw.id.trim().to_owned();
    if id.is_empty() {
        return Err(MqError::Metadata(
            "loader profile JSON has an empty 'id' field".to_owned(),
        ));
    }

    let inherits_from = raw
        .inherits_from
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let main_class = raw
        .main_class
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();

    let rule_ctx = RuleContext::default();

    let (jvm_arguments, game_arguments) = match &raw.arguments {
        Some(args) => (
            flatten_arguments(&args.jvm, &rule_ctx),
            flatten_arguments(&args.game, &rule_ctx),
        ),
        None => {
            let legacy_game = raw
                .minecraft_arguments
                .as_deref()
                .map(|s| s.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default();
            (Vec::new(), legacy_game)
        }
    };

    let mut libraries = Vec::with_capacity(raw.libraries.len());
    for raw_lib in &raw.libraries {
        if let Some(rules) = &raw_lib.rules {
            if !rules_allow(rules, &rule_ctx) {
                continue;
            }
        }
        libraries.push(raw_lib.to_resolved_library_for_loader(loader_kind)?);
    }

    let normalized_loader_version = loader_version
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());

    let asset_index = raw
        .asset_index
        .map(|ai| crate::models::AssetIndexPointer {
            id: ai.id,
            sha1: ai.sha1,
            size: ai.size.unwrap_or(0),
            total_size: ai.total_size.unwrap_or(0),
            url: ai.url.unwrap_or_default(),
        })
        .or_else(|| {
            raw.assets
                .filter(|a| !a.trim().is_empty())
                .map(|a| crate::models::AssetIndexPointer {
                    id: a,
                    sha1: None,
                    size: 0,
                    total_size: 0,
                    url: String::new(),
                })
        });

    let client_jar = raw.downloads.and_then(|d| d.client).map(|art| {
        let path = art.path.unwrap_or_else(|| "client.jar".to_string());
        let url = art.url.unwrap_or_default();
        ArtifactDownload {
            path,
            url,
            sha1: art.sha1,
            size: art.size.unwrap_or(0),
        }
    });

    let logging = raw.logging.and_then(|l| l.client);

    Ok(LoaderProfile {
        id,
        loader: loader_kind,
        loader_version: normalized_loader_version,
        inherits_from,
        main_class,
        libraries,
        jvm_arguments,
        game_arguments,
        asset_index,
        client_jar,
        java_version: raw.java_version,
        logging,
    })
}

/// Compares two Maven version strings according to Maven version sorting conventions.
///
/// Handles numeric dot-separated tokens (`9.7.1` > `9.3.0`, `10.0` > `2.0`),
/// qualifier ordering (`snapshot` < `alpha` < `beta` < `milestone`/`m` < `rc` < `ga`/`final`/`release`/empty < `sp`),
/// and deterministic tie-breaking.
pub fn compare_maven_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    if v1 == v2 {
        return std::cmp::Ordering::Equal;
    }

    let tokens1 = tokenize_maven_version(v1);
    let tokens2 = tokenize_maven_version(v2);

    let max_len = tokens1.len().max(tokens2.len());
    for i in 0..max_len {
        let t1 = tokens1.get(i);
        let t2 = tokens2.get(i);
        match (t1, t2) {
            (Some(a), Some(b)) => {
                let ord = compare_maven_token(a, b);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (Some(a), None) => {
                let ord = compare_maven_token(a, &MavenToken::Empty);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (None, Some(b)) => {
                let ord = compare_maven_token(&MavenToken::Empty, b);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (None, None) => break,
        }
    }

    v1.cmp(v2)
}

#[derive(Debug, PartialEq, Eq)]
enum MavenToken {
    Numeric(u64),
    Qualifier(String),
    Empty,
}

fn tokenize_maven_version(v: &str) -> Vec<MavenToken> {
    let mut tokens = Vec::new();
    let parts = v.split(['.', '-', '_', '+']);
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(num) = trimmed.parse::<u64>() {
            tokens.push(MavenToken::Numeric(num));
        } else {
            let mut curr = String::new();
            let mut is_num = None;
            for ch in trimmed.chars() {
                let ch_num = ch.is_ascii_digit();
                if let Some(prev_num) = is_num {
                    if prev_num != ch_num {
                        if prev_num {
                            if let Ok(n) = curr.parse::<u64>() {
                                tokens.push(MavenToken::Numeric(n));
                            }
                        } else {
                            tokens.push(MavenToken::Qualifier(curr.to_ascii_lowercase()));
                        }
                        curr.clear();
                    }
                }
                is_num = Some(ch_num);
                curr.push(ch);
            }
            if !curr.is_empty() {
                if is_num == Some(true) {
                    if let Ok(n) = curr.parse::<u64>() {
                        tokens.push(MavenToken::Numeric(n));
                    }
                } else {
                    tokens.push(MavenToken::Qualifier(curr.to_ascii_lowercase()));
                }
            }
        }
    }
    tokens
}

fn qualifier_rank(q: &str) -> i32 {
    match q {
        "snapshot" => 1,
        "alpha" | "a" => 2,
        "beta" | "b" => 3,
        "milestone" | "m" => 4,
        "rc" | "cr" => 5,
        "" | "ga" | "final" | "release" => 6,
        "sp" => 7,
        _ => 5,
    }
}

fn compare_maven_token(t1: &MavenToken, t2: &MavenToken) -> std::cmp::Ordering {
    match (t1, t2) {
        (MavenToken::Numeric(a), MavenToken::Numeric(b)) => a.cmp(b),
        (MavenToken::Numeric(_), MavenToken::Qualifier(_)) => std::cmp::Ordering::Greater,
        (MavenToken::Qualifier(_), MavenToken::Numeric(_)) => std::cmp::Ordering::Less,
        (MavenToken::Qualifier(a), MavenToken::Qualifier(b)) => {
            let r1 = qualifier_rank(a);
            let r2 = qualifier_rank(b);
            if r1 != r2 {
                r1.cmp(&r2)
            } else {
                a.cmp(b)
            }
        }
        (MavenToken::Numeric(a), MavenToken::Empty) => {
            if *a == 0 {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Greater
            }
        }
        (MavenToken::Empty, MavenToken::Numeric(b)) => {
            if *b == 0 {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Less
            }
        }
        (MavenToken::Qualifier(a), MavenToken::Empty) => qualifier_rank(a).cmp(&6),
        (MavenToken::Empty, MavenToken::Qualifier(b)) => 6.cmp(&qualifier_rank(b)),
        (MavenToken::Empty, MavenToken::Empty) => std::cmp::Ordering::Equal,
    }
}

/// Extracts the version portion from a Maven coordinate `group:artifact:version[:classifier]`.
pub fn extract_version_from_maven_coord(name: &str) -> Option<&str> {
    let clean = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = clean.split(':').collect();
    parts.get(2).copied()
}

/// Strategy for resolving duplicate libraries that share `(group, artifact, classifier)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryDuplicateStrategy {
    /// Keep the first occurrence in order (Mojang profile order).
    FirstWins,
    /// Keep the child/last occurrence in order (inheritsFrom overriding parent).
    ChildOverridesParent,
    /// Keep the library with the newer/higher Maven version.
    PreferNewerVersion,
}

/// Deduplicates libraries sharing identical module key `(group, artifact, classifier)`
/// using the specified resolution strategy.
pub fn deduplicate_libraries(
    libraries: &[ResolvedLibrary],
    strategy: LibraryDuplicateStrategy,
) -> Vec<ResolvedLibrary> {
    if libraries.is_empty() {
        return Vec::new();
    }

    match strategy {
        LibraryDuplicateStrategy::FirstWins => {
            let mut result = Vec::with_capacity(libraries.len());
            let mut seen_modules = HashSet::new();
            let mut seen_names = HashSet::new();
            for lib in libraries {
                if !seen_names.insert(lib.name.clone()) {
                    continue;
                }
                let key = lib.module_key();
                if !key.0.is_empty() && !key.1.is_empty() && !seen_modules.insert(key) {
                    continue;
                }
                result.push(lib.clone());
            }
            result
        }
        LibraryDuplicateStrategy::ChildOverridesParent => {
            let mut result: Vec<ResolvedLibrary> = Vec::with_capacity(libraries.len());
            let mut seen_modules = HashSet::new();
            let mut seen_names = HashSet::new();
            for lib in libraries.iter().rev() {
                if !seen_names.insert(lib.name.clone()) {
                    continue;
                }
                let key = lib.module_key();
                if !key.0.is_empty() && !key.1.is_empty() && !seen_modules.insert(key) {
                    continue;
                }
                result.push(lib.clone());
            }
            result.reverse();
            result
        }
        LibraryDuplicateStrategy::PreferNewerVersion => {
            let mut by_module: BTreeMap<(String, String, Option<String>), ResolvedLibrary> =
                BTreeMap::new();
            let mut non_module = Vec::new();

            for lib in libraries {
                let key = lib.module_key();
                if key.0.is_empty() || key.1.is_empty() {
                    non_module.push(lib.clone());
                    continue;
                }

                if let Some(existing) = by_module.get_mut(&key) {
                    let v_existing = extract_version_from_maven_coord(&existing.name).unwrap_or("");
                    let v_new = extract_version_from_maven_coord(&lib.name).unwrap_or("");
                    if compare_maven_versions(v_new, v_existing) == std::cmp::Ordering::Greater {
                        *existing = lib.clone();
                    }
                } else {
                    by_module.insert(key, lib.clone());
                }
            }

            let mut result = Vec::with_capacity(by_module.len() + non_module.len());
            for lib in libraries {
                let key = lib.module_key();
                if key.0.is_empty() || key.1.is_empty() {
                    continue;
                }
                if let Some(winner) = by_module.remove(&key) {
                    result.push(winner);
                }
            }
            result.extend(non_module);
            result
        }
    }
}

fn build_meta_endpoint_url(base_url: &str, segments: &[&str]) -> MqResult<Url> {
    for seg in segments {
        if seg.trim().is_empty() {
            return Err(MqError::Metadata(
                "loader metadata URL path segment must not be empty".to_owned(),
            ));
        }
    }

    let clean_base = base_url.trim().trim_end_matches('/');
    let mut url = Url::parse(clean_base).map_err(|err| {
        MqError::Metadata(format!(
            "invalid loader metadata base URL '{base_url}': {err}"
        ))
    })?;

    {
        let mut path_segs = url.path_segments_mut().map_err(|()| {
            MqError::Metadata(format!(
                "loader metadata base URL cannot be a base: '{base_url}'"
            ))
        })?;
        path_segs.pop_if_empty();
        path_segs.extend(segments);
    }

    Ok(url)
}

async fn get_text_checked(client: &reqwest::Client, url: Url) -> MqResult<String> {
    let url_str = url.to_string();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|err| MqError::Network(err.to_string()))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|err| MqError::Network(err.to_string()))?;

    if !status.is_success() {
        let truncated_body: String = body.chars().take(512).collect();
        return Err(MqError::HttpStatus {
            status: status.as_u16(),
            url: url_str,
            body: truncated_body,
        });
    }

    Ok(body)
}

fn parse_loader_versions_response(body: &str) -> MqResult<Vec<String>> {
    let entries: Vec<LoaderVersionResponseEntry> = serde_json::from_str(body)?;
    let mut versions = Vec::with_capacity(entries.len());
    let mut seen = HashSet::with_capacity(entries.len());

    for entry in entries {
        let raw_ver = match entry {
            LoaderVersionResponseEntry::Nested { loader } => loader.version,
            LoaderVersionResponseEntry::Flat { version } => version,
            LoaderVersionResponseEntry::Plain(version) => version,
        };
        let ver = raw_ver.trim().to_owned();
        if !ver.is_empty() && seen.insert(ver.clone()) {
            versions.push(ver);
        }
    }

    Ok(versions)
}

/// Fetches ordered Fabric loader versions compatible with `mc_version` from
/// `{base_url}/v2/versions/loader/{mc_version}`.
pub async fn fetch_fabric_loader_versions(
    client: &reqwest::Client,
    base_url: Option<&str>,
    mc_version: &str,
) -> MqResult<Vec<String>> {
    let base = base_url.unwrap_or(FABRIC_META_BASE_URL);
    let url = build_meta_endpoint_url(base, &["v2", "versions", "loader", mc_version.trim()])?;
    let body = get_text_checked(client, url).await?;
    parse_loader_versions_response(&body)
}

/// Fetches ordered Quilt loader versions compatible with `mc_version` from
/// `{base_url}/v3/versions/loader/{mc_version}`.
pub async fn fetch_quilt_loader_versions(
    client: &reqwest::Client,
    base_url: Option<&str>,
    mc_version: &str,
) -> MqResult<Vec<String>> {
    let base = base_url.unwrap_or(QUILT_META_BASE_URL);
    let url = build_meta_endpoint_url(base, &["v3", "versions", "loader", mc_version.trim()])?;
    let body = get_text_checked(client, url).await?;
    parse_loader_versions_response(&body)
}

/// Fetches and parses a Fabric loader profile from
/// `{base_url}/v2/versions/loader/{mc_version}/{loader_version}/profile/json`.
pub async fn fetch_fabric_profile(
    client: &reqwest::Client,
    base_url: Option<&str>,
    mc_version: &str,
    loader_version: &str,
) -> MqResult<LoaderProfile> {
    let base = base_url.unwrap_or(FABRIC_META_BASE_URL);
    let mc = mc_version.trim();
    let loader = loader_version.trim();
    let url = build_meta_endpoint_url(
        base,
        &["v2", "versions", "loader", mc, loader, "profile", "json"],
    )?;
    let body = get_text_checked(client, url).await?;
    parse_loader_profile(&body, ModLoaderKind::Fabric, Some(loader.to_owned()))
}

/// Fetches and parses a Quilt loader profile from
/// `{base_url}/v3/versions/loader/{mc_version}/{loader_version}/profile/json`.
pub async fn fetch_quilt_profile(
    client: &reqwest::Client,
    base_url: Option<&str>,
    mc_version: &str,
    loader_version: &str,
) -> MqResult<LoaderProfile> {
    let base = base_url.unwrap_or(QUILT_META_BASE_URL);
    let mc = mc_version.trim();
    let loader = loader_version.trim();
    let url = build_meta_endpoint_url(
        base,
        &["v3", "versions", "loader", mc, loader, "profile", "json"],
    )?;
    let body = get_text_checked(client, url).await?;
    parse_loader_profile(&body, ModLoaderKind::Quilt, Some(loader.to_owned()))
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn maven_coord_resolves_standard_classifier_and_jar_suffix() {
        let sha1 =
            Sha1Digest::from_hex("f0ed132a49244b042cd0e15702ab9f2ce3cc8436").expect("valid sha1");
        let fabric = maven_coord_to_artifact(
            "net.fabricmc:fabric-loader:0.16.9",
            Some("https://maven.fabricmc.net/"),
            Some(sha1),
            1_234_567,
        )
        .expect("resolve fabric-loader coordinate");

        assert_eq!(
            fabric.path,
            "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
        assert_eq!(
            fabric.url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
        assert_eq!(fabric.sha1, Some(sha1));
        assert_eq!(fabric.size, 1_234_567);

        let with_clf =
            maven_coord_to_artifact("org.lwjgl:lwjgl:3.3.3:natives-linux@jar", None, None, 0)
                .expect("resolve classifier coordinate with @jar");

        assert_eq!(
            with_clf.path,
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
        assert_eq!(
            with_clf.url,
            "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
    }

    #[test]
    fn maven_coord_synthesizes_paths_for_forge_and_neoforge() {
        let forge_lib = RawLoaderLibrary {
            name: "net.minecraftforge:forge:1.20.1-47.2.0:launcher".to_string(),
            url: None,
            sha1: None,
            size: None,
            downloads: None,
            rules: None,
            extract: None,
            natives: None,
        };
        let resolved_forge = forge_lib
            .to_resolved_library_for_loader(ModLoaderKind::Forge)
            .expect("resolve forge lib");
        let art = resolved_forge.artifact.expect("artifact");
        assert_eq!(
            art.path,
            "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-launcher.jar"
        );
        assert_eq!(
            art.url,
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-launcher.jar"
        );

        let neo_lib = RawLoaderLibrary {
            name: "net.neoforged:neoforge:20.4.80-beta".to_string(),
            url: None,
            sha1: None,
            size: None,
            downloads: None,
            rules: None,
            extract: None,
            natives: None,
        };
        let resolved_neo = neo_lib
            .to_resolved_library_for_loader(ModLoaderKind::NeoForge)
            .expect("resolve neoforge lib");
        let neo_art = resolved_neo.artifact.expect("neoforge artifact");
        assert_eq!(
            neo_art.path,
            "net/neoforged/neoforge/20.4.80-beta/neoforge-20.4.80-beta.jar"
        );
        assert_eq!(
            neo_art.url,
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/20.4.80-beta/neoforge-20.4.80-beta.jar"
        );
    }

    #[test]
    fn maven_coord_rejects_malformed_and_traversal_inputs() {
        for bad in [
            "",
            "net.fabricmc:fabric-loader",
            "a:b:c:d:e",
            "../evil:artifact:1.0",
            "org.example:../evil:1.0",
            "org.example:artifact:1.0/../../pwn",
            "org..example:artifact:1.0",
            "org.example:artifact:1.0@",
            "org.example:artifact:1.0@../jar",
        ] {
            assert!(
                maven_coord_to_artifact(bad, Some("https://maven.fabricmc.net/"), None, 0).is_err(),
                "expected error for coordinate {bad:?}"
            );
        }
    }

    #[test]
    fn parses_fabric_and_quilt_profiles_with_maven_and_explicit_downloads() {
        let fabric_json = r#"{
            "id": "fabric-loader-0.16.9-1.21.1",
            "inheritsFrom": "1.21.1",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {
                "game": [],
                "jvm": [
                    "-DFabricMcEmu= net.minecraft.client.main.Main ",
                    {
                        "rules": [{ "action": "allow", "os": { "name": "osx" } }],
                        "value": ["-XstartOnFirstThread"]
                    }
                ]
            },
            "libraries": [
                {
                    "name": "org.ow2.asm:asm:9.7.1",
                    "url": "https://maven.fabricmc.net/",
                    "sha1": "f0ed132a49244b042cd0e15702ab9f2ce3cc8436",
                    "size": 126093
                },
                {
                    "name": "net.fabricmc:fabric-loader:0.16.9",
                    "url": "https://maven.fabricmc.net/"
                }
            ]
        }"#;

        let fabric = parse_loader_profile(
            fabric_json,
            ModLoaderKind::Fabric,
            Some("0.16.9".to_owned()),
        )
        .expect("parse fabric profile");

        assert_eq!(fabric.id, "fabric-loader-0.16.9-1.21.1");
        assert_eq!(fabric.loader, ModLoaderKind::Fabric);
        assert_eq!(fabric.loader_version.as_deref(), Some("0.16.9"));
        assert_eq!(fabric.inherits_from.as_deref(), Some("1.21.1"));
        assert_eq!(
            fabric.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(
            fabric.jvm_arguments,
            vec!["-DFabricMcEmu= net.minecraft.client.main.Main ".to_owned()]
        );
        assert!(fabric.game_arguments.is_empty());
        assert_eq!(fabric.libraries.len(), 2);
        assert_eq!(
            fabric.libraries[0].artifact.as_ref().unwrap().path,
            "org/ow2/asm/asm/9.7.1/asm-9.7.1.jar"
        );
        assert_eq!(fabric.libraries[0].artifact.as_ref().unwrap().size, 126093);
        assert!(fabric.libraries[0]
            .artifact
            .as_ref()
            .unwrap()
            .sha1
            .is_some());
        assert_eq!(
            fabric.libraries[1].artifact.as_ref().unwrap().url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );

        let quilt_json = r#"{
            "id": "quilt-loader-0.27.1-1.21.1",
            "inheritsFrom": "1.21.1",
            "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient",
            "arguments": {
                "game": ["--quilt-profile"]
            },
            "libraries": [
                {
                    "name": "org.quiltmc:quilt-loader:0.27.1",
                    "url": "https://maven.quiltmc.org/repository/release/"
                },
                {
                    "name": "cpw.mods:bootstraplauncher:2.0.2",
                    "downloads": {
                        "artifact": {
                            "path": "cpw/mods/bootstraplauncher/2.0.2/bootstraplauncher-2.0.2.jar",
                            "url": "https://maven.neoforged.net/releases/cpw/mods/bootstraplauncher/2.0.2/bootstraplauncher-2.0.2.jar",
                            "sha1": "",
                            "size": 45678
                        }
                    }
                }
            ]
        }"#;

        let quilt =
            parse_loader_profile(quilt_json, ModLoaderKind::Quilt, Some("0.27.1".to_owned()))
                .expect("parse quilt profile");

        assert_eq!(quilt.loader, ModLoaderKind::Quilt);
        assert!(quilt.jvm_arguments.is_empty());
        assert_eq!(quilt.game_arguments, vec!["--quilt-profile"]);
        assert_eq!(quilt.libraries.len(), 2);
        assert_eq!(
            quilt.libraries[0].artifact.as_ref().unwrap().url,
            "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/0.27.1/quilt-loader-0.27.1.jar"
        );
        assert_eq!(quilt.libraries[1].artifact.as_ref().unwrap().sha1, None);
        assert_eq!(quilt.libraries[1].artifact.as_ref().unwrap().size, 45678);
    }

    #[test]
    fn parses_forge_and_neoforge_profiles_with_natives_and_java_version() {
        let forge_json = r#"{
            "id": "1.12.2-forge-14.23.5.2860",
            "inheritsFrom": "1.12.2",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "javaVersion": {
                "component": "jre-legacy",
                "majorVersion": 8
            },
            "logging": {
                "client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": {
                        "id": "client-1.12.xml",
                        "size": 877,
                        "url": "https://piston-data.mojang.com/v1/objects/ef4f57b9222277c603b5443217b1fb797c293eb7/client-1.12.xml"
                    },
                    "type": "log4j2-xml"
                }
            },
            "libraries": [
                {
                    "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
                    "natives": {
                        "linux": "natives-linux"
                    },
                    "extract": {
                        "exclude": ["META-INF/"]
                    }
                }
            ]
        }"#;

        let forge = parse_loader_profile(forge_json, ModLoaderKind::Forge, None)
            .expect("parse forge profile");

        assert_eq!(forge.loader, ModLoaderKind::Forge);
        assert_eq!(forge.main_class, "net.minecraft.launchwrapper.Launch");
        assert_eq!(forge.java_version.as_ref().unwrap().major_version, 8);
        assert!(forge.logging.is_some());
        assert_eq!(forge.libraries.len(), 1);

        let lwjgl = &forge.libraries[0];
        assert!(lwjgl.native_linux.is_some());
        assert_eq!(
            lwjgl.native_linux.as_ref().unwrap().path,
            "org/lwjgl/lwjgl/lwjgl-platform/2.9.4-nightly-20150209/lwjgl-platform-2.9.4-nightly-20150209-natives-linux.jar"
        );
        assert_eq!(lwjgl.extract_exclude, vec!["META-INF/"]);
        assert!(!lwjgl.include_in_classpath);
    }

    #[test]
    fn compares_maven_versions_correctly() {
        use std::cmp::Ordering;
        assert_eq!(compare_maven_versions("9.7.1", "9.3.0"), Ordering::Greater);
        assert_eq!(compare_maven_versions("10.0", "2.0"), Ordering::Greater);
        assert_eq!(compare_maven_versions("1.21.1", "1.21.1"), Ordering::Equal);
        assert_eq!(
            compare_maven_versions("1.0.0-snapshot", "1.0.0"),
            Ordering::Less
        );
        assert_eq!(compare_maven_versions("1.0.0-rc1", "1.0.0"), Ordering::Less);
        assert_eq!(
            compare_maven_versions("1.0.0-beta", "1.0.0-rc1"),
            Ordering::Less
        );
        assert_eq!(
            compare_maven_versions("20.4.80-beta", "20.4.79"),
            Ordering::Greater
        );
    }

    #[test]
    fn resolves_library_duplicates_with_mojang_and_version_strategies() {
        let make_lib = |name: &str| ResolvedLibrary {
            name: name.to_string(),
            artifact: None,
            native_linux: None,
            extract_exclude: Vec::new(),
            include_in_classpath: true,
        };

        let libs = vec![
            make_lib("org.ow2.asm:asm:9.3"),
            make_lib("com.google.code.gson:gson:2.10.1"),
            make_lib("org.ow2.asm:asm:9.7.1"),
        ];

        // Strategy 1: FirstWins (keeps 9.3)
        let first_wins = deduplicate_libraries(&libs, LibraryDuplicateStrategy::FirstWins);
        assert_eq!(first_wins.len(), 2);
        assert_eq!(first_wins[0].name, "org.ow2.asm:asm:9.3");

        // Strategy 2: ChildOverridesParent (keeps 9.7.1)
        let child_overrides =
            deduplicate_libraries(&libs, LibraryDuplicateStrategy::ChildOverridesParent);
        assert_eq!(child_overrides.len(), 2);
        assert_eq!(child_overrides[1].name, "org.ow2.asm:asm:9.7.1");

        // Strategy 3: PreferNewerVersion (picks 9.7.1 over 9.3)
        let newer = deduplicate_libraries(&libs, LibraryDuplicateStrategy::PreferNewerVersion);
        assert_eq!(newer.len(), 2);
        assert_eq!(newer[0].name, "org.ow2.asm:asm:9.7.1");
    }

    #[tokio::test]
    async fn fetches_fabric_versions_and_fabric_quilt_profiles_over_http() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock listener");
        let addr = listener.local_addr().expect("local addr");

        let server = tokio::spawn(async move {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.expect("accept client");
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).await.expect("read request");
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");

                let (status, body) = if first_line.starts_with("GET /v2/versions/loader/1.21.1 ") {
                    (
                        "200 OK",
                        r#"[{"loader":{"version":"0.16.9"}},{"loader":{"version":"0.16.8"}},{"loader":{"version":"0.16.9"}}]"#,
                    )
                } else if first_line
                    .starts_with("GET /v2/versions/loader/1.21.1/0.16.9/profile/json ")
                {
                    (
                        "200 OK",
                        r#"{"id":"fabric-loader-0.16.9-1.21.1","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.9","url":"https://maven.fabricmc.net/"}]}"#,
                    )
                } else if first_line
                    .starts_with("GET /v3/versions/loader/1.21.1/0.27.1/profile/json ")
                {
                    (
                        "200 OK",
                        r#"{"id":"quilt-loader-0.27.1-1.21.1","inheritsFrom":"1.21.1","mainClass":"org.quiltmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"org.quiltmc:quilt-loader:0.27.1","url":"https://maven.quiltmc.org/repository/release/"}]}"#,
                    )
                } else {
                    ("404 Not Found", "missing version")
                };

                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
        });

        let base = format!("http://{addr}");
        let client = reqwest::Client::new();

        let versions = fetch_fabric_loader_versions(&client, Some(&base), "1.21.1")
            .await
            .expect("fetch fabric versions");
        assert_eq!(versions, vec!["0.16.9".to_owned(), "0.16.8".to_owned()]);

        let fabric_profile = fetch_fabric_profile(&client, Some(&base), "1.21.1", "0.16.9")
            .await
            .expect("fetch fabric profile");
        assert_eq!(fabric_profile.loader, ModLoaderKind::Fabric);
        assert_eq!(fabric_profile.loader_version.as_deref(), Some("0.16.9"));
        assert_eq!(fabric_profile.libraries.len(), 1);

        let quilt_profile = fetch_quilt_profile(&client, Some(&base), "1.21.1", "0.27.1")
            .await
            .expect("fetch quilt profile");
        assert_eq!(quilt_profile.loader, ModLoaderKind::Quilt);
        assert_eq!(quilt_profile.loader_version.as_deref(), Some("0.27.1"));

        let missing_err = fetch_fabric_profile(&client, Some(&base), "1.21.1", "0.0.0")
            .await
            .expect_err("expected 404 error");
        assert!(matches!(
            missing_err,
            MqError::HttpStatus { status: 404, .. }
        ));

        server.await.expect("mock server finished");
    }
}
