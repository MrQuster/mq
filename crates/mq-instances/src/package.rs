//! Instance import/export (.zip / .mrpack), safe extraction, and screenshots management.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use mq_core::{InstanceId, ModLoaderKind, MqError, MqResult};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::manifest::{InstanceManifest, INSTANCE_MANIFEST_FILE};

/// Screenshot metadata found inside `<instance_dir>/screenshots/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_at: chrono::DateTime<Utc>,
}

/// Lists all screenshots inside `<instance_dir>/screenshots/`, sorted newest first.
pub fn list_screenshots(instance_dir: &Path) -> MqResult<Vec<ScreenshotInfo>> {
    let scr_dir = instance_dir.join("screenshots");
    if !scr_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut screenshots = Vec::new();
    for entry in fs::read_dir(&scr_dir)?.flatten() {
        let path = entry.path();
        if path.is_file() {
            let is_img = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false);

            if is_img {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let meta = entry.metadata()?;
                let size_bytes = meta.len();
                let created_at: chrono::DateTime<Utc> = meta
                    .modified()
                    .unwrap_or_else(|_| std::time::SystemTime::now())
                    .into();

                screenshots.push(ScreenshotInfo {
                    file_name,
                    path,
                    size_bytes,
                    created_at,
                });
            }
        }
    }

    screenshots.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    Ok(screenshots)
}

/// Exports an instance directory into a portable standalone `.zip` archive.
pub fn export_instance_to_zip(instance_dir: &Path, out_zip_path: &Path) -> MqResult<()> {
    if !instance_dir.is_dir() {
        return Err(MqError::Internal(format!(
            "instance directory not found: {}",
            instance_dir.display()
        )));
    }

    if let Some(p) = out_zip_path.parent() {
        fs::create_dir_all(p)?;
    }

    let file = File::create(out_zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // List of subdirectories to include in portable instance export
    let allowed_dirs = ["mods", "config", "resourcepacks", "shaderpacks", "saves"];

    for dir_name in allowed_dirs {
        let sub = instance_dir.join(dir_name);
        if sub.is_dir() {
            add_dir_to_zip(&mut zip, instance_dir, &sub, options)?;
        }
    }

    // Include manifest and options if present
    for fname in [INSTANCE_MANIFEST_FILE, "options.txt", "servers.dat"] {
        let fpath = instance_dir.join(fname);
        if fpath.is_file() {
            zip.start_file(fname, options).map_err(|e| {
                MqError::Internal(format!("failed starting file {fname} in ZIP: {e}"))
            })?;
            let mut f = File::open(&fpath)?;
            std::io::copy(&mut f, &mut zip)?;
        }
    }

    zip.finish()
        .map_err(|e| MqError::Internal(format!("failed finalizing export ZIP archive: {e}")))?;

    Ok(())
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    base_dir: &Path,
    current_dir: &Path,
    options: SimpleFileOptions,
) -> MqResult<()> {
    for entry in fs::read_dir(current_dir)?.flatten() {
        let path = entry.path();
        let relative = path
            .strip_prefix(base_dir)
            .map_err(|e| MqError::Internal(format!("strip prefix failed: {e}")))?;
        let name_str = relative.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            zip.add_directory(&name_str, options)
                .map_err(|e| MqError::Internal(format!("failed adding dir to ZIP: {e}")))?;
            add_dir_to_zip(zip, base_dir, &path, options)?;
        } else if path.is_file() {
            zip.start_file(&name_str, options)
                .map_err(|e| MqError::Internal(format!("failed starting file in ZIP: {e}")))?;
            let mut f = File::open(&path)?;
            std::io::copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Imports an instance from a `.zip` archive into `<instances_root>/<slug>`.
/// Enforces strict Zip-Slip path sanitization.
pub fn import_instance_from_zip(
    zip_path: &Path,
    instances_root: &Path,
    override_name: Option<&str>,
) -> MqResult<InstanceManifest> {
    if !zip_path.is_file() {
        return Err(MqError::Internal(format!(
            "archive file not found: {}",
            zip_path.display()
        )));
    }

    let default_name = zip_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ImportedInstance");
    let target_name = override_name.unwrap_or(default_name);
    let slug = InstanceId::sanitize_slug(target_name);

    let dest_dir = instances_root.join(slug.as_str());
    if dest_dir.exists() {
        return Err(MqError::Internal(format!(
            "instance directory already exists: {}",
            dest_dir.display()
        )));
    }
    fs::create_dir_all(&dest_dir)?;

    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| MqError::Internal(format!("failed opening import ZIP archive: {e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| MqError::Internal(format!("corrupt entry in ZIP: {e}")))?;

        let entry_name = entry.name().to_string();
        // Strict Zip-Slip pre-check
        if entry_name.contains("..") || entry_name.starts_with('/') || entry_name.starts_with('\\')
        {
            continue;
        }

        let out_path = dest_dir.join(&entry_name);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }

    // Check for instance.toml, or generate a default manifest
    let manifest_path = dest_dir.join(INSTANCE_MANIFEST_FILE);
    let manifest = if manifest_path.is_file() {
        let content = fs::read_to_string(&manifest_path)?;
        let mut m: InstanceManifest = toml::from_str(&content).map_err(|e| {
            MqError::Internal(format!("invalid instance.toml in imported archive: {e}"))
        })?;
        m.name = target_name.to_string();
        m.id = slug;
        m.save_to_dir(&dest_dir)?;
        m
    } else {
        // Fallback default manifest
        let m = InstanceManifest::new(target_name, "1.21.1", ModLoaderKind::Vanilla, None);
        m.save_to_dir(&dest_dir)?;
        m
    };

    Ok(manifest)
}
