//! Handles checking for updates and downloading the latest lje-w64.dll.
//! Also uses the Win32 versioning API to read the dll's LJE version.

use serde::Serialize;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr;

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

use crate::settings::Settings;

const USER_AGENT: &str = "LJE-Launcher";
const COMMITS_URL: &str = "https://api.github.com/repos/lj-expand/lj-expand/commits/";
const DOWNLOAD_URL: &str = "https://github.com/lj-expand/lj-expand/releases/download/";
const PRODUCT_VERSION_SUB_BLOCK: &str = r"\StringFileInfo\040904B0\ProductVersion";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub out_of_date: bool,
}

/// Path to the injected DLL, next to the current executable.
pub fn dll_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop();
    path.push("lje-w64.dll");
    path
}

/// "not installed" if the DLL is missing, otherwise its ProductVersion, or
/// "unknown" if it can't be read.
pub fn get_current_version() -> String {
    let path = dll_path();
    if !path.exists() {
        return "not installed".to_string();
    }

    match get_dll_version(&path) {
        Some(version) if !version.is_empty() => version,
        _ => "unknown".to_string(),
    }
}

/// Reads the ProductVersion resource.
fn get_dll_version(path: &Path) -> Option<String> {
    // SAFETY: all buffers live for the duration of their respective calls.
    unsafe {
        let filename = wide(&path.to_string_lossy());

        let size = GetFileVersionInfoSizeW(PCWSTR::from_raw(filename.as_ptr()), None);
        if size == 0 {
            return None;
        }

        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR::from_raw(filename.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void)
            .ok()?;

        let sub_block = wide(PRODUCT_VERSION_SUB_BLOCK);
        let mut buffer: *mut c_void = ptr::null_mut();
        let mut len: u32 = 0;
        let found = VerQueryValueW(
            data.as_ptr() as *const c_void,
            PCWSTR::from_raw(sub_block.as_ptr()),
            &mut buffer,
            &mut len,
        );
        if !found.as_bool() {
            return None;
        }

        // VerQueryValueW reports the length in characters including the NUL
        // terminator.
        let chars = len.saturating_sub(1) as usize;
        let version = String::from_utf16_lossy(std::slice::from_raw_parts(buffer as *const u16, chars));
        let version = version.trim().to_string();
        Some(version)
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))
}

/// Latest commit sha prefix (7 chars) for the configured release branch.
pub async fn get_latest_version(settings: &Settings) -> Result<String, String> {
    let url = format!("{COMMITS_URL}{}", settings.release_branch);
    let response = http_client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("update check failed: {e}"))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("update check failed: {e}"))?;

    let sha = json
        .get("sha")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "update check failed: response missing sha".to_string())?;
    sha.get(..7)
        .map(|s| s.to_string())
        .ok_or_else(|| "update check failed: unexpected sha".to_string())
}

pub async fn check_update() -> Result<UpdateStatus, String> {
    let settings = Settings::load();
    let latest = get_latest_version(&settings).await?;
    let current = get_current_version();
    let out_of_date = current != latest;
    Ok(UpdateStatus {
        current,
        latest,
        out_of_date,
    })
}

/// Downloads the latest lje-w64.dll next to the current executable.
pub async fn download_update() -> Result<(), String> {
    let settings = Settings::load();
    let url = format!("{DOWNLOAD_URL}{}-latest/lje-w64.dll", settings.release_branch);

    let bytes = http_client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    let path = dll_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("download failed: {e}"))?;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("download failed: {e}"))?;
    Ok(())
}

/// Encodes a string as UTF-16 with a trailing NUL, for PCWSTR params.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
