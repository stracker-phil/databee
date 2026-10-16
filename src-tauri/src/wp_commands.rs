use tauri::State;

use crate::db::changes::Plan;
use crate::db::plans::PlannedChange;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::wp::cache::WpCache;
use crate::wp::changes::{self, WpChangeRequest};
use crate::wp::context::WpContext;
use crate::wp::dashboard::{self, BrowserSection, Dashboard, DayBucket};
use crate::wp::detail::{self, EntityView, SearchHit};
use crate::wp::entity::DocField;
use crate::wp::entity::{self, Catalog};
use crate::wp::lists::{
    self, ActionSummary, ListQuery, OrderSummary, Page, PostSummary, ProductSummary,
    SessionSummary, UserSummary,
};
use crate::wp::options::OptionsOverview;
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
    let session = state.sessions.get(session_id)?;
    let cache = session.ext.get_or_insert_with(WpCache::default);
    let writes = session.writes(database);
    if !refresh && let Some(cached) = cache.get(database, writes) {
        return Ok(Some(cached));
    }
    let ctx = lens::detect(session.db.as_ref(), database).await?;
    match &ctx {
        Some(c) => log::info!(
            "Detected WordPress in {database} (prefix {}, WooCommerce: {})",
            c.prefix,
            c.wc.as_ref()
                .map_or("no".into(), |wc| format!("{:?}", wc.hpos))
        ),
        None => log::debug!("{database} is not a WordPress database"),
    }
    match &ctx {
        Some(c) => cache.insert(database, c.clone(), writes),
        None => cache.remove(database),
    }
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
pub async fn wp_options(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<OptionsOverview> {
    let ctx = required_context(&state, &session_id, &database).await?;
    let session = state.sessions.get(&session_id)?;
    lens::options_overview(session.db.as_ref(), &ctx, &database).await
}

/// Shared prelude for WordPress-mode commands: context, catalog and session.
async fn wp_env(
    state: &AppState,
    session_id: &str,
    database: &str,
) -> AppResult<(WpContext, Catalog, std::sync::Arc<crate::session::Session>)> {
    let ctx = required_context(state, session_id, database).await?;
    let catalog = catalog(state, &ctx)?;
    Ok((ctx, catalog, state.sessions.get(session_id)?))
}

#[tauri::command]
pub async fn wp_dashboard(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Dashboard> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    dashboard::dashboard(s.db.as_ref(), &ctx, &database).await
}

#[tauri::command]
pub async fn wp_browser(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<BrowserSection>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    dashboard::browser(s.db.as_ref(), &ctx, &database).await
}

#[tauri::command]
pub async fn wp_orders(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    order_type: String,
    query: ListQuery,
) -> AppResult<Page<OrderSummary>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    let order_type = if order_type == "shop_subscription" {
        "shop_subscription"
    } else {
        "shop_order"
    };
    lists::orders(s.db.as_ref(), &ctx, &database, order_type, &query).await
}

#[tauri::command]
pub async fn wp_users(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<UserSummary>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    lists::users(
        s.db.as_ref(),
        &ctx,
        &database,
        &query,
        state.settings.get().gravatar,
    )
    .await
}

#[tauri::command]
pub async fn wp_products(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<ProductSummary>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    lists::products(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_actions(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<ActionSummary>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    lists::actions(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_failed_by_hook(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<lists::HookFailures>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    lists::failed_by_hook(s.db.as_ref(), &ctx, &database).await
}

#[tauri::command]
pub async fn wp_action_histogram(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    hook: Option<String>,
) -> AppResult<Vec<DayBucket>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    dashboard::action_histogram(s.db.as_ref(), &ctx, &database, hook.as_deref()).await
}

#[tauri::command]
pub async fn wp_sessions(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<SessionSummary>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    lists::sessions(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_content(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<PostSummary>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    lists::content(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_view(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    entity: String,
    id: String,
) -> AppResult<EntityView> {
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    detail::entity_view(
        s.db.as_ref(),
        &ctx,
        &catalog,
        &database,
        &entity,
        &id,
        state.settings.get().gravatar,
    )
    .await
}

#[tauri::command]
pub async fn wp_peek(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    entity: String,
    id: String,
) -> AppResult<Vec<crate::wp::peek::PeekFact>> {
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    crate::wp::peek::peek(s.db.as_ref(), &ctx, &catalog, &database, &entity, &id).await
}

#[tauri::command]
pub async fn wp_find_value(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    value: String,
) -> AppResult<Vec<crate::wp::peek::FindGroup>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    crate::wp::peek::find_value(s.db.as_ref(), &ctx, &database, &value).await
}

#[tauri::command]
pub async fn wp_search(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: String,
) -> AppResult<Vec<SearchHit>> {
    let (ctx, _, s) = wp_env(&state, &session_id, &database).await?;
    detail::search(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_options_group(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    prefix: String,
) -> AppResult<Vec<DocField>> {
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    lens::options_group(s.db.as_ref(), &ctx, &catalog, &database, &prefix).await
}

#[tauri::command]
pub async fn wp_order_logs(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    id: String,
) -> AppResult<Vec<detail::TimelineEvent>> {
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    detail::order_logs(s.db.as_ref(), &ctx, &catalog, &database, &id).await
}

#[tauri::command]
pub async fn wp_snapshot_take(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    scope: crate::wp::snapshot::Scope,
    label: String,
) -> AppResult<crate::store::snapshots::SnapshotInfo> {
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    let values =
        crate::wp::snapshot::capture(s.db.as_ref(), &ctx, &catalog, &database, &scope).await?;
    log::info!("Snapshot '{label}' with {} paths", values.len());
    state.snapshots.add(crate::store::snapshots::Snapshot {
        id: String::new(),
        label,
        account: s.spec.account(),
        database,
        scope,
        created: chrono::Utc::now(),
        values,
    })
}

#[tauri::command]
pub fn wp_snapshots(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<crate::store::snapshots::SnapshotInfo>> {
    let s = state.sessions.get(&session_id)?;
    Ok(state.snapshots.list(&s.spec.account(), &database))
}

#[tauri::command]
pub async fn wp_snapshot_diff(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    id: String,
) -> AppResult<Vec<crate::wp::snapshot::DiffEntry>> {
    let snapshot = state.snapshots.get(&id)?;
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    let now =
        crate::wp::snapshot::capture(s.db.as_ref(), &ctx, &catalog, &database, &snapshot.scope)
            .await?;
    Ok(crate::wp::snapshot::diff(&snapshot.values, &now))
}

#[tauri::command]
pub fn wp_snapshot_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.snapshots.delete(&id)
}

#[tauri::command]
pub async fn wp_compare(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    entity: String,
    a: String,
    b: String,
) -> AppResult<Vec<crate::wp::snapshot::CompareRow>> {
    let (ctx, catalog, s) = wp_env(&state, &session_id, &database).await?;
    crate::wp::snapshot::compare(s.db.as_ref(), &ctx, &catalog, &database, &entity, &a, &b).await
}

#[tauri::command]
pub async fn wp_plan_change(
    state: State<'_, AppState>,
    session_id: String,
    change: WpChangeRequest,
) -> AppResult<PlannedChange> {
    let session = state.sessions.get(&session_id)?;
    let plan = plan_wp(&state, &session_id, &change).await?;
    Ok(session.plans.add(plan))
}

async fn plan_wp(state: &AppState, session_id: &str, change: &WpChangeRequest) -> AppResult<Plan> {
    let (ctx, catalog, session) = wp_env(state, session_id, change.database()).await?;
    changes::plan(session.db.as_ref(), &ctx, &catalog, change).await
}
