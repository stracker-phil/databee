use serde::{Deserialize, Serialize};

use super::Cell;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
}

#[derive(Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    /// Rows the server sent; larger than `rows.len()` when the row limit cut the result.
    pub total_rows: u64,
    pub truncated: bool,
    pub affected_rows: u64,
    pub last_insert_id: Option<u64>,
    pub returns_rows: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResponse {
    pub results: Vec<ResultSet>,
    pub elapsed_ms: u64,
    /// Current database after execution (`USE` may have changed it).
    pub database: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub is_system: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub is_view: bool,
    pub rows_estimate: Option<u64>,
    pub size_bytes: Option<u64>,
    pub comment: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub key: String,
    pub extra: String,
    pub comment: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableStructure {
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    /// `None` when `SHOW CREATE TABLE` returned no row.
    pub ddl: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
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
    pub descending: bool,
    /// Raw SQL condition typed by the user, e.g. `post_type = 'page'`, or ID shorthand
    /// (`3229`, `3220-3229,3200`) when `primary_key` names a single-column key.
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub primary_key: Option<String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FilterKind {
    /// Primary-key shorthand such as `3220-3229`.
    Ids,
    /// A raw SQL condition, as typed.
    Sql,
}

/// The condition that actually ran, so the UI can show its interpretation.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFilter {
    pub kind: FilterKind,
    pub sql: String,
    /// Human label, e.g. `ID in (3229, 3227)`.
    pub label: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    pub result: ResultSet,
    pub has_more: bool,
    pub filter: Option<AppliedFilter>,
}
