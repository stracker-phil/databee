//! Column kinds, per-cell kinds and styles of a result whose columns all come from one table.

use super::Lens;
use super::cells::{IndexedColumns, add_cells, table_columns};
use crate::db::{ColumnMeta, ResultSet};
use crate::value::{self, ColumnKind};

/// Column kinds, per-cell kinds and styles of a result whose columns all come from `table`
/// under their own names (`SELECT * FROM table`).
pub fn annotate_kinds(lens: &dyn Lens, table: &str, result: &mut ResultSet) {
    result.kinds = lens.kinds(table, &result.columns);
    let names: IndexedColumns = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| (i, c.name.clone()))
        .collect();
    add_cells(result, lens, &table_columns(table, &names));
}

/// `annotate_kinds` plus the cells that decode as serialized data.
pub fn annotate_table(lens: &dyn Lens, table: &str, result: &mut ResultSet) {
    value::find_serialized(result);
    annotate_kinds(lens, table, result);
}

pub fn type_kinds(columns: &[ColumnMeta]) -> Vec<ColumnKind> {
    columns
        .iter()
        .map(|c| value::type_kind(&c.type_name))
        .collect()
}
