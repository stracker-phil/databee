//! The Settings window: a regular window of its own, built on first use and hidden on close, so it
//! reopens on the page it showed (docs/adr/0031).

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const LABEL: &str = "settings";
/// Sent to an open Settings window with the page or section to show.
pub const OPEN_SETTINGS_EVENT: &str = "open_settings";
/// Sent to the main page whenever Settings shows or hides (`true` while shown), for the Stage hook.
pub const SETTINGS_SHOWN_EVENT: &str = "settings_shown";

fn announce_shown(app: &AppHandle, shown: bool) {
    if let Err(e) = app.emit_to(super::MAIN, SETTINGS_SHOWN_EVENT, shown) {
        log::warn!(
            "Couldn't tell the page that Settings is {}: {e}",
            if shown { "shown" } else { "hidden" }
        );
    }
}

fn url(page: Option<&str>) -> WebviewUrl {
    let query = page.map(|page| format!("?page={page}")).unwrap_or_default();
    WebviewUrl::App(format!("src/windows/settings/index.html{query}").into())
}

fn build(app: &AppHandle, page: Option<&str>) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, LABEL, url(page))
        .title("Settings")
        .inner_size(880.0, 720.0)
        .min_inner_size(760.0, 560.0)
        .maximizable(false)
        .visible(false)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(20.0, 22.0))
        .build()
}

/// Shows Settings at `page` (a page or section id) when given, else where it was, and brings it forward.
pub fn open(app: &AppHandle, page: Option<&str>) {
    let window = match app.get_webview_window(LABEL) {
        Some(window) => {
            if let Some(page) = page
                && let Err(e) = window.emit_to(LABEL, OPEN_SETTINGS_EVENT, page)
            {
                log::warn!("Couldn't show the Settings page {page}: {e}");
            }
            window
        }
        None => match build(app, page) {
            Ok(window) => window,
            Err(e) => return log::warn!("Couldn't open Settings: {e}"),
        },
    };
    if let Err(e) = window.show() {
        log::warn!("Couldn't show Settings: {e}");
    }
    super::focus(&window);
    announce_shown(app, true);
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL)
        && let Err(e) = window.hide()
    {
        log::warn!("Couldn't hide Settings: {e}");
    }
    announce_shown(app, false);
    super::bring_to_front(app);
}
