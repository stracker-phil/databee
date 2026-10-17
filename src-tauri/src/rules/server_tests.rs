//! The built-in presets (`presets/*.toml`) against the Docker fixtures (`dev/fixture.sql`): where
//! they apply, that every reference they declare resolves on real tables, and what their kind
//! rules decide for real columns. They live in the crate because the session, the schema cache
//! and the lens are private modules.

use std::time::Duration;

use tempfile::{TempDir, tempdir};

use super::catalog::{BUILT_IN, PresetCatalog};
use super::coverage::{Coverage, reference_coverage};
use super::effective::{RuleSources, RulesInfo, rules_info};
use super::kinds::Zone;
use super::preset;
use super::project::ProjectsStore;
use crate::db::{Cell, ColumnMeta, ResultSet};
use crate::lens::{ColumnsRef, Lens, ResolvedReference, TableColumns};
use crate::rules::lens;
use crate::session::Session;
use crate::test_support::{connect_to, servers, spec};
use crate::value::{ColumnKind, ReferenceSource, ValueKind, value_at};

const TIMEOUT: Duration = Duration::from_secs(10);
const ALL_PRESETS: [&str; 3] = ["wordpress", "woocommerce", "action-scheduler"];

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

    async fn info(&self, session: &Session, database: &str) -> RulesInfo {
        rules_info(session, database, self.sources()).await.unwrap()
    }

    async fn lens(&self, session: &Session, database: &str) -> Box<dyn Lens> {
        lens::for_database(session, database, Some(self.sources()))
            .await
            .unwrap()
    }

    async fn outgoing(
        &self,
        session: &Session,
        database: &str,
        table: &str,
    ) -> Vec<ResolvedReference> {
        lens::table_references(session, database, table, Some(self.sources()))
            .await
            .unwrap()
            .outgoing
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

async fn fetch(session: &Session, sql: &str) -> ResultSet {
    session
        .db
        .fetch(sql, TIMEOUT)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e:?}"))
}

fn by_preset(preset: &str, id: &str) -> ReferenceSource {
    ReferenceSource::Preset {
        preset: preset.into(),
        id: id.into(),
    }
}

/// What the fixture holds for a reference's source rows.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Data {
    /// Some rows find their target.
    Resolved,
    /// Rows hold the reference, none finds a target.
    Unresolved,
    /// The source table has no row the reference covers.
    Empty,
    /// A table the reference names is not in the fixture, so the rule is dropped.
    Absent,
}

use Data::{Absent, Empty, Resolved, Unresolved};

/// `(source table, preset, reference id, data)`, tables without prefix.
type Case = (&'static str, &'static str, &'static str, Data);

/// Every reference of the built-in presets, on `wp_test`.
const WP_TEST: &[Case] = &[
    ("posts", "wordpress", "posts.post_author", Resolved),
    ("posts", "wordpress", "posts.post_parent", Resolved),
    (
        "postmeta",
        "wordpress",
        "postmeta.meta_value?meta_key=_thumbnail_id",
        Resolved,
    ),
    (
        "postmeta",
        "wordpress",
        "postmeta.meta_value?meta_key=_edit_last",
        Empty,
    ),
    (
        "comments",
        "wordpress",
        "comments.comment_post_ID",
        Resolved,
    ),
    ("comments", "wordpress", "comments.user_id", Resolved),
    (
        "comments",
        "wordpress",
        "comments.comment_parent",
        Unresolved,
    ),
    (
        "term_relationships",
        "wordpress",
        "term_relationships.object_id",
        Resolved,
    ),
    (
        "term_relationships",
        "wordpress",
        "term_relationships.term_taxonomy_id",
        Resolved,
    ),
    (
        "term_taxonomy",
        "wordpress",
        "term_taxonomy.term_id",
        Resolved,
    ),
    (
        "term_taxonomy",
        "wordpress",
        "term_taxonomy.parent",
        Unresolved,
    ),
    ("links", "wordpress", "links.link_owner", Absent),
    ("blogs", "wordpress", "blogs.site_id", Absent),
    (
        "registration_log",
        "wordpress",
        "registration_log.blog_id",
        Absent,
    ),
    ("postmeta", "wordpress", "postmeta.post_id", Resolved),
    ("usermeta", "wordpress", "usermeta.user_id", Resolved),
    (
        "commentmeta",
        "wordpress",
        "commentmeta.comment_id",
        Resolved,
    ),
    ("termmeta", "wordpress", "termmeta.term_id", Absent),
    ("sitemeta", "wordpress", "sitemeta.site_id", Absent),
    ("blogmeta", "wordpress", "blogmeta.blog_id", Absent),
    (
        "wc_orders",
        "woocommerce",
        "wc_orders.customer_id",
        Resolved,
    ),
    (
        "wc_orders",
        "woocommerce",
        "wc_orders.parent_order_id",
        Resolved,
    ),
    ("wc_orders", "woocommerce", "guest_orders", Unresolved),
    (
        "wc_order_addresses",
        "woocommerce",
        "wc_order_addresses.order_id",
        Resolved,
    ),
    (
        "wc_order_operational_data",
        "woocommerce",
        "wc_order_operational_data.order_id",
        Resolved,
    ),
    (
        "woocommerce_order_items",
        "woocommerce",
        "woocommerce_order_items.order_id->wc_orders.id",
        Resolved,
    ),
    (
        "woocommerce_order_items",
        "woocommerce",
        "woocommerce_order_items.order_id->posts.ID",
        Resolved,
    ),
    (
        "woocommerce_order_itemmeta",
        "woocommerce",
        "woocommerce_order_itemmeta.meta_value?meta_key=[_product_id,_variation_id]",
        Resolved,
    ),
    (
        "comments",
        "woocommerce",
        "comments.comment_post_ID?comment_type=order_note",
        Resolved,
    ),
    (
        "wc_orders_meta",
        "woocommerce",
        "wc_orders_meta.meta_value?meta_key=_ppcp_paypal_order_id",
        Unresolved,
    ),
    (
        "wc_orders_meta",
        "woocommerce",
        "wc_orders_meta.meta_value?meta_key=[_subscription_renewal,_subscription_resubscribe,_subscription_switch]",
        Empty,
    ),
    (
        "postmeta",
        "woocommerce",
        "postmeta.meta_value?meta_key=_customer_user",
        Empty,
    ),
    (
        "postmeta",
        "woocommerce",
        "postmeta.meta_value?meta_key=_ppcp_paypal_order_id",
        Empty,
    ),
    (
        "postmeta",
        "woocommerce",
        "postmeta.meta_value?meta_key=[_subscription_renewal,_subscription_resubscribe,_subscription_switch]",
        Empty,
    ),
    (
        "wc_order_product_lookup",
        "woocommerce",
        "wc_order_product_lookup.order_id",
        Resolved,
    ),
    (
        "wc_order_product_lookup",
        "woocommerce",
        "wc_order_product_lookup.order_item_id",
        Resolved,
    ),
    (
        "wc_order_product_lookup",
        "woocommerce",
        "wc_order_product_lookup.product_id",
        Resolved,
    ),
    (
        "wc_order_product_lookup",
        "woocommerce",
        "wc_order_product_lookup.variation_id",
        Resolved,
    ),
    (
        "wc_order_product_lookup",
        "woocommerce",
        "wc_order_product_lookup.customer_id",
        Absent,
    ),
    (
        "wc_order_stats",
        "woocommerce",
        "wc_order_stats.order_id",
        Absent,
    ),
    (
        "wc_order_stats",
        "woocommerce",
        "wc_order_stats.parent_id",
        Absent,
    ),
    (
        "wc_order_stats",
        "woocommerce",
        "wc_order_stats.customer_id",
        Absent,
    ),
    (
        "wc_customer_lookup",
        "woocommerce",
        "wc_customer_lookup.user_id",
        Absent,
    ),
    (
        "wc_product_meta_lookup",
        "woocommerce",
        "wc_product_meta_lookup.product_id",
        Resolved,
    ),
    (
        "woocommerce_payment_tokens",
        "woocommerce",
        "woocommerce_payment_tokens.user_id",
        Resolved,
    ),
    (
        "woocommerce_sessions",
        "woocommerce",
        "woocommerce_sessions.session_key?session_key=regexp(^[0-9]+$)",
        Resolved,
    ),
    (
        "woocommerce_downloadable_product_permissions",
        "woocommerce",
        "woocommerce_downloadable_product_permissions.order_id",
        Absent,
    ),
    (
        "woocommerce_downloadable_product_permissions",
        "woocommerce",
        "woocommerce_downloadable_product_permissions.product_id",
        Absent,
    ),
    (
        "woocommerce_downloadable_product_permissions",
        "woocommerce",
        "woocommerce_downloadable_product_permissions.user_id",
        Absent,
    ),
    (
        "wc_orders_meta",
        "woocommerce",
        "wc_orders_meta.order_id",
        Resolved,
    ),
    (
        "woocommerce_order_itemmeta",
        "woocommerce",
        "woocommerce_order_itemmeta.order_item_id",
        Resolved,
    ),
    (
        "woocommerce_payment_tokenmeta",
        "woocommerce",
        "woocommerce_payment_tokenmeta.payment_token_id",
        Resolved,
    ),
    (
        "actionscheduler_actions",
        "action-scheduler",
        "actionscheduler_actions.group_id",
        Resolved,
    ),
    (
        "actionscheduler_actions",
        "action-scheduler",
        "actionscheduler_actions.claim_id",
        Absent,
    ),
    (
        "actionscheduler_logs",
        "action-scheduler",
        "actionscheduler_logs.action_id",
        Resolved,
    ),
];

/// Checks each case on `database`, whose tables all carry the prefix `wp_`.
async fn check(rig: &Rig, session: &Session, database: &str, server: &str, cases: &[Case]) {
    for &(table, preset, id, data) in cases {
        let label = format!("{server}: {database} {preset} {id}");
        let wanted = by_preset(preset, id);
        let references = rig
            .outgoing(session, database, &format!("wp_{table}"))
            .await;
        let Some(reference) = references.iter().find(|r| r.by == wanted) else {
            assert_eq!(data, Absent, "{label}: not among {references:?}");
            continue;
        };
        let coverage = reference_coverage(session, database, reference, rig.sources(), TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{label}: {e:?}"));
        match data {
            Resolved => assert!(coverage.resolved > 0, "{label}: {coverage:?}"),
            Unresolved => assert!(
                coverage.total > 0 && coverage.resolved == 0,
                "{label}: {coverage:?}"
            ),
            Empty => assert_eq!(coverage.total, 0, "{label}: {coverage:?}"),
            Absent => panic!("{label}: the reference exists although a table is missing"),
        }
    }
}

#[test]
fn the_fixture_cases_name_every_reference_of_the_built_in_presets() {
    let mut declared: Vec<(String, String)> = Vec::new();
    for (file, src) in BUILT_IN {
        let preset = preset::parse(file, src).unwrap_or_else(|e| panic!("{e}"));
        for reference in &preset.references {
            declared.push((preset.name.clone(), reference.id.clone()));
        }
        for meta in &preset.meta {
            if let Some(fk) = &meta.fk {
                declared.push((preset.name.clone(), format!("{}.{fk}", meta.table)));
            }
        }
    }
    let mut cased: Vec<(String, String)> = WP_TEST
        .iter()
        .map(|(_, preset, id, _)| (preset.to_string(), id.to_string()))
        .collect();
    declared.sort();
    cased.sort();

    assert_eq!(cased, declared);
}

#[test]
fn the_fixture_cases_start_each_id_with_its_source_table() {
    for (table, preset, id, _) in WP_TEST {
        assert!(
            id.starts_with(&format!("{table}.")) || *id == "guest_orders",
            "{preset} {id}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn the_built_in_presets_apply_where_their_marker_tables_exist() {
    let cases: [(&str, &str, &[&str]); 5] = [
        ("wp_test", "wp_", &ALL_PRESETS),
        ("wp_legacy", "wp_", &ALL_PRESETS),
        ("wp_custom", "xy_", &["wordpress"]),
        ("wp_multi", "wp_", &["wordpress"]),
        ("databee_test", "", &[]),
    ];
    let rig = Rig::new();
    for (host, port) in servers() {
        for (database, prefix, applied) in cases {
            let session = session_on(&host, port, database).await;

            let info = rig.info(&session, database).await;

            let label = format!("{host}:{port}: {database}");
            assert_eq!(info.prefix, prefix, "{label}");
            assert_eq!(info.network_prefix, prefix, "{label}");
            assert_eq!(info.applied_presets, applied, "{label}");
            assert_eq!(info.available_presets, ALL_PRESETS, "{label}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn a_subsite_project_resolves_the_users_table_under_the_network_prefix() {
    for (host, port) in servers() {
        let rig = Rig::new();
        rig.projects
            .update(&slug(port), "wp_multi", |p| {
                p.prefix = Some("wp_2_".into());
                p.network_prefix = Some("wp_".into());
                p.presets = Some(vec!["wordpress".into()]);
                Ok(())
            })
            .await
            .unwrap();
        let session = session_on(&host, port, "wp_multi").await;
        let server = format!("{host}:{port}");

        let info = rig.info(&session, "wp_multi").await;
        let references = rig.outgoing(&session, "wp_multi", "wp_2_posts").await;

        assert_eq!(info.prefix, "wp_2_", "{server}");
        assert_eq!(info.network_prefix, "wp_", "{server}");
        assert_eq!(info.applied_presets, ["wordpress"], "{server}");
        let author = references
            .iter()
            .find(|r| r.by == by_preset("wordpress", "posts.post_author"))
            .unwrap_or_else(|| panic!("{server}: {references:?}"));
        assert_eq!(
            author.to,
            ColumnsRef::new("wp_users", vec!["ID".into()]),
            "{server}"
        );
        let coverage = reference_coverage(&session, "wp_multi", author, rig.sources(), TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));
        let rows = coverage
            .rows
            .clone()
            .unwrap_or_else(|| panic!("{server}: {coverage:?}"));
        assert_eq!(
            Coverage {
                rows: None,
                ..coverage
            },
            Coverage {
                total: 1,
                resolved: 1,
                ..Coverage::default()
            },
            "{server}"
        );
        assert!(
            rows.unresolved
                .contains("`wp_2_posts`.`post_author` IS NULL")
                && rows.unresolved.contains("`wp_multi`.`wp_users` AS t"),
            "{server}: {}",
            rows.unresolved
        );
        assert!(
            rows.zero_or_null
                .contains("`wp_2_posts`.`post_author` IS NULL"),
            "{server}: {}",
            rows.zero_or_null
        );
    }
}

#[tokio::test]
#[ignore]
async fn every_preset_reference_resolves_on_the_wordpress_fixture() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;

        check(
            &rig,
            &session,
            "wp_test",
            &format!("{host}:{port}"),
            WP_TEST,
        )
        .await;
    }
}

#[tokio::test]
#[ignore]
async fn the_network_references_resolve_on_the_multisite_fixture() {
    let cases: &[Case] = &[
        ("posts", "wordpress", "posts.post_author", Resolved),
        ("blogs", "wordpress", "blogs.site_id", Resolved),
        ("sitemeta", "wordpress", "sitemeta.site_id", Resolved),
        (
            "registration_log",
            "wordpress",
            "registration_log.blog_id",
            Absent,
        ),
        ("blogmeta", "wordpress", "blogmeta.blog_id", Absent),
    ];
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_multi").await;

        check(&rig, &session, "wp_multi", &format!("{host}:{port}"), cases).await;
    }
}

#[tokio::test]
#[ignore]
async fn the_posts_shaped_orders_resolve_on_the_legacy_fixture() {
    let cases: &[Case] = &[
        (
            "postmeta",
            "woocommerce",
            "postmeta.meta_value?meta_key=_customer_user",
            Resolved,
        ),
        (
            "woocommerce_order_items",
            "woocommerce",
            "woocommerce_order_items.order_id->posts.ID",
            Resolved,
        ),
        (
            "woocommerce_order_items",
            "woocommerce",
            "woocommerce_order_items.order_id->wc_orders.id",
            Absent,
        ),
    ];
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_legacy").await;

        check(
            &rig,
            &session,
            "wp_legacy",
            &format!("{host}:{port}"),
            cases,
        )
        .await;
    }
}

fn annotate(lens: &dyn Lens, table: &str, mut result: ResultSet) -> ResultSet {
    result.kinds = lens.kinds(table, &result.columns);
    let cells = {
        let columns = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.name.as_str()))
            .collect();
        lens.cells(&TableColumns { table, columns }, &result)
    };
    result.cell_kinds = cells.kinds;
    result.styles = cells.styles;
    result
}

fn meta_page(rows: &[(&str, &str)]) -> ResultSet {
    let column = |name: &str| ColumnMeta {
        name: name.into(),
        type_name: "TEXT".into(),
        origin: None,
    };
    ResultSet {
        columns: vec![column("meta_key"), column("meta_value")],
        rows: rows
            .iter()
            .map(|(key, value)| vec![Cell::Text((*key).into()), Cell::Text((*value).into())])
            .collect(),
        ..ResultSet::default()
    }
}

#[tokio::test]
#[ignore]
async fn kind_rules_decide_the_kind_of_real_columns() {
    let money = || ColumnKind::Money {
        currency: vec!["currency".into()],
    };
    let amount = || ColumnKind::Money {
        currency: Vec::new(),
    };
    let zone = |zone| ColumnKind::Timestamp { zone: Some(zone) };
    let cases = [
        ("wp_users", "user_pass", ColumnKind::Secret),
        ("wp_users", "user_activation_key", ColumnKind::Secret),
        ("wp_users", "user_registered", zone(Zone::Utc)),
        ("wp_posts", "post_date", zone(Zone::Local)),
        ("wp_posts", "post_date_gmt", zone(Zone::Utc)),
        ("wp_comments", "comment_date", zone(Zone::Local)),
        ("wp_comments", "comment_date_gmt", zone(Zone::Utc)),
        ("wp_comments", "comment_approved", ColumnKind::Enum),
        ("wp_term_taxonomy", "taxonomy", ColumnKind::Enum),
        ("wp_wc_orders", "status", ColumnKind::Enum),
        ("wp_wc_orders", "total_amount", money()),
        ("wp_wc_orders", "tax_amount", money()),
        (
            "wp_wc_order_operational_data",
            "order_key",
            ColumnKind::Secret,
        ),
        (
            "wp_wc_order_operational_data",
            "shipping_total_amount",
            money(),
        ),
        (
            "wp_wc_order_operational_data",
            "discount_tax_amount",
            money(),
        ),
        (
            "wp_wc_order_product_lookup",
            "product_net_revenue",
            amount(),
        ),
        (
            "wp_wc_order_product_lookup",
            "product_gross_revenue",
            amount(),
        ),
        ("wp_actionscheduler_logs", "log_date_gmt", zone(Zone::Utc)),
        (
            "wp_actionscheduler_logs",
            "log_date_local",
            zone(Zone::Local),
        ),
    ];
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;
        let lens = rig.lens(&session, "wp_test").await;
        for (table, column, expected) in &cases {
            let sql = format!("SELECT `{column}` FROM `wp_test`.`{table}` LIMIT 1");
            let result = fetch(&session, &sql).await;

            let kinds = lens.kinds(table, &result.columns);

            assert_eq!(kinds, [expected.clone()], "{host}:{port}: {table}.{column}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn a_secret_kind_masks_the_password_and_the_activation_key() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;
        let lens = rig.lens(&session, "wp_test").await;
        let sql = "SELECT ID, user_pass, user_activation_key FROM wp_test.wp_users ORDER BY ID";
        let result = annotate(lens.as_ref(), "wp_users", fetch(&session, sql).await);

        let kinds = |row: usize| -> Vec<ValueKind> {
            (1..3)
                .map(|col| value_at(&result, row, col, None).kind)
                .collect()
        };

        let server = format!("{host}:{port}");
        assert_eq!(kinds(0), [ValueKind::Secret, ValueKind::Empty], "{server}");
        assert_eq!(kinds(1), [ValueKind::Secret, ValueKind::Secret], "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn money_takes_its_currency_from_the_currency_column_of_the_row() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;
        let lens = rig.lens(&session, "wp_test").await;
        let orders =
            "SELECT total_amount, tax_amount, currency FROM wp_test.wp_wc_orders WHERE id = 4711";
        let orders = annotate(lens.as_ref(), "wp_wc_orders", fetch(&session, orders).await);
        let shipping = "SELECT shipping_total_amount FROM wp_test.wp_wc_order_operational_data";
        let shipping = annotate(
            lens.as_ref(),
            "wp_wc_order_operational_data",
            fetch(&session, shipping).await,
        );

        let server = format!("{host}:{port}");
        for col in 0..2 {
            let ValueKind::Money { currency } = value_at(&orders, 0, col, None).kind else {
                panic!("{server}: column {col} is not money");
            };
            let currency = currency.unwrap_or_else(|| panic!("{server}: no currency"));
            assert_eq!(
                currency.raw.as_text(),
                Some("EUR"),
                "{server}: column {col}"
            );
        }
        assert_eq!(
            value_at(&shipping, 0, 0, None).kind,
            ValueKind::Money { currency: None },
            "{server}: no currency column in the row"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_local_timestamp_key_keeps_its_stored_text_and_names_no_moment() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;
        let lens = rig.lens(&session, "wp_test").await;
        let sql = "SELECT meta_key, meta_value FROM wp_test.wp_wc_orders_meta \
                   WHERE meta_key = '_wc_order_attribution_session_start_time'";
        let result = annotate(
            lens.as_ref(),
            "wp_wc_orders_meta",
            fetch(&session, sql).await,
        );

        let value = value_at(&result, 0, 1, None);

        let server = format!("{host}:{port}");
        assert_eq!(
            value.kind,
            ValueKind::Timestamp {
                utc: None,
                zone: Some(Zone::Local)
            },
            "{server}"
        );
        let stored = value.raw.as_text().unwrap_or_default();
        assert!(
            chrono::NaiveDateTime::parse_from_str(stored, "%Y-%m-%d %H:%M:%S").is_ok(),
            "{server}: {stored:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn sent_marker_keys_decode_as_unix_seconds() {
    let rig = Rig::new();
    for (host, port) in servers() {
        let session = session_on(&host, port, "wp_test").await;
        let lens = rig.lens(&session, "wp_test").await;
        let page = annotate(
            lens.as_ref(),
            "wp_wc_orders_meta",
            meta_page(&[
                ("_wc_email_sent", "1700000000"),
                ("_wc_email_sending", "1700000000"),
            ]),
        );

        let kinds: Vec<ValueKind> = (0..2)
            .map(|row| value_at(&page, row, 1, None).kind)
            .collect();

        assert_eq!(
            kinds,
            [
                ValueKind::Timestamp {
                    utc: Some(1_700_000_000),
                    zone: Some(Zone::Unix)
                },
                ValueKind::Text
            ],
            "{host}:{port}"
        );
    }
}
