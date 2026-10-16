use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::connection::ConnectionSpec;
use crate::error::{AppError, AppResult};
use crate::launch::LaunchEvent;
use crate::state::AppState;
use crate::store::favorites::Favorite;
use crate::store::history::HistoryEntry;
use crate::store::keybindings::Keybindings;
use crate::store::recents::RecentConnection;
use crate::store::settings::{Settings, Theme};
use crate::store::ui_state::UiState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Gives the window the app's theme, so the vibrancy behind the chrome matches the colors
/// drawn on top of it; `System` follows macOS.
pub fn apply_window_theme(app: &AppHandle, theme: Theme) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_theme(match theme {
            Theme::System => None,
            Theme::Light => Some(tauri::Theme::Light),
            Theme::Dark => Some(tauri::Theme::Dark),
        });
    }
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    let saved = state.settings.update(settings)?;
    apply_window_theme(&app, saved.theme);
    log::set_max_level(saved.log_level.into());
    log::info!("Settings updated");
    Ok(saved)
}

#[tauri::command]
pub fn get_keybindings(state: State<'_, AppState>) -> Keybindings {
    state.keybindings.get()
}

#[tauri::command]
pub fn set_keybindings(
    state: State<'_, AppState>,
    bindings: Keybindings,
) -> AppResult<Keybindings> {
    state.keybindings.update(bindings)
}

#[tauri::command]
pub fn get_ui_state(state: State<'_, AppState>, identity: String) -> UiState {
    state.ui_state.get(&identity)
}

#[tauri::command]
pub fn set_ui_state(
    state: State<'_, AppState>,
    identity: String,
    ui: UiState,
) -> AppResult<UiState> {
    state.ui_state.set(&identity, ui)
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

/// Only web links may be opened from data; anything else (file:, custom schemes) is refused.
fn is_web_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> AppResult<()> {
    if !is_web_url(&url) {
        return Err(AppError::InvalidInput(format!("Not a web link: {url}")));
    }
    app.opener()
        .open_url(url, None::<&str>)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_links_can_be_opened() {
        assert!(is_web_url("https://wc-pp.ddev.site/checkout"));
        assert!(is_web_url("http://localhost:8080"));
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("not a url"));
    }
}
