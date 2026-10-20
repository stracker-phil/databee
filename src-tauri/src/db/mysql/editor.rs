//! The dedicated connection user queries run on, so session state (`USE`, variables,
//! transactions) survives between runs.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sqlx::mysql::{MySqlConnectOptions, MySqlConnection};
use sqlx::{AssertSqlSafe, ConnectOptions};
use tokio::sync::{MappedMutexGuard, MutexGuard};

use super::kill::{KillTarget, Killer};
use super::script::{scalar, thread_id};
use super::session_state::{self, SessionState, restore_sql};
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
    /// Without a database; reconnects add the current one.
    options: MySqlConnectOptions,
    state: Mutex<Tracked>,
    connect_timeout: Duration,
    killer: Killer,
}

/// The session state of the live connection: as it opened, and as the last probe found it.
struct Tracked {
    opened: SessionState,
    current: SessionState,
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
        killer: Killer,
    ) -> AppResult<(Self, ServerInfo)> {
        let mut conn = with_database(&options, database.as_deref())
            .connect()
            .await?;
        let thread_id = thread_id(&mut conn).await?;
        let version = scalar(&mut conn, "SELECT VERSION()").await?;
        let opened = session_state::read(&mut conn).await?;
        let info = ServerInfo {
            version: version.unwrap_or_default(),
            dialect: opened.dialect(),
        };
        let current = SessionState {
            database,
            ..opened.clone()
        };
        let editor = Self {
            conn: tokio::sync::Mutex::new(Some(conn)),
            thread_id: AtomicU64::new(thread_id),
            killer,
            options,
            state: Mutex::new(Tracked { opened, current }),
            connect_timeout,
        };
        Ok((editor, info))
    }

    pub async fn lock(&self) -> AppResult<MappedMutexGuard<'_, MySqlConnection>> {
        MutexGuard::try_map(self.conn.lock().await, Option::as_mut)
            .map_err(|_| AppError::Connection("session closed".into()))
    }

    /// The connection, unless a query holds it or the session is closed.
    pub fn try_lock(&self) -> Option<MappedMutexGuard<'_, MySqlConnection>> {
        MutexGuard::try_map(self.conn.try_lock().ok()?, Option::as_mut).ok()
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
        self.state.lock_unpoisoned().current.database.clone()
    }

    pub fn set_database(&self, database: Option<String>) {
        self.state.lock_unpoisoned().current.database = database;
    }

    /// The state the probe after a user script found.
    pub fn remember(&self, state: SessionState) {
        self.state.lock_unpoisoned().current = state;
    }

    /// Interrupts the running statement; the connection stays usable.
    pub async fn kill_query(&self) -> AppResult<()> {
        let thread_id = self.thread_id.load(Ordering::SeqCst);
        self.killer.kill(KillTarget::Query, thread_id).await
    }

    pub async fn kill_connection(&self) -> AppResult<()> {
        let thread_id = self.thread_id.load(Ordering::SeqCst);
        self.killer.kill(KillTarget::Connection, thread_id).await
    }

    /// Replaces the connection with a fresh one in the current database, sets the session
    /// variables the user had changed again, and returns the dialect of the server's defaults
    /// (which the metadata pool shares). `conn` is the guard from `lock()`, which the caller
    /// keeps holding.
    pub async fn reconnect(&self, conn: &mut MySqlConnection) -> AppResult<Dialect> {
        let (lost_opened, lost_current) = {
            let state = self.state.lock_unpoisoned();
            (state.opened.clone(), state.current.clone())
        };
        let options = with_database(&self.options, lost_current.database.as_deref());
        let reconnect = async {
            let mut fresh = options.connect().await?;
            let id = thread_id(&mut fresh).await?;
            let opened = session_state::read(&mut fresh).await?;
            let current = restore(&mut fresh, &lost_opened, &lost_current, &opened).await;
            Ok((fresh, id, opened, current))
        };
        let (fresh, id, opened, current) = connect_within(self.connect_timeout, reconnect).await?;
        self.thread_id.store(id, Ordering::SeqCst);
        *conn = fresh;
        let dialect = opened.dialect();
        *self.state.lock_unpoisoned() = Tracked { opened, current };
        Ok(dialect)
    }
}

/// Sets the variables the lost connection had changed; a failure costs only them, logged.
async fn restore(
    fresh: &mut MySqlConnection,
    lost_opened: &SessionState,
    lost_current: &SessionState,
    opened: &SessionState,
) -> SessionState {
    let Some(sql) = restore_sql(lost_opened, lost_current, opened.dialect()) else {
        return opened.clone();
    };
    let restored = async {
        sqlx::raw_sql(AssertSqlSafe(sql))
            .execute(&mut *fresh)
            .await?;
        session_state::read(fresh).await
    };
    match restored.await {
        Ok(current) => {
            log::info!("Restored the session variables on the new editor connection");
            current
        }
        Err(e) => {
            log::warn!("Could not restore the session variables after reconnecting: {e}");
            opened.clone()
        }
    }
}
