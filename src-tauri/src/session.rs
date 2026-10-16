use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::connection::{ConnectionSpec, Secret};
use crate::db::{self, Database};
use crate::error::{AppError, AppResult};
use crate::lock::RwLockExt;
use crate::secrets::{PasswordSource, SecretStore, resolve_password};
use crate::state::AppState;

pub struct Session {
    pub id: String,
    pub spec: ConnectionSpec,
    pub db: Box<dyn Database>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub spec: ConnectionSpec,
    pub identity: String,
    /// The Keychain account, which also scopes favorites.
    pub account: String,
    pub display_name: String,
    pub server_version: String,
    pub database: Option<String>,
}

impl SessionInfo {
    pub fn new(
        id: &str,
        spec: &ConnectionSpec,
        server_version: &str,
        database: Option<String>,
    ) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            spec: spec.clone(),
            identity: spec.identity(),
            account: spec.account(),
            display_name: spec.display_name(),
            server_version: server_version.to_string(),
            database,
        }
    }
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo::new(
            &self.id,
            &self.spec,
            self.db.server_version(),
            self.db.current_database(),
        )
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpenArgs {
    pub spec: ConnectionSpec,
    #[serde(default)]
    pub password: Option<Secret>,
    /// A CLI launch request whose password should be used.
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub save_password: bool,
}

/// The error the UI sees when the server rejects the login. `password_required` and
/// `auth_failed` both make the UI prompt; `auth_failed` also shows the message, so a
/// rejected Keychain password or a failed Keychain lookup is explained in the prompt.
fn auth_error(source: &PasswordSource, account: String, message: String) -> AppError {
    let message = match source {
        PasswordSource::None => return AppError::PasswordRequired(account),
        PasswordSource::Unavailable(reason) => format!("No password was sent ({reason})"),
        PasswordSource::Stored(_) => {
            format!("The password stored in the Keychain was rejected ({message})")
        }
        PasswordSource::Provided(_) => message,
    };
    AppError::LoginFailed { account, message }
}

/// Connects with the best password at hand and registers the session.
pub async fn open(state: &AppState, args: OpenArgs) -> AppResult<Arc<Session>> {
    let identity = args.spec.identity();
    log::info!("Connecting to {identity}");
    let (db, source) = connect(state, &args).await.inspect_err(|e| match e {
        AppError::LoginFailed { .. } | AppError::PasswordRequired(_) => {
            log::warn!("Authentication failed for {identity}")
        }
        e => log::warn!("Connection to {identity} failed: {e}"),
    })?;
    if let (true, PasswordSource::Provided(secret)) = (args.save_password, &source) {
        save_password(state.secrets.clone(), args.spec.account(), secret.clone()).await;
    }
    if let Err(e) = state.recents.touch(&args.spec) {
        log::warn!("Could not update recent connections: {e}");
    }
    log::info!("Connected to {identity} ({})", db.server_version());
    Ok(state.sessions.insert(Session {
        id: uuid::Uuid::new_v4().to_string(),
        spec: args.spec,
        db,
    }))
}

/// Resolves the password, connects, and settles the launch password: a timeout or network
/// error keeps it for a retry, a rejected one is useless.
async fn connect(
    state: &AppState,
    args: &OpenArgs,
) -> AppResult<(Box<dyn Database>, PasswordSource)> {
    // Always looked up, so a launch password is not left behind when the page also sends one.
    let launch_password = args
        .request_id
        .as_deref()
        .and_then(|id| state.launches.peek_password(id));
    let explicit = args.password.clone().or(launch_password);
    let source = password_source(state.secrets.clone(), explicit, &args.spec).await;
    let settings = state.settings.get();
    let connecting = db::open(
        &args.spec,
        source.secret(),
        settings.connect_timeout(),
        settings.query_timeout(),
    )
    .await;
    let launch_password_spent = matches!(connecting, Ok(_) | Err(AppError::AuthFailed(_)));
    if launch_password_spent && let Some(id) = &args.request_id {
        state.launches.take_password(id);
    }
    match connecting {
        Ok(db) => Ok((db, source)),
        Err(AppError::AuthFailed(message)) => {
            Err(auth_error(&source, args.spec.account(), message))
        }
        Err(e) => Err(e),
    }
}

/// The Keychain may block on a permission dialog, so it is asked off the async workers.
async fn password_source(
    secrets: Arc<dyn SecretStore>,
    explicit: Option<Secret>,
    spec: &ConnectionSpec,
) -> PasswordSource {
    let spec = spec.clone();
    tokio::task::spawn_blocking(move || resolve_password(explicit, secrets.as_ref(), &spec))
        .await
        .unwrap_or_else(|e| PasswordSource::Unavailable(e.to_string()))
}

async fn save_password(secrets: Arc<dyn SecretStore>, account: String, secret: Secret) {
    let label = account.clone();
    let saved = tokio::task::spawn_blocking(move || secrets.set(&account, &secret))
        .await
        .map_err(|e| e.to_string())
        .and_then(|result| result.map_err(|e| e.to_string()));
    match saved {
        Ok(()) => log::info!("Saved password for {label} to Keychain"),
        Err(e) => log::warn!("Could not save password: {e}"),
    }
}

/// Removes the session and closes it, ending a running query.
pub async fn close(state: &AppState, session_id: &str) {
    state.invalidate_wp(session_id);
    if let Some(session) = state.sessions.remove(session_id) {
        session.db.close().await;
        log::info!("Disconnected from {}", session.spec.identity());
    }
}

#[derive(Default)]
pub struct Sessions(RwLock<HashMap<String, Arc<Session>>>);

impl Sessions {
    pub fn insert(&self, session: Session) -> Arc<Session> {
        let session = Arc::new(session);
        self.0
            .write_unpoisoned()
            .insert(session.id.clone(), session.clone());
        session
    }

    pub fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.0
            .read_unpoisoned()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.0.write_unpoisoned().remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::FakeDatabase;

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            spec: ConnectionSpec {
                name: None,
                host: "127.0.0.1".into(),
                port: 3306,
                socket: None,
                user: "root".into(),
                database: None,
            },
            db: Box::new(FakeDatabase::new().with_database("wp")),
        }
    }

    #[test]
    fn unknown_session_id_is_a_session_not_found_error() {
        let err = Sessions::default().get("missing").err().unwrap();

        assert_eq!(err.kind(), "session_not_found");
    }

    #[test]
    fn get_returns_the_inserted_session() {
        let sessions = Sessions::default();
        let inserted = sessions.insert(session("a"));

        let found = sessions.get("a").unwrap();

        assert!(Arc::ptr_eq(&inserted, &found));
    }

    #[test]
    fn remove_returns_the_session_once() {
        let sessions = Sessions::default();
        sessions.insert(session("a"));

        assert!(sessions.remove("a").is_some());
        assert!(sessions.remove("a").is_none());
        assert_eq!(sessions.get("a").err().unwrap().kind(), "session_not_found");
    }

    #[test]
    fn info_reports_server_version_and_current_database() {
        let info = session("a").info();

        assert_eq!(info.id, "a");
        assert_eq!(info.server_version, "fake");
        assert_eq!(info.database.as_deref(), Some("wp"));
    }

    fn socket_spec() -> ConnectionSpec {
        ConnectionSpec {
            name: Some("Local".into()),
            host: "localhost".into(),
            port: 3306,
            socket: Some("/tmp/mysql.sock".into()),
            user: "wp".into(),
            database: Some("shop".into()),
        }
    }

    #[test]
    fn session_info_derives_identity_account_and_display_name_from_the_spec() {
        let host_spec = session("a").spec;
        for spec in [host_spec, socket_spec()] {
            let info = SessionInfo::new("a", &spec, "8.4.0", Some("wp".into()));

            assert_eq!(info.id, "a");
            assert_eq!(info.identity, spec.identity());
            assert_eq!(info.account, spec.account());
            assert_eq!(info.display_name, spec.display_name());
            assert_eq!(info.server_version, "8.4.0");
            assert_eq!(info.database.as_deref(), Some("wp"));
            assert_eq!(info.spec, spec);
        }
    }

    #[test]
    fn session_info_serializes_camel_case_with_an_account_and_no_password() {
        let spec = socket_spec();

        let value = serde_json::to_value(SessionInfo::new("a", &spec, "8.4.0", None)).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "id": "a",
                "spec": serde_json::to_value(&spec).unwrap(),
                "identity": "wp@/tmp/mysql.sock/shop",
                "account": "wp@/tmp/mysql.sock",
                "displayName": "Local",
                "serverVersion": "8.4.0",
                "database": null,
            })
        );
        assert!(value.get("password").is_none());
    }

    #[tokio::test]
    async fn close_removes_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(crate::secrets::MemoryStore::default()));
        state.sessions.insert(session("a"));

        close(&state, "a").await;

        assert_eq!(
            state.sessions.get("a").err().unwrap().kind(),
            "session_not_found"
        );
    }

    #[tokio::test]
    async fn closing_an_unknown_session_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(crate::secrets::MemoryStore::default()));
        state.sessions.insert(session("a"));

        close(&state, "missing").await;

        assert!(state.sessions.get("a").is_ok());
    }

    #[test]
    fn missing_password_asks_for_one() {
        let err = auth_error(&PasswordSource::None, "wp@h:1".into(), "denied".into());
        assert_eq!(err.kind(), "password_required");
        assert_eq!(err.to_string(), "A password is required for wp@h:1");
    }

    #[test]
    fn keychain_failure_is_explained_in_the_message() {
        let source = PasswordSource::Unavailable("Keychain locked".into());
        let err = auth_error(&source, "a".into(), "denied".into());
        assert_eq!(err.kind(), "auth_failed");
        assert_eq!(err.to_string(), "No password was sent (Keychain locked)");
    }

    #[test]
    fn rejected_stored_password_mentions_the_keychain() {
        let source = PasswordSource::Stored(Secret::new("old"));
        let err = auth_error(&source, "a".into(), "Access denied".into());
        assert_eq!(err.kind(), "auth_failed");
        assert_eq!(
            err.to_string(),
            "The password stored in the Keychain was rejected (Access denied)"
        );
    }

    #[test]
    fn rejected_provided_password_passes_the_server_message_through() {
        let source = PasswordSource::Provided(Secret::new("typed"));
        let err = auth_error(&source, "a".into(), "Access denied".into());
        assert_eq!(err.kind(), "auth_failed");
        assert_eq!(err.to_string(), "Access denied");
        assert_eq!(err.account(), Some("a"));
    }

    #[test]
    fn missing_password_error_carries_the_account() {
        let err = auth_error(&PasswordSource::None, "wp@h:1".into(), "denied".into());
        assert_eq!(err.account(), Some("wp@h:1"));
    }
}
