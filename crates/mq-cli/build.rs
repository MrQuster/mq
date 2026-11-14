use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../packaging/windows/mq.ico");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icon_path = manifest_dir
        .join("..")
        .join("..")
        .join("packaging")
        .join("windows")
        .join("mq.ico");

    assert!(
        icon_path.exists(),
        "Windows icon file not found at {}",
        icon_path.display()
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let (major, minor, patch) = parse_semver(&version);

    let rc_path = out_dir.join("mq.rc");
    let res_path = out_dir.join("mq.res");

    let icon_escaped = icon_path.to_string_lossy().replace('\\', "\\\\");
    let rc_content = format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "MrQuster\0"
            VALUE "FileDescription", "MQ Minecraft Launcher\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "mq\0"
            VALUE "OriginalFilename", "mq.exe\0"
            VALUE "ProductName", "MQ\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#,
        icon = icon_escaped,
        major = major,
        minor = minor,
        patch = patch,
        version = version,
    );

    fs::write(&rc_path, rc_content).expect("failed to write mq.rc");

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "msvc" {
        if let Some(rc_exe) = find_rc_exe() {
            let status = Command::new(&rc_exe)
                .args(["/nologo", "/fo"])
                .arg(&res_path)
                .arg(&rc_path)
                .status()
                .expect("failed to invoke rc.exe");
            assert!(status.success(), "rc.exe failed with status {status}");
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        } else {
            println!("cargo:warning=rc.exe not found; skipping Windows resource embedding");
        }
    } else if let Some(windres) =
        find_in_path("windres.exe").or_else(|| find_in_path("x86_64-w64-mingw32-windres"))
    {
        let status = Command::new(windres)
            .arg("-i")
            .arg(&rc_path)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(&res_path)
            .status()
            .expect("failed to invoke windres");
        assert!(status.success(), "windres failed with status {status}");
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
    }
}

fn parse_semver(ver: &str) -> (u64, u64, u64) {
    let core = ver.split(['-', '+']).next().unwrap_or("0.0.0");
    let mut parts = core.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_rc_exe() -> Option<PathBuf> {
    if let Some(p) = find_in_path("rc.exe") {
        return Some(p);
    }

    if let Ok(sdk_bin) = env::var("WindowsSdkVerBinPath") {
        let candidate = Path::new(&sdk_bin).join("x64").join("rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let kits_bin = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = fs::read_dir(kits_bin) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        versions.sort();
        versions.reverse();
        for ver_dir in versions {
            let candidate = ver_dir.join("x64").join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}
