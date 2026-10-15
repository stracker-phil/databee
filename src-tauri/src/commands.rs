use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::store::settings::Settings;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    let saved = state.settings.update(settings)?;
    log::set_max_level(saved.log_level.into());
    log::info!("Settings updated");
    Ok(saved)
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> AppResult<()> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Io(e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}
