//! Mojang `assets/indexes/<id>.json` parser and deterministic SHA-1 deduplication.

use std::collections::BTreeMap;

use mq_core::{MqError, MqResult, Sha1Digest};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

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
        // Lexicographically first logical name is chosen for shared hash.
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
    fn rejects_invalid_hex_hash_or_empty_id() {
        let valid_json = r#"{"objects": {}}"#;
        assert!(parse_asset_index("", valid_json).is_err());

        let bad_hash_json = r#"{
            "objects": {
                "pack.mcmeta": { "hash": "not-a-sha1", "size": 10 }
            }
        }"#;
        assert!(parse_asset_index("17", bad_hash_json).is_err());
    }

    #[test]
    fn serializes_to_mojang_format_and_deserializes_back() {
        let json = r#"{
            "objects": {
                "minecraft/lang/ru_ru.json": {
                    "hash": "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
                    "size": 12048
                }
            },
            "virtual": true,
            "map_to_resources": true
        }"#;

        let index = parse_asset_index("1.12", json).expect("parse asset index");
        assert!(index.is_virtual);
        assert!(index.map_to_resources);
        let mojang_json = index.to_mojang_json().expect("to mojang json");
        assert!(mojang_json.contains("\"minecraft/lang/ru_ru.json\""));
        assert!(mojang_json.contains("\"virtual\": true"));

        // Roundtrip via serde_json::from_str
        let deserialized: AssetIndex = serde_json::from_str(&mojang_json).expect("deserialize");
        assert_eq!(deserialized.objects.len(), 1);
        assert_eq!(deserialized.objects[0].name, "minecraft/lang/ru_ru.json");
        assert!(deserialized.is_virtual);
        assert!(deserialized.map_to_resources);
    }
}
