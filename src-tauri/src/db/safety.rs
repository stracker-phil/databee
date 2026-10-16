use super::classify::verb_index;
use super::lexer::{Piece, split_statements, words};

/// Returns a human-readable reason when a script contains a statement that can destroy
/// data without an obvious scope, or `None` when it looks safe to run unconfirmed.
pub fn destructive_reason(sql: &str) -> Option<String> {
    split_statements(sql).into_iter().find_map(check_statement)
}

fn check_statement(statement: &str) -> Option<String> {
    let pieces = words(statement);
    let verb_at = verb_index(&pieces)?;
    let verb = &pieces[verb_at];
    let rest = &pieces[verb_at + 1..];
    // A WHERE inside a subquery doesn't scope the outer statement.
    let has_where = rest
        .iter()
        .any(|w| w.depth == verb.depth && w.is_keyword("WHERE"));
    let next_is = |keyword| rest.first().is_some_and(|w| w.is_keyword(keyword));

    if verb.is_keyword("DROP") {
        let object = rest
            .first()
            .map(|w| w.text.to_ascii_uppercase())
            .unwrap_or_default();
        return Some(format!("DROP {object} permanently removes data"));
    }
    if verb.is_keyword("TRUNCATE") {
        return Some("TRUNCATE removes all rows of the table".into());
    }
    if verb.is_keyword("DELETE") && !has_where {
        return Some("DELETE without WHERE removes all rows".into());
    }
    if verb.is_keyword("UPDATE") && !has_where {
        return Some("UPDATE without WHERE changes all rows".into());
    }
    if verb.is_keyword("ALTER") && rest.iter().any(|w| w.is_keyword("DROP")) {
        return Some("ALTER … DROP removes columns or indexes".into());
    }
    if verb.is_keyword("RENAME") && next_is("TABLE") {
        return Some("RENAME TABLE moves tables to new names".into());
    }
    if verb.is_keyword("CREATE") && next_is("OR") {
        return replaced_object(rest);
    }
    None
}

/// Object kinds `CREATE OR REPLACE` can name; the first one names the replaced object.
const REPLACEABLE: &[&str] = &[
    "TABLE",
    "VIEW",
    "PROCEDURE",
    "FUNCTION",
    "TRIGGER",
    "EVENT",
    "SEQUENCE",
    "INDEX",
    "DATABASE",
    "SCHEMA",
    "USER",
    "ROLE",
    "SERVER",
    "PACKAGE",
];

/// `CREATE OR REPLACE TABLE` and `VIEW` drop the existing object first.
fn replaced_object(rest: &[Piece]) -> Option<String> {
    let object = rest
        .iter()
        .find(|w| REPLACEABLE.iter().any(|kind| w.is_keyword(kind)))?;
    if object.is_keyword("TABLE") {
        return Some("CREATE OR REPLACE TABLE drops the existing table and its rows".into());
    }
    if object.is_keyword("VIEW") {
        return Some("CREATE OR REPLACE VIEW replaces the existing view".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_reads_and_scoped_writes_are_safe() {
        for sql in [
            "SELECT * FROM wp_posts",
            "DELETE FROM wp_options WHERE option_name LIKE '_transient_%'",
            "UPDATE wp_users SET user_email = 'x' WHERE ID = 1",
            "INSERT INTO t VALUES (1)",
        ] {
            assert_eq!(destructive_reason(sql), None, "{sql}");
        }
    }

    #[test]
    fn unscoped_or_dropping_statements_need_confirmation() {
        for sql in [
            "DROP TABLE wp_posts",
            "drop database wordpress",
            "TRUNCATE wp_actionscheduler_logs",
            "DELETE FROM wp_postmeta",
            "update wp_options set autoload = 'no'",
            "ALTER TABLE t DROP COLUMN c",
        ] {
            assert!(destructive_reason(sql).is_some(), "{sql}");
        }
    }

    #[test]
    fn where_inside_a_string_does_not_count_as_scope() {
        assert!(destructive_reason("UPDATE t SET note = 'where'").is_some());
    }

    #[test]
    fn any_destructive_statement_in_a_script_is_reported() {
        let reason = destructive_reason("SELECT 1; TRUNCATE t; SELECT 2").unwrap();
        assert!(reason.contains("TRUNCATE"));
    }

    #[test]
    fn where_that_does_not_scope_the_statement_is_still_flagged() {
        for sql in [
            "UPDATE t SET a = (SELECT x FROM y WHERE z)",
            "UPDATE t SET a = @where",
            "/*!50000 DROP TABLE t */",
            "WITH c AS (SELECT 1) DELETE FROM t",
            "DELETE FROM t LIMIT 1",
        ] {
            assert!(destructive_reason(sql).is_some(), "{sql}");
        }
    }

    #[test]
    fn where_after_a_subquery_still_scopes_the_statement() {
        let sql = "UPDATE t SET a = 1 WHERE id IN (SELECT 1)";
        assert_eq!(destructive_reason(sql), None);
    }

    #[test]
    fn reasons_name_the_destructive_action() {
        for (sql, reason) in [
            (
                "CREATE OR REPLACE TABLE t (id int)",
                "CREATE OR REPLACE TABLE drops the existing table and its rows",
            ),
            (
                "CREATE OR REPLACE VIEW v AS SELECT 1",
                "CREATE OR REPLACE VIEW replaces the existing view",
            ),
            (
                "RENAME TABLE a TO b",
                "RENAME TABLE moves tables to new names",
            ),
            ("DROP TABLE t", "DROP TABLE permanently removes data"),
        ] {
            assert_eq!(destructive_reason(sql).as_deref(), Some(reason), "{sql}");
        }
    }

    #[test]
    fn leading_comments_are_ignored() {
        assert!(destructive_reason("/* cleanup */ -- now\nDROP TABLE t").is_some());
    }
}
