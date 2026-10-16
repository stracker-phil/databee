//! What a statement is, read from its words.

use super::lexer::{Piece, words};

/// Statements that can follow a `WITH` clause.
const CTE_STATEMENTS: &[&str] = &[
    "SELECT", "TABLE", "VALUES", "UPDATE", "DELETE", "INSERT", "REPLACE",
];

const ROW_STATEMENTS: &[&str] = &[
    "SELECT", "SHOW", "DESC", "DESCRIBE", "EXPLAIN", "VALUES", "TABLE", "CALL", "CHECK", "ANALYZE",
    "OPTIMIZE", "REPAIR", "CHECKSUM", "HANDLER",
];

/// Index of the word that says what the statement does: the first word, or after a `WITH`
/// clause the first statement keyword at the same depth.
pub fn verb_index(pieces: &[Piece]) -> Option<usize> {
    let first = pieces.first()?;
    if !first.is_keyword("WITH") {
        return Some(0);
    }
    pieces.iter().position(|w| {
        w.depth == first.depth && CTE_STATEMENTS.iter().any(|keyword| w.is_keyword(keyword))
    })
}

/// Whether a statement produces a result set (so an empty result still has columns).
pub fn returns_rows(statement: &str) -> bool {
    let pieces = words(statement);
    verb_index(&pieces).is_some_and(|i| {
        ROW_STATEMENTS
            .iter()
            .any(|keyword| pieces[i].is_keyword(keyword))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_row_returning_statements() {
        assert!(returns_rows("  select 1"));
        assert!(returns_rows("SHOW TABLES"));
        assert!(returns_rows("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(!returns_rows("UPDATE t SET a = 1 WHERE b"));
    }

    #[test]
    fn row_returning_verbs_are_classified() {
        for (sql, rows) in [
            ("CALL p()", true),
            ("CHECK TABLE t", true),
            ("ANALYZE TABLE t", true),
            ("OPTIMIZE TABLE t", true),
            ("REPAIR TABLE t", true),
            ("CHECKSUM TABLE t", true),
            ("HANDLER t READ FIRST", true),
            ("(SELECT 1)", true),
            ("WITH c AS (SELECT 1) TABLE c", true),
            ("WITH c AS (SELECT 1) DELETE FROM t", false),
            ("UPDATE t SET a=1", false),
            ("", false),
        ] {
            assert_eq!(returns_rows(sql), rows, "{sql}");
        }
    }

    #[test]
    fn verb_index_skips_the_with_clause() {
        for (sql, index) in [
            ("SELECT 1", Some(0)),
            ("", None),
            ("WITH c AS (SELECT 1) DELETE FROM t", Some(4)),
            ("WITH c AS (SELECT 1) SELECT * FROM c", Some(4)),
            ("WITH c AS (SELECT 1)", None),
        ] {
            assert_eq!(verb_index(&words(sql)), index, "{sql}");
        }
    }
}
