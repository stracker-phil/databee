pub mod cell;
pub mod id_filter;
pub mod lexer;
pub mod mysql;
pub mod safety;

use serde::{Deserialize, Serialize};

pub use cell::Cell;

use crate::error::{AppError, AppResult};

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
    pub ddl: String,
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

/// The condition that actually ran, so the UI can show its interpretation.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFilter {
    /// `ids` for primary-key shorthand, `sql` for a raw condition.
    pub kind: String,
    pub sql: String,
    /// Human label, e.g. `ID in (3229, 3227)`.
    pub label: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    pub result: ResultSet,
    pub has_more: bool,
    #[serde(default)]
    pub filter: Option<AppliedFilter>,
}

pub const SYSTEM_SCHEMAS: &[&str] = &["information_schema", "mysql", "performance_schema", "sys"];

/// Quotes a MySQL identifier with backticks.
pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// Resolves the typed filter: ID shorthand against the primary key, or raw SQL.
pub fn applied_filter(req: &BrowseRequest) -> AppResult<Option<AppliedFilter>> {
    let Some(filter) = req
        .filter
        .as_deref()
        .map(str::trim)
        .filter(|f| !f.is_empty())
    else {
        return Ok(None);
    };
    if let (Some(pk), Some(ids)) = (req.primary_key.as_deref(), id_filter::parse_ids(filter)) {
        return Ok(Some(AppliedFilter {
            kind: "ids".into(),
            sql: ids.to_sql(pk),
            label: ids.label(pk),
        }));
    }
    if lexer::split_statements(filter).len() != 1 {
        return Err(AppError::InvalidInput(
            "The filter must be a single condition".into(),
        ));
    }
    Ok(Some(AppliedFilter {
        kind: "sql".into(),
        sql: filter.to_string(),
        label: filter.to_string(),
    }))
}

/// Builds the paged SELECT for the table view. Fetches one extra row to detect more pages.
pub fn browse_sql(req: &BrowseRequest) -> AppResult<String> {
    let mut sql = format!(
        "SELECT * FROM {}.{}",
        quote_ident(&req.database),
        quote_ident(&req.table)
    );
    if let Some(filter) = applied_filter(req)? {
        sql.push_str(&format!(" WHERE {}", filter.sql));
    }
    if let Some(col) = &req.order_by {
        let dir = if req.descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(" ORDER BY {} {dir}", quote_ident(col)));
    }
    sql.push_str(&format!(
        " LIMIT {} OFFSET {}",
        u64::from(req.limit) + 1,
        req.offset
    ));
    Ok(sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> BrowseRequest {
        BrowseRequest {
            database: "wordpress".into(),
            table: "wp_posts".into(),
            limit: 100,
            ..Default::default()
        }
    }

    #[test]
    fn quotes_identifiers_and_escapes_backticks() {
        assert_eq!(quote_ident("wp_posts"), "`wp_posts`");
        assert_eq!(quote_ident("we`ird"), "`we``ird`");
    }

    #[test]
    fn browse_fetches_one_extra_row_for_paging() {
        assert_eq!(
            browse_sql(&BrowseRequest {
                offset: 200,
                ..req()
            })
            .unwrap(),
            "SELECT * FROM `wordpress`.`wp_posts` LIMIT 101 OFFSET 200"
        );
    }

    #[test]
    fn browse_applies_filter_and_order() {
        let sql = browse_sql(&BrowseRequest {
            order_by: Some("post_date".into()),
            descending: true,
            filter: Some(" post_type = 'page' ".into()),
            ..req()
        })
        .unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM `wordpress`.`wp_posts` WHERE post_type = 'page' ORDER BY `post_date` DESC LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn browse_rejects_filters_with_extra_statements() {
        let err = browse_sql(&BrowseRequest {
            filter: Some("1; DROP TABLE wp_users".into()),
            ..req()
        })
        .unwrap_err();
        assert_eq!(err.kind(), "invalid_input");
    }

    #[test]
    fn id_shorthand_uses_the_primary_key_only_when_there_is_one() {
        let with_pk = BrowseRequest {
            filter: Some("3220-3225, 3229".into()),
            primary_key: Some("ID".into()),
            ..req()
        };
        assert_eq!(
            browse_sql(&with_pk).unwrap(),
            "SELECT * FROM `wordpress`.`wp_posts` WHERE (`ID` BETWEEN 3220 AND 3225 OR `ID` = 3229) LIMIT 101 OFFSET 0"
        );
        let applied = applied_filter(&with_pk).unwrap().unwrap();
        assert_eq!(applied.kind, "ids");
        assert_eq!(applied.label, "ID in (3220–3225, 3229)");

        // Composite or unknown key: the text is SQL as typed.
        let without = BrowseRequest {
            filter: Some("3229".into()),
            ..req()
        };
        assert_eq!(
            browse_sql(&without).unwrap(),
            "SELECT * FROM `wordpress`.`wp_posts` WHERE 3229 LIMIT 101 OFFSET 0"
        );
        assert_eq!(applied_filter(&without).unwrap().unwrap().kind, "sql");

        // Real SQL is never mistaken for IDs.
        let sql = BrowseRequest {
            filter: Some("ID > 3229".into()),
            primary_key: Some("ID".into()),
            ..req()
        };
        assert_eq!(applied_filter(&sql).unwrap().unwrap().kind, "sql");
    }

    #[test]
    fn blank_filter_is_ignored() {
        let sql = browse_sql(&BrowseRequest {
            filter: Some("  ".into()),
            ..req()
        })
        .unwrap();
        assert!(!sql.contains("WHERE"));
    }
}
