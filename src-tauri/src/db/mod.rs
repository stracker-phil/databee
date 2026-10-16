pub mod browse;
pub mod cell;
pub mod classify;
pub mod dialect;
#[cfg(any(test, feature = "test-util"))]
mod fake;
pub mod id_filter;
pub mod lexer;
pub mod mysql;
pub mod safety;
mod types;

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;

pub use cell::Cell;
pub use dialect::{Dialect, escape_like};
#[cfg(any(test, feature = "test-util"))]
pub use fake::FakeDatabase;
pub use types::*;

use crate::connection::{ConnectionSpec, Secret};
use crate::error::AppResult;

/// One open connection to a database server, whatever the engine (ADR 0011).
#[async_trait]
pub trait Database: Send + Sync {
    /// How identifiers and string literals for this server are spelled.
    fn dialect(&self) -> Dialect;
    fn server_version(&self) -> &str;
    fn current_database(&self) -> Option<String>;
    async fn close(&self);
    /// Runs a user script on the editor connection, which keeps session state.
    async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse>;
    async fn cancel(&self) -> AppResult<()>;
    async fn use_database(&self, database: &str) -> AppResult<()>;
    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>>;
    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>>;
    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure>;
    /// Table → column names, for editor autocompletion.
    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>>;
    async fn browse(&self, req: &BrowseRequest, timeout: Duration) -> AppResult<BrowsePage>;
    /// One read-only query off the editor connection.
    async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet>;
    /// Runs the statements in one transaction; returns the affected row count.
    async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64>;
}

pub async fn open(
    spec: &ConnectionSpec,
    password: Option<&Secret>,
    connect_timeout: Duration,
    query_timeout: Duration,
) -> AppResult<Box<dyn Database>> {
    let session =
        mysql::MySqlSession::connect(spec, password, connect_timeout, query_timeout).await?;
    Ok(Box::new(session))
}
