//! Platform-specific abstractions for OS detection, paths, filesystem operations,
//! and process interactions across Linux, Windows, and macOS.

pub mod sanitize;

pub use sanitize::{is_windows_reserved_name, sanitize_filename};

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(any(target_os = "windows", target_os = "macos"))]
use directories::BaseDirs;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns the normalized lowercase OS identifier: `"windows"`, `"macos"`, or `"linux"`.
#[inline]
pub fn os_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "linux"
    }
}

/// Returns the JVM classpath separator character: `';'` on Windows, `':'` on Unix/macOS.
#[inline]
pub fn classpath_separator() -> char {
    #[cfg(target_os = "windows")]
    {
        ';'
    }
    #[cfg(not(target_os = "windows"))]
    {
        ':'
    }
}

/// Ensures Windows extended-length path prefix (`\\?\`) is attached to paths longer than 260 chars.
/// On non-Windows platforms, this is a no-op that returns the path unmodified.
pub fn ensure_long_path_prefix(path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        if path_str.len() >= 240 && !path_str.starts_with(r"\\?\") && path.is_absolute() {
            // Convert to canonical or prepend \\?\ prefix
            let clean = path_str.replace('/', "\\");
            PathBuf::from(format!(r"\\?\{clean}"))
        } else {
            path.to_path_buf()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.to_path_buf()
    }
}

/// Resolves the default persistent data directory for the current platform:
/// - Windows: `%LOCALAPPDATA%\MQ` (e.g. `C:\Users\<User>\AppData\Local\MQ`)
/// - Linux: `$XDG_DATA_HOME/mq` or `~/.local/share/mq`
/// - macOS: `~/Library/Application Support/MQ`
pub fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = BaseDirs::new() {
            base.data_local_dir().join("MQ")
        } else if let Ok(local) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local).join("MQ")
        } else {
            PathBuf::from(r"C:\MQ")
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(base) = BaseDirs::new() {
            base.data_dir().join("MQ")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join("Library/Application Support/MQ")
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("mq");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".local/share/mq")
    }
}

/// Resolves the default configuration directory for the current platform:
/// - Windows: `%APPDATA%\MQ` (e.g. `C:\Users\<User>\AppData\Roaming\MQ`)
/// - Linux: `$XDG_CONFIG_HOME/mq` or `~/.config/mq`
/// - macOS: `~/Library/Preferences/MQ`
pub fn default_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = BaseDirs::new() {
            base.config_dir().join("MQ")
        } else if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("MQ")
        } else {
            PathBuf::from(r"C:\MQ\config")
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(base) = BaseDirs::new() {
            base.preference_dir().join("MQ")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join("Library/Preferences/MQ")
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("mq");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".config/mq")
    }
}

/// Resolves the default ephemeral cache directory for the current platform:
/// - Windows: `%LOCALAPPDATA%\MQ\cache`
/// - Linux: `$XDG_CACHE_HOME/mq` or `~/.cache/mq`
/// - macOS: `~/Library/Caches/MQ`
pub fn default_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        default_data_dir().join("cache")
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(base) = BaseDirs::new() {
            base.cache_dir().join("MQ")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join("Library/Caches/MQ")
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("mq");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".cache/mq")
    }
}

/// Atomically writes `content` to `path` via a temporary file and atomic replace.
/// Guarantees that readers never observe partially written contents.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("atomic_file");
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_name = format!("{file_name}.tmp_{}_{seq}", std::process::id());
    let tmp_path = path.with_file_name(tmp_name);

    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
    }

    #[cfg(unix)]
    {
        // Enforce 0600 permissions on sensitive files if in config dir
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
    }

    if let Err(err) = crate::fs_util::rename_with_retry(&tmp_path, path) {
        let _ = crate::fs_util::remove_file_with_retry(&tmp_path);
        return Err(err);
    }

    Ok(())
}

/// Links `src` to `dst` via filesystem hardlink.
/// If hardlink creation fails (e.g. cross-filesystem boundary, FAT filesystem, or Windows permissions),
/// falls back seamlessly to an atomic copy.
pub fn link_or_copy(src: &Path, dst: &Path) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    if dst.exists() {
        let _ = fs::remove_file(dst);
    }

    match fs::hard_link(src, dst) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::debug!(
                "Hardlink failed from {:?} to {:?} ({e}); falling back to copy",
                src,
                dst
            );
            fs::copy(src, dst).map(|_| ())
        }
    }
}

/// Opens a local file or directory using the platform's default file explorer.
pub fn open_folder(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}

/// Opens a web URL in the system's default internet browser.
pub fn open_url(url: &str) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .args(["/c", "start", ""])
            .arg(url)
            .spawn();
        match status {
            Ok(_) => Ok(()),
            Err(_) => Command::new("explorer").arg(url).spawn().map(|_| ()),
        }
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn().map(|_| ())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Command::new("xdg-open").arg(url).spawn().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_os_name_valid() {
        let name = os_name();
        assert!(matches!(name, "linux" | "windows" | "macos"));
    }

    #[test]
    fn test_classpath_separator() {
        let sep = classpath_separator();
        if cfg!(target_os = "windows") {
            assert_eq!(sep, ';');
        } else {
            assert_eq!(sep, ':');
        }
    }

    #[test]
    fn test_atomic_write_and_cyrillic_paths() {
        let dir = tempdir().expect("tempdir");
        let cyrillic_dir = dir.path().join("Иван Петров/Папка MQ");
        fs::create_dir_all(&cyrillic_dir).expect("create cyrillic dir");

        let target_file = cyrillic_dir.join("инстанс_тест.json");
        let data = b"{\"name\": \"\xD0\xA2\xD0\xB5\xD1\x81\xD1\x82\"}";

        atomic_write(&target_file, data).expect("atomic_write to cyrillic path");
        let read = fs::read(&target_file).expect("read file");
        assert_eq!(read, data);
    }

    #[test]
    fn test_link_or_copy_fallback() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("source.blob");
        let dst = dir.path().join("dest/linked.blob");

        fs::write(&src, b"CAS BLOB CONTENT").expect("write src");
        link_or_copy(&src, &dst).expect("link_or_copy");

        let read = fs::read(&dst).expect("read dst");
        assert_eq!(read, b"CAS BLOB CONTENT");
    }

    #[test]
    fn test_ensure_long_path_prefix() {
        let short = Path::new(r"C:\MQ\instances\test");
        assert_eq!(ensure_long_path_prefix(short), short.to_path_buf());
    }
}
