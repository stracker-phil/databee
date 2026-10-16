use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::json_store::JsonStore;
use crate::error::AppResult;

/// Across all accounts, not per account.
const MAX_ENTRIES: usize = 500;
const MAX_SQL_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub account: String,
    pub database: Option<String>,
    pub sql: String,
    /// Set by `record` when `sql` was cut to `MAX_SQL_BYTES`.
    #[serde(default)]
    pub truncated: bool,
    pub executed_at: DateTime<Utc>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct StoredEntry {
    #[serde(flatten)]
    entry: HistoryEntry,
    /// Of the full SQL, so a rerun of a truncated statement replaces its entry. `None` in
    /// files written before the hash existed.
    #[serde(default)]
    hash: Option<u64>,
}

pub struct HistoryStore {
    store: JsonStore<Vec<StoredEntry>>,
}

impl HistoryStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open_with(dir.join("history.json"), hash_old_entries),
        }
    }

    /// Newest first, at most `MAX_ENTRIES` in total. Re-running the same SQL on the same
    /// account replaces its previous entry.
    pub fn record(&self, mut entry: HistoryEntry) -> AppResult<()> {
        let hash = Some(sql_hash(&entry.sql));
        entry.truncated = truncate_sql(&mut entry.sql);
        self.store.update(|items| {
            items.retain(|e| !(e.hash == hash && e.entry.account == entry.account));
            items.insert(0, StoredEntry { entry, hash });
            items.truncate(MAX_ENTRIES);
        })
    }

    pub fn list(&self, account: &str) -> Vec<HistoryEntry> {
        self.store.read(|items| {
            items
                .iter()
                .filter(|e| e.entry.account == account)
                .map(|e| e.entry.clone())
                .collect()
        })
    }

    pub fn clear(&self, account: &str) -> AppResult<()> {
        self.store
            .update(|items| items.retain(|e| e.entry.account != account))
    }
}

/// Entries without a hash predate truncation, so their stored SQL is complete.
fn hash_old_entries(mut items: Vec<StoredEntry>) -> Vec<StoredEntry> {
    for item in items.iter_mut().filter(|e| e.hash.is_none()) {
        item.hash = Some(sql_hash(&item.entry.sql));
    }
    items
}

/// Stable across Rust releases, unlike `DefaultHasher`, so stored hashes stay valid.
fn sql_hash(sql: &str) -> u64 {
    let digest = Sha256::digest(sql.as_bytes());
    u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 has 32 bytes"))
}

fn truncate_sql(sql: &mut String) -> bool {
    if sql.len() <= MAX_SQL_BYTES {
        return false;
    }
    let mut end = MAX_SQL_BYTES;
    while !sql.is_char_boundary(end) {
        end -= 1;
    }
    sql.truncate(end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(account: &str, sql: &str) -> HistoryEntry {
        HistoryEntry {
            account: account.into(),
            database: None,
            sql: sql.into(),
            truncated: false,
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

    #[test]
    fn same_sql_on_different_accounts_is_kept_twice() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1")).unwrap();
        store.record(entry("b", "SELECT 1")).unwrap();
        assert_eq!(sqls(store.list("a")), ["SELECT 1"]);
        assert_eq!(sqls(store.list("b")), ["SELECT 1"]);
    }

    #[test]
    fn failed_statement_keeps_its_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        let failed = HistoryEntry {
            error: Some("Unknown column".into()),
            ..entry("a", "SELECT nope")
        };
        store.record(failed.clone()).unwrap();
        assert_eq!(HistoryStore::open(dir.path()).list("a"), vec![failed]);
    }

    #[test]
    fn entry_cap_is_global_across_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        for i in 0..300 {
            store.record(entry("a", &format!("SELECT {i}"))).unwrap();
        }
        for i in 0..300 {
            store.record(entry("b", &format!("SELECT {i}"))).unwrap();
        }
        let (a, b) = (store.list("a"), store.list("b"));
        assert_eq!(a.len() + b.len(), 500);
        assert_eq!(b.len(), 300);
        assert_eq!(a.len(), 200);
        assert_eq!(a[0].sql, "SELECT 299");
        assert_eq!(a[199].sql, "SELECT 100");
    }

    fn long_sql() -> String {
        let mut sql = String::from("SELECT '");
        while sql.len() < 70 * 1024 {
            sql.push_str("aé€");
        }
        sql
    }

    #[test]
    fn over_long_sql_is_cut_on_a_char_boundary_and_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        let sql = long_sql();
        store.record(entry("a", &sql)).unwrap();
        let stored = &store.list("a")[0];
        assert!(stored.sql.len() <= MAX_SQL_BYTES);
        assert!(stored.truncated);
        assert!(sql.starts_with(&stored.sql));
    }

    #[test]
    fn rerunning_an_over_long_sql_replaces_its_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        let sql = long_sql();
        store.record(entry("a", &sql)).unwrap();
        store.record(entry("a", &sql)).unwrap();
        assert_eq!(store.list("a").len(), 1);
    }

    #[test]
    fn old_file_without_hash_loads_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("history.json"),
            r#"[{"account":"a","database":null,"sql":"SELECT 1","executedAt":"2024-01-01T00:00:00Z","elapsedMs":3,"error":null}]"#,
        )
        .unwrap();
        let store = HistoryStore::open(dir.path());
        let loaded = store.list("a");
        assert_eq!(sqls(loaded.clone()), ["SELECT 1"]);
        assert!(!loaded[0].truncated);

        store.record(entry("a", "SELECT 1")).unwrap();
        let after = store.list("a");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].elapsed_ms, 1);
    }
}
