//! XDG Base Directory layout (`~/.local/share/mq`, `~/.config/mq`, `~/.cache/mq`),
//! `--data-dir` root overrides, and Zip-Slip path safety checks.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use directories::BaseDirs;

use crate::error::{MqError, MqResult};

const APP_DIR_NAME: &str = "mq";

/// Canonical directory layout for all persistent data, configuration, and caches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqPaths {
    /// Persistent data root (default: `~/.local/share/mq`).
    pub data_dir: PathBuf,
    /// Configuration root (default: `~/.config/mq`).
    pub config_dir: PathBuf,
    /// Ephemeral cache root (default: `~/.cache/mq`).
    pub cache_dir: PathBuf,
}

impl MqPaths {
    /// Resolves canonical paths for the active platform:
    /// - Checks `MQ_DATA_DIR` environment variable; if set and non-empty, returns `Self::with_root(val)`.
    /// - Checks for `portable.flag` adjacent to `current_exe()`; if found, runs in portable mode rooted at `<exe_dir>/data`.
    /// - On Windows: `%LOCALAPPDATA%\MQ` (data), `%APPDATA%\MQ` (config), `%LOCALAPPDATA%\MQ\cache` (cache).
    /// - On Linux: strictly follows XDG Base Directory specification (`~/.local/share/mq`, `~/.config/mq`, `~/.cache/mq`).
    pub fn platform_default() -> MqResult<Self> {
        if let Some(val) = env::var_os("MQ_DATA_DIR").filter(|v| !v.is_empty()) {
            return Ok(Self::with_root(PathBuf::from(val)));
        }

        if let Ok(exe) = env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                if let Some(portable) = Self::portable_paths(exe_dir) {
                    return Ok(portable);
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            Ok(Self {
                data_dir: crate::platform::default_data_dir(),
                config_dir: crate::platform::default_config_dir(),
                cache_dir: crate::platform::default_cache_dir(),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::xdg_default()
        }
    }

    /// Standard entry point resolving platform-default directories.
    #[inline]
    pub fn default_paths() -> MqResult<Self> {
        Self::platform_default()
    }

    /// Returns portable paths rooted at `<exe_dir>/data` when `portable.flag` exists in `exe_dir`.
    pub fn portable_paths(exe_dir: &Path) -> Option<Self> {
        if exe_dir.join("portable.flag").is_file() {
            Some(Self::with_root(exe_dir.join("data")))
        } else {
            None
        }
    }

    /// Resolves paths strictly following the XDG Base Directory Specification:
    /// - `$XDG_DATA_HOME/mq` or `$HOME/.local/share/mq`
    /// - `$XDG_CONFIG_HOME/mq` or `$HOME/.config/mq`
    /// - `$XDG_CACHE_HOME/mq` or `$HOME/.cache/mq`
    pub fn xdg_default() -> MqResult<Self> {
        let xdg_data = env::var_os("XDG_DATA_HOME");
        let xdg_config = env::var_os("XDG_CONFIG_HOME");
        let xdg_cache = env::var_os("XDG_CACHE_HOME");
        let home = env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .or_else(|| BaseDirs::new().map(|base| base.home_dir().as_os_str().to_os_string()));

        Self::from_xdg_vars(
            xdg_data.as_deref(),
            xdg_config.as_deref(),
            xdg_cache.as_deref(),
            home.as_deref(),
        )
    }

    /// Pure helper resolving XDG directories from explicit environment values.
    /// Relative or empty `$XDG_*` values are ignored per the XDG specification.
    pub fn from_xdg_vars(
        xdg_data: Option<&OsStr>,
        xdg_config: Option<&OsStr>,
        xdg_cache: Option<&OsStr>,
        home: Option<&OsStr>,
    ) -> MqResult<Self> {
        let home_path = home.filter(|h| !h.is_empty()).map(Path::new);

        let data_dir = resolve_xdg_dir(xdg_data, home_path, ".local/share")?;
        let config_dir = resolve_xdg_dir(xdg_config, home_path, ".config")?;
        let cache_dir = resolve_xdg_dir(xdg_cache, home_path, ".cache")?;

        Ok(Self {
            data_dir,
            config_dir,
            cache_dir,
        })
    }

    /// Constructs self-contained paths rooted under `root` (used for `--data-dir <path>`
    /// and hermetic tests):
    /// - `data_dir = <root>`
    /// - `config_dir = <root>/config`
    /// - `cache_dir = <root>/cache`
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            data_dir: root,
        }
    }

    /// Creates all required directory trees idempotently.
    pub fn ensure_dirs(&self) -> MqResult<()> {
        let dirs = [
            self.data_dir.clone(),
            self.config_dir.clone(),
            self.cache_dir.clone(),
            self.cas_dir(),
            self.cas_dir().join("sha1"),
            self.instances_dir(),
            self.versions_dir(),
            self.assets_dir(),
            self.libraries_dir(),
            self.temp_dir(),
            self.data_dir.join("cache"),
            self.java_dir(),
            self.backups_dir(),
            self.logs_dir(),
        ];

        for dir in dirs {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Content-addressable blob store root (`<data_dir>/cas`).
    #[inline]
    pub fn cas_dir(&self) -> PathBuf {
        self.data_dir.join("cas")
    }

    /// Sharded CAS blob path for a 40-char SHA-1 hex digest
    /// (`<data_dir>/cas/sha1/<xx>/<hex_sha1>`).
    pub fn cas_sha1_path(&self, hex_sha1: &str) -> PathBuf {
        let lower = hex_sha1.trim().to_ascii_lowercase();
        let shard = if lower.len() >= 2 { &lower[..2] } else { "00" };
        self.cas_dir().join("sha1").join(shard).join(lower)
    }

    /// Root directory containing isolated game instances (`<data_dir>/instances`).
    #[inline]
    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    /// Directory for a specific instance slug (`<data_dir>/instances/<slug>`).
    #[inline]
    pub fn instance_dir(&self, slug: &str) -> PathBuf {
        self.instances_dir().join(slug)
    }

    /// Shared version manifests and client JARs directory (`<data_dir>/versions`).
    #[inline]
    pub fn versions_dir(&self) -> PathBuf {
        self.data_dir.join("versions")
    }

    /// Shared Minecraft assets directory (`<data_dir>/assets`).
    #[inline]
    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    /// Shared Maven libraries directory (`<data_dir>/libraries`).
    #[inline]
    pub fn libraries_dir(&self) -> PathBuf {
        self.data_dir.join("libraries")
    }

    /// Launcher-managed Java runtimes directory (`<data_dir>/java`).
    #[inline]
    pub fn java_dir(&self) -> PathBuf {
        self.data_dir.join("java")
    }

    /// World backups directory (`<data_dir>/backups`).
    #[inline]
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Stored player accounts file (`<data_dir>/accounts.json`).
    #[inline]
    pub fn accounts_file(&self) -> PathBuf {
        self.data_dir.join("accounts.json")
    }

    /// Global TOML configuration file (`<config_dir>/config.toml`).
    #[inline]
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Dedicated theme configuration file (`<config_dir>/theme.toml`).
    #[inline]
    pub fn theme_file(&self) -> PathBuf {
        self.config_dir.join("theme.toml")
    }

    /// Canonical path to the cached `authlib-injector.jar` agent
    /// (`<data_dir>/cache/authlib-injector.jar`, i.e. `~/.local/share/mq/cache/authlib-injector.jar`).
    #[inline]
    pub fn authlib_injector_jar(&self) -> PathBuf {
        self.data_dir.join("cache").join("authlib-injector.jar")
    }

    /// Staging directory for in-flight `.part` downloads on the same filesystem
    /// as `data_dir` so atomic `rename(2)` never crosses mount boundaries (`<data_dir>/tmp`).
    #[inline]
    pub fn temp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }

    /// Directory for crash reports and diagnostic logs:
    /// - When `MQ_DATA_DIR` is set or paths were created via `with_root`: `<data_dir>/logs`
    /// - On Windows: `<data_dir>/logs` (`%LOCALAPPDATA%\MQ\logs`)
    /// - On Linux: `$XDG_STATE_HOME/mq/logs` or `$HOME/.local/state/mq/logs` (fallback: `<data_dir>/logs`)
    pub fn logs_dir(&self) -> PathBuf {
        let mq_data_dir = env::var_os("MQ_DATA_DIR");
        let xdg_state = env::var_os("XDG_STATE_HOME");
        let home = env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .or_else(|| BaseDirs::new().map(|base| base.home_dir().as_os_str().to_os_string()));

        self.resolve_logs_dir_from_vars(
            mq_data_dir.as_deref(),
            xdg_state.as_deref(),
            home.as_deref(),
        )
    }

    /// Pure helper for resolving `logs_dir` from explicit environment values.
    pub fn resolve_logs_dir_from_vars(
        &self,
        mq_data_dir: Option<&OsStr>,
        xdg_state: Option<&OsStr>,
        home: Option<&OsStr>,
    ) -> PathBuf {
        if mq_data_dir.filter(|v| !v.is_empty()).is_some()
            || self.config_dir == self.data_dir.join("config")
        {
            return self.data_dir.join("logs");
        }

        #[cfg(target_os = "windows")]
        {
            let _ = (xdg_state, home);
            self.data_dir.join("logs")
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Some(raw) = xdg_state.filter(|v| !v.is_empty()) {
                let candidate = Path::new(raw);
                if candidate.is_absolute() {
                    return candidate.join(APP_DIR_NAME).join("logs");
                }
            }
            if let Some(home_os) = home.filter(|v| !v.is_empty()) {
                return Path::new(home_os)
                    .join(".local/state")
                    .join(APP_DIR_NAME)
                    .join("logs");
            }
            self.data_dir.join("logs")
        }
    }

    /// Path to the persistent installation crash/interruption journal (`<data_dir>/install.journal`).
    #[inline]
    pub fn install_journal_path(&self) -> PathBuf {
        self.data_dir.join("install.journal")
    }
}

fn resolve_xdg_dir(
    xdg_val: Option<&OsStr>,
    home: Option<&Path>,
    home_rel_fallback: &str,
) -> MqResult<PathBuf> {
    if let Some(raw) = xdg_val.filter(|v| !v.is_empty()) {
        let candidate = Path::new(raw);
        if candidate.is_absolute() {
            return Ok(candidate.join(APP_DIR_NAME));
        }
    }

    let home = home.ok_or_else(|| {
        MqError::Internal("unable to resolve XDG paths: $HOME is not set".to_owned())
    })?;
    Ok(home.join(home_rel_fallback).join(APP_DIR_NAME))
}

/// Resolves an untrusted archive or manifest relative path strictly inside `base`,
/// rejecting empty paths, NUL bytes, absolute paths, drive prefixes, `..` traversal
/// segments, and symlink escapes (`MqError::ZipSlip`).
pub fn resolve_safe_path(base: &Path, untrusted_rel: &str) -> MqResult<PathBuf> {
    if untrusted_rel.is_empty()
        || untrusted_rel.contains('\0')
        || untrusted_rel.starts_with('/')
        || untrusted_rel.starts_with('\\')
    {
        return Err(MqError::ZipSlip {
            entry: untrusted_rel.to_owned(),
        });
    }

    // Reject Windows drive-letter prefixes such as "C:foo" or "C:\foo".
    let bytes = untrusted_rel.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(MqError::ZipSlip {
            entry: untrusted_rel.to_owned(),
        });
    }

    // Normalize backslash separators used by non-compliant ZIP archives before checking components.
    let normalized = untrusted_rel.replace('\\', "/");
    let rel_path = Path::new(&normalized);

    let mut clean_rel = PathBuf::new();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(seg) if !seg.is_empty() && seg != "." && seg != ".." => {
                clean_rel.push(seg);
            }
            Component::CurDir => {}
            _ => {
                return Err(MqError::ZipSlip {
                    entry: untrusted_rel.to_owned(),
                });
            }
        }
    }

    if clean_rel.as_os_str().is_empty() {
        return Err(MqError::ZipSlip {
            entry: untrusted_rel.to_owned(),
        });
    }

    // If `base` already exists on disk, ensure no existing symlink along `clean_rel` escapes `base`.
    if let Ok(canon_base) = base.canonicalize() {
        let mut cursor = base.to_path_buf();
        for seg in clean_rel.components() {
            cursor.push(seg);
            if cursor.symlink_metadata().is_ok() {
                let canon_cursor = cursor.canonicalize().map_err(|_| MqError::ZipSlip {
                    entry: untrusted_rel.to_owned(),
                })?;
                if !canon_cursor.starts_with(&canon_base) {
                    return Err(MqError::ZipSlip {
                        entry: untrusted_rel.to_owned(),
                    });
                }
            }
        }
    }

    Ok(base.join(clean_rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_home_fallbacks_match_specification() {
        let paths = MqPaths::from_xdg_vars(None, None, None, Some(OsStr::new("/home/player")))
            .expect("resolve from HOME");

        assert_eq!(
            paths.data_dir,
            PathBuf::from("/home/player/.local/share/mq")
        );
        assert_eq!(paths.config_dir, PathBuf::from("/home/player/.config/mq"));
        assert_eq!(paths.cache_dir, PathBuf::from("/home/player/.cache/mq"));
        assert_eq!(
            paths.authlib_injector_jar(),
            PathBuf::from("/home/player/.local/share/mq/cache/authlib-injector.jar")
        );
        assert_eq!(
            paths.accounts_file(),
            PathBuf::from("/home/player/.local/share/mq/accounts.json")
        );
        assert_eq!(
            paths.config_file(),
            PathBuf::from("/home/player/.config/mq/config.toml")
        );
    }

    #[test]
    #[cfg(unix)]
    fn xdg_explicit_overrides_and_relative_ignore_rules() {
        let paths = MqPaths::from_xdg_vars(
            Some(OsStr::new("/custom/data")),
            Some(OsStr::new("relative/ignored")),
            Some(OsStr::new("")),
            Some(OsStr::new("/home/player")),
        )
        .expect("resolve mixed XDG");

        assert_eq!(paths.data_dir, PathBuf::from("/custom/data/mq"));
        assert_eq!(paths.config_dir, PathBuf::from("/home/player/.config/mq"));
        assert_eq!(paths.cache_dir, PathBuf::from("/home/player/.cache/mq"));
        assert_eq!(
            paths.authlib_injector_jar(),
            PathBuf::from("/custom/data/mq/cache/authlib-injector.jar")
        );

        let missing_home = MqPaths::from_xdg_vars(None, None, None, None);
        assert!(missing_home.is_err());
    }

    #[test]
    fn with_root_and_ensure_dirs_create_complete_hierarchy() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());

        assert_eq!(paths.data_dir, tmp.path());
        assert_eq!(paths.config_dir, tmp.path().join("config"));
        assert_eq!(paths.cache_dir, tmp.path().join("cache"));
        assert_eq!(
            paths.authlib_injector_jar(),
            tmp.path().join("cache").join("authlib-injector.jar")
        );
        assert_eq!(
            paths.cas_sha1_path("A94A8FE5CCB19BA61C4C0873D391E987982FBBD3"),
            tmp.path()
                .join("cas/sha1/a9/a94a8fe5ccb19ba61c4c0873d391e987982fbbd3")
        );
        assert_eq!(
            paths.instance_dir("fabric-1.21"),
            tmp.path().join("instances/fabric-1.21")
        );

        paths.ensure_dirs().expect("ensure dirs");
        assert!(paths.cas_dir().is_dir());
        assert!(paths.instances_dir().is_dir());
        assert!(paths.versions_dir().is_dir());
        assert!(paths.assets_dir().is_dir());
        assert!(paths.libraries_dir().is_dir());
        assert!(paths.temp_dir().is_dir());
        assert!(paths.config_dir.is_dir());
        assert!(paths.cache_dir.is_dir());
        assert!(paths.logs_dir().is_dir());
        assert_eq!(paths.logs_dir(), tmp.path().join("logs"));
        assert_eq!(
            paths.install_journal_path(),
            tmp.path().join("install.journal")
        );
        assert!(paths.authlib_injector_jar().parent().unwrap().is_dir());
    }

    #[test]
    fn portable_flag_uses_data_dir_next_to_exe() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        assert!(MqPaths::portable_paths(tmp.path()).is_none());

        fs::write(tmp.path().join("portable.flag"), b"").expect("write portable.flag");
        let paths = MqPaths::portable_paths(tmp.path()).expect("portable paths should resolve");
        let expected_root = tmp.path().join("data");
        assert_eq!(paths.data_dir, expected_root);
        assert_eq!(paths.config_dir, expected_root.join("config"));
        assert_eq!(paths.cache_dir, expected_root.join("cache"));
        assert_eq!(paths.instances_dir(), expected_root.join("instances"));
    }

    #[test]
    fn logs_dir_resolution_rules() {
        let paths = MqPaths::from_xdg_vars(None, None, None, Some(OsStr::new("/home/player")))
            .expect("resolve from HOME");

        // When MQ_DATA_DIR is set, logs_dir is inside data_dir/logs
        assert_eq!(
            paths.resolve_logs_dir_from_vars(
                Some(OsStr::new("/tmp/mq-test")),
                Some(OsStr::new("/var/state")),
                Some(OsStr::new("/home/player")),
            ),
            paths.data_dir.join("logs")
        );

        #[cfg(not(target_os = "windows"))]
        {
            // Explicit absolute XDG_STATE_HOME
            assert_eq!(
                paths.resolve_logs_dir_from_vars(
                    None,
                    Some(OsStr::new("/custom/state")),
                    Some(OsStr::new("/home/player")),
                ),
                PathBuf::from("/custom/state/mq/logs")
            );

            // Relative XDG_STATE_HOME is ignored per XDG spec, falls back to $HOME/.local/state/mq/logs
            assert_eq!(
                paths.resolve_logs_dir_from_vars(
                    None,
                    Some(OsStr::new("relative/state")),
                    Some(OsStr::new("/home/player")),
                ),
                PathBuf::from("/home/player/.local/state/mq/logs")
            );
        }
    }

    #[test]
    fn resolve_safe_path_blocks_zip_slip_attacks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path();

        let ok = resolve_safe_path(base, "mods/sodium-fabric.jar").expect("valid path");
        assert_eq!(ok, base.join("mods/sodium-fabric.jar"));

        for malicious in [
            "",
            "../evil.jar",
            "mods/../../etc/passwd",
            "..\\..\\evil.dll",
            "/etc/shadow",
            "\\Windows\\System32\\cmd.exe",
            "C:\\evil.exe",
            "mods/bad\0name.jar",
            ".",
        ] {
            let err = resolve_safe_path(base, malicious).unwrap_err();
            assert!(
                matches!(err, MqError::ZipSlip { .. }),
                "expected ZipSlip for {malicious:?}, got {err:?}"
            );
        }

        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().expect("outside tempdir");
            let escape_link = base.join("escape_dir");
            std::os::unix::fs::symlink(outside.path(), &escape_link).expect("create symlink");
            let err = resolve_safe_path(base, "escape_dir/payload.jar").unwrap_err();
            assert!(matches!(err, MqError::ZipSlip { .. }));
        }
    }
}
