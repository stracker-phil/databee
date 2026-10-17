//! Documents of the Docker fixtures (`dev/fixture.sql`) with the built-in presets: the own row,
//! meta, incoming references, patterns and `events` of real rows on MariaDB and MySQL. Rows are
//! only read. They live in the crate because the session, the schema cache and the lens are
//! private modules.

use std::time::Duration;

use tempfile::{TempDir, tempdir};

use super::{Document, Group, GroupLink, GroupRow, GroupShape, open};
use crate::address::{Address, Target};
use crate::rules::catalog::PresetCatalog;
use crate::rules::effective::RuleSources;
use crate::rules::kinds::Zone;
use crate::rules::project::ProjectsStore;
use crate::rules::types::{ColumnRef, Conditions, Reference};
use crate::session::Session;
use crate::test_support::{connect_to, servers, spec};
use crate::value::{KeyPart, Provenance, RowKey, Value, ValueKind, utc_seconds};

const TIMEOUT: Duration = Duration::from_secs(10);
const ORDER: &str = "databee://{slug}/wp_test/wp_wc_orders/4711";
const LEGACY_ORDER: &str = "databee://{slug}/wp_legacy/wp_posts/501";
const AUTHOR: &str = "databee://{slug}/databee_test/authors/1";
const PARENT: &str = "databee://{slug}/databee_test/cov_parents/1";

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

/// The document of `text` on every fixture server, with the server's name. `{slug}` in `text`
/// is the connection slug of the server.
async fn documents(rig: &Rig, database: &str, text: &str) -> Vec<(String, Document)> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        let server = format!("{host}:{port}");
        let session = session_on(&host, port, database).await;
        let text = text.replace("{slug}", &slug(port));
        let document = open(
            &session,
            &text,
            Some(database.to_string()),
            rig.sources(),
            TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("{server}: {text}: {e:?}"));
        out.push((server, document));
    }
    out
}

fn row_address(port_slug: &str, database: &str, table: &str, key: &str) -> Address {
    Address::new(
        port_slug,
        database,
        Target::Row {
            table: table.into(),
            key: key.into(),
        },
    )
}

fn port_of(server: &str) -> u16 {
    server.rsplit_once(':').unwrap().1.parse().unwrap()
}

fn field<'a>(row: &'a GroupRow, name: &str) -> &'a Value {
    row.fields
        .iter()
        .find(|(column, _)| column == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("no field {name} in {:?}", row.fields))
}

fn text(value: &Value) -> Option<&str> {
    value.raw.as_text()
}

fn provenance(table: &str, column: &str, key_column: &str, key: &str) -> Provenance {
    Provenance {
        table: table.into(),
        column: column.into(),
        row: RowKey(vec![KeyPart {
            column: key_column.into(),
            value: key.into(),
        }]),
    }
}

/// The first part of each row's key, in row order.
fn keys(group: &Group) -> Vec<String> {
    group
        .rows
        .iter()
        .map(|row| row.key.as_ref().unwrap().0[0].value.clone())
        .collect()
}

fn sorted_keys(group: &Group) -> Vec<String> {
    let mut keys = keys(group);
    keys.sort();
    keys
}

fn incoming_index(doc: &Document, table: &str, label: Option<&str>) -> Option<usize> {
    doc.groups.iter().position(|g| {
        let GroupLink::Incoming { label: found, .. } = &g.via else {
            return false;
        };
        g.table == table && label.is_none_or(|wanted| found.as_deref() == Some(wanted))
    })
}

fn incoming<'a>(server: &str, doc: &'a Document, table: &str, label: Option<&str>) -> &'a Group {
    let index = incoming_index(doc, table, label)
        .unwrap_or_else(|| panic!("{server}: no incoming group {table} {label:?}"));
    &doc.groups[index]
}

fn group_by_id<'a>(server: &str, doc: &'a Document, id: &str) -> &'a Group {
    doc.groups
        .iter()
        .find(|g| g.id == id)
        .unwrap_or_else(|| panic!("{server}: no group {id}"))
}

fn pattern<'a>(server: &str, doc: &'a Document, rule: &str) -> &'a Group {
    doc.groups
        .iter()
        .find(|g| matches!(&g.via, GroupLink::Pattern { rule: r, .. } if r == rule))
        .unwrap_or_else(|| panic!("{server}: no pattern {rule}"))
}

fn meta_value<'a>(server: &str, group: &'a Group, key: &str) -> &'a Value {
    let row = group
        .rows
        .iter()
        .find(|row| text(&row.fields[0].1) == Some(key))
        .unwrap_or_else(|| panic!("{server}: no meta key {key}"));
    &row.fields[1].1
}

fn is_reference(value: &Value, exists: Option<bool>) -> bool {
    matches!(&value.kind, ValueKind::Reference { exists: found, .. } if *found == exists)
}

fn money_currency(value: &Value) -> Option<&Value> {
    match &value.kind {
        ValueKind::Money { currency } => currency.as_deref(),
        _ => None,
    }
}

fn moment(value: &Value) -> i64 {
    let ValueKind::Timestamp { utc, .. } = value.kind else {
        panic!("not a timestamp: {value:?}");
    };
    utc.or_else(|| utc_seconds(text(value)?, Some(Zone::Utc)))
        .unwrap_or_else(|| panic!("no moment: {value:?}"))
}

#[tokio::test]
#[ignore]
async fn an_hpos_order_names_its_entity_and_lists_all_its_columns_first() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let slug = slug(port_of(&server));

        assert_eq!(
            doc.address,
            row_address(&slug, "wp_test", "wp_wc_orders", "4711"),
            "{server}"
        );
        assert_eq!(doc.entity.as_deref(), Some("order"), "{server}");
        assert!(doc.alternatives.is_empty(), "{server}");
        let own = &doc.groups[0];
        assert_eq!(own.id, "own", "{server}");
        assert_eq!(own.via, GroupLink::Own, "{server}");
        assert_eq!(own.shape, GroupShape::Fields, "{server}");
        assert_eq!(own.table, "wp_wc_orders", "{server}");
        assert_eq!(own.columns.len(), 14, "{server}: {:?}", own.columns);
        assert_eq!(own.total, 1, "{server}");
        assert_eq!(own.rows.len(), 1, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn the_order_header_shows_the_summary_columns_with_their_source_rows() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let names: Vec<&str> = doc.header.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "type",
                "status",
                "payment_method",
                "total_amount",
                "date_created_gmt"
            ],
            "{server}"
        );
        for (name, value) in &doc.header {
            assert_eq!(
                value.source,
                Some(provenance("wp_wc_orders", name, "id", "4711")),
                "{server}: {name}"
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn order_amounts_take_their_currency_from_the_currency_column() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let own = &doc.groups[0].rows[0];
        let total = field(own, "total_amount");

        let currency = money_currency(total)
            .unwrap_or_else(|| panic!("{server}: no currency on {:?}", total.kind));

        assert_eq!(text(currency), Some("EUR"), "{server}");
        assert_eq!(
            currency.source,
            Some(provenance("wp_wc_orders", "currency", "id", "4711")),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn an_existing_customer_is_a_checked_reference_and_a_missing_parent_is_no_reference() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let own = &doc.groups[0].rows[0];

        assert!(
            is_reference(field(own, "customer_id"), Some(true)),
            "{server}: {:?}",
            field(own, "customer_id").kind
        );
        assert_eq!(
            field(own, "parent_order_id").kind,
            ValueKind::NoReference,
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn the_meta_group_lists_the_meta_rows_in_stored_order() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let meta = group_by_id(&server, &doc, "wp_wc_orders_meta");

        assert_eq!(meta.shape, GroupShape::Meta, "{server}");
        assert_eq!(meta.table, "wp_wc_orders_meta", "{server}");
        assert_eq!(meta.columns, ["meta_key", "meta_value"], "{server}");
        assert_eq!(meta.total, 8, "{server}");
        assert!(!meta.capped, "{server}");
        assert_eq!(
            keys(meta),
            (1..=8).map(|id| id.to_string()).collect::<Vec<_>>(),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn incoming_groups_follow_the_order_of_the_rules() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let order = [
            incoming_index(&doc, "wp_comments", Some("notes")),
            incoming_index(&doc, "wp_wc_orders_meta", None),
            incoming_index(&doc, "wp_wc_orders", Some("children")),
            incoming_index(&doc, "wp_wc_order_addresses", None),
            incoming_index(&doc, "wp_wc_order_operational_data", None),
            incoming_index(&doc, "wp_woocommerce_order_items", Some("items")),
            incoming_index(&doc, "wp_wc_order_product_lookup", None),
        ];

        let positions: Vec<usize> = order
            .iter()
            .map(|p| p.unwrap_or_else(|| panic!("{server}: missing group in {order:?}")))
            .collect();
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "{server}: {positions:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn incoming_groups_hold_the_rows_that_point_at_the_order() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let notes = incoming(&server, &doc, "wp_comments", Some("notes"));
        let children = incoming(&server, &doc, "wp_wc_orders", Some("children"));
        let addresses = incoming(&server, &doc, "wp_wc_order_addresses", None);
        let operational = incoming(&server, &doc, "wp_wc_order_operational_data", None);
        let lookup = incoming(&server, &doc, "wp_wc_order_product_lookup", None);
        let renewals = incoming(&server, &doc, "wp_wc_orders_meta", None);

        assert_eq!(keys(notes), ["1"], "{server}");
        assert_eq!(keys(children), ["4712", "4714"], "{server}");
        assert_eq!(children.total, 2, "{server}");
        assert_eq!(sorted_keys(addresses), ["1", "2"], "{server}");
        assert_eq!(operational.total, 1, "{server}");
        assert_eq!(lookup.total, 1, "{server}");
        assert_eq!(renewals.total, 0, "{server}");
        assert!(renewals.rows.is_empty(), "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn groups_whose_rule_does_not_apply_are_left_out() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let guest_orders = incoming_index(&doc, "wp_wc_orders", Some("same billing_email"));
        let meta_owner = doc.groups.iter().find(|g| {
            matches!(
                &g.via,
                GroupLink::Incoming { reference, .. }
                    if reference.from.table == "wp_wc_orders_meta"
                        && reference.from.columns == ["order_id"]
            )
        });

        assert_eq!(guest_orders, None, "{server}");
        assert_eq!(meta_owner, None, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn order_items_pivot_their_meta_keys_into_columns() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let items = incoming(&server, &doc, "wp_woocommerce_order_items", Some("items"));
        let item = |id: &str| {
            items
                .rows
                .iter()
                .find(|row| text(field(row, "order_item_id")) == Some(id))
                .unwrap_or_else(|| panic!("{server}: no item {id}"))
        };

        assert_eq!(items.total, 2, "{server}");
        assert_eq!(items.rows.len(), 2, "{server}");
        assert_eq!(
            items.columns,
            [
                "order_item_id",
                "order_item_type",
                "order_item_name",
                "_qty",
                "_line_total",
                "_product_id",
                "_variation_id"
            ],
            "{server}"
        );
        let first = item("1");
        assert_eq!(text(field(first, "_qty")), Some("2"), "{server}");
        assert_eq!(text(field(first, "_line_total")), Some("45.00"), "{server}");
        let product = field(first, "_product_id");
        assert_eq!(text(product), Some("123"), "{server}");
        assert!(
            is_reference(product, Some(true)),
            "{server}: {:?}",
            product.kind
        );
        assert_eq!(
            product.source,
            Some(provenance(
                "wp_woocommerce_order_itemmeta",
                "meta_value",
                "meta_id",
                "1"
            )),
            "{server}"
        );
        assert_eq!(
            field(first, "_variation_id").kind,
            ValueKind::NoReference,
            "{server}"
        );
        let second = item("2");
        for key in ["_qty", "_line_total", "_product_id", "_variation_id"] {
            assert_eq!(field(second, key).kind, ValueKind::Null, "{server}: {key}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn pattern_groups_select_rows_by_the_order_id_as_a_whole_number() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let log = pattern(&server, &doc, "woocommerce_log");
        let actions = pattern(&server, &doc, "actionscheduler_actions");

        assert_eq!(log.shape, GroupShape::Rows, "{server}");
        assert_eq!(log.table, "wp_woocommerce_log", "{server}");
        assert_eq!(log.total, 3, "{server}");
        assert_eq!(sorted_keys(log), ["1", "2", "5"], "{server}");
        assert_eq!(actions.table, "wp_actionscheduler_actions", "{server}");
        assert_eq!(keys(actions), ["1"], "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn events_come_last_with_every_timestamp_in_ascending_order() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", ORDER).await {
        let events = doc.groups.last().unwrap();

        assert_eq!(events.id, "events", "{server}");
        assert_eq!(events.via, GroupLink::Events, "{server}");
        assert_eq!(events.table, "", "{server}");
        assert_eq!(events.total, 11, "{server}");
        assert_eq!(events.rows.len(), 11, "{server}");
        let moments: Vec<i64> = events
            .rows
            .iter()
            .map(|row| {
                assert_eq!(row.fields.len(), 1, "{server}: {row:?}");
                assert!(row.fields[0].1.source.is_some(), "{server}: {row:?}");
                moment(&row.fields[0].1)
            })
            .collect();
        assert!(
            moments.windows(2).all(|pair| pair[0] <= pair[1]),
            "{server}: {moments:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_user_password_is_a_secret_value() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", "databee://{slug}/wp_test/wp_users/12").await {
        let own = &doc.groups[0].rows[0];

        assert_eq!(field(own, "user_pass").kind, ValueKind::Secret, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn the_synced_post_of_an_order_has_the_comments_and_items_that_point_at_it() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", "databee://{slug}/wp_test/wp_posts/4711").await
    {
        let comments = incoming(&server, &doc, "wp_comments", None);
        let items = incoming(&server, &doc, "wp_woocommerce_order_items", Some("items"));

        assert_eq!(doc.entity.as_deref(), Some("order"), "{server}");
        assert_eq!(keys(comments), ["2"], "{server}");
        assert_eq!(items.total, 2, "{server}");
        assert_eq!(items.rows.len(), 2, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn the_order_shorthand_opens_the_hpos_row_and_lists_the_synced_post_as_alternative() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_test", "order 4711").await {
        let slug = slug(port_of(&server));

        assert_eq!(
            doc.address,
            row_address(&slug, "wp_test", "wp_wc_orders", "4711"),
            "{server}"
        );
        assert_eq!(
            doc.alternatives,
            [row_address(&slug, "wp_test", "wp_posts", "4711")],
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_posts_shaped_order_reads_its_amount_and_currency_from_post_meta() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_legacy", LEGACY_ORDER).await {
        let total = &doc
            .header
            .iter()
            .find(|(name, _)| name == "_order_total")
            .unwrap_or_else(|| panic!("{server}: no _order_total in {:?}", doc.header))
            .1;

        assert_eq!(doc.entity.as_deref(), Some("order"), "{server}");
        assert_eq!(text(total), Some("30.00"), "{server}");
        assert_eq!(
            total.source,
            Some(provenance("wp_postmeta", "meta_value", "meta_id", "4")),
            "{server}"
        );
        let currency = money_currency(total)
            .unwrap_or_else(|| panic!("{server}: no currency on {:?}", total.kind));
        assert_eq!(text(currency), Some("USD"), "{server}");
        assert_eq!(
            currency.source,
            Some(provenance("wp_postmeta", "meta_value", "meta_id", "5")),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_posts_shaped_order_lists_its_meta_and_what_points_at_it() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_legacy", LEGACY_ORDER).await {
        let meta = group_by_id(&server, &doc, "wp_postmeta");
        let items = incoming(&server, &doc, "wp_woocommerce_order_items", Some("items"));
        let children = incoming(&server, &doc, "wp_posts", None);
        let comments = incoming(&server, &doc, "wp_comments", None);
        let log = pattern(&server, &doc, "woocommerce_log");
        let actions = pattern(&server, &doc, "actionscheduler_actions");

        assert_eq!(meta.total, 11, "{server}");
        assert_eq!(
            keys(meta),
            (3..=13).map(|id| id.to_string()).collect::<Vec<_>>(),
            "{server}"
        );
        assert_eq!(items.total, 1, "{server}");
        assert_eq!(keys(children), ["504"], "{server}");
        assert_eq!(comments.total, 2, "{server}");
        assert_eq!(keys(log), ["1"], "{server}");
        assert_eq!(actions.total, 0, "{server}");
        assert!(
            is_reference(meta_value(&server, meta, "_customer_user"), Some(true)),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn the_order_shorthand_opens_the_post_of_a_posts_shaped_order_without_alternatives() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "wp_legacy", "order 501").await {
        let slug = slug(port_of(&server));

        assert_eq!(
            doc.address,
            row_address(&slug, "wp_legacy", "wp_posts", "501"),
            "{server}"
        );
        assert!(doc.alternatives.is_empty(), "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn a_declared_foreign_key_adds_an_incoming_group_without_any_rule() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "databee_test", AUTHOR).await {
        let books = group_by_id(&server, &doc, "books_author");

        assert_eq!(doc.entity, None, "{server}");
        assert!(doc.header.is_empty(), "{server}");
        assert_eq!(books.table, "books", "{server}");
        assert_eq!(books.total, 1, "{server}");
        assert_eq!(keys(books), ["1"], "{server}");
        assert_eq!(
            text(field(&books.rows[0], "title")),
            Some("Notes"),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn a_same_name_column_adds_an_inferred_incoming_group() {
    let rig = Rig::new();
    for (server, doc) in documents(&rig, "databee_test", PARENT).await {
        let children = group_by_id(&server, &doc, "inferred:cov_children.parent_id");

        assert_eq!(children.table, "cov_children", "{server}");
        assert_eq!(children.total, 1, "{server}");
        assert_eq!(keys(children), ["1"], "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn a_project_reference_adds_an_incoming_group_with_its_label_and_columns() {
    for (host, port) in servers() {
        let server = format!("{host}:{port}");
        let rig = Rig::new();
        rig.projects
            .update(&slug(port), "databee_test", |project| {
                project.references.push(Reference {
                    id: "cov_children.parent_id".into(),
                    from: ColumnRef::try_from("cov_children.parent_id".to_string()).unwrap(),
                    when: Conditions::new(),
                    to: ColumnRef::try_from("cov_parents.parent_id".to_string()).unwrap(),
                    to_when: Conditions::new(),
                    label: Some("Children".into()),
                    columns: vec!["child_id".into(), "parent_id".into()],
                });
                Ok(())
            })
            .await
            .unwrap();
        let session = session_on(&host, port, "databee_test").await;
        let address = format!("databee://{}/databee_test/cov_parents/1", slug(port));

        let doc = open(&session, &address, None, rig.sources(), TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));

        let children = incoming(&server, &doc, "cov_children", Some("Children"));
        assert_eq!(children.columns, ["child_id", "parent_id"], "{server}");
        assert_eq!(children.total, 1, "{server}");
        assert_eq!(keys(children), ["1"], "{server}");
        assert!(
            doc.groups
                .iter()
                .all(|g| g.id != "inferred:cov_children.parent_id"),
            "{server}: the project rule replaces the inferred reference"
        );
    }
}
