//! Running `information_schema` queries and reading their rows by column alias.

use std::time::Duration;

use sqlx::Row;
use sqlx::mysql::{MySqlPool, MySqlRow};

use super::timeouts::query_within;
use crate::error::AppResult;

/// Runs a metadata query with bound parameters; every column must be text (CAST as needed)
/// and is read by its alias.
pub async fn rows(
    pool: &MySqlPool,
    timeout: Duration,
    sql: &'static str,
    binds: &[&str],
) -> AppResult<Vec<MySqlRow>> {
    let mut query = sqlx::query(sql);
    for b in binds {
        query = query.bind(b.to_string());
    }
    query_within(timeout, async { Ok(query.fetch_all(pool).await?) }).await
}

pub fn text(row: &MySqlRow, column: &str) -> AppResult<Option<String>> {
    Ok(row.try_get_unchecked::<Option<String>, _>(column)?)
}

pub fn text_or_empty(row: &MySqlRow, column: &str) -> AppResult<String> {
    Ok(text(row, column)?.unwrap_or_default())
}
