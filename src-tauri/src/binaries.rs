//! Binary management for `~/.lje/binaries`

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

const BINARIES_DIR_NAME: &str = "binaries";
const PREFIX: &str = "lje-";
const DLL_EXT: &str = ".dll";
const DISABLED_SUFFIX: &str = ".dll.disabled";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryInfo {
    /// e.g. "lje-ffi" (no extension).
    pub name: String,
    pub enabled: bool,
    /// Full path to the active `.dll` (or the `.disabled` file if disabled).
    pub path: String,
}

fn binaries_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").unwrap_or_default();
    PathBuf::from(home).join(".lje").join(BINARIES_DIR_NAME)
}

fn is_binary_filename(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with(PREFIX)
        && (lower.ends_with(DLL_EXT) || lower.ends_with(DISABLED_SUFFIX))
}

/// Strips the `lje-` prefix and extension from a filename.
fn display_name(filename: &str) -> String {
    let lower = filename.to_lowercase();
    let stem = lower
        .strip_suffix(DISABLED_SUFFIX)
        .or_else(|| lower.strip_suffix(DLL_EXT))
        .unwrap_or(filename);
    stem.to_string()
}

/// Lists binaries sorted by name. Enabled = the `.dll` exists.
pub fn list_binaries() -> Vec<BinaryInfo> {
    let mut binaries = Vec::new();

    let Ok(entries) = fs::read_dir(binaries_dir()) else {
        return binaries;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(filename) = path.file_name().map(|f| f.to_string_lossy().into_owned()) else {
            continue;
        };
        if !is_binary_filename(&filename) {
            continue;
        }

        // The entry file itself is the state: `.dll` = enabled,
        // `.dll.disabled` = disabled.
        let enabled = !filename.to_lowercase().ends_with(DISABLED_SUFFIX);
        binaries.push(BinaryInfo {
            name: display_name(&filename),
            enabled,
            path: path.to_string_lossy().into_owned(),
        });
    }

    binaries.sort_by(|a, b| a.name.cmp(&b.name));
    binaries
}

/// Enables/disables a binary by renaming `.dll` <-> `.dll.disabled`.
/// `path` may point at either file; idempotent.
pub fn set_binary_enabled(path: &str, enabled: bool) -> Result<(), String> {
    let p = PathBuf::from(path);
    let base = {
        let s = p.to_string_lossy();
        if s.to_lowercase().ends_with(DISABLED_SUFFIX) {
            // "...\lje-ffi.dll.disabled" -> "...\lje-ffi.dll" (keep the .dll)
            PathBuf::from(s.strip_suffix(".disabled").unwrap_or(&s))
        } else {
            p
        }
    };
    let disabled = PathBuf::from(format!("{}.disabled", base.to_string_lossy()));

    if enabled {
        if disabled.exists() {
            fs::rename(&disabled, &base).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    } else if base.exists() {
        fs::rename(&base, &disabled).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_binary_filenames() {
        assert!(is_binary_filename("lje-ffi.dll"));
        assert!(is_binary_filename("lje-ffi.dll.disabled"));
        assert!(is_binary_filename("LJE-FFI.DLL"));
        assert!(!is_binary_filename("ffi.dll"));
        assert!(!is_binary_filename("lje-ffi.txt"));
        assert!(!is_binary_filename("lje-ffi"));
    }

    #[test]
    fn parses_display_names() {
        assert_eq!(display_name("lje-ffi.dll"), "lje-ffi");
        assert_eq!(display_name("lje-ffi.dll.disabled"), "lje-ffi");
        assert_eq!(display_name("LJE-IMGUI.DLL"), "lje-imgui");
    }

    #[test]
    fn toggle_renames_binary() {
        let dir = std::env::temp_dir().join(format!("lje-bin-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dll = dir.join("lje-ffi.dll");
        fs::write(&dll, b"fake").unwrap();

        let path = dll.to_string_lossy().into_owned();
        set_binary_enabled(&path, false).unwrap();
        assert!(!dll.exists());
        assert!(dir.join("lje-ffi.dll.disabled").exists());

        let disabled_path = dir.join("lje-ffi.dll.disabled").to_string_lossy().into_owned();
        set_binary_enabled(&disabled_path, true).unwrap();
        assert!(dll.exists());
        assert!(!dir.join("lje-ffi.dll.disabled").exists());

        fs::remove_dir_all(&dir).unwrap();
    }
}
