//! The lens seam: what a column holds and which references a table has. `SchemaLens` answers
//! from the schema cache (primary keys, declared foreign keys, same-name inference);
//! `rules::lens::RulesLens` wraps it with the effective rules of presets and the project, and
//! `rules::lens::build` picks between them.

mod annotate;
mod cells;
mod schema_lens;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use annotate::{annotate_kinds, annotate_table, type_kinds};
pub use cells::{IndexedColumns, add_cells, table_columns};
pub use schema_lens::SchemaLens;

use crate::db::{ColumnMeta, ResultSet};
use crate::value::{CellAnnotations, ColumnKind, ReferenceSource};

pub trait Lens: Send + Sync {
    /// The kind of each of `columns` (columns of `table`), in order.
    fn kinds(&self, table: &str, columns: &[ColumnMeta]) -> Vec<ColumnKind>;
    /// References whose source columns are in `table`.
    fn references_from(&self, table: &str) -> Vec<ResolvedReference>;
    /// References that point at `table`.
    fn references_to(&self, table: &str) -> Vec<ResolvedReference>;
    /// Kinds and styles that each row decides (conditional references, meta-key kinds, value
    /// styles) for the result columns of one table, after `kinds` filled `result.kinds`.
    fn cells(&self, _columns: &TableColumns<'_>, _result: &ResultSet) -> CellAnnotations {
        CellAnnotations::default()
    }
}

/// The result columns that come from one table: `(position in the result, name in the table)`.
pub struct TableColumns<'a> {
    pub table: &'a str,
    pub columns: Vec<(usize, &'a str)>,
}

/// Columns of one table; a reference's two ends pair up by position.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ColumnsRef {
    pub table: String,
    pub columns: Vec<String>,
}

impl ColumnsRef {
    pub fn new(table: &str, columns: Vec<String>) -> Self {
        Self {
            table: table.to_string(),
            columns,
        }
    }
}

/// A reference from `from` to `to` and why it exists. An ambiguous inferred column gives one
/// reference per target table.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ResolvedReference {
    pub from: ColumnsRef,
    pub to: ColumnsRef,
    pub by: ReferenceSource,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct TableReferences {
    pub outgoing: Vec<ResolvedReference>,
    pub incoming: Vec<ResolvedReference>,
}
