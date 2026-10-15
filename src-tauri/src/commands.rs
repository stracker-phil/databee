use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::connection::ConnectionSpec;
use crate::error::{AppError, AppResult};
use crate::launch::LaunchEvent;
use crate::state::AppState;
use crate::store::favorites::Favorite;
use crate::store::history::HistoryEntry;
use crate::store::recents::RecentConnection;
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

#[tauri::command]
pub fn take_launch_events(state: State<'_, AppState>) -> Vec<LaunchEvent> {
    state.launches.take_events()
}

#[tauri::command]
pub fn list_recents(state: State<'_, AppState>) -> Vec<RecentConnection> {
    state.recents.list()
}

#[tauri::command]
pub fn remove_recent(state: State<'_, AppState>, identity: String) -> AppResult<()> {
    state.recents.remove(&identity)
}

#[tauri::command]
pub fn forget_password(state: State<'_, AppState>, spec: ConnectionSpec) -> AppResult<()> {
    state.secrets.delete(&spec.account())?;
    log::info!("Removed stored password for {}", spec.account());
    Ok(())
}

#[tauri::command]
pub fn list_favorites(state: State<'_, AppState>, account: String) -> Vec<Favorite> {
    state.favorites.list(&account)
}

#[tauri::command]
pub fn save_favorite(state: State<'_, AppState>, favorite: Favorite) -> AppResult<Favorite> {
    state.favorites.save(favorite)
}

#[tauri::command]
pub fn remove_favorite(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.favorites.remove(&id)
}

#[tauri::command]
pub fn reorder_favorites(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<()> {
    state.favorites.reorder(&ids)
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>, account: String) -> Vec<HistoryEntry> {
    state.history.list(&account)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>, account: String) -> AppResult<()> {
    state.history.clear(&account)
}

#[tauri::command]
pub fn log_frontend_error(message: String) {
    log::error!(target: "frontend", "{message}");
}
