use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use ts_rs::TS;

use crate::connection::{ConnectionSpec, Secret};
use crate::db::cell_cache::CellCache;
use crate::db::classify::changes_schema;
use crate::db::plans::{AppliedBatchInfo, BatchId, PlanId, Plans};
use crate::db::{BrowsePage, BrowseRequest, CellLimit, Connector, Database, ExecuteResponse};
use crate::error::{AppError, AppResult};
use crate::health::{Guarded, HealthEvent, SessionHealth};
use crate::lock::RwLockExt;
use crate::rules::annotate;
use crate::rules::effective::{self, RuleSources};
use crate::schema;
use crate::secrets::{PasswordSource, SecretStore, resolve_password};
use crate::state::AppState;
use crate::store::settings::Settings;
use crate::store::slug::default_slug;

pub struct Session {
    pub id: String,
    pub spec: ConnectionSpec,
    /// Behind the session's health guard, which refuses calls while the server is gone.
    pub db: Box<dyn Database>,
    pub health: Arc<SessionHealth>,
    /// Per-session data of modules, freed with the session (ADR 0013).
    pub ext: Extensions,
    /// Writes planned for review, and the undo history (ADR 0012).
    pub plans: Plans,
    /// Full values of the long text cells sent as previews, for `fetch_cell`.
    pub cells: CellCache,
    /// User scripts with DDL; the schema cache reloads when it changes. Session-wide, because a
    /// statement may name any database.
    ddl_writes: AtomicU64,
    /// The connection slug addresses start with; renaming the saved connection updates it.
    slug: RwLock<String>,
}

/// A map from type to one shared value of that type, so a module can attach its own data to a
/// session without the core naming the module.
#[derive(Default)]
pub struct Extensions(RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>);

impl Extensions {
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        let value = self.0.read_unpoisoned().get(&TypeId::of::<T>())?.clone();
        value.downcast().ok()
    }

    /// The value of type `T`, created with `init` on first use.
    pub fn get_or_insert_with<T: Any + Send + Sync>(&self, init: impl FnOnce() -> T) -> Arc<T> {
        if let Some(value) = self.get() {
            return value;
        }
        let mut map = self.0.write_unpoisoned();
        let value = map
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(init()))
            .clone();
        value
            .downcast()
            .expect("the entry for a TypeId holds that type")
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub spec: ConnectionSpec,
    pub identity: String,
    /// The Keychain account, which also scopes history.
    pub account: String,
    pub display_name: String,
    pub server_version: String,
    pub database: Option<String>,
    /// The connection slug (`databee://<slug>/…`).
    pub slug: String,
}

impl SessionInfo {
    pub fn new(
        id: &str,
        spec: &ConnectionSpec,
        server_version: &str,
        database: Option<String>,
        slug: String,
    ) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            spec: spec.clone(),
            identity: spec.identity(),
            account: spec.account(),
            display_name: spec.display_name(),
            server_version: server_version.to_string(),
            database,
            slug,
        }
    }
}

impl Session {
    pub fn new(id: String, spec: ConnectionSpec, slug: String, db: Box<dyn Database>) -> Self {
        let health = Arc::new(SessionHealth::new(&id));
        Self {
            db: Box::new(Guarded::new(db, health.clone())),
            health,
            id,
            slug: RwLock::new(slug),
            spec,
            ext: Extensions::default(),
            plans: Plans::default(),
            cells: CellCache::default(),
            ddl_writes: AtomicU64::new(0),
        }
    }

    pub fn slug(&self) -> String {
        self.slug.read_unpoisoned().clone()
    }

    pub fn set_slug(&self, slug: &str) {
        *self.slug.write_unpoisoned() = slug.to_string();
    }

    /// How many user scripts with DDL ran in this session.
    pub fn ddl_writes(&self) -> u64 {
        self.ddl_writes.load(Ordering::SeqCst)
    }

    /// Runs a user query; the results get their column kinds (with `rules`, also the rules'
    /// kinds and styles), and long text cells are kept for `fetch_cell`.
    pub async fn execute(
        &self,
        sql: &str,
        row_limit: usize,
        cell_limit: CellLimit,
        timeout: Duration,
        rules: Option<RuleSources<'_>>,
    ) -> AppResult<ExecuteResponse> {
        let response = self.db.execute(sql, row_limit, cell_limit, timeout).await;
        // Counted after the run, also a failed one (a script may stop after its DDL), so a
        // schema read that overlapped the DDL is not kept under the new count.
        if changes_schema(sql) {
            self.ddl_writes.fetch_add(1, Ordering::SeqCst);
        }
        let mut response = response?;
        annotate::annotate_execute(self, &mut response, rules).await;
        for result in &mut response.results {
            self.cells.keep(result);
        }
        Ok(response)
    }

    /// Reads a page of a table with its column kinds and row keys (with `rules`, also the rules'
    /// kinds and styles); its long text cells are kept for `fetch_cell`.
    pub async fn browse(
        &self,
        request: &BrowseRequest,
        cell_limit: CellLimit,
        timeout: Duration,
        rules: Option<RuleSources<'_>>,
    ) -> AppResult<BrowsePage> {
        // The schema is cached after the first page, so loading it alongside costs nothing later.
        let (page, schema) = tokio::join!(
            self.db.browse(request, cell_limit, timeout),
            schema::database(self, &request.database)
        );
        let mut page = page?;
        let effective = match (&schema, rules) {
            (Ok(schema), Some(sources)) => Some(effective::effective(
                self,
                &request.database,
                schema,
                sources,
            )),
            _ => None,
        };
        annotate::annotate_browse(self, &mut page, &request.database, schema, effective);
        self.cells.keep(&mut page.result);
        Ok(page)
    }

    /// Applies pending plans in one transaction. On failure the plans stay pending for a retry.
    pub async fn apply_plans(
        &self,
        ids: &[PlanId],
        timeout: Duration,
    ) -> AppResult<AppliedBatchInfo> {
        let plans = self.plans.take(ids)?;
        let statements: Vec<String> = plans
            .iter()
            .flat_map(|(_, plan)| plan.statements.iter().cloned())
            .collect();
        log::info!(
            "Applying {} statements on {}",
            statements.len(),
            self.spec.identity()
        );
        for sql in &statements {
            log::trace!("SQL: {sql}");
        }
        let affected = match self.db.apply(&statements, timeout).await {
            Ok(affected) => affected,
            Err(e) => {
                self.plans.restore(plans);
                return Err(e);
            }
        };
        let batch = self.plans.record_applied(&plans);
        Ok(AppliedBatchInfo {
            batch_id: batch.id,
            affected,
            can_undo: batch.can_undo,
        })
    }

    /// Runs a batch's undo statements; on failure the batch stays undoable for a retry.
    pub async fn undo_batch(&self, batch_id: BatchId, timeout: Duration) -> AppResult<u64> {
        let batch = self.plans.take_undo(batch_id)?;
        log::info!(
            "Undoing batch #{batch_id} ({} statements) on {}",
            batch.undo.len(),
            self.spec.identity()
        );
        match self.db.apply(&batch.undo, timeout).await {
            Ok(affected) => Ok(affected),
            Err(e) => {
                self.plans.restore_undo(batch);
                Err(e)
            }
        }
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo::new(
            &self.id,
            &self.spec,
            self.db.server_version(),
            self.db.current_database(),
            self.slug(),
        )
    }
}

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

/// Health changes queued for the page; a longer burst makes the receiver resend every state.
const HEALTH_EVENTS: usize = 64;

pub struct Sessions {
    open: RwLock<HashMap<String, Arc<Session>>>,
    /// The health changes of every registered session (`health::watch` sends them to the page).
    health: broadcast::Sender<HealthEvent>,
}

impl Default for Sessions {
    fn default() -> Self {
        Self {
            open: RwLock::default(),
            health: broadcast::channel(HEALTH_EVENTS).0,
        }
    }
}

impl Sessions {
    /// Registers the session; from now on its health changes are reported.
    pub fn insert(&self, session: Session) -> Arc<Session> {
        session.health.attach(self.health.clone());
        let session = Arc::new(session);
        self.open
            .write_unpoisoned()
            .insert(session.id.clone(), session.clone());
        session
    }

    pub fn get(&self, id: &str) -> AppResult<Arc<Session>> {
        self.open
            .read_unpoisoned()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    pub fn all(&self) -> Vec<Arc<Session>> {
        self.open.read_unpoisoned().values().cloned().collect()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.open.write_unpoisoned().remove(id)
    }

    pub fn health_events(&self) -> broadcast::Receiver<HealthEvent> {
        self.health.subscribe()
    }

    pub fn has_slug(&self, slug: &str) -> bool {
        self.open
            .read_unpoisoned()
            .values()
            .any(|s| s.slug() == slug)
    }

    /// Gives every open session of the connection `identity` the renamed slug.
    pub fn rename_slug(&self, identity: &str, slug: &str) {
        for session in self.open.read_unpoisoned().values() {
            if session.spec.identity() == identity {
                session.set_slug(slug);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::db::changes::Plan;
    use crate::db::{FakeDatabase, ResultSet};
    use crate::error::ErrorKind;
    use crate::lock::MutexExt;

    fn session(id: &str) -> Session {
        Session::new(
            id.into(),
            local_spec(),
            "local".into(),
            Box::new(FakeDatabase::new().with_database("wp")),
        )
    }

    fn local_spec() -> ConnectionSpec {
        ConnectionSpec {
            name: None,
            host: "127.0.0.1".into(),
            port: 3306,
            socket: None,
            user: "root".into(),
            database: None,
        }
    }

    const TIMEOUT: Duration = Duration::from_secs(1);

    fn plan(name: &str, undo: &[&str]) -> Plan {
        Plan {
            description: format!("change {name}"),
            statements: vec![format!("DO {name}")],
            undo: undo.iter().map(ToString::to_string).collect(),
        }
    }

    fn session_over(db: &Arc<FakeDatabase>) -> Session {
        Session::new(
            "a".into(),
            local_spec(),
            "local".into(),
            Box::new(db.clone()),
        )
    }

    #[tokio::test]
    async fn applying_plans_sends_their_statements_in_the_given_order_in_one_batch() {
        let db = Arc::new(FakeDatabase::new());
        let session = session_over(&db);
        let first = session.plans.add(plan("first", &["UNDO first"]));
        let second = session.plans.add(plan("second", &["UNDO second"]));

        let info = session
            .apply_plans(&[second.id, first.id], TIMEOUT)
            .await
            .unwrap();

        assert_eq!(db.applied(), [["DO second", "DO first"]]);
        assert_eq!(info.affected, 2);
        assert!(info.can_undo);
    }

    #[tokio::test]
    async fn applied_batch_is_undone_with_the_last_plans_undo_first() {
        let db = Arc::new(FakeDatabase::new());
        let session = session_over(&db);
        let first = session.plans.add(plan("first", &["UNDO first"]));
        let second = session.plans.add(plan("second", &["UNDO second"]));
        let info = session
            .apply_plans(&[first.id, second.id], TIMEOUT)
            .await
            .unwrap();

        let affected = session.undo_batch(info.batch_id, TIMEOUT).await.unwrap();

        assert_eq!(affected, 2);
        assert_eq!(db.applied()[1], ["UNDO second", "UNDO first"]);
        assert_eq!(
            session
                .undo_batch(info.batch_id, TIMEOUT)
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn applying_an_unknown_plan_fails_and_leaves_the_known_plans_pending() {
        let db = Arc::new(FakeDatabase::new());
        let session = session_over(&db);
        let known = session.plans.add(plan("a", &["UNDO a"]));

        let err = session
            .apply_plans(&[known.id, known.id + 100], TIMEOUT)
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(db.applied().is_empty());
        assert!(session.apply_plans(&[known.id], TIMEOUT).await.is_ok());
    }

    #[tokio::test]
    async fn applying_a_plan_without_undo_reports_that_it_cannot_be_undone() {
        let db = Arc::new(FakeDatabase::new());
        let session = session_over(&db);
        let a = session.plans.add(plan("a", &[]));

        let info = session.apply_plans(&[a.id], TIMEOUT).await.unwrap();

        assert!(!info.can_undo);
        assert_eq!(
            session
                .undo_batch(info.batch_id, TIMEOUT)
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput
        );
        assert_eq!(db.applied().len(), 1);
    }

    #[tokio::test]
    async fn applied_batch_info_serializes_camel_case() {
        let db = Arc::new(FakeDatabase::new());
        let session = session_over(&db);
        let a = session.plans.add(plan("a", &["UNDO a"]));

        let info = session.apply_plans(&[a.id], TIMEOUT).await.unwrap();

        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({"batchId": info.batch_id, "affected": 1, "canUndo": true})
        );
    }

    #[tokio::test]
    #[ignore]
    async fn planned_row_edit_is_applied_by_id_and_undone_by_batch_on_the_fixture() {
        use crate::db::changes::{ChangeRequest, plan};
        use crate::db::mysql::MySqlSession;

        for (host, port) in [("127.0.0.1", 33306), ("127.0.0.1", 33307)] {
            let spec = ConnectionSpec {
                port,
                database: Some("wp_write".into()),
                ..local_spec()
            };
            let db = MySqlSession::connect(&spec, Some(&Secret::new("root")), TIMEOUT, TIMEOUT)
                .await
                .unwrap();
            let session = Session::new("it".into(), spec, "it".into(), Box::new(db));
            let read = |sql: &'static str| {
                let session = &session;
                async move {
                    session.db.fetch(sql, TIMEOUT).await.unwrap().rows[0][0]
                        .as_text()
                        .map(str::to_string)
                }
            };
            let sql = "SELECT post_name FROM wp_write.wp_posts WHERE ID = 201";
            let before = read(sql).await;
            let request = ChangeRequest::SetCell {
                database: "wp_write".into(),
                table: "wp_posts".into(),
                key_column: "ID".into(),
                key: "201".into(),
                column: "post_name".into(),
                value: Some("applied-by-id".into()),
            };
            let planned = plan(session.db.as_ref(), &request, TIMEOUT).await.unwrap();
            let change = session.plans.add(planned);
            assert_eq!(
                read(sql).await,
                before,
                "{host}:{port}: planning writes nothing"
            );

            let info = session.apply_plans(&[change.id], TIMEOUT).await.unwrap();
            assert_eq!(
                read(sql).await.as_deref(),
                Some("applied-by-id"),
                "{host}:{port}"
            );

            session.undo_batch(info.batch_id, TIMEOUT).await.unwrap();
            assert_eq!(
                read(sql).await,
                before,
                "{host}:{port}: undo restores the row"
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn applying_an_unknown_plan_id_writes_nothing_on_the_fixture() {
        use crate::db::mysql::MySqlSession;

        let spec = ConnectionSpec {
            port: 33306,
            database: Some("wp_write".into()),
            ..local_spec()
        };
        let db = MySqlSession::connect(&spec, Some(&Secret::new("root")), TIMEOUT, TIMEOUT)
            .await
            .unwrap();
        let session = Session::new("it".into(), spec, "it".into(), Box::new(db));

        let err = session.apply_plans(&[999], TIMEOUT).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn unknown_session_id_is_a_session_not_found_error() {
        let err = Sessions::default().get("missing").err().unwrap();

        assert_eq!(err.kind(), ErrorKind::SessionNotFound);
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
        assert_eq!(
            sessions.get("a").err().unwrap().kind(),
            ErrorKind::SessionNotFound
        );
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
            let info = SessionInfo::new("a", &spec, "8.4.0", Some("wp".into()), "wp".into());

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

        let value =
            serde_json::to_value(SessionInfo::new("a", &spec, "8.4.0", None, "local".into()))
                .unwrap();

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
                "slug": "local",
            })
        );
        assert!(value.get("password").is_none());
    }

    #[test]
    fn info_slug_is_the_given_slug_and_follows_set_slug() {
        let session = session("a");
        assert_eq!(session.info().slug, "local");

        session.set_slug("mine");

        assert_eq!(session.slug(), "mine");
        assert_eq!(session.info().slug, "mine");
    }

    #[test]
    fn renaming_a_slug_changes_only_the_sessions_of_that_connection() {
        let sessions = Sessions::default();
        let first = sessions.insert(session("a"));
        let second = sessions.insert(session("b"));
        let other = sessions.insert(Session::new(
            "c".into(),
            socket_spec(),
            "socket".into(),
            Box::new(FakeDatabase::new()),
        ));

        sessions.rename_slug(&local_spec().identity(), "renamed");

        assert_eq!(first.slug(), "renamed");
        assert_eq!(second.slug(), "renamed");
        assert_eq!(other.slug(), "socket");
    }

    #[test]
    fn has_slug_is_true_only_for_the_slug_of_an_open_session() {
        let sessions = Sessions::default();
        assert!(!sessions.has_slug("local"));

        sessions.insert(session("a"));

        assert!(sessions.has_slug("local"));
        assert!(!sessions.has_slug("other"));
    }

    #[test]
    fn has_slug_follows_a_renamed_slug() {
        let sessions = Sessions::default();
        sessions.insert(session("a"));

        sessions.rename_slug(&local_spec().identity(), "renamed");

        assert!(sessions.has_slug("renamed"));
        assert!(!sessions.has_slug("local"));
    }

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

    #[test]
    fn get_or_insert_with_runs_init_once_and_returns_the_same_value() {
        let ext = Extensions::default();
        let mut calls = 0;

        let first = ext.get_or_insert_with(|| {
            calls += 1;
            String::from("a")
        });
        let second = ext.get_or_insert_with(|| {
            calls += 1;
            String::from("b")
        });

        assert_eq!(calls, 1);
        assert_eq!(*second, "a");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&first, &ext.get::<String>().unwrap()));
    }

    #[test]
    fn extensions_of_different_types_are_stored_independently() {
        let ext = Extensions::default();

        ext.get_or_insert_with(|| 7u32);
        ext.get_or_insert_with(|| String::from("text"));

        assert_eq!(*ext.get::<u32>().unwrap(), 7);
        assert_eq!(*ext.get::<String>().unwrap(), "text");
    }

    #[test]
    fn get_of_an_absent_type_is_none() {
        let ext = Extensions::default();
        ext.get_or_insert_with(|| 7u32);

        assert!(ext.get::<String>().is_none());
    }

    #[test]
    fn extensions_are_gone_when_the_session_is_removed_and_reopened() {
        struct Marker(u32);

        let sessions = Sessions::default();
        let first = sessions.insert(session("a"));
        first.ext.get_or_insert_with(|| Marker(7));
        assert_eq!(first.ext.get::<Marker>().unwrap().0, 7);

        sessions.remove("a");
        let reopened = sessions.insert(session("b"));

        assert!(reopened.ext.get::<Marker>().is_none());
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

    async fn run(session: &Session, sql: &str) -> AppResult<ExecuteResponse> {
        session
            .execute(sql, 10, CellLimit::NONE, TIMEOUT, None)
            .await
    }

    #[tokio::test]
    async fn scripts_with_ddl_count_as_ddl_writes() {
        let session = Session::new(
            "a".into(),
            local_spec(),
            "local".into(),
            Box::new(FakeDatabase::new().on("", ResultSet::default())),
        );
        assert_eq!(session.ddl_writes(), 0);

        for (i, sql) in [
            "CREATE TABLE t (id int)",
            "ALTER TABLE t ADD COLUMN x int",
            "DROP TABLE t",
            "RENAME TABLE a TO b",
            "TRUNCATE TABLE t",
        ]
        .into_iter()
        .enumerate()
        {
            run(&session, sql).await.unwrap();
            assert_eq!(session.ddl_writes(), i as u64 + 1, "{sql}");
        }
    }

    #[tokio::test]
    async fn failed_ddl_script_still_counts_as_a_ddl_write() {
        let session = Session::new(
            "a".into(),
            local_spec(),
            "local".into(),
            Box::new(FakeDatabase::new().failing("ALTER", AppError::Cancelled)),
        );

        run(&session, "ALTER TABLE t ADD COLUMN x int")
            .await
            .unwrap_err();

        assert_eq!(session.ddl_writes(), 1);
    }

    #[tokio::test]
    async fn scripts_without_ddl_do_not_count_as_ddl_writes() {
        let session = Session::new(
            "a".into(),
            local_spec(),
            "local".into(),
            Box::new(FakeDatabase::new().on("", ResultSet::default())),
        );

        run(&session, "SELECT 1").await.unwrap();
        run(&session, "UPDATE t SET a = 1 WHERE id = 2")
            .await
            .unwrap();

        assert_eq!(session.ddl_writes(), 0);
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
