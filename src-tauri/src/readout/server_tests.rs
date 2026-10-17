//! The readout over the Docker fixtures (`dev/fixture.sql`) with the built-in presets, on MariaDB
//! and MySQL. Everything reads `wp_test` and `databee_test`; nothing is written.

use std::time::Duration;

use tempfile::{TempDir, tempdir};

use super::{AnomalyLine, NewObjects, PinnedResult, Readout, readout};
use crate::address::{Address, Target};
use crate::rules::catalog::PresetCatalog;
use crate::rules::effective::RuleSources;
use crate::rules::project::{ProjectsStore, SavedQuery};
use crate::rules::queries::Home;
use crate::session::Session;
use crate::store::recent_objects::RecentObject;
use crate::test_support::{connect_to, servers, spec};

const TIMEOUT: Duration = Duration::from_secs(10);

struct Rig {
    _dir: TempDir,
    catalog: PresetCatalog,
    projects: ProjectsStore,
}

impl Rig {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        Self {
            catalog: PresetCatalog::open(dir.path()),
            projects: ProjectsStore::open(dir.path()),
            _dir: dir,
        }
    }

    fn sources(&self) -> RuleSources<'_> {
        RuleSources {
            presets: &self.catalog,
            projects: &self.projects,
        }
    }
}

fn slug(port: u16) -> String {
    format!("it-{port}")
}

async fn session_on(host: &str, port: u16, database: &str) -> Session {
    let db = connect_to(host, port, database).await;
    Session::new(
        "it".into(),
        spec(host, port, database),
        slug(port),
        Box::new(db),
    )
}

struct Read {
    server: String,
    host: String,
    port: u16,
    session: Session,
    readout: Readout,
}

impl Read {
    fn address(&self, database: &str, target: Target) -> Address {
        Address::new(slug(self.port), database, target)
    }

    fn query(&self, database: &str, id: &str) -> Address {
        self.address(database, Target::Query { id: id.into() })
    }

    fn row(&self, database: &str, table: &str, key: &str) -> Address {
        self.address(
            database,
            Target::Row {
                table: table.into(),
                key: key.into(),
            },
        )
    }

    fn anomaly(&self, id: &str) -> Option<&AnomalyLine> {
        self.readout.anomalies.items.iter().find(|l| l.id == id)
    }

    fn new_objects(&self, entity: &str, table: &str) -> &NewObjects {
        self.readout
            .new_objects
            .items
            .iter()
            .find(|l| l.entity == entity && l.table == table)
            .unwrap_or_else(|| {
                panic!(
                    "{}: no new objects of {entity} on {table}: {:?}",
                    self.server, self.readout.new_objects
                )
            })
    }
}

async fn read_all(rig: &Rig, database: &str, recent: &[RecentObject]) -> Vec<Read> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, database).await;
        let readout = readout(&session, database, rig.sources(), recent, TIMEOUT).await;
        out.push(Read {
            server: format!("{host}:{port}"),
            host,
            port,
            session,
            readout,
        });
    }
    out
}

fn row_keys(line: &NewObjects) -> Vec<String> {
    line.rows
        .iter()
        .map(|row| match row.address.as_ref().map(|a| &a.target) {
            Some(Target::Row { key, .. }) => key.clone(),
            other => panic!("{}: row without a row address: {other:?}", line.entity),
        })
        .collect()
}

#[tokio::test]
#[ignore]
async fn the_status_names_the_server_database_prefix_and_applied_presets() {
    let rig = Rig::new();
    for read in read_all(&rig, "wp_test", &[]).await {
        let status = &read.readout.status;
        let server = &read.server;

        assert_eq!(status.host, read.host, "{server}");
        assert_eq!(status.slug, slug(read.port), "{server}");
        assert_eq!(status.database, "wp_test", "{server}");
        assert_eq!(status.prefix, "wp_", "{server}");
        assert_eq!(
            status.presets,
            ["wordpress", "woocommerce", "action-scheduler"],
            "{server}"
        );
        assert_eq!(status.server_version, read.session.db.server_version());
        assert!(!status.server_version.is_empty(), "{server}");
        assert_eq!(
            read.readout.address,
            read.address("wp_test", Target::Home),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn the_status_shows_the_value_pins_with_their_saved_query_addresses() {
    let rig = Rig::new();
    for read in read_all(&rig, "wp_test", &[]).await {
        let server = &read.server;
        let values = &read.readout.status.values;

        assert_eq!(values.error, None, "{server}");
        let shown: Vec<(&str, Option<&str>)> = values
            .items
            .iter()
            .map(|v| {
                (
                    v.id.as_str(),
                    v.value.as_ref().and_then(|v| v.raw.as_text()),
                )
            })
            .collect();
        assert_eq!(
            &shown[..2],
            [
                ("site_url", Some("https://shop.test")),
                ("db_version", Some("58975"))
            ],
            "{server}"
        );
        assert_eq!(shown[2].0, "autoload_bytes", "{server}");
        let bytes: u64 = shown[2].1.unwrap().parse().unwrap();
        assert!(bytes > 0, "{server}");
        assert_eq!(shown[3], ("woocommerce_version", Some("9.3.0")), "{server}");
        assert_eq!(shown.len(), 4, "{server}: {shown:?}");
        for line in &values.items {
            assert_eq!(line.address, read.query("wp_test", &line.id), "{server}");
            assert_eq!(line.error, None, "{server}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn anomalies_list_what_the_fixture_data_trips_with_counts_and_addresses() {
    let rig = Rig::new();
    for read in read_all(&rig, "wp_test", &[]).await {
        let server = &read.server;
        assert_eq!(read.readout.anomalies.error, None, "{server}");

        let drift = read.anomaly("hpos_drift");
        let drift = drift.unwrap_or_else(|| panic!("{server}: {:?}", read.readout.anomalies));
        assert_eq!(drift.preset, "woocommerce", "{server}");
        assert_eq!(drift.count.as_deref(), Some("2"), "{server}");
        assert_eq!(
            drift.address,
            Some(read.query("wp_test", "hpos_drift")),
            "{server}"
        );
        assert_eq!(
            drift.text,
            "wp_wc_orders differ from wp_posts while data sync is on"
        );

        let transients = read.anomaly("transients_expired").unwrap();
        assert_eq!(transients.count.as_deref(), Some("1"), "{server}");
        assert_eq!(
            transients.address,
            Some(read.query("wp_test", "transients")),
            "{server}"
        );
        let fix = transients.fix.as_ref().unwrap();
        assert_eq!(fix.action, "delete_expired_transients", "{server}");
        assert_eq!(
            fix.address,
            read.address(
                "wp_test",
                Target::Table {
                    table: "wp_options".into(),
                    filter: Vec::new()
                }
            ),
            "{server}"
        );

        let failed = read.anomaly("actions_failed").unwrap();
        assert_eq!(failed.count.as_deref(), Some("1"), "{server}");
        assert_eq!(
            failed.address,
            Some(read.address(
                "wp_test",
                Target::Table {
                    table: "wp_actionscheduler_actions".into(),
                    filter: Vec::new()
                }
            )),
            "{server}"
        );
        assert_eq!(
            failed.filter.as_deref(),
            Some("status = 'failed'"),
            "{server}"
        );

        assert!(read.anomaly("autoload_heavy").is_none(), "{server}");
        assert!(
            read.readout
                .anomalies
                .items
                .iter()
                .all(|l| l.error.is_none()),
            "{server}: {:?}",
            read.readout.anomalies
        );
    }
}

#[tokio::test]
#[ignore]
async fn new_objects_show_the_newest_rows_of_each_entity_with_a_created_column() {
    let rig = Rig::new();
    for read in read_all(&rig, "wp_test", &[]).await {
        let server = &read.server;
        assert_eq!(read.readout.new_objects.error, None, "{server}");

        let orders = read.new_objects("order", "wp_wc_orders");
        assert_eq!(orders.error, None, "{server}");
        assert_eq!(orders.preset, "woocommerce", "{server}");
        assert_eq!(orders.columns[0], "id", "{server}");
        assert!(
            orders.columns.contains(&"date_created_gmt".to_string()),
            "{server}"
        );
        let keys = row_keys(orders);
        assert_eq!(keys.len(), 4, "{server}");
        assert_eq!(keys.first().map(String::as_str), Some("4712"), "{server}");
        assert_eq!(keys.last().map(String::as_str), Some("4713"), "{server}");
        assert_eq!(
            orders.rows[0].address,
            Some(read.row("wp_test", "wp_wc_orders", "4712")),
            "{server}"
        );

        let posts = read.new_objects("post", "wp_posts");
        assert_eq!(posts.rows.len(), 5, "{server}");
        assert!(
            posts.sql.ends_with("ORDER BY `post_date_gmt` DESC LIMIT 5"),
            "{server}: {}",
            posts.sql
        );
    }
}

#[tokio::test]
#[ignore]
async fn new_objects_of_a_filtered_entity_only_list_rows_that_pass_its_filter() {
    let rig = Rig::new();
    for read in read_all(&rig, "wp_test", &[]).await {
        let server = &read.server;

        let order_posts = read.new_objects("order", "wp_posts");
        let products = read.new_objects("product", "wp_posts");

        assert_eq!(row_keys(order_posts), ["4711"], "{server}");
        let mut product_keys = row_keys(products);
        product_keys.sort();
        assert_eq!(product_keys, ["123", "200", "201", "202"], "{server}");
        assert!(order_posts.sql.contains(" WHERE "), "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn pinned_queries_show_a_count_and_the_newest_rows_with_row_addresses() {
    let rig = Rig::new();
    for read in read_all(&rig, "wp_test", &[]).await {
        let server = &read.server;
        assert_eq!(read.readout.pinned.error, None, "{server}");
        let line = |id: &str| {
            read.readout
                .pinned
                .items
                .iter()
                .find(|l| l.id == id)
                .unwrap_or_else(|| panic!("{server}: no pinned {id}: {:?}", read.readout.pinned))
        };

        let transients = line("transients");
        let orders = line("orders");

        assert_eq!(transients.error, None, "{server}");
        assert_eq!(
            transients.result,
            Some(PinnedResult::Count { count: 2 }),
            "{server}"
        );
        assert_eq!(
            transients.address,
            read.query("wp_test", "transients"),
            "{server}"
        );
        assert_eq!(orders.error, None, "{server}");
        let Some(PinnedResult::Rows { columns, rows }) = &orders.result else {
            panic!("{server}: {orders:?}");
        };
        assert_eq!(columns[0], "id", "{server}");
        assert_eq!(rows.len(), 4, "{server}");
        assert_eq!(rows[0].values[0].raw.as_text(), Some("4714"), "{server}");
        assert_eq!(
            rows[0].address,
            Some(read.row("wp_test", "wp_wc_orders", "4714")),
            "{server}"
        );
        assert!(
            read.readout.pinned.items.iter().all(|l| !matches!(
                l.id.as_str(),
                "site_url" | "db_version" | "autoload_bytes" | "woocommerce_version"
            )),
            "{server}: value pins belong to the status"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_pinned_query_with_broken_sql_carries_its_error_and_the_other_parts_stand() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let slug = slug(port);
        rig.projects
            .update(&slug, "wp_test", |p| {
                p.saved = vec![
                    SavedQuery {
                        id: "broken".into(),
                        title: "Broken".into(),
                        sql: "SELEKT nope FROM nowhere".into(),
                        pin: Some(Home::Count),
                    },
                    SavedQuery {
                        id: "mine".into(),
                        title: "Mine".into(),
                        sql: "SELECT option_id FROM wp_options".into(),
                        pin: Some(Home::Count),
                    },
                ];
                Ok(())
            })
            .await
            .unwrap();
        let session = session_on(&host, port, "wp_test").await;

        let read = readout(&session, "wp_test", rig.sources(), &[], TIMEOUT).await;

        let server = format!("{host}:{port}");
        assert_eq!(read.pinned.error, None, "{server}");
        let broken = &read.pinned.items[0];
        assert_eq!(broken.id, "broken", "{server}");
        assert_eq!(broken.result, None, "{server}");
        assert!(broken.error.is_some(), "{server}");
        let mine = &read.pinned.items[1];
        assert_eq!(mine.id, "mine", "{server}");
        assert_eq!(mine.error, None, "{server}");
        assert!(
            matches!(mine.result, Some(PinnedResult::Count { count }) if count > 10),
            "{server}: {:?}",
            mine.result
        );
        assert!(
            read.pinned
                .items
                .iter()
                .any(|l| l.id == "transients" && l.error.is_none()),
            "{server}"
        );
        assert_eq!(read.status.values.error, None, "{server}");
        assert_eq!(read.status.values.items.len(), 4, "{server}");
        assert_eq!(read.anomalies.error, None, "{server}");
        assert!(!read.anomalies.items.is_empty(), "{server}");
        assert_eq!(read.new_objects.error, None, "{server}");
        assert!(!read.new_objects.items.is_empty(), "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn recent_objects_show_the_values_of_their_row() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;
        let address = Address::new(
            slug(port),
            "wp_test",
            Target::Row {
                table: "wp_wc_orders".into(),
                key: "4711".into(),
            },
        );
        let recent = [RecentObject {
            address: address.to_string(),
            at: 99,
        }];

        let read = readout(&session, "wp_test", rig.sources(), &recent, TIMEOUT).await;

        let server = format!("{host}:{port}");
        assert_eq!(read.recent.error, None, "{server}");
        assert_eq!(read.recent.items.len(), 1, "{server}");
        let line = &read.recent.items[0];
        assert_eq!((line.address.clone(), line.at), (address, 99), "{server}");
        assert_eq!(line.error, None, "{server}");
        let status = line.summary.iter().find(|s| s.column == "status");
        let status = status.unwrap_or_else(|| panic!("{server}: {:?}", line.summary));
        assert_eq!(
            status.value.raw.as_text(),
            Some("wc-processing"),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_database_no_preset_fits_has_a_bare_status_and_no_other_lines() {
    let rig = Rig::new();
    for read in read_all(&rig, "databee_test", &[]).await {
        let server = &read.server;
        let readout = &read.readout;

        assert_eq!(readout.status.database, "databee_test", "{server}");
        assert_eq!(readout.status.prefix, "", "{server}");
        assert!(readout.status.presets.is_empty(), "{server}");
        assert!(!readout.status.server_version.is_empty(), "{server}");
        assert!(readout.status.values.items.is_empty(), "{server}");
        assert!(readout.anomalies.items.is_empty(), "{server}");
        assert!(readout.new_objects.items.is_empty(), "{server}");
        assert!(readout.pinned.items.is_empty(), "{server}");
        for error in [
            &readout.status.values.error,
            &readout.anomalies.error,
            &readout.new_objects.error,
            &readout.pinned.error,
            &readout.recent.error,
        ] {
            assert_eq!(error, &None, "{server}");
        }
    }
}
