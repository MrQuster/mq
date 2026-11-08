//! JVM runtime discovery (`/usr/lib/jvm/*`, `$JAVA_HOME`, `$PATH`), fast-path `<home>/release`
//! metadata parsing, `java -version` fallback, and automatic Java 21 / 17 / 8 selection.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mq_core::{MqError, MqResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Standard system JVM installation root directories probed on Linux.
pub const SYSTEM_JVM_DIRS: [&str; 2] = ["/usr/lib/jvm", "/usr/lib64/jvm"];

/// Discovered Java Virtual Machine installation on the local system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaRuntime {
    /// Canonical path to the `java` executable (`<home>/bin/java`).
    pub path: PathBuf,
    /// Root directory of the JVM installation (`<home>`).
    pub home_dir: PathBuf,
    /// Parsed major Java feature version (e.g. `8`, `17`, `21`).
    pub major_version: u32,
    /// Full version string (e.g. `"21.0.13"` or `"1.8.0_412"`).
    pub full_version: String,
    /// Implementor or vendor label (e.g. `"Arch Linux"`, `"Eclipse Adoptium"`, `"OpenJDK"`).
    pub vendor: String,
}

/// Discovers all installed Java runtimes across system directories, `~/.local/share/mq/java/*`
/// (or Windows `%LOCALAPPDATA%\MQ\java`), `$JAVA_HOME`, and `$PATH`.
///
/// Deduplicates runtimes by canonical binary path and sorts deterministically by
/// `(major_version, path)`.
pub fn discover_java_runtimes() -> Vec<JavaRuntime> {
    let mut jvm_roots: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for env_var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(dir) = env::var(env_var) {
                let base = PathBuf::from(dir);
                jvm_roots.push(base.join("Java"));
                jvm_roots.push(base.join("Eclipse Adoptium"));
                jvm_roots.push(base.join("AdoptOpenJDK"));
                jvm_roots.push(base.join("Zulu"));
                jvm_roots.push(base.join("BellSoft"));
                jvm_roots.push(base.join("Programs").join("Eclipse Adoptium"));
            }
        }
        let mq_java = mq_core::platform::default_data_dir().join("java");
        if mq_java.exists() {
            jvm_roots.push(mq_java);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        jvm_roots.extend(SYSTEM_JVM_DIRS.iter().map(PathBuf::from));
        if let Ok(home) = env::var("HOME") {
            let mq_java = PathBuf::from(home).join(".local/share/mq/java");
            if mq_java.exists() {
                jvm_roots.push(mq_java);
            }
        }
    }

    let roots_ref: Vec<&Path> = jvm_roots.iter().map(PathBuf::as_path).collect();
    let java_home = env::var_os("JAVA_HOME").map(PathBuf::from);
    let path_env = env::var_os("PATH");

    discover_java_runtimes_from_sources(&roots_ref, java_home.as_deref(), path_env.as_deref())
}

/// Pure/testable JVM discovery helper accepting explicit search roots, `JAVA_HOME`, and `PATH`.
pub fn discover_java_runtimes_from_sources(
    jvm_roots: &[&Path],
    java_home: Option<&Path>,
    path_env: Option<&OsStr>,
) -> Vec<JavaRuntime> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let java_bin_names: &[&str] = if cfg!(target_os = "windows") {
        &["java.exe", "javaw.exe", "java"]
    } else {
        &["java"]
    };

    // 1. Scan JVM roots
    for root in jvm_roots {
        if let Ok(entries) = fs::read_dir(root) {
            let mut dir_paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|entry| entry.path()))
                .collect();
            dir_paths.sort();

            for entry_path in dir_paths {
                if !entry_path.is_dir() {
                    continue;
                }
                for &bin_name in java_bin_names {
                    let bin_java = entry_path.join("bin").join(bin_name);
                    if is_executable_file(&bin_java) {
                        candidates.push(bin_java);
                    }
                    let jre_bin_java = entry_path.join("jre").join("bin").join(bin_name);
                    if is_executable_file(&jre_bin_java) {
                        candidates.push(jre_bin_java);
                    }
                }
            }
        }
    }

    // 2. Check $JAVA_HOME
    if let Some(home) = java_home.filter(|p| !p.as_os_str().is_empty()) {
        for &bin_name in java_bin_names {
            let bin_java = home.join("bin").join(bin_name);
            if is_executable_file(&bin_java) {
                candidates.push(bin_java);
            }
        }
        if is_executable_file(home) {
            candidates.push(home.to_path_buf());
        }
    }

    // 3. Scan $PATH entries
    if let Some(paths_os) = path_env.filter(|p| !p.is_empty()) {
        for dir in env::split_paths(paths_os) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            for &bin_name in java_bin_names {
                let bin_java = dir.join(bin_name);
                if is_executable_file(&bin_java) {
                    candidates.push(bin_java);
                }
            }
        }
    }

    // Deduplicate by canonical path and probe each unique binary once.
    let mut by_canon_path: BTreeMap<PathBuf, JavaRuntime> = BTreeMap::new();
    for candidate in candidates {
        let canon = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if by_canon_path.contains_key(&canon) {
            continue;
        }
        if let Some(runtime) = probe_java_binary(&canon) {
            by_canon_path.insert(canon, runtime);
        }
    }

    let mut runtimes: Vec<JavaRuntime> = by_canon_path.into_values().collect();
    runtimes.sort_by(|a, b| {
        a.major_version
            .cmp(&b.major_version)
            .then_with(|| a.path.cmp(&b.path))
    });
    runtimes
}

/// Probes a single `java` binary (or JVM home directory), preferring the fast-path
/// `<home>/release` file before falling back to spawning `<java_bin> -version`.
pub fn probe_java_binary(java_bin_or_home: &Path) -> Option<JavaRuntime> {
    let raw_bin = if java_bin_or_home.is_dir() {
        let primary = java_bin_or_home.join("bin").join("java");
        if primary.is_file() {
            primary
        } else {
            java_bin_or_home.join("jre").join("bin").join("java")
        }
    } else {
        java_bin_or_home.to_path_buf()
    };

    if !raw_bin.is_file() {
        return None;
    }

    let canon_bin = fs::canonicalize(&raw_bin).unwrap_or(raw_bin);
    let home_dir = infer_jvm_home(&canon_bin);

    // 1. Fast path: check `<home_dir>/release` (or parent `<home_dir>/../release` for `jre/bin/java`).
    let mut release_candidates = vec![home_dir.join("release")];
    if let Some(parent_home) = home_dir.parent() {
        release_candidates.push(parent_home.join("release"));
    }

    for rel_path in release_candidates {
        if let Ok(content) = fs::read_to_string(&rel_path) {
            if let Some((major_version, full_version, vendor)) = parse_java_release_file(&content) {
                let effective_home = rel_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| home_dir.clone());
                return Some(JavaRuntime {
                    path: canon_bin,
                    home_dir: effective_home,
                    major_version,
                    full_version,
                    vendor,
                });
            }
        }
    }

    // 2. Fallback: run `<canon_bin> -version`.
    if let Ok(output) = Command::new(&canon_bin).arg("-version").output() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = if stderr.trim().is_empty() {
            stdout.into_owned()
        } else if stdout.trim().is_empty() {
            stderr.into_owned()
        } else {
            format!("{stderr}\n{stdout}")
        };

        if let Some((major_version, full_version, vendor)) = parse_java_version_output(&combined) {
            return Some(JavaRuntime {
                path: canon_bin,
                home_dir,
                major_version,
                full_version,
                vendor,
            });
        }
    }

    None
}

/// Parses an OpenJDK `<jvm_home>/release` key-value file into `(major_version, full_version, vendor)`.
///
/// Supports both modern versions (`JAVA_VERSION="21.0.13"`, `JAVA_VERSION="17.0.21"`) and
/// legacy Java 8 versions (`JAVA_VERSION="1.8.0_412"`).
pub fn parse_java_release_file(content: &str) -> Option<(u32, String, String)> {
    let mut raw_version: Option<String> = None;
    let mut implementor: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let clean_val = val.trim().trim_matches('"').trim_matches('\'').trim();
        if clean_val.is_empty() {
            continue;
        }

        match key {
            "JAVA_VERSION" => raw_version = Some(clean_val.to_owned()),
            "IMPLEMENTOR" => implementor = Some(clean_val.to_owned()),
            "VENDOR" if implementor.is_none() => implementor = Some(clean_val.to_owned()),
            _ => {}
        }
    }

    let version_str = raw_version?;
    let (major, full) = parse_java_version_str(&version_str)?;
    let vendor = implementor.unwrap_or_else(|| "OpenJDK".to_owned());
    Some((major, full, vendor))
}

/// Parses `java -version` stderr/stdout output into `(major_version, full_version, vendor)`.
pub fn parse_java_version_output(output: &str) -> Option<(u32, String, String)> {
    let mut version_tuple: Option<(u32, String)> = None;
    let mut vendor = "OpenJDK".to_owned();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if version_tuple.is_none() {
            // Standard format: `openjdk version "21.0.5" 2024-10-15` or `java version "1.8.0_412"`
            if let Some(first_quote) = trimmed.find('"') {
                let rest = &trimmed[first_quote + 1..];
                if let Some(end_quote) = rest.find('"') {
                    let quoted = &rest[..end_quote];
                    version_tuple = parse_java_version_str(quoted);
                }
            }
            if version_tuple.is_none() {
                for token in trimmed.split_whitespace() {
                    if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                        if let Some(parsed) = parse_java_version_str(token) {
                            version_tuple = Some(parsed);
                            break;
                        }
                    }
                }
            }
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("temurin") || lower.contains("adoptium") {
            vendor = "Eclipse Adoptium".to_owned();
        } else if lower.contains("zulu") {
            vendor = "Azul Zulu".to_owned();
        } else if lower.contains("corretto") {
            vendor = "Amazon Corretto".to_owned();
        } else if lower.contains("graalvm") {
            vendor = "GraalVM".to_owned();
        } else if lower.starts_with("java(tm)") {
            vendor = "Oracle".to_owned();
        }
    }

    let (major, full) = version_tuple?;
    Some((major, full, vendor))
}

/// Parses a Java version string (such as `"21.0.13"`, `"17.0.21"`, `"1.8.0_412"`, `"21-ea"`)
/// into `(major_version, cleaned_version_string)`.
pub fn parse_java_version_str(raw: &str) -> Option<(u32, String)> {
    let cleaned = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if cleaned.is_empty() {
        return None;
    }

    if let Some(after_one_dot) = cleaned.strip_prefix("1.") {
        let major_digits: String = after_one_dot
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let major = major_digits.parse::<u32>().ok()?;
        if major > 0 {
            return Some((major, cleaned.to_owned()));
        }
    }

    let major_digits: String = cleaned.chars().take_while(|c| c.is_ascii_digit()).collect();
    let major = major_digits.parse::<u32>().ok()?;
    if major == 0 {
        return None;
    }
    Some((major, cleaned.to_owned()))
}

/// Returns the required Java major version for a given Minecraft version.
///
/// Priority 1 rule: If `spec_hint` is provided (from `version.json`'s `javaVersion.majorVersion`),
/// it is returned strictly without modification.
/// If `spec_hint` is `None`, falls back to inferring the Java version based on the Minecraft version:
/// - `25` for Minecraft `>= 26.x`
/// - `21` for Minecraft `>= 1.20.5`
/// - `17` for Minecraft `1.18..=1.20.4`
/// - `8` for Minecraft `<= 1.17.x`
pub fn required_java_major_for_mc(mc_version: &str, spec_hint: Option<u32>) -> u32 {
    if let Some(hint) = spec_hint {
        return hint;
    }
    infer_java_major_from_mc_version(mc_version)
}

/// Infers the expected Java major version from a Minecraft version string when `version.json` lacks
/// explicit `javaVersion` metadata.
pub fn infer_java_major_from_mc_version(mc_version: &str) -> u32 {
    let core = mc_version
        .trim()
        .split(['-', '+', ' '])
        .next()
        .unwrap_or("");
    let parts: Vec<u32> = core
        .split('.')
        .map_while(|seg| seg.parse::<u32>().ok())
        .collect();

    match parts.as_slice() {
        [major, ..] if *major >= 26 => 25,
        [1, minor, patch, ..] => {
            if *minor > 20 || (*minor == 20 && *patch >= 5) {
                21
            } else if *minor >= 18 {
                17
            } else {
                8
            }
        }
        [1, minor] => {
            if *minor > 20 {
                21
            } else if *minor >= 18 {
                17
            } else {
                8
            }
        }
        _ => 21,
    }
}

/// Selects the best matching [`JavaRuntime`] for `required_major`:
/// 1. If `override_path` is provided, uses that binary (probing its metadata if possible, or
///    wrapping it directly if it exists on disk).
/// 2. Picks an exact match where `major_version == required_major`.
/// 3. Otherwise picks the closest compatible runtime with `major_version > required_major`
///    (smallest `major_version >= required_major`).
/// 4. Otherwise falls back to the highest available `major_version < required_major` with a
///    diagnostic warning.
/// 5. Returns [`MqError::JavaNotFound`] if no Java runtime is available.
pub fn select_java_runtime(
    runtimes: &[JavaRuntime],
    required_major: u32,
    override_path: Option<&Path>,
) -> MqResult<JavaRuntime> {
    if let Some(custom) = override_path.filter(|p| !p.as_os_str().is_empty()) {
        let canon_custom = fs::canonicalize(custom).unwrap_or_else(|_| custom.to_path_buf());
        if let Some(found) = runtimes
            .iter()
            .find(|r| r.path == custom || r.path == canon_custom || r.home_dir == canon_custom)
        {
            return Ok(found.clone());
        }

        if let Some(probed) = probe_java_binary(custom) {
            return Ok(probed);
        }

        if custom.is_file() {
            return Ok(JavaRuntime {
                path: canon_custom.clone(),
                home_dir: infer_jvm_home(&canon_custom),
                major_version: required_major,
                full_version: format!("{required_major}.0.0"),
                vendor: "Custom".to_owned(),
            });
        }

        return Err(MqError::JavaNotFound {
            required_major,
            hint: format!(
                "configured java_path override '{}' does not exist or is not a valid executable",
                custom.display()
            ),
        });
    }

    if runtimes.is_empty() {
        return Err(MqError::JavaNotFound {
            required_major,
            hint: format!(
                "no Java runtime found in /usr/lib/jvm, $JAVA_HOME, or $PATH; install jdk{required_major}-openjdk"
            ),
        });
    }

    // 1. Exact major_version match (prefer the last/newest path among identical majors).
    if let Some(exact) = runtimes
        .iter()
        .rev()
        .find(|r| r.major_version == required_major)
    {
        return Ok(exact.clone());
    }

    // 2. Closest installed major_version > required_major.
    if let Some(compatible_higher) = runtimes
        .iter()
        .filter(|r| r.major_version > required_major)
        .min_by(|a, b| {
            a.major_version
                .cmp(&b.major_version)
                .then_with(|| a.path.cmp(&b.path))
        })
    {
        tracing::info!(
            required_major,
            selected_major = compatible_higher.major_version,
            path = %compatible_higher.path.display(),
            "exact Java major version not installed; selected closest newer compatible JVM"
        );
        return Ok(compatible_higher.clone());
    }

    // 3. Fallback to highest installed major_version < required_major.
    if let Some(fallback_lower) = runtimes.iter().max_by(|a, b| {
        a.major_version
            .cmp(&b.major_version)
            .then_with(|| a.path.cmp(&b.path))
    }) {
        tracing::warn!(
            required_major,
            selected_major = fallback_lower.major_version,
            path = %fallback_lower.path.display(),
            "Java {}+ not found; falling back to closest installed JVM (Java {})",
            required_major,
            fallback_lower.major_version
        );
        return Ok(fallback_lower.clone());
    }

    Err(MqError::JavaNotFound {
        required_major,
        hint: format!("install jdk{required_major}-openjdk under /usr/lib/jvm"),
    })
}

fn infer_jvm_home(java_bin: &Path) -> PathBuf {
    if let Some(bin_dir) = java_bin.parent() {
        if bin_dir.file_name().and_then(|n| n.to_str()) == Some("bin") {
            if let Some(home) = bin_dir.parent() {
                return home.to_path_buf();
            }
        }
        return bin_dir.to_path_buf();
    }
    java_bin.to_path_buf()
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Verifies that `<java_bin> -version` runs successfully and parses its output into a [`JavaRuntime`].
pub fn verify_java_executable(java_bin: &Path) -> MqResult<JavaRuntime> {
    if !java_bin.is_file() {
        return Err(MqError::JavaNotFound {
            required_major: 0,
            hint: format!("Java binary '{}' does not exist", java_bin.display()),
        });
    }

    let canon_bin = fs::canonicalize(java_bin).unwrap_or_else(|_| java_bin.to_path_buf());
    let output = Command::new(&canon_bin)
        .arg("-version")
        .output()
        .map_err(|e| {
            MqError::Internal(format!(
                "failed to execute '{} -version': {e}",
                canon_bin.display()
            ))
        })?;

    if !output.status.success() {
        return Err(MqError::Internal(format!(
            "executing '{} -version' exited with code {:?}",
            canon_bin.display(),
            output.status.code()
        )));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = if stderr.trim().is_empty() {
        stdout.into_owned()
    } else if stdout.trim().is_empty() {
        stderr.into_owned()
    } else {
        format!("{stderr}\n{stdout}")
    };

    if let Some((major_version, full_version, vendor)) = parse_java_version_output(&combined) {
        let home_dir = infer_jvm_home(&canon_bin);
        Ok(JavaRuntime {
            path: canon_bin,
            home_dir,
            major_version,
            full_version,
            vendor,
        })
    } else {
        Err(MqError::Internal(format!(
            "could not parse java -version output from '{}':\n{combined}",
            canon_bin.display()
        )))
    }
}

/// Origin of a resolved or installed Java runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JavaSource {
    /// System-installed runtime from `/usr/lib/jvm`, `$JAVA_HOME`, or `$PATH`.
    System,
    /// Official Mojang JRE package (`mojang-<component>`).
    Mojang,
    /// Eclipse Adoptium Temurin JRE.
    Adoptium,
    /// Azul Zulu JRE.
    Zulu,
    /// Custom path explicitly provided by user.
    Custom,
}

/// Java runtime selected and verified through the cascading priority engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedJava {
    pub runtime: JavaRuntime,
    pub source: JavaSource,
    pub component: Option<String>,
}

/// Cascading Java runtime resolver with automatic download:
/// 1. Priority 1: Suitable system Java (`/usr/lib/jvm`, `$JAVA_HOME`, `$PATH`)
///    matching or compatible with `required_major`.
/// 2. Priority 2: Official Mojang runtime (`mojang-<component>`) for the specified
///    `component_hint` or `required_major`.
/// 3. Priority 3: Adoptium Temurin or Azul Zulu JRE (8, 17, 21, 25).
pub async fn ensure_java_runtime(
    client: &reqwest::Client,
    java_dir: &Path,
    required_major: u32,
    component_hint: Option<&str>,
    override_path: Option<&Path>,
) -> MqResult<SelectedJava> {
    // 0. Explicit override path
    if let Some(custom) = override_path.filter(|p| !p.as_os_str().is_empty()) {
        if let Ok(verified) = verify_java_executable(custom) {
            return Ok(SelectedJava {
                runtime: verified,
                source: JavaSource::Custom,
                component: None,
            });
        }
        if let Some(probed) = probe_java_binary(custom) {
            return Ok(SelectedJava {
                runtime: probed,
                source: JavaSource::Custom,
                component: None,
            });
        }
        return Err(MqError::JavaNotFound {
            required_major,
            hint: format!(
                "override java_path '{}' does not exist or failed verification",
                custom.display()
            ),
        });
    }

    // 1. Priority (1): System Java matching or compatible with required_major
    let system_runtimes = discover_java_runtimes();

    // Check exact match first
    if let Some(exact) = system_runtimes
        .iter()
        .find(|r| r.major_version == required_major)
    {
        if let Ok(verified) = verify_java_executable(&exact.path) {
            return Ok(SelectedJava {
                runtime: verified,
                source: JavaSource::System,
                component: None,
            });
        }
    }

    // Check compatible higher system Java (e.g. required 17, system has 21)
    if required_major >= 17 {
        if let Some(compatible) = system_runtimes
            .iter()
            .filter(|r| r.major_version > required_major)
            .min_by_key(|r| r.major_version)
        {
            if let Ok(verified) = verify_java_executable(&compatible.path) {
                tracing::info!(
                    required_major,
                    system_major = verified.major_version,
                    path = %verified.path.display(),
                    "using compatible newer system Java"
                );
                return Ok(SelectedJava {
                    runtime: verified,
                    source: JavaSource::System,
                    component: None,
                });
            }
        }
    }

    // 2. Priority (2): Official Mojang Java Runtime
    let comp = component_hint
        .unwrap_or_else(|| crate::mojang_java::component_for_major_version(required_major));

    match crate::mojang_java::ensure_mojang_java_runtime(client, java_dir, comp, None, None).await {
        Ok(mojang_rt) => {
            return Ok(SelectedJava {
                runtime: mojang_rt,
                source: JavaSource::Mojang,
                component: Some(comp.to_string()),
            });
        }
        Err(e) => {
            tracing::warn!(
                component = comp,
                error = %e,
                "Mojang java-runtime download failed; attempting Adoptium fallback"
            );
        }
    }

    // 3. Priority (3): Adoptium Temurin or Zulu JRE (8, 17, 21, 25)
    match download_adoptium_java(client, required_major, java_dir).await {
        Ok(adoptium_rt) => {
            return Ok(SelectedJava {
                runtime: adoptium_rt,
                source: JavaSource::Adoptium,
                component: None,
            });
        }
        Err(e) => {
            tracing::warn!(
                feature_version = required_major,
                error = %e,
                "Adoptium JRE download failed; attempting Azul Zulu fallback"
            );
        }
    }

    match download_zulu_java(client, required_major, java_dir).await {
        Ok(zulu_rt) => {
            return Ok(SelectedJava {
                runtime: zulu_rt,
                source: JavaSource::Zulu,
                component: None,
            });
        }
        Err(e) => {
            tracing::error!(
                feature_version = required_major,
                error = %e,
                "Azul Zulu JRE download failed"
            );
        }
    }

    Err(MqError::JavaNotFound {
        required_major,
        hint: format!(
            "failed to find or download Java {required_major} runtime from all sources (System, Mojang, Adoptium, Zulu)"
        ),
    })
}

/// Downloads and installs an Eclipse Temurin JRE binary from the Adoptium API v3.
/// Supported feature versions: 8, 11, 16, 17, 21, 25.
/// Target directory: `<target_parent_dir>/temurin-<version>-<arch>`
pub async fn download_adoptium_java(
    client: &reqwest::Client,
    feature_version: u32,
    target_parent_dir: &Path,
) -> MqResult<JavaRuntime> {
    if !matches!(feature_version, 8 | 11 | 16 | 17 | 21 | 25) {
        return Err(MqError::Internal(format!(
            "unsupported Java feature version: {feature_version} (supported: 8, 11, 16, 17, 21, 25)"
        )));
    }

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => {
            return Err(MqError::Internal(format!(
                "unsupported CPU architecture for Temurin JRE: {other}"
            )))
        }
    };

    let target_os = if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let bin_name = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    let dest_dir_name = format!("temurin-{feature_version}-{arch}");
    let dest_dir = target_parent_dir.join(&dest_dir_name);
    let bin_java = dest_dir.join("bin").join(bin_name);

    // If already installed and probeable, return existing
    if bin_java.is_file() {
        if let Ok(rt) = verify_java_executable(&bin_java) {
            return Ok(rt);
        }
        if let Some(rt) = probe_java_binary(&bin_java) {
            return Ok(rt);
        }
    }

    fs::create_dir_all(target_parent_dir)?;

    // 1. Fetch checksum if available
    let checksum_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{feature_version}/hotspot?architecture={arch}&image_type=jre&os={target_os}"
    );
    let mut expected_checksum = None;
    if let Ok(resp) = client.get(&checksum_url).send().await {
        if resp.status().is_success() {
            if let Ok(val) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = val.as_array() {
                    for item in arr {
                        if let Some(cs) = item
                            .pointer("/binary/package/checksum")
                            .and_then(|v| v.as_str())
                        {
                            expected_checksum = Some(cs.trim().to_lowercase());
                            break;
                        }
                    }
                }
            }
        }
    }

    // 2. Download binary (zip for Windows, tar.gz for Linux)
    let binary_url = format!(
        "https://api.adoptium.net/v3/binary/latest/{feature_version}/ga/{target_os}/{arch}/jre/hotspot/normal/eclipse"
    );
    let resp = client
        .get(&binary_url)
        .send()
        .await
        .map_err(|e| MqError::Network(format!("failed to initiate Adoptium download: {e}")))?;

    if !resp.status().is_success() {
        return Err(MqError::Network(format!(
            "Adoptium returned HTTP status {} for Java {feature_version}",
            resp.status()
        )));
    }

    let archive_bytes = resp
        .bytes()
        .await
        .map_err(|e| MqError::Network(format!("failed to download Adoptium archive: {e}")))?;

    // 3. Verify SHA-256 if expected checksum is found
    if let Some(ref expected) = expected_checksum {
        let mut hasher = Sha256::new();
        hasher.update(&archive_bytes);
        let actual = hex::encode(hasher.finalize()).to_lowercase();
        if actual != *expected {
            return Err(MqError::Internal(format!(
                "Adoptium Java {feature_version} SHA-256 mismatch: expected {expected}, got {actual}"
            )));
        }
    }

    // 4. Safely extract archive
    let temp_extract = target_parent_dir.join(format!(
        ".tmp_extract_{feature_version}_{}",
        std::process::id()
    ));
    if temp_extract.exists() {
        let _ = fs::remove_dir_all(&temp_extract);
    }
    fs::create_dir_all(&temp_extract)?;

    if target_os == "windows" {
        let cursor = std::io::Cursor::new(&archive_bytes[..]);
        let mut zip_archive = zip::ZipArchive::new(cursor).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_extract);
            MqError::Internal(format!("failed to parse Adoptium zip: {e}"))
        })?;
        zip_archive.extract(&temp_extract).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_extract);
            MqError::Internal(format!("failed to unpack Adoptium zip archive: {e}"))
        })?;
    } else {
        let gz_decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
        let mut archive = tar::Archive::new(gz_decoder);
        archive.unpack(&temp_extract).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_extract);
            MqError::Internal(format!("failed to unpack Adoptium archive: {e}"))
        })?;
    }

    // Adoptium tarballs extract into a single root folder (e.g. `jdk-21.0.5+11-jre`)
    let mut root_extracted = temp_extract.clone();
    if let Ok(entries) = fs::read_dir(&temp_extract) {
        let valid_entries: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        if valid_entries.len() == 1 && valid_entries[0].is_dir() {
            root_extracted = valid_entries[0].clone();
        }
    }

    if dest_dir.exists() {
        let _ = fs::remove_dir_all(&dest_dir);
    }
    fs::rename(&root_extracted, &dest_dir)?;
    let _ = fs::remove_dir_all(&temp_extract);

    // Ensure permissions
    let final_bin = dest_dir.join("bin").join(bin_name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if final_bin.exists() {
            let _ = fs::set_permissions(&final_bin, fs::Permissions::from_mode(0o755));
        }
    }

    // Verify or probe the runtime
    verify_java_executable(&final_bin).or_else(|_| {
        probe_java_binary(&final_bin).ok_or_else(|| {
            MqError::Internal(format!(
                "installed Adoptium Java {feature_version} could not be probed at {}",
                final_bin.display()
            ))
        })
    })
}

/// Downloads and installs an Azul Zulu JRE binary.
/// Supported feature versions: 8, 11, 17, 21, 25.
/// Target directory: `<target_parent_dir>/zulu-<version>-<arch>`
pub async fn download_zulu_java(
    client: &reqwest::Client,
    feature_version: u32,
    target_parent_dir: &Path,
) -> MqResult<JavaRuntime> {
    if !matches!(feature_version, 8 | 11 | 17 | 21 | 25) {
        return Err(MqError::Internal(format!(
            "unsupported Java feature version for Zulu: {feature_version}"
        )));
    }

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => {
            return Err(MqError::Internal(format!(
                "unsupported CPU architecture for Zulu JRE: {other}"
            )))
        }
    };

    let (target_os, ext, bin_name) = if cfg!(target_os = "windows") {
        ("windows", "zip", "java.exe")
    } else {
        ("linux", "tar.gz", "java")
    };

    let dest_dir = target_parent_dir.join(format!("zulu-{feature_version}-{arch}"));
    let bin_java = dest_dir.join("bin").join(bin_name);

    if bin_java.is_file() {
        if let Ok(rt) = verify_java_executable(&bin_java) {
            return Ok(rt);
        }
        if let Some(rt) = probe_java_binary(&bin_java) {
            return Ok(rt);
        }
    }

    // Query Azul Zulu packages metadata API
    let api_url = format!(
        "https://api.azul.com/metadata/v1/zulu/packages/?java_version={feature_version}&os={target_os}&arch={arch}&archive_type={ext}&java_package_type=jre"
    );

    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| MqError::Network(format!("failed to query Zulu API: {e}")))?;

    if !resp.status().is_success() {
        return Err(MqError::Network(format!(
            "Zulu API returned HTTP status {}",
            resp.status()
        )));
    }

    let val = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| MqError::Network(format!("failed to parse Zulu JSON response: {e}")))?;

    let download_url = val
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("download_url"))
        .and_then(|u| u.as_str())
        .ok_or_else(|| {
            MqError::Network(format!(
                "no download URL found in Zulu API response for Java {feature_version}"
            ))
        })?;

    let dl_resp = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| MqError::Network(format!("failed to download Zulu archive: {e}")))?;

    if !dl_resp.status().is_success() {
        return Err(MqError::Network(format!(
            "Zulu download returned HTTP status {}",
            dl_resp.status()
        )));
    }

    let archive_bytes = dl_resp
        .bytes()
        .await
        .map_err(|e| MqError::Network(format!("failed to read Zulu archive bytes: {e}")))?;

    fs::create_dir_all(target_parent_dir)?;
    let temp_extract = target_parent_dir.join(format!(
        ".tmp_extract_zulu_{feature_version}_{}",
        std::process::id()
    ));
    if temp_extract.exists() {
        let _ = fs::remove_dir_all(&temp_extract);
    }
    fs::create_dir_all(&temp_extract)?;

    if target_os == "windows" {
        let cursor = std::io::Cursor::new(&archive_bytes[..]);
        let mut zip_archive = zip::ZipArchive::new(cursor).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_extract);
            MqError::Internal(format!("failed to parse Zulu zip: {e}"))
        })?;
        zip_archive.extract(&temp_extract).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_extract);
            MqError::Internal(format!("failed to unpack Zulu zip archive: {e}"))
        })?;
    } else {
        let gz_decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
        let mut archive = tar::Archive::new(gz_decoder);
        archive.unpack(&temp_extract).map_err(|e| {
            let _ = fs::remove_dir_all(&temp_extract);
            MqError::Internal(format!("failed to unpack Zulu archive: {e}"))
        })?;
    }

    let mut root_extracted = temp_extract.clone();
    if let Ok(entries) = fs::read_dir(&temp_extract) {
        let valid_entries: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        if valid_entries.len() == 1 && valid_entries[0].is_dir() {
            root_extracted = valid_entries[0].clone();
        }
    }

    if dest_dir.exists() {
        let _ = fs::remove_dir_all(&dest_dir);
    }
    fs::rename(&root_extracted, &dest_dir)?;
    let _ = fs::remove_dir_all(&temp_extract);

    let final_bin = dest_dir.join("bin").join(bin_name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if final_bin.exists() {
            let _ = fs::set_permissions(&final_bin, fs::Permissions::from_mode(0o755));
        }
    }

    verify_java_executable(&final_bin).or_else(|_| {
        probe_java_binary(&final_bin).ok_or_else(|| {
            MqError::Internal(format!(
                "installed Zulu Java {feature_version} could not be probed at {}",
                final_bin.display()
            ))
        })
    })
}
