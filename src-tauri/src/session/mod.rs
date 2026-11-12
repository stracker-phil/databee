mod extensions;
mod open;
mod registry;
#[cfg(test)]
mod testing;

pub use extensions::Extensions;
pub use open::{OpenArgs, close, open, open_detached};
pub use registry::Sessions;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use crate::connection::ConnectionSpec;
use crate::db::cell_cache::CellCache;
use crate::db::classify::changes_schema;
use crate::db::plans::{AppliedBatchInfo, BatchId, PlanId, Plans};
use crate::db::{BrowsePage, BrowseRequest, CellLimit, Database, ExecuteResponse, browse};
use crate::error::AppResult;
use crate::health::{Guarded, SessionHealth};
use crate::lock::RwLockExt;
use crate::rules::annotate;
use crate::rules::effective::{self, RuleSources};
use crate::schema;

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
        let (page, schema) = if browse::text_search(request).is_some() {
            let schema = schema::database(self, &request.database).await;
            let columns = schema
                .as_ref()
                .ok()
                .and_then(|s| s.columns.get(&request.table));
            let request = BrowseRequest {
                columns: columns.cloned().unwrap_or_default(),
                ..request.clone()
            };
            (self.db.browse(&request, cell_limit, timeout).await, schema)
        } else {
            // The schema is cached after the first page, so loading it alongside costs nothing later.
            tokio::join!(
                self.db.browse(request, cell_limit, timeout),
                schema::database(self, &request.database)
            )
        };
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

#[cfg(test)]
mod tests {
    use super::testing::{local_spec, session, socket_spec};
    use super::*;
    use crate::connection::Secret;
    use crate::db::changes::Plan;
    use crate::db::{FakeDatabase, ResultSet};
    use crate::error::{AppError, ErrorKind};

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

    fn posts_table() -> crate::db::TableStructure {
        let column = |name: &str| crate::db::ColumnInfo {
            name: name.into(),
            column_type: "text".into(),
            nullable: true,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        };
        crate::db::TableStructure {
            columns: vec![column("ID"), column("post_title")],
            indexes: Vec::new(),
            primary_key: vec!["ID".into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    fn browse_request(
        table: &str,
        filter: &str,
        filter_mode: crate::db::FilterMode,
    ) -> BrowseRequest {
        BrowseRequest {
            database: "wp".into(),
            table: table.into(),
            limit: 100,
            filter: Some(filter.into()),
            filter_mode,
            ..BrowseRequest::default()
        }
    }

    #[tokio::test]
    async fn smart_text_filter_searches_the_schema_columns_of_the_browsed_table() {
        use crate::db::{FilterKind, FilterMode};

        let db = Arc::new(
            FakeDatabase::new()
                .with_database("wp")
                .with_table("wp", "wp_posts", posts_table())
                .on("LIKE", ResultSet::default()),
        );
        let session = session_over(&db);

        let page = session
            .browse(
                &browse_request("wp_posts", "home_", FilterMode::Smart),
                CellLimit::NONE,
                TIMEOUT,
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            db.queries(),
            [
                "SELECT * FROM `wp`.`wp_posts` WHERE (`ID` LIKE '%home\\\\_%' OR `post_title` LIKE '%home\\\\_%'\n) LIMIT 101 OFFSET 0"
            ]
        );
        assert_eq!(page.filter.unwrap().kind, FilterKind::Text);
    }

    #[tokio::test]
    async fn smart_text_filter_on_a_table_without_known_columns_is_invalid_input() {
        use crate::db::FilterMode;

        let db = Arc::new(FakeDatabase::new().with_database("wp"));
        let session = session_over(&db);

        let err = session
            .browse(
                &browse_request("missing", "home", FilterMode::Smart),
                CellLimit::NONE,
                TIMEOUT,
                None,
            )
            .await
            .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(db.queries().is_empty());
    }

    #[tokio::test]
    async fn sql_filter_is_sent_as_typed_without_the_schema_columns() {
        use crate::db::{FilterKind, FilterMode};

        let db = Arc::new(
            FakeDatabase::new()
                .with_database("wp")
                .with_table("wp", "wp_posts", posts_table())
                .on("post_type", ResultSet::default()),
        );
        let session = session_over(&db);

        let page = session
            .browse(
                &browse_request("wp_posts", "post_type = 'page'", FilterMode::Sql),
                CellLimit::NONE,
                TIMEOUT,
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            db.queries(),
            ["SELECT * FROM `wp`.`wp_posts` WHERE (post_type = 'page'\n) LIMIT 101 OFFSET 0"]
        );
        assert_eq!(page.filter.unwrap().kind, FilterKind::Sql);
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
    fn info_reports_server_version_and_current_database() {
        let info = session("a").info();

        assert_eq!(info.id, "a");
        assert_eq!(info.server_version, "fake");
        assert_eq!(info.database.as_deref(), Some("wp"));
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
}
