//! WordPress edits as reviewable SQL, planned into the session's plans like row edits
//! (`db::changes`): each planner fetches the current state, then a pure builder turns it into
//! a `Plan` with the statements that undo it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::catalog::{Catalog, MetaDef};
use super::fetch;
use super::rows::{self, cell_str, text};
use super::site::WpContext;
use super::sql::table_ref;
use crate::db::changes::{Plan, reinsert_note, reinsert_sql, restore_undo, sql_value};
use crate::db::{Database, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WpChangeRequest {
    /// WordPress meta of an entity (insert or update).
    #[serde(rename_all = "camelCase")]
    SetMeta {
        database: String,
        entity: String,
        id: String,
        key: String,
        value: String,
    },
    #[serde(rename_all = "camelCase")]
    DeleteMeta {
        database: String,
        entity: String,
        id: String,
        key: String,
    },
    #[serde(rename_all = "camelCase")]
    SetOrderStatus {
        database: String,
        id: String,
        status: String,
    },
    #[serde(rename_all = "camelCase")]
    SetAutoload {
        database: String,
        option: String,
        on: bool,
    },
    #[serde(rename_all = "camelCase")]
    DeleteExpiredTransients { database: String },
    /// Retention for `woocommerce_log`: drop entries older than `days` (optionally one source).
    #[serde(rename_all = "camelCase")]
    DeleteLogsBefore {
        database: String,
        days: u32,
        #[serde(default)]
        #[ts(optional = nullable)]
        source: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    RetryAction { database: String, id: String },
    #[serde(rename_all = "camelCase")]
    CancelAction { database: String, id: String },
}

impl WpChangeRequest {
    pub fn database(&self) -> &str {
        match self {
            WpChangeRequest::SetMeta { database, .. }
            | WpChangeRequest::DeleteMeta { database, .. }
            | WpChangeRequest::SetOrderStatus { database, .. }
            | WpChangeRequest::SetAutoload { database, .. }
            | WpChangeRequest::DeleteExpiredTransients { database }
            | WpChangeRequest::DeleteLogsBefore { database, .. }
            | WpChangeRequest::RetryAction { database, .. }
            | WpChangeRequest::CancelAction { database, .. } => database,
        }
    }
}

/// `cutoff` is a literal fixed at plan time, so the applied DELETE removes exactly the
/// entries that were counted.
pub fn logs_before_filter(dialect: Dialect, cutoff: &str, source: Option<&str>) -> String {
    let mut f = format!("`timestamp` < {}", dialect.sql_string(cutoff));
    if let Some(s) = source {
        f.push_str(&format!(" AND `source` = {}", dialect.sql_string(s)));
    }
    f
}

fn first_text(rs: &ResultSet) -> Option<Option<String>> {
    rs.rows.first().map(|r| text(&r[0]))
}

fn parse_id(id: &str) -> AppResult<u64> {
    id.parse()
        .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))
}

/// Fixed at plan time, so the review shows exactly the rows the DELETE removes.
async fn log_cutoff(session: &dyn Database, days: u32) -> AppResult<String> {
    let rs = fetch(
        session,
        &format!("SELECT UTC_TIMESTAMP() - INTERVAL {days} DAY"),
    )
    .await?;
    first_text(&rs).flatten().ok_or_else(|| AppError::Database {
        message: "The server returned no time for UTC_TIMESTAMP()".into(),
        code: None,
    })
}

/// Where an entity's meta is stored: the qualified table and its column names.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaTarget {
    pub table: String,
    pub fk: String,
    pub key: String,
    pub value: String,
}

/// One meta key of one entity, as named in the request.
#[derive(Debug, Clone, Copy)]
pub struct MetaKey<'a> {
    pub entity: &'a str,
    pub id: &'a str,
    pub key: &'a str,
}

impl MetaTarget {
    fn new(table: String, meta: &MetaDef) -> Self {
        Self {
            table,
            fk: meta.fk.clone(),
            key: meta.key.clone(),
            value: meta.value.clone(),
        }
    }

    fn filter(&self, dialect: Dialect, meta: MetaKey) -> String {
        format!(
            "{} = {} AND {} = {}",
            dialect.quote_ident(&self.fk),
            dialect.sql_string(meta.id),
            dialect.quote_ident(&self.key),
            dialect.sql_string(meta.key)
        )
    }
}

pub fn meta_target(
    dialect: Dialect,
    catalog: &Catalog,
    database: &str,
    entity: &str,
) -> AppResult<MetaTarget> {
    let meta = catalog
        .get(entity)?
        .meta
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput(format!("{entity} has no meta")))?;
    Ok(MetaTarget::new(
        dialect.table_ref(database, &meta.table),
        meta,
    ))
}

pub async fn plan(
    session: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    req: &WpChangeRequest,
) -> AppResult<Plan> {
    let dialect = session.dialect();
    match req {
        WpChangeRequest::SetMeta {
            database,
            entity,
            id,
            key,
            value,
        } => {
            let target = meta_target(dialect, catalog, database, entity)?;
            let meta = MetaKey { entity, id, key };
            let sql = format!(
                "SELECT {} FROM {} WHERE {}",
                dialect.quote_ident(&target.value),
                target.table,
                target.filter(dialect, meta)
            );
            let old = fetch(session, &sql).await?;
            build_set_meta(dialect, database, &target, meta, value, &old)
        }
        WpChangeRequest::DeleteMeta {
            database,
            entity,
            id,
            key,
        } => {
            let target = meta_target(dialect, catalog, database, entity)?;
            let meta = MetaKey { entity, id, key };
            let sql = format!(
                "SELECT * FROM {} WHERE {}",
                target.table,
                target.filter(dialect, meta)
            );
            let rows = fetch(session, &sql).await?;
            build_delete_meta(dialect, database, &target, meta, &rows)
        }
        WpChangeRequest::SetOrderStatus {
            database,
            id,
            status,
        } => plan_set_order_status(session, dialect, ctx, database, id, status).await,
        WpChangeRequest::SetAutoload {
            database,
            option,
            on,
        } => {
            let t = table_ref(dialect, database, ctx, "options");
            let sql = format!(
                "SELECT autoload FROM {t} WHERE option_name = {}",
                dialect.sql_string(option)
            );
            let before = first_text(&fetch(session, &sql).await?)
                .flatten()
                .ok_or_else(|| AppError::InvalidInput(format!("Option {option} not found")))?;
            Ok(build_set_autoload(
                dialect, database, &t, option, *on, &before,
            ))
        }
        WpChangeRequest::DeleteExpiredTransients { database } => {
            let t = table_ref(dialect, database, ctx, "options");
            let overview = super::options_overview(session, ctx, database).await?;
            let names = expired_transient_rows(&overview);
            if names.is_empty() {
                return Err(AppError::InvalidInput(
                    "There are no expired transients".into(),
                ));
            }
            let filter = option_names_filter(dialect, &names);
            let rows = fetch(session, &format!("SELECT * FROM {t} WHERE {filter}")).await?;
            Ok(build_delete_expired_transients(
                dialect, database, &t, &names, &rows,
            ))
        }
        WpChangeRequest::DeleteLogsBefore {
            database,
            days,
            source,
        } => {
            if !ctx.has_table("woocommerce_log") {
                return Err(AppError::InvalidInput(
                    "This site has no woocommerce_log table".into(),
                ));
            }
            let t = table_ref(dialect, database, ctx, "woocommerce_log");
            let cutoff = log_cutoff(session, *days).await?;
            let filter = logs_before_filter(dialect, &cutoff, source.as_deref());
            let count = fetch(session, &format!("SELECT COUNT(*) FROM {t} WHERE {filter}")).await?;
            let n = count
                .rows
                .first()
                .and_then(|r| rows::count(&r[0]))
                .unwrap_or(0);
            build_delete_logs_before(database, &t, &filter, *days, source.as_deref(), n)
        }
        WpChangeRequest::RetryAction { database, id } => {
            plan_action(session, dialect, ctx, database, id, true).await
        }
        WpChangeRequest::CancelAction { database, id } => {
            plan_action(session, dialect, ctx, database, id, false).await
        }
    }
}

async fn plan_action(
    session: &dyn Database,
    dialect: Dialect,
    ctx: &WpContext,
    database: &str,
    id: &str,
    retry: bool,
) -> AppResult<Plan> {
    let n = parse_id(id)?;
    let t = table_ref(dialect, database, ctx, "actionscheduler_actions");
    let sql = format!("SELECT status, scheduled_date_gmt FROM {t} WHERE action_id = {n}");
    let rows = fetch(session, &sql).await?;
    let row = rows
        .rows
        .first()
        .ok_or_else(|| AppError::InvalidInput(format!("Action #{id} was not found")))?;
    let before = (cell_str(&row[0]), cell_str(&row[1]));
    Ok(build_action(dialect, database, &t, n, retry, before))
}

/// `old` holds the meta value of every row stored under the key for the entity.
pub fn build_set_meta(
    dialect: Dialect,
    database: &str,
    target: &MetaTarget,
    meta: MetaKey,
    value: &str,
    old: &ResultSet,
) -> AppResult<Plan> {
    let MetaKey { entity, id, key } = meta;
    let t = &target.table;
    let filter = target.filter(dialect, meta);
    let value_column = dialect.quote_ident(&target.value);
    let (description, statements, undo) = match old.rows.len() {
        0 => (
            format!("Add meta {key} to {entity} #{id}"),
            vec![format!(
                "INSERT INTO {t} ({}, {}, {value_column}) VALUES ({}, {}, {})",
                dialect.quote_ident(&target.fk),
                dialect.quote_ident(&target.key),
                dialect.sql_string(id),
                dialect.sql_string(key),
                dialect.sql_string(value)
            )],
            vec![format!("DELETE FROM {t} WHERE {filter}")],
        ),
        1 => {
            let (undo, note) =
                restore_undo(dialect, t, &target.value, &old.rows[0][0], Some(&filter));
            (
                format!("Meta {key} of {entity} #{id}{note}"),
                vec![format!(
                    "UPDATE {t} SET {value_column} = {} WHERE {filter}",
                    dialect.sql_string(value)
                )],
                undo,
            )
        }
        // One undo value can't restore rows that held different values.
        n => {
            return Err(AppError::InvalidInput(format!(
                "{entity} #{id} has {n} rows of meta {key}; edit them in the Database view"
            )));
        }
    };
    Ok(Plan {
        database: database.to_string(),
        description,
        statements,
        undo,
    })
}

/// `rows` holds every stored row of the meta key, all columns.
pub fn build_delete_meta(
    dialect: Dialect,
    database: &str,
    target: &MetaTarget,
    meta: MetaKey,
    rows: &ResultSet,
) -> AppResult<Plan> {
    let MetaKey { entity, id, key } = meta;
    if rows.rows.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "{entity} #{id} has no meta {key}"
        )));
    }
    let t = &target.table;
    Ok(Plan {
        database: database.to_string(),
        description: format!("Delete meta {key} of {entity} #{id}{}", reinsert_note(rows)),
        statements: vec![format!(
            "DELETE FROM {t} WHERE {}",
            target.filter(dialect, meta)
        )],
        undo: reinsert_sql(dialect, t, rows, &[]),
    })
}

async fn plan_set_order_status(
    session: &dyn Database,
    dialect: Dialect,
    ctx: &WpContext,
    database: &str,
    id: &str,
    status: &str,
) -> AppResult<Plan> {
    let n = parse_id(id)?;
    let posts = table_ref(dialect, database, ctx, "posts");
    let post_status = fetch(
        session,
        &format!(
            "SELECT post_status FROM {posts} WHERE ID = {n} AND post_type LIKE {}",
            dialect.like(r"shop\_%")
        ),
    )
    .await?;
    let hpos = if ctx.hpos() {
        let orders = table_ref(dialect, database, ctx, "wc_orders");
        let old = fetch(
            session,
            &format!("SELECT status FROM {orders} WHERE id = {n}"),
        )
        .await?;
        let before = first_text(&old)
            .flatten()
            .ok_or_else(|| AppError::InvalidInput(format!("Order #{id} was not found")))?;
        Some(OrderRow {
            table: orders,
            before,
        })
    } else {
        None
    };
    // Legacy store, or the synced posts copy (placeholders keep their own status).
    let post = first_text(&post_status)
        .flatten()
        .filter(|s| s != "draft")
        .map(|before| OrderRow {
            table: posts,
            before,
        });
    build_set_order_status(dialect, database, n, status, hpos, post)
}

/// An order's table and its status before the change.
pub struct OrderRow {
    pub table: String,
    pub before: String,
}

/// `hpos` is the `wc_orders` row, `post` the `posts` row; either may be missing.
pub fn build_set_order_status(
    dialect: Dialect,
    database: &str,
    id: u64,
    status: &str,
    hpos: Option<OrderRow>,
    post: Option<OrderRow>,
) -> AppResult<Plan> {
    let status = if status.starts_with("wc-") || status == "trash" {
        status.to_string()
    } else {
        format!("wc-{status}")
    };
    let mut statements = Vec::new();
    let mut undo = Vec::new();
    if let Some(OrderRow { table, before }) = hpos {
        statements.push(format!(
            "UPDATE {table} SET status = {}, date_updated_gmt = UTC_TIMESTAMP() WHERE id = {id}",
            dialect.sql_string(&status)
        ));
        undo.push(format!(
            "UPDATE {table} SET status = {} WHERE id = {id}",
            dialect.sql_string(&before)
        ));
    }
    if let Some(OrderRow { table, before }) = post {
        statements.push(format!(
            "UPDATE {table} SET post_status = {}, post_modified_gmt = UTC_TIMESTAMP(), post_modified = NOW() WHERE ID = {id}",
            dialect.sql_string(&status)
        ));
        undo.push(format!(
            "UPDATE {table} SET post_status = {} WHERE ID = {id}",
            dialect.sql_string(&before)
        ));
    }
    if statements.is_empty() {
        return Err(AppError::InvalidInput(format!("Order #{id} was not found")));
    }
    Ok(Plan {
        database: database.to_string(),
        description: format!("Order #{id} status → {}", status.trim_start_matches("wc-")),
        statements,
        undo,
    })
}

/// `before` is the stored autoload value; its style (`yes`/`no` or the WordPress 6.6
/// `on`/`off` family) decides the new value's spelling.
pub fn build_set_autoload(
    dialect: Dialect,
    database: &str,
    table: &str,
    option: &str,
    on: bool,
    before: &str,
) -> Plan {
    let modern = !matches!(before, "yes" | "no");
    let value = match (modern, on) {
        (true, true) => "on",
        (true, false) => "off",
        (false, true) => "yes",
        (false, false) => "no",
    };
    let name = dialect.sql_string(option);
    Plan {
        database: database.to_string(),
        description: format!("Autoload of {option} → {value}"),
        statements: vec![format!(
            "UPDATE {table} SET autoload = '{value}' WHERE option_name = {name}"
        )],
        undo: vec![format!(
            "UPDATE {table} SET autoload = {} WHERE option_name = {name}",
            dialect.sql_string(before)
        )],
    }
}

/// The option names of every expired transient: its value row and its timeout row.
fn expired_transient_rows(overview: &super::options::OptionsOverview) -> Vec<String> {
    overview
        .transients
        .iter()
        .filter(|tr| tr.expired)
        .flat_map(|tr| {
            let (value, timeout) = if tr.site_wide {
                ("_site_transient_", "_site_transient_timeout_")
            } else {
                ("_transient_", "_transient_timeout_")
            };
            [
                format!("{value}{}", tr.name),
                format!("{timeout}{}", tr.name),
            ]
        })
        .collect()
}

fn option_names_filter(dialect: Dialect, names: &[String]) -> String {
    let list = names
        .iter()
        .map(|n| dialect.sql_string(n))
        .collect::<Vec<_>>()
        .join(", ");
    format!("option_name IN ({list})")
}

/// `names` are the value and timeout rows of the expired transients; `rows` their stored rows.
pub fn build_delete_expired_transients(
    dialect: Dialect,
    database: &str,
    table: &str,
    names: &[String],
    rows: &ResultSet,
) -> Plan {
    Plan {
        database: database.to_string(),
        description: format!("Delete {} expired transients", names.len() / 2),
        statements: vec![format!(
            "DELETE FROM {table} WHERE {}",
            option_names_filter(dialect, names)
        )],
        undo: reinsert_sql(dialect, table, rows, &[]),
    }
}

/// `filter` comes from `logs_before_filter`; `count` is how many entries it matches.
pub fn build_delete_logs_before(
    database: &str,
    table: &str,
    filter: &str,
    days: u32,
    source: Option<&str>,
    count: u64,
) -> AppResult<Plan> {
    if count == 0 {
        return Err(AppError::InvalidInput(format!(
            "No log entries are older than {days} days"
        )));
    }
    let from = source.map(|s| format!(" from {s}")).unwrap_or_default();
    Ok(Plan {
        database: database.to_string(),
        description: format!("Delete {count} log entries older than {days} days{from} (no undo)"),
        statements: vec![format!("DELETE FROM {table} WHERE {filter}")],
        undo: Vec::new(),
    })
}

/// `before` is the action's `(status, scheduled_date_gmt)`.
pub fn build_action(
    dialect: Dialect,
    database: &str,
    table: &str,
    id: u64,
    retry: bool,
    before: (Option<&str>, Option<&str>),
) -> Plan {
    let statement = if retry {
        format!(
            "UPDATE {table} SET status = 'pending', scheduled_date_gmt = UTC_TIMESTAMP(), claim_id = 0 WHERE action_id = {id}"
        )
    } else {
        format!("UPDATE {table} SET status = 'canceled' WHERE action_id = {id}")
    };
    Plan {
        database: database.to_string(),
        description: format!("{} action #{id}", if retry { "Retry" } else { "Cancel" }),
        statements: vec![statement],
        undo: vec![format!(
            "UPDATE {table} SET status = {}, scheduled_date_gmt = {} WHERE action_id = {id}",
            sql_value(dialect, before.0),
            sql_value(dialect, before.1)
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ColumnMeta, FakeDatabase};
    use crate::error::ErrorKind;
    use crate::wp::testing::site;

    const D: Dialect = Dialect::MYSQL;
    const OPTIONS: &str = "`wp`.`wp_options`";
    const POSTMETA: &str = "`wp`.`wp_postmeta`";

    fn cell(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn rs(cols: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: cols
                .iter()
                .map(|c| ColumnMeta {
                    name: c.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            returns_rows: true,
            ..Default::default()
        }
    }

    fn postmeta() -> MetaTarget {
        MetaTarget {
            table: POSTMETA.into(),
            fk: "post_id".into(),
            key: "meta_key".into(),
            value: "meta_value".into(),
        }
    }

    const META: MetaKey<'static> = MetaKey {
        entity: "post",
        id: "5",
        key: "k",
    };

    const META_FILTER: &str = "`post_id` = '5' AND `meta_key` = 'k'";

    #[test]
    fn log_retention_filter_targets_age_and_source() {
        assert_eq!(
            logs_before_filter(Dialect::MYSQL, "2026-09-16 12:00:00", None),
            "`timestamp` < '2026-09-16 12:00:00'"
        );
        assert_eq!(
            logs_before_filter(
                Dialect::MYSQL,
                "2026-10-09 12:00:00",
                Some("woocommerce-paypal-payments")
            ),
            "`timestamp` < '2026-10-09 12:00:00' AND `source` = 'woocommerce-paypal-payments'"
        );
    }

    #[test]
    fn every_request_names_its_database() {
        let requests = [
            WpChangeRequest::SetMeta {
                database: "wp".into(),
                entity: "post".into(),
                id: "5".into(),
                key: "k".into(),
                value: "v".into(),
            },
            WpChangeRequest::DeleteMeta {
                database: "wp".into(),
                entity: "post".into(),
                id: "5".into(),
                key: "k".into(),
            },
            WpChangeRequest::SetOrderStatus {
                database: "wp".into(),
                id: "5".into(),
                status: "completed".into(),
            },
            WpChangeRequest::SetAutoload {
                database: "wp".into(),
                option: "o".into(),
                on: true,
            },
            WpChangeRequest::DeleteExpiredTransients {
                database: "wp".into(),
            },
            WpChangeRequest::DeleteLogsBefore {
                database: "wp".into(),
                days: 30,
                source: None,
            },
            WpChangeRequest::RetryAction {
                database: "wp".into(),
                id: "3".into(),
            },
            WpChangeRequest::CancelAction {
                database: "wp".into(),
                id: "3".into(),
            },
        ];

        for request in requests {
            assert_eq!(request.database(), "wp", "{request:?}");
        }
    }

    #[test]
    fn requests_use_camel_case_tags_and_fields_and_default_the_log_source() {
        let logs: WpChangeRequest = serde_json::from_value(serde_json::json!({
            "type": "deleteLogsBefore",
            "database": "wp",
            "days": 30,
        }))
        .unwrap();
        let retry = WpChangeRequest::RetryAction {
            database: "wp".into(),
            id: "3".into(),
        };

        assert_eq!(
            logs,
            WpChangeRequest::DeleteLogsBefore {
                database: "wp".into(),
                days: 30,
                source: None,
            }
        );
        assert_eq!(
            serde_json::to_value(&retry).unwrap(),
            serde_json::json!({"type": "retryAction", "database": "wp", "id": "3"})
        );
    }

    #[test]
    fn set_meta_without_a_row_inserts_and_undo_deletes() {
        let plan = build_set_meta(
            D,
            "wp",
            &postmeta(),
            META,
            "v",
            &rs(&["meta_value"], vec![]),
        )
        .unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Add meta k to post #5".into(),
                statements: vec![format!(
                    "INSERT INTO {POSTMETA} (`post_id`, `meta_key`, `meta_value`) VALUES ('5', 'k', 'v')"
                )],
                undo: vec![format!("DELETE FROM {POSTMETA} WHERE {META_FILTER}")],
            }
        );
    }

    #[test]
    fn set_meta_with_one_row_updates_and_undo_restores_the_old_value() {
        for (old, literal) in [
            (cell("old"), "'old'"),
            (cell(""), "''"),
            (Cell::Null, "NULL"),
        ] {
            let rows = rs(&["meta_value"], vec![vec![old.clone()]]);

            let plan = build_set_meta(D, "wp", &postmeta(), META, "v", &rows).unwrap();

            assert_eq!(plan.description, "Meta k of post #5", "{old:?}");
            assert_eq!(
                plan.statements,
                [format!(
                    "UPDATE {POSTMETA} SET `meta_value` = 'v' WHERE {META_FILTER}"
                )]
            );
            assert_eq!(
                plan.undo,
                [format!(
                    "UPDATE {POSTMETA} SET `meta_value` = {literal} WHERE {META_FILTER}"
                )],
                "{old:?}"
            );
        }
    }

    #[test]
    fn set_meta_with_several_rows_is_refused() {
        let rows = rs(&["meta_value"], vec![vec![cell("a")], vec![cell("b")]]);

        let err = build_set_meta(D, "wp", &postmeta(), META, "v", &rows).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.to_string(),
            "post #5 has 2 rows of meta k; edit them in the Database view"
        );
    }

    #[test]
    fn delete_meta_removes_the_key_and_undo_reinserts_every_row() {
        let rows = rs(
            &["meta_id", "post_id", "meta_key", "meta_value"],
            vec![vec![
                Cell::Number("9".into()),
                Cell::Number("5".into()),
                cell("k"),
                cell("v"),
            ]],
        );

        let plan = build_delete_meta(D, "wp", &postmeta(), META, &rows).unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Delete meta k of post #5".into(),
                statements: vec![format!("DELETE FROM {POSTMETA} WHERE {META_FILTER}")],
                undo: vec![format!(
                    "INSERT INTO {POSTMETA} (`meta_id`, `post_id`, `meta_key`, `meta_value`) VALUES ('9', '5', 'k', 'v')"
                )],
            }
        );
    }

    #[test]
    fn delete_meta_without_rows_is_refused() {
        let err =
            build_delete_meta(D, "wp", &postmeta(), META, &rs(&["meta_id"], vec![])).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "post #5 has no meta k");
    }

    fn orders_row(before: &str) -> Option<OrderRow> {
        Some(OrderRow {
            table: "`wp`.`wp_wc_orders`".into(),
            before: before.into(),
        })
    }

    fn posts_row(before: &str) -> Option<OrderRow> {
        Some(OrderRow {
            table: "`wp`.`wp_posts`".into(),
            before: before.into(),
        })
    }

    #[test]
    fn order_status_updates_the_hpos_row_and_the_posts_copy_with_matching_undo() {
        let plan = build_set_order_status(
            D,
            "wp",
            4711,
            "completed",
            orders_row("wc-processing"),
            posts_row("wc-on-hold"),
        )
        .unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Order #4711 status → completed".into(),
                statements: vec![
                    "UPDATE `wp`.`wp_wc_orders` SET status = 'wc-completed', date_updated_gmt = UTC_TIMESTAMP() WHERE id = 4711".into(),
                    "UPDATE `wp`.`wp_posts` SET post_status = 'wc-completed', post_modified_gmt = UTC_TIMESTAMP(), post_modified = NOW() WHERE ID = 4711".into(),
                ],
                undo: vec![
                    "UPDATE `wp`.`wp_wc_orders` SET status = 'wc-processing' WHERE id = 4711".into(),
                    "UPDATE `wp`.`wp_posts` SET post_status = 'wc-on-hold' WHERE ID = 4711".into(),
                ],
            }
        );
    }

    #[test]
    fn order_status_gets_the_wc_prefix_unless_it_has_one_or_is_trash() {
        for (given, stored) in [
            ("completed", "wc-completed"),
            ("wc-completed", "wc-completed"),
            ("trash", "trash"),
        ] {
            let plan =
                build_set_order_status(D, "wp", 1, given, orders_row("wc-pending"), None).unwrap();

            assert_eq!(
                plan.statements,
                [format!(
                    "UPDATE `wp`.`wp_wc_orders` SET status = '{stored}', date_updated_gmt = UTC_TIMESTAMP() WHERE id = 1"
                )],
                "{given}"
            );
        }
    }

    #[test]
    fn order_status_with_only_one_store_updates_only_that_store() {
        let hpos_only = build_set_order_status(D, "wp", 1, "x", orders_row("wc-a"), None).unwrap();
        let post_only = build_set_order_status(D, "wp", 1, "x", None, posts_row("wc-a")).unwrap();

        assert_eq!(hpos_only.statements.len(), 1);
        assert_eq!(
            hpos_only.undo,
            ["UPDATE `wp`.`wp_wc_orders` SET status = 'wc-a' WHERE id = 1"]
        );
        assert_eq!(post_only.statements.len(), 1);
        assert_eq!(
            post_only.undo,
            ["UPDATE `wp`.`wp_posts` SET post_status = 'wc-a' WHERE ID = 1"]
        );
    }

    #[test]
    fn order_status_of_an_order_in_no_store_is_refused() {
        let err = build_set_order_status(D, "wp", 4711, "completed", None, None).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "Order #4711 was not found");
    }

    #[test]
    fn autoload_keeps_the_spelling_style_of_the_stored_value() {
        for (before, on, value) in [
            ("yes", false, "no"),
            ("no", true, "yes"),
            ("on", false, "off"),
            ("off", true, "on"),
            ("auto-on", false, "off"),
            ("auto-off", true, "on"),
        ] {
            let plan = build_set_autoload(D, "wp", OPTIONS, "foo", on, before);

            assert_eq!(
                plan,
                Plan {
                    database: "wp".into(),
                    description: format!("Autoload of foo → {value}"),
                    statements: vec![format!(
                        "UPDATE {OPTIONS} SET autoload = '{value}' WHERE option_name = 'foo'"
                    )],
                    undo: vec![format!(
                        "UPDATE {OPTIONS} SET autoload = '{before}' WHERE option_name = 'foo'"
                    )],
                },
                "{before} -> on={on}"
            );
        }
    }

    #[test]
    fn expired_transients_are_deleted_by_one_statement_naming_value_and_timeout_rows() {
        let names = vec![
            "_transient_a".to_string(),
            "_transient_timeout_a".to_string(),
        ];
        let rows = rs(
            &["option_id", "option_name", "option_value"],
            vec![
                vec![Cell::Number("1".into()), cell("_transient_a"), cell("x")],
                vec![
                    Cell::Number("2".into()),
                    cell("_transient_timeout_a"),
                    cell("100"),
                ],
            ],
        );

        let plan = build_delete_expired_transients(D, "wp", OPTIONS, &names, &rows);

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Delete 1 expired transients".into(),
                statements: vec![format!(
                    "DELETE FROM {OPTIONS} WHERE option_name IN ('_transient_a', '_transient_timeout_a')"
                )],
                undo: vec![
                    format!(
                        "INSERT INTO {OPTIONS} (`option_id`, `option_name`, `option_value`) VALUES ('1', '_transient_a', 'x')"
                    ),
                    format!(
                        "INSERT INTO {OPTIONS} (`option_id`, `option_name`, `option_value`) VALUES ('2', '_transient_timeout_a', '100')"
                    ),
                ],
            }
        );
    }

    #[test]
    fn log_retention_without_matching_entries_is_refused() {
        let err = build_delete_logs_before("wp", "t", "f", 30, None, 0).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "No log entries are older than 30 days");
    }

    #[test]
    fn log_retention_deletes_by_the_given_filter_and_cannot_be_undone() {
        let plan = build_delete_logs_before(
            "wp",
            "`wp`.`wp_woocommerce_log`",
            "`timestamp` < 'c'",
            30,
            None,
            5,
        )
        .unwrap();
        let sourced = build_delete_logs_before("wp", "t", "f", 7, Some("paypal"), 2).unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Delete 5 log entries older than 30 days (no undo)".into(),
                statements: vec![
                    "DELETE FROM `wp`.`wp_woocommerce_log` WHERE `timestamp` < 'c'".into()
                ],
                undo: Vec::new(),
            }
        );
        assert_eq!(
            sourced.description,
            "Delete 2 log entries older than 7 days from paypal (no undo)"
        );
    }

    #[test]
    fn retrying_an_action_makes_it_pending_and_undo_restores_status_and_date() {
        let table = "`wp`.`wp_actionscheduler_actions`";

        let plan = build_action(
            D,
            "wp",
            table,
            3,
            true,
            (Some("failed"), Some("2026-01-01 00:00:00")),
        );

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Retry action #3".into(),
                statements: vec![format!(
                    "UPDATE {table} SET status = 'pending', scheduled_date_gmt = UTC_TIMESTAMP(), claim_id = 0 WHERE action_id = 3"
                )],
                undo: vec![format!(
                    "UPDATE {table} SET status = 'failed', scheduled_date_gmt = '2026-01-01 00:00:00' WHERE action_id = 3"
                )],
            }
        );
    }

    #[test]
    fn cancelling_an_action_sets_canceled_and_undo_restores_nulls() {
        let plan = build_action(D, "wp", "t", 3, false, (None, None));

        assert_eq!(plan.description, "Cancel action #3");
        assert_eq!(
            plan.statements,
            ["UPDATE t SET status = 'canceled' WHERE action_id = 3"]
        );
        assert_eq!(
            plan.undo,
            ["UPDATE t SET status = NULL, scheduled_date_gmt = NULL WHERE action_id = 3"]
        );
    }

    #[test]
    fn meta_target_qualifies_the_entitys_meta_table() {
        let (_, catalog) = site(&["posts", "postmeta"]);

        assert_eq!(meta_target(D, &catalog, "wp", "post").unwrap(), postmeta());
        assert!(meta_target(D, &catalog, "wp", "no_such_entity").is_err());
    }

    #[test]
    fn meta_target_follows_the_meta_table_of_the_entitys_storage() {
        let (_, catalog) = site(&["posts", "postmeta", "wc_orders", "wc_orders_meta"]);

        assert_eq!(
            meta_target(D, &catalog, "wp", "order").unwrap(),
            MetaTarget {
                table: "`wp`.`wp_wc_orders_meta`".into(),
                fk: "order_id".into(),
                key: "meta_key".into(),
                value: "meta_value".into(),
            }
        );
    }

    #[test]
    fn meta_target_of_an_entity_without_meta_is_refused() {
        let (_, catalog) = site(&["posts", "postmeta", "comments"]);

        let err = meta_target(D, &catalog, "wp", "comment").unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "comment has no meta");
    }

    #[tokio::test]
    async fn planning_an_autoload_change_reads_the_stored_value_first() {
        let (ctx, catalog) = site(&["options"]);
        let db = FakeDatabase::new().on(
            "SELECT autoload FROM `wp`.`wp_options` WHERE option_name = 'foo'",
            rs(&["autoload"], vec![vec![cell("yes")]]),
        );
        let request = WpChangeRequest::SetAutoload {
            database: "wp".into(),
            option: "foo".into(),
            on: false,
        };

        let plan = plan(&db, &ctx, &catalog, &request).await.unwrap();

        assert_eq!(
            plan,
            build_set_autoload(D, "wp", OPTIONS, "foo", false, "yes")
        );
    }

    #[tokio::test]
    async fn planning_a_meta_change_reads_the_value_from_the_entitys_meta_table() {
        let (ctx, catalog) = site(&["posts", "postmeta"]);
        let stored = rs(&["meta_value"], vec![vec![cell("old")]]);
        let db = FakeDatabase::new().on("SELECT `meta_value`", stored.clone());
        let request = WpChangeRequest::SetMeta {
            database: "wp".into(),
            entity: "post".into(),
            id: "5".into(),
            key: "k".into(),
            value: "v".into(),
        };

        let plan = plan(&db, &ctx, &catalog, &request).await.unwrap();

        assert_eq!(
            db.queries(),
            [format!(
                "SELECT `meta_value` FROM {POSTMETA} WHERE {META_FILTER}"
            )]
        );
        assert_eq!(
            plan,
            build_set_meta(D, "wp", &postmeta(), META, "v", &stored).unwrap()
        );
    }

    #[tokio::test]
    async fn planning_an_autoload_change_of_a_missing_option_is_refused() {
        let (ctx, catalog) = site(&["options"]);
        let db = FakeDatabase::new().on("SELECT autoload", rs(&["autoload"], vec![]));
        let request = WpChangeRequest::SetAutoload {
            database: "wp".into(),
            option: "foo".into(),
            on: true,
        };

        let err = plan(&db, &ctx, &catalog, &request).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "Option foo not found");
    }

    #[tokio::test]
    async fn planning_an_action_change_with_a_non_numeric_id_is_refused_without_reading() {
        let (ctx, catalog) = site(&[]);
        let db = FakeDatabase::new();
        let request = WpChangeRequest::RetryAction {
            database: "wp".into(),
            id: "abc".into(),
        };

        let err = plan(&db, &ctx, &catalog, &request).await.unwrap_err();

        assert_eq!(err.to_string(), "'abc' is not a valid ID");
        assert!(db.queries().is_empty());
    }

    #[tokio::test]
    async fn planning_a_log_cleanup_needs_the_log_table() {
        let (ctx, catalog) = site(&["options"]);
        let db = FakeDatabase::new();
        let request = WpChangeRequest::DeleteLogsBefore {
            database: "wp".into(),
            days: 30,
            source: None,
        };

        let err = plan(&db, &ctx, &catalog, &request).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "This site has no woocommerce_log table");
    }
}
