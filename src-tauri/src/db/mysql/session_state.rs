//! What a reconnect of the editor connection carries over: the current database and the
//! session variables the user changed. Temporary tables, user variables and open transactions
//! cannot be carried over.

use sqlx::AssertSqlSafe;
use sqlx::mysql::MySqlConnection;

use super::script::raw_text;
use crate::db::Dialect;
use crate::error::AppResult;

#[derive(Clone, Copy)]
enum Literal {
    Text,
    Number,
}

/// Session variables both MySQL and MariaDB have that a user plausibly `SET`s. Character sets
/// stay out: cell decoding relies on the connection's own. `sql_mode` stays first (`SessionState::dialect`).
const KEPT: &[(&str, Literal)] = &[
    ("sql_mode", Literal::Text),
    ("time_zone", Literal::Text),
    ("autocommit", Literal::Number),
    ("foreign_key_checks", Literal::Number),
    ("unique_checks", Literal::Number),
    ("sql_safe_updates", Literal::Number),
    ("group_concat_max_len", Literal::Number),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionState {
    pub database: Option<String>,
    /// One value per `KEPT` variable, in its order.
    pub variables: Vec<Option<String>>,
}

impl SessionState {
    fn sql_mode(&self) -> &str {
        self.variables
            .first()
            .and_then(|mode| mode.as_deref())
            .unwrap_or_default()
    }

    pub fn dialect(&self) -> Dialect {
        Dialect::from_sql_mode(self.sql_mode())
    }
}

fn probe_sql() -> String {
    let variables: Vec<String> = KEPT
        .iter()
        .map(|(name, _)| format!("@@SESSION.{name}"))
        .collect();
    format!("SELECT DATABASE(), {}", variables.join(", "))
}

/// One query for the database and every kept variable.
pub async fn read(conn: &mut MySqlConnection) -> AppResult<SessionState> {
    let row = sqlx::raw_sql(AssertSqlSafe(probe_sql()))
        .fetch_optional(&mut *conn)
        .await?;
    let Some(row) = row else {
        return Ok(SessionState::default());
    };
    Ok(SessionState {
        database: raw_text(&row, 0),
        variables: (1..=KEPT.len()).map(|i| raw_text(&row, i)).collect(),
    })
}

/// The `SET` that gives a fresh connection every variable whose `current` value differs from
/// the `opened` one (both of the connection being replaced), quoted for the fresh connection's
/// `dialect`; `None` when the user changed none.
pub fn restore_sql(
    opened: &SessionState,
    current: &SessionState,
    dialect: Dialect,
) -> Option<String> {
    let assignments: Vec<String> = KEPT
        .iter()
        .enumerate()
        .filter_map(|(i, (name, literal))| {
            let value = current.variables.get(i)?.as_ref()?;
            if opened.variables.get(i).and_then(Option::as_ref) == Some(value) {
                return None;
            }
            let literal = match literal {
                Literal::Text => dialect.sql_string(value),
                Literal::Number
                    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    value.clone()
                }
                Literal::Number => return None,
            };
            Some(format!("@@SESSION.{name} = {literal}"))
        })
        .collect();
    (!assignments.is_empty()).then(|| format!("SET {}", assignments.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: [&str; 7] = ["STRICT_TRANS_TABLES", "SYSTEM", "1", "1", "1", "0", "1024"];
    const NO_BACKSLASH: Dialect = Dialect {
        backslash_escapes: false,
    };

    fn state(values: [&str; 7]) -> SessionState {
        SessionState {
            database: Some("wp".into()),
            variables: values.iter().map(|v| Some(v.to_string())).collect(),
        }
    }

    fn with(index: usize, value: &str) -> SessionState {
        let mut values = DEFAULTS;
        values[index] = value;
        state(values)
    }

    #[test]
    fn nothing_to_restore_when_no_variable_changed() {
        assert_eq!(
            restore_sql(&state(DEFAULTS), &state(DEFAULTS), Dialect::MYSQL),
            None
        );
    }

    #[test]
    fn a_state_without_variables_restores_nothing() {
        assert_eq!(
            restore_sql(
                &SessionState::default(),
                &SessionState::default(),
                Dialect::MYSQL
            ),
            None
        );
    }

    #[test]
    fn a_changed_database_alone_restores_no_variables() {
        let moved = SessionState {
            database: Some("other".into()),
            ..state(DEFAULTS)
        };

        assert_eq!(restore_sql(&state(DEFAULTS), &moved, Dialect::MYSQL), None);
    }

    #[test]
    fn only_the_changed_variables_are_set_in_their_fixed_order() {
        let current = state(["ANSI_QUOTES", "+03:00", "1", "0", "1", "0", "1024"]);

        assert_eq!(
            restore_sql(&state(DEFAULTS), &current, Dialect::MYSQL).as_deref(),
            Some(
                "SET @@SESSION.sql_mode = 'ANSI_QUOTES', @@SESSION.time_zone = '+03:00', \
                 @@SESSION.foreign_key_checks = 0"
            )
        );
    }

    #[test]
    fn a_single_changed_number_is_set_unquoted() {
        assert_eq!(
            restore_sql(&state(DEFAULTS), &with(6, "2048"), Dialect::MYSQL).as_deref(),
            Some("SET @@SESSION.group_concat_max_len = 2048")
        );
    }

    #[test]
    fn text_values_are_quoted_for_the_dialect_of_the_fresh_connection() {
        let current = with(1, r"a'b\c");

        assert_eq!(
            restore_sql(&state(DEFAULTS), &current, Dialect::MYSQL).as_deref(),
            Some(r"SET @@SESSION.time_zone = 'a''b\\c'")
        );
        assert_eq!(
            restore_sql(&state(DEFAULTS), &current, NO_BACKSLASH).as_deref(),
            Some(r"SET @@SESSION.time_zone = 'a''b\c'")
        );
    }

    #[test]
    fn numeric_variables_with_non_digit_values_are_skipped() {
        for value in ["ON", "", "1.5", "-1", "1; DROP TABLE t"] {
            assert_eq!(
                restore_sql(&state(DEFAULTS), &with(2, value), Dialect::MYSQL),
                None,
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_skipped_value_leaves_the_other_changes_in_place() {
        let mut current = with(2, "ON");
        current.variables[1] = Some("+03:00".into());

        assert_eq!(
            restore_sql(&state(DEFAULTS), &current, Dialect::MYSQL).as_deref(),
            Some("SET @@SESSION.time_zone = '+03:00'")
        );
    }

    #[test]
    fn a_variable_the_server_did_not_report_is_not_restored() {
        let mut current = state(DEFAULTS);
        current.variables[3] = None;

        assert_eq!(
            restore_sql(&state(DEFAULTS), &current, Dialect::MYSQL),
            None
        );
    }

    #[test]
    fn a_variable_missing_from_the_opened_state_counts_as_changed() {
        let mut opened = state(DEFAULTS);
        opened.variables[5] = None;

        assert_eq!(
            restore_sql(&opened, &state(DEFAULTS), Dialect::MYSQL).as_deref(),
            Some("SET @@SESSION.sql_safe_updates = 0")
        );
    }

    #[test]
    fn the_dialect_follows_the_sql_mode() {
        assert!(state(DEFAULTS).dialect().backslash_escapes);
        assert!(
            !with(0, "STRICT_TRANS_TABLES,NO_BACKSLASH_ESCAPES")
                .dialect()
                .backslash_escapes
        );
        assert!(SessionState::default().dialect().backslash_escapes);
    }
}
