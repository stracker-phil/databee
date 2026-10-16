//! Edits as reviewable SQL: every change request is planned into statements plus the
//! statements that undo it (built from the current values), shown to the user, and only
//! then applied in one transaction.

use serde::{Deserialize, Serialize};

use super::context::WpContext;
use super::entity::{Catalog, cell_str};
use super::fetch;
use super::lists::text;
use crate::db::Database;
use crate::db::{Cell, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChangeRequest {
    /// Any table cell, addressed by a unique key column.
    #[serde(rename_all = "camelCase")]
    SetCell {
        database: String,
        table: String,
        key_column: String,
        key: String,
        column: String,
        value: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    DeleteRow {
        database: String,
        table: String,
        key_column: String,
        key: String,
    },
    #[serde(rename_all = "camelCase")]
    DuplicateRow {
        database: String,
        table: String,
        key_column: String,
        key: String,
    },
    #[serde(rename_all = "camelCase")]
    InsertRow {
        database: String,
        table: String,
        values: Vec<(String, Option<String>)>,
    },
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
        source: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    RetryAction { database: String, id: String },
    #[serde(rename_all = "camelCase")]
    CancelAction { database: String, id: String },
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedChange {
    pub description: String,
    pub statements: Vec<String>,
    /// Restores the previous state; empty when a change can't be undone automatically.
    pub undo: Vec<String>,
}

pub fn sql_value(dialect: Dialect, v: Option<&str>) -> String {
    v.map_or_else(|| "NULL".to_string(), |v| dialect.sql_string(v))
}

fn tref(dialect: Dialect, db: &str, table: &str) -> String {
    format!("{}.{}", dialect.quote_ident(db), dialect.quote_ident(table))
}

fn wp_table(dialect: Dialect, db: &str, ctx: &WpContext, table: &str) -> String {
    tref(dialect, db, &ctx.table(table))
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

/// The SQL literal that restores `cell` exactly: `NULL` stays `NULL`, `''` stays `''`.
/// `None` for binary cells, which only carry a preview.
fn restore_literal(dialect: Dialect, cell: &Cell) -> Option<String> {
    match cell {
        Cell::Bytes { .. } => None,
        _ => Some(sql_value(dialect, cell_str(cell))),
    }
}

/// `INSERT` statements that recreate the given rows exactly; empty when a row holds
/// binary data.
pub fn reinsert_sql(dialect: Dialect, table: &str, rows: &ResultSet) -> Vec<String> {
    let Some(values) = rows
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|cell| restore_literal(dialect, cell))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    let cols = rows
        .columns
        .iter()
        .map(|c| dialect.quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    values
        .iter()
        .map(|v| format!("INSERT INTO {table} ({cols}) VALUES ({})", v.join(", ")))
        .collect()
}

fn first_text(rs: &ResultSet) -> Option<Option<String>> {
    rs.rows.first().map(|r| text(&r[0]))
}

/// The plan's `undo` restoring `column` from `old`, plus a description note when there is
/// none.
fn restore_undo(
    dialect: Dialect,
    table: &str,
    column: &str,
    old: &Cell,
    filter: Option<&str>,
) -> (Vec<String>, String) {
    let before = restore_literal(dialect, old);
    if let (Some(before), Some(filter)) = (&before, filter) {
        let sql = format!(
            "UPDATE {table} SET {} = {before} WHERE {filter}",
            dialect.quote_ident(column)
        );
        return (vec![sql], String::new());
    }
    let reason = if before.is_none() {
        "binary value"
    } else {
        "the key becomes NULL"
    };
    (Vec::new(), format!(" (no undo: {reason})"))
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

pub async fn plan(
    session: &dyn Database,
    ctx: Option<&WpContext>,
    catalog: Option<&Catalog>,
    req: &ChangeRequest,
) -> AppResult<PlannedChange> {
    let dialect = session.dialect();
    let need_wp = || ctx.ok_or_else(|| AppError::InvalidInput("Not a WordPress database".into()));
    Ok(match req {
        ChangeRequest::SetCell {
            database,
            table,
            key_column,
            key,
            column,
            value,
        } => {
            let t = tref(dialect, database, table);
            let filter = format!(
                "{} = {}",
                dialect.quote_ident(key_column),
                dialect.sql_string(key)
            );
            let old = fetch(
                session,
                &format!(
                    "SELECT {} FROM {t} WHERE {filter}",
                    dialect.quote_ident(column)
                ),
            )
            .await?;
            if old.rows.len() != 1 {
                return Err(AppError::InvalidInput(format!(
                    "{key_column} = {key} matches {} rows; edits need a unique key",
                    old.rows.len()
                )));
            }
            // After a key change the row is only found by its new key.
            let undo_filter = if column == key_column {
                value.as_deref().map(|v| {
                    format!(
                        "{} = {}",
                        dialect.quote_ident(key_column),
                        dialect.sql_string(v)
                    )
                })
            } else {
                Some(filter.clone())
            };
            let (undo, note) =
                restore_undo(dialect, &t, column, &old.rows[0][0], undo_filter.as_deref());
            PlannedChange {
                description: format!("{table}.{column} of {key_column} {key}{note}"),
                statements: vec![format!(
                    "UPDATE {t} SET {} = {} WHERE {filter}",
                    dialect.quote_ident(column),
                    sql_value(dialect, value.as_deref())
                )],
                undo,
            }
        }
        ChangeRequest::DeleteRow {
            database,
            table,
            key_column,
            key,
        } => {
            let t = tref(dialect, database, table);
            let filter = format!(
                "{} = {}",
                dialect.quote_ident(key_column),
                dialect.sql_string(key)
            );
            let rows = fetch(session, &format!("SELECT * FROM {t} WHERE {filter}")).await?;
            if rows.rows.len() != 1 {
                return Err(AppError::InvalidInput(format!(
                    "{key_column} = {key} matches {} rows",
                    rows.rows.len()
                )));
            }
            PlannedChange {
                description: format!("Delete {table} row {key_column} {key}"),
                statements: vec![format!("DELETE FROM {t} WHERE {filter} LIMIT 1")],
                undo: reinsert_sql(dialect, &t, &rows),
            }
        }
        ChangeRequest::DuplicateRow {
            database,
            table,
            key_column,
            key,
        } => {
            let t = tref(dialect, database, table);
            let structure = session.structure(database, table).await?;
            let cols: Vec<String> = structure
                .columns
                .iter()
                .filter(|c| !c.extra.contains("auto_increment"))
                .map(|c| dialect.quote_ident(&c.name))
                .collect();
            PlannedChange {
                description: format!("Duplicate {table} row {key_column} {key}"),
                statements: vec![format!(
                    "INSERT INTO {t} ({cols}) SELECT {cols} FROM {t} WHERE {} = {}",
                    dialect.quote_ident(key_column),
                    dialect.sql_string(key),
                    cols = cols.join(", ")
                )],
                undo: Vec::new(),
            }
        }
        ChangeRequest::InsertRow {
            database,
            table,
            values,
        } => {
            let t = tref(dialect, database, table);
            let cols = values
                .iter()
                .map(|(c, _)| dialect.quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ");
            let vals = values
                .iter()
                .map(|(_, v)| sql_value(dialect, v.as_deref()))
                .collect::<Vec<_>>()
                .join(", ");
            PlannedChange {
                description: format!("Insert into {table}"),
                statements: vec![format!("INSERT INTO {t} ({cols}) VALUES ({vals})")],
                undo: Vec::new(),
            }
        }
        ChangeRequest::SetMeta {
            database,
            entity,
            id,
            key,
            value,
        } => {
            let ctx = need_wp()?;
            let def = catalog
                .ok_or_else(|| AppError::InvalidInput("No entity catalog".into()))?
                .get(entity)?;
            let meta = def
                .meta
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput(format!("{entity} has no meta")))?;
            let t = wp_table(dialect, database, ctx, &meta.table);
            let filter = format!(
                "{} = {} AND {} = {}",
                dialect.quote_ident(&meta.fk),
                dialect.sql_string(id),
                dialect.quote_ident(&meta.key),
                dialect.sql_string(key)
            );
            let old = fetch(
                session,
                &format!(
                    "SELECT {} FROM {t} WHERE {filter}",
                    dialect.quote_ident(&meta.value)
                ),
            )
            .await?;
            match old.rows.len() {
                0 => PlannedChange {
                    description: format!("Add meta {key} to {entity} #{id}"),
                    statements: vec![format!(
                        "INSERT INTO {t} ({}, {}, {}) VALUES ({}, {}, {})",
                        dialect.quote_ident(&meta.fk),
                        dialect.quote_ident(&meta.key),
                        dialect.quote_ident(&meta.value),
                        dialect.sql_string(id),
                        dialect.sql_string(key),
                        dialect.sql_string(value)
                    )],
                    undo: vec![format!("DELETE FROM {t} WHERE {filter}")],
                },
                1 => {
                    let (undo, note) =
                        restore_undo(dialect, &t, &meta.value, &old.rows[0][0], Some(&filter));
                    PlannedChange {
                        description: format!("Meta {key} of {entity} #{id}{note}"),
                        statements: vec![format!(
                            "UPDATE {t} SET {} = {} WHERE {filter}",
                            dialect.quote_ident(&meta.value),
                            dialect.sql_string(value)
                        )],
                        undo,
                    }
                }
                // One undo value can't restore rows that held different values.
                n => {
                    return Err(AppError::InvalidInput(format!(
                        "{entity} #{id} has {n} rows of meta {key}; edit them in the Database view"
                    )));
                }
            }
        }
        ChangeRequest::DeleteMeta {
            database,
            entity,
            id,
            key,
        } => {
            let ctx = need_wp()?;
            let def = catalog
                .ok_or_else(|| AppError::InvalidInput("No entity catalog".into()))?
                .get(entity)?;
            let meta = def
                .meta
                .as_ref()
                .ok_or_else(|| AppError::InvalidInput(format!("{entity} has no meta")))?;
            let t = wp_table(dialect, database, ctx, &meta.table);
            let filter = format!(
                "{} = {} AND {} = {}",
                dialect.quote_ident(&meta.fk),
                dialect.sql_string(id),
                dialect.quote_ident(&meta.key),
                dialect.sql_string(key)
            );
            let rows = fetch(session, &format!("SELECT * FROM {t} WHERE {filter}")).await?;
            if rows.rows.is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "{entity} #{id} has no meta {key}"
                )));
            }
            PlannedChange {
                description: format!("Delete meta {key} of {entity} #{id}"),
                statements: vec![format!("DELETE FROM {t} WHERE {filter}")],
                undo: reinsert_sql(dialect, &t, &rows),
            }
        }
        ChangeRequest::SetOrderStatus {
            database,
            id,
            status,
        } => {
            let ctx = need_wp()?;
            let n: u64 = id
                .parse()
                .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
            let status = if status.starts_with("wc-") || status == "trash" {
                status.clone()
            } else {
                format!("wc-{status}")
            };
            let mut statements = Vec::new();
            let mut undo = Vec::new();
            let posts = wp_table(dialect, database, ctx, "posts");
            let post_status = fetch(
                session,
                &format!(
                    "SELECT post_status FROM {posts} WHERE ID = {n} AND post_type LIKE {}",
                    dialect.like(r"shop\_%")
                ),
            )
            .await?;
            if ctx.hpos() {
                let orders = wp_table(dialect, database, ctx, "wc_orders");
                let old = fetch(
                    session,
                    &format!("SELECT status FROM {orders} WHERE id = {n}"),
                )
                .await?;
                let before = first_text(&old)
                    .flatten()
                    .ok_or_else(|| AppError::InvalidInput(format!("Order #{id} was not found")))?;
                statements.push(format!("UPDATE {orders} SET status = {}, date_updated_gmt = UTC_TIMESTAMP() WHERE id = {n}", dialect.sql_string(&status)));
                undo.push(format!(
                    "UPDATE {orders} SET status = {} WHERE id = {n}",
                    dialect.sql_string(&before)
                ));
            }
            // Legacy store, or the synced posts copy (placeholders keep their own status).
            if let Some(before) = first_text(&post_status).flatten().filter(|s| s != "draft") {
                statements.push(format!(
                    "UPDATE {posts} SET post_status = {}, post_modified_gmt = UTC_TIMESTAMP(), post_modified = NOW() WHERE ID = {n}",
                    dialect.sql_string(&status)
                ));
                undo.push(format!(
                    "UPDATE {posts} SET post_status = {} WHERE ID = {n}",
                    dialect.sql_string(&before)
                ));
            }
            if statements.is_empty() {
                return Err(AppError::InvalidInput(format!("Order #{id} was not found")));
            }
            PlannedChange {
                description: format!("Order #{id} status → {}", status.trim_start_matches("wc-")),
                statements,
                undo,
            }
        }
        ChangeRequest::SetAutoload {
            database,
            option,
            on,
        } => {
            let ctx = need_wp()?;
            let t = wp_table(dialect, database, ctx, "options");
            let old = fetch(
                session,
                &format!(
                    "SELECT autoload FROM {t} WHERE option_name = {}",
                    dialect.sql_string(option)
                ),
            )
            .await?;
            let before = first_text(&old)
                .flatten()
                .ok_or_else(|| AppError::InvalidInput(format!("Option {option} not found")))?;
            // WordPress 6.6 switched from yes/no to on/off (and auto variants).
            let modern = !matches!(before.as_str(), "yes" | "no");
            let value = match (modern, on) {
                (true, true) => "on",
                (true, false) => "off",
                (false, true) => "yes",
                (false, false) => "no",
            };
            PlannedChange {
                description: format!("Autoload of {option} → {value}"),
                statements: vec![format!(
                    "UPDATE {t} SET autoload = '{value}' WHERE option_name = {}",
                    dialect.sql_string(option)
                )],
                undo: vec![format!(
                    "UPDATE {t} SET autoload = {} WHERE option_name = {}",
                    dialect.sql_string(&before),
                    dialect.sql_string(option)
                )],
            }
        }
        ChangeRequest::DeleteExpiredTransients { database } => {
            let ctx = need_wp()?;
            let t = wp_table(dialect, database, ctx, "options");
            let overview = super::options_overview(session, ctx, database).await?;
            let names: Vec<String> = overview
                .transients
                .iter()
                .filter(|tr| tr.expired)
                .flat_map(|tr| {
                    let (v, to) = if tr.site_wide {
                        ("_site_transient_", "_site_transient_timeout_")
                    } else {
                        ("_transient_", "_transient_timeout_")
                    };
                    [
                        dialect.sql_string(&format!("{v}{}", tr.name)),
                        dialect.sql_string(&format!("{to}{}", tr.name)),
                    ]
                })
                .collect();
            if names.is_empty() {
                return Err(AppError::InvalidInput(
                    "There are no expired transients".into(),
                ));
            }
            let filter = format!("option_name IN ({})", names.join(", "));
            let rows = fetch(session, &format!("SELECT * FROM {t} WHERE {filter}")).await?;
            PlannedChange {
                description: format!("Delete {} expired transients", names.len() / 2),
                statements: vec![format!("DELETE FROM {t} WHERE {filter}")],
                undo: reinsert_sql(dialect, &t, &rows),
            }
        }
        ChangeRequest::DeleteLogsBefore {
            database,
            days,
            source,
        } => {
            let ctx = need_wp()?;
            if !ctx.has_table("woocommerce_log") {
                return Err(AppError::InvalidInput(
                    "This site has no woocommerce_log table".into(),
                ));
            }
            let t = wp_table(dialect, database, ctx, "woocommerce_log");
            let filter = logs_before_filter(
                dialect,
                &log_cutoff(session, *days).await?,
                source.as_deref(),
            );
            let count = fetch(session, &format!("SELECT COUNT(*) FROM {t} WHERE {filter}")).await?;
            let n = count
                .rows
                .first()
                .and_then(|r| super::lists::num(&r[0]))
                .unwrap_or(0);
            if n == 0 {
                return Err(AppError::InvalidInput(format!(
                    "No log entries are older than {days} days"
                )));
            }
            PlannedChange {
                description: format!(
                    "Delete {n} log entries older than {days} days{} (no undo)",
                    source
                        .as_deref()
                        .map(|s| format!(" from {s}"))
                        .unwrap_or_default()
                ),
                statements: vec![format!("DELETE FROM {t} WHERE {filter}")],
                undo: vec![],
            }
        }
        ChangeRequest::RetryAction { database, id }
        | ChangeRequest::CancelAction { database, id } => {
            let ctx = need_wp()?;
            let n: u64 = id
                .parse()
                .map_err(|_| AppError::InvalidInput(format!("'{id}' is not a valid ID")))?;
            let t = wp_table(dialect, database, ctx, "actionscheduler_actions");
            let rows = fetch(
                session,
                &format!("SELECT status, scheduled_date_gmt FROM {t} WHERE action_id = {n}"),
            )
            .await?;
            let row = rows
                .rows
                .first()
                .ok_or_else(|| AppError::InvalidInput(format!("Action #{id} was not found")))?;
            let (before_status, before_date) = (cell_str(&row[0]), cell_str(&row[1]));
            let retry = matches!(req, ChangeRequest::RetryAction { .. });
            PlannedChange {
                description: format!("{} action #{id}", if retry { "Retry" } else { "Cancel" }),
                statements: vec![if retry {
                    format!(
                        "UPDATE {t} SET status = 'pending', scheduled_date_gmt = UTC_TIMESTAMP(), claim_id = 0 WHERE action_id = {n}"
                    )
                } else {
                    format!("UPDATE {t} SET status = 'canceled' WHERE action_id = {n}")
                }],
                undo: vec![format!(
                    "UPDATE {t} SET status = {}, scheduled_date_gmt = {} WHERE action_id = {n}",
                    sql_value(dialect, before_status),
                    sql_value(dialect, before_date)
                )],
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnMeta;

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
    fn reinsert_recreates_rows_with_nulls() {
        let rs = ResultSet {
            columns: vec![
                ColumnMeta {
                    name: "option_id".into(),
                    type_name: "BIGINT".into(),
                },
                ColumnMeta {
                    name: "option_value".into(),
                    type_name: "TEXT".into(),
                },
            ],
            rows: vec![
                vec![Cell::Number("7".into()), Cell::Null],
                vec![Cell::Number("8".into()), Cell::Text("it's".into())],
            ],
            ..Default::default()
        };
        assert_eq!(
            reinsert_sql(Dialect::MYSQL, "`db`.`wp_options`", &rs),
            [
                "INSERT INTO `db`.`wp_options` (`option_id`, `option_value`) VALUES ('7', NULL)",
                "INSERT INTO `db`.`wp_options` (`option_id`, `option_value`) VALUES ('8', 'it''s')",
            ]
        );
    }
}
