//! Snapshots of rows and saved-query results (`snapshots/v2/`): the index in `index.json`, each
//! snapshot's cells, keyed by provenance, in `<id>.json`, so listing never loads cells. The
//! WordPress snapshots before them (`snapshots.json`, `snapshots/<id>.json`) are left untouched
//! and never read.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::json_file::JsonFile;
use super::json_store::JsonStore;
use crate::address::Address;
use crate::document::diff::FlatDocument;
use crate::error::{AppError, AppResult};

/// Snapshots kept per connection and database; the oldest go first.
pub const MAX_PER_DATABASE: usize = 30;
const DIR: &str = "snapshots/v2";
const INDEX_FILE: &str = "index.json";
const OLD_INDEX_FILE: &str = "snapshots.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub id: String,
    pub label: String,
    /// The connection slug and database the snapshot belongs to.
    pub connection: String,
    pub database: String,
    /// The row or saved query (`databee://…/q/<id>`) the snapshot was taken of.
    pub address: String,
    /// Unix seconds.
    #[ts(type = "number")]
    pub created: i64,
    /// Stored values.
    pub values: usize,
}

pub struct SnapshotStore {
    index: JsonStore<Vec<SnapshotInfo>>,
    dir: PathBuf,
}

impl SnapshotStore {
    pub fn open(config_dir: &Path) -> Self {
        let old = config_dir.join(OLD_INDEX_FILE);
        if old.exists() {
            log::info!(
                "Snapshots of the removed WordPress mode in {} and {} are not read",
                old.display(),
                config_dir.join("snapshots").display()
            );
        }
        let dir = config_dir.join(DIR);
        Self {
            index: JsonStore::open(dir.join(INDEX_FILE)),
            dir,
        }
    }

    /// Newest first, for one connection and database; only those of `address` when given.
    pub fn list(
        &self,
        connection: &str,
        database: &str,
        address: Option<&str>,
    ) -> Vec<SnapshotInfo> {
        self.index.read(|items| {
            items
                .iter()
                .filter(|s| s.connection == connection && s.database == database)
                .filter(|s| address.is_none_or(|a| s.address == a))
                .cloned()
                .collect()
        })
    }

    pub async fn get(&self, id: &str) -> AppResult<(SnapshotInfo, FlatDocument)> {
        let info = self
            .index
            .read(|items| items.iter().find(|s| s.id == id).cloned())
            .ok_or_else(|| AppError::InvalidInput(format!("No snapshot {id}")))?;
        let path = self.cells_path(id)?;
        let document = blocking(move || read_cells(&path)).await?;
        Ok((info, document))
    }

    /// Stores `document` as a new snapshot of `address`, created at `created`. The cells are
    /// saved before the index, so the index never names a snapshot without them.
    pub async fn add(
        &self,
        label: String,
        address: &Address,
        created: i64,
        document: FlatDocument,
    ) -> AppResult<SnapshotInfo> {
        let info = SnapshotInfo {
            id: uuid::Uuid::new_v4().to_string(),
            label,
            connection: address.connection.clone(),
            database: address.database.clone(),
            address: address.to_string(),
            created,
            values: count_values(&document),
        };
        let path = self.cells_path(&info.id)?;
        blocking(move || JsonFile::new(path).save(&document)).await?;
        let entry = info.clone();
        let indexed = self
            .index
            .update_async(move |items| {
                let (connection, database) = (entry.connection.clone(), entry.database.clone());
                items.insert(0, entry);
                evict(items, &connection, &database)
            })
            .await;
        match indexed {
            Ok(evicted) => self.remove_files(evicted).await,
            Err(e) => {
                self.remove_files(vec![info.id]).await;
                return Err(e);
            }
        }
        Ok(info)
    }

    /// Removes the index entry before the file: a stray file is harmless, an entry without
    /// cells is not.
    pub async fn delete(&self, id: &str) -> AppResult<()> {
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

    /// Moves the snapshots of connection `old` to the slug `new`, addresses included.
    pub async fn rename_connection(&self, old: &str, new: &str) -> AppResult<()> {
        if old == new {
            return Ok(());
        }
        let (old, new) = (old.to_string(), new.to_string());
        self.index
            .update_async(move |items| {
                for item in items.iter_mut().filter(|s| s.connection == old) {
                    item.connection = new.clone();
                    if let Ok(mut address) = Address::parse(&item.address) {
                        address.connection = new.clone();
                        item.address = address.to_string();
                    }
                }
            })
            .await
    }

    /// Ids name files, so only uuid characters are accepted.
    fn cells_path(&self, id: &str) -> AppResult<PathBuf> {
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
            .filter_map(|id| self.cells_path(id).ok())
            .collect();
        if paths.is_empty() {
            return;
        }
        let removed = blocking(move || {
            for path in &paths {
                if let Err(e) = fs::remove_file(path)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    log::warn!("Could not remove {}: {e}", path.display());
                }
            }
            Ok(())
        })
        .await;
        if let Err(e) = removed {
            log::warn!("Could not remove snapshot files: {e}");
        }
    }
}

fn count_values(document: &FlatDocument) -> usize {
    document
        .groups
        .iter()
        .flat_map(|g| &g.rows)
        .map(|r| r.fields.len())
        .sum()
}

/// A missing or unreadable file is an error here, never an empty snapshot.
fn read_cells(path: &Path) -> AppResult<FlatDocument> {
    let raw = fs::read(path)
        .map_err(|e| AppError::Io(format!("Could not read {}: {e}", path.display())))?;
    serde_json::from_slice(&raw)
        .map_err(|e| AppError::Io(format!("Could not read {}: {e}", path.display())))
}

/// Keeps the newest `MAX_PER_DATABASE` of one connection and database; returns the evicted ids.
fn evict(items: &mut Vec<SnapshotInfo>, connection: &str, database: &str) -> Vec<String> {
    let mut kept = 0;
    let mut evicted = Vec::new();
    items.retain(|s| {
        if s.connection != connection || s.database != database {
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

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Io(format!("Snapshot file task failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Target;
    use crate::document::diff::fixtures::status_doc;
    use crate::error::ErrorKind;

    fn row(connection: &str, database: &str, key: &str) -> Address {
        Address::new(
            connection,
            database,
            Target::Row {
                table: "orders".into(),
                key: key.into(),
            },
        )
    }

    fn cells_file(dir: &Path, id: &str) -> PathBuf {
        dir.join("snapshots/v2").join(format!("{id}.json"))
    }

    #[tokio::test]
    async fn an_added_snapshot_reads_back_with_the_same_cells() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let document = status_doc("paid");

        let info = store
            .add(
                "before".into(),
                &row("local", "wp", "1"),
                100,
                document.clone(),
            )
            .await
            .unwrap();
        let (read, cells) = store.get(&info.id).await.unwrap();

        assert_eq!(read, info);
        assert_eq!(cells, document);
        assert_eq!(
            (
                info.label.as_str(),
                info.connection.as_str(),
                info.database.as_str()
            ),
            ("before", "local", "wp")
        );
        assert_eq!(info.address, "databee://local/wp/orders/1");
        assert_eq!((info.created, info.values), (100, 2));
    }

    #[tokio::test]
    async fn a_snapshot_survives_reopening_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let info = SnapshotStore::open(dir.path())
            .add("x".into(), &row("local", "wp", "1"), 1, status_doc("a"))
            .await
            .unwrap();

        let (_, cells) = SnapshotStore::open(dir.path()).get(&info.id).await.unwrap();

        assert_eq!(cells, status_doc("a"));
    }

    #[tokio::test]
    async fn an_unknown_snapshot_id_is_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());

        let err = store.get("missing").await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn list_is_newest_first_and_filters_by_connection_database_and_address() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let mut ids = Vec::new();
        for (n, (connection, database, key)) in [
            ("local", "wp", "1"),
            ("local", "wp", "2"),
            ("local", "other", "1"),
            ("remote", "wp", "1"),
            ("local", "wp", "1"),
        ]
        .into_iter()
        .enumerate()
        {
            let info = store
                .add(
                    format!("s{n}"),
                    &row(connection, database, key),
                    n as i64,
                    status_doc("a"),
                )
                .await
                .unwrap();
            ids.push(info.id);
        }
        let ids_of = |list: Vec<SnapshotInfo>| list.into_iter().map(|s| s.id).collect::<Vec<_>>();

        assert_eq!(
            ids_of(store.list("local", "wp", None)),
            [ids[4].clone(), ids[1].clone(), ids[0].clone()]
        );
        assert_eq!(
            ids_of(store.list("local", "wp", Some("databee://local/wp/orders/1"))),
            [ids[4].clone(), ids[0].clone()]
        );
        assert_eq!(ids_of(store.list("local", "other", None)), [ids[2].clone()]);
        assert_eq!(ids_of(store.list("remote", "wp", None)), [ids[3].clone()]);
        assert!(store.list("local", "missing", None).is_empty());
    }

    #[tokio::test]
    async fn delete_removes_the_entry_and_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let info = store
            .add("x".into(), &row("local", "wp", "1"), 1, status_doc("a"))
            .await
            .unwrap();
        assert!(cells_file(dir.path(), &info.id).exists());

        store.delete(&info.id).await.unwrap();

        assert!(store.list("local", "wp", None).is_empty());
        assert!(!cells_file(dir.path(), &info.id).exists());
        store.delete(&info.id).await.unwrap();
    }

    #[tokio::test]
    async fn the_31st_snapshot_of_a_database_evicts_the_oldest_and_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        let other = store
            .add(
                "other".into(),
                &row("local", "other", "1"),
                0,
                status_doc("a"),
            )
            .await
            .unwrap();
        let mut ids = Vec::new();
        for n in 0..=MAX_PER_DATABASE {
            let info = store
                .add(
                    format!("s{n}"),
                    &row("local", "wp", "1"),
                    n as i64,
                    status_doc("a"),
                )
                .await
                .unwrap();
            ids.push(info.id);
        }

        let kept: Vec<String> = store
            .list("local", "wp", None)
            .into_iter()
            .map(|s| s.id)
            .collect();

        assert_eq!(kept.len(), MAX_PER_DATABASE);
        assert!(!kept.contains(&ids[0]));
        assert_eq!(kept[0], ids[MAX_PER_DATABASE]);
        assert!(!cells_file(dir.path(), &ids[0]).exists());
        assert!(cells_file(dir.path(), &ids[1]).exists());
        assert_eq!(store.list("local", "other", None).len(), 1);
        assert!(cells_file(dir.path(), &other.id).exists());
    }

    #[tokio::test]
    async fn renaming_a_connection_rewrites_slug_and_address_of_its_snapshots_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::open(dir.path());
        store
            .add("a".into(), &row("old", "wp", "1"), 1, status_doc("a"))
            .await
            .unwrap();
        store
            .add("b".into(), &row("keep", "wp", "1"), 2, status_doc("a"))
            .await
            .unwrap();

        store.rename_connection("old", "new").await.unwrap();

        assert!(store.list("old", "wp", None).is_empty());
        let renamed = store.list("new", "wp", None);
        assert_eq!(renamed.len(), 1);
        assert_eq!(renamed[0].address, "databee://new/wp/orders/1");
        assert_eq!(
            store.list("keep", "wp", None)[0].address,
            "databee://keep/wp/orders/1"
        );
        assert_eq!(
            SnapshotStore::open(dir.path()).list("new", "wp", None),
            renamed
        );
    }

    #[tokio::test]
    async fn snapshots_of_the_removed_wordpress_mode_are_not_read_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let old_index = br#"[{"id":"abc","label":"old","connection":"local","database":"wp","address":"databee://local/wp/orders/1","created":1,"values":2}]"#;
        let old_cells = br#"{"groups":[]}"#;
        std::fs::write(dir.path().join("snapshots.json"), old_index).unwrap();
        std::fs::create_dir_all(dir.path().join("snapshots")).unwrap();
        std::fs::write(dir.path().join("snapshots/abc.json"), old_cells).unwrap();

        let store = SnapshotStore::open(dir.path());

        assert!(store.list("local", "wp", None).is_empty());
        assert_eq!(
            std::fs::read(dir.path().join("snapshots.json")).unwrap(),
            old_index
        );
        assert_eq!(
            std::fs::read(dir.path().join("snapshots/abc.json")).unwrap(),
            old_cells
        );
    }
}
