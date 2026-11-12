//! What Settings → Status shows: the build, where DataBee keeps its files, the log level in effect and the open
//! connections with their health.

use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::health::Health;
use crate::state::AppState;
use crate::store::settings::LogLevel;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: String,
    /// `release`, or `debug` for the separate debug app on demo data.
    pub build: String,
    /// With the home folder as `~`.
    pub data_folder: String,
    pub log_folder: String,
    pub log_level: LogLevel,
    /// The level comes from `--log-level` for this run, not from Settings.
    pub log_level_from_cli: bool,
    pub connections: Vec<ConnectionStatus>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub slug: String,
    pub account: String,
    pub database: Option<String>,
    pub server_version: String,
    pub health: Health,
}

fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn status(state: &AppState, log_folder: &Path, home: Option<&Path>) -> AppStatus {
    let settings = state.settings.get();
    let mut connections: Vec<ConnectionStatus> = state
        .sessions
        .all()
        .iter()
        .map(|session| {
            let info = session.info();
            ConnectionStatus {
                slug: info.slug,
                account: info.account,
                database: info.database,
                server_version: info.server_version,
                health: session.health.get(),
            }
        })
        .collect();
    connections.sort_by(|a, b| (&a.slug, &a.database).cmp(&(&b.slug, &b.database)));
    AppStatus {
        version: env!("CARGO_PKG_VERSION").into(),
        build: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        data_folder: tilde(&state.config_dir, home),
        log_folder: tilde(log_folder, home),
        log_level: state.log_override.unwrap_or(settings.log_level),
        log_level_from_cli: state.log_override.is_some(),
        connections,
    }
}

#[tauri::command]
pub fn app_status(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppStatus> {
    let log_folder = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(status(&state, &log_folder, std::env::home_dir().as_deref()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;
    use crate::secrets::MemoryStore;
    use crate::store::settings::Settings;

    fn open_state(dir: &Path) -> AppState {
        AppState::open(dir, Arc::new(MemoryStore::default()))
    }

    #[test]
    fn data_folder_is_the_full_path_without_a_home() {
        let dir = tempfile::tempdir().unwrap();

        let report = status(&open_state(dir.path()), dir.path(), None);

        assert_eq!(report.data_folder, dir.path().display().to_string());
    }

    #[test]
    fn data_folder_equal_to_home_is_a_tilde() {
        let dir = tempfile::tempdir().unwrap();

        let report = status(&open_state(dir.path()), dir.path(), Some(dir.path()));

        assert_eq!(report.data_folder, "~");
    }

    #[test]
    fn data_folder_outside_home_with_a_shared_string_prefix_stays_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let full = dir.path().display().to_string();
        let home = PathBuf::from(&full[..full.len() - 1]);

        let report = status(&open_state(dir.path()), dir.path(), Some(&home));

        assert_eq!(report.data_folder, full);
    }

    #[test]
    fn status_reports_build_folders_and_default_log_level() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().parent().unwrap();
        let name = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let logs: PathBuf = dir.path().join("logs");

        let report = status(&open_state(dir.path()), &logs, Some(home));

        assert_eq!(
            report,
            AppStatus {
                version: env!("CARGO_PKG_VERSION").into(),
                build: "debug".into(),
                data_folder: format!("~/{name}"),
                log_folder: format!("~/{name}/logs"),
                log_level: LogLevel::Info,
                log_level_from_cli: false,
                connections: vec![],
            }
        );
    }

    #[tokio::test]
    async fn status_reports_the_level_from_settings() {
        let dir = tempfile::tempdir().unwrap();
        let state = open_state(dir.path());
        let settings = Settings {
            log_level: LogLevel::Debug,
            ..Settings::default()
        };
        state.settings.update(settings).await.unwrap();

        let report = status(&state, dir.path(), None);

        assert_eq!(report.log_level, LogLevel::Debug);
        assert!(!report.log_level_from_cli);
    }

    #[tokio::test]
    async fn cli_log_level_wins_over_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = open_state(dir.path());
        state
            .settings
            .update(Settings {
                log_level: LogLevel::Debug,
                ..Settings::default()
            })
            .await
            .unwrap();
        state.log_override = Some(LogLevel::Trace);

        let report = status(&state, dir.path(), None);

        assert_eq!(report.log_level, LogLevel::Trace);
        assert!(report.log_level_from_cli);
    }
}
