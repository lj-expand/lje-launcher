//! Each script lives in its own folder with an `info.toml` containing a
//! `[script]` table. Disabled scripts have the file renamed to
//! `info.toml.disabled`. This is an actual convention over in the LJE core,
//! so that the warning won't be spammed when a script is disabled.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

const SCRIPTS_DIR_NAME: &str = "scripts";
const INFO_FILE: &str = "info.toml";
const DISABLED_FILE: &str = "info.toml.disabled";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInfo {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub url: Option<String>,
    pub dependencies: Vec<String>,
    pub enabled: bool,
    /// Full path of the script folder (passed back to toggle).
    pub path: String,
}

#[derive(serde::Deserialize)]
struct ScriptToml {
    script: ScriptMeta,
}

#[derive(serde::Deserialize)]
struct ScriptMeta {
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    url: Option<String>,
    #[serde(default)]
    dependencies: Option<Vec<String>>,
}

pub fn scripts_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").unwrap_or_default();
    PathBuf::from(home).join(".lje").join(SCRIPTS_DIR_NAME)
}

/// Lists all scripts, sorted by name. Folders without a parseable
/// `info.toml`/`info.toml.disabled` are skipped.
pub fn list_scripts() -> Vec<ScriptInfo> {
    let mut scripts = Vec::new();

    let Ok(entries) = fs::read_dir(scripts_dir()) else {
        return scripts;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let info_path = path.join(INFO_FILE);
        let disabled_path = path.join(DISABLED_FILE);

        let enabled = info_path.exists();
        let toml_path = if enabled { &info_path } else { &disabled_path };
        if !toml_path.exists() {
            continue;
        }

        let Ok(text) = fs::read_to_string(toml_path) else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<ScriptToml>(&text) else {
            continue;
        };

        let meta = parsed.script;
        scripts.push(ScriptInfo {
            name: meta.name,
            author: meta.author,
            version: meta.version,
            url: meta.url,
            dependencies: meta.dependencies.unwrap_or_default(),
            enabled,
            path: path.to_string_lossy().into_owned(),
        });
    }

    scripts.sort_by(|a, b| a.name.cmp(&b.name));
    scripts
}

/// Enables/disables a script by renaming `info.toml` <-> `info.toml.disabled`.
pub fn set_script_enabled(path: &str, enabled: bool) -> Result<(), String> {
    let dir = PathBuf::from(path);
    let info_path = dir.join(INFO_FILE);
    let disabled_path = dir.join(DISABLED_FILE);

    if enabled {
        if disabled_path.exists() {
            fs::rename(&disabled_path, &info_path).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    } else if info_path.exists() {
        fs::rename(&info_path, &disabled_path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_script_toml() {
        let toml = r#"
[script]
name = "gilbhax"
author = "yogwoggf"
version = "1.0.0"
url = "https://github.com"
dependencies = ["Eyoko1.ljeutil", "someother.lib"]
"#;
        let parsed: ScriptToml = toml::from_str(toml).unwrap();
        assert_eq!(parsed.script.name.as_deref(), Some("gilbhax"));
        assert_eq!(parsed.script.author.as_deref(), Some("yogwoggf"));
        assert_eq!(parsed.script.version.as_deref(), Some("1.0.0"));
        assert_eq!(parsed.script.url.as_deref(), Some("https://github.com"));
        assert_eq!(
            parsed.script.dependencies.as_deref(),
            Some(&["Eyoko1.ljeutil".to_string(), "someother.lib".to_string()][..])
        );
    }

    #[test]
    fn parses_minimal_script_toml() {
        let parsed: ScriptToml = toml::from_str("[script]\nname = \"aimbot\"\n").unwrap();
        assert_eq!(parsed.script.name.as_deref(), Some("aimbot"));
        assert!(parsed.script.url.is_none());
        assert!(parsed.script.dependencies.is_none());
    }

    #[test]
    fn toggle_renames_info_file() {
        let dir = std::env::temp_dir().join(format!("lje-script-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(INFO_FILE), "[script]\nname = \"t\"\n").unwrap();
        let path = dir.to_string_lossy().into_owned();

        set_script_enabled(&path, false).unwrap();
        assert!(!dir.join(INFO_FILE).exists());
        assert!(dir.join(DISABLED_FILE).exists());

        set_script_enabled(&path, true).unwrap();
        assert!(dir.join(INFO_FILE).exists());
        assert!(!dir.join(DISABLED_FILE).exists());

        set_script_enabled(&path, true).unwrap();
        assert!(dir.join(INFO_FILE).exists());

        fs::remove_dir_all(&dir).unwrap();
    }
}
