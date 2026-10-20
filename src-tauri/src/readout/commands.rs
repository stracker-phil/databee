use std::time::{SystemTime, UNIX_EPOCH};

use tauri::State;

use super::Readout;
use super::watched;
use crate::document::parse_address;
use crate::error::AppResult;
use crate::query::Ctx;
use crate::rules::project::WatchedValue;
use crate::state::AppState;
use crate::value::Provenance;

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

/// Watches `source` (a cell, or the node at `path` in its decoded value) on the readout of
/// `database`; a value watched already keeps its watch.
#[tauri::command]
pub async fn readout_watch(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    source: Provenance,
    path: Vec<String>,
) -> AppResult<WatchedValue> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let ctx = Ctx::load(&session, &database, state.rules(), timeout).await?;
    let row = watched::identity(&ctx, &source).await?;
    let watch = WatchedValue {
        id: crate::saved::new_id(),
        label: None,
        table: source.table,
        column: source.column,
        row,
        path,
    };
    state
        .projects
        .watch(&session.slug(), &database, watch)
        .await
}

#[tauri::command]
pub async fn readout_unwatch(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    id: String,
) -> AppResult<()> {
    let session = state.sessions.get(&session_id)?;
    state
        .projects
        .unwatch(&session.slug(), &database, &id)
        .await
}

/// `None` or a blank label goes back to the default one.
#[tauri::command]
pub async fn readout_watch_label(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    id: String,
    label: Option<String>,
) -> AppResult<WatchedValue> {
    let session = state.sessions.get(&session_id)?;
    state
        .projects
        .label_watched(&session.slug(), &database, &id, label)
        .await
}
