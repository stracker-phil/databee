use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::wp::context::WpContext;
use crate::wp::entity::{self, Catalog, EntityDoc};
use crate::wp::options::{self, OptionsOverview};
use crate::wp::value::{self, DecodedValue};
use crate::wp::{self as lens, WpInfo};

fn catalog(state: &AppState, ctx: &WpContext) -> AppResult<Catalog> {
    let path = state.config_dir.join("entities.toml");
    let user = std::fs::read_to_string(&path).ok();
    let defs = entity::definitions(user.as_deref())
        .map_err(|e| AppError::InvalidInput(format!("{}: {e}", path.display())))?;
    Ok(Catalog::new(&defs, ctx))
}

async fn context(
    state: &AppState,
    session_id: &str,
    database: &str,
    refresh: bool,
) -> AppResult<Option<WpContext>> {
    let key = format!("{session_id}/{database}");
    if !refresh && let Some(cached) = state.wp_cache.lock().unwrap().get(&key) {
        return Ok(cached.clone());
    }
    let session = state.sessions.get(session_id)?;
    let ctx = lens::detect(&session.db, database).await?;
    match &ctx {
        Some(c) => log::info!(
            "Detected WordPress in {database} (prefix {}, WooCommerce: {})",
            c.prefix,
            c.wc.as_ref()
                .map_or("no".into(), |wc| format!("{:?}", wc.hpos))
        ),
        None => log::debug!("{database} is not a WordPress database"),
    }
    state.wp_cache.lock().unwrap().insert(key, ctx.clone());
    Ok(ctx)
}

async fn required_context(
    state: &AppState,
    session_id: &str,
    database: &str,
) -> AppResult<WpContext> {
    context(state, session_id, database, false)
        .await?
        .ok_or_else(|| AppError::InvalidInput(format!("{database} is not a WordPress database")))
}

#[tauri::command]
pub async fn wp_detect(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    refresh: bool,
) -> AppResult<Option<WpInfo>> {
    let Some(ctx) = context(&state, &session_id, &database, refresh).await? else {
        return Ok(None);
    };
    let catalog = catalog(&state, &ctx)?;
    Ok(Some(WpInfo::new(ctx, &catalog)))
}

#[tauri::command]
pub async fn wp_load(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    entity: String,
    ids: Vec<String>,
) -> AppResult<Vec<EntityDoc>> {
    let ctx = required_context(&state, &session_id, &database).await?;
    let catalog = catalog(&state, &ctx)?;
    let session = state.sessions.get(&session_id)?;
    lens::load_docs(&session.db, &ctx, &catalog, &database, &entity, &ids).await
}

#[tauri::command]
pub async fn wp_expand(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    entity: String,
    id: String,
    relation: String,
) -> AppResult<Vec<EntityDoc>> {
    let ctx = required_context(&state, &session_id, &database).await?;
    let catalog = catalog(&state, &ctx)?;
    let session = state.sessions.get(&session_id)?;
    lens::expand(
        &session.db,
        &ctx,
        &catalog,
        &database,
        &entity,
        &id,
        &relation,
    )
    .await
}

#[tauri::command]
pub async fn wp_options(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<OptionsOverview> {
    let ctx = required_context(&state, &session_id, &database).await?;
    let session = state.sessions.get(&session_id)?;
    lens::options_overview(&session.db, &ctx, &database).await
}

#[tauri::command]
pub async fn wp_transient_cleanup_sql(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<String> {
    let overview = wp_options(state, session_id, database.clone()).await?;
    Ok(options::cleanup_sql(
        &database,
        &overview.table,
        &overview.transients,
    ))
}

#[tauri::command]
pub fn decode_value(text: String) -> Option<DecodedValue> {
    value::decode(&text)
}
