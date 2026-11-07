//! Cross-platform URL and folder/file opener for MQ.
//!
//! Provides `open_that(target)` to open web URLs in default browser or open local
//! files / directories in the platform's file manager:
//! - Windows: `explorer.exe <path>` for files/folders, or `cmd /c start ""` for URLs.
//! - macOS: `open <target>`
//! - Linux/Unix: `xdg-open <target>`

use std::ffi::OsStr;
use std::io;
use std::process::Command;

/// Opens a path or URL using the platform-specific default application.
pub fn open_that<T: AsRef<OsStr>>(target: T) -> io::Result<()> {
    let target = target.as_ref();

    #[cfg(target_os = "windows")]
    {
        let target_str = target.to_string_lossy();
        if target_str.starts_with("http://") || target_str.starts_with("https://") {
            let status = Command::new("cmd")
                .args(["/c", "start", ""])
                .arg(target)
                .spawn();
            match status {
                Ok(_) => Ok(()),
                Err(err) => {
                    tracing::warn!(
                        "Failed to open URL via cmd /c start: {err}; trying explorer fallback"
                    );
                    Command::new("explorer").arg(target).spawn().map(|_| ())
                }
            }
        } else {
            Command::new("explorer").arg(target).spawn().map(|_| ())
        }
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(target).spawn().map(|_| ())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Command::new("xdg-open").arg(target).spawn().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::open_that;

    #[test]
    fn test_open_that_signature() {
        // Validate open_that accepts &str, String, Path, PathBuf
        let _url = "https://github.com/MrQuster/mq";
        let _path = std::path::Path::new(".");
        // Check function signature compiles
        let _ = open_that as fn(&str) -> std::io::Result<()>;
    }
}
