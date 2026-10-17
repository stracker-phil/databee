//! What the UI shows for one stored value: the raw cell, what kind of value
//! it is and where it was read.
//!
//! Results carry one `ColumnKind` per column plus the few cells that decode as serialized data,
//! and the page derives each cell's `Value` from them (`src/lib/values.ts`; `value_at` here is
//! the same rule in Rust). A page of 500 rows would otherwise send 500 copies of each kind.

mod condition;
mod rows;
mod serialized;
mod time;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use condition::{stored_key, value_condition};
pub use rows::{ResultValues, page_value_at, row_keys, value_at};
pub use serialized::{find_serialized, serialized};
pub use time::utc_seconds;

use crate::address::Address;
use crate::db::Cell;
use crate::db::cell::{self, Family};
use crate::decode::value::Format;
use crate::rules::kinds::Zone;

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
    /// `utc` is the moment in Unix seconds when the zone is known and the value is a time (zero
    /// dates are not); `zone` is the kind rule's, `None` for a temporal column without one.
    Timestamp {
        #[ts(type = "number | null")]
        utc: Option<i64>,
        zone: Option<Zone>,
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
    /// `0` or NULL in a reference column: no target.
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RowKey(pub Vec<KeyPart>);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
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
    /// A temporal SQL type (zone `None` unless a kind rule names it) or a kind rule.
    Timestamp {
        zone: Option<Zone>,
    },
    /// From a kind rule; `currency` names the row's columns to take the currency from, first
    /// non-empty wins.
    Money {
        currency: Vec<String>,
    },
    Enum,
    Secret,
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

/// A cell whose kind its row decides (a conditional reference, a meta key's kind rule), sent only
/// where it differs from the column's kind.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct CellKind {
    pub row: usize,
    pub col: usize,
    pub kind: ColumnKind,
}

/// A cell a value style colors (`Value.style`).
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct CellStyle {
    pub row: usize,
    pub col: usize,
    /// A scheme style color name.
    pub style: String,
}

/// What a lens decides per cell, for one result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellAnnotations {
    pub kinds: Vec<CellKind>,
    pub styles: Vec<CellStyle>,
}

/// A table a reference column points at, and the column there that holds the value.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ReferenceTarget {
    /// The target table's rows, filtered by the `=` conditions of the reference's `to_when`
    /// (a non-key target such as a meta value under one key); unfiltered otherwise.
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
        Family::Temporal => ColumnKind::Timestamp { zone: None },
        Family::Json | Family::Bit | Family::Binary | Family::Text => ColumnKind::Text,
    }
}
