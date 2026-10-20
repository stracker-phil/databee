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
pub fn restore_sql(opened: &SessionState, current: &SessionState, dialect: Dialect) -> Option<String> {
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
                Literal::Number if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                    value.clone()
                }
                Literal::Number => return None,
            };
            Some(format!("@@SESSION.{name} = {literal}"))
        })
        .collect();
    (!assignments.is_empty()).then(|| format!("SET {}", assignments.join(", ")))
}
