use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use super::Readout;
use crate::document::parse_address;
use crate::error::AppResult;
use crate::state::AppState;

/// The readout of `database`; parts that fail carry their error.
#[tauri::command]
pub async fn readout(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Readout> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let recent = state.recent_objects.get(&session.slug(), &database);
    Ok(super::readout(&session, &database, state.rules(), &recent, timeout).await)
}

/// Records a row address opened in the app as the newest of its project's recent objects.
#[tauri::command]
pub async fn remember_object(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
) -> AppResult<()> {
    let session = state.sessions.get(&session_id)?;
    let address = parse_address(&session, &address)?;
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    super::remember(&state.recent_objects, &address, at).await
}
