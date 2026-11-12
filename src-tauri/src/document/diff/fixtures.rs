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

pub fn rows_doc(rows: Vec<FlatRow>, capped: bool) -> FlatDocument {
    doc(vec![group(
        "orders",
        "orders",
        GroupShape::Rows,
        capped,
        rows,
    )])
}

pub fn meta_row(id: &str, meta_key: &str, value: &str) -> FlatRow {
    row(
        "meta_id",
        id,
        vec![
            sourced("ordermeta", "meta_id", id, "k", text(meta_key)),
            sourced("ordermeta", "meta_id", id, "v", text(value)),
        ],
    )
}

pub fn meta_doc(rows: Vec<FlatRow>) -> FlatDocument {
    doc(vec![group(
        "meta",
        "ordermeta",
        GroupShape::Meta,
        false,
        rows,
    )])
}

pub fn labels(marks: &[RowMark]) -> Vec<&str> {
    marks.iter().map(|m| m.label.as_str()).collect()
}
