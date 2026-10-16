use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use ts_rs::TS;

use super::json_file::JsonFile;
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// Across all accounts, not per account.
const MAX_ENTRIES: usize = 500;
const MAX_SQL_BYTES: usize = 64 * 1024;
const WRITE_DELAY: Duration = Duration::from_millis(500);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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

/// Kept in memory and written behind: `record` never waits for the disk.
pub struct HistoryStore {
    shared: Arc<Shared>,
}

struct Shared {
    file: JsonFile,
    items: Mutex<Vec<StoredEntry>>,
    /// Set after every change, cleared by the write that covers it.
    dirty: AtomicBool,
    /// One file write at a time, so an older copy never lands after a newer one.
    writing: Mutex<()>,
    changed: Notify,
}

impl HistoryStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("history.json"));
        let items = hash_old_entries(file.load());
        Self {
            shared: Arc::new(Shared {
                file,
                items: Mutex::new(items),
                dirty: AtomicBool::new(false),
                writing: Mutex::new(()),
                changed: Notify::new(),
            }),
        }
    }

    /// Writes the file at most once per `WRITE_DELAY` while there are unsaved entries. A failed
    /// write is logged and retried after the next change.
    pub fn start_writer(&self) {
        let shared = Arc::clone(&self.shared);
        tauri::async_runtime::spawn(async move {
            loop {
                shared.changed.notified().await;
                tokio::time::sleep(WRITE_DELAY).await;
                let writer = Arc::clone(&shared);
                let written = tokio::task::spawn_blocking(move || writer.flush())
                    .await
                    .unwrap_or_else(|e| Err(AppError::Io(e.to_string())));
                if let Err(e) = written {
                    log::warn!("Could not save the query history: {e}");
                }
            }
        });
    }

    /// Newest first, at most `MAX_ENTRIES` in total. Re-running the same SQL on the same
    /// account replaces its previous entry.
    pub fn record(&self, mut entry: HistoryEntry) {
        let hash = Some(sql_hash(&entry.sql));
        entry.truncated = truncate_sql(&mut entry.sql);
        self.shared.change(|items| {
            items.retain(|e| !(e.hash == hash && e.entry.account == entry.account));
            items.insert(0, StoredEntry { entry, hash });
            items.truncate(MAX_ENTRIES);
        });
    }

    pub fn list(&self, account: &str) -> Vec<HistoryEntry> {
        self.shared
            .items
            .lock_unpoisoned()
            .iter()
            .filter(|e| e.entry.account == account)
            .map(|e| e.entry.clone())
            .collect()
    }

    /// Saved right away, so cleared queries do not come back after a crash.
    pub async fn clear(&self, account: &str) -> AppResult<()> {
        self.shared
            .change(|items| items.retain(|e| e.entry.account != account));
        let shared = Arc::clone(&self.shared);
        tokio::task::spawn_blocking(move || shared.flush())
            .await
            .map_err(|e| AppError::Io(format!("Could not save the query history: {e}")))?
    }

    /// Writes the file now if there are unsaved changes; blocks on the disk.
    pub fn flush(&self) -> AppResult<()> {
        self.shared.flush()
    }
}

impl Shared {
    fn change(&self, f: impl FnOnce(&mut Vec<StoredEntry>)) {
        f(&mut self.items.lock_unpoisoned());
        self.dirty.store(true, Ordering::SeqCst);
        self.changed.notify_one();
    }

    /// Clears `dirty` before copying, so a change made during the write marks it again.
    fn flush(&self) -> AppResult<()> {
        let _writing = self.writing.lock_unpoisoned();
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        let items = self.items.lock_unpoisoned().clone();
        let saved = self.file.save(&items);
        if saved.is_err() {
            self.dirty.store(true, Ordering::SeqCst);
        }
        saved
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
        store.record(entry("a", "SELECT 1"));
        store.record(entry("a", "SELECT 2"));
        store.record(entry("a", "SELECT 1"));
        assert_eq!(sqls(store.list("a")), ["SELECT 1", "SELECT 2"]);
    }

    #[tokio::test]
    async fn history_is_per_server_and_clearable() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1"));
        store.record(entry("b", "SELECT 2"));
        store.clear("a").await.unwrap();
        assert!(store.list("a").is_empty());
        assert_eq!(sqls(HistoryStore::open(dir.path()).list("b")), ["SELECT 2"]);
    }

    #[test]
    fn recorded_entry_is_written_by_flush_and_seen_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1"));
        assert!(!dir.path().join("history.json").exists());

        store.flush().unwrap();

        assert_eq!(sqls(HistoryStore::open(dir.path()).list("a")), ["SELECT 1"]);
    }

    #[test]
    fn flush_without_changes_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());

        store.flush().unwrap();

        assert!(!dir.path().join("history.json").exists());
    }

    #[tokio::test]
    async fn clear_persists_an_empty_history_for_that_account() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1"));
        store.flush().unwrap();

        store.clear("a").await.unwrap();

        assert!(store.list("a").is_empty());
        assert!(HistoryStore::open(dir.path()).list("a").is_empty());
    }

    #[test]
    fn keeps_a_bounded_number_of_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        for i in 0..510 {
            store.record(entry("a", &format!("SELECT {i}")));
        }
        assert_eq!(store.list("a").len(), 500);
    }

    #[test]
    fn same_sql_on_different_accounts_is_kept_twice() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        store.record(entry("a", "SELECT 1"));
        store.record(entry("b", "SELECT 1"));
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
        store.record(failed.clone());
        store.flush().unwrap();
        assert_eq!(HistoryStore::open(dir.path()).list("a"), vec![failed]);
    }

    #[test]
    fn entry_cap_is_global_across_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(dir.path());
        for i in 0..300 {
            store.record(entry("a", &format!("SELECT {i}")));
        }
        for i in 0..300 {
            store.record(entry("b", &format!("SELECT {i}")));
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
        store.record(entry("a", &sql));
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
        store.record(entry("a", &sql));
        store.record(entry("a", &sql));
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

        store.record(entry("a", "SELECT 1"));
        let after = store.list("a");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].elapsed_ms, 1);
    }
}
