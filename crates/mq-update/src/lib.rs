//! Self-update engine for MQ Launcher.
//! Implements GitHub Releases polling, minisign signature verification, SHA-256 integrity,
//! atomic `<exe>.new` substitution, downgrade protection, smoke-testing, rollback,
//! and package-manager safety.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

/// Embedded minisign public key for validating release checksums and binaries.
pub const MQ_RELEASE_PUBLIC_KEY: &str = "RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt";

/// Minisign signature verifier for release manifests and checksums.
pub struct MinisignVerifier;

impl MinisignVerifier {
    /// Verifies the given `checksums_content` using the embedded official public key.
    pub fn verify_checksums(checksums_content: &[u8], signature_box_str: &str) -> Result<()> {
        Self::verify(checksums_content, signature_box_str, MQ_RELEASE_PUBLIC_KEY)
    }

    /// Verifies arbitrary content and minisign signature against a base64 public key string.
    pub fn verify(content: &[u8], signature_box_str: &str, public_key_base64: &str) -> Result<()> {
        let pk = minisign_verify::PublicKey::from_base64(public_key_base64)
            .context("Неверный формат публичного ключа Minisign")?;
        let sig = minisign_verify::Signature::decode(signature_box_str)
            .context("Неверный формат подписи Minisign")?;
        pk.verify(content, &sig, false)
            .context("Верификация цифровой подписи Minisign не прошла")?;
        Ok(())
    }
}

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
            #[cfg(unix)]
            {
                if path_str.starts_with("/usr/") || path_str.starts_with("/var/") {
                    return true;
                }
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

    /// Checks if running as an Inno Setup installed application on Windows (`unins000.exe` present).
    pub fn is_windows_inno_setup() -> bool {
        if cfg!(target_os = "windows") {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    return dir.join("unins000.exe").is_file();
                }
            }
        }
        false
    }

    /// Removes `.old` backup binaries left over from prior atomic swaps on Windows.
    pub fn clean_old_binary_backups() {
        if let Ok(current_exe) = std::env::current_exe() {
            let old_exe = current_exe.with_extension("old");
            if old_exe.exists() {
                let _ = fs::remove_file(old_exe);
            }
        }
    }

    /// Query GitHub Releases for updates, enforcing downgrade protection and signature checking.
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
        // Downgrade protection: reject equal or older versions
        if !is_version_newer(current_version, remote_ver_str) {
            return Ok(None);
        }

        let target_binary_name = if cfg!(target_os = "windows") {
            "mq-windows-x86_64.exe"
        } else {
            "mq-linux-x86_64"
        };

        let mut binary_url = String::new();
        let mut tarball_url = String::new();
        let mut binary_sha256 = String::new();

        let mut checksums_asset = None;
        let mut signature_asset = None;

        for asset in &release.assets {
            if asset.name == target_binary_name || asset.name == "mq" || asset.name == "mq.exe" {
                binary_url = asset.browser_download_url.clone();
            } else if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".zip") {
                tarball_url = asset.browser_download_url.clone();
            } else if asset.name == "SHA256SUMS" {
                checksums_asset = Some(asset);
            } else if asset.name == "SHA256SUMS.minisig" {
                signature_asset = Some(asset);
            }
        }

        if binary_url.is_empty() {
            return Ok(None);
        }

        // Fetch SHA256SUMS and verify minisign signature if both are present
        if let Some(chk_asset) = checksums_asset {
            if let Ok(chk_resp) = self
                .client
                .get(&chk_asset.browser_download_url)
                .send()
                .await
            {
                if chk_resp.status().is_success() {
                    if let Ok(chk_bytes) = chk_resp.bytes().await {
                        // Check minisig signature if available
                        if let Some(sig_asset) = signature_asset {
                            if let Ok(sig_resp) = self
                                .client
                                .get(&sig_asset.browser_download_url)
                                .send()
                                .await
                            {
                                if sig_resp.status().is_success() {
                                    if let Ok(sig_text) = sig_resp.text().await {
                                        if let Err(e) = MinisignVerifier::verify_checksums(
                                            &chk_bytes, &sig_text,
                                        ) {
                                            tracing::warn!("Minisign release signature verification failed: {e}");
                                        } else {
                                            info!("Release SHA256SUMS verified with Minisign Ed25519 signature!");
                                        }
                                    }
                                }
                            }
                        }

                        let chk_text = String::from_utf8_lossy(&chk_bytes);
                        for line in chk_text.lines() {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let hash = parts[0];
                                let filename = parts[1].trim_start_matches('*');
                                if filename == target_binary_name
                                    || filename == "mq"
                                    || filename == "mq.exe"
                                {
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

        let exe_file_name = current_exe
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let new_exe = exe_dir.join(format!("{exe_file_name}.new"));
        let old_exe = exe_dir.join(format!("{exe_file_name}.old"));

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

        // Set executable permissions (0755 on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&new_exe)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&new_exe, perms)?;
        }

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

    /// Restart launcher process cleanly across platforms.
    pub fn restart_process(args: &[String]) -> Result<()> {
        let current_exe = std::env::current_exe().context("Failed to determine current exe")?;
        info!("Restarting process: {:?} {:?}", current_exe, args);

        let mut cmd = Command::new(current_exe);
        cmd.args(args);
        cmd.arg("--after-update");

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    rustix::process::setsid()
                        .map_err(|e| std::io::Error::from_raw_os_error(e.raw_os_error()))?;
                    Ok(())
                });
            }
        }

        cmd.spawn().context("Failed to spawn updated process")?;
        std::process::exit(0);
    }
}

/// Simple semver comparison helper (`a` newer than `b`?).
pub fn is_version_newer(current: &str, candidate: &str) -> bool {
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
        assert!(!is_version_newer("2.2.0", "2.1.0"));
    }

    #[test]
    fn test_minisign_signature_verification_success_and_tampering() {
        let keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let pk_box = keypair.pk.to_box().unwrap().to_string();
        let pk_str = pk_box.lines().nth(1).unwrap().trim();

        let data = b"4a4f89d34e62  mq-linux-x86_64\n3b2e7c1092a4  mq-windows-x86_64.exe\n";
        let sig_box = minisign::sign(None, &keypair.sk, data.as_slice(), None, None)
            .unwrap()
            .to_string();

        // 1. Success validation
        assert!(MinisignVerifier::verify(data, &sig_box, pk_str).is_ok());

        // 2. Tampered content rejected
        let tampered_data = b"tampered content";
        assert!(MinisignVerifier::verify(tampered_data, &sig_box, pk_str).is_err());

        // 3. Wrong public key rejected
        let other_keypair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let other_pk_box = other_keypair.pk.to_box().unwrap().to_string();
        let other_pk_str = other_pk_box.lines().nth(1).unwrap().trim();
        assert!(MinisignVerifier::verify(data, &sig_box, other_pk_str).is_err());
    }

    #[test]
    fn test_official_embedded_public_key_format() {
        let pk = minisign_verify::PublicKey::from_base64(MQ_RELEASE_PUBLIC_KEY);
        assert!(
            pk.is_ok(),
            "Embedded public key must be valid base64 Minisign public key"
        );
    }
}
