//! Watched values: one column of one row, or a node of its decoded value, that the readout shows
//! as it is now. The watches live in the project (`Project::watched`); each table they name is
//! read with one query.

use std::collections::BTreeMap;

use futures_util::future::join_all;
use serde::Serialize;
use ts_rs::TS;

use crate::address::Address;
use crate::db::browse::aliased_equals_sql;
use crate::db::{Cell, ColumnEquals};
use crate::decode::value::{DecodedValue, NodeKind, ValueNode, decode};
use crate::error::{AppError, AppResult};
use crate::query::{Ctx, Rows};
use crate::rules::project::WatchedValue;
use crate::value::{KeyPart, Provenance, RowKey, Value, ValueKind};

/// Rows one table's read returns at most; a watched row is one row unless its key stopped
/// being unique.
const TABLE_ROWS: usize = 100;

/// A watched value as it is now. Exactly one of `value`, `tree`, `missing` and `error` is set.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct WatchedLine {
    pub id: String,
    /// The user's label, else `<table>.<column> › path`.
    pub label: String,
    /// Whether `label` is the user's.
    pub labelled: bool,
    /// Where the watch reads: the table, the column and the row as the watch names it.
    pub source: Provenance,
    pub path: Vec<String>,
    /// The row, once it was found.
    pub address: Option<Address>,
    /// The cell, or the leaf at `path`; sourced from the row's primary key.
    pub value: Option<Value>,
    /// The array or object at `path`.
    pub tree: Option<DecodedValue>,
    /// Why there is no value: no such table, column, row or key.
    pub missing: Option<String>,
    pub error: Option<String>,
}

impl WatchedLine {
    fn new(watched: &WatchedValue) -> Self {
        Self {
            id: watched.id.clone(),
            label: watched.label(),
            labelled: watched.label.is_some(),
            source: Provenance {
                table: watched.table.clone(),
                column: watched.column.clone(),
                row: watched.row.clone(),
            },
            path: watched.path.clone(),
            address: None,
            value: None,
            tree: None,
            missing: None,
            error: None,
        }
    }

    fn missing(mut self, why: String) -> Self {
        self.missing = Some(why);
        self
    }
}

/// Every watch's line, in the watches' order: one query per table they name.
pub async fn lines(ctx: &Ctx<'_>, watched: &[WatchedValue]) -> Vec<WatchedLine> {
    let mut by_table: BTreeMap<&str, Vec<&WatchedValue>> = BTreeMap::new();
    for w in watched {
        by_table.entry(&w.table).or_default().push(w);
    }
    let read = join_all(
        by_table
            .iter()
            .map(|(table, members)| table_lines(ctx, table, members)),
    )
    .await;
    let mut by_id: BTreeMap<String, WatchedLine> = read
        .into_iter()
        .flatten()
        .map(|line| (line.id.clone(), line))
        .collect();
    watched.iter().filter_map(|w| by_id.remove(&w.id)).collect()
}

async fn table_lines(ctx: &Ctx<'_>, table: &str, members: &[&WatchedValue]) -> Vec<WatchedLine> {
    if ctx.schema.table(table).is_none() {
        return members
            .iter()
            .map(|w| WatchedLine::new(w).missing(format!("no table `{table}`")))
            .collect();
    }
    let has = |name: &str| has_column(ctx, table, name);
    let mut keys: Vec<&RowKey> = Vec::new();
    for w in members {
        if w.row.0.iter().all(|p| has(&p.column)) && !keys.contains(&&w.row) {
            keys.push(&w.row);
        }
    }
    let rows = if keys.is_empty() {
        None
    } else {
        Some(read_rows(ctx, table, &keys).await)
    };
    members
        .iter()
        .map(|w| line(ctx, w, rows.as_ref()))
        .collect()
}

fn has_column(ctx: &Ctx<'_>, table: &str, name: &str) -> bool {
    ctx.schema
        .columns(table)
        .iter()
        .any(|c| c.eq_ignore_ascii_case(name))
}

/// The rows of `table` that any of `keys` names, in one query.
async fn read_rows(ctx: &Ctx<'_>, table: &str, keys: &[&RowKey]) -> AppResult<Rows> {
    let conditions: Vec<String> = keys
        .iter()
        .map(|key| format!("({})", aliased_equals_sql(ctx.dialect, "t", &equals(key))))
        .collect();
    let sql = format!(
        "SELECT * FROM {} AS t WHERE {} LIMIT {TABLE_ROWS}",
        ctx.table_ref(table),
        conditions.join(" OR ")
    );
    ctx.rows(table, &sql).await
}

fn equals(key: &RowKey) -> Vec<ColumnEquals> {
    key.0
        .iter()
        .map(|p| ColumnEquals {
            column: p.column.clone(),
            value: Some(p.value.clone()),
        })
        .collect()
}

fn line(ctx: &Ctx<'_>, watched: &WatchedValue, rows: Option<&AppResult<Rows>>) -> WatchedLine {
    let line = WatchedLine::new(watched);
    let table = &watched.table;
    let absent = watched
        .row
        .0
        .iter()
        .map(|p| p.column.as_str())
        .chain([watched.column.as_str()])
        .find(|c| !has_column(ctx, table, c));
    if let Some(column) = absent {
        return line.missing(format!("`{table}` has no column `{column}`"));
    }
    let rows = match rows {
        Some(Ok(rows)) => rows,
        Some(Err(e)) => {
            return WatchedLine {
                error: Some(e.to_string()),
                ..line
            };
        }
        None => return line.missing(format!("no row {} in `{table}`", key_text(&watched.row))),
    };
    let Some(row) = find_row(rows, &watched.row) else {
        return line.missing(format!("no row {} in `{table}`", key_text(&watched.row)));
    };
    let Some(col) = rows.column(&watched.column) else {
        return line.missing(format!("`{table}` has no column `{}`", watched.column));
    };
    let value = rows.row_values(row, &|_, _| None).swap_remove(col);
    let line = WatchedLine {
        address: rows.key(row).map(|key| ctx.key_address(table, &key)),
        ..line
    };
    if watched.path.is_empty() {
        return WatchedLine {
            value: Some(value),
            ..line
        };
    }
    at_path(line, value, &watched.path)
}

/// `option_name = 'x', blog_id = '1'`: the key as the watch names it.
fn key_text(key: &RowKey) -> String {
    key.0
        .iter()
        .map(|p| format!("{} = '{}'", p.column, p.value))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The row whose key columns hold the key's values; exactly, else ignoring ASCII case, as a
/// case-insensitive collation matched it.
fn find_row(rows: &Rows, key: &RowKey) -> Option<usize> {
    let holds = |row: usize, same: fn(&str, &str) -> bool| {
        key.0
            .iter()
            .all(|p| matches!(rows.text(row, &p.column), Some(Some(text)) if same(text, &p.value)))
    };
    (0..rows.len())
        .find(|&row| holds(row, |a, b| a == b))
        .or_else(|| (0..rows.len()).find(|&row| holds(row, str::eq_ignore_ascii_case)))
}

/// The node at `path` in the cell's decoded value: a leaf as a value, an array or object as a
/// tree.
fn at_path(line: WatchedLine, cell: Value, path: &[String]) -> WatchedLine {
    let column = line.source.column.clone();
    let Some(text) = cell.raw.as_text() else {
        let what = if cell.raw == Cell::Null {
            "NULL"
        } else {
            "binary"
        };
        return line.missing(format!("`{column}` is {what}, nothing to decode"));
    };
    let Some(decoded) = decode(text) else {
        return line.missing(format!("`{column}` holds no PHP-serialized or JSON value"));
    };
    let Some(root) = &decoded.root else {
        let error = decoded.error.clone().unwrap_or_default();
        return WatchedLine {
            error: Some(format!("`{column}` does not decode: {error}")),
            ..line
        };
    };
    let mut node = root;
    for (depth, step) in path.iter().enumerate() {
        let Some(child) = node.children.iter().find(|c| &c.key == step) else {
            return line.missing(format!(
                "no key `{step}` in {}",
                place(&column, &path[..depth])
            ));
        };
        node = &child.node;
    }
    if node.children.is_empty() && !matches!(node.kind, NodeKind::Array | NodeKind::Object) {
        return WatchedLine {
            value: Some(leaf_value(node, cell.source)),
            ..line
        };
    }
    WatchedLine {
        tree: Some(DecodedValue {
            format: decoded.format,
            root: Some(node.clone()),
            warning: decoded.warning.clone(),
            error: None,
        }),
        ..line
    }
}

/// `option_value › a › b`, where a path step was looked for.
fn place(column: &str, path: &[String]) -> String {
    let steps: String = path.iter().map(|step| format!(" › {step}")).collect();
    format!("`{column}{steps}`")
}

/// A leaf of a decoded value as a value with the cell's provenance: numbers as numbers, an empty
/// string as empty, null as NULL, anything else as its text.
fn leaf_value(node: &ValueNode, source: Option<Provenance>) -> Value {
    let text = node.text.clone().unwrap_or_default();
    let (raw, kind) = match node.kind {
        NodeKind::Null => (Cell::Null, ValueKind::Null),
        NodeKind::Int | NodeKind::Float => (Cell::Number(text), ValueKind::Number),
        NodeKind::String if text.is_empty() => (Cell::Text(text), ValueKind::Empty),
        _ => (Cell::Text(text), ValueKind::Text),
    };
    Value {
        raw,
        kind,
        source,
        style: None,
        secondary: None,
    }
}

/// The row a new watch names: by the table's unique key with the fewest columns other than the
/// primary key when the row holds a value in each of them (so a deleted and re-added
/// `wp_options` row is still found by its `option_name`), else by its primary key.
pub async fn identity(ctx: &Ctx<'_>, source: &Provenance) -> AppResult<RowKey> {
    let table = &source.table;
    if ctx.schema.table(table).is_none() {
        return Err(AppError::InvalidInput(format!("No table `{table}`")));
    }
    if source.row.0.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "A row of `{table}` without a primary key cannot be watched"
        )));
    }
    let named = source
        .row
        .0
        .iter()
        .map(|p| p.column.as_str())
        .chain([source.column.as_str()]);
    if let Some(column) = named.into_iter().find(|c| !has_column(ctx, table, c)) {
        return Err(AppError::InvalidInput(format!(
            "`{table}` has no column `{column}`"
        )));
    }
    let Some(unique) = unique_key(ctx, table) else {
        return Ok(source.row.clone());
    };
    let columns: Vec<String> = unique.iter().map(|c| ctx.column(Some("t"), c)).collect();
    let sql = format!(
        "SELECT {} FROM {} AS t WHERE {} LIMIT 2",
        columns.join(", "),
        ctx.table_ref(table),
        aliased_equals_sql(ctx.dialect, "t", &equals(&source.row))
    );
    let result = ctx.fetch(&sql).await?;
    let [row] = result.rows.as_slice() else {
        return Ok(source.row.clone());
    };
    let parts: Option<Vec<KeyPart>> = unique
        .iter()
        .zip(row)
        .map(|(column, cell)| {
            Some(KeyPart {
                column: column.clone(),
                value: cell.as_text()?.to_string(),
            })
        })
        .collect();
    Ok(parts.map_or_else(|| source.row.clone(), RowKey))
}

fn unique_key<'c>(ctx: &'c Ctx<'_>, table: &str) -> Option<&'c [String]> {
    let keys = ctx.schema.keys(table)?;
    keys.indexes
        .iter()
        .filter(|i| i.unique && i.name != "PRIMARY" && i.columns != keys.primary_key)
        .min_by_key(|i| i.columns.len())
        .map(|i| i.columns.as_slice())
}
