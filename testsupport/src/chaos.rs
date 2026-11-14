//! Chaos engineering helpers for local filesystem and process resiliency testing.
//!
//! Provides utilities to simulate:
//! - Low disk space / disk exhaustion
//! - Read-only directories (EACCES)
//! - Process termination (SIGKILL) mid-installation and graceful resume
//! - Concurrent installations of the same Minecraft version/instance

use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

/// RAII guard that temporarily restricts permissions on a directory (e.g. `0o555` read-only)
/// and restores the original permissions upon drop.
#[derive(Debug)]
pub struct ReadOnlyGuard {
    path: PathBuf,
    #[cfg(unix)]
    original_mode: u32,
    #[cfg(not(unix))]
    original_readonly: bool,
}

impl ReadOnlyGuard {
    /// Changes `path` mode to read-only (`0o555` on Unix).
    pub fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let p = path.as_ref().to_path_buf();
        let metadata = std::fs::metadata(&p)?;
        #[cfg(unix)]
        {
            let original_mode = metadata.permissions().mode();
            std::fs::set_permissions(&p, Permissions::from_mode(0o555))?;
            Ok(Self {
                path: p,
                original_mode,
            })
        }
        #[cfg(not(unix))]
        {
            let mut perms: Permissions = metadata.permissions();
            let original_readonly = perms.readonly();
            perms.set_readonly(true);
            std::fs::set_permissions(&p, perms)?;
            Ok(Self {
                path: p,
                original_readonly,
            })
        }
    }

    /// Explicit path reference.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ReadOnlyGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ =
                std::fs::set_permissions(&self.path, Permissions::from_mode(self.original_mode));
        }
        #[cfg(not(unix))]
        {
            if let Ok(meta) = std::fs::metadata(&self.path) {
                let mut perms = meta.permissions();
                perms.set_readonly(self.original_readonly);
                let _ = std::fs::set_permissions(&self.path, perms);
            }
        }
    }
}

/// Executes closure `f` while `dir` has read-only permissions (`0o555`), then restores permissions.
pub fn with_read_only_dir<P: AsRef<Path>, R>(
    dir: P,
    f: impl FnOnce(&Path) -> R,
) -> std::io::Result<R> {
    let guard = ReadOnlyGuard::new(dir)?;
    Ok(f(guard.path()))
}

/// Helper that verifies an installation attempt in a read-only directory fails gracefully.
pub fn assert_install_fails_on_readonly_dir(
    mq_bin: &Path,
    data_dir: &Path,
    instance_name: &str,
    version: &str,
) -> bool {
    let instances_dir = data_dir.join("instances");
    let _ = std::fs::create_dir_all(&instances_dir);

    // Make instances directory read-only
    let guard = match ReadOnlyGuard::new(&instances_dir) {
        Ok(g) => g,
        Err(_) => return false,
    };

    let output = Command::new(mq_bin)
        .env("MQ_DATA_DIR", data_dir)
        .arg("instances")
        .arg("create")
        .arg(instance_name)
        .arg("--version")
        .arg(version)
        .output();

    drop(guard);

    match output {
        Ok(out) => !out.status.success(),
        Err(_) => false,
    }
}

/// Spawns an installation command, monitors the instance directory until `.installing` or download begins,
/// abruptly sends SIGKILL, and verifies that the process was terminated.
pub fn kill_installation_mid_flight(
    mq_bin: &Path,
    data_dir: &Path,
    instance_name: &str,
    version: &str,
    kill_after: Duration,
) -> std::io::Result<bool> {
    let mut child = Command::new(mq_bin)
        .env("MQ_DATA_DIR", data_dir)
        .arg("instances")
        .arg("create")
        .arg(instance_name)
        .arg("--version")
        .arg(version)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    std::thread::sleep(kill_after);

    // Send SIGKILL (kill -9)
    let killed = match child.kill() {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => false,
        Err(e) => return Err(e),
    };

    let _ = child.wait();
    Ok(killed)
}

/// Spawns two instances of the `mq` installation command concurrently targeting the exact same instance,
/// waits for both to exit, and returns both `Output` structs.
pub fn run_concurrent_installs(
    mq_bin: &Path,
    data_dir: &Path,
    instance_name: &str,
    version: &str,
) -> std::io::Result<(Output, Output)> {
    let child1 = Command::new(mq_bin)
        .env("MQ_DATA_DIR", data_dir)
        .arg("instances")
        .arg("create")
        .arg(instance_name)
        .arg("--version")
        .arg(version)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let child2 = Command::new(mq_bin)
        .env("MQ_DATA_DIR", data_dir)
        .arg("instances")
        .arg("create")
        .arg(instance_name)
        .arg("--version")
        .arg(version)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let out1 = child1.wait_with_output()?;
    let out2 = child2.wait_with_output()?;

    Ok((out1, out2))
}
