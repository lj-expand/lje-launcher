//! `#[serde(rename_all = "PascalCase")]` is chosen here since this is a migration from the old C# version.
//! Some people actually used it, so need to keep it compatible.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Settings {
    #[serde(default)]
    pub launch_args: String,
    #[serde(default = "default_release_branch")]
    pub release_branch: String,
}

fn default_release_branch() -> String {
    "expansion".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            launch_args: String::new(),
            release_branch: default_release_branch(),
        }
    }
}

impl Settings {
    /// `%USERPROFILE%\.lje_launcher_settings.json`
    pub fn settings_path() -> Option<PathBuf> {
        std::env::var_os("USERPROFILE")
            .map(|profile| PathBuf::from(profile).join(".lje_launcher_settings.json"))
    }

    /// Missing or corrupt settings file silently fall back to defaults (matches C#).
    pub fn load() -> Self {
        let Some(path) = Self::settings_path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|_| Self::default())
    }

    /// Writes pretty JSON, creating the parent directory if it doesn't exist.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::settings_path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "USERPROFILE is not set",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }
}
