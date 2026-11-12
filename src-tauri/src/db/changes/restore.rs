//! The SQL that restores what a change overwrites or removes: literals, re-inserts and undo
//! updates.

use crate::db::{Cell, ColumnInfo, Dialect, ResultSet};

pub(super) fn sql_value(dialect: Dialect, v: Option<&str>) -> String {
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
pub(super) fn restore_undo(
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

#[cfg(test)]
mod tests {
    use super::super::testing::{auto_increment_column, column, generated_column, result, text};
    use super::*;

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
}
