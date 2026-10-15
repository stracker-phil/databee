use std::path::Path;

use crate::store::settings::SettingsStore;

pub struct AppState {
    pub settings: SettingsStore,
}

impl AppState {
    pub fn open(config_dir: &Path) -> Self {
        Self {
            settings: SettingsStore::open(config_dir),
        }
    }
}
