//! Resilient `version.json` parsing, cycle-safe `inheritsFrom` resolution,
//! and forward-compatibility with modern Minecraft formats (including 26.x).

#![allow(clippy::result_large_err)]

use std::collections::{HashMap, HashSet};
use std::future::Future;

use mq_core::{ModLoaderKind, Sha1Digest};
use mq_meta::{
    infer_java_version, ArtifactDownload, AssetIndexPointer, JavaVersionSpec, LoggingConfig,
    ResolvedLibrary, ResolvedVersion, RuleContext,
};
use serde::{Deserialize, Serialize};

use crate::state::{InstallError, InstallStage};

/// Maximum allowed depth for `inheritsFrom` chains to prevent stack overflow or DoS attacks.
pub const MAX_INHERITS_FROM_DEPTH: usize = 16;

/// Maximally tolerant deserialization representation of `version.json`.
/// Unknown fields are ignored by default. All fields are optional or defaulted.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleVersionJson {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(default, rename = "mainClass")]
    pub main_class: Option<String>,
    #[serde(default, rename = "javaVersion")]
    pub java_version: Option<FlexibleJavaVersion>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<FlexibleAssetIndex>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<FlexibleDownloads>,
    #[serde(default)]
    pub libraries: Vec<FlexibleLibrary>,
    #[serde(default)]
    pub arguments: Option<FlexibleArguments>,
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub logging: Option<FlexibleLogging>,
    #[serde(default, rename = "type")]
    pub release_type: Option<String>,
}

impl FlexibleVersionJson {
    /// Determines the effective identifier of this version document.
    #[must_use]
    pub fn effective_id(&self) -> String {
        self.id
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.inherits_from
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Determines the Minecraft game version.
    #[must_use]
    pub fn effective_mc_version(&self) -> String {
        self.inherits_from
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.id
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Converts this flexible document into a standard [`ResolvedVersion`],
    /// resolving libraries and arguments against `ctx`.
    pub fn to_resolved_version(&self, ctx: &RuleContext) -> Result<ResolvedVersion, InstallError> {
        let id = self.effective_id();
        let mc_version = self.effective_mc_version();

        let main_class = self
            .main_class
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                if self.inherits_from.is_some() {
                    Some(String::new())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let java_version = self
            .java_version
            .as_ref()
            .and_then(|j| {
                j.major_version.map(|m| JavaVersionSpec {
                    major_version: m,
                    component: j
                        .component
                        .clone()
                        .unwrap_or_else(|| format!("java-runtime-{m}")),
                })
            })
            .unwrap_or_else(|| infer_java_version(&mc_version));

        let asset_index = match &self.asset_index {
            Some(ai) => AssetIndexPointer {
                id: ai
                    .id
                    .clone()
                    .or_else(|| self.assets.clone())
                    .unwrap_or_else(|| mc_version.clone()),
                sha1: ai
                    .sha1
                    .as_deref()
                    .and_then(|s| Sha1Digest::from_hex(s).ok()),
                size: ai.size.unwrap_or(0),
                total_size: ai.total_size.unwrap_or(0),
                url: ai.url.clone().unwrap_or_default(),
            },
            None => AssetIndexPointer {
                id: self.assets.clone().unwrap_or_else(|| {
                    if self.inherits_from.is_some() {
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

        let client_jar = match self.downloads.as_ref().and_then(|d| d.client.as_ref()) {
            Some(client) => ArtifactDownload {
                path: client
                    .path
                    .clone()
                    .filter(|p| !p.trim().is_empty())
                    .unwrap_or_else(|| format!("versions/{id}/{id}.jar")),
                url: client.url.clone().unwrap_or_default(),
                sha1: client
                    .sha1
                    .as_deref()
                    .and_then(|s| Sha1Digest::from_hex(s).ok()),
                size: client.size.unwrap_or(0),
            },
            None => ArtifactDownload {
                path: if self.inherits_from.is_some() {
                    String::new()
                } else {
                    format!("versions/{id}/{id}.jar")
                },
                url: String::new(),
                sha1: None,
                size: 0,
            },
        };

        let mut libraries = Vec::new();
        for lib in &self.libraries {
            if let Some(resolved) = resolve_flexible_library(lib, ctx) {
                libraries.push(resolved);
            }
        }

        let mut jvm_arguments = Vec::new();
        let mut game_arguments = Vec::new();

        if let Some(ref args) = self.arguments {
            for arg_val in &args.jvm {
                extract_flexible_arg(arg_val, ctx, &mut jvm_arguments);
            }
            for arg_val in &args.game {
                extract_flexible_arg(arg_val, ctx, &mut game_arguments);
            }
        }

        if let Some(ref legacy_args) = self.minecraft_arguments {
            if game_arguments.is_empty() {
                game_arguments.extend(legacy_args.split_whitespace().map(str::to_string));
            }
        }

        let logging = self.logging.as_ref().map(|l| LoggingConfig {
            client: l.client.as_ref().map(|c| mq_meta::LoggingClientConfig {
                argument: c.argument.clone().unwrap_or_default(),
                file: ArtifactDownload {
                    path: c
                        .file
                        .as_ref()
                        .and_then(|f| f.path.clone())
                        .unwrap_or_default(),
                    url: c
                        .file
                        .as_ref()
                        .and_then(|f| f.url.clone())
                        .unwrap_or_default(),
                    sha1: c
                        .file
                        .as_ref()
                        .and_then(|f| f.sha1.as_deref().and_then(|s| Sha1Digest::from_hex(s).ok())),
                    size: c.file.as_ref().and_then(|f| f.size).unwrap_or(0),
                },
                logging_type: c.logging_type.clone().unwrap_or_default(),
            }),
        });

        Ok(ResolvedVersion {
            id,
            mc_version,
            loader: ModLoaderKind::Vanilla,
            loader_version: None,
            inherits_from: self.inherits_from.clone(),
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
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleJavaVersion {
    #[serde(default, rename = "majorVersion")]
    pub major_version: Option<u32>,
    #[serde(default)]
    pub component: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleAssetIndex {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default, rename = "totalSize")]
    pub total_size: Option<u64>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleDownloads {
    #[serde(default)]
    pub client: Option<FlexibleArtifact>,
    #[serde(default)]
    pub server: Option<FlexibleArtifact>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleArtifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleLibrary {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub downloads: Option<FlexibleLibraryDownloads>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Vec<FlexibleRule>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<FlexibleExtract>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleLibraryDownloads {
    #[serde(default)]
    pub artifact: Option<FlexibleArtifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, FlexibleArtifact>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleExtract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleRule {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub os: Option<FlexibleRuleOs>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleRuleOs {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleArguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleLogging {
    #[serde(default)]
    pub client: Option<FlexibleLoggingClient>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FlexibleLoggingClient {
    #[serde(default)]
    pub argument: Option<String>,
    #[serde(default)]
    pub file: Option<FlexibleArtifact>,
    #[serde(default, rename = "type")]
    pub logging_type: Option<String>,
}

/// Helper that evaluates a rule against the current platform.
fn evaluate_rule(rule: &FlexibleRule, ctx: &RuleContext) -> bool {
    let action_allow = rule.action.as_deref().unwrap_or("allow") == "allow";

    if let Some(ref os) = rule.os {
        if let Some(ref os_name) = os.name {
            let matches_os = match os_name.to_lowercase().as_str() {
                "linux" => ctx.os_name == "linux",
                "osx" | "macos" => ctx.os_name == "osx",
                "windows" => ctx.os_name == "windows",
                _ => false,
            };
            if !matches_os {
                return !action_allow;
            }
        }
        if let Some(ref arch) = os.arch {
            if arch.to_lowercase() != ctx.os_arch.to_lowercase() {
                return !action_allow;
            }
        }
    }

    if let Some(ref features) = rule.features {
        for (feat, &expected) in features {
            let actual = ctx.features.get(feat).copied().unwrap_or(false);
            if actual != expected {
                return !action_allow;
            }
        }
    }

    action_allow
}

/// Evaluates a list of rules: default is allowed if rules is empty.
fn is_library_allowed(rules: &[FlexibleRule], ctx: &RuleContext) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if evaluate_rule(rule, ctx) {
            allowed = rule.action.as_deref().unwrap_or("allow") == "allow";
        }
    }
    allowed
}

/// Resolves a single flexible library against the context.
fn resolve_flexible_library(lib: &FlexibleLibrary, ctx: &RuleContext) -> Option<ResolvedLibrary> {
    if !is_library_allowed(&lib.rules, ctx) {
        return None;
    }

    let name = lib
        .name
        .clone()
        .unwrap_or_else(|| "unknown:lib:1.0".to_string());

    let artifact = lib
        .downloads
        .as_ref()
        .and_then(|d| d.artifact.as_ref())
        .map(|art| ArtifactDownload {
            path: art.path.clone().unwrap_or_else(|| {
                // Approximate maven coordinate path if missing
                let parts: Vec<&str> = name.split(':').collect();
                if parts.len() >= 3 {
                    let group = parts[0].replace('.', "/");
                    let artifact = parts[1];
                    let version = parts[2];
                    format!("{group}/{artifact}/{version}/{artifact}-{version}.jar")
                } else {
                    format!("libraries/{name}.jar")
                }
            }),
            url: art.url.clone().unwrap_or_default(),
            sha1: art
                .sha1
                .as_deref()
                .and_then(|s| Sha1Digest::from_hex(s).ok()),
            size: art.size.unwrap_or(0),
        });

    let native_linux = lib
        .downloads
        .as_ref()
        .and_then(|d| d.classifiers.as_ref())
        .and_then(|classifiers| {
            classifiers
                .get("natives-linux")
                .or_else(|| classifiers.get("natives-linux-x86_64"))
                .or_else(|| classifiers.get("linux"))
        })
        .map(|nat| ArtifactDownload {
            path: nat.path.clone().unwrap_or_default(),
            url: nat.url.clone().unwrap_or_default(),
            sha1: nat
                .sha1
                .as_deref()
                .and_then(|s| Sha1Digest::from_hex(s).ok()),
            size: nat.size.unwrap_or(0),
        });

    let extract_exclude = lib
        .extract
        .as_ref()
        .map(|e| e.exclude.clone())
        .unwrap_or_default();

    if artifact.is_none() && native_linux.is_none() {
        return None;
    }

    let include_in_classpath = artifact.is_some();

    Some(ResolvedLibrary {
        name,
        artifact,
        native_linux,
        extract_exclude,
        include_in_classpath,
    })
}

/// Extracts arguments from arbitrary JSON values into strings.
fn extract_flexible_arg(val: &serde_json::Value, ctx: &RuleContext, out: &mut Vec<String>) {
    match val {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(arr) => {
            for item in arr {
                extract_flexible_arg(item, ctx, out);
            }
        }
        serde_json::Value::Object(map) => {
            // Complex argument object with rules and values
            let rules_allowed = if let Some(rules_val) = map.get("rules") {
                if let Ok(rules) = serde_json::from_value::<Vec<FlexibleRule>>(rules_val.clone()) {
                    is_library_allowed(&rules, ctx)
                } else {
                    true
                }
            } else {
                true
            };

            if rules_allowed {
                if let Some(value) = map.get("value") {
                    extract_flexible_arg(value, ctx, out);
                }
            }
        }
        _ => {}
    }
}

/// Parses a raw `version.json` string into a validated [`FlexibleVersionJson`].
/// Returns a structured [`InstallError`] with clear Russian messages if the JSON is malformed or truncated.
pub fn parse_version_document(raw_json: &str) -> Result<FlexibleVersionJson, InstallError> {
    if raw_json.trim().is_empty() {
        return Err(InstallError {
            stage: InstallStage::VersionJson,
            localized_title: "Пустой файл конфигурации версии".to_string(),
            localized_details: "Полученный документ version.json пуст".to_string(),
            target_url: None,
            file_path: None,
            http_status: None,
            is_retryable: false,
            raw_cause: "empty json payload".to_string(),
        });
    }

    serde_json::from_str::<FlexibleVersionJson>(raw_json).map_err(|err| InstallError {
        stage: InstallStage::VersionJson,
        localized_title: "Ошибка синтаксиса version.json".to_string(),
        localized_details: format!(
            "Файл конфигурации версии содержит некорректный или обрезанный JSON: {err}"
        ),
        target_url: None,
        file_path: None,
        http_status: None,
        is_retryable: false,
        raw_cause: err.to_string(),
    })
}

/// Parses and resolves a `version.json` string into [`ResolvedVersion`] against `ctx`,
/// preferring the strict metadata parser when available and falling back to the flexible
/// 26.x-compatible parser when novel or optional fields are present.
pub fn parse_flexible_version_json(
    raw_json: &str,
    ctx: &RuleContext,
) -> Result<ResolvedVersion, InstallError> {
    // Check basic validity first
    let doc = parse_version_document(raw_json)?;

    // Try mq_meta's parser first if standard
    if let Ok(resolved) = mq_meta::parse_vanilla_version(raw_json, ctx) {
        return Ok(resolved);
    }

    // Fall back to flexible resolution for 26.x / unusual variants
    doc.to_resolved_version(ctx)
}

/// Merges two resolved versions, giving the child precedence while ensuring compatibility.
pub fn merge_resolved_versions_tolerant(
    parent: &ResolvedVersion,
    child: &ResolvedVersion,
) -> Result<ResolvedVersion, InstallError> {
    // If inherits_from matches parent, standard merge works cleanly
    match mq_meta::merge_resolved_versions(parent, child) {
        Ok(merged) => Ok(merged),
        Err(_) => {
            // If ID/mc_version check in standard merge was strict, perform tolerant merge
            let id = if !child.id.trim().is_empty() {
                child.id.clone()
            } else {
                parent.id.clone()
            };

            let loader = if child.loader != ModLoaderKind::Vanilla {
                child.loader
            } else {
                parent.loader
            };

            let loader_version = child
                .loader_version
                .clone()
                .or_else(|| parent.loader_version.clone());

            let main_class = if !child.main_class.trim().is_empty() {
                child.main_class.clone()
            } else {
                parent.main_class.clone()
            };

            let java_version = if child.java_version.major_version > 0 {
                child.java_version.clone()
            } else {
                parent.java_version.clone()
            };

            let asset_index = if !child.asset_index.id.trim().is_empty() {
                child.asset_index.clone()
            } else {
                parent.asset_index.clone()
            };

            let client_jar = if !child.client_jar.url.trim().is_empty() {
                child.client_jar.clone()
            } else {
                parent.client_jar.clone()
            };

            // Merge libraries: child libraries override parent duplicate modules
            let mut merged_libs = child.libraries.clone();
            let child_names: HashSet<&str> =
                child.libraries.iter().map(|l| l.name.as_str()).collect();

            for parent_lib in &parent.libraries {
                if !child_names.contains(parent_lib.name.as_str()) {
                    merged_libs.push(parent_lib.clone());
                }
            }

            let mut jvm_arguments = parent.jvm_arguments.clone();
            jvm_arguments.extend(child.jvm_arguments.clone());

            let mut game_arguments = parent.game_arguments.clone();
            game_arguments.extend(child.game_arguments.clone());

            let logging = child.logging.clone().or_else(|| parent.logging.clone());

            Ok(ResolvedVersion {
                id,
                mc_version: parent.mc_version.clone(),
                loader,
                loader_version,
                inherits_from: child.inherits_from.clone(),
                main_class,
                java_version,
                asset_index,
                client_jar,
                libraries: merged_libs,
                jvm_arguments,
                game_arguments,
                logging,
            })
        }
    }
}

/// Resolves an `inheritsFrom` chain synchronously and iteratively with cycle detection
/// (`HashSet` of visited version IDs) and a strict depth limit of [`MAX_INHERITS_FROM_DEPTH`].
pub fn resolve_version_inherits_from<F>(
    root_json: &str,
    ctx: &RuleContext,
    mut fetch_parent: F,
) -> Result<ResolvedVersion, InstallError>
where
    F: FnMut(&str) -> Result<String, InstallError>,
{
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<ResolvedVersion> = Vec::new();
    let mut current_json = root_json.to_string();

    loop {
        let parsed = parse_flexible_version_json(&current_json, ctx)?;
        let current_id = parsed.id.trim().to_string();

        if current_id.is_empty() {
            return Err(InstallError {
                stage: InstallStage::VersionJson,
                localized_title: "Некорректный идентификатор версии".to_string(),
                localized_details: "Поле 'id' в version.json отсутствует или пусто".to_string(),
                target_url: None,
                file_path: None,
                http_status: None,
                is_retryable: false,
                raw_cause: "empty version id".to_string(),
            });
        }

        // Cycle check: has current_id already been visited?
        if !visited.insert(current_id.clone()) {
            return Err(InstallError {
                stage: InstallStage::VersionJson,
                localized_title: "Циклическая зависимость версий".to_string(),
                localized_details: format!(
                    "Обнаружен цикл в наследовании версий (inheritsFrom): версия '{current_id}' уже посещена в цепочке"
                ),
                target_url: None,
                file_path: None,
                http_status: None,
                is_retryable: false,
                raw_cause: format!("cycle detected for version '{current_id}'"),
            });
        }

        let parent_id_opt = parsed
            .inherits_from
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != &current_id);

        // Check if version inherits from itself directly
        if let Some(ref p) = parsed.inherits_from {
            if p.trim() == current_id {
                return Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Циклическая зависимость версий".to_string(),
                    localized_details: format!(
                        "Версия '{current_id}' ссылается сама на себя через inheritsFrom"
                    ),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: format!("self-referential inheritsFrom in '{current_id}'"),
                });
            }
        }

        if let Some(parent_id) = parent_id_opt {
            if visited.contains(&parent_id) {
                return Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Циклическая зависимость версий".to_string(),
                    localized_details: format!(
                        "Обнаружен цикл в наследовании версий: родительская версия '{parent_id}' уже посещена"
                    ),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: format!("cycle detected: '{current_id}' -> '{parent_id}'"),
                });
            }

            if chain.len() >= MAX_INHERITS_FROM_DEPTH {
                return Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Превышена глубина наследования".to_string(),
                    localized_details: format!(
                        "Глубина цепочки inheritsFrom превысила допустимый максимум ({MAX_INHERITS_FROM_DEPTH})"
                    ),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: format!("inheritsFrom depth exceeded {MAX_INHERITS_FROM_DEPTH}"),
                });
            }

            chain.push(parsed);
            current_json = fetch_parent(&parent_id)?;
        } else {
            chain.push(parsed);
            break;
        }
    }

    // Chain: [leaf, intermediate_1, ..., root_parent]
    // Reverse to fold from root parent to leaf child
    chain.reverse();
    let mut iter = chain.into_iter();
    let root = iter.next().expect("chain is non-empty");
    let mut resolved = root;

    for child in iter {
        resolved = merge_resolved_versions_tolerant(&resolved, &child)?;
    }

    Ok(resolved)
}

/// Resolves an `inheritsFrom` chain asynchronously and iteratively with cycle detection
/// (`HashSet` of visited version IDs) and a strict depth limit of [`MAX_INHERITS_FROM_DEPTH`].
pub async fn resolve_version_inherits_from_async<F, Fut>(
    root_json: &str,
    ctx: &RuleContext,
    mut fetch_parent: F,
) -> Result<ResolvedVersion, InstallError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, InstallError>>,
{
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<ResolvedVersion> = Vec::new();
    let mut current_json = root_json.to_string();

    loop {
        let parsed = parse_flexible_version_json(&current_json, ctx)?;
        let current_id = parsed.id.trim().to_string();

        if current_id.is_empty() {
            return Err(InstallError {
                stage: InstallStage::VersionJson,
                localized_title: "Некорректный идентификатор версии".to_string(),
                localized_details: "Поле 'id' в version.json отсутствует или пусто".to_string(),
                target_url: None,
                file_path: None,
                http_status: None,
                is_retryable: false,
                raw_cause: "empty version id".to_string(),
            });
        }

        // Cycle check: has current_id already been visited?
        if !visited.insert(current_id.clone()) {
            return Err(InstallError {
                stage: InstallStage::VersionJson,
                localized_title: "Циклическая зависимость версий".to_string(),
                localized_details: format!(
                    "Обнаружен цикл в наследовании версий (inheritsFrom): версия '{current_id}' уже посещена в цепочке"
                ),
                target_url: None,
                file_path: None,
                http_status: None,
                is_retryable: false,
                raw_cause: format!("cycle detected for version '{current_id}'"),
            });
        }

        let parent_id_opt = parsed
            .inherits_from
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != &current_id);

        if let Some(ref p) = parsed.inherits_from {
            if p.trim() == current_id {
                return Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Циклическая зависимость версий".to_string(),
                    localized_details: format!(
                        "Версия '{current_id}' ссылается сама на себя через inheritsFrom"
                    ),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: format!("self-referential inheritsFrom in '{current_id}'"),
                });
            }
        }

        if let Some(parent_id) = parent_id_opt {
            if visited.contains(&parent_id) {
                return Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Циклическая зависимость версий".to_string(),
                    localized_details: format!(
                        "Обнаружен цикл в наследовании версий: родительская версия '{parent_id}' уже посещена"
                    ),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: format!("cycle detected: '{current_id}' -> '{parent_id}'"),
                });
            }

            if chain.len() >= MAX_INHERITS_FROM_DEPTH {
                return Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Превышена глубина наследования".to_string(),
                    localized_details: format!(
                        "Глубина цепочки inheritsFrom превысила допустимый максимум ({MAX_INHERITS_FROM_DEPTH})"
                    ),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: format!("inheritsFrom depth exceeded {MAX_INHERITS_FROM_DEPTH}"),
                });
            }

            chain.push(parsed);
            current_json = fetch_parent(parent_id).await?;
        } else {
            chain.push(parsed);
            break;
        }
    }

    chain.reverse();
    let mut iter = chain.into_iter();
    let root = iter.next().expect("chain is non-empty");
    let mut resolved = root;

    for child in iter {
        resolved = merge_resolved_versions_tolerant(&resolved, &child)?;
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_truncated_or_corrupted_json_safely_returning_typed_error() {
        let truncated = r#"{"id": "26.3", "libraries": ["#;
        let err = parse_version_document(truncated).unwrap_err();
        assert_eq!(err.stage, InstallStage::VersionJson);
        assert!(err.localized_title.contains("Ошибка синтаксиса"));
        assert!(err.localized_details.contains("JSON"));

        let empty = "";
        let err_empty = parse_version_document(empty).unwrap_err();
        assert_eq!(err_empty.stage, InstallStage::VersionJson);
        assert!(err_empty.localized_title.contains("Пустой"));
    }

    #[test]
    fn detects_self_referential_cycle_in_inherits_from() {
        let json = r#"{
            "id": "1.21.1-custom",
            "inheritsFrom": "1.21.1-custom",
            "mainClass": "net.minecraft.client.main.Main"
        }"#;
        let ctx = RuleContext::linux_x86_64();
        let err = resolve_version_inherits_from(json, &ctx, |_| unreachable!()).unwrap_err();
        assert_eq!(err.stage, InstallStage::VersionJson);
        assert!(err.localized_title.contains("Циклическая"));
        assert!(err.localized_details.contains("ссылается сама на себя"));
    }

    #[test]
    fn detects_mutual_cycle_between_two_versions() {
        let ver_a = r#"{
            "id": "ver-a",
            "inheritsFrom": "ver-b",
            "mainClass": "net.minecraft.client.main.Main"
        }"#;
        let ver_b = r#"{
            "id": "ver-b",
            "inheritsFrom": "ver-a",
            "mainClass": "net.minecraft.client.main.Main"
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let err = resolve_version_inherits_from(ver_a, &ctx, |parent_id| {
            if parent_id == "ver-b" {
                Ok(ver_b.to_string())
            } else {
                panic!("unexpected parent {parent_id}");
            }
        })
        .unwrap_err();

        assert_eq!(err.stage, InstallStage::VersionJson);
        assert!(err.localized_title.contains("Циклическая"));
        assert!(err.localized_details.contains("ver-a"));
    }

    #[test]
    fn detects_indirect_three_node_cycle() {
        let ver_a = r#"{"id": "ver-a", "inheritsFrom": "ver-b"}"#;
        let ver_b = r#"{"id": "ver-b", "inheritsFrom": "ver-c"}"#;
        let ver_c = r#"{"id": "ver-c", "inheritsFrom": "ver-a"}"#;

        let ctx = RuleContext::linux_x86_64();
        let err = resolve_version_inherits_from(ver_a, &ctx, |id| match id {
            "ver-b" => Ok(ver_b.to_string()),
            "ver-c" => Ok(ver_c.to_string()),
            _ => panic!("unexpected parent {id}"),
        })
        .unwrap_err();

        assert_eq!(err.stage, InstallStage::VersionJson);
        assert!(err.localized_title.contains("Циклическая"));
    }

    #[test]
    fn rejects_chains_exceeding_max_depth_of_16() {
        let ctx = RuleContext::linux_x86_64();
        let root_json = r#"{"id": "node-0", "inheritsFrom": "node-1"}"#;

        let err = resolve_version_inherits_from(root_json, &ctx, |id| {
            let idx: usize = id.strip_prefix("node-").unwrap().parse().unwrap();
            if idx <= 20 {
                Ok(format!(
                    r#"{{"id": "node-{idx}", "inheritsFrom": "node-{}"}}"#,
                    idx + 1
                ))
            } else {
                Ok(format!(r#"{{"id": "node-{idx}"}}"#))
            }
        })
        .unwrap_err();

        assert_eq!(err.stage, InstallStage::VersionJson);
        assert!(err.localized_title.contains("Превышена глубина"));
        assert!(err.localized_details.contains("16"));
    }

    #[test]
    fn parses_modern_minecraft_26_3_with_unknown_fields_and_missing_size() {
        let json_26 = r#"{
            "id": "26.3",
            "future_engine_flag": true,
            "experimental_options": {"pipeline": "v3", "threads": 16},
            "new_26_3_array": [100, 200, 300],
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {
                "id": "26.3",
                "totalSize": 45000000,
                "url": "https://resources.download.minecraft.net/26.3.json",
                "future_index_property": "ignored"
            },
            "downloads": {
                "client": {
                    "sha1": "da39a3ee5e6b4b0d3255bfef95601890afd80709",
                    "size": 0,
                    "url": "https://piston-data.mojang.com/v1/objects/26.3/client.jar"
                }
            },
            "javaVersion": {
                "component": "java-runtime-epsilon",
                "majorVersion": 25
            }
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let parsed = parse_flexible_version_json(json_26, &ctx).expect("must parse 26.3");

        assert_eq!(parsed.id, "26.3");
        assert_eq!(parsed.mc_version, "26.3");
        assert_eq!(parsed.java_version.major_version, 25);
        assert_eq!(parsed.asset_index.size, 0); // AssetIndex without explicit 'size' defaults to 0
        assert_eq!(parsed.asset_index.total_size, 45000000);
        assert_eq!(parsed.client_jar.size, 0); // Zero file size handled cleanly
    }

    #[test]
    fn handles_zero_size_files_and_missing_optional_fields_cleanly() {
        let minimal_json = r#"{
            "id": "26.3-zero",
            "assetIndex": {
                "id": "26.3-zero"
            }
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let parsed = parse_flexible_version_json(minimal_json, &ctx).expect("minimal version");

        assert_eq!(parsed.id, "26.3-zero");
        assert_eq!(parsed.asset_index.size, 0);
        assert_eq!(parsed.asset_index.total_size, 0);
        assert_eq!(parsed.client_jar.size, 0);
        assert!(parsed.libraries.is_empty());
        assert!(parsed.jvm_arguments.is_empty());
        assert!(parsed.game_arguments.is_empty());
    }

    #[test]
    fn resolves_valid_multi_level_inheritance_successfully() {
        let child = r#"{
            "id": "fabric-loader-0.16.9-1.21.1",
            "inheritsFrom": "1.21.1",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries": [
                {
                    "name": "net.fabricmc:fabric-loader:0.16.9",
                    "downloads": {
                        "artifact": {
                            "path": "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar",
                            "url": "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar",
                            "size": 500000
                        }
                    }
                }
            ]
        }"#;

        let parent = r#"{
            "id": "1.21.1",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {
                "id": "1.21.1",
                "size": 100,
                "totalSize": 200,
                "url": "https://assets.mojang.com"
            },
            "downloads": {
                "client": {
                    "path": "versions/1.21.1/1.21.1.jar",
                    "url": "https://piston-data.mojang.com/1.21.1.jar",
                    "size": 25000000
                }
            },
            "libraries": [
                {
                    "name": "org.lwjgl:lwjgl:3.3.3",
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                            "url": "https://libraries.minecraft.net/lwjgl.jar",
                            "size": 300000
                        }
                    }
                }
            ]
        }"#;

        let ctx = RuleContext::linux_x86_64();
        let resolved = resolve_version_inherits_from(child, &ctx, |parent_id| {
            if parent_id == "1.21.1" {
                Ok(parent.to_string())
            } else {
                Err(InstallError {
                    stage: InstallStage::VersionJson,
                    localized_title: "Родитель не найден".to_string(),
                    localized_details: parent_id.to_string(),
                    target_url: None,
                    file_path: None,
                    http_status: None,
                    is_retryable: false,
                    raw_cause: "not found".to_string(),
                })
            }
        })
        .expect("must resolve chain");

        assert_eq!(resolved.id, "fabric-loader-0.16.9-1.21.1");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(
            resolved.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(resolved.client_jar.size, 25000000); // Inherited from parent
        assert_eq!(resolved.libraries.len(), 2); // 1 child + 1 parent
    }
}
