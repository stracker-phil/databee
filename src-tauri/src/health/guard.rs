use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::{SessionHealth, check};
use crate::db::{
    BrowsePage, BrowseRequest, CellLimit, Database, DatabaseInfo, Dialect, ExecuteResponse,
    ResultSet, TableInfo, TableKeys, TableStructure,
};
use crate::error::{AppError, AppResult};

/// A session's database as every caller sees it: while the session is not `ok`, a call that
/// needs the server fails at once with `ConnectionLost` instead of waiting on a dead socket, and
/// an error that suggests a lost connection starts a check right away.
pub struct Guarded {
    inner: Arc<dyn Database>,
    health: Arc<SessionHealth>,
}

impl Guarded {
    pub fn new(inner: Box<dyn Database>, health: Arc<SessionHealth>) -> Self {
        Self {
            inner: Arc::from(inner),
            health,
        }
    }

    fn admit(&self) -> AppResult<()> {
        self.health.refusal().map_or(Ok(()), Err)
    }

    fn observe<T>(&self, result: AppResult<T>) -> AppResult<T> {
        if let Err(error) = &result {
            self.suspect(error);
        }
        result
    }

    /// The check runs apart from the call, so the caller gets its error without waiting for a ping.
    fn suspect(&self, error: &AppError) {
        if !error.suggests_connection_loss() {
            return;
        }
        let (db, health) = (self.inner.clone(), self.health.clone());
        tokio::spawn(async move { check(db.as_ref(), &health).await });
    }
}

#[async_trait]
impl Database for Guarded {
    fn dialect(&self) -> Dialect {
        self.inner.dialect()
    }

    fn server_version(&self) -> &str {
        self.inner.server_version()
    }

    fn current_database(&self) -> Option<String> {
        self.inner.current_database()
    }

    async fn close(&self) {
        self.inner.close().await
    }

    async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        self.admit()?;
        let response =
            self.observe(self.inner.execute(sql, row_limit, cell_limit, timeout).await)?;
        if let Some(failure) = &response.failure {
            self.suspect(&failure.error);
        }
        Ok(response)
    }

    async fn cancel(&self) -> AppResult<()> {
        self.inner.cancel().await
    }

    async fn use_database(&self, database: &str) -> AppResult<()> {
        self.admit()?;
        self.observe(self.inner.use_database(database).await)
    }

    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        self.admit()?;
        self.observe(self.inner.databases().await)
    }

    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        self.admit()?;
        self.observe(self.inner.tables(database).await)
    }

    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        self.admit()?;
        self.observe(self.inner.structure(database, table).await)
    }

    async fn keys(&self, database: &str) -> AppResult<BTreeMap<String, TableKeys>> {
        self.admit()?;
        self.observe(self.inner.keys(database).await)
    }

    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        self.admit()?;
        self.observe(self.inner.completion(database).await)
    }

    async fn browse(
        &self,
        req: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<BrowsePage> {
        self.admit()?;
        self.observe(self.inner.browse(req, cell_limit, timeout).await)
    }

    async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet> {
        self.admit()?;
        self.observe(self.inner.fetch(sql, timeout).await)
    }

    async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        self.admit()?;
        self.observe(self.inner.apply(statements, timeout).await)
    }

    async fn ping(&self) -> AppResult<()> {
        self.inner.ping().await
    }

    async fn reconnect(&self) -> AppResult<()> {
        self.inner.reconnect().await
    }
}
