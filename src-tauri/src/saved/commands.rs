use std::sync::Arc;

use tauri::State;

use super::SavedEntry;
use crate::error::AppResult;
use crate::rules::effective::{self, EffectiveRules};
use crate::rules::project::SavedQuery;
use crate::rules::queries::Home;
use crate::schema;
use crate::session::Session;
use crate::state::AppState;

async fn rules(
    state: &AppState,
    session: &Session,
    database: &str,
) -> AppResult<Arc<EffectiveRules>> {
    let database_schema = schema::database(session, database).await?;
    Ok(effective::effective(
        session,
        database,
        &database_schema,
        state.rules(),
    ))
}

/// The saved queries of `database`: the project's, the global ones and the applied presets'.
#[tauri::command]
pub async fn saved_queries(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<SavedEntry>> {
    let session = state.sessions.get(&session_id)?;
    let rules = rules(&state, &session, &database).await?;
    let project = state.projects.get(&session.slug(), &database);
    Ok(super::list(&project, &state.projects.global(), &rules))
}

/// Stores a new query in the project of the session's connection and `database`.
#[tauri::command]
pub async fn save_query(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    title: String,
    sql: String,
    pin: Option<Home>,
) -> AppResult<SavedQuery> {
    let query = SavedQuery {
        id: super::new_id(),
        title: super::checked_title(&title)?,
        sql,
        pin,
    };
    let session = state.sessions.get(&session_id)?;
    super::add_to_project(&state.projects, &session.slug(), &database, query).await
}

/// Replaces the project or global query with the same id.
#[tauri::command]
pub async fn update_saved_query(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: SavedQuery,
) -> AppResult<SavedQuery> {
    let session = state.sessions.get(&session_id)?;
    super::update(&state.projects, &session.slug(), &database, query).await
}

/// Removes the project or global query `id`.
#[tauri::command]
pub async fn remove_saved_query(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    id: String,
) -> AppResult<()> {
    let session = state.sessions.get(&session_id)?;
    super::remove(&state.projects, &session.slug(), &database, id).await
}

/// Copies the pinned query `id` of the applied preset `preset` into the project, editable.
#[tauri::command]
pub async fn copy_preset_query_to_project(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    preset: String,
    id: String,
) -> AppResult<SavedQuery> {
    let session = state.sessions.get(&session_id)?;
    let rules = rules(&state, &session, &database).await?;
    let query = super::preset_copy(&rules, &preset, &id)?;
    super::add_to_project(&state.projects, &session.slug(), &database, query).await
}
