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
pub const LAUNCHER_VERSION: &str = "0.1.0";

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
    let game_dir = ctx.instance.game_dir(&ctx.paths);
    let natives_dir = ctx.instance.natives_dir(&ctx.paths);
    let libraries_dir = ctx.paths.libraries_dir();
    let assets_dir = ctx.paths.assets_dir();
    let client_jar_path = resolve_client_jar_path(&ctx.paths, &ctx.version)?;

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

    // 3. User custom JVM arguments from `instance.toml`.
    for custom_arg in &ctx.instance.jvm_args {
        let trimmed = custom_arg.trim();
        if !trimmed.is_empty() {
            jvm_flags.push(trimmed.to_owned());
        }
    }

    // 4. Expanded version JVM arguments.
    let natives_str = natives_dir.to_string_lossy();
    let libs_str = libraries_dir.to_string_lossy();
    let mut has_library_path = false;
    let mut has_classpath = false;

    for raw_jvm_arg in &ctx.version.jvm_arguments {
        let expanded = expand_jvm_placeholder(
            raw_jvm_arg,
            &natives_str,
            &libs_str,
            &classpath_str,
            &ctx.version.id,
        );
        if expanded.starts_with("-Djava.library.path=") {
            has_library_path = true;
        }
        if expanded == "-cp" || expanded == "-classpath" {
            has_classpath = true;
        }
        jvm_flags.push(expanded);
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

    // 6. Expanded game arguments.
    let game_dir_str = game_dir.to_string_lossy();
    let assets_dir_str = assets_dir.to_string_lossy();
    let uuid_simple = ctx.account.uuid_simple();
    let access_token = match ctx.account.kind {
        AccountKind::Offline => "0",
        AccountKind::ElyBy | AccountKind::Microsoft => ctx.account.access_token.expose_secret(),
    };
    let user_type = ctx.account.kind.user_type_arg();

    let default_game_args: Vec<String>;
    let source_game_args: &[String] = if ctx.version.game_arguments.is_empty() {
        default_game_args = default_minecraft_game_arguments();
        &default_game_args
    } else {
        &ctx.version.game_arguments
    };

    for raw_game_arg in source_game_args {
        let expanded = expand_game_placeholder(
            raw_game_arg,
            &ctx.account.username,
            &ctx.version.id,
            &game_dir_str,
            &assets_dir_str,
            &ctx.version.asset_index.id,
            &uuid_simple,
            access_token,
            user_type,
        );
        jvm_flags.push(expanded);
    }

    // 7. Apply optional wrapper command prefix (`gamemoderun`, `mangohud`, `prime-run`).
    let (program, args) = match ctx
        .instance
        .wrapper_cmd
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
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

    Ok(BuiltLaunchCommand {
        program,
        args,
        working_dir: instance_dir,
        env: BTreeMap::new(),
    })
}

fn resolve_client_jar_path(paths: &MqPaths, version: &ResolvedVersion) -> MqResult<PathBuf> {
    let raw_path = version.client_jar.path.trim();
    if raw_path.is_empty() {
        return Ok(paths
            .versions_dir()
            .join(&version.mc_version)
            .join(format!("{}.jar", version.mc_version)));
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

fn expand_jvm_placeholder(
    template: &str,
    natives_dir: &str,
    libraries_dir: &str,
    classpath: &str,
    version_name: &str,
) -> String {
    if !template.contains("${") {
        return template.to_owned();
    }
    template
        .replace("${natives_directory}", natives_dir)
        .replace("${launcher_name}", LAUNCHER_NAME)
        .replace("${launcher_version}", LAUNCHER_VERSION)
        .replace("${classpath}", classpath)
        .replace("${library_directory}", libraries_dir)
        .replace("${classpath_separator}", ":")
        .replace("${version_name}", version_name)
}

#[allow(clippy::too_many_arguments)]
fn expand_game_placeholder(
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
    if !template.contains("${") {
        return template.to_owned();
    }
    template
        .replace("${auth_player_name}", player_name)
        .replace("${version_name}", version_name)
        .replace("${game_directory}", game_dir)
        .replace("${assets_root}", assets_root)
        .replace("${game_assets}", assets_root)
        .replace("${assets_index_name}", assets_index_name)
        .replace("${auth_uuid}", auth_uuid)
        .replace("${auth_access_token}", auth_access_token)
        .replace("${auth_session}", auth_access_token)
        .replace("${user_type}", user_type)
        .replace("${version_type}", "release")
        .replace("${user_properties}", "{}")
        .replace("${clientid}", "0")
        .replace("${auth_xuid}", "0")
        .replace("${resolution_width}", "854")
        .replace("${resolution_height}", "480")
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
