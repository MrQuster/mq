//! Deterministic Minecraft + Modloader launch command builder (Lesson E8),
//! Ely.by `-javaagent` first-flag injection, Offline UUID/token substitution,
//! wrapper command prefixing (`gamemoderun`, `mangohud`, `prime-run`), and `--dry-run` formatting.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mq_auth::{elyby_jvm_injection_args, AccountKind, AccountProfile};
use mq_core::{redact_text, resolve_safe_path, MqError, MqPaths, MqResult};
use mq_instances::InstanceManifest;
use mq_meta::{build_deterministic_classpath, format_classpath_string, ResolvedVersion};
use serde::{Deserialize, Serialize};

/// Canonical launcher brand string injected into `${launcher_name}`.
pub const LAUNCHER_NAME: &str = "mq";

/// Canonical launcher version string injected into `${launcher_version}`.
pub const LAUNCHER_VERSION: &str = "1.1.0";

/// Complete input context required to construct a deterministic Minecraft launch command.
#[derive(Debug, Clone)]
pub struct LaunchContext {
    pub paths: MqPaths,
    pub instance: InstanceManifest,
    pub version: ResolvedVersion,
    pub account: AccountProfile,
    pub java_bin: PathBuf,
    pub authlib_injector_jar: Option<PathBuf>,
    pub extra_secrets_to_redact: Vec<String>,
}

impl LaunchContext {
    /// Collects all sensitive tokens (access token, refresh token, client token, and extra secrets)
    /// that must be scrubbed from command-line logs and game `stdout`/`stderr` streams.
    pub fn collect_secrets(&self) -> Vec<String> {
        let mut secrets = Vec::new();
        let access = self.account.access_token.expose_secret();
        if !access.is_empty() && access != "0" {
            secrets.push(access.to_owned());
        }
        if let Some(ref refresh) = self.account.refresh_token {
            let s = refresh.expose_secret();
            if !s.is_empty() {
                secrets.push(s.to_owned());
            }
        }
        if let Some(ref client_tok) = self.account.client_token {
            let s = client_tok.expose_secret();
            if !s.is_empty() {
                secrets.push(s.to_owned());
            }
        }
        for extra in &self.extra_secrets_to_redact {
            if !extra.is_empty() {
                secrets.push(extra.clone());
            }
        }
        secrets
    }
}

/// Deterministically constructed child process command ready for `--dry-run` output or execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltLaunchCommand {
    /// Executable binary (`java_bin`, or wrapper binary such as `"gamemoderun"` if configured).
    pub program: PathBuf,
    /// Exact ordered process arguments.
    pub args: Vec<String>,
    /// Working directory (`<data_dir>/instances/<id>`).
    pub working_dir: PathBuf,
    /// Deterministic environment variable overrides.
    pub env: BTreeMap<String, String>,
}

impl BuiltLaunchCommand {
    /// Renders a deterministic single-line shell command representation (quoting arguments
    /// containing whitespace or quotes), suitable for `mq launch --dry-run`.
    pub fn to_shell_command_line(&self) -> String {
        let mut tokens = Vec::with_capacity(1 + self.args.len());
        tokens.push(shell_quote(&self.program.to_string_lossy()));
        for arg in &self.args {
            tokens.push(shell_quote(arg));
        }
        tokens.join(" ")
    }

    /// Renders the single-line shell command with all sensitive tokens replaced by `"[REDACTED]"`.
    pub fn to_redacted_command_line(&self, secrets: &[&str]) -> String {
        redact_text(&self.to_shell_command_line(), secrets)
    }
}

/// Builds a strictly deterministic [`BuiltLaunchCommand`] from [`LaunchContext`] (Lesson E8).
///
/// Guarantees:
/// - When `ctx.account.kind == AccountKind::ElyBy`, the VERY FIRST JVM flag (index 0 of JVM
///   arguments, before `-Xms`, `-Xmx`, `-D...`, `-cp`, and `mainClass`) is:
///   `-javaagent:/full/path/to/authlib-injector.jar=https://account.ely.by/api/authlib-injector`
///   immediately followed by `-Dauthlibinjector.side=client`.
/// - When `ctx.account.kind == AccountKind::Offline`, `${auth_access_token}` is ALWAYS `"0"`,
///   `${user_type}` is `"legacy"`, `${user_properties}` is `"{}"`, and `${auth_uuid}` is the
///   undashed 32-hex Offline UUID v3.
/// - Supports custom RAM (`-Xms<min>M`, `-Xmx<max>M`), user `instance.jvm_args`, and optional
///   wrapper commands (`gamemoderun`, `mangohud`, `prime-run`).
pub fn build_launch_command(ctx: &LaunchContext) -> MqResult<BuiltLaunchCommand> {
    if ctx.version.main_class.trim().is_empty() {
        return Err(MqError::Metadata(
            "resolved version main_class must not be empty".to_owned(),
        ));
    }

    let instance_dir = ctx.instance.instance_dir(&ctx.paths);
    let raw_game_dir = ctx.instance.game_dir(&ctx.paths);
    let game_dir = raw_game_dir
        .canonicalize()
        .unwrap_or_else(|_| std::path::absolute(&raw_game_dir).unwrap_or(raw_game_dir));
    let natives_dir = ctx.instance.natives_dir(&ctx.paths);
    let libraries_dir = ctx.paths.libraries_dir();
    let raw_assets_dir = ctx.paths.assets_dir();
    let assets_dir = raw_assets_dir
        .canonicalize()
        .unwrap_or_else(|_| std::path::absolute(&raw_assets_dir).unwrap_or(raw_assets_dir));
    let client_jar_path = resolve_client_jar_path(&ctx.paths, &instance_dir, &ctx.version)?;

    let classpath_entries =
        build_deterministic_classpath(&ctx.version, &libraries_dir, &client_jar_path);
    let classpath_str = format_classpath_string(&classpath_entries);

    let mut jvm_flags: Vec<String> = Vec::new();

    // 1. CRITICAL RULE FOR ELY.BY (Section 5.1.2 & DoD #6):
    // Always inject `-javaagent:<jar>=https://account.ely.by/api/authlib-injector` as the
    // VERY FIRST JVM flag before everything else, followed by `-Dauthlibinjector.side=client`.
    if ctx.account.kind == AccountKind::ElyBy {
        let injector_jar = ctx
            .authlib_injector_jar
            .clone()
            .unwrap_or_else(|| ctx.paths.authlib_injector_jar());
        let [agent_flag, side_flag] = elyby_jvm_injection_args(&injector_jar);
        jvm_flags.push(agent_flag);
        jvm_flags.push(side_flag);
    }

    // 2. Heap memory flags (-Xms / -Xmx).
    let min_mb = ctx.instance.memory_min_mb.max(256);
    let max_mb = ctx.instance.memory_max_mb.max(min_mb);
    jvm_flags.push(format!("-Xms{min_mb}M"));
    jvm_flags.push(format!("-Xmx{max_mb}M"));

    // Prepare JVM placeholders for substitutions
    let natives_str = natives_dir.to_string_lossy();
    let libs_str = libraries_dir.to_string_lossy();
    let game_dir_str = game_dir.to_string_lossy();
    let assets_dir_str = assets_dir.to_string_lossy();

    let log_path_str = if let Some(ref logging) = ctx.version.logging {
        if let Some(ref client) = logging.client {
            if !client.file.path.is_empty() {
                ctx.paths
                    .assets_dir()
                    .join(&client.file.path)
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let mut cp_sep_buf = [0u8; 4];
    let cp_sep = mq_core::platform::classpath_separator().encode_utf8(&mut cp_sep_buf);

    let jvm_subs = [
        ("natives_directory", natives_str.as_ref()),
        ("launcher_name", LAUNCHER_NAME),
        ("launcher_version", LAUNCHER_VERSION),
        ("classpath", classpath_str.as_str()),
        ("library_directory", libs_str.as_ref()),
        ("classpath_separator", cp_sep),
        ("version_name", ctx.version.id.as_str()),
        ("game_directory", game_dir_str.as_ref()),
        ("assets_root", assets_dir_str.as_ref()),
        ("path", log_path_str.as_str()),
    ];

    // 3. User custom JVM arguments from `instance.toml` (sanitized against stray placeholders).
    for custom_arg in &ctx.instance.jvm_args {
        let expanded = expand_and_sanitize_arg(custom_arg, &jvm_subs);
        let trimmed = expanded.trim();
        if !trimmed.is_empty() {
            jvm_flags.push(trimmed.to_owned());
        }
    }

    // 4. Expanded version JVM arguments.
    let mut has_library_path = false;
    let mut has_classpath = false;
    let mut has_logging_config = false;

    for raw_jvm_arg in &ctx.version.jvm_arguments {
        let expanded = expand_and_sanitize_arg(raw_jvm_arg, &jvm_subs);
        if expanded.starts_with("-Djava.library.path=") {
            has_library_path = true;
        }
        if expanded == "-cp" || expanded == "-classpath" {
            has_classpath = true;
        }
        if expanded.starts_with("-Dlog4j.configurationFile=") {
            has_logging_config = true;
        }
        jvm_flags.push(expanded);
    }

    // Inject logging configuration if specified in version.json and not yet added
    if !has_logging_config {
        if let Some(ref logging) = ctx.version.logging {
            if let Some(ref client) = logging.client {
                if !client.argument.trim().is_empty() {
                    let log_flag = expand_and_sanitize_arg(&client.argument, &jvm_subs);
                    if !log_flag.is_empty() {
                        jvm_flags.push(log_flag);
                    }
                }
            }
        }
    }

    if !has_library_path {
        jvm_flags.push(format!("-Djava.library.path={natives_str}"));
    }
    if !has_classpath {
        jvm_flags.push("-cp".to_owned());
        jvm_flags.push(classpath_str);
    }

    // 5. Main class.
    jvm_flags.push(ctx.version.main_class.trim().to_owned());

    // 5.1 Ensure UTF-8 file encoding on Windows for Cyrillic username/path safety
    if cfg!(target_os = "windows") && !jvm_flags.iter().any(|f| f.starts_with("-Dfile.encoding=")) {
        jvm_flags.push("-Dfile.encoding=UTF-8".to_string());
    }

    // 6. Expanded game arguments.
    let uuid_simple = ctx.account.uuid_simple();
    let access_token = match ctx.account.kind {
        AccountKind::Offline => "0",
        AccountKind::ElyBy | AccountKind::Microsoft => ctx.account.access_token.expose_secret(),
    };
    let user_type = ctx.account.kind.user_type_arg();
    let version_type = if ctx.version.id.contains("snapshot") || ctx.version.id.contains('w') {
        "snapshot"
    } else {
        "release"
    };
    let profile_name = ctx.instance.name.as_str();

    let game_subs = [
        ("auth_player_name", ctx.account.username.as_str()),
        ("version_name", ctx.version.id.as_str()),
        ("game_directory", game_dir_str.as_ref()),
        ("assets_root", assets_dir_str.as_ref()),
        ("game_assets", assets_dir_str.as_ref()),
        ("assets_index_name", ctx.version.asset_index.id.as_str()),
        ("auth_uuid", uuid_simple.as_str()),
        ("auth_access_token", access_token),
        ("auth_session", access_token),
        ("user_type", user_type),
        ("version_type", version_type),
        ("launcher_name", LAUNCHER_NAME),
        ("launcher_version", LAUNCHER_VERSION),
        ("user_properties", "{}"),
        ("clientid", "0"),
        ("auth_xuid", "0"),
        ("resolution_width", "854"),
        ("resolution_height", "480"),
        ("quickPlayPath", ""),
        ("quick_play_path", ""),
        ("quickPlaySingleplayer", ""),
        ("quickPlayMultiplayer", ""),
        ("quickPlayRealms", ""),
        ("path", log_path_str.as_str()),
        ("profile_name", profile_name),
    ];

    let default_game_args: Vec<String>;
    let source_game_args: &[String] = if ctx.version.game_arguments.is_empty() {
        default_game_args = default_minecraft_game_arguments();
        &default_game_args
    } else {
        &ctx.version.game_arguments
    };

    for raw_game_arg in source_game_args {
        let expanded = expand_and_sanitize_arg(raw_game_arg, &game_subs);
        jvm_flags.push(expanded);
    }

    // 7. Apply optional wrapper command prefix (Linux only: `gamemoderun`, `mangohud`, `prime-run`).
    let wrapper_effective = if cfg!(target_os = "windows") {
        None
    } else {
        ctx.instance
            .wrapper_cmd
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };

    let (program, args) = match wrapper_effective {
        Some(wrapper_raw) => {
            let mut wrapper_tokens = wrapper_raw.split_whitespace();
            let first_bin = wrapper_tokens.next().expect("non-empty wrapper");
            let mut full_args = Vec::with_capacity(2 + jvm_flags.len());
            for extra_tok in wrapper_tokens {
                full_args.push(extra_tok.to_owned());
            }
            full_args.push(ctx.java_bin.to_string_lossy().into_owned());
            full_args.extend(jvm_flags);
            (PathBuf::from(first_bin), full_args)
        }
        None => (ctx.java_bin.clone(), jvm_flags),
    };

    // Strict guarantee: sanitize any remaining stray unexpanded placeholders across all arguments
    let sanitized_args = args
        .into_iter()
        .map(|arg| {
            if arg.contains("${") {
                expand_and_sanitize_arg(&arg, &[])
            } else {
                arg
            }
        })
        .collect();

    Ok(BuiltLaunchCommand {
        program,
        args: sanitized_args,
        working_dir: instance_dir,
        env: BTreeMap::new(),
    })
}

fn resolve_client_jar_path(
    paths: &MqPaths,
    instance_dir: &Path,
    version: &ResolvedVersion,
) -> MqResult<PathBuf> {
    let instance_client_jar = instance_dir.join("client.jar");
    if instance_client_jar.exists() {
        return Ok(instance_client_jar);
    }

    let default_version_jar = paths
        .versions_dir()
        .join(&version.mc_version)
        .join(format!("{}.jar", version.mc_version));
    if default_version_jar.exists() {
        return Ok(default_version_jar);
    }

    let raw_path = version.client_jar.path.trim();
    if raw_path.is_empty() {
        return Ok(default_version_jar);
    }

    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        return Ok(candidate.to_path_buf());
    }

    if raw_path.starts_with("versions/") || raw_path.starts_with("versions\\") {
        resolve_safe_path(&paths.data_dir, raw_path)
    } else {
        resolve_safe_path(&paths.versions_dir(), raw_path)
    }
}

/// Expands known placeholders and guarantees that NO unreplaced `${...}` substrings
/// remain in the resulting argument string.
pub fn expand_and_sanitize_arg(template: &str, substitutions: &[(&str, &str)]) -> String {
    if !template.contains("${") {
        return template.to_owned();
    }

    let mut result = template.to_owned();
    for (k, v) in substitutions {
        let needle = format!("${{{k}}}");
        if result.contains(&needle) {
            result = result.replace(&needle, v);
        }
    }

    if result.contains("${") {
        let mut clean = String::with_capacity(result.len());
        let mut chars = result.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '$' && chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut var_name = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    if inner == '}' {
                        closed = true;
                        break;
                    }
                    var_name.push(inner);
                }
                if closed {
                    tracing::warn!(
                        "Unrecognized placeholder '${{{var_name}}}' in launch argument, replaced with empty string"
                    );
                } else {
                    tracing::warn!("Malformed unclosed placeholder in launch argument, scrubbed");
                }
            } else {
                clean.push(ch);
            }
        }
        result = clean;
    }

    result
}

pub fn expand_jvm_placeholder(
    template: &str,
    natives_dir: &str,
    libraries_dir: &str,
    classpath: &str,
    version_name: &str,
) -> String {
    let mut cp_sep_buf = [0u8; 4];
    let cp_sep = mq_core::platform::classpath_separator().encode_utf8(&mut cp_sep_buf);
    let subs = [
        ("natives_directory", natives_dir),
        ("launcher_name", LAUNCHER_NAME),
        ("launcher_version", LAUNCHER_VERSION),
        ("classpath", classpath),
        ("library_directory", libraries_dir),
        ("classpath_separator", cp_sep),
        ("version_name", version_name),
    ];
    expand_and_sanitize_arg(template, &subs)
}

#[allow(clippy::too_many_arguments)]
pub fn expand_game_placeholder(
    template: &str,
    player_name: &str,
    version_name: &str,
    game_dir: &str,
    assets_root: &str,
    assets_index_name: &str,
    auth_uuid: &str,
    auth_access_token: &str,
    user_type: &str,
) -> String {
    let subs = [
        ("auth_player_name", player_name),
        ("version_name", version_name),
        ("game_directory", game_dir),
        ("assets_root", assets_root),
        ("game_assets", assets_root),
        ("assets_index_name", assets_index_name),
        ("auth_uuid", auth_uuid),
        ("auth_access_token", auth_access_token),
        ("auth_session", auth_access_token),
        ("user_type", user_type),
        ("version_type", "release"),
        ("user_properties", "{}"),
        ("clientid", "0"),
        ("auth_xuid", "0"),
        ("resolution_width", "854"),
        ("resolution_height", "480"),
        ("launcher_name", LAUNCHER_NAME),
        ("launcher_version", LAUNCHER_VERSION),
        ("quickPlayPath", ""),
        ("quick_play_path", ""),
        ("quickPlaySingleplayer", ""),
        ("quickPlayMultiplayer", ""),
        ("quickPlayRealms", ""),
    ];
    expand_and_sanitize_arg(template, &subs)
}

fn default_minecraft_game_arguments() -> Vec<String> {
    [
        "--username",
        "${auth_player_name}",
        "--version",
        "${version_name}",
        "--gameDir",
        "${game_directory}",
        "--assetsDir",
        "${assets_root}",
        "--assetIndex",
        "${assets_index_name}",
        "--uuid",
        "${auth_uuid}",
        "--accessToken",
        "${auth_access_token}",
        "--userType",
        "${user_type}",
        "--versionType",
        "${version_type}",
        "--userProperties",
        "${user_properties}",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_owned();
    }
    let needs_quoting = arg
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'));
    if !needs_quoting {
        return arg.to_owned();
    }

    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    for ch in arg.chars() {
        if matches!(ch, '"' | '\\') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_all_known_placeholders_and_scrubs_unknowns_without_leaving_placeholder_markers() {
        let template = "--user ${auth_player_name} --ver ${version_name} --unknown ${unknown_var} --flag=${another_bad_var}";
        let subs = [("auth_player_name", "TestUser"), ("version_name", "26.3")];
        let expanded = expand_and_sanitize_arg(template, &subs);
        assert_eq!(expanded, "--user TestUser --ver 26.3 --unknown  --flag=");
        assert!(
            !expanded.contains("${"),
            "Result contains unexpanded placeholder marker!"
        );
    }

    #[test]
    fn scrubs_malformed_unclosed_placeholder() {
        let template = "-Dfoo=${unclosed_var";
        let expanded = expand_and_sanitize_arg(template, &[]);
        assert_eq!(expanded, "-Dfoo=");
        assert!(!expanded.contains("${"));
    }

    #[test]
    fn guarantee_no_placeholders_leak_into_built_command() {
        use mq_core::{ModLoaderKind, MqPaths};
        use mq_meta::{ArtifactDownload, AssetIndexPointer, JavaVersionSpec, ResolvedVersion};

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = MqPaths::with_root(temp.path());
        paths.ensure_dirs().expect("dirs");

        let mut instance = InstanceManifest::new("Test 26", "26.3", ModLoaderKind::Vanilla, None);
        instance.memory_min_mb = 1024;
        instance.memory_max_mb = 2048;
        instance.jvm_args = vec!["-Dcustom.unknown=${user_custom_prop}".to_string()];

        let version = ResolvedVersion {
            id: "26.3".to_string(),
            mc_version: "26.3".to_string(),
            loader: ModLoaderKind::Vanilla,
            loader_version: None,
            inherits_from: None,
            main_class: "net.minecraft.client.main.Main".to_string(),
            java_version: JavaVersionSpec {
                component: "java-runtime-epsilon".to_string(),
                major_version: 25,
            },
            asset_index: AssetIndexPointer {
                id: "34".to_string(),
                sha1: None,
                size: 100,
                total_size: 1000,
                url: String::new(),
            },
            client_jar: ArtifactDownload {
                path: "versions/26.3/26.3.jar".to_string(),
                url: String::new(),
                sha1: None,
                size: 100,
            },
            libraries: vec![],
            jvm_arguments: vec![
                "-Djava.library.path=${natives_directory}".to_string(),
                "-cp".to_string(),
                "${classpath}".to_string(),
                "-Dlegacy.unknown=${some_unknown_jvm_flag}".to_string(),
            ],
            game_arguments: vec![
                "--username".to_string(),
                "${auth_player_name}".to_string(),
                "--version".to_string(),
                "${version_name}".to_string(),
                "--gameDir".to_string(),
                "${game_directory}".to_string(),
                "--assetsDir".to_string(),
                "${assets_root}".to_string(),
                "--assetIndex".to_string(),
                "${assets_index_name}".to_string(),
                "--uuid".to_string(),
                "${auth_uuid}".to_string(),
                "--accessToken".to_string(),
                "${auth_access_token}".to_string(),
                "--userType".to_string(),
                "${user_type}".to_string(),
                "--versionType".to_string(),
                "${version_type}".to_string(),
                "--quickPlayPath".to_string(),
                "${quickPlayPath}".to_string(),
                "--unknownGameFlag".to_string(),
                "${unrecognized_game_placeholder}".to_string(),
            ],
            logging: Some(mq_meta::LoggingConfig {
                client: Some(mq_meta::LoggingClientConfig {
                    argument: "-Dlog4j.configurationFile=${path}".to_string(),
                    file: ArtifactDownload {
                        path: "log_configs/client-26.xml".to_string(),
                        url: String::new(),
                        sha1: None,
                        size: 50,
                    },
                    logging_type: "log4j2-xml".to_string(),
                }),
            }),
        };

        let account = mq_auth::create_offline_account("Steve").expect("account");

        let ctx = LaunchContext {
            paths,
            instance,
            version,
            account,
            java_bin: PathBuf::from("/usr/bin/java"),
            authlib_injector_jar: None,
            extra_secrets_to_redact: vec![],
        };

        let cmd = build_launch_command(&ctx).expect("build launch command");

        for (idx, arg) in cmd.args.iter().enumerate() {
            assert!(
                !arg.contains("${"),
                "Unexpanded placeholder '${{' leaked at argument index {idx}: {arg}"
            );
        }

        let full_cmd_line = cmd.to_shell_command_line();
        assert!(
            !full_cmd_line.contains("${"),
            "Unexpanded placeholder '${{' leaked into full command line: {full_cmd_line}"
        );

        // Verify logging argument was added
        assert!(cmd
            .args
            .iter()
            .any(|arg| arg.starts_with("-Dlog4j.configurationFile=")));
    }
}
