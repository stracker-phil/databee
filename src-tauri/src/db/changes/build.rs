//! Pure builders that turn the state a planner fetched into a `Plan`.

use super::restore::{reinsert_note, reinsert_sql, restore_undo, sql_value};
use super::{ChangeKey, Plan, key_filter};
use crate::db::{ColumnInfo, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

/// `old` holds the edited column of every row the key matches.
pub(super) fn build_set_cell(
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
pub(super) fn build_delete_row(
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
pub(super) fn build_duplicate_row(
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

pub(super) fn build_insert_row(
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
pub(super) fn build_add_row(
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
    use super::super::testing::*;
    use super::*;
    use crate::db::Cell;
    use crate::db::changes::generated_columns;
    use crate::error::ErrorKind;

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
}
