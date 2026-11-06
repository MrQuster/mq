#![deny(clippy::await_holding_lock)]
//! Local mod file manager: 1-click enable/disable, deletion, and batch installation
//! of resolved mods into `<instance>/mods/` via CAS hardlinks.

use std::fs;
use std::path::{Path, PathBuf};

use mq_core::{resolve_safe_path, MqError, MqPaths, MqResult};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::EpochGuard;

use crate::models::ResolvedModDownload;
use crate::zip_safe::link_or_copy_cas_blob;

/// Toggles a mod between active (`.jar`) and disabled (`.jar.disabled`) by renaming it.
pub fn toggle_mod(mods_dir: &Path, file_name: &str) -> MqResult<PathBuf> {
    let safe_src = resolve_safe_path(mods_dir, file_name)?;
    if !safe_src.exists() {
        return Err(MqError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("mod file not found: {file_name}"),
        )));
    }

    let target_name = if file_name.ends_with(".jar.disabled") {
        file_name.trim_end_matches(".disabled").to_string()
    } else if file_name.ends_with(".jar") {
        format!("{file_name}.disabled")
    } else {
        return Err(MqError::Internal(format!(
            "file '{file_name}' is not a recognized mod file (must end in .jar or .jar.disabled)"
        )));
    };

    let safe_dst = resolve_safe_path(mods_dir, &target_name)?;
    fs::rename(&safe_src, &safe_dst)?;
    Ok(safe_dst)
}

/// Deletes a mod file safely from the mods directory.
pub fn delete_mod(mods_dir: &Path, file_name: &str) -> MqResult<()> {
    let safe_path = resolve_safe_path(mods_dir, file_name)?;
    if safe_path.exists() {
        fs::remove_file(&safe_path)?;
    }
    Ok(())
}

/// Downloads and installs resolved mods and dependencies into `mods_dir` via CAS hardlinks.
pub async fn install_resolved_mods(
    downloads: &[ResolvedModDownload],
    mods_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    epoch_guard: &EpochGuard,
) -> MqResult<Vec<PathBuf>> {
    fs::create_dir_all(mods_dir)?;

    let mut download_specs = Vec::with_capacity(downloads.len());
    let mut link_pairs = Vec::with_capacity(downloads.len());

    for d in downloads {
        let dest_jar = resolve_safe_path(mods_dir, &d.filename)?;

        let cas_blob = if let Some(sha1) = d.sha1 {
            paths.cas_sha1_path(&sha1.to_hex())
        } else {
            dest_jar.clone()
        };

        download_specs.push(DownloadSpec {
            id: format!("mod_{}_{}", d.project_id, d.version_id),
            urls: vec![d.url.clone()],
            dest_path: cas_blob.clone(),
            expected_sha1: d.sha1,
            expected_sha512: d.sha512,
            expected_size: if d.size > 0 { Some(d.size) } else { None },
        });

        if cas_blob != dest_jar {
            link_pairs.push((cas_blob, dest_jar));
        }
    }

    // Execute downloads in parallel
    net.download_batch(download_specs, "install_mods", epoch_guard)
        .await?;

    let mut installed_paths = Vec::with_capacity(downloads.len());

    // Materialize hardlinks
    for (cas_blob, dest_jar) in link_pairs {
        link_or_copy_cas_blob(&cas_blob, &dest_jar)?;
        installed_paths.push(dest_jar);
    }

    Ok(installed_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
