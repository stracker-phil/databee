use super::lexer::{split_statements, words};

/// Returns a human-readable reason when a script contains a statement that can destroy
/// data without an obvious scope, or `None` when it looks safe to run unconfirmed.
pub fn destructive_reason(sql: &str) -> Option<String> {
    split_statements(sql).into_iter().find_map(check_statement)
}

fn check_statement(statement: &str) -> Option<String> {
    let w = words(statement);
    let first = w.first()?.as_str();
    let has_where = w.iter().any(|x| x == "WHERE");
    match first {
        "DROP" => Some(format!(
            "DROP {} permanently removes data",
            w.get(1).map_or("", String::as_str)
        )),
        "TRUNCATE" => Some("TRUNCATE removes all rows of the table".into()),
        "DELETE" if !has_where => Some("DELETE without WHERE removes all rows".into()),
        "UPDATE" if !has_where => Some("UPDATE without WHERE changes all rows".into()),
        "ALTER" if w.iter().any(|x| x == "DROP") => {
            Some("ALTER … DROP removes columns or indexes".into())
        }
        _ => None,
    }
}

/// Whether a statement produces a result set (so an empty result still has columns).
pub fn returns_rows(statement: &str) -> bool {
    matches!(
        words(statement).first().map(String::as_str),
        Some("SELECT" | "SHOW" | "DESC" | "DESCRIBE" | "EXPLAIN" | "WITH" | "VALUES" | "TABLE")
    )
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
    fn leading_comments_are_ignored() {
        assert!(destructive_reason("/* cleanup */ -- now\nDROP TABLE t").is_some());
    }

    #[test]
    fn detects_row_returning_statements() {
        assert!(returns_rows("  select 1"));
        assert!(returns_rows("SHOW TABLES"));
        assert!(returns_rows("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(!returns_rows("UPDATE t SET a = 1 WHERE b"));
    }
}
