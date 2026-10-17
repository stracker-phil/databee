use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;
use zeroize::Zeroizing;

/// Everything needed to reach a database, except the password.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[ts(optional_fields = nullable)]
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
    /// Used as the Keychain account, so one password covers all databases. The user is
    /// escaped so `a@b` on `c` and `a` on `b@c` get different accounts.
    pub fn account(&self) -> String {
        let user = escape_user(&self.user);
        match &self.socket {
            Some(socket) => format!("{user}@{socket}"),
            None => format!("{user}@{}:{}", self.host, self.port),
        }
    }

    /// Stable key for recents and history: endpoint plus database.
    pub fn identity(&self) -> String {
        format!(
            "{}/{}",
            self.account(),
            self.database.as_deref().unwrap_or("")
        )
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

/// Percent-encodes the characters that separate the account's parts. Users without
/// them keep their plain name, so existing Keychain entries still match.
fn escape_user(user: &str) -> String {
    // `%` first, so the escapes added below are not escaped again.
    user.replace('%', "%25")
        .replace('@', "%40")
        .replace(':', "%3A")
}

/// A password that never shows up in logs or debug output and is zeroed when dropped.
/// It deserializes from a plain string but only serializes through `serialize_exposed`.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.expose() == other.expose()
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Secret::new)
    }
}

/// Writes the plain value, for `serialize_with` on the IPC wire format only.
pub fn serialize_exposed<S: Serializer>(
    secret: &Option<Secret>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    secret.as_ref().map(Secret::expose).serialize(serializer)
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
            database: Some("site".into()),
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
        assert_eq!(spec().identity(), "wp@127.0.0.1:3306/site");
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
        assert_eq!(spec().display_name(), "site on 127.0.0.1:3306");
        let named = ConnectionSpec {
            name: Some("Shop (Docker)".into()),
            ..spec()
        };
        assert_eq!(named.display_name(), "Shop (Docker)");
    }

    #[test]
    fn identity_without_database_ends_in_a_slash() {
        let s = ConnectionSpec {
            database: None,
            ..spec()
        };
        assert_eq!(s.identity(), "wp@127.0.0.1:3306/");
    }

    #[test]
    fn display_name_falls_back_to_database_on_socket_then_account() {
        let socket = ConnectionSpec {
            socket: Some("/tmp/mysql.sock".into()),
            ..spec()
        };
        assert_eq!(socket.display_name(), "site on /tmp/mysql.sock");
        let bare = ConnectionSpec {
            database: None,
            ..spec()
        };
        assert_eq!(bare.display_name(), "wp@127.0.0.1:3306");
    }

    #[test]
    fn serialized_spec_has_no_password_key() {
        let json = serde_json::to_value(spec()).unwrap();
        assert!(json.get("password").is_none());
    }

    #[test]
    fn account_escapes_separators_in_the_user() {
        let first = ConnectionSpec {
            user: "a@b".into(),
            host: "c".into(),
            ..spec()
        };
        let second = ConnectionSpec {
            user: "a".into(),
            host: "b@c".into(),
            ..spec()
        };
        assert_eq!(first.account(), "a%40b@c:3306");
        assert_ne!(first.account(), second.account());
    }

    #[test]
    fn secret_deserializes_from_a_plain_string() {
        assert_eq!(
            serde_json::from_str::<Secret>("\"pw\"").unwrap(),
            Secret::new("pw")
        );
    }

    #[test]
    fn secret_is_redacted_in_debug_output() {
        let debug = format!("{:?}", Secret::new("hunter2"));
        assert!(!debug.contains("hunter2"));
    }
}
