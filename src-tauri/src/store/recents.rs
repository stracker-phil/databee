use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::json_store::JsonStore;
use crate::connection::ConnectionSpec;
use crate::error::AppResult;

const MAX_RECENTS: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentConnection {
    pub identity: String,
    /// Derived from `spec` when listed; older files lack it.
    #[serde(default)]
    pub account: String,
    pub spec: ConnectionSpec,
    pub last_used: DateTime<Utc>,
}

pub struct RecentsStore {
    store: JsonStore<Vec<RecentConnection>>,
}

impl RecentsStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open(dir.join("connections.json")),
        }
    }

    /// Most recently used first.
    pub fn list(&self) -> Vec<RecentConnection> {
        let mut recents = self.store.read(Vec::clone);
        for recent in &mut recents {
            recent.account = recent.spec.account();
        }
        recents
    }

    pub fn touch(&self, spec: &ConnectionSpec) -> AppResult<()> {
        let identity = spec.identity();
        self.store.update(|items| {
            items.retain(|r| r.identity != identity);
            items.insert(
                0,
                RecentConnection {
                    identity,
                    account: spec.account(),
                    spec: spec.clone(),
                    last_used: Utc::now(),
                },
            );
            items.truncate(MAX_RECENTS);
        })
    }

    pub fn remove(&self, identity: &str) -> AppResult<()> {
        self.store
            .update(|items| items.retain(|r| r.identity != identity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(db: &str) -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: Some(db.into()),
        }
    }

    fn dbs(store: &RecentsStore) -> Vec<String> {
        store
            .list()
            .into_iter()
            .map(|r| r.spec.database.unwrap())
            .collect()
    }

    #[test]
    fn most_recent_comes_first_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).unwrap();
        store.touch(&spec("b")).unwrap();
        store.touch(&spec("a")).unwrap();
        assert_eq!(dbs(&store), ["a", "b"]);
    }

    #[test]
    fn keeps_at_most_twenty_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        for i in 0..25 {
            store.touch(&spec(&format!("db{i}"))).unwrap();
        }
        assert_eq!(store.list().len(), 20);
        assert_eq!(dbs(&store)[0], "db24");
    }

    #[test]
    fn list_fills_the_account_from_the_spec() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).unwrap();

        assert_eq!(store.list()[0].account, "root@127.0.0.1:3306");
    }

    #[test]
    fn list_fills_the_account_for_entries_written_without_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("connections.json"),
            r#"[{
                "identity": "root@127.0.0.1:3306/a",
                "spec": {"host": "127.0.0.1", "port": 3306, "user": "root", "database": "a"},
                "lastUsed": "2024-01-01T00:00:00Z"
            }]"#,
        )
        .unwrap();

        let recents = RecentsStore::open(dir.path()).list();

        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].account, "root@127.0.0.1:3306");
    }

    #[test]
    fn removal_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).unwrap();
        store.touch(&spec("b")).unwrap();
        store.remove(&spec("a").identity()).unwrap();
        assert_eq!(dbs(&RecentsStore::open(dir.path())), ["b"]);
    }
}
