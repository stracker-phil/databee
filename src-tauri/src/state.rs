use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::{Connector, MySqlConnector};
use crate::error::AppResult;
use crate::launch::LaunchQueue;
use crate::rules::effective::RuleSources;
use crate::rules::{PresetCatalog, ProjectsStore};
use crate::saved::migrate::migrate_favorites;
use crate::secrets::SecretStore;
use crate::session::Sessions;
use crate::store::history::HistoryStore;
use crate::store::keybindings::KeybindingsStore;
use crate::store::recent_objects::RecentObjectsStore;
use crate::store::recents::RecentsStore;
use crate::store::settings::{LogLevel, SettingsStore};
use crate::store::snapshots::SnapshotStore;
use crate::store::ui_state::UiStateStore;

pub struct AppState {
    pub settings: SettingsStore,
    pub keybindings: KeybindingsStore,
    pub recents: RecentsStore,
    pub history: HistoryStore,
    pub ui_state: UiStateStore,
    pub presets: PresetCatalog,
    pub projects: ProjectsStore,
    pub recent_objects: RecentObjectsStore,
    pub snapshots: SnapshotStore,
    pub launches: LaunchQueue,
    /// Shared so Keychain calls can run in blocking tasks.
    pub secrets: Arc<dyn SecretStore>,
    pub sessions: Sessions,
    pub connector: Arc<dyn Connector>,
    pub config_dir: PathBuf,
    /// `--log-level` from the CLI; wins over the stored level until the app quits.
    pub log_override: Option<LogLevel>,
}

impl AppState {
    pub fn open(config_dir: &Path, secrets: Arc<dyn SecretStore>) -> Self {
        if let Err(e) = migrate_favorites(config_dir) {
            log::warn!("Favorites were not moved to saved queries: {e}");
        }
        Self {
            settings: SettingsStore::open(config_dir),
            keybindings: KeybindingsStore::open(config_dir),
            recents: RecentsStore::open(config_dir),
            history: HistoryStore::open(config_dir),
            ui_state: UiStateStore::open(config_dir),
            presets: PresetCatalog::open(config_dir),
            projects: ProjectsStore::open(config_dir),
            recent_objects: RecentObjectsStore::open(config_dir),
            snapshots: SnapshotStore::open(config_dir),
            launches: LaunchQueue::default(),
            secrets,
            sessions: Sessions::default(),
            connector: Arc::new(MySqlConnector),
            config_dir: config_dir.to_path_buf(),
            log_override: None,
        }
    }

    /// The presets and projects the rules lens reads.
    pub fn rules(&self) -> RuleSources<'_> {
        RuleSources {
            presets: &self.presets,
            projects: &self.projects,
        }
    }

    /// Renames the saved connection's slug; open sessions of it take the new slug right away and
    /// its projects, recent objects and snapshots move along. Only the saved connection must
    /// succeed: a store that fails to move keeps its entries under the old slug, logged.
    pub async fn rename_connection(&self, identity: &str, slug: &str) -> AppResult<()> {
        let old = self.recents.rename_slug(identity, slug).await?;
        self.sessions.rename_slug(identity, slug);
        let moves = [
            (
                "projects",
                self.projects.rename_connection(&old, slug).await,
            ),
            (
                "recent objects",
                self.recent_objects.rename_connection(&old, slug).await,
            ),
            (
                "snapshots",
                self.snapshots.rename_connection(&old, slug).await,
            ),
        ];
        for (store, moved) in moves {
            if let Err(e) = moved {
                log::warn!("The {store} of `{old}` were not moved to `{slug}`: {e}");
            }
        }
        Ok(())
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

    fn spec(database: &str) -> crate::connection::ConnectionSpec {
        crate::connection::ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: Some(database.into()),
        }
    }

    async fn saved_with_recent_object(state: &AppState, database: &str) -> String {
        let slug = state.recents.touch(&spec(database)).await.unwrap();
        state
            .recent_objects
            .remember(
                &slug,
                database,
                format!("databee://{slug}/{database}/t/1"),
                0,
            )
            .await
            .unwrap();
        slug
    }

    fn open_state(dir: &Path) -> AppState {
        AppState::open(dir, Arc::new(MemoryStore::default()))
    }

    #[tokio::test]
    async fn renaming_a_connection_moves_its_slug_sessions_and_recent_objects() {
        let dir = tempfile::tempdir().unwrap();
        let state = open_state(dir.path());
        let old = saved_with_recent_object(&state, "wp").await;
        let session = state.sessions.insert(crate::session::Session::new(
            "a".into(),
            spec("wp"),
            old.clone(),
            Box::new(crate::db::FakeDatabase::new()),
        ));

        state
            .rename_connection(&spec("wp").identity(), "mine")
            .await
            .unwrap();

        assert_eq!(state.recents.list()[0].slug, "mine");
        assert_eq!(session.slug(), "mine");
        assert_eq!(state.recent_objects.get("mine", "wp").len(), 1);
        assert!(state.recent_objects.get(&old, "wp").is_empty());
    }

    #[tokio::test]
    async fn a_refused_rename_leaves_every_store_under_the_old_slug() {
        let dir = tempfile::tempdir().unwrap();
        let state = open_state(dir.path());
        let old = saved_with_recent_object(&state, "wp").await;
        let taken = saved_with_recent_object(&state, "shop").await;

        let err = state
            .rename_connection(&spec("wp").identity(), &taken)
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(state.recent_objects.get(&old, "wp").len(), 1);
        assert!(state.recent_objects.get(&taken, "wp").is_empty());
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_written_does_not_stop_the_rename() {
        let dir = tempfile::tempdir().unwrap();
        let state = open_state(dir.path());
        saved_with_recent_object(&state, "wp").await;
        std::fs::create_dir(dir.path().join("projects.json")).unwrap();

        state
            .rename_connection(&spec("wp").identity(), "mine")
            .await
            .unwrap();

        assert_eq!(state.recents.list()[0].slug, "mine");
        assert_eq!(state.recent_objects.get("mine", "wp").len(), 1);
    }
}
