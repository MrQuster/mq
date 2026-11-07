//! Pre-launch verification and repair of Minecraft assets and language files.
//!
//! Verifies that `assets/indexes/<id>.json` and critical language objects (`ru_ru.json` or `ru_RU.lang`, `pack.mcmeta`)
//! are present on disk prior to spawning the JVM. If any are missing, downloads and reconstructs them immediately.

use std::fs;
use std::path::Path;

use mq_core::{MqPaths, MqResult};
use mq_meta::{fetch_asset_index_raw, parse_asset_index, ResolvedVersion};

/// Verifies and guarantees that the asset index and language files exist on disk before JVM spawn.
pub async fn ensure_prelaunch_assets(
    paths: &MqPaths,
    instance_dir: &Path,
    resolved_ver: &ResolvedVersion,
) -> MqResult<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let asset_index_id = &resolved_ver.asset_index.id;
    let asset_index_path = paths
        .assets_dir()
        .join("indexes")
        .join(format!("{asset_index_id}.json"));

    // 1. Ensure assets/indexes/<id>.json exists
    let (asset_index, raw_json) = if asset_index_path.is_file() {
        if let Ok(content) = fs::read_to_string(&asset_index_path) {
            if let Ok(idx) = parse_asset_index(asset_index_id, &content) {
                (idx, content)
            } else {
                let fetched = fetch_asset_index_raw(&client, &resolved_ver.asset_index).await?;
                if let Some(p) = asset_index_path.parent() {
                    let _ = fs::create_dir_all(p);
                }
                let _ = fs::write(&asset_index_path, fetched.1.as_bytes());
                fetched
            }
        } else {
            let fetched = fetch_asset_index_raw(&client, &resolved_ver.asset_index).await?;
            if let Some(p) = asset_index_path.parent() {
                let _ = fs::create_dir_all(p);
            }
            let _ = fs::write(&asset_index_path, fetched.1.as_bytes());
            fetched
        }
    } else {
        let fetched = fetch_asset_index_raw(&client, &resolved_ver.asset_index).await?;
        if let Some(p) = asset_index_path.parent() {
            let _ = fs::create_dir_all(p);
        }
        let _ = fs::write(&asset_index_path, fetched.1.as_bytes());
        fetched
    };

    let _ = raw_json;

    // 2. Ensure critical language files & pack.mcmeta are downloaded
    let objects_dir = paths.assets_dir().join("objects");
    let mut missing_objects = Vec::new();

    for obj in &asset_index.objects {
        let is_lang_or_meta = obj.name.starts_with("minecraft/lang/")
            || obj.name.ends_with(".lang")
            || obj.name == "pack.mcmeta";

        if is_lang_or_meta {
            let hex = &obj.hash.to_hex();
            if hex.len() >= 2 {
                let prefix = &hex[..2];
                let obj_path = objects_dir.join(prefix).join(hex);
                if !obj_path.exists() {
                    missing_objects.push((obj.name.clone(), hex.clone()));
                }
            }
        }
    }

    // Download any missing language objects directly
    for (_name, hex) in missing_objects {
        let prefix = &hex[..2];
        let target_dir = objects_dir.join(prefix);
        let target_file = target_dir.join(&hex);
        let _ = fs::create_dir_all(&target_dir);

        let url = format!("https://resources.download.minecraft.net/{prefix}/{hex}");
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = fs::write(&target_file, &bytes);
                }
            }
        }
    }

    // 3. Virtual and map_to_resources reconstruction for legacy versions (1.7.10 - 1.12.2)
    if asset_index.is_virtual {
        let virtual_base = paths.assets_dir().join("virtual").join(&asset_index.id);
        for obj in &asset_index.objects {
            let hex = obj.hash.to_hex();
            let src = objects_dir.join(&hex[..2]).join(&hex);
            let dst = virtual_base.join(&obj.name);
            if src.exists() && !dst.exists() {
                if let Some(p) = dst.parent() {
                    let _ = fs::create_dir_all(p);
                }
                if fs::hard_link(&src, &dst).is_err() {
                    let _ = fs::copy(&src, &dst);
                }
            }
        }
    }

    if asset_index.map_to_resources {
        let resources_base = instance_dir.join("resources");
        for obj in &asset_index.objects {
            let hex = obj.hash.to_hex();
            let src = objects_dir.join(&hex[..2]).join(&hex);
            let dst = resources_base.join(&obj.name);
            if src.exists() && !dst.exists() {
                if let Some(p) = dst.parent() {
                    let _ = fs::create_dir_all(p);
                }
                if fs::hard_link(&src, &dst).is_err() {
                    let _ = fs::copy(&src, &dst);
                }
            }
        }
    }

    Ok(())
}
