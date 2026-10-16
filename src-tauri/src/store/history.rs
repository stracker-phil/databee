use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::json_store::JsonStore;
use crate::error::AppResult;

const MAX_ENTRIES: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub account: String,
    pub database: Option<String>,
    pub sql: String,
    pub executed_at: DateTime<Utc>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

pub struct HistoryStore {
    store: JsonStore<Vec<HistoryEntry>>,
}

impl HistoryStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open(dir.join("history.json")),
        }
    }

    /// Newest first. Re-running the same SQL replaces its previous entry.
    pub fn record(&self, entry: HistoryEntry) -> AppResult<()> {
        self.store.update(|items| {
            items.retain(|e| !(e.account == entry.account && e.sql == entry.sql));
            items.insert(0, entry);
            items.truncate(MAX_ENTRIES);
        })
    }

    pub fn list(&self, account: &str) -> Vec<HistoryEntry> {
        self.store.read(|items| {
            items
                .iter()
                .filter(|e| e.account == account)
                .cloned()
                .collect()
        })
    }

    pub fn clear(&self, account: &str) -> AppResult<()> {
        self.store
            .update(|items| items.retain(|e| e.account != account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(account: &str, sql: &str) -> HistoryEntry {
        HistoryEntry {
            account: account.into(),
            database: None,
            sql: sql.into(),
            executed_at: Utc::now(),
            elapsed_ms: 1,
            error: None,
        }
    }

    fn sqls(list: Vec<HistoryEntry>) -> Vec<String> {
        list.into_iter().map(|e| e.sql).collect()
    }

    #[test]
    fn newest_first_and_rerun_moves_to_top() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1")).unwrap();
        store.record(entry("a", "SELECT 2")).unwrap();
        store.record(entry("a", "SELECT 1")).unwrap();
        assert_eq!(sqls(store.list("a")), ["SELECT 1", "SELECT 2"]);
    }

    #[test]
    fn history_is_per_server_and_clearable() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1")).unwrap();
        store.record(entry("b", "SELECT 2")).unwrap();
        store.clear("a").unwrap();
        assert!(store.list("a").is_empty());
        assert_eq!(sqls(HistoryStore::open(dir.path()).list("b")), ["SELECT 2"]);
    }

    #[test]
    fn keeps_a_bounded_number_of_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        for i in 0..510 {
            store.record(entry("a", &format!("SELECT {i}"))).unwrap();
        }
        assert_eq!(store.list("a").len(), 500);
    }
}
