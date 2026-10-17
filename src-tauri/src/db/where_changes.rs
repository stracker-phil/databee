//! Changes to the rows a condition selects. The rows are read first and the statements name
//! exactly their keys, so the review shows what is written and the undo restores each row's
//! before-image. A condition may read the table it changes (MySQL refuses that in `UPDATE` and
//! `DELETE`), because only the read uses it.

use std::time::Duration;

use super::changes::{Plan, generated_columns, reinsert_note, reinsert_sql, restore_literal};
use super::{Database, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

/// More rows than this are refused: a plan this large cannot be reviewed.
pub const MAX_ROWS: usize = 10_000;

/// `UPDATE` of the rows of `table` where `filter` holds.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsUpdate {
    pub table: String,
    /// The primary key, which addresses each matched row.
    pub key_columns: Vec<String>,
    /// An SQL condition over the table's columns.
    pub filter: String,
    /// Column and SQL expression.
    pub set: Vec<(String, String)>,
}

/// `DELETE` of the rows of `table` where `filter` holds.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsDelete {
    pub table: String,
    pub key_columns: Vec<String>,
    pub filter: String,
}

pub async fn plan_update(
    db: &dyn Database,
    database: &str,
    updates: &[RowsUpdate],
    timeout: Duration,
) -> AppResult<Plan> {
    let dialect = db.dialect();
    let mut befores = Vec::with_capacity(updates.len());
    for update in updates {
        let mut columns = update.key_columns.clone();
        for (column, _) in &update.set {
            if !columns.contains(column) {
                columns.push(column.clone());
            }
        }
        let sql = select_sql(dialect, database, &update.table, &columns, &update.filter);
        befores.push(db.fetch(&sql, timeout).await?);
    }
    build_update(dialect, database, updates, &befores)
}

pub async fn plan_delete(
    db: &dyn Database,
    database: &str,
    delete: &RowsDelete,
    timeout: Duration,
) -> AppResult<Plan> {
    let dialect = db.dialect();
    let sql = select_sql(dialect, database, &delete.table, &[], &delete.filter);
    let rows = db.fetch(&sql, timeout).await?;
    let structure = db.structure(database, &delete.table).await?;
    build_delete(
        dialect,
        database,
        delete,
        &rows,
        &generated_columns(&structure.columns),
    )
}

/// Every column when `columns` is empty.
fn select_sql(
    dialect: Dialect,
    database: &str,
    table: &str,
    columns: &[String],
    filter: &str,
) -> String {
    let list = if columns.is_empty() {
        "*".to_string()
    } else {
        columns
            .iter()
            .map(|c| dialect.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "SELECT {list} FROM {} WHERE ({filter}) LIMIT {}",
        dialect.table_ref(database, table),
        MAX_ROWS + 1
    )
}

/// `befores[i]` holds the key columns, then the set columns, of every row `updates[i]` matches.
fn build_update(
    dialect: Dialect,
    database: &str,
    updates: &[RowsUpdate],
    befores: &[ResultSet],
) -> AppResult<Plan> {
    let mut statements = Vec::new();
    let mut undo_steps = Vec::new();
    let mut counts = Vec::new();
    let mut binary = false;
    for (update, before) in updates.iter().zip(befores) {
        check_not_key_columns(update)?;
        check_size(&update.table, before)?;
        if before.rows.is_empty() {
            continue;
        }
        counts.push(rows_text(&update.table, before.rows.len()));
        let t = dialect.table_ref(database, &update.table);
        let keys = Keys::read(dialect, &update.key_columns, before)?;
        let assignments = update
            .set
            .iter()
            .map(|(c, expr)| format!("{} = {expr}", dialect.quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        statements.push(format!(
            "UPDATE {t} SET {assignments} WHERE {}",
            keys.matching()
        ));
        match restore_statements(dialect, &t, update, before, &keys) {
            Some(undo) => undo_steps.push(undo),
            None => binary = true,
        }
    }
    if counts.is_empty() {
        return Err(no_rows());
    }
    let (undo, note) = if binary {
        (Vec::new(), " (no undo: binary value)")
    } else {
        (undo_steps.into_iter().rev().flatten().collect(), "")
    };
    Ok(Plan {
        database: database.to_string(),
        description: format!("Update {}{note}", counts.join(", ")),
        statements,
        undo,
    })
}

fn check_not_key_columns(update: &RowsUpdate) -> AppResult<()> {
    match update
        .set
        .iter()
        .find(|(c, _)| update.key_columns.contains(c))
    {
        Some((column, _)) => Err(AppError::InvalidInput(format!(
            "Updating the key column {}.{column} is not supported",
            update.table
        ))),
        None => Ok(()),
    }
}

/// One `UPDATE` per row restoring its set columns; `None` when a value is binary.
fn restore_statements(
    dialect: Dialect,
    table_ref: &str,
    update: &RowsUpdate,
    before: &ResultSet,
    keys: &Keys,
) -> Option<Vec<String>> {
    before
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let set = update
                .set
                .iter()
                .map(|(c, _)| {
                    let col = before.columns.iter().position(|m| &m.name == c)?;
                    let old = restore_literal(dialect, &row[col])?;
                    Some(format!("{} = {old}", dialect.quote_ident(c)))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(format!(
                "UPDATE {table_ref} SET {} WHERE {}",
                set.join(", "),
                keys.matches_row(i)
            ))
        })
        .collect()
}

fn no_rows() -> AppError {
    AppError::InvalidInput("No rows match".into())
}

/// `rows` holds every column of every row `delete` matches; `generated` are left out of the
/// re-inserts.
fn build_delete(
    dialect: Dialect,
    database: &str,
    delete: &RowsDelete,
    rows: &ResultSet,
    generated: &[String],
) -> AppResult<Plan> {
    check_size(&delete.table, rows)?;
    if rows.rows.is_empty() {
        return Err(no_rows());
    }
    let t = dialect.table_ref(database, &delete.table);
    let keys = Keys::read(dialect, &delete.key_columns, rows)?;
    Ok(Plan {
        database: database.to_string(),
        description: format!(
            "Delete {}{}",
            rows_text(&delete.table, rows.rows.len()),
            reinsert_note(rows)
        ),
        statements: vec![format!("DELETE FROM {t} WHERE {}", keys.matching())],
        undo: reinsert_sql(dialect, &t, rows, generated),
    })
}

fn check_size(table: &str, rows: &ResultSet) -> AppResult<()> {
    if rows.rows.len() > MAX_ROWS {
        return Err(AppError::InvalidInput(format!(
            "More than {MAX_ROWS} rows of {table} match"
        )));
    }
    Ok(())
}

fn rows_text(table: &str, count: usize) -> String {
    let rows = if count == 1 { "row" } else { "rows" };
    format!("{count} {table} {rows}")
}

/// The key of every matched row, as quoted columns and SQL literals.
struct Keys {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Keys {
    fn read(dialect: Dialect, key_columns: &[String], rows: &ResultSet) -> AppResult<Self> {
        if key_columns.is_empty() {
            return Err(AppError::InvalidInput(
                "Rows without a primary key cannot be changed by condition".into(),
            ));
        }
        let positions = key_columns
            .iter()
            .map(|k| {
                rows.columns
                    .iter()
                    .position(|c| c.name.eq_ignore_ascii_case(k))
                    .ok_or_else(|| {
                        AppError::InvalidInput(format!("The rows have no key column {k}"))
                    })
            })
            .collect::<AppResult<Vec<usize>>>()?;
        let values = rows
            .rows
            .iter()
            .map(|row| {
                key_columns
                    .iter()
                    .zip(&positions)
                    .map(|(k, &i)| {
                        let value = row[i].as_text().ok_or_else(|| {
                            AppError::InvalidInput(format!("A row has no usable value in {k}"))
                        })?;
                        Ok(dialect.sql_string(value))
                    })
                    .collect::<AppResult<Vec<String>>>()
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Self {
            columns: key_columns.iter().map(|k| dialect.quote_ident(k)).collect(),
            rows: values,
        })
    }

    fn matches_row(&self, i: usize) -> String {
        self.columns
            .iter()
            .zip(&self.rows[i])
            .map(|(c, v)| format!("{c} = {v}"))
            .collect::<Vec<_>>()
            .join(" AND ")
    }

    /// Every row: `k IN (…)` for a single-column key.
    fn matching(&self) -> String {
        if let [column] = self.columns.as_slice() {
            let values: Vec<&str> = self.rows.iter().map(|r| r[0].as_str()).collect();
            return format!("{column} IN ({})", values.join(", "));
        }
        if self.rows.len() == 1 {
            return self.matches_row(0);
        }
        (0..self.rows.len())
            .map(|i| format!("({})", self.matches_row(i)))
            .collect::<Vec<_>>()
            .join(" OR ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ColumnMeta, FakeDatabase};

    const D: Dialect = Dialect::MYSQL;

    fn rows(columns: &[&str], rows: &[&[&str]]) -> ResultSet {
        ResultSet {
            columns: columns
                .iter()
                .map(|name| ColumnMeta {
                    name: name.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows: rows
                .iter()
                .map(|row| row.iter().map(|c| Cell::Text(c.to_string())).collect())
                .collect(),
            ..Default::default()
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn update(table: &str, key: &[&str], set: &[(&str, &str)]) -> RowsUpdate {
        RowsUpdate {
            table: table.into(),
            key_columns: strings(key),
            filter: "1".into(),
            set: set
                .iter()
                .map(|(c, e)| (c.to_string(), e.to_string()))
                .collect(),
        }
    }

    fn delete(table: &str, key: &[&str]) -> RowsDelete {
        RowsDelete {
            table: table.into(),
            key_columns: strings(key),
            filter: "1".into(),
        }
    }

    fn invalid_input(err: AppError) -> String {
        match err {
            AppError::InvalidInput(message) => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn two_tables_give_two_statements_and_undo_runs_the_last_step_first() {
        let updates = [
            update("orders", &["id"], &[("status", "'x'")]),
            update("posts", &["ID"], &[("post_status", "'x'")]),
        ];
        let befores = [
            rows(&["id", "status"], &[&["1", "a"], &["2", "b"]]),
            rows(&["ID", "post_status"], &[&["1", "c"]]),
        ];

        let plan = build_update(D, "wp", &updates, &befores).unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Update 2 orders rows, 1 posts row".into(),
                statements: strings(&[
                    "UPDATE `wp`.`orders` SET `status` = 'x' WHERE `id` IN ('1', '2')",
                    "UPDATE `wp`.`posts` SET `post_status` = 'x' WHERE `ID` IN ('1')",
                ]),
                undo: strings(&[
                    "UPDATE `wp`.`posts` SET `post_status` = 'c' WHERE `ID` = '1'",
                    "UPDATE `wp`.`orders` SET `status` = 'a' WHERE `id` = '1'",
                    "UPDATE `wp`.`orders` SET `status` = 'b' WHERE `id` = '2'",
                ]),
            }
        );
    }

    #[test]
    fn undo_restores_null_and_empty_values_exactly() {
        let updates = [update("orders", &["id"], &[("a", "'x'"), ("b", "'x'")])];
        let mut before = rows(&["id", "a", "b"], &[&["1", "", ""]]);
        before.rows[0][2] = Cell::Null;

        let plan = build_update(D, "wp", &updates, &[before]).unwrap();

        assert_eq!(
            plan.undo,
            ["UPDATE `wp`.`orders` SET `a` = '', `b` = NULL WHERE `id` = '1'"]
        );
    }

    #[test]
    fn an_update_matching_no_rows_anywhere_is_an_error() {
        let updates = [
            update("orders", &["id"], &[("status", "'x'")]),
            update("posts", &["ID"], &[("post_status", "'x'")]),
        ];
        let befores = [
            rows(&["id", "status"], &[]),
            rows(&["ID", "post_status"], &[]),
        ];

        let err = build_update(D, "wp", &updates, &befores).unwrap_err();

        assert_eq!(invalid_input(err), "No rows match");
    }

    #[test]
    fn a_step_without_rows_is_skipped_while_another_step_has_rows() {
        let updates = [
            update("orders", &["id"], &[("status", "'x'")]),
            update("posts", &["ID"], &[("post_status", "'x'")]),
        ];
        let befores = [
            rows(&["id", "status"], &[]),
            rows(&["ID", "post_status"], &[&["7", "c"]]),
        ];

        let plan = build_update(D, "wp", &updates, &befores).unwrap();

        assert_eq!(plan.description, "Update 1 posts row");
        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`posts` SET `post_status` = 'x' WHERE `ID` IN ('7')"]
        );
        assert_eq!(
            plan.undo,
            ["UPDATE `wp`.`posts` SET `post_status` = 'c' WHERE `ID` = '7'"]
        );
    }

    #[test]
    fn more_than_max_rows_are_refused() {
        let row: &[&str] = &["1", "a"];
        let many = vec![row; MAX_ROWS + 1];
        let before = rows(&["id", "status"], &many);
        let refused = format!("More than {MAX_ROWS} rows of orders match");

        let updates = [update("orders", &["id"], &[("status", "'x'")])];
        let err = build_update(D, "wp", &updates, std::slice::from_ref(&before)).unwrap_err();
        assert_eq!(invalid_input(err), refused);

        let err = build_delete(D, "wp", &delete("orders", &["id"]), &before, &[]).unwrap_err();
        assert_eq!(invalid_input(err), refused);
    }

    #[test]
    fn exactly_max_rows_are_accepted() {
        let row: &[&str] = &["1"];
        let many = vec![row; MAX_ROWS];

        let plan = build_delete(
            D,
            "wp",
            &delete("orders", &["id"]),
            &rows(&["id"], &many),
            &[],
        )
        .unwrap();

        assert_eq!(plan.description, format!("Delete {MAX_ROWS} orders rows"));
    }

    #[test]
    fn updating_a_key_column_is_refused_even_when_nothing_matches() {
        let updates = [update("orders", &["id"], &[("id", "5")])];

        let err = build_update(D, "wp", &updates, &[rows(&["id"], &[])]).unwrap_err();

        assert_eq!(
            invalid_input(err),
            "Updating the key column orders.id is not supported"
        );
    }

    #[test]
    fn a_binary_old_value_keeps_the_update_but_drops_the_undo() {
        let updates = [update("orders", &["id"], &[("blob", "'x'")])];
        let mut before = rows(&["id", "blob"], &[&["1", ""]]);
        before.rows[0][1] = Cell::Bytes {
            len: 2,
            hex_preview: "00ff".into(),
        };

        let plan = build_update(D, "wp", &updates, &[before]).unwrap();

        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`orders` SET `blob` = 'x' WHERE `id` IN ('1')"]
        );
        assert!(plan.undo.is_empty());
        assert_eq!(
            plan.description,
            "Update 1 orders row (no undo: binary value)"
        );
    }

    #[test]
    fn a_composite_key_filters_each_row_by_all_key_columns_joined_with_or() {
        let updates = [update("rel", &["a", "b"], &[("v", "'x'")])];
        let before = rows(&["a", "b", "v"], &[&["1", "2", "o"], &["3", "4", "p"]]);

        let plan = build_update(D, "wp", &updates, &[before]).unwrap();

        assert_eq!(
            plan.statements,
            [
                "UPDATE `wp`.`rel` SET `v` = 'x' WHERE (`a` = '1' AND `b` = '2') OR (`a` = '3' AND `b` = '4')"
            ]
        );
        assert_eq!(
            plan.undo,
            [
                "UPDATE `wp`.`rel` SET `v` = 'o' WHERE `a` = '1' AND `b` = '2'",
                "UPDATE `wp`.`rel` SET `v` = 'p' WHERE `a` = '3' AND `b` = '4'",
            ]
        );
    }

    #[test]
    fn a_single_composite_row_needs_no_parentheses() {
        let before = rows(&["a", "b"], &[&["1", "2"]]);

        let plan = build_delete(D, "wp", &delete("rel", &["a", "b"]), &before, &[]).unwrap();

        assert_eq!(
            plan.statements,
            ["DELETE FROM `wp`.`rel` WHERE `a` = '1' AND `b` = '2'"]
        );
    }

    #[test]
    fn key_values_with_quotes_are_escaped() {
        let before = rows(&["k", "v"], &[&["it's", "o"]]);
        let updates = [update("opts", &["k"], &[("v", "'x'")])];

        let plan = build_update(D, "wp", &updates, &[before]).unwrap();

        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`opts` SET `v` = 'x' WHERE `k` IN ('it''s')"]
        );
    }

    #[test]
    fn rows_without_a_primary_key_cannot_be_changed() {
        let updates = [update("log", &[], &[("v", "'x'")])];

        let err = build_update(D, "wp", &updates, &[rows(&["v"], &[&["o"]])]).unwrap_err();

        assert_eq!(
            invalid_input(err),
            "Rows without a primary key cannot be changed by condition"
        );
    }

    #[test]
    fn a_delete_removes_rows_by_key_and_undo_reinserts_them_without_generated_columns() {
        let before = rows(
            &["id", "name", "name_len"],
            &[&["1", "ab", "2"], &["2", "it's", "4"]],
        );

        let plan = build_delete(
            D,
            "wp",
            &delete("people", &["id"]),
            &before,
            &strings(&["name_len"]),
        )
        .unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Delete 2 people rows".into(),
                statements: strings(&["DELETE FROM `wp`.`people` WHERE `id` IN ('1', '2')"]),
                undo: strings(&[
                    "INSERT INTO `wp`.`people` (`id`, `name`) VALUES ('1', 'ab')",
                    "INSERT INTO `wp`.`people` (`id`, `name`) VALUES ('2', 'it''s')",
                ]),
            }
        );
    }

    #[test]
    fn a_delete_matching_no_rows_is_an_error() {
        let err = build_delete(
            D,
            "wp",
            &delete("people", &["id"]),
            &rows(&["id"], &[]),
            &[],
        )
        .unwrap_err();

        assert_eq!(invalid_input(err), "No rows match");
    }

    #[test]
    fn deleting_a_zero_date_notes_that_the_undo_may_fail() {
        let mut before = rows(&["id", "at"], &[&["1", ""]]);
        before.rows[0][1] = Cell::Temporal("0000-00-00 00:00:00".into());

        let plan = build_delete(D, "wp", &delete("people", &["id"]), &before, &[]).unwrap();

        assert_eq!(
            plan.description,
            "Delete 1 people row (undo re-inserts a zero date, which fails under NO_ZERO_DATE)"
        );
    }

    #[tokio::test]
    async fn planning_an_update_reads_the_keys_and_set_columns_of_the_matching_rows() {
        let db = FakeDatabase::new().on("SELECT", rows(&["id", "status"], &[&["1", "a"]]));
        let updates = [RowsUpdate {
            filter: "status = 'old'".into(),
            ..update("orders", &["id"], &[("status", "'x'")])
        }];

        let plan = plan_update(&db, "wp", &updates, Duration::from_secs(1))
            .await
            .unwrap();

        assert_eq!(
            db.queries(),
            ["SELECT `id`, `status` FROM `wp`.`orders` WHERE (status = 'old') LIMIT 10001"]
        );
        assert_eq!(
            plan.statements,
            ["UPDATE `wp`.`orders` SET `status` = 'x' WHERE `id` IN ('1')"]
        );
    }
}
