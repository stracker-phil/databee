use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

pub type AppResult<T> = Result<T, AppError>;

/// Every error that crosses the Tauri boundary. Serialized as
/// `{ kind, message, code? }` so the UI can decide how to present it.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Database {
        message: String,
        code: Option<String>,
    },
    #[error("Could not connect: {0}")]
    Connection(String),
    #[error("Access denied: {0}")]
    AuthFailed(String),
    #[error("A password is required for {0}")]
    PasswordRequired(String),
    #[error("Query timed out after {0} s")]
    Timeout(u32),
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

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

const MYSQL_ACCESS_DENIED: &str = "1045";
const MYSQL_QUERY_INTERRUPTED: &str = "1317";

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
        let value = serde_json::to_value(AppError::Timeout(30)).unwrap();
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
}
