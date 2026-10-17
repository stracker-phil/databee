use tauri::State;

use super::Document;
use crate::error::AppResult;
use crate::state::AppState;

/// The document of a row address, or of an entity shorthand (`order 4711`) in `database`.
#[tauri::command]
pub async fn document(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
    database: Option<String>,
) -> AppResult<Document> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    super::open(&session, &address, database, state.rules(), timeout).await
}
