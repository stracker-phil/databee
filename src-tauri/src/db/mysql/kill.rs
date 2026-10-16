//! `KILL` on a one-off connection, so it works while the target holds the editor lock and
//! every pool connection is busy.

use std::time::Duration;

use sqlx::mysql::MySqlConnectOptions;
use sqlx::{AssertSqlSafe, ConnectOptions};

use super::timeouts::connect_within;
use crate::error::AppResult;

#[derive(Clone, Copy, Debug)]
pub enum KillTarget {
    /// Interrupts the running statement; the connection stays usable.
    Query,
    Connection,
}

#[derive(Clone)]
pub struct Killer {
    /// Without a database, so `KILL` still connects after the current one was dropped.
    options: MySqlConnectOptions,
    connect_timeout: Duration,
}

impl Killer {
    pub fn new(options: MySqlConnectOptions, connect_timeout: Duration) -> Self {
        Self {
            options,
            connect_timeout,
        }
    }

    pub async fn kill(&self, target: KillTarget, thread_id: u64) -> AppResult<()> {
        let command = match target {
            KillTarget::Query => "KILL QUERY",
            KillTarget::Connection => "KILL",
        };
        let sql = format!("{command} {thread_id}");
        connect_within(self.connect_timeout, async {
            let mut conn = self.options.connect().await?;
            sqlx::raw_sql(AssertSqlSafe(sql)).execute(&mut conn).await?;
            sqlx::Connection::close(conn).await?;
            Ok(())
        })
        .await
    }

    /// Stops a timed-out query in the background; the caller has already given up on it.
    pub fn spawn_kill_query(&self, thread_id: u64) {
        let killer = self.clone();
        tokio::spawn(async move {
            if let Err(e) = killer.kill(KillTarget::Query, thread_id).await {
                log::warn!("Could not kill the timed-out query on thread {thread_id}: {e}");
            }
        });
    }
}
