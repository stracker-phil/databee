//! Every server call is bounded, so a half-dead connection can't hang a command.

use std::time::Duration;

use sqlx::mysql::{MySqlConnection, MySqlPool};

use super::kill::Killer;
use super::script::thread_id;
use crate::error::{AppError, AppResult, human_duration};

/// Bounds connecting: a hung handshake reads as a connection failure.
pub async fn connect_within<T>(
    limit: Duration,
    fut: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::time::timeout(limit, fut).await.unwrap_or_else(|_| {
        Err(AppError::Connection(format!(
            "timed out after {}",
            human_duration(limit)
        )))
    })
}

pub async fn query_within<T>(
    limit: Duration,
    fut: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::time::timeout(limit, fut)
        .await
        .unwrap_or_else(|_| Err(AppError::timeout(limit)))
}

/// Runs `work` on a pool connection. On timeout the server-side query is killed (closing the
/// socket alone leaves it running), and the connection, abandoned mid-result or
/// mid-transaction, is closed instead of going back to the pool.
pub async fn pooled_within<T>(
    limit: Duration,
    pool: &MySqlPool,
    killer: &Killer,
    work: impl AsyncFnOnce(&mut MySqlConnection) -> AppResult<T>,
) -> AppResult<T> {
    let mut conn = pool.acquire().await?;
    let mut thread = None;
    let run = async {
        thread = Some(thread_id(&mut conn).await?);
        work(&mut conn).await
    };
    match tokio::time::timeout(limit, run).await {
        Ok(result) => result,
        Err(_) => {
            if let Some(thread) = thread {
                killer.spawn_kill_query(thread);
            }
            conn.close_on_drop();
            Err(AppError::timeout(limit))
        }
    }
}
