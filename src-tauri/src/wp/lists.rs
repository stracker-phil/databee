//! List screens of the WordPress mode: one query per page (plus one batched query for
//! per-row extras), filters and status counts. SQL builders are pure and unit-tested.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::rows::{count, facets, int, money, text};
use super::site::WpContext;
use super::sql::{Filter, id_match, paid_statuses_sql, table_ref};
use super::{fetch, post_images};
use crate::db::Database;
use crate::db::{Cell, Dialect, ResultSet};
use crate::error::AppResult;

pub const PAGE_MAX: u32 = 200;

#[derive(Deserialize, Debug, Clone, Default, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[ts(optional_fields = nullable)]
#[serde(rename_all = "camelCase", default)]
pub struct ListQuery {
    pub status: Option<String>,
    /// Orders: gateway id; products: product type; users: role; content: post type.
    pub kind: Option<String>,
    pub search: Option<String>,
    #[ts(as = "Option<u64>", optional)]
    pub offset: u64,
    #[ts(as = "Option<u32>", optional)]
    pub limit: u32,
    /// Sort key from the list's whitelist (see `order_by`); unknown keys use the default.
    pub sort: Option<String>,
    pub descending: Option<bool>,
}

/// Sort keys per list, mapped to SQL expressions; the first entry is the default.
pub const SORT_KEYS: &[(&str, &[(&str, &str)])] = &[
    (
        "orders_hpos",
        &[
            ("created", "o.date_created_gmt"),
            ("total", "o.total_amount"),
            ("status", "o.status"),
            ("id", "o.id"),
        ],
    ),
    (
        "orders_legacy",
        &[
            ("created", "p.post_date_gmt"),
            (
                "total",
                "CAST(MAX(CASE WHEN m.meta_key = '_order_total' THEN m.meta_value END) AS DECIMAL(18,4))",
            ),
            ("status", "p.post_status"),
            ("id", "p.ID"),
        ],
    ),
    (
        "users",
        &[
            ("registered", "u.user_registered"),
            ("name", "u.display_name"),
            ("email", "u.user_email"),
        ],
    ),
    (
        "products",
        &[
            ("created", "p.post_date_gmt"),
            ("name", "p.post_title"),
            ("price", "l.min_price"),
            ("sales", "l.total_sales"),
            ("status", "p.post_status"),
        ],
    ),
    (
        "actions",
        &[
            ("scheduled", "a.scheduled_date_gmt"),
            ("hook", "a.hook"),
            ("status", "a.status"),
            ("attempts", "a.attempts"),
        ],
    ),
    (
        "content",
        &[
            ("date", "p.post_date_gmt"),
            ("title", "p.post_title"),
            ("status", "p.post_status"),
            ("modified", "p.post_modified_gmt"),
        ],
    ),
];

/// `ORDER BY <expr> <dir>, <tiebreak>` for a list; defaults to the first key, descending.
pub fn order_by(list: &str, q: &ListQuery, tiebreak: &str) -> String {
    let keys = SORT_KEYS
        .iter()
        .find(|(l, _)| *l == list)
        .map(|(_, k)| *k)
        .unwrap_or(&[]);
    let (key, expr) = q
        .sort
        .as_deref()
        .and_then(|s| keys.iter().find(|(k, _)| *k == s))
        .or_else(|| keys.first())
        .copied()
        .unwrap_or(("", tiebreak));
    let default_desc = key != "name" && key != "title" && key != "hook" && key != "email";
    let desc = q.descending.unwrap_or(default_desc);
    let dir = if desc { "DESC" } else { "ASC" };
    format!("ORDER BY {expr} {dir}, {tiebreak} {dir}")
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

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Facet {
    pub value: String,
    pub label: String,
    pub count: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
    /// Page size after the clamp to `PAGE_MAX`; the next page starts at `offset + limit`.
    pub limit: u32,
    /// Status (or kind) counts for filter chips, independent of the current filter.
    pub facets: Vec<Facet>,
    pub kinds: Vec<Facet>,
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

// ---------- orders & subscriptions ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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

pub fn orders_sql(
    dialect: Dialect,
    db: &str,
    ctx: &WpContext,
    order_type: &str,
    q: &ListQuery,
) -> String {
    let limit = q.limit() + 1;
    if ctx.hpos() {
        let mut filter = format!("o.type = {}", dialect.sql_string(order_type));
        if let Some(s) = &q.status {
            filter += &format!(" AND o.status = {}", dialect.sql_string(s));
        }
        if let Some(g) = &q.kind {
            filter += &format!(" AND o.payment_method = {}", dialect.sql_string(g));
        }
        if let Some(s) = q.search() {
            let id_clause = id_match("o.id", s.trim_start_matches('#'));
            filter += &format!(
                " AND ({id_clause}o.billing_email LIKE {l} OR o.transaction_id = {e} OR a.first_name LIKE {l} OR a.last_name LIKE {l})",
                l = dialect.like_contains(s),
                e = dialect.sql_string(s)
            );
        }
        format!(
            "SELECT o.id, o.type, o.status, o.total_amount, o.currency, o.payment_method, o.payment_method_title, \
             o.billing_email, TRIM(CONCAT(COALESCE(a.first_name, ''), ' ', COALESCE(a.last_name, ''))), o.customer_id, {created}, \
             (SELECT COUNT(*) FROM {items} i WHERE i.order_id = o.id AND i.order_item_type = 'line_item') \
             FROM {orders} o LEFT JOIN {addr} a ON a.order_id = o.id AND a.address_type = 'billing' \
             WHERE {filter} {order} LIMIT {limit} OFFSET {offset}",
            order = order_by("orders_hpos", q, "o.id"),
            created = epoch("o.date_created_gmt"),
            items = table_ref(dialect, db, ctx, "woocommerce_order_items"),
            orders = table_ref(dialect, db, ctx, "wc_orders"),
            addr = table_ref(dialect, db, ctx, "wc_order_addresses"),
            offset = q.offset,
        )
    } else {
        let meta = |key: &str| format!("MAX(CASE WHEN m.meta_key = '{key}' THEN m.meta_value END)");
        let mut filter = format!("p.post_type = {}", dialect.sql_string(order_type));
        if let Some(s) = &q.status {
            filter += &format!(" AND p.post_status = {}", dialect.sql_string(s));
        }
        let mut having = Vec::new();
        if let Some(g) = &q.kind {
            having.push(format!(
                "{} = {}",
                meta("_payment_method"),
                dialect.sql_string(g)
            ));
        }
        if let Some(s) = q.search() {
            let id_clause = id_match("p.ID", s.trim_start_matches('#'));
            having.push(format!(
                "({id_clause}{email} LIKE {l} OR {tx} = {e} OR {first} LIKE {l} OR {last} LIKE {l})",
                email = meta("_billing_email"),
                tx = meta("_transaction_id"),
                first = meta("_billing_first_name"),
                last = meta("_billing_last_name"),
                l = dialect.like_contains(s),
                e = dialect.sql_string(s)
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
             WHERE {filter} GROUP BY p.ID{having} {order} LIMIT {limit} OFFSET {offset}",
            order = order_by("orders_legacy", q, "p.ID"),
            total = meta("_order_total"),
            currency = meta("_order_currency"),
            gw = meta("_payment_method"),
            gwt = meta("_payment_method_title"),
            email = meta("_billing_email"),
            first = meta("_billing_first_name"),
            last = meta("_billing_last_name"),
            customer = meta("_customer_user"),
            created = epoch("p.post_date_gmt"),
            items = table_ref(dialect, db, ctx, "woocommerce_order_items"),
            posts = table_ref(dialect, db, ctx, "posts"),
            postmeta = table_ref(dialect, db, ctx, "postmeta"),
            offset = q.offset,
        )
    }
}

/// Totals show as stored, with the order's own currency.
pub fn order_rows(rs: &ResultSet) -> Vec<OrderSummary> {
    rs.rows
        .iter()
        .filter(|r| r.len() > ORDER_COLS)
        .map(|r| {
            let currency = text(&r[4]);
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
                item_count: count(&r[11]).unwrap_or(0),
            }
        })
        .collect()
}

fn order_facets_sql(
    dialect: Dialect,
    db: &str,
    ctx: &WpContext,
    order_type: &str,
) -> (String, String) {
    if ctx.hpos() {
        let from = format!(
            "{} WHERE type = {}",
            table_ref(dialect, db, ctx, "wc_orders"),
            dialect.sql_string(order_type)
        );
        (
            format!("SELECT status, COUNT(*) FROM {from} GROUP BY status"),
            format!(
                "SELECT payment_method, COUNT(*) FROM {from} AND payment_method <> '' GROUP BY payment_method"
            ),
        )
    } else {
        (
            format!(
                "SELECT post_status, COUNT(*) FROM {} WHERE post_type = {} GROUP BY post_status",
                table_ref(dialect, db, ctx, "posts"),
                dialect.sql_string(order_type)
            ),
            format!(
                "SELECT m.meta_value, COUNT(*) FROM {posts} p JOIN {pm} m ON m.post_id = p.ID AND m.meta_key = '_payment_method' \
                 WHERE p.post_type = {ty} AND m.meta_value <> '' GROUP BY m.meta_value",
                posts = table_ref(dialect, db, ctx, "posts"),
                pm = table_ref(dialect, db, ctx, "postmeta"),
                ty = dialect.sql_string(order_type)
            ),
        )
    }
}

/// The order list screens; each lists one post type.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    ShopOrder,
    ShopSubscription,
}

impl OrderType {
    pub fn post_type(self) -> &'static str {
        match self {
            OrderType::ShopOrder => "shop_order",
            OrderType::ShopSubscription => "shop_subscription",
        }
    }
}

pub async fn orders(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    order_type: &str,
    q: &ListQuery,
) -> AppResult<Page<OrderSummary>> {
    let dialect = session.dialect();
    let rs = fetch(session, &orders_sql(dialect, db, ctx, order_type, q)).await?;
    let (items, has_more) = page_split(order_rows(&rs), q.limit());
    let (status_sql, gateway_sql) = order_facets_sql(dialect, db, ctx, order_type);
    let statuses = fetch(session, &status_sql).await?;
    let gateways = fetch(session, &gateway_sql).await?;
    Ok(Page {
        limit: q.limit(),
        items,
        has_more,
        facets: facets(&statuses, 0, 1),
        kinds: facets(&gateways, 0, 1),
    })
}

// ---------- users ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
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

pub fn users_sql(dialect: Dialect, db: &str, ctx: &WpContext, q: &ListQuery) -> String {
    let caps = dialect.sql_string(&format!("{}capabilities", ctx.prefix));
    let mut filter = Filter::new(dialect).and("1 = 1");
    if let Some(role) = &q.kind {
        filter = filter.and(format!(
            "c.meta_value LIKE {}",
            dialect.like_contains(&format!("\"{role}\""))
        ));
    }
    if let Some(s) = q.search() {
        filter = filter
            .id_or("u.ID", s)
            .search_any(&["u.user_login", "u.user_email", "u.display_name"], s);
    }
    format!(
        "SELECT u.ID, u.user_login, u.user_email, u.display_name, {registered}, c.meta_value \
         FROM {users} u LEFT JOIN {usermeta} c ON c.user_id = u.ID AND c.meta_key = {caps}{filter} \
         {order} LIMIT {limit} OFFSET {offset}",
        filter = filter.sql(),
        order = order_by("users", q, "u.ID"),
        registered = epoch("u.user_registered"),
        users = table_ref(dialect, db, ctx, "users"),
        usermeta = table_ref(dialect, db, ctx, "usermeta"),
        limit = q.limit() + 1,
        offset = q.offset,
    )
}

/// Virtual customers for guest checkouts are addressed as `guest:<billing email>`.
pub const GUEST_PREFIX: &str = "guest:";

/// (count, sum of totals, last order) per customer for paid-ish orders.
pub fn customer_stats_sql(
    dialect: Dialect,
    db: &str,
    ctx: &WpContext,
    ids: &[String],
) -> Option<String> {
    let list = ids
        .iter()
        .filter_map(|i| i.parse::<u64>().ok())
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if list.is_empty() || !ctx.woocommerce() {
        return None;
    }
    Some(if ctx.hpos() {
        format!(
            "SELECT customer_id, COUNT(*), SUM(CASE WHEN {paid} THEN total_amount ELSE 0 END), \
             MAX({created}) FROM {orders} WHERE type = 'shop_order' AND customer_id IN ({list}) GROUP BY customer_id",
            paid = paid_statuses_sql("status"),
            created = epoch("date_created_gmt"),
            orders = table_ref(dialect, db, ctx, "wc_orders"),
        )
    } else {
        format!(
            "SELECT c.meta_value, COUNT(*), SUM(CASE WHEN {paid} THEN tot.meta_value ELSE 0 END), \
             MAX({created}) FROM {posts} p JOIN {pm} c ON c.post_id = p.ID AND c.meta_key = '_customer_user' \
             LEFT JOIN {pm} tot ON tot.post_id = p.ID AND tot.meta_key = '_order_total' \
             WHERE p.post_type = 'shop_order' AND c.meta_value IN ({list}) GROUP BY c.meta_value",
            paid = paid_statuses_sql("p.post_status"),
            created = epoch("p.post_date_gmt"),
            posts = table_ref(dialect, db, ctx, "posts"),
            pm = table_ref(dialect, db, ctx, "postmeta"),
        )
    })
}

/// Role names from a serialized capabilities array (`a:1:{s:8:"customer";b:1;}`).
pub fn roles_of(capabilities: Option<&str>) -> Vec<String> {
    use crate::decode::php::{self, PhpKey, PhpValue};
    let Some(Ok(PhpValue::Array(items))) = capabilities.map(|c| php::parse(c.as_bytes())) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter(|(_, v)| matches!(v, PhpValue::Bool(true)))
        .filter_map(|(k, _)| match k {
            PhpKey::Str(s) => String::from_utf8(s).ok(),
            PhpKey::Int(_) => None,
        })
        .collect()
}

/// Gravatar URL (SHA-256 of the trimmed, lower-cased email). Only used when enabled.
pub fn gravatar(email: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(email.trim().to_lowercase().as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("https://gravatar.com/avatar/{hex}?s=96&d=404")
}

/// `wp_users` with roles and, on shops, order stats. Guest checkouts are not users; they
/// open from their orders (`guest:<email>`).
pub async fn users(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
    with_gravatar: bool,
) -> AppResult<Page<UserSummary>> {
    let dialect = session.dialect();
    let rs = fetch(session, &users_sql(dialect, db, ctx, q)).await?;
    let rows: Vec<UserSummary> = rs
        .rows
        .iter()
        .map(|r| {
            let email = text(&r[2]);
            UserSummary {
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
    if let Some(sql) = customer_stats_sql(dialect, db, ctx, &ids) {
        let stats = fetch(session, &sql).await?;
        let by_id: HashMap<String, &Vec<Cell>> = stats
            .rows
            .iter()
            .filter_map(|r| Some((text(&r[0])?, r)))
            .collect();
        for c in &mut items {
            if let Some(r) = by_id.get(&c.id) {
                c.order_count = count(&r[1]).unwrap_or(0);
                c.total_spent = text(&r[2]).and_then(|v| money(&v, None));
                c.last_order = int(&r[3]);
            }
        }
    }
    Ok(Page {
        limit: q.limit(),
        items,
        has_more,
        facets: Vec::new(),
        kinds: role_facets(session, ctx, db).await?,
    })
}

/// Role counts from `{prefix}capabilities` for the kind filter.
async fn role_facets(session: &dyn Database, ctx: &WpContext, db: &str) -> AppResult<Vec<Facet>> {
    let dialect = session.dialect();
    let roles = fetch(
        session,
        &format!(
            "SELECT meta_value, COUNT(*) FROM {} WHERE meta_key = {} GROUP BY meta_value",
            table_ref(dialect, db, ctx, "usermeta"),
            dialect.sql_string(&format!("{}capabilities", ctx.prefix))
        ),
    )
    .await?;
    let mut role_counts: HashMap<String, u64> = HashMap::new();
    for r in &roles.rows {
        for role in roles_of(text(&r[0]).as_deref()) {
            *role_counts.entry(role).or_default() += count(&r[1]).unwrap_or(0);
        }
    }
    let mut kinds: Vec<Facet> = role_counts
        .into_iter()
        .map(|(value, count)| Facet {
            label: value.clone(),
            value,
            count,
        })
        .collect();
    kinds.sort_by_key(|a| std::cmp::Reverse(a.count));
    Ok(kinds)
}

// ---------- products ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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

pub fn products_sql(dialect: Dialect, db: &str, ctx: &WpContext, q: &ListQuery) -> String {
    let lookup = ctx.has_table("wc_product_meta_lookup");
    let mut filter = Filter::new(dialect)
        .and("p.post_type = 'product'")
        .and("p.post_status NOT IN ('trash', 'auto-draft')");
    if let Some(s) = &q.status {
        filter = filter.and(format!("p.post_status = {}", dialect.sql_string(s)));
    }
    if let Some(k) = &q.kind {
        filter = filter.and(format!("pt.slug = {}", dialect.sql_string(k)));
    }
    if let Some(s) = q.search() {
        let cols: &[&str] = if lookup {
            &["p.post_title", "l.sku"]
        } else {
            &["p.post_title"]
        };
        filter = filter.id_or("p.ID", s).search_any(cols, s);
    }
    let (cols, join) = if lookup {
        (
            "l.sku, CASE WHEN l.min_price = l.max_price THEN l.min_price ELSE CONCAT(l.min_price, '–', l.max_price) END, l.stock_status, l.stock_quantity, l.total_sales",
            format!(
                " LEFT JOIN {} l ON l.product_id = p.ID",
                table_ref(dialect, db, ctx, "wc_product_meta_lookup")
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
         JOIN {terms} te ON te.term_id = tt.term_id) pt ON pt.object_id = p.ID{filter} \
         {order} LIMIT {limit} OFFSET {offset}",
        filter = filter.sql(),
        order = if lookup {
            order_by("products", q, "p.ID")
        } else {
            order_by(
                "content",
                &ListQuery {
                    sort: q
                        .sort
                        .clone()
                        .filter(|s| s == "name" || s == "status")
                        .map(|s| if s == "name" { "title".to_string() } else { s }),
                    ..q.clone()
                },
                "p.ID",
            )
        },
        posts = table_ref(dialect, db, ctx, "posts"),
        tr = table_ref(dialect, db, ctx, "term_relationships"),
        tt = table_ref(dialect, db, ctx, "term_taxonomy"),
        terms = table_ref(dialect, db, ctx, "terms"),
        limit = q.limit() + 1,
        offset = q.offset,
    )
}

pub async fn products(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<ProductSummary>> {
    let dialect = session.dialect();
    let rs = fetch(session, &products_sql(dialect, db, ctx, q)).await?;
    let rows: Vec<ProductSummary> = rs
        .rows
        .iter()
        .map(|r| ProductSummary {
            id: text(&r[0]).unwrap_or_default(),
            name: text(&r[1]).unwrap_or_else(|| "(no title)".into()),
            status: text(&r[2]).unwrap_or_default(),
            kind: text(&r[3]),
            sku: text(&r[4]),
            price: text(&r[5]),
            stock_status: text(&r[6]),
            stock: text(&r[7]),
            total_sales: count(&r[8]).unwrap_or(0),
            variations: count(&r[9]).unwrap_or(0),
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
            table_ref(dialect, db, ctx, "posts")
        ),
    )
    .await?;
    let kinds = fetch(
        session,
        &format!(
            "SELECT te.slug, COUNT(*) FROM {tr} tr JOIN {tt} tt ON tt.term_taxonomy_id = tr.term_taxonomy_id AND tt.taxonomy = 'product_type' \
             JOIN {terms} te ON te.term_id = tt.term_id GROUP BY te.slug",
            tr = table_ref(dialect, db, ctx, "term_relationships"),
            tt = table_ref(dialect, db, ctx, "term_taxonomy"),
            terms = table_ref(dialect, db, ctx, "terms"),
        ),
    )
    .await?;
    Ok(Page {
        limit: q.limit(),
        items,
        has_more,
        facets: facets(&statuses, 0, 1),
        kinds: facets(&kinds, 0, 1),
    })
}

// ---------- scheduled actions ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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

pub fn actions_sql(dialect: Dialect, db: &str, ctx: &WpContext, q: &ListQuery) -> String {
    let mut filter = Filter::new(dialect).and("1 = 1");
    if let Some(s) = &q.status {
        filter = filter.and(format!("a.status = {}", dialect.sql_string(s)));
    }
    if let Some(h) = &q.kind {
        filter = filter.and(format!("a.hook = {}", dialect.sql_string(h)));
    }
    if let Some(s) = q.search() {
        filter = filter
            .id_or("a.action_id", s)
            .search_any(&["a.hook", "a.args"], s);
    }
    format!(
        "SELECT a.action_id, a.hook, a.status, {sched}, {last}, a.attempts, a.args, g.slug, \
         (SELECT l.message FROM {logs} l WHERE l.action_id = a.action_id ORDER BY l.log_id DESC LIMIT 1) \
         FROM {actions} a LEFT JOIN {groups} g ON g.group_id = a.group_id{filter} \
         {order} LIMIT {limit} OFFSET {offset}",
        filter = filter.sql(),
        order = order_by("actions", q, "a.action_id"),
        sched = epoch("a.scheduled_date_gmt"),
        last = epoch("IF(a.last_attempt_gmt < '1000-01-01', NULL, a.last_attempt_gmt)"),
        logs = table_ref(dialect, db, ctx, "actionscheduler_logs"),
        actions = table_ref(dialect, db, ctx, "actionscheduler_actions"),
        groups = table_ref(dialect, db, ctx, "actionscheduler_groups"),
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
            attempts: count(&r[5]).unwrap_or(0),
            args: text(&r[6]).filter(|a| a != "[]"),
            group: text(&r[7]),
            message: text(&r[8]),
        })
        .collect()
}

pub async fn actions(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<ActionSummary>> {
    let dialect = session.dialect();
    let rs = fetch(session, &actions_sql(dialect, db, ctx, q)).await?;
    let (items, has_more) = page_split(action_rows(&rs), q.limit());
    let table = table_ref(dialect, db, ctx, "actionscheduler_actions");
    let statuses = fetch(
        session,
        &format!("SELECT status, COUNT(*) FROM {table} GROUP BY status"),
    )
    .await?;
    let status_filter = q
        .status
        .as_ref()
        .map(|s| format!(" WHERE status = {}", dialect.sql_string(s)))
        .unwrap_or_default();
    let hooks = fetch(
        session,
        &format!("SELECT hook, COUNT(*) FROM {table}{status_filter} GROUP BY hook ORDER BY COUNT(*) DESC LIMIT 30"),
    )
    .await?;
    Ok(Page {
        limit: q.limit(),
        items,
        has_more,
        facets: facets(&statuses, 0, 1),
        kinds: facets(&hooks, 0, 1),
    })
}

/// Failed actions grouped by hook, newest failure first.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct HookFailures {
    pub hook: String,
    pub count: u64,
    pub last_failure: Option<i64>,
    pub last_message: Option<String>,
}

pub async fn failed_by_hook(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
) -> AppResult<Vec<HookFailures>> {
    let dialect = session.dialect();
    let actions = table_ref(dialect, db, ctx, "actionscheduler_actions");
    let logs = table_ref(dialect, db, ctx, "actionscheduler_logs");
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
                count: count(&r[1]).unwrap_or(0),
                last_failure: int(&r[2]),
                last_message: text(&r[3]).and_then(|id| messages.get(&id).cloned()),
                hook,
            }
        })
        .collect())
}

// ---------- sessions ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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
pub fn session_summary(value: &str) -> (u64, Option<String>, Option<String>) {
    use crate::decode::php::{PhpValue, parse};
    let Ok(top) = parse(value.as_bytes()) else {
        return (0, None, None);
    };
    let inner = |key: &str| match top.get(key)? {
        PhpValue::Str(s) => parse(s).ok(),
        _ => None,
    };
    let items = match inner("cart") {
        Some(PhpValue::Array(cart)) => cart
            .iter()
            .map(|(_, item)| match item {
                PhpValue::Array(_) => item
                    .text_at("quantity")
                    .and_then(|q| q.parse::<u64>().ok())
                    .unwrap_or(1),
                _ => 0,
            })
            .sum(),
        _ => 0,
    };
    let total = inner("cart_totals")
        .and_then(|t| t.text_at("total"))
        .and_then(|v| money(&v, None));
    let email = inner("customer")
        .and_then(|c| c.text_at("email"))
        .filter(|e| !e.is_empty());
    (items, total, email)
}

pub async fn sessions(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<SessionSummary>> {
    let dialect = session.dialect();
    let rs = fetch(
        session,
        &format!(
            "SELECT session_key, session_expiry, session_value FROM {} ORDER BY session_expiry DESC LIMIT {} OFFSET {}",
            table_ref(dialect, db, ctx, "woocommerce_sessions"),
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
                .map(|v| session_summary(&v))
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
        limit: q.limit(),
        items,
        has_more,
        facets: Vec::new(),
        kinds: Vec::new(),
    })
}

// ---------- content ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    q: &ListQuery,
) -> AppResult<Page<PostSummary>> {
    let dialect = session.dialect();
    let posts = table_ref(dialect, db, ctx, "posts");
    let hidden = HIDDEN_POST_TYPES
        .iter()
        .map(|h| dialect.sql_string(h))
        .collect::<Vec<_>>()
        .join(", ");
    let kinds = fetch(
        session,
        &format!("SELECT post_type, COUNT(*) FROM {posts} WHERE post_type NOT IN ({hidden}) AND post_status <> 'auto-draft' GROUP BY post_type"),
    )
    .await?;
    let kinds = facets(&kinds, 0, 1);
    // No kind lists every visible type, so the "All" chip shows what was fetched.
    let type_filter = match &q.kind {
        Some(kind) => format!("p.post_type = {}", dialect.sql_string(kind)),
        None => format!("p.post_type NOT IN ({hidden})"),
    };
    let mut filter = Filter::new(dialect)
        .and(type_filter.as_str())
        .and("p.post_status <> 'auto-draft'");
    if let Some(s) = &q.status {
        filter = filter.and(format!("p.post_status = {}", dialect.sql_string(s)));
    }
    if let Some(s) = q.search() {
        filter = filter
            .id_or("p.ID", s)
            .search_any(&["p.post_title", "p.post_name"], s);
    }
    let rs = fetch(
        session,
        &format!(
            "SELECT p.ID, p.post_title, p.post_type, p.post_status, u.display_name, {date}, {modified} FROM {posts} p \
             LEFT JOIN {users} u ON u.ID = p.post_author{filter} {order} LIMIT {limit} OFFSET {offset}",
            filter = filter.sql(),
            order = order_by("content", q, "p.ID"),
            date = epoch("IF(p.post_date_gmt < '1000-01-01', NULL, p.post_date_gmt)"),
            modified = epoch("IF(p.post_modified_gmt < '1000-01-01', NULL, p.post_modified_gmt)"),
            users = table_ref(dialect, db, ctx, "users"),
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
        &format!("SELECT p.post_status, COUNT(*) FROM {posts} p WHERE {type_filter} AND p.post_status <> 'auto-draft' GROUP BY p.post_status"),
    )
    .await?;
    Ok(Page {
        limit: q.limit(),
        items,
        has_more,
        facets: facets(&statuses, 0, 1),
        kinds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_keys_are_whitelisted_with_sensible_defaults() {
        let q = |sort: Option<&str>, desc: Option<bool>| ListQuery {
            sort: sort.map(String::from),
            descending: desc,
            ..Default::default()
        };
        assert_eq!(
            order_by("orders_hpos", &q(None, None), "o.id"),
            "ORDER BY o.date_created_gmt DESC, o.id DESC"
        );
        assert_eq!(
            order_by("orders_hpos", &q(Some("total"), Some(false)), "o.id"),
            "ORDER BY o.total_amount ASC, o.id ASC"
        );
        assert_eq!(
            order_by("orders_hpos", &q(Some("DROP TABLE"), None), "o.id"),
            "ORDER BY o.date_created_gmt DESC, o.id DESC"
        );
        // Text columns sort ascending unless told otherwise.
        assert_eq!(
            order_by("actions", &q(Some("hook"), None), "a.action_id"),
            "ORDER BY a.hook ASC, a.action_id ASC"
        );
        assert_eq!(
            order_by("unknown", &q(Some("x"), None), "id"),
            "ORDER BY id DESC, id DESC"
        );
    }

    #[test]
    fn page_size_defaults_to_fifty_and_is_clamped_to_the_maximum() {
        for (requested, expected) in [
            (0, 50),
            (1, 1),
            (PAGE_MAX, PAGE_MAX),
            (PAGE_MAX + 1, PAGE_MAX),
            (u32::MAX, PAGE_MAX),
        ] {
            let q = ListQuery {
                limit: requested,
                ..Default::default()
            };
            assert_eq!(q.limit(), expected, "{requested}");
        }
    }

    use crate::db::ColumnMeta;
    use crate::wp::testing::site;

    /// A WooCommerce site; `hpos` adds the `wc_orders` table.
    fn ctx(hpos: bool) -> WpContext {
        let mut tables = vec![
            "options",
            "posts",
            "users",
            "woocommerce_order_items",
            "wc_product_meta_lookup",
        ];
        if hpos {
            tables.push("wc_orders");
        }
        site(&tables).0
    }

    fn t(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn result(width: usize, rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: (0..width)
                .map(|i| ColumnMeta {
                    name: i.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            ..Default::default()
        }
    }

    #[test]
    fn hpos_order_search_matches_id_email_transaction_and_name() {
        let q = ListQuery {
            search: Some("#3088".into()),
            status: Some("wc-processing".into()),
            ..Default::default()
        };
        let sql = orders_sql(Dialect::MYSQL, "db", &ctx(true), "shop_order", &q);
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
        let sql = orders_sql(Dialect::MYSQL, "db", &ctx(false), "shop_order", &q);
        assert!(sql.contains("FROM `db`.`wp_posts` p LEFT JOIN `db`.`wp_postmeta` m"));
        assert!(sql.contains("HAVING MAX(CASE WHEN m.meta_key = '_payment_method' THEN m.meta_value END) = 'ppcp-gateway'"));
    }

    #[test]
    fn search_input_is_escaped() {
        let q = ListQuery {
            search: Some("o'neil 100%".into()),
            ..Default::default()
        };
        let sql = orders_sql(Dialect::MYSQL, "db", &ctx(true), "shop_order", &q);
        assert!(sql.contains("'%o''neil 100\\\\%%'"), "{sql}");
    }

    #[test]
    fn order_rows_show_the_stored_amount_with_its_currency_code_and_drop_guest_ids() {
        let rs = result(
            12,
            vec![vec![
                t("3229"),
                t("shop_order"),
                t("wc-processing"),
                t("95.40000000"),
                t("USD"),
                t("ppcp-gateway"),
                t("PayPal"),
                t("a@b.c"),
                t("Jay Dee"),
                t("0"),
                t("1791906926"),
                t("2"),
            ]],
        );

        let rows = order_rows(&rs);

        assert_eq!(
            rows,
            [OrderSummary {
                id: "3229".into(),
                kind: "shop_order".into(),
                status: "wc-processing".into(),
                total: Some("95.40000000 USD".into()),
                gateway: Some("ppcp-gateway".into()),
                gateway_title: Some("PayPal".into()),
                email: Some("a@b.c".into()),
                name: Some("Jay Dee".into()),
                customer_id: None,
                created: Some(1791906926),
                item_count: 2,
            }]
        );
    }

    #[test]
    fn order_totals_without_a_currency_show_the_bare_amount() {
        let mut row = vec![
            t("1"),
            t("shop_order"),
            t("wc-pending"),
            t("12.50"),
            Cell::Null,
            t(""),
            t(""),
            t(""),
            t(""),
            t("7"),
            t("1"),
            t("0"),
        ];
        let rs = result(12, vec![row.clone()]);
        row[3] = Cell::Null;
        let no_total = result(12, vec![row]);

        assert_eq!(order_rows(&rs)[0].total.as_deref(), Some("12.50"));
        assert_eq!(order_rows(&rs)[0].customer_id.as_deref(), Some("7"));
        assert_eq!(order_rows(&no_total)[0].total, None);
    }

    #[test]
    fn order_rows_with_too_few_columns_are_skipped() {
        assert!(order_rows(&result(11, vec![vec![t("1"); 11]])).is_empty());
    }

    #[test]
    fn roles_are_the_capabilities_granted_as_true() {
        for (stored, expected) in [
            (r#"a:1:{s:8:"customer";b:1;}"#, vec!["customer"]),
            (
                r#"a:2:{s:13:"administrator";b:1;s:6:"editor";b:1;}"#,
                vec!["administrator", "editor"],
            ),
            (
                r#"a:2:{s:8:"customer";b:1;s:6:"editor";b:0;}"#,
                vec!["customer"],
            ),
            (r#"a:1:{i:0;b:1;}"#, vec![]),
            (r#"s:8:"customer";"#, vec![]),
            ("garbage", vec![]),
            ("", vec![]),
        ] {
            assert_eq!(roles_of(Some(stored)), expected, "{stored:?}");
        }
        assert!(roles_of(None).is_empty());
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
            session_summary(&value),
            (3, Some("42.00".into()), Some("j@x.at".into()))
        );
        assert_eq!(session_summary("garbage"), (0, None, None));
    }

    #[test]
    fn gravatar_uses_sha256_of_normalized_email() {
        assert_eq!(gravatar(" Jane@Example.com "), gravatar("jane@example.com"));
        assert!(gravatar("jane@example.com").starts_with("https://gravatar.com/avatar/"));
    }
}
