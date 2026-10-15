use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::TryStreamExt;
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection, MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::{
    AssertSqlSafe, Column, ConnectOptions, Either, Executor, Row, SqlSafeStr, Statement, TypeInfo,
};

use super::lexer::split_statements;
use super::safety::returns_rows;
use super::{
    BrowsePage, BrowseRequest, Cell, ColumnInfo, ColumnMeta, DatabaseInfo, ExecuteResponse,
    IndexInfo, ResultSet, SYSTEM_SCHEMAS, TableInfo, TableStructure, applied_filter, browse_sql,
    cell, quote_ident,
};
use crate::connection::{ConnectionSpec, Secret};
use crate::error::{AppError, AppResult};

const META_POOL_SIZE: u32 = 3;

/// One logical connection: a dedicated "editor" connection that keeps session state
/// (`USE`, variables, transactions) for user queries, plus a small pool for metadata
/// and table browsing so the sidebar stays responsive while a query runs.
pub struct MySqlSession {
    options: MySqlConnectOptions,
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
    if let Some(db) = &spec.database {
        options = options.database(db);
    }
    options
}

async fn with_timeout<T>(limit: Duration, fut: impl Future<Output = AppResult<T>>) -> AppResult<T> {
    tokio::time::timeout(limit, fut).await.unwrap_or_else(|_| {
        Err(AppError::Connection(format!(
            "timed out after {} s",
            limit.as_secs()
        )))
    })
}

impl MySqlSession {
    pub async fn connect(
        spec: &ConnectionSpec,
        password: Option<&Secret>,
        connect_timeout: Duration,
    ) -> AppResult<Self> {
        let options = connect_options(spec, password);
        let meta = with_timeout(connect_timeout, async {
            Ok(MySqlPoolOptions::new()
                .max_connections(META_POOL_SIZE)
                .acquire_timeout(connect_timeout)
                .connect_with(options.clone())
                .await?)
        })
        .await?;
        let mut editor =
            with_timeout(connect_timeout, async { Ok(options.connect().await?) }).await?;
        let thread_id = thread_id(&mut editor).await?;
        let server_version = scalar(&mut editor, "SELECT VERSION()")
            .await?
            .unwrap_or_default();
        Ok(Self {
            options,
            meta,
            editor: tokio::sync::Mutex::new(editor),
            editor_thread_id: AtomicU64::new(thread_id),
            database: Mutex::new(spec.database.clone()),
            server_version,
        })
    }

    pub fn current_database(&self) -> Option<String> {
        self.database.lock().unwrap().clone()
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
        let outcome = tokio::time::timeout(timeout, run_script(&mut conn, sql, row_limit)).await;
        let results = match outcome {
            Ok(Ok(results)) => results,
            Ok(Err(e)) => {
                if matches!(e, AppError::Connection(_)) {
                    log::warn!("Editor connection lost, reconnecting");
                    let _ = self.reconnect_editor(&mut conn).await;
                }
                return Err(e);
            }
            Err(_) => {
                // The abandoned connection is mid-result; kill it server-side and start fresh.
                self.kill(false).await;
                self.reconnect_editor(&mut conn).await?;
                return Err(AppError::Timeout(timeout.as_secs() as u32));
            }
        };
        let database = scalar(&mut conn, "SELECT DATABASE()").await?;
        *self.database.lock().unwrap() = database.clone();
        Ok(ExecuteResponse {
            results,
            elapsed_ms: started.elapsed().as_millis() as u64,
            database,
        })
    }

    /// Interrupts the statement running on the editor connection.
    pub async fn cancel(&self) {
        self.kill(true).await;
    }

    async fn kill(&self, query_only: bool) {
        let id = self.editor_thread_id.load(Ordering::SeqCst);
        let sql = if query_only {
            format!("KILL QUERY {id}")
        } else {
            format!("KILL {id}")
        };
        if let Err(e) = sqlx::raw_sql(AssertSqlSafe(sql)).execute(&self.meta).await {
            log::warn!("Could not kill editor thread {id}: {e}");
        }
    }

    async fn reconnect_editor(&self, conn: &mut MySqlConnection) -> AppResult<()> {
        let mut options = self.options.clone();
        if let Some(db) = self.current_database() {
            options = options.database(&db);
        }
        let mut fresh = options.connect().await?;
        self.editor_thread_id
            .store(thread_id(&mut fresh).await?, Ordering::SeqCst);
        *conn = fresh;
        Ok(())
    }

    pub async fn use_database(&self, database: &str) -> AppResult<()> {
        let mut conn = self.editor.lock().await;
        sqlx::raw_sql(AssertSqlSafe(format!("USE {}", quote_ident(database))))
            .execute(&mut *conn)
            .await?;
        *self.database.lock().unwrap() = Some(database.to_string());
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
                is_system: SYSTEM_SCHEMAS.contains(&name.to_ascii_lowercase().as_str()),
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
        let columns = self
            .strings(
                "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_KEY, EXTRA, \
                 COLUMN_COMMENT FROM information_schema.COLUMNS \
                 WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
                &[database, table],
            )
            .await?
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

        let mut indexes: Vec<IndexInfo> = Vec::new();
        for r in self
            .strings(
                "SELECT INDEX_NAME, CAST(NON_UNIQUE AS CHAR), COLUMN_NAME \
                 FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
                 ORDER BY INDEX_NAME = 'PRIMARY' DESC, INDEX_NAME, SEQ_IN_INDEX",
                &[database, table],
            )
            .await?
        {
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

        let ddl_sql = format!(
            "SHOW CREATE TABLE {}.{}",
            quote_ident(database),
            quote_ident(table)
        );
        let mut conn = self.meta.acquire().await?;
        let ddl = sqlx::raw_sql(AssertSqlSafe(ddl_sql))
            .fetch_optional(&mut *conn)
            .await?
            .and_then(|row| raw_text(&row, 1))
            .unwrap_or_default();

        Ok(TableStructure {
            columns,
            indexes,
            ddl,
        })
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
        let sql = browse_sql(req)?;
        let limit = req.limit as usize;
        let mut conn = self.meta.acquire().await?;
        let outcome = tokio::time::timeout(timeout, run_script(&mut conn, &sql, limit)).await;
        let mut result = match outcome {
            Ok(results) => results?.pop().unwrap_or_default(),
            Err(_) => {
                conn.close_on_drop();
                return Err(AppError::Timeout(timeout.as_secs() as u32));
            }
        };
        let has_more = result.truncated;
        result.truncated = false;
        result.total_rows = result.rows.len() as u64;
        Ok(BrowsePage {
            result,
            has_more,
            filter: applied_filter(req)?,
        })
    }

    /// Runs the statements in one transaction (all or nothing for transactional tables).
    pub async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        use sqlx::Connection;
        let mut conn = self.meta.acquire().await?;
        let work = async {
            let mut tx = conn.begin().await?;
            let mut affected = 0;
            for sql in statements {
                affected += sqlx::raw_sql(AssertSqlSafe(sql.clone()))
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
            }
            tx.commit().await?;
            Ok::<u64, sqlx::Error>(affected)
        };
        match tokio::time::timeout(timeout, work).await {
            Ok(result) => Ok(result?),
            Err(_) => Err(AppError::Timeout(timeout.as_secs() as u32)),
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
                Err(AppError::Timeout(timeout.as_secs() as u32))
            }
        }
    }

    /// Runs a metadata query with bound parameters; every column must be text (CAST as needed).
    async fn strings(
        &self,
        sql: &'static str,
        binds: &[&str],
    ) -> AppResult<Vec<Vec<Option<String>>>> {
        let mut query = sqlx::query(sql);
        for b in binds {
            query = query.bind(b.to_string());
        }
        let rows = query.fetch_all(&self.meta).await?;
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

fn columns_of(row: &MySqlRow) -> Vec<ColumnMeta> {
    row.columns()
        .iter()
        .map(|c| ColumnMeta {
            name: c.name().to_string(),
            type_name: c.type_info().name().to_string(),
        })
        .collect()
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

/// Runs a (possibly multi-statement) script over the text protocol. Each statement yields
/// one `ResultSet`; rows beyond `row_limit` are counted but not kept.
async fn run_script(
    conn: &mut MySqlConnection,
    sql: &str,
    row_limit: usize,
) -> AppResult<Vec<ResultSet>> {
    let mut results = Vec::new();
    let mut current = ResultSet::default();
    {
        let mut stream = sqlx::raw_sql(AssertSqlSafe(sql.to_owned())).fetch_many(&mut *conn);
        while let Some(item) = stream.try_next().await? {
            match item {
                Either::Right(row) => {
                    if current.columns.is_empty() {
                        current.columns = columns_of(&row);
                    }
                    current.returns_rows = true;
                    current.total_rows += 1;
                    if current.rows.len() < row_limit {
                        current.rows.push(decode_row(&row, &current.columns)?);
                    } else {
                        current.truncated = true;
                    }
                }
                Either::Left(done) => {
                    current.affected_rows = done.rows_affected();
                    let id = done.last_insert_id();
                    current.last_insert_id = (id > 0).then_some(id);
                    results.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if current.returns_rows {
        results.push(current);
    }

    // Empty results carry no column info over the text protocol; ask the server via PREPARE.
    let statements = split_statements(sql);
    if statements.len() == results.len() {
        for (statement, result) in statements.iter().zip(results.iter_mut()) {
            if result.columns.is_empty() && returns_rows(statement) {
                result.returns_rows = true;
                result.affected_rows = 0;
                let prepared = AssertSqlSafe(statement.to_string()).into_sql_str();
                if let Ok(stmt) = conn.prepare(prepared).await {
                    result.columns = stmt
                        .columns()
                        .iter()
                        .map(|c| ColumnMeta {
                            name: c.name().to_string(),
                            type_name: c.type_info().name().to_string(),
                        })
                        .collect();
                }
            }
        }
    }
    Ok(results)
}
