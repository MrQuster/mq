//! Mojang `assets/indexes/<id>.json` parser, client logging configurations,
//! legacy virtual/resources reconstruction, and deterministic SHA-1 deduplication.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use mq_core::{MqError, MqResult, Sha1Digest};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha1::{Digest as _, Sha1};

use crate::models::{AssetIndex, AssetIndexPointer, AssetObject};

#[derive(Debug, Deserialize)]
struct RawAssetIndex {
    #[serde(default)]
    objects: BTreeMap<String, RawAssetObject>,
    #[serde(default, rename = "virtual")]
    is_virtual: Option<bool>,
    #[serde(default)]
    map_to_resources: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawAssetObject {
    hash: Sha1Digest,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct StructAssetIndex {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    is_virtual: Option<bool>,
    #[serde(default)]
    map_to_resources: Option<bool>,
    #[serde(default)]
    objects: Vec<AssetObject>,
}

/// Client logging configuration file descriptor inside Mojang `version.json` (`logging.client.file`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientLoggingConfigFile {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[serde(default)]
    pub size: u64,
    pub url: String,
}

/// Client logging configuration descriptor inside Mojang `version.json` (`logging.client`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientLoggingConfig {
    #[serde(default)]
    pub argument: String,
    pub file: ClientLoggingConfigFile,
    #[serde(default, rename = "type")]
    pub logging_type: String,
}

#[derive(Debug, Deserialize)]
struct RawLoggingRoot {
    #[serde(default)]
    logging: Option<RawLoggingSection>,
}

#[derive(Debug, Deserialize)]
struct RawLoggingSection {
    #[serde(default)]
    client: Option<ClientLoggingConfig>,
}

/// Verification report detailing results of asset objects, virtual, or legacy resources checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetIntegrityReport {
    pub total_objects: usize,
    pub valid_objects: usize,
    pub corrupted_objects: Vec<PathBuf>,
    pub missing_objects: Vec<PathBuf>,
}

impl AssetIntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.corrupted_objects.is_empty() && self.missing_objects.is_empty()
    }
}

impl Serialize for AssetIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;

        let mut objects_map = BTreeMap::new();
        for obj in &self.objects {
            objects_map.insert(
                &obj.name,
                serde_json::json!({
                    "hash": obj.hash.to_hex(),
                    "size": obj.size,
                }),
            );
        }
        map.serialize_entry("objects", &objects_map)?;
        if self.is_virtual {
            map.serialize_entry("virtual", &true)?;
        }
        if self.map_to_resources {
            map.serialize_entry("map_to_resources", &true)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for AssetIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(obj_val) = value.get("objects") {
            if obj_val.is_array() {
                let parsed: StructAssetIndex =
                    serde_json::from_value(value).map_err(de::Error::custom)?;
                return Ok(AssetIndex {
                    id: parsed.id.unwrap_or_default(),
                    is_virtual: parsed.is_virtual.unwrap_or(false),
                    map_to_resources: parsed.map_to_resources.unwrap_or(false),
                    objects: parsed.objects,
                });
            }
        }

        let raw: RawAssetIndex = serde_json::from_value(value).map_err(de::Error::custom)?;
        let mut objects = Vec::with_capacity(raw.objects.len());
        for (name, entry) in raw.objects {
            objects.push(AssetObject {
                name,
                hash: entry.hash,
                size: entry.size,
            });
        }
        objects.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(AssetIndex {
            id: String::new(),
            is_virtual: raw.is_virtual.unwrap_or(false),
            map_to_resources: raw.map_to_resources.unwrap_or(false),
            objects,
        })
    }
}

/// Parses a Mojang `assets/indexes/<id>.json` document into a deterministic [`AssetIndex`].
///
/// Entries in [`AssetIndex::objects`] are sorted lexicographically by logical asset name,
/// while [`AssetIndex::unique_objects_by_hash`] provides deterministic deduplicated objects
/// so identical blobs shared across multiple logical paths are downloaded only once.
pub fn parse_asset_index(index_id: &str, json: &str) -> MqResult<AssetIndex> {
    if index_id.trim().is_empty() {
        return Err(MqError::Metadata(
            "asset index id must not be empty".to_string(),
        ));
    }

    let raw: RawAssetIndex = serde_json::from_str(json)?;
    let mut objects = Vec::with_capacity(raw.objects.len());
    for (name, entry) in raw.objects {
        objects.push(AssetObject {
            name,
            hash: entry.hash,
            size: entry.size,
        });
    }
    objects.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(AssetIndex {
        id: index_id.to_string(),
        is_virtual: raw.is_virtual.unwrap_or(false),
        map_to_resources: raw.map_to_resources.unwrap_or(false),
        objects,
    })
}

/// Resolves the absolute path for an asset index JSON file: `<assets_dir>/indexes/<index_id>.json`.
pub fn asset_index_file_path(assets_dir: &Path, index_id: &str) -> PathBuf {
    assets_dir
        .join("indexes")
        .join(format!("{}.json", index_id.trim()))
}

/// Saves raw asset index JSON bytes byte-for-byte to `<indexes_dir>/<id>.json` without any mutation.
pub fn save_asset_index_raw(indexes_dir: &Path, id: &str, raw_json: &str) -> MqResult<PathBuf> {
    fs::create_dir_all(indexes_dir)?;
    let path = indexes_dir.join(format!("{}.json", id.trim()));
    fs::write(&path, raw_json.as_bytes())?;
    Ok(path)
}

/// Fetches and parses a Mojang asset index JSON document over HTTP, returning both parsed index
/// and exact raw JSON string bytes for byte-for-byte exact disk storage.
pub async fn fetch_asset_index_raw(
    client: &reqwest::Client,
    pointer: &AssetIndexPointer,
) -> MqResult<(AssetIndex, String)> {
    let response = client.get(&pointer.url).send().await.map_err(|err| {
        MqError::Network(format!(
            "failed to fetch asset index '{}': {err}",
            pointer.url
        ))
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        MqError::Network(format!(
            "failed to read asset index '{}': {err}",
            pointer.url
        ))
    })?;

    if !status.is_success() {
        return Err(MqError::HttpStatus {
            status: status.as_u16(),
            url: pointer.url.clone(),
            body,
        });
    }

    let parsed = parse_asset_index(&pointer.id, &body)?;
    Ok((parsed, body))
}

/// Fetches and parses a Mojang asset index JSON document over HTTP.
pub async fn fetch_asset_index(
    client: &reqwest::Client,
    pointer: &AssetIndexPointer,
) -> MqResult<AssetIndex> {
    fetch_asset_index_raw(client, pointer)
        .await
        .map(|(idx, _)| idx)
}

/// Parses the optional `logging.client` configuration from a Mojang `version.json` document.
pub fn parse_client_logging_config(version_json: &str) -> MqResult<Option<ClientLoggingConfig>> {
    let root: RawLoggingRoot = serde_json::from_str(version_json)?;
    Ok(root.logging.and_then(|l| l.client))
}

/// Returns the path to the logging configuration file inside `<assets_dir>/log_configs/<file_id>`.
pub fn client_logging_file_path(assets_dir: &Path, file_id: &str) -> PathBuf {
    assets_dir.join("log_configs").join(file_id.trim())
}

/// Formats the JVM logging configuration argument (`-Dlog4j.configurationFile=...`).
///
/// Replaces `${path}` in `argument_template` with the absolute path of `config_file_path`.
/// If `argument_template` is empty or missing, defaults to `-Dlog4j.configurationFile=<path>`.
pub fn format_logging_jvm_argument(argument_template: &str, config_file_path: &Path) -> String {
    let path_str = config_file_path.to_string_lossy();
    if argument_template.trim().is_empty() {
        format!("-Dlog4j.configurationFile={path_str}")
    } else if argument_template.contains("${path}") {
        argument_template.replace("${path}", &path_str)
    } else {
        argument_template.to_string()
    }
}

/// Verifies whether the logging config file exists in `<assets_dir>/log_configs/` and matches SHA-1 (if provided).
pub fn verify_client_logging_file(assets_dir: &Path, config: &ClientLoggingConfig) -> bool {
    let path = client_logging_file_path(assets_dir, &config.file.id);
    if !path.is_file() {
        return false;
    }

    if let Ok(metadata) = path.metadata() {
        if config.file.size > 0 && metadata.len() != config.file.size {
            return false;
        }
    } else {
        return false;
    }

    if let Some(expected_sha1) = config.file.sha1 {
        let mut file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut hasher = Sha1::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => hasher.update(&buf[..n]),
                Err(_) => return false,
            }
        }
        let actual: [u8; 20] = hasher.finalize().into();
        if actual != *expected_sha1.as_bytes() {
            return false;
        }
    }

    true
}

/// Verifies the integrity of all asset objects in `<assets_dir>/objects/<xx>/<hash>`.
pub fn verify_asset_objects_integrity(
    assets_dir: &Path,
    index: &AssetIndex,
) -> AssetIntegrityReport {
    let objects_base = assets_dir.join("objects");
    let mut valid = 0usize;
    let mut missing = Vec::new();
    let mut corrupted = Vec::new();

    for obj in &index.objects {
        let p = objects_base.join(obj.relative_object_path());
        if !p.is_file() {
            missing.push(p);
            continue;
        }

        let file = match File::open(&p) {
            Ok(f) => f,
            Err(_) => {
                missing.push(p);
                continue;
            }
        };

        let mut reader = std::io::BufReader::with_capacity(32 * 1024, file);
        let mut hasher = Sha1::new();
        let mut buf = [0u8; 32 * 1024];
        let mut bytes_read = 0u64;
        let mut read_err = false;

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    bytes_read += n as u64;
                    hasher.update(&buf[..n]);
                }
                Err(_) => {
                    read_err = true;
                    break;
                }
            }
        }

        if read_err || bytes_read != obj.size {
            corrupted.push(p);
            continue;
        }

        let actual: [u8; 20] = hasher.finalize().into();
        if actual != *obj.hash.as_bytes() {
            corrupted.push(p);
        } else {
            valid += 1;
        }
    }

    AssetIntegrityReport {
        total_objects: index.objects.len(),
        valid_objects: valid,
        corrupted_objects: corrupted,
        missing_objects: missing,
    }
}

/// Reconstructs legacy virtual assets (`virtual: true`) into `<assets_dir>/virtual/<id>/<name>`.
///
/// Used for Minecraft 1.7.10 - 1.12.2 legacy sound and resource lookups.
pub fn reconstruct_virtual_assets(asset_index: &AssetIndex, assets_dir: &Path) -> MqResult<usize> {
    if !asset_index.is_virtual {
        return Ok(0);
    }

    let objects_base = assets_dir.join("objects");
    let virtual_base = assets_dir.join("virtual").join(&asset_index.id);
    let mut count = 0usize;

    for obj in &asset_index.objects {
        let src = objects_base.join(obj.relative_object_path());
        if !src.exists() {
            continue;
        }

        let dst = virtual_base.join(&obj.name);
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p)?;
        }

        if !dst.exists() {
            if fs::hard_link(&src, &dst).is_err() {
                fs::copy(&src, &dst)?;
            }
            count += 1;
        }
    }

    Ok(count)
}

/// Reconstructs legacy instance resources (`map_to_resources: true`) into `<instance_dir>/resources/<name>`.
///
/// Used for early Minecraft versions that look for sounds/resources directly in the instance directory.
pub fn reconstruct_resource_assets(
    asset_index: &AssetIndex,
    instance_dir: &Path,
    assets_dir: &Path,
) -> MqResult<usize> {
    if !asset_index.map_to_resources {
        return Ok(0);
    }

    let objects_base = assets_dir.join("objects");
    let resources_base = instance_dir.join("resources");
    let mut count = 0usize;

    for obj in &asset_index.objects {
        let src = objects_base.join(obj.relative_object_path());
        if !src.exists() {
            continue;
        }

        let dst = resources_base.join(&obj.name);
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p)?;
        }

        if !dst.exists() {
            if fs::hard_link(&src, &dst).is_err() {
                fs::copy(&src, &dst)?;
            }
            count += 1;
        }
    }

    Ok(count)
}

/// Verifies whether all objects in a virtual asset index are materialized in `<assets_dir>/virtual/<id>`.
pub fn verify_virtual_assets(asset_index: &AssetIndex, assets_dir: &Path) -> AssetIntegrityReport {
    let virtual_base = assets_dir.join("virtual").join(&asset_index.id);
    let mut valid = 0usize;
    let mut missing = Vec::new();
    let mut corrupted = Vec::new();

    for obj in &asset_index.objects {
        let p = virtual_base.join(&obj.name);
        if !p.is_file() {
            missing.push(p);
            continue;
        }
        if let Ok(meta) = p.metadata() {
            if meta.len() != obj.size {
                corrupted.push(p);
                continue;
            }
        }
        valid += 1;
    }

    AssetIntegrityReport {
        total_objects: asset_index.objects.len(),
        valid_objects: valid,
        corrupted_objects: corrupted,
        missing_objects: missing,
    }
}

/// Checks if an asset name is a localization file (`minecraft/lang/*.json` or legacy `*.lang` or `pack.mcmeta`).
pub fn is_language_asset(name: &str) -> bool {
    name.starts_with("minecraft/lang/") || name.ends_with(".lang") || name == "pack.mcmeta"
}

/// Checks if an asset name is specifically the Russian localization file (`ru_ru.json` or `ru_RU.lang`).
pub fn is_russian_language_asset(name: &str) -> bool {
    name == "minecraft/lang/ru_ru.json"
        || name.ends_with("/ru_ru.json")
        || name.ends_with("/ru_RU.lang")
}

impl AssetIndex {
    /// Re-serializes the asset index into official Mojang JSON format:
    /// `{"objects": {"<path>": {"hash": "<sha1>", "size": <bytes>}}}`
    pub fn to_mojang_json(&self) -> Result<String, serde_json::Error> {
        let mut map = BTreeMap::new();
        for obj in &self.objects {
            map.insert(
                &obj.name,
                serde_json::json!({
                    "hash": obj.hash.to_hex(),
                    "size": obj.size,
                }),
            );
        }
        let mut root = serde_json::json!({
            "objects": map,
        });
        if self.is_virtual {
            root["virtual"] = serde_json::json!(true);
        }
        if self.map_to_resources {
            root["map_to_resources"] = serde_json::json!(true);
        }
        serde_json::to_string_pretty(&root)
    }

    /// Returns a deterministically ordered list of asset objects deduplicated by SHA-1 digest.
    ///
    /// In modern Minecraft asset indexes, thousands of logical asset entries share identical
    /// content hashes (e.g. empty sound/model placeholders or identical localization files).
    /// Returning one representative [`AssetObject`] per unique [`Sha1Digest`] (ordered by
    /// `(hash, name)`) ensures each blob is scheduled and downloaded at most once.
    pub fn unique_objects_by_hash(&self) -> Vec<&AssetObject> {
        let mut by_hash: BTreeMap<Sha1Digest, &AssetObject> = BTreeMap::new();
        for obj in &self.objects {
            by_hash
                .entry(obj.hash)
                .and_modify(|existing| {
                    if obj.name < existing.name {
                        *existing = obj;
                    }
                })
                .or_insert(obj);
        }
        by_hash.into_values().collect()
    }

    /// Computes the total byte size of unique SHA-1 blobs in this asset index.
    pub fn unique_total_bytes(&self) -> u64 {
        self.unique_objects_by_hash()
            .into_iter()
            .map(|obj| obj.size)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanilla::MOJANG_RESOURCES_BASE_URL;
    use tempfile::tempdir;

    #[test]
    fn parses_and_deduplicates_asset_index_deterministically() {
        let json = r#"{
            "objects": {
                "minecraft/sounds/mob/cat/meow2.ogg": {
                    "hash": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                    "size": 12048
                },
                "minecraft/sounds/mob/cat/meow1.ogg": {
                    "hash": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                    "size": 12048
                },
                "icons/icon_16x16.png": {
                    "hash": "0123456789abcdef0123456789abcdef01234567",
                    "size": 736
                },
                "minecraft/lang/en_gb.json": {
                    "hash": "fedcba9876543210fedcba9876543210fedcba98",
                    "size": 4096
                }
            }
        }"#;

        let index = parse_asset_index("17", json).expect("parse asset index");
        assert_eq!(index.id, "17");
        assert_eq!(index.objects.len(), 4);

        // Objects are deterministically ordered by logical name.
        let names: Vec<&str> = index.objects.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "icons/icon_16x16.png",
                "minecraft/lang/en_gb.json",
                "minecraft/sounds/mob/cat/meow1.ogg",
                "minecraft/sounds/mob/cat/meow2.ogg",
            ]
        );

        // Duplicate hashes are collapsed into 3 unique blobs in deterministic hash order.
        let unique = index.unique_objects_by_hash();
        assert_eq!(unique.len(), 3);
        assert_eq!(
            unique[0].hash.to_hex(),
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(
            unique[1].hash.to_hex(),
            "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3"
        );
        assert_eq!(unique[1].name, "minecraft/sounds/mob/cat/meow1.ogg");
        assert_eq!(
            unique[2].hash.to_hex(),
            "fedcba9876543210fedcba9876543210fedcba98"
        );
        assert_eq!(index.unique_total_bytes(), 736 + 12048 + 4096);

        assert_eq!(
            unique[0].relative_object_path(),
            "01/0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(
            unique[0].download_url(MOJANG_RESOURCES_BASE_URL),
            "https://resources.download.minecraft.net/01/0123456789abcdef0123456789abcdef01234567"
        );
    }

    #[test]
    fn parses_and_formats_client_logging_config() {
        let version_json = r#"{
            "id": "1.12.2",
            "logging": {
                "client": {
                    "argument": "-Dlog4j.configurationFile=${path}",
                    "file": {
                        "id": "client-1.12.xml",
                        "sha1": "ef4f57b9222277c603b5443217b1fb797c293eb7",
                        "size": 877,
                        "url": "https://piston-data.mojang.com/v1/objects/ef4f57b9222277c603b5443217b1fb797c293eb7/client-1.12.xml"
                    },
                    "type": "log4j2-xml"
                }
            }
        }"#;

        let config = parse_client_logging_config(version_json)
            .expect("parse logging")
            .expect("logging must be present");

        assert_eq!(config.file.id, "client-1.12.xml");
        assert_eq!(config.file.size, 877);
        assert_eq!(
            config.file.sha1.unwrap().to_hex(),
            "ef4f57b9222277c603b5443217b1fb797c293eb7"
        );

        let assets_dir = Path::new("/var/mc/assets");
        let log_file = client_logging_file_path(assets_dir, &config.file.id);
        assert_eq!(
            log_file,
            PathBuf::from("/var/mc/assets/log_configs/client-1.12.xml")
        );

        let jvm_arg = format_logging_jvm_argument(&config.argument, &log_file);
        assert_eq!(
            jvm_arg.replace('\\', "/"),
            "-Dlog4j.configurationFile=/var/mc/assets/log_configs/client-1.12.xml"
        );
    }

    #[test]
    fn reconstructs_and_verifies_legacy_virtual_and_resources_assets() {
        let dir = tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        let instance_dir = dir.path().join("instance");
        let objects_dir = assets_dir.join("objects");

        let hash_str = "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3";
        let blob_dir = objects_dir.join(&hash_str[..2]);
        fs::create_dir_all(&blob_dir).unwrap();
        let blob_path = blob_dir.join(hash_str);
        fs::write(&blob_path, b"test asset content").unwrap();

        let json = format!(
            r#"{{
                "objects": {{
                    "minecraft/lang/ru_ru.json": {{
                        "hash": "{hash_str}",
                        "size": 18
                    }}
                }},
                "virtual": true,
                "map_to_resources": true
            }}"#
        );

        let index = parse_asset_index("legacy", &json).unwrap();
        assert!(index.is_virtual);
        assert!(index.map_to_resources);
        assert!(is_language_asset("minecraft/lang/ru_ru.json"));
        assert!(is_russian_language_asset("minecraft/lang/ru_ru.json"));

        let count_virtual = reconstruct_virtual_assets(&index, &assets_dir).unwrap();
        assert_eq!(count_virtual, 1);
        let virt_path = assets_dir
            .join("virtual")
            .join("legacy")
            .join("minecraft/lang/ru_ru.json");
        assert!(virt_path.exists());

        let count_res = reconstruct_resource_assets(&index, &instance_dir, &assets_dir).unwrap();
        assert_eq!(count_res, 1);
        let res_path = instance_dir.join("resources/minecraft/lang/ru_ru.json");
        assert!(res_path.exists());

        let report = verify_virtual_assets(&index, &assets_dir);
        assert!(report.is_clean());
        assert_eq!(report.valid_objects, 1);
    }
}
