//! Reads the Steam install path from the 64-bit view of the registry and walks
//! `steamapps/libraryfolders.vdf` looking for a library that contains GMod
//! (app id 4000) with a win64 build.

use std::path::PathBuf;
use std::ptr;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
    KEY_WOW64_64KEY, REG_SZ, REG_VALUE_TYPE,
};

use crate::vdf::{self, VdfValue};

const STEAM_REGISTRY_SUBKEY: &str = r"SOFTWARE\WOW6432Node\Valve\Steam";
const STEAM_PATH_KEY: &str = "InstallPath";
const GMOD_APP_ID: &str = "4000";

/// Returns the full path to `bin/win64/gmod.exe`, or `None` if it can't be found.
pub fn locate() -> Option<String> {
    let steam_path = steam_install_path()?;

    let vdf_path = PathBuf::from(&steam_path).join("steamapps").join("libraryfolders.vdf");
    if !vdf_path.exists() {
        return None;
    }

    let text = std::fs::read_to_string(&vdf_path).ok()?;
    let root = vdf::parse(&text).ok()?;
    let library_folders = root.get("libraryfolders")?.as_block();

    for value in library_folders.values() {
        let VdfValue::Block(library) = value else {
            continue;
        };

        let Some(VdfValue::Block(apps)) = library.get("apps") else {
            continue;
        };
        if !apps.contains_key(GMOD_APP_ID) {
            continue;
        }

        let Some(VdfValue::String(path)) = library.get("path") else {
            continue;
        };

        let gmod_path = PathBuf::from(path)
            .join("steamapps")
            .join("common")
            .join("GarrysMod")
            .join("bin")
            .join("win64")
            .join("gmod.exe");
        if gmod_path.exists() {
            return Some(gmod_path.to_string_lossy().into_owned());
        }
    }

    None
}

fn steam_install_path() -> Option<String> {
    let subkey = wide(STEAM_REGISTRY_SUBKEY);
    let mut key: HKEY = HKEY(ptr::null_mut());
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR::from_raw(subkey.as_ptr()),
            None,
            KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    let value = query_reg_sz(key, STEAM_PATH_KEY);
    unsafe {
        let _ = RegCloseKey(key);
    }
    value
}

/// Queries a REG_SZ value. Returns `None` on any failure or wrong type.
fn query_reg_sz(key: HKEY, name: &str) -> Option<String> {
    let name_wide = wide(name);

    let mut value_type = REG_VALUE_TYPE(0);
    let mut size: u32 = 0;
    let status: WIN32_ERROR = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR::from_raw(name_wide.as_ptr()),
            None,
            Some(&mut value_type),
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }

    let mut data = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR::from_raw(name_wide.as_ptr()),
            None,
            Some(&mut value_type),
            Some(data.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS || value_type != REG_SZ {
        return None;
    }

    let mut units = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
}

/// Encodes a string as UTF-16 with a trailing NUL, for PCWSTR params.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
