//! The dedicated connection user queries run on, so session state (`USE`, variables,
//! transactions) survives between runs.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sqlx::mysql::{MySqlConnectOptions, MySqlConnection};
use sqlx::{AssertSqlSafe, ConnectOptions};
use tokio::sync::{MappedMutexGuard, MutexGuard};

use super::script::{read_dialect, scalar, thread_id};
use super::timeouts::connect_within;
use super::with_database;
use crate::db::Dialect;
use crate::error::{AppError, AppResult};
use crate::lock::MutexExt;

pub struct EditorConnection {
    /// `None` once the session is closed.
    conn: tokio::sync::Mutex<Option<MySqlConnection>>,
    /// Server thread of `conn`, the target of `KILL`.
    thread_id: AtomicU64,
    /// Without a database, so `KILL` still connects after the current one was dropped.
    options: MySqlConnectOptions,
    database: Mutex<Option<String>>,
    connect_timeout: Duration,
}

/// What a fresh editor connection reports about its server.
pub struct ServerInfo {
    pub version: String,
    pub dialect: Dialect,
}

impl EditorConnection {
    pub async fn connect(
        options: MySqlConnectOptions,
        database: Option<String>,
        connect_timeout: Duration,
    ) -> AppResult<(Self, ServerInfo)> {
        let mut conn = with_database(&options, database.as_deref())
            .connect()
            .await?;
        let thread_id = thread_id(&mut conn).await?;
        let version = scalar(&mut conn, "SELECT VERSION()").await?;
        let dialect = read_dialect(&mut conn).await?;
        let editor = Self {
            conn: tokio::sync::Mutex::new(Some(conn)),
            thread_id: AtomicU64::new(thread_id),
            options,
            database: Mutex::new(database),
            connect_timeout,
        };
        let info = ServerInfo {
            version: version.unwrap_or_default(),
            dialect,
        };
        Ok((editor, info))
    }

    pub async fn lock(&self) -> AppResult<MappedMutexGuard<'_, MySqlConnection>> {
        MutexGuard::try_map(self.conn.lock().await, Option::as_mut)
            .map_err(|_| AppError::Connection("session closed".into()))
    }

    /// Ends a running query, then closes the connection; later calls fail.
    pub async fn close(&self) {
        if let Err(e) = self.kill_query().await {
            log::debug!("Could not interrupt the editor connection before closing: {e}");
        }
        let Some(conn) = self.conn.lock().await.take() else {
            return;
        };
        if let Err(e) = sqlx::Connection::close(conn).await {
            log::debug!("Could not close the editor connection cleanly: {e}");
        }
    }

    pub fn current_database(&self) -> Option<String> {
        self.database.lock_unpoisoned().clone()
    }

    pub fn set_database(&self, database: Option<String>) {
        *self.database.lock_unpoisoned() = database;
    }

    /// Interrupts the running statement; the connection stays usable.
    pub async fn kill_query(&self) -> AppResult<()> {
        self.kill("KILL QUERY").await
    }

    pub async fn kill_connection(&self) -> AppResult<()> {
        self.kill("KILL").await
    }

    /// Runs on a one-off connection, so it works even while every pool connection is busy.
    async fn kill(&self, command: &str) -> AppResult<()> {
        let sql = format!("{command} {}", self.thread_id.load(Ordering::SeqCst));
        connect_within(self.connect_timeout, async {
            let mut conn = self.options.connect().await?;
            sqlx::raw_sql(AssertSqlSafe(sql)).execute(&mut conn).await?;
            sqlx::Connection::close(conn).await?;
            Ok(())
        })
        .await
    }

    /// Replaces the connection with a fresh one in the current database and returns its
    /// dialect. `conn` is the guard from `lock()`, which the caller keeps holding.
    pub async fn reconnect(&self, conn: &mut MySqlConnection) -> AppResult<Dialect> {
        let options = with_database(&self.options, self.current_database().as_deref());
        let reconnect = async {
            let mut fresh = options.connect().await?;
            let id = thread_id(&mut fresh).await?;
            let dialect = read_dialect(&mut fresh).await?;
            Ok((fresh, id, dialect))
        };
        let (fresh, id, dialect) = connect_within(self.connect_timeout, reconnect).await?;
        self.thread_id.store(id, Ordering::SeqCst);
        *conn = fresh;
        Ok(dialect)
    }
}
