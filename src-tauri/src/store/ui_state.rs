//! Per-connection UI state (`ui_state.json`), keyed by connection identity
//! (`user@host:port/database`): column widths per table and pane sizes (all in px), and whether the
//! sidebar is collapsed, and the object view's collapsed section bands.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::json_store::JsonStore;
use crate::error::AppResult;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(default, rename_all = "camelCase")]
pub struct TableUi {
    pub widths: BTreeMap<String, u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub tables: BTreeMap<String, TableUi>,
    #[serde(alias = "splitWidth")]
    pub detail_pane_width: Option<u32>,
    #[serde(alias = "editorHeight")]
    pub sql_editor_height: Option<u32>,
    pub sidebar_hidden: bool,
    /// Collapsed object view groups by database, each as `<document table>::<group id>`.
    pub collapsed_groups: BTreeMap<String, Vec<String>>,
}

pub struct UiStateStore {
    store: JsonStore<BTreeMap<String, UiState>>,
}

impl UiStateStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open(dir.join("ui_state.json")),
        }
    }

    pub fn get(&self, identity: &str) -> UiState {
        self.store
            .read(|items| items.get(identity).cloned())
            .unwrap_or_default()
    }

    pub async fn set(&self, identity: &str, state: UiState) -> AppResult<UiState> {
        let identity = identity.to_string();
        self.store
            .update_async(move |items| {
                items.insert(identity, state.clone());
                state
            })
            .await
    }

    pub async fn forget(&self, identity: &str) -> AppResult<()> {
        let identity = identity.to_string();
        self.store
            .update_async(move |items| {
                items.remove(&identity);
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn widths_persist_per_connection_and_table() {
        let dir = tempfile::tempdir().unwrap();
        let store = UiStateStore::open(dir.path());
        let mut state = UiState::default();
        state.tables.insert(
            "wp_posts".into(),
            TableUi {
                widths: [("ID".to_string(), 80u32), ("post_title".to_string(), 400)]
                    .into_iter()
                    .collect(),
            },
        );
        state.detail_pane_width = Some(520);
        state.sql_editor_height = Some(90);
        store
            .set("root@127.0.0.1:32768/db", state.clone())
            .await
            .unwrap();

        let reopened = UiStateStore::open(dir.path());
        assert_eq!(reopened.get("root@127.0.0.1:32768/db"), state);
        assert_eq!(reopened.get("other@host:3306/db"), UiState::default());
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ui_state.json"),
            r#"{"a@b:1/db": {"tables": {"t": {}}}}"#,
        )
        .unwrap();
        let state = UiStateStore::open(dir.path()).get("a@b:1/db");
        assert!(state.tables["t"].widths.is_empty());
        assert_eq!(state.detail_pane_width, None);
    }

    #[test]
    fn old_pane_size_names_load_into_the_current_fields_and_retired_ones_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ui_state.json"),
            r#"{"a@b:1/db": {"splitWidth": 500, "paneHeight": 250, "inspectorWidth": 410, "editorHeight": 120}}"#,
        )
        .unwrap();
        let state = UiStateStore::open(dir.path()).get("a@b:1/db");
        assert_eq!(state.detail_pane_width, Some(500));
        assert_eq!(state.sql_editor_height, Some(120));
    }

    #[tokio::test]
    async fn hidden_sidebar_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = UiStateStore::open(dir.path());
        let state = UiState {
            sidebar_hidden: true,
            ..UiState::default()
        };
        store.set("a@b:1/db", state.clone()).await.unwrap();

        let reopened = UiStateStore::open(dir.path()).get("a@b:1/db");
        assert!(reopened.sidebar_hidden);
        assert_eq!(reopened, state);
    }

    #[test]
    fn file_without_sidebar_hidden_loads_as_visible_and_keeps_other_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ui_state.json"),
            r#"{"a@b:1/db": {"inspectorWidth": 410, "tables": {"t": {"widths": {"id": 60}}}}}"#,
        )
        .unwrap();
        let state = UiStateStore::open(dir.path()).get("a@b:1/db");
        assert!(!state.sidebar_hidden);
        assert_eq!(state.tables["t"].widths["id"], 60);
    }

    #[tokio::test]
    async fn collapsed_groups_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = UiStateStore::open(dir.path());
        let state = UiState {
            collapsed_groups: BTreeMap::from([
                (
                    "shop".to_string(),
                    vec!["wp_posts::meta".to_string(), "wp_posts::links".to_string()],
                ),
                ("blog".to_string(), vec![]),
            ]),
            ..UiState::default()
        };
        store.set("a@b:1/db", state.clone()).await.unwrap();

        let reopened = UiStateStore::open(dir.path()).get("a@b:1/db");
        assert_eq!(reopened.collapsed_groups, state.collapsed_groups);
        assert_eq!(reopened, state);
    }

    #[test]
    fn file_without_collapsed_groups_loads_empty_and_keeps_other_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ui_state.json"),
            r#"{"a@b:1/db": {"inspectorWidth": 410, "sidebarHidden": true, "tables": {"t": {"widths": {"id": 60}}}}}"#,
        )
        .unwrap();
        let state = UiStateStore::open(dir.path()).get("a@b:1/db");
        assert!(state.collapsed_groups.is_empty());
        assert!(state.sidebar_hidden);
        assert_eq!(state.tables["t"].widths["id"], 60);
    }

    #[tokio::test]
    async fn forget_removes_one_entry_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = UiStateStore::open(dir.path());
        let kept = UiState {
            detail_pane_width: Some(300),
            ..UiState::default()
        };
        let gone = UiState {
            detail_pane_width: Some(400),
            ..UiState::default()
        };
        store.set("keep", kept.clone()).await.unwrap();
        store.set("drop", gone).await.unwrap();
        store.forget("drop").await.unwrap();

        let reopened = UiStateStore::open(dir.path());
        assert_eq!(reopened.get("drop"), UiState::default());
        assert_eq!(reopened.get("keep"), kept);
    }
}
