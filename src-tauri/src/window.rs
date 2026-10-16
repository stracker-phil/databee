use tauri::{AppHandle, Manager};

use crate::store::settings::Theme;

/// Gives the window the app's theme, so the vibrancy behind the chrome matches the colors
/// drawn on top of it; `System` follows macOS.
pub fn apply_window_theme(app: &AppHandle, theme: Theme) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let theme = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    if let Err(e) = window.set_theme(theme) {
        log::warn!("Couldn't set the window theme: {e}");
    }
}

/// Skipped with `DATABEE_BACKGROUND=1` in debug builds so scripts/dev never steal focus.
pub fn bring_to_front(app: &AppHandle) {
    #[cfg(debug_assertions)]
    if crate::dev::background() {
        return;
    }
    // Failing only means the window is gone; there is nothing left to bring forward.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
