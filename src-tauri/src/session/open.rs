use std::sync::Arc;

use serde::Deserialize;
use ts_rs::TS;

use super::Session;
use crate::connection::{ConnectionSpec, Secret};
use crate::db::{Connector, Database};
use crate::error::{AppError, AppResult};
use crate::secrets::{PasswordSource, SecretStore, resolve_password};
use crate::state::AppState;
use crate::store::settings::Settings;
use crate::store::slug::default_slug;

#[derive(Deserialize, Debug, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct OpenArgs {
    pub spec: ConnectionSpec,
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub password: Option<Secret>,
    /// A CLI launch request whose password should be used.
    #[serde(default)]
    #[ts(optional)]
    pub request_id: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
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
    // Without the store the session still gets its default slug.
    let slug = match state.recents.touch(&args.spec).await {
        Ok(slug) => slug,
        Err(e) => {
            log::warn!("Could not update recent connections: {e}");
            default_slug(&args.spec)
        }
    };
    log::info!("Connected to {identity} ({})", db.server_version());
    let id = uuid::Uuid::new_v4().to_string();
    Ok(state.sessions.insert(Session::new(id, args.spec, slug, db)))
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
    let connecting = state
        .connector
        .open(
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

/// Connects with the Keychain password (if any) without registering the session or touching the
/// recents store, for a process other than the app (`databee mcp`).
pub async fn open_detached(
    spec: ConnectionSpec,
    slug: String,
    secrets: Arc<dyn SecretStore>,
    connector: &dyn Connector,
    settings: &Settings,
) -> AppResult<Session> {
    let source = password_source(secrets, None, &spec).await;
    let db = connector
        .open(
            &spec,
            source.secret(),
            settings.connect_timeout(),
            settings.query_timeout(),
        )
        .await
        .map_err(|e| match e {
            AppError::AuthFailed(message) => auth_error(&source, spec.account(), message),
            e => e,
        })?;
    let id = uuid::Uuid::new_v4().to_string();
    Ok(Session::new(id, spec, slug, db))
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
    if let Some(session) = state.sessions.remove(session_id) {
        session.db.close().await;
        log::info!("Disconnected from {}", session.spec.identity());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use crate::db::FakeDatabase;
    use crate::error::ErrorKind;
    use crate::lock::MutexExt;
    use crate::session::testing::{local_spec, session};

    #[tokio::test]
    async fn close_removes_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(crate::secrets::MemoryStore::default()));
        state.sessions.insert(session("a"));

        close(&state, "a").await;

        assert_eq!(
            state.sessions.get("a").err().unwrap().kind(),
            ErrorKind::SessionNotFound
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

    struct LockedKeychain;

    impl SecretStore for LockedKeychain {
        fn get(&self, _: &str) -> AppResult<Option<Secret>> {
            Err(AppError::Keychain("locked".into()))
        }
        fn set(&self, _: &str, _: &Secret) -> AppResult<()> {
            Err(AppError::Keychain("locked".into()))
        }
    }

    #[tokio::test]
    async fn failing_secret_store_makes_the_password_source_unavailable() {
        let spec = ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "wp".into(),
            database: None,
        };

        let source = password_source(Arc::new(LockedKeychain), None, &spec).await;

        assert!(
            matches!(source, PasswordSource::Unavailable(_)),
            "{source:?}"
        );
    }

    #[test]
    fn missing_password_asks_for_one() {
        let err = auth_error(&PasswordSource::None, "wp@h:1".into(), "denied".into());
        assert_eq!(err.kind(), ErrorKind::PasswordRequired);
        assert_eq!(err.to_string(), "A password is required for wp@h:1");
    }

    #[test]
    fn keychain_failure_is_explained_in_the_message() {
        let source = PasswordSource::Unavailable("Keychain locked".into());
        let err = auth_error(&source, "a".into(), "denied".into());
        assert_eq!(err.kind(), ErrorKind::AuthFailed);
        assert_eq!(err.to_string(), "No password was sent (Keychain locked)");
    }

    #[test]
    fn rejected_stored_password_mentions_the_keychain() {
        let source = PasswordSource::Stored(Secret::new("old"));
        let err = auth_error(&source, "a".into(), "Access denied".into());
        assert_eq!(err.kind(), ErrorKind::AuthFailed);
        assert_eq!(
            err.to_string(),
            "The password stored in the Keychain was rejected (Access denied)"
        );
    }

    #[test]
    fn rejected_provided_password_passes_the_server_message_through() {
        let source = PasswordSource::Provided(Secret::new("typed"));
        let err = auth_error(&source, "a".into(), "Access denied".into());
        assert_eq!(err.kind(), ErrorKind::AuthFailed);
        assert_eq!(err.to_string(), "Access denied");
        assert_eq!(err.account(), Some("a"));
    }

    #[test]
    fn missing_password_error_carries_the_account() {
        let err = auth_error(&PasswordSource::None, "wp@h:1".into(), "denied".into());
        assert_eq!(err.account(), Some("wp@h:1"));
    }
    #[derive(Default)]
    struct FakeConnector {
        failure: Mutex<Option<AppError>>,
        passwords: Mutex<Vec<Option<String>>>,
    }

    impl FakeConnector {
        fn failing(error: AppError) -> Arc<Self> {
            Arc::new(Self {
                failure: Mutex::new(Some(error)),
                ..Self::default()
            })
        }

        fn passwords(&self) -> Vec<Option<String>> {
            self.passwords.lock_unpoisoned().clone()
        }
    }

    #[async_trait::async_trait]
    impl Connector for FakeConnector {
        async fn open(
            &self,
            _: &ConnectionSpec,
            password: Option<&Secret>,
            _: Duration,
            _: Duration,
        ) -> AppResult<Box<dyn Database>> {
            self.passwords
                .lock_unpoisoned()
                .push(password.map(|p| p.expose().to_string()));
            match self.failure.lock_unpoisoned().take() {
                Some(error) => Err(error),
                None => Ok(Box::new(FakeDatabase::new())),
            }
        }
    }

    struct Fixture {
        dir: tempfile::TempDir,
        state: AppState,
        secrets: Arc<crate::secrets::MemoryStore>,
        connector: Arc<FakeConnector>,
    }

    fn fixture(connector: Arc<FakeConnector>) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(crate::secrets::MemoryStore::default());
        let mut state = AppState::open(dir.path(), secrets.clone());
        state.connector = connector.clone();
        Fixture {
            dir,
            state,
            secrets,
            connector,
        }
    }

    fn args(password: Option<&str>, request_id: Option<&str>, save_password: bool) -> OpenArgs {
        OpenArgs {
            spec: local_spec(),
            password: password.map(Secret::new),
            request_id: request_id.map(str::to_string),
            save_password,
        }
    }

    fn queue_launch_password(state: &AppState, password: &str) -> String {
        state.launches.push(crate::ipc::IpcMessage::Launch {
            spec: local_spec(),
            password: Some(Secret::new(password)),
            save_password: false,
        });
        match state.launches.take_events().pop() {
            Some(crate::launch::LaunchEvent::Connect { request_id, .. }) => request_id,
            other => panic!("expected a connect event, got {other:?}"),
        }
    }

    fn stored(secrets: &crate::secrets::MemoryStore) -> Option<String> {
        secrets
            .get(&local_spec().account())
            .unwrap()
            .map(|s| s.expose().to_string())
    }

    #[tokio::test]
    async fn the_launch_password_is_sent_and_spent_once_connected() {
        let f = fixture(Arc::default());
        let request = queue_launch_password(&f.state, "launch");

        open(&f.state, args(None, Some(&request), false))
            .await
            .unwrap();

        assert_eq!(f.connector.passwords(), [Some("launch".to_string())]);
        assert!(f.state.launches.peek_password(&request).is_none());
    }

    #[tokio::test]
    async fn a_connection_error_keeps_the_launch_password_for_a_retry() {
        let f = fixture(FakeConnector::failing(AppError::Connection(
            "refused".into(),
        )));
        let request = queue_launch_password(&f.state, "launch");

        let err = open(&f.state, args(None, Some(&request), false))
            .await
            .err()
            .unwrap();

        assert_eq!(err.kind(), ErrorKind::Connection);
        assert!(f.state.launches.peek_password(&request).is_some());
    }

    #[tokio::test]
    async fn a_rejected_launch_password_is_spent_and_reported_with_the_server_message() {
        let f = fixture(FakeConnector::failing(AppError::AuthFailed(
            "Access denied".into(),
        )));
        let request = queue_launch_password(&f.state, "launch");

        let err = open(&f.state, args(None, Some(&request), false))
            .await
            .err()
            .unwrap();

        assert_eq!(err.kind(), ErrorKind::AuthFailed);
        assert_eq!(err.to_string(), "Access denied");
        assert!(f.state.launches.peek_password(&request).is_none());
    }

    #[tokio::test]
    async fn a_password_from_the_page_wins_and_the_launch_password_is_still_spent() {
        let f = fixture(Arc::default());
        let request = queue_launch_password(&f.state, "launch");

        open(&f.state, args(Some("typed"), Some(&request), false))
            .await
            .unwrap();

        assert_eq!(f.connector.passwords(), [Some("typed".to_string())]);
        assert!(f.state.launches.peek_password(&request).is_none());
    }

    #[tokio::test]
    async fn without_a_given_password_the_keychain_password_is_sent() {
        let f = fixture(Arc::default());
        f.secrets
            .set(&local_spec().account(), &Secret::new("stored"))
            .unwrap();

        open(&f.state, args(None, None, false)).await.unwrap();

        assert_eq!(f.connector.passwords(), [Some("stored".to_string())]);
    }

    #[tokio::test]
    async fn a_provided_password_is_saved_to_the_keychain_only_when_asked_and_accepted() {
        let f = fixture(Arc::default());
        open(&f.state, args(Some("typed"), None, false))
            .await
            .unwrap();
        assert_eq!(stored(&f.secrets), None);

        open(&f.state, args(Some("typed"), None, true))
            .await
            .unwrap();
        assert_eq!(stored(&f.secrets).as_deref(), Some("typed"));
    }

    #[tokio::test]
    async fn a_rejected_password_is_not_saved_to_the_keychain() {
        let f = fixture(FakeConnector::failing(AppError::AuthFailed(
            "denied".into(),
        )));

        open(&f.state, args(Some("typed"), None, true))
            .await
            .err()
            .unwrap();

        assert_eq!(stored(&f.secrets), None);
    }

    #[tokio::test]
    async fn without_a_password_a_rejected_login_asks_for_one() {
        let f = fixture(FakeConnector::failing(AppError::AuthFailed(
            "denied".into(),
        )));

        let err = open(&f.state, args(None, None, false)).await.err().unwrap();

        assert_eq!(err.kind(), ErrorKind::PasswordRequired);
        assert_eq!(f.connector.passwords(), [None]);
    }

    #[tokio::test]
    async fn an_opened_session_is_registered_under_the_saved_connections_slug() {
        let f = fixture(Arc::default());
        f.state.recents.touch(&local_spec()).await.unwrap();
        f.state
            .recents
            .rename_slug(&local_spec().identity(), "mine")
            .await
            .unwrap();

        let session = open(&f.state, args(None, None, false)).await.unwrap();

        assert_eq!(session.slug(), "mine");
        assert!(Arc::ptr_eq(
            &f.state.sessions.get(&session.id).unwrap(),
            &session
        ));
    }

    #[tokio::test]
    async fn when_the_saved_connections_cannot_be_written_the_session_gets_the_default_slug() {
        use std::os::unix::fs::PermissionsExt;

        let f = fixture(Arc::default());
        f.state.recents.touch(&local_spec()).await.unwrap();
        f.state
            .recents
            .rename_slug(&local_spec().identity(), "mine")
            .await
            .unwrap();
        let mode = |m| std::fs::Permissions::from_mode(m);
        std::fs::set_permissions(f.dir.path(), mode(0o500)).unwrap();

        let opened = open(&f.state, args(None, None, false)).await;

        std::fs::set_permissions(f.dir.path(), mode(0o700)).unwrap();
        let session = opened.unwrap();
        assert_eq!(session.slug(), default_slug(&local_spec()));
        assert!(f.state.sessions.get(&session.id).is_ok());
    }

    #[tokio::test]
    async fn a_detached_session_connects_with_the_keychain_password_and_the_given_slug() {
        let connector = FakeConnector::default();
        let secrets = crate::secrets::MemoryStore::default();
        secrets
            .set(&local_spec().account(), &Secret::new("stored"))
            .unwrap();

        let session = open_detached(
            local_spec(),
            "given".into(),
            Arc::new(secrets),
            &connector,
            &Settings::default(),
        )
        .await
        .unwrap();

        assert_eq!(session.slug(), "given");
        assert_eq!(connector.passwords(), [Some("stored".to_string())]);
    }

    #[tokio::test]
    async fn a_detached_session_whose_keychain_password_is_rejected_says_so() {
        let connector = FakeConnector {
            failure: Mutex::new(Some(AppError::AuthFailed("Access denied".into()))),
            ..FakeConnector::default()
        };
        let secrets = crate::secrets::MemoryStore::default();
        secrets
            .set(&local_spec().account(), &Secret::new("old"))
            .unwrap();

        let err = open_detached(
            local_spec(),
            "given".into(),
            Arc::new(secrets),
            &connector,
            &Settings::default(),
        )
        .await
        .err()
        .unwrap();

        assert_eq!(err.kind(), ErrorKind::AuthFailed);
        assert_eq!(
            err.to_string(),
            "The password stored in the Keychain was rejected (Access denied)"
        );
    }
}
