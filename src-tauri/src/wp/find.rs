//! Find everywhere: one value looked up across options, every meta table, Action Scheduler
//! args and the WooCommerce log.

use serde::Serialize;
use ts_rs::TS;

use super::fetch;
use super::rows::{self, text};
use super::site::WpContext;
use crate::db::Database;
use crate::error::AppResult;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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

pub async fn find_value(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    value: &str,
) -> AppResult<Vec<FindGroup>> {
    let dialect = session.dialect();
    let value = value.trim();
    if value.len() < 2 {
        return Ok(Vec::new());
    }
    let mut groups = Vec::new();
    for src in SOURCES {
        if !ctx.has_table(src.table) {
            continue;
        }
        let table = format!(
            "{}.{}",
            dialect.quote_ident(db),
            dialect.quote_ident(&ctx.table(src.table))
        );
        let conds: Vec<String> = src
            .match_columns
            .iter()
            .map(|c| {
                if src.like {
                    format!(
                        "{} LIKE {}",
                        dialect.quote_ident(c),
                        dialect.like_contains(value)
                    )
                } else {
                    format!("{} = {}", dialect.quote_ident(c), dialect.sql_string(value))
                }
            })
            .collect();
        let filter = conds.join(" OR ");
        let count = fetch(
            session,
            &format!("SELECT COUNT(*) FROM {table} WHERE {filter}"),
        )
        .await?;
        let count = count
            .rows
            .first()
            .and_then(|r| rows::count(&r[0]))
            .unwrap_or(0);
        if count == 0 {
            continue;
        }
        let matched: Vec<String> = src
            .match_columns
            .iter()
            .map(|c| {
                let cond = if src.like {
                    format!(
                        "{} LIKE {}",
                        dialect.quote_ident(c),
                        dialect.like_contains(value)
                    )
                } else {
                    format!("{} = {}", dialect.quote_ident(c), dialect.sql_string(value))
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
                id = dialect.quote_ident(src.id_column),
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
