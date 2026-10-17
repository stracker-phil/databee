use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::launch::LaunchQueue;
use crate::rules::{PresetCatalog, ProjectsStore};
use crate::secrets::SecretStore;
use crate::session::Sessions;
use crate::store::favorites::FavoritesStore;
use crate::store::history::HistoryStore;
use crate::store::keybindings::KeybindingsStore;
use crate::store::recents::RecentsStore;
use crate::store::settings::{LogLevel, SettingsStore};
use crate::store::ui_state::UiStateStore;

pub struct AppState {
    pub settings: SettingsStore,
    pub keybindings: KeybindingsStore,
    pub recents: RecentsStore,
    pub favorites: FavoritesStore,
    pub history: HistoryStore,
    pub ui_state: UiStateStore,
    pub presets: PresetCatalog,
    pub projects: ProjectsStore,
    pub launches: LaunchQueue,
    /// Shared so Keychain calls can run in blocking tasks.
    pub secrets: Arc<dyn SecretStore>,
    pub sessions: Sessions,
    pub config_dir: PathBuf,
    /// `--log-level` from the CLI; wins over the stored level until the app quits.
    pub log_override: Option<LogLevel>,
}

impl AppState {
    pub fn open(config_dir: &Path, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            settings: SettingsStore::open(config_dir),
            keybindings: KeybindingsStore::open(config_dir),
            recents: RecentsStore::open(config_dir),
            favorites: FavoritesStore::open(config_dir),
            history: HistoryStore::open(config_dir),
            ui_state: UiStateStore::open(config_dir),
            presets: PresetCatalog::open(config_dir),
            projects: ProjectsStore::open(config_dir),
            launches: LaunchQueue::default(),
            secrets,
            sessions: Sessions::default(),
            config_dir: config_dir.to_path_buf(),
            log_override: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::secrets::MemoryStore;
    use crate::store::settings::Settings;

    #[test]
    fn opening_an_empty_dir_starts_with_default_settings_and_no_sessions() {
        let dir = tempfile::tempdir().unwrap();

        let state = AppState::open(dir.path(), Arc::new(MemoryStore::default()));

        assert_eq!(state.settings.get(), Settings::default());
        assert_eq!(state.config_dir, dir.path());
        assert!(state.log_override.is_none());
        assert_eq!(
            state.sessions.get("any").err().unwrap().kind(),
            ErrorKind::SessionNotFound
        );
    }

    #[test]
    fn a_corrupt_settings_file_is_quarantined_and_settings_are_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), "{ not json").unwrap();

        let state = AppState::open(dir.path(), Arc::new(MemoryStore::default()));

        assert_eq!(state.settings.get(), Settings::default());
        assert!(!dir.path().join("settings.json").exists());
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                let name = p.file_name().unwrap().to_string_lossy();
                name.starts_with("settings.json.") && name.ends_with(".corrupt")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), "{ not json");
    }
}
