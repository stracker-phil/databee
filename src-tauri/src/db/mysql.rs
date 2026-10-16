use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::TryStreamExt;
use sqlx::mysql::{
    MySqlColumn, MySqlConnectOptions, MySqlConnection, MySqlPool, MySqlPoolOptions, MySqlRow,
};
use sqlx::{
    AssertSqlSafe, Column, ConnectOptions, Either, Executor, Row, SqlSafeStr, Statement, TypeInfo,
};

use super::browse::{browse_sql, page_limit};
use super::classify::returns_rows;
use super::lexer::split_statements;
use super::{
    BrowsePage, BrowseRequest, Cell, ColumnInfo, ColumnMeta, DatabaseInfo, Dialect,
    ExecuteResponse, IndexInfo, ResultSet, TableInfo, TableStructure, cell, quote_ident,
};
use crate::connection::{ConnectionSpec, Secret};
use crate::error::{AppError, AppResult, human_duration};
use crate::lock::MutexExt;

const META_POOL_SIZE: u32 = 3;

/// One logical connection: a dedicated "editor" connection that keeps session state
/// (`USE`, variables, transactions) for user queries, plus a small pool for metadata
/// and table browsing so the sidebar stays responsive while a query runs.
pub struct MySqlSession {
    /// Without a database, so `KILL` still connects after the current one was dropped.
    editor_options: MySqlConnectOptions,
    connect_timeout: Duration,
    /// Bounds metadata calls; `execute`, `browse`, `fetch` and `apply` take their own.
    query_timeout: Duration,
    meta: MySqlPool,
    editor: tokio::sync::Mutex<MySqlConnection>,
    editor_thread_id: AtomicU64,
    database: Mutex<Option<String>>,
    pub server_version: String,
}

fn connect_options(spec: &ConnectionSpec, password: Option<&Secret>) -> MySqlConnectOptions {
    let mut options = MySqlConnectOptions::new()
        .username(&spec.user)
        // Keep server defaults: a SQL client must not alter sql_mode or time_zone.
        .pipes_as_concat(false)
        .no_engine_substitution(false)
        .timezone(None);
    options = match &spec.socket {
        Some(socket) => options.socket(socket),
        None => options.host(&spec.host).port(spec.port),
    };
    if let Some(password) = password {
        options = options.password(password.expose());
    }
    options
}

fn with_database(options: &MySqlConnectOptions, database: Option<&str>) -> MySqlConnectOptions {
    match database {
        Some(db) => options.clone().database(db),
        None => options.clone(),
    }
}

/// Bounds connecting: a hung handshake reads as a connection failure.
async fn connect_within<T>(
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

async fn query_within<T>(limit: Duration, fut: impl Future<Output = AppResult<T>>) -> AppResult<T> {
    tokio::time::timeout(limit, fut)
        .await
        .unwrap_or(Err(AppError::Timeout(limit)))
}

impl MySqlSession {
    pub async fn connect(
        spec: &ConnectionSpec,
        password: Option<&Secret>,
        connect_timeout: Duration,
        query_timeout: Duration,
    ) -> AppResult<Self> {
        let options = connect_options(spec, password);
        // Column recovery prepares each empty SELECT once; without a cache the statement is
        // closed right away instead of piling up on the long-lived editor connection.
        let editor_options = options.clone().statement_cache_capacity(0);
        let pool = async {
            Ok::<_, AppError>(
                MySqlPoolOptions::new()
                    .max_connections(META_POOL_SIZE)
                    .acquire_timeout(connect_timeout)
                    .connect_with(with_database(&options, spec.database.as_deref()))
                    .await?,
            )
        };
        let editor = async {
            let mut conn = with_database(&editor_options, spec.database.as_deref())
                .connect()
                .await?;
            let thread_id = thread_id(&mut conn).await?;
            let version = scalar(&mut conn, "SELECT VERSION()").await?;
            Ok::<_, AppError>((conn, thread_id, version.unwrap_or_default()))
        };
        let (meta, (editor, thread_id, server_version)) =
            connect_within(connect_timeout, async { tokio::try_join!(pool, editor) }).await?;
        Ok(Self {
            editor_options,
            connect_timeout,
            query_timeout,
            meta,
            editor: tokio::sync::Mutex::new(editor),
            editor_thread_id: AtomicU64::new(thread_id),
            database: Mutex::new(spec.database.clone()),
            server_version,
        })
    }

    pub fn current_database(&self) -> Option<String> {
        self.database.lock_unpoisoned().clone()
    }

    pub async fn close(&self) {
        self.meta.close().await;
    }

    pub async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        let started = Instant::now();
        let mut conn = self.editor.lock().await;
        let run = async {
            let results = run_script(&mut conn, sql, row_limit).await?;
            let database = scalar(&mut conn, "SELECT DATABASE()").await;
            Ok((results, database))
        };
        let (results, database) = match tokio::time::timeout(timeout, run).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) => {
                if matches!(e, AppError::Connection(_)) {
                    log::warn!("Editor connection lost, reconnecting");
                    self.reconnect_or_log(&mut conn).await;
                }
                return Err(e);
            }
            Err(_) => {
                // The abandoned connection is mid-result; kill it server-side and start fresh.
                if let Err(e) = self.kill(false).await {
                    log::warn!("Could not kill the timed-out editor thread: {e}");
                }
                self.reconnect_or_log(&mut conn).await;
                return Err(AppError::Timeout(timeout));
            }
        };
        // The script already ran (and may have committed), so a failed probe must not lose it.
        let database = match database {
            Ok(database) => {
                *self.database.lock_unpoisoned() = database.clone();
                database
            }
            Err(e) => {
                log::warn!("Could not read the current database after a query: {e}");
                self.current_database()
            }
        };
        Ok(ExecuteResponse {
            results,
            elapsed_ms: started.elapsed().as_millis() as u64,
            database,
        })
    }

    /// Interrupts the statement running on the editor connection.
    pub async fn cancel(&self) -> AppResult<()> {
        self.kill(true).await
    }

    /// Runs `KILL` on a one-off connection, so it works even while every pool connection is busy.
    async fn kill(&self, query_only: bool) -> AppResult<()> {
        let id = self.editor_thread_id.load(Ordering::SeqCst);
        let sql = if query_only {
            format!("KILL QUERY {id}")
        } else {
            format!("KILL {id}")
        };
        connect_within(self.connect_timeout, async {
            let mut conn = self.editor_options.connect().await?;
            sqlx::raw_sql(AssertSqlSafe(sql)).execute(&mut conn).await?;
            sqlx::Connection::close(conn).await?;
            Ok(())
        })
        .await
    }

    async fn reconnect_editor(&self, conn: &mut MySqlConnection) -> AppResult<()> {
        let options = with_database(&self.editor_options, self.current_database().as_deref());
        let reconnect = async {
            let mut fresh = options.connect().await?;
            let id = thread_id(&mut fresh).await?;
            Ok((fresh, id))
        };
        let (fresh, id) = connect_within(self.connect_timeout, reconnect).await?;
        self.editor_thread_id.store(id, Ordering::SeqCst);
        *conn = fresh;
        Ok(())
    }

    /// Reconnects after a failed run. The run's own error is what the user sees; a failed
    /// reconnect only leaves the broken connection for the next run to replace.
    async fn reconnect_or_log(&self, conn: &mut MySqlConnection) {
        if let Err(e) = self.reconnect_editor(conn).await {
            log::warn!("Could not reconnect the editor connection: {e}");
        }
    }

    pub async fn use_database(&self, database: &str) -> AppResult<()> {
        let mut conn = self.editor.lock().await;
        let sql = format!("USE {}", quote_ident(database));
        query_within(self.query_timeout, async {
            sqlx::raw_sql(AssertSqlSafe(sql))
                .execute(&mut *conn)
                .await?;
            Ok(())
        })
        .await?;
        *self.database.lock_unpoisoned() = Some(database.to_string());
        Ok(())
    }

    pub async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        let rows = self
            .strings(
                "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|mut r| r.remove(0))
            .map(|name| DatabaseInfo {
                is_system: Dialect::MYSQL
                    .system_schemas()
                    .contains(&name.to_ascii_lowercase().as_str()),
                name,
            })
            .collect())
    }

    pub async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        let rows = self
            .strings(
                "SELECT TABLE_NAME, TABLE_TYPE, CAST(TABLE_ROWS AS CHAR), \
                 CAST(DATA_LENGTH + INDEX_LENGTH AS CHAR), TABLE_COMMENT \
                 FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
                &[database],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| TableInfo {
                name: r[0].clone().unwrap_or_default(),
                is_view: r[1].as_deref() == Some("VIEW"),
                rows_estimate: r[2].as_deref().and_then(|v| v.parse().ok()),
                size_bytes: r[3].as_deref().and_then(|v| v.parse().ok()),
                comment: r[4].clone().unwrap_or_default(),
            })
            .collect())
    }

    pub async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        let binds = [database, table];
        let (column_rows, index_rows, ddl) = tokio::try_join!(
            self.strings(
                "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_KEY, EXTRA, \
                 COLUMN_COMMENT FROM information_schema.COLUMNS \
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
                &binds,
            ),
            self.strings(
                "SELECT INDEX_NAME, CAST(NON_UNIQUE AS CHAR), COLUMN_NAME \
                 FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
                 ORDER BY INDEX_NAME = 'PRIMARY' DESC, INDEX_NAME, SEQ_IN_INDEX",
                &binds,
            ),
            self.create_statement(database, table),
        )?;
        let columns = column_rows
            .into_iter()
            .map(|r| ColumnInfo {
                name: r[0].clone().unwrap_or_default(),
                column_type: r[1].clone().unwrap_or_default(),
                nullable: r[2].as_deref() == Some("YES"),
                default: r[3].clone(),
                key: r[4].clone().unwrap_or_default(),
                extra: r[5].clone().unwrap_or_default(),
                comment: r[6].clone().unwrap_or_default(),
            })
            .collect();

        Ok(TableStructure {
            columns,
            indexes: group_indexes(index_rows),
            ddl,
        })
    }

    /// The `SHOW CREATE TABLE` text, or `None` when the server returns no row.
    async fn create_statement(&self, database: &str, table: &str) -> AppResult<Option<String>> {
        let sql = format!(
            "SHOW CREATE TABLE {}.{}",
            quote_ident(database),
            quote_ident(table)
        );
        query_within(self.query_timeout, async {
            let row = sqlx::raw_sql(AssertSqlSafe(sql))
                .fetch_optional(&self.meta)
                .await?;
            Ok(row.and_then(|row| raw_text(&row, 1)))
        })
        .await
    }

    /// Table → column names, for editor autocompletion.
    pub async fn completion(
        &self,
        database: &str,
    ) -> AppResult<std::collections::BTreeMap<String, Vec<String>>> {
        let mut map = std::collections::BTreeMap::<String, Vec<String>>::new();
        for r in self
            .strings(
                "SELECT TABLE_NAME, COLUMN_NAME FROM information_schema.COLUMNS \
                 WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME, ORDINAL_POSITION",
                &[database],
            )
            .await?
        {
            if let (Some(t), Some(c)) = (r[0].clone(), r[1].clone()) {
                map.entry(t).or_default().push(c);
            }
        }
        Ok(map)
    }

    pub async fn browse(&self, req: &BrowseRequest, timeout: Duration) -> AppResult<BrowsePage> {
        let (sql, filter) = browse_sql(req, Dialect::MYSQL)?;
        let limit = page_limit(req) as usize;
        let mut conn = self.meta.acquire().await?;
        let outcome = tokio::time::timeout(timeout, run_script(&mut conn, &sql, limit)).await;
        let mut result = match outcome {
            Ok(results) => results?.pop().unwrap_or_default(),
            Err(_) => {
                conn.close_on_drop();
                return Err(AppError::Timeout(timeout));
            }
        };
        let has_more = result.truncated;
        result.truncated = false;
        result.total_rows = result.rows.len() as u64;
        Ok(BrowsePage {
            result,
            has_more,
            filter,
        })
    }

    /// Runs the statements in one transaction (all or nothing for transactional tables).
    pub async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        use sqlx::Connection;
        let mut conn = self.meta.acquire().await?;
        let commit_sent = AtomicBool::new(false);
        let work = async {
            let mut tx = conn.begin().await?;
            let mut affected = 0;
            for sql in statements {
                affected += sqlx::raw_sql(AssertSqlSafe(sql.clone()))
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
            }
            commit_sent.store(true, Ordering::SeqCst);
            tx.commit().await?;
            Ok::<u64, sqlx::Error>(affected)
        };
        match tokio::time::timeout(timeout, work).await {
            Ok(result) => Ok(result?),
            Err(_) => {
                // Mid-transaction, the connection must not go back to the pool.
                conn.close_on_drop();
                if commit_sent.load(Ordering::SeqCst) {
                    return Err(AppError::Database {
                        message: "Timed out after COMMIT was sent; whether the changes were saved is unknown".into(),
                        code: None,
                    });
                }
                Err(AppError::Timeout(timeout))
            }
        }
    }

    /// Runs a read query on the metadata pool and returns its (last) result set.
    pub async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet> {
        const MAX_ROWS: usize = 50_000;
        let mut conn = self.meta.acquire().await?;
        match tokio::time::timeout(timeout, run_script(&mut conn, sql, MAX_ROWS)).await {
            Ok(results) => Ok(results?.pop().unwrap_or_default()),
            Err(_) => {
                conn.close_on_drop();
                Err(AppError::Timeout(timeout))
            }
        }
    }

    /// Runs a metadata query with bound parameters under the session's query timeout; every
    /// column must be text (CAST as needed).
    async fn strings(
        &self,
        sql: &'static str,
        binds: &[&str],
    ) -> AppResult<Vec<Vec<Option<String>>>> {
        let mut query = sqlx::query(sql);
        for b in binds {
            query = query.bind(b.to_string());
        }
        let rows = query_within(self.query_timeout, async {
            Ok(query.fetch_all(&self.meta).await?)
        })
        .await?;
        rows.iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| row.try_get_unchecked::<Option<String>, _>(i))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(AppError::from)
            })
            .collect()
    }
}

/// One `IndexInfo` per index from `STATISTICS` rows ordered by index and column position.
fn group_indexes(rows: Vec<Vec<Option<String>>>) -> Vec<IndexInfo> {
    let mut indexes: Vec<IndexInfo> = Vec::new();
    for r in rows {
        let name = r[0].clone().unwrap_or_default();
        let column = r[2].clone().unwrap_or_default();
        match indexes.last_mut() {
            Some(last) if last.name == name => last.columns.push(column),
            _ => indexes.push(IndexInfo {
                unique: r[1].as_deref() == Some("0"),
                name,
                columns: vec![column],
            }),
        }
    }
    indexes
}

async fn thread_id(conn: &mut MySqlConnection) -> AppResult<u64> {
    scalar(conn, "SELECT CONNECTION_ID()")
        .await?
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| AppError::Connection("server did not report a connection id".into()))
}

async fn scalar(conn: &mut MySqlConnection, sql: &'static str) -> AppResult<Option<String>> {
    let row = sqlx::raw_sql(sql).fetch_optional(&mut *conn).await?;
    Ok(row.and_then(|r| raw_text(&r, 0)))
}

fn raw_text(row: &MySqlRow, index: usize) -> Option<String> {
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

fn decode_row(row: &MySqlRow, columns: &[ColumnMeta]) -> AppResult<Vec<Cell>> {
    columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let raw: Option<&[u8]> = row.try_get_unchecked(i)?;
            Ok(cell::decode(&col.type_name, raw))
        })
        .collect()
}

/// Builds one `ResultSet` per statement from the script's stream of rows and completions.
struct ScriptResults {
    row_limit: usize,
    finished: Vec<ResultSet>,
    current: ResultSet,
}

impl ScriptResults {
    fn new(row_limit: usize) -> Self {
        Self {
            row_limit,
            finished: Vec::new(),
            current: ResultSet::default(),
        }
    }

    /// Adds a row; `columns` runs for the first row of a result set, `decode` only for rows
    /// within the limit (later rows are counted as truncated).
    fn row(
        &mut self,
        columns: impl FnOnce() -> Vec<ColumnMeta>,
        decode: impl FnOnce(&[ColumnMeta]) -> AppResult<Vec<Cell>>,
    ) -> AppResult<()> {
        let current = &mut self.current;
        if current.columns.is_empty() {
            current.columns = columns();
        }
        current.returns_rows = true;
        current.total_rows += 1;
        if current.rows.len() < self.row_limit {
            current.rows.push(decode(&current.columns)?);
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
async fn run_script(
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
                    assembled.row(|| columns_of(&row), |columns| decode_row(&row, columns))?
                }
                Either::Left(done) => assembled.done(done.rows_affected(), done.last_insert_id()),
            }
        }
    }
    let mut results = assembled.finish();

    let statements = split_statements(sql);
    if statements.len() != results.len() {
        return Ok(results);
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
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(err.kind(), "invalid_input");
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
