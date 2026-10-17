//! Per-cell kinds and styles: which result columns come from which table, and merging what the
//! lens decides per row into the result.

use std::collections::{BTreeMap, HashMap};

use super::{Lens, TableColumns};
use crate::db::{ColumnMeta, ResultSet};
use crate::rules::effective::RuleSources;
use crate::session::Session;

/// Result columns as `(position in the result, name in their table)`.
pub type IndexedColumns = Vec<(usize, String)>;

/// Result columns grouped by `(database, table)` of their origin.
pub type ColumnsByOrigin = BTreeMap<(String, String), IndexedColumns>;

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

/// Kinds of the columns whose origin names a table, from that database's lens (loaded once per
/// database), then the per-cell kinds and styles of each origin table.
pub async fn annotate_origins(
    session: &Session,
    current: Option<&str>,
    result: &mut ResultSet,
    lenses: &mut HashMap<String, Option<Box<dyn Lens>>>,
    rules: Option<RuleSources<'_>>,
) {
    let mut by_origin = ColumnsByOrigin::new();
    for (col, column) in result.columns.iter().enumerate() {
        let Some(origin) = &column.origin else {
            continue;
        };
        let Some(database) = origin.database.as_deref().or(current) else {
            continue;
        };
        if !lenses.contains_key(database) {
            let lens = load(session, database, rules).await;
            lenses.insert(database.to_string(), lens);
        }
        let Some(Some(lens)) = lenses.get(database) else {
            continue;
        };
        // The lens knows the column by its real name, not the alias the result shows.
        let real = ColumnMeta {
            name: origin.column.clone(),
            ..column.clone()
        };
        if let Some(kind) = lens.kinds(&origin.table, &[real]).into_iter().next() {
            result.kinds[col] = kind;
        }
        by_origin
            .entry((database.to_string(), origin.table.clone()))
            .or_default()
            .push((col, origin.column.clone()));
    }
    for ((database, table), columns) in &by_origin {
        let Some(Some(lens)) = lenses.get(database) else {
            continue;
        };
        add_cells(result, lens.as_ref(), &table_columns(table, columns));
    }
}

async fn load(
    session: &Session,
    database: &str,
    rules: Option<RuleSources<'_>>,
) -> Option<Box<dyn Lens>> {
    match super::for_database(session, database, rules).await {
        Ok(lens) => Some(lens),
        Err(e) => {
            log::debug!("Schema of {database} not loaded for result kinds: {e}");
            None
        }
    }
}
