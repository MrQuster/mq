//! Linux system font loader with full Cyrillic + Latin proportional and monospace discovery,
//! falling back gracefully to built-in `egui` fonts when running in minimal environments.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Ordered proportional font candidates on Arch Linux / standard Linux distributions.
pub const PROPORTIONAL_FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
    "/usr/share/fonts/TTF/Inter-Regular.ttf",
    "/usr/share/fonts/inter/Inter-Regular.ttf",
    "/usr/share/fonts/cantarell/Cantarell-VF.otf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/gnu-free/FreeSans.otf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
];

/// Ordered monospace font candidates on Arch Linux / standard Linux distributions.
pub const MONOSPACE_FONT_CANDIDATES: &[&str] = &[
    "/home/honey/.local/share/fonts/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf",
    "/usr/share/fonts/IosevkaNerdFont/IosevkaNerdFontMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/gnu-free/FreeMono.otf",
];

const SYSTEM_PROPORTIONAL_KEY: &str = "mq_system_proportional";
const SYSTEM_MONOSPACE_KEY: &str = "mq_system_monospace";

/// Summary of the proportional and monospace fonts selected during startup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedFontSummary {
    /// Path to the loaded system proportional font, or `None` if using `egui` built-in fallback.
    pub proportional_path: Option<PathBuf>,
    /// Path to the loaded system monospace font, or `None` if using `egui` built-in fallback.
    pub monospace_path: Option<PathBuf>,
}

impl LoadedFontSummary {
    /// Returns `true` if a system proportional font was loaded from disk.
    #[inline]
    pub fn has_system_proportional(&self) -> bool {
        self.proportional_path.is_some()
    }

    /// Returns `true` if a system monospace font was loaded from disk.
    #[inline]
    pub fn has_system_monospace(&self) -> bool {
        self.monospace_path.is_some()
    }

    /// Human-readable description of active fonts.
    pub fn describe(&self) -> String {
        let prop = self
            .proportional_path
            .as_deref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("egui-builtin-proportional");
        let mono = self
            .monospace_path
            .as_deref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("egui-builtin-monospace");
        format!("proportional={prop}, monospace={mono}")
    }
}

fn discover_user_jetbrains_mono() -> Vec<PathBuf> {
    let mut results = Vec::new();
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return results;
    };
    let local_fonts = home.join(".local/share/fonts");
    let direct = local_fonts.join("JetBrainsMono-Regular.ttf");
    if direct.is_file() {
        results.push(direct);
    }
    if let Ok(entries) = fs::read_dir(&local_fonts) {
        let mut matched: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|name| {
                            name.contains("JetBrains")
                                && name.contains("Mono")
                                && name.contains("Regular")
                                && name.ends_with(".ttf")
                        })
                        .unwrap_or(false)
            })
            .collect();
        matched.sort();
        for p in matched {
            if !results.contains(&p) {
                results.push(p);
            }
        }
    }
    results
}

fn load_first_available_font<I, P>(candidates: I) -> Option<(PathBuf, Vec<u8>)>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    for candidate in candidates {
        let path = candidate.as_ref();
        if path.is_file() {
            if let Ok(bytes) = fs::read(path) {
                if bytes.len() >= 64 {
                    return Some((path.to_path_buf(), bytes));
                }
            }
        }
    }
    None
}

/// Probes Linux system font paths and builds [`FontDefinitions`] with Cyrillic + Latin
/// proportional and monospace fonts inserted at priority index `0`, preserving built-in
/// `egui` fonts as fallbacks.
pub fn build_font_definitions() -> (FontDefinitions, LoadedFontSummary) {
    let mut defs = FontDefinitions::default();
    let mut summary = LoadedFontSummary::default();

    if let Some((path, bytes)) = load_first_available_font(PROPORTIONAL_FONT_CANDIDATES) {
        defs.font_data.insert(
            SYSTEM_PROPORTIONAL_KEY.to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, SYSTEM_PROPORTIONAL_KEY.to_owned());
        summary.proportional_path = Some(path);
    }

    let mut mono_candidates: Vec<PathBuf> = discover_user_jetbrains_mono();
    for &static_path in MONOSPACE_FONT_CANDIDATES {
        let p = PathBuf::from(static_path);
        if !mono_candidates.contains(&p) {
            mono_candidates.push(p);
        }
    }

    if let Some((path, bytes)) = load_first_available_font(&mono_candidates) {
        defs.font_data.insert(
            SYSTEM_MONOSPACE_KEY.to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        defs.families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, SYSTEM_MONOSPACE_KEY.to_owned());
        // Also append monospace as fallback in Proportional for box-drawing / technical glyphs.
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(SYSTEM_MONOSPACE_KEY.to_owned());
        summary.monospace_path = Some(path);
    }

    (defs, summary)
}

/// Loads system fonts via [`build_font_definitions`] and installs them into `ctx`.
pub fn configure_system_fonts(ctx: &Context) -> LoadedFontSummary {
    let (defs, summary) = build_font_definitions();
    ctx.set_fonts(defs);
    tracing::debug!(fonts = %summary.describe(), "configured UI font families");
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_font_definitions_loads_system_fonts_and_keeps_fallbacks() {
        let (defs, summary) = build_font_definitions();

        let prop_list = defs
            .families
            .get(&FontFamily::Proportional)
            .expect("Proportional family must exist");
        let mono_list = defs
            .families
            .get(&FontFamily::Monospace)
            .expect("Monospace family must exist");

        assert!(!prop_list.is_empty());
        assert!(!mono_list.is_empty());

        if let Some(ref prop_path) = summary.proportional_path {
            assert!(prop_path.is_file());
            assert_eq!(prop_list[0], SYSTEM_PROPORTIONAL_KEY);
            assert!(defs.font_data.contains_key(SYSTEM_PROPORTIONAL_KEY));
            // Built-in fallback remains after index 0
            assert!(prop_list.len() >= 2);
        }

        if let Some(ref mono_path) = summary.monospace_path {
            assert!(mono_path.is_file());
            assert_eq!(mono_list[0], SYSTEM_MONOSPACE_KEY);
            assert!(defs.font_data.contains_key(SYSTEM_MONOSPACE_KEY));
            assert!(mono_list.len() >= 2);
        }

        // On this Arch Linux workstation at least one proportional and one monospace font exist.
        assert!(
            summary.has_system_proportional(),
            "expected system proportional font on Linux host"
        );
        assert!(
            summary.has_system_monospace(),
            "expected system monospace font on Linux host"
        );
    }
}
