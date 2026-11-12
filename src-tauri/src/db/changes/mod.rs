//! Row edits as reviewable SQL: each request is planned into statements plus the statements
//! that undo it (built from the current values). Every planner fetches what it needs, then a
//! pure builder turns that state into a `Plan`.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::browse::equals_sql;
use super::{ColumnEquals, Database, Dialect};
use crate::error::AppResult;
use build::{
    build_add_row, build_delete_row, build_duplicate_row, build_insert_row, build_set_cell,
};

mod build;
mod restore;
#[cfg(test)]
mod testing;

pub use restore::{generated_columns, reinsert_note, reinsert_sql, restore_literal};

#[derive(Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChangeRequest {
    /// Any table cell, addressed by a unique key column.
    #[serde(rename_all = "camelCase")]
    SetCell {
        database: String,
        table: String,
        key_column: String,
        key: String,
        column: String,
        value: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    DeleteRow {
        database: String,
        table: String,
        key_column: String,
        key: String,
    },
    #[serde(rename_all = "camelCase")]
    DuplicateRow {
        database: String,
        table: String,
        key_column: String,
        key: String,
    },
    #[serde(rename_all = "camelCase")]
    InsertRow {
        database: String,
        table: String,
        values: Vec<(String, Option<String>)>,
    },
    /// A row whose single-column key the server assigns (a meta row); undo deletes the newest
    /// row that holds these values.
    #[serde(rename_all = "camelCase")]
    AddRow {
        database: String,
        table: String,
        key_column: String,
        values: Vec<(String, Option<String>)>,
    },
}

/// A row a pending change touches, so the page can mark it before the change is applied.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct PlannedRow {
    pub database: String,
    pub table: String,
    pub key_column: String,
    pub key: String,
    pub effect: RowEffect,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RowEffect {
    Delete,
    Duplicate,
    Set { column: String },
}

impl ChangeRequest {
    /// The existing rows the change touches; inserted rows have no key yet.
    pub fn touched_rows(&self) -> Vec<PlannedRow> {
        let row = |database: &str, table: &str, key_column: &str, key: &str, effect| PlannedRow {
            database: database.to_string(),
            table: table.to_string(),
            key_column: key_column.to_string(),
            key: key.to_string(),
            effect,
        };
        match self {
            Self::SetCell {
                database,
                table,
                key_column,
                key,
                column,
                ..
            } => vec![row(
                database,
                table,
                key_column,
                key,
                RowEffect::Set {
                    column: column.clone(),
                },
            )],
            Self::DeleteRow {
                database,
                table,
                key_column,
                key,
            } => vec![row(database, table, key_column, key, RowEffect::Delete)],
            Self::DuplicateRow {
                database,
                table,
                key_column,
                key,
            } => vec![row(database, table, key_column, key, RowEffect::Duplicate)],
            Self::InsertRow { .. } | Self::AddRow { .. } => Vec::new(),
        }
    }
}

/// A planned write, kept in the session until it is applied or discarded.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub description: String,
    pub statements: Vec<String>,
    /// Restores the previous state; empty when a change can't be undone automatically.
    pub undo: Vec<String>,
}

/// A row addressed by a unique key column.
#[derive(Debug, Clone, Copy)]
struct ChangeKey<'a> {
    database: &'a str,
    table: &'a str,
    key_column: &'a str,
    key: &'a str,
}

impl ChangeKey<'_> {
    fn table_ref(&self, dialect: Dialect) -> String {
        dialect.table_ref(self.database, self.table)
    }

    fn filter(&self, dialect: Dialect) -> String {
        key_filter(dialect, self.key_column, self.key)
    }

    fn select(&self, dialect: Dialect, what: &str) -> String {
        format!(
            "SELECT {what} FROM {} WHERE {}",
            self.table_ref(dialect),
            self.filter(dialect)
        )
    }
}

fn key_filter(dialect: Dialect, key_column: &str, key: &str) -> String {
    let column = ColumnEquals {
        column: key_column.to_string(),
        value: Some(key.to_string()),
    };
    equals_sql(dialect, &[column])
}

/// Plans a change; `timeout` bounds the reads that capture the current state.
pub async fn plan(db: &dyn Database, change: &ChangeRequest, timeout: Duration) -> AppResult<Plan> {
    let dialect = db.dialect();
    match change {
        ChangeRequest::SetCell {
            database,
            table,
            key_column,
            key,
            column,
            value,
        } => {
            let row = ChangeKey {
                database,
                table,
                key_column,
                key,
            };
            plan_set_cell(db, row, column, value.as_deref(), timeout).await
        }
        ChangeRequest::DeleteRow {
            database,
            table,
            key_column,
            key,
        } => {
            let row = ChangeKey {
                database,
                table,
                key_column,
                key,
            };
            plan_delete_row(db, row, timeout).await
        }
        ChangeRequest::DuplicateRow {
            database,
            table,
            key_column,
            key,
        } => {
            let row = ChangeKey {
                database,
                table,
                key_column,
                key,
            };
            plan_duplicate_row(db, row, timeout).await
        }
        ChangeRequest::InsertRow {
            database,
            table,
            values,
        } => Ok(build_insert_row(dialect, database, table, values)),
        ChangeRequest::AddRow {
            database,
            table,
            key_column,
            values,
        } => Ok(build_add_row(dialect, database, table, key_column, values)),
    }
}

async fn plan_set_cell(
    db: &dyn Database,
    row: ChangeKey<'_>,
    column: &str,
    value: Option<&str>,
    timeout: Duration,
) -> AppResult<Plan> {
    let dialect = db.dialect();
    let old = db
        .fetch(&row.select(dialect, &dialect.quote_ident(column)), timeout)
        .await?;
    build_set_cell(dialect, row, column, value, &old)
}

async fn plan_delete_row(
    db: &dyn Database,
    row: ChangeKey<'_>,
    timeout: Duration,
) -> AppResult<Plan> {
    let dialect = db.dialect();
    let rows = db.fetch(&row.select(dialect, "*"), timeout).await?;
    let structure = db.structure(row.database, row.table).await?;
    build_delete_row(dialect, row, &rows, &generated_columns(&structure.columns))
}

async fn plan_duplicate_row(
    db: &dyn Database,
    row: ChangeKey<'_>,
    timeout: Duration,
) -> AppResult<Plan> {
    let dialect = db.dialect();
    let count = db.fetch(&row.select(dialect, "COUNT(*)"), timeout).await?;
    let matches = count
        .rows
        .first()
        .and_then(|r| r[0].as_text())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let structure = db.structure(row.database, row.table).await?;
    build_duplicate_row(dialect, row, matches, &structure.columns)
}

#[cfg(test)]
mod tests {
    use super::testing::{result, text};
    use super::*;
    use crate::db::{Cell, FakeDatabase};
    use crate::error::ErrorKind;

    const TIMEOUT: Duration = Duration::from_secs(1);

    #[test]
    fn touched_rows_name_the_existing_row_each_request_changes() {
        let planned = |effect| {
            vec![PlannedRow {
                database: "wp".into(),
                table: "t".into(),
                key_column: "id".into(),
                key: "7".into(),
                effect,
            }]
        };
        let (database, table, key_column, key) = (
            "wp".to_string(),
            "t".to_string(),
            "id".to_string(),
            "7".to_string(),
        );
        let values = vec![("a".to_string(), Some("1".to_string()))];

        let cases = [
            (
                "set cell",
                ChangeRequest::SetCell {
                    database: database.clone(),
                    table: table.clone(),
                    key_column: key_column.clone(),
                    key: key.clone(),
                    column: "name".into(),
                    value: None,
                },
                planned(RowEffect::Set {
                    column: "name".into(),
                }),
            ),
            (
                "delete row",
                ChangeRequest::DeleteRow {
                    database: database.clone(),
                    table: table.clone(),
                    key_column: key_column.clone(),
                    key: key.clone(),
                },
                planned(RowEffect::Delete),
            ),
            (
                "duplicate row",
                ChangeRequest::DuplicateRow {
                    database: database.clone(),
                    table: table.clone(),
                    key_column: key_column.clone(),
                    key: key.clone(),
                },
                planned(RowEffect::Duplicate),
            ),
            (
                "insert row",
                ChangeRequest::InsertRow {
                    database: database.clone(),
                    table: table.clone(),
                    values: values.clone(),
                },
                Vec::new(),
            ),
            (
                "add row",
                ChangeRequest::AddRow {
                    database,
                    table,
                    key_column,
                    values,
                },
                Vec::new(),
            ),
        ];

        for (name, request, expected) in cases {
            assert_eq!(request.touched_rows(), expected, "{name}");
        }
    }

    fn set_cell_request() -> ChangeRequest {
        ChangeRequest::SetCell {
            database: "wp".into(),
            table: "t".into(),
            key_column: "id".into(),
            key: "7".into(),
            column: "name".into(),
            value: Some("x".into()),
        }
    }

    #[tokio::test]
    async fn planning_an_add_row_needs_no_read() {
        let db = FakeDatabase::new();
        let request = ChangeRequest::AddRow {
            database: "wp".into(),
            table: "t".into(),
            key_column: "id".into(),
            values: vec![("name".into(), Some("a".into()))],
        };

        let plan = plan(&db, &request, TIMEOUT).await.unwrap();

        assert_eq!(
            plan.undo,
            ["DELETE FROM `wp`.`t` WHERE `name` <=> 'a' ORDER BY `id` DESC LIMIT 1"]
        );
        assert!(db.queries().is_empty());
    }

    #[test]
    fn add_row_request_deserializes_from_the_page_shape() {
        let add = serde_json::json!({
            "type": "addRow",
            "database": "wp",
            "table": "t",
            "keyColumn": "id",
            "values": [["name", "a"], ["slug", null]],
        });

        assert_eq!(
            serde_json::from_value::<ChangeRequest>(add).unwrap(),
            ChangeRequest::AddRow {
                database: "wp".into(),
                table: "t".into(),
                key_column: "id".into(),
                values: vec![("name".into(), Some("a".into())), ("slug".into(), None)],
            }
        );
    }

    #[test]
    fn requests_deserialize_from_the_page_shape() {
        let set_cell = serde_json::json!({
            "type": "setCell",
            "database": "wp",
            "table": "t",
            "keyColumn": "id",
            "key": "7",
            "column": "name",
            "value": null,
        });
        let insert = serde_json::json!({
            "type": "insertRow",
            "database": "wp",
            "table": "t",
            "values": [["name", "a"], ["slug", null]],
        });

        assert_eq!(
            serde_json::from_value::<ChangeRequest>(set_cell).unwrap(),
            ChangeRequest::SetCell {
                database: "wp".into(),
                table: "t".into(),
                key_column: "id".into(),
                key: "7".into(),
                column: "name".into(),
                value: None,
            }
        );
        assert_eq!(
            serde_json::from_value::<ChangeRequest>(insert).unwrap(),
            ChangeRequest::InsertRow {
                database: "wp".into(),
                table: "t".into(),
                values: vec![("name".into(), Some("a".into())), ("slug".into(), None)],
            }
        );
    }

    #[tokio::test]
    async fn planning_a_set_cell_reads_the_column_and_returns_the_built_plan() {
        let db = FakeDatabase::new().on(
            "SELECT `name` FROM `wp`.`t` WHERE `id` = '7'",
            result(&["name"], vec![vec![text("before")]]),
        );

        let plan = plan(&db, &set_cell_request(), TIMEOUT).await.unwrap();

        assert_eq!(
            plan,
            Plan {
                description: "t.name of id 7".into(),
                statements: vec!["UPDATE `wp`.`t` SET `name` = 'x' WHERE `id` = '7'".into()],
                undo: vec!["UPDATE `wp`.`t` SET `name` = 'before' WHERE `id` = '7'".into()],
            }
        );
        assert_eq!(
            db.queries(),
            ["SELECT `name` FROM `wp`.`t` WHERE `id` = '7'"]
        );
    }

    #[tokio::test]
    async fn planning_a_set_cell_on_an_unmatched_key_is_refused() {
        let db = FakeDatabase::new().on("SELECT `name`", result(&["name"], Vec::new()));

        let err = plan(&db, &set_cell_request(), TIMEOUT).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn planning_an_insert_needs_no_read() {
        let db = FakeDatabase::new();
        let request = ChangeRequest::InsertRow {
            database: "wp".into(),
            table: "t".into(),
            values: vec![("name".into(), Some("a".into()))],
        };

        let plan = plan(&db, &request, TIMEOUT).await.unwrap();

        assert_eq!(
            plan.statements,
            ["INSERT INTO `wp`.`t` (`name`) VALUES ('a')"]
        );
        assert!(db.queries().is_empty());
    }

    #[tokio::test]
    async fn planning_a_duplicate_counts_the_matching_rows_and_refuses_several() {
        let db = FakeDatabase::new().on(
            "SELECT COUNT(*) FROM `wp`.`t` WHERE `id` = '7'",
            result(&["COUNT(*)"], vec![vec![Cell::Number("2".into())]]),
        );
        let request = ChangeRequest::DuplicateRow {
            database: "wp".into(),
            table: "t".into(),
            key_column: "id".into(),
            key: "7".into(),
        };

        let err = plan(&db, &request, TIMEOUT).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn planning_a_delete_reads_the_whole_row_for_the_undo() {
        let db = FakeDatabase::new().on(
            "SELECT * FROM `wp`.`t` WHERE `id` = '7'",
            result(
                &["id", "name"],
                vec![vec![Cell::Number("7".into()), text("a")]],
            ),
        );
        let request = ChangeRequest::DeleteRow {
            database: "wp".into(),
            table: "t".into(),
            key_column: "id".into(),
            key: "7".into(),
        };

        let plan = plan(&db, &request, TIMEOUT).await.unwrap();

        assert_eq!(
            plan.undo,
            ["INSERT INTO `wp`.`t` (`id`, `name`) VALUES ('7', 'a')"]
        );
    }
}
