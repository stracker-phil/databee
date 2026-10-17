use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::launch::LaunchEvent;
use crate::menu;
use crate::state::AppState;
use crate::store::favorites::{Favorite, NewFavorite};
use crate::store::history::HistoryEntry;
use crate::store::keybindings::Keybindings;
use crate::store::recents::RecentConnection;
use crate::store::settings::{LogLevel, Settings, Theme};
use crate::store::ui_state::UiState;
use crate::window;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// What a settings change has to apply outside the store.
#[derive(Debug, PartialEq)]
struct SettingsEffects {
    theme: Option<Theme>,
    log_level: Option<log::LevelFilter>,
}

/// Without `previous` (at startup) everything applies. A `--log-level` from the CLI wins over
/// the stored level for the app's lifetime.
fn settings_effects(
    previous: Option<&Settings>,
    saved: &Settings,
    log_override: Option<LogLevel>,
) -> SettingsEffects {
    let startup = previous.is_none();
    let log_level = match log_override {
        Some(cli_level) => startup.then_some(cli_level),
        None => previous
            .is_none_or(|p| p.log_level != saved.log_level)
            .then_some(saved.log_level),
    };
    SettingsEffects {
        theme: previous
            .is_none_or(|p| p.theme != saved.theme)
            .then_some(saved.theme),
        log_level: log_level.map(Into::into),
    }
}

pub fn apply_settings(app: &AppHandle, previous: Option<&Settings>, saved: &Settings) {
    let effects = settings_effects(previous, saved, app.state::<AppState>().log_override);
    if let Some(theme) = effects.theme {
        window::apply_window_theme(app, theme);
    }
    if let Some(level) = effects.log_level {
        log::set_max_level(level);
    }
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    let (previous, saved) = state.settings.update(settings).await?;
    apply_settings(&app, Some(&previous), &saved);
    log::info!("Settings updated");
    Ok(saved)
}

#[tauri::command]
pub fn get_keybindings(state: State<'_, AppState>) -> Keybindings {
    state.keybindings.get()
}

#[tauri::command]
pub async fn set_keybindings(
    app: AppHandle,
    state: State<'_, AppState>,
    bindings: Keybindings,
) -> AppResult<Keybindings> {
    let saved = state.keybindings.update(bindings).await?;
    menu::rebuild(&app);
    Ok(saved)
}

#[tauri::command]
pub fn get_ui_state(state: State<'_, AppState>, identity: String) -> UiState {
    state.ui_state.get(&identity)
}

#[tauri::command]
pub async fn set_ui_state(
    state: State<'_, AppState>,
    identity: String,
    ui: UiState,
) -> AppResult<UiState> {
    state.ui_state.set(&identity, ui).await
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
/// http(s) only, and without credentials, which the browser would send on.
fn is_web_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https") && u.username().is_empty() && u.password().is_none()
    })
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
pub async fn remove_recent(state: State<'_, AppState>, identity: String) -> AppResult<()> {
    state.recents.remove(&identity).await?;
    state.ui_state.forget(&identity).await
}

/// Renames the saved connection's slug; open sessions of it take the new slug right away and
/// its projects move along.
#[tauri::command]
pub async fn rename_connection_slug(
    state: State<'_, AppState>,
    identity: String,
    slug: String,
) -> AppResult<()> {
    let old = state.recents.rename_slug(&identity, &slug).await?;
    state.sessions.rename_slug(&identity, &slug);
    state.projects.rename_connection(&old, &slug).await
}

#[tauri::command]
pub fn list_favorites(state: State<'_, AppState>, account: String) -> Vec<Favorite> {
    state.favorites.list(&account)
}

#[tauri::command]
pub async fn create_favorite(
    state: State<'_, AppState>,
    favorite: NewFavorite,
) -> AppResult<Favorite> {
    state.favorites.create(favorite).await
}

#[tauri::command]
pub async fn update_favorite(
    state: State<'_, AppState>,
    favorite: Favorite,
) -> AppResult<Favorite> {
    state.favorites.update(favorite).await
}

#[tauri::command]
pub async fn remove_favorite(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.favorites.remove(&id).await
}

#[tauri::command]
pub async fn reorder_favorites(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<()> {
    state.favorites.reorder(ids).await
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>, account: String) -> Vec<HistoryEntry> {
    state.history.list(&account)
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>, account: String) -> AppResult<()> {
    state.history.clear(&account).await
}

#[tauri::command]
pub fn log_frontend_error(message: String) {
    log::error!(target: "frontend", "{}", sanitize_log_text(&message));
}

const MAX_LOGGED_BYTES: usize = 4096;

/// Bounds page-supplied text and escapes line breaks, so it cannot flood the log or
/// forge log lines.
fn sanitize_log_text(text: &str) -> String {
    text[..text.floor_char_boundary(MAX_LOGGED_BYTES)]
        .replace('\n', "\\n")
        .replace('\r', "\\r")
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

    #[test]
    fn web_url_rules_cover_scheme_case_and_credentials() {
        for (url, allowed) in [
            ("mailto:a@b.c", false),
            ("HTTPS://example.com", true),
            ("https://user:pw@example.com", false),
            ("https://user@example.com", false),
        ] {
            assert_eq!(is_web_url(url), allowed, "{url}");
        }
    }

    #[test]
    fn logged_text_is_cut_on_a_char_boundary() {
        let text = format!("{}é", "a".repeat(MAX_LOGGED_BYTES - 1));
        let cut = sanitize_log_text(&text);
        assert_eq!(cut, "a".repeat(MAX_LOGGED_BYTES - 1));
        assert!(sanitize_log_text(&"x".repeat(10_000)).len() <= MAX_LOGGED_BYTES);
    }

    #[test]
    fn logged_text_escapes_line_breaks() {
        assert_eq!(sanitize_log_text("a\nb\rc"), "a\\nb\\rc");
    }

    fn with_theme(theme: Theme) -> Settings {
        Settings {
            theme,
            ..Settings::default()
        }
    }

    #[test]
    fn changed_theme_is_applied() {
        let previous = with_theme(Theme::Light);
        let saved = with_theme(Theme::Dark);
        let effects = settings_effects(Some(&previous), &saved, None);
        assert_eq!(
            effects,
            SettingsEffects {
                theme: Some(Theme::Dark),
                log_level: None,
            }
        );
    }

    #[test]
    fn unchanged_settings_apply_nothing() {
        let settings = Settings::default();
        let effects = settings_effects(Some(&settings), &settings, None);
        assert_eq!(
            effects,
            SettingsEffects {
                theme: None,
                log_level: None,
            }
        );
    }

    #[test]
    fn changed_log_level_is_applied() {
        let previous = Settings::default();
        let saved = Settings {
            log_level: LogLevel::Debug,
            ..Settings::default()
        };
        let effects = settings_effects(Some(&previous), &saved, None);
        assert_eq!(effects.log_level, Some(log::LevelFilter::Debug));
    }

    #[test]
    fn cli_log_override_beats_a_changed_log_level() {
        let previous = Settings::default();
        let saved = Settings {
            log_level: LogLevel::Trace,
            ..Settings::default()
        };
        let effects = settings_effects(Some(&previous), &saved, Some(LogLevel::Error));
        assert_eq!(effects.log_level, None);
    }

    #[test]
    fn cli_log_override_is_applied_at_startup() {
        let saved = Settings {
            log_level: LogLevel::Trace,
            ..Settings::default()
        };
        let effects = settings_effects(None, &saved, Some(LogLevel::Error));
        assert_eq!(effects.log_level, Some(log::LevelFilter::Error));
    }

    #[test]
    fn startup_without_override_applies_everything() {
        let saved = Settings {
            theme: Theme::Dark,
            log_level: LogLevel::Warn,
            ..Settings::default()
        };
        assert_eq!(
            settings_effects(None, &saved, None),
            SettingsEffects {
                theme: Some(Theme::Dark),
                log_level: Some(log::LevelFilter::Warn),
            }
        );
    }
}
