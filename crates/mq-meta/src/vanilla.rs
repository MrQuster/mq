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
/// - `major_version: 21` for MC >= 1.20.5
/// - `major_version: 17` for MC 1.18..=1.20.4
/// - `major_version: 8` for older versions
pub fn infer_java_version(mc_version: &str) -> JavaVersionSpec {
    let core = mc_version
        .trim()
        .split(['-', '+', ' '])
        .next()
        .unwrap_or("");
    let nums: Vec<u32> = core
        .split('.')
        .map_while(|seg| seg.parse::<u32>().ok())
        .collect();

    let major_version = match nums.as_slice() {
        [1, minor, patch, ..] => {
            if *minor > 20 || (*minor == 20 && *patch >= 5) {
                21
            } else if *minor >= 18 {
                17
            } else {
                8
            }
        }
        [1, minor] => {
            if *minor > 20 {
                21
            } else if *minor >= 18 {
                17
            } else {
                8
            }
        }
        [major, ..] if *major > 1 => 21,
        _ => 8,
    };

    let component = match major_version {
        21.. => "java-runtime-delta",
        17..=20 => "java-runtime-gamma",
        _ => "jre-legacy",
    };

    JavaVersionSpec {
        component: component.to_string(),
        major_version,
    }
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaRawVersion {
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
    if id.is_empty() {
        return Err(MqError::Metadata(
            "version.json 'id' must not be empty".to_string(),
        ));
    }

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
            id: raw.assets.unwrap_or_else(|| mc_version.clone()),
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
            path: format!("versions/{id}/{id}.jar"),
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

    parse_vanilla_version(&body, ctx)
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
        assert_eq!(infer_java_version("1.7.10").major_version, 8);
        assert_eq!(infer_java_version("1.12.2").major_version, 8);
        assert_eq!(infer_java_version("1.16.5").major_version, 8);
        assert_eq!(infer_java_version("1.18").major_version, 17);
        assert_eq!(infer_java_version("1.20.4").major_version, 17);
        assert_eq!(infer_java_version("1.20.5").major_version, 21);
        assert_eq!(infer_java_version("1.21.4").major_version, 21);
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
}
