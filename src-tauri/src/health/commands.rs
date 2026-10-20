use tauri::State;

use super::{Health, retry};
use crate::error::AppResult;
use crate::state::AppState;

/// Retry (the toolbar's button, and once when the window regains focus): reconnects a `down`
/// session with the password it already holds, so nothing asks for one. Returns where the
/// session stands; an `ok` or already reconnecting session is left alone.
#[tauri::command]
pub async fn reconnect_session(state: State<'_, AppState>, session_id: String) -> AppResult<Health> {
    let session = state.sessions.get(&session_id)?;
    Ok(retry(session.db.as_ref(), &session.health).await)
}
