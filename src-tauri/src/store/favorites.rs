use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::json_store::JsonStore;
use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FavoriteTarget {
    Table { database: String, table: String },
    Query { sql: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    pub id: String,
    pub name: String,
    /// Server account (`user@host:port`) the favorite belongs to; `None` shows it everywhere.
    #[serde(default)]
    pub scope: Option<String>,
    pub target: FavoriteTarget,
}

#[derive(Deserialize, Clone, Debug, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct NewFavorite {
    pub name: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub scope: Option<String>,
    pub target: FavoriteTarget,
}

pub struct FavoritesStore {
    store: JsonStore<Vec<Favorite>>,
}

impl FavoritesStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            store: JsonStore::open(dir.join("favorites.json")),
        }
    }

    /// Favorites visible for a server account: its own plus global ones, in bar order.
    pub fn list(&self, account: &str) -> Vec<Favorite> {
        self.store.read(|items| {
            items
                .iter()
                .filter(|f| f.scope.as_deref().is_none_or(|s| s == account))
                .cloned()
                .collect()
        })
    }

    /// Appends a favorite with a fresh id.
    pub fn create(&self, new: NewFavorite) -> AppResult<Favorite> {
        require_name(&new.name)?;
        let favorite = Favorite {
            id: uuid::Uuid::new_v4().to_string(),
            name: new.name,
            scope: new.scope,
            target: new.target,
        };
        self.store.update(|items| {
            reject_duplicate(items, &favorite)?;
            items.push(favorite.clone());
            Ok(favorite)
        })?
    }

    /// Replaces the favorite with the same id, in place.
    pub fn update(&self, favorite: Favorite) -> AppResult<Favorite> {
        require_name(&favorite.name)?;
        self.store.update(|items| {
            reject_duplicate(items, &favorite)?;
            let existing = items
                .iter_mut()
                .find(|f| f.id == favorite.id)
                .ok_or_else(|| {
                    AppError::InvalidInput(format!("Favorite {} not found", favorite.id))
                })?;
            *existing = favorite.clone();
            Ok(favorite)
        })?
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        self.store.update(|items| items.retain(|f| f.id != id))
    }

    /// Puts the given ids first, in that order; everything else keeps its relative order.
    /// Unknown ids are ignored.
    pub fn reorder(&self, ids: &[String]) -> AppResult<()> {
        let rank = |f: &Favorite| ids.iter().position(|id| *id == f.id).unwrap_or(usize::MAX);
        self.store.update(|items| items.sort_by_key(rank))
    }
}

fn require_name(name: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("A favorite needs a name".into()));
    }
    Ok(())
}

/// One favorite per target and scope; `favorite` itself (same id) does not count.
fn reject_duplicate(items: &[Favorite], favorite: &Favorite) -> AppResult<()> {
    let taken = items
        .iter()
        .any(|f| f.id != favorite.id && f.scope == favorite.scope && f.target == favorite.target);
    if taken {
        return Err(AppError::InvalidInput(format!(
            "{} is already in favorites",
            favorite.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn query(name: &str, scope: Option<&str>) -> NewFavorite {
        NewFavorite {
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
        store.create(query("global", None)).unwrap();
        store
            .create(query("shop", Some("root@127.0.0.1:52958")))
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
        let a = store.create(query("a", None)).unwrap();
        store.create(query("b", None)).unwrap();
        store
            .update(Favorite {
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
        let a = store.create(query("a", None)).unwrap();
        let b = store.create(query("b", None)).unwrap();
        let c = store.create(query("c", None)).unwrap();
        store.reorder(&[c.id.clone(), a.id.clone()]).unwrap();
        store.remove(&b.id).unwrap();
        assert_eq!(
            names(FavoritesStore::open(dir.path()).list("x")),
            ["c", "a"]
        );
    }

    #[test]
    fn creating_the_same_target_twice_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        store.create(query("posts", Some("a@h:1"))).unwrap();
        let err = store.create(query("posts", Some("a@h:1"))).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(store.list("a@h:1").len(), 1);
    }

    #[test]
    fn updating_an_unknown_id_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let ghost = Favorite {
            id: "missing".into(),
            name: "ghost".into(),
            scope: None,
            target: FavoriteTarget::Query {
                sql: "SELECT 1".into(),
            },
        };
        assert_eq!(
            store.update(ghost).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }

    #[test]
    fn table_favorite_round_trips_through_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let created = FavoritesStore::open(dir.path())
            .create(NewFavorite {
                name: "posts".into(),
                scope: Some("a@h:1".into()),
                target: FavoriteTarget::Table {
                    database: "wp".into(),
                    table: "wp_posts".into(),
                },
            })
            .unwrap();
        assert_eq!(
            FavoritesStore::open(dir.path()).list("a@h:1"),
            vec![created]
        );
    }

    #[test]
    fn reorder_ignores_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let a = store.create(query("a", None)).unwrap();
        store.create(query("b", None)).unwrap();
        store.reorder(&["nope".into(), a.id]).unwrap();
        assert_eq!(names(store.list("x")), ["a", "b"]);
    }

    #[test]
    fn same_target_with_different_scopes_stays_two_favorites() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        store.create(query("posts", Some("a@h:1"))).unwrap();
        store.create(query("posts", None)).unwrap();
        assert_eq!(store.list("a@h:1").len(), 2);
    }

    #[test]
    fn update_to_another_favorites_target_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let a = store.create(query("a", Some("a@h:1"))).unwrap();
        let b = store.create(query("b", Some("a@h:1"))).unwrap();
        let err = store
            .update(Favorite {
                target: a.target.clone(),
                ..b
            })
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn update_keeping_its_own_target_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        let a = store.create(query("a", Some("a@h:1"))).unwrap();
        store
            .update(Favorite {
                name: "renamed".into(),
                ..a
            })
            .unwrap();
        assert_eq!(names(store.list("a@h:1")), ["renamed"]);
    }

    #[test]
    fn blank_names_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FavoritesStore::open(dir.path());
        assert!(store.create(query("  ", None)).is_err());
    }
}
