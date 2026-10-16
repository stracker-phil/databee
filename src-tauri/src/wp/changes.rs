//! Edits as reviewable SQL: every change request is planned into statements plus the
//! statements that undo it (built from the current values), shown to the user, and only
//! then applied in one transaction.

use serde::{Deserialize, Serialize};

use super::context::WpContext;
use super::entity::Catalog;
use super::lists::text;
use super::php::{self, PhpKey, PhpValue};
use super::{fetch, sql_string};
use crate::db::mysql::MySqlSession;
use crate::db::{Cell, ResultSet, quote_ident};
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

pub fn sql_value(v: Option<&str>) -> String {
    v.map_or_else(|| "NULL".to_string(), sql_string)
}

fn tref(db: &str, table: &str) -> String {
    format!("{}.{}", quote_ident(db), quote_ident(table))
}

fn wp_table(db: &str, ctx: &WpContext, table: &str) -> String {
    tref(db, &ctx.table(table))
}

pub fn logs_before_filter(days: u32, source: Option<&str>) -> String {
    let mut f = format!("`timestamp` < UTC_TIMESTAMP() - INTERVAL {days} DAY");
    if let Some(s) = source {
        f.push_str(&format!(" AND `source` = {}", sql_string(s)));
    }
    f
}

/// `INSERT` statements that recreate the given rows exactly.
pub fn reinsert_sql(table: &str, rows: &ResultSet) -> Vec<String> {
    // Binary cells only carry a preview, so such rows can't be restored from SQL text.
    if rows
        .rows
        .iter()
        .flatten()
        .any(|c| matches!(c, Cell::Bytes { .. }))
    {
        return Vec::new();
    }
    let cols = rows
        .columns
        .iter()
        .map(|c| quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    rows.rows
        .iter()
        .map(|r| {
            let values = r
                .iter()
                .map(|c| match c {
                    Cell::Null | Cell::Bytes { .. } => "NULL".to_string(),
                    Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => {
                        sql_string(s)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("INSERT INTO {table} ({cols}) VALUES ({values})")
        })
        .collect()
}

fn first_text(rs: &ResultSet) -> Option<Option<String>> {
    rs.rows.first().map(|r| text(&r[0]))
}

pub async fn plan(
    session: &MySqlSession,
    ctx: Option<&WpContext>,
    catalog: Option<&Catalog>,
    req: &ChangeRequest,
) -> AppResult<PlannedChange> {
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
            let t = tref(database, table);
            let filter = format!("{} = {}", quote_ident(key_column), sql_string(key));
            let old = fetch(
                session,
                &format!("SELECT {} FROM {t} WHERE {filter}", quote_ident(column)),
            )
            .await?;
            if old.rows.len() != 1 {
                return Err(AppError::InvalidInput(format!(
                    "{key_column} = {key} matches {} rows; edits need a unique key",
                    old.rows.len()
                )));
            }
            let before = first_text(&old).flatten();
            PlannedChange {
                description: format!("{table}.{column} of {key_column} {key}"),
                statements: vec![format!(
                    "UPDATE {t} SET {} = {} WHERE {filter}",
                    quote_ident(column),
                    sql_value(value.as_deref())
                )],
                undo: vec![format!(
                    "UPDATE {t} SET {} = {} WHERE {filter}",
                    quote_ident(column),
                    sql_value(before.as_deref())
                )],
            }
        }
        ChangeRequest::DeleteRow {
            database,
            table,
            key_column,
            key,
        } => {
            let t = tref(database, table);
            let filter = format!("{} = {}", quote_ident(key_column), sql_string(key));
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
                undo: reinsert_sql(&t, &rows),
            }
        }
        ChangeRequest::DuplicateRow {
            database,
            table,
            key_column,
            key,
        } => {
            let t = tref(database, table);
            let structure = session.structure(database, table).await?;
            let cols: Vec<String> = structure
                .columns
                .iter()
                .filter(|c| !c.extra.contains("auto_increment"))
                .map(|c| quote_ident(&c.name))
                .collect();
            PlannedChange {
                description: format!("Duplicate {table} row {key_column} {key}"),
                statements: vec![format!(
                    "INSERT INTO {t} ({cols}) SELECT {cols} FROM {t} WHERE {} = {}",
                    quote_ident(key_column),
                    sql_string(key),
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
            let t = tref(database, table);
            let cols = values
                .iter()
                .map(|(c, _)| quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ");
            let vals = values
                .iter()
                .map(|(_, v)| sql_value(v.as_deref()))
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
            let t = wp_table(database, ctx, &meta.table);
            let filter = format!(
                "{} = {} AND {} = {}",
                quote_ident(&meta.fk),
                sql_string(id),
                quote_ident(&meta.key),
                sql_string(key)
            );
            let old = fetch(
                session,
                &format!(
                    "SELECT {} FROM {t} WHERE {filter}",
                    quote_ident(&meta.value)
                ),
            )
            .await?;
            match old.rows.len() {
                0 => PlannedChange {
                    description: format!("Add meta {key} to {entity} #{id}"),
                    statements: vec![format!(
                        "INSERT INTO {t} ({}, {}, {}) VALUES ({}, {}, {})",
                        quote_ident(&meta.fk),
                        quote_ident(&meta.key),
                        quote_ident(&meta.value),
                        sql_string(id),
                        sql_string(key),
                        sql_string(value)
                    )],
                    undo: vec![format!("DELETE FROM {t} WHERE {filter}")],
                },
                n => PlannedChange {
                    description: format!(
                        "Meta {key} of {entity} #{id}{}",
                        if n > 1 {
                            format!(" ({n} rows)")
                        } else {
                            String::new()
                        }
                    ),
                    statements: vec![format!(
                        "UPDATE {t} SET {} = {} WHERE {filter}",
                        quote_ident(&meta.value),
                        sql_string(value)
                    )],
                    undo: vec![format!(
                        "UPDATE {t} SET {} = {} WHERE {filter}",
                        quote_ident(&meta.value),
                        sql_value(first_text(&old).flatten().as_deref())
                    )],
                },
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
            let t = wp_table(database, ctx, &meta.table);
            let filter = format!(
                "{} = {} AND {} = {}",
                quote_ident(&meta.fk),
                sql_string(id),
                quote_ident(&meta.key),
                sql_string(key)
            );
            let rows = fetch(session, &format!("SELECT * FROM {t} WHERE {filter}")).await?;
            PlannedChange {
                description: format!("Delete meta {key} of {entity} #{id}"),
                statements: vec![format!("DELETE FROM {t} WHERE {filter}")],
                undo: reinsert_sql(&t, &rows),
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
            let posts = wp_table(database, ctx, "posts");
            let post_status = fetch(
                session,
                &format!(
                    "SELECT post_status FROM {posts} WHERE ID = {n} AND post_type LIKE 'shop\\_%'"
                ),
            )
            .await?;
            if ctx.hpos() {
                let orders = wp_table(database, ctx, "wc_orders");
                let old = fetch(
                    session,
                    &format!("SELECT status FROM {orders} WHERE id = {n}"),
                )
                .await?;
                let before = first_text(&old)
                    .flatten()
                    .ok_or_else(|| AppError::InvalidInput(format!("Order #{id} was not found")))?;
                statements.push(format!("UPDATE {orders} SET status = {}, date_updated_gmt = UTC_TIMESTAMP() WHERE id = {n}", sql_string(&status)));
                undo.push(format!(
                    "UPDATE {orders} SET status = {} WHERE id = {n}",
                    sql_string(&before)
                ));
            }
            // Legacy store, or the synced posts copy (placeholders keep their own status).
            if let Some(before) = first_text(&post_status).flatten().filter(|s| s != "draft") {
                statements.push(format!(
                    "UPDATE {posts} SET post_status = {}, post_modified_gmt = UTC_TIMESTAMP(), post_modified = NOW() WHERE ID = {n}",
                    sql_string(&status)
                ));
                undo.push(format!(
                    "UPDATE {posts} SET post_status = {} WHERE ID = {n}",
                    sql_string(&before)
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
            let t = wp_table(database, ctx, "options");
            let old = fetch(
                session,
                &format!(
                    "SELECT autoload FROM {t} WHERE option_name = {}",
                    sql_string(option)
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
                    sql_string(option)
                )],
                undo: vec![format!(
                    "UPDATE {t} SET autoload = {} WHERE option_name = {}",
                    sql_string(&before),
                    sql_string(option)
                )],
            }
        }
        ChangeRequest::DeleteExpiredTransients { database } => {
            let ctx = need_wp()?;
            let t = wp_table(database, ctx, "options");
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
                        sql_string(&format!("{v}{}", tr.name)),
                        sql_string(&format!("{to}{}", tr.name)),
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
                undo: reinsert_sql(&t, &rows),
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
            let t = wp_table(database, ctx, "woocommerce_log");
            let filter = logs_before_filter(*days, source.as_deref());
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
            let t = wp_table(database, ctx, "actionscheduler_actions");
            let rows = fetch(
                session,
                &format!("SELECT status, scheduled_date_gmt FROM {t} WHERE action_id = {n}"),
            )
            .await?;
            let row = rows
                .rows
                .first()
                .ok_or_else(|| AppError::InvalidInput(format!("Action #{id} was not found")))?;
            let (before_status, before_date) = (text(&row[0]).unwrap_or_default(), text(&row[1]));
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
                    sql_string(&before_status),
                    sql_value(before_date.as_deref())
                )],
            }
        }
    })
}

// ---------- editing serialized values ----------

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum NewValue {
    String(String),
    Int(String),
    Float(String),
    Bool(bool),
    Null,
}

fn php_new(v: &NewValue) -> AppResult<PhpValue> {
    Ok(match v {
        NewValue::String(s) => PhpValue::Str(s.as_bytes().to_vec()),
        NewValue::Int(s) => PhpValue::Int(
            s.trim()
                .parse()
                .map_err(|_| AppError::InvalidInput(format!("'{s}' is not an integer")))?,
        ),
        NewValue::Float(s) => {
            s.trim()
                .parse::<f64>()
                .map_err(|_| AppError::InvalidInput(format!("'{s}' is not a number")))?;
            PhpValue::Float(s.trim().to_string())
        }
        NewValue::Bool(b) => PhpValue::Bool(*b),
        NewValue::Null => PhpValue::Null,
    })
}

fn key_matches(key: &PhpKey, segment: &str) -> bool {
    match key {
        PhpKey::Int(i) => i.to_string() == segment,
        PhpKey::Str(s) => {
            let s = String::from_utf8_lossy(s);
            // Mangled property names ("\0*\0name", "\0Class\0name") are shown demangled.
            s == segment || s.rsplit('\0').next() == Some(segment)
        }
    }
}

fn set_php(value: &mut PhpValue, path: &[String], new: &NewValue) -> AppResult<()> {
    let Some((head, rest)) = path.split_first() else {
        *value = php_new(new)?;
        return Ok(());
    };
    match value {
        PhpValue::Array(items) | PhpValue::Object { props: items, .. } => {
            let (_, child) = items
                .iter_mut()
                .find(|(k, _)| key_matches(k, head))
                .ok_or_else(|| AppError::InvalidInput(format!("No key '{head}' in the value")))?;
            set_php(child, rest, new)
        }
        // "decoded" steps into serialized data stored inside a string.
        PhpValue::Str(s) if head == "decoded" => {
            let inner = String::from_utf8(s.clone())
                .map_err(|_| AppError::InvalidInput("Nested value is not UTF-8".into()))?;
            *s = edit_serialized(&inner, rest, new)?.into_bytes();
            Ok(())
        }
        _ => Err(AppError::InvalidInput(format!(
            "'{head}' is not inside an array or object"
        ))),
    }
}

fn set_json(value: &mut serde_json::Value, path: &[String], new: &NewValue) -> AppResult<()> {
    let Some((head, rest)) = path.split_first() else {
        *value = match new {
            NewValue::String(s) => serde_json::Value::String(s.clone()),
            NewValue::Int(s) | NewValue::Float(s) => {
                serde_json::from_str::<serde_json::Number>(s.trim())
                    .map(serde_json::Value::Number)
                    .map_err(|_| AppError::InvalidInput(format!("'{s}' is not a number")))?
            }
            NewValue::Bool(b) => serde_json::Value::Bool(*b),
            NewValue::Null => serde_json::Value::Null,
        };
        return Ok(());
    };
    let child = match value {
        serde_json::Value::Object(map) => map.get_mut(head.as_str()),
        serde_json::Value::Array(items) => {
            head.parse::<usize>().ok().and_then(|i| items.get_mut(i))
        }
        serde_json::Value::String(s) if head == "decoded" => {
            *s = edit_serialized(s, rest, new)?;
            return Ok(());
        }
        _ => None,
    }
    .ok_or_else(|| AppError::InvalidInput(format!("No key '{head}' in the value")))?;
    set_json(child, rest, new)
}

/// Replaces the value at `path` (keys as shown in the value tree) and re-serializes.
/// PHP output has correct byte lengths; untouched parts stay byte-identical.
pub fn edit_serialized(raw: &str, path: &[String], new: &NewValue) -> AppResult<String> {
    let trimmed = raw.trim();
    if php::looks_serialized(trimmed) {
        let mut v = php::parse(trimmed.as_bytes())
            .map_err(|e| AppError::InvalidInput(format!("Cannot edit: {e}")))?;
        set_php(&mut v, path, new)?;
        return String::from_utf8(php::serialize(&v))
            .map_err(|_| AppError::InvalidInput("Result is not UTF-8".into()));
    }
    let mut v: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|e| AppError::InvalidInput(format!("Cannot edit: {e}")))?;
    set_json(&mut v, path, new)?;
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnMeta;

    fn path(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn edits_php_values_with_correct_lengths() {
        let raw = r#"a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";}"#;
        let out = edit_serialized(
            raw,
            &path(&["title"]),
            &NewValue::String("PayPal Checkout".into()),
        )
        .unwrap();
        assert_eq!(
            out,
            r#"a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:15:"PayPal Checkout";}"#
        );
        let out =
            edit_serialized(raw, &path(&["enabled"]), &NewValue::String("nö".into())).unwrap();
        assert!(out.contains(r#"s:3:"nö""#), "lengths count bytes: {out}");
    }

    #[test]
    fn edits_nested_and_typed_values() {
        let raw = r#"a:1:{s:4:"fees";a:2:{s:5:"gross";d:95.4;s:4:"live";b:0;}}"#;
        let out = edit_serialized(raw, &path(&["fees", "live"]), &NewValue::Bool(true)).unwrap();
        assert_eq!(
            out,
            r#"a:1:{s:4:"fees";a:2:{s:5:"gross";d:95.4;s:4:"live";b:1;}}"#
        );
        let out = edit_serialized(
            raw,
            &path(&["fees", "gross"]),
            &NewValue::Float("100.5".into()),
        )
        .unwrap();
        assert!(out.contains("d:100.5;"));
        assert!(
            edit_serialized(raw, &path(&["fees", "gross"]), &NewValue::Int("x".into())).is_err()
        );
        assert!(edit_serialized(raw, &path(&["missing"]), &NewValue::Null).is_err());
    }

    #[test]
    fn edits_serialized_data_inside_strings() {
        let inner = r#"a:1:{s:1:"k";i:1;}"#;
        let raw = format!(r#"a:1:{{s:4:"data";s:{}:"{}";}}"#, inner.len(), inner);
        let out = edit_serialized(
            &raw,
            &path(&["data", "decoded", "k"]),
            &NewValue::Int("42".into()),
        )
        .unwrap();
        let new_inner = r#"a:1:{s:1:"k";i:42;}"#;
        assert_eq!(
            out,
            format!(
                r#"a:1:{{s:4:"data";s:{}:"{}";}}"#,
                new_inner.len(),
                new_inner
            )
        );
    }

    #[test]
    fn edits_protected_object_properties_by_their_shown_name() {
        let raw = "O:3:\"Foo\":1:{s:6:\"\0*\0bar\";i:1;}";
        let out = edit_serialized(raw, &path(&["bar"]), &NewValue::Int("2".into())).unwrap();
        assert_eq!(out, "O:3:\"Foo\":1:{s:6:\"\0*\0bar\";i:2;}");
    }

    #[test]
    fn edits_json_keeping_key_order() {
        let out = edit_serialized(
            r#"{"z":1,"a":{"b":"x"}}"#,
            &path(&["a", "b"]),
            &NewValue::String("y".into()),
        )
        .unwrap();
        assert_eq!(out, r#"{"z":1,"a":{"b":"y"}}"#);
        let out = edit_serialized("[1,2]", &path(&["1"]), &NewValue::Int("5".into())).unwrap();
        assert_eq!(out, "[1,5]");
    }

    #[test]
    fn log_retention_filter_targets_age_and_source() {
        assert_eq!(
            logs_before_filter(30, None),
            "`timestamp` < UTC_TIMESTAMP() - INTERVAL 30 DAY"
        );
        assert_eq!(
            logs_before_filter(7, Some("woocommerce-paypal-payments")),
            "`timestamp` < UTC_TIMESTAMP() - INTERVAL 7 DAY AND `source` = 'woocommerce-paypal-payments'"
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
            reinsert_sql("`db`.`wp_options`", &rs),
            [
                "INSERT INTO `db`.`wp_options` (`option_id`, `option_value`) VALUES ('7', NULL)",
                "INSERT INTO `db`.`wp_options` (`option_id`, `option_value`) VALUES ('8', 'it''s')",
            ]
        );
    }
}
