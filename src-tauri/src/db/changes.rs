//! Row edits as reviewable SQL: each request is planned into statements plus the statements
//! that undo it (built from the current values). Every planner fetches what it needs, then a
//! pure builder turns that state into a `Plan`.

use std::time::Duration;

use serde::Deserialize;
use ts_rs::TS;

use super::browse::equals_sql;
use super::{Cell, ColumnEquals, ColumnInfo, Database, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

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

/// A planned write, kept in the session until it is applied or discarded.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub description: String,
    pub statements: Vec<String>,
    /// Restores the previous state; empty when a change can't be undone automatically.
    pub undo: Vec<String>,
}

fn sql_value(dialect: Dialect, v: Option<&str>) -> String {
    v.map_or_else(|| "NULL".to_string(), |v| dialect.sql_string(v))
}

/// The SQL literal that restores `cell` exactly: `NULL` stays `NULL`, `''` stays `''`.
/// `None` for binary cells, which only carry a preview.
pub fn restore_literal(dialect: Dialect, cell: &Cell) -> Option<String> {
    match cell {
        Cell::Bytes { .. } => None,
        _ => Some(sql_value(dialect, cell.as_text())),
    }
}

/// `INSERT` statements that recreate the given rows exactly, leaving out `skip_columns`
/// (generated columns, which the server computes); empty when a row holds binary data.
pub fn reinsert_sql(
    dialect: Dialect,
    table: &str,
    rows: &ResultSet,
    skip_columns: &[String],
) -> Vec<String> {
    let kept: Vec<usize> = (0..rows.columns.len())
        .filter(|&i| !skip_columns.contains(&rows.columns[i].name))
        .collect();
    let Some(values) = rows
        .rows
        .iter()
        .map(|r| {
            kept.iter()
                .map(|&i| restore_literal(dialect, &r[i]))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    let cols = kept
        .iter()
        .map(|&i| dialect.quote_ident(&rows.columns[i].name))
        .collect::<Vec<_>>()
        .join(", ");
    values
        .iter()
        .map(|v| format!("INSERT INTO {table} ({cols}) VALUES ({})", v.join(", ")))
        .collect()
}

/// A description note for rows a re-insert may not restore: zero dates fail under
/// `NO_ZERO_DATE`.
pub fn reinsert_note(rows: &ResultSet) -> &'static str {
    let zero_date = rows
        .rows
        .iter()
        .flatten()
        .any(|cell| matches!(cell, Cell::Temporal(t) if t.starts_with("0000-00-00")));
    if zero_date {
        " (undo re-inserts a zero date, which fails under NO_ZERO_DATE)"
    } else {
        ""
    }
}

/// The plan's `undo` restoring `column` from `old`, plus a description note when there is
/// none.
fn restore_undo(
    dialect: Dialect,
    table: &str,
    column: &str,
    old: &Cell,
    filter: Option<&str>,
) -> (Vec<String>, String) {
    let before = restore_literal(dialect, old);
    if let (Some(before), Some(filter)) = (&before, filter) {
        let sql = format!(
            "UPDATE {table} SET {} = {before} WHERE {filter}",
            dialect.quote_ident(column)
        );
        return (vec![sql], String::new());
    }
    let reason = if before.is_none() {
        "binary value"
    } else {
        "the key becomes NULL"
    };
    (Vec::new(), format!(" (no undo: {reason})"))
}

/// Columns the server computes; inserting a value into them fails.
pub fn generated_columns(columns: &[ColumnInfo]) -> Vec<String> {
    columns
        .iter()
        .filter(|c| c.generated)
        .map(|c| c.name.clone())
        .collect()
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

/// `old` holds the edited column of every row the key matches.
fn build_set_cell(
    dialect: Dialect,
    row: ChangeKey,
    column: &str,
    value: Option<&str>,
    old: &ResultSet,
) -> AppResult<Plan> {
    let ChangeKey {
        table,
        key_column,
        key,
        ..
    } = row;
    if old.rows.len() != 1 {
        return Err(AppError::InvalidInput(format!(
            "{key_column} = {key} matches {} rows; edits need a unique key",
            old.rows.len()
        )));
    }
    let t = row.table_ref(dialect);
    let filter = row.filter(dialect);
    // After a key change the row is only found by its new key.
    let undo_filter = if column == key_column {
        value.map(|v| key_filter(dialect, key_column, v))
    } else {
        Some(filter.clone())
    };
    let (undo, note) = restore_undo(dialect, &t, column, &old.rows[0][0], undo_filter.as_deref());
    Ok(Plan {
        description: format!("{table}.{column} of {key_column} {key}{note}"),
        statements: vec![format!(
            "UPDATE {t} SET {} = {} WHERE {filter}",
            dialect.quote_ident(column),
            sql_value(dialect, value)
        )],
        undo,
    })
}

/// `rows` holds every row the key matches, all columns.
fn build_delete_row(
    dialect: Dialect,
    row: ChangeKey,
    rows: &ResultSet,
    generated: &[String],
) -> AppResult<Plan> {
    let ChangeKey {
        table,
        key_column,
        key,
        ..
    } = row;
    if rows.rows.len() != 1 {
        return Err(AppError::InvalidInput(format!(
            "{key_column} = {key} matches {} rows",
            rows.rows.len()
        )));
    }
    let t = row.table_ref(dialect);
    Ok(Plan {
        description: format!(
            "Delete {table} row {key_column} {key}{}",
            reinsert_note(rows)
        ),
        statements: vec![format!(
            "DELETE FROM {t} WHERE {} LIMIT 1",
            row.filter(dialect)
        )],
        undo: reinsert_sql(dialect, &t, rows, generated),
    })
}

/// `matches` is how many rows the key selects; `columns` is the table's structure.
fn build_duplicate_row(
    dialect: Dialect,
    row: ChangeKey,
    matches: u64,
    columns: &[ColumnInfo],
) -> AppResult<Plan> {
    let ChangeKey {
        table,
        key_column,
        key,
        ..
    } = row;
    if matches != 1 {
        return Err(AppError::InvalidInput(format!(
            "{key_column} = {key} matches {matches} rows; duplicating needs a unique key"
        )));
    }
    let cols = columns
        .iter()
        .filter(|c| !c.auto_increment && !c.generated)
        .map(|c| dialect.quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let t = row.table_ref(dialect);
    Ok(Plan {
        description: format!("Duplicate {table} row {key_column} {key}"),
        statements: vec![format!(
            "INSERT INTO {t} ({cols}) SELECT {cols} FROM {t} WHERE {}",
            row.filter(dialect)
        )],
        undo: Vec::new(),
    })
}

fn build_insert_row(
    dialect: Dialect,
    database: &str,
    table: &str,
    values: &[(String, Option<String>)],
) -> Plan {
    let cols = values
        .iter()
        .map(|(c, _)| dialect.quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let vals = values
        .iter()
        .map(|(_, v)| sql_value(dialect, v.as_deref()))
        .collect::<Vec<_>>()
        .join(", ");
    Plan {
        description: format!("Insert into {table}"),
        statements: vec![format!(
            "INSERT INTO {} ({cols}) VALUES ({vals})",
            dialect.table_ref(database, table)
        )],
        undo: Vec::new(),
    }
}

/// `build_insert_row` with an undo that deletes the newest row (by `key_column`) holding exactly
/// the inserted values, since the new key is only known once the insert ran.
fn build_add_row(
    dialect: Dialect,
    database: &str,
    table: &str,
    key_column: &str,
    values: &[(String, Option<String>)],
) -> Plan {
    let insert = build_insert_row(dialect, database, table, values);
    let same = values
        .iter()
        .map(|(c, v)| {
            format!(
                "{} <=> {}",
                dialect.quote_ident(c),
                sql_value(dialect, v.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ");
    Plan {
        description: format!("Add a {table} row"),
        undo: vec![format!(
            "DELETE FROM {} WHERE {same} ORDER BY {} DESC LIMIT 1",
            dialect.table_ref(database, table),
            dialect.quote_ident(key_column)
        )],
        ..insert
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnMeta, FakeDatabase};
    use crate::error::ErrorKind;

    const TIMEOUT: Duration = Duration::from_secs(1);

    const ROW: ChangeKey<'static> = ChangeKey {
        database: "wp",
        table: "t",
        key_column: "id",
        key: "7",
    };

    fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: columns
                .iter()
                .map(|name| ColumnMeta {
                    name: name.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            ..Default::default()
        }
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            column_type: "text".into(),
            nullable: true,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        }
    }

    fn auto_increment_column(name: &str) -> ColumnInfo {
        ColumnInfo {
            auto_increment: true,
            ..column(name)
        }
    }

    fn generated_column(name: &str) -> ColumnInfo {
        ColumnInfo {
            generated: true,
            ..column(name)
        }
    }

    fn text(s: &str) -> Cell {
        Cell::Text(s.into())
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

    #[test]
    fn set_cell_updates_one_cell_and_undo_restores_the_old_value() {
        let old = result(&["name"], vec![vec![text("before")]]);

        let plan = build_set_cell(Dialect::MYSQL, ROW, "name", Some("it's"), &old).unwrap();

        assert_eq!(
            plan,
            Plan {
                description: "t.name of id 7".into(),
                statements: vec!["UPDATE `wp`.`t` SET `name` = 'it''s' WHERE `id` = '7'".into()],
                undo: vec!["UPDATE `wp`.`t` SET `name` = 'before' WHERE `id` = '7'".into()],
            }
        );
    }

    #[test]
    fn set_cell_to_null_writes_null() {
        let old = result(&["name"], vec![vec![text("before")]]);

        let plan = build_set_cell(Dialect::MYSQL, ROW, "name", None, &old).unwrap();

        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`t` SET `name` = NULL WHERE `id` = '7'"]
        );
    }

    #[test]
    fn set_cell_undo_restores_the_old_cell_exactly() {
        for (old, literal) in [
            (text(""), "''"),
            (Cell::Null, "NULL"),
            (text("it's"), "'it''s'"),
            (Cell::Number("5".into()), "'5'"),
            (
                Cell::Temporal("0000-00-00 00:00:00".into()),
                "'0000-00-00 00:00:00'",
            ),
        ] {
            let plan = build_set_cell(
                Dialect::MYSQL,
                ROW,
                "name",
                Some("x"),
                &result(&["name"], vec![vec![old.clone()]]),
            )
            .unwrap();

            assert_eq!(
                plan.undo,
                [format!(
                    "UPDATE `wp`.`t` SET `name` = {literal} WHERE `id` = '7'"
                )],
                "{old:?}"
            );
        }
    }

    #[test]
    fn set_cell_on_a_binary_value_has_no_undo_and_says_so() {
        let old = result(
            &["name"],
            vec![vec![Cell::Bytes {
                len: 3,
                hex_preview: "00ff00".into(),
            }]],
        );

        let plan = build_set_cell(Dialect::MYSQL, ROW, "name", Some("x"), &old).unwrap();

        assert!(plan.undo.is_empty());
        assert_eq!(plan.description, "t.name of id 7 (no undo: binary value)");
        assert_eq!(plan.statements.len(), 1);
    }

    #[test]
    fn changing_the_key_column_undoes_through_the_new_key() {
        let old = result(&["id"], vec![vec![Cell::Number("7".into())]]);

        let plan = build_set_cell(Dialect::MYSQL, ROW, "id", Some("8"), &old).unwrap();

        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`t` SET `id` = '8' WHERE `id` = '7'"]
        );
        assert_eq!(
            plan.undo,
            ["UPDATE `wp`.`t` SET `id` = '7' WHERE `id` = '8'"]
        );
    }

    #[test]
    fn setting_the_key_column_to_null_has_no_undo() {
        let old = result(&["id"], vec![vec![Cell::Number("7".into())]]);

        let plan = build_set_cell(Dialect::MYSQL, ROW, "id", None, &old).unwrap();

        assert!(plan.undo.is_empty());
        assert_eq!(
            plan.description,
            "t.id of id 7 (no undo: the key becomes NULL)"
        );
    }

    #[test]
    fn set_cell_refuses_a_key_that_matches_no_or_several_rows() {
        for count in [0, 2] {
            let old = result(&["name"], vec![vec![text("a")]; count]);

            let err = build_set_cell(Dialect::MYSQL, ROW, "name", Some("x"), &old).unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{count} rows");
            assert_eq!(
                err.to_string(),
                format!("id = 7 matches {count} rows; edits need a unique key")
            );
        }
    }

    #[test]
    fn delete_row_removes_one_row_and_undo_reinserts_it_without_generated_columns() {
        let rows = result(
            &["id", "name", "total"],
            vec![vec![
                Cell::Number("7".into()),
                text("a"),
                Cell::Number("9".into()),
            ]],
        );

        let plan = build_delete_row(Dialect::MYSQL, ROW, &rows, &["total".into()]).unwrap();

        assert_eq!(
            plan,
            Plan {
                description: "Delete t row id 7".into(),
                statements: vec!["DELETE FROM `wp`.`t` WHERE `id` = '7' LIMIT 1".into()],
                undo: vec!["INSERT INTO `wp`.`t` (`id`, `name`) VALUES ('7', 'a')".into()],
            }
        );
    }

    #[test]
    fn delete_row_refuses_a_key_that_matches_no_or_several_rows() {
        for count in [0, 2] {
            let rows = result(&["id"], vec![vec![Cell::Number("7".into())]; count]);

            let err = build_delete_row(Dialect::MYSQL, ROW, &rows, &[]).unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{count} rows");
            assert_eq!(err.to_string(), format!("id = 7 matches {count} rows"));
        }
    }

    #[test]
    fn delete_row_with_a_zero_date_warns_in_the_description() {
        let rows = result(
            &["id", "created"],
            vec![vec![
                Cell::Number("7".into()),
                Cell::Temporal("0000-00-00 00:00:00".into()),
            ]],
        );

        let plan = build_delete_row(Dialect::MYSQL, ROW, &rows, &[]).unwrap();

        assert_eq!(
            plan.description,
            "Delete t row id 7 (undo re-inserts a zero date, which fails under NO_ZERO_DATE)"
        );
    }

    #[test]
    fn delete_row_with_binary_data_has_no_undo() {
        let rows = result(
            &["id", "blob"],
            vec![vec![
                Cell::Number("7".into()),
                Cell::Bytes {
                    len: 2,
                    hex_preview: "0001".into(),
                },
            ]],
        );

        let plan = build_delete_row(Dialect::MYSQL, ROW, &rows, &[]).unwrap();

        assert_eq!(plan.statements.len(), 1);
        assert!(plan.undo.is_empty());
    }

    #[test]
    fn duplicate_row_copies_all_but_auto_increment_and_generated_columns() {
        let columns = [
            auto_increment_column("id"),
            column("name"),
            column("slug"),
            generated_column("upper_name"),
            generated_column("stored_name"),
        ];

        let plan = build_duplicate_row(Dialect::MYSQL, ROW, 1, &columns).unwrap();

        assert_eq!(
            plan,
            Plan {
                description: "Duplicate t row id 7".into(),
                statements: vec![
                    "INSERT INTO `wp`.`t` (`name`, `slug`) SELECT `name`, `slug` FROM `wp`.`t` WHERE `id` = '7'"
                        .into()
                ],
                undo: Vec::new(),
            }
        );
    }

    #[test]
    fn duplicate_row_refuses_a_key_that_does_not_match_exactly_one_row() {
        for matches in [0, 2] {
            let err =
                build_duplicate_row(Dialect::MYSQL, ROW, matches, &[column("name")]).unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{matches} rows");
            assert_eq!(
                err.to_string(),
                format!("id = 7 matches {matches} rows; duplicating needs a unique key")
            );
        }
    }

    #[test]
    fn insert_row_lists_columns_and_values_with_null() {
        let values = [
            ("name".to_string(), Some("it's".to_string())),
            ("slug".to_string(), None),
        ];

        let plan = build_insert_row(Dialect::MYSQL, "wp", "t", &values);

        assert_eq!(
            plan,
            Plan {
                description: "Insert into t".into(),
                statements: vec![
                    "INSERT INTO `wp`.`t` (`name`, `slug`) VALUES ('it''s', NULL)".into()
                ],
                undo: Vec::new(),
            }
        );
    }

    #[test]
    fn add_row_inserts_and_undo_deletes_the_newest_row_holding_the_same_values() {
        let values = [
            ("post_id".to_string(), Some("7".to_string())),
            ("meta_key".to_string(), Some("it's".to_string())),
            ("meta_value".to_string(), None),
        ];

        let plan = build_add_row(Dialect::MYSQL, "wp", "t", "meta_id", &values);

        assert_eq!(
            plan,
            Plan {
                description: "Add a t row".into(),
                statements: vec![
                    "INSERT INTO `wp`.`t` (`post_id`, `meta_key`, `meta_value`) VALUES ('7', 'it''s', NULL)"
                        .into()
                ],
                undo: vec![
                    "DELETE FROM `wp`.`t` WHERE `post_id` <=> '7' AND `meta_key` <=> 'it''s' AND `meta_value` <=> NULL ORDER BY `meta_id` DESC LIMIT 1"
                        .into()
                ],
            }
        );
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
    fn reinsert_recreates_rows_with_nulls_and_quotes() {
        let rows = result(
            &["option_id", "option_value"],
            vec![
                vec![Cell::Number("7".into()), Cell::Null],
                vec![Cell::Number("8".into()), text("it's")],
            ],
        );

        assert_eq!(
            reinsert_sql(Dialect::MYSQL, "`db`.`wp_options`", &rows, &[]),
            [
                "INSERT INTO `db`.`wp_options` (`option_id`, `option_value`) VALUES ('7', NULL)",
                "INSERT INTO `db`.`wp_options` (`option_id`, `option_value`) VALUES ('8', 'it''s')",
            ]
        );
    }

    #[test]
    fn reinsert_leaves_out_the_skipped_columns() {
        let rows = result(
            &["id", "total", "name"],
            vec![vec![
                Cell::Number("1".into()),
                Cell::Number("9".into()),
                text("a"),
            ]],
        );

        assert_eq!(
            reinsert_sql(Dialect::MYSQL, "t", &rows, &["total".into()]),
            ["INSERT INTO t (`id`, `name`) VALUES ('1', 'a')"]
        );
    }

    #[test]
    fn reinsert_of_a_row_with_bytes_returns_nothing() {
        let rows = result(
            &["id", "blob"],
            vec![
                vec![Cell::Number("1".into()), text("a")],
                vec![
                    Cell::Number("2".into()),
                    Cell::Bytes {
                        len: 1,
                        hex_preview: "ff".into(),
                    },
                ],
            ],
        );

        assert!(reinsert_sql(Dialect::MYSQL, "t", &rows, &[]).is_empty());
    }

    #[test]
    fn reinsert_writes_temporal_and_json_cells_as_their_stored_text() {
        let rows = result(
            &["created", "data"],
            vec![vec![
                Cell::Temporal("2026-01-02 03:04:05".into()),
                Cell::Json(r#"{"a":"it's"}"#.into()),
            ]],
        );

        assert_eq!(
            reinsert_sql(Dialect::MYSQL, "t", &rows, &[]),
            [
                r#"INSERT INTO t (`created`, `data`) VALUES ('2026-01-02 03:04:05', '{"a":"it''s"}')"#
            ]
        );
    }

    #[test]
    fn reinsert_note_mentions_zero_dates_only() {
        let zero = result(
            &["d"],
            vec![vec![Cell::Temporal("0000-00-00 00:00:00".into())]],
        );
        let zero_day = result(&["d"], vec![vec![Cell::Temporal("0000-00-00".into())]]);
        let real = result(&["d"], vec![vec![Cell::Temporal("2026-01-02".into())]]);

        for rows in [&zero, &zero_day] {
            assert_eq!(
                reinsert_note(rows),
                " (undo re-inserts a zero date, which fails under NO_ZERO_DATE)"
            );
        }
        assert_eq!(reinsert_note(&real), "");
    }

    #[test]
    fn generated_columns_are_the_ones_flagged_generated() {
        let columns = [
            auto_increment_column("id"),
            column("name"),
            generated_column("a"),
            generated_column("b"),
        ];

        assert_eq!(generated_columns(&columns), ["a", "b"]);
    }

    #[test]
    fn duplicate_row_keeps_writable_columns_and_skips_generated_ones() {
        let columns = [
            auto_increment_column("ID"),
            column("created"),
            column("updated"),
            generated_column("maria"),
        ];

        let plan = build_duplicate_row(Dialect::MYSQL, ROW, 1, &columns).unwrap();

        assert_eq!(
            plan.statements,
            [
                "INSERT INTO `wp`.`t` (`created`, `updated`) SELECT `created`, `updated` FROM `wp`.`t` WHERE `id` = '7'"
            ]
        );
    }

    #[test]
    fn delete_row_undo_restores_writable_columns() {
        let columns = [
            column("id"),
            column("created"),
            column("updated"),
            generated_column("maria"),
        ];
        let rows = result(
            &["id", "created", "updated", "maria"],
            vec![vec![
                Cell::Number("7".into()),
                Cell::Temporal("2026-01-02 03:04:05".into()),
                Cell::Temporal("2026-02-03 04:05:06".into()),
                text("m"),
            ]],
        );

        let plan =
            build_delete_row(Dialect::MYSQL, ROW, &rows, &generated_columns(&columns)).unwrap();

        assert_eq!(
            plan.undo,
            [
                "INSERT INTO `wp`.`t` (`id`, `created`, `updated`) VALUES ('7', '2026-01-02 03:04:05', '2026-02-03 04:05:06')"
            ]
        );
    }

    #[test]
    fn restore_literal_is_missing_only_for_binary_cells() {
        let bytes = Cell::Bytes {
            len: 1,
            hex_preview: "ff".into(),
        };

        assert_eq!(restore_literal(Dialect::MYSQL, &bytes), None);
        assert_eq!(
            restore_literal(Dialect::MYSQL, &Cell::Null).as_deref(),
            Some("NULL")
        );
        assert_eq!(
            restore_literal(Dialect::MYSQL, &text("")).as_deref(),
            Some("''")
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
