//! Every server call is bounded, so a half-dead connection can't hang a command.

use std::time::Duration;

use sqlx::mysql::{MySqlConnection, MySqlPool};

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
        .unwrap_or_else(|_| Err(AppError::Timeout(limit)))
}

/// Runs `work` on a pool connection. A connection abandoned mid-result or mid-transaction is
/// closed instead of going back to the pool.
pub async fn pooled_within<T>(
    limit: Duration,
    pool: &MySqlPool,
    work: impl AsyncFnOnce(&mut MySqlConnection) -> AppResult<T>,
) -> AppResult<T> {
    let mut conn = pool.acquire().await?;
    match tokio::time::timeout(limit, work(&mut conn)).await {
        Ok(result) => result,
        Err(_) => {
            conn.close_on_drop();
            Err(AppError::Timeout(limit))
        }
    }
}
