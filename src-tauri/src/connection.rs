use std::fmt;

use serde::{Deserialize, Serialize};

/// Everything needed to reach a database, except the password.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSpec {
    #[serde(default)]
    pub name: Option<String>,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub socket: Option<String>,
    pub user: String,
    #[serde(default)]
    pub database: Option<String>,
}

impl ConnectionSpec {
    /// The server endpoint without database, e.g. `root@127.0.0.1:3306`.
    /// Used as the Keychain account, so one password covers all databases.
    pub fn account(&self) -> String {
        match &self.socket {
            Some(socket) => format!("{}@{}", self.user, socket),
            None => format!("{}@{}:{}", self.user, self.host, self.port),
        }
    }

    /// Stable key for recents, favorites and history: endpoint plus database.
    pub fn identity(&self) -> String {
        format!("{}/{}", self.account(), self.database.as_deref().unwrap_or(""))
    }

    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| match &self.database {
            Some(db) => format!("{db} on {}", self.endpoint()),
            None => self.account(),
        })
    }

    fn endpoint(&self) -> String {
        match &self.socket {
            Some(socket) => socket.clone(),
            None => format!("{}:{}", self.host, self.port),
        }
    }
}

/// A password that never shows up in logs, debug output or serialized data.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "wp".into(),
            database: Some("wordpress".into()),
        }
    }

    #[test]
    fn account_ignores_database_so_one_password_covers_the_server() {
        let mut other = spec();
        other.database = Some("shop".into());
        assert_eq!(spec().account(), "wp@127.0.0.1:3306");
        assert_eq!(spec().account(), other.account());
    }

    #[test]
    fn identity_distinguishes_databases() {
        assert_eq!(spec().identity(), "wp@127.0.0.1:3306/wordpress");
    }

    #[test]
    fn socket_connections_are_identified_by_socket_path() {
        let s = ConnectionSpec {
            socket: Some("/tmp/mysql.sock".into()),
            ..spec()
        };
        assert_eq!(s.account(), "wp@/tmp/mysql.sock");
    }

    #[test]
    fn display_name_prefers_explicit_name() {
        assert_eq!(spec().display_name(), "wordpress on 127.0.0.1:3306");
        let named = ConnectionSpec {
            name: Some("Shop (Docker)".into()),
            ..spec()
        };
        assert_eq!(named.display_name(), "Shop (Docker)");
    }

    #[test]
    fn secret_is_redacted_in_debug_output() {
        let debug = format!("{:?}", Secret::new("hunter2"));
        assert!(!debug.contains("hunter2"));
    }
}
