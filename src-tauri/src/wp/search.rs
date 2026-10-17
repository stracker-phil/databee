//! Omnisearch for the command palette: IDs, emails, payment IDs, SKUs and names.

use serde::Serialize;
use ts_rs::TS;

use super::fetch;
use super::lists;
use super::rows::text;
use super::site::WpContext;
use super::sql::table_ref;
use crate::db::{Database, ResultSet};
use crate::error::AppResult;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub entity: String,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    /// What matched, e.g. "transaction_id" or "_ppcp_paypal_order_id".
    pub matched: String,
}

fn hits(rs: &ResultSet, entity: &str, matched: &str) -> Vec<SearchHit> {
    rs.rows
        .iter()
        .filter_map(|r| {
            Some(SearchHit {
                entity: entity.into(),
                id: text(&r[0])?,
                title: text(r.get(1)?)
                    .unwrap_or_else(|| format!("{entity} #{}", text(&r[0]).unwrap_or_default())),
                subtitle: r.get(2).and_then(text),
                matched: r.get(3).and_then(text).unwrap_or_else(|| matched.into()),
            })
        })
        .collect()
}

/// Finds anything pasted into ⌘K: IDs, emails, PayPal/transaction IDs, SKUs, names.
pub async fn search(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    query: &str,
) -> AppResult<Vec<SearchHit>> {
    let dialect = session.dialect();
    let q = query.trim().trim_start_matches('#');
    if q.len() < 2 {
        return Ok(Vec::new());
    }
    let exact = dialect.sql_string(q);
    let like = dialect.like_contains(q);
    let mut out = Vec::new();
    let posts = table_ref(dialect, db, ctx, "posts");
    let users = table_ref(dialect, db, ctx, "users");

    if let Ok(n) = q.parse::<u64>() {
        if ctx.hpos() {
            let rs = fetch(session, &format!("SELECT id, CONCAT('Order #', id), CONCAT(status, ' · ', COALESCE(billing_email, '')), 'id' FROM {} WHERE id = {n}", table_ref(dialect, db, ctx, "wc_orders"))).await?;
            out.extend(hits(&rs, "order", "id"));
        }
        let rs = fetch(session, &format!("SELECT ID, COALESCE(NULLIF(post_title, ''), CONCAT(post_type, ' #', ID)), post_type, 'id' FROM {posts} WHERE ID = {n}")).await?;
        for mut h in hits(&rs, "post", "id") {
            if !ctx.hpos()
                && h.subtitle
                    .as_deref()
                    .is_some_and(|s| s.starts_with("shop_order"))
            {
                h.entity = "order".into();
            }
            if !(ctx.hpos() && h.subtitle.as_deref() == Some("shop_order_placehold")) {
                out.push(h);
            }
        }
        let rs = fetch(
            session,
            &format!("SELECT ID, display_name, user_email, 'user id' FROM {users} WHERE ID = {n}"),
        )
        .await?;
        out.extend(hits(&rs, "user", "user id"));
        if ctx.has_table("actionscheduler_actions") {
            let rs = fetch(
                session,
                &format!(
                    "SELECT action_id, hook, status, 'action id' FROM {} WHERE action_id = {n}",
                    table_ref(dialect, db, ctx, "actionscheduler_actions")
                ),
            )
            .await?;
            out.extend(hits(&rs, "action", "action id"));
        }
    }

    if q.contains('@') {
        let rs = fetch(session, &format!("SELECT ID, display_name, user_email, 'email' FROM {users} WHERE user_email LIKE {like} LIMIT 8")).await?;
        out.extend(hits(&rs, "user", "email"));
        if ctx.woocommerce() {
            let guests = if ctx.hpos() {
                format!(
                    "SELECT CONCAT('{prefix}', billing_email), billing_email, CONCAT('Guest · ', COUNT(*), ' orders'), 'billing_email' \
                     FROM {orders} WHERE type = 'shop_order' AND customer_id = 0 AND billing_email LIKE {like} GROUP BY billing_email LIMIT 5",
                    prefix = lists::GUEST_PREFIX,
                    orders = table_ref(dialect, db, ctx, "wc_orders"),
                )
            } else {
                format!(
                    "SELECT CONCAT('{prefix}', e.meta_value), e.meta_value, CONCAT('Guest · ', COUNT(*), ' orders'), 'billing_email' \
                     FROM {posts} p JOIN {pm} e ON e.post_id = p.ID AND e.meta_key = '_billing_email' \
                     LEFT JOIN {pm} c ON c.post_id = p.ID AND c.meta_key = '_customer_user' \
                     WHERE p.post_type = 'shop_order' AND COALESCE(c.meta_value, '0') = '0' AND e.meta_value LIKE {like} GROUP BY e.meta_value LIMIT 5",
                    prefix = lists::GUEST_PREFIX,
                    posts = posts,
                    pm = table_ref(dialect, db, ctx, "postmeta"),
                )
            };
            let rs = fetch(session, &guests).await?;
            out.extend(hits(&rs, "user", "billing_email"));
        }
        if ctx.hpos() {
            let rs = fetch(
                session,
                &format!("SELECT id, CONCAT('Order #', id), CONCAT(status, ' · ', billing_email), 'billing_email' FROM {} WHERE billing_email LIKE {like} ORDER BY id DESC LIMIT 8", table_ref(dialect, db, ctx, "wc_orders")),
            )
            .await?;
            out.extend(hits(&rs, "order", "billing_email"));
        }
    }

    if q.len() >= 4 {
        // Exact identifiers stored anywhere in order/post/user meta (PayPal IDs, tokens, …).
        if ctx.hpos() {
            let rs = fetch(
                session,
                &format!("SELECT id, CONCAT('Order #', id), CONCAT(status, ' · ', COALESCE(billing_email, '')), 'transaction_id' FROM {} WHERE transaction_id = {exact} LIMIT 5", table_ref(dialect, db, ctx, "wc_orders")),
            )
            .await?;
            out.extend(hits(&rs, "order", "transaction_id"));
            let rs = fetch(
                session,
                &format!("SELECT order_id, CONCAT('Order #', order_id), NULL, meta_key FROM {} WHERE meta_value = {exact} LIMIT 10", table_ref(dialect, db, ctx, "wc_orders_meta")),
            )
            .await?;
            out.extend(hits(&rs, "order", "meta"));
        }
        let rs = fetch(
            session,
            &format!(
                "SELECT p.ID, COALESCE(NULLIF(p.post_title, ''), CONCAT(p.post_type, ' #', p.ID)), p.post_type, m.meta_key FROM {pm} m \
                 JOIN {posts} p ON p.ID = m.post_id WHERE m.meta_value = {exact} AND p.post_type <> 'shop_order_placehold' LIMIT 10",
                pm = table_ref(dialect, db, ctx, "postmeta")
            ),
        )
        .await?;
        for mut h in hits(&rs, "post", "meta") {
            if h.subtitle
                .as_deref()
                .is_some_and(|s| s.starts_with("shop_order") || s == "shop_subscription")
            {
                h.entity = "order".into();
            }
            out.push(h);
        }
        let rs = fetch(
            session,
            &format!(
                "SELECT u.ID, u.display_name, u.user_email, m.meta_key FROM {um} m JOIN {users} u ON u.ID = m.user_id WHERE m.meta_value = {exact} LIMIT 5",
                um = table_ref(dialect, db, ctx, "usermeta")
            ),
        )
        .await?;
        out.extend(hits(&rs, "user", "meta"));
    }

    // Names, titles and SKUs.
    let rs = fetch(
        session,
        &format!(
            "SELECT ID, post_title, post_type, 'title' FROM {posts} WHERE post_title LIKE {like} AND post_type IN ('product', 'post', 'page') \
             AND post_status NOT IN ('trash', 'auto-draft') ORDER BY post_type = 'product' DESC, ID DESC LIMIT 8"
        ),
    )
    .await?;
    out.extend(hits(&rs, "post", "title"));
    if ctx.has_table("wc_product_meta_lookup") {
        let rs = fetch(
            session,
            &format!(
                "SELECT p.ID, p.post_title, CONCAT('SKU ', l.sku), 'sku' FROM {} l JOIN {posts} p ON p.ID = l.product_id WHERE l.sku LIKE {like} LIMIT 8",
                table_ref(dialect, db, ctx, "wc_product_meta_lookup")
            ),
        )
        .await?;
        out.extend(hits(&rs, "post", "sku"));
    }
    let rs = fetch(
        session,
        &format!("SELECT ID, display_name, user_email, 'name' FROM {users} WHERE display_name LIKE {like} OR user_login LIKE {like} LIMIT 8"),
    )
    .await?;
    out.extend(hits(&rs, "user", "name"));
    let rs = fetch(session, &format!("SELECT option_id, option_name, autoload, 'option' FROM {} WHERE option_name LIKE {like} LIMIT 6", table_ref(dialect, db, ctx, "options"))).await?;
    out.extend(hits(&rs, "option", "option"));

    let mut seen = std::collections::HashSet::new();
    out.retain(|h| seen.insert((h.entity.clone(), h.id.clone())));
    out.truncate(40);
    Ok(out)
}
