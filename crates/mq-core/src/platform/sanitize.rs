//! Filename sanitization adhering to Windows and cross-platform filesystem constraints.

/// Returns `true` if `stem` matches a Windows reserved device name (`CON`, `PRN`, `AUX`, `NUL`,
/// `COM1`..`COM9`, `LPT1`..`LPT9`), case-insensitively.
#[inline]
pub fn is_windows_reserved_name(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Sanitizes a filename for safe cross-platform use:
/// 1. Strips Windows forbidden characters: `< > : " / \ | ? *` and ASCII control characters (`\0`..=`\x1f`).
/// 2. Strips Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`).
/// 3. Strips trailing dots and spaces (which are disallowed on Windows NTFS/FAT).
///
/// Preserves Cyrillic and unicode characters as well as valid inner spaces.
pub fn sanitize_filename(name: &str) -> String {
    // 1. Strip forbidden characters: <>:"/\|?* and control characters.
    let mut stripped = String::with_capacity(name.len());
    for ch in name.chars() {
        if !matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'..='\x1f') {
            stripped.push(ch);
        }
    }

    // 2. Check for Windows reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9).
    // Reserved names apply to the base filename stem before the first dot, or the full name.
    let stem = match stripped.split_once('.') {
        Some((stem, _)) => stem,
        None => stripped.as_str(),
    };

    if is_windows_reserved_name(stem) {
        stripped = stripped[stem.len()..].to_string();
    }

    // 3. Strip trailing dots and spaces.
    let trimmed = stripped.trim_end_matches(['.', ' ']);
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_illegal_windows_characters() {
        assert_eq!(
            sanitize_filename("hello<world>:test?\"pipe|star*slash/backslash\\end.txt"),
            "helloworldtestpipestarslashbackslashend.txt"
        );
        assert_eq!(sanitize_filename("clean_filename_123.jar"), "clean_filename_123.jar");
        assert_eq!(sanitize_filename("control\0\t\r\nchars.txt"), "controlchars.txt");
    }

    #[test]
    fn strips_windows_reserved_device_names() {
        for reserved in [
            "CON", "con", "Con", "PRN", "prn", "AUX", "aux", "NUL", "nul", "COM1", "com1", "COM9",
            "com9", "LPT1", "lpt1", "LPT9", "lpt9",
        ] {
            assert_eq!(
                sanitize_filename(reserved),
                "",
                "expected '{reserved}' to be stripped to empty"
            );
            assert_eq!(
                sanitize_filename(&format!("{reserved}.txt")),
                ".txt",
                "expected stem '{reserved}' to be stripped from filename with extension"
            );
        }

        // Non-reserved names should NOT be stripped
        assert_eq!(sanitize_filename("COM0.txt"), "COM0.txt");
        assert_eq!(sanitize_filename("COM10.txt"), "COM10.txt");
        assert_eq!(sanitize_filename("LPT0.txt"), "LPT0.txt");
        assert_eq!(sanitize_filename("LPT10.txt"), "LPT10.txt");
        assert_eq!(sanitize_filename("con_file.txt"), "con_file.txt");
        assert_eq!(sanitize_filename("my_con.txt"), "my_con.txt");
        assert_eq!(sanitize_filename("constant.txt"), "constant.txt");
        assert_eq!(sanitize_filename("auxiliary.png"), "auxiliary.png");
    }

    #[test]
    fn strips_trailing_dots_and_spaces() {
        assert_eq!(sanitize_filename("test.txt.  . "), "test.txt");
        assert_eq!(sanitize_filename("filename..."), "filename");
        assert_eq!(sanitize_filename("filename   "), "filename");
        assert_eq!(sanitize_filename("CON.  ."), "");
        assert_eq!(sanitize_filename("..."), "");
        assert_eq!(sanitize_filename("   "), "");
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn preserves_cyrillic_and_inner_spaces() {
        assert_eq!(
            sanitize_filename("Мод <Красивый Мир>: Тест? 1.21.jar. "),
            "Мод Красивый Мир Тест 1.21.jar"
        );
        assert_eq!(
            sanitize_filename("Папка пользователя/Иван Петров.zip"),
            "Папка пользователяИван Петров.zip"
        );
        assert_eq!(
            sanitize_filename("Сервер #1 [Выживание] (1.21.1).json"),
            "Сервер #1 [Выживание] (1.21.1).json"
        );
    }
}
