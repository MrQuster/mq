//! Fabric, Quilt, and local `inheritsFrom` (Forge / NeoForge / Fabric) modloader profile
//! metadata resolution and Maven coordinate artifact mapping.

use std::collections::{BTreeMap, HashSet};

use mq_core::{ModLoaderKind, MqError, MqResult, Sha1Digest};
use serde::{de, Deserialize, Deserializer, Serialize};
use url::Url;

use crate::models::{ArtifactDownload, ResolvedLibrary, RuleContext};

/// Default base URL for the official Fabric Meta v2 API.
pub const FABRIC_META_BASE_URL: &str = "https://meta.fabricmc.net";

/// Default base URL for the official Quilt Meta v3 API.
pub const QUILT_META_BASE_URL: &str = "https://meta.quiltmc.org";

/// Fallback Maven repository URL when a profile library omits both `downloads.artifact` and `url`.
pub const DEFAULT_LIBRARIES_BASE_URL: &str = "https://libraries.minecraft.net";

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

    /// Converts this raw profile library entry into a [`ResolvedLibrary`].
    pub fn to_resolved_library(&self) -> MqResult<ResolvedLibrary> {
        let coord_artifact = maven_coord_to_artifact(
            &self.name,
            self.url.as_deref(),
            self.sha1,
            self.size.unwrap_or(0),
        )?;

        let (artifact, native_linux) = match &self.downloads {
            Some(downloads) => {
                let primary = if let Some(raw_art) = &downloads.artifact {
                    Some(resolve_raw_artifact(
                        raw_art,
                        &coord_artifact,
                        self.url.as_deref(),
                        self.sha1,
                        self.size,
                    )?)
                } else if !downloads.classifiers.is_empty() {
                    None
                } else {
                    Some(coord_artifact.clone())
                };

                let native = if let Some(raw_nat) = downloads.classifiers.get("natives-linux") {
                    let fallback_native = maven_coord_to_artifact(
                        &format!(
                            "{}:natives-linux",
                            self.name.split('@').next().unwrap_or(&self.name)
                        ),
                        self.url.as_deref(),
                        None,
                        0,
                    )
                    .unwrap_or_else(|_| coord_artifact.clone());
                    Some(resolve_raw_artifact(
                        raw_nat,
                        &fallback_native,
                        self.url.as_deref(),
                        None,
                        None,
                    )?)
                } else {
                    None
                };

                (primary, native)
            }
            None => (Some(coord_artifact), None),
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
        libraries.push(raw_lib.to_resolved_library()?);
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
        client_jar: None,
    })
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
