//! What the UI shows for one stored value: the raw cell, what kind of value
//! it is and where it was read.
//!
//! Results carry one `ColumnKind` per column plus the few cells that decode as serialized data,
//! and the page derives each cell's `Value` from them (`src/lib/values.ts`; `value_at` here is
//! the same rule in Rust). A page of 500 rows would otherwise send 500 copies of each kind.

mod rows;
mod serialized;

use serde::Serialize;
use ts_rs::TS;

pub use rows::{ResultValues, page_value_at, row_keys, value_at};
pub use serialized::{find_serialized, serialized};

use crate::address::Address;
use crate::db::Cell;
use crate::db::cell::{self, Family};
use crate::decode::value::Format;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Value {
    pub raw: Cell,
    pub kind: ValueKind,
    /// `None` for query results and for rows of a table without a primary key.
    pub source: Option<Provenance>,
    /// A scheme color name from a value-style rule.
    pub style: Option<String>,
    pub secondary: Option<Secondary>,
}

/// There is no status kind: status values are enums, and color comes only from value styles.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ValueKind {
    Text,
    Number,
    /// From a kind rule; the currency is itself a sourced value.
    Money {
        currency: Option<Box<Value>>,
    },
    Timestamp {
        #[ts(type = "number | null")]
        utc: Option<i64>,
    },
    /// The row's own key; `address` opens the row.
    PrimaryKey {
        address: Address,
    },
    Reference {
        targets: Vec<Address>,
        by: ReferenceSource,
        /// Whether the target row exists; `None` until someone checks.
        exists: Option<bool>,
    },
    /// `0` or NULL in a reference column: no target, as WordPress stores "none".
    NoReference,
    Enum,
    /// Text that the PHP or JSON decoder reads.
    Serialized {
        format: Format,
        preview: String,
    },
    /// Masked until clicked.
    Secret,
    Binary,
    Null,
    /// `''`, which is never shown like NULL.
    Empty,
}

/// Why a column counts as a reference.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReferenceSource {
    /// A declared constraint.
    ForeignKey {
        constraint: String,
    },
    Project {
        id: String,
    },
    Preset {
        preset: String,
        id: String,
    },
    /// Inferred: the column has the name of another table's primary key (docs/database.md).
    SameName {
        column: String,
        indexed: bool,
    },
}

/// Where a value was read: rendered as `wp_postmeta.meta_value · meta_id 9123`.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub table: String,
    pub column: String,
    pub row: RowKey,
}

/// Dim text next to the raw value, such as a relative time, with its own source.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Secondary {
    pub text: String,
    pub source: Option<Provenance>,
}

/// A row's primary key, in key order. Unlike `db::changes::RowKey` it owns its values and
/// covers composite keys.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RowKey(pub Vec<KeyPart>);

#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct KeyPart {
    pub column: String,
    pub value: String,
}

/// What a column holds as far as the lens says. The cell decides the rest: NULL, `''`, binary,
/// serialized text and "no reference" are per cell.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ColumnKind {
    Text,
    Number,
    Timestamp,
    /// Part of `table`'s primary key `columns`; the row's address comes from their values.
    PrimaryKey {
        table: Address,
        columns: Vec<String>,
    },
    Reference {
        targets: Vec<ReferenceTarget>,
        by: ReferenceSource,
    },
}

/// A table a reference column points at, and the column there that holds the value.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ReferenceTarget {
    /// The target table's rows, without a filter.
    pub table: Address,
    pub column: String,
    /// `column` is the table's single-column primary key, so a value addresses one row.
    pub key: bool,
}

/// A cell whose text decodes as PHP-serialized data or JSON.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SerializedCell {
    pub row: usize,
    pub col: usize,
    pub format: Format,
    /// The value's head, e.g. `a:3:{…}`.
    pub preview: String,
}

/// The kind the SQL type alone gives: text columns (meta values) are text until a kind rule says
/// otherwise.
pub fn type_kind(type_name: &str) -> ColumnKind {
    match cell::family(type_name) {
        Family::Number => ColumnKind::Number,
        Family::Temporal => ColumnKind::Timestamp,
        Family::Json | Family::Bit | Family::Binary | Family::Text => ColumnKind::Text,
    }
}
