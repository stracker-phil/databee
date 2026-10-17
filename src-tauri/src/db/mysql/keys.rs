//! Indexes and foreign keys from `information_schema`, for one table or a whole database.

use std::collections::BTreeMap;
use std::time::Duration;

use sqlx::mysql::{MySqlPool, MySqlRow};

use super::meta_rows::{rows, text, text_or_empty};
use crate::db::{ForeignKey, IndexInfo, TableKeys};
use crate::error::AppResult;

/// `STATISTICS` rows, `PRIMARY` first per table, then by index name and column position.
macro_rules! index_sql {
    ($filter:literal) => {
        concat!(
            "SELECT TABLE_NAME AS table_name, INDEX_NAME AS name, \
             CAST(NON_UNIQUE AS CHAR) AS non_unique, COLUMN_NAME AS column_name \
             FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = ?",
            $filter,
            " ORDER BY TABLE_NAME, INDEX_NAME = 'PRIMARY' DESC, INDEX_NAME, SEQ_IN_INDEX"
        )
    };
}

/// `KEY_COLUMN_USAGE` rows of foreign keys into the same database, in key column order.
/// Cross-database references are left out: the schema cache and its addresses are per database.
macro_rules! foreign_key_sql {
    ($filter:literal) => {
        concat!(
            "SELECT TABLE_NAME AS table_name, CONSTRAINT_NAME AS name, \
             COLUMN_NAME AS column_name, REFERENCED_TABLE_NAME AS ref_table, \
             REFERENCED_COLUMN_NAME AS ref_column FROM information_schema.KEY_COLUMN_USAGE \
             WHERE TABLE_SCHEMA = ? AND REFERENCED_TABLE_SCHEMA = TABLE_SCHEMA \
             AND REFERENCED_TABLE_NAME IS NOT NULL",
            $filter,
            " ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION"
        )
    };
}

/// The indexes and foreign keys of one table.
pub async fn table_keys(
    pool: &MySqlPool,
    timeout: Duration,
    database: &str,
    table: &str,
) -> AppResult<(Vec<IndexInfo>, Vec<ForeignKey>)> {
    let binds = [database, table];
    let (index_rows, foreign_key_rows) = tokio::try_join!(
        rows(pool, timeout, index_sql!(" AND TABLE_NAME = ?"), &binds),
        rows(
            pool,
            timeout,
            foreign_key_sql!(" AND TABLE_NAME = ?"),
            &binds
        ),
    )?;
    let indexes = group_indexes(&index_rows)?
        .into_iter()
        .map(|(_, index)| index)
        .collect();
    let foreign_keys = group_foreign_keys(&foreign_key_rows)?
        .into_iter()
        .map(|(_, foreign_key)| foreign_key)
        .collect();
    Ok((indexes, foreign_keys))
}

/// The keys of every table in `database` with at least one index or foreign key, in two
/// queries, so callers that need every table's keys don't ask table by table.
pub async fn database_keys(
    pool: &MySqlPool,
    timeout: Duration,
    database: &str,
) -> AppResult<BTreeMap<String, TableKeys>> {
    let binds = [database];
    let (index_rows, foreign_key_rows) = tokio::try_join!(
        rows(pool, timeout, index_sql!(""), &binds),
        rows(pool, timeout, foreign_key_sql!(""), &binds),
    )?;
    let mut keys = BTreeMap::<String, TableKeys>::new();
    for (table, index) in group_indexes(&index_rows)? {
        keys.entry(table).or_default().indexes.push(index);
    }
    for entry in keys.values_mut() {
        entry.primary_key = primary_key(&entry.indexes);
    }
    for (table, foreign_key) in group_foreign_keys(&foreign_key_rows)? {
        keys.entry(table)
            .or_default()
            .foreign_keys
            .push(foreign_key);
    }
    Ok(keys)
}

/// The columns of the `PRIMARY` index; empty without one.
pub fn primary_key(indexes: &[IndexInfo]) -> Vec<String> {
    indexes
        .iter()
        .find(|i| i.name == "PRIMARY")
        .map(|i| i.columns.clone())
        .unwrap_or_default()
}

/// One `IndexInfo` per table and index from rows ordered by table, index and column position.
fn group_indexes(rows: &[MySqlRow]) -> AppResult<Vec<(String, IndexInfo)>> {
    let mut indexes: Vec<(String, IndexInfo)> = Vec::new();
    for row in rows {
        let table = text_or_empty(row, "table_name")?;
        let name = text_or_empty(row, "name")?;
        let column = text_or_empty(row, "column_name")?;
        match indexes.last_mut() {
            Some((last_table, last)) if *last_table == table && last.name == name => {
                last.columns.push(column)
            }
            _ => indexes.push((
                table,
                IndexInfo {
                    unique: text(row, "non_unique")?.as_deref() == Some("0"),
                    name,
                    columns: vec![column],
                },
            )),
        }
    }
    Ok(indexes)
}

/// One `ForeignKey` per table and constraint from rows ordered by table, constraint and position.
fn group_foreign_keys(rows: &[MySqlRow]) -> AppResult<Vec<(String, ForeignKey)>> {
    let mut keys: Vec<(String, ForeignKey)> = Vec::new();
    for row in rows {
        let table = text_or_empty(row, "table_name")?;
        let name = text_or_empty(row, "name")?;
        let column = text_or_empty(row, "column_name")?;
        let ref_column = text_or_empty(row, "ref_column")?;
        match keys.last_mut() {
            Some((last_table, last)) if *last_table == table && last.name == name => {
                last.columns.push(column);
                last.ref_columns.push(ref_column);
            }
            _ => keys.push((
                table,
                ForeignKey {
                    name,
                    columns: vec![column],
                    ref_table: text_or_empty(row, "ref_table")?,
                    ref_columns: vec![ref_column],
                },
            )),
        }
    }
    Ok(keys)
}
