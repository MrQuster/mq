//! `mq-instances`: Isolated Minecraft instance lifecycle (`instance.toml`), directory hierarchy
//! management (`mods/`, `config/`, `resourcepacks/`, `shaderpacks/`, `natives/`), deterministic
//! listing, hardlink cloning, and offline `resolved_version.json` caching.

#![deny(clippy::await_holding_lock)]

pub mod manager;
pub mod manifest;
pub mod package;
pub mod servers;
pub mod worlds;

pub use manager::InstanceManager;
pub use manifest::{
    InstanceManifest, DEFAULT_MEMORY_MAX_MB, DEFAULT_MEMORY_MIN_MB, INSTANCE_MANIFEST_FILE,
    INSTANCE_SUBDIRS, RESOLVED_VERSION_FILE,
};
pub use package::{
    export_instance_to_zip, import_instance_from_zip, list_screenshots, ScreenshotInfo,
};
pub use servers::{load_servers_dat, ping_server, save_servers_dat, ServerEntry, ServerPingResult};
pub use worlds::{
    backup_world, compute_dir_size, delete_world, export_world_to_zip, list_backups, list_worlds,
    restore_world, WorldBackupInfo, WorldInfo,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use mq_core::{ModLoaderKind, MqError, MqPaths};
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct DummyResolvedVersion {
        id: String,
        mc_version: String,
        main_class: String,
    }

    #[test]
    fn instance_manager_crud_and_case_insensitive_lookup() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());
        paths.ensure_dirs().expect("ensure base dirs");

        let mgr = InstanceManager::new(paths.clone());
        assert!(mgr.list_instances().expect("empty list").is_empty());

        let created = mgr
            .create_instance(
                "Fabulously Optimized",
                "1.21.1",
                ModLoaderKind::Fabric,
                Some("0.16.9".to_owned()),
            )
            .expect("create instance");

        assert_eq!(created.id.as_str(), "fabulously-optimized");
        assert_eq!(created.name, "Fabulously Optimized");
        assert_eq!(created.mc_version, "1.21.1");
        assert_eq!(created.loader, ModLoaderKind::Fabric);
        assert_eq!(created.loader_version.as_deref(), Some("0.16.9"));
        assert_eq!(created.memory_min_mb, 1024);
        assert_eq!(created.memory_max_mb, 4096);
        assert_eq!(created.playtime_secs, 0);
        assert!(created.last_played.is_none());

        // Verify all required directories and instance.toml exist on disk.
        assert!(created.instance_dir(&paths).is_dir());
        assert!(created.game_dir(&paths).is_dir());
        assert!(created.mods_dir(&paths).is_dir());
        assert!(created.config_dir(&paths).is_dir());
        assert!(created.resourcepacks_dir(&paths).is_dir());
        assert!(created.shaderpacks_dir(&paths).is_dir());
        assert!(created.natives_dir(&paths).is_dir());
        assert!(created.manifest_path(&paths).is_file());

        // Verify duplicate creation is rejected.
        let dup_err = mgr
            .create_instance(
                "fabulously-optimized",
                "1.21.1",
                ModLoaderKind::Vanilla,
                None,
            )
            .expect_err("duplicate slug must fail");
        assert!(matches!(dup_err, MqError::InstanceAlreadyExists(_)));

        // Lookup by exact slug, uppercase slug, exact display name, and lowercase display name.
        for query in [
            "fabulously-optimized",
            "FABULOUSLY-OPTIMIZED",
            "Fabulously Optimized",
            "fabulously optimized",
        ] {
            let loaded = mgr.load_instance(query).expect("load instance");
            assert_eq!(loaded.id, created.id);
            assert_eq!(loaded.name, "Fabulously Optimized");
        }

        // Update manifest fields and save atomically.
        let mut updated = created.clone();
        updated.memory_min_mb = 2048;
        updated.memory_max_mb = 8192;
        updated.java_path = Some(PathBuf::from("/usr/lib/jvm/java-21-openjdk/bin/java"));
        updated.jvm_args = vec!["-XX:+UseZGC".to_owned()];
        updated.wrapper_cmd = Some("gamemoderun".to_owned());
        updated.modpack_source = Some("modrinth:fabulously-optimized".to_owned());
        mgr.save_instance(&updated).expect("save updated manifest");

        let reloaded = mgr
            .load_instance("Fabulously Optimized")
            .expect("reload updated manifest");
        assert_eq!(reloaded.memory_min_mb, 2048);
        assert_eq!(reloaded.memory_max_mb, 8192);
        assert_eq!(
            reloaded.java_path.as_deref(),
            Some(std::path::Path::new(
                "/usr/lib/jvm/java-21-openjdk/bin/java"
            ))
        );
        assert_eq!(reloaded.jvm_args, vec!["-XX:+UseZGC".to_owned()]);
        assert_eq!(reloaded.wrapper_cmd.as_deref(), Some("gamemoderun"));
        assert_eq!(
            reloaded.modpack_source.as_deref(),
            Some("modrinth:fabulously-optimized")
        );

        // Touch playtime and last_played.
        let touched = mgr
            .touch_last_played("fabulously-optimized", 180)
            .expect("touch playtime");
        assert!(touched.last_played.is_some());
        assert_eq!(touched.playtime_secs, 180);

        // Delete by case-insensitive display name.
        mgr.delete_instance("fabulously optimized")
            .expect("delete instance");
        assert!(!created.instance_dir(&paths).exists());
        let missing = mgr
            .load_instance("fabulously-optimized")
            .expect_err("deleted instance must not load");
        assert!(matches!(missing, MqError::InstanceNotFound(_)));
    }

    #[test]
    fn list_instances_is_deterministically_sorted_by_name_then_id() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());
        let mgr = InstanceManager::new(paths);

        mgr.create_instance("Zulu Pack", "1.20.4", ModLoaderKind::Quilt, None)
            .expect("create Zulu");
        mgr.create_instance("Alpha Vanilla", "1.21.1", ModLoaderKind::Vanilla, None)
            .expect("create Alpha");
        mgr.create_instance("Bravo Fabric", "1.21.1", ModLoaderKind::Fabric, None)
            .expect("create Bravo");

        let listed = mgr.list_instances().expect("list instances");
        let names: Vec<&str> = listed.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha Vanilla", "Bravo Fabric", "Zulu Pack"]);
    }

    #[test]
    fn clone_instance_hardlinks_mods_configs_and_resolved_version_json() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());
        let mgr = InstanceManager::new(paths.clone());

        let src = mgr
            .create_instance(
                "Source Pack",
                "1.21.1",
                ModLoaderKind::Fabric,
                Some("0.16.9".to_owned()),
            )
            .expect("create source instance");

        // Populate mods/, config/, and resolved_version.json in source instance.
        let mod_file = src.mods_dir(&paths).join("sodium-fabric.jar");
        let cfg_file = src.config_dir(&paths).join("sodium-options.json");
        fs::write(&mod_file, b"fake-sodium-jar-bytes").expect("write mod");
        fs::write(&cfg_file, b"{\"quality\":\"high\"}").expect("write config");

        let dummy_resolved = DummyResolvedVersion {
            id: "fabric-loader-0.16.9-1.21.1".to_owned(),
            mc_version: "1.21.1".to_owned(),
            main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".to_owned(),
        };
        let saved_json_path = mgr
            .save_resolved_version("Source Pack", &dummy_resolved)
            .expect("save resolved_version.json");
        assert_eq!(saved_json_path, src.resolved_version_json_path(&paths));

        let _ = mgr
            .touch_last_played("source-pack", 600)
            .expect("touch source playtime");

        // Clone by display name.
        let cloned = mgr
            .clone_instance("Source Pack", "Cloned Pack")
            .expect("clone instance");
        assert_eq!(cloned.id.as_str(), "cloned-pack");
        assert_eq!(cloned.name, "Cloned Pack");
        assert_eq!(cloned.mc_version, "1.21.1");
        assert_eq!(cloned.loader, ModLoaderKind::Fabric);
        assert_eq!(cloned.loader_version.as_deref(), Some("0.16.9"));
        assert_eq!(cloned.playtime_secs, 0);
        assert!(cloned.last_played.is_none());

        let cloned_mod = cloned.mods_dir(&paths).join("sodium-fabric.jar");
        let cloned_cfg = cloned.config_dir(&paths).join("sodium-options.json");
        assert_eq!(
            fs::read(&cloned_mod).expect("read cloned mod"),
            b"fake-sodium-jar-bytes"
        );
        assert_eq!(
            fs::read(&cloned_cfg).expect("read cloned config"),
            b"{\"quality\":\"high\"}"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let src_ino = fs::metadata(&mod_file).expect("stat src mod").ino();
            let dst_ino = fs::metadata(&cloned_mod).expect("stat dst mod").ino();
            assert_eq!(src_ino, dst_ino, "cloned mod should share hardlink inode");
        }

        // Verify cached resolved_version.json was also cloned and loads cleanly offline.
        let loaded_resolved: DummyResolvedVersion = mgr
            .load_resolved_version("cloned-pack")
            .expect("load cloned resolved_version.json");
        assert_eq!(loaded_resolved, dummy_resolved);
    }

    #[test]
    fn world_backup_restore_and_servers_dat() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let inst_dir = tmp.path().join("test_inst");
        fs::create_dir_all(inst_dir.join("saves").join("SurvivalWorld")).unwrap();
        fs::write(
            inst_dir
                .join("saves")
                .join("SurvivalWorld")
                .join("level.dat"),
            b"fake_level",
        )
        .unwrap();

        let worlds = list_worlds(&inst_dir).unwrap();
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].folder_name, "SurvivalWorld");

        let backups_root = tmp.path().join("backups");
        let backup_path =
            backup_world(&inst_dir, &backups_root, "test_inst", "SurvivalWorld").unwrap();
        assert!(backup_path.is_file());

        let backups = list_backups(&backups_root, "test_inst").unwrap();
        assert_eq!(backups.len(), 1);

        // Delete world and restore
        delete_world(&inst_dir, "SurvivalWorld").unwrap();
        assert_eq!(list_worlds(&inst_dir).unwrap().len(), 0);

        let restored = restore_world(
            &backups_root,
            "test_inst",
            &backups[0].file_name,
            &inst_dir,
            Some("RestoredWorld"),
        )
        .unwrap();
        assert!(restored.is_dir());
        assert_eq!(list_worlds(&inst_dir).unwrap().len(), 1);

        // Test servers.dat
        let servers = vec![ServerEntry {
            name: "Hypixel".to_string(),
            ip: "mc.hypixel.net".to_string(),
            icon: None,
            accept_textures: 1,
        }];
        save_servers_dat(&inst_dir, &servers).unwrap();
        let loaded_servers = load_servers_dat(&inst_dir).unwrap();
        assert_eq!(loaded_servers.len(), 1);
        assert_eq!(loaded_servers[0].name, "Hypixel");
        assert_eq!(loaded_servers[0].ip, "mc.hypixel.net");
    }
}
