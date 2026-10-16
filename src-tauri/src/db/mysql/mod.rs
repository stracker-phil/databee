mod editor;
mod introspect;
mod kill;
mod script;
mod timeouts;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sqlx::AssertSqlSafe;
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection, MySqlPool, MySqlPoolOptions};

use self::editor::EditorConnection;
use self::kill::Killer;
use self::script::{ScriptOutcome, run_script, scalar};
use self::timeouts::{connect_within, pooled_within, query_within};
use super::browse::{browse_sql, columns_filter, page_limit};
use super::{
    BrowsePage, BrowseRequest, CellLimit, Database, DatabaseInfo, Dialect, ExecuteResponse,
    ResultSet, TableInfo, TableStructure,
};
use crate::connection::{ConnectionSpec, Secret};
use crate::error::{AppError, AppResult};

const META_POOL_SIZE: u32 = 3;
/// Rows a WordPress or planning read may return.
const MAX_FETCH_ROWS: usize = 50_000;

/// One logical connection: a dedicated "editor" connection that keeps session state
/// (`USE`, variables, transactions) for user queries, plus a small pool for metadata
/// and table browsing so the sidebar stays responsive while a query runs.
pub struct MySqlSession {
    /// Default bound for metadata and `USE`; `execute`, `browse`, `fetch` and `apply` take their own.
    query_timeout: Duration,
    meta: MySqlPool,
    editor: EditorConnection,
    /// Kills timed-out queries on pool connections.
    killer: Killer,
    /// From a fresh connection's `sql_mode`, the server default the metadata pool shares.
    backslash_escapes: AtomicBool,
    server_version: String,
}

/// Without TLS support compiled in, `caching_sha2_password` fetches the server's RSA key in
/// plain text; sqlx has no switch to refuse that, so it is only safe on loopback or sockets.
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
        let killer = Killer::new(options.clone(), connect_timeout);
        let editor = EditorConnection::connect(
            editor_options,
            spec.database.clone(),
            connect_timeout,
            killer.clone(),
        );
        let (meta, (editor, server)) =
            connect_within(connect_timeout, async { tokio::try_join!(pool, editor) }).await?;
        Ok(Self {
            query_timeout,
            meta,
            editor,
            killer,
            backslash_escapes: AtomicBool::new(server.dialect.backslash_escapes),
            server_version: server.version,
        })
    }

    /// After a timeout the abandoned connection is mid-result: kill it server-side and start
    /// fresh. A lost connection is rebuilt; other errors leave it as is. A timeout comes back
    /// telling the UI whether the editor session (temporary tables, variables, open
    /// transactions) went with the replaced connection.
    async fn recover_editor(&self, error: AppError, conn: &mut MySqlConnection) -> AppError {
        match error {
            AppError::Timeout { limit, .. } => {
                if let Err(e) = self.editor.kill_connection().await {
                    log::warn!("Could not kill the timed-out editor thread: {e}");
                }
                AppError::Timeout {
                    limit,
                    session_reset: self.reconnect_or_log(conn).await,
                }
            }
            AppError::Connection(_) => {
                log::warn!("Editor connection lost, reconnecting");
                self.reconnect_or_log(conn).await;
                error
            }
            other => other,
        }
    }

    /// The script already ran (and may have committed), so a failed probe must not lose it.
    fn remember_database(&self, probe: AppResult<Option<String>>) -> Option<String> {
        match probe {
            Ok(database) => {
                self.editor.set_database(database.clone());
                database
            }
            Err(e) => {
                log::warn!("Could not read the current database after a query: {e}");
                self.current_database()
            }
        }
    }

    /// Reconnects after a failed run. The run's own error is what the user sees; a failed
    /// reconnect only leaves the broken connection for the next run to replace.
    async fn reconnect_or_log(&self, conn: &mut MySqlConnection) -> bool {
        match self.editor.reconnect(conn).await {
            Ok(dialect) => {
                self.backslash_escapes
                    .store(dialect.backslash_escapes, Ordering::SeqCst);
                true
            }
            Err(e) => {
                log::warn!("Could not reconnect the editor connection: {e}");
                false
            }
        }
    }
}

#[async_trait]
impl Database for MySqlSession {
    fn dialect(&self) -> Dialect {
        Dialect {
            backslash_escapes: self.backslash_escapes.load(Ordering::SeqCst),
        }
    }

    fn server_version(&self) -> &str {
        &self.server_version
    }

    fn current_database(&self) -> Option<String> {
        self.editor.current_database()
    }

    async fn close(&self) {
        self.editor.close().await;
        self.meta.close().await;
    }

    async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<ExecuteResponse> {
        let started = Instant::now();
        let mut conn = self.editor.lock().await?;
        let run = async {
            let outcome = run_script(&mut conn, sql, row_limit, cell_limit).await;
            // Also after a failure: a `USE` before the failing statement still counts.
            let database = scalar(&mut conn, "SELECT DATABASE()").await;
            Ok((outcome, database))
        };
        let (ScriptOutcome { results, failure }, probe) = match query_within(timeout, run).await {
            Ok(outcome) => outcome,
            Err(e) => return Err(self.recover_editor(e, &mut conn).await),
        };
        let database = self.remember_database(probe);
        let failure = match failure {
            Some(mut failure) => {
                failure.error = self.recover_editor(failure.error, &mut conn).await;
                Some(failure)
            }
            None => None,
        };
        if let Some(failure) = failure.as_ref().filter(|f| f.index == 0) {
            return Err(failure.error.clone());
        }
        Ok(ExecuteResponse {
            results,
            elapsed_ms: started.elapsed().as_millis() as u64,
            database,
            failure,
        })
    }

    /// Interrupts the statement running on the editor connection.
    async fn cancel(&self) -> AppResult<()> {
        self.editor.kill_query().await
    }

    async fn use_database(&self, database: &str) -> AppResult<()> {
        let sql = format!("USE {}", self.dialect().quote_ident(database));
        let mut conn = self.editor.lock().await?;
        query_within(self.query_timeout, async {
            sqlx::raw_sql(AssertSqlSafe(sql))
                .execute(&mut *conn)
                .await?;
            Ok(())
        })
        .await?;
        self.editor.set_database(Some(database.to_string()));
        Ok(())
    }

    async fn databases(&self) -> AppResult<Vec<DatabaseInfo>> {
        introspect::databases(&self.meta, self.query_timeout, self.dialect()).await
    }

    async fn tables(&self, database: &str) -> AppResult<Vec<TableInfo>> {
        introspect::tables(&self.meta, self.query_timeout, database).await
    }

    async fn structure(&self, database: &str, table: &str) -> AppResult<TableStructure> {
        introspect::structure(
            &self.meta,
            self.query_timeout,
            self.dialect(),
            database,
            table,
        )
        .await
    }

    async fn completion(&self, database: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
        introspect::completion(&self.meta, self.query_timeout, database).await
    }

    async fn browse(
        &self,
        req: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
    ) -> AppResult<BrowsePage> {
        let (sql, filter) = browse_sql(req, self.dialect())?;
        let limit = page_limit(req);
        let results = pooled_within(timeout, &self.meta, &self.killer, async |conn| {
            run_script(conn, &sql, limit as usize, cell_limit)
                .await
                .into_results()
        })
        .await?;
        let mut result = results.into_iter().last().unwrap_or_default();
        let has_more = result.truncated;
        result.truncated = false;
        result.total_rows = result.rows.len() as u64;
        Ok(BrowsePage {
            result,
            has_more,
            limit,
            filter,
            columns_filter: columns_filter(req, self.dialect()),
        })
    }

    /// Runs the statements in one transaction (all or nothing for transactional tables).
    async fn apply(&self, statements: &[String], timeout: Duration) -> AppResult<u64> {
        use sqlx::Connection;
        let commit_sent = AtomicBool::new(false);
        let applied = pooled_within(timeout, &self.meta, &self.killer, async |conn| {
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
            Ok(affected)
        })
        .await;
        match applied {
            Err(AppError::Timeout { .. }) if commit_sent.load(Ordering::SeqCst) => {
                Err(AppError::Database {
                    message:
                        "Timed out after COMMIT was sent; whether the changes were saved is unknown"
                            .into(),
                    code: None,
                })
            }
            other => other,
        }
    }

    /// Runs a read query on the metadata pool and returns its (last) result set.
    async fn fetch(&self, sql: &str, timeout: Duration) -> AppResult<ResultSet> {
        let results = pooled_within(timeout, &self.meta, &self.killer, async |conn| {
            run_script(conn, sql, MAX_FETCH_ROWS, CellLimit::NONE)
                .await
                .into_results()
        })
        .await?;
        Ok(results.into_iter().last().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(socket: Option<&str>) -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "db.test".into(),
            port: 3307,
            socket: socket.map(Into::into),
            user: "dev".into(),
            database: None,
        }
    }

    #[test]
    fn socket_in_the_spec_is_used_instead_of_host_and_port() {
        let options = connect_options(&spec(Some("/tmp/mysql.sock")), None);
        assert_eq!(
            options.get_socket().and_then(|p| p.to_str()),
            Some("/tmp/mysql.sock")
        );
    }

    #[test]
    fn without_a_socket_host_and_port_come_from_the_spec() {
        let options = connect_options(&spec(None), None);
        assert_eq!(options.get_host(), "db.test");
        assert_eq!(options.get_port(), 3307);
        assert!(options.get_socket().is_none());
    }

    #[test]
    fn username_comes_from_the_spec() {
        let options = connect_options(&spec(None), Some(&Secret::new("pw")));
        assert_eq!(options.get_username(), "dev");
    }

    #[test]
    fn connect_options_select_no_database_until_one_is_given() {
        let options = connect_options(&spec(None), None);
        assert_eq!(options.get_database(), None);
        assert_eq!(
            with_database(&options, Some("wp")).get_database(),
            Some("wp")
        );
        assert_eq!(with_database(&options, None).get_database(), None);
    }
}
