use std::time::Duration;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

pub type AppResult<T> = Result<T, AppError>;

/// Every error that crosses the Tauri boundary. Serialized as
/// `{ kind, message, code: string | null }` so the UI can decide how to present it.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Database {
        message: String,
        code: Option<String>,
    },
    #[error("Could not connect: {0}")]
    Connection(String),
    #[error("{0}")]
    AuthFailed(String),
    #[error("A password is required for {0}")]
    PasswordRequired(String),
    #[error("Query timed out after {}", human_duration(*.0))]
    Timeout(Duration),
    #[error("Query was cancelled")]
    Cancelled,
    #[error("This statement needs confirmation: {0}")]
    ConfirmationRequired(String),
    #[error("Unknown session {0}")]
    SessionNotFound(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("Keychain error: {0}")]
    Keychain(String),
    #[error("File error: {0}")]
    Io(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Database { .. } => "database",
            AppError::Connection(_) => "connection",
            AppError::AuthFailed(_) => "auth_failed",
            AppError::PasswordRequired(_) => "password_required",
            AppError::Timeout(_) => "timeout",
            AppError::Cancelled => "cancelled",
            AppError::ConfirmationRequired(_) => "confirmation_required",
            AppError::SessionNotFound(_) => "session_not_found",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::Keychain(_) => "keychain",
            AppError::Io(_) => "io",
        }
    }

    fn code(&self) -> Option<&str> {
        match self {
            AppError::Database { code, .. } => code.as_deref(),
            _ => None,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.serialize_field("code", &self.code())?;
        s.end()
    }
}

/// `500 ms` below one second, whole seconds (`30 s`) otherwise.
pub fn human_duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{} s", duration.as_secs())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

const MYSQL_ACCESS_DENIED: &str = "1045";
const MYSQL_QUERY_INTERRUPTED: &str = "1317";
/// Server shutdown, server gone, lost during query, connection killed, idle timeout: the
/// connection is unusable, so the session must reconnect.
const MYSQL_CONNECTION_LOST: &[&str] = &["1053", "2006", "2013", "1927", "4031"];

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::Database(db) => {
                // Prefer the MySQL error number (e.g. 1146) over the generic SQLSTATE.
                let code = db
                    .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
                    .map(|e| e.number().to_string())
                    .or_else(|| db.code().map(|c| c.into_owned()));
                match code.as_deref() {
                    Some(MYSQL_ACCESS_DENIED) => AppError::AuthFailed(db.message().to_string()),
                    Some(MYSQL_QUERY_INTERRUPTED) => AppError::Cancelled,
                    Some(c) if MYSQL_CONNECTION_LOST.contains(&c) => {
                        AppError::Connection(db.message().to_string())
                    }
                    _ => AppError::Database {
                        message: db.message().to_string(),
                        code,
                    },
                }
            }
            sqlx::Error::Io(io) => AppError::Connection(io.to_string()),
            sqlx::Error::Tls(tls) => AppError::Connection(tls.to_string()),
            sqlx::Error::PoolTimedOut => AppError::Connection("connection pool timed out".into()),
            sqlx::Error::PoolClosed => AppError::Connection("connection was closed".into()),
            sqlx::Error::Protocol(message) => AppError::Connection(message),
            sqlx::Error::WorkerCrashed => AppError::Connection("connection worker crashed".into()),
            other => AppError::Database {
                message: other.to_string(),
                code: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_kind_message_and_code() {
        let err = AppError::Database {
            message: "Table 'wp.x' doesn't exist".into(),
            code: Some("1146".into()),
        };
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            json!({"kind": "database", "message": "Table 'wp.x' doesn't exist", "code": "1146"})
        );
    }

    #[test]
    fn non_database_errors_have_null_code() {
        let value = serde_json::to_value(AppError::Timeout(Duration::from_secs(30))).unwrap();
        assert_eq!(value["kind"], "timeout");
        assert_eq!(value["message"], "Query timed out after 30 s");
        assert!(value["code"].is_null());
    }

    #[test]
    fn io_failures_map_to_connection_errors() {
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = AppError::from(sqlx::Error::Io(io));
        assert_eq!(err.kind(), "connection");
    }

    #[derive(Debug, thiserror::Error)]
    #[error("{message}")]
    struct FakeDatabaseError {
        message: String,
        code: String,
    }

    impl sqlx::error::DatabaseError for FakeDatabaseError {
        fn message(&self) -> &str {
            &self.message
        }

        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(std::borrow::Cow::Borrowed(&self.code))
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    fn database_error(code: &str) -> AppError {
        AppError::from(sqlx::Error::Database(Box::new(FakeDatabaseError {
            message: "server said no".into(),
            code: code.into(),
        })))
    }

    #[test]
    fn server_codes_map_to_error_kinds() {
        for (code, kind) in [
            ("42S02", "database"),
            ("1045", "auth_failed"),
            ("1317", "cancelled"),
            ("1053", "connection"),
            ("2006", "connection"),
            ("2013", "connection"),
            ("1927", "connection"),
            ("4031", "connection"),
        ] {
            assert_eq!(database_error(code).kind(), kind, "{code}");
        }
    }

    #[test]
    fn database_errors_keep_their_code_and_message() {
        match database_error("42S02") {
            AppError::Database { message, code } => {
                assert_eq!(message, "server said no");
                assert_eq!(code.as_deref(), Some("42S02"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pool_and_protocol_failures_are_connection_errors() {
        for err in [
            sqlx::Error::PoolTimedOut,
            sqlx::Error::PoolClosed,
            sqlx::Error::Protocol("bad packet".into()),
        ] {
            let label = format!("{err:?}");
            assert_eq!(AppError::from(err).kind(), "connection", "{label}");
        }
    }

    #[test]
    fn std_io_errors_map_to_io() {
        let err = AppError::from(std::io::Error::other("disk full"));
        assert_eq!(err.kind(), "io");
        assert_eq!(err.to_string(), "File error: disk full");
    }

    #[test]
    fn auth_failed_message_has_no_prefix() {
        let err = AppError::AuthFailed("Access denied for user 'x'".into());
        assert_eq!(err.to_string(), "Access denied for user 'x'");
    }

    #[test]
    fn timeout_message_uses_milliseconds_below_a_second_and_seconds_above() {
        for (duration, message) in [
            (Duration::from_millis(500), "Query timed out after 500 ms"),
            (Duration::from_secs(30), "Query timed out after 30 s"),
        ] {
            assert_eq!(AppError::Timeout(duration).to_string(), message);
        }
    }
}
