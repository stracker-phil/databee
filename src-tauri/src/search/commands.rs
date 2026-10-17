use tauri::State;

use super::PaletteHit;
use crate::error::AppResult;
use crate::state::AppState;

/// The palette's hits for `query` in `database` (`#id`, `@name`, `=value` or free text).
#[tauri::command]
pub async fn palette_search(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: String,
) -> AppResult<Vec<PaletteHit>> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    super::search(&session, &database, &query, state.rules(), timeout).await
}
