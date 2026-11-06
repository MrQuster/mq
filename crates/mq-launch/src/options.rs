//! Minecraft client `options.txt` synchronization module.
//!
//! Preserves all user-configured game settings (keybindings, FOV, render distance,
//! sound levels) while updating or injecting the active Minecraft locale (`lang:<code_lang>`).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use mq_core::{MqError, MqResult};

/// Canonical filename for Minecraft's settings file (`options.txt`).
pub const MINECRAFT_OPTIONS_FILE: &str = "options.txt";

/// Synchronizes the Minecraft game language in `<instance_dir>/options.txt`.
///
/// If `options.txt` exists:
/// - Replaces any existing `lang:<value>` line with `lang:<clean_locale>`.
/// - If no `lang:` key is found, appends `lang:<clean_locale>`.
/// - Preserves all other game options and comments.
///
/// If `options.txt` does not exist:
/// - Creates a new `options.txt` containing `lang:<clean_locale>\n`.
///
/// Uses an atomic tempfile-to-target rename to guarantee crash-safety.
pub fn sync_minecraft_options_lang(instance_dir: &Path, locale: &str) -> MqResult<()> {
    let clean_locale = locale.trim().to_ascii_lowercase();
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

    let tmp_path = instance_dir.join(format!(
        ".{MINECRAFT_OPTIONS_FILE}.{}.tmp",
        std::process::id()
    ));

    let write_res = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, &options_path)?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(MqError::Io(err));
    }

    Ok(())
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
            let clean = val.trim().to_ascii_lowercase();
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_creates_options_file_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        sync_minecraft_options_lang(tmp.path(), "uk_ua").expect("sync lang");

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read options");
        assert_eq!(content.trim(), "lang:uk_ua");
        assert_eq!(
            read_minecraft_options_lang(tmp.path()).as_deref(),
            Some("uk_ua")
        );
    }

    #[test]
    fn sync_updates_existing_lang_preserving_other_settings() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let initial = "fov:70.0\nlang:en_us\nrenderDistance:12\nguiScale:2\n";
        fs::write(tmp.path().join("options.txt"), initial).expect("write initial options");

        sync_minecraft_options_lang(tmp.path(), "ru_ru").expect("sync ru_ru");

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read options");
        assert!(content.contains("fov:70.0"));
        assert!(content.contains("renderDistance:12"));
        assert!(content.contains("guiScale:2"));
        assert!(content.contains("lang:ru_ru"));
        assert!(!content.contains("lang:en_us"));
        assert_eq!(
            read_minecraft_options_lang(tmp.path()).as_deref(),
            Some("ru_ru")
        );
    }

    #[test]
    fn sync_appends_lang_if_other_settings_exist_without_lang() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let initial = "fov:90.0\nrenderDistance:16\n";
        fs::write(tmp.path().join("options.txt"), initial).expect("write initial options");

        sync_minecraft_options_lang(tmp.path(), "pl_pl").expect("sync pl_pl");

        let content = fs::read_to_string(tmp.path().join("options.txt")).expect("read options");
        assert!(content.contains("fov:90.0"));
        assert!(content.contains("renderDistance:16"));
        assert!(content.contains("lang:pl_pl"));
        assert_eq!(
            read_minecraft_options_lang(tmp.path()).as_deref(),
            Some("pl_pl")
        );
    }
}
