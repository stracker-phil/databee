use std::collections::BTreeMap;

use tauri::State;

use super::actions::{self, ObjectAction};
use super::context::Ctx;
use super::{Document, Peek, parse_address};
use crate::db::plans::PlannedChange;
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

/// The `scope = "table"` actions of `table`.
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
