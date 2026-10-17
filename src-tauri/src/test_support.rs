//! Helpers shared by unit tests and the integration tests in `tests/` (through the `test-util`
//! feature): the Docker fixture servers and the plugin command lists in `build.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::connection::{ConnectionSpec, Secret};
use crate::db::Database;
use crate::db::mysql::MySqlSession;
use crate::rules::PresetCatalog;
use crate::rules::catalog::BUILT_IN;
use crate::rules::effective::EffectiveRules;
use crate::rules::project::Project;
use crate::schema::DatabaseSchema;
use crate::wp::catalog::Catalog;
use crate::wp::site::WpContext;

const TIMEOUT: Duration = Duration::from_secs(10);
const BUILD_RS: &str = include_str!("../build.rs");

/// The fixture servers from `dev/docker-compose.yml`, or `DATABEE_TEST_SERVERS="host:port,…"`.
pub fn servers() -> Vec<(String, u16)> {
    std::env::var("DATABEE_TEST_SERVERS")
        .unwrap_or_else(|_| "127.0.0.1:33306,127.0.0.1:33307".into())
        .split(',')
        .map(|s| {
            let (host, port) = s.rsplit_once(':').unwrap();
            (host.to_string(), port.parse().unwrap())
        })
        .collect()
}

/// The fixtures' `root` account (password `root`) on one server.
pub fn spec(host: &str, port: u16, database: &str) -> ConnectionSpec {
    ConnectionSpec {
        name: None,
        host: host.into(),
        port,
        socket: None,
        user: "root".into(),
        database: Some(database.into()),
    }
}

pub async fn connect_to(host: &str, port: u16, database: &str) -> MySqlSession {
    MySqlSession::connect(
        &spec(host, port, database),
        Some(&Secret::new("root")),
        TIMEOUT,
        TIMEOUT,
    )
    .await
    .unwrap_or_else(|e| panic!("{host}:{port}: {e}"))
}

/// One session per fixture server, each on `database`.
pub async fn sessions(database: &str) -> Vec<MySqlSession> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        out.push(connect_to(&host, port, database).await);
    }
    out
}

/// The WordPress screens' context and entity catalog of `database`, built as the commands build
/// them: the built-in presets over the database's schema, no project rules. `None` unless the
/// `wordpress` preset applies.
pub async fn wordpress(db: &MySqlSession, database: &str) -> Option<(WpContext, Catalog)> {
    let (tables, keys, columns) = tokio::try_join!(
        db.tables(database),
        db.keys(database),
        db.completion(database)
    )
    .unwrap_or_else(|e| panic!("{database}: {e:?}"));
    let schema = DatabaseSchema {
        tables,
        keys,
        columns,
        incoming: Default::default(),
    };
    let presets = PresetCatalog::with_built_in(PathBuf::new(), BUILT_IN).presets();
    let rules = Arc::new(EffectiveRules::build(
        &Project::new("test", database),
        &schema,
        &presets,
    ));
    let catalog = Catalog::new(&rules, &schema);
    WpContext::new(rules).map(|ctx| (ctx, catalog))
}

/// The quoted names in `build.rs` that start with `prefix`, i.e. one plugin's granted commands.
pub fn build_rs_commands(prefix: &str) -> Vec<&'static str> {
    BUILD_RS
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| s.starts_with(prefix))
        .collect()
}
