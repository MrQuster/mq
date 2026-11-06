//! Mojang `assets/indexes/<id>.json` parser and deterministic SHA-1 deduplication.

use std::collections::BTreeMap;

use mq_core::{MqError, MqResult, Sha1Digest};
use serde::Deserialize;

use crate::models::{AssetIndex, AssetIndexPointer, AssetObject};

#[derive(Debug, Deserialize)]
struct RawAssetIndex {
    #[serde(default)]
    objects: BTreeMap<String, RawAssetObject>,
}

#[derive(Debug, Deserialize)]
struct RawAssetObject {
    hash: Sha1Digest,
    size: u64,
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

    Ok(AssetIndex {
        id: index_id.to_string(),
        objects,
    })
}

/// Fetches and parses a Mojang asset index JSON document over HTTP.
pub async fn fetch_asset_index(
    client: &reqwest::Client,
    pointer: &AssetIndexPointer,
) -> MqResult<AssetIndex> {
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

    parse_asset_index(&pointer.id, &body)
}

impl AssetIndex {
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
}
