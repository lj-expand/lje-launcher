//! Since LJE needs to take over lua_shared.dll before it loads, the launcher
//! acts as a small debugger. It waits until menusystem.dll (which loads right
//! after lua_shared.dll) is mapped, then injects lje-w64.dll via a remote
//! LoadLibrary thread. Pretty simple and classic.

use std::ffi::c_void;
use std::path::Path;

use serde::Serialize;
use tauri::Emitter;
use windows::core::{s, w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, DBG_EXCEPTION_NOT_HANDLED, ERROR_FILE_NOT_FOUND, GetLastError, HANDLE,
};
use windows::Win32::System::Diagnostics::Debug::{
    ContinueDebugEvent, DebugActiveProcessStop, WaitForDebugEvent, WriteProcessMemory,
    DEBUG_EVENT as WIN32_DEBUG_EVENT,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows::Win32::System::ProcessStatus::GetMappedFileNameW;
use windows::Win32::System::Threading::{
    CreateProcessW, CreateRemoteThread, ResumeThread, WaitForSingleObject, CREATE_SUSPENDED,
    DEBUG_ONLY_THIS_PROCESS, INFINITE, LPTHREAD_START_ROUTINE, PROCESS_INFORMATION, STARTUPINFOW,
};

use crate::settings::Settings;
use crate::updater;

const LOAD_DLL_DEBUG_EVENT: u32 = 6;
const EXIT_PROCESS_DEBUG_EVENT: u32 = 5;
const MAX_PATH: usize = 260;
const INJECT_DLL_MARKER: &str = "menusystem.dll";

/// Payload for the `"log"` event: `{ message, success }`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPayload {
    pub message: String,
    pub success: bool,
}

// Native Windows structs

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct LoadDllDebugInfo {
    h_file: HANDLE,
    lp_base_of_dll: *mut c_void,
    dw_debug_info_file_offset: u32,
    n_debug_info_size: u32,
    lp_image_name: *mut c_void,
    f_unicode: u16,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
struct ExitProcessDebugInfo {
    dw_exit_code: u32,
}

#[allow(dead_code)]
#[repr(C)]
union DebugEventUnion {
    load_dll: LoadDllDebugInfo,
    exit_process: ExitProcessDebugInfo,
    padding: [u8; 176],
}

#[repr(C)]
struct DebugEvent {
    dw_debug_event_code: u32,
    dw_process_id: u32,
    dw_thread_id: u32,
    u: DebugEventUnion,
}

fn emit_log(app: &tauri::AppHandle, message: &str, success: bool) {
    let _ = app.emit(
        "log",
        LogPayload {
            message: message.to_string(),
            success,
        },
    );
}

fn emit_state(app: &tauri::AppHandle, state: &str) {
    let _ = app.emit("state", state);
}

/// Runs the full injection flow, emitting log/state events as it goes.
pub fn inject(app: &tauri::AppHandle, gmod_path: &str, settings: &Settings) -> Result<(), String> {
    let lje_path = updater::dll_path();
    if !lje_path.exists() {
        return Err("lje-w64.dll not found".to_string());
    }

    emit_log(app, "starting injection...", false);
    emit_state(app, "injecting");

    let pi = launch_gmod(gmod_path, &settings.launch_args)?;
    emit_log(app, "gmod launched, waiting for marker dll to load...", false);
    let _ = unsafe { ResumeThread(pi.hThread) };
    start_debug_loop(app, &pi, &lje_path)?;

    emit_log(app, "enjoy!", false);
    Ok(())
}

fn launch_gmod(gmod_path: &str, launch_args: &str) -> Result<PROCESS_INFORMATION, String> {
    let mut startup_info = STARTUPINFOW::default();
    startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;

    let app_name: Vec<u16> = wide(gmod_path);
    let command_line_text = format!("{} {}", gmod_path, launch_args).trim().to_string();
    let mut command_line: Vec<u16> = wide(&command_line_text);

    let mut process_info = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            PCWSTR::from_raw(app_name.as_ptr()),
            Some(PWSTR::from_raw(command_line.as_mut_ptr())),
            None,
            None,
            false,
            DEBUG_ONLY_THIS_PROCESS | CREATE_SUSPENDED,
            None,
            PCWSTR::null(),
            &startup_info,
            &mut process_info,
        )
    };

    if let Err(e) = created {
        let error = unsafe { GetLastError() };
        if error == ERROR_FILE_NOT_FOUND {
            // File wasn't found... so likely not on x86-64 or something.
            return Err("gmod.exe not found. Make sure you are on the x86-64 branch of GMod."
                .to_string());
        }
        return Err(format!("failed to create process: Error {}: {}", error.0, e.message()));
    }

    Ok(process_info)
}

fn start_debug_loop(app: &tauri::AppHandle, pi: &PROCESS_INFORMATION, lje_path: &Path) -> Result<(), String> {
    loop {
        let mut debug_event: DebugEvent = unsafe { std::mem::zeroed() };
        let wait_result = unsafe {
            WaitForDebugEvent(&mut debug_event as *mut DebugEvent as *mut WIN32_DEBUG_EVENT, INFINITE)
        };
        if let Err(e) = wait_result {
            let error = unsafe { GetLastError() };
            return Err(format!(
                "failed to wait for debug event: Error {}: {}",
                error.0,
                e.message()
            ));
        }

        if debug_event.dw_debug_event_code == LOAD_DLL_DEBUG_EVENT {
            let base_of_dll = unsafe { debug_event.u.load_dll.lp_base_of_dll };
            let name = get_dll_filename(pi.hProcess, base_of_dll);
            if name.to_lowercase().ends_with(INJECT_DLL_MARKER) {
                emit_log(app, "marker dll loaded, injecting now!", false);
                inject_dll(app, pi, lje_path)?;
                break;
            }
        }

        if debug_event.dw_debug_event_code == EXIT_PROCESS_DEBUG_EVENT {
            emit_log(
                app,
                "something happened - ensure you are targeting bin/win64/gmod.exe",
                false,
            );
            return Err("process exited before injection".to_string());
        }

        // Pass GMod its exceptions again.
        let _ = unsafe {
            ContinueDebugEvent(
                debug_event.dw_process_id,
                debug_event.dw_thread_id,
                DBG_EXCEPTION_NOT_HANDLED,
            )
        };
    }

    Ok(())
}

fn get_dll_filename(process: HANDLE, address: *mut c_void) -> String {
    let mut buffer = vec![0u16; MAX_PATH];
    let len = unsafe { GetMappedFileNameW(process, address, &mut buffer) };
    if len > 0 {
        String::from_utf16_lossy(&buffer[..len as usize])
    } else {
        String::new()
    }
}

fn inject_dll(app: &tauri::AppHandle, pi: &PROCESS_INFORMATION, lje_path: &Path) -> Result<(), String> {
    let process = pi.hProcess;
    let pid = pi.dwProcessId;

    // Classic remote thread technique: allocate remote memory, write the DLL
    // path into it, then wind up a remote LoadLibraryA thread.
    // Encoding.ASCII.GetBytes(_ljePath + "\0"): non-ASCII chars become '?'.
    let path_text = lje_path.to_string_lossy();
    let mut path_bytes: Vec<u8> = path_text
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    path_bytes.push(0);

    let remote_mem = unsafe {
        VirtualAllocEx(
            process,
            None,
            path_bytes.len(),
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    if remote_mem.is_null() {
        return Err("failed to allocate memory for path".to_string());
    }
    emit_log(app, "allocated remote memory for path", false);

    let written = unsafe {
        WriteProcessMemory(
            process,
            remote_mem,
            path_bytes.as_ptr() as *const c_void,
            path_bytes.len(),
            None,
        )
    };
    if written.is_err() {
        let _ = unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };
        return Err("failed to write path to remote process".to_string());
    }

    // kernel32 loads at the same base address across all userspace processes
    // on 64-bit Windows, so we can grab our own and use it.
    let kernel32 = unsafe { GetModuleHandleW(w!("kernel32.dll")) }
        .map_err(|e| format!("failed to get kernel32 module: {}", e.message()))?;
    let load_library = unsafe { GetProcAddress(kernel32, s!("LoadLibraryA")) }
        .ok_or_else(|| "failed to get LoadLibraryA proc address".to_string())?;
    let start_routine: LPTHREAD_START_ROUTINE = unsafe { std::mem::transmute(load_library) };

    let remote_thread = unsafe {
        CreateRemoteThread(
            process,
            None,
            0,
            start_routine,
            Some(remote_mem as *const c_void),
            0,
            None,
        )
    };
    
    let remote_thread = match remote_thread {
        Ok(thread) => thread,
        Err(_) => return Err("failed to create remote thread".to_string()),
    };

    emit_log(app, "created remote thread to inject lje, waiting...", false);

    // Detach our debugger so we can finally inject
    let _ = unsafe { DebugActiveProcessStop(pid) };
    let _ = unsafe { WaitForSingleObject(remote_thread, INFINITE) };

    emit_log(app, "injection complete, resuming process", true);
    emit_state(app, "success");

    let _ = unsafe { VirtualFreeEx(process, remote_mem, 0, MEM_RELEASE) };
    let _ = unsafe { CloseHandle(remote_thread) };

    emit_log(app, "done", false);
    Ok(())
}

/// Encodes a string as UTF-16 with a trailing NUL, for PWSTR/PCWSTR params.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
