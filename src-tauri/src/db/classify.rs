//! What a statement is, read from its words.

use super::lexer::{Piece, split_statements, words};

/// Statements that can follow a `WITH` clause.
const CTE_STATEMENTS: &[&str] = &[
    "SELECT", "TABLE", "VALUES", "UPDATE", "DELETE", "INSERT", "REPLACE",
];

const ROW_STATEMENTS: &[&str] = &[
    "SELECT", "SHOW", "DESC", "DESCRIBE", "EXPLAIN", "VALUES", "TABLE", "CALL", "CHECK", "ANALYZE",
    "OPTIMIZE", "REPAIR", "CHECKSUM", "HANDLER",
];

/// Statements that change tables, columns or keys, so cached schema goes stale (`TRUNCATE` is
/// DDL to MySQL, too).
const DDL_STATEMENTS: &[&str] = &["CREATE", "ALTER", "DROP", "RENAME", "TRUNCATE"];

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

/// Whether the statement's verb (`verb_index`) is one of `keywords`.
fn leading_verb_is(statement: &str, keywords: &[&str]) -> bool {
    let pieces = words(statement);
    verb_index(&pieces).is_some_and(|i| keywords.iter().any(|k| pieces[i].is_keyword(k)))
}

/// Whether a statement produces a result set (so an empty result still has columns).
pub fn returns_rows(statement: &str) -> bool {
    leading_verb_is(statement, ROW_STATEMENTS)
}

/// Whether any statement of the script changes the schema.
pub fn changes_schema(script: &str) -> bool {
    split_statements(script)
        .into_iter()
        .any(|statement| leading_verb_is(statement, DDL_STATEMENTS))
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
    fn ddl_verbs_and_scripts_containing_one_change_the_schema() {
        for sql in [
            "CREATE TABLE t (id int)",
            "alter table t add column x int",
            "DROP TABLE t",
            "RENAME TABLE a TO b",
            "TRUNCATE TABLE t",
            "SELECT 1; DROP TABLE t",
            "UPDATE t SET a = 1; ALTER TABLE t DROP COLUMN a;",
            "WITH c AS (SELECT 1) SELECT * FROM c; CREATE INDEX i ON t (a)",
        ] {
            assert!(changes_schema(sql), "{sql}");
        }
    }

    #[test]
    fn other_statements_do_not_change_the_schema() {
        for sql in [
            "SELECT * FROM t",
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET a = 1",
            "WITH c AS (SELECT 1) DELETE FROM t",
            "SELECT 'DROP TABLE t'",
            "SELECT 1 -- DROP TABLE t",
            "/* ALTER TABLE t */ SELECT 1",
            "UPDATE t SET a = 'CREATE TABLE x'",
            "",
            "   ;  ",
        ] {
            assert!(!changes_schema(sql), "{sql}");
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
