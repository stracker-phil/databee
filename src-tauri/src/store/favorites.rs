use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::json_file::JsonFile;
use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FavoriteTarget {
    Table { database: String, table: String },
    Query { sql: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    /// Empty when creating; assigned on save.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Server account (`user@host:port`) the favorite belongs to; `None` shows it everywhere.
    #[serde(default)]
    pub scope: Option<String>,
    pub target: FavoriteTarget,
}

pub struct FavoritesStore {
    file: JsonFile,
    items: Mutex<Vec<Favorite>>,
}

impl FavoritesStore {
    pub fn open(dir: &Path) -> Self {
        let file = JsonFile::new(dir.join("favorites.json"));
        let items = Mutex::new(file.load());
        Self { file, items }
    }

    /// Favorites visible for a server account: its own plus global ones, in bar order.
    pub fn list(&self, account: &str) -> Vec<Favorite> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|f| f.scope.as_deref().is_none_or(|s| s == account))
            .cloned()
            .collect()
    }

    /// Inserts a new favorite (appended) or updates an existing one in place. A new favorite
    /// whose target is already saved for the same scope returns the existing one instead.
    pub fn save(&self, mut favorite: Favorite) -> AppResult<Favorite> {
        if favorite.name.trim().is_empty() {
            return Err(AppError::InvalidInput("A favorite needs a name".into()));
        }
        let mut items = self.items.lock().unwrap();
        match items
            .iter_mut()
            .find(|f| f.id == favorite.id && !f.id.is_empty())
        {
            Some(existing) => *existing = favorite.clone(),
            None => {
                if let Some(existing) = items
                    .iter()
                    .find(|f| f.target == favorite.target && f.scope == favorite.scope)
                {
                    return Ok(existing.clone());
                }
                favorite.id = uuid::Uuid::new_v4().to_string();
                items.push(favorite.clone());
            }
        }
        self.file.save(&*items)?;
        Ok(favorite)
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|f| f.id != id);
        self.file.save(&*items)
    }

    /// Puts the given ids first, in that order; everything else keeps its relative order.
    pub fn reorder(&self, ids: &[String]) -> AppResult<()> {
        let mut items = self.items.lock().unwrap();
        let rank = |f: &Favorite| ids.iter().position(|id| *id == f.id).unwrap_or(usize::MAX);
        items.sort_by_key(rank);
        self.file.save(&*items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(name: &str, scope: Option<&str>) -> Favorite {
        Favorite {
            id: String::new(),
            name: name.into(),
            scope: scope.map(String::from),
            target: FavoriteTarget::Query {
                sql: format!("SELECT '{name}'"),
            },
        }
    }

    fn names(list: Vec<Favorite>) -> Vec<String> {
        list.into_iter().map(|f| f.name).collect()
    }

    #[test]
    fn scoped_favorites_only_show_for_their_server() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        store.save(query("global", None)).unwrap();
        store
            .save(query("shop", Some("root@127.0.0.1:52958")))
            .unwrap();
        assert_eq!(
            names(store.list("root@127.0.0.1:52958")),
            ["global", "shop"]
        );
        assert_eq!(names(store.list("root@127.0.0.1:3306")), ["global"]);
    }

    #[test]
    fn saving_with_id_updates_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let a = store.save(query("a", None)).unwrap();
        store.save(query("b", None)).unwrap();
        store
            .save(Favorite {
                name: "renamed".into(),
                ..a
            })
            .unwrap();
        assert_eq!(names(store.list("x")), ["renamed", "b"]);
    }

    #[test]
    fn reorder_and_remove_persist() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let a = store.save(query("a", None)).unwrap();
        let b = store.save(query("b", None)).unwrap();
        let c = store.save(query("c", None)).unwrap();
        store.reorder(&[c.id.clone(), a.id.clone()]).unwrap();
        store.remove(&b.id).unwrap();
        assert_eq!(
            names(FavoritesStore::open(dir.path()).list("x")),
            ["c", "a"]
        );
    }

    #[test]
    fn saving_the_same_target_twice_keeps_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let first = store.save(query("posts", Some("a@h:1"))).unwrap();
        let again = store.save(query("posts", Some("a@h:1"))).unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(store.list("a@h:1").len(), 1);
    }

    #[test]
    fn blank_names_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        assert!(store.save(query("  ", None)).is_err());
    }
}
