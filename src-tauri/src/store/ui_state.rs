//! Per-connection UI state (`ui_state.json`), keyed by connection identity
//! (`user@host:port/database`): column widths per table, split-view width.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::json_file::JsonFile;
use crate::error::AppResult;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TableUi {
    /// Column widths in px.
    pub widths: BTreeMap<String, u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub tables: BTreeMap<String, TableUi>,
    pub split_width: Option<u32>,
}

pub struct UiStateStore {
    file: JsonFile,
    items: Mutex<BTreeMap<String, UiState>>,
}

impl UiStateStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("ui_state.json"));
        let items = Mutex::new(file.load());
        Self { file, items }
    }

    pub fn get(&self, identity: &str) -> UiState {
        self.items
            .lock()
            .unwrap()
            .get(identity)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set(&self, identity: &str, state: UiState) -> AppResult<UiState> {
        let mut items = self.items.lock().unwrap();
        items.insert(identity.to_string(), state.clone());
        self.file.save(&*items)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_persist_per_connection_and_table() {
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
        state.split_width = Some(520);
        store.set("root@127.0.0.1:52958/db", state.clone()).unwrap();

        let reopened = UiStateStore::open(dir.path());
        assert_eq!(reopened.get("root@127.0.0.1:52958/db"), state);
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
        assert_eq!(state.split_width, None);
    }
}
