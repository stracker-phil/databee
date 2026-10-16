use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::launch::LaunchQueue;
use crate::lock::MutexExt;
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
    /// Detected WordPress context per `session/database`. Databases that are not WordPress
    /// are not cached, so one that becomes WordPress is detected on the next call.
    pub wp_cache: Mutex<HashMap<String, WpContext>>,
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

    /// Forgets the detected contexts of all databases of the session.
    pub fn invalidate_wp(&self, session_id: &str) {
        let prefix = wp_cache_key(session_id, "");
        self.wp_cache
            .lock_unpoisoned()
            .retain(|key, _| !key.starts_with(&prefix));
    }
}

pub fn wp_cache_key(session_id: &str, database: &str) -> String {
    format!("{session_id}/{database}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;

    fn context() -> WpContext {
        WpContext {
            version: None,
            prefix: "wp_".into(),
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: Default::default(),
        }
    }

    #[test]
    fn invalidating_a_session_keeps_the_contexts_of_other_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Box::new(MemoryStore::default()));
        let keys = [
            wp_cache_key("s1", "wp_a"),
            wp_cache_key("s1", "wp_b"),
            wp_cache_key("s2", "wp_a"),
            wp_cache_key("s10", "wp_a"),
        ];
        for key in &keys {
            state
                .wp_cache
                .lock_unpoisoned()
                .insert(key.clone(), context());
        }

        state.invalidate_wp("s1");

        let mut left: Vec<String> = state.wp_cache.lock_unpoisoned().keys().cloned().collect();
        left.sort();
        assert_eq!(left, ["s10/wp_a", "s2/wp_a"]);
    }
}
