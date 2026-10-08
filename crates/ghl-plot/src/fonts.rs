//! System Font Auto-Discovery and Registration Engine (RFC 16 - Phase 7.1).
//!
//! Indexes and scans host operating system font directories (`C:\Windows\Fonts`,
//! `/Library/Fonts`, `/usr/share/fonts`, etc.) to automatically register
//! TrueType (`.ttf`) and OpenType (`.otf`) fonts with Plotters for pixel-perfect
//! raster (PNG) and vector rendering without external R dependencies like `showtext`.

use plotters::style::FontStyle;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static REGISTERED_FONTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Return the candidate OS font directories for the current host system.
pub fn os_font_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(format!("{windir}\\Fonts")));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(format!(
                "{local_app_data}\\Microsoft\\Windows\\Fonts"
            )));
        }
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/Library/Fonts")));
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
            dirs.push(PathBuf::from(format!("{home}/.fonts")));
        }
    }

    dirs
}

/// Normalize a font name for case-insensitive, punctuation-free comparison.
fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Common OS font family alias mapping to primary file stem.
fn common_font_file_stem(norm: &str) -> Option<&'static str> {
    match norm {
        "segoeui" | "segoe" => Some("segoeui"),
        "arial" => Some("arial"),
        "calibri" => Some("calibri"),
        "consolas" | "consola" => Some("consola"),
        "firacode" | "fira" => Some("firacode"),
        "cascadiacode" | "cascadia" => Some("cascadiacode"),
        "couriernew" | "courier" => Some("cour"),
        "timesnewroman" | "times" => Some("times"),
        "tahoma" => Some("tahoma"),
        "verdana" => Some("verdana"),
        "trebuchetms" | "trebuchet" => Some("trebuc"),
        "comicsansms" | "comicsans" => Some("comic"),
        "georgia" => Some("georgia"),
        "impact" => Some("impact"),
        _ => None,
    }
}

/// Recursively search for a matching font file up to `max_depth`.
fn find_font_file(
    dir: &Path,
    target_norm: &str,
    known_stem: Option<&str>,
    depth: usize,
) -> Option<PathBuf> {
    if depth > 3 || !dir.exists() || !dir.is_dir() {
        return None;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return None,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_font_file(&path, target_norm, known_stem, depth + 1) {
                return Some(found);
            }
        } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let ext_lower = ext.to_lowercase();
            if ext_lower == "ttf" || ext_lower == "otf" {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let stem_norm = normalize_name(stem);
                    if stem_norm == target_norm {
                        return Some(path);
                    }
                    if let Some(ks) = known_stem {
                        if stem_norm.starts_with(ks) {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Automatically discover and register a system font with Plotters.
///
/// Returns `true` if the font is available (either built-in, already registered,
/// or successfully loaded from disk and registered).
pub fn discover_and_register_font(family: &str) -> bool {
    let lower = family.trim().to_lowercase();
    if lower == "sans-serif" || lower == "serif" || lower == "monospace" {
        return true;
    }

    let mut guard = REGISTERED_FONTS.lock().unwrap();
    let registered = guard.get_or_insert_with(HashSet::new);
    if registered.contains(family) {
        return true;
    }

    let target_norm = normalize_name(family);
    let known_stem = common_font_file_stem(&target_norm);

    for dir in os_font_directories() {
        if let Some(path) = find_font_file(&dir, &target_norm, known_stem, 0) {
            if let Ok(bytes) = std::fs::read(&path) {
                let static_bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
                let _ = plotters::style::register_font(family, FontStyle::Normal, static_bytes);
                registered.insert(family.to_string());
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_font_directories_exist() {
        let dirs = os_font_directories();
        assert!(
            !dirs.is_empty(),
            "Host OS must provide at least one font directory"
        );
    }

    #[test]
    fn test_system_font_discovery() {
        // On Windows, Segoe UI or Arial or Consolas always exist
        #[cfg(target_os = "windows")]
        {
            let found =
                discover_and_register_font("Segoe UI") || discover_and_register_font("Arial");
            assert!(
                found,
                "Should discover Windows standard font Segoe UI or Arial"
            );
        }
    }
}
