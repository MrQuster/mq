//! Minecraft client `options.txt` synchronization module.
//!
//! Preserves all user-configured game settings (keybindings, FOV, render distance,
//! sound levels) while updating or injecting the active Minecraft locale (`lang:<code_lang>`).
//! Supports legacy capitalization rules (`ru_RU` for MC <= 1.10 vs `ru_ru` for modern versions)
//! and respects user in-game language changes via `lang_initialized`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use mq_core::{MqError, MqResult};

/// Canonical filename for Minecraft's settings file (`options.txt`).
pub const MINECRAFT_OPTIONS_FILE: &str = "options.txt";

/// Determines whether a Minecraft version is legacy (1.10 or older) requiring uppercase locale (`ru_RU`).
pub fn is_legacy_mc_version(mc_version: &str) -> bool {
    let trimmed = mc_version.trim().trim_start_matches('v');
    let mut parts = trimmed.split('.');
    let major = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let minor = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20);

    major < 1 || (major == 1 && minor <= 10)
}

/// Returns the recommended default Russian locale code for the specified Minecraft version.
pub fn default_mc_locale_for_version(mc_version: &str) -> &'static str {
    if is_legacy_mc_version(mc_version) {
        "ru_RU"
    } else {
        "ru_ru"
    }
}

/// Synchronizes the Minecraft game language in `<instance_dir>/options.txt`.
///
/// Rules:
/// 1. If `options.txt` does not exist:
///    Creates a new `options.txt` with `lang:<target_locale>\n` and returns `Ok(true)` (initialized).
/// 2. If `options.txt` exists and `lang_initialized` is false:
///    If `lang:en_us` or missing `lang:`: replaces or appends `lang:<target_locale>` while keeping
///    all other settings intact. Returns `Ok(true)`.
///    If player has already set another language, preserves it and returns `Ok(true)`.
/// 3. If `lang_initialized` is true:
///    Launcher NEVER overwrites user's in-game choice. Returns `Ok(true)`.
pub fn patch_minecraft_options_lang(
    instance_dir: &Path,
    mc_version: &str,
    lang_initialized: bool,
) -> MqResult<bool> {
    if !instance_dir.exists() {
        fs::create_dir_all(instance_dir)?;
    }

    let target_locale = default_mc_locale_for_version(mc_version);
    let options_path = instance_dir.join(MINECRAFT_OPTIONS_FILE);

    if !options_path.is_file() {
        // 1. Missing options.txt -> create with target locale
        let output = format!("lang:{target_locale}\n");
        atomic_write_file(&options_path, output.as_bytes())?;
        return Ok(true);
    }

    if lang_initialized {
        // User has already launched and configured their preferences; do not overwrite!
        return Ok(true);
    }

    // 2. Existing options.txt with uninitialized state
    let content = fs::read_to_string(&options_path)?;
    let mut updated_lines = Vec::new();
    let mut found_lang = false;
    let mut needs_write = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("lang:") {
            found_lang = true;
            let val = trimmed
                .strip_prefix("lang:")
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if val == "en_us" || val.is_empty() {
                updated_lines.push(format!("lang:{target_locale}"));
                needs_write = true;
            } else {
                updated_lines.push(line.to_string());
            }
        } else {
            updated_lines.push(line.to_string());
        }
    }

    if !found_lang {
        updated_lines.push(format!("lang:{target_locale}"));
        needs_write = true;
    }

    if needs_write {
        let mut output = updated_lines.join("\n");
        output.push('\n');
        atomic_write_file(&options_path, output.as_bytes())?;
    }

    Ok(true)
}

/// Explicitly synchronizes the Minecraft game language in `<instance_dir>/options.txt`.
/// Replaces any existing `lang:<value>` line with `lang:<clean_locale>`, or appends it.
pub fn sync_minecraft_options_lang(instance_dir: &Path, locale: &str) -> MqResult<()> {
    let clean_locale = locale.trim();
    if clean_locale.is_empty() {
        return Ok(());
    }

    if !instance_dir.exists() {
        fs::create_dir_all(instance_dir)?;
    }

    let options_path = instance_dir.join(MINECRAFT_OPTIONS_FILE);
    let mut updated_lines = Vec::new();
    let mut found_lang = false;

    if options_path.is_file() {
        let content = fs::read_to_string(&options_path)?;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("lang:") {
                updated_lines.push(format!("lang:{clean_locale}"));
                found_lang = true;
            } else {
                updated_lines.push(line.to_string());
            }
        }
    }

    if !found_lang {
        updated_lines.push(format!("lang:{clean_locale}"));
    }

    let mut output = updated_lines.join("\n");
    output.push('\n');

    atomic_write_file(&options_path, output.as_bytes())
}

/// Reads the currently configured `lang:<value>` from `<instance_dir>/options.txt`,
/// or returns `None` if the file or key does not exist.
pub fn read_minecraft_options_lang(instance_dir: &Path) -> Option<String> {
    let options_path = instance_dir.join(MINECRAFT_OPTIONS_FILE);
    if !options_path.is_file() {
        return None;
    }
    let content = fs::read_to_string(&options_path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("lang:") {
            let clean = val.trim();
            if !clean.is_empty() {
                return Some(clean.to_owned());
            }
        }
    }
    None
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> MqResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|f| f.to_str()).unwrap_or("file"),
        std::process::id()
    ));

    let res = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)?;
        Ok(())
    })();

    if let Err(err) = res {
        let _ = fs::remove_file(&tmp_path);
        return Err(MqError::Io(err));
    }

    Ok(())
}

/// Unified options manager conforming to Section 4.5 and CONTRACTS.md.
pub struct MinecraftOptionsManager;

impl MinecraftOptionsManager {
    /// Synchronizes language setting into `options.txt`.
    pub fn sync_lang(options_path: &Path, lang: &str, force: bool) -> MqResult<()> {
        let parent = options_path.parent().unwrap_or(options_path);
        patch_minecraft_options_lang(parent, lang, !force).map(|_| ())
    }

    /// Reads resourcepacks array from `options.txt` (`resourcePacks:[...]`).
    pub fn read_resource_packs(options_path: &Path) -> MqResult<Vec<String>> {
        if !options_path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(options_path)?;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("resourcePacks:") {
                let json_slice = trimmed.trim_start_matches("resourcePacks:");
                if let Ok(packs) = serde_json::from_str::<Vec<String>>(json_slice) {
                    return Ok(packs);
                }
            }
        }
        Ok(Vec::new())
    }

    /// Updates resourcepacks in `options.txt` preserving other lines atomically.
    pub fn update_resource_packs(options_path: &Path, packs: &[String]) -> MqResult<()> {
        let json_packs = serde_json::to_string(packs)?;
        let target_line = format!("resourcePacks:{json_packs}");

        let mut lines = Vec::new();
        let mut found = false;

        if options_path.exists() {
            let content = fs::read_to_string(options_path)?;
            for line in content.lines() {
                if line.trim().starts_with("resourcePacks:") {
                    lines.push(target_line.clone());
                    found = true;
                } else {
                    lines.push(line.to_string());
                }
            }
        }

        if !found {
            lines.push(target_line);
        }

        let mut out = lines.join("\n");
        out.push('\n');

        let tmp_path = options_path.with_extension("tmp");
        fs::write(&tmp_path, out)?;
        fs::rename(tmp_path, options_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_legacy_versions_accurately() {
        assert!(is_legacy_mc_version("1.7.10"));
        assert!(is_legacy_mc_version("1.8.9"));
        assert!(is_legacy_mc_version("1.9.4"));
        assert!(is_legacy_mc_version("1.10.2"));
        assert!(!is_legacy_mc_version("1.12.2"));
        assert!(!is_legacy_mc_version("1.16.5"));
        assert!(!is_legacy_mc_version("1.20.1"));
        assert!(!is_legacy_mc_version("1.21.1"));
    }

    #[test]
    fn patch_creates_ru_ru_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let initialized = patch_minecraft_options_lang(tmp.path(), "1.21.1", false).expect("patch");
        assert!(initialized);

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read");
        assert_eq!(content.trim(), "lang:ru_ru");
    }

    #[test]
    fn patch_creates_ru_ru_uppercase_for_legacy() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let initialized = patch_minecraft_options_lang(tmp.path(), "1.7.10", false).expect("patch");
        assert!(initialized);

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read");
        assert_eq!(content.trim(), "lang:ru_RU");
    }

    #[test]
    fn patch_replaces_en_us_when_uninitialized() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let initial = "fov:70.0\nlang:en_us\nrenderDistance:12\n";
        fs::write(tmp.path().join("options.txt"), initial).expect("write");

        let initialized = patch_minecraft_options_lang(tmp.path(), "1.20.1", false).expect("patch");
        assert!(initialized);

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read");
        assert!(content.contains("fov:70.0"));
        assert!(content.contains("renderDistance:12"));
        assert!(content.contains("lang:ru_ru"));
        assert!(!content.contains("lang:en_us"));
    }

    #[test]
    fn patch_preserves_user_change_when_initialized() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let initial = "fov:80.0\nlang:en_us\nrenderDistance:14\n";
        fs::write(tmp.path().join("options.txt"), initial).expect("write");

        // Player explicitly selected en_us in game settings after initialization
        let initialized = patch_minecraft_options_lang(tmp.path(), "1.20.1", true).expect("patch");
        assert!(initialized);

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read");
        assert!(content.contains("lang:en_us"));
        assert!(!content.contains("lang:ru_ru"));
    }
}
