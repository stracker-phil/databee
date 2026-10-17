//! The readout: the home page of one database (`databee://<slug>/<db>/`). A status line, the
//! anomalies of the applied presets, the newest rows of each entity with a `created` column,
//! the pinned saved queries and the objects opened last. Every part runs with its own timeout;
//! a part or a line that fails carries its error, and the rest of the readout stands.

pub mod commands;
mod parts;
#[cfg(test)]
mod server_tests;
mod sql;
#[cfg(test)]
mod testing;

use std::future::Future;
use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use crate::address::{Address, Target};
use crate::document::Ctx;
use crate::error::{AppError, AppResult};
use crate::rules::effective::RuleSources;
use crate::saved::SavedOrigin;
use crate::session::Session;
use crate::store::recent_objects::{RecentObject, RecentObjectsStore};
use crate::value::Value;

/// How long one query of the readout may run (the query timeout when lower).
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
/// How long one part may take in all; its queries run side by side.
const PART_TIMEOUT: Duration = Duration::from_secs(10);
/// Rows of each entity in `new_objects`.
const NEW_ROWS: u32 = 5;
/// Rows of a pinned query shown as rows without a `limit`.
const PINNED_ROWS: u32 = 5;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Readout {
    pub address: Address,
    pub status: Status,
    pub anomalies: Part<AnomalyLine>,
    pub new_objects: Part<NewObjects>,
    pub pinned: Part<PinnedLine>,
    pub recent: Part<RecentLine>,
}

/// One section's lines, or why the section could not be read.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Part<T> {
    pub items: Vec<T>,
    pub error: Option<String>,
}

impl<T> Default for Part<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            error: None,
        }
    }
}

impl<T> Part<T> {
    fn failed(error: String) -> Self {
        Self {
            items: Vec::new(),
            error: Some(error),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The server's host, or its socket path.
    pub host: String,
    pub slug: String,
    pub database: String,
    pub prefix: String,
    /// The applied presets, in apply order.
    pub presets: Vec<String>,
    pub server_version: String,
    /// Saved queries pinned with `show = "value"`.
    pub values: Part<StatusValue>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct StatusValue {
    pub id: String,
    pub title: String,
    pub sql: String,
    pub address: Address,
    /// The first column of the first row; `None` without a row.
    pub value: Option<Value>,
    pub error: Option<String>,
}

/// An `[[anomaly]]` whose value is a non-zero number, or whose query failed.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct AnomalyLine {
    pub id: String,
    pub preset: String,
    pub text: String,
    /// The value as the query returned it.
    pub count: Option<String>,
    /// What the line opens: a saved query or a table.
    pub address: Option<Address>,
    /// The `where` of `open = { table, where }`, as SQL on that table (not addressable).
    pub filter: Option<String>,
    pub fix: Option<AnomalyFix>,
    pub error: Option<String>,
}

/// A `scope = "table"` action offered next to an anomaly; `address` is the table to plan it on.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct AnomalyFix {
    pub action: String,
    pub address: Address,
}

/// The newest rows of one entity, by its `created` column.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct NewObjects {
    pub entity: String,
    pub preset: String,
    pub table: String,
    pub sql: String,
    pub columns: Vec<String>,
    pub rows: Vec<ReadoutRow>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ReadoutRow {
    /// The row, when its table has a single-column primary key in the result.
    pub address: Option<Address>,
    pub values: Vec<Value>,
}

/// A saved query pinned with `show = "count"` or `"rows"`.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct PinnedLine {
    pub id: String,
    pub title: String,
    pub sql: String,
    pub origin: SavedOrigin,
    pub address: Address,
    pub result: Option<PinnedResult>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "show", rename_all = "camelCase")]
pub enum PinnedResult {
    Count {
        #[ts(type = "number")]
        count: u64,
    },
    Rows {
        columns: Vec<String>,
        rows: Vec<ReadoutRow>,
    },
}

/// A row opened in the app, newest first, with the values a peek shows of it.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecentLine {
    pub address: Address,
    /// Unix seconds.
    #[ts(type = "number")]
    pub at: i64,
    pub summary: Vec<SummaryValue>,
    pub error: Option<String>,
}

/// A value of a recent object's peek, under its column or meta key.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct SummaryValue {
    pub column: String,
    pub value: Value,
}

pub async fn readout(
    session: &Session,
    database: &str,
    sources: RuleSources<'_>,
    recent: &[RecentObject],
    query_timeout: Duration,
) -> Readout {
    let timeout = query_timeout.min(QUERY_TIMEOUT);
    let ctx = match within(Ctx::load(session, database, sources, timeout)).await {
        Ok(ctx) => ctx,
        Err(e) => return unreadable(session, database, e),
    };
    let project = sources.projects.get(&session.slug(), database);
    let entries = crate::saved::list(&project, &sources.projects.global(), &ctx.rules);
    let (values, anomalies, new_objects, pinned, recent) = tokio::join!(
        part(parts::status_values(&ctx, sources, &entries)),
        part(parts::anomalies(&ctx)),
        part(parts::new_objects(&ctx)),
        part(parts::pinned(&ctx, sources, &entries)),
        part(parts::recent(&ctx, recent)),
    );
    let applied = &ctx.rules.applied;
    Readout {
        address: home(session, database),
        status: status(
            session,
            database,
            applied.prefix.clone(),
            applied.preset_names(),
            values,
        ),
        anomalies,
        new_objects,
        pinned,
        recent,
    }
}

fn home(session: &Session, database: &str) -> Address {
    Address::new(session.slug(), database, Target::Home)
}

fn status(
    session: &Session,
    database: &str,
    prefix: String,
    presets: Vec<String>,
    values: Part<StatusValue>,
) -> Status {
    let spec = &session.spec;
    Status {
        host: spec.socket.clone().unwrap_or_else(|| spec.host.clone()),
        slug: session.slug(),
        database: database.to_string(),
        prefix,
        presets,
        server_version: session.db.server_version().to_string(),
        values,
    }
}

/// Without a schema or rules, every part says why.
fn unreadable(session: &Session, database: &str, error: String) -> Readout {
    Readout {
        address: home(session, database),
        status: status(
            session,
            database,
            String::new(),
            Vec::new(),
            Part::failed(error.clone()),
        ),
        anomalies: Part::failed(error.clone()),
        new_objects: Part::failed(error.clone()),
        pinned: Part::failed(error.clone()),
        recent: Part::failed(error),
    }
}

async fn within<T>(work: impl Future<Output = AppResult<T>>) -> Result<T, String> {
    match tokio::time::timeout(PART_TIMEOUT, work).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!("Timed out after {} s", PART_TIMEOUT.as_secs())),
    }
}

async fn part<T>(work: impl Future<Output = AppResult<Vec<T>>>) -> Part<T> {
    match within(work).await {
        Ok(items) => Part {
            items,
            ..Part::default()
        },
        Err(error) => Part::failed(error),
    }
}

/// Records a row address as the newest recent object of its project.
pub async fn remember(store: &RecentObjectsStore, address: &Address, at: i64) -> AppResult<()> {
    let Target::Row { .. } = &address.target else {
        return Err(AppError::InvalidInput(format!(
            "Only a row is a recent object, not {address}"
        )));
    };
    store
        .remember(
            &address.connection,
            &address.database,
            address.to_string(),
            at,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::testing::Rig;
    use super::*;
    use crate::db::{Cell, FakeDatabase};
    use crate::document::testing::{TIMEOUT, num, result, shop};
    use crate::error::ErrorKind;
    use crate::rules::project::SavedQuery;
    use crate::rules::queries::Home;

    const RULES: &str = r#"
[[pinned]]
id = "ver"
sql = "SELECT 1 AS ver FROM {p}orders"
home = { show = "value" }

[[anomaly]]
id = "bad"
sql = "SELECT 'bad' FROM {p}orders"
text = "bad"
"#;

    fn db() -> FakeDatabase {
        shop()
            .on(
                "AS ver",
                result(&["v"], vec![vec![Cell::Text("1.0".into())]]),
            )
            .on("AS good", result(&["c"], vec![vec![num("3")]]))
            .failing("'bad'", AppError::InvalidInput("boom".into()))
    }

    fn saved(id: &str, sql: &str) -> SavedQuery {
        SavedQuery {
            id: id.into(),
            title: id.into(),
            sql: sql.into(),
            pin: Some(Home::Count),
        }
    }

    #[tokio::test]
    async fn a_failing_anomaly_and_a_broken_pinned_query_leave_the_other_parts_standing() {
        let rig = Rig::new(db(), RULES, &[]);
        rig.projects
            .update("local", "wp", |p| {
                p.saved = vec![
                    saved("broken", "SELECT 1; SELECT 2"),
                    saved("good", "SELECT 1 AS good FROM {p}orders"),
                ];
                Ok(())
            })
            .await
            .unwrap();

        let readout = readout(&rig.session, "wp", rig.sources(), &[], TIMEOUT).await;

        assert_eq!(readout.address, home(&rig.session, "wp"));
        assert_eq!(
            (
                readout.status.host.as_str(),
                readout.status.slug.as_str(),
                readout.status.database.as_str(),
                readout.status.prefix.as_str(),
                readout.status.server_version.as_str(),
            ),
            ("127.0.0.1", "local", "wp", "f_", "fake")
        );
        assert_eq!(readout.status.presets, ["doc"]);
        assert_eq!(readout.status.values.error, None);
        let versions: Vec<&str> = readout
            .status
            .values
            .items
            .iter()
            .map(|v| v.id.as_str())
            .collect();
        assert_eq!(versions, ["ver"]);
        assert_eq!(readout.anomalies.error, None);
        assert_eq!(readout.anomalies.items.len(), 1);
        assert_eq!(readout.anomalies.items[0].error.as_deref(), Some("boom"));
        assert_eq!(readout.pinned.error, None);
        let pinned: Vec<(&str, bool, Option<&PinnedResult>)> = readout
            .pinned
            .items
            .iter()
            .map(|l| (l.id.as_str(), l.error.is_some(), l.result.as_ref()))
            .collect();
        assert_eq!(
            pinned,
            [
                ("broken", true, None),
                ("good", false, Some(&PinnedResult::Count { count: 3 })),
            ]
        );
        assert_eq!(readout.recent, Part::default());
    }

    #[tokio::test]
    async fn remember_refuses_an_address_that_is_not_a_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());
        for target in [
            Target::Home,
            Target::Project,
            Target::Query { id: "q".into() },
            Target::Table {
                table: "t".into(),
                filter: Vec::new(),
            },
        ] {
            let address = Address::new("local", "wp", target);

            let err = remember(&store, &address, 1).await.unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{address}");
        }
        assert!(store.get("local", "wp").is_empty());
    }

    #[tokio::test]
    async fn remember_stores_a_row_address_for_its_connection_and_database() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentObjectsStore::open(dir.path());
        let address = Address::new(
            "local",
            "wp",
            Target::Row {
                table: "wp_posts".into(),
                key: "7".into(),
            },
        );

        remember(&store, &address, 42).await.unwrap();

        assert_eq!(
            store.get("local", "wp"),
            [RecentObject {
                address: "databee://local/wp/wp_posts/7".into(),
                at: 42
            }]
        );
        assert!(store.get("local", "other").is_empty());
        assert!(store.get("elsewhere", "wp").is_empty());
    }
}
