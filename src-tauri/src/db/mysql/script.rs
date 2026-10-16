//! Running SQL text on a connection and assembling the results.

use futures_util::TryStreamExt;
use sqlx::mysql::{MySqlColumn, MySqlConnection, MySqlRow};
use sqlx::{AssertSqlSafe, Column, Either, Executor, Row, SqlSafeStr, Statement, TypeInfo};

use crate::db::cell::{self, Family};
use crate::db::classify::returns_rows;
use crate::db::lexer::split_statements;
use crate::db::{Cell, ColumnMeta, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

pub async fn thread_id(conn: &mut MySqlConnection) -> AppResult<u64> {
    scalar(conn, "SELECT CONNECTION_ID()")
        .await?
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| AppError::Connection("server did not report a connection id".into()))
}

pub async fn read_dialect(conn: &mut MySqlConnection) -> AppResult<Dialect> {
    let sql_mode = scalar(conn, "SELECT @@SESSION.sql_mode").await?;
    Ok(Dialect::from_sql_mode(&sql_mode.unwrap_or_default()))
}

pub async fn scalar(conn: &mut MySqlConnection, sql: &'static str) -> AppResult<Option<String>> {
    let row = sqlx::raw_sql(sql).fetch_optional(&mut *conn).await?;
    Ok(row.and_then(|r| raw_text(&r, 0)))
}

/// Lossy: binary literals (e.g. a BLOB default in DDL) show as U+FFFD.
pub fn raw_text(row: &MySqlRow, index: usize) -> Option<String> {
    let raw: Option<&[u8]> = row.try_get_unchecked(index).ok()?;
    raw.map(|b| String::from_utf8_lossy(b).into_owned())
}

fn column_meta(column: &MySqlColumn) -> ColumnMeta {
    ColumnMeta {
        name: column.name().to_string(),
        type_name: column.type_info().name().to_string(),
    }
}

fn columns_of(row: &MySqlRow) -> Vec<ColumnMeta> {
    row.columns().iter().map(column_meta).collect()
}

fn decode_row(row: &MySqlRow, families: &[Family]) -> AppResult<Vec<Cell>> {
    families
        .iter()
        .enumerate()
        .map(|(i, family)| {
            let raw: Option<&[u8]> = row.try_get_unchecked(i)?;
            Ok(cell::decode(*family, raw))
        })
        .collect()
}

/// Builds one `ResultSet` per statement from the script's stream of rows and completions.
struct ScriptResults {
    row_limit: usize,
    finished: Vec<ResultSet>,
    current: ResultSet,
    /// Decoding family per column of `current`, computed once per result set.
    families: Vec<Family>,
}

impl ScriptResults {
    fn new(row_limit: usize) -> Self {
        Self {
            row_limit,
            finished: Vec::new(),
            current: ResultSet::default(),
            families: Vec::new(),
        }
    }

    /// Adds a row; `columns` runs for the first row of a result set, `decode` only for rows
    /// within the limit (later rows are counted as truncated).
    fn row(
        &mut self,
        columns: impl FnOnce() -> Vec<ColumnMeta>,
        decode: impl FnOnce(&[Family]) -> AppResult<Vec<Cell>>,
    ) -> AppResult<()> {
        let current = &mut self.current;
        if current.columns.is_empty() {
            current.columns = columns();
            self.families = current
                .columns
                .iter()
                .map(|c| cell::family(&c.type_name))
                .collect();
        }
        current.returns_rows = true;
        current.total_rows += 1;
        if current.rows.len() < self.row_limit {
            current.rows.push(decode(&self.families)?);
        } else {
            current.truncated = true;
        }
        Ok(())
    }

    /// Closes the current result set when its statement completes.
    fn done(&mut self, affected_rows: u64, last_insert_id: u64) {
        self.current.affected_rows = affected_rows;
        self.current.last_insert_id = (last_insert_id > 0).then_some(last_insert_id);
        self.finished.push(std::mem::take(&mut self.current));
    }

    /// The result sets, including rows that arrived without a completion.
    fn finish(mut self) -> Vec<ResultSet> {
        if self.current.returns_rows {
            self.finished.push(self.current);
        }
        self.finished
    }
}

/// An empty result carries no column info over the text protocol, so a row statement that
/// changed nothing has its columns recovered via `PREPARE`.
fn needs_column_recovery(statement: &str, affected_rows: u64, has_columns: bool) -> bool {
    !has_columns && affected_rows == 0 && returns_rows(statement)
}

/// Runs a (possibly multi-statement) script over the text protocol. Each statement yields
/// one `ResultSet`; rows beyond `row_limit` are counted but not kept.
pub async fn run_script(
    conn: &mut MySqlConnection,
    sql: &str,
    row_limit: usize,
) -> AppResult<Vec<ResultSet>> {
    let mut assembled = ScriptResults::new(row_limit);
    {
        let mut stream = sqlx::raw_sql(AssertSqlSafe(sql.to_owned())).fetch_many(&mut *conn);
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Right(row) => {
                    assembled.row(|| columns_of(&row), |families| decode_row(&row, families))?
                }
                Either::Left(done) => assembled.done(done.rows_affected(), done.last_insert_id()),
            }
        }
    }
    let mut results = assembled.finish();
    recover_empty_columns(conn, sql, &mut results).await;
    Ok(results)
}

async fn recover_empty_columns(conn: &mut MySqlConnection, sql: &str, results: &mut [ResultSet]) {
    let statements = split_statements(sql);
    if statements.len() != results.len() {
        return;
    }
    for (statement, result) in statements.iter().zip(results.iter_mut()) {
        if !needs_column_recovery(statement, result.affected_rows, !result.columns.is_empty()) {
            continue;
        }
        result.returns_rows = true;
        let prepared = AssertSqlSafe(statement.to_string()).into_sql_str();
        match conn.prepare(prepared).await {
            Ok(stmt) => result.columns = stmt.columns().iter().map(column_meta).collect(),
            Err(e) => log::debug!("Could not recover the columns of an empty result: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn column() -> Vec<ColumnMeta> {
        vec![ColumnMeta {
            name: "a".into(),
            type_name: "INT".into(),
        }]
    }

    fn push_row(results: &mut ScriptResults, value: &str) {
        results
            .row(column, |_| Ok(vec![Cell::Number(value.into())]))
            .unwrap();
    }

    #[test]
    fn rows_beyond_the_limit_are_counted_but_not_kept() {
        let mut results = ScriptResults::new(2);
        for value in ["1", "2", "3"] {
            push_row(&mut results, value);
        }
        results.done(0, 0);
        let sets = results.finish();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].rows.len(), 2);
        assert_eq!(sets[0].total_rows, 3);
        assert!(sets[0].truncated);
        assert!(sets[0].returns_rows);
        assert_eq!(sets[0].columns, column());
    }

    #[test]
    fn rows_within_the_limit_are_not_truncated() {
        let mut results = ScriptResults::new(3);
        for value in ["1", "2", "3"] {
            push_row(&mut results, value);
        }
        results.done(0, 0);
        let sets = results.finish();
        assert_eq!(sets[0].total_rows, 3);
        assert!(!sets[0].truncated);
    }

    #[test]
    fn last_insert_id_is_none_when_zero() {
        for (id, expected) in [(0, None), (7, Some(7))] {
            let mut results = ScriptResults::new(10);
            results.done(1, id);
            let sets = results.finish();
            assert_eq!(sets.len(), 1, "{id}");
            assert_eq!(sets[0].affected_rows, 1, "{id}");
            assert_eq!(sets[0].last_insert_id, expected, "{id}");
            assert!(!sets[0].returns_rows, "{id}");
        }
    }

    #[test]
    fn each_completion_closes_one_result_set() {
        let mut results = ScriptResults::new(10);
        push_row(&mut results, "1");
        results.done(0, 0);
        push_row(&mut results, "2");
        push_row(&mut results, "3");
        results.done(0, 0);
        let sets = results.finish();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].total_rows, 1);
        assert_eq!(sets[1].total_rows, 2);
    }

    #[test]
    fn trailing_rows_without_completion_are_kept() {
        let mut results = ScriptResults::new(10);
        results.done(2, 0);
        push_row(&mut results, "1");
        let sets = results.finish();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].affected_rows, 2);
        assert_eq!(sets[1].rows, [vec![Cell::Number("1".into())]]);
    }

    #[test]
    fn decode_failure_surfaces_as_an_error() {
        let mut results = ScriptResults::new(10);
        let err = results
            .row(column, |_| Err(AppError::InvalidInput("bad cell".into())))
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn column_recovery_applies_only_to_empty_row_statements_without_columns() {
        for (statement, affected_rows, has_columns, expected) in [
            ("SELECT * FROM t WHERE 0", 0, false, true),
            ("SELECT * FROM t WHERE 0", 0, true, false),
            ("WITH c AS (SELECT 1) DELETE FROM t", 3, false, false),
            ("UPDATE t SET a = 1", 0, false, false),
        ] {
            assert_eq!(
                needs_column_recovery(statement, affected_rows, has_columns),
                expected,
                "{statement} / {affected_rows} / {has_columns}"
            );
        }
    }
}
