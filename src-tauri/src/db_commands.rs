use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;
use tauri::State;

use crate::connection::{ConnectionSpec, Secret};
use crate::db::mysql::MySqlSession;
use crate::db::safety::destructive_reason;
use crate::db::{
    BrowsePage, BrowseRequest, DatabaseInfo, ExecuteResponse, TableInfo, TableStructure,
};
use crate::error::{AppError, AppResult};
use crate::secrets::{PasswordSource, resolve_password};
use crate::session::{Session, SessionInfo};
use crate::state::AppState;
use crate::store::history::HistoryEntry;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConnectArgs {
    pub spec: ConnectionSpec,
    #[serde(default)]
    pub password: Option<Secret>,
    /// A CLI launch request whose password should be used.
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub save_password: bool,
}

/// The error the UI sees when the server rejects the login. `password_required` and
/// `auth_failed` both make the UI prompt; `auth_failed` also shows the message, so a
/// rejected Keychain password or a failed Keychain lookup is explained in the prompt.
fn auth_error(source: &PasswordSource, account: String, message: String) -> AppError {
    match source {
        PasswordSource::None => AppError::PasswordRequired(account),
        PasswordSource::Unavailable(reason) => {
            AppError::AuthFailed(format!("No password was sent ({reason})"))
        }
        PasswordSource::Stored(_) => AppError::AuthFailed(format!(
            "The password stored in the Keychain was rejected ({message})"
        )),
        PasswordSource::Provided(_) => AppError::AuthFailed(message),
    }
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, args: ConnectArgs) -> AppResult<SessionInfo> {
    // Always looked up, so a launch password is not left behind when the page also sends one.
    let launch_password = args
        .request_id
        .as_deref()
        .and_then(|id| state.launches.peek_password(id));
    let explicit = args.password.or(launch_password);
    let source = resolve_password(explicit, state.secrets.as_ref(), &args.spec);
    let settings = state.settings.get();
    let connect_timeout = Duration::from_secs(settings.connect_timeout_secs.into());
    let query_timeout = Duration::from_secs(settings.query_timeout_secs.into());
    let identity = args.spec.identity();
    log::info!("Connecting to {identity}");

    let connecting =
        MySqlSession::connect(&args.spec, source.secret(), connect_timeout, query_timeout).await;
    // A timeout or network error keeps the launch password for a retry; a rejected one is useless.
    let launch_password_spent = matches!(connecting, Ok(_) | Err(AppError::AuthFailed(_)));
    if launch_password_spent && let Some(id) = &args.request_id {
        state.launches.take_password(id);
    }
    let db = match connecting {
        Ok(db) => db,
        Err(AppError::AuthFailed(message)) => {
            log::warn!("Authentication failed for {identity}");
            return Err(auth_error(&source, args.spec.account(), message));
        }
        Err(e) => {
            log::warn!("Connection to {identity} failed: {e}");
            return Err(e);
        }
    };

    if let (true, PasswordSource::Provided(secret)) = (args.save_password, &source) {
        match state.secrets.set(&args.spec.account(), secret) {
            Ok(()) => log::info!("Saved password for {} to Keychain", args.spec.account()),
            Err(e) => log::warn!("Could not save password: {e}"),
        }
    }
    if let Err(e) = state.recents.touch(&args.spec) {
        log::warn!("Could not update recent connections: {e}");
    }
    log::info!("Connected to {identity} ({})", db.server_version);
    let session = state.sessions.insert(Session {
        id: uuid::Uuid::new_v4().to_string(),
        spec: args.spec,
        db,
    });
    Ok(session.info())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    if let Some(session) = state.sessions.remove(&session_id) {
        session.db.close().await;
        log::info!("Disconnected from {}", session.spec.identity());
    }
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
    let settings = state.settings.get();
    if settings.confirm_destructive
        && !confirmed
        && let Some(reason) = destructive_reason(&sql)
    {
        return Err(AppError::ConfirmationRequired(reason));
    }
    let session = state.sessions.get(&session_id)?;
    log::debug!("Executing query on {}", session.spec.identity());
    log::trace!("SQL: {sql}");
    let timeout = Duration::from_secs(settings.query_timeout_secs.into());
    let database = session.db.current_database();
    let started = std::time::Instant::now();
    let response = session
        .db
        .execute(&sql, settings.row_limit as usize, timeout)
        .await;
    match &response {
        Ok(r) => log::debug!("Query finished in {} ms", r.elapsed_ms),
        Err(e) => {
            // The message may quote the SQL, which stays out of the log by default.
            log::info!("Query failed: kind={} code={:?}", e.kind(), e.code());
            log::debug!("Query error: {e}");
        }
    }
    let entry = HistoryEntry {
        account: session.spec.account(),
        database,
        sql,
        truncated: false,
        executed_at: chrono::Utc::now(),
        elapsed_ms: started.elapsed().as_millis() as u64,
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
    let timeout = Duration::from_secs(state.settings.get().query_timeout_secs.into());
    state
        .sessions
        .get(&session_id)?
        .db
        .browse(&request, timeout)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_password_asks_for_one() {
        let err = auth_error(&PasswordSource::None, "wp@h:1".into(), "denied".into());
        assert_eq!(err.kind(), "password_required");
        assert_eq!(err.to_string(), "A password is required for wp@h:1");
    }

    #[test]
    fn keychain_failure_is_explained_in_the_message() {
        let source = PasswordSource::Unavailable("Keychain locked".into());
        let err = auth_error(&source, "a".into(), "denied".into());
        assert_eq!(err.kind(), "auth_failed");
        assert_eq!(err.to_string(), "No password was sent (Keychain locked)");
    }

    #[test]
    fn rejected_stored_password_mentions_the_keychain() {
        let source = PasswordSource::Stored(Secret::new("old"));
        let err = auth_error(&source, "a".into(), "Access denied".into());
        assert_eq!(err.kind(), "auth_failed");
        assert_eq!(
            err.to_string(),
            "The password stored in the Keychain was rejected (Access denied)"
        );
    }

    #[test]
    fn rejected_provided_password_passes_the_server_message_through() {
        let source = PasswordSource::Provided(Secret::new("typed"));
        let err = auth_error(&source, "a".into(), "Access denied".into());
        assert_eq!(err.kind(), "auth_failed");
        assert_eq!(err.to_string(), "Access denied");
    }
}
