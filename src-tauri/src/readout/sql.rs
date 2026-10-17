//! Saved-query SQL as the readout runs it: one statement, counted or limited, in the readout's
//! database.

use crate::db::Dialect;
use crate::db::lexer::{pieces, statement_ranges};

/// The only statement of `sql`, without its `;`.
pub fn single_statement(sql: &str) -> Result<&str, String> {
    match statement_ranges(sql).as_slice() {
        [only] => Ok(sql[only.clone()].trim().trim_end_matches(';').trim_end()),
        [] => Err("The query is empty".into()),
        _ => Err("Only a query with one statement shows on the readout".into()),
    }
}

/// `statement` with `LIMIT limit` unless it has a `LIMIT` of its own; placed after its last
/// piece of code, so a trailing comment does not swallow it.
pub fn limited(statement: &str, limit: u32) -> String {
    let code: Vec<_> = pieces(statement)
        .into_iter()
        .filter(|p| !p.is_comment())
        .collect();
    if code.iter().any(|p| p.depth == 0 && p.is_keyword("LIMIT")) {
        return statement.to_string();
    }
    let end = code.last().map_or(statement.len(), |p| p.span().end);
    format!("{} LIMIT {limit}{}", &statement[..end], &statement[end..])
}

pub fn counted(statement: &str) -> String {
    format!("SELECT COUNT(*) FROM (\n{statement}\n) AS counted")
}

/// `sql` run with `database` as the default database, since saved queries name their tables
/// without it.
pub fn in_database(dialect: Dialect, database: &str, sql: &str) -> String {
    format!("USE {};\n{sql}", dialect.quote_ident(database))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_statement_drops_the_semicolon_and_surrounding_space() {
        for (sql, expected) in [
            ("SELECT 1", "SELECT 1"),
            ("  SELECT 1;  ", "SELECT 1"),
            ("SELECT 1 ;\n", "SELECT 1"),
            ("SELECT ';' AS semi", "SELECT ';' AS semi"),
        ] {
            assert_eq!(single_statement(sql), Ok(expected), "{sql}");
        }
    }

    #[test]
    fn single_statement_refuses_an_empty_query() {
        for sql in ["", "   ", ";", "-- nothing\n"] {
            assert_eq!(
                single_statement(sql),
                Err("The query is empty".to_string()),
                "{sql:?}"
            );
        }
    }

    #[test]
    fn single_statement_refuses_several_statements() {
        for sql in ["SELECT 1; SELECT 2", "SELECT 1;\nSELECT 2;"] {
            assert_eq!(
                single_statement(sql),
                Err("Only a query with one statement shows on the readout".to_string()),
                "{sql}"
            );
        }
    }

    #[test]
    fn limited_appends_a_limit() {
        assert_eq!(
            limited("SELECT id FROM t ORDER BY id", 5),
            "SELECT id FROM t ORDER BY id LIMIT 5"
        );
    }

    #[test]
    fn limited_keeps_a_limit_of_its_own_in_any_case() {
        for sql in [
            "SELECT id FROM t LIMIT 100",
            "SELECT id FROM t limit 2, 3",
            "SELECT id FROM t LIMIT 3 OFFSET 1 -- note",
        ] {
            assert_eq!(limited(sql, 5), sql);
        }
    }

    #[test]
    fn limited_ignores_a_limit_inside_a_subquery() {
        assert_eq!(
            limited("SELECT * FROM (SELECT id FROM t LIMIT 10) AS s", 5),
            "SELECT * FROM (SELECT id FROM t LIMIT 10) AS s LIMIT 5"
        );
    }

    #[test]
    fn limited_ignores_the_word_limit_in_a_string_or_comment() {
        assert_eq!(
            limited("SELECT 'LIMIT' AS w FROM t /* LIMIT 1 */", 5),
            "SELECT 'LIMIT' AS w FROM t LIMIT 5 /* LIMIT 1 */"
        );
    }

    #[test]
    fn limited_puts_the_limit_before_a_trailing_comment() {
        assert_eq!(
            limited("SELECT id FROM t -- newest first", 5),
            "SELECT id FROM t LIMIT 5 -- newest first"
        );
    }

    #[test]
    fn counted_wraps_the_statement_in_a_derived_table() {
        assert_eq!(
            counted("SELECT id FROM t"),
            "SELECT COUNT(*) FROM (\nSELECT id FROM t\n) AS counted"
        );
    }

    #[test]
    fn in_database_selects_the_quoted_database_first() {
        assert_eq!(
            in_database(Dialect::MYSQL, "wp_test", "SELECT 1"),
            "USE `wp_test`;\nSELECT 1"
        );
    }
}
