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

/// Synonyms in MySQL: each describes a table or explains a statement.
const EXPLAIN_VERBS: &[&str] = &["EXPLAIN", "DESCRIBE", "DESC"];

const STATEMENT_PREVIEW_CHARS: usize = 80;

/// `Ok` when every statement of the script only reads: `SELECT`, `SHOW`, `DESCRIBE`/`DESC`,
/// `EXPLAIN` or `WITH … SELECT`. Otherwise the first refused statement and why.
pub fn is_read_only(script: &str) -> Result<(), String> {
    let statements = split_statements(script);
    if statements.is_empty() {
        return Err("There is no statement to run".into());
    }
    for statement in statements {
        if let Some(reason) = write_reason(&words(statement)) {
            return Err(format!(
                "`{}` is not read-only: {reason}",
                preview(statement)
            ));
        }
    }
    Ok(())
}

fn write_reason(pieces: &[Piece]) -> Option<String> {
    if pieces.is_empty() {
        return Some("it has no statement keyword".into());
    }
    let Some(verb_at) = verb_index(pieces) else {
        return Some("a WITH clause needs a SELECT".into());
    };
    let verb = &pieces[verb_at];
    if verb.is_keyword("SELECT") {
        return select_reason(pieces);
    }
    if verb.is_keyword("SHOW") {
        return None;
    }
    if EXPLAIN_VERBS.iter().any(|k| verb.is_keyword(k)) {
        return explain_reason(&pieces[verb_at + 1..]);
    }
    let verb_name = verb.text.to_ascii_uppercase();
    Some(match verb_name.as_str() {
        "CALL" => "CALL runs a stored procedure, which may write".into(),
        "SET" => "SET changes session or server state".into(),
        "HANDLER" => "HANDLER keeps tables open across statements".into(),
        _ if pieces[0].is_keyword("WITH") => format!("the WITH clause leads into {verb_name}"),
        _ => format!("{verb_name} is not one of SELECT, SHOW, DESCRIBE, EXPLAIN or WITH … SELECT"),
    })
}

/// `SELECT … INTO` and locking reads, at any depth.
fn select_reason(pieces: &[Piece]) -> Option<String> {
    pieces.iter().enumerate().find_map(|(i, w)| {
        let next_is = |keyword| pieces.get(i + 1).is_some_and(|n| n.is_keyword(keyword));
        if w.is_keyword("INTO") {
            Some("SELECT … INTO writes to a file or variable".to_string())
        } else if w.is_keyword("FOR") && next_is("UPDATE") {
            Some("FOR UPDATE locks the rows it reads".into())
        } else if w.is_keyword("FOR") && next_is("SHARE") {
            Some("FOR SHARE locks the rows it reads".into())
        } else if w.is_keyword("LOCK") && next_is("IN") {
            Some("LOCK IN SHARE MODE locks the rows it reads".into())
        } else {
            None
        }
    })
}

/// A plain `EXPLAIN` only plans; `EXPLAIN ANALYZE` runs the statement, so it must read only.
fn explain_reason(rest: &[Piece]) -> Option<String> {
    let explained = rest
        .iter()
        .position(|w| w.is_keyword("WITH") || CTE_STATEMENTS.iter().any(|k| w.is_keyword(k)));
    let options = &rest[..explained.unwrap_or(rest.len())];
    if !options.iter().any(|w| w.is_keyword("ANALYZE")) {
        return None;
    }
    match explained {
        Some(at) => {
            write_reason(&rest[at..]).map(|reason| format!("EXPLAIN ANALYZE runs it, and {reason}"))
        }
        None => Some("EXPLAIN ANALYZE without a statement".into()),
    }
}

fn preview(statement: &str) -> String {
    let flat = statement.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(STATEMENT_PREVIEW_CHARS) {
        Some((cut, _)) => format!("{}…", &flat[..cut]),
        None => flat,
    }
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
    fn read_only_scripts_are_accepted() {
        for sql in [
            "SELECT * FROM t",
            "(SELECT 1)",
            "WITH c AS (SELECT 1) SELECT * FROM c",
            "SHOW TABLES",
            "DESCRIBE t",
            "DESC t",
            "EXPLAIN SELECT 1",
            "EXPLAIN UPDATE t SET a=1",
            "EXPLAIN ANALYZE SELECT 1",
            "SELECT 1; SELECT 2;",
            "SELECT 'INTO'",
            "SELECT 'FOR UPDATE'",
            "SELECT 1 -- INTO OUTFILE",
            "/* FOR UPDATE */ SELECT 1",
        ] {
            assert_eq!(is_read_only(sql), Ok(()), "{sql}");
        }
    }

    #[test]
    fn scripts_that_may_write_are_refused() {
        for sql in [
            "UPDATE t SET a=1",
            "DELETE FROM t",
            "INSERT INTO t VALUES (1)",
            "SELECT 1; UPDATE t SET a=1",
            "SELECT * FROM t INTO OUTFILE '/tmp/x'",
            "SELECT * FROM t INTO DUMPFILE '/tmp/x'",
            "SELECT 1 INTO @v",
            "SELECT * FROM t FOR UPDATE",
            "SELECT * FROM t FOR SHARE",
            "SELECT * FROM t LOCK IN SHARE MODE",
            "SELECT * FROM (SELECT * FROM t FOR UPDATE) s",
            "SELECT * FROM t WHERE a IN (SELECT a FROM u LOCK IN SHARE MODE)",
            "CALL p()",
            "SET @a=1",
            "HANDLER t OPEN",
            "EXPLAIN ANALYZE DELETE FROM t",
            "WITH c AS (SELECT 1) DELETE FROM t",
            "WITH c AS (SELECT 1)",
            "",
            "  ; ",
        ] {
            assert!(is_read_only(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn refusal_names_the_statement_and_the_reason() {
        assert_eq!(
            is_read_only("SELECT 1;\nDELETE   FROM t"),
            Err(
                "`DELETE FROM t` is not read-only: DELETE is not one of SELECT, SHOW, DESCRIBE, EXPLAIN or WITH … SELECT"
                    .to_string()
            )
        );
        assert_eq!(
            is_read_only("SELECT * FROM t FOR UPDATE"),
            Err(
                "`SELECT * FROM t FOR UPDATE` is not read-only: FOR UPDATE locks the rows it reads"
                    .to_string()
            )
        );
        assert_eq!(
            is_read_only("SET @a=1"),
            Err("`SET @a=1` is not read-only: SET changes session or server state".to_string())
        );
        assert_eq!(
            is_read_only(""),
            Err("There is no statement to run".to_string())
        );
    }

    #[test]
    fn long_refused_statements_are_cut_in_the_message() {
        let sql = format!("DELETE FROM {}", "t".repeat(200));
        let err = is_read_only(&sql).unwrap_err();
        assert!(
            err.starts_with(&format!("`{}…` is not read-only", &sql[..80])),
            "{err}"
        );
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
