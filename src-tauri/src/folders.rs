//! Opens the app's own folders in Finder: the log folder and the data (config) folder.

use std::path::Path;

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn reveal_logs(app: &AppHandle) -> AppResult<()> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Io(e.to_string()))?;
    reveal(app, &dir)
}

pub fn reveal_data(app: &AppHandle) -> AppResult<()> {
    reveal(app, &app.state::<AppState>().config_dir)
}

fn reveal(app: &AppHandle, dir: &Path) -> AppResult<()> {
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}
