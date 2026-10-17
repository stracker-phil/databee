//! Schema metadata from `information_schema`, read on the metadata pool.

use std::collections::BTreeMap;
use std::time::Duration;

use sqlx::AssertSqlSafe;
use sqlx::mysql::{MySqlPool, MySqlRow};

use super::keys::{primary_key, table_keys};
use super::meta_rows::{rows, text, text_or_empty};
use super::script::raw_text;
use super::timeouts::query_within;
use crate::db::{ColumnInfo, DatabaseInfo, Dialect, TableInfo, TableStructure};
use crate::error::AppResult;

fn number(row: &MySqlRow, column: &str) -> AppResult<Option<u64>> {
    Ok(text(row, column)?.and_then(|v| v.parse().ok()))
}

pub async fn databases(
    pool: &MySqlPool,
    timeout: Duration,
    dialect: Dialect,
) -> AppResult<Vec<DatabaseInfo>> {
    let rows = rows(
        pool,
        timeout,
        "SELECT SCHEMA_NAME AS name FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
        &[],
    )
    .await?;
    let mut databases = Vec::new();
    for row in &rows {
        let Some(name) = text(row, "name")? else {
            continue;
        };
        databases.push(DatabaseInfo {
            is_system: dialect
                .system_schemas()
                .contains(&name.to_ascii_lowercase().as_str()),
            name,
        });
    }
    Ok(databases)
}

pub async fn tables(
    pool: &MySqlPool,
    timeout: Duration,
    database: &str,
) -> AppResult<Vec<TableInfo>> {
    let rows = rows(
        pool,
        timeout,
        "SELECT TABLE_NAME AS name, TABLE_TYPE AS table_type, \
         CAST(TABLE_ROWS AS CHAR) AS rows_estimate, \
         CAST(DATA_LENGTH + INDEX_LENGTH AS CHAR) AS size_bytes, TABLE_COMMENT AS comment \
         FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
        &[database],
    )
    .await?;
    rows.iter()
        .map(|row| {
            Ok(TableInfo {
                name: text_or_empty(row, "name")?,
                is_view: text(row, "table_type")?.as_deref() == Some("VIEW"),
                rows_estimate: number(row, "rows_estimate")?,
                size_bytes: number(row, "size_bytes")?,
                comment: text_or_empty(row, "comment")?,
            })
        })
        .collect()
}

pub async fn structure(
    pool: &MySqlPool,
    timeout: Duration,
    dialect: Dialect,
    database: &str,
    table: &str,
) -> AppResult<TableStructure> {
    let binds = [database, table];
    let (column_rows, (indexes, foreign_keys), ddl) = tokio::try_join!(
        rows(
            pool,
            timeout,
            "SELECT COLUMN_NAME AS name, COLUMN_TYPE AS column_type, IS_NULLABLE AS nullable, \
             COLUMN_DEFAULT AS default_value, COLUMN_KEY AS column_key, EXTRA AS extra, \
             COLUMN_COMMENT AS comment FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
            &binds,
        ),
        table_keys(pool, timeout, database, table),
        create_statement(pool, timeout, dialect, database, table),
    )?;
    let columns = column_rows
        .iter()
        .map(|row| {
            let extra = text_or_empty(row, "extra")?;
            Ok(ColumnInfo {
                name: text_or_empty(row, "name")?,
                column_type: text_or_empty(row, "column_type")?,
                nullable: text(row, "nullable")?.as_deref() == Some("YES"),
                default: text(row, "default_value")?,
                key: text_or_empty(row, "column_key")?,
                auto_increment: extra_has(&extra, "AUTO_INCREMENT"),
                generated: is_generated(&extra),
                extra,
                comment: text_or_empty(row, "comment")?,
            })
        })
        .collect::<AppResult<_>>()?;
    Ok(TableStructure {
        columns,
        primary_key: primary_key(&indexes),
        indexes,
        foreign_keys,
        ddl,
    })
}

fn extra_has(extra: &str, word: &str) -> bool {
    extra.to_ascii_uppercase().contains(word)
}

/// `EXTRA` marks computed columns `VIRTUAL GENERATED` / `STORED GENERATED` (MariaDB also
/// `PERSISTENT`); MySQL's `DEFAULT_GENERATED` is an ordinary column with an expression default.
fn is_generated(extra: &str) -> bool {
    ["VIRTUAL", "STORED", "PERSISTENT"]
        .iter()
        .any(|word| extra_has(extra, word))
}

/// The `SHOW CREATE TABLE` text, or `None` when the server returns no row.
async fn create_statement(
    pool: &MySqlPool,
    timeout: Duration,
    dialect: Dialect,
    database: &str,
    table: &str,
) -> AppResult<Option<String>> {
    let sql = format!("SHOW CREATE TABLE {}", dialect.table_ref(database, table));
    query_within(timeout, async {
        let row = sqlx::raw_sql(AssertSqlSafe(sql))
            .fetch_optional(pool)
            .await?;
        Ok(row.and_then(|row| raw_text(&row, 1)))
    })
    .await
}

/// Table → column names, for editor autocompletion.
pub async fn completion(
    pool: &MySqlPool,
    timeout: Duration,
    database: &str,
) -> AppResult<BTreeMap<String, Vec<String>>> {
    let rows = rows(
        pool,
        timeout,
        "SELECT TABLE_NAME AS table_name, COLUMN_NAME AS column_name FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
        &[database],
    )
    .await?;
    let mut map = BTreeMap::<String, Vec<String>>::new();
    for row in &rows {
        if let (Some(table), Some(column)) = (text(row, "table_name")?, text(row, "column_name")?) {
            map.entry(table).or_default().push(column);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computed_columns_are_generated_and_expression_defaults_are_not() {
        for (extra, generated) in [
            ("VIRTUAL GENERATED", true),
            ("STORED GENERATED", true),
            ("PERSISTENT GENERATED", true),
            ("virtual generated", true),
            ("DEFAULT_GENERATED", false),
            ("DEFAULT_GENERATED on update CURRENT_TIMESTAMP", false),
            ("auto_increment", false),
            ("", false),
        ] {
            assert_eq!(is_generated(extra), generated, "{extra}");
        }
    }
}
