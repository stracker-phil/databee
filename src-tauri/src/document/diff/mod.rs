//! Live diff, snapshots and compare. A document is reduced to its stored cells
//! (`FlatDocument`), keyed by provenance: a group, a row key and a column. Two of them give a
//! `DocDelta`, the marks the page draws on the newer document.

mod compare;
#[cfg(test)]
pub(crate) mod fixtures;
mod marks;

pub use compare::compare_documents;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Document, GroupLink, GroupShape};
use crate::db::Cell;
use crate::value::{Provenance, RowKey, Value};
use marks::{changed_fields, mark};

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
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::value::ValueKind;

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
}
