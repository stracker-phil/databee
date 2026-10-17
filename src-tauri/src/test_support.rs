//! Helpers shared by unit tests and the integration tests in `tests/` (through the `test-util`
//! feature): the Docker fixture servers, a statement-logging `Database` wrapper and the plugin
//! command lists in `build.rs`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use crate::connection::{ConnectionSpec, Secret};
use crate::db::mysql::MySqlSession;
use crate::db::{
    BrowsePage, BrowseRequest, CellLimit, Database, DatabaseInfo, Dialect, ExecuteResponse,
    ResultSet, TableInfo, TableKeys, TableStructure,
};
use crate::error::AppResult;
use crate::lock::MutexExt;

const TIMEOUT: Duration = Duration::from_secs(10);
const BUILD_RS: &str = include_str!("../build.rs");

/// The fixture servers from `dev/docker-compose.yml`, or `DATABEE_TEST_SERVERS="host:port,…"`.
pub fn servers() -> Vec<(String, u16)> {
    std::env::var("DATABEE_TEST_SERVERS")
        .unwrap_or_else(|_| "127.0.0.1:33306,127.0.0.1:33307".into())
        .split(',')
        .map(|s| {
            let (host, port) = s.rsplit_once(':').unwrap();
            (host.to_string(), port.parse().unwrap())
        })
        .collect()
}

/// The fixtures' `root` account (password `root`) on one server.
pub fn spec(host: &str, port: u16, database: &str) -> ConnectionSpec {
    ConnectionSpec {
        name: None,
        host: host.into(),
        port,
        socket: None,
        user: "root".into(),
        database: Some(database.into()),
    }
}

pub async fn connect_to(host: &str, port: u16, database: &str) -> MySqlSession {
    MySqlSession::connect(
        &spec(host, port, database),
        Some(&Secret::new("root")),
        TIMEOUT,
        TIMEOUT,
    )
    .await
    .unwrap_or_else(|e| panic!("{host}:{port}: {e}"))
}

/// One session per fixture server, each on `database`.
pub async fn sessions(database: &str) -> Vec<MySqlSession> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        out.push(connect_to(&host, port, database).await);
    }
    out
}

/// The quoted names in `build.rs` that start with `prefix`, i.e. one plugin's granted commands.
pub fn build_rs_commands(prefix: &str) -> Vec<&'static str> {
    BUILD_RS
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| s.starts_with(prefix))
        .collect()
}

/// What a `Counting` database sent to the server: the SQL of each `execute`, `fetch` and
/// `browse`, and the name of each metadata call (`tables`, `keys`, …).
#[derive(Clone, Default)]
pub struct StatementLog(Arc<Mutex<Vec<String>>>);

impl StatementLog {
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock_unpoisoned())
    }

    fn push(&self, entry: impl Into<String>) {
        self.0.lock_unpoisoned().push(entry.into());
    }
}

/// Wraps a database and logs every call that reaches the server.
pub struct Counting {
    inner: Box<dyn Database>,
    log: StatementLog,
}

impl Counting {
    pub fn new(inner: Box<dyn Database>) -> (Self, StatementLog) {
        let log = StatementLog::default();
        let counting = Self {
            inner,
            log: log.clone(),
        };
        (counting, log)
    }
}

#[async_trait]
impl Database for Counting {
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
        self.log.push(sql);
        self.inner
            .execute(sql, row_limit, cell_limit, timeout)
            .await
    }

    async fn cancel(&self) -> AppResult<()> {
        self.inner.cancel().await
    }

    async fn use_database(&self, database: &str) -> AppResult<()> {
        self.log.push("use_database");
        self.inner.use_database(database).await
    }

    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        self.log.push("databases");
        self.inner.databases().await
    }

    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        self.log.push("tables");
        self.inner.tables(database).await
    }

    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        self.log.push(format!("structure {table}"));
        self.inner.structure(database, table).await
    }

    async fn keys(&self, database: &str) -> AppResult<BTreeMap<String, TableKeys>> {
        self.log.push("keys");
        self.inner.keys(database).await
    }

    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        self.log.push("completion");
        self.inner.completion(database).await
    }

    async fn browse(
        &self,
        req: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<BrowsePage> {
        self.log.push(format!("browse {}", req.table));
        self.inner.browse(req, cell_limit, timeout).await
    }

    async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet> {
        self.log.push(sql);
        self.inner.fetch(sql, timeout).await
    }

    async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        for sql in statements {
            self.log.push(sql.as_str());
        }
        self.inner.apply(statements, timeout).await
    }
}
