use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Cell;
use crate::error::AppError;
use crate::value::{ColumnKind, RowKey, SerializedCell};

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
    /// The table column the value was read from, as the server reports it; `None` for
    /// expressions and for results built in code.
    pub origin: Option<ColumnOrigin>,
}

/// The real table and column behind a result column, also under an alias (`org_table`,
/// `org_name` of the protocol).
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnOrigin {
    /// `None` when the server sent no schema (derived tables on some servers).
    pub database: Option<String>,
    pub table: String,
    pub column: String,
}

#[derive(Serialize, Debug, Clone, Default, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    /// Names the result for `fetch_cell`; unique per session once the session keeps it, 0 before.
    pub id: u64,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    /// Rows the server sent; larger than `rows.len()` when the row limit cut the result.
    pub total_rows: u64,
    pub truncated: bool,
    pub affected_rows: u64,
    pub last_insert_id: Option<u64>,
    pub returns_rows: bool,
    /// What each column holds, in column order; filled by `Session::execute` (SQL types and
    /// references by column origin) and `Session::browse` (the lens), empty elsewhere.
    pub kinds: Vec<ColumnKind>,
    /// The cells whose text decodes as PHP-serialized or JSON, by row then column.
    pub serialized: Vec<SerializedCell>,
    /// Full values of the `LongText` cells, which the session moves into its cell cache.
    #[serde(skip)]
    #[ts(skip)]
    pub full_texts: Vec<FullText>,
}

/// The full value of a `LongText` cell at `row`, `col` of its result set.
#[derive(Debug, Clone, PartialEq)]
pub struct FullText {
    pub row: usize,
    pub col: usize,
    pub text: String,
}

/// The statement of a script that failed, after the earlier ones succeeded.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct StatementFailure {
    /// 0-based position of the statement in the script.
    pub index: usize,
    #[ts(type = "AppError")]
    pub error: AppError,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResponse {
    pub results: Vec<ResultSet>,
    pub elapsed_ms: u64,
    /// Current database after execution (`USE` may have changed it).
    pub database: Option<String>,
    /// Set when a statement after the first failed; `results` holds the ones before it.
    pub failure: Option<StatementFailure>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub is_system: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub is_view: bool,
    pub rows_estimate: Option<u64>,
    pub size_bytes: Option<u64>,
    pub comment: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub key: String,
    pub extra: String,
    pub comment: String,
    pub auto_increment: bool,
    /// Computed by the server (virtual or stored); inserting a value fails.
    pub generated: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    /// Columns of the `PRIMARY` index in key order; empty without one.
    pub primary_key: Vec<String>,
    /// Declared foreign keys to tables of the same database.
    pub foreign_keys: Vec<ForeignKey>,
    /// `None` when `SHOW CREATE TABLE` returned no row.
    pub ddl: Option<String>,
}

/// A declared foreign key; `columns[i]` references `ref_columns[i]` of `ref_table`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    /// The constraint name.
    pub name: String,
    pub columns: Vec<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

/// The keys of one table, as the bulk loader reads them for a whole database.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableKeys {
    pub primary_key: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[ts(optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub struct BrowseRequest {
    pub database: String,
    pub table: String,
    #[serde(default)]
    pub offset: u64,
    pub limit: u32,
    #[serde(default)]
    pub order_by: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub descending: bool,
    /// Raw SQL condition typed by the user, e.g. `post_type = 'page'`, or ID shorthand
    /// (`3229`, `3220-3229,3200`) when `primary_key` names a single-column key.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub primary_key: Option<String>,
    /// Exact column matches (`openRow`), built into SQL by Rust; combined with `filter` by `AND`.
    #[serde(default)]
    #[ts(as = "Option<Vec<ColumnEquals>>", optional)]
    pub r#where: Vec<ColumnEquals>,
}

/// `column = value`, or `column IS NULL` for a `None` value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ColumnEquals {
    pub column: String,
    pub value: Option<String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub enum FilterKind {
    /// Primary-key shorthand such as `3220-3229`.
    Ids,
    /// A raw SQL condition, as typed.
    Sql,
    /// The structured `where` of the request.
    Columns,
}

/// The condition that actually ran, so the UI can show its interpretation.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct AppliedFilter {
    pub kind: FilterKind,
    pub sql: String,
    /// Human label, e.g. `ID in (3229, 3227)`.
    pub label: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    /// The browsed table, which `keys` and `result.kinds` describe.
    pub table: String,
    pub result: ResultSet,
    /// The primary key of each row, in row order (set by `Session::browse`); empty when the
    /// table has no primary key.
    pub keys: Vec<RowKey>,
    pub has_more: bool,
    /// Page size after the clamp; the next page starts at `offset + limit`.
    pub limit: u32,
    pub filter: Option<AppliedFilter>,
    /// The structured `where` as it ran, for the filter bar.
    pub columns_filter: Option<AppliedFilter>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            column_type: "bigint".into(),
            nullable: false,
            default: None,
            key: "PRI".into(),
            extra: "auto_increment".into(),
            comment: String::new(),
            auto_increment: true,
            generated: false,
        }
    }

    #[test]
    fn column_info_serializes_auto_increment_and_generated_flags() {
        assert_eq!(
            serde_json::to_value(column("id")).unwrap(),
            json!({
                "name": "id",
                "columnType": "bigint",
                "nullable": false,
                "default": null,
                "key": "PRI",
                "extra": "auto_increment",
                "comment": "",
                "autoIncrement": true,
                "generated": false
            })
        );
    }

    #[test]
    fn table_structure_serializes_its_primary_key_columns() {
        let structure = TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: vec!["post_id".into(), "meta_key".into()],
            foreign_keys: Vec::new(),
            ddl: None,
        };

        assert_eq!(
            serde_json::to_value(structure).unwrap(),
            json!({
                "columns": [],
                "indexes": [],
                "primaryKey": ["post_id", "meta_key"],
                "foreignKeys": [],
                "ddl": null
            })
        );
    }
}
