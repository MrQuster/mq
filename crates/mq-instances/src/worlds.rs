//! Minecraft world saves scanner, 1-click ZIP backup and restore, and export.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use flate2::read::GzDecoder;
use mq_core::{MqError, MqResult};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Information about a single Minecraft world save directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldInfo {
    /// Directory name under `<instance_dir>/saves/<folder_name>`.
    pub folder_name: String,
    /// Human-readable world name from `level.dat` or folder name fallback.
    pub display_name: String,
    /// Game mode string (e.g. "Выживание", "Творчество", "Хардкор").
    pub game_mode: String,
    /// Cumulative directory size in bytes.
    pub size_bytes: u64,
    /// Last played or modified UTC timestamp.
    pub last_played: Option<DateTime<Utc>>,
}

/// Metadata describing a single world backup ZIP archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldBackupInfo {
    /// Backup file name (e.g. `MyWorld_2026-11-07_01-45-00.zip`).
    pub file_name: String,
    /// Absolute path to the ZIP backup archive.
    pub path: PathBuf,
    /// File size in bytes.
    pub size_bytes: u64,
    /// UTC timestamp of the backup.
    pub created_at: DateTime<Utc>,
}

/// Lists all world saves inside `<instance_dir>/saves/`.
pub fn list_worlds(instance_dir: &Path) -> MqResult<Vec<WorldInfo>> {
    let saves_dir = instance_dir.join("saves");
    if !saves_dir.exists() {
        return Ok(Vec::new());
    }

    let mut worlds = Vec::new();
    let entries = fs::read_dir(&saves_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder_name = entry.file_name().to_string_lossy().into_owned();
        let level_dat = path.join("level.dat");

        let mut display_name = folder_name.clone();
        let mut game_mode = "Выживание".to_string();
        let mut last_played = None;

        if level_dat.is_file() {
            if let Ok(meta) = fs::metadata(&level_dat) {
                if let Ok(mtime) = meta.modified() {
                    let dt: DateTime<Utc> = mtime.into();
                    last_played = Some(dt);
                }
            }
            if let Ok((name, mode, lp)) = parse_level_dat(&level_dat) {
                if !name.is_empty() {
                    display_name = name;
                }
                game_mode = mode;
                if let Some(ts) = lp {
                    last_played = Some(ts);
                }
            }
        } else if let Ok(meta) = fs::metadata(&path) {
            if let Ok(mtime) = meta.modified() {
                let dt: DateTime<Utc> = mtime.into();
                last_played = Some(dt);
            }
        }

        let size_bytes = compute_dir_size(&path);

        worlds.push(WorldInfo {
            folder_name,
            display_name,
            game_mode,
            size_bytes,
            last_played,
        });
    }

    worlds.sort_by_key(|a| std::cmp::Reverse(a.last_played));
    Ok(worlds)
}

/// Computes cumulative directory size in bytes.
pub fn compute_dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            } else if path.is_dir() {
                total += compute_dir_size(&path);
            }
        }
    }
    total
}

/// Parses basic metadata from `level.dat` using Gzip decompression and simple NBT scanning.
fn parse_level_dat(path: &Path) -> MqResult<(String, String, Option<DateTime<Utc>>)> {
    let file = File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;

    let mut name = String::new();
    let mut game_mode = "Выживание".to_string();
    let mut last_played = None;

    // Scan for LevelName tag: TAG_String (8) followed by name "LevelName"
    if let Some(pos) = find_subsequence(&bytes, b"LevelName") {
        let after = pos + 9;
        if after + 2 <= bytes.len() {
            let len = u16::from_be_bytes([bytes[after], bytes[after + 1]]) as usize;
            if after + 2 + len <= bytes.len() {
                if let Ok(s) = std::str::from_utf8(&bytes[after + 2..after + 2 + len]) {
                    name = s.to_string();
                }
            }
        }
    }

    // Scan for GameType tag: TAG_Int (3) or TAG_Byte (1) with name "GameType"
    if let Some(pos) = find_subsequence(&bytes, b"GameType") {
        let after = pos + 8;
        if after + 4 <= bytes.len() {
            let mode_val = i32::from_be_bytes([
                bytes[after],
                bytes[after + 1],
                bytes[after + 2],
                bytes[after + 3],
            ]);
            game_mode = match mode_val {
                0 => "Выживание".to_string(),
                1 => "Творчество".to_string(),
                2 => "Приключение".to_string(),
                3 => "Наблюдатель".to_string(),
                _ => "Выживание".to_string(),
            };
        }
    }

    // Scan for hardcore tag: TAG_Byte (1) with name "hardcore"
    if let Some(pos) = find_subsequence(&bytes, b"hardcore") {
        let after = pos + 8;
        if after < bytes.len() && bytes[after] == 1 {
            game_mode = "Хардкор".to_string();
        }
    }

    // Scan for LastPlayed: TAG_Long (4) with name "LastPlayed"
    if let Some(pos) = find_subsequence(&bytes, b"LastPlayed") {
        let after = pos + 10;
        if after + 8 <= bytes.len() {
            let millis = i64::from_be_bytes([
                bytes[after],
                bytes[after + 1],
                bytes[after + 2],
                bytes[after + 3],
                bytes[after + 4],
                bytes[after + 5],
                bytes[after + 6],
                bytes[after + 7],
            ]);
            if millis > 0 {
                last_played = Utc.timestamp_millis_opt(millis).single();
            }
        }
    }

    Ok((name, game_mode, last_played))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Creates a 1-click ZIP backup of the specified world save directory under
/// `<backups_root>/<instance_id>/<world_folder>_<timestamp>.zip`.
pub fn backup_world(
    instance_dir: &Path,
    backups_root: &Path,
    instance_id: &str,
    world_folder: &str,
) -> MqResult<PathBuf> {
    let source_dir = instance_dir.join("saves").join(world_folder);
    if !source_dir.is_dir() {
        return Err(MqError::Internal(format!(
            "world directory not found: {}",
            source_dir.display()
        )));
    }

    let target_dir = backups_root.join(instance_id);
    fs::create_dir_all(&target_dir)?;

    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let zip_name = format!("{world_folder}_{timestamp}.zip");
    let zip_path = target_dir.join(&zip_name);

    export_world_to_zip(instance_dir, world_folder, &zip_path)?;
    Ok(zip_path)
}

/// Lists all backups for a given instance.
pub fn list_backups(backups_root: &Path, instance_id: &str) -> MqResult<Vec<WorldBackupInfo>> {
    let target_dir = backups_root.join(instance_id);
    if !target_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();
    for entry in fs::read_dir(&target_dir)?.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("zip") {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata()?;
            let size_bytes = meta.len();
            let created_at: DateTime<Utc> = meta
                .modified()
                .unwrap_or_else(|_| std::time::SystemTime::now())
                .into();

            backups.push(WorldBackupInfo {
                file_name,
                path,
                size_bytes,
                created_at,
            });
        }
    }

    backups.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    Ok(backups)
}

/// Restores a world save from a backup ZIP archive into `<instance_dir>/saves/<world_folder>`.
pub fn restore_world(
    backups_root: &Path,
    instance_id: &str,
    backup_file_name: &str,
    instance_dir: &Path,
    target_folder_name: Option<&str>,
) -> MqResult<PathBuf> {
    let backup_path = backups_root.join(instance_id).join(backup_file_name);
    if !backup_path.is_file() {
        return Err(MqError::Internal(format!(
            "backup archive not found: {}",
            backup_path.display()
        )));
    }

    let resolved_folder = target_folder_name
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            backup_file_name
                .trim_end_matches(".zip")
                .split('_')
                .next()
                .unwrap_or("RestoredWorld")
                .to_string()
        });

    let saves_dir = instance_dir.join("saves");
    fs::create_dir_all(&saves_dir)?;
    let dest_dir = saves_dir.join(&resolved_folder);

    if dest_dir.exists() {
        fs::remove_dir_all(&dest_dir)?;
    }
    fs::create_dir_all(&dest_dir)?;

    let file = File::open(&backup_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| MqError::Internal(format!("failed to open backup ZIP archive: {e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| MqError::Internal(format!("corrupted entry in backup ZIP: {e}")))?;

        let entry_name = entry.name().to_string();
        // Prevent Zip Slip
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

    Ok(dest_dir)
}

/// Exports a world directory to a standalone ZIP archive.
pub fn export_world_to_zip(
    instance_dir: &Path,
    world_folder: &str,
    dest_zip_path: &Path,
) -> MqResult<()> {
    let src_dir = instance_dir.join("saves").join(world_folder);
    if !src_dir.is_dir() {
        return Err(MqError::Internal(format!(
            "source world directory not found: {}",
            src_dir.display()
        )));
    }

    if let Some(p) = dest_zip_path.parent() {
        fs::create_dir_all(p)?;
    }

    let file = File::create(dest_zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

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

    add_dir_to_zip(&mut zip, &src_dir, &src_dir, options)?;
    zip.finish()
        .map_err(|e| MqError::Internal(format!("failed finalizing ZIP archive: {e}")))?;

    Ok(())
}

/// Deletes a world save folder from `<instance_dir>/saves/<world_folder>`.
pub fn delete_world(instance_dir: &Path, world_folder: &str) -> MqResult<()> {
    let world_dir = instance_dir.join("saves").join(world_folder);
    if world_dir.exists() {
        fs::remove_dir_all(&world_dir)?;
    }
    Ok(())
}
