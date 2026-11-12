//! Rows, columns and the key the change planner tests share.

use super::ChangeKey;
use crate::db::{Cell, ColumnInfo, ColumnMeta, ResultSet};

pub(super) const ROW: ChangeKey<'static> = ChangeKey {
    database: "wp",
    table: "t",
    key_column: "id",
    key: "7",
};

pub(super) fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
    ResultSet {
        columns: columns
            .iter()
            .map(|name| ColumnMeta {
                name: name.to_string(),
                type_name: "TEXT".into(),
                origin: None,
            })
            .collect(),
        rows,
        ..Default::default()
    }
}

pub(super) fn column(name: &str) -> ColumnInfo {
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

pub(super) fn auto_increment_column(name: &str) -> ColumnInfo {
    ColumnInfo {
        auto_increment: true,
        ..column(name)
    }
}

pub(super) fn generated_column(name: &str) -> ColumnInfo {
    ColumnInfo {
        generated: true,
        ..column(name)
    }
}

pub(super) fn text(s: &str) -> Cell {
    Cell::Text(s.into())
}
