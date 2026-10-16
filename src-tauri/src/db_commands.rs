use std::collections::BTreeMap;
use std::time::Instant;

use tauri::State;

use crate::db::safety::destructive_reason;
use crate::db::{
    BrowsePage, BrowseRequest, DatabaseInfo, ExecuteResponse, TableInfo, TableStructure,
};
use crate::error::{AppError, AppResult};
use crate::session::{self, OpenArgs, SessionInfo};
use crate::state::AppState;
use crate::store::history::HistoryEntry;

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

#[tauri::command]
pub async fn describe_table(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
) -> AppResult<TableStructure> {
    state
        .sessions
        .get(&session_id)?
        .db
        .structure(&database, &table)
        .await
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
        .db
        .execute(&sql, settings.row_limit as usize, settings.query_timeout())
        .await;
    let elapsed_ms = match &response {
        Ok(r) => {
            log::debug!("Query finished in {} ms", r.elapsed_ms);
            r.elapsed_ms
        }
        Err(e) => {
            // The message may quote the SQL, which stays out of the log by default.
            log::info!("Query failed: kind={} code={:?}", e.kind(), e.code());
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
        error: response.as_ref().err().map(ToString::to_string),
    };
    if let Err(e) = state.history.record(entry) {
        log::warn!("Could not record history: {e}");
    }
    response
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.sessions.get(&session_id)?.db.cancel().await?;
    log::info!("Cancel requested");
    Ok(())
}

#[tauri::command]
pub async fn browse_table(
    state: State<'_, AppState>,
    session_id: String,
    request: BrowseRequest,
) -> AppResult<BrowsePage> {
    let timeout = state.settings.get().query_timeout();
    state
        .sessions
        .get(&session_id)?
        .db
        .browse(&request, timeout)
        .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::connection::ConnectionSpec;
    use crate::db::{FakeDatabase, ResultSet};
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
        state.sessions.insert(Session {
            id: "s1".into(),
            spec: spec(),
            db: Box::new(db),
            ext: Default::default(),
        });
        (dir, state)
    }

    #[tokio::test]
    async fn destructive_statement_without_confirmation_is_rejected_before_the_session_lookup() {
        let (_dir, state) = state_with(FakeDatabase::new());

        let err = run_query(&state, "unknown", "DROP TABLE t".into(), false)
            .await
            .unwrap_err();

        assert_eq!(err.kind(), "confirmation_required");
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

        assert_eq!(err.kind(), "session_not_found");
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

        assert_eq!(err.kind(), "invalid_input");
        let history = state.history.list(&spec().account());
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sql, "SELECT 1");
        assert_eq!(history[0].error.as_deref(), Some(err.to_string().as_str()));
    }
}
