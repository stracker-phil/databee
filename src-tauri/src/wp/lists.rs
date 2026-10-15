//! List screens of the WordPress mode: one query per page (plus one batched query for
//! per-row extras), filters and status counts. SQL builders are pure and unit-tested.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::context::WpContext;
use super::entity::cell_str;
use super::hints::money;
use super::{fetch, post_images, sql_string};
use crate::db::mysql::MySqlSession;
use crate::db::{Cell, ResultSet, quote_ident};
use crate::error::AppResult;

pub const PAGE_MAX: u32 = 200;

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ListQuery {
    pub status: Option<String>,
    /// Orders: gateway id; products: product type; customers: role; content: post type.
    pub kind: Option<String>,
    pub search: Option<String>,
    pub offset: u64,
    pub limit: u32,
}

impl ListQuery {
    fn limit(&self) -> u32 {
        if self.limit == 0 {
            50
        } else {
            self.limit.min(PAGE_MAX)
        }
    }
    fn search(&self) -> Option<&str> {
        self.search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Facet {
    pub value: String,
    pub label: String,
    pub count: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
    /// Status (or kind) counts for filter chips, independent of the current filter.
    pub facets: Vec<Facet>,
    pub kinds: Vec<Facet>,
}

pub(crate) fn text(cell: &Cell) -> Option<String> {
    cell_str(cell).map(String::from).filter(|s| !s.is_empty())
}

pub(crate) fn num(cell: &Cell) -> Option<u64> {
    cell_str(cell)?.trim().parse().ok()
}

pub(crate) fn int(cell: &Cell) -> Option<i64> {
    cell_str(cell)?.trim().parse::<f64>().ok().map(|f| f as i64)
}

fn like(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    sql_string(&format!("%{escaped}%"))
}

/// Seconds since epoch of a UTC DATETIME column, independent of the session time zone.
pub(crate) fn epoch(col: &str) -> String {
    format!("TIMESTAMPDIFF(SECOND, '1970-01-01 00:00:00', {col})")
}

fn page_split<T>(mut items: Vec<T>, limit: u32) -> (Vec<T>, bool) {
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);
    (items, has_more)
}

fn facets(rs: &ResultSet, label: impl Fn(&str) -> String) -> Vec<Facet> {
    let mut out: Vec<Facet> = rs
        .rows
        .iter()
        .filter_map(|r| {
            let value = text(&r[0])?;
            Some(Facet {
                label: label(&value),
                count: num(&r[1]).unwrap_or(0),
                value,
            })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.count));
    out
}

/// `wc-processing` → `Processing`.
pub fn status_label(status: &str) -> String {
    let s = status.trim_start_matches("wc-").replace(['-', '_'], " ");
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn t(db: &str, ctx: &WpContext, table: &str) -> String {
    format!("{}.{}", quote_ident(db), quote_ident(&ctx.table(table)))
}

// ---------- orders & subscriptions ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderSummary {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub total: Option<String>,
    pub gateway: Option<String>,
    pub gateway_title: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub customer_id: Option<String>,
    pub created: Option<i64>,
    pub item_count: u64,
}

const ORDER_COLS: usize = 11;

pub fn orders_sql(db: &str, ctx: &WpContext, order_type: &str, q: &ListQuery) -> String {
    let limit = q.limit() + 1;
    if ctx.hpos() {
        let mut filter = format!("o.type = {}", sql_string(order_type));
        if let Some(s) = &q.status {
            filter += &format!(" AND o.status = {}", sql_string(s));
        }
        if let Some(g) = &q.kind {
            filter += &format!(" AND o.payment_method = {}", sql_string(g));
        }
        if let Some(s) = q.search() {
            let id = s.trim_start_matches('#');
            let id_match = id
                .parse::<u64>()
                .map(|n| format!("o.id = {n} OR "))
                .unwrap_or_default();
            filter += &format!(
                " AND ({id_match}o.billing_email LIKE {l} OR o.transaction_id = {e} OR a.first_name LIKE {l} OR a.last_name LIKE {l})",
                l = like(s),
                e = sql_string(s)
            );
        }
        format!(
            "SELECT o.id, o.type, o.status, o.total_amount, o.currency, o.payment_method, o.payment_method_title, \
             o.billing_email, TRIM(CONCAT(COALESCE(a.first_name, ''), ' ', COALESCE(a.last_name, ''))), o.customer_id, {created}, \
             (SELECT COUNT(*) FROM {items} i WHERE i.order_id = o.id AND i.order_item_type = 'line_item') \
             FROM {orders} o LEFT JOIN {addr} a ON a.order_id = o.id AND a.address_type = 'billing' \
             WHERE {filter} ORDER BY o.date_created_gmt DESC, o.id DESC LIMIT {limit} OFFSET {offset}",
            created = epoch("o.date_created_gmt"),
            items = t(db, ctx, "woocommerce_order_items"),
            orders = t(db, ctx, "wc_orders"),
            addr = t(db, ctx, "wc_order_addresses"),
            offset = q.offset,
        )
    } else {
        let meta = |key: &str| format!("MAX(CASE WHEN m.meta_key = '{key}' THEN m.meta_value END)");
        let mut filter = format!("p.post_type = {}", sql_string(order_type));
        if let Some(s) = &q.status {
            filter += &format!(" AND p.post_status = {}", sql_string(s));
        }
        let mut having = Vec::new();
        if let Some(g) = &q.kind {
            having.push(format!("{} = {}", meta("_payment_method"), sql_string(g)));
        }
        if let Some(s) = q.search() {
            let id = s.trim_start_matches('#');
            let id_match = id
                .parse::<u64>()
                .map(|n| format!("p.ID = {n} OR "))
                .unwrap_or_default();
            having.push(format!(
                "({id_match}{email} LIKE {l} OR {tx} = {e} OR {first} LIKE {l} OR {last} LIKE {l})",
                email = meta("_billing_email"),
                tx = meta("_transaction_id"),
                first = meta("_billing_first_name"),
                last = meta("_billing_last_name"),
                l = like(s),
                e = sql_string(s)
            ));
        }
        let having = if having.is_empty() {
            String::new()
        } else {
            format!(" HAVING {}", having.join(" AND "))
        };
        format!(
            "SELECT p.ID, p.post_type, p.post_status, {total}, {currency}, {gw}, {gwt}, {email}, \
             TRIM(CONCAT(COALESCE({first}, ''), ' ', COALESCE({last}, ''))), {customer}, {created}, \
             (SELECT COUNT(*) FROM {items} i WHERE i.order_id = p.ID AND i.order_item_type = 'line_item') \
             FROM {posts} p LEFT JOIN {postmeta} m ON m.post_id = p.ID AND m.meta_key IN \
             ('_order_total', '_order_currency', '_payment_method', '_payment_method_title', '_billing_email', \
             '_billing_first_name', '_billing_last_name', '_customer_user', '_transaction_id') \
             WHERE {filter} GROUP BY p.ID{having} ORDER BY p.post_date_gmt DESC, p.ID DESC LIMIT {limit} OFFSET {offset}",
            total = meta("_order_total"),
            currency = meta("_order_currency"),
            gw = meta("_payment_method"),
            gwt = meta("_payment_method_title"),
            email = meta("_billing_email"),
            first = meta("_billing_first_name"),
            last = meta("_billing_last_name"),
            customer = meta("_customer_user"),
            created = epoch("p.post_date_gmt"),
            items = t(db, ctx, "woocommerce_order_items"),
            posts = t(db, ctx, "posts"),
            postmeta = t(db, ctx, "postmeta"),
            offset = q.offset,
        )
    }
}

pub fn order_rows(rs: &ResultSet, fallback_currency: Option<&str>) -> Vec<OrderSummary> {
    rs.rows
        .iter()
        .filter(|r| r.len() > ORDER_COLS)
        .map(|r| {
            let currency = text(&r[4]).or_else(|| fallback_currency.map(String::from));
            OrderSummary {
                id: text(&r[0]).unwrap_or_default(),
                kind: text(&r[1]).unwrap_or_default(),
                status: text(&r[2]).unwrap_or_default(),
                total: text(&r[3]).and_then(|v| money(&v, currency.as_deref())),
                gateway: text(&r[5]),
                gateway_title: text(&r[6]),
                email: text(&r[7]),
                name: text(&r[8]),
                customer_id: text(&r[9]).filter(|c| c != "0"),
                created: int(&r[10]),
                item_count: num(&r[11]).unwrap_or(0),
            }
        })
        .collect()
}

fn order_facets_sql(db: &str, ctx: &WpContext, order_type: &str) -> (String, String) {
    if ctx.hpos() {
        let from = format!(
            "{} WHERE type = {}",
            t(db, ctx, "wc_orders"),
            sql_string(order_type)
        );
        (
            format!("SELECT status, COUNT(*) FROM {from} GROUP BY status"),
            format!(
                "SELECT payment_method, COUNT(*), MAX(payment_method_title) FROM {from} AND payment_method <> '' GROUP BY payment_method"
            ),
        )
    } else {
        (
            format!(
                "SELECT post_status, COUNT(*) FROM {} WHERE post_type = {} GROUP BY post_status",
                t(db, ctx, "posts"),
                sql_string(order_type)
            ),
            format!(
                "SELECT m.meta_value, COUNT(*), MAX(t.meta_value) FROM {posts} p JOIN {pm} m ON m.post_id = p.ID AND m.meta_key = '_payment_method' \
                 LEFT JOIN {pm} t ON t.post_id = p.ID AND t.meta_key = '_payment_method_title' \
                 WHERE p.post_type = {ty} AND m.meta_value <> '' GROUP BY m.meta_value",
                posts = t(db, ctx, "posts"),
                pm = t(db, ctx, "postmeta"),
                ty = sql_string(order_type)
            ),
        )
    }
}

pub async fn orders(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    order_type: &str,
    q: &ListQuery,
) -> AppResult<Page<OrderSummary>> {
    let rs = fetch(session, &orders_sql(db, ctx, order_type, q)).await?;
    let (items, has_more) = page_split(order_rows(&rs, ctx.site.currency.as_deref()), q.limit());
    let (status_sql, gateway_sql) = order_facets_sql(db, ctx, order_type);
    let statuses = fetch(session, &status_sql).await?;
    let gateways = fetch(session, &gateway_sql).await?;
    let gateway_titles: HashMap<String, String> = gateways
        .rows
        .iter()
        .filter_map(|r| Some((text(&r[0])?, text(&r[2])?)))
        .collect();
    Ok(Page {
        items,
        has_more,
        facets: facets(&statuses, status_label),
        kinds: facets(&gateways, |g| {
            gateway_titles
                .get(g)
                .cloned()
                .unwrap_or_else(|| g.to_string())
        }),
    })
}

// ---------- customers ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomerSummary {
    pub id: String,
    pub login: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub registered: Option<i64>,
    pub roles: Vec<String>,
    pub order_count: u64,
    pub total_spent: Option<String>,
    pub last_order: Option<i64>,
    pub avatar: Option<String>,
}

pub fn customers_sql(db: &str, ctx: &WpContext, q: &ListQuery) -> String {
    let caps = sql_string(&format!("{}capabilities", ctx.prefix));
    let mut filter = "1 = 1".to_string();
    if let Some(role) = &q.kind {
        filter += &format!(" AND c.meta_value LIKE {}", like(&format!("\"{role}\"")));
    }
    if let Some(s) = q.search() {
        let id_match = s
            .parse::<u64>()
            .map(|n| format!("u.ID = {n} OR "))
            .unwrap_or_default();
        filter += &format!(
            " AND ({id_match}u.user_login LIKE {l} OR u.user_email LIKE {l} OR u.display_name LIKE {l})",
            l = like(s)
        );
    }
    format!(
        "SELECT u.ID, u.user_login, u.user_email, u.display_name, {registered}, c.meta_value \
         FROM {users} u LEFT JOIN {usermeta} c ON c.user_id = u.ID AND c.meta_key = {caps} \
         WHERE {filter} ORDER BY u.user_registered DESC, u.ID DESC LIMIT {limit} OFFSET {offset}",
        registered = epoch("u.user_registered"),
        users = t(db, ctx, "users"),
        usermeta = t(db, ctx, "usermeta"),
        limit = q.limit() + 1,
        offset = q.offset,
    )
}

/// (count, sum of totals, last order) per customer for paid-ish orders.
pub fn customer_stats_sql(db: &str, ctx: &WpContext, ids: &[String]) -> Option<String> {
    let list = ids
        .iter()
        .filter_map(|i| i.parse::<u64>().ok())
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if list.is_empty() || ctx.wc.is_none() {
        return None;
    }
    Some(if ctx.hpos() {
        format!(
            "SELECT customer_id, COUNT(*), SUM(CASE WHEN status IN ('wc-completed', 'wc-processing') THEN total_amount ELSE 0 END), \
             MAX({created}) FROM {orders} WHERE type = 'shop_order' AND customer_id IN ({list}) GROUP BY customer_id",
            created = epoch("date_created_gmt"),
            orders = t(db, ctx, "wc_orders"),
        )
    } else {
        format!(
            "SELECT c.meta_value, COUNT(*), SUM(CASE WHEN p.post_status IN ('wc-completed', 'wc-processing') THEN tot.meta_value ELSE 0 END), \
             MAX({created}) FROM {posts} p JOIN {pm} c ON c.post_id = p.ID AND c.meta_key = '_customer_user' \
             LEFT JOIN {pm} tot ON tot.post_id = p.ID AND tot.meta_key = '_order_total' \
             WHERE p.post_type = 'shop_order' AND c.meta_value IN ({list}) GROUP BY c.meta_value",
            created = epoch("p.post_date_gmt"),
            posts = t(db, ctx, "posts"),
            pm = t(db, ctx, "postmeta"),
        )
    })
}

pub fn roles_of(capabilities: Option<&str>) -> Vec<String> {
    match capabilities
        .map(|c| super::hints::annotate("capabilities", Some(c), &Default::default()).hint)
    {
        Some(Some(super::hints::Hint::Roles { roles })) => roles,
        _ => Vec::new(),
    }
}

/// Gravatar URL (SHA-256 of the trimmed, lower-cased email). Only used when enabled.
pub fn gravatar(email: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(email.trim().to_lowercase().as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("https://gravatar.com/avatar/{hex}?s=96&d=404")
}

pub async fn customers(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
    with_gravatar: bool,
) -> AppResult<Page<CustomerSummary>> {
    let rs = fetch(session, &customers_sql(db, ctx, q)).await?;
    let rows: Vec<CustomerSummary> = rs
        .rows
        .iter()
        .map(|r| {
            let email = text(&r[2]);
            CustomerSummary {
                id: text(&r[0]).unwrap_or_default(),
                login: text(&r[1]).unwrap_or_default(),
                avatar: email.as_deref().filter(|_| with_gravatar).map(gravatar),
                email,
                name: text(&r[3]),
                registered: int(&r[4]),
                roles: roles_of(text(&r[5]).as_deref()),
                order_count: 0,
                total_spent: None,
                last_order: None,
            }
        })
        .collect();
    let (mut items, has_more) = page_split(rows, q.limit());
    let ids: Vec<String> = items.iter().map(|c| c.id.clone()).collect();
    if let Some(sql) = customer_stats_sql(db, ctx, &ids) {
        let stats = fetch(session, &sql).await?;
        let by_id: HashMap<String, &Vec<Cell>> = stats
            .rows
            .iter()
            .filter_map(|r| Some((text(&r[0])?, r)))
            .collect();
        for c in &mut items {
            if let Some(r) = by_id.get(&c.id) {
                c.order_count = num(&r[1]).unwrap_or(0);
                c.total_spent = text(&r[2]).and_then(|v| money(&v, ctx.site.currency.as_deref()));
                c.last_order = int(&r[3]);
            }
        }
    }
    let roles = fetch(
        session,
        &format!(
            "SELECT meta_value, COUNT(*) FROM {} WHERE meta_key = {} GROUP BY meta_value",
            t(db, ctx, "usermeta"),
            sql_string(&format!("{}capabilities", ctx.prefix))
        ),
    )
    .await?;
    let mut role_counts: HashMap<String, u64> = HashMap::new();
    for r in &roles.rows {
        for role in roles_of(text(&r[0]).as_deref()) {
            *role_counts.entry(role).or_default() += num(&r[1]).unwrap_or(0);
        }
    }
    let mut kinds: Vec<Facet> = role_counts
        .into_iter()
        .map(|(value, count)| Facet {
            label: status_label(&value),
            value,
            count,
        })
        .collect();
    kinds.sort_by_key(|a| std::cmp::Reverse(a.count));
    Ok(Page {
        items,
        has_more,
        facets: Vec::new(),
        kinds,
    })
}

// ---------- products ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub kind: Option<String>,
    pub sku: Option<String>,
    pub price: Option<String>,
    pub stock_status: Option<String>,
    pub stock: Option<String>,
    pub total_sales: u64,
    pub variations: u64,
    pub image: Option<String>,
}

pub fn products_sql(db: &str, ctx: &WpContext, q: &ListQuery) -> String {
    let lookup = ctx.has_table("wc_product_meta_lookup");
    let mut filter =
        "p.post_type = 'product' AND p.post_status NOT IN ('trash', 'auto-draft')".to_string();
    if let Some(s) = &q.status {
        filter += &format!(" AND p.post_status = {}", sql_string(s));
    }
    if let Some(k) = &q.kind {
        filter += &format!(" AND pt.slug = {}", sql_string(k));
    }
    if let Some(s) = q.search() {
        let id_match = s
            .parse::<u64>()
            .map(|n| format!("p.ID = {n} OR "))
            .unwrap_or_default();
        let sku = if lookup {
            format!(" OR l.sku LIKE {}", like(s))
        } else {
            String::new()
        };
        filter += &format!(" AND ({id_match}p.post_title LIKE {l}{sku})", l = like(s));
    }
    let (cols, join) = if lookup {
        (
            "l.sku, CASE WHEN l.min_price = l.max_price THEN l.min_price ELSE CONCAT(l.min_price, '–', l.max_price) END, l.stock_status, l.stock_quantity, l.total_sales",
            format!(
                " LEFT JOIN {} l ON l.product_id = p.ID",
                t(db, ctx, "wc_product_meta_lookup")
            ),
        )
    } else {
        ("NULL, NULL, NULL, NULL, 0", String::new())
    };
    format!(
        "SELECT p.ID, p.post_title, p.post_status, pt.slug, {cols}, \
         (SELECT COUNT(*) FROM {posts} v WHERE v.post_parent = p.ID AND v.post_type = 'product_variation') \
         FROM {posts} p{join} \
         LEFT JOIN (SELECT tr.object_id, te.slug FROM {tr} tr JOIN {tt} tt ON tt.term_taxonomy_id = tr.term_taxonomy_id AND tt.taxonomy = 'product_type' \
         JOIN {terms} te ON te.term_id = tt.term_id) pt ON pt.object_id = p.ID \
         WHERE {filter} ORDER BY p.post_date_gmt DESC, p.ID DESC LIMIT {limit} OFFSET {offset}",
        posts = t(db, ctx, "posts"),
        tr = t(db, ctx, "term_relationships"),
        tt = t(db, ctx, "term_taxonomy"),
        terms = t(db, ctx, "terms"),
        limit = q.limit() + 1,
        offset = q.offset,
    )
}

fn price_text(raw: &str, currency: Option<&str>) -> String {
    match raw.split_once('–') {
        Some((a, b)) => format!(
            "{}–{}",
            money(a, currency).unwrap_or_else(|| a.into()),
            money(b, currency).unwrap_or_else(|| b.into())
        ),
        None => money(raw, currency).unwrap_or_else(|| raw.into()),
    }
}

pub async fn products(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<ProductSummary>> {
    let rs = fetch(session, &products_sql(db, ctx, q)).await?;
    let currency = ctx.site.currency.as_deref();
    let rows: Vec<ProductSummary> = rs
        .rows
        .iter()
        .map(|r| ProductSummary {
            id: text(&r[0]).unwrap_or_default(),
            name: text(&r[1]).unwrap_or_else(|| "(no title)".into()),
            status: text(&r[2]).unwrap_or_default(),
            kind: text(&r[3]),
            sku: text(&r[4]),
            price: text(&r[5]).map(|p| price_text(&p, currency)),
            stock_status: text(&r[6]),
            stock: text(&r[7]),
            total_sales: num(&r[8]).unwrap_or(0),
            variations: num(&r[9]).unwrap_or(0),
            image: None,
        })
        .collect();
    let (mut items, has_more) = page_split(rows, q.limit());
    let images = post_images(
        session,
        ctx,
        db,
        &items.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
    )
    .await?;
    for p in &mut items {
        p.image = images.get(&p.id).cloned();
    }
    let statuses = fetch(
        session,
        &format!(
            "SELECT post_status, COUNT(*) FROM {} WHERE post_type = 'product' AND post_status NOT IN ('trash', 'auto-draft') GROUP BY post_status",
            t(db, ctx, "posts")
        ),
    )
    .await?;
    let kinds = fetch(
        session,
        &format!(
            "SELECT te.slug, COUNT(*) FROM {tr} tr JOIN {tt} tt ON tt.term_taxonomy_id = tr.term_taxonomy_id AND tt.taxonomy = 'product_type' \
             JOIN {terms} te ON te.term_id = tt.term_id GROUP BY te.slug",
            tr = t(db, ctx, "term_relationships"),
            tt = t(db, ctx, "term_taxonomy"),
            terms = t(db, ctx, "terms"),
        ),
    )
    .await?;
    Ok(Page {
        items,
        has_more,
        facets: facets(&statuses, status_label),
        kinds: facets(&kinds, status_label),
    })
}

// ---------- scheduled actions ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionSummary {
    pub id: String,
    pub hook: String,
    pub status: String,
    pub scheduled: Option<i64>,
    pub last_attempt: Option<i64>,
    pub attempts: u64,
    pub args: Option<String>,
    pub group: Option<String>,
    /// Last log message (failure reason for failed actions).
    pub message: Option<String>,
}

pub fn actions_sql(db: &str, ctx: &WpContext, q: &ListQuery) -> String {
    let mut filter = "1 = 1".to_string();
    if let Some(s) = &q.status {
        filter += &format!(" AND a.status = {}", sql_string(s));
    }
    if let Some(h) = &q.kind {
        filter += &format!(" AND a.hook = {}", sql_string(h));
    }
    if let Some(s) = q.search() {
        let id_match = s
            .parse::<u64>()
            .map(|n| format!("a.action_id = {n} OR "))
            .unwrap_or_default();
        filter += &format!(
            " AND ({id_match}a.hook LIKE {l} OR a.args LIKE {l})",
            l = like(s)
        );
    }
    format!(
        "SELECT a.action_id, a.hook, a.status, {sched}, {last}, a.attempts, a.args, g.slug, \
         (SELECT l.message FROM {logs} l WHERE l.action_id = a.action_id ORDER BY l.log_id DESC LIMIT 1) \
         FROM {actions} a LEFT JOIN {groups} g ON g.group_id = a.group_id \
         WHERE {filter} ORDER BY a.scheduled_date_gmt DESC, a.action_id DESC LIMIT {limit} OFFSET {offset}",
        sched = epoch("a.scheduled_date_gmt"),
        last = epoch("IF(a.last_attempt_gmt < '1000-01-01', NULL, a.last_attempt_gmt)"),
        logs = t(db, ctx, "actionscheduler_logs"),
        actions = t(db, ctx, "actionscheduler_actions"),
        groups = t(db, ctx, "actionscheduler_groups"),
        limit = q.limit() + 1,
        offset = q.offset,
    )
}

pub fn action_rows(rs: &ResultSet) -> Vec<ActionSummary> {
    rs.rows
        .iter()
        .map(|r| ActionSummary {
            id: text(&r[0]).unwrap_or_default(),
            hook: text(&r[1]).unwrap_or_default(),
            status: text(&r[2]).unwrap_or_default(),
            scheduled: int(&r[3]),
            last_attempt: int(&r[4]),
            attempts: num(&r[5]).unwrap_or(0),
            args: text(&r[6]).filter(|a| a != "[]"),
            group: text(&r[7]),
            message: text(&r[8]),
        })
        .collect()
}

pub async fn actions(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<ActionSummary>> {
    let rs = fetch(session, &actions_sql(db, ctx, q)).await?;
    let (items, has_more) = page_split(action_rows(&rs), q.limit());
    let table = t(db, ctx, "actionscheduler_actions");
    let statuses = fetch(
        session,
        &format!("SELECT status, COUNT(*) FROM {table} GROUP BY status"),
    )
    .await?;
    let status_filter = q
        .status
        .as_ref()
        .map(|s| format!(" WHERE status = {}", sql_string(s)))
        .unwrap_or_default();
    let hooks = fetch(
        session,
        &format!("SELECT hook, COUNT(*) FROM {table}{status_filter} GROUP BY hook ORDER BY COUNT(*) DESC LIMIT 30"),
    )
    .await?;
    Ok(Page {
        items,
        has_more,
        facets: facets(&statuses, status_label),
        kinds: facets(&hooks, |h| h.to_string()),
    })
}

/// Failed actions grouped by hook, newest failure first.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HookFailures {
    pub hook: String,
    pub count: u64,
    pub last_failure: Option<i64>,
    pub last_message: Option<String>,
    /// Owning plugin from the registry, when known.
    pub owner: Option<String>,
}

pub async fn failed_by_hook(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
) -> AppResult<Vec<HookFailures>> {
    let actions = t(db, ctx, "actionscheduler_actions");
    let logs = t(db, ctx, "actionscheduler_logs");
    let rs = fetch(
        session,
        &format!(
            "SELECT a.hook, COUNT(*), MAX({last}), MAX(a.action_id) \
             FROM {actions} a WHERE a.status = 'failed' \
             GROUP BY a.hook ORDER BY MAX(a.action_id) DESC LIMIT 100",
            last = epoch("IF(a.last_attempt_gmt < '1000-01-01', NULL, a.last_attempt_gmt)"),
        ),
    )
    .await?;
    let latest_ids: Vec<String> = rs.rows.iter().filter_map(|r| text(&r[3])).collect();
    let mut messages: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if !latest_ids.is_empty() {
        let ms = fetch(
            session,
            &format!(
                "SELECT action_id, message FROM {logs} WHERE action_id IN ({}) ORDER BY log_id",
                latest_ids.join(", ")
            ),
        )
        .await?;
        for r in &ms.rows {
            if let (Some(id), Some(m)) = (text(&r[0]), text(&r[1])) {
                messages.insert(id, m);
            }
        }
    }
    Ok(rs
        .rows
        .iter()
        .map(|r| {
            let hook = text(&r[0]).unwrap_or_default();
            HookFailures {
                owner: super::plugins::for_hook(&hook).map(|p| p.name.clone()),
                count: num(&r[1]).unwrap_or(0),
                last_failure: int(&r[2]),
                last_message: text(&r[3]).and_then(|id| messages.get(&id).cloned()),
                hook,
            }
        })
        .collect())
}

// ---------- sessions ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub key: String,
    pub expires: Option<i64>,
    pub customer_id: Option<String>,
    pub email: Option<String>,
    pub items: u64,
    pub total: Option<String>,
}

/// Cart summary from a `woocommerce_sessions.session_value` (serialized array whose
/// `cart`, `cart_totals` and `customer` entries are serialized strings themselves).
pub fn session_summary(
    value: &str,
    currency: Option<&str>,
) -> (u64, Option<String>, Option<String>) {
    use super::php::{PhpKey, PhpValue, parse};
    let Ok(PhpValue::Array(top)) = parse(value.as_bytes()) else {
        return (0, None, None);
    };
    let inner = |key: &str| -> Option<Vec<(PhpKey, PhpValue)>> {
        top.iter()
            .find(|(k, _)| matches!(k, PhpKey::Str(s) if s == key.as_bytes()))
            .and_then(|(_, v)| match v {
                PhpValue::Str(s) => match parse(s) {
                    Ok(PhpValue::Array(a)) => Some(a),
                    _ => None,
                },
                _ => None,
            })
    };
    let field = |arr: &[(PhpKey, PhpValue)], key: &str| -> Option<String> {
        arr.iter()
            .find(|(k, _)| matches!(k, PhpKey::Str(s) if s == key.as_bytes()))
            .and_then(|(_, v)| match v {
                PhpValue::Str(s) => String::from_utf8(s.clone()).ok(),
                PhpValue::Int(i) => Some(i.to_string()),
                PhpValue::Float(f) => Some(f.clone()),
                _ => None,
            })
    };
    let items = inner("cart")
        .map(|cart| {
            cart.iter()
                .map(|(_, item)| match item {
                    PhpValue::Array(fields) => field(fields, "quantity")
                        .and_then(|q| q.parse::<u64>().ok())
                        .unwrap_or(1),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0);
    let total = inner("cart_totals")
        .and_then(|t| field(&t, "total"))
        .and_then(|v| money(&v, currency));
    let email = inner("customer")
        .and_then(|c| field(&c, "email"))
        .filter(|e| !e.is_empty());
    (items, total, email)
}

pub async fn sessions(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<SessionSummary>> {
    let rs = fetch(
        session,
        &format!(
            "SELECT session_key, session_expiry, session_value FROM {} ORDER BY session_expiry DESC LIMIT {} OFFSET {}",
            t(db, ctx, "woocommerce_sessions"),
            q.limit() + 1,
            q.offset
        ),
    )
    .await?;
    let rows = rs
        .rows
        .iter()
        .map(|r| {
            let key = text(&r[0]).unwrap_or_default();
            let (items, total, email) = text(&r[2])
                .map(|v| session_summary(&v, ctx.site.currency.as_deref()))
                .unwrap_or((0, None, None));
            SessionSummary {
                customer_id: key.parse::<u64>().ok().map(|n| n.to_string()),
                key,
                expires: int(&r[1]),
                email,
                items,
                total,
            }
        })
        .collect();
    let (items, has_more) = page_split(rows, q.limit());
    Ok(Page {
        items,
        has_more,
        facets: Vec::new(),
        kinds: Vec::new(),
    })
}

// ---------- content ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PostSummary {
    pub id: String,
    pub title: Option<String>,
    pub kind: String,
    pub status: String,
    pub author: Option<String>,
    pub date: Option<i64>,
    pub modified: Option<i64>,
}

/// Internal post types that are shown elsewhere or are noise.
pub const HIDDEN_POST_TYPES: &[&str] = &[
    "revision",
    "nav_menu_item",
    "customize_changeset",
    "oembed_cache",
    "user_request",
    "wp_global_styles",
    "wp_navigation",
    "wp_template",
    "wp_template_part",
    "wp_font_family",
    "wp_font_face",
    "shop_order",
    "shop_order_refund",
    "shop_order_placehold",
    "shop_subscription",
    "scheduled-action",
    "product",
    "product_variation",
    "shop_coupon",
    "custom_css",
];

pub async fn content(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<PostSummary>> {
    let posts = t(db, ctx, "posts");
    let hidden = HIDDEN_POST_TYPES
        .iter()
        .map(|h| sql_string(h))
        .collect::<Vec<_>>()
        .join(", ");
    let kinds = fetch(
        session,
        &format!("SELECT post_type, COUNT(*) FROM {posts} WHERE post_type NOT IN ({hidden}) AND post_status <> 'auto-draft' GROUP BY post_type"),
    )
    .await?;
    let kinds = facets(&kinds, status_label);
    let kind = q.kind.clone().or_else(|| {
        kinds
            .iter()
            .find(|k| k.value == "post")
            .or(kinds.first())
            .map(|k| k.value.clone())
    });
    let Some(kind) = kind else {
        return Ok(Page {
            items: Vec::new(),
            has_more: false,
            facets: Vec::new(),
            kinds,
        });
    };
    let mut filter = format!(
        "p.post_type = {} AND p.post_status <> 'auto-draft'",
        sql_string(&kind)
    );
    if let Some(s) = &q.status {
        filter += &format!(" AND p.post_status = {}", sql_string(s));
    }
    if let Some(s) = q.search() {
        let id_match = s
            .parse::<u64>()
            .map(|n| format!("p.ID = {n} OR "))
            .unwrap_or_default();
        filter += &format!(
            " AND ({id_match}p.post_title LIKE {l} OR p.post_name LIKE {l})",
            l = like(s)
        );
    }
    let rs = fetch(
        session,
        &format!(
            "SELECT p.ID, p.post_title, p.post_type, p.post_status, u.display_name, {date}, {modified} FROM {posts} p \
             LEFT JOIN {users} u ON u.ID = p.post_author WHERE {filter} ORDER BY p.post_date_gmt DESC, p.ID DESC LIMIT {limit} OFFSET {offset}",
            date = epoch("IF(p.post_date_gmt < '1000-01-01', NULL, p.post_date_gmt)"),
            modified = epoch("IF(p.post_modified_gmt < '1000-01-01', NULL, p.post_modified_gmt)"),
            users = t(db, ctx, "users"),
            limit = q.limit() + 1,
            offset = q.offset,
        ),
    )
    .await?;
    let rows = rs
        .rows
        .iter()
        .map(|r| PostSummary {
            id: text(&r[0]).unwrap_or_default(),
            title: text(&r[1]),
            kind: text(&r[2]).unwrap_or_default(),
            status: text(&r[3]).unwrap_or_default(),
            author: text(&r[4]),
            date: int(&r[5]),
            modified: int(&r[6]),
        })
        .collect();
    let (items, has_more) = page_split(rows, q.limit());
    let statuses = fetch(
        session,
        &format!("SELECT post_status, COUNT(*) FROM {posts} WHERE post_type = {} AND post_status <> 'auto-draft' GROUP BY post_status", sql_string(&kind)),
    )
    .await?;
    Ok(Page {
        items,
        has_more,
        facets: facets(&statuses, status_label),
        kinds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnMeta;
    use crate::wp::context::{HposMode, WcContext};

    fn ctx(hpos: HposMode) -> WpContext {
        WpContext {
            prefix: "wp_".into(),
            version: None,
            sites: vec![],
            wc: Some(WcContext {
                version: None,
                hpos,
                has_action_scheduler: true,
            }),
            site: Default::default(),
            tables: ["wp_wc_product_meta_lookup".to_string()].into(),
        }
    }

    #[test]
    fn status_labels_are_readable() {
        assert_eq!(status_label("wc-on-hold"), "On hold");
        assert_eq!(status_label("publish"), "Publish");
        assert_eq!(status_label("in-progress"), "In progress");
    }

    #[test]
    fn hpos_order_search_matches_id_email_transaction_and_name() {
        let q = ListQuery {
            search: Some("#3088".into()),
            status: Some("wc-processing".into()),
            ..Default::default()
        };
        let sql = orders_sql("db", &ctx(HposMode::On), "shop_order", &q);
        assert!(sql.contains("FROM `db`.`wp_wc_orders` o"));
        assert!(sql.contains("o.id = 3088 OR o.billing_email LIKE '%#3088%'"));
        assert!(sql.contains("o.status = 'wc-processing'"));
        assert!(sql.contains("LIMIT 51 OFFSET 0"));
    }

    #[test]
    fn legacy_orders_pivot_post_meta() {
        let q = ListQuery {
            kind: Some("ppcp-gateway".into()),
            ..Default::default()
        };
        let sql = orders_sql("db", &ctx(HposMode::Off), "shop_order", &q);
        assert!(sql.contains("FROM `db`.`wp_posts` p LEFT JOIN `db`.`wp_postmeta` m"));
        assert!(sql.contains("HAVING MAX(CASE WHEN m.meta_key = '_payment_method' THEN m.meta_value END) = 'ppcp-gateway'"));
    }

    #[test]
    fn search_input_is_escaped() {
        let q = ListQuery {
            search: Some("o'neil 100%".into()),
            ..Default::default()
        };
        let sql = orders_sql("db", &ctx(HposMode::On), "shop_order", &q);
        assert!(sql.contains("'%o''neil 100\\\\%%'"), "{sql}");
    }

    #[test]
    fn order_rows_format_money_and_drop_guest_ids() {
        let cell = |s: &str| Cell::Text(s.into());
        let rs = ResultSet {
            columns: (0..12)
                .map(|i| ColumnMeta {
                    name: i.to_string(),
                    type_name: "TEXT".into(),
                })
                .collect(),
            rows: vec![vec![
                cell("3229"),
                cell("shop_order"),
                cell("wc-processing"),
                cell("95.40000000"),
                cell("USD"),
                cell("ppcp-gateway"),
                cell("PayPal"),
                cell("a@b.c"),
                cell("Jay Dee"),
                cell("0"),
                cell("1791906926"),
                cell("2"),
            ]],
            ..Default::default()
        };
        let o = &order_rows(&rs, None)[0];
        assert_eq!(o.total.as_deref(), Some("$95.40"));
        assert_eq!(o.customer_id, None);
        assert_eq!(o.item_count, 2);
        assert_eq!(o.created, Some(1791906926));
    }

    #[test]
    fn product_prices_render_ranges() {
        assert_eq!(price_text("10.0000–24.5000", Some("EUR")), "€10.00–€24.50");
        assert_eq!(price_text("5", Some("USD")), "$5.00");
    }

    #[test]
    fn session_values_summarize_the_cart() {
        let cart = r#"a:2:{s:32:"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";a:1:{s:8:"quantity";i:2;}s:32:"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";a:1:{s:8:"quantity";i:1;}}"#;
        let totals = r#"a:1:{s:5:"total";s:5:"42.00";}"#;
        let customer = r#"a:1:{s:5:"email";s:6:"j@x.at";}"#;
        let value = format!(
            r#"a:3:{{s:4:"cart";s:{}:"{}";s:11:"cart_totals";s:{}:"{}";s:8:"customer";s:{}:"{}";}}"#,
            cart.len(),
            cart,
            totals.len(),
            totals,
            customer.len(),
            customer
        );
        assert_eq!(
            session_summary(&value, Some("EUR")),
            (3, Some("€42.00".into()), Some("j@x.at".into()))
        );
        assert_eq!(session_summary("garbage", None), (0, None, None));
    }

    #[test]
    fn gravatar_uses_sha256_of_normalized_email() {
        assert_eq!(gravatar(" Jane@Example.com "), gravatar("jane@example.com"));
        assert!(gravatar("jane@example.com").starts_with("https://gravatar.com/avatar/"));
    }
}
