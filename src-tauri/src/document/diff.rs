//! Live diff, snapshots and compare. A document is reduced to its stored cells
//! (`FlatDocument`), keyed by provenance: a group, a row key and a column. Two of them give a
//! `DocDelta`, the marks the page draws on the newer document.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Document, GroupLink, GroupShape};
use crate::db::Cell;
use crate::value::{Provenance, RowKey, Value, ValueKind};

/// A document's stored cells, group by group; `events` repeats values of other groups and is
/// left out, and so is a group whose query failed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlatDocument {
    pub groups: Vec<FlatGroup>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlatGroup {
    pub id: String,
    pub table: String,
    pub shape: GroupShape,
    /// Rows beyond the cap were not read, so a row missing from a capped group proves nothing.
    pub capped: bool,
    pub rows: Vec<FlatRow>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlatRow {
    pub key: Option<RowKey>,
    pub fields: Vec<FlatField>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlatField {
    pub name: String,
    pub source: Option<Provenance>,
    pub cell: Cell,
}

/// What changed from one document to another, as marks on the newer one.
#[derive(Serialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct DocDelta {
    /// Values of rows in both documents whose stored value differs, in key order: by table, row
    /// key, then column order. Changes within one poll have no order of their own.
    pub changed: Vec<ValueChange>,
    /// Rows only in the newer document (in compare: only in the compared row's).
    pub added: Vec<RowMark>,
    /// Rows only in the older document, with their old cells.
    pub removed: Vec<RowMark>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ValueChange {
    pub group: String,
    /// The column, or the meta key of a meta value.
    pub label: String,
    /// Where the new value is stored.
    pub source: Provenance,
    /// Where the old value was stored; another row than `source` only in compare.
    pub old_source: Provenance,
    pub old: Cell,
    pub new: Cell,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RowMark {
    pub group: String,
    pub table: String,
    pub key: Option<RowKey>,
    /// The meta key of a meta row, else the row's key as `column value`.
    pub label: String,
    /// The value of a meta row; `None` for other rows.
    pub value: Option<Cell>,
    /// The row's fields as its group shows them, from the stored cells.
    pub fields: Vec<(String, Value)>,
}

impl FlatDocument {
    pub fn of(document: &Document) -> Self {
        let groups = document
            .groups
            .iter()
            .filter(|g| g.error.is_none() && !matches!(g.via, GroupLink::Events))
            .map(|g| FlatGroup {
                id: g.id.clone(),
                table: g.table.clone(),
                shape: g.shape,
                capped: g.capped,
                rows: g
                    .rows
                    .iter()
                    .map(|row| FlatRow {
                        key: row.key.clone(),
                        fields: row
                            .fields
                            .iter()
                            .map(|(name, value)| FlatField {
                                name: name.clone(),
                                source: value.source.clone(),
                                cell: value.raw.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        Self { groups }
    }

    fn group(&self, id: &str) -> Option<&FlatGroup> {
        self.groups.iter().find(|g| g.id == id)
    }
}

/// The changes from `old` to `new`, two reads of the same row: rows match by their key, values
/// by column. Groups in only one of them (a failed query) are left out.
pub fn diff_documents(old: &FlatDocument, new: &FlatDocument) -> DocDelta {
    let mut delta = DocDelta::default();
    for group in &new.groups {
        if let Some(before) = old.group(&group.id) {
            diff_group(before, group, &mut delta);
        }
    }
    delta.sort();
    delta
}

fn diff_group(old: &FlatGroup, new: &FlatGroup, delta: &mut DocDelta) {
    let before: HashMap<&RowKey, &FlatRow> = old
        .rows
        .iter()
        .filter_map(|row| Some((row.key.as_ref()?, row)))
        .collect();
    for row in &new.rows {
        let Some(key) = &row.key else { continue };
        match before.get(key) {
            Some(old_row) => changed_fields(new, old_row, row, delta),
            None if !old.capped => delta.added.push(mark(new, row)),
            None => {}
        }
    }
    if new.capped {
        return;
    }
    let now: HashSet<&RowKey> = new.rows.iter().filter_map(|r| r.key.as_ref()).collect();
    delta.removed.extend(
        old.rows
            .iter()
            .filter(|row| row.key.as_ref().is_some_and(|key| !now.contains(key)))
            .map(|row| mark(old, row)),
    );
}

/// The differences between row `left` and row `right` of one table, `left` as the old side:
/// groups match by id, the own row column by column, meta rows by key (the n-th row of a key
/// with the n-th), other rows by position.
pub fn compare_documents(left: &FlatDocument, right: &FlatDocument) -> DocDelta {
    let mut delta = DocDelta::default();
    for group in &right.groups {
        let Some(before) = left.group(&group.id) else {
            continue;
        };
        let pairs = match group.shape {
            GroupShape::Meta => pairs_by_meta_key(before, group),
            GroupShape::Fields | GroupShape::Rows => pairs_by_position(before, group),
        };
        for pair in pairs {
            match pair {
                Pair::Both(old_row, new_row) => changed_fields(group, old_row, new_row, &mut delta),
                Pair::Right(row) if !before.capped => delta.added.push(mark(group, row)),
                Pair::Left(row) if !group.capped => delta.removed.push(mark(before, row)),
                Pair::Right(_) | Pair::Left(_) => {}
            }
        }
    }
    delta.sort();
    delta
}

enum Pair<'a> {
    Both(&'a FlatRow, &'a FlatRow),
    Left(&'a FlatRow),
    Right(&'a FlatRow),
}

fn pairs_by_position<'a>(left: &'a FlatGroup, right: &'a FlatGroup) -> Vec<Pair<'a>> {
    let shared = left.rows.len().min(right.rows.len());
    let both = left
        .rows
        .iter()
        .zip(&right.rows)
        .map(|(l, r)| Pair::Both(l, r));
    both.chain(left.rows[shared..].iter().map(Pair::Left))
        .chain(right.rows[shared..].iter().map(Pair::Right))
        .collect()
}

fn pairs_by_meta_key<'a>(left: &'a FlatGroup, right: &'a FlatGroup) -> Vec<Pair<'a>> {
    let occurrences = |group: &'a FlatGroup| {
        let mut seen: HashMap<String, usize> = HashMap::new();
        group
            .rows
            .iter()
            .map(|row| {
                let key = meta_key(row).unwrap_or_default();
                let n = seen.entry(key.clone()).or_default();
                *n += 1;
                ((key, *n), row)
            })
            .collect::<Vec<_>>()
    };
    let lefts = occurrences(left);
    let rights = occurrences(right);
    let by_key: HashMap<&(String, usize), &FlatRow> = lefts.iter().map(|(k, r)| (k, *r)).collect();
    let right_keys: HashSet<&(String, usize)> = rights.iter().map(|(k, _)| k).collect();
    let mut pairs: Vec<Pair> = rights
        .iter()
        .map(|(k, row)| match by_key.get(k) {
            Some(old_row) => Pair::Both(old_row, row),
            None => Pair::Right(row),
        })
        .collect();
    pairs.extend(
        lefts
            .iter()
            .filter(|(k, _)| !right_keys.contains(k))
            .map(|(_, row)| Pair::Left(row)),
    );
    pairs
}

fn changed_fields(group: &FlatGroup, old: &FlatRow, new: &FlatRow, delta: &mut DocDelta) {
    for field in &new.fields {
        let Some(source) = &field.source else {
            continue;
        };
        let Some(before) = old.fields.iter().find(|f| f.name == field.name) else {
            continue;
        };
        if before.cell == field.cell {
            continue;
        }
        delta.changed.push(ValueChange {
            group: group.id.clone(),
            label: label(group, new, field),
            source: source.clone(),
            old_source: before.source.clone().unwrap_or_else(|| source.clone()),
            old: before.cell.clone(),
            new: field.cell.clone(),
        });
    }
}

/// The meta key of a meta value (the second field of a meta row), else the column.
fn label(group: &FlatGroup, row: &FlatRow, field: &FlatField) -> String {
    let is_meta_value = group.shape == GroupShape::Meta
        && row.fields.get(1).is_some_and(|f| std::ptr::eq(f, field));
    match meta_key(row) {
        Some(key) if is_meta_value => key,
        _ => field.name.clone(),
    }
}

fn meta_key(row: &FlatRow) -> Option<String> {
    row.fields.first()?.cell.as_text().map(str::to_string)
}

fn mark(group: &FlatGroup, row: &FlatRow) -> RowMark {
    let meta = group.shape == GroupShape::Meta;
    let label = match (meta, meta_key(row), &row.key) {
        (true, Some(key), _) => key,
        (_, _, Some(key)) => key_text(key),
        _ => String::new(),
    };
    RowMark {
        group: group.id.clone(),
        table: group.table.clone(),
        key: row.key.clone(),
        label,
        value: row.fields.get(1).filter(|_| meta).map(|f| f.cell.clone()),
        fields: row
            .fields
            .iter()
            .map(|f| (f.name.clone(), stored_value(&f.cell, f.source.clone())))
            .collect(),
    }
}

/// `meta_id 9`, `order_id 4 · product_id 7`.
fn key_text(key: &RowKey) -> String {
    key.0
        .iter()
        .map(|part| format!("{} {}", part.column, part.value))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A value read back from a stored cell: its kind is what the cell alone says.
fn stored_value(cell: &Cell, source: Option<Provenance>) -> Value {
    let kind = match cell {
        Cell::Null => ValueKind::Null,
        Cell::Text(text) if text.is_empty() => ValueKind::Empty,
        Cell::Bytes { .. } => ValueKind::Binary,
        Cell::Number(_) => ValueKind::Number,
        _ => ValueKind::Text,
    };
    Value {
        raw: cell.clone(),
        kind,
        source,
        style: None,
        secondary: None,
    }
}

impl DocDelta {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }

    fn sort(&mut self) {
        self.changed.sort_by(|a, b| {
            a.source
                .table
                .cmp(&b.source.table)
                .then_with(|| key_order(&a.source.row, &b.source.row))
        });
        let by_key = |a: &RowMark, b: &RowMark| {
            a.table.cmp(&b.table).then_with(|| match (&a.key, &b.key) {
                (Some(x), Some(y)) => key_order(x, y),
                _ => Ordering::Equal,
            })
        };
        self.added.sort_by(by_key);
        self.removed.sort_by(by_key);
    }
}

/// Key values compare as numbers when both are integers, else as text.
fn key_order(a: &RowKey, b: &RowKey) -> Ordering {
    for (x, y) in a.0.iter().zip(&b.0) {
        let order = match (x.value.parse::<i128>(), y.value.parse::<i128>()) {
            (Ok(m), Ok(n)) => m.cmp(&n),
            _ => x.value.cmp(&y.value),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.0.len().cmp(&b.0.len())
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::value::KeyPart;

    pub fn key(column: &str, value: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: column.into(),
            value: value.into(),
        }])
    }

    pub fn prov(table: &str, column: &str, key_column: &str, id: &str) -> Provenance {
        Provenance {
            table: table.into(),
            column: column.into(),
            row: key(key_column, id),
        }
    }

    pub fn text(value: &str) -> Cell {
        Cell::Text(value.into())
    }

    pub fn number(value: &str) -> Cell {
        Cell::Number(value.into())
    }

    pub fn sourced(table: &str, key_column: &str, id: &str, name: &str, cell: Cell) -> FlatField {
        FlatField {
            name: name.into(),
            source: Some(prov(table, name, key_column, id)),
            cell,
        }
    }

    pub fn row(key_column: &str, id: &str, fields: Vec<FlatField>) -> FlatRow {
        FlatRow {
            key: Some(key(key_column, id)),
            fields,
        }
    }

    pub fn group(
        id: &str,
        table: &str,
        shape: GroupShape,
        capped: bool,
        rows: Vec<FlatRow>,
    ) -> FlatGroup {
        FlatGroup {
            id: id.into(),
            table: table.into(),
            shape,
            capped,
            rows,
        }
    }

    pub fn doc(groups: Vec<FlatGroup>) -> FlatDocument {
        FlatDocument { groups }
    }

    pub fn order(id: &str, status: &str) -> FlatRow {
        row(
            "id",
            id,
            vec![
                sourced("orders", "id", id, "id", number(id)),
                sourced("orders", "id", id, "status", text(status)),
            ],
        )
    }

    pub fn status_doc(status: &str) -> FlatDocument {
        doc(vec![group(
            "own",
            "orders",
            GroupShape::Fields,
            false,
            vec![order("1", status)],
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn rows_doc(rows: Vec<FlatRow>, capped: bool) -> FlatDocument {
        doc(vec![group(
            "orders",
            "orders",
            GroupShape::Rows,
            capped,
            rows,
        )])
    }

    fn meta_row(id: &str, meta_key: &str, value: &str) -> FlatRow {
        row(
            "meta_id",
            id,
            vec![
                sourced("ordermeta", "meta_id", id, "k", text(meta_key)),
                sourced("ordermeta", "meta_id", id, "v", text(value)),
            ],
        )
    }

    fn meta_doc(rows: Vec<FlatRow>) -> FlatDocument {
        doc(vec![group(
            "meta",
            "ordermeta",
            GroupShape::Meta,
            false,
            rows,
        )])
    }

    fn labels(marks: &[RowMark]) -> Vec<&str> {
        marks.iter().map(|m| m.label.as_str()).collect()
    }

    #[test]
    fn a_changed_value_reports_both_cells_and_its_column_as_label() {
        let delta = diff_documents(&status_doc("pending"), &status_doc("paid"));

        assert_eq!(
            delta,
            DocDelta {
                changed: vec![ValueChange {
                    group: "own".into(),
                    label: "status".into(),
                    source: prov("orders", "status", "id", "1"),
                    old_source: prov("orders", "status", "id", "1"),
                    old: text("pending"),
                    new: text("paid"),
                }],
                ..DocDelta::default()
            }
        );
    }

    #[test]
    fn identical_documents_have_an_empty_delta() {
        let delta = diff_documents(&status_doc("paid"), &status_doc("paid"));

        assert_eq!(delta, DocDelta::default());
        assert!(delta.is_empty());
    }

    #[test]
    fn a_changed_meta_value_is_labeled_with_its_meta_key() {
        let old = meta_doc(vec![
            meta_row("9", "_sku", "A"),
            meta_row("10", "_color", "red"),
        ]);
        let new = meta_doc(vec![
            meta_row("9", "_sku", "B"),
            meta_row("10", "_color", "red"),
        ]);

        let delta = diff_documents(&old, &new);

        assert_eq!(
            delta.changed,
            vec![ValueChange {
                group: "meta".into(),
                label: "_sku".into(),
                source: prov("ordermeta", "v", "meta_id", "9"),
                old_source: prov("ordermeta", "v", "meta_id", "9"),
                old: text("A"),
                new: text("B"),
            }]
        );
        assert!(delta.added.is_empty() && delta.removed.is_empty());
    }

    #[test]
    fn rows_only_in_the_new_document_are_added_and_only_in_the_old_are_removed() {
        let old = rows_doc(vec![order("1", "a"), order("2", "b")], false);
        let new = rows_doc(vec![order("2", "b"), order("3", "c")], false);

        let delta = diff_documents(&old, &new);

        assert!(delta.changed.is_empty());
        assert_eq!(
            delta.added,
            vec![RowMark {
                group: "orders".into(),
                table: "orders".into(),
                key: Some(key("id", "3")),
                label: "id 3".into(),
                value: None,
                fields: vec![
                    (
                        "id".into(),
                        Value {
                            raw: number("3"),
                            kind: ValueKind::Number,
                            source: Some(prov("orders", "id", "id", "3")),
                            style: None,
                            secondary: None,
                        }
                    ),
                    (
                        "status".into(),
                        Value {
                            raw: text("c"),
                            kind: ValueKind::Text,
                            source: Some(prov("orders", "status", "id", "3")),
                            style: None,
                            secondary: None,
                        }
                    ),
                ],
            }]
        );
        assert_eq!(labels(&delta.removed), ["id 1"]);
    }

    #[test]
    fn added_and_removed_meta_rows_are_labeled_with_the_key_and_carry_the_value() {
        let old = meta_doc(vec![meta_row("9", "_old", "x")]);
        let new = meta_doc(vec![meta_row("10", "_new", "y")]);

        let delta = diff_documents(&old, &new);

        assert_eq!(labels(&delta.added), ["_new"]);
        assert_eq!(delta.added[0].value, Some(text("y")));
        assert_eq!(labels(&delta.removed), ["_old"]);
        assert_eq!(delta.removed[0].value, Some(text("x")));
    }

    #[test]
    fn a_capped_old_group_proves_no_row_added() {
        let old = rows_doc(vec![order("1", "a"), order("2", "b")], true);
        let new = rows_doc(vec![order("2", "b"), order("3", "c")], false);

        let delta = diff_documents(&old, &new);

        assert!(delta.added.is_empty());
        assert_eq!(labels(&delta.removed), ["id 1"]);
    }

    #[test]
    fn a_capped_new_group_proves_no_row_removed() {
        let old = rows_doc(vec![order("1", "a"), order("2", "b")], false);
        let new = rows_doc(vec![order("2", "b"), order("3", "c")], true);

        let delta = diff_documents(&old, &new);

        assert!(delta.removed.is_empty());
        assert_eq!(labels(&delta.added), ["id 3"]);
    }

    #[test]
    fn a_group_missing_on_one_side_is_skipped() {
        let old = doc(vec![
            group(
                "own",
                "orders",
                GroupShape::Fields,
                false,
                vec![order("1", "a")],
            ),
            group(
                "gone",
                "orders",
                GroupShape::Rows,
                false,
                vec![order("7", "a")],
            ),
        ]);
        let new = doc(vec![
            group(
                "own",
                "orders",
                GroupShape::Fields,
                false,
                vec![order("1", "b")],
            ),
            group(
                "fresh",
                "orders",
                GroupShape::Rows,
                false,
                vec![order("8", "a")],
            ),
        ]);

        let delta = diff_documents(&old, &new);

        assert_eq!(delta.changed.len(), 1);
        assert_eq!(delta.changed[0].group, "own");
        assert!(delta.added.is_empty() && delta.removed.is_empty());
    }

    #[test]
    fn changes_come_in_key_order_with_integer_keys_compared_as_numbers() {
        for (keys, expected) in [
            (["10", "9", "2"], ["2", "9", "10"]),
            (["b", "c", "a"], ["a", "b", "c"]),
        ] {
            let old = rows_doc(keys.iter().map(|k| order(k, "x")).collect(), false);
            let new = rows_doc(keys.iter().map(|k| order(k, "y")).collect(), false);

            let ordered: Vec<String> = diff_documents(&old, &new)
                .changed
                .iter()
                .map(|c| c.source.row.0[0].value.clone())
                .collect();

            assert_eq!(ordered, expected, "{keys:?}");
        }
    }

    #[test]
    fn compare_aligns_meta_rows_by_key_and_names_both_rows_as_sources() {
        let left = meta_doc(vec![meta_row("1", "a", "1"), meta_row("2", "b", "2")]);
        let right = meta_doc(vec![meta_row("11", "b", "3"), meta_row("12", "c", "4")]);

        let delta = compare_documents(&left, &right);

        assert_eq!(
            delta.changed,
            vec![ValueChange {
                group: "meta".into(),
                label: "b".into(),
                source: prov("ordermeta", "v", "meta_id", "11"),
                old_source: prov("ordermeta", "v", "meta_id", "2"),
                old: text("2"),
                new: text("3"),
            }]
        );
        assert_eq!(labels(&delta.added), ["c"]);
        assert_eq!(delta.added[0].key, Some(key("meta_id", "12")));
        assert_eq!(labels(&delta.removed), ["a"]);
        assert_eq!(delta.removed[0].key, Some(key("meta_id", "1")));
    }

    #[test]
    fn compare_pairs_duplicate_meta_keys_by_occurrence() {
        let left = meta_doc(vec![meta_row("1", "x", "1"), meta_row("2", "x", "2")]);
        let right = meta_doc(vec![
            meta_row("11", "x", "1"),
            meta_row("12", "x", "5"),
            meta_row("13", "x", "6"),
        ]);

        let delta = compare_documents(&left, &right);

        assert_eq!(delta.changed.len(), 1);
        assert_eq!(
            delta.changed[0].old_source,
            prov("ordermeta", "v", "meta_id", "2")
        );
        assert_eq!(
            delta.changed[0].source,
            prov("ordermeta", "v", "meta_id", "12")
        );
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.added[0].key, Some(key("meta_id", "13")));
        assert!(delta.removed.is_empty());
    }

    #[test]
    fn compare_aligns_rows_groups_by_position() {
        let left = rows_doc(vec![order("1", "a"), order("2", "b")], false);
        let right = rows_doc(vec![order("11", "z")], false);

        let delta = compare_documents(&left, &right);

        assert_eq!(delta.changed.len(), 2);
        assert_eq!(delta.changed[0].label, "id");
        assert_eq!(delta.changed[0].old_source, prov("orders", "id", "id", "1"));
        assert_eq!(delta.changed[0].source, prov("orders", "id", "id", "11"));
        assert_eq!(delta.changed[1].label, "status");
        assert_eq!(delta.changed[1].old, text("a"));
        assert_eq!(delta.changed[1].new, text("z"));
        assert!(delta.added.is_empty());
        assert_eq!(labels(&delta.removed), ["id 2"]);
    }

    #[test]
    fn compare_marks_surplus_right_rows_as_added() {
        let left = rows_doc(vec![order("1", "a")], false);
        let right = rows_doc(vec![order("1", "a"), order("2", "b")], false);

        let delta = compare_documents(&left, &right);

        assert!(delta.changed.is_empty() && delta.removed.is_empty());
        assert_eq!(labels(&delta.added), ["id 2"]);
    }
}
