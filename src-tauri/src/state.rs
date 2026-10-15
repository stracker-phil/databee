use std::path::Path;

use crate::launch::LaunchQueue;
use crate::secrets::SecretStore;
use crate::session::Sessions;
use crate::store::recents::RecentsStore;
use crate::store::settings::SettingsStore;

pub struct AppState {
    pub settings: SettingsStore,
    pub recents: RecentsStore,
    pub launches: LaunchQueue,
    pub secrets: Box<dyn SecretStore>,
    pub sessions: Sessions,
}

impl AppState {
    pub fn open(config_dir: &Path, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            settings: SettingsStore::open(config_dir),
            recents: RecentsStore::open(config_dir),
            launches: LaunchQueue::default(),
            secrets,
            sessions: Sessions::default(),
        }
    }
}
