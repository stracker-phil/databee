pub mod browse;
pub mod cell;
pub mod cell_cache;
pub mod changes;
pub mod classify;
pub mod dialect;
#[cfg(any(test, feature = "test-util"))]
mod fake;
pub mod id_filter;
pub mod lexer;
pub mod mysql;
pub mod plans;
pub mod safety;
mod types;

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;

pub use cell::{Cell, CellLimit};
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
    /// Ends a running query and closes every connection; later calls fail.
    async fn close(&self);
    /// Runs a user script on the editor connection, which keeps session state. A failure
    /// after the first statement comes back as `ExecuteResponse::failure` with the earlier
    /// results; a failure of the first statement is an `Err`.
    async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse>;
    async fn cancel(&self) -> AppResult<()>;
    async fn use_database(&self, database: &str) -> AppResult<()>;
    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>>;
    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>>;
    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure>;
    /// Table → column names, for editor autocompletion.
    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>>;
    async fn browse(
        &self,
        req: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<BrowsePage>;
    /// One read-only query off the editor connection; text cells come back in full.
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
