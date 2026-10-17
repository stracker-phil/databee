//! WordPress mode's Tauri commands, registered by the `wp` plugin (`wp::init`).

use std::sync::Arc;

use tauri::State;

use super::catalog::Catalog;
use super::changes::{self, WpChangeRequest};
use super::dashboard::{self, ActionHistogram, BrowserSection, Dashboard};
use super::document::DocField;
use super::lists::{
    self, ActionSummary, ListQuery, OrderSummary, OrderType, Page, PostSummary, ProductSummary,
    SessionSummary, UserSummary,
};
use super::options::OptionsOverview;
use super::site::WpContext;
use super::snapshot::{self, DiffEntry, Scope};
use super::snapshots_store::SnapshotInfo;
use super::{self as lens, WpInfo, WpState};
use crate::db::plans::PlannedChange;
use crate::error::{AppError, AppResult};
use crate::rules::effective::{self, RulesInfo};
use crate::schema;
use crate::session::Session;
use crate::state::AppState;

/// What every WordPress-mode command needs: the database's rules as a context, the entity
/// catalog read from them and the session.
struct WpEnv {
    ctx: WpContext,
    catalog: Catalog,
    session: Arc<Session>,
}

/// The rules of `database` as the WordPress screens read them; `None` unless the `wordpress`
/// preset applies.
async fn load(
    state: &AppState,
    session: &Session,
    database: &str,
) -> AppResult<Option<(WpContext, Catalog)>> {
    let schema = schema::database(session, database).await?;
    let rules = effective::effective(session, database, &schema, state.rules());
    let catalog = Catalog::new(&rules, &schema);
    Ok(WpContext::new(rules).map(|ctx| (ctx, catalog)))
}

async fn wp_env(state: &AppState, session_id: &str, database: &str) -> AppResult<WpEnv> {
    let session = state.sessions.get(session_id)?;
    let (ctx, catalog) = load(state, &session, database)
        .await?
        .ok_or_else(|| AppError::InvalidInput(format!("{database} is not a WordPress database")))?;
    Ok(WpEnv {
        ctx,
        catalog,
        session,
    })
}

/// `rules_info` in the old `WpInfo` shape; `None` unless the `wordpress` preset applies.
#[tauri::command]
pub async fn wp_detect(
    state: State<'_, AppState>,
    wp: State<'_, WpState>,
    session_id: String,
    database: String,
    refresh: bool,
) -> AppResult<Option<WpInfo>> {
    let session = state.sessions.get(&session_id)?;
    if refresh {
        schema::refresh(&session);
    }
    let Some((ctx, catalog)) = load(&state, &session, &database).await? else {
        return Ok(None);
    };
    let loaded = effective::loaded_presets(&state.presets.entries());
    let info = RulesInfo::new(&ctx.rules, &loaded);
    Ok(Some(WpInfo::new(
        info,
        &catalog,
        wp.legacy_entities.as_deref(),
    )))
}

#[tauri::command]
pub async fn wp_options(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<OptionsOverview> {
    let env = wp_env(&state, &session_id, &database).await?;
    lens::options_overview(env.session.db.as_ref(), &env.ctx, &database).await
}

#[tauri::command]
pub async fn wp_dashboard(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Dashboard> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    dashboard::dashboard(s.db.as_ref(), &ctx, &database).await
}

#[tauri::command]
pub async fn wp_browser(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<BrowserSection>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    dashboard::browser(s.db.as_ref(), &ctx, &database).await
}

#[tauri::command]
pub async fn wp_orders(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    order_type: OrderType,
    query: ListQuery,
) -> AppResult<Page<OrderSummary>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    lists::orders(
        s.db.as_ref(),
        &ctx,
        &database,
        order_type.post_type(),
        &query,
    )
    .await
}

#[tauri::command]
pub async fn wp_users(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<UserSummary>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
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
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    lists::products(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_actions(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<ActionSummary>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    lists::actions(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_failed_by_hook(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<lists::HookFailures>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    lists::failed_by_hook(s.db.as_ref(), &ctx, &database).await
}

#[tauri::command]
pub async fn wp_action_histogram(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    hook: Option<String>,
) -> AppResult<ActionHistogram> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    dashboard::action_histogram(s.db.as_ref(), &ctx, &database, hook.as_deref()).await
}

#[tauri::command]
pub async fn wp_sessions(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<SessionSummary>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    lists::sessions(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_content(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    query: ListQuery,
) -> AppResult<Page<PostSummary>> {
    let WpEnv {
        ctx, session: s, ..
    } = wp_env(&state, &session_id, &database).await?;
    lists::content(s.db.as_ref(), &ctx, &database, &query).await
}

#[tauri::command]
pub async fn wp_options_group(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    prefix: String,
) -> AppResult<Vec<DocField>> {
    let WpEnv {
        ctx,
        catalog,
        session: s,
    } = wp_env(&state, &session_id, &database).await?;
    lens::options_group(s.db.as_ref(), &ctx, &catalog, &database, &prefix).await
}

#[tauri::command]
pub async fn wp_snapshot_take(
    state: State<'_, AppState>,
    wp: State<'_, WpState>,
    session_id: String,
    database: String,
    scope: Scope,
    label: String,
) -> AppResult<SnapshotInfo> {
    let WpEnv {
        ctx,
        catalog,
        session: s,
    } = wp_env(&state, &session_id, &database).await?;
    let values = snapshot::capture(s.db.as_ref(), &ctx, &catalog, &database, &scope).await?;
    wp.snapshots
        .take(label, s.spec.account(), database, scope, values)
        .await
}

#[tauri::command]
pub fn wp_snapshots(
    state: State<'_, AppState>,
    wp: State<'_, WpState>,
    session_id: String,
    database: String,
) -> AppResult<Vec<SnapshotInfo>> {
    let s = state.sessions.get(&session_id)?;
    Ok(wp.snapshots.list(&s.spec.account(), &database))
}

#[tauri::command]
pub async fn wp_snapshot_diff(
    state: State<'_, AppState>,
    wp: State<'_, WpState>,
    session_id: String,
    database: String,
    id: String,
) -> AppResult<Vec<DiffEntry>> {
    let taken = wp.snapshots.get(&id).await?;
    let WpEnv {
        ctx,
        catalog,
        session: s,
    } = wp_env(&state, &session_id, &database).await?;
    snapshot::diff_against_now(s.db.as_ref(), &ctx, &catalog, &database, &taken).await
}

#[tauri::command]
pub async fn wp_snapshot_delete(wp: State<'_, WpState>, id: String) -> AppResult<()> {
    wp.snapshots.delete(&id).await
}

#[tauri::command]
pub async fn wp_plan_change(
    state: State<'_, AppState>,
    session_id: String,
    change: WpChangeRequest,
) -> AppResult<PlannedChange> {
    let env = wp_env(&state, &session_id, change.database()).await?;
    let plan = changes::plan(env.session.db.as_ref(), &env.ctx, &change).await?;
    Ok(env.session.plans.add(plan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_types_deserialize_from_their_post_types() {
        for (wire, order_type) in [
            ("shop_order", OrderType::ShopOrder),
            ("shop_subscription", OrderType::ShopSubscription),
        ] {
            let parsed: OrderType = serde_json::from_value(serde_json::json!(wire)).unwrap();

            assert_eq!(parsed, order_type, "{wire}");
            assert_eq!(parsed.post_type(), wire);
        }
    }

    #[test]
    fn unknown_order_type_fails_to_deserialize() {
        for wire in ["shop_refund", "shopOrder", ""] {
            let parsed = serde_json::from_value::<OrderType>(serde_json::json!(wire));

            assert!(parsed.is_err(), "{wire:?}");
        }
    }
}
