//! Mojang `version_manifest_v2.json` and `version.json` parsing, Maven coordinate synthesis,
//! Java version inference, and HTTP fetching.

use std::collections::BTreeMap;

use mq_core::{ModLoaderKind, MqError, MqResult, Sha1Digest};
use serde::{Deserialize, Serialize};

use crate::models::{
    ArtifactDownload, AssetIndexPointer, JavaVersionSpec, ResolvedLibrary, ResolvedVersion,
    RuleContext, VersionManifest, VersionManifestEntry,
};
use crate::rules::{evaluate_rules, normalize_arch, normalize_os_name, RawRule};

/// Official Mojang v2 version manifest endpoint.
pub const MOJANG_VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Official Mojang asset objects CDN base URL.
pub const MOJANG_RESOURCES_BASE_URL: &str = "https://resources.download.minecraft.net";

/// Official Mojang Maven libraries CDN base URL.
pub const MOJANG_LIBRARIES_BASE_URL: &str = "https://libraries.minecraft.net";

/// Default JVM arguments synthesized when legacy `minecraftArguments` is used without `arguments.jvm`.
pub const DEFAULT_LEGACY_JVM_ARGS: [&str; 3] = [
    "-Djava.library.path=${natives_directory}",
    "-cp",
    "${classpath}",
];

/// Parses Mojang's `version_manifest_v2.json` document.
pub fn parse_version_manifest(json: &str) -> MqResult<VersionManifest> {
    let manifest: VersionManifest = serde_json::from_str(json)?;
    Ok(manifest)
}

/// Looks up a specific Minecraft version entry by ID in [`VersionManifest`].
pub fn find_version_entry<'a>(
    manifest: &'a VersionManifest,
    id: &str,
) -> MqResult<&'a VersionManifestEntry> {
    let target = id.trim();
    manifest
        .versions
        .iter()
        .find(|entry| entry.id == target)
        .ok_or_else(|| {
            MqError::Metadata(format!(
                "Minecraft version '{target}' not found in version manifest"
            ))
        })
}

/// Converts a Maven coordinate (`group:artifact:version[:classifier][@ext]`) into its
/// standard relative repository path (`group_path/artifact/version/artifact-version[-classifier].ext`).
pub fn maven_coordinate_to_path(coord: &str) -> MqResult<String> {
    let trimmed = coord.trim();
    if trimmed.is_empty() {
        return Err(MqError::Metadata(
            "Maven coordinate must not be empty".to_string(),
        ));
    }

    let (coord_part, ext) = match trimmed.split_once('@') {
        Some((c, e)) if !e.trim().is_empty() => (c.trim(), e.trim()),
        Some((c, _)) => (c.trim(), "jar"),
        None => (trimmed, "jar"),
    };

    let parts: Vec<&str> = coord_part.split(':').collect();
    if parts.len() < 3 || parts.len() > 4 || parts.iter().any(|p| p.trim().is_empty()) {
        return Err(MqError::Metadata(format!(
            "invalid Maven coordinate '{coord}' (expected group:artifact:version[:classifier])"
        )));
    }

    let group_path = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];

    let file_name = if let Some(classifier) = parts.get(3) {
        format!("{artifact}-{version}-{classifier}.{ext}")
    } else {
        format!("{artifact}-{version}.{ext}")
    };

    Ok(format!("{group_path}/{artifact}/{version}/{file_name}"))
}

/// Infers the required [`JavaVersionSpec`] from a Minecraft version string when `javaVersion`
/// is absent from `version.json`:
/// - `major_version: 25` for Mojang 26.x scheme and >= 25.x (or 26w.. snapshots)
/// - `major_version: 21` for MC >= 1.20.5 (or 24w14a..25w snapshots)
/// - `major_version: 17` for MC 1.18..=1.20.4 (or 22w..24w snapshots)
/// - `major_version: 8` for older versions (1.16.5 and below, Alpha, Beta, etc.)
pub fn infer_java_version(mc_version: &str) -> JavaVersionSpec {
    let trimmed = mc_version.trim();

    // Old alpha, beta, classic, indev, infdev, or custom prefixes run on Java 8
    if trimmed.starts_with('c')
        || trimmed.starts_with("inf-")
        || trimmed.starts_with("rd-")
        || trimmed.starts_with('a')
        || trimmed.starts_with('b')
    {
        return JavaVersionSpec {
            component: "jre-legacy".to_string(),
            major_version: 8,
        };
    }

    // Snapshot detection: e.g. "25w02a", "26w04a"
    if let Some(w_pos) = trimmed.find('w') {
        let prefix = &trimmed[..w_pos];
        if let Ok(year) = prefix.parse::<u32>() {
            if year >= 26 {
                return JavaVersionSpec {
                    component: "java-runtime-epsilon".to_string(),
                    major_version: 25,
                };
            } else if year >= 24 {
                return JavaVersionSpec {
                    component: "java-runtime-delta".to_string(),
                    major_version: 21,
                };
            } else if year >= 22 {
                return JavaVersionSpec {
                    component: "java-runtime-gamma".to_string(),
                    major_version: 17,
                };
            } else {
                return JavaVersionSpec {
                    component: "jre-legacy".to_string(),
                    major_version: 8,
                };
            }
        }
    }

    // Strip hyphens, pluses, pre, rc, snapshot suffixes safely without panic
    let core = trimmed
        .split(['-', '+', ' '])
        .next()
        .unwrap_or("");
    let nums: Vec<u32> = core
        .split('.')
        .map_while(|seg| seg.parse::<u32>().ok())
        .collect();

    let (major_version, component) = match nums.as_slice() {
        // Mojang 26.x scheme (e.g. 26.1, 26.3, 27.x)
        [major, ..] if *major >= 25 => (25, "java-runtime-epsilon"),
        // 1.20.5+ and 1.21.x -> Java 21
        [1, minor, patch, ..] if *minor > 20 || (*minor == 20 && *patch >= 5) => {
            (21, "java-runtime-delta")
        }
        [1, minor] if *minor > 20 => (21, "java-runtime-delta"),
        // 1.18 .. 1.20.4 -> Java 17
        [1, minor, ..] if *minor >= 18 => (17, "java-runtime-gamma"),
        [1, 17, ..] => (17, "java-runtime-gamma"),
        // Older versions (1.16.5 and below, or 0.x) -> Java 8
        _ => (8, "jre-legacy"),
    };

    JavaVersionSpec {
        component: component.to_string(),
        major_version,
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
struct VanillaRawVersion {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "inheritsFrom")]
    inherits_from: Option<String>,
    #[serde(default, rename = "mainClass")]
    main_class: Option<String>,
    #[serde(default, rename = "javaVersion")]
    java_version: Option<JavaVersionSpec>,
    #[serde(default, rename = "assetIndex")]
    asset_index: Option<VanillaRawAssetIndexPointer>,
    #[serde(default)]
    assets: Option<String>,
    #[serde(default)]
    downloads: Option<VanillaRawVersionDownloads>,
    #[serde(default)]
    libraries: Vec<VanillaRawLibrary>,
    #[serde(default)]
    arguments: Option<VanillaRawArguments>,
    #[serde(default, rename = "minecraftArguments")]
    minecraft_arguments: Option<String>,
    #[serde(default)]
    logging: Option<VanillaRawLogging>,
    #[serde(default, rename = "releaseTime")]
    release_time: Option<String>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default, rename = "type")]
    release_type: Option<String>,
    #[serde(default, rename = "complianceLevel")]
    compliance_level: Option<i32>,
    #[serde(default, rename = "minimumLauncherVersion")]
    minimum_launcher_version: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct VanillaRawLogging {
    #[serde(default)]
    client: Option<VanillaRawLoggingClient>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct VanillaRawLoggingClient {
    #[serde(default)]
    argument: Option<String>,
    #[serde(default)]
    file: Option<VanillaRawLoggingFile>,
    #[serde(default, rename = "type")]
    logging_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct VanillaRawLoggingFile {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    sha1: Option<Sha1Digest>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawAssetIndexPointer {
    id: String,
    #[serde(default)]
    sha1: Option<Sha1Digest>,
    #[serde(default)]
    size: u64,
    #[serde(default, rename = "totalSize")]
    total_size: u64,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawVersionDownloads {
    #[serde(default)]
    client: Option<VanillaRawArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawArtifact {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    sha1: Option<Sha1Digest>,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawLibrary {
    name: String,
    #[serde(default)]
    downloads: Option<VanillaRawLibraryDownloads>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    rules: Option<Vec<RawRule>>,
    #[serde(default)]
    natives: Option<BTreeMap<String, String>>,
    #[serde(default)]
    extract: Option<VanillaRawExtract>,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawLibraryDownloads {
    #[serde(default)]
    artifact: Option<VanillaRawArtifact>,
    #[serde(default)]
    classifiers: BTreeMap<String, VanillaRawArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawExtract {
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct VanillaRawArguments {
    #[serde(default)]
    game: Vec<VanillaRawArgumentEntry>,
    #[serde(default)]
    jvm: Vec<VanillaRawArgumentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum VanillaRawArgumentEntry {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Option<Vec<RawRule>>,
        #[serde(alias = "values")]
        value: VanillaRawArgumentValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum VanillaRawArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

fn evaluate_argument_entries(
    entries: &[VanillaRawArgumentEntry],
    ctx: &RuleContext,
) -> Vec<String> {
    let mut out = Vec::new();
    for entry in entries {
        match entry {
            VanillaRawArgumentEntry::Plain(arg) => out.push(arg.clone()),
            VanillaRawArgumentEntry::Conditional { rules, value } => {
                if evaluate_rules(rules.as_deref(), ctx) {
                    match value {
                        VanillaRawArgumentValue::Single(s) => out.push(s.clone()),
                        VanillaRawArgumentValue::Multiple(list) => out.extend(list.iter().cloned()),
                    }
                }
            }
        }
    }
    out
}

fn arch_bitness_token(arch: &str) -> &'static str {
    match normalize_arch(arch) {
        "x86" => "32",
        _ => "64",
    }
}

fn resolve_artifact_download(
    raw: &VanillaRawArtifact,
    coord: &str,
    classifier_override: Option<&str>,
    repo_base_url: Option<&str>,
) -> MqResult<ArtifactDownload> {
    let effective_coord = match classifier_override {
        Some(cls) => {
            let (base, ext_opt) = coord
                .split_once('@')
                .map_or((coord, None), |(b, e)| (b, Some(e)));
            let parts: Vec<&str> = base.split(':').collect();
            if parts.len() == 3 {
                match ext_opt {
                    Some(ext) => format!("{base}:{cls}@{ext}"),
                    None => format!("{base}:{cls}"),
                }
            } else {
                coord.to_string()
            }
        }
        None => coord.to_string(),
    };

    let path = match raw.path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => maven_coordinate_to_path(&effective_coord)?,
    };

    let raw_url = raw.url.trim();
    let url = if !raw_url.is_empty() {
        if raw_url.ends_with(&path) || raw_url.ends_with(".jar") || raw_url.ends_with(".zip") {
            raw_url.to_string()
        } else {
            format!("{}/{path}", raw_url.trim_end_matches('/'))
        }
    } else {
        let base = repo_base_url
            .unwrap_or(MOJANG_LIBRARIES_BASE_URL)
            .trim_end_matches('/');
        format!("{base}/{path}")
    };

    Ok(ArtifactDownload {
        path,
        url,
        sha1: raw.sha1,
        size: raw.size,
    })
}

fn resolve_native_classifier(
    raw_lib: &VanillaRawLibrary,
    ctx: &RuleContext,
) -> MqResult<Option<ArtifactDownload>> {
    let Some(ref natives_map) = raw_lib.natives else {
        return Ok(None);
    };

    let target_os = normalize_os_name(&ctx.os_name);
    let classifier_template = natives_map
        .iter()
        .find(|(k, _)| normalize_os_name(k).eq_ignore_ascii_case(target_os))
        .map(|(_, v)| v.as_str());

    let Some(template) = classifier_template else {
        return Ok(None);
    };

    let Some(ref downloads) = raw_lib.downloads else {
        return Ok(None);
    };

    let candidates = [
        template.replace("${arch}", &ctx.os_arch),
        template.replace("${arch}", normalize_arch(&ctx.os_arch)),
        template.replace("${arch}", arch_bitness_token(&ctx.os_arch)),
        template.to_string(),
    ];

    for key in &candidates {
        if let Some(raw_cls_art) = downloads.classifiers.get(key) {
            let resolved = resolve_artifact_download(
                raw_cls_art,
                &raw_lib.name,
                Some(key),
                raw_lib.url.as_deref(),
            )?;
            return Ok(Some(resolved));
        }
    }

    Ok(None)
}

fn resolve_library(
    raw_lib: &VanillaRawLibrary,
    ctx: &RuleContext,
) -> MqResult<Option<ResolvedLibrary>> {
    if !evaluate_rules(raw_lib.rules.as_deref(), ctx) {
        return Ok(None);
    }

    let native_linux = resolve_native_classifier(raw_lib, ctx)?;

    let artifact = if let Some(ref downloads) = raw_lib.downloads {
        if let Some(ref raw_art) = downloads.artifact {
            Some(resolve_artifact_download(
                raw_art,
                &raw_lib.name,
                None,
                raw_lib.url.as_deref(),
            )?)
        } else {
            None
        }
    } else if raw_lib.natives.is_none() {
        let path = maven_coordinate_to_path(&raw_lib.name)?;
        let base = raw_lib
            .url
            .as_deref()
            .unwrap_or(MOJANG_LIBRARIES_BASE_URL)
            .trim_end_matches('/');
        Some(ArtifactDownload {
            url: format!("{base}/{path}"),
            path,
            sha1: None,
            size: 0,
        })
    } else {
        None
    };

    if artifact.is_none() && native_linux.is_none() {
        return Ok(None);
    }

    let extract_exclude = raw_lib
        .extract
        .as_ref()
        .map(|e| e.exclude.clone())
        .unwrap_or_default();

    let include_in_classpath = artifact.is_some();

    Ok(Some(ResolvedLibrary {
        name: raw_lib.name.clone(),
        artifact,
        native_linux,
        extract_exclude,
        include_in_classpath,
    }))
}

/// Parses a Mojang `version.json` document and resolves rules, libraries, natives, and arguments
/// against `ctx` while strictly preserving manifest ordering (Lesson E8).
pub fn parse_vanilla_version(json: &str, ctx: &RuleContext) -> MqResult<ResolvedVersion> {
    let raw: VanillaRawVersion = serde_json::from_str(json)?;
    let id = raw.id.trim().to_string();
    if id.is_empty() && raw.inherits_from.is_none() {
        return Err(MqError::Metadata(
            "version.json 'id' must not be empty".to_string(),
        ));
    }
    let id = if id.is_empty() {
        raw.inherits_from.as_deref().unwrap_or("").to_string()
    } else {
        id
    };

    let mc_version = raw
        .inherits_from
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.clone());

    let main_class = raw
        .main_class
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if raw.inherits_from.is_some() {
                Some(String::new())
            } else {
                None
            }
        })
        .ok_or_else(|| MqError::Metadata(format!("version.json '{id}' missing 'mainClass'")))?;

    let java_version = raw
        .java_version
        .unwrap_or_else(|| infer_java_version(&mc_version));

    let asset_index = match raw.asset_index {
        Some(ai) => AssetIndexPointer {
            id: ai.id,
            sha1: ai.sha1,
            size: ai.size,
            total_size: ai.total_size,
            url: ai.url,
        },
        None => AssetIndexPointer {
            id: raw.assets.unwrap_or_else(|| {
                if raw.inherits_from.is_some() {
                    String::new()
                } else {
                    mc_version.clone()
                }
            }),
            sha1: None,
            size: 0,
            total_size: 0,
            url: String::new(),
        },
    };

    let client_jar = match raw.downloads.and_then(|d| d.client) {
        Some(client) => ArtifactDownload {
            path: client
                .path
                .filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| format!("versions/{id}/{id}.jar")),
            url: client.url,
            sha1: client.sha1,
            size: client.size,
        },
        None => ArtifactDownload {
            path: if raw.inherits_from.is_some() {
                String::new()
            } else {
                format!("versions/{id}/{id}.jar")
            },
            url: String::new(),
            sha1: None,
            size: 0,
        },
    };

    let mut libraries = Vec::with_capacity(raw.libraries.len());
    for raw_lib in &raw.libraries {
        if let Some(resolved) = resolve_library(raw_lib, ctx)? {
            libraries.push(resolved);
        }
    }

    let mut jvm_arguments = Vec::new();
    let mut game_arguments = Vec::new();

    let has_modern_jvm = raw
        .arguments
        .as_ref()
        .is_some_and(|args| !args.jvm.is_empty());
    let has_modern_game = raw
        .arguments
        .as_ref()
        .is_some_and(|args| !args.game.is_empty());

    if let Some(ref args) = raw.arguments {
        if !args.jvm.is_empty() {
            jvm_arguments.extend(evaluate_argument_entries(&args.jvm, ctx));
        }
        if !args.game.is_empty() {
            game_arguments.extend(evaluate_argument_entries(&args.game, ctx));
        }
    }

    if let Some(ref legacy_args) = raw.minecraft_arguments {
        if !has_modern_game {
            game_arguments.extend(legacy_args.split_whitespace().map(str::to_string));
        }
        if !has_modern_jvm {
            jvm_arguments.extend(DEFAULT_LEGACY_JVM_ARGS.iter().map(|s| (*s).to_string()));
        }
    }

    let logging = raw.logging.and_then(|log| {
        log.client.map(|client| {
            let (file_id, sha1, size, url) = if let Some(f) = client.file {
                (
                    f.id.unwrap_or_default(),
                    f.sha1,
                    f.size.unwrap_or(0),
                    f.url.unwrap_or_default(),
                )
            } else {
                (String::new(), None, 0, String::new())
            };

            let path = if !file_id.is_empty() {
                format!("log_configs/{file_id}")
            } else {
                String::new()
            };

            crate::models::LoggingConfig {
                client: Some(crate::models::LoggingClientConfig {
                    argument: client.argument.unwrap_or_default(),
                    file: ArtifactDownload {
                        path,
                        url,
                        sha1,
                        size,
                    },
                    logging_type: client.logging_type.unwrap_or_default(),
                }),
            }
        })
    });

    Ok(ResolvedVersion {
        id,
        mc_version,
        loader: ModLoaderKind::Vanilla,
        loader_version: None,
        inherits_from: raw.inherits_from,
        main_class,
        java_version,
        asset_index,
        client_jar,
        libraries,
        jvm_arguments,
        game_arguments,
        logging,
    })
}

/// Fetches and parses Mojang's `version_manifest_v2.json`.
///
/// If `base_url_override` is provided:
/// - If it ends with `.json`, it is used as the full manifest URL.
/// - Otherwise `/mc/game/version_manifest_v2.json` is appended to the base URL.
pub async fn fetch_version_manifest(
    client: &reqwest::Client,
    base_url_override: Option<&str>,
) -> MqResult<VersionManifest> {
    let url = match base_url_override.map(str::trim).filter(|s| !s.is_empty()) {
        Some(override_url) if override_url.ends_with(".json") => override_url.to_string(),
        Some(base) => format!(
            "{}/mc/game/version_manifest_v2.json",
            base.trim_end_matches('/')
        ),
        None => MOJANG_VERSION_MANIFEST_URL.to_string(),
    };

    let response = client.get(&url).send().await.map_err(|err| {
        MqError::Network(format!("failed to fetch version manifest '{url}': {err}"))
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        MqError::Network(format!("failed to read version manifest '{url}': {err}"))
    })?;

    if !status.is_success() {
        return Err(MqError::HttpStatus {
            status: status.as_u16(),
            url,
            body,
        });
    }

    parse_version_manifest(&body)
}

/// Target locator for [`fetch_vanilla_version`]: accepts a [`VersionManifestEntry`] reference,
/// a direct `version.json` URL (`&str` / `String`), or a Minecraft version ID (`"1.21.4"`).
pub trait VanillaVersionSource {
    fn as_source_str(&self) -> &str;
}

impl VanillaVersionSource for str {
    fn as_source_str(&self) -> &str {
        self
    }
}

impl VanillaVersionSource for &str {
    fn as_source_str(&self) -> &str {
        self
    }
}

impl VanillaVersionSource for String {
    fn as_source_str(&self) -> &str {
        self.as_str()
    }
}

impl VanillaVersionSource for &String {
    fn as_source_str(&self) -> &str {
        self.as_str()
    }
}

impl VanillaVersionSource for VersionManifestEntry {
    fn as_source_str(&self) -> &str {
        self.url.as_str()
    }
}

impl VanillaVersionSource for &VersionManifestEntry {
    fn as_source_str(&self) -> &str {
        self.url.as_str()
    }
}

/// Fetches and parses a Mojang `version.json` document into a [`ResolvedVersion`].
///
/// `source` may be a [`VersionManifestEntry`], a direct `http(s)://...` URL to `version.json`,
/// or a version ID string (which will be resolved via [`fetch_version_manifest`]).
pub async fn fetch_vanilla_version<S: VanillaVersionSource>(
    client: &reqwest::Client,
    source: S,
    ctx: &RuleContext,
) -> MqResult<ResolvedVersion> {
    let raw_target = source.as_source_str().trim();
    let url = if raw_target.starts_with("http://") || raw_target.starts_with("https://") {
        raw_target.to_string()
    } else {
        let manifest = fetch_version_manifest(client, None).await?;
        let entry = find_version_entry(&manifest, raw_target)?;
        entry.url.clone()
    };

    let response = client.get(&url).send().await.map_err(|err| {
        MqError::Network(format!("failed to fetch vanilla version '{url}': {err}"))
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        MqError::Network(format!("failed to read vanilla version '{url}': {err}"))
    })?;

    if !status.is_success() {
        return Err(MqError::HttpStatus {
            status: status.as_u16(),
            url,
            body,
        });
    }

    let parsed = parse_vanilla_version(&body, ctx)?;
    if let Some(ref parent_id) = parsed.inherits_from {
        if parent_id != &parsed.id {
            let parent = Box::pin(fetch_vanilla_version(client, parent_id.as_str(), ctx)).await?;
            return crate::merge::merge_resolved_versions(&parent, &parsed);
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_manifest_and_looks_up_entries() {
        let manifest_json = r#"{
            "latest": {
                "release": "1.21.4",
                "snapshot": "25w02a"
            },
            "versions": [
                {
                    "id": "1.21.4",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/aaaa/1.21.4.json",
                    "sha1": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                    "releaseTime": "2024-12-03T10:12:57+00:00"
                },
                {
                    "id": "1.7.10",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/bbbb/1.7.10.json"
                }
            ]
        }"#;

        let manifest = parse_version_manifest(manifest_json).expect("parse version manifest");
        assert_eq!(manifest.latest.release, "1.21.4");
        assert_eq!(manifest.latest.snapshot, "25w02a");
        assert_eq!(manifest.versions.len(), 2);

        let v121 = find_version_entry(&manifest, "1.21.4").expect("find 1.21.4");
        assert_eq!(v121.release_type, "release");
        assert!(v121.url.ends_with("1.21.4.json"));

        assert!(find_version_entry(&manifest, "1.99.9").is_err());
    }

    #[test]
    fn synthesizes_maven_coordinates_with_and_without_classifiers() {
        assert_eq!(
            maven_coordinate_to_path("com.mojang:logging:1.2.7").expect("coord"),
            "com/mojang/logging/1.2.7/logging-1.2.7.jar"
        );
        assert_eq!(
            maven_coordinate_to_path("org.lwjgl:lwjgl:3.3.3:natives-linux").expect("coord"),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
        assert_eq!(
            maven_coordinate_to_path("net.fabricmc:sponge-mixin:0.15.3+mixin.0.8.7@jar")
                .expect("coord"),
            "net/fabricmc/sponge-mixin/0.15.3+mixin.0.8.7/sponge-mixin-0.15.3+mixin.0.8.7.jar"
        );
        assert!(maven_coordinate_to_path("invalid-coord").is_err());
    }

    #[test]
    fn infers_java_version_across_mc_eras() {
        // Classic, Indev, Alpha, Beta
        assert_eq!(infer_java_version("c0.0.13a").major_version, 8);
        assert_eq!(infer_java_version("inf-20100618").major_version, 8);
        assert_eq!(infer_java_version("a1.0.14").major_version, 8);
        assert_eq!(infer_java_version("b1.7.3").major_version, 8);

        // Pre-modern releases
        assert_eq!(infer_java_version("1.7.10").major_version, 8);
        assert_eq!(infer_java_version("1.12.2").major_version, 8);
        assert_eq!(infer_java_version("1.16.5").major_version, 8);
        assert_eq!(infer_java_version("1.18").major_version, 17);
        assert_eq!(infer_java_version("1.20.4").major_version, 17);
        assert_eq!(infer_java_version("1.20.5").major_version, 21);
        assert_eq!(infer_java_version("1.21.4").major_version, 21);

        // Mojang 26.x scheme & snapshots
        assert_eq!(infer_java_version("26.1").major_version, 25);
        assert_eq!(infer_java_version("26.1").component, "java-runtime-epsilon");
        assert_eq!(infer_java_version("26.3").major_version, 25);
        assert_eq!(infer_java_version("26.1-snapshot-1").major_version, 25);
        assert_eq!(infer_java_version("26.1-pre1").major_version, 25);
        assert_eq!(infer_java_version("26.1-rc1").major_version, 25);
        assert_eq!(infer_java_version("26w04a").major_version, 25);
        assert_eq!(infer_java_version("25w02a").major_version, 21);
    }

    #[test]
    fn parses_modern_version_json_filters_rules_and_preserves_order() {
        let modern_json = r#"{
            "id": "1.21.4",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {
                "component": "java-runtime-delta",
                "majorVersion": 21
            },
            "assetIndex": {
                "id": "19",
                "sha1": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                "size": 450123,
                "totalSize": 680123456,
                "url": "https://piston-meta.mojang.com/v1/packages/a94a8fe5ccb19ba61c4c0873d391e987982fbbd3/19.json"
            },
            "downloads": {
                "client": {
                    "sha1": "0123456789abcdef0123456789abcdef01234567",
                    "size": 26500000,
                    "url": "https://piston-data.mojang.com/v1/objects/0123456789abcdef0123456789abcdef01234567/client.jar"
                }
            },
            "arguments": {
                "jvm": [
                    {
                        "rules": [{ "action": "allow", "os": { "name": "osx" } }],
                        "value": ["-XstartOnFirstThread"]
                    },
                    {
                        "rules": [{ "action": "allow", "os": { "name": "linux" } }],
                        "value": "-Dos.name=Linux"
                    },
                    "-Djava.library.path=${natives_directory}",
                    "-cp",
                    "${classpath}"
                ],
                "game": [
                    "--username",
                    "${auth_player_name}",
                    "--version",
                    "${version_name}",
                    {
                        "rules": [{ "action": "allow", "features": { "is_demo_user": true } }],
                        "value": "--demo"
                    }
                ]
            },
            "libraries": [
                {
                    "name": "ca.weblite:java-objc-bridge:1.1",
                    "downloads": {
                        "artifact": {
                            "path": "ca/weblite/java-objc-bridge/1.1/java-objc-bridge-1.1.jar",
                            "sha1": "0123456789abcdef0123456789abcdef01234567",
                            "size": 40500,
                            "url": "https://libraries.minecraft.net/ca/weblite/java-objc-bridge/1.1/java-objc-bridge-1.1.jar"
                        }
                    },
                    "rules": [{ "action": "allow", "os": { "name": "osx" } }]
                },
                {
                    "name": "com.mojang:logging:1.2.7",
                    "downloads": {
                        "artifact": {
                            "url": "https://libraries.minecraft.net",
                            "sha1": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                            "size": 15342
                        }
                    }
                },
                {
                    "name": "org.lwjgl:lwjgl:3.3.3",
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                            "sha1": "fedcba9876543210fedcba9876543210fedcba98",
                            "size": 712345,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                        }
                    }
                },
                {
                    "name": "org.lwjgl:lwjgl:3.3.3:natives-linux",
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar",
                            "sha1": "0123456789abcdef0123456789abcdef01234567",
                            "size": 512000,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
                        }
                    },
                    "rules": [{ "action": "allow", "os": { "name": "linux" } }]
                },
                {
                    "name": "org.lwjgl:lwjgl:3.3.3:natives-windows",
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                            "sha1": "0123456789abcdef0123456789abcdef01234567",
                            "size": 612000,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
                        }
                    },
                    "rules": [{ "action": "allow", "os": { "name": "windows" } }]
                }
            ]
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let resolved = parse_vanilla_version(modern_json, &ctx).expect("parse modern version");

        assert_eq!(resolved.id, "1.21.4");
        assert_eq!(resolved.mc_version, "1.21.4");
        assert_eq!(resolved.loader, ModLoaderKind::Vanilla);
        assert_eq!(resolved.java_version.major_version, 21);
        assert_eq!(resolved.asset_index.id, "19");

        // macOS and Windows libraries excluded; remaining libraries in strict manifest order.
        let lib_names: Vec<&str> = resolved.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            lib_names,
            vec![
                "com.mojang:logging:1.2.7",
                "org.lwjgl:lwjgl:3.3.3",
                "org.lwjgl:lwjgl:3.3.3:natives-linux",
            ]
        );

        // Missing path on com.mojang:logging:1.2.7 synthesized from Maven coordinate + base url.
        let logging_art = resolved.libraries[0]
            .artifact
            .as_ref()
            .expect("logging artifact");
        assert_eq!(
            logging_art.path,
            "com/mojang/logging/1.2.7/logging-1.2.7.jar"
        );
        assert_eq!(
            logging_art.url,
            "https://libraries.minecraft.net/com/mojang/logging/1.2.7/logging-1.2.7.jar"
        );

        assert_eq!(
            resolved.jvm_arguments,
            vec![
                "-Dos.name=Linux",
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
            ]
        );
        assert_eq!(
            resolved.game_arguments,
            vec![
                "--username",
                "${auth_player_name}",
                "--version",
                "${version_name}",
            ]
        );
    }

    #[test]
    fn parses_legacy_version_json_with_minecraft_arguments_and_native_classifiers() {
        let legacy_json = r#"{
            "id": "1.7.10",
            "mainClass": "net.minecraft.client.main.Main",
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name}",
            "assetIndex": {
                "id": "1.7.10",
                "sha1": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                "size": 72000,
                "totalSize": 112000000,
                "url": "https://launchermeta.mojang.com/v1/packages/a94a8fe5ccb19ba61c4c0873d391e987982fbbd3/1.7.10.json"
            },
            "downloads": {
                "client": {
                    "sha1": "0123456789abcdef0123456789abcdef01234567",
                    "size": 5200000,
                    "url": "https://launcher.mojang.com/v1/objects/0123456789abcdef0123456789abcdef01234567/client.jar"
                }
            },
            "libraries": [
                {
                    "name": "org.lwjgl.lwjgl:lwjgl:2.9.1",
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/lwjgl/2.9.1/lwjgl-2.9.1.jar",
                            "sha1": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                            "size": 1014790,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/lwjgl/2.9.1/lwjgl-2.9.1.jar"
                        }
                    }
                },
                {
                    "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.1",
                    "downloads": {
                        "classifiers": {
                            "natives-linux-64": {
                                "path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.1/lwjgl-platform-2.9.1-natives-linux-64.jar",
                                "sha1": "fedcba9876543210fedcba9876543210fedcba98",
                                "size": 578680,
                                "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/lwjgl-platform/2.9.1/lwjgl-platform-2.9.1-natives-linux-64.jar"
                            }
                        }
                    },
                    "extract": {
                        "exclude": ["META-INF/"]
                    },
                    "natives": {
                        "linux": "natives-linux-${arch}",
                        "osx": "natives-osx",
                        "windows": "natives-windows-${arch}"
                    }
                }
            ]
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let resolved = parse_vanilla_version(legacy_json, &ctx).expect("parse legacy 1.7.10");

        assert_eq!(resolved.id, "1.7.10");
        assert_eq!(resolved.java_version.major_version, 8);
        assert_eq!(
            resolved.jvm_arguments,
            vec![
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
            ]
        );
        assert_eq!(
            resolved.game_arguments,
            vec![
                "--username",
                "${auth_player_name}",
                "--version",
                "${version_name}",
                "--gameDir",
                "${game_directory}",
                "--assetsDir",
                "${assets_root}",
                "--assetIndex",
                "${assets_index_name}",
            ]
        );

        assert_eq!(resolved.libraries.len(), 2);
        assert!(resolved.libraries[0].include_in_classpath);
        assert!(resolved.libraries[0].native_linux.is_none());

        let platform_lib = &resolved.libraries[1];
        assert_eq!(platform_lib.name, "org.lwjgl.lwjgl:lwjgl-platform:2.9.1");
        assert!(!platform_lib.include_in_classpath);
        assert!(platform_lib.artifact.is_none());
        let native_art = platform_lib
            .native_linux
            .as_ref()
            .expect("resolved native_linux classifier");
        assert_eq!(
            native_art.path,
            "org/lwjgl/lwjgl/lwjgl-platform/2.9.1/lwjgl-platform-2.9.1-natives-linux-64.jar"
        );
        assert_eq!(platform_lib.extract_exclude, vec!["META-INF/"]);
    }

    #[test]
    fn manifest_sorts_by_release_time_and_handles_26x() {
        let manifest_json = r#"{
            "latest": {
                "release": "26.3",
                "snapshot": "26.1-snapshot-1"
            },
            "versions": [
                {
                    "id": "1.7.10",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/1.7.10.json",
                    "releaseTime": "2014-05-14T19:29:23+00:00"
                },
                {
                    "id": "26.3",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/26.3.json",
                    "releaseTime": "2026-10-15T12:00:00+00:00"
                },
                {
                    "id": "a1.0.14",
                    "type": "old_alpha",
                    "url": "https://piston-meta.mojang.com/v1/packages/a1.0.14.json",
                    "releaseTime": "2010-08-11T00:00:00+00:00"
                },
                {
                    "id": "26.1-snapshot-1",
                    "type": "snapshot",
                    "url": "https://piston-meta.mojang.com/v1/packages/26.1-snapshot-1.json",
                    "releaseTime": "2026-02-01T10:00:00+00:00"
                },
                {
                    "id": "b1.7.3",
                    "type": "old_beta",
                    "url": "https://piston-meta.mojang.com/v1/packages/b1.7.3.json",
                    "releaseTime": "2011-07-08T00:00:00+00:00"
                },
                {
                    "id": "1.21.4",
                    "type": "release",
                    "url": "https://piston-meta.mojang.com/v1/packages/1.21.4.json",
                    "releaseTime": "2024-12-03T10:12:57+00:00"
                }
            ]
        }"#;

        let mut manifest = parse_version_manifest(manifest_json).expect("parse manifest");
        assert_eq!(manifest.versions.len(), 6);

        // Chronological sort: newest first
        manifest.sort_by_release_time_desc();
        let ids: Vec<&str> = manifest.versions.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["26.3", "26.1-snapshot-1", "1.21.4", "1.7.10", "b1.7.3", "a1.0.14"]
        );

        // Verify type filtering
        let releases = manifest.filter_by_type("release");
        assert_eq!(releases.len(), 3);
        assert_eq!(releases[0].id, "26.3");
        assert_eq!(releases[1].id, "1.21.4");
        assert_eq!(releases[2].id, "1.7.10");

        let snapshots = manifest.filter_by_type("snapshot");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, "26.1-snapshot-1");

        let betas = manifest.filter_by_type("old_beta");
        assert_eq!(betas.len(), 1);
        assert_eq!(betas[0].id, "b1.7.3");

        let alphas = manifest.filter_by_type("old_alpha");
        assert_eq!(alphas.len(), 1);
        assert_eq!(alphas[0].id, "a1.0.14");
    }

    #[test]
    fn parses_alpha_and_beta_fixtures() {
        let alpha_json = r#"{
            "id": "a1.0.14",
            "type": "old_alpha",
            "time": "2010-08-11T00:00:00+00:00",
            "releaseTime": "2010-08-11T00:00:00+00:00",
            "mainClass": "net.minecraft.client.Minecraft",
            "minecraftArguments": "${auth_player_name} ${auth_session} --gameDir ${game_directory} --assetsDir ${game_assets}",
            "assets": "legacy",
            "downloads": {
                "client": {
                    "sha1": "abcdef0123456789abcdef0123456789abcdef01",
                    "size": 123456,
                    "url": "https://launcher.mojang.com/v1/objects/abcdef/client.jar"
                }
            },
            "libraries": []
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let alpha = parse_vanilla_version(alpha_json, &ctx).expect("parse alpha");
        assert_eq!(alpha.id, "a1.0.14");
        assert_eq!(alpha.java_version.major_version, 8);
        assert_eq!(alpha.java_version.component, "jre-legacy");
        assert_eq!(alpha.main_class, "net.minecraft.client.Minecraft");
        assert_eq!(alpha.asset_index.id, "legacy");
        assert_eq!(
            alpha.game_arguments,
            vec![
                "${auth_player_name}",
                "${auth_session}",
                "--gameDir",
                "${game_directory}",
                "--assetsDir",
                "${game_assets}"
            ]
        );

        let beta_json = r#"{
            "id": "b1.7.3",
            "type": "old_beta",
            "time": "2011-07-08T00:00:00+00:00",
            "releaseTime": "2011-07-08T00:00:00+00:00",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "minecraftArguments": "${auth_player_name} ${auth_session} --gameDir ${game_directory} --assetsDir ${game_assets}",
            "assets": "legacy",
            "downloads": {
                "client": {
                    "sha1": "123456789abcdef0123456789abcdef012345678",
                    "size": 234567,
                    "url": "https://launcher.mojang.com/v1/objects/123456/client.jar"
                }
            },
            "libraries": []
        }"#;

        let beta = parse_vanilla_version(beta_json, &ctx).expect("parse beta");
        assert_eq!(beta.id, "b1.7.3");
        assert_eq!(beta.java_version.major_version, 8);
        assert_eq!(beta.main_class, "net.minecraft.launchwrapper.Launch");
    }

    #[test]
    fn parses_1_12_2_with_logging_config() {
        let v112_json = r#"{
            "id": "1.12.2",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {
                "id": "1.12",
                "sha1": "ef4f57b922226dd74b67710842b557c1ab3326f7",
                "size": 133200,
                "totalSize": 150000000,
                "url": "https://launchermeta.mojang.com/v1/packages/1.12.json"
            },
            "downloads": {
                "client": {
                    "sha1": "0123456789abcdef0123456789abcdef01234567",
                    "size": 10000000,
                    "url": "https://launcher.mojang.com/v1/objects/client.jar"
                }
            },
            "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --versionType ${version_type}",
            "logging": {
                "client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": {
                        "id": "client-1.12.xml",
                        "sha1": "ef4f57b922226dd74b67710842b557c1ab3326f7",
                        "size": 877,
                        "url": "https://launcher.mojang.com/v1/objects/ef4f57b922226dd74b67710842b557c1ab3326f7/client-1.12.xml"
                    },
                    "type": "log4j2-xml"
                }
            },
            "libraries": []
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let v112 = parse_vanilla_version(v112_json, &ctx).expect("parse 1.12.2");
        assert_eq!(v112.id, "1.12.2");
        assert_eq!(v112.java_version.major_version, 8);
        assert_eq!(v112.asset_index.id, "1.12");

        let logging = v112.logging.expect("logging config");
        let client_log = logging.client.expect("client logging");
        assert_eq!(client_log.argument, "-Dlog4j.configurationFile=${path}");
        assert_eq!(client_log.file.path, "log_configs/client-1.12.xml");
        assert_eq!(client_log.logging_type, "log4j2-xml");
    }

    #[test]
    fn parses_1_16_5_fixture() {
        let v116_json = r#"{
            "id": "1.16.5",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {
                "component": "jre-legacy",
                "majorVersion": 8
            },
            "arguments": {
                "game": [
                    "--username",
                    "${auth_player_name}",
                    "--version",
                    "${version_name}",
                    "--gameDir",
                    "${game_directory}",
                    "--assetsDir",
                    "${assets_root}",
                    "--assetIndex",
                    "${assets_index_name}",
                    "--uuid",
                    "${auth_uuid}",
                    "--accessToken",
                    "${auth_access_token}",
                    "--userType",
                    "${user_type}",
                    "--versionType",
                    "${version_type}"
                ],
                "jvm": [
                    {
                        "rules": [
                            {
                                "action": "allow",
                                "os": { "name": "osx" }
                            }
                        ],
                        "value": ["-XstartOnFirstThread"]
                    },
                    "-Djava.library.path=${natives_directory}",
                    "-cp",
                    "${classpath}"
                ]
            },
            "assetIndex": {
                "id": "1.16",
                "size": 150000,
                "totalSize": 200000000,
                "url": "https://launchermeta.mojang.com/v1/packages/1.16.json"
            },
            "downloads": {
                "client": {
                    "path": "versions/1.16.5/1.16.5.jar",
                    "size": 16000000,
                    "url": "https://launcher.mojang.com/v1/objects/116.jar"
                }
            },
            "libraries": []
        }"#;

        let linux_ctx = RuleContext::linux_x86_64();
        let v116 = parse_vanilla_version(v116_json, &linux_ctx).expect("parse 1.16.5");
        assert_eq!(v116.id, "1.16.5");
        assert_eq!(v116.java_version.major_version, 8);
        // On Linux, -XstartOnFirstThread must NOT be present
        assert!(!v116.jvm_arguments.iter().any(|a| a == "-XstartOnFirstThread"));
        assert_eq!(
            v116.jvm_arguments,
            vec!["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
        );

        let osx_ctx = RuleContext::macos_x86_64();
        let v116_osx = parse_vanilla_version(v116_json, &osx_ctx).expect("parse 1.16.5 osx");
        // On OSX, -XstartOnFirstThread MUST be present
        assert_eq!(v116_osx.jvm_arguments[0], "-XstartOnFirstThread");
    }

    #[test]
    fn parses_1_20_1_fixture_with_rules() {
        let v120_json = r#"{
            "id": "1.20.1",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {
                "component": "java-runtime-gamma",
                "majorVersion": 17
            },
            "arguments": {
                "game": [
                    "--username",
                    "${auth_player_name}",
                    {
                        "rules": [
                            {
                                "action": "allow",
                                "features": { "is_demo_user": true }
                            }
                        ],
                        "value": "--demo"
                    },
                    {
                        "rules": [
                            {
                                "action": "allow",
                                "features": { "has_custom_resolution": true }
                            }
                        ],
                        "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]
                    }
                ],
                "jvm": [
                    {
                        "rules": [
                            {
                                "action": "allow",
                                "os": { "name": "windows", "version": "^10\\." }
                            }
                        ],
                        "value": ["-Dos.name=Windows 10", "-Dos.version=10.0"]
                    },
                    "-cp",
                    "${classpath}"
                ]
            },
            "assetIndex": {
                "id": "5",
                "size": 180000,
                "totalSize": 250000000,
                "url": "https://launchermeta.mojang.com/v1/packages/5.json"
            },
            "downloads": {
                "client": {
                    "path": "versions/1.20.1/1.20.1.jar",
                    "size": 25000000,
                    "url": "https://launcher.mojang.com/v1/objects/120.jar"
                }
            },
            "libraries": []
        }"#;

        let linux_ctx = RuleContext::linux_x86_64();
        let v120 = parse_vanilla_version(v120_json, &linux_ctx).expect("parse 1.20.1");
        assert_eq!(v120.java_version.major_version, 17);
        // Features disabled by default: no --demo, no --width
        assert!(!v120.game_arguments.iter().any(|a| a == "--demo"));
        assert!(!v120.game_arguments.iter().any(|a| a == "--width"));
        assert!(!v120.jvm_arguments.iter().any(|a| a.contains("Windows 10")));

        // Windows 10 context
        let mut win10_ctx = RuleContext::windows_x86_64();
        win10_ctx.features.insert("is_demo_user".to_string(), true);
        let v120_win = parse_vanilla_version(v120_json, &win10_ctx).expect("parse win10");
        assert!(v120_win.jvm_arguments.iter().any(|a| a == "-Dos.name=Windows 10"));
        assert!(v120_win.game_arguments.iter().any(|a| a == "--demo"));
    }

    #[test]
    fn parses_26_x_fixture() {
        let v26_json = r#"{
            "id": "26.3",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {
                "component": "java-runtime-epsilon",
                "majorVersion": 25
            },
            "arguments": {
                "game": [
                    "--username",
                    "${auth_player_name}",
                    "--version",
                    "${version_name}",
                    {
                        "rules": [
                            {
                                "action": "allow",
                                "features": { "quick_play_path": true }
                            }
                        ],
                        "value": ["--quickPlayPath", "${quick_play_path}"]
                    }
                ],
                "jvm": [
                    {
                        "rules": [
                            {
                                "action": "allow",
                                "os": { "arch": "arm64" }
                            }
                        ],
                        "value": ["-XX:+UseZGC"]
                    },
                    "-cp",
                    "${classpath}"
                ]
            },
            "assetIndex": {
                "id": "34",
                "size": 220000,
                "totalSize": 350000000,
                "url": "https://launchermeta.mojang.com/v1/packages/34.json"
            },
            "downloads": {
                "client": {
                    "path": "versions/26.3/26.3.jar",
                    "size": 35000000,
                    "url": "https://launcher.mojang.com/v1/objects/263.jar"
                }
            },
            "libraries": []
        }"#;

        let linux_x64 = RuleContext::linux_x86_64();
        let v26 = parse_vanilla_version(v26_json, &linux_x64).expect("parse 26.3 on x86_64");
        assert_eq!(v26.id, "26.3");
        assert_eq!(v26.java_version.major_version, 25);
        assert_eq!(v26.java_version.component, "java-runtime-epsilon");
        // No arm64 flag on x86_64
        assert!(!v26.jvm_arguments.iter().any(|a| a == "-XX:+UseZGC"));
        // quick_play_path disabled by default
        assert!(!v26.game_arguments.iter().any(|a| a == "--quickPlayPath"));

        // On ARM64:
        let linux_arm = RuleContext::linux_arm64();
        let v26_arm = parse_vanilla_version(v26_json, &linux_arm).expect("parse 26.3 on arm64");
        assert!(v26_arm.jvm_arguments.iter().any(|a| a == "-XX:+UseZGC"));

        // Snapshot with suffix
        let snap_json = r#"{
            "id": "26.1-snapshot-1",
            "type": "snapshot",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": []
        }"#;
        let snap = parse_vanilla_version(snap_json, &linux_x64).expect("parse 26.1-snapshot-1");
        assert_eq!(snap.id, "26.1-snapshot-1");
        assert_eq!(snap.java_version.major_version, 25);
        assert_eq!(snap.java_version.component, "java-runtime-epsilon");
    }

    #[test]
    fn parses_inherits_from_and_merges_properties() {
        let parent_json = r#"{
            "id": "1.21.1",
            "mainClass": "net.minecraft.client.main.Main",
            "javaVersion": {
                "component": "java-runtime-delta",
                "majorVersion": 21
            },
            "assetIndex": {
                "id": "17",
                "size": 4096,
                "totalSize": 1000000,
                "url": "https://piston-meta.mojang.com/v1/packages/17.json"
            },
            "downloads": {
                "client": {
                    "path": "versions/1.21.1/1.21.1.jar",
                    "size": 25000000,
                    "url": "https://launcher.mojang.com/v1/objects/1211.jar"
                }
            },
            "arguments": {
                "jvm": ["-Dparent.prop=true"],
                "game": ["--parentGame"]
            },
            "logging": {
                "client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": {
                        "id": "client-1.21.1.xml",
                        "size": 100,
                        "url": "https://launcher.mojang.com/v1/objects/log.xml"
                    },
                    "type": "log4j2-xml"
                }
            },
            "libraries": [
                {
                    "name": "com.google.code.gson:gson:2.10.1",
                    "downloads": {
                        "artifact": {
                            "path": "com/google/code/gson/gson/2.10.1/gson-2.10.1.jar",
                            "size": 1024,
                            "url": "https://repo/gson.jar"
                        }
                    }
                }
            ]
        }"#;

        let child_json = r#"{
            "id": "1.21.1-OptiFine_HD_U_J1",
            "inheritsFrom": "1.21.1",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "arguments": {
                "jvm": ["-Dchild.prop=true"],
                "game": ["--childGame"]
            },
            "libraries": [
                {
                    "name": "optifine:OptiFine:1.21.1_HD_U_J1",
                    "downloads": {
                        "artifact": {
                            "path": "optifine/OptiFine/1.21.1_HD_U_J1/OptiFine-1.21.1_HD_U_J1.jar",
                            "size": 2048,
                            "url": "https://optifine.net/download"
                        }
                    }
                }
            ]
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let parent = parse_vanilla_version(parent_json, &ctx).expect("parse parent");
        let child = parse_vanilla_version(child_json, &ctx).expect("parse child");

        let merged = crate::merge::merge_resolved_versions(&parent, &child).expect("merge versions");
        assert_eq!(merged.id, "1.21.1-OptiFine_HD_U_J1");
        assert_eq!(merged.mc_version, "1.21.1");
        assert_eq!(merged.main_class, "net.minecraft.launchwrapper.Launch");
        // Inherited assetIndex
        assert_eq!(merged.asset_index.id, "17");
        // Inherited client jar
        assert_eq!(merged.client_jar.path, "versions/1.21.1/1.21.1.jar");
        // Inherited javaVersion
        assert_eq!(merged.java_version.major_version, 21);
        // Inherited logging
        assert!(merged.logging.is_some());
        assert_eq!(
            merged.logging.as_ref().unwrap().client.as_ref().unwrap().argument,
            "-Dlog4j.configurationFile=${path}"
        );
        // Libraries: child prepended, parent follows
        assert_eq!(merged.libraries.len(), 2);
        assert_eq!(merged.libraries[0].name, "optifine:OptiFine:1.21.1_HD_U_J1");
        assert_eq!(merged.libraries[1].name, "com.google.code.gson:gson:2.10.1");
        // Arguments concatenated
        assert_eq!(merged.jvm_arguments, vec!["-Dparent.prop=true", "-Dchild.prop=true"]);
        assert_eq!(merged.game_arguments, vec!["--parentGame", "--childGame"]);
    }
}
