pub mod navigation;
pub mod settings;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::store::settings::Theme;

/// Gives every window the app's theme, so the vibrancy behind the chrome matches the colors
/// drawn on top of it; `System` follows macOS.
pub fn apply_window_theme(app: &AppHandle, theme: Theme) {
    let theme = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    for window in app.webview_windows().into_values() {
        if let Err(e) = window.set_theme(theme) {
            log::warn!("Couldn't set the window theme: {e}");
        }
    }
}

pub const MAIN: &str = "main";

/// Sent to every window after the settings or keybindings changed, so each reloads them.
const SETTINGS_CHANGED_EVENT: &str = "settings_changed";

pub fn announce_settings(app: &AppHandle) {
    if let Err(e) = app.emit(SETTINGS_CHANGED_EVENT, ()) {
        log::warn!("Couldn't announce the settings change: {e}");
    }
}

/// Closing a window hides it: the app keeps running until Quit (docs/adr/0037), and Settings reopens on its
/// last page. True when the close was taken over.
pub fn hide_instead_of_close(app: &AppHandle, label: &str) -> bool {
    match label {
        settings::LABEL => settings::hide(app),
        MAIN => hide_main(app),
        _ => return false,
    }
    true
}

pub fn hide_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN)
        && let Err(e) = window.hide()
    {
        log::warn!("Couldn't hide the window: {e}");
    }
}

pub fn main_visible(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

pub fn bring_to_front(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        focus(&window);
    }
}

/// Skipped with `DATABEE_BACKGROUND=1` in debug builds so scripts/dev never steal focus.
fn focus(window: &WebviewWindow) {
    #[cfg(debug_assertions)]
    if crate::dev::background() {
        return;
    }
    // Failing only means the window is gone; there is nothing left to bring forward.
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}
