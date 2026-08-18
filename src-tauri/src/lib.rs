mod injector;
mod locator;
mod scripts;
mod settings;
mod updater;
mod vdf;

use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
fn settings_get() -> settings::Settings {
    settings::Settings::load()
}

#[tauri::command]
fn settings_save(launch_args: String, release_branch: String) -> Result<(), String> {
    let settings = settings::Settings {
        launch_args,
        release_branch,
    };
    settings.save().map_err(|e| e.to_string())
}

// A lot of these commands are just wrappers.

#[tauri::command]
fn locate_gmod() -> Option<String> {
    locator::locate()
}

#[tauri::command]
fn get_current_version() -> String {
    updater::get_current_version()
}

#[tauri::command]
async fn check_update() -> Result<updater::UpdateStatus, String> {
    updater::check_update().await
}

#[tauri::command]
async fn download_update() -> Result<(), String> {
    updater::download_update().await
}

#[tauri::command]
fn scripts_dir() -> String {
    scripts::scripts_dir().to_string_lossy().into_owned()
}

#[tauri::command]
fn open_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_scripts() -> Vec<scripts::ScriptInfo> {
    scripts::list_scripts()
}

#[tauri::command]
fn set_script_enabled(path: String, enabled: bool) -> Result<(), String> {
    scripts::set_script_enabled(&path, enabled)
}

/// Emits state "fail" and returns the error, so the frontend logs
/// "injection failed: {msg}".
fn inject_failed(app: &tauri::AppHandle, message: String) -> Result<(), String> {
    let _ = app.emit("state", "fail");
    Err(message)
}

#[tauri::command]
async fn inject(gmod_path: String, app: tauri::AppHandle) -> Result<(), String> {
    let settings = settings::Settings::load();
    let thread_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        injector::inject(&thread_app, &gmod_path, &settings)
    })
    .await
    .map_err(|e| format!("injection failed: {e}"))?;

    match result {
        Ok(()) => Ok(()),
        Err(message) => inject_failed(&app, message),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_save,
            locate_gmod,
            get_current_version,
            check_update,
            download_update,
            inject,
            scripts_dir,
            open_folder,
            list_scripts,
            set_script_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
