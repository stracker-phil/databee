use std::collections::BTreeMap;

use tauri::State;

use super::PresetInfo;
use super::coverage::{self, Coverage};
use super::draft::{self, RuleDraft};
use super::effective::{self, RulesInfo};
use super::overview::{self, ProjectRules};
use super::profile::{self, TableProfile};
use super::project::Project;
use super::project_change::{self, ChangeContext, ProjectChange};
use super::types::Reference;
use crate::error::{AppError, AppResult};
use crate::lens::ResolvedReference;
use crate::state::AppState;
use crate::value::Provenance;

/// Every known preset, with the parse error of a user file that does not load.
#[tauri::command]
pub fn presets_list(state: State<'_, AppState>) -> Vec<PresetInfo> {
    state.presets.list()
}

#[tauri::command]
pub async fn rules_info(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<RulesInfo> {
    let session = state.sessions.get(&session_id)?;
    effective::rules_info(&session, &database, state.rules()).await
}

/// The project of the session's connection and `database`; an empty one when none is stored.
#[tauri::command]
pub fn project_get(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Project> {
    let session = state.sessions.get(&session_id)?;
    Ok(state.projects.get(&session.slug(), &database))
}

/// Checks `change` against the database's tables and columns, then stores it.
#[tauri::command]
pub async fn project_update(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    change: ProjectChange,
) -> AppResult<Project> {
    let session = state.sessions.get(&session_id)?;
    let ctx = ChangeContext::load(&session, &database, &state.presets).await?;
    state
        .projects
        .update(&session.slug(), &database, move |project| {
            project_change::apply(project, change, &ctx)
        })
        .await
}

/// Every project's accent by `<slug>/<database>`, read from the projects file only (no server
/// round trip), so the page knows a database's accent before anything of it loads.
#[tauri::command]
pub fn project_accents(state: State<'_, AppState>) -> BTreeMap<String, String> {
    state.projects.accents()
}

/// Sets the accent of the session's connection and `database`; `null` uses the app's again.
/// Not a `ProjectChange`: it changes no rule, so nothing reloads and "Your changes" stays as it is.
#[tauri::command]
pub async fn project_set_accent(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    accent: Option<String>,
) -> AppResult<Option<String>> {
    let session = state.sessions.get(&session_id)?;
    state
        .projects
        .set_accent(&session.slug(), &database, accent)
        .await
}

#[tauri::command]
pub fn project_export(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<String> {
    let session = state.sessions.get(&session_id)?;
    state.projects.get(&session.slug(), &database).export()
}

#[tauri::command]
pub async fn project_export_to(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    path: String,
) -> AppResult<()> {
    let text = project_export(state, session_id, database)?;
    tokio::fs::write(&path, text)
        .await
        .map_err(|e| AppError::Io(format!("Writing {path} failed: {e}")))
}

#[tauri::command]
pub async fn project_rules(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<ProjectRules> {
    let session = state.sessions.get(&session_id)?;
    overview::for_database(&session, &database, state.rules()).await
}

/// What a table's Structure view lists: per column its kinds, references and value styles, then
/// the table's meta-key rules, incoming references, entities and actions.
#[tauri::command]
pub async fn table_rules(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
) -> AppResult<TableProfile> {
    let session = state.sessions.get(&session_id)?;
    profile::for_table(&session, &database, &table, state.rules()).await
}

#[tauri::command]
pub async fn rule_draft(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    provenance: Provenance,
) -> AppResult<RuleDraft> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    draft::rule_draft(&session, &database, &provenance, state.rules(), timeout).await
}

/// The coverage of an unsaved `reference`; `replacing` is the project reference it would replace.
#[tauri::command]
pub async fn coverage_preview(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    reference: Reference,
    replacing: Option<String>,
) -> AppResult<Coverage> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    coverage::coverage_preview(
        &session,
        &database,
        reference,
        replacing,
        state.rules(),
        timeout,
    )
    .await
}

/// How many source rows of `reference` (as `table_references` returned it) find a target row.
#[tauri::command]
pub async fn reference_coverage(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    reference: ResolvedReference,
) -> AppResult<Coverage> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    coverage::reference_coverage(&session, &database, &reference, state.rules(), timeout).await
}
