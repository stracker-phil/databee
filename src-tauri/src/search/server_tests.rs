//! The palette's search over the Docker fixtures (`dev/fixture.sql`) with the built-in presets, on
//! MariaDB and MySQL. Everything reads `wp_test` and `databee_test`, except the cap test, which
//! builds a scratch database of its own.

use std::time::Duration;

use tempfile::{TempDir, tempdir};

use super::{PaletteHit, SECTION_CAP, search};
use crate::address::{Address, Target};
use crate::db::Database;
use crate::rules::catalog::PresetCatalog;
use crate::rules::effective::RuleSources;
use crate::rules::project::ProjectsStore;
use crate::session::Session;
use crate::test_support::{connect_to, servers, spec};

const TIMEOUT: Duration = Duration::from_secs(10);
const SCRATCH: &str = "search_cap_probe";

/// The built-in presets and an empty projects file, both in a tempdir.
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

struct Found {
    server: String,
    port: u16,
    hits: Vec<PaletteHit>,
}

impl Found {
    fn row(&self, database: &str, table: &str, key: &str) -> Address {
        Address::new(
            slug(self.port),
            database,
            Target::Row {
                table: table.into(),
                key: key.into(),
            },
        )
    }

    /// `(source, key)` of the hits of `section`, in order.
    fn section(&self, section: &str) -> Vec<(String, String)> {
        self.hits
            .iter()
            .filter(|hit| hit.section == section)
            .map(|hit| (hit.source.clone(), key(hit)))
            .collect()
    }

    fn has(&self, section: &str, source: &str, key: &str) -> bool {
        self.section(section)
            .contains(&(source.to_string(), key.to_string()))
    }
}

fn key(hit: &PaletteHit) -> String {
    hit.title[0]
        .raw
        .as_text()
        .unwrap_or_else(|| panic!("no text key in {hit:?}"))
        .to_string()
}

fn text(value: &crate::value::Value) -> Option<&str> {
    value.raw.as_text()
}

/// The hits for `input` in `database` on every fixture server.
async fn search_all(rig: &Rig, database: &str, input: &str) -> Vec<Found> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, database).await;
        let server = format!("{host}:{port}");
        let hits = search(&session, database, input, rig.sources(), TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{server}: {input}: {e:?}"));
        out.push(Found { server, port, hits });
    }
    out
}

#[tokio::test]
#[ignore]
async fn an_id_finds_the_objects_with_that_id_and_the_rows_that_reference_it() {
    let rig = Rig::new();
    for found in search_all(&rig, "wp_test", "#4711").await {
        let server = &found.server;
        let objects = "objects with id 4711";
        for (table, id) in [("wp_wc_orders", "4711"), ("wp_posts", "4711")] {
            assert!(
                found.has(objects, table, id),
                "{server}: {:?}",
                found.section(objects)
            );
        }

        let references = "rows that reference 4711";
        for (source, id) in [
            ("wp_wc_orders.parent_order_id", "4712"),
            ("wp_wc_orders.parent_order_id", "4714"),
            ("wp_actionscheduler_actions", "1"),
            ("wp_woocommerce_log", "2"),
            ("wp_woocommerce_log", "5"),
            ("wp_wc_order_addresses.order_id", "1"),
            ("wp_wc_order_addresses.order_id", "2"),
            ("wp_comments.comment_post_ID", "1"),
            ("wp_comments.comment_post_ID", "2"),
            ("wp_woocommerce_order_items.order_id", "1"),
            ("wp_woocommerce_order_items.order_id", "2"),
            ("wp_wc_order_operational_data.order_id", "1"),
            ("wp_wc_order_product_lookup.order_id", "1"),
        ] {
            assert!(
                found.has(references, source, id),
                "{server}: {source} {id} in {:?}",
                found.section(references)
            );
        }
        for (source, id) in [
            ("wp_actionscheduler_actions", "3"),
            ("wp_woocommerce_log", "3"),
            ("wp_woocommerce_log", "4"),
        ] {
            assert!(
                !found.has(references, source, id),
                "{server}: {source} {id} in {:?}",
                found.section(references)
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn an_id_lists_each_row_once_and_each_section_at_most_the_cap() {
    let rig = Rig::new();
    for found in search_all(&rig, "wp_test", "#4711").await {
        let mut addresses: Vec<String> = found.hits.iter().map(|h| h.address.to_string()).collect();
        addresses.sort();
        let total = addresses.len();
        addresses.dedup();
        assert_eq!(
            addresses.len(),
            total,
            "{}: a row is listed twice",
            found.server
        );

        for section in ["objects with id 4711", "rows that reference 4711"] {
            let count = found.section(section).len();
            assert!(
                (1..=SECTION_CAP).contains(&count),
                "{}: {section} has {count} hits",
                found.server
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn a_pasted_external_id_leads_with_the_row_that_holds_it() {
    let rig = Rig::new();
    for found in search_all(&rig, "wp_test", "66M12696JX411763D").await {
        let first = found
            .hits
            .first()
            .unwrap_or_else(|| panic!("{}: no hits", found.server));

        assert_eq!(first.section, "external id 66M12696JX411763D");
        assert_eq!(first.source, "wp_wc_orders");
        assert_eq!(first.address, found.row("wp_test", "wp_wc_orders", "4711"));
        assert_eq!(key(first), "4711");
        assert_eq!(text(&first.detail[0]), Some("_ppcp_paypal_order_id"));
        assert_eq!(text(&first.detail[1]), Some("66M12696JX411763D"));
    }
}

#[tokio::test]
#[ignore]
async fn people_search_lists_the_user_before_the_addresses() {
    let rig = Rig::new();
    for found in search_all(&rig, "wp_test", "@jane").await {
        let people = found.section("people matching jane");

        assert_eq!(
            people[0],
            ("wp_users".to_string(), "12".to_string()),
            "{}: {people:?}",
            found.server
        );
        let mut addresses = people[1..].to_vec();
        addresses.sort();
        assert_eq!(
            addresses,
            [
                ("wp_wc_order_addresses".to_string(), "1".to_string()),
                ("wp_wc_order_addresses".to_string(), "2".to_string()),
            ],
            "{}",
            found.server
        );
    }
}

#[tokio::test]
#[ignore]
async fn find_everywhere_opens_the_owner_of_a_matching_meta_value_and_the_log_row() {
    let rig = Rig::new();
    for found in search_all(&rig, "wp_test", "=sandbox").await {
        let server = &found.server;
        let meta: Vec<_> = found
            .hits
            .iter()
            .filter(|hit| hit.section == "wp_wc_orders_meta.meta_value")
            .collect();

        assert_eq!(meta.len(), 1, "{server}: {meta:?}");
        let owner = meta[0];
        assert_eq!(owner.source, "wp_wc_orders", "{server}");
        assert_eq!(
            owner.address,
            found.row("wp_test", "wp_wc_orders", "4711"),
            "{server}"
        );
        assert_eq!(key(owner), "4711", "{server}");
        assert_eq!(
            text(&owner.detail[0]),
            Some("_ppcp_paypal_payment_mode"),
            "{server}"
        );
        assert_eq!(text(&owner.detail[1]), Some("sandbox"), "{server}");
        assert_eq!(
            found.section("wp_woocommerce_log.message"),
            [("wp_woocommerce_log.message".to_string(), "1".to_string())],
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn find_everywhere_without_the_woocommerce_preset_has_no_meta_or_log_section() {
    let rig = Rig::new();
    for (_, port) in servers() {
        rig.projects
            .update(&slug(port), "wp_test", |project| {
                project.presets = Some(vec!["wordpress".into(), "action-scheduler".into()]);
                Ok(())
            })
            .await
            .unwrap();
    }
    for found in search_all(&rig, "wp_test", "=sandbox").await {
        let sections: Vec<&str> = found.hits.iter().map(|h| h.section.as_str()).collect();

        assert!(
            !sections.contains(&"wp_wc_orders_meta.meta_value")
                && !sections.contains(&"wp_woocommerce_log.message"),
            "{}: {sections:?}",
            found.server
        );
    }
}

#[tokio::test]
#[ignore]
async fn an_id_without_rules_reads_the_primary_keys_of_the_database() {
    let rig = Rig::new();
    for found in search_all(&rig, "databee_test", "#1").await {
        let server = &found.server;
        let objects = "objects with id 1";
        for table in ["authors", "books", "cov_parents", "wp_posts"] {
            assert!(
                found.has(objects, table, "1"),
                "{server}: {table} in {:?}",
                found.section(objects)
            );
        }
        let tables: Vec<String> = found.section(objects).into_iter().map(|(t, _)| t).collect();
        assert!(
            !tables.contains(&"published".to_string()),
            "{server}: {tables:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_section_of_referencing_rows_is_capped_at_the_section_cap() {
    let rig = Rig::new();
    let mut build = vec![
        format!("CREATE DATABASE `{SCRATCH}`"),
        format!("CREATE TABLE `{SCRATCH}`.`parents` (parent_id int PRIMARY KEY) ENGINE=InnoDB"),
        format!(
            "CREATE TABLE `{SCRATCH}`.`children` (child_id int PRIMARY KEY, parent_id int, \
             FOREIGN KEY (parent_id) REFERENCES `{SCRATCH}`.`parents` (parent_id)) ENGINE=InnoDB"
        ),
        format!("INSERT INTO `{SCRATCH}`.`parents` VALUES (777001)"),
    ];
    for i in 1..=SECTION_CAP + 5 {
        let child = 880_000 + i;
        build.push(format!(
            "INSERT INTO `{SCRATCH}`.`children` VALUES ({child}, 777001)"
        ));
    }
    let teardown = [format!("DROP DATABASE IF EXISTS `{SCRATCH}`")];

    for (host, port) in servers() {
        let admin = connect_to(&host, port, "databee_test").await;
        admin.apply(&teardown, TIMEOUT).await.unwrap();
        admin.apply(&build, TIMEOUT).await.unwrap();
        let session = session_on(&host, port, SCRATCH).await;

        let hits = search(&session, SCRATCH, "#777001", rig.sources(), TIMEOUT).await;

        admin.apply(&teardown, TIMEOUT).await.unwrap();
        let found = Found {
            server: format!("{host}:{port}"),
            port,
            hits: hits.unwrap_or_else(|e| panic!("{host}:{port}: {e:?}")),
        };
        assert_eq!(
            found.section("objects with id 777001"),
            [("parents".to_string(), "777001".to_string())],
            "{}",
            found.server
        );
        let referencing = found.section("rows that reference 777001");
        assert_eq!(referencing.len(), SECTION_CAP, "{}", found.server);
        let source = "children.parent_id";
        assert!(
            referencing.iter().all(|(s, _)| s == source),
            "{}: {referencing:?}",
            found.server
        );
    }
}
