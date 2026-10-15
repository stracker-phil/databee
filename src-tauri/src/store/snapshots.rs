use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::json_file::JsonFile;
use crate::error::{AppError, AppResult};
use crate::wp::snapshot::{Paths, Scope};

const MAX_PER_SERVER: usize = 30;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub label: String,
    pub account: String,
    pub database: String,
    pub scope: Scope,
    pub created: DateTime<Utc>,
    pub values: Paths,
}

/// A snapshot without its values, for lists.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub id: String,
    pub label: String,
    pub scope: Scope,
    pub created: DateTime<Utc>,
    pub paths: usize,
}

impl From<&Snapshot> for SnapshotInfo {
    fn from(s: &Snapshot) -> Self {
        Self {
            id: s.id.clone(),
            label: s.label.clone(),
            scope: s.scope.clone(),
            created: s.created,
            paths: s.values.len(),
        }
    }
}

pub struct SnapshotStore {
    file: JsonFile,
    items: Mutex<Vec<Snapshot>>,
}

impl SnapshotStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("snapshots.json"));
        let items = Mutex::new(file.load());
        Self { file, items }
    }

    /// Newest first, for one server and database.
    pub fn list(&self, account: &str, database: &str) -> Vec<SnapshotInfo> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.account == account && s.database == database)
            .map(SnapshotInfo::from)
            .collect()
    }

    pub fn get(&self, id: &str) -> AppResult<Snapshot> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput(format!("Snapshot {id} not found")))
    }

    pub fn add(&self, mut snapshot: Snapshot) -> AppResult<SnapshotInfo> {
        snapshot.id = uuid::Uuid::new_v4().to_string();
        let info = SnapshotInfo::from(&snapshot);
        let mut items = self.items.lock().unwrap();
        items.insert(0, snapshot);
        let account = items[0].account.clone();
        let mut kept = 0;
        items.retain(|s| {
            if s.account != account {
                return true;
            }
            kept += 1;
            kept <= MAX_PER_SERVER
        });
        self.file.save(&*items)?;
        Ok(info)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|s| s.id != id);
        self.file.save(&*items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(account: &str, label: &str) -> Snapshot {
        Snapshot {
            id: String::new(),
            label: label.into(),
            account: account.into(),
            database: "db".into(),
            scope: Scope {
                entity: Some("order".into()),
                id: Some("1".into()),
                include_related: false,
                option_pattern: None,
                include_actions: false,
            },
            created: Utc::now(),
            values: [("field/status".to_string(), "wc-pending".to_string())].into(),
        }
    }

    #[test]
    fn snapshots_round_trip_and_list_newest_first_per_server() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        store.add(snap("a", "first")).unwrap();
        let second = store.add(snap("a", "second")).unwrap();
        store.add(snap("b", "other")).unwrap();
        let reopened = SnapshotStore::open(dir.path());
        let labels: Vec<_> = reopened
            .list("a", "db")
            .into_iter()
            .map(|s| s.label)
            .collect();
        assert_eq!(labels, ["second", "first"]);
        assert_eq!(
            reopened.get(&second.id).unwrap().values["field/status"],
            "wc-pending"
        );
        reopened.delete(&second.id).unwrap();
        assert_eq!(reopened.list("a", "db").len(), 1);
    }

    #[test]
    fn keeps_a_bounded_number_per_server() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        for i in 0..35 {
            store.add(snap("a", &i.to_string())).unwrap();
        }
        assert_eq!(store.list("a", "db").len(), 30);
        assert_eq!(store.list("a", "db")[0].label, "34");
    }
}
