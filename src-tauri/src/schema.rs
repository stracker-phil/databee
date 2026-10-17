//! The per-session schema cache: per database the tables, every table's column names and keys
//! (loaded in bulk), foreign keys by referenced table, and table structures loaded on first use.
//!
//! Only DDL changes what it holds, so it is kept until the session runs a script with DDL
//! (`Session::ddl_writes`) or the user refreshes (`refresh`). DDL from outside the app shows
//! after a refresh.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::db::{ForeignKey, TableInfo, TableKeys, TableStructure};
use crate::error::AppResult;
use crate::lock::MutexExt;
use crate::session::Session;

/// What one database looked like when it was loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseSchema {
    /// Tables and views by name; row estimates and sizes are as of the load.
    pub tables: Vec<TableInfo>,
    /// Keys of every table in `tables`, empty for a table without any.
    pub keys: BTreeMap<String, TableKeys>,
    /// Column names of every table, in table order; same-name inference reads them.
    pub columns: BTreeMap<String, Vec<String>>,
    /// Foreign keys that point at each table, by referenced table.
    pub incoming: BTreeMap<String, Vec<IncomingKey>>,
}

/// A foreign key of `table` that points at the table it is listed under.
#[derive(Debug, Clone, PartialEq)]
pub struct IncomingKey {
    pub table: String,
    pub foreign_key: ForeignKey,
}

impl DatabaseSchema {
    fn new(
        tables: Vec<TableInfo>,
        mut keys: BTreeMap<String, TableKeys>,
        columns: BTreeMap<String, Vec<String>>,
    ) -> Self {
        for table in &tables {
            keys.entry(table.name.clone()).or_default();
        }
        let mut incoming = BTreeMap::<String, Vec<IncomingKey>>::new();
        for (table, table_keys) in &keys {
            for foreign_key in &table_keys.foreign_keys {
                incoming
                    .entry(foreign_key.ref_table.clone())
                    .or_default()
                    .push(IncomingKey {
                        table: table.clone(),
                        foreign_key: foreign_key.clone(),
                    });
            }
        }
        Self {
            tables,
            keys,
            columns,
            incoming,
        }
    }

    pub fn table(&self, name: &str) -> Option<&TableInfo> {
        self.tables.iter().find(|t| t.name == name)
    }

    pub fn keys(&self, table: &str) -> Option<&TableKeys> {
        self.keys.get(table)
    }

    pub fn columns(&self, table: &str) -> &[String] {
        self.columns.get(table).map_or(&[], Vec::as_slice)
    }

    pub fn incoming(&self, table: &str) -> &[IncomingKey] {
        self.incoming.get(table).map_or(&[], Vec::as_slice)
    }
}

/// The DDL count and refresh count an entry was loaded under; any change makes it stale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Stamp {
    ddl_writes: u64,
    refreshes: u64,
}

#[derive(Default)]
struct Entry {
    stamp: Stamp,
    schema: Option<Arc<DatabaseSchema>>,
    structures: HashMap<String, Arc<TableStructure>>,
}

/// Lives in `Session.ext` (ADR 0013), so it dies with the session.
#[derive(Default)]
pub struct SchemaCache {
    databases: Mutex<HashMap<String, Entry>>,
    /// Part of the stamp, so a load that overlapped a refresh is not kept.
    refreshes: AtomicU64,
}

impl SchemaCache {
    fn of(session: &Session) -> Arc<SchemaCache> {
        session.ext.get_or_insert_with(SchemaCache::default)
    }

    fn stamp(&self, session: &Session) -> Stamp {
        Stamp {
            ddl_writes: session.ddl_writes(),
            refreshes: self.refreshes.load(Ordering::SeqCst),
        }
    }

    fn cached<T>(
        &self,
        database: &str,
        stamp: Stamp,
        pick: impl FnOnce(&Entry) -> Option<T>,
    ) -> Option<T> {
        let databases = self.databases.lock_unpoisoned();
        let entry = databases.get(database)?;
        if entry.stamp != stamp {
            return None;
        }
        pick(entry)
    }

    /// Stores what was loaded under `stamp`, unless DDL or a refresh happened since.
    fn store(&self, session: &Session, database: &str, stamp: Stamp, put: impl FnOnce(&mut Entry)) {
        if stamp != self.stamp(session) {
            return;
        }
        let mut databases = self.databases.lock_unpoisoned();
        let entry = databases.entry(database.to_string()).or_default();
        if entry.stamp != stamp {
            *entry = Entry {
                stamp,
                ..Entry::default()
            };
        }
        put(entry);
    }
}

/// The tables, columns and keys of `database`: four queries on first use, then from the cache.
pub async fn database(session: &Session, database: &str) -> AppResult<Arc<DatabaseSchema>> {
    let cache = SchemaCache::of(session);
    let stamp = cache.stamp(session);
    if let Some(schema) = cache.cached(database, stamp, |e| e.schema.clone()) {
        return Ok(schema);
    }
    let (tables, keys, columns) = tokio::try_join!(
        session.db.tables(database),
        session.db.keys(database),
        session.db.completion(database)
    )?;
    let schema = Arc::new(DatabaseSchema::new(tables, keys, columns));
    cache.store(session, database, stamp, |e| {
        e.schema = Some(schema.clone())
    });
    Ok(schema)
}

/// The structure of one table, read from the server on first use.
pub async fn structure(
    session: &Session,
    database: &str,
    table: &str,
) -> AppResult<Arc<TableStructure>> {
    let cache = SchemaCache::of(session);
    let stamp = cache.stamp(session);
    if let Some(structure) = cache.cached(database, stamp, |e| e.structures.get(table).cloned()) {
        return Ok(structure);
    }
    let structure = Arc::new(session.db.structure(database, table).await?);
    cache.store(session, database, stamp, |e| {
        e.structures.insert(table.to_string(), structure.clone());
    });
    Ok(structure)
}

/// Reads a table's structure from the server and keeps it, for callers that want the current
/// `SHOW CREATE TABLE` text (its `AUTO_INCREMENT` moves with every insert).
pub async fn fresh_structure(
    session: &Session,
    database: &str,
    table: &str,
) -> AppResult<TableStructure> {
    let cache = SchemaCache::of(session);
    let stamp = cache.stamp(session);
    let structure = session.db.structure(database, table).await?;
    let kept = Arc::new(structure.clone());
    cache.store(session, database, stamp, |e| {
        e.structures.insert(table.to_string(), kept);
    });
    Ok(structure)
}

/// Forgets everything, so the next read sees DDL made outside the app.
pub fn refresh(session: &Session) {
    let cache = SchemaCache::of(session);
    cache.refreshes.fetch_add(1, Ordering::SeqCst);
    cache.databases.lock_unpoisoned().clear();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::connection::{ConnectionSpec, Secret};
    use crate::db::mysql::MySqlSession;
    use crate::db::{CellLimit, FakeDatabase, ResultSet};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const PORTS: [u16; 2] = [33306, 33307];

    fn spec(port: u16, database: &str) -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port,
            socket: None,
            user: "root".into(),
            database: Some(database.into()),
        }
    }

    fn fk(name: &str, column: &str, ref_table: &str) -> ForeignKey {
        ForeignKey {
            name: name.into(),
            columns: vec![column.into()],
            ref_table: ref_table.into(),
            ref_columns: vec!["id".into()],
        }
    }

    fn declared(ddl: &str, foreign_keys: Vec<ForeignKey>) -> TableStructure {
        TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: vec!["id".into()],
            foreign_keys,
            ddl: Some(ddl.into()),
        }
    }

    fn ddl_of(structure: &TableStructure) -> &str {
        structure.ddl.as_deref().unwrap()
    }

    fn shop() -> Arc<FakeDatabase> {
        Arc::new(
            FakeDatabase::new()
                .with_database("shop")
                .with_table("shop", "authors", declared("authors v1", Vec::new()))
                .with_table(
                    "shop",
                    "books",
                    declared("books v1", vec![fk("books_author", "author_id", "authors")]),
                )
                .with_table(
                    "shop",
                    "log",
                    TableStructure {
                        primary_key: Vec::new(),
                        ..declared("log v1", Vec::new())
                    },
                )
                .on("ALTER TABLE", ResultSet::default())
                .on("UPDATE", ResultSet::default()),
        )
    }

    fn session_over(db: &Arc<FakeDatabase>) -> Session {
        Session::new(
            "s".into(),
            spec(3306, "shop"),
            "local".into(),
            Box::new(db.clone()),
        )
    }

    async fn run(session: &Session, sql: &str) {
        session
            .execute(sql, 10, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();
    }

    fn names(schema: &DatabaseSchema) -> Vec<&str> {
        schema.tables.iter().map(|t| t.name.as_str()).collect()
    }

    #[tokio::test]
    async fn database_lists_declared_tables_with_their_keys() {
        let db = shop();
        let session = session_over(&db);

        let schema = database(&session, "shop").await.unwrap();

        assert_eq!(names(&schema), ["authors", "books", "log"]);
        assert_eq!(schema.keys("authors").unwrap().primary_key, ["id"]);
        assert_eq!(
            schema.keys("books").unwrap().foreign_keys,
            [fk("books_author", "author_id", "authors")]
        );
        assert_eq!(schema.keys("log"), Some(&TableKeys::default()));
        assert_eq!(schema.keys("missing"), None);
    }

    #[tokio::test]
    async fn incoming_lists_the_foreign_keys_that_point_at_a_table() {
        let db = shop();
        let session = session_over(&db);

        let schema = database(&session, "shop").await.unwrap();

        assert_eq!(
            schema.incoming("authors"),
            [IncomingKey {
                table: "books".into(),
                foreign_key: fk("books_author", "author_id", "authors"),
            }]
        );
        assert!(schema.incoming("books").is_empty());
    }

    #[tokio::test]
    async fn second_database_call_is_served_from_the_cache() {
        let db = shop();
        let session = session_over(&db);
        database(&session, "shop").await.unwrap();

        db.set_table("shop", "extra", declared("extra v1", Vec::new()));
        let schema = database(&session, "shop").await.unwrap();

        assert_eq!(names(&schema), ["authors", "books", "log"]);
    }

    #[tokio::test]
    async fn ddl_through_the_session_makes_the_next_read_see_the_new_schema() {
        let db = shop();
        let session = session_over(&db);
        database(&session, "shop").await.unwrap();
        structure_of(&session, "authors").await;

        db.set_table("shop", "extra", declared("extra v1", Vec::new()));
        db.set_table("shop", "authors", declared("authors v2", Vec::new()));
        run(&session, "ALTER TABLE authors ADD COLUMN x int").await;

        let schema = database(&session, "shop").await.unwrap();
        assert_eq!(names(&schema), ["authors", "books", "extra", "log"]);
        assert_eq!(
            ddl_of(&structure_of(&session, "authors").await),
            "authors v2"
        );
    }

    #[tokio::test]
    async fn statements_without_ddl_keep_the_cache() {
        let db = shop();
        let session = session_over(&db);
        database(&session, "shop").await.unwrap();

        db.set_table("shop", "extra", declared("extra v1", Vec::new()));
        run(&session, "UPDATE authors SET name = 'x' WHERE id = 1").await;

        let schema = database(&session, "shop").await.unwrap();
        assert_eq!(names(&schema), ["authors", "books", "log"]);
    }

    #[tokio::test]
    async fn refresh_makes_changes_from_outside_visible() {
        let db = shop();
        let session = session_over(&db);
        database(&session, "shop").await.unwrap();
        structure_of(&session, "authors").await;

        db.set_table("shop", "extra", declared("extra v1", Vec::new()));
        db.set_table("shop", "authors", declared("authors v2", Vec::new()));
        refresh(&session);

        let schema = database(&session, "shop").await.unwrap();
        assert_eq!(names(&schema), ["authors", "books", "extra", "log"]);
        assert_eq!(
            ddl_of(&structure_of(&session, "authors").await),
            "authors v2"
        );
    }

    async fn structure_of(session: &Session, table: &str) -> TableStructure {
        TableStructure::clone(&structure(session, "shop", table).await.unwrap())
    }

    #[tokio::test]
    async fn structure_is_read_once_and_then_cached() {
        let db = shop();
        let session = session_over(&db);
        assert_eq!(
            ddl_of(&structure_of(&session, "authors").await),
            "authors v1"
        );

        db.set_table("shop", "authors", declared("authors v2", Vec::new()));

        assert_eq!(
            ddl_of(&structure_of(&session, "authors").await),
            "authors v1"
        );
    }

    #[tokio::test]
    async fn fresh_structure_reads_the_server_and_updates_the_cache() {
        let db = shop();
        let session = session_over(&db);
        structure_of(&session, "authors").await;

        db.set_table("shop", "authors", declared("authors v2", Vec::new()));
        let fresh = fresh_structure(&session, "shop", "authors").await.unwrap();

        assert_eq!(ddl_of(&fresh), "authors v2");
        assert_eq!(
            ddl_of(&structure_of(&session, "authors").await),
            "authors v2"
        );
    }

    async fn fixture_session(port: u16, database: &str) -> Session {
        let spec = spec(port, database);
        let db = MySqlSession::connect(&spec, Some(&Secret::new("root")), TIMEOUT, TIMEOUT)
            .await
            .unwrap();
        Session::new("it".into(), spec, format!("it-{port}"), Box::new(db))
    }

    #[tokio::test]
    #[ignore]
    async fn incoming_lists_books_for_authors_on_the_fixture() {
        for port in PORTS {
            let session = fixture_session(port, "databee_test").await;

            let schema = database(&session, "databee_test").await.unwrap();

            assert_eq!(
                schema.incoming("authors"),
                [IncomingKey {
                    table: "books".into(),
                    foreign_key: fk("books_author", "author_id", "authors"),
                }],
                "{port}"
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn ddl_through_the_session_shows_in_the_structure_on_the_fixture() {
        for port in PORTS {
            let session = fixture_session(port, "databee_test").await;
            let has_probe = |s: &Arc<TableStructure>| s.columns.iter().any(|c| c.name == "probe");
            // A failed earlier run may have left the column behind.
            let _ = session
                .execute(
                    "ALTER TABLE databee_test.authors DROP COLUMN probe",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await;
            let before = structure(&session, "databee_test", "authors")
                .await
                .unwrap();
            assert!(!has_probe(&before), "{port}");

            let added = session
                .execute(
                    "ALTER TABLE databee_test.authors ADD COLUMN probe int",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await;
            let after_add = structure(&session, "databee_test", "authors").await;
            let dropped = session
                .execute(
                    "ALTER TABLE databee_test.authors DROP COLUMN probe",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await;
            let after_drop = structure(&session, "databee_test", "authors").await;

            added.unwrap();
            dropped.unwrap();
            assert!(has_probe(&after_add.unwrap()), "{port}");
            assert!(!has_probe(&after_drop.unwrap()), "{port}");
        }
    }
}
