use std::collections::BTreeMap;
use std::time::Instant;

use tauri::State;

use crate::address::Address;
use crate::address::resolve::{self, Resolved};
use crate::db::browse::select_all_sql;
use crate::db::changes::{self, ChangeRequest};
use crate::db::plans::{AppliedBatchInfo, BatchId, PlanId, PlannedChange};
use crate::db::safety::destructive_reason;
use crate::db::{
    BrowsePage, BrowseRequest, DatabaseInfo, ExecuteResponse, TableInfo, TableStructure,
};
use crate::error::{AppError, AppResult};
use crate::lens::{self, TableReferences};
use crate::schema;
use crate::session::{self, OpenArgs, SessionInfo};
use crate::state::AppState;
use crate::store::history::HistoryEntry;
use crate::value::{self, Provenance};

/// Rows a "New query" on a table asks for.
const NEW_QUERY_LIMIT: u32 = 100;

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, args: OpenArgs) -> AppResult<SessionInfo> {
    Ok(session::open(&state, args).await?.info())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    session::close(&state, &session_id).await;
    Ok(())
}

#[tauri::command]
pub async fn list_databases(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<Vec<DatabaseInfo>> {
    state.sessions.get(&session_id)?.db.databases().await
}

#[tauri::command]
pub async fn list_tables(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<TableInfo>> {
    state.sessions.get(&session_id)?.db.tables(&database).await
}

/// Always read from the server, because the DDL text's `AUTO_INCREMENT` moves with inserts; the
/// result refreshes the schema cache on the way.
#[tauri::command]
pub async fn describe_table(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
) -> AppResult<TableStructure> {
    let session = state.sessions.get(&session_id)?;
    schema::fresh_structure(&session, &database, &table).await
}

/// Drops the session's schema cache, so DDL made outside the app shows (`view.refresh`).
#[tauri::command]
pub fn refresh_schema(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    let session = state.sessions.get(&session_id)?;
    schema::refresh(&session);
    Ok(())
}

#[tauri::command]
pub async fn completion_schema(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<BTreeMap<String, Vec<String>>> {
    state
        .sessions
        .get(&session_id)?
        .db
        .completion(&database)
        .await
}

#[tauri::command]
pub async fn use_database(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<()> {
    state
        .sessions
        .get(&session_id)?
        .db
        .use_database(&database)
        .await
}

#[tauri::command]
pub async fn execute(
    state: State<'_, AppState>,
    session_id: String,
    sql: String,
    confirmed: bool,
) -> AppResult<ExecuteResponse> {
    run_query(&state, &session_id, sql, confirmed).await
}

/// Runs a user query and records it in the history, failed or not. The confirmation gate
/// comes before the session lookup, so it holds even for a stale session id.
pub async fn run_query(
    state: &AppState,
    session_id: &str,
    sql: String,
    confirmed: bool,
) -> AppResult<ExecuteResponse> {
    let settings = state.settings.get();
    if settings.confirm_destructive
        && !confirmed
        && let Some(reason) = destructive_reason(&sql)
    {
        return Err(AppError::ConfirmationRequired(reason));
    }
    let session = state.sessions.get(session_id)?;
    log::debug!("Executing query on {}", session.spec.identity());
    log::trace!("SQL: {sql}");
    let database = session.db.current_database();
    let started = Instant::now();
    let response = session
        .execute(
            &sql,
            settings.row_limit as usize,
            settings.cell_limit(),
            settings.query_timeout(),
            Some(state.rules()),
        )
        .await;
    let elapsed_ms = match &response {
        Ok(r) => {
            log::debug!("Query finished in {} ms", r.elapsed_ms);
            r.elapsed_ms
        }
        Err(e) => {
            // The message may quote the SQL, which stays out of the log by default.
            log::info!("Query failed: kind={:?} code={:?}", e.kind(), e.code());
            log::debug!("Query error: {e}");
            started.elapsed().as_millis() as u64
        }
    };
    let entry = HistoryEntry {
        account: session.spec.account(),
        database,
        sql,
        truncated: false,
        executed_at: chrono::Utc::now(),
        elapsed_ms,
        error: history_error(&response),
    };
    state.history.record(entry);
    response
}

/// The error of the run, or of the statement that stopped a partly run script.
fn history_error(response: &AppResult<ExecuteResponse>) -> Option<String> {
    match response {
        Ok(r) => r.failure.as_ref().map(|f| f.error.to_string()),
        Err(e) => Some(e.to_string()),
    }
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.sessions.get(&session_id)?.db.cancel().await?;
    log::info!("Cancel requested");
    Ok(())
}

/// The starting SQL of a "New query" on a table; quoting stays in Rust.
#[tauri::command]
pub fn table_select_sql(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
) -> AppResult<String> {
    let dialect = state.sessions.get(&session_id)?.db.dialect();
    Ok(select_all_sql(dialect, &database, &table, NEW_QUERY_LIMIT))
}

#[tauri::command]
pub async fn browse_table(
    state: State<'_, AppState>,
    session_id: String,
    request: BrowseRequest,
) -> AppResult<BrowsePage> {
    let settings = state.settings.get();
    let session = state.sessions.get(&session_id)?;
    session
        .browse(
            &request,
            settings.cell_limit(),
            settings.query_timeout(),
            Some(state.rules()),
        )
        .await
}

/// Parses `address` and checks it against the session: its slug, database, table and key.
#[tauri::command]
pub async fn resolve_address(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
) -> AppResult<Resolved> {
    let address = Address::parse(&address)?;
    let session = state.sessions.get(&session_id)?;
    resolve::resolve(&session, address, state.settings.get().query_timeout()).await
}

/// The references from and to `table`: declared foreign keys, project and preset references,
/// and same-name inference.
#[tauri::command]
pub async fn table_references(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
) -> AppResult<TableReferences> {
    let session = state.sessions.get(&session_id)?;
    lens::table_references(&session, &database, &table, Some(state.rules())).await
}

/// The value at `provenance` as a SQL condition, quoted for the server (`value.copyCondition`);
/// `raw: None` is NULL.
#[tauri::command]
pub async fn value_condition(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    provenance: Provenance,
    raw: Option<String>,
) -> AppResult<String> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    value::value_condition(
        &session,
        &database,
        &provenance,
        raw.as_deref(),
        state.rules(),
        timeout,
    )
    .await
}

/// The full text of a cell that a result set sent as a `LongText` preview.
#[tauri::command]
pub fn fetch_cell(
    state: State<'_, AppState>,
    session_id: String,
    result_id: u64,
    row: usize,
    col: usize,
) -> AppResult<String> {
    state
        .sessions
        .get(&session_id)?
        .cells
        .get(result_id, row, col)
}

#[tauri::command]
pub async fn plan_change(
    state: State<'_, AppState>,
    session_id: String,
    change: ChangeRequest,
) -> AppResult<PlannedChange> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let plan = changes::plan(session.db.as_ref(), &change, timeout).await?;
    Ok(session.plans.add(plan))
}

#[tauri::command]
pub fn discard_change(state: State<'_, AppState>, session_id: String, id: PlanId) -> AppResult<()> {
    state.sessions.get(&session_id)?.plans.discard(id);
    Ok(())
}

#[tauri::command]
pub async fn apply_changes(
    state: State<'_, AppState>,
    session_id: String,
    ids: Vec<PlanId>,
) -> AppResult<AppliedBatchInfo> {
    let timeout = state.settings.get().query_timeout();
    state
        .sessions
        .get(&session_id)?
        .apply_plans(&ids, timeout)
        .await
}

#[tauri::command]
pub async fn undo_batch(
    state: State<'_, AppState>,
    session_id: String,
    batch_id: BatchId,
) -> AppResult<u64> {
    let timeout = state.settings.get().query_timeout();
    state
        .sessions
        .get(&session_id)?
        .undo_batch(batch_id, timeout)
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::db::{Cell, FakeDatabase, FullText, ResultSet, StatementFailure};
    use crate::error::ErrorKind;
    use crate::secrets::MemoryStore;
    use crate::session::Session;

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: Some("wp".into()),
        }
    }

    fn state_with(db: FakeDatabase) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(MemoryStore::default()));
        state.sessions.insert(Session::new(
            "s1".into(),
            spec(),
            "local".into(),
            Box::new(db),
        ));
        (dir, state)
    }

    #[tokio::test]
    async fn destructive_statement_without_confirmation_is_rejected_before_the_session_lookup() {
        let (_dir, state) = state_with(FakeDatabase::new());

        let err = run_query(&state, "unknown", "DROP TABLE t".into(), false)
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::ConfirmationRequired);
        assert!(state.history.list(&spec().account()).is_empty());
    }

    #[tokio::test]
    async fn confirmed_destructive_statement_runs() {
        let db = FakeDatabase::new().on("DROP TABLE t", ResultSet::default());
        let (_dir, state) = state_with(db);

        let response = run_query(&state, "s1", "DROP TABLE t".into(), true)
            .await
            .unwrap();

        assert_eq!(response.results.len(), 1);
    }

    #[tokio::test]
    async fn unknown_session_is_reported_for_a_harmless_statement() {
        let (_dir, state) = state_with(FakeDatabase::new());

        let err = run_query(&state, "unknown", "SELECT 1".into(), false)
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::SessionNotFound);
    }

    #[tokio::test]
    async fn successful_query_is_recorded_in_the_history_without_an_error() {
        let (_dir, state) = state_with(
            FakeDatabase::new()
                .with_database("wp")
                .on("SELECT 1", ResultSet::default()),
        );

        run_query(&state, "s1", "SELECT 1".into(), false)
            .await
            .unwrap();

        let history = state.history.list(&spec().account());
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sql, "SELECT 1");
        assert_eq!(history[0].database.as_deref(), Some("wp"));
        assert_eq!(history[0].error, None);
    }

    #[tokio::test]
    async fn failed_query_is_recorded_in_the_history_with_its_error_and_returned() {
        let (_dir, state) = state_with(FakeDatabase::new());

        let err = run_query(&state, "s1", "SELECT 1".into(), false)
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        let history = state.history.list(&spec().account());
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sql, "SELECT 1");
        assert_eq!(history[0].error.as_deref(), Some(err.to_string().as_str()));
    }

    fn response(failure: Option<StatementFailure>) -> ExecuteResponse {
        ExecuteResponse {
            results: Vec::new(),
            elapsed_ms: 0,
            database: None,
            failure,
        }
    }

    #[test]
    fn history_records_the_error_of_the_statement_that_stopped_a_script() {
        let failed = response(Some(StatementFailure {
            index: 1,
            error: AppError::InvalidInput("bad statement".into()),
        }));

        assert_eq!(history_error(&Ok(failed)).as_deref(), Some("bad statement"));
        assert_eq!(history_error(&Ok(response(None))), None);
        assert_eq!(
            history_error(&Err(AppError::Cancelled)).as_deref(),
            Some("Query was cancelled")
        );
    }

    #[tokio::test]
    async fn full_text_of_a_long_cell_is_kept_in_the_session_cache_under_the_result_id() {
        let long = ResultSet {
            rows: vec![vec![Cell::LongText {
                len: 9,
                preview: "abc".into(),
            }]],
            full_texts: vec![FullText {
                row: 0,
                col: 0,
                text: "abcdefghi".into(),
            }],
            ..ResultSet::default()
        };
        let (_dir, state) = state_with(FakeDatabase::new().on("SELECT long", long));

        let response = run_query(&state, "s1", "SELECT long".into(), false)
            .await
            .unwrap();

        let result = &response.results[0];
        assert_ne!(result.id, 0);
        assert!(result.full_texts.is_empty());
        let session = state.sessions.get("s1").unwrap();
        assert_eq!(session.cells.get(result.id, 0, 0).unwrap(), "abcdefghi");
        assert_eq!(
            session.cells.get(result.id + 1, 0, 0).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
}
