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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Target;
    use crate::db::{ColumnInfo, FakeDatabase, IndexInfo, ResultSet, TableStructure};
    use crate::decode::value::Format;
    use crate::query::testing::{num, result, shop, text};
    use crate::readout::testing::Rig;

    const JSON: &str =
        r#"{"qty":3,"ratio":2.5,"name":"hi","note":"","gone":null,"items":[5],"nested":{"a":1}}"#;

    fn key(column: &str, value: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: column.into(),
            value: value.into(),
        }])
    }

    fn watch(id: &str, table: &str, column: &str, row: RowKey, path: &[&str]) -> WatchedValue {
        WatchedValue {
            id: id.into(),
            label: None,
            table: table.into(),
            column: column.into(),
            row,
            path: path.iter().map(|step| step.to_string()).collect(),
        }
    }

    fn order(id: &str, column: &str, order_id: &str) -> WatchedValue {
        watch(id, "f_orders", column, key("id", order_id), &[])
    }

    fn meta(id: &str, path: &[&str]) -> WatchedValue {
        watch(id, "f_ordermeta", "v", key("meta_id", "1"), path)
    }

    fn orders_result(rows: &[(&str, &str)]) -> ResultSet {
        let rows = rows
            .iter()
            .map(|(id, status)| vec![num(id), text(status), num("1")])
            .collect();
        result(&["id", "status", "total"], rows)
    }

    fn meta_result(cell: Cell) -> ResultSet {
        result(&["meta_id", "v"], vec![vec![num("1"), cell]])
    }

    fn row_address(table: &str, key: &str) -> Address {
        Address::new(
            "local",
            "wp",
            Target::Row {
                table: table.into(),
                key: key.into(),
            },
        )
    }

    fn meta_cell_source() -> Provenance {
        Provenance {
            table: "f_ordermeta".into(),
            column: "v".into(),
            row: key("meta_id", "1"),
        }
    }

    async fn read(db: FakeDatabase, watches: &[WatchedValue]) -> Vec<WatchedLine> {
        let rig = Rig::new(db, "", &[]);
        let ctx = rig.ctx().await;
        lines(&ctx, watches).await
    }

    fn node_at(json: &str, key: &str) -> ValueNode {
        let child = decode(json)
            .unwrap()
            .root
            .unwrap()
            .children
            .into_iter()
            .find(|c| c.key == key)
            .unwrap();
        child.node
    }

    #[tokio::test]
    async fn watches_on_two_tables_run_one_query_each() {
        let db = shop()
            .on("`f_orders`", orders_result(&[("7", "paid")]))
            .on("`f_ordermeta`", meta_result(text("x")));
        let rig = Rig::new(db, "", &[]);
        let ctx = rig.ctx().await;

        lines(&ctx, &[order("a", "status", "7"), meta("b", &[])]).await;

        let mut queries = rig.db.queries();
        queries.sort();
        assert_eq!(
            queries,
            [
                "SELECT * FROM `wp`.`f_ordermeta` AS t WHERE (t.`meta_id` = '1') LIMIT 100",
                "SELECT * FROM `wp`.`f_orders` AS t WHERE (t.`id` = '7') LIMIT 100",
            ]
        );
    }

    #[tokio::test]
    async fn several_rows_of_one_table_run_one_query_with_their_conditions_ored() {
        let db = shop().on("`f_orders`", orders_result(&[("7", "paid"), ("8", "open")]));
        let rig = Rig::new(db, "", &[]);
        let ctx = rig.ctx().await;
        let watches = [
            order("a", "status", "7"),
            order("b", "status", "8"),
            order("c", "total", "7"),
        ];

        let lines = lines(&ctx, &watches).await;

        assert_eq!(
            rig.db.queries(),
            ["SELECT * FROM `wp`.`f_orders` AS t WHERE (t.`id` = '7') OR (t.`id` = '8') LIMIT 100"]
        );
        let values: Vec<_> = lines
            .iter()
            .map(|l| l.value.as_ref().map(|v| v.raw.clone()))
            .collect();
        assert_eq!(
            values,
            [Some(text("paid")), Some(text("open")), Some(num("1"))]
        );
    }

    #[tokio::test]
    async fn lines_come_in_the_order_the_watches_are_stored() {
        let db = shop()
            .on("`f_orders`", orders_result(&[("7", "paid"), ("8", "open")]))
            .on("`f_ordermeta`", meta_result(text("x")));
        let watches = [
            order("a", "status", "7"),
            meta("b", &[]),
            order("c", "status", "8"),
        ];

        let lines = read(db, &watches).await;

        let ids: Vec<&str> = lines.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn a_whole_cell_carries_its_kind_and_the_provenance_of_the_row() {
        let db = shop().on("`f_orders`", orders_result(&[("7", "paid")]));
        let source = Provenance {
            table: "f_orders".into(),
            column: "status".into(),
            row: key("id", "7"),
        };

        let lines = read(db, &[order("w", "status", "7")]).await;

        assert_eq!(
            lines,
            [WatchedLine {
                id: "w".into(),
                label: "f_orders.status".into(),
                labelled: false,
                source: source.clone(),
                path: Vec::new(),
                address: Some(row_address("f_orders", "7")),
                value: Some(Value {
                    raw: text("paid"),
                    kind: ValueKind::Text,
                    source: Some(source),
                    style: None,
                    secondary: None,
                }),
                tree: None,
                missing: None,
                error: None,
            }]
        );
    }

    #[tokio::test]
    async fn a_null_cell_without_a_path_is_a_null_value() {
        let db = shop().on("`f_ordermeta`", meta_result(Cell::Null));

        let lines = read(db, &[meta("w", &[])]).await;

        let value = lines[0].value.as_ref().unwrap();
        assert_eq!((&value.raw, &value.kind), (&Cell::Null, &ValueKind::Null));
        assert_eq!(lines[0].missing, None);
    }

    #[tokio::test]
    async fn a_path_to_a_leaf_gives_a_value_of_its_own_kind_with_the_cells_provenance() {
        let cases: [(&[&str], Cell, ValueKind); 6] = [
            (&["qty"], num("3"), ValueKind::Number),
            (&["ratio"], num("2.5"), ValueKind::Number),
            (&["name"], text("hi"), ValueKind::Text),
            (&["note"], text(""), ValueKind::Empty),
            (&["gone"], Cell::Null, ValueKind::Null),
            (&["nested", "a"], num("1"), ValueKind::Number),
        ];
        let watches: Vec<WatchedValue> = cases
            .iter()
            .enumerate()
            .map(|(i, (path, _, _))| meta(&i.to_string(), path))
            .collect();
        let db = shop().on("`f_ordermeta`", meta_result(text(JSON)));

        let lines = read(db, &watches).await;

        for (line, (path, raw, kind)) in lines.iter().zip(cases) {
            let expected = Value {
                raw,
                kind,
                source: Some(meta_cell_source()),
                style: None,
                secondary: None,
            };
            assert_eq!(line.value.as_ref(), Some(&expected), "{path:?}");
            assert_eq!(
                (&line.tree, &line.missing, &line.error),
                (&None, &None, &None)
            );
            assert_eq!(line.address, Some(row_address("f_ordermeta", "1")));
        }
    }

    #[tokio::test]
    async fn a_path_through_a_php_value_reaches_its_leaf() {
        let php = r#"a:1:{s:3:"tax";a:1:{s:4:"rate";d:0.19;}}"#;
        let db = shop().on("`f_ordermeta`", meta_result(text(php)));

        let lines = read(db, &[meta("w", &["tax", "rate"])]).await;

        let value = lines[0].value.as_ref().unwrap();
        assert_eq!(
            (&value.raw, &value.kind),
            (&num("0.19"), &ValueKind::Number)
        );
    }

    #[tokio::test]
    async fn a_path_to_an_array_or_object_gives_a_tree_of_that_node() {
        let db = shop().on("`f_ordermeta`", meta_result(text(JSON)));

        let lines = read(db, &[meta("a", &["items"]), meta("b", &["nested"])]).await;

        for (line, key) in lines.iter().zip(["items", "nested"]) {
            assert_eq!(
                line.tree,
                Some(DecodedValue {
                    format: Format::Json,
                    root: Some(node_at(JSON, key)),
                    warning: None,
                    error: None,
                }),
                "{key}"
            );
            assert_eq!(
                (&line.value, &line.missing, &line.error),
                (&None, &None, &None)
            );
        }
    }

    #[tokio::test]
    async fn a_watch_says_why_it_has_no_value() {
        let cases: Vec<(FakeDatabase, WatchedValue, &str)> = vec![
            (
                shop(),
                watch("w", "f_nope", "x", key("id", "1"), &[]),
                "no table `f_nope`",
            ),
            (
                shop(),
                watch("w", "f_orders", "status", key("nope", "7"), &[]),
                "`f_orders` has no column `nope`",
            ),
            (
                shop().on("`f_orders`", orders_result(&[("7", "paid")])),
                order("w", "nope", "7"),
                "`f_orders` has no column `nope`",
            ),
            (
                shop().on("`f_orders`", orders_result(&[])),
                order("w", "status", "7"),
                "no row id = '7' in `f_orders`",
            ),
            (
                shop().on("`f_orders`", orders_result(&[("8", "paid")])),
                order("w", "status", "7"),
                "no row id = '7' in `f_orders`",
            ),
            (
                shop().on("`f_ordermeta`", meta_result(text(r#"{"a":1}"#))),
                meta("w", &["b"]),
                "no key `b` in `v`",
            ),
            (
                shop().on("`f_ordermeta`", meta_result(text(r#"{"a":{"x":1}}"#))),
                meta("w", &["a", "y"]),
                "no key `y` in `v › a`",
            ),
            (
                shop().on("`f_ordermeta`", meta_result(Cell::Null)),
                meta("w", &["a"]),
                "`v` is NULL, nothing to decode",
            ),
            (
                shop().on("`f_ordermeta`", meta_result(text("hello"))),
                meta("w", &["a"]),
                "`v` holds no PHP-serialized or JSON value",
            ),
        ];

        for (db, watch, why) in cases {
            let lines = read(db, std::slice::from_ref(&watch)).await;

            assert_eq!(lines[0].missing.as_deref(), Some(why), "{watch:?}");
            assert_eq!(
                (&lines[0].value, &lines[0].tree, &lines[0].error),
                (&None, &None, &None),
                "{watch:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_failed_read_gives_every_watch_of_that_table_an_error_and_spares_the_others() {
        let db = shop()
            .failing("`f_orders`", AppError::InvalidInput("boom".into()))
            .on("`f_ordermeta`", meta_result(text("x")));
        let watches = [
            order("a", "status", "7"),
            order("b", "status", "8"),
            meta("c", &[]),
        ];

        let lines = read(db, &watches).await;

        let outcome: Vec<_> = lines
            .iter()
            .map(|l| (l.error.as_deref(), l.value.is_some(), l.missing.is_some()))
            .collect();
        assert_eq!(
            outcome,
            [
                (Some("boom"), false, false),
                (Some("boom"), false, false),
                (None, true, false),
            ]
        );
    }

    #[tokio::test]
    async fn a_value_that_does_not_decode_carries_an_error_naming_the_column() {
        let db = shop().on("`f_ordermeta`", meta_result(text("a:1:{x}")));

        let lines = read(db, &[meta("w", &["a"])]).await;

        let error = lines[0].error.as_deref().unwrap();
        assert!(error.starts_with("`v` does not decode: "), "{error}");
        assert_eq!(
            (&lines[0].value, &lines[0].tree, &lines[0].missing),
            (&None, &None, &None)
        );
    }

    fn people_watch(id: &str, name: &str) -> WatchedValue {
        watch(id, "f_people", "name", key("name", name), &[])
    }

    #[tokio::test]
    async fn a_row_whose_key_differs_only_in_case_is_found() {
        let rows = result(&["person_id", "name"], vec![vec![num("3"), text("Ada")]]);
        let db = shop().on("`f_people`", rows);

        let lines = read(db, &[people_watch("w", "ADA")]).await;

        assert_eq!(lines[0].value.as_ref().unwrap().raw, text("Ada"));
        assert_eq!(lines[0].address, Some(row_address("f_people", "3")));
        assert_eq!(lines[0].missing, None);
    }

    #[tokio::test]
    async fn an_exact_key_match_wins_over_a_case_variant() {
        let rows = result(
            &["person_id", "name"],
            vec![vec![num("1"), text("Ada")], vec![num("2"), text("ADA")]],
        );
        let db = shop().on("`f_people`", rows);

        let lines = read(db, &[people_watch("a", "ADA"), people_watch("b", "ada")]).await;

        let found: Vec<_> = lines
            .iter()
            .map(|l| (l.address.clone(), l.value.as_ref().unwrap().raw.clone()))
            .collect();
        assert_eq!(
            found,
            [
                (Some(row_address("f_people", "2")), text("ADA")),
                (Some(row_address("f_people", "1")), text("Ada")),
            ]
        );
    }

    #[tokio::test]
    async fn a_label_is_the_users_or_table_column_and_path() {
        let db = shop()
            .on("`f_orders`", orders_result(&[("7", "paid")]))
            .on("`f_ordermeta`", meta_result(text("x")));
        let mut named = meta("c", &["a", "b"]);
        named.label = Some("Site".into());
        let watches = [order("a", "status", "7"), meta("b", &["a", "b"]), named];

        let lines = read(db, &watches).await;

        let labels: Vec<_> = lines
            .iter()
            .map(|l| (l.label.as_str(), l.labelled))
            .collect();
        assert_eq!(
            labels,
            [
                ("f_orders.status", false),
                ("f_ordermeta.v › a › b", false),
                ("Site", true),
            ]
        );
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            column_type: "text".into(),
            nullable: true,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        }
    }

    fn index(name: &str, unique: bool, columns: &[&str]) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            unique,
            columns: columns.iter().map(|c| c.to_string()).collect(),
        }
    }

    fn options_db(indexes: Vec<IndexInfo>, result: Option<ResultSet>) -> FakeDatabase {
        let structure = TableStructure {
            columns: ["option_id", "option_name", "option_value", "blog_id"]
                .into_iter()
                .map(column)
                .collect(),
            indexes,
            primary_key: vec!["option_id".into()],
            foreign_keys: Vec::new(),
            ddl: None,
        };
        let db = shop().with_table("wp", "f_options", structure);
        match result {
            Some(result) => db.on("AS t WHERE t.`option_id`", result),
            None => db,
        }
    }

    fn option_source() -> Provenance {
        Provenance {
            table: "f_options".into(),
            column: "option_value".into(),
            row: key("option_id", "5"),
        }
    }

    async fn identify(db: FakeDatabase, source: &Provenance) -> (AppResult<RowKey>, Vec<String>) {
        let rig = Rig::new(db, "", &[]);
        let ctx = rig.ctx().await;
        let identity = identity(&ctx, source).await;
        (identity, rig.db.queries())
    }

    fn primary() -> IndexInfo {
        index("PRIMARY", true, &["option_id"])
    }

    #[tokio::test]
    async fn identity_prefers_the_unique_key_with_the_fewest_columns() {
        let db = options_db(
            vec![
                primary(),
                index("blog_option", true, &["blog_id", "option_name"]),
                index("option_name", true, &["option_name"]),
                index("autoload", false, &["blog_id"]),
            ],
            Some(result(&["option_name"], vec![vec![text("siteurl")]])),
        );

        let (identity, queries) = identify(db, &option_source()).await;

        assert_eq!(identity.unwrap(), key("option_name", "siteurl"));
        assert_eq!(
            queries,
            ["SELECT t.`option_name` FROM `wp`.`f_options` AS t WHERE t.`option_id` = '5' LIMIT 2"]
        );
    }

    #[tokio::test]
    async fn identity_by_a_composite_unique_key_names_every_column() {
        let db = options_db(
            vec![
                primary(),
                index("blog_option", true, &["blog_id", "option_name"]),
            ],
            Some(result(
                &["blog_id", "option_name"],
                vec![vec![num("1"), text("siteurl")]],
            )),
        );

        let (identity, queries) = identify(db, &option_source()).await;

        assert_eq!(
            identity.unwrap(),
            RowKey(vec![
                KeyPart {
                    column: "blog_id".into(),
                    value: "1".into()
                },
                KeyPart {
                    column: "option_name".into(),
                    value: "siteurl".into()
                },
            ])
        );
        assert_eq!(
            queries,
            [
                "SELECT t.`blog_id`, t.`option_name` FROM `wp`.`f_options` AS t WHERE t.`option_id` = '5' LIMIT 2"
            ]
        );
    }

    #[tokio::test]
    async fn identity_is_the_primary_key_without_another_unique_key_and_asks_nothing() {
        let db = options_db(
            vec![primary(), index("autoload", false, &["blog_id"])],
            None,
        );

        let (identity, queries) = identify(db, &option_source()).await;

        assert_eq!(identity.unwrap(), key("option_id", "5"));
        assert!(queries.is_empty(), "{queries:?}");
    }

    #[tokio::test]
    async fn identity_falls_back_to_the_primary_key_when_the_unique_key_has_no_usable_value() {
        let by_name = || vec![primary(), index("option_name", true, &["option_name"])];
        let cases = [
            ("no row", result(&["option_name"], Vec::new())),
            ("NULL", result(&["option_name"], vec![vec![Cell::Null]])),
            (
                "two rows",
                result(&["option_name"], vec![vec![text("a")], vec![text("b")]]),
            ),
        ];

        for (what, rows) in cases {
            let db = options_db(by_name(), Some(rows));

            let (identity, _) = identify(db, &option_source()).await;

            assert_eq!(identity.unwrap(), key("option_id", "5"), "{what}");
        }
    }

    #[tokio::test]
    async fn identity_refuses_what_the_schema_does_not_have() {
        let named = |table: &str, column: &str, row: RowKey| Provenance {
            table: table.into(),
            column: column.into(),
            row,
        };
        let cases = [
            (named("f_nope", "x", key("id", "1")), "No table `f_nope`"),
            (
                named("f_options", "nope", key("option_id", "5")),
                "`f_options` has no column `nope`",
            ),
            (
                named("f_options", "option_value", key("nope", "5")),
                "`f_options` has no column `nope`",
            ),
            (
                named("f_options", "option_value", RowKey(Vec::new())),
                "A row of `f_options` without a primary key cannot be watched",
            ),
        ];

        for (source, expected) in cases {
            let db = options_db(vec![primary()], None);

            let (identity, queries) = identify(db, &source).await;

            let err = identity.unwrap_err();
            let AppError::InvalidInput(message) = &err else {
                panic!("{err:?}");
            };
            assert_eq!(message, expected);
            assert!(queries.is_empty(), "{queries:?}");
        }
    }
}
