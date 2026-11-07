//! Self-update engine for MQ Launcher.
//! Implements GitHub Releases polling, SHA-256 verification, atomic `<exe>.new` substitution,
//! smoke-testing, rollback, and package-manager safety.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAssetInfo {
    pub version: String,
    pub release_notes: String,
    pub binary_url: String,
    pub binary_sha256: String,
    pub tarball_url: String,
    pub published_at: DateTime<Utc>,
    pub is_prerelease: bool,
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpdateAvailable(ReleaseAssetInfo),
    Downloading {
        progress: f32,
        bytes: u64,
        total: Option<u64>,
    },
    Verifying,
    ReadyToRestart {
        exe_path: PathBuf,
        old_path: PathBuf,
    },
    PackageManagerManaged {
        message: String,
        release_url: String,
    },
    Error(String),
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<DateTime<Utc>>,
    prerelease: bool,
    draft: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub struct UpdateManager {
    repo_owner: String,
    repo_name: String,
    client: reqwest::Client,
    api_base_url: Option<String>,
}

impl UpdateManager {
    pub fn new(repo_owner: &str, repo_name: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("mq-updater")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            repo_owner: repo_owner.to_string(),
            repo_name: repo_name.to_string(),
            client,
            api_base_url: None,
        }
    }

    pub fn with_api_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.api_base_url = Some(base_url.into());
        self
    }

    /// Check if the binary is located under `/usr/` or an unwriteable system directory.
    pub fn is_packaged_install() -> bool {
        if let Ok(exe_path) = std::env::current_exe() {
            let path_str = exe_path.to_string_lossy();
            if path_str.starts_with("/usr/") || path_str.starts_with("/var/") {
                return true;
            }
            if let Some(parent) = exe_path.parent() {
                if let Ok(metadata) = fs::metadata(parent) {
                    if metadata.permissions().readonly() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Query GitHub Releases for updates.
    pub async fn check_latest(
        &self,
        current_version: &str,
        include_prereleases: bool,
    ) -> Result<Option<ReleaseAssetInfo>> {
        let base = self
            .api_base_url
            .clone()
            .unwrap_or_else(|| "https://api.github.com".to_string());

        let url = if include_prereleases {
            format!(
                "{base}/repos/{}/{}/releases",
                self.repo_owner, self.repo_name
            )
        } else {
            format!(
                "{base}/repos/{}/{}/releases/latest",
                self.repo_owner, self.repo_name
            )
        };

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to query GitHub Releases API")?;

        if !response.status().is_success() {
            bail!("GitHub API returned status: {}", response.status());
        }

        let release: GitHubRelease = if include_prereleases {
            let releases: Vec<GitHubRelease> = response.json().await?;
            let valid = releases
                .into_iter()
                .find(|r| !r.draft && (include_prereleases || !r.prerelease));
            match valid {
                Some(r) => r,
                None => return Ok(None),
            }
        } else {
            response.json().await?
        };

        let remote_ver_str = release.tag_name.trim_start_matches('v');
        if !is_version_newer(current_version, remote_ver_str) {
            return Ok(None);
        }

        // Locate binary asset and checksum
        let binary_asset = release
            .assets
            .iter()
            .find(|a| a.name == "mq-linux-x86_64" || a.name == "mq");

        let checksum_asset = release
            .assets
            .iter()
            .find(|a| a.name == "SHA256SUMS" || a.name.ends_with(".sha256"));

        let tarball_asset = release
            .assets
            .iter()
            .find(|a| a.name.contains("portable") && a.name.ends_with(".tar.gz"));

        let binary_url = match binary_asset {
            Some(a) => a.browser_download_url.clone(),
            None => bail!(
                "Release {} does not contain linux-x86_64 binary asset",
                release.tag_name
            ),
        };

        let tarball_url = tarball_asset
            .map(|a| a.browser_download_url.clone())
            .unwrap_or_default();

        let mut binary_sha256 = String::new();
        if let Some(chk_asset) = checksum_asset {
            if let Ok(chk_resp) = self
                .client
                .get(&chk_asset.browser_download_url)
                .send()
                .await
            {
                if chk_resp.status().is_success() {
                    if let Ok(chk_text) = chk_resp.text().await {
                        for line in chk_text.lines() {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let hash = parts[0];
                                let filename = parts[1].trim_start_matches('*');
                                if filename == "mq-linux-x86_64" || filename == "mq" {
                                    binary_sha256 = hash.to_string();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Some(ReleaseAssetInfo {
            version: remote_ver_str.to_string(),
            release_notes: release.body.unwrap_or_default(),
            binary_url,
            binary_sha256,
            tarball_url,
            published_at: release.published_at.unwrap_or_else(Utc::now),
            is_prerelease: release.prerelease,
        }))
    }

    /// Download and atomically replace the current executable.
    pub async fn download_and_install<F>(
        &self,
        asset: &ReleaseAssetInfo,
        on_progress: F,
    ) -> Result<PathBuf>
    where
        F: Fn(u64, Option<u64>) + Send + 'static,
    {
        let current_exe =
            std::env::current_exe().context("Failed to determine current exe path")?;
        let exe_dir = current_exe
            .parent()
            .context("Failed to get exe directory")?;

        let new_exe = exe_dir.join(format!(
            "{}.new",
            current_exe.file_name().unwrap().to_string_lossy()
        ));
        let old_exe = exe_dir.join(format!(
            "{}.old",
            current_exe.file_name().unwrap().to_string_lossy()
        ));

        info!(
            "Downloading new binary from {} to {:?}",
            asset.binary_url, new_exe
        );

        // Download binary with progress
        let resp = self
            .client
            .get(&asset.binary_url)
            .send()
            .await
            .context("Failed to download update binary")?;

        if !resp.status().is_success() {
            bail!("Download failed with status: {}", resp.status());
        }

        let total_size = resp.content_length();
        let mut downloaded: u64 = 0;
        let mut hasher = Sha256::new();

        let mut file = tokio::fs::File::create(&new_exe)
            .await
            .context("Failed to create temporary update binary")?;

        let mut stream = resp.bytes_stream();

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.context("Error reading binary stream chunk")?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                .await
                .context("Failed to write to update file")?;
            on_progress(downloaded, total_size);
        }

        tokio::io::AsyncWriteExt::flush(&mut file).await?;
        drop(file);

        // Verify SHA-256 checksum if provided
        let computed_hash = hex::encode(hasher.finalize());
        if !asset.binary_sha256.is_empty() {
            if !computed_hash.eq_ignore_ascii_case(&asset.binary_sha256) {
                let _ = fs::remove_file(&new_exe);
                bail!(
                    "SHA-256 checksum mismatch! Expected: {}, Computed: {}",
                    asset.binary_sha256,
                    computed_hash
                );
            }
            info!("SHA-256 checksum verified successfully: {}", computed_hash);
        }

        // Set executable permissions (0755)
        let mut perms = fs::metadata(&new_exe)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&new_exe, perms)?;

        // Smoke-test `<exe>.new --version`
        info!("Running smoke-test on {:?}", new_exe);
        let smoke_output = Command::new(&new_exe)
            .arg("--version")
            .output()
            .context("Failed to execute smoke-test on downloaded binary")?;

        if !smoke_output.status.success() {
            let _ = fs::remove_file(&new_exe);
            bail!(
                "Smoke test failed on downloaded binary with code: {:?}",
                smoke_output.status.code()
            );
        }

        info!(
            "Smoke-test succeeded: {}",
            String::from_utf8_lossy(&smoke_output.stdout).trim()
        );

        // Atomic swap
        if old_exe.exists() {
            let _ = fs::remove_file(&old_exe);
        }

        if let Err(e) = fs::rename(&current_exe, &old_exe) {
            let _ = fs::remove_file(&new_exe);
            bail!("Failed to rename current binary to old: {}", e);
        }

        if let Err(e) = fs::rename(&new_exe, &current_exe) {
            // Rollback immediately
            let _ = fs::rename(&old_exe, &current_exe);
            let _ = fs::remove_file(&new_exe);
            bail!("Failed to install new binary: {}. Restored old binary.", e);
        }

        info!("Update successfully installed to {:?}", current_exe);
        Ok(current_exe)
    }

    /// Roll back to `<exe>.old` if available.
    pub fn rollback() -> Result<()> {
        let current_exe = std::env::current_exe().context("Failed to determine current exe")?;
        let old_exe = current_exe.with_extension("old");

        if !old_exe.exists() {
            bail!("No previous version backup found at {:?}", old_exe);
        }

        let backup_of_failed = current_exe.with_extension("failed");
        fs::rename(&current_exe, &backup_of_failed)?;
        if let Err(e) = fs::rename(&old_exe, &current_exe) {
            let _ = fs::rename(&backup_of_failed, &current_exe);
            bail!("Failed to restore old binary: {}", e);
        }
        let _ = fs::remove_file(&backup_of_failed);
        info!("Successfully rolled back to previous version");
        Ok(())
    }

    /// Restart launcher using `setsid` so existing child games continue running.
    pub fn restart_process(args: &[String]) -> Result<()> {
        let current_exe = std::env::current_exe().context("Failed to determine current exe")?;
        info!("Restarting process: {:?} {:?}", current_exe, args);

        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new(current_exe);
        cmd.args(args);
        cmd.arg("--after-update");

        unsafe {
            cmd.pre_exec(|| {
                // Detach from controlling terminal into new session
                rustix::process::setsid()
                    .map_err(|e| std::io::Error::from_raw_os_error(e.raw_os_error()))?;
                Ok(())
            });
        }

        cmd.spawn().context("Failed to spawn updated process")?;
        std::process::exit(0);
    }
}

/// Simple semver comparison helper (`a` newer than `b`?).
fn is_version_newer(current: &str, candidate: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split(&['.', '-'][..])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let cur = parse(current);
    let cand = parse(candidate);

    for (c, n) in cand.iter().zip(cur.iter()) {
        if c > n {
            return true;
        }
        if c < n {
            return false;
        }
    }
    cand.len() > cur.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_newer_logic() {
        assert!(is_version_newer("2.0.0", "2.1.0"));
        assert!(is_version_newer("2.0.99", "2.1.0"));
        assert!(!is_version_newer("2.1.0", "2.1.0"));
        assert!(!is_version_newer("2.1.0", "2.0.0"));
        assert!(is_version_newer("1.20.1", "1.21.1"));
    }
}
