//! Rows opened in the app per project (`recent_objects.json`), keyed by `<slug>/<database>`,
//! newest first, each address once, at most `RECENT_CAP`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::json_store::JsonStore;
use crate::error::AppResult;
use crate::rules::project::project_key;

pub const RECENT_CAP: usize = 20;

/// A row address and when it was opened (Unix seconds).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RecentObject {
    pub address: String,
    pub at: i64,
}

pub struct RecentObjectsStore {
    store: JsonStore<BTreeMap<String, Vec<RecentObject>>>,
}

impl RecentObjectsStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open(dir.join("recent_objects.json")),
        }
    }

    pub fn get(&self, connection: &str, database: &str) -> Vec<RecentObject> {
        let key = project_key(connection, database);
        self.store
            .read(|items| items.get(&key).cloned())
            .unwrap_or_default()
    }

    /// Puts `address` first, once, and drops what is beyond `RECENT_CAP`.
    pub async fn remember(
        &self,
        connection: &str,
        database: &str,
        address: String,
        at: i64,
    ) -> AppResult<()> {
        let key = project_key(connection, database);
        self.store
            .update_async(move |items| {
                let recent = items.entry(key).or_default();
                recent.retain(|r| r.address != address);
                recent.insert(0, RecentObject { address, at });
                recent.truncate(RECENT_CAP);
            })
            .await
    }

    /// Moves every `<old>/…` entry to the new slug after a slug rename.
    pub async fn rename_connection(&self, old: &str, new: &str) -> AppResult<()> {
        if old == new {
            return Ok(());
        }
        let old_prefix = format!("{old}/");
        let new_prefix = format!("{new}/");
        self.store
            .update_async(move |items| {
                let keys: Vec<String> = items
                    .keys()
                    .filter(|key| key.starts_with(&old_prefix))
                    .cloned()
                    .collect();
                for key in keys {
                    if let Some(recent) = items.remove(&key) {
                        let database = &key[old_prefix.len()..];
                        items.insert(format!("{new_prefix}{database}"), recent);
                    }
                }
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addresses(store: &RecentObjectsStore, connection: &str, database: &str) -> Vec<String> {
        store
            .get(connection, database)
            .into_iter()
            .map(|r| r.address)
            .collect()
    }

    #[test]
    fn a_project_without_recent_objects_has_none() {
        let dir = tempfile::tempdir().unwrap();

        assert!(
            RecentObjectsStore::open(dir.path())
                .get("local", "wp")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn the_newest_object_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());

        store.remember("local", "wp", "a".into(), 1).await.unwrap();
        store.remember("local", "wp", "b".into(), 2).await.unwrap();

        assert_eq!(
            store.get("local", "wp"),
            [
                RecentObject {
                    address: "b".into(),
                    at: 2
                },
                RecentObject {
                    address: "a".into(),
                    at: 1
                },
            ]
        );
    }

    #[tokio::test]
    async fn remembering_an_address_again_moves_it_to_the_front_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());
        for (address, at) in [("a", 1), ("b", 2), ("a", 3)] {
            store
                .remember("local", "wp", address.into(), at)
                .await
                .unwrap();
        }

        assert_eq!(
            store.get("local", "wp"),
            [
                RecentObject {
                    address: "a".into(),
                    at: 3
                },
                RecentObject {
                    address: "b".into(),
                    at: 2
                },
            ]
        );
    }

    #[tokio::test]
    async fn only_the_newest_twenty_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());
        for i in 0..25 {
            store
                .remember("local", "wp", format!("row{i}"), i)
                .await
                .unwrap();
        }

        let kept = addresses(&store, "local", "wp");

        assert_eq!(kept.len(), RECENT_CAP);
        assert_eq!(kept.first().map(String::as_str), Some("row24"));
        assert_eq!(kept.last().map(String::as_str), Some("row5"));
    }

    #[tokio::test]
    async fn projects_keep_separate_lists_that_survive_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());
        store.remember("local", "wp", "a".into(), 1).await.unwrap();
        store
            .remember("local", "other", "b".into(), 2)
            .await
            .unwrap();
        store.remember("remote", "wp", "c".into(), 3).await.unwrap();

        let reopened = RecentObjectsStore::open(dir.path());

        assert_eq!(addresses(&reopened, "local", "wp"), ["a"]);
        assert_eq!(addresses(&reopened, "local", "other"), ["b"]);
        assert_eq!(addresses(&reopened, "remote", "wp"), ["c"]);
    }

    #[tokio::test]
    async fn renaming_a_connection_moves_its_lists_and_leaves_similar_slugs_alone() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());
        store.remember("old", "db1", "a".into(), 1).await.unwrap();
        store.remember("old", "db2", "b".into(), 2).await.unwrap();
        store.remember("older", "db1", "c".into(), 3).await.unwrap();

        store.rename_connection("old", "new").await.unwrap();

        assert_eq!(addresses(&store, "new", "db1"), ["a"]);
        assert_eq!(addresses(&store, "new", "db2"), ["b"]);
        assert_eq!(addresses(&store, "older", "db1"), ["c"]);
        assert!(store.get("old", "db1").is_empty());
    }
}
