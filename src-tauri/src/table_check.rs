//! The live check of a table tab: the rows a page shows, read again by primary key as one
//! checksum per row, and the rows whose checksum moved read in full to say which values changed.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::db::{Cell, CellLimit, Dialect, ResultSet};
use crate::error::{AppError, AppResult};
use crate::session::Session;
use crate::state::AppState;
use crate::value::RowKey;

/// A check runs every few seconds; one that takes longer is not worth waiting for.
const CHECK_TIMEOUT: Duration = Duration::from_secs(3);
/// The largest page a table tab shows (`db::browse`).
const MAX_ROWS: usize = 500;

/// The rows of a page by primary key, and the page's columns the checksum covers.
#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableCheckRequest {
    pub database: String,
    pub table: String,
    pub columns: Vec<String>,
    pub keys: Vec<RowKey>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableCheck {
    /// One per key, in request order: the stored row's checksum, `None` when no row has the key.
    pub hashes: Vec<Option<String>>,
}

/// Rows whose checksum moved, with the cells the page shows for them.
#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableCheckRowsRequest {
    pub database: String,
    pub table: String,
    pub columns: Vec<String>,
    /// The page's result set, whose long text cells the session keeps in full.
    #[ts(type = "number")]
    pub result_id: u64,
    pub rows: Vec<ShownRow>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ShownRow {
    /// The row's index in the page.
    pub row: usize,
    pub key: RowKey,
    /// In `columns` order.
    pub cells: Vec<Cell>,
}

/// A shown row as it is stored now; a row that is gone is left out.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct CheckedRow {
    pub row: usize,
    /// The checksum these cells have.
    pub hash: String,
    /// In `columns` order, cut like a page's cells.
    pub cells: Vec<Cell>,
    /// The columns whose stored value differs from the shown one. Empty when only a part the
    /// page does not show changed (binary values beyond their preview).
    pub changed: Vec<usize>,
}

/// `MD5` of every column's `MD5`, `-` for NULL: 32 hex characters or `-` per column, so no two
/// rows that differ in any value (NULL and `''` included) share the text that is hashed.
fn row_hash_sql(dialect: Dialect, columns: &[String]) -> String {
    let parts: Vec<String> = columns
        .iter()
        .map(|c| format!("IFNULL(MD5({}),'-')", dialect.quote_ident(c)))
        .collect();
    format!("MD5(CONCAT({}))", parts.join(","))
}

/// `k IN ('1','2')`, or `(k1,k2) IN (('1','a'),('2','b'))` for a composite key; values are
/// quoted strings like a structured filter's, which the server compares through the key index.
fn keys_in_sql(dialect: Dialect, key_columns: &[String], keys: &[RowKey]) -> String {
    let tuple = |values: Vec<String>| match values.as_slice() {
        [one] => one.clone(),
        _ => format!("({})", values.join(",")),
    };
    let columns = tuple(key_columns.iter().map(|c| dialect.quote_ident(c)).collect());
    let values: Vec<String> = keys
        .iter()
        .map(|key| tuple(key.0.iter().map(|p| dialect.sql_string(&p.value)).collect()))
        .collect();
    format!("{columns} IN ({})", values.join(","))
}

/// The key columns of `keys`, which must all name the same columns in the same order.
fn key_columns(keys: &[RowKey]) -> AppResult<Vec<String>> {
    let Some(first) = keys.first() else {
        return Ok(Vec::new());
    };
    let columns: Vec<String> = first.0.iter().map(|p| p.column.clone()).collect();
    let same = |key: &RowKey| {
        key.0.len() == columns.len() && key.0.iter().zip(&columns).all(|(p, c)| &p.column == c)
    };
    if columns.is_empty() || !keys.iter().all(same) {
        return Err(AppError::InvalidInput(
            "Every row of a live check needs the same primary key".into(),
        ));
    }
    Ok(columns)
}

fn check_size(columns: &[String], rows: usize) -> AppResult<()> {
    if columns.is_empty() {
        return Err(AppError::InvalidInput("A live check needs columns".into()));
    }
    if rows > MAX_ROWS {
        return Err(AppError::InvalidInput(format!(
            "A live check reads at most {MAX_ROWS} rows"
        )));
    }
    Ok(())
}

/// The key columns, then `select`, of every row of `table` with one of `keys`.
fn select_by_keys(
    dialect: Dialect,
    database: &str,
    table: &str,
    key_columns: &[String],
    select: &str,
    keys: &[RowKey],
) -> String {
    let keys_select: Vec<String> = key_columns.iter().map(|c| dialect.quote_ident(c)).collect();
    format!(
        "SELECT {}, {select} FROM {} WHERE {}",
        keys_select.join(", "),
        dialect.table_ref(database, table),
        keys_in_sql(dialect, key_columns, keys)
    )
}

/// The rows of `result` by the text of their first `width` cells, the key; a row whose key is
/// not text cannot match a shown key and is left out.
fn rows_by_key(result: ResultSet, width: usize) -> HashMap<Vec<String>, Vec<Cell>> {
    result
        .rows
        .into_iter()
        .filter_map(|mut cells| {
            let rest = cells.split_off(width.min(cells.len()));
            let key = cells
                .iter()
                .map(|c| c.as_text().map(str::to_string))
                .collect::<Option<Vec<_>>>()?;
            Some((key, rest))
        })
        .collect()
}

fn key_text(key: &RowKey) -> Vec<String> {
    key.0.iter().map(|p| p.value.clone()).collect()
}

/// One statement: the checksum of every row with one of the keys.
pub(crate) async fn check(
    session: &Session,
    request: &TableCheckRequest,
    timeout: Duration,
) -> AppResult<TableCheck> {
    check_size(&request.columns, request.keys.len())?;
    let key_columns = key_columns(&request.keys)?;
    if request.keys.is_empty() {
        return Ok(TableCheck { hashes: Vec::new() });
    }
    let dialect = session.db.dialect();
    let sql = select_by_keys(
        dialect,
        &request.database,
        &request.table,
        &key_columns,
        &row_hash_sql(dialect, &request.columns),
        &request.keys,
    );
    let stored = rows_by_key(
        session.db.fetch(&sql, timeout.min(CHECK_TIMEOUT)).await?,
        key_columns.len(),
    );
    let hashes = request
        .keys
        .iter()
        .map(|key| {
            stored
                .get(&key_text(key))
                .and_then(|rest| rest.first())
                .and_then(Cell::as_text)
                .map(str::to_string)
        })
        .collect();
    Ok(TableCheck { hashes })
}

/// Whether the stored `new` cell holds what the page shows as `old`. A long text compares in
/// full when the session still keeps it (`old_full`), else by length and preview.
fn same_value(old: &Cell, old_full: Option<&str>, new: &Cell) -> bool {
    match (old, new) {
        (Cell::LongText { len, preview }, Cell::Text(text)) => match old_full {
            Some(full) => full == text,
            None => text.chars().count() as u64 == *len && text.starts_with(preview.as_str()),
        },
        _ => old == new,
    }
}

/// One statement: the rows in full with their checksum, compared with the shown cells.
pub(crate) async fn changed_rows(
    session: &Session,
    request: &TableCheckRowsRequest,
    cell_limit: CellLimit,
    timeout: Duration,
) -> AppResult<Vec<CheckedRow>> {
    check_size(&request.columns, request.rows.len())?;
    let keys: Vec<RowKey> = request.rows.iter().map(|r| r.key.clone()).collect();
    let key_columns = key_columns(&keys)?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let dialect = session.db.dialect();
    let mut select: Vec<String> = request
        .columns
        .iter()
        .map(|c| dialect.quote_ident(c))
        .collect();
    select.push(row_hash_sql(dialect, &request.columns));
    let sql = select_by_keys(
        dialect,
        &request.database,
        &request.table,
        &key_columns,
        &select.join(", "),
        &keys,
    );
    let mut stored = rows_by_key(
        session.db.fetch(&sql, timeout.min(CHECK_TIMEOUT)).await?,
        key_columns.len(),
    );
    let width = request.columns.len();
    Ok(request
        .rows
        .iter()
        .filter_map(|shown| {
            let mut cells = stored.remove(&key_text(&shown.key))?;
            let hash = cells.get(width)?.as_text()?.to_string();
            cells.truncate(width);
            let changed = (0..width)
                .filter(|&col| {
                    let Some(old) = shown.cells.get(col) else {
                        return false;
                    };
                    let full = matches!(old, Cell::LongText { .. })
                        .then(|| session.cells.get(request.result_id, shown.row, col).ok())
                        .flatten();
                    !same_value(old, full.as_deref(), &cells[col])
                })
                .collect();
            Some(CheckedRow {
                row: shown.row,
                hash,
                cells: cells.into_iter().map(|c| c.limited(cell_limit)).collect(),
                changed,
            })
        })
        .collect())
}

/// The checksums of a page's rows (one statement); see `check`.
#[tauri::command]
pub async fn table_check(
    state: State<'_, AppState>,
    session_id: String,
    request: TableCheckRequest,
) -> AppResult<TableCheck> {
    let session = state.sessions.get(&session_id)?;
    check(&session, &request, state.settings.get().query_timeout()).await
}

/// Rows whose checksum moved, as stored now (one statement); see `changed_rows`.
#[tauri::command]
pub async fn table_check_rows(
    state: State<'_, AppState>,
    session_id: String,
    request: TableCheckRowsRequest,
) -> AppResult<Vec<CheckedRow>> {
    let settings = state.settings.get();
    let session = state.sessions.get(&session_id)?;
    changed_rows(
        &session,
        &request,
        settings.cell_limit(),
        settings.query_timeout(),
    )
    .await
}
