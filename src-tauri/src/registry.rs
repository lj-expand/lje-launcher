//! Community script registry.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git;

const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/lj-expand/lje-registry/refs/heads/registry/registry.json";
const CACHE_FILE: &str = "registry.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryScript {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub dependencies: Vec<String>,
    pub binaries: Vec<String>,
    pub repo: String,
    pub url: String,
    pub pushed_at: String,
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    generated_at: String,
    scripts: Vec<RegistryScript>,
}

/// Registry artifact: generation timestamp + entries.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryData {
    pub generated_at: String,
    pub scripts: Vec<RegistryScript>,
}

/// What an install actually did, for the UI to report.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub installed: Vec<String>,
    pub external: Vec<String>,
}

fn cache_path() -> PathBuf {
    crate::scripts::scripts_dir().join(CACHE_FILE)
}

/// Fetches the registry artifact, validates + caches it, returns the data.
pub async fn refresh() -> Result<RegistryData, String> {
    let client = reqwest::Client::builder()
        .user_agent("LJE-Launcher")
        .build()
        .map_err(|e| e.to_string())?;

    let bytes = client
        .get(REGISTRY_URL)
        .send()
        .await
        .map_err(|e| format!("failed to fetch registry: {e}"))?
        .error_for_status()
        .map_err(|e| format!("registry fetch failed (is the repo public?): {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let file: RegistryFile =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid registry.json: {e}"))?;

    if let Some(parent) = cache_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&cache_path(), &bytes).map_err(|e| e.to_string())?;

    Ok(RegistryData {
        generated_at: file.generated_at,
        scripts: file.scripts,
    })
}

/// Reads the cached registry artifact.
pub fn list() -> Result<RegistryData, String> {
    let text =
        std::fs::read_to_string(cache_path()).map_err(|_| "registry not fetched yet".to_string())?;
    let file: RegistryFile =
        serde_json::from_str(&text).map_err(|e| format!("cached registry.json is invalid: {e}"))?;
    Ok(RegistryData {
        generated_at: file.generated_at,
        scripts: file.scripts,
    })
}

fn find_entry<'a>(entries: &'a [RegistryScript], name: &str) -> Option<&'a RegistryScript> {
    // Case-insensitive: dep strings like "LJE-HTTP" must resolve to "lje-http".
    entries.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

/// Installs a script by registry name, resolves dependencies recursively. Returns the list of installed scripts and any external dependencies that aren't in the registry (e.g. Eyoko1.ljeutil).
pub fn install(name: &str) -> Result<InstallResult, String> {
    let data = list()?;
    let entries = &data.scripts;
    let scripts_dir = crate::scripts::scripts_dir();

    let entry = find_entry(entries, name)
        .ok_or_else(|| format!("'{name}' not found in registry"))?;

    let mut result = InstallResult {
        installed: Vec::new(),
        external: Vec::new(),
    };
    let mut visiting: HashSet<String> = HashSet::new();

    install_rec(entries, &entry.name, &scripts_dir, &mut result, &mut visiting)?;

    Ok(result)
}

fn install_rec(
    entries: &[RegistryScript],
    name: &str,
    scripts_dir: &Path,
    result: &mut InstallResult,
    visiting: &mut HashSet<String>,
) -> Result<(), String> {
    if !visiting.insert(name.to_string()) {
        return Err(format!("circular dependency detected at '{name}'"));
    }

    let entry = find_entry(entries, name)
        .ok_or_else(|| format!("dependency '{name}' not found in registry"))?;

    let target = scripts_dir.join(name);
    if target.exists() {
        // Already installed — count it in the report, don't re-clone.
        if !result.installed.iter().any(|i| i == name) {
            result.installed.push(name.to_string());
        }
        visiting.remove(name);
        return Ok(());
    }

    let clone_url = format!("https://github.com/{}.git", entry.repo);
    git::git(
        scripts_dir,
        &["clone", "--depth", "1", "--recurse-submodules", &clone_url, name],
    )
    .map_err(|e| format!("failed to clone '{name}': {e}"))?;

    if !target.join("info.toml").exists() {
        let _ = std::fs::remove_dir_all(&target);
        return Err(format!("'{name}' has no info.toml - not a valid LJE script"));
    }

    // Resolve dependencies from the freshly cloned info.toml (source of
    // truth for what the script actually needs).
    let deps = crate::scripts::read_dependencies(&target);
    for dep in deps {
        let dep = dep.trim();
        if dep.is_empty() {
            continue;
        }
        if let Some(dep_entry) = find_entry(entries, dep) {
            let canonical = &dep_entry.name;
            if !result.installed.iter().any(|i| i == canonical) {
                install_rec(entries, canonical, scripts_dir, result, visiting)?;
            }
        } else if !result.external.iter().any(|i| i == dep) {
            result.external.push(dep.to_string());
        }
    }

    visiting.remove(name);
    result.installed.push(name.to_string());
    Ok(())
}

/// Removes a script folder (name = folder name in ~/.lje/scripts).
pub fn uninstall(name: &str) -> Result<(), String> {
    let target = crate::scripts::scripts_dir().join(name);
    if !target.exists() {
        return Err(format!("'{name}' is not installed"));
    }
    std::fs::remove_dir_all(&target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "generatedAt": "2026-08-18T19:47:00.455Z",
      "scripts": [
        {
          "name": "antifreeze",
          "version": "2.2.0",
          "authors": ["Xandertron", "yogwoggf"],
          "dependencies": [],
          "binaries": ["lje-ffi"],
          "repo": "Xandertron/antifreeze",
          "url": "https://github.com/Xandertron/antifreeze",
          "pushedAt": "2026-07-30T06:42:04Z",
          "description": "a swiss army knife multi-tool"
        }
      ]
    }"#;

    #[test]
    fn parses_registry_artifact() {
        let file: RegistryFile = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(file.generated_at, "2026-08-18T19:47:00.455Z");
        assert_eq!(file.scripts.len(), 1);
        let s = &file.scripts[0];
        assert_eq!(s.name, "antifreeze");
        assert_eq!(s.authors, vec!["Xandertron".to_string(), "yogwoggf".to_string()]);
        assert_eq!(s.binaries, vec!["lje-ffi".to_string()]);
        assert_eq!(s.dependencies, Vec::<String>::new());
        assert_eq!(s.repo, "Xandertron/antifreeze");
    }

    /// Try installing a script from the registry, then check that it appears in list_scripts.
    #[test]
    #[ignore]
    fn e2e_install_and_list() {
        let temp = std::env::temp_dir().join(format!("lje-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        unsafe { std::env::set_var("USERPROFILE", &temp) };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let data = rt.block_on(refresh()).expect("refresh should fetch");
        assert!(!data.scripts.is_empty(), "registry has entries");

        let result = install(&data.scripts[0].name).expect("install should succeed");
        assert_eq!(result.installed[result.installed.len() - 1], data.scripts[0].name);

        let listed = crate::scripts::list_scripts();
        let names: Vec<String> = listed
            .iter()
            .map(|s| {
                std::path::Path::new(&s.path)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        assert!(
            names.iter().any(|n| n == &data.scripts[0].name),
            "installed script should appear in list_scripts, got: {names:?}"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }
}
