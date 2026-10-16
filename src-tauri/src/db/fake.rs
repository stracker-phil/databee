//! In-memory `Database` for unit tests; not compiled into the app.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use super::browse::browse_sql;
use super::{
    BrowsePage, BrowseRequest, Database, DatabaseInfo, Dialect, ExecuteResponse, ResultSet,
    TableInfo, TableStructure,
};
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

/// Answers queries from canned results, matched by a fragment of the SQL text.
pub struct FakeDatabase {
    database: Mutex<Option<String>>,
    responses: Vec<(String, AppResult<ResultSet>)>,
    queries: Mutex<Vec<String>>,
    applied: Mutex<Vec<Vec<String>>>,
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
            closed: AtomicBool::new(false),
        }
    }

    pub fn with_database(self, database: &str) -> Self {
        *self.database.lock_unpoisoned() = Some(database.to_string());
        self
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
        _timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        Ok(ExecuteResponse {
            results: vec![self.respond(sql)?],
            elapsed_ms: 0,
            database: self.current_database(),
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

    async fn tables(&self, _database: &str) -> AppResult<Vec<TableInfo>> {
        Ok(Vec::new())
    }

    async fn structure(&self, _database: &str, _table: &str) -> AppResult<TableStructure> {
        Ok(TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            ddl: None,
        })
    }

    async fn completion(&self, _database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        Ok(BTreeMap::new())
    }

    async fn browse(&self, req: &BrowseRequest, _timeout: Duration) -> AppResult<BrowsePage> {
        let (sql, filter) = browse_sql(req, Dialect::MYSQL)?;
        Ok(BrowsePage {
            result: self.respond(&sql)?,
            has_more: false,
            limit: req.limit,
            filter,
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
        timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        (**self).execute(sql, row_limit, timeout).await
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

    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        (**self).completion(database).await
    }

    async fn browse(&self, req: &BrowseRequest, timeout: Duration) -> AppResult<BrowsePage> {
        (**self).browse(req, timeout).await
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

        assert_eq!(err.kind(), "invalid_input");
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

    #[tokio::test]
    async fn queries_lists_the_sql_seen_in_order() {
        let db = FakeDatabase::new().on("t", rows_affected(0));

        db.fetch("SELECT 1 FROM t", TIMEOUT).await.unwrap();
        db.execute("SELECT 2 FROM t", 10, TIMEOUT).await.unwrap();

        assert_eq!(db.queries(), vec!["SELECT 1 FROM t", "SELECT 2 FROM t"]);
    }
}
