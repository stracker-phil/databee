//! In-memory `Database` for unit tests; not compiled into the app.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use super::browse::browse_sql;
use super::{
    BrowsePage, BrowseRequest, CellLimit, Database, DatabaseInfo, Dialect, ExecuteResponse,
    ResultSet, TableInfo, TableKeys, TableStructure,
};
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// Answers queries from canned results, matched by a fragment of the SQL text.
pub struct FakeDatabase {
    database: Mutex<Option<String>>,
    responses: Vec<(String, AppResult<ResultSet>)>,
    queries: Mutex<Vec<String>>,
    applied: Mutex<Vec<Vec<String>>>,
    /// Structures by database and table, behind `tables`, `structure` and `keys`.
    schema: Mutex<BTreeMap<(String, String), TableStructure>>,
    closed: AtomicBool,
}

impl Default for FakeDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDatabase {
    pub fn new() -> Self {
        Self {
            database: Mutex::new(None),
            responses: Vec::new(),
            queries: Mutex::default(),
            applied: Mutex::default(),
            schema: Mutex::default(),
            closed: AtomicBool::new(false),
        }
    }

    pub fn with_database(self, database: &str) -> Self {
        *self.database.lock_unpoisoned() = Some(database.to_string());
        self
    }

    /// Declares `database`.`table` with `structure`, for `tables`, `structure` and `keys`.
    pub fn with_table(self, database: &str, table: &str, structure: TableStructure) -> Self {
        self.set_table(database, table, structure);
        self
    }

    /// Adds or replaces a table after the fake was handed to a session, like DDL would.
    pub fn set_table(&self, database: &str, table: &str, structure: TableStructure) {
        self.schema
            .lock_unpoisoned()
            .insert((database.to_string(), table.to_string()), structure);
    }

    /// SQL containing `sql_contains` gets `result`; the first registered match wins.
    pub fn on(mut self, sql_contains: &str, result: ResultSet) -> Self {
        self.responses.push((sql_contains.to_string(), Ok(result)));
        self
    }

    pub fn failing(mut self, sql_contains: &str, error: AppError) -> Self {
        self.responses.push((sql_contains.to_string(), Err(error)));
        self
    }

    /// Every SQL text seen by `execute`, `fetch` and `browse`, in order.
    pub fn queries(&self) -> Vec<String> {
        self.queries.lock_unpoisoned().clone()
    }

    /// The statements of every `apply` call, in order.
    pub fn applied(&self) -> Vec<Vec<String>> {
        self.applied.lock_unpoisoned().clone()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn respond(&self, sql: &str) -> AppResult<ResultSet> {
        self.queries.lock_unpoisoned().push(sql.to_string());
        self.responses
            .iter()
            .find(|(fragment, _)| sql.contains(fragment.as_str()))
            .map(|(_, result)| result.clone())
            .unwrap_or_else(|| Err(AppError::InvalidInput(format!("no fake result for {sql}"))))
    }
}

#[async_trait]
impl Database for FakeDatabase {
    fn dialect(&self) -> Dialect {
        Dialect::MYSQL
    }

    fn server_version(&self) -> &str {
        "fake"
    }

    fn current_database(&self) -> Option<String> {
        self.database.lock_unpoisoned().clone()
    }

    async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    async fn execute(
        &self,
        sql: &str,
        _row_limit: usize,
        _cell_limit: CellLimit,
        _timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        Ok(ExecuteResponse {
            results: vec![self.respond(sql)?],
            elapsed_ms: 0,
            database: self.current_database(),
            failure: None,
        })
    }

    async fn cancel(&self) -> AppResult<()> {
        Ok(())
    }

    async fn use_database(&self, database: &str) -> AppResult<()> {
        *self.database.lock_unpoisoned() = Some(database.to_string());
        Ok(())
    }

    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        Ok(self
            .current_database()
            .map(|name| DatabaseInfo {
                name,
                is_system: false,
            })
            .into_iter()
            .collect())
    }

    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        let schema = self.schema.lock_unpoisoned();
        let tables = schema
            .keys()
            .filter(|(db, _)| db == database)
            .map(|(_, table)| TableInfo {
                name: table.clone(),
                is_view: false,
                rows_estimate: None,
                size_bytes: None,
                comment: String::new(),
            });
        Ok(tables.collect())
    }

    /// The declared structure; an undeclared table has no columns and no keys.
    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        let key = (database.to_string(), table.to_string());
        let declared = self.schema.lock_unpoisoned().get(&key).cloned();
        Ok(declared.unwrap_or(TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: Vec::new(),
            foreign_keys: Vec::new(),
            ddl: None,
        }))
    }

    async fn keys(&self, database: &str) -> AppResult<BTreeMap<String, TableKeys>> {
        let schema = self.schema.lock_unpoisoned();
        let keys = schema
            .iter()
            .filter(|((db, _), s)| {
                let keyless =
                    s.primary_key.is_empty() && s.indexes.is_empty() && s.foreign_keys.is_empty();
                db == database && !keyless
            })
            .map(|((_, table), s)| {
                let keys = TableKeys {
                    primary_key: s.primary_key.clone(),
                    indexes: s.indexes.clone(),
                    foreign_keys: s.foreign_keys.clone(),
                };
                (table.clone(), keys)
            });
        Ok(keys.collect())
    }

    /// The column names of the declared structures; a table declared without columns is absent.
    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        let schema = self.schema.lock_unpoisoned();
        let columns = schema
            .iter()
            .filter(|((db, _), s)| db == database && !s.columns.is_empty())
            .map(|((_, table), s)| {
                let names = s.columns.iter().map(|c| c.name.clone()).collect();
                (table.clone(), names)
            });
        Ok(columns.collect())
    }

    async fn browse(
        &self,
        req: &BrowseRequest,
        _cell_limit: CellLimit,
        _timeout: Duration,
    ) -> AppResult<BrowsePage> {
        let (sql, filter) = browse_sql(req, Dialect::MYSQL)?;
        Ok(BrowsePage {
            table: req.table.clone(),
            result: self.respond(&sql)?,
            keys: Vec::new(),
            has_more: false,
            limit: req.limit,
            filter,
            columns_filter: super::browse::columns_filter(req, Dialect::MYSQL),
        })
    }

    async fn fetch(&self, sql: &str, _timeout: Duration) -> AppResult<ResultSet> {
        self.respond(sql)
    }

    async fn apply(&self, statements: &[String], _timeout: Duration) -> AppResult<u64> {
        self.applied.lock_unpoisoned().push(statements.to_vec());
        Ok(statements.len() as u64)
    }
}

/// Lets a test keep a handle on the fake after a session has taken ownership of it.
#[async_trait]
impl Database for Arc<FakeDatabase> {
    fn dialect(&self) -> Dialect {
        (**self).dialect()
    }

    fn server_version(&self) -> &str {
        (**self).server_version()
    }

    fn current_database(&self) -> Option<String> {
        (**self).current_database()
    }

    async fn close(&self) {
        (**self).close().await
    }

    async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        (**self).execute(sql, row_limit, cell_limit, timeout).await
    }

    async fn cancel(&self) -> AppResult<()> {
        (**self).cancel().await
    }

    async fn use_database(&self, database: &str) -> AppResult<()> {
        (**self).use_database(database).await
    }

    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        (**self).databases().await
    }

    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        (**self).tables(database).await
    }

    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        (**self).structure(database, table).await
    }

    async fn keys(&self, database: &str) -> AppResult<BTreeMap<String, TableKeys>> {
        (**self).keys(database).await
    }

    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        (**self).completion(database).await
    }

    async fn browse(
        &self,
        req: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<BrowsePage> {
        (**self).browse(req, cell_limit, timeout).await
    }

    async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet> {
        (**self).fetch(sql, timeout).await
    }

    async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        (**self).apply(statements, timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ForeignKey;
    use crate::error::ErrorKind;

    const TIMEOUT: Duration = Duration::from_secs(1);

    fn rows_affected(n: u64) -> ResultSet {
        ResultSet {
            affected_rows: n,
            ..ResultSet::default()
        }
    }

    #[tokio::test]
    async fn registered_fragment_returns_its_result_from_fetch() {
        let db = FakeDatabase::new().on("FROM wp_posts", rows_affected(3));

        let set = db.fetch("SELECT * FROM wp_posts", TIMEOUT).await.unwrap();

        assert_eq!(set, rows_affected(3));
    }

    #[tokio::test]
    async fn first_registered_match_wins_when_two_fragments_match() {
        let db = FakeDatabase::new()
            .on("wp_posts", rows_affected(1))
            .on("FROM wp_posts", rows_affected(2));

        let set = db.fetch("SELECT * FROM wp_posts", TIMEOUT).await.unwrap();

        assert_eq!(set.affected_rows, 1);
    }

    #[tokio::test]
    async fn unmatched_sql_is_an_invalid_input_error() {
        let db = FakeDatabase::new().on("wp_posts", rows_affected(1));

        let err = db.fetch("SELECT 1", TIMEOUT).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn failing_fragment_returns_the_given_error() {
        let db = FakeDatabase::new().failing("wp_posts", AppError::Cancelled);

        let err = db
            .fetch("SELECT * FROM wp_posts", TIMEOUT)
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::Cancelled), "{err:?}");
    }

    #[tokio::test]
    async fn apply_records_statement_batches_in_order() {
        let db = FakeDatabase::new();
        let first = vec!["UPDATE a SET x = 1".to_string()];
        let second = vec!["DELETE FROM b".to_string(), "DELETE FROM c".to_string()];

        assert_eq!(db.apply(&first, TIMEOUT).await.unwrap(), 1);
        assert_eq!(db.apply(&second, TIMEOUT).await.unwrap(), 2);

        assert_eq!(db.applied(), vec![first, second]);
    }

    fn structure(primary_key: &[&str], foreign_keys: Vec<ForeignKey>) -> TableStructure {
        TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: primary_key.iter().map(ToString::to_string).collect(),
            foreign_keys,
            ddl: None,
        }
    }

    #[tokio::test]
    async fn declared_tables_drive_tables_structure_and_keys() {
        let fk = ForeignKey {
            name: "books_author".into(),
            columns: vec!["author_id".into()],
            ref_table: "authors".into(),
            ref_columns: vec!["id".into()],
        };
        let books = structure(&["id"], vec![fk.clone()]);
        let db = FakeDatabase::new()
            .with_table("shop", "authors", structure(&["id"], Vec::new()))
            .with_table("shop", "books", books.clone())
            .with_table("other", "elsewhere", structure(&["id"], Vec::new()));

        let names: Vec<_> = db
            .tables("shop")
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        let keys = db.keys("shop").await.unwrap();

        assert_eq!(names, ["authors", "books"]);
        assert_eq!(db.structure("shop", "books").await.unwrap(), books);
        assert_eq!(keys["books"].primary_key, ["id"]);
        assert_eq!(keys["books"].foreign_keys, [fk]);
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn keyless_table_is_listed_but_absent_from_keys() {
        let db = FakeDatabase::new().with_table("shop", "log", structure(&[], Vec::new()));

        let tables = db.tables("shop").await.unwrap();
        let keys = db.keys("shop").await.unwrap();

        assert_eq!(tables.len(), 1);
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn queries_lists_the_sql_seen_in_order() {
        let db = FakeDatabase::new().on("t", rows_affected(0));

        db.fetch("SELECT 1 FROM t", TIMEOUT).await.unwrap();
        db.execute("SELECT 2 FROM t", 10, CellLimit::NONE, TIMEOUT)
            .await
            .unwrap();

        assert_eq!(db.queries(), vec!["SELECT 1 FROM t", "SELECT 2 FROM t"]);
    }
}
