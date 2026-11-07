//! `mq-launch`: JVM discovery (`/usr/lib/jvm/*`, `$JAVA_HOME`, `$PATH`) with fast `<home>/release`
//! parsing, automatic Java 21 / 17 / 8 selection, deterministic command builder (Lesson E8),
//! Ely.by `-javaagent` first-flag injection, and non-blocking redacted game process runner.

#![deny(clippy::await_holding_lock)]

pub mod command;
pub mod crash_analyzer;
pub mod java;
pub mod options;
pub mod prelaunch;
pub mod runner;

pub use command::{
    build_launch_command, BuiltLaunchCommand, LaunchContext, LAUNCHER_NAME, LAUNCHER_VERSION,
};
pub use crash_analyzer::{analyze_crash, upload_to_mclogs, CrashAnalysis};
pub use java::{
    discover_java_runtimes, discover_java_runtimes_from_sources, download_adoptium_java,
    parse_java_release_file, parse_java_version_output, parse_java_version_str, probe_java_binary,
    required_java_major_for_mc, select_java_runtime, JavaRuntime, SYSTEM_JVM_DIRS,
};
pub use options::{
    default_mc_locale_for_version, is_legacy_mc_version, patch_minecraft_options_lang,
    read_minecraft_options_lang, sync_minecraft_options_lang,
};
pub use prelaunch::ensure_prelaunch_assets;
pub use runner::{kill_process_tree, spawn_and_stream_game};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use mq_auth::{
        compute_offline_uuid, create_offline_account, AccountKind, AccountProfile,
        ELYBY_INJECTOR_CONFIG_URL,
    };
    use mq_core::{
        AccountId, Event, EventBus, GameLogLevel, ModLoaderKind, MqError, MqPaths, SecretString,
    };
    use mq_instances::InstanceManifest;
    use mq_meta::{
        ArtifactDownload, AssetIndexPointer, JavaVersionSpec, ResolvedLibrary, ResolvedVersion,
    };

    use super::*;

    fn sample_resolved_version() -> ResolvedVersion {
        ResolvedVersion {
            id: "fabric-loader-0.16.9-1.21.1".to_owned(),
            mc_version: "1.21.1".to_owned(),
            loader: ModLoaderKind::Fabric,
            loader_version: Some("0.16.9".to_owned()),
            inherits_from: Some("1.21.1".to_owned()),
            main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".to_owned(),
            java_version: JavaVersionSpec {
                component: "java-runtime-delta".to_owned(),
                major_version: 21,
            },
            asset_index: AssetIndexPointer {
                id: "17".to_owned(),
                sha1: None,
                size: 4096,
                total_size: 1_048_576,
                url: "https://piston-meta.mojang.com/v1/packages/hash/17.json".to_owned(),
            },
            client_jar: ArtifactDownload {
                path: "versions/1.21.1/1.21.1.jar".to_owned(),
                url: "https://piston-data.mojang.com/v1/objects/hash/client.jar".to_owned(),
                sha1: None,
                size: 25_000_000,
            },
            libraries: vec![
                ResolvedLibrary {
                    name: "net.fabricmc:fabric-loader:0.16.9".to_owned(),
                    artifact: Some(ArtifactDownload {
                        path: "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
                            .to_owned(),
                        url: "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
                            .to_owned(),
                        sha1: None,
                        size: 1024,
                    }),
                    native_linux: None,
                    extract_exclude: Vec::new(),
                    include_in_classpath: true,
                },
                ResolvedLibrary {
                    name: "org.lwjgl:lwjgl:3.3.3".to_owned(),
                    artifact: Some(ArtifactDownload {
                        path: "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar".to_owned(),
                        url: "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                            .to_owned(),
                        sha1: None,
                        size: 2048,
                    }),
                    native_linux: None,
                    extract_exclude: Vec::new(),
                    include_in_classpath: true,
                },
            ],
            jvm_arguments: vec![
                "-Djava.library.path=${natives_directory}".to_owned(),
                "-Dminecraft.launcher.brand=${launcher_name}".to_owned(),
                "-Dminecraft.launcher.version=${launcher_version}".to_owned(),
                "-cp".to_owned(),
                "${classpath}".to_owned(),
            ],
            game_arguments: vec![
                "--username".to_owned(),
                "${auth_player_name}".to_owned(),
                "--version".to_owned(),
                "${version_name}".to_owned(),
                "--gameDir".to_owned(),
                "${game_directory}".to_owned(),
                "--assetsDir".to_owned(),
                "${assets_root}".to_owned(),
                "--assetIndex".to_owned(),
                "${assets_index_name}".to_owned(),
                "--uuid".to_owned(),
                "${auth_uuid}".to_owned(),
                "--accessToken".to_owned(),
                "${auth_access_token}".to_owned(),
                "--userType".to_owned(),
                "${user_type}".to_owned(),
                "--versionType".to_owned(),
                "${version_type}".to_owned(),
                "--userProperties".to_owned(),
                "${user_properties}".to_owned(),
            ],
        }
    }

    #[test]
    fn parses_release_files_and_java_version_strings_for_java_8_17_and_21() {
        let rel_21 = r#"
IMPLEMENTOR="Arch Linux"
JAVA_VERSION="21.0.13"
JAVA_VERSION_DATE="2024-10-15"
OS_NAME="Linux"
OS_ARCH="amd64"
"#;
        let (maj21, full21, vendor21) = parse_java_release_file(rel_21).expect("parse Java 21");
        assert_eq!(maj21, 21);
        assert_eq!(full21, "21.0.13");
        assert_eq!(vendor21, "Arch Linux");

        let rel_17 = r#"
IMPLEMENTOR="Eclipse Adoptium"
JAVA_VERSION="17.0.21"
"#;
        let (maj17, full17, vendor17) = parse_java_release_file(rel_17).expect("parse Java 17");
        assert_eq!(maj17, 17);
        assert_eq!(full17, "17.0.21");
        assert_eq!(vendor17, "Eclipse Adoptium");

        let rel_8 = r#"
JAVA_VERSION="1.8.0_412"
IMPLEMENTOR="Oracle Corporation"
"#;
        let (maj8, full8, vendor8) = parse_java_release_file(rel_8).expect("parse Java 8");
        assert_eq!(maj8, 8);
        assert_eq!(full8, "1.8.0_412");
        assert_eq!(vendor8, "Oracle Corporation");

        let stderr_out = "openjdk version \"21.0.5\" 2024-10-15\nOpenJDK Runtime Environment Temurin-21.0.5+11 (build 21.0.5+11)\n";
        let (m_out, f_out, v_out) =
            parse_java_version_output(stderr_out).expect("parse java -version output");
        assert_eq!(m_out, 21);
        assert_eq!(f_out, "21.0.5");
        assert_eq!(v_out, "Eclipse Adoptium");
    }

    #[test]
    fn required_java_major_for_mc_maps_versions_accurately() {
        assert_eq!(required_java_major_for_mc("1.21.1", None), 21);
        assert_eq!(required_java_major_for_mc("1.21", None), 21);
        assert_eq!(required_java_major_for_mc("1.20.5", None), 21);
        assert_eq!(required_java_major_for_mc("1.20.4", None), 17);
        assert_eq!(required_java_major_for_mc("1.19.4", None), 17);
        assert_eq!(required_java_major_for_mc("1.18", None), 17);
        assert_eq!(required_java_major_for_mc("1.17.1", None), 8);
        assert_eq!(required_java_major_for_mc("1.16.5", None), 8);
        assert_eq!(required_java_major_for_mc("1.12.2", None), 8);
        assert_eq!(required_java_major_for_mc("1.16.5", Some(17)), 17);
    }

    #[test]
    fn synthetic_and_live_jvm_discovery_and_selection() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let jvm_root = tmp.path().join("jvm");

        for (dir_name, ver_str) in [
            ("java-8-openjdk", "1.8.0_412"),
            ("java-17-openjdk", "17.0.21"),
            ("java-21-openjdk", "21.0.13"),
        ] {
            let home = jvm_root.join(dir_name);
            let bin_dir = home.join("bin");
            fs::create_dir_all(&bin_dir).expect("create bin dir");
            let java_bin = bin_dir.join("java");
            fs::write(&java_bin, b"#!/bin/sh\nexit 0\n").expect("write dummy java");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&java_bin).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&java_bin, perms).unwrap();
            }
            fs::write(
                home.join("release"),
                format!("JAVA_VERSION=\"{ver_str}\"\nIMPLEMENTOR=\"Arch Linux\"\n"),
            )
            .expect("write release file");
        }

        let discovered = discover_java_runtimes_from_sources(&[&jvm_root], None, None);
        assert_eq!(discovered.len(), 3);
        assert_eq!(discovered[0].major_version, 8);
        assert_eq!(discovered[1].major_version, 17);
        assert_eq!(discovered[2].major_version, 21);

        let sel_21 = select_java_runtime(&discovered, 21, None).expect("select Java 21");
        assert_eq!(sel_21.major_version, 21);

        let sel_17 = select_java_runtime(&discovered, 17, None).expect("select Java 17");
        assert_eq!(sel_17.major_version, 17);

        let sel_8 = select_java_runtime(&discovered, 8, None).expect("select Java 8");
        assert_eq!(sel_8.major_version, 8);

        // When Java 8 is not installed, requesting 8 picks closest newer (Java 17).
        let only_17_and_21 = &discovered[1..];
        let fallback_for_8 =
            select_java_runtime(only_17_and_21, 8, None).expect("fallback Java 8 -> 17");
        assert_eq!(fallback_for_8.major_version, 17);

        // Empty runtime list returns MqError::JavaNotFound.
        let empty_err = select_java_runtime(&[], 21, None).expect_err("empty must fail");
        assert!(matches!(
            empty_err,
            MqError::JavaNotFound {
                required_major: 21,
                ..
            }
        ));

        // Also run live discovery on the current Arch Linux host to verify no panics and deterministic sorting.
        let host_runtimes = discover_java_runtimes();
        for pair in host_runtimes.windows(2) {
            assert!(
                (pair[0].major_version, &pair[0].path) <= (pair[1].major_version, &pair[1].path),
                "host runtimes must be sorted by (major_version, path)"
            );
        }
    }

    #[test]
    fn build_launch_command_offline_substitutions_and_determinism() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());
        let mut instance = InstanceManifest::new(
            "Offline Fabric",
            "1.21.1",
            ModLoaderKind::Fabric,
            Some("0.16.9".to_owned()),
        );
        instance.memory_min_mb = 1024;
        instance.memory_max_mb = 4096;
        instance.jvm_args = vec!["-XX:+UseG1GC".to_owned()];

        let account = create_offline_account("Mr_Quster").expect("create offline account");
        let expected_uuid_simple = compute_offline_uuid("Mr_Quster").simple().to_string();

        let ctx = LaunchContext {
            paths: paths.clone(),
            instance,
            version: sample_resolved_version(),
            account,
            java_bin: PathBuf::from("/usr/lib/jvm/java-21-openjdk/bin/java"),
            authlib_injector_jar: None,
            extra_secrets_to_redact: Vec::new(),
        };

        let cmd1 = build_launch_command(&ctx).expect("build command 1");
        let cmd2 = build_launch_command(&ctx).expect("build command 2");
        assert_eq!(
            cmd1, cmd2,
            "launch command construction must be 100% deterministic"
        );
        assert_eq!(
            cmd1.to_shell_command_line(),
            cmd2.to_shell_command_line(),
            "shell command line must be 100% deterministic"
        );

        assert_eq!(
            cmd1.program,
            PathBuf::from("/usr/lib/jvm/java-21-openjdk/bin/java")
        );
        assert_eq!(cmd1.args[0], "-Xms1024M");
        assert_eq!(cmd1.args[1], "-Xmx4096M");
        assert_eq!(cmd1.args[2], "-XX:+UseG1GC");

        // Verify Offline game argument substitutions.
        let args = &cmd1.args;
        let idx_user = args.iter().position(|a| a == "--username").unwrap();
        assert_eq!(args[idx_user + 1], "Mr_Quster");

        let idx_uuid = args.iter().position(|a| a == "--uuid").unwrap();
        assert_eq!(args[idx_uuid + 1], expected_uuid_simple);
        assert_eq!(args[idx_uuid + 1].len(), 32);

        let idx_tok = args.iter().position(|a| a == "--accessToken").unwrap();
        assert_eq!(args[idx_tok + 1], "0");

        let idx_type = args.iter().position(|a| a == "--userType").unwrap();
        assert_eq!(args[idx_type + 1], "legacy");

        let idx_props = args.iter().position(|a| a == "--userProperties").unwrap();
        assert_eq!(args[idx_props + 1], "{}");
    }

    #[test]
    fn build_launch_command_elyby_injects_javaagent_first_and_redacts_tokens() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());
        let instance = InstanceManifest::new(
            "Ely Fabric",
            "1.21.1",
            ModLoaderKind::Fabric,
            Some("0.16.9".to_owned()),
        );

        let ely_account = AccountProfile {
            id: AccountId::new_v4(),
            kind: AccountKind::ElyBy,
            username: "ElyGamer".to_owned(),
            uuid: compute_offline_uuid("ElyGamer"),
            access_token: SecretString::new("ely_super_secret_access_token_777"),
            refresh_token: None,
            client_token: Some(SecretString::new("ely_client_token_888")),
            expires_at: None,
            skin_url: None,
            avatar_head: None,
        };

        let injector_path = paths.authlib_injector_jar();
        let ctx = LaunchContext {
            paths: paths.clone(),
            instance: instance.clone(),
            version: sample_resolved_version(),
            account: ely_account.clone(),
            java_bin: PathBuf::from("/usr/lib/jvm/java-21-openjdk/bin/java"),
            authlib_injector_jar: Some(injector_path.clone()),
            extra_secrets_to_redact: Vec::new(),
        };

        let cmd = build_launch_command(&ctx).expect("build Ely.by launch command");

        // DoD #6: args[0] MUST be -javaagent:<full_path>=https://account.ely.by/api/authlib-injector
        // and args[1] MUST be -Dauthlibinjector.side=client, BEFORE -Xms and -Xmx!
        let expected_agent = format!(
            "-javaagent:{}={}",
            injector_path.display(),
            ELYBY_INJECTOR_CONFIG_URL
        );
        assert_eq!(cmd.args[0], expected_agent);
        assert!(cmd.args[0].starts_with("-javaagent:"));
        assert!(cmd.args[0].ends_with("=https://account.ely.by/api/authlib-injector"));
        assert_eq!(cmd.args[1], "-Dauthlibinjector.side=client");
        assert_eq!(cmd.args[2], "-Xms1024M");
        assert_eq!(cmd.args[3], "-Xmx4096M");

        let idx_type = cmd.args.iter().position(|a| a == "--userType").unwrap();
        assert_eq!(cmd.args[idx_type + 1], "mojang");

        let secrets = ctx.collect_secrets();
        let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
        let redacted_line = cmd.to_redacted_command_line(&secret_refs);
        assert!(!redacted_line.contains("ely_super_secret_access_token_777"));
        assert!(redacted_line.contains("--accessToken [REDACTED]"));

        // Now test with wrapper_cmd = Some("gamemoderun")
        let mut wrapped_instance = instance;
        wrapped_instance.wrapper_cmd = Some("gamemoderun".to_owned());
        let wrapped_ctx = LaunchContext {
            instance: wrapped_instance,
            ..ctx
        };
        let wrapped_cmd = build_launch_command(&wrapped_ctx).expect("build wrapped command");
        assert_eq!(wrapped_cmd.program, PathBuf::from("gamemoderun"));
        assert_eq!(wrapped_cmd.args[0], "/usr/lib/jvm/java-21-openjdk/bin/java");
        assert_eq!(wrapped_cmd.args[1], expected_agent);
        assert_eq!(wrapped_cmd.args[2], "-Dauthlibinjector.side=client");
    }

    #[tokio::test]
    async fn runner_streams_stdout_stderr_classifies_levels_and_redacts_secrets() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let bus = EventBus::with_capacity(64);

        let script = concat!(
            "echo '[12:00:01] [Render thread/INFO]: Starting game with token secret_jwt_token_99999'\n",
            "echo '[12:00:02] [Render thread/WARN]: Missing texture in pack'\n",
            "echo '[12:00:03] [Server thread/ERROR]: Crash with token secret_jwt_token_99999' >&2\n"
        );

        let cmd = BuiltLaunchCommand {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_owned(), script.to_owned()],
            working_dir: tmp.path().to_path_buf(),
            env: BTreeMap::new(),
        };

        let status = spawn_and_stream_game(
            &cmd,
            "fabric-1.21.1",
            vec!["secret_jwt_token_99999".to_owned()],
            Some(bus.clone()),
        )
        .await
        .expect("spawn and stream game");

        assert!(status.success());

        let events = bus.drain_all();
        assert_eq!(events.len(), 3);

        let mut logs = Vec::new();
        for ev in events {
            if let Event::GameLog(entry) = ev {
                assert_eq!(entry.instance, "fabric-1.21.1");
                assert!(
                    !entry.line.contains("secret_jwt_token_99999"),
                    "secret token must never appear in GameLogEntry: {}",
                    entry.line
                );
                logs.push(entry);
            }
        }

        assert_eq!(logs.len(), 3);
        assert!(logs.iter().any(|l| l.level == GameLogLevel::Info
            && l.line.contains("Starting game with token [REDACTED]")));
        assert!(logs
            .iter()
            .any(|l| l.level == GameLogLevel::Warn && l.line.contains("Missing texture in pack")));
        assert!(logs
            .iter()
            .any(|l| l.level == GameLogLevel::Error
                && l.line.contains("Crash with token [REDACTED]")));
    }

    #[test]
    fn select_java_runtime_supports_custom_override_path() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let custom_java = tmp.path().join("custom-java");
        fs::write(&custom_java, b"#!/bin/sh\nexit 0\n").expect("write custom java");

        let selected = select_java_runtime(&[], 21, Some(Path::new(&custom_java)))
            .expect("select custom java override");
        assert_eq!(
            selected.path,
            fs::canonicalize(&custom_java).unwrap_or(custom_java)
        );
        assert_eq!(selected.major_version, 21);
    }
}
