use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::launch::LaunchQueue;
use crate::secrets::SecretStore;
use crate::session::Sessions;
use crate::store::favorites::FavoritesStore;
use crate::store::history::HistoryStore;
use crate::store::keybindings::KeybindingsStore;
use crate::store::recents::RecentsStore;
use crate::store::settings::SettingsStore;
use crate::store::snapshots::SnapshotStore;
use crate::store::ui_state::UiStateStore;
use crate::wp::context::WpContext;

pub struct AppState {
    pub settings: SettingsStore,
    pub keybindings: KeybindingsStore,
    pub recents: RecentsStore,
    pub favorites: FavoritesStore,
    pub history: HistoryStore,
    pub snapshots: SnapshotStore,
    pub ui_state: UiStateStore,
    pub launches: LaunchQueue,
    pub secrets: Box<dyn SecretStore>,
    pub sessions: Sessions,
    pub config_dir: PathBuf,
    /// Detected WordPress context per `session/database`.
    pub wp_cache: Mutex<HashMap<String, Option<WpContext>>>,
}

impl AppState {
    pub fn open(config_dir: &Path, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            settings: SettingsStore::open(config_dir),
            keybindings: KeybindingsStore::open(config_dir),
            recents: RecentsStore::open(config_dir),
            favorites: FavoritesStore::open(config_dir),
            history: HistoryStore::open(config_dir),
            snapshots: SnapshotStore::open(config_dir),
            ui_state: UiStateStore::open(config_dir),
            launches: LaunchQueue::default(),
            secrets,
            sessions: Sessions::default(),
            config_dir: config_dir.to_path_buf(),
            wp_cache: Mutex::default(),
        }
    }
}
