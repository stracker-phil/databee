use super::{DocDelta, FlatField, FlatGroup, FlatRow, RowMark, ValueChange};
use crate::db::Cell;
use crate::document::GroupShape;
use crate::value::{Provenance, RowKey, Value, ValueKind};

pub(super) fn changed_fields(
    group: &FlatGroup,
    old: &FlatRow,
    new: &FlatRow,
    delta: &mut DocDelta,
) {
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

pub(super) fn meta_key(row: &FlatRow) -> Option<String> {
    row.fields.first()?.cell.as_text().map(str::to_string)
}

pub(super) fn mark(group: &FlatGroup, row: &FlatRow) -> RowMark {
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
