//! One-shot move of `favorites.json` into saved queries: account-scoped favorites go to the
//! project of a saved connection of that account, the others to the global list; the file is
//! then renamed to `favorites.migrated.json`.

use std::path::Path;

use serde::Deserialize;

use crate::db::Dialect;
use crate::error::{AppError, AppResult};
use crate::rules::ProjectsStore;
use crate::rules::project::{ProjectQueries, SavedQuery};
use crate::store::json_file::JsonFile;
use crate::store::recents::{RecentConnection, RecentsStore};

pub const FAVORITES_FILE: &str = "favorites.json";
pub const MIGRATED_FILE: &str = "favorites.migrated.json";

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Target {
    Table { database: String, table: String },
    Query { sql: String },
}

#[derive(Deserialize)]
struct Favorite {
    id: String,
    name: String,
    #[serde(default)]
    scope: Option<String>,
    target: Target,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Migrated {
    pub project: usize,
    pub global: usize,
}

/// Migrates `<dir>/favorites.json` once; `None` when there is no such file.
pub fn migrate_favorites(dir: &Path) -> AppResult<Option<Migrated>> {
    let path = dir.join(FAVORITES_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let favorites: Vec<Favorite> = JsonFile::new(&path).load();
    let recents = RecentsStore::open(dir).list();
    let mut projects: Vec<ProjectQueries> = Vec::new();
    let mut global = Vec::new();
    for favorite in favorites {
        match project_of(&favorite, &recents) {
            Some((connection, database)) => {
                let query = saved_query(&favorite, Some(&database));
                add(&mut projects, connection, database, query);
            }
            None => global.push(saved_query(&favorite, None)),
        }
    }
    let migrated = Migrated {
        project: projects.iter().map(|p| p.queries.len()).sum(),
        global: global.len(),
    };
    ProjectsStore::open(dir).import_saved(projects, global)?;
    match std::fs::rename(&path, dir.join(MIGRATED_FILE)) {
        // A file that did not parse was already moved aside by `JsonFile::load`.
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(AppError::Io(format!(
            "Renaming {} failed: {e}",
            path.display()
        ))),
        _ => Ok(Some(migrated)),
    }
}

/// A table without a connection to its own database falls back to any connection of the account.
fn project_of(favorite: &Favorite, recents: &[RecentConnection]) -> Option<(String, String)> {
    let account = favorite.scope.as_deref()?;
    let of_account: Vec<&RecentConnection> =
        recents.iter().filter(|r| r.account == account).collect();
    match &favorite.target {
        Target::Table { database, .. } => {
            let same = of_account
                .iter()
                .find(|r| r.spec.database.as_deref() == Some(database));
            let connection = same.or(of_account.first())?;
            Some((connection.slug.clone(), database.clone()))
        }
        Target::Query { .. } => of_account.iter().find_map(|r| {
            let database = r.spec.database.clone()?;
            Some((r.slug.clone(), database))
        }),
    }
}

/// A table favorite becomes `SELECT * FROM <table>`, qualified unless it is in `database`.
fn saved_query(favorite: &Favorite, database: Option<&str>) -> SavedQuery {
    let dialect = Dialect::MYSQL;
    let sql = match &favorite.target {
        Target::Query { sql } => sql.clone(),
        Target::Table {
            database: db,
            table,
        } => {
            let from = if database == Some(db.as_str()) {
                dialect.quote_ident(table)
            } else {
                dialect.table_ref(db, table)
            };
            format!("SELECT * FROM {from}")
        }
    };
    SavedQuery {
        id: favorite.id.clone(),
        title: favorite.name.clone(),
        sql,
        pin: None,
    }
}

fn add(
    projects: &mut Vec<ProjectQueries>,
    connection: String,
    database: String,
    query: SavedQuery,
) {
    let found = projects
        .iter_mut()
        .find(|p| p.connection == connection && p.database == database);
    match found {
        Some(project) => project.queries.push(query),
        None => projects.push(ProjectQueries {
            connection,
            database,
            queries: vec![query],
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};

    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::test_support::spec;

    fn connection(database: &str) -> ConnectionSpec {
        spec("127.0.0.1", 3306, database)
    }

    fn account() -> String {
        connection("any").account()
    }

    async fn touch(dir: &Path, spec: &ConnectionSpec) -> String {
        RecentsStore::open(dir).touch(spec).await.unwrap()
    }

    fn query_favorite(id: &str, name: &str, scope: Option<&str>, sql: &str) -> Value {
        json!({"id": id, "name": name, "scope": scope, "target": {"kind": "query", "sql": sql}})
    }

    fn table_favorite(id: &str, scope: Option<&str>, database: &str, table: &str) -> Value {
        json!({
            "id": id,
            "name": table,
            "scope": scope,
            "target": {"kind": "table", "database": database, "table": table},
        })
    }

    fn write_favorites(dir: &Path, favorites: Vec<Value>) {
        fs::write(
            dir.join(FAVORITES_FILE),
            Value::Array(favorites).to_string(),
        )
        .unwrap();
    }

    fn saved(id: &str, title: &str, sql: &str) -> SavedQuery {
        SavedQuery {
            id: id.into(),
            title: title.into(),
            sql: sql.into(),
            pin: None,
        }
    }

    #[tokio::test]
    async fn a_query_favorite_goes_to_the_project_of_the_accounts_latest_connection_with_a_database()
     {
        let dir = tempfile::tempdir().unwrap();
        let old = touch(dir.path(), &connection("old")).await;
        let wp = touch(dir.path(), &connection("wp")).await;
        touch(
            dir.path(),
            &ConnectionSpec {
                database: None,
                ..connection("unused")
            },
        )
        .await;
        write_favorites(
            dir.path(),
            vec![query_favorite(
                "f1",
                "Open orders",
                Some(&account()),
                "SELECT 1",
            )],
        );

        let migrated = migrate_favorites(dir.path()).unwrap();

        let store = ProjectsStore::open(dir.path());
        assert_eq!(
            migrated,
            Some(Migrated {
                project: 1,
                global: 0
            })
        );
        assert_eq!(
            store.get(&wp, "wp").saved,
            [saved("f1", "Open orders", "SELECT 1")]
        );
        assert!(store.get(&old, "old").saved.is_empty());
        assert!(store.global().is_empty());
    }

    #[tokio::test]
    async fn a_table_favorite_becomes_a_select_in_the_project_of_its_database() {
        let dir = tempfile::tempdir().unwrap();
        let shop = touch(dir.path(), &connection("shop")).await;
        let wp = touch(dir.path(), &connection("wp")).await;
        let scope = account();
        write_favorites(
            dir.path(),
            vec![
                table_favorite("t1", Some(&scope), "shop", "orders"),
                table_favorite("t2", Some(&scope), "shop", "users"),
                table_favorite("t3", Some(&scope), "other", "logs"),
            ],
        );

        let migrated = migrate_favorites(dir.path()).unwrap();

        let store = ProjectsStore::open(dir.path());
        assert_eq!(
            migrated,
            Some(Migrated {
                project: 3,
                global: 0
            })
        );
        assert_eq!(
            store.get(&shop, "shop").saved,
            [
                saved("t1", "orders", "SELECT * FROM `orders`"),
                saved("t2", "users", "SELECT * FROM `users`"),
            ]
        );
        assert_eq!(
            store.get(&wp, "other").saved,
            [saved("t3", "logs", "SELECT * FROM `logs`")]
        );
    }

    #[tokio::test]
    async fn global_and_unmatched_favorites_go_to_the_global_list_with_tables_qualified() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path(),
            &ConnectionSpec {
                database: None,
                ..connection("unused")
            },
        )
        .await;
        write_favorites(
            dir.path(),
            vec![
                table_favorite("g1", None, "shop", "orders"),
                query_favorite("g2", "Elsewhere", Some("nobody@elsewhere:1"), "SELECT 2"),
                query_favorite("g3", "No database", Some(&account()), "SELECT 3"),
                table_favorite("g4", Some("nobody@elsewhere:1"), "wp", "posts"),
            ],
        );

        let migrated = migrate_favorites(dir.path()).unwrap();

        assert_eq!(
            migrated,
            Some(Migrated {
                project: 0,
                global: 4
            })
        );
        assert_eq!(
            ProjectsStore::open(dir.path()).global(),
            [
                saved("g1", "orders", "SELECT * FROM `shop`.`orders`"),
                saved("g2", "Elsewhere", "SELECT 2"),
                saved("g3", "No database", "SELECT 3"),
                saved("g4", "posts", "SELECT * FROM `wp`.`posts`"),
            ]
        );
    }

    #[tokio::test]
    async fn the_file_is_renamed_and_a_second_run_moves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let wp = touch(dir.path(), &connection("wp")).await;
        write_favorites(
            dir.path(),
            vec![
                query_favorite("f1", "Mine", Some(&account()), "SELECT 1"),
                query_favorite("f2", "Shared", None, "SELECT 2"),
            ],
        );
        let original = fs::read_to_string(dir.path().join(FAVORITES_FILE)).unwrap();

        let first = migrate_favorites(dir.path()).unwrap();
        let second = migrate_favorites(dir.path()).unwrap();

        let store = ProjectsStore::open(dir.path());
        assert_eq!(
            first,
            Some(Migrated {
                project: 1,
                global: 1
            })
        );
        assert_eq!(second, None);
        assert!(!dir.path().join(FAVORITES_FILE).exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(MIGRATED_FILE)).unwrap(),
            original
        );
        assert_eq!(
            store.get(&wp, "wp").saved,
            [saved("f1", "Mine", "SELECT 1")]
        );
        assert_eq!(store.global(), [saved("f2", "Shared", "SELECT 2")]);
    }

    #[tokio::test]
    async fn migrated_favorites_are_appended_to_existing_saved_queries() {
        let dir = tempfile::tempdir().unwrap();
        let wp = touch(dir.path(), &connection("wp")).await;
        let store = ProjectsStore::open(dir.path());
        store
            .update(&wp, "wp", |project| {
                project.saved.push(saved("kept", "Kept", "SELECT 0"));
                Ok(())
            })
            .await
            .unwrap();
        store
            .update_global(|global| {
                global.push(saved("kept-global", "Kept", "SELECT 0"));
                Ok(())
            })
            .await
            .unwrap();
        write_favorites(
            dir.path(),
            vec![
                query_favorite("f1", "Mine", Some(&account()), "SELECT 1"),
                query_favorite("f2", "Shared", None, "SELECT 2"),
            ],
        );

        migrate_favorites(dir.path()).unwrap();

        let reopened = ProjectsStore::open(dir.path());
        assert_eq!(
            reopened.get(&wp, "wp").saved,
            [
                saved("kept", "Kept", "SELECT 0"),
                saved("f1", "Mine", "SELECT 1")
            ]
        );
        assert_eq!(
            reopened.global(),
            [
                saved("kept-global", "Kept", "SELECT 0"),
                saved("f2", "Shared", "SELECT 2")
            ]
        );
    }

    #[test]
    fn without_a_favorites_file_there_is_nothing_to_migrate() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(migrate_favorites(dir.path()).unwrap(), None);
        assert!(!dir.path().join(MIGRATED_FILE).exists());
        assert!(!dir.path().join("projects.json").exists());
    }

    #[test]
    fn an_unreadable_favorites_file_migrates_nothing_and_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FAVORITES_FILE), "not json").unwrap();

        let migrated = migrate_favorites(dir.path()).unwrap();

        assert_eq!(migrated, Some(Migrated::default()));
        assert!(!dir.path().join(FAVORITES_FILE).exists());
        assert!(ProjectsStore::open(dir.path()).global().is_empty());
        assert_eq!(migrate_favorites(dir.path()).unwrap(), None);
    }
}
