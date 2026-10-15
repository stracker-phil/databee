//! Hover peek: 3–5 key facts about an entity, derived from its document so HPOS and legacy
//! storage need no special cases. Also "find everywhere": one value looked up across options,
//! every meta table, Action Scheduler args and the WooCommerce log.

use serde::Serialize;

use super::context::WpContext;
use super::entity::{Catalog, EntityDoc, cell_str};
use super::hints::Hint;
use super::lists::{num, text};
use super::{fetch, load_docs, quote_ident, sql_string};
use crate::db::mysql::MySqlSession;
use crate::error::AppResult;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeekFact {
    pub label: String,
    pub value: String,
    /// text, status, money, time (value is a unix timestamp)
    pub kind: &'static str,
}

/// (label, candidate field or meta names)
type FactSpec = (&'static str, &'static [&'static str]);

/// Which fields make a good summary, per entity.
const FACTS: &[(&str, &[FactSpec])] = &[
    (
        "order",
        &[
            ("Status", &["status", "post_status"]),
            ("Total", &["total_amount", "_order_total"]),
            (
                "Gateway",
                &[
                    "payment_method_title",
                    "_payment_method_title",
                    "payment_method",
                ],
            ),
            ("Customer", &["billing_email", "_billing_email"]),
            ("Created", &["date_created_gmt", "post_date_gmt"]),
        ],
    ),
    (
        "user",
        &[
            ("Name", &["display_name"]),
            ("Email", &["user_email"]),
            ("Login", &["user_login"]),
            ("Registered", &["user_registered"]),
        ],
    ),
    (
        "post",
        &[
            ("Title", &["post_title"]),
            ("Type", &["post_type"]),
            ("Status", &["post_status"]),
            ("Price", &["_price"]),
            ("Modified", &["post_modified_gmt"]),
        ],
    ),
    (
        "action",
        &[
            ("Hook", &["hook"]),
            ("Status", &["status"]),
            ("Scheduled", &["scheduled_date_gmt"]),
            ("Attempts", &["attempts"]),
        ],
    ),
    (
        "session",
        &[("Key", &["session_key"]), ("Expires", &["session_expiry"])],
    ),
];

fn fact_from(doc: &EntityDoc, label: &str, names: &[&str]) -> Option<PeekFact> {
    let field = doc
        .fields
        .iter()
        .chain(doc.meta.iter().flat_map(|g| g.entries.iter()))
        .find(|f| names.contains(&f.name.as_str()))?;
    let raw = cell_str(&field.value)?.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    let (value, kind) = match &field.hint {
        Some(Hint::Money { text }) => (text.clone(), "money"),
        Some(Hint::Time { unix, .. }) => (unix.to_string(), "time"),
        _ if label == "Status" => (raw, "status"),
        _ => (raw, "text"),
    };
    Some(PeekFact {
        label: label.into(),
        value,
        kind,
    })
}

pub async fn peek(
    session: &MySqlSession,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    entity: &str,
    id: &str,
) -> AppResult<Vec<PeekFact>> {
    let Some(doc) = load_docs(session, ctx, catalog, db, entity, &[id.to_string()])
        .await?
        .pop()
    else {
        return Ok(Vec::new());
    };
    let specs = FACTS
        .iter()
        .find(|(e, _)| *e == entity)
        .map(|(_, s)| *s)
        .unwrap_or(&[]);
    let mut facts: Vec<PeekFact> = specs
        .iter()
        .filter_map(|(label, names)| fact_from(&doc, label, names))
        .collect();
    if facts.is_empty()
        && let Some(title) = &doc.title
    {
        facts.push(PeekFact {
            label: doc.label.clone(),
            value: title.clone(),
            kind: "text",
        });
    }
    if entity == "user" && ctx.hpos() {
        let rs = fetch(
            session,
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(total_amount), 0) FROM {}.{} WHERE type = 'shop_order' AND customer_id = {} AND status IN ('wc-completed', 'wc-processing')",
                quote_ident(db),
                quote_ident(&ctx.table("wc_orders")),
                id.parse::<u64>().unwrap_or(0)
            ),
        )
        .await?;
        if let Some(r) = rs.rows.first() {
            let n = num(&r[0]).unwrap_or(0);
            facts.push(PeekFact {
                label: "Orders".into(),
                value: format!("{n}"),
                kind: "text",
            });
            if let Some(spent) =
                text(&r[1]).and_then(|s| super::hints::money(&s, ctx.site.currency.as_deref()))
            {
                facts.push(PeekFact {
                    label: "Spent".into(),
                    value: spent,
                    kind: "money",
                });
            }
        }
    }
    facts.truncate(5);
    Ok(facts)
}

// ---------- find everywhere ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindHit {
    /// Row key in `table`.
    pub row_id: String,
    pub label: String,
    /// Entity the row belongs to (post, user, order, action), when there is one.
    pub entity: Option<String>,
    pub entity_id: Option<String>,
    /// Column that matched, e.g. `meta_value`.
    pub column: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindGroup {
    /// Human source name, e.g. "Post meta".
    pub source: String,
    pub table: String,
    pub id_column: String,
    pub count: u64,
    pub hits: Vec<FindHit>,
}

const HIT_LIMIT: usize = 50;

struct Source {
    label: &'static str,
    table: &'static str,
    id_column: &'static str,
    /// SQL selecting (id, label expression, entity id or NULL, matched column name) rows.
    columns: &'static str,
    entity: Option<&'static str>,
    /// `=` for exact matches, `LIKE` for containment (args, log messages).
    like: bool,
    match_columns: &'static [&'static str],
}

const SOURCES: &[Source] = &[
    Source {
        label: "Options",
        table: "options",
        id_column: "option_id",
        columns: "option_id, option_name, NULL",
        entity: Some("option"),
        like: false,
        match_columns: &["option_value", "option_name"],
    },
    Source {
        label: "Post meta",
        table: "postmeta",
        id_column: "meta_id",
        columns: "meta_id, CONCAT('post #', post_id, ' · ', meta_key), post_id",
        entity: Some("post"),
        like: false,
        match_columns: &["meta_value"],
    },
    Source {
        label: "User meta",
        table: "usermeta",
        id_column: "umeta_id",
        columns: "umeta_id, CONCAT('user #', user_id, ' · ', meta_key), user_id",
        entity: Some("user"),
        like: false,
        match_columns: &["meta_value"],
    },
    Source {
        label: "Term meta",
        table: "termmeta",
        id_column: "meta_id",
        columns: "meta_id, CONCAT('term #', term_id, ' · ', meta_key), term_id",
        entity: None,
        like: false,
        match_columns: &["meta_value"],
    },
    Source {
        label: "Comment meta",
        table: "commentmeta",
        id_column: "meta_id",
        columns: "meta_id, CONCAT('comment #', comment_id, ' · ', meta_key), comment_id",
        entity: None,
        like: false,
        match_columns: &["meta_value"],
    },
    Source {
        label: "Order meta",
        table: "wc_orders_meta",
        id_column: "id",
        columns: "id, CONCAT('order #', order_id, ' · ', meta_key), order_id",
        entity: Some("order"),
        like: false,
        match_columns: &["meta_value"],
    },
    Source {
        label: "Order item meta",
        table: "woocommerce_order_itemmeta",
        id_column: "meta_id",
        columns: "meta_id, CONCAT('item #', order_item_id, ' · ', meta_key), order_item_id",
        entity: None,
        like: false,
        match_columns: &["meta_value"],
    },
    Source {
        label: "Orders",
        table: "wc_orders",
        id_column: "id",
        columns: "id, CONCAT('order #', id, ' · ', status), id",
        entity: Some("order"),
        like: false,
        match_columns: &["transaction_id", "billing_email"],
    },
    Source {
        label: "Scheduled actions",
        table: "actionscheduler_actions",
        id_column: "action_id",
        columns: "action_id, CONCAT(hook, ' · ', status), action_id",
        entity: Some("action"),
        like: true,
        match_columns: &["args"],
    },
    Source {
        label: "WooCommerce log",
        table: "woocommerce_log",
        id_column: "log_id",
        columns: "log_id, CONCAT(source, ' · ', LEFT(message, 100)), NULL",
        entity: None,
        like: true,
        match_columns: &["message", "context"],
    },
];

fn escaped_like(value: &str) -> String {
    sql_string(&format!(
        "%{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    ))
}

pub async fn find_value(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    value: &str,
) -> AppResult<Vec<FindGroup>> {
    let value = value.trim();
    if value.len() < 2 {
        return Ok(Vec::new());
    }
    let mut groups = Vec::new();
    for src in SOURCES {
        if !ctx.has_table(src.table) {
            continue;
        }
        let table = format!("{}.{}", quote_ident(db), quote_ident(&ctx.table(src.table)));
        let conds: Vec<String> = src
            .match_columns
            .iter()
            .map(|c| {
                if src.like {
                    format!("{} LIKE {}", quote_ident(c), escaped_like(value))
                } else {
                    format!("{} = {}", quote_ident(c), sql_string(value))
                }
            })
            .collect();
        let filter = conds.join(" OR ");
        let count = fetch(
            session,
            &format!("SELECT COUNT(*) FROM {table} WHERE {filter}"),
        )
        .await?;
        let count = count.rows.first().and_then(|r| num(&r[0])).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let matched: Vec<String> = src
            .match_columns
            .iter()
            .map(|c| {
                let cond = if src.like {
                    format!("{} LIKE {}", quote_ident(c), escaped_like(value))
                } else {
                    format!("{} = {}", quote_ident(c), sql_string(value))
                };
                format!("WHEN {cond} THEN '{c}'")
            })
            .collect();
        let rs = fetch(
            session,
            &format!(
                "SELECT {cols}, CASE {matched} END FROM {table} WHERE {filter} ORDER BY {id} DESC LIMIT {limit}",
                cols = src.columns,
                matched = matched.join(" "),
                id = quote_ident(src.id_column),
                limit = HIT_LIMIT
            ),
        )
        .await?;
        let hits = rs
            .rows
            .iter()
            .filter_map(|r| {
                let row_id = text(&r[0])?;
                let label = text(&r[1]).unwrap_or_else(|| row_id.clone());
                let (entity, entity_id) = match (src.entity, text(&r[2])) {
                    (Some("option"), _) => (Some("option".to_string()), Some(label.clone())),
                    (Some(e), Some(id)) => (Some(e.to_string()), Some(id)),
                    _ => (None, None),
                };
                Some(FindHit {
                    row_id,
                    label,
                    entity,
                    entity_id,
                    column: text(&r[3]).unwrap_or_default(),
                })
            })
            .collect();
        groups.push(FindGroup {
            source: src.label.to_string(),
            table: ctx.table(src.table),
            id_column: src.id_column.to_string(),
            count,
            hits,
        });
    }
    Ok(groups)
}
