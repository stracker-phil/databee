//! Purpose-built detail views (order, customer, product, action, session) and omnisearch.
//! Each view embeds the generic `EntityDoc` so nothing is hidden; the extra structure is
//! derived from it plus a few batched queries.

use std::collections::HashMap;

use serde::Serialize;

use super::context::WpContext;
use super::entity::{Catalog, EntityDoc, EntityLink, cell_str};
use super::hints::{Hint, money};
use super::lists::{self, ListQuery, OrderSummary, epoch, int, num, roles_of, text};
use super::php::{self, PhpKey, PhpValue};
use super::{fetch, load_docs, post_images, sql_string, titles, value};
use crate::db::mysql::MySqlSession;
use crate::db::{ResultSet, quote_ident};
use crate::error::{AppError, AppResult};

fn t(db: &str, ctx: &WpContext, table: &str) -> String {
    format!("{}.{}", quote_ident(db), quote_ident(&ctx.table(table)))
}

/// First non-empty value among field/meta names.
fn pick(doc: &EntityDoc, names: &[&str]) -> Option<String> {
    names.iter().find_map(|n| doc.text(n).map(String::from))
}

fn time_of(doc: &EntityDoc, names: &[&str]) -> Option<i64> {
    names
        .iter()
        .find_map(|n| match doc.field(n)?.hint.as_ref()? {
            Hint::Time { unix, .. } => Some(*unix),
            _ => None,
        })
}

fn link(entity: &str, id: impl Into<String>) -> EntityLink {
    EntityLink {
        entity: entity.into(),
        id: id.into(),
        label: None,
        image: None,
    }
}

fn php_entry<'a>(items: &'a [(PhpKey, PhpValue)], key: &str) -> Option<&'a PhpValue> {
    items
        .iter()
        .find(|(k, _)| matches!(k, PhpKey::Str(s) if s == key.as_bytes()))
        .map(|(_, v)| v)
}

fn php_str(v: Option<&PhpValue>) -> Option<String> {
    match v? {
        PhpValue::Str(s) => String::from_utf8(s.clone()).ok(),
        PhpValue::Int(i) => Some(i.to_string()),
        PhpValue::Float(f) => Some(f.clone()),
        _ => None,
    }
}

// ---------- shared pieces ----------

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub name: Option<String>,
    pub company: Option<String>,
    pub lines: Vec<String>,
    pub country: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

impl Address {
    fn is_empty(&self) -> bool {
        self.name.is_none() && self.lines.is_empty() && self.email.is_none() && self.phone.is_none()
    }

    /// Builds an address from `get("first_name")`-style lookups.
    fn from_parts(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let name = [get("first_name"), get("last_name")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let city_line = [get("postcode"), get("city"), get("state")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let a = Address {
            name: (!name.is_empty()).then_some(name),
            company: get("company"),
            lines: [
                get("address_1"),
                get("address_2"),
                (!city_line.is_empty()).then_some(city_line),
            ]
            .into_iter()
            .flatten()
            .collect(),
            country: get("country"),
            email: get("email"),
            phone: get("phone"),
        };
        (!a.is_empty()).then_some(a)
    }
}

/// Differences between billing and shipping worth a second look.
pub fn address_mismatches(billing: &Address, shipping: &Address) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(b), Some(s)) = (&billing.country, &shipping.country)
        && b != s
    {
        out.push(format!("Country differs: billing {b}, shipping {s}"));
    }
    if let (Some(b), Some(s)) = (&billing.name, &shipping.name)
        && !b.eq_ignore_ascii_case(s)
    {
        out.push(format!("Name differs: billing {b}, shipping {s}"));
    }
    if billing.lines.last() != shipping.lines.last()
        && !billing.lines.is_empty()
        && !shipping.lines.is_empty()
    {
        out.push("City/postcode differs".into());
    }
    out
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    pub at: i64,
    /// created, paid, completed, modified, note, customer_note, action, log, session, refund
    pub kind: &'static str,
    pub title: String,
    pub detail: Option<String>,
    /// info, warning, error
    pub level: &'static str,
    pub link: Option<EntityLink>,
    /// The full original text when `title` is a summary (log lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// The table row behind the event, for "Source row".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<RowRef>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RowRef {
    pub table: String,
    pub id_column: String,
    pub id: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelatedChip {
    pub label: String,
    pub link: Option<EntityLink>,
    /// For counts without a single target (e.g. "3 notes").
    pub count: Option<u64>,
    /// Route hint for the UI: entity, list:<name>
    pub target: Option<String>,
}

// ---------- order ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineItem {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub quantity: Option<String>,
    pub total: Option<String>,
    pub product: Option<EntityLink>,
    pub sku: Option<String>,
    /// Item meta without `_` keys, empty and serialized values; the rest is one click away
    /// in `woocommerce_order_itemmeta`.
    pub meta: Vec<(String, String)>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fees {
    pub gross: String,
    pub fee: String,
    pub net: String,
    /// Fee share of gross, 0–1, for the waterfall bar.
    pub fee_ratio: f64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPanel {
    pub gateway: Option<String>,
    pub gateway_title: Option<String>,
    pub transaction_id: Option<String>,
    pub paypal_order_id: Option<String>,
    pub intent: Option<String>,
    pub mode: Option<String>,
    pub source: Option<String>,
    pub payer_email: Option<String>,
    pub fees: Option<Fees>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub source: Option<String>,
    pub source_type: Option<String>,
    pub referrer: Option<String>,
    pub entry: Option<String>,
    pub pages: Option<String>,
    pub sessions: Option<String>,
    pub device: Option<String>,
    pub user_agent: Option<String>,
    pub session_start: Option<i64>,
    pub utm: Vec<(String, String)>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderView {
    pub doc: EntityDoc,
    pub kind: String,
    pub status: String,
    pub total: Option<String>,
    pub currency: Option<String>,
    pub created: Option<i64>,
    pub paid: Option<i64>,
    pub completed: Option<i64>,
    pub customer: Option<EntityLink>,
    pub customer_email: Option<String>,
    pub parent: Option<EntityLink>,
    pub billing: Option<Address>,
    pub shipping: Option<Address>,
    pub address_notes: Vec<String>,
    pub items: Vec<LineItem>,
    pub totals: Vec<(String, String)>,
    pub payment: Option<PaymentPanel>,
    pub attribution: Option<Attribution>,
    pub timeline: Vec<TimelineEvent>,
    pub related: Vec<RelatedChip>,
}

/// WooCommerce stores some item meta HTML-escaped (`Mug &times; 1`).
pub fn decode_entities(s: &str) -> String {
    s.replace("&times;", "×")
        .replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// Gross → fee → net from `_ppcp_paypal_fees` (serialized PayPal breakdown).
pub fn paypal_fees(raw: &str) -> Option<Fees> {
    let PhpValue::Array(top) = php::parse(raw.as_bytes()).ok()? else {
        return None;
    };
    let amount = |key: &str| -> Option<(String, String)> {
        let PhpValue::Array(a) = php_entry(&top, key)? else {
            return None;
        };
        Some((
            php_str(php_entry(a, "value"))?,
            php_str(php_entry(a, "currency_code")).unwrap_or_default(),
        ))
    };
    let (gross, currency) = amount("gross_amount")?;
    let (fee, _) = amount("paypal_fee")?;
    let net = amount("net_amount").map(|(v, _)| v);
    let g: f64 = gross.parse().ok()?;
    let f: f64 = fee.parse().ok()?;
    let n = net.and_then(|n| n.parse::<f64>().ok()).unwrap_or(g - f);
    let cur = (!currency.is_empty()).then_some(currency.as_str());
    Some(Fees {
        gross: money(&g.to_string(), cur)?,
        fee: money(&f.to_string(), cur)?,
        net: money(&n.to_string(), cur)?,
        fee_ratio: if g > 0.0 {
            (f / g).clamp(0.0, 1.0)
        } else {
            0.0
        },
    })
}

/// WooCommerce log levels (RFC 5424 mapped to 100–800).
fn log_level(level: Option<u64>) -> &'static str {
    match level.unwrap_or(0) {
        0..=250 => "info",
        251..=300 => "warning",
        _ => "error",
    }
}

/// Whether Action Scheduler args (JSON) reference the given ID as a number or string.
pub fn args_reference(args: &str, id: &str) -> bool {
    fn walk(v: &serde_json::Value, id: &str) -> bool {
        match v {
            serde_json::Value::Number(n) => n.to_string() == id,
            serde_json::Value::String(s) => s == id,
            serde_json::Value::Array(a) => a.iter().any(|x| walk(x, id)),
            serde_json::Value::Object(o) => o.values().any(|x| walk(x, id)),
            _ => false,
        }
    }
    serde_json::from_str::<serde_json::Value>(args).is_ok_and(|v| walk(&v, id))
}

async fn order_items(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    order_id: u64,
    currency: Option<&str>,
) -> AppResult<(Vec<LineItem>, HashMap<String, f64>)> {
    let items = fetch(
        session,
        &format!(
            "SELECT order_item_id, order_item_name, order_item_type FROM {} WHERE order_id = {order_id} ORDER BY order_item_id",
            t(db, ctx, "woocommerce_order_items")
        ),
    )
    .await?;
    let ids: Vec<String> = items.rows.iter().filter_map(|r| text(&r[0])).collect();
    let mut meta: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if !ids.is_empty() {
        let rs = fetch(
            session,
            &format!(
                "SELECT order_item_id, meta_key, meta_value FROM {} WHERE order_item_id IN ({}) ORDER BY meta_id",
                t(db, ctx, "woocommerce_order_itemmeta"),
                ids.join(", ")
            ),
        )
        .await?;
        for r in &rs.rows {
            if let (Some(id), Some(k)) = (text(&r[0]), text(&r[1])) {
                meta.entry(id)
                    .or_default()
                    .push((k, text(&r[2]).unwrap_or_default()));
            }
        }
    }
    let get = |m: &[(String, String)], k: &str| {
        m.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
    };
    let mut sums: HashMap<String, f64> = HashMap::new();
    let mut add = |k: &str, v: Option<String>| {
        if let Some(n) = v.and_then(|v| v.parse::<f64>().ok()) {
            *sums.entry(k.into()).or_default() += n;
        }
    };
    let mut out = Vec::new();
    for r in &items.rows {
        let id = text(&r[0]).unwrap_or_default();
        let kind = text(&r[2]).unwrap_or_default();
        let m = meta.remove(&id).unwrap_or_default();
        let (total, quantity) = match kind.as_str() {
            "line_item" => {
                add("subtotal", get(&m, "_line_subtotal"));
                add("tax", get(&m, "_line_tax"));
                (get(&m, "_line_total"), get(&m, "_qty"))
            }
            "shipping" => {
                add("shipping", get(&m, "cost"));
                add("tax", get(&m, "total_tax"));
                (get(&m, "cost"), None)
            }
            "fee" => {
                add("fees", get(&m, "_line_total"));
                add("tax", get(&m, "_line_tax"));
                (get(&m, "_line_total"), None)
            }
            "coupon" => {
                add("discount", get(&m, "discount_amount"));
                (get(&m, "discount_amount").map(|d| format!("-{d}")), None)
            }
            "tax" => (get(&m, "tax_amount"), None),
            _ => (None, None),
        };
        let product_id = get(&m, "_variation_id")
            .filter(|v| v != "0")
            .or_else(|| get(&m, "_product_id"))
            .filter(|v| v != "0");
        out.push(LineItem {
            name: text(&r[1]).unwrap_or_default(),
            total: total.and_then(|v| money(&v, currency)),
            quantity,
            product: product_id.map(|p| link("post", p)),
            sku: None,
            meta: m
                .into_iter()
                .filter(|(k, v)| !k.starts_with('_') && !v.is_empty() && !php::looks_serialized(v))
                .map(|(k, v)| (k, decode_entities(&v)))
                .collect(),
            kind,
            id,
        });
    }
    // Product names, images and SKUs in one batch each.
    let product_ids: Vec<String> = out
        .iter()
        .filter_map(|i| i.product.as_ref().map(|p| p.id.clone()))
        .collect();
    if !product_ids.is_empty() {
        let names = titles(session, ctx, catalog, db, "post", &product_ids).await?;
        let mut images = post_images(session, ctx, db, &product_ids).await?;
        // Variations without their own image show the parent's.
        let parents = fetch(
            session,
            &format!(
                "SELECT ID, post_parent FROM {} WHERE ID IN ({}) AND post_type = 'product_variation'",
                t(db, ctx, "posts"),
                product_ids.join(", ")
            ),
        )
        .await?;
        let parent_of: HashMap<String, String> = parents
            .rows
            .iter()
            .filter_map(|r| Some((text(&r[0])?, text(&r[1])?)))
            .collect();
        let missing: Vec<String> = product_ids
            .iter()
            .filter(|p| !images.contains_key(*p))
            .filter_map(|p| parent_of.get(p).cloned())
            .collect();
        let parent_images = post_images(session, ctx, db, &missing).await?;
        for (v, p) in &parent_of {
            if let (false, Some(img)) = (images.contains_key(v), parent_images.get(p)) {
                images.insert(v.clone(), img.clone());
            }
        }
        let skus = fetch(
            session,
            &format!(
                "SELECT post_id, meta_value FROM {} WHERE meta_key = '_sku' AND post_id IN ({})",
                t(db, ctx, "postmeta"),
                product_ids.join(", ")
            ),
        )
        .await?;
        let sku_of: HashMap<String, String> = skus
            .rows
            .iter()
            .filter_map(|r| Some((text(&r[0])?, text(&r[1])?)))
            .collect();
        for item in &mut out {
            if let Some(p) = item.product.as_mut() {
                p.label = names.get(&p.id).cloned();
                p.image = images.get(&p.id).cloned();
                item.sku = sku_of.get(&p.id).cloned();
            }
        }
    }
    Ok((out, sums))
}

pub async fn order_view(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    entity: &str,
    id: &str,
) -> AppResult<OrderView> {
    let doc = load_docs(session, ctx, catalog, db, entity, &[id.to_string()])
        .await?
        .pop()
        .ok_or_else(|| AppError::InvalidInput(format!("Order #{id} was not found")))?;
    let order_id: u64 = id
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
    let currency =
        pick(&doc, &["currency", "_order_currency"]).or_else(|| ctx.site.currency.clone());
    let cur = currency.as_deref();

    // Addresses: HPOS table or legacy meta.
    let (billing, shipping) = if ctx.hpos() {
        let rs = fetch(
            session,
            &format!(
                "SELECT address_type, first_name, last_name, company, address_1, address_2, city, state, postcode, country, email, phone \
                 FROM {} WHERE order_id = {order_id}",
                t(db, ctx, "wc_order_addresses")
            ),
        )
        .await?;
        let addr = |kind: &str| {
            let row = rs
                .rows
                .iter()
                .find(|r| text(&r[0]).as_deref() == Some(kind))?;
            let col = |i: usize| text(&row[i]);
            Address::from_parts(|k| match k {
                "first_name" => col(1),
                "last_name" => col(2),
                "company" => col(3),
                "address_1" => col(4),
                "address_2" => col(5),
                "city" => col(6),
                "state" => col(7),
                "postcode" => col(8),
                "country" => col(9),
                "email" => col(10),
                "phone" => col(11),
                _ => None,
            })
        };
        (addr("billing"), addr("shipping"))
    } else {
        (
            Address::from_parts(|k| doc.text(&format!("_billing_{k}")).map(String::from)),
            Address::from_parts(|k| doc.text(&format!("_shipping_{k}")).map(String::from)),
        )
    };
    let address_notes = match (&billing, &shipping) {
        (Some(b), Some(s)) => address_mismatches(b, s),
        _ => Vec::new(),
    };

    // Operational data (HPOS) holds paid/completed dates and order-level discounts.
    let mut paid = time_of(&doc, &["_date_paid", "_paid_date"]);
    let mut completed = time_of(&doc, &["_date_completed", "_completed_date"]);
    if ctx.has_table("wc_order_operational_data") && ctx.hpos() {
        let rs = fetch(
            session,
            &format!(
                "SELECT {}, {} FROM {} WHERE order_id = {order_id}",
                epoch("date_paid_gmt"),
                epoch("date_completed_gmt"),
                t(db, ctx, "wc_order_operational_data")
            ),
        )
        .await?;
        if let Some(r) = rs.rows.first() {
            paid = paid.or(int(&r[0]));
            completed = completed.or(int(&r[1]));
        }
    }

    let (items, sums) = order_items(session, ctx, catalog, db, order_id, cur).await?;
    let total = pick(&doc, &["total_amount", "_order_total"]);
    let fmt = |k: &str| {
        sums.get(k)
            .filter(|v| **v != 0.0)
            .and_then(|v| money(&format!("{v:.4}"), cur))
    };
    let mut totals: Vec<(String, String)> = [
        ("Subtotal", fmt("subtotal")),
        ("Discount", fmt("discount").map(|d| format!("−{d}"))),
        ("Shipping", fmt("shipping")),
        ("Fees", fmt("fees")),
        ("Tax", fmt("tax")),
    ]
    .into_iter()
    .filter_map(|(l, v)| Some((l.to_string(), v?)))
    .collect();

    // Refunds and subscriptions hanging off this order.
    let (children_sql, parent) = if ctx.hpos() {
        (
            format!(
                "SELECT id, type, total_amount, {} FROM {} WHERE parent_order_id = {order_id} ORDER BY id",
                epoch("date_created_gmt"),
                t(db, ctx, "wc_orders")
            ),
            pick(&doc, &["parent_order_id"]).filter(|p| p != "0"),
        )
    } else {
        (
            format!(
                "SELECT p.ID, p.post_type, m.meta_value, {} FROM {} p LEFT JOIN {} m ON m.post_id = p.ID AND m.meta_key = '_order_total' \
                 WHERE p.post_parent = {order_id} AND p.post_type IN ('shop_order_refund', 'shop_subscription') ORDER BY p.ID",
                epoch("p.post_date_gmt"),
                t(db, ctx, "posts"),
                t(db, ctx, "postmeta")
            ),
            pick(&doc, &["post_parent"]).filter(|p| p != "0"),
        )
    };
    let children = fetch(session, &children_sql).await?;
    let mut refunds = Vec::new();
    let mut subscriptions = Vec::new();
    let mut refunded = 0.0;
    for r in &children.rows {
        let (Some(cid), Some(kind)) = (text(&r[0]), text(&r[1])) else {
            continue;
        };
        if kind == "shop_order_refund" {
            refunded += text(&r[2])
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
                .abs();
            refunds.push((cid, int(&r[3]), text(&r[2])));
        } else if kind == "shop_subscription" {
            subscriptions.push(cid);
        }
    }
    if refunded > 0.0 {
        totals.push((
            "Refunded".into(),
            format!("−{}", money(&refunded.to_string(), cur).unwrap_or_default()),
        ));
    }
    if let Some(t) = total.as_deref().and_then(|v| money(v, cur)) {
        totals.push(("Total".into(), t));
    }

    let payment = {
        let gateway = pick(&doc, &["payment_method", "_payment_method"]);
        let fees = pick(&doc, &["_ppcp_paypal_fees"])
            .and_then(|f| paypal_fees(&f))
            .or_else(|| {
                let fee = pick(&doc, &["PayPal Transaction Fee"])?;
                let gross: f64 = total.as_deref()?.parse().ok()?;
                let f: f64 = fee.parse().ok()?;
                Some(Fees {
                    gross: money(&gross.to_string(), cur)?,
                    fee: money(&f.to_string(), cur)?,
                    net: money(&(gross - f).to_string(), cur)?,
                    fee_ratio: if gross > 0.0 { f / gross } else { 0.0 },
                })
            });
        let p = PaymentPanel {
            gateway_title: pick(&doc, &["payment_method_title", "_payment_method_title"]),
            transaction_id: pick(&doc, &["transaction_id", "_transaction_id"]),
            paypal_order_id: pick(&doc, &["_ppcp_paypal_order_id"]),
            intent: pick(&doc, &["_ppcp_paypal_intent"]),
            mode: pick(&doc, &["_ppcp_paypal_payment_mode"]),
            source: pick(&doc, &["_ppcp_paypal_payment_source"]),
            payer_email: pick(&doc, &["_ppcp_paypal_payer_email"]),
            fees,
            gateway,
        };
        (p.gateway.is_some() || p.transaction_id.is_some()).then_some(p)
    };

    let attribution = {
        let a = |k: &str| pick(&doc, &[&format!("_wc_order_attribution_{k}")]);
        let utm: Vec<(String, String)> = [
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_content",
            "utm_term",
            "utm_id",
        ]
        .iter()
        .filter_map(|k| Some((k.trim_start_matches("utm_").to_string(), a(k)?)))
        .collect();
        let at = Attribution {
            source: a("utm_source"),
            source_type: a("source_type"),
            referrer: a("referrer"),
            entry: a("session_entry"),
            pages: a("session_pages"),
            sessions: a("session_count"),
            device: a("device_type"),
            user_agent: a("user_agent"),
            session_start: time_of(&doc, &["_wc_order_attribution_session_start_time"]),
            utm,
        };
        (at.source_type.is_some() || at.entry.is_some()).then_some(at)
    };

    let customer = pick(&doc, &["customer_id", "_customer_user"])
        .filter(|c| c != "0")
        .map(|c| link("user", c));
    let customer = match customer {
        Some(mut c) => {
            c.label = titles(
                session,
                ctx,
                catalog,
                db,
                "user",
                std::slice::from_ref(&c.id),
            )
            .await?
            .remove(&c.id);
            Some(c)
        }
        None => None,
    };
    let created = time_of(&doc, &["date_created_gmt", "post_date_gmt"]);

    // ---- timeline ----
    let mut timeline = Vec::new();
    let mut event = |at: Option<i64>,
                     kind,
                     title: String,
                     detail: Option<String>,
                     level,
                     link: Option<EntityLink>| {
        if let Some(at) = at {
            timeline.push(TimelineEvent {
                at,
                kind,
                title,
                detail,
                level,
                link,
                raw: None,
                row: None,
            });
        }
    };
    if let Some(a) = &attribution {
        event(
            a.session_start,
            "session",
            "Shopping session started".into(),
            a.entry.clone(),
            "info",
            None,
        );
    }
    event(
        created,
        "created",
        "Order created".into(),
        pick(&doc, &["created_via"]),
        "info",
        None,
    );
    event(
        paid,
        "paid",
        "Payment received".into(),
        payment.as_ref().and_then(|p| p.transaction_id.clone()),
        "info",
        None,
    );
    event(
        completed,
        "completed",
        "Order completed".into(),
        None,
        "info",
        None,
    );
    for (rid, at, amount) in &refunds {
        event(
            *at,
            "refund",
            format!("Refund #{rid}"),
            amount.as_deref().and_then(|a| money(a, cur)),
            "warning",
            Some(link("order", rid.clone())),
        );
    }

    let notes = fetch(
        session,
        &format!(
            "SELECT c.comment_ID, {}, c.comment_content, c.comment_author, \
             (SELECT meta_value FROM {cm} WHERE comment_id = c.comment_ID AND meta_key = 'is_customer_note' LIMIT 1) \
             FROM {} c WHERE c.comment_post_ID = {order_id} AND c.comment_type = 'order_note' ORDER BY c.comment_date_gmt, c.comment_ID",
            epoch("c.comment_date_gmt"),
            t(db, ctx, "comments"),
            cm = t(db, ctx, "commentmeta"),
        ),
    )
    .await?;
    let note_count = notes.rows.len() as u64;
    for r in &notes.rows {
        let customer_note = text(&r[4]).as_deref() == Some("1");
        let content = text(&r[2]).unwrap_or_default();
        let level = if content.to_lowercase().contains("fail")
            || content.to_lowercase().contains("error")
        {
            "error"
        } else {
            "info"
        };
        event(
            int(&r[1]),
            if customer_note {
                "customer_note"
            } else {
                "note"
            },
            content,
            text(&r[3])
                .filter(|a| a != "WooCommerce")
                .map(|a| format!("by {a}")),
            level,
            None,
        );
    }

    let mut action_count = 0;
    if ctx.has_table("actionscheduler_actions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT action_id, hook, status, {}, {}, args FROM {} WHERE args LIKE {} ORDER BY scheduled_date_gmt LIMIT 200",
                epoch("scheduled_date_gmt"),
                epoch("IF(last_attempt_gmt < '1000-01-01', NULL, last_attempt_gmt)"),
                t(db, ctx, "actionscheduler_actions"),
                sql_string(&format!("%{order_id}%"))
            ),
        )
        .await?;
        for r in rs
            .rows
            .iter()
            .filter(|r| text(&r[5]).is_some_and(|a| args_reference(&a, id)))
        {
            action_count += 1;
            let status = text(&r[2]).unwrap_or_default();
            let at = int(&r[4]).or(int(&r[3]));
            event(
                at,
                "action",
                text(&r[1]).unwrap_or_default(),
                Some(status.clone()),
                if status == "failed" { "error" } else { "info" },
                text(&r[0]).map(|a| link("action", a)),
            );
        }
    }

    timeline.sort_by_key(|e| e.at);

    // ---- relation strip ----
    let mut related = Vec::new();
    if let Some(c) = &customer {
        related.push(RelatedChip {
            label: c
                .label
                .clone()
                .unwrap_or_else(|| format!("Customer #{}", c.id)),
            link: Some(c.clone()),
            count: None,
            target: None,
        });
    }
    if let Some(p) = &parent {
        related.push(RelatedChip {
            label: format!("Parent #{p}"),
            link: Some(link("order", p.clone())),
            count: None,
            target: None,
        });
    }
    for item in items.iter().filter(|i| i.product.is_some()) {
        let p = item.product.clone().unwrap();
        related.push(RelatedChip {
            label: p.label.clone().unwrap_or_else(|| item.name.clone()),
            link: Some(p),
            count: None,
            target: None,
        });
    }
    for (rid, _, _) in &refunds {
        related.push(RelatedChip {
            label: format!("Refund #{rid}"),
            link: Some(link("order", rid.clone())),
            count: None,
            target: None,
        });
    }
    for s in &subscriptions {
        related.push(RelatedChip {
            label: format!("Subscription #{s}"),
            link: Some(link("order", s.clone())),
            count: None,
            target: None,
        });
    }
    if note_count > 0 {
        related.push(RelatedChip {
            label: "Notes".into(),
            link: None,
            count: Some(note_count),
            target: Some("timeline".into()),
        });
    }
    if action_count > 0 {
        related.push(RelatedChip {
            label: "Scheduled actions".into(),
            link: None,
            count: Some(action_count),
            target: Some("timeline".into()),
        });
    }

    Ok(OrderView {
        kind: pick(&doc, &["type", "post_type"]).unwrap_or_else(|| "shop_order".into()),
        status: pick(&doc, &["status", "post_status"]).unwrap_or_default(),
        total: total.as_deref().and_then(|v| money(v, cur)),
        currency,
        created,
        paid,
        completed,
        customer_email: billing
            .as_ref()
            .and_then(|b| b.email.clone())
            .or_else(|| pick(&doc, &["billing_email", "_billing_email"])),
        customer,
        parent: parent.map(|p| link("order", p)),
        billing,
        shipping,
        address_notes,
        items,
        totals,
        payment,
        attribution,
        timeline,
        related,
        doc,
    })
}

/// WooCommerce log lines that mention the order (by `#id`, the `order` context key, or its
/// PayPal order / transaction ID) within its lifetime. Separate from `order_view` because
/// it scans the log table, which can be large; the UI merges these into the timeline.
pub async fn order_logs(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    id: &str,
) -> AppResult<Vec<TimelineEvent>> {
    if !ctx.has_table("woocommerce_log") {
        return Ok(Vec::new());
    }
    let order_id: u64 = id
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
    let doc = load_docs(session, ctx, catalog, db, "order", &[id.to_string()])
        .await?
        .pop();
    let Some(doc) = doc else {
        return Ok(Vec::new());
    };
    let mut patterns = vec![
        format!("message LIKE {}", sql_string(&format!("%#{order_id}%"))),
        format!(
            "context LIKE {}",
            sql_string(&format!("%\"order\": {order_id}%"))
        ),
    ];
    for extra in [
        pick(&doc, &["_ppcp_paypal_order_id"]),
        pick(&doc, &["transaction_id", "_transaction_id"]),
    ]
    .into_iter()
    .flatten()
    {
        patterns.push(format!(
            "message LIKE {}",
            sql_string(&format!("%{extra}%"))
        ));
    }
    let window = match time_of(&doc, &["date_created_gmt", "post_date_gmt"]) {
        Some(created) => format!(
            " AND {} BETWEEN {} AND {}",
            epoch("timestamp"),
            created - 86_400,
            created + 90 * 86_400
        ),
        None => String::new(),
    };
    let rs = fetch(
        session,
        &format!(
            "SELECT log_id, {}, level, source, message FROM {} WHERE ({}){window} ORDER BY timestamp LIMIT 100",
            epoch("timestamp"),
            t(db, ctx, "woocommerce_log"),
            patterns.join(" OR ")
        ),
    )
    .await?;
    Ok(rs
        .rows
        .iter()
        .filter_map(|r| {
            let message = text(&r[4]).unwrap_or_default();
            let source = text(&r[3]);
            let summary = super::logs::humanize(source.as_deref(), &message);
            Some(TimelineEvent {
                at: int(&r[1])?,
                kind: "log",
                title: summary.title,
                detail: source,
                level: log_level(num(&r[2])),
                link: None,
                raw: Some(message),
                row: text(&r[0]).map(|id| RowRef {
                    table: ctx.table("woocommerce_log"),
                    id_column: "log_id".into(),
                    id,
                }),
            })
        })
        .collect())
}

// ---------- customer ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentToken {
    pub id: String,
    pub gateway: String,
    pub kind: String,
    pub is_default: bool,
    pub summary: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomerView {
    pub doc: EntityDoc,
    pub name: String,
    pub login: Option<String>,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub roles: Vec<String>,
    pub registered: Option<i64>,
    pub last_active: Option<i64>,
    pub billing: Option<Address>,
    pub shipping: Option<Address>,
    pub order_count: u64,
    pub total_spent: Option<String>,
    pub average_order: Option<String>,
    pub orders: Vec<OrderSummary>,
    pub subscriptions: Vec<OrderSummary>,
    pub tokens: Vec<PaymentToken>,
    pub session: Option<EntityLink>,
    pub related: Vec<RelatedChip>,
}

/// "VISA •••• 2503 · 01/2029" from token meta.
pub fn token_summary(kind: &str, meta: &HashMap<String, String>) -> String {
    let get = |k: &str| meta.get(k).filter(|v| !v.is_empty()).cloned();
    match (get("card_type"), get("last4")) {
        (brand, Some(last4)) => {
            let mut s = format!(
                "{} •••• {last4}",
                brand.unwrap_or_else(|| "Card".into()).to_uppercase()
            );
            if let (Some(m), Some(y)) = (get("expiry_month"), get("expiry_year")) {
                s += &format!(" · {m}/{y}");
            }
            s
        }
        _ => get("email")
            .or_else(|| get("payer_email"))
            .map_or_else(|| kind.to_string(), |e| format!("{kind} · {e}")),
    }
}

/// A guest checkout customer: everything we know comes from the orders placed with this email.
pub async fn guest_view(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    email: &str,
    with_gravatar: bool,
) -> AppResult<CustomerView> {
    let stats = fetch(
        session,
        &lists::guests_sql(db, ctx, None, Some(email), 1, 0),
    )
    .await?;
    let summary = lists::guest_rows(&stats, ctx.site.currency.as_deref())
        .pop()
        .ok_or_else(|| AppError::InvalidInput(format!("No guest orders for {email}")))?;
    let q = ListQuery {
        search: Some(email.to_string()),
        limit: 10,
        ..Default::default()
    };
    let orders: Vec<OrderSummary> = lists::orders(session, ctx, db, "shop_order", &q)
        .await?
        .items
        .into_iter()
        .filter(|o| o.email.as_deref() == Some(email) && o.customer_id.is_none())
        .collect();
    let session_link = if ctx.has_table("woocommerce_sessions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT session_id FROM {} WHERE session_value LIKE {} ORDER BY session_expiry DESC LIMIT 1",
                t(db, ctx, "woocommerce_sessions"),
                sql_string(&format!("%{email}%"))
            ),
        )
        .await?;
        rs.rows
            .first()
            .and_then(|r| text(&r[0]))
            .map(|s| link("session", s))
    } else {
        None
    };
    let average = (summary.order_count > 0)
        .then(|| {
            stats
                .rows
                .first()
                .and_then(|r| text(&r[2]))
                .and_then(|v| v.parse::<f64>().ok())
        })
        .flatten()
        .and_then(|spent| {
            money(
                &(spent / summary.order_count as f64).to_string(),
                ctx.site.currency.as_deref(),
            )
        });
    let name = summary.name.clone().unwrap_or_else(|| email.to_string());
    let mut related = vec![RelatedChip {
        label: "Orders".into(),
        link: None,
        count: Some(summary.order_count),
        target: Some(format!("list:orders:search={email}")),
    }];
    if let Some(s) = &session_link {
        related.push(RelatedChip {
            label: "Session".into(),
            link: Some(s.clone()),
            count: None,
            target: None,
        });
    }
    let billing = Address {
        name: summary.name.clone(),
        company: None,
        lines: Vec::new(),
        country: None,
        email: Some(email.to_string()),
        phone: None,
    };
    Ok(CustomerView {
        doc: EntityDoc {
            entity: "user".into(),
            label: "Guest".into(),
            id: format!("{}{email}", lists::GUEST_PREFIX),
            title: Some(name.clone()),
            subtitle: Some("guest checkout".into()),
            image: None,
            table: if ctx.hpos() {
                ctx.table("wc_orders")
            } else {
                ctx.table("postmeta")
            },
            id_column: if ctx.hpos() {
                "billing_email".into()
            } else {
                "meta_value".into()
            },
            fields: Vec::new(),
            meta: Vec::new(),
            meta_source: None,
            relations: Vec::new(),
        },
        name,
        login: None,
        avatar: with_gravatar.then(|| lists::gravatar(email)),
        email: Some(email.to_string()),
        roles: vec!["Guest".into()],
        registered: summary.registered,
        last_active: summary.last_order,
        billing: Some(billing),
        shipping: None,
        order_count: summary.order_count,
        total_spent: summary.total_spent,
        average_order: average,
        orders,
        subscriptions: Vec::new(),
        tokens: Vec::new(),
        session: session_link,
        related,
    })
}

pub async fn customer_view(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    id: &str,
    with_gravatar: bool,
) -> AppResult<CustomerView> {
    let doc = load_docs(session, ctx, catalog, db, "user", &[id.to_string()])
        .await?
        .pop()
        .ok_or_else(|| AppError::InvalidInput(format!("User #{id} was not found")))?;
    let user_id: u64 = id
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
    let email = pick(&doc, &["user_email"]);
    let roles = roles_of(doc.text(&format!("{}capabilities", ctx.prefix)));

    let (mut orders, mut subscriptions, mut order_count, mut total_spent, mut average_order) =
        (Vec::new(), Vec::new(), 0, None, None);
    if ctx.wc.is_some() {
        let q = ListQuery {
            search: None,
            limit: 10,
            ..Default::default()
        };
        orders = customer_orders(session, ctx, db, "shop_order", user_id, &q).await?;
        subscriptions = customer_orders(session, ctx, db, "shop_subscription", user_id, &q).await?;
        if let Some(sql) = lists::customer_stats_sql(db, ctx, &[id.to_string()]) {
            let rs = fetch(session, &sql).await?;
            if let Some(r) = rs.rows.first() {
                order_count = num(&r[1]).unwrap_or(0);
                let spent: f64 = text(&r[2]).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                total_spent = money(&spent.to_string(), ctx.site.currency.as_deref());
                if order_count > 0 {
                    average_order = money(
                        &(spent / order_count as f64).to_string(),
                        ctx.site.currency.as_deref(),
                    );
                }
            }
        }
    }

    let mut tokens = Vec::new();
    if ctx.has_table("woocommerce_payment_tokens") {
        let rs = fetch(
            session,
            &format!("SELECT token_id, gateway_id, type, is_default FROM {} WHERE user_id = {user_id} ORDER BY token_id", t(db, ctx, "woocommerce_payment_tokens")),
        )
        .await?;
        let ids: Vec<String> = rs.rows.iter().filter_map(|r| text(&r[0])).collect();
        let mut meta: HashMap<String, HashMap<String, String>> = HashMap::new();
        if !ids.is_empty() && ctx.has_table("woocommerce_payment_tokenmeta") {
            let m = fetch(
                session,
                &format!(
                    "SELECT payment_token_id, meta_key, meta_value FROM {} WHERE payment_token_id IN ({})",
                    t(db, ctx, "woocommerce_payment_tokenmeta"),
                    ids.join(", ")
                ),
            )
            .await?;
            for r in &m.rows {
                if let (Some(id), Some(k), Some(v)) = (text(&r[0]), text(&r[1]), text(&r[2])) {
                    meta.entry(id).or_default().insert(k, v);
                }
            }
        }
        tokens = rs
            .rows
            .iter()
            .map(|r| {
                let id = text(&r[0]).unwrap_or_default();
                let kind = text(&r[2]).unwrap_or_default();
                PaymentToken {
                    summary: token_summary(&kind, meta.get(&id).unwrap_or(&HashMap::new())),
                    gateway: text(&r[1]).unwrap_or_default(),
                    is_default: text(&r[3]).as_deref() == Some("1"),
                    kind,
                    id,
                }
            })
            .collect();
    }

    let session_link = if ctx.has_table("woocommerce_sessions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT session_id FROM {} WHERE session_key = {}",
                t(db, ctx, "woocommerce_sessions"),
                sql_string(id)
            ),
        )
        .await?;
        rs.rows
            .first()
            .and_then(|r| text(&r[0]))
            .map(|s| link("session", s))
    } else {
        None
    };

    let addr = |prefix: &str| {
        Address::from_parts(|k| doc.text(&format!("{prefix}_{k}")).map(String::from))
    };
    let name = pick(&doc, &["display_name"])
        .or_else(|| {
            let n = [doc.text("first_name"), doc.text("last_name")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            (!n.is_empty()).then_some(n)
        })
        .unwrap_or_else(|| format!("User #{id}"));

    let mut related = Vec::new();
    if order_count > 0 {
        related.push(RelatedChip {
            label: "Orders".into(),
            link: None,
            count: Some(order_count),
            target: Some(format!("list:orders:customer={id}")),
        });
    }
    if !subscriptions.is_empty() {
        related.push(RelatedChip {
            label: "Subscriptions".into(),
            link: None,
            count: Some(subscriptions.len() as u64),
            target: None,
        });
    }
    if !tokens.is_empty() {
        related.push(RelatedChip {
            label: "Payment tokens".into(),
            link: None,
            count: Some(tokens.len() as u64),
            target: None,
        });
    }
    if let Some(s) = &session_link {
        related.push(RelatedChip {
            label: "Active session".into(),
            link: Some(s.clone()),
            count: None,
            target: None,
        });
    }

    Ok(CustomerView {
        login: pick(&doc, &["user_login"]),
        avatar: email
            .as_deref()
            .filter(|_| with_gravatar)
            .map(lists::gravatar),
        email,
        roles,
        registered: time_of(&doc, &["user_registered"]),
        last_active: time_of(&doc, &["wc_last_active"]),
        billing: addr("billing"),
        shipping: addr("shipping"),
        order_count,
        total_spent,
        average_order,
        orders,
        subscriptions,
        tokens,
        session: session_link,
        related,
        name,
        doc,
    })
}

async fn customer_orders(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    order_type: &str,
    user_id: u64,
    q: &ListQuery,
) -> AppResult<Vec<OrderSummary>> {
    let sql = lists::orders_sql(db, ctx, order_type, q);
    // Narrow the generic list query to one customer.
    let sql = if ctx.hpos() {
        sql.replacen(
            "WHERE o.type",
            &format!("WHERE o.customer_id = {user_id} AND o.type"),
            1,
        )
    } else {
        sql.replacen(
            "WHERE p.post_type",
            &format!(
                "WHERE p.ID IN (SELECT post_id FROM {} WHERE meta_key = '_customer_user' AND meta_value = '{user_id}') AND p.post_type",
                t(db, ctx, "postmeta")
            ),
            1,
        )
    };
    let rs = fetch(session, &sql).await?;
    let mut rows = lists::order_rows(&rs, ctx.site.currency.as_deref());
    rows.truncate(q.limit.max(1) as usize);
    Ok(rows)
}

// ---------- product ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Variation {
    pub id: String,
    pub attributes: Vec<(String, String)>,
    pub sku: Option<String>,
    pub price: Option<String>,
    pub stock_status: Option<String>,
    pub stock: Option<String>,
    pub image: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProductView {
    pub doc: EntityDoc,
    pub name: String,
    pub status: String,
    pub kind: Option<String>,
    pub image: Option<String>,
    pub sku: Option<String>,
    pub price: Option<String>,
    pub regular_price: Option<String>,
    pub sale_price: Option<String>,
    pub stock_status: Option<String>,
    pub stock: Option<String>,
    pub total_sales: Option<String>,
    pub terms: Vec<(String, Vec<String>)>,
    pub attributes: Vec<(String, Vec<String>)>,
    pub variations: Vec<Variation>,
    pub related: Vec<RelatedChip>,
}

/// `(name, values)` from `_product_attributes`; taxonomy attributes are filled in later.
pub fn product_attributes(raw: &str) -> Vec<(String, String, bool)> {
    let Ok(PhpValue::Array(items)) = php::parse(raw.as_bytes()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|(_, v)| {
            let PhpValue::Array(a) = v else { return None };
            let name = php_str(php_entry(a, "name"))?;
            let value = php_str(php_entry(a, "value")).unwrap_or_default();
            let taxonomy = matches!(
                php_entry(a, "is_taxonomy"),
                Some(PhpValue::Int(1)) | Some(PhpValue::Bool(true))
            );
            Some((name, value, taxonomy))
        })
        .collect()
}

pub async fn product_view(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    id: &str,
) -> AppResult<ProductView> {
    let doc = load_docs(session, ctx, catalog, db, "post", &[id.to_string()])
        .await?
        .pop()
        .ok_or_else(|| AppError::InvalidInput(format!("Product #{id} was not found")))?;
    let pid: u64 = id
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
    let cur = ctx.site.currency.as_deref();
    let m = |k: &str| doc.text(k).map(String::from);

    let terms_rs = fetch(
        session,
        &format!(
            "SELECT tt.taxonomy, te.name FROM {tr} tr JOIN {tt} tt ON tt.term_taxonomy_id = tr.term_taxonomy_id \
             JOIN {terms} te ON te.term_id = tt.term_id WHERE tr.object_id = {pid} ORDER BY tt.taxonomy, te.name",
            tr = t(db, ctx, "term_relationships"),
            tt = t(db, ctx, "term_taxonomy"),
            terms = t(db, ctx, "terms"),
        ),
    )
    .await?;
    let mut by_tax: Vec<(String, Vec<String>)> = Vec::new();
    for r in &terms_rs.rows {
        let (Some(tax), Some(name)) = (text(&r[0]), text(&r[1])) else {
            continue;
        };
        match by_tax.iter_mut().find(|(t, _)| *t == tax) {
            Some((_, v)) => v.push(name),
            None => by_tax.push((tax, vec![name])),
        }
    }
    let kind = by_tax
        .iter()
        .find(|(t, _)| t == "product_type")
        .and_then(|(_, v)| v.first().cloned());
    let attributes = m("_product_attributes")
        .map(|raw| {
            product_attributes(&raw)
                .into_iter()
                .map(|(name, value, taxonomy)| {
                    let values = if taxonomy {
                        by_tax
                            .iter()
                            .find(|(t, _)| *t == name)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default()
                    } else {
                        value
                            .split('|')
                            .map(|v| v.trim().to_string())
                            .filter(|v| !v.is_empty())
                            .collect()
                    };
                    let label = name.strip_prefix("pa_").unwrap_or(&name).to_string();
                    (label, values)
                })
                .collect()
        })
        .unwrap_or_default();
    let terms: Vec<(String, Vec<String>)> = by_tax
        .into_iter()
        .filter(|(t, _)| matches!(t.as_str(), "product_cat" | "product_tag" | "product_brand"))
        .collect();

    // Variations with their attributes and stock in two queries.
    let vars = fetch(
        session,
        &format!(
            "SELECT ID FROM {} WHERE post_parent = {pid} AND post_type = 'product_variation' AND post_status <> 'trash' ORDER BY menu_order, ID",
            t(db, ctx, "posts")
        ),
    )
    .await?;
    let var_ids: Vec<String> = vars.rows.iter().filter_map(|r| text(&r[0])).collect();
    let mut variations = Vec::new();
    if !var_ids.is_empty() {
        let meta = fetch(
            session,
            &format!(
                "SELECT post_id, meta_key, meta_value FROM {} WHERE post_id IN ({}) AND (meta_key LIKE 'attribute\\_%' \
                 OR meta_key IN ('_sku', '_price', '_stock_status', '_stock'))",
                t(db, ctx, "postmeta"),
                var_ids.join(", ")
            ),
        )
        .await?;
        let mut by_id: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for r in &meta.rows {
            if let (Some(id), Some(k)) = (text(&r[0]), text(&r[1])) {
                by_id
                    .entry(id)
                    .or_default()
                    .push((k, text(&r[2]).unwrap_or_default()));
            }
        }
        let images = post_images(session, ctx, db, &var_ids).await?;
        for vid in &var_ids {
            let meta = by_id.remove(vid).unwrap_or_default();
            let get = |k: &str| {
                meta.iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
                    .filter(|v| !v.is_empty())
            };
            variations.push(Variation {
                attributes: meta
                    .iter()
                    .filter_map(|(k, v)| {
                        let name = k.strip_prefix("attribute_")?;
                        Some((
                            name.strip_prefix("pa_").unwrap_or(name).to_string(),
                            if v.is_empty() {
                                "Any".into()
                            } else {
                                v.clone()
                            },
                        ))
                    })
                    .collect(),
                sku: get("_sku"),
                price: get("_price").and_then(|p| money(&p, cur)),
                stock_status: get("_stock_status"),
                stock: get("_stock"),
                image: images.get(vid).cloned(),
                id: vid.clone(),
            });
        }
    }

    let mut related = Vec::new();
    if ctx.has_table("wc_order_product_lookup") {
        let rs = fetch(
            session,
            &format!(
                "SELECT COUNT(DISTINCT order_id) FROM {} WHERE product_id = {pid} OR variation_id = {pid}",
                t(db, ctx, "wc_order_product_lookup")
            ),
        )
        .await?;
        if let Some(n) = rs.rows.first().and_then(|r| num(&r[0])).filter(|n| *n > 0) {
            related.push(RelatedChip {
                label: "Orders".into(),
                link: None,
                count: Some(n),
                target: None,
            });
        }
    }
    if !variations.is_empty() {
        related.push(RelatedChip {
            label: "Variations".into(),
            link: None,
            count: Some(variations.len() as u64),
            target: None,
        });
    }

    Ok(ProductView {
        name: doc
            .title
            .clone()
            .unwrap_or_else(|| format!("Product #{id}")),
        status: pick(&doc, &["post_status"]).unwrap_or_default(),
        image: doc.image.clone(),
        sku: m("_sku"),
        price: m("_price").and_then(|p| money(&p, cur)),
        regular_price: m("_regular_price").and_then(|p| money(&p, cur)),
        sale_price: m("_sale_price").and_then(|p| money(&p, cur)),
        stock_status: m("_stock_status"),
        stock: m("_stock"),
        total_sales: m("total_sales"),
        kind,
        terms,
        attributes,
        variations,
        related,
        doc,
    })
}

// ---------- action & session ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub at: Option<i64>,
    pub message: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub doc: EntityDoc,
    pub hook: String,
    pub status: String,
    pub scheduled: Option<i64>,
    pub args: Option<value::DecodedValue>,
    pub group: Option<String>,
    pub logs: Vec<LogLine>,
}

pub async fn action_view(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    id: &str,
) -> AppResult<ActionView> {
    let doc = load_docs(session, ctx, catalog, db, "action", &[id.to_string()])
        .await?
        .pop()
        .ok_or_else(|| AppError::InvalidInput(format!("Action #{id} was not found")))?;
    let aid: u64 = id
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
    let logs = fetch(
        session,
        &format!(
            "SELECT {}, message FROM {} WHERE action_id = {aid} ORDER BY log_id",
            epoch("log_date_gmt"),
            t(db, ctx, "actionscheduler_logs")
        ),
    )
    .await?;
    let group = match pick(&doc, &["group_id"]).filter(|g| g != "0") {
        Some(g) => {
            let rs = fetch(
                session,
                &format!(
                    "SELECT slug FROM {} WHERE group_id = {}",
                    t(db, ctx, "actionscheduler_groups"),
                    sql_string(&g)
                ),
            )
            .await?;
            rs.rows.first().and_then(|r| text(&r[0]))
        }
        None => None,
    };
    Ok(ActionView {
        hook: pick(&doc, &["hook"]).unwrap_or_default(),
        status: pick(&doc, &["status"]).unwrap_or_default(),
        scheduled: time_of(&doc, &["scheduled_date_gmt"]),
        args: pick(&doc, &["args", "extended_args"]).and_then(|a| value::decode(&a)),
        group,
        logs: logs
            .rows
            .iter()
            .map(|r| LogLine {
                at: int(&r[0]),
                message: text(&r[1]).unwrap_or_default(),
            })
            .collect(),
        doc,
    })
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub doc: EntityDoc,
    pub key: String,
    pub expires: Option<i64>,
    pub customer: Option<EntityLink>,
    pub items: u64,
    pub total: Option<String>,
    pub email: Option<String>,
    pub value: Option<value::DecodedValue>,
}

pub async fn session_view(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    id: &str,
) -> AppResult<SessionView> {
    let doc = load_docs(session, ctx, catalog, db, "session", &[id.to_string()])
        .await?
        .pop()
        .ok_or_else(|| AppError::InvalidInput(format!("Session #{id} was not found")))?;
    let key = pick(&doc, &["session_key"]).unwrap_or_default();
    let raw = pick(&doc, &["session_value"]);
    let (items, total, email) = raw
        .as_deref()
        .map(|v| lists::session_summary(v, ctx.site.currency.as_deref()))
        .unwrap_or((0, None, None));
    let customer = match key.parse::<u64>() {
        Ok(uid) => {
            let mut l = link("user", uid.to_string());
            l.label = titles(
                session,
                ctx,
                catalog,
                db,
                "user",
                std::slice::from_ref(&l.id),
            )
            .await?
            .remove(&l.id);
            Some(l)
        }
        Err(_) => None,
    };
    Ok(SessionView {
        expires: doc
            .field("session_expiry")
            .and_then(|f| cell_str(&f.value)?.parse().ok()),
        value: raw.as_deref().and_then(value::decode),
        key,
        customer,
        items,
        total,
        email,
        doc,
    })
}

// ---------- dispatch ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "view", rename_all = "camelCase")]
pub enum EntityView {
    Order(Box<OrderView>),
    Customer(Box<CustomerView>),
    Product(Box<ProductView>),
    Action(Box<ActionView>),
    Session(Box<SessionView>),
    Generic(Box<EntityDoc>),
}

/// The best view for an entity: posts that are products or legacy orders get their
/// purpose-built layout.
pub async fn entity_view(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    entity: &str,
    id: &str,
    with_gravatar: bool,
) -> AppResult<EntityView> {
    Ok(match entity {
        "order" => EntityView::Order(Box::new(
            order_view(session, ctx, catalog, db, "order", id).await?,
        )),
        "user" if id.starts_with(lists::GUEST_PREFIX) => EntityView::Customer(Box::new(
            guest_view(
                session,
                ctx,
                db,
                &id[lists::GUEST_PREFIX.len()..],
                with_gravatar,
            )
            .await?,
        )),
        "user" => EntityView::Customer(Box::new(
            customer_view(session, ctx, catalog, db, id, with_gravatar).await?,
        )),
        "action" => EntityView::Action(Box::new(action_view(session, ctx, catalog, db, id).await?)),
        "session" => {
            EntityView::Session(Box::new(session_view(session, ctx, catalog, db, id).await?))
        }
        "post" => {
            let doc = load_docs(session, ctx, catalog, db, "post", &[id.to_string()])
                .await?
                .pop()
                .ok_or_else(|| AppError::InvalidInput(format!("Post #{id} was not found")))?;
            match doc.text("post_type") {
                Some("product") => EntityView::Product(Box::new(
                    product_view(session, ctx, catalog, db, id).await?,
                )),
                Some("product_variation") => match doc.text("post_parent").filter(|p| *p != "0") {
                    Some(parent) => EntityView::Product(Box::new(
                        product_view(session, ctx, catalog, db, parent).await?,
                    )),
                    None => EntityView::Generic(Box::new(doc)),
                },
                Some("shop_order" | "shop_order_refund" | "shop_subscription") if !ctx.hpos() => {
                    EntityView::Order(Box::new(
                        order_view(session, ctx, catalog, db, "order", id).await?,
                    ))
                }
                _ => EntityView::Generic(Box::new(doc)),
            }
        }
        _ => EntityView::Generic(Box::new(
            load_docs(session, ctx, catalog, db, entity, &[id.to_string()])
                .await?
                .pop()
                .ok_or_else(|| AppError::InvalidInput(format!("{entity} #{id} was not found")))?,
        )),
    })
}

// ---------- omnisearch ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
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
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    query: &str,
) -> AppResult<Vec<SearchHit>> {
    let q = query.trim().trim_start_matches('#');
    if q.len() < 2 {
        return Ok(Vec::new());
    }
    let exact = sql_string(q);
    let like = sql_string(&format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    ));
    let mut out = Vec::new();
    let posts = t(db, ctx, "posts");
    let users = t(db, ctx, "users");

    if let Ok(n) = q.parse::<u64>() {
        if ctx.hpos() {
            let rs = fetch(session, &format!("SELECT id, CONCAT('Order #', id), CONCAT(status, ' · ', COALESCE(billing_email, '')), 'id' FROM {} WHERE id = {n}", t(db, ctx, "wc_orders"))).await?;
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
                    t(db, ctx, "actionscheduler_actions")
                ),
            )
            .await?;
            out.extend(hits(&rs, "action", "action id"));
        }
    }

    if q.contains('@') {
        let rs = fetch(session, &format!("SELECT ID, display_name, user_email, 'email' FROM {users} WHERE user_email LIKE {like} LIMIT 8")).await?;
        out.extend(hits(&rs, "user", "email"));
        if ctx.wc.is_some() {
            let guests = if ctx.hpos() {
                format!(
                    "SELECT CONCAT('{prefix}', billing_email), billing_email, CONCAT('Guest · ', COUNT(*), ' orders'), 'billing_email' \
                     FROM {orders} WHERE type = 'shop_order' AND customer_id = 0 AND billing_email LIKE {like} GROUP BY billing_email LIMIT 5",
                    prefix = lists::GUEST_PREFIX,
                    orders = t(db, ctx, "wc_orders"),
                )
            } else {
                format!(
                    "SELECT CONCAT('{prefix}', e.meta_value), e.meta_value, CONCAT('Guest · ', COUNT(*), ' orders'), 'billing_email' \
                     FROM {posts} p JOIN {pm} e ON e.post_id = p.ID AND e.meta_key = '_billing_email' \
                     LEFT JOIN {pm} c ON c.post_id = p.ID AND c.meta_key = '_customer_user' \
                     WHERE p.post_type = 'shop_order' AND COALESCE(c.meta_value, '0') = '0' AND e.meta_value LIKE {like} GROUP BY e.meta_value LIMIT 5",
                    prefix = lists::GUEST_PREFIX,
                    posts = posts,
                    pm = t(db, ctx, "postmeta"),
                )
            };
            let rs = fetch(session, &guests).await?;
            out.extend(hits(&rs, "user", "billing_email"));
        }
        if ctx.hpos() {
            let rs = fetch(
                session,
                &format!("SELECT id, CONCAT('Order #', id), CONCAT(status, ' · ', billing_email), 'billing_email' FROM {} WHERE billing_email LIKE {like} ORDER BY id DESC LIMIT 8", t(db, ctx, "wc_orders")),
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
                &format!("SELECT id, CONCAT('Order #', id), CONCAT(status, ' · ', COALESCE(billing_email, '')), 'transaction_id' FROM {} WHERE transaction_id = {exact} LIMIT 5", t(db, ctx, "wc_orders")),
            )
            .await?;
            out.extend(hits(&rs, "order", "transaction_id"));
            let rs = fetch(
                session,
                &format!("SELECT order_id, CONCAT('Order #', order_id), NULL, meta_key FROM {} WHERE meta_value = {exact} LIMIT 10", t(db, ctx, "wc_orders_meta")),
            )
            .await?;
            out.extend(hits(&rs, "order", "meta"));
        }
        let rs = fetch(
            session,
            &format!(
                "SELECT p.ID, COALESCE(NULLIF(p.post_title, ''), CONCAT(p.post_type, ' #', p.ID)), p.post_type, m.meta_key FROM {pm} m \
                 JOIN {posts} p ON p.ID = m.post_id WHERE m.meta_value = {exact} AND p.post_type <> 'shop_order_placehold' LIMIT 10",
                pm = t(db, ctx, "postmeta")
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
                um = t(db, ctx, "usermeta")
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
                t(db, ctx, "wc_product_meta_lookup")
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
    let rs = fetch(session, &format!("SELECT option_id, option_name, autoload, 'option' FROM {} WHERE option_name LIKE {like} LIMIT 6", t(db, ctx, "options"))).await?;
    out.extend(hits(&rs, "option", "option"));

    let mut seen = std::collections::HashSet::new();
    out.retain(|h| seen.insert((h.entity.clone(), h.id.clone())));
    out.truncate(40);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_entities_in_item_meta_are_decoded() {
        assert_eq!(decode_entities("Mug &times; 1 &amp; Cup"), "Mug × 1 & Cup");
    }

    #[test]
    fn paypal_fee_breakdown_becomes_a_waterfall() {
        let raw = r#"a:3:{s:12:"gross_amount";a:2:{s:13:"currency_code";s:3:"USD";s:5:"value";s:5:"95.40";}s:10:"paypal_fee";a:2:{s:13:"currency_code";s:3:"USD";s:5:"value";s:4:"3.82";}s:10:"net_amount";a:2:{s:13:"currency_code";s:3:"USD";s:5:"value";s:5:"91.58";}}"#;
        let f = paypal_fees(raw).unwrap();
        assert_eq!(
            (f.gross.as_str(), f.fee.as_str(), f.net.as_str()),
            ("$95.40", "$3.82", "$91.58")
        );
        assert!((f.fee_ratio - 0.04).abs() < 0.001);
        assert_eq!(paypal_fees("nonsense"), None);
    }

    #[test]
    fn address_mismatches_are_reported() {
        let b = Address {
            name: Some("Daniel Turner".into()),
            lines: vec!["123 Main St".into(), "12345 Uptown CA".into()],
            country: Some("US".into()),
            ..Default::default()
        };
        let s = Address {
            name: Some("Daniel Turner".into()),
            lines: vec!["Marktstraße 1".into(), "6850 Dornbirn".into()],
            country: Some("AT".into()),
            ..Default::default()
        };
        let notes = address_mismatches(&b, &s);
        assert!(
            notes
                .iter()
                .any(|n| n.contains("Country differs: billing US, shipping AT"))
        );
        assert!(notes.iter().any(|n| n.contains("City/postcode")));
        assert!(address_mismatches(&b, &b).is_empty());
    }

    #[test]
    fn addresses_join_name_and_city_lines() {
        let a = Address::from_parts(|k| match k {
            "first_name" => Some("Jay".into()),
            "last_name" => Some("Dee".into()),
            "address_1" => Some("123 Main St".into()),
            "city" => Some("Uptown".into()),
            "state" => Some("CA".into()),
            "postcode" => Some("12345".into()),
            "country" => Some("US".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(a.name.as_deref(), Some("Jay Dee"));
        assert_eq!(a.lines, ["123 Main St", "12345 Uptown CA"]);
        assert_eq!(Address::from_parts(|_| None), None);
    }

    #[test]
    fn action_args_must_reference_the_exact_id() {
        assert!(args_reference("[3088,false]", "3088"));
        assert!(args_reference(r#"{"order_id":"3088"}"#, "3088"));
        assert!(!args_reference("[13088]", "3088"));
        assert!(!args_reference("not json 3088", "3088"));
    }

    #[test]
    fn tokens_summarize_cards_and_wallets() {
        let card = HashMap::from([
            ("card_type".to_string(), "visa".to_string()),
            ("last4".to_string(), "2503".to_string()),
            ("expiry_month".to_string(), "01".to_string()),
            ("expiry_year".to_string(), "2029".to_string()),
        ]);
        assert_eq!(token_summary("CC", &card), "VISA •••• 2503 · 01/2029");
        let paypal = HashMap::from([("email".to_string(), "jay@paypal.test".to_string())]);
        assert_eq!(token_summary("PayPal", &paypal), "PayPal · jay@paypal.test");
        assert_eq!(token_summary("ApplePay", &HashMap::new()), "ApplePay");
    }

    #[test]
    fn product_attributes_split_custom_values() {
        let raw = r#"a:2:{s:4:"size";a:3:{s:4:"name";s:4:"Size";s:5:"value";s:17:"14 inch | 16 inch";s:11:"is_taxonomy";i:0;}s:8:"pa_color";a:3:{s:4:"name";s:8:"pa_color";s:5:"value";s:0:"";s:11:"is_taxonomy";i:1;}}"#;
        assert_eq!(
            product_attributes(raw),
            [
                ("Size".to_string(), "14 inch | 16 inch".to_string(), false),
                ("pa_color".to_string(), String::new(), true)
            ]
        );
    }
}
