use std::collections::BTreeMap;

use tauri::State;

use super::actions::{self, ObjectAction};
use super::compare::Comparison;
use super::context::Ctx;
use super::diff::FlatDocument;
use super::snapshot::{self, SnapshotDiff};
use super::watch::{WatchPoll, Watches};
use super::{Document, Peek, parse_address};
use crate::db::plans::PlannedChange;
use crate::error::AppResult;
use crate::state::AppState;
use crate::store::snapshots::SnapshotInfo;

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

/// The values that say which row `address` is: its entity's summary, else its first columns.
#[tauri::command]
pub async fn peek(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
) -> AppResult<Peek> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let address = parse_address(&session, &address)?;
    let ctx = Ctx::load(&session, &address.database, state.rules(), timeout).await?;
    super::peek::peek(&ctx, &address).await
}

#[tauri::command]
pub async fn table_actions(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
) -> AppResult<Vec<ObjectAction>> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let ctx = Ctx::load(&session, &database, state.rules(), timeout).await?;
    Ok(actions::for_table(&ctx, &table).await)
}

/// Plans action `action` on the row at `address`, or on the table at a table address without a
/// filter, for review in the pending changes.
#[tauri::command]
pub async fn plan_action(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
    action: String,
    params: BTreeMap<String, String>,
) -> AppResult<PlannedChange> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let address = parse_address(&session, &address)?;
    let ctx = Ctx::load(&session, &address.database, state.rules(), timeout).await?;
    let plan = actions::plan(&ctx, &address, &action, &params).await?;
    Ok(session.plans.add(plan))
}

/// Starts watching the row at `address`; each `watch_poll` returns what changed since the last.
#[tauri::command]
pub async fn watch_start(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
) -> AppResult<String> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let document = super::open(&session, &address, None, state.rules(), timeout).await?;
    let watches = session.ext.get_or_insert_with(Watches::default);
    Ok(watches.start(address, FlatDocument::of(&document)))
}

/// Reads the watched row again: the changes since the last poll, and the new document when
/// anything changed.
#[tauri::command]
pub async fn watch_poll(
    state: State<'_, AppState>,
    session_id: String,
    id: String,
) -> AppResult<WatchPoll> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let watches = session.ext.get_or_insert_with(Watches::default);
    let address = watches.address(&id)?;
    let document = super::open(&session, &address, None, state.rules(), timeout).await?;
    let delta = watches.advance(&id, FlatDocument::of(&document))?;
    let document = (!delta.is_empty()).then_some(document);
    Ok(WatchPoll { delta, document })
}

/// Ends a watch; one that already ended, or whose session closed, is no error.
#[tauri::command]
pub fn watch_stop(state: State<'_, AppState>, session_id: String, id: String) {
    if let Ok(session) = state.sessions.get(&session_id)
        && let Some(watches) = session.ext.get::<Watches>()
    {
        watches.stop(&id);
    }
}

#[tauri::command]
pub async fn compare(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
) -> AppResult<Comparison> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    super::compare::compare(&session, &address, state.rules(), timeout).await
}

#[tauri::command]
pub async fn snapshot_take(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
    label: String,
) -> AppResult<SnapshotInfo> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    snapshot::take(
        &session,
        &state.snapshots,
        &address,
        &label,
        state.rules(),
        timeout,
    )
    .await
}

#[tauri::command]
pub fn snapshot_list(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    address: Option<String>,
) -> AppResult<Vec<SnapshotInfo>> {
    let session = state.sessions.get(&session_id)?;
    snapshot::list(&session, &state.snapshots, &database, address.as_deref())
}

#[tauri::command]
pub async fn snapshot_diff(
    state: State<'_, AppState>,
    session_id: String,
    id: String,
) -> AppResult<SnapshotDiff> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    snapshot::diff(&session, &state.snapshots, &id, state.rules(), timeout).await
}

#[tauri::command]
pub async fn snapshot_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.snapshots.delete(&id).await
}
