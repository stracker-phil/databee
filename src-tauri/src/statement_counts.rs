//! How many statements the object view, the palette, the readout and the Project page's coverage
//! send to the server on `wp_test` with the built-in presets, on every fixture server
//! (docs/database.md, "Statement counts"). The schema is loaded first, as it is once per session.

use std::time::Duration;

use tempfile::{TempDir, tempdir};

use crate::document;
use crate::query::Ctx;
use crate::rules::catalog::PresetCatalog;
use crate::rules::coverage::reference_coverage;
use crate::rules::effective::RuleSources;
use crate::rules::overview;
use crate::rules::project::{ProjectsStore, WatchedValue};
use crate::schema;
use crate::session::Session;
use crate::test_support::{Counting, StatementLog, connect_to, servers, spec};
use crate::value::{KeyPart, RowKey};

const TIMEOUT: Duration = Duration::from_secs(10);
const DATABASE: &str = "wp_test";
const ORDER: &str = "databee://{slug}/wp_test/wp_wc_orders/4711";

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

struct Counted {
    server: String,
    slug: String,
    session: Session,
    log: StatementLog,
}

impl Counted {
    fn order(&self) -> String {
        ORDER.replace("{slug}", &self.slug)
    }

    fn assert_count(&self, what: &str, expected: usize) {
        let statements = self.log.take();
        assert_eq!(
            statements.len(),
            expected,
            "{}: {what}: {statements:#?}",
            self.server
        );
    }
}

/// A session on every fixture server whose schema of `wp_test` is loaded, with an empty log.
async fn counted_sessions() -> Vec<Counted> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        let slug = format!("it-{port}");
        let db = connect_to(&host, port, DATABASE).await;
        let (counting, log) = Counting::new(Box::new(db));
        let session = Session::new(
            "it".into(),
            spec(&host, port, DATABASE),
            slug.clone(),
            Box::new(counting),
        );
        schema::database(&session, DATABASE).await.unwrap();
        log.take();
        out.push(Counted {
            server: format!("{host}:{port}"),
            slug,
            session,
            log,
        });
    }
    out
}

#[tokio::test]
#[ignore]
async fn the_document_of_order_4711_runs_18_statements() {
    let rig = Rig::new();
    for counted in counted_sessions().await {
        document::open(
            &counted.session,
            &counted.order(),
            None,
            rig.sources(),
            TIMEOUT,
        )
        .await
        .unwrap();
        counted.assert_count("document", 18);
    }
}

#[tokio::test]
#[ignore]
async fn the_peek_of_order_4711_runs_1_statement() {
    let rig = Rig::new();
    for counted in counted_sessions().await {
        let address = document::parse_address(&counted.session, &counted.order()).unwrap();
        let ctx = Ctx::load(&counted.session, DATABASE, rig.sources(), TIMEOUT)
            .await
            .unwrap();
        document::peek::peek(&ctx, &address).await.unwrap();
        counted.assert_count("peek", 1);
    }
}

#[tokio::test]
#[ignore]
async fn palette_searches_for_an_id_and_a_value_run_43_and_16_statements() {
    let rig = Rig::new();
    for counted in counted_sessions().await {
        for (query, expected) in [("#4711", 43), ("=sandbox", 16)] {
            crate::search::search(&counted.session, DATABASE, query, rig.sources(), TIMEOUT)
                .await
                .unwrap();
            counted.assert_count(query, expected);
        }
    }
}

#[tokio::test]
#[ignore]
async fn the_readout_runs_20_statements() {
    let rig = Rig::new();
    for counted in counted_sessions().await {
        crate::readout::readout(&counted.session, DATABASE, rig.sources(), &[], TIMEOUT).await;
        counted.assert_count("readout", 20);
    }
}

#[tokio::test]
#[ignore]
async fn two_watched_options_add_one_statement_to_the_readout() {
    let rig = Rig::new();
    for counted in counted_sessions().await {
        for name in ["siteurl", "blogname"] {
            let watch = WatchedValue {
                id: name.into(),
                label: None,
                table: "wp_options".into(),
                column: "option_value".into(),
                row: RowKey(vec![KeyPart {
                    column: "option_name".into(),
                    value: name.into(),
                }]),
                path: Vec::new(),
            };
            rig.projects
                .watch(&counted.slug, DATABASE, watch)
                .await
                .unwrap();
        }

        crate::readout::readout(&counted.session, DATABASE, rig.sources(), &[], TIMEOUT).await;

        counted.assert_count("readout with two watched options", 21);
    }
}

#[tokio::test]
#[ignore]
async fn the_coverage_of_the_40_project_references_runs_43_statements() {
    let rig = Rig::new();
    for counted in counted_sessions().await {
        let session = &counted.session;
        let rows = overview::for_database(session, DATABASE, rig.sources())
            .await
            .unwrap();
        let references: Vec<_> = rows
            .groups
            .iter()
            .flat_map(|g| &g.references)
            .filter_map(|r| r.reference.clone())
            .collect();
        for reference in &references {
            reference_coverage(session, DATABASE, reference, rig.sources(), TIMEOUT)
                .await
                .unwrap();
        }
        assert_eq!(references.len(), 40, "{}", counted.server);
        counted.assert_count("coverage", 43);
    }
}
