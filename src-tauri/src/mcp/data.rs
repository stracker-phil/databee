//! The data tools: own connections from the saved connections and the Keychain, the app's
//! effective rules. Settings, saved connections and projects are read on every call.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::address::{Address, Target};
use crate::db::classify::is_read_only;
use crate::db::{BrowsePage, BrowseRequest, ColumnEquals, ExecuteResponse};
use crate::document::{self, Document};
use crate::error::{AppError, AppResult};
use crate::lens::{self, ResolvedReference};
use crate::rules::effective::RuleSources;
use crate::rules::{PresetCatalog, ProjectsStore};
use crate::schema;
use crate::search::{self, PaletteHit};
use crate::secrets::SecretStore;
use crate::session::{self, Session};
use crate::store::recents::{RecentConnection, RecentsStore};
use crate::store::settings::{Settings, SettingsStore};

pub const DEFAULT_ROWS: u32 = 200;
pub const MAX_ROWS: u32 = 1000;

#[derive(Serialize)]
pub struct SavedConnection {
    pub slug: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: Option<String>,
}

pub struct Core {
    config_dir: PathBuf,
    secrets: Arc<dyn SecretStore>,
    presets: PresetCatalog,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl Core {
    pub fn new(config_dir: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            presets: PresetCatalog::open(&config_dir),
            config_dir,
            secrets,
            sessions: Mutex::default(),
        }
    }

    fn settings(&self) -> Settings {
        SettingsStore::open(&self.config_dir).get()
    }

    fn saved(&self) -> Vec<RecentConnection> {
        RecentsStore::open(&self.config_dir).list()
    }

    fn saved_connection(&self, slug: &str) -> AppResult<RecentConnection> {
        self.saved()
            .into_iter()
            .find(|r| r.slug == slug)
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "No saved connection is named `{slug}`; `connections` lists them"
                ))
            })
    }

    pub fn connections(&self) -> Vec<SavedConnection> {
        self.saved()
            .into_iter()
            .map(|r| SavedConnection {
                name: r.spec.display_name(),
                slug: r.slug,
                host: r.spec.socket.clone().unwrap_or(r.spec.host),
                port: r.spec.port,
                database: r.spec.database,
            })
            .collect()
    }

    /// Connected on first use, and again when the saved connection changed.
    async fn session(&self, slug: &str) -> AppResult<Arc<Session>> {
        let saved = self.saved_connection(slug)?;
        if let Some(session) = self.sessions.lock().await.get(slug)
            && session.spec == saved.spec
        {
            return Ok(session.clone());
        }
        let session = Arc::new(
            session::open_detached(
                saved.spec,
                saved.slug,
                self.secrets.clone(),
                &self.settings(),
            )
            .await?,
        );
        self.sessions
            .lock()
            .await
            .insert(slug.to_string(), session.clone());
        Ok(session)
    }

    async fn with_rules<T, F>(&self, slug: &str, run: F) -> AppResult<T>
    where
        F: AsyncFnOnce(&Session, RuleSources<'_>, &Settings) -> AppResult<T>,
    {
        let session = self.session(slug).await?;
        let projects = ProjectsStore::open(&self.config_dir);
        let sources = RuleSources {
            presets: &self.presets,
            projects: &projects,
        };
        run(&session, sources, &self.settings()).await
    }

    pub async fn document(&self, text: &str) -> AppResult<Document> {
        let address = Address::parse(text)?;
        self.with_rules(&address.connection, async |session, sources, settings| {
            document::open(session, text, None, sources, settings.query_timeout()).await
        })
        .await
    }

    pub async fn rows(&self, text: &str, limit: Option<u32>) -> AppResult<BrowsePage> {
        let address = Address::parse(text)?;
        let limit = row_cap(limit);
        self.with_rules(&address.connection, async |session, sources, settings| {
            let request = browse_request(session, &address, limit).await?;
            session
                .browse(
                    &request,
                    settings.cell_limit(),
                    settings.query_timeout(),
                    Some(sources),
                )
                .await
        })
        .await
    }

    pub async fn query(
        &self,
        connection: &str,
        sql: &str,
        limit: Option<u32>,
    ) -> AppResult<ExecuteResponse> {
        is_read_only(sql).map_err(AppError::InvalidInput)?;
        let limit = row_cap(limit) as usize;
        self.with_rules(connection, async |session, sources, settings| {
            session
                .execute(
                    sql,
                    limit,
                    settings.cell_limit(),
                    settings.query_timeout(),
                    Some(sources),
                )
                .await
        })
        .await
    }

    /// The palette's search: `#<id>`, `@<name>`, `=<value>` or free text; only SELECTs, each
    /// with the search's timeout and section caps.
    pub async fn find(
        &self,
        connection: &str,
        database: &str,
        query: &str,
    ) -> AppResult<Vec<PaletteHit>> {
        self.with_rules(connection, async |session, sources, settings| {
            search::search(session, database, query, sources, settings.query_timeout()).await
        })
        .await
    }

    pub async fn references(
        &self,
        connection: &str,
        database: &str,
    ) -> AppResult<Vec<ResolvedReference>> {
        self.with_rules(connection, async |session, sources, _| {
            let schema = schema::database(session, database).await?;
            let lens = lens::for_database(session, database, Some(sources)).await?;
            Ok(schema
                .tables
                .iter()
                .flat_map(|t| lens.references_from(&t.name))
                .collect())
        })
        .await
    }
}

pub fn row_cap(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_ROWS).clamp(1, MAX_ROWS)
}

async fn browse_request(
    session: &Session,
    address: &Address,
    limit: u32,
) -> AppResult<BrowseRequest> {
    let (table, filter) = match &address.target {
        Target::Table { table, filter } => (table, filter.clone()),
        Target::Row { table, key } => (table, row_filter(session, address, table, key).await?),
        _ => {
            return Err(AppError::InvalidInput(format!(
                "`rows` takes a table or row address, not {address}"
            )));
        }
    };
    Ok(BrowseRequest {
        database: address.database.clone(),
        table: table.clone(),
        limit,
        r#where: filter,
        ..BrowseRequest::default()
    })
}

/// The row by its single-column primary key.
async fn row_filter(
    session: &Session,
    address: &Address,
    table: &str,
    key: &str,
) -> AppResult<Vec<ColumnEquals>> {
    let schema = schema::database(session, &address.database).await?;
    let Some([column]) = schema.keys(table).map(|k| k.primary_key.as_slice()) else {
        return Err(AppError::InvalidInput(format!(
            "{table} has no single-column primary key, so {address} names no row"
        )));
    };
    Ok(vec![ColumnEquals {
        column: column.clone(),
        value: Some(key.to_string()),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use serde_json::json;
    use tempfile::{TempDir, tempdir};

    fn core(dir: &TempDir) -> Core {
        Core::new(dir.path().to_path_buf(), Arc::new(MemoryStore::default()))
    }

    #[test]
    fn row_cap_defaults_to_200_and_stays_between_1_and_1000() {
        for (limit, cap) in [
            (None, 200),
            (Some(0), 1),
            (Some(1), 1),
            (Some(500), 500),
            (Some(1000), 1000),
            (Some(5000), 1000),
        ] {
            assert_eq!(row_cap(limit), cap, "{limit:?}");
        }
    }

    #[tokio::test]
    async fn writing_sql_is_refused_before_any_connection_is_looked_up() {
        let dir = tempdir().unwrap();
        let core = core(&dir);

        for (sql, statement) in [
            ("UPDATE t SET a=1", "UPDATE t SET a=1"),
            ("SELECT 1; DELETE FROM t", "DELETE FROM t"),
        ] {
            let err = core.query("nope", sql, None).await.err().unwrap();

            assert!(
                matches!(&err, AppError::InvalidInput(m) if m.starts_with(&format!("`{statement}` is not read-only"))),
                "{sql}: {err:?}"
            );
        }
    }

    #[test]
    fn connections_list_the_saved_slug_host_and_database() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("connections.json"),
            r#"[
                {
                    "identity": "root@db.local:3307/shop",
                    "spec": {"name": "Shop", "host": "db.local", "port": 3307, "user": "root", "database": "shop"},
                    "lastUsed": "2024-01-02T00:00:00Z",
                    "slug": "shop"
                },
                {
                    "identity": "root@localhost:3306/local",
                    "spec": {"name": "Local", "host": "localhost", "port": 3306, "socket": "/tmp/mysql.sock", "user": "root"},
                    "lastUsed": "2024-01-01T00:00:00Z",
                    "slug": "local"
                }
            ]"#,
        )
        .unwrap();

        let listed = serde_json::to_value(core(&dir).connections()).unwrap();

        assert_eq!(
            listed,
            json!([
                {"slug": "shop", "name": "Shop", "host": "db.local", "port": 3307, "database": "shop"},
                {"slug": "local", "name": "Local", "host": "/tmp/mysql.sock", "port": 3306, "database": null},
            ])
        );
    }

    #[test]
    fn no_saved_connections_list_as_empty() {
        let dir = tempdir().unwrap();

        assert!(core(&dir).connections().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_connection_slug_fails_with_the_hint_to_list_them() {
        let dir = tempdir().unwrap();
        let core = core(&dir);
        let expected = AppError::InvalidInput(
            "No saved connection is named `nope`; `connections` lists them".into(),
        );

        let document = core.document("databee://nope/db/t/1").await.err();
        let rows = core.rows("databee://nope/db/t", None).await.err();
        let references = core.references("nope", "db").await.err();
        let query = core.query("nope", "SELECT 1", None).await.err();

        for err in [document, rows, references, query] {
            assert_eq!(err, Some(expected.clone()));
        }
    }

    #[tokio::test]
    async fn a_malformed_address_is_invalid_input() {
        let dir = tempdir().unwrap();

        let err = core(&dir).document("not an address").await.err();

        assert!(matches!(err, Some(AppError::InvalidInput(_))), "{err:?}");
    }
}

#[cfg(test)]
mod server_tests {
    use super::*;
    use crate::connection::Secret;
    use crate::secrets::MemoryStore;
    use crate::test_support::{servers, spec};
    use crate::value::{KeyPart, Provenance, RowKey};
    use tempfile::{TempDir, tempdir};

    const DB: &str = "wp_test";

    /// A config dir whose only saved connection, `fixture-<port>`, points at a fixture server;
    /// its password lives in a `MemoryStore`.
    struct Rig {
        _dir: TempDir,
        core: Core,
        slug: String,
        server: String,
    }

    fn rig(host: &str, port: u16) -> Rig {
        let dir = tempdir().unwrap();
        let spec = spec(host, port, DB);
        let slug = format!("fixture-{port}");
        let saved = RecentConnection {
            identity: spec.identity(),
            account: spec.account(),
            spec: spec.clone(),
            last_used: chrono::Utc::now(),
            slug: slug.clone(),
        };
        std::fs::write(
            dir.path().join("connections.json"),
            serde_json::to_string(&[saved]).unwrap(),
        )
        .unwrap();
        let secrets = MemoryStore::default();
        secrets.set(&spec.account(), &Secret::new("root")).unwrap();
        Rig {
            core: Core::new(dir.path().to_path_buf(), Arc::new(secrets)),
            _dir: dir,
            slug,
            server: format!("{host}:{port}"),
        }
    }

    fn rigs() -> Vec<Rig> {
        servers().iter().map(|(h, p)| rig(h, *p)).collect()
    }

    #[tokio::test]
    #[ignore]
    async fn a_document_value_carries_its_table_column_and_key() {
        for rig in rigs() {
            let document = rig
                .core
                .document(&format!("databee://{}/{DB}/wp_users/12", rig.slug))
                .await
                .unwrap_or_else(|e| panic!("{}: {e:?}", rig.server));

            let own = &document.groups[0].rows[0];
            let login = own
                .fields
                .iter()
                .find(|(column, _)| column == "user_login")
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("{}: no user_login in {:?}", rig.server, own.fields));
            assert_eq!(
                login.source,
                Some(Provenance {
                    table: "wp_users".into(),
                    column: "user_login".into(),
                    row: RowKey(vec![KeyPart {
                        column: "ID".into(),
                        value: "12".into()
                    }]),
                }),
                "{}",
                rig.server
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn rows_of_a_home_or_project_address_are_refused() {
        for rig in rigs() {
            for suffix in ["", "?project"] {
                let address = format!("databee://{}/{DB}/{suffix}", rig.slug);
                let shown = Address::parse(&address).unwrap();

                let err = rig.core.rows(&address, None).await.err();

                assert_eq!(
                    err,
                    Some(AppError::InvalidInput(format!(
                        "`rows` takes a table or row address, not {shown}"
                    ))),
                    "{}: {address}",
                    rig.server
                );
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn references_name_the_tables_they_connect_and_their_source() {
        for rig in rigs() {
            let references = rig
                .core
                .references(&rig.slug, DB)
                .await
                .unwrap_or_else(|e| panic!("{}: {e:?}", rig.server));

            assert!(
                references
                    .iter()
                    .any(|r| r.from.table == "wp_comments" && r.to.table == "wp_posts"),
                "{}: {} references",
                rig.server,
                references.len()
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn find_returns_the_palette_hits_with_their_sections_and_sources() {
        for rig in rigs() {
            let hits = rig
                .core
                .find(&rig.slug, DB, "#4711")
                .await
                .unwrap_or_else(|e| panic!("{}: {e:?}", rig.server));

            let objects: Vec<&str> = hits
                .iter()
                .filter(|hit| hit.section == "objects with id 4711")
                .map(|hit| hit.source.as_str())
                .collect();
            assert!(
                objects.contains(&"wp_wc_orders") && objects.contains(&"wp_posts"),
                "{}: {objects:?}",
                rig.server
            );
            assert!(
                hits.iter()
                    .any(|hit| hit.section == "rows that reference 4711"
                        && hit.source == "wp_wc_orders.parent_order_id"),
                "{}: {} hits",
                rig.server,
                hits.len()
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn find_returns_no_hits_for_input_only_the_page_answers() {
        for rig in rigs() {
            let hits = rig.core.find(&rig.slug, DB, "t:orders").await;

            assert_eq!(hits, Ok(Vec::new()), "{}", rig.server);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn read_only_sql_runs_and_writing_sql_never_reaches_the_server() {
        for rig in rigs() {
            let ok = rig.core.query(&rig.slug, "SELECT 1", Some(1)).await;
            let refused = rig
                .core
                .query(&rig.slug, "DELETE FROM wp_users", None)
                .await
                .err();

            assert!(ok.is_ok(), "{}", rig.server);
            assert!(
                matches!(refused, Some(AppError::InvalidInput(_))),
                "{}: {refused:?}",
                rig.server
            );
        }
    }
}
