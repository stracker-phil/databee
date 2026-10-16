use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::json_file::JsonFile;
use super::json_store::JsonStore;
use crate::error::{AppError, AppResult};
use crate::wp::snapshot::{Paths, Scope};

const MAX_PER_DATABASE: usize = 30;
const INDEX_FILE: &str = "snapshots.json";
const LEGACY_COPY: &str = "snapshots.json.legacy";
const VALUES_DIR: &str = "snapshots";

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
#[derive(Serialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub id: String,
    pub label: String,
    pub scope: Scope,
    pub created: DateTime<Utc>,
    pub paths: usize,
}

/// One entry of `snapshots.json`; the values live in `snapshots/<id>.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SnapshotMeta {
    id: String,
    label: String,
    account: String,
    database: String,
    scope: Scope,
    created: DateTime<Utc>,
    paths: usize,
}

impl SnapshotMeta {
    fn of(s: &Snapshot) -> Self {
        Self {
            id: s.id.clone(),
            label: s.label.clone(),
            account: s.account.clone(),
            database: s.database.clone(),
            scope: s.scope.clone(),
            created: s.created,
            paths: s.values.len(),
        }
    }

    fn with_values(self, values: Paths) -> Snapshot {
        Snapshot {
            id: self.id,
            label: self.label,
            account: self.account,
            database: self.database,
            scope: self.scope,
            created: self.created,
            values,
        }
    }
}

impl From<&SnapshotMeta> for SnapshotInfo {
    fn from(s: &SnapshotMeta) -> Self {
        Self {
            id: s.id.clone(),
            label: s.label.clone(),
            scope: s.scope.clone(),
            created: s.created,
            paths: s.paths,
        }
    }
}

/// `snapshots.json` is the index; older versions stored every snapshot with its values in it.
/// An index entry has `paths`, a legacy entry does not.
#[derive(Deserialize)]
#[serde(untagged)]
enum IndexFile {
    Index(Vec<SnapshotMeta>),
    Legacy(Vec<Snapshot>),
}

impl Default for IndexFile {
    fn default() -> Self {
        Self::Index(Vec::new())
    }
}

/// Snapshot metadata in `snapshots.json`, each snapshot's values in `snapshots/<id>.json`, so
/// listing never loads values and a change rewrites only the small index.
pub struct SnapshotStore {
    index: JsonStore<Vec<SnapshotMeta>>,
    dir: PathBuf,
    /// Values of a legacy file that could not be migrated. While set, the store refuses
    /// changes, so the legacy file is never overwritten by an index without values.
    unmigrated: Option<HashMap<String, Paths>>,
}

impl SnapshotStore {
    pub fn open(config_dir: &Path) -> Self {
        let file = JsonFile::new(config_dir.join(INDEX_FILE));
        let dir = config_dir.join(VALUES_DIR);
        match file.load::<IndexFile>() {
            IndexFile::Index(index) => {
                remove_leftover_legacy_copy(config_dir);
                Self {
                    index: JsonStore::from_loaded(file, index),
                    dir,
                    unmigrated: None,
                }
            }
            IndexFile::Legacy(snapshots) => Self::migrate(config_dir, file, dir, snapshots),
        }
    }

    /// Writes every snapshot to its own file, then rewrites `snapshots.json` as the index. On
    /// any error the legacy file stays and the snapshots are served from memory.
    fn migrate(config_dir: &Path, file: JsonFile, dir: PathBuf, snapshots: Vec<Snapshot>) -> Self {
        let index = snapshots.iter().map(SnapshotMeta::of).collect();
        let store = Self {
            index: JsonStore::from_loaded(file, index),
            dir,
            unmigrated: None,
        };
        match store.write_migration(config_dir, &snapshots) {
            Ok(()) => {
                log::info!(
                    "Moved {} snapshots to {}",
                    snapshots.len(),
                    store.dir.display()
                );
                store
            }
            Err(e) => {
                log::warn!(
                    "Could not migrate {INDEX_FILE}; snapshots are read-only until it succeeds: {e}"
                );
                let values = snapshots.into_iter().map(|s| (s.id, s.values)).collect();
                Self {
                    unmigrated: Some(values),
                    ..store
                }
            }
        }
    }

    fn write_migration(&self, config_dir: &Path, snapshots: &[Snapshot]) -> AppResult<()> {
        let legacy_copy = config_dir.join(LEGACY_COPY);
        fs::copy(config_dir.join(INDEX_FILE), &legacy_copy)?;
        for snapshot in snapshots {
            JsonFile::new(self.values_path(&snapshot.id)?).save(&snapshot.values)?;
        }
        // The index is already in memory; an empty update saves it over the legacy file.
        self.index.update(|_| ())?;
        if let Err(e) = fs::remove_file(&legacy_copy) {
            log::warn!("Could not remove {}: {e}", legacy_copy.display());
        }
        Ok(())
    }

    /// Newest first, for one server and database.
    pub fn list(&self, account: &str, database: &str) -> Vec<SnapshotInfo> {
        self.index.read(|items| {
            items
                .iter()
                .filter(|s| s.account == account && s.database == database)
                .map(SnapshotInfo::from)
                .collect()
        })
    }

    pub async fn get(&self, id: &str) -> AppResult<Snapshot> {
        let meta = self
            .index
            .read(|items| items.iter().find(|s| s.id == id).cloned())
            .ok_or_else(|| AppError::InvalidInput(format!("Snapshot {id} not found")))?;
        if let Some(values) = self.unmigrated.as_ref().and_then(|v| v.get(id)) {
            return Ok(meta.with_values(values.clone()));
        }
        let path = self.values_path(id)?;
        let values = blocking(move || read_values(&path)).await?;
        Ok(meta.with_values(values))
    }

    /// Saves the values before the index, so the index never names a snapshot without values.
    pub async fn add(&self, mut snapshot: Snapshot) -> AppResult<SnapshotInfo> {
        self.refuse_unmigrated()?;
        snapshot.id = uuid::Uuid::new_v4().to_string();
        let meta = SnapshotMeta::of(&snapshot);
        let info = SnapshotInfo::from(&meta);
        let path = self.values_path(&snapshot.id)?;
        let values = snapshot.values;
        blocking(move || JsonFile::new(path).save(&values)).await?;
        let indexed = self
            .index
            .update_async(move |items| {
                let (account, database) = (meta.account.clone(), meta.database.clone());
                items.insert(0, meta);
                evict(items, &account, &database)
            })
            .await;
        match indexed {
            Ok(evicted) => self.remove_files(evicted).await,
            Err(e) => {
                self.remove_files(vec![snapshot.id]).await;
                return Err(e);
            }
        }
        Ok(info)
    }

    /// Removes the index entry before the file: a stray file is harmless, an entry without
    /// values is not.
    pub async fn delete(&self, id: &str) -> AppResult<()> {
        self.refuse_unmigrated()?;
        let target = id.to_string();
        let removed = self
            .index
            .update_async(move |items| {
                let before = items.len();
                items.retain(|s| s.id != target);
                items.len() < before
            })
            .await?;
        if removed {
            self.remove_files(vec![id.to_string()]).await;
        }
        Ok(())
    }

    fn refuse_unmigrated(&self) -> AppResult<()> {
        if self.unmigrated.is_none() {
            return Ok(());
        }
        Err(AppError::Io(format!(
            "{INDEX_FILE} could not be migrated to one file per snapshot; restart DataBee to retry"
        )))
    }

    /// Ids name files, so only uuid characters are accepted.
    fn values_path(&self, id: &str) -> AppResult<PathBuf> {
        let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !valid {
            return Err(AppError::InvalidInput(format!("Invalid snapshot id {id}")));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    /// Best effort: a file that stays behind is never read again.
    async fn remove_files(&self, ids: Vec<String>) {
        let paths: Vec<PathBuf> = ids
            .iter()
            .filter_map(|id| self.values_path(id).ok())
            .collect();
        if paths.is_empty() {
            return;
        }
        let removed = blocking(move || {
            for path in &paths {
                remove_if_present(path);
            }
            Ok(())
        })
        .await;
        if let Err(e) = removed {
            log::warn!("Could not remove snapshot files: {e}");
        }
    }
}

/// Keeps the newest `MAX_PER_DATABASE` of one account and database; returns the evicted ids.
fn evict(items: &mut Vec<SnapshotMeta>, account: &str, database: &str) -> Vec<String> {
    let mut kept = 0;
    let mut evicted = Vec::new();
    items.retain(|s| {
        if s.account != account || s.database != database {
            return true;
        }
        kept += 1;
        if kept <= MAX_PER_DATABASE {
            return true;
        }
        evicted.push(s.id.clone());
        false
    });
    evicted
}

fn read_values(path: &Path) -> AppResult<Paths> {
    let raw = fs::read(path)?;
    serde_json::from_slice(&raw)
        .map_err(|e| AppError::Io(format!("{} is not valid JSON ({e})", path.display())))
}

fn remove_if_present(path: &Path) {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            log::warn!("Could not remove {}: {e}", path.display());
        }
        _ => {}
    }
}

/// A copy left by a migration that stopped after rewriting the index. Only removed while
/// `snapshots.json` exists, so a missing or quarantined index never costs the copy.
fn remove_leftover_legacy_copy(config_dir: &Path) {
    let legacy_copy = config_dir.join(LEGACY_COPY);
    if legacy_copy.exists() && config_dir.join(INDEX_FILE).exists() {
        remove_if_present(&legacy_copy);
    }
}

async fn blocking<R: Send + 'static>(
    f: impl FnOnce() -> AppResult<R> + Send + 'static,
) -> AppResult<R> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Io(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

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

    #[tokio::test]
    async fn snapshots_round_trip_and_list_newest_first_per_server() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        store.add(snap("a", "first")).await.unwrap();
        let second = store.add(snap("a", "second")).await.unwrap();
        store.add(snap("b", "other")).await.unwrap();
        let reopened = SnapshotStore::open(dir.path());
        let labels: Vec<_> = reopened
            .list("a", "db")
            .into_iter()
            .map(|s| s.label)
            .collect();
        assert_eq!(labels, ["second", "first"]);
        assert_eq!(
            reopened.get(&second.id).await.unwrap().values["field/status"],
            "wc-pending"
        );
        reopened.delete(&second.id).await.unwrap();
        assert_eq!(reopened.list("a", "db").len(), 1);
    }

    fn value_files(config_dir: &Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(config_dir.join(VALUES_DIR)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn legacy_file(snapshots: &[Snapshot]) -> String {
        serde_json::to_string(snapshots).unwrap()
    }

    fn legacy_snap(id: &str, label: &str) -> Snapshot {
        Snapshot {
            id: id.into(),
            created: "2024-01-01T00:00:00Z".parse().unwrap(),
            ..snap("a", label)
        }
    }

    #[tokio::test]
    async fn add_writes_the_values_file_and_the_index_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());

        let info = store.add(snap("a", "first")).await.unwrap();

        assert_eq!(value_files(dir.path()), [format!("{}.json", info.id)]);
        let values: Paths = serde_json::from_slice(
            &fs::read(
                dir.path()
                    .join(VALUES_DIR)
                    .join(format!("{}.json", info.id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(values["field/status"], "wc-pending");
        let index: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join(INDEX_FILE)).unwrap()).unwrap();
        assert_eq!(index[0]["id"], info.id);
        assert_eq!(index[0]["paths"], 1);
        assert!(index[0].get("values").is_none());
    }

    #[tokio::test]
    async fn list_reads_only_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let info = store.add(snap("a", "first")).await.unwrap();
        fs::remove_dir_all(dir.path().join(VALUES_DIR)).unwrap();

        let reopened = SnapshotStore::open(dir.path());

        assert_eq!(reopened.list("a", "db"), vec![info.clone()]);
        assert_eq!(info.paths, 1);
    }

    #[tokio::test]
    async fn get_returns_the_metadata_with_the_values() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let added = snap("a", "first");
        let info = store.add(added.clone()).await.unwrap();

        let loaded = store.get(&info.id).await.unwrap();

        assert_eq!(
            loaded,
            Snapshot {
                id: info.id,
                ..added
            }
        );
    }

    #[tokio::test]
    async fn delete_removes_the_index_entry_and_the_values_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let keep = store.add(snap("a", "keep")).await.unwrap();
        let gone = store.add(snap("a", "gone")).await.unwrap();

        store.delete(&gone.id).await.unwrap();

        assert_eq!(value_files(dir.path()), [format!("{}.json", keep.id)]);
        let reopened = SnapshotStore::open(dir.path());
        assert_eq!(reopened.list("a", "db"), vec![keep]);
        assert_eq!(
            reopened.get(&gone.id).await.unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn eviction_removes_the_values_files_of_evicted_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let first = store.add(snap("a", "0")).await.unwrap();
        for i in 1..35 {
            store.add(snap("a", &i.to_string())).await.unwrap();
        }

        let files = value_files(dir.path());

        assert_eq!(files.len(), MAX_PER_DATABASE);
        assert!(!files.contains(&format!("{}.json", first.id)));
    }

    #[tokio::test]
    async fn ids_with_path_characters_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(INDEX_FILE),
            r#"[{"id":"../evil","label":"x","account":"a","database":"db","scope":{"entity":null,"id":null,"includeRelated":false,"optionPattern":null,"includeActions":false},"created":"2024-01-01T00:00:00Z","paths":1}]"#,
        )
        .unwrap();
        let store = SnapshotStore::open(dir.path());
        assert_eq!(store.list("a", "db").len(), 1);

        let err = store.get("../evil").await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "Invalid snapshot id ../evil");
    }

    #[tokio::test]
    async fn legacy_file_migrates_to_index_and_value_files_without_losing_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(INDEX_FILE),
            legacy_file(&[legacy_snap("id-1", "one"), legacy_snap("id-2", "two")]),
        )
        .unwrap();

        let store = SnapshotStore::open(dir.path());

        let labels: Vec<_> = store.list("a", "db").into_iter().map(|s| s.label).collect();
        assert_eq!(labels, ["one", "two"]);
        assert_eq!(value_files(dir.path()), ["id-1.json", "id-2.json"]);
        assert!(!dir.path().join(LEGACY_COPY).exists());
        let reopened = SnapshotStore::open(dir.path());
        assert_eq!(reopened.list("a", "db").len(), 2);
        assert_eq!(
            reopened.get("id-2").await.unwrap(),
            legacy_snap("id-2", "two")
        );
    }

    #[tokio::test]
    async fn migration_that_cannot_write_keeps_the_legacy_file_and_serves_from_memory() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = legacy_file(&[legacy_snap("id-1", "one")]);
        fs::write(dir.path().join(INDEX_FILE), &legacy).unwrap();
        fs::write(dir.path().join(VALUES_DIR), "blocks the directory").unwrap();

        let store = SnapshotStore::open(dir.path());

        assert_eq!(
            fs::read_to_string(dir.path().join(INDEX_FILE)).unwrap(),
            legacy
        );
        assert_eq!(store.list("a", "db").len(), 1);
        assert_eq!(store.get("id-1").await.unwrap(), legacy_snap("id-1", "one"));
        assert_eq!(
            store.add(snap("a", "new")).await.unwrap_err().kind(),
            ErrorKind::Io
        );
        assert_eq!(
            store.delete("id-1").await.unwrap_err().kind(),
            ErrorKind::Io
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(INDEX_FILE)).unwrap(),
            legacy
        );
    }

    #[tokio::test]
    async fn keeps_a_bounded_number_per_server() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        for i in 0..35 {
            store.add(snap("a", &i.to_string())).await.unwrap();
        }
        assert_eq!(store.list("a", "db").len(), 30);
        assert_eq!(store.list("a", "db")[0].label, "34");
    }

    #[tokio::test]
    async fn unknown_snapshot_id_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        assert_eq!(
            store.get("unknown").await.unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn eviction_only_affects_the_database_that_overflowed() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        for i in 0..35 {
            store.add(snap("a", &i.to_string())).await.unwrap();
        }
        store
            .add(Snapshot {
                database: "db2".into(),
                ..snap("a", "other db")
            })
            .await
            .unwrap();
        assert_eq!(store.list("a", "db").len(), MAX_PER_DATABASE);
        assert_eq!(store.list("a", "db2").len(), 1);
    }

    #[tokio::test]
    async fn eviction_leaves_other_accounts_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        store.add(snap("b", "keep")).await.unwrap();
        for i in 0..35 {
            store.add(snap("a", &i.to_string())).await.unwrap();
        }
        assert_eq!(store.list("b", "db").len(), 1);
        assert_eq!(store.list("a", "db").len(), MAX_PER_DATABASE);
    }
}
