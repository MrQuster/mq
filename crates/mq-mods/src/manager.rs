#![deny(clippy::await_holding_lock)]
//! Local content file manager: 1-click enable/disable, deletion, batch installation
//! of mods, resourcepacks, and shaderpacks into `<instance>/{mods,resourcepacks,shaderpacks}/`
//! with HTTPS host allowlist enforcement, dual SHA-1/SHA-512 verification, and
//! 1-click updates with `.bak` backup and automatic rollback on failure.

use std::fs;
use std::path::{Path, PathBuf};

use mq_core::{resolve_safe_path, MqError, MqPaths, MqResult, Sha1Digest, Sha512Digest};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::EpochGuard;

use crate::curseforge::is_allowed_curseforge_download_url;
use crate::lockfile::{ModLockEntry, ModLockFile};
use crate::models::{LockSource, ProjectKind, ResolvedModDownload};
use crate::modrinth::is_allowed_modrinth_download_url;
use crate::zip_safe::{
    link_or_copy_cas_blob, stream_hash_file_sha1_sha512, verify_file_sha1_and_sha512,
};

/// Returns the target directory inside `instance_dir` for the given [`ProjectKind`]:
/// - [`ProjectKind::Mod`] -> `<instance_dir>/mods`
/// - [`ProjectKind::ResourcePack`] -> `<instance_dir>/resourcepacks`
/// - [`ProjectKind::Shader`] -> `<instance_dir>/shaderpacks`
/// - [`ProjectKind::Modpack`] -> `<instance_dir>`
pub fn content_target_dir(instance_dir: &Path, kind: ProjectKind) -> PathBuf {
    let sub = kind.target_subfolder();
    if sub.is_empty() {
        instance_dir.to_path_buf()
    } else {
        instance_dir.join(sub)
    }
}

/// Checks whether `raw_url` points to an allowed Modrinth or CurseForge HTTPS download host
/// (or `http://127.0.0.1` / `http://localhost` for local mock servers in integration tests).
pub fn is_allowed_content_download_url(raw_url: &str) -> bool {
    is_allowed_modrinth_download_url(raw_url) || is_allowed_curseforge_download_url(raw_url)
}

/// Validates that `raw_url` is non-empty and points to an allowed HTTPS download host.
pub fn validate_content_download_url(raw_url: &str) -> MqResult<()> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(MqError::Network(
            "Скачивание файла заблокировано автором (downloadUrl пуст); требуется ручная загрузка"
                .to_owned(),
        ));
    }
    if !is_allowed_content_download_url(trimmed) {
        return Err(MqError::Network(format!(
            "Запрещённый хост или небезопасная схема URL для скачивания контента: '{trimmed}'"
        )));
    }
    Ok(())
}

/// Toggles a mod or content pack between active (`.jar` / `.zip`) and disabled (`.jar.disabled` / `.zip.disabled`) by renaming it.
pub fn toggle_mod(mods_dir: &Path, file_name: &str) -> MqResult<PathBuf> {
    let safe_src = resolve_safe_path(mods_dir, file_name)?;
    if !safe_src.exists() {
        return Err(MqError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("mod file not found: {file_name}"),
        )));
    }

    let target_name = if file_name.ends_with(".jar.disabled")
        || file_name.ends_with(".zip.disabled")
    {
        file_name.trim_end_matches(".disabled").to_string()
    } else if file_name.ends_with(".jar") || file_name.ends_with(".zip") {
        format!("{file_name}.disabled")
    } else {
        return Err(MqError::Internal(format!(
            "file '{file_name}' is not a recognized content file (must end in .jar/.zip or .disabled)"
        )));
    };

    let safe_dst = resolve_safe_path(mods_dir, &target_name)?;
    fs::rename(&safe_src, &safe_dst)?;
    Ok(safe_dst)
}

/// Deletes a mod or content pack file safely from the target directory.
pub fn delete_mod(mods_dir: &Path, file_name: &str) -> MqResult<()> {
    let safe_path = resolve_safe_path(mods_dir, file_name)?;
    if safe_path.exists() {
        fs::remove_file(&safe_path)?;
    }
    Ok(())
}

/// Downloads and installs resolved mods and dependencies into `mods_dir` via CAS hardlinks,
/// validating allowed download hosts and SHA-1/SHA-512 hashes.
pub async fn install_resolved_mods(
    downloads: &[ResolvedModDownload],
    mods_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    epoch_guard: &EpochGuard,
) -> MqResult<Vec<PathBuf>> {
    install_resolved_To_dir(
        downloads,
        mods_dir,
        ProjectKind::Mod,
        LockSource::Modrinth,
        paths,
        net,
        epoch_guard,
    )
    .await
}

/// Downloads and installs resolved content (`Mod`, `ResourcePack`, `Shader`) into the appropriate
/// subfolder of `instance_dir` (`mods/`, `resourcepacks/`, `shaderpacks/`), verifying allowed HTTPS
/// hosts and SHA-1/SHA-512 digests and recording entries in `.mq-lock.json`.
pub async fn install_resolved_content(
    downloads: &[ResolvedModDownload],
    instance_dir: &Path,
    kind: ProjectKind,
    source: LockSource,
    paths: &MqPaths,
    net: &NetEngine,
    epoch_guard: &EpochGuard,
) -> MqResult<Vec<PathBuf>> {
    let target_dir = content_target_dir(instance_dir, kind);
    let installed = install_resolved_To_dir(
        downloads,
        &target_dir,
        kind,
        source,
        paths,
        net,
        epoch_guard,
    )
    .await?;

    // Mirror lockfile at instance root as well
    let target_lock = ModLockFile::load_or_default(&target_dir);
    let mut inst_lock = ModLockFile::load_for_instance(instance_dir);
    for (k, v) in target_lock.entries {
        inst_lock.record_install(k, v);
    }
    let _ = inst_lock.save_for_instance(instance_dir);

    Ok(installed)
}

#[allow(non_snake_case)]
async fn install_resolved_To_dir(
    downloads: &[ResolvedModDownload],
    target_dir: &Path,
    kind: ProjectKind,
    source: LockSource,
    paths: &MqPaths,
    net: &NetEngine,
    epoch_guard: &EpochGuard,
) -> MqResult<Vec<PathBuf>> {
    fs::create_dir_all(target_dir)?;

    for d in downloads {
        validate_content_download_url(&d.url)?;
    }

    let mut download_specs = Vec::with_capacity(downloads.len());
    let mut destinations = Vec::with_capacity(downloads.len());

    for d in downloads {
        let dest_file = resolve_safe_path(target_dir, &d.filename)?;

        let cas_blob = if let Some(sha1) = d.sha1 {
            paths.cas_sha1_path(&sha1.to_hex())
        } else {
            dest_file.clone()
        };

        download_specs.push(DownloadSpec {
            id: format!("{}_{}_{}", kind, d.project_id, d.version_id),
            urls: vec![d.url.clone()],
            dest_path: cas_blob.clone(),
            expected_sha1: d.sha1,
            expected_sha512: d.sha512,
            expected_size: if d.size > 0 { Some(d.size) } else { None },
        });

        destinations.push((d, cas_blob, dest_file));
    }

    // Execute downloads in parallel
    net.download_batch(download_specs, "install_mods", epoch_guard)
        .await?;

    let root_project_id = downloads
        .iter()
        .find(|d| !d.is_dependency)
        .map(|d| d.project_id.clone());

    let mut lock = ModLockFile::load_or_default(target_dir);
    let mut installed_paths = Vec::with_capacity(downloads.len());

    for (d, cas_blob, dest_file) in destinations {
        if cas_blob != dest_file {
            link_or_copy_cas_blob(&cas_blob, &dest_file)?;
        }

        if let Err(err) = verify_file_sha1_and_sha512(&dest_file, &d.filename, d.sha1, d.sha512) {
            let _ = fs::remove_file(&dest_file);
            return Err(err);
        }

        let (actual_sha1, actual_sha512, _) = stream_hash_file_sha1_sha512(&dest_file)?;
        let mut required_by = Vec::new();
        if d.is_dependency {
            if let Some(ref root_id) = root_project_id {
                if root_id != &d.project_id {
                    required_by.push(root_id.clone());
                }
            }
        }

        lock.record_install(
            d.project_id.clone(),
            ModLockEntry {
                source,
                project_id: d.project_id.clone(),
                version_id: d.version_id.clone(),
                file_id: Some(d.version_id.clone()),
                filename: d.filename.clone(),
                sha1: Some(actual_sha1.to_hex()),
                sha512: actual_sha512.to_hex(),
                installed_as_dependency: d.is_dependency,
                required_by,
                kind,
            },
        );

        installed_paths.push(dest_file);
    }

    let _ = lock.save(target_dir);

    Ok(installed_paths)
}

/// Atomically updates an installed content file in `target_dir` using a `.bak` backup file
/// and automatic rollback if `writer` or SHA-1/SHA-512 verification fails.
pub fn update_file_with_backup_rollback<F>(
    target_dir: &Path,
    old_file_name: &str,
    new_file_name: &str,
    expected_sha1: Option<&Sha1Digest>,
    expected_sha512: Option<&Sha512Digest>,
    writer: F,
) -> MqResult<PathBuf>
where
    F: FnOnce(&Path) -> MqResult<()>,
{
    let old_path = resolve_safe_path(target_dir, old_file_name)?;
    if !old_path.exists() {
        return Err(MqError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Исходный файл для обновления не найден: {old_file_name}"),
        )));
    }

    let new_path = resolve_safe_path(target_dir, new_file_name)?;
    let bak_name = format!("{old_file_name}.bak");
    let bak_path = resolve_safe_path(target_dir, &bak_name)?;

    if bak_path.exists() {
        let _ = fs::remove_file(&bak_path);
    }

    // 1. Move old file to `<old_file_name>.bak`
    fs::rename(&old_path, &bak_path)?;

    // 2. Write new file and verify hashes
    let write_and_verify = (|| -> MqResult<()> {
        writer(&new_path)?;
        if !new_path.exists() {
            return Err(MqError::Internal(format!(
                "Новый файл '{}' не был создан при обновлении",
                new_file_name
            )));
        }
        verify_file_sha1_and_sha512(
            &new_path,
            new_file_name,
            expected_sha1.copied(),
            expected_sha512.copied(),
        )?;
        Ok(())
    })();

    match write_and_verify {
        Ok(()) => {
            // 3a. Success: remove `.bak`
            let _ = fs::remove_file(&bak_path);
            Ok(new_path)
        }
        Err(err) => {
            // 3b. Rollback: remove broken new file and restore `.bak` -> `old_path`
            if new_path.exists() {
                let _ = fs::remove_file(&new_path);
            }
            if bak_path.exists() {
                let _ = fs::rename(&bak_path, &old_path);
            }
            Err(err)
        }
    }
}

/// Performs a 1-click update of an installed content file (`old_file_name`) to `new_download`
/// inside `target_dir`, creating `<old_file_name>.bak` before downloading and rolling back
/// automatically if host validation, download, or SHA-1/SHA-512 verification fails.
#[allow(clippy::too_many_arguments)]
pub async fn update_installed_content(
    target_dir: &Path,
    old_file_name: &str,
    new_download: &ResolvedModDownload,
    kind: ProjectKind,
    source: LockSource,
    paths: &MqPaths,
    net: &NetEngine,
    epoch_guard: &EpochGuard,
) -> MqResult<PathBuf> {
    let old_path = resolve_safe_path(target_dir, old_file_name)?;
    if !old_path.exists() {
        return Err(MqError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Исходный файл для обновления не найден: {old_file_name}"),
        )));
    }

    let new_path = resolve_safe_path(target_dir, &new_download.filename)?;
    let bak_name = format!("{old_file_name}.bak");
    let bak_path = resolve_safe_path(target_dir, &bak_name)?;

    if bak_path.exists() {
        let _ = fs::remove_file(&bak_path);
    }

    // Create `.bak` backup before attempting update
    fs::rename(&old_path, &bak_path)?;

    let update_res: MqResult<()> = async {
        validate_content_download_url(&new_download.url)?;

        let cas_blob = if let Some(sha1) = new_download.sha1 {
            paths.cas_sha1_path(&sha1.to_hex())
        } else {
            new_path.clone()
        };

        let spec = DownloadSpec {
            id: format!(
                "update_{}_{}_{}",
                kind, new_download.project_id, new_download.version_id
            ),
            urls: vec![new_download.url.clone()],
            dest_path: cas_blob.clone(),
            expected_sha1: new_download.sha1,
            expected_sha512: new_download.sha512,
            expected_size: if new_download.size > 0 {
                Some(new_download.size)
            } else {
                None
            },
        };

        net.download_batch(vec![spec], "update_content", epoch_guard)
            .await?;

        if cas_blob != new_path {
            link_or_copy_cas_blob(&cas_blob, &new_path)?;
        }

        verify_file_sha1_and_sha512(
            &new_path,
            &new_download.filename,
            new_download.sha1,
            new_download.sha512,
        )?;

        Ok(())
    }
    .await;

    match update_res {
        Ok(()) => {
            let _ = fs::remove_file(&bak_path);

            if let Ok((actual_sha1, actual_sha512, _)) = stream_hash_file_sha1_sha512(&new_path) {
                let mut lock = ModLockFile::load_or_default(target_dir);
                let existing_dep = lock
                    .find_by_filename(old_file_name)
                    .map(|(_, e)| (e.installed_as_dependency, e.required_by.clone()))
                    .unwrap_or((false, Vec::new()));

                lock.record_install(
                    new_download.project_id.clone(),
                    ModLockEntry {
                        source,
                        project_id: new_download.project_id.clone(),
                        version_id: new_download.version_id.clone(),
                        file_id: Some(new_download.version_id.clone()),
                        filename: new_download.filename.clone(),
                        sha1: Some(actual_sha1.to_hex()),
                        sha512: actual_sha512.to_hex(),
                        installed_as_dependency: existing_dep.0,
                        required_by: existing_dep.1,
                        kind,
                    },
                );
                let _ = lock.save(target_dir);
            }

            Ok(new_path)
        }
        Err(err) => {
            if new_path.exists() {
                let _ = fs::remove_file(&new_path);
            }
            if bak_path.exists() {
                let _ = fs::rename(&bak_path, &old_path);
            }
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::{Digest as _, Sha1};
    use tempfile::tempdir;

    #[test]
    fn toggles_and_deletes_mods_cleanly() {
        let dir = tempdir().unwrap();
        let jar = dir.path().join("testmod.jar");
        fs::write(&jar, b"content").unwrap();

        // 1. Disable
        let disabled = toggle_mod(dir.path(), "testmod.jar").unwrap();
        assert_eq!(disabled.file_name().unwrap(), "testmod.jar.disabled");
        assert!(disabled.exists());
        assert!(!jar.exists());

        // 2. Enable
        let enabled = toggle_mod(dir.path(), "testmod.jar.disabled").unwrap();
        assert_eq!(enabled.file_name().unwrap(), "testmod.jar");
        assert!(enabled.exists());

        // 3. Delete
        delete_mod(dir.path(), "testmod.jar").unwrap();
        assert!(!enabled.exists());
    }

    #[test]
    fn content_target_dir_and_allowed_hosts() {
        let inst = Path::new("/tmp/inst");
        assert_eq!(
            content_target_dir(inst, ProjectKind::Mod),
            PathBuf::from("/tmp/inst/mods")
        );
        assert_eq!(
            content_target_dir(inst, ProjectKind::ResourcePack),
            PathBuf::from("/tmp/inst/resourcepacks")
        );
        assert_eq!(
            content_target_dir(inst, ProjectKind::Shader),
            PathBuf::from("/tmp/inst/shaderpacks")
        );

        assert!(validate_content_download_url(
            "https://cdn.modrinth.com/data/AANobbMI/versions/1/sodium.jar"
        )
        .is_ok());
        assert!(validate_content_download_url(
            "https://edge.forgecdn.net/files/4572/915/sodium.jar"
        )
        .is_ok());
        assert!(validate_content_download_url("").is_err());
        assert!(validate_content_download_url("http://cdn.modrinth.com/sodium.jar").is_err());
        assert!(validate_content_download_url("https://evil.example.com/sodium.jar").is_err());
    }

    #[test]
    fn update_file_with_backup_rolls_back_on_bad_hash_and_cleans_bak_on_success() {
        let dir = tempdir().unwrap();
        let old_file = dir.path().join("sodium-0.5.0.jar");
        fs::write(&old_file, b"old-working-bytes").unwrap();

        let new_bytes = b"new-updated-bytes";
        let mut h = Sha1::new();
        h.update(new_bytes);
        let good_sha1 = Sha1Digest::from_bytes(h.finalize().into());
        let wrong_sha1 = Sha1Digest::from_bytes([0xAB; 20]);

        // 1. Failed update (hash mismatch) must restore old_file from .bak and delete broken new file.
        let err = update_file_with_backup_rollback(
            dir.path(),
            "sodium-0.5.0.jar",
            "sodium-0.6.0.jar",
            Some(&wrong_sha1),
            None,
            |new_path| {
                fs::write(new_path, new_bytes)?;
                Ok(())
            },
        )
        .expect_err("hash mismatch must fail and rollback");
        assert!(err.to_string().contains("SHA-1"));
        assert!(
            old_file.exists(),
            "old file must be restored after rollback"
        );
        assert_eq!(fs::read(&old_file).unwrap(), b"old-working-bytes");
        assert!(!dir.path().join("sodium-0.6.0.jar").exists());
        assert!(!dir.path().join("sodium-0.5.0.jar.bak").exists());

        // 2. Successful update must replace old_file with new_file and remove .bak.
        let updated_path = update_file_with_backup_rollback(
            dir.path(),
            "sodium-0.5.0.jar",
            "sodium-0.6.0.jar",
            Some(&good_sha1),
            None,
            |new_path| {
                fs::write(new_path, new_bytes)?;
                Ok(())
            },
        )
        .expect("valid update succeeds");
        assert_eq!(updated_path, dir.path().join("sodium-0.6.0.jar"));
        assert!(updated_path.exists());
        assert_eq!(fs::read(&updated_path).unwrap(), new_bytes);
        assert!(!old_file.exists());
        assert!(!dir.path().join("sodium-0.5.0.jar.bak").exists());
    }
}
