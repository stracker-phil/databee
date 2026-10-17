use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use super::actions::{self, ObjectAction};
use super::context::Ctx;
use super::diff::{DocDelta, FlatDocument, compare_documents, diff_documents};
use super::snapshot;
use super::watch::{WatchPoll, Watches};
use super::{Document, Peek, parse_address};
use crate::address::{Address, Target};
use crate::db::plans::PlannedChange;
use crate::error::{AppError, AppResult};
use crate::session::Session;
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

/// Two rows of one table side by side: the row of `address` and the one its `compare` names.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub left: Document,
    pub right: Document,
    /// `left` as the old side, `right` as the new.
    pub delta: DocDelta,
}

/// The documents of a row address with `?compare=<key>` and their differences.
#[tauri::command]
pub async fn compare(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
) -> AppResult<Comparison> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let address = parse_address(&session, &address)?;
    let (Target::Row { table, .. }, Some(other)) = (&address.target, &address.compare) else {
        return Err(AppError::InvalidInput(format!(
            "Not a row address with `?compare=`: {address}"
        )));
    };
    let left_address = Address {
        view: None,
        compare: None,
        ..address.clone()
    };
    let right_address = Address::new(
        address.connection.clone(),
        address.database.clone(),
        Target::Row {
            table: table.clone(),
            key: other.clone(),
        },
    );
    let (left, right) = futures_util::future::try_join(
        super::open(
            &session,
            &left_address.to_string(),
            None,
            state.rules(),
            timeout,
        ),
        super::open(
            &session,
            &right_address.to_string(),
            None,
            state.rules(),
            timeout,
        ),
    )
    .await?;
    let delta = compare_documents(&FlatDocument::of(&left), &FlatDocument::of(&right));
    Ok(Comparison { left, right, delta })
}

/// Stores the row or saved query at `address` as it is now.
#[tauri::command]
pub async fn snapshot_take(
    state: State<'_, AppState>,
    session_id: String,
    address: String,
    label: String,
) -> AppResult<SnapshotInfo> {
    let label = label.trim();
    if label.is_empty() {
        return Err(AppError::InvalidInput("A snapshot needs a label".into()));
    }
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let address = snapshot_address(&session, &address)?;
    let (cells, _) = snapshot::capture(&session, &address, state.rules(), timeout).await?;
    let created = chrono::Utc::now().timestamp();
    state
        .snapshots
        .add(label.to_string(), &address, created, cells)
        .await
}

/// The snapshots of `database` in this connection, newest first; only those of `address` when
/// given.
#[tauri::command]
pub fn snapshot_list(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    address: Option<String>,
) -> AppResult<Vec<SnapshotInfo>> {
    let session = state.sessions.get(&session_id)?;
    let address = address
        .map(|text| snapshot_address(&session, &text).map(|a| a.to_string()))
        .transpose()?;
    Ok(state
        .snapshots
        .list(&session.slug(), &database, address.as_deref()))
}

/// What changed since snapshot `id`: its address read now against the stored cells.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub snapshot: SnapshotInfo,
    pub delta: DocDelta,
    /// The row's document now; `None` for a saved query.
    pub document: Option<Document>,
}

#[tauri::command]
pub async fn snapshot_diff(
    state: State<'_, AppState>,
    session_id: String,
    id: String,
) -> AppResult<SnapshotDiff> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let (snapshot, stored) = state.snapshots.get(&id).await?;
    let address = snapshot_address(&session, &snapshot.address)?;
    let (now, document) = snapshot::capture(&session, &address, state.rules(), timeout).await?;
    let delta = diff_documents(&stored, &now);
    Ok(SnapshotDiff {
        snapshot,
        delta,
        document,
    })
}

#[tauri::command]
pub async fn snapshot_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.snapshots.delete(&id).await
}

/// A row or saved-query address of this session's connection, without view or compare.
fn snapshot_address(session: &Session, text: &str) -> AppResult<Address> {
    let address = parse_address(session, text)?;
    Ok(Address {
        view: None,
        compare: None,
        ..address
    })
}
