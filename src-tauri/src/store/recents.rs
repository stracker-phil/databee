use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::json_file::JsonFile;
use crate::connection::ConnectionSpec;
use crate::error::AppResult;

const MAX_RECENTS: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentConnection {
    pub identity: String,
    pub spec: ConnectionSpec,
    pub last_used: DateTime<Utc>,
}

pub struct RecentsStore {
    file: JsonFile,
    items: Mutex<Vec<RecentConnection>>,
}

impl RecentsStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("connections.json"));
        let items = Mutex::new(file.load());
        Self { file, items }
    }

    /// Most recently used first.
    pub fn list(&self) -> Vec<RecentConnection> {
        self.items.lock().unwrap().clone()
    }

    pub fn touch(&self, spec: &ConnectionSpec) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        let identity = spec.identity();
        items.retain(|r| r.identity != identity);
        items.insert(
            0,
            RecentConnection {
                identity,
                spec: spec.clone(),
                last_used: Utc::now(),
            },
        );
        items.truncate(MAX_RECENTS);
        self.file.save(&*items)
    }

    pub fn remove(&self, identity: &str) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|r| r.identity != identity);
        self.file.save(&*items)
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
    fn removal_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentsStore::open(dir.path());
        store.touch(&spec("a")).unwrap();
        store.touch(&spec("b")).unwrap();
        store.remove(&spec("a").identity()).unwrap();
        assert_eq!(dbs(&RecentsStore::open(dir.path())), ["b"]);
    }
}
