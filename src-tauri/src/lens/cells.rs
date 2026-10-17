//! Per-cell kinds and styles: result columns of one table, and merging what the lens decides per
//! row into the result.

use super::{Lens, TableColumns};
use crate::db::ResultSet;

/// Result columns as `(position in the result, name in their table)`.
pub type IndexedColumns = Vec<(usize, String)>;

/// `columns` of `table` in the shape `Lens::cells` reads.
pub fn table_columns<'a>(table: &'a str, columns: &'a IndexedColumns) -> TableColumns<'a> {
    TableColumns {
        table,
        columns: columns.iter().map(|(i, n)| (*i, n.as_str())).collect(),
    }
}

/// Adds the lens's per-cell kinds and styles, kept sorted by row, then column.
pub fn add_cells(result: &mut ResultSet, lens: &dyn Lens, columns: &TableColumns<'_>) {
    let cells = lens.cells(columns, result);
    result.cell_kinds.extend(cells.kinds);
    result.styles.extend(cells.styles);
    result.cell_kinds.sort_by_key(|c| (c.row, c.col));
    result.styles.sort_by_key(|c| (c.row, c.col));
}
