//! Integration tests against real servers; every one is `#[ignore]`
//! (`cargo test --lib -- --ignored wp::`).
//!
//! - The lens against the Docker fixtures (`bun run test:db`): reads go to `wp_test` (HPOS with
//!   sync), edits to its copy `wp_write`, and `wp_legacy` is a posts-based store (HPOS off).
//! - Read-only checks against a real WordPress database of your own, in `mod real_data`. They panic
//!   unless `DATABEE_WP_TEST=mysql://user:pass@host:port/db` is set, so `bun run test:db` skips them
//!   (`--skip real_data`). Run them by name:
//!
//!   DATABEE_WP_TEST=mysql://root:root@127.0.0.1:52958/db cargo test --lib -- --ignored --nocapture real_data
//!
//! Every test that writes owns its rows in `wp_write` (see `dev/fixture.sql`), so tests never share one.

use std::collections::HashMap;
use std::time::Duration;

use crate::db::changes::{ChangeRequest, Plan, plan as plan_row_change};
use crate::db::mysql::MySqlSession;
use crate::db::{Cell, Database};
use crate::error::AppError;
use crate::test_support::sessions;
use crate::wp::changes::{WpChangeRequest, plan as plan_wp_change};
use crate::wp::context::{HposMode, SiteInfo, SiteZone};
use crate::wp::dashboard::{Granularity, action_histogram, browser, dashboard};
use crate::wp::detail::{EntityView, EventKind, EventLevel, entity_view, search};
use crate::wp::entity::{Catalog, definitions};
use crate::wp::lists::{self, ListQuery};
use crate::wp::peek::{FactKind, FindGroup, FindHit, PeekFact, find_value, peek};
use crate::wp::{WpList, detect, expand, load_docs, options_overview, post_images};

const TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
#[ignore]
async fn detects_woocommerce_with_hpos_sync() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        assert_eq!(ctx.prefix, "wp_");
        let wc = ctx.wc.unwrap();
        assert_eq!(wc.version.as_deref(), Some("9.3.0"));
        assert_eq!(wc.hpos, HposMode::OnWithSync);
        assert!(wc.has_action_scheduler);
        assert!(detect(&db, "databee_test").await.unwrap().is_none());
    }
}

#[tokio::test]
#[ignore]
async fn the_site_zone_comes_from_timezone_string() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        assert_eq!(ctx.site.zone, SiteZone::Named(chrono_tz::Europe::Berlin));
        assert_eq!(ctx.site.zone_source, "timezone_string");
    }
}

#[tokio::test]
#[ignore]
async fn loads_an_order_document_with_meta_links_and_relation_counts() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        let docs = load_docs(&db, &ctx, &catalog, "wp_test", "order", &["4711".into()])
            .await
            .unwrap();
        let order = &docs[0];
        assert_eq!(order.title.as_deref(), Some("jane@shop.test"));
        assert_eq!(order.subtitle.as_deref(), Some("wc-processing"));

        let customer = order
            .fields
            .iter()
            .find(|f| f.name == "customer_id")
            .unwrap();
        let link = customer.link.as_ref().unwrap();
        assert_eq!(link.entity, "user");
        assert_eq!(link.label.as_deref(), Some("Jane Doe"));

        let total = order
            .fields
            .iter()
            .find(|f| f.name == "total_amount")
            .unwrap();
        assert_eq!(serde_json::to_value(&total.hint).unwrap()["text"], "€49.90");

        let ppcp = order.meta.iter().find(|g| g.name == "_ppcp*").unwrap();
        assert!(ppcp.private);
        assert!(ppcp.entries.iter().all(|e| e.name.starts_with("_ppcp")));

        let count = |name: &str| {
            order
                .relations
                .iter()
                .find(|r| r.name == name)
                .unwrap()
                .count
        };
        assert_eq!(count("items"), 2);
        assert_eq!(count("notes"), 1, "reviews are not order notes");
        assert_eq!(count("refunds"), 1);
        assert_eq!(count("addresses"), 2);
    }
}

#[tokio::test]
#[ignore]
async fn expands_relations_lazily_and_follows_meta_links() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        let items = expand(&db, &ctx, &catalog, "wp_test", "order", "4711", "items")
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        let mug = &items[0];
        assert_eq!(mug.title.as_deref(), Some("Mug"));
        let product = mug
            .meta
            .iter()
            .flat_map(|g| &g.entries)
            .find(|e| e.name == "_product_id")
            .unwrap();
        let link = product.link.as_ref().unwrap();
        assert_eq!((link.entity.as_str(), link.id.as_str()), ("post", "123"));
        assert_eq!(link.label.as_deref(), Some("Mug"));

        let user = load_docs(&db, &ctx, &catalog, "wp_test", "user", &["12".into()])
            .await
            .unwrap();
        assert!(user[0].fields.iter().all(|f| f.name != "user_pass"));
    }
}

#[tokio::test]
#[ignore]
async fn options_overview_reports_groups_and_expired_transients() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        let o = options_overview(&db, &ctx, "wp_test").await.unwrap();
        assert_eq!(o.total_count, 17);
        assert!(
            o.groups
                .iter()
                .any(|g| g.prefix == "woocommerce" && g.count == 5)
        );
        let old = o.transients.iter().find(|t| t.name == "old_cache").unwrap();
        assert!(old.expired);
        let fresh = o
            .transients
            .iter()
            .find(|t| t.name == "fresh_cache")
            .unwrap();
        assert!(!fresh.expired);
    }
}

async fn env() -> Vec<(MySqlSession, crate::wp::context::WpContext, Catalog)> {
    let mut out = Vec::new();
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        out.push((db, ctx, catalog));
    }
    out
}

#[tokio::test]
#[ignore]
async fn dashboard_summarizes_orders_actions_and_health() {
    for (db, ctx, _) in env().await {
        assert_eq!(ctx.site.currency.as_deref(), Some("EUR"));
        assert_eq!(ctx.site.active_plugins.plugins, ["woocommerce"]);
        let d = dashboard(&db, &ctx, "wp_test").await.unwrap();
        let orders = d.orders.unwrap();
        assert_eq!(orders.total, 2, "subscriptions and refunds are not orders");
        // The fixture has orders on fewer than 3 days, so the chart falls back to 24 hourly buckets.
        assert_eq!(orders.granularity, Granularity::Hour);
        assert_eq!(orders.days.len(), 24);
        assert_eq!(d.recent_orders[0].id, "4711");
        assert_eq!(d.recent_orders[0].total.as_deref(), Some("€49.90"));
        let actions = d.actions.unwrap();
        assert_eq!(actions.recent_failed[0].hook, "ppcp_check_payment_status");
        assert_eq!(
            actions.recent_failed[0].message.as_deref(),
            Some("action failed via WP Cron: PayPal API returned 500")
        );
        assert_eq!(actions.overdue, 1);
        assert_eq!(d.transients_expired, 1);
        assert_eq!(
            d.hpos_drift,
            Some(2),
            "refund 4712 and order 4713 have no posts copy"
        );
        for (table, category) in [
            ("wp_woocommerce_log", "logs"),
            ("wp_wc_orders", "woocommerce"),
            ("wp_actionscheduler_actions", "action_scheduler"),
            ("wp_posts", "core"),
        ] {
            assert_eq!(
                d.tables
                    .iter()
                    .find(|t| t.name == table)
                    .map(|t| t.category),
                Some(category),
                "{table}"
            );
        }
        let insights: Vec<&str> = d.insights.iter().map(|i| i.text.as_str()).collect();
        assert!(
            insights.contains(&"2 orders differ between wc_orders and their posts copy."),
            "{insights:?}"
        );
        assert!(
            insights
                .iter()
                .any(|t| t.contains("scheduled actions failed")),
            "{insights:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn network_tables_count_as_core_in_the_dashboard() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_multi").await.unwrap().unwrap();
        let d = dashboard(&db, &ctx, "wp_multi").await.unwrap();
        assert_eq!(d.hpos_drift, None);
        for table in ["wp_blogs", "wp_sitemeta", "wp_options"] {
            assert_eq!(
                d.tables
                    .iter()
                    .find(|t| t.name == table)
                    .map(|t| t.category),
                Some("core"),
                "{table}"
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn action_histogram_covers_fourteen_days_ending_today_in_the_site_zone() {
    for (db, ctx, _) in env().await {
        let now = scalar(&db, "SELECT UTC_TIMESTAMP()").await.unwrap();
        let now = chrono::NaiveDateTime::parse_from_str(&now, "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc();
        let today = SiteZone::Named(chrono_tz::Europe::Berlin)
            .to_local(now)
            .date()
            .format("%Y-%m-%d")
            .to_string();

        let histogram = action_histogram(&db, &ctx, "wp_test", None).await.unwrap();
        assert_eq!(histogram.days.len(), 14);
        assert_eq!(histogram.days.last().unwrap().date, today);
    }
}

#[tokio::test]
#[ignore]
async fn lists_sort_by_whitelisted_keys() {
    fn query(sort: Option<&str>, descending: Option<bool>) -> ListQuery {
        ListQuery {
            sort: sort.map(String::from),
            descending,
            ..Default::default()
        }
    }
    for (db, ctx, _) in env().await {
        // Orders: 4711 totals 49.90 and was created a day ago, 4713 totals 18.90 and three days ago.
        for (sort, descending, expected) in [
            (Some("total"), Some(false), ["4713", "4711"]),
            (Some("total"), Some(true), ["4711", "4713"]),
            (Some("total"), None, ["4711", "4713"]),
            (Some("id"), Some(false), ["4711", "4713"]),
            (Some("id"), None, ["4713", "4711"]),
            (None, None, ["4711", "4713"]),
            (Some("no-such-key"), None, ["4711", "4713"]),
        ] {
            let page = lists::orders(&db, &ctx, "wp_test", "shop_order", &query(sort, descending))
                .await
                .unwrap();
            let ids: Vec<&str> = page.items.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, expected, "orders {sort:?} {descending:?}");
        }

        // Actions 1..4 by hook: ppcp_check_payment_status, woocommerce_cleanup_draft_orders,
        // wc-admin_import_orders, woocommerce_run_on_woocommerce_admin_updated.
        // Scheduled: 1 a day ago, 2 two days ago, 3 in an hour, 4 an hour ago.
        for (sort, descending, expected) in [
            (Some("hook"), None, ["1", "3", "2", "4"]),
            (Some("hook"), Some(true), ["4", "2", "3", "1"]),
            (Some("scheduled"), Some(false), ["2", "1", "4", "3"]),
            (None, None, ["3", "4", "1", "2"]),
        ] {
            let page = lists::actions(&db, &ctx, "wp_test", &query(sort, descending))
                .await
                .unwrap();
            let ids: Vec<&str> = page.items.iter().map(|a| a.id.as_str()).collect();
            assert_eq!(ids, expected, "actions {sort:?} {descending:?}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn guest_checkouts_are_not_users_but_open_from_orders_and_search() {
    for (db, ctx, catalog) in env().await {
        let all = lists::users(&db, &ctx, "wp_test", &lists::ListQuery::default(), false)
            .await
            .unwrap();
        assert!(!all.items.iter().any(|c| c.id.starts_with("guest:")));
        assert!(!all.kinds.iter().any(|k| k.value == "guest"));

        let EntityView::Customer(c) = entity_view(
            &db,
            &ctx,
            &catalog,
            "wp_test",
            "user",
            "guest:guest@shop.test",
            false,
        )
        .await
        .unwrap() else {
            panic!("expected customer view")
        };
        assert_eq!(c.email.as_deref(), Some("guest@shop.test"));
        assert_eq!(
            c.orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            ["4713"]
        );
        assert_eq!(c.roles, ["Guest"]);
        assert!(c.tokens.is_empty());

        let hits = crate::wp::detail::search(&db, &ctx, "wp_test", "guest@")
            .await
            .unwrap();
        assert!(
            hits.iter()
                .any(|h| h.entity == "user" && h.id == "guest:guest@shop.test"),
            "{hits:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn peek_summarizes_an_entity_in_a_few_facts() {
    for (db, ctx, catalog) in env().await {
        let facts = peek(&db, &ctx, &catalog, "wp_test", "order", "4711")
            .await
            .unwrap();
        let shape = |facts: &[PeekFact]| -> Vec<(String, FactKind, String)> {
            facts
                .iter()
                .map(|f| (f.label.clone(), f.kind, f.value.clone()))
                .collect()
        };
        let row = |label: &str, kind: FactKind, value: &str| -> (String, FactKind, String) {
            (label.into(), kind, value.into())
        };

        // "Created" is relative to the fixture load time, so only its label and kind are exact.
        let (created, rest) = facts.split_last().unwrap();
        assert_eq!(
            shape(rest),
            [
                row("Status", FactKind::Status, "wc-processing"),
                row("Total", FactKind::Money, "€49.90"),
                row("Gateway", FactKind::Text, "ppcp-gateway"),
                row("Customer", FactKind::Text, "jane@shop.test"),
            ]
        );
        assert_eq!(
            (created.label.as_str(), created.kind),
            ("Created", FactKind::Time)
        );
        assert!(created.value.parse::<i64>().unwrap() > 0, "{created:?}");

        // Order stats follow the identity fact; registration is a site-local time (Europe/Berlin).
        let admin = peek(&db, &ctx, &catalog, "wp_test", "user", "1")
            .await
            .unwrap();
        assert_eq!(
            shape(&admin),
            [
                row("Name", FactKind::Text, "Admin"),
                row("Orders", FactKind::Text, "0"),
                row("Spent", FactKind::Money, "€0.00"),
                row("Email", FactKind::Text, "admin@shop.test"),
                row("Login", FactKind::Text, "admin"),
                row("Registered", FactKind::Time, "1735686000"),
            ]
        );
        let buyer = peek(&db, &ctx, &catalog, "wp_test", "user", "12")
            .await
            .unwrap();
        assert_eq!(
            shape(&buyer),
            [
                row("Name", FactKind::Text, "Jane Doe"),
                row("Orders", FactKind::Text, "1"),
                row("Spent", FactKind::Money, "€49.90"),
                row("Email", FactKind::Text, "jane@shop.test"),
                row("Login", FactKind::Text, "jane"),
                row("Registered", FactKind::Time, "1767344400"),
            ]
        );
        assert!(
            peek(&db, &ctx, &catalog, "wp_test", "order", "999999")
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
#[ignore]
async fn find_value_scans_meta_options_actions_and_logs() {
    for (db, ctx, _) in env().await {
        let hit = |row_id: &str, label: &str, entity: Option<(&str, &str)>, column: &str| FindHit {
            row_id: row_id.into(),
            label: label.into(),
            entity: entity.map(|(e, _)| e.into()),
            entity_id: entity.map(|(_, id)| id.into()),
            column: column.into(),
        };
        let group = |source: &str, table: &str, id_column: &str, hits: Vec<FindHit>| FindGroup {
            source: source.into(),
            table: table.into(),
            id_column: id_column.into(),
            count: hits.len() as u64,
            hits,
        };

        let paypal_id = find_value(&db, &ctx, "wp_test", "66M12696JX411763D")
            .await
            .unwrap();
        assert_eq!(
            paypal_id,
            [
                group(
                    "Order meta",
                    "wp_wc_orders_meta",
                    "id",
                    vec![hit(
                        "1",
                        "order #4711 · _ppcp_paypal_order_id",
                        Some(("order", "4711")),
                        "meta_value"
                    )]
                ),
                group(
                    "WooCommerce log",
                    "wp_woocommerce_log",
                    "log_id",
                    vec![hit(
                        "1",
                        "woocommerce-paypal-payments · PATCH https://api-m.sandbox.paypal.com/v2/checkout/orders/66M12696JX411763D",
                        None,
                        "message"
                    )]
                ),
            ]
        );

        let transaction = find_value(&db, &ctx, "wp_test", "61P3809224872131H")
            .await
            .unwrap();
        assert_eq!(
            transaction,
            [group(
                "Orders",
                "wp_wc_orders",
                "id",
                vec![hit(
                    "4711",
                    "order #4711 · wc-processing",
                    Some(("order", "4711")),
                    "transaction_id"
                )]
            )]
        );

        // Action args and log messages match by containment; "14711" is not "4711".
        let contained = find_value(&db, &ctx, "wp_test", "14711").await.unwrap();
        assert_eq!(
            contained,
            [
                group(
                    "Scheduled actions",
                    "wp_actionscheduler_actions",
                    "action_id",
                    vec![hit(
                        "3",
                        "wc-admin_import_orders · pending",
                        Some(("action", "3")),
                        "args"
                    )]
                ),
                group(
                    "WooCommerce log",
                    "wp_woocommerce_log",
                    "log_id",
                    vec![hit("3", "other · Unrelated error #14711", None, "message")]
                ),
            ]
        );

        assert!(
            find_value(&db, &ctx, "wp_test", "no-such-value-anywhere")
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
#[ignore]
async fn failed_actions_group_by_hook_with_last_message() {
    for (db, ctx, _) in env().await {
        let groups = crate::wp::lists::failed_by_hook(&db, &ctx, "wp_test")
            .await
            .unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].hook, "ppcp_check_payment_status");
        assert_eq!(groups[0].count, 1);
        assert_eq!(
            groups[0].last_message.as_deref(),
            Some("action failed via WP Cron: PayPal API returned 500")
        );
        assert!(groups[0].last_failure.is_some());
    }
}

#[tokio::test]
#[ignore]
async fn entity_browser_counts_sections() {
    for (db, ctx, _) in env().await {
        let sections = browser(&db, &ctx, "wp_test").await.unwrap();
        let count = |list: WpList| sections.iter().find(|s| s.list == list).map(|s| s.count);
        assert_eq!(count(WpList::Orders), Some(2));
        assert_eq!(count(WpList::Subscriptions), Some(1));
        assert_eq!(count(WpList::Users), Some(2));
        assert_eq!(count(WpList::Products), Some(2));
        assert_eq!(count(WpList::Actions), Some(4));
        assert_eq!(count(WpList::Sessions), Some(2));
        let content = sections.iter().find(|s| s.list == WpList::Content).unwrap();
        assert!(content.breakdown.iter().any(|f| f.value == "post"));
        assert!(
            content
                .breakdown
                .iter()
                .all(|f| f.value != "shop_order" && f.value != "product")
        );
    }
}

#[tokio::test]
#[ignore]
async fn lists_filter_and_search() {
    for (db, ctx, _) in env().await {
        let q = |status: Option<&str>, search: Option<&str>| ListQuery {
            status: status.map(String::from),
            search: search.map(String::from),
            ..Default::default()
        };
        let all = lists::orders(&db, &ctx, "wp_test", "shop_order", &q(None, None))
            .await
            .unwrap();
        assert_eq!(
            all.items.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            ["4711", "4713"]
        );
        assert_eq!(all.items[0].name.as_deref(), Some("Jane Doe"));
        assert_eq!(all.items[0].item_count, 1);
        assert!(
            all.facets
                .iter()
                .any(|f| f.value == "wc-failed" && f.label == "wc-failed")
        );
        assert!(
            all.kinds
                .iter()
                .any(|g| g.value == "ppcp-gateway" && g.label == "ppcp-gateway")
        );
        let failed = lists::orders(
            &db,
            &ctx,
            "wp_test",
            "shop_order",
            &q(Some("wc-failed"), None),
        )
        .await
        .unwrap();
        assert_eq!(failed.items.len(), 1);
        let by_tx = lists::orders(
            &db,
            &ctx,
            "wp_test",
            "shop_order",
            &q(None, Some("61P3809224872131H")),
        )
        .await
        .unwrap();
        assert_eq!(by_tx.items[0].id, "4711");
        let subs = lists::orders(&db, &ctx, "wp_test", "shop_subscription", &q(None, None))
            .await
            .unwrap();
        assert_eq!(subs.items[0].id, "4714");

        let users = lists::users(&db, &ctx, "wp_test", &q(None, None), false)
            .await
            .unwrap();
        let jane = users.items.iter().find(|c| c.id == "12").unwrap();
        assert_eq!(jane.roles, ["customer"]);
        assert_eq!(jane.order_count, 1);
        assert_eq!(jane.total_spent.as_deref(), Some("€49.90"));
        assert!(users.kinds.iter().any(|k| k.value == "administrator"));

        let products = lists::products(&db, &ctx, "wp_test", &q(None, Some("MUG")))
            .await
            .unwrap();
        let mug = &products.items[0];
        assert_eq!(
            (mug.name.as_str(), mug.kind.as_deref(), mug.price.as_deref()),
            ("Mug", Some("simple"), Some("€24.95"))
        );
        assert_eq!(
            mug.image.as_deref(),
            Some("https://shop.test/wp-content/uploads/2026/01/mug-150x150.jpg")
        );
        let shirt = lists::products(&db, &ctx, "wp_test", &q(None, Some("Shirt")))
            .await
            .unwrap();
        assert_eq!(shirt.items[0].variations, 2);
        assert_eq!(shirt.items[0].price.as_deref(), Some("€20.00–€22.00"));

        let actions = lists::actions(&db, &ctx, "wp_test", &q(Some("pending"), None))
            .await
            .unwrap();
        assert_eq!(actions.items.len(), 2);
        let sessions = lists::sessions(&db, &ctx, "wp_test", &q(None, None))
            .await
            .unwrap();
        let s = sessions.items.iter().find(|s| s.key == "12").unwrap();
        assert_eq!(
            (s.items, s.total.as_deref(), s.customer_id.as_deref()),
            (2, Some("€49.90"), Some("12"))
        );
        let content = lists::content(&db, &ctx, "wp_test", &q(None, None))
            .await
            .unwrap();
        assert_eq!(content.items[0].title.as_deref(), Some("Hello world"));
    }
}

#[tokio::test]
#[ignore]
async fn content_without_a_kind_lists_every_visible_post_type() {
    for (db, ctx, _) in env().await {
        let q = |kind: Option<&str>| ListQuery {
            kind: kind.map(String::from),
            ..Default::default()
        };
        let all = lists::content(&db, &ctx, "wp_test", &q(None))
            .await
            .unwrap();
        let mut kinds: Vec<_> = all.items.iter().map(|p| p.kind.as_str()).collect();
        kinds.sort();
        assert_eq!(kinds, ["attachment", "post"]);

        let mut facet_kinds: Vec<_> = all
            .kinds
            .iter()
            .map(|f| (f.value.as_str(), f.count))
            .collect();
        facet_kinds.sort();
        assert_eq!(facet_kinds, [("attachment", 1), ("post", 1)]);

        let mut statuses: Vec<_> = all
            .facets
            .iter()
            .map(|f| (f.value.as_str(), f.count))
            .collect();
        statuses.sort();
        assert_eq!(statuses, [("inherit", 1), ("publish", 1)]);

        let posts = lists::content(&db, &ctx, "wp_test", &q(Some("post")))
            .await
            .unwrap();
        assert_eq!(posts.items.len(), 1);
        assert_eq!(posts.items[0].title.as_deref(), Some("Hello world"));
        assert_eq!(posts.kinds, all.kinds);
        assert_eq!(
            posts
                .facets
                .iter()
                .map(|f| f.value.as_str())
                .collect::<Vec<_>>(),
            ["publish"]
        );
    }
}

#[tokio::test]
#[ignore]
async fn order_view_tells_the_story_of_a_checkout() {
    for (db, ctx, catalog) in env().await {
        let EntityView::Order(o) =
            entity_view(&db, &ctx, &catalog, "wp_test", "order", "4711", false)
                .await
                .unwrap()
        else {
            panic!("expected order view")
        };
        assert_eq!(
            (o.status.as_str(), o.total.as_deref()),
            ("wc-processing", Some("€49.90"))
        );
        assert_eq!(
            o.customer.as_ref().unwrap().label.as_deref(),
            Some("Jane Doe")
        );
        assert!(o.paid.is_some());
        let mug = &o.items[0];
        assert_eq!(
            (
                mug.name.as_str(),
                mug.quantity.as_deref(),
                mug.sku.as_deref()
            ),
            ("Mug", Some("2"), Some("MUG-1"))
        );
        assert_eq!(
            mug.product.as_ref().unwrap().image.as_deref(),
            Some("https://shop.test/wp-content/uploads/2026/01/mug-150x150.jpg")
        );
        assert!(
            o.totals
                .iter()
                .any(|(l, v)| l == "Shipping" && v == "€4.90")
        );
        assert!(
            o.totals
                .iter()
                .any(|(l, v)| l == "Refunded" && v == "−€10.00")
        );
        assert!(o.address_notes.iter().any(|n| n.contains("City/postcode")));
        let p = o.payment.as_ref().unwrap();
        assert_eq!(p.paypal_order_id.as_deref(), Some("66M12696JX411763D"));
        assert_eq!(p.fees.as_ref().unwrap().net, "€47.86");
        assert!(
            o.attribution
                .as_ref()
                .unwrap()
                .user_agent
                .as_deref()
                .unwrap()
                .starts_with("Mozilla/5.0 (Macintosh"),
            "the stored user agent, not a summary"
        );

        let kinds: Vec<EventKind> = o.timeline.iter().map(|e| e.kind).collect();
        let logs = crate::wp::detail::order_logs(&db, &ctx, &catalog, "wp_test", "4711")
            .await
            .unwrap();
        assert_eq!(
            logs.len(),
            3,
            "PayPal PATCH row, #4711 email row and serialized-context webhook row: {logs:?}"
        );
        for l in &logs {
            let text = format!("{} {}", l.title, l.raw.as_deref().unwrap_or(""));
            assert!(
                !text.contains("Stock reduced for order #47110")
                    && !text.contains("Unrelated error #14711"),
                "{text}"
            );
        }
        let webhook = logs
            .iter()
            .find(|l| l.title.contains("Webhook delivered"))
            .unwrap();
        assert_eq!(webhook.detail.as_deref(), Some("webhooks"));
        let paypal = logs
            .iter()
            .find(|l| l.detail.as_deref() == Some("woocommerce-paypal-payments"))
            .unwrap();
        assert_eq!(paypal.level, EventLevel::Info);
        assert_eq!(paypal.title, "PayPal API · PATCH order");
        assert!(paypal.raw.as_deref().unwrap().contains("66M12696JX411763D"));
        let row = paypal.row.as_ref().unwrap();
        assert_eq!(
            (row.table.as_str(), row.id_column.as_str()),
            ("wp_woocommerce_log", "log_id")
        );
        for k in [
            EventKind::Session,
            EventKind::Created,
            EventKind::Paid,
            EventKind::Note,
            EventKind::Action,
            EventKind::Refund,
        ] {
            assert!(kinds.contains(&k), "timeline lacks {k:?}: {kinds:?}");
        }
        assert!(o.timeline.windows(2).all(|w| w[0].at <= w[1].at));
        assert!(
            o.timeline.iter().all(|e| !e.title.contains("14711")),
            "unrelated IDs must not match"
        );
        let failed = o
            .timeline
            .iter()
            .find(|e| e.kind == EventKind::Action)
            .unwrap();
        assert_eq!(failed.level, EventLevel::Error);
        assert!(o.related.iter().any(|r| r.label == "Subscription #4714"));
        assert!(o.related.iter().any(|r| r.label == "Refund #4712"));
    }
}

#[tokio::test]
#[ignore]
async fn customer_and_product_views() {
    for (db, ctx, catalog) in env().await {
        let EntityView::Customer(c) =
            entity_view(&db, &ctx, &catalog, "wp_test", "user", "12", true)
                .await
                .unwrap()
        else {
            panic!("expected customer view")
        };
        assert_eq!(c.name, "Jane Doe");
        assert_eq!(c.roles, ["customer"]);
        assert_eq!(c.orders[0].id, "4711");
        assert_eq!(c.subscriptions[0].id, "4714");
        assert_eq!(c.tokens[0].summary, "VISA •••• 2503 · 01/2029");
        assert!(
            c.avatar
                .unwrap()
                .starts_with("https://gravatar.com/avatar/")
        );
        assert!(c.session.is_some());
        assert_eq!(c.billing.as_ref().unwrap().country.as_deref(), Some("AT"));
        let groups: Vec<&str> = c.doc.meta.iter().map(|g| g.name.as_str()).collect();
        assert!(
            groups.iter().all(|g| *g == "Other" || g.ends_with('*')),
            "meta is grouped by literal key prefix: {groups:?}"
        );

        let EntityView::Product(p) =
            entity_view(&db, &ctx, &catalog, "wp_test", "post", "200", false)
                .await
                .unwrap()
        else {
            panic!("expected product view")
        };
        assert_eq!(p.kind.as_deref(), Some("variable"));
        assert_eq!(
            p.attributes,
            [(
                "color".to_string(),
                vec!["Blue".to_string(), "Red".to_string()]
            )]
        );
        assert_eq!(p.variations.len(), 2);
        assert_eq!(
            p.variations[1].attributes,
            [("color".to_string(), "blue".to_string())]
        );
        assert_eq!(p.variations[1].stock_status.as_deref(), Some("outofstock"));
        assert!(matches!(
            entity_view(&db, &ctx, &catalog, "wp_test", "post", "201", false).await.unwrap(),
            EntityView::Product(v) if v.name == "Shirt"
        ));

        let EntityView::Action(a) =
            entity_view(&db, &ctx, &catalog, "wp_test", "action", "1", false)
                .await
                .unwrap()
        else {
            panic!("expected action view")
        };
        assert_eq!(a.group.as_deref(), Some("woocommerce-paypal-payments"));
        assert_eq!(a.logs.len(), 2);
    }
}

#[tokio::test]
#[ignore]
async fn omnisearch_finds_pasted_identifiers() {
    for (db, ctx, _) in env().await {
        let first = |hits: Vec<crate::wp::detail::SearchHit>| {
            hits.into_iter().next().map(|h| (h.entity, h.id, h.matched))
        };
        assert_eq!(
            first(
                search(&db, &ctx, "wp_test", "61P3809224872131H")
                    .await
                    .unwrap()
            ),
            Some(("order".into(), "4711".into(), "transaction_id".into()))
        );
        assert_eq!(
            first(
                search(&db, &ctx, "wp_test", "66M12696JX411763D")
                    .await
                    .unwrap()
            ),
            Some((
                "order".into(),
                "4711".into(),
                "_ppcp_paypal_order_id".into()
            ))
        );
        let jane = search(&db, &ctx, "wp_test", "jane@shop").await.unwrap();
        assert!(jane.iter().any(|h| h.entity == "user" && h.id == "12"));
        assert!(jane.iter().any(|h| h.entity == "order" && h.id == "4711"));
        let sku = search(&db, &ctx, "wp_test", "SHIRT-R").await.unwrap();
        assert!(sku.iter().any(|h| h.entity == "post" && h.id == "201"));
        let id = search(&db, &ctx, "wp_test", "#4711").await.unwrap();
        assert!(id.iter().any(|h| h.entity == "order" && h.id == "4711"));
    }
}

// ---------- edits (on the writable wp_write copy) ----------

async fn write_env() -> Vec<(MySqlSession, crate::wp::context::WpContext, Catalog)> {
    let mut out = Vec::new();
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_write").await.unwrap().unwrap();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        out.push((db, ctx, catalog));
    }
    out
}

async fn scalar(db: &MySqlSession, sql: &str) -> Option<String> {
    let rs = db.fetch(sql, TIMEOUT).await.unwrap();
    rs.rows.first().and_then(|r| match &r[0] {
        crate::db::Cell::Null => None,
        c => Some(crate::wp::rows::cell_str(c).unwrap_or_default().to_string()),
    })
}

async fn apply(db: &MySqlSession, statements: &[String]) {
    db.apply(statements, TIMEOUT).await.unwrap();
}

async fn wp_plan(
    db: &MySqlSession,
    ctx: &crate::wp::context::WpContext,
    catalog: &Catalog,
    request: WpChangeRequest,
) -> Plan {
    plan_wp_change(db, ctx, catalog, &request).await.unwrap()
}

async fn row_plan(db: &MySqlSession, request: ChangeRequest) -> Plan {
    plan_row_change(db, &request, TIMEOUT).await.unwrap()
}

/// The first row of `sql`, NULL cells as `None`.
async fn first_row(db: &MySqlSession, sql: &str) -> Vec<Option<String>> {
    let rs = db.fetch(sql, TIMEOUT).await.unwrap();
    rs.rows[0]
        .iter()
        .map(|c| match c {
            Cell::Null => None,
            c => Some(crate::wp::rows::cell_str(c).unwrap_or_default().to_string()),
        })
        .collect()
}

/// `(meta_key, meta_value)` of an order's meta rows in insertion order.
async fn order_meta(db: &MySqlSession, order_id: u64) -> Vec<(String, String)> {
    let rs = db
        .fetch(
            &format!("SELECT meta_key, meta_value FROM wp_write.wp_wc_orders_meta WHERE order_id = {order_id} ORDER BY id"),
            TIMEOUT,
        )
        .await
        .unwrap();
    rs.rows
        .iter()
        .map(|r| {
            let text = |c: &Cell| crate::wp::rows::cell_str(c).unwrap_or_default().to_string();
            (text(&r[0]), text(&r[1]))
        })
        .collect()
}

#[tokio::test]
#[ignore]
async fn set_order_status_updates_both_copies_and_undo_restores_the_status() {
    for (db, ctx, catalog) in write_env().await {
        let hpos = "SELECT status, date_updated_gmt > '2026-01-01 10:00:00' FROM wp_write.wp_wc_orders WHERE id = 9101";
        let posts = "SELECT post_status, post_modified_gmt > '2026-01-01 10:00:00', post_modified > '2026-01-01 11:00:00' FROM wp_write.wp_posts WHERE ID = 9101";
        let strings = |cells: Vec<Option<String>>| -> Vec<String> {
            cells.into_iter().map(Option::unwrap_or_default).collect()
        };
        assert_eq!(strings(first_row(&db, hpos).await), ["wc-processing", "0"]);
        assert_eq!(
            strings(first_row(&db, posts).await),
            ["wc-processing", "0", "0"]
        );

        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::SetOrderStatus {
                database: "wp_write".into(),
                id: "9101".into(),
                status: "completed".into(),
            },
        )
        .await;
        assert_eq!(change.statements.len(), 2, "HPOS table and posts copy");
        apply(&db, &change.statements).await;
        assert_eq!(strings(first_row(&db, hpos).await), ["wc-completed", "1"]);
        assert_eq!(
            strings(first_row(&db, posts).await),
            ["wc-completed", "1", "1"],
            "the posts copy follows, with both modified timestamps bumped"
        );

        apply(&db, &change.undo).await;
        assert_eq!(strings(first_row(&db, hpos).await)[0], "wc-processing");
        assert_eq!(strings(first_row(&db, posts).await)[0], "wc-processing");
    }
}

#[tokio::test]
#[ignore]
async fn set_meta_adds_a_new_key_and_undo_removes_it() {
    for (db, ctx, catalog) in write_env().await {
        let count = "SELECT COUNT(*) FROM wp_write.wp_wc_orders_meta WHERE order_id = 9102 AND meta_key = '_debug_flag'";
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::SetMeta {
                database: "wp_write".into(),
                entity: "order".into(),
                id: "9102".into(),
                key: "_debug_flag".into(),
                value: "1".into(),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(
            scalar(&db, "SELECT meta_value FROM wp_write.wp_wc_orders_meta WHERE order_id = 9102 AND meta_key = '_debug_flag'")
                .await
                .as_deref(),
            Some("1")
        );
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, count).await.as_deref(), Some("0"));
    }
}

#[tokio::test]
#[ignore]
async fn delete_meta_removes_only_that_key_and_undo_reinserts_it() {
    for (db, ctx, catalog) in write_env().await {
        let both = vec![
            ("_keep".to_string(), "k".to_string()),
            ("_to_delete".to_string(), "bye".to_string()),
        ];
        assert_eq!(order_meta(&db, 9103).await, both);

        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::DeleteMeta {
                database: "wp_write".into(),
                entity: "order".into(),
                id: "9103".into(),
                key: "_to_delete".into(),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(
            order_meta(&db, 9103).await,
            [("_keep".to_string(), "k".to_string())]
        );
        apply(&db, &change.undo).await;
        assert_eq!(order_meta(&db, 9103).await, both);
    }
}

#[tokio::test]
#[ignore]
async fn editing_a_serialized_value_by_path_keeps_it_parseable_and_undo_restores_it() {
    for (db, _, _) in write_env().await {
        let read =
            "SELECT option_value FROM wp_write.wp_options WHERE option_name = 'zz_serialized_edit'";
        let raw = scalar(&db, read).await.unwrap();
        let edited = crate::decode::edit::edit_serialized(
            &raw,
            &[1],
            &crate::decode::edit::NewValue::String("PayPal ✓".into()),
        )
        .unwrap();
        let change = row_plan(
            &db,
            ChangeRequest::SetCell {
                database: "wp_write".into(),
                table: "wp_options".into(),
                key_column: "option_name".into(),
                key: "zz_serialized_edit".into(),
                column: "option_value".into(),
                value: Some(edited.clone()),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        let stored = scalar(&db, read).await.unwrap();
        assert_eq!(stored, edited);
        assert!(
            crate::decode::php::parse(stored.as_bytes()).is_ok(),
            "{stored}"
        );
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, read).await.unwrap(), raw);
    }
}

#[tokio::test]
#[ignore]
async fn expired_transients_are_deleted_and_restored_by_undo() {
    for (db, ctx, catalog) in write_env().await {
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::DeleteExpiredTransients {
                database: "wp_write".into(),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM wp_write.wp_options WHERE option_name LIKE '%old\\_cache'"
            )
            .await
            .as_deref(),
            Some("0")
        );
        apply(&db, &change.undo).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM wp_write.wp_options WHERE option_name LIKE '%old\\_cache'"
            )
            .await
            .as_deref(),
            Some("2")
        );
    }
}

#[tokio::test]
#[ignore]
async fn autoload_keeps_the_yes_no_style_and_undo_restores_it() {
    for (db, ctx, catalog) in write_env().await {
        let read =
            "SELECT autoload FROM wp_write.wp_options WHERE option_name = 'zz_autoload_probe'";
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::SetAutoload {
                database: "wp_write".into(),
                option: "zz_autoload_probe".into(),
                on: false,
            },
        )
        .await;
        assert_eq!(
            change.statements,
            [
                "UPDATE `wp_write`.`wp_options` SET autoload = 'no' WHERE option_name = 'zz_autoload_probe'"
            ]
        );
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, read).await.as_deref(), Some("no"));
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, read).await.as_deref(), Some("yes"));
    }
}

#[tokio::test]
#[ignore]
async fn retrying_a_failed_action_makes_it_pending_and_undo_restores_status_and_date() {
    for (db, ctx, catalog) in write_env().await {
        let read = "SELECT status, scheduled_date_gmt FROM wp_write.wp_actionscheduler_actions WHERE action_id = 9201";
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::RetryAction {
                database: "wp_write".into(),
                id: "9201".into(),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        let after = first_row(&db, read).await;
        assert_eq!(after[0].as_deref(), Some("pending"));
        assert_ne!(after[1].as_deref(), Some("2026-01-01 00:00:00"));
        apply(&db, &change.undo).await;
        assert_eq!(
            first_row(&db, read).await,
            [
                Some("failed".to_string()),
                Some("2026-01-01 00:00:00".to_string())
            ]
        );
    }
}

#[tokio::test]
#[ignore]
async fn cancelling_a_pending_action_marks_it_canceled_and_undo_restores_it() {
    for (db, ctx, catalog) in write_env().await {
        let read = "SELECT status, scheduled_date_gmt FROM wp_write.wp_actionscheduler_actions WHERE action_id = 9202";
        let pending = vec![
            Some("pending".to_string()),
            Some("2030-01-01 00:00:00".to_string()),
        ];
        assert_eq!(first_row(&db, read).await, pending);
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::CancelAction {
                database: "wp_write".into(),
                id: "9202".into(),
            },
        )
        .await;
        assert_eq!(change.description, "Cancel action #9202");
        apply(&db, &change.statements).await;
        assert_eq!(
            first_row(&db, read).await,
            [
                Some("canceled".to_string()),
                Some("2030-01-01 00:00:00".to_string())
            ]
        );
        apply(&db, &change.undo).await;
        assert_eq!(first_row(&db, read).await, pending);
    }
}

#[tokio::test]
#[ignore]
async fn deleting_a_row_with_a_null_cell_restores_it_exactly() {
    for (db, _, _) in write_env().await {
        let read = "SELECT meta_id, post_id, meta_key, meta_value FROM wp_write.wp_postmeta WHERE meta_id = 9301";
        let before = first_row(&db, read).await;
        assert_eq!(before[3], None, "the fixture row has a NULL meta_value");
        let change = row_plan(
            &db,
            ChangeRequest::DeleteRow {
                database: "wp_write".into(),
                table: "wp_postmeta".into(),
                key_column: "meta_id".into(),
                key: "9301".into(),
            },
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM wp_write.wp_postmeta WHERE meta_id = 9301"
            )
            .await
            .as_deref(),
            Some("0")
        );
        apply(&db, &change.undo).await;
        assert_eq!(first_row(&db, read).await, before);
    }
}

#[tokio::test]
#[ignore]
async fn duplicating_a_row_inserts_a_copy_with_a_new_auto_increment_id() {
    for (db, _, _) in write_env().await {
        let copies = "SELECT meta_id, post_id, meta_value FROM wp_write.wp_postmeta WHERE meta_key = 'zz_duplicate_me' ORDER BY meta_id";
        let change = row_plan(
            &db,
            ChangeRequest::DuplicateRow {
                database: "wp_write".into(),
                table: "wp_postmeta".into(),
                key_column: "meta_id".into(),
                key: "9302".into(),
            },
        )
        .await;
        assert!(change.undo.is_empty());
        apply(&db, &change.statements).await;
        let rs = db.fetch(copies, TIMEOUT).await.unwrap();
        let rows: Vec<(String, String)> = rs
            .rows
            .iter()
            .map(|r| {
                let text = |c: &Cell| crate::wp::rows::cell_str(c).unwrap_or_default().to_string();
                (text(&r[1]), text(&r[2]))
            })
            .collect();
        let ids: Vec<String> = rs
            .rows
            .iter()
            .map(|r| crate::wp::rows::cell_str(&r[0]).unwrap().to_string())
            .collect();
        apply(
            &db,
            &["DELETE FROM wp_write.wp_postmeta WHERE meta_key = 'zz_duplicate_me' AND meta_id <> 9302".into()],
        )
        .await;
        assert_eq!(
            rows,
            [
                ("123".to_string(), "v".to_string()),
                ("123".to_string(), "v".to_string())
            ]
        );
        assert_eq!(ids[0], "9302");
        assert_ne!(ids[1], "9302");
    }
}

#[tokio::test]
#[ignore]
async fn inserting_a_row_writes_null_for_a_missing_value() {
    for (db, _, _) in write_env().await {
        let change = row_plan(
            &db,
            ChangeRequest::InsertRow {
                database: "wp_write".into(),
                table: "wp_postmeta".into(),
                values: vec![
                    ("post_id".into(), Some("123".into())),
                    ("meta_key".into(), Some("zz_inserted".into())),
                    ("meta_value".into(), None),
                ],
            },
        )
        .await;
        assert!(change.undo.is_empty());
        apply(&db, &change.statements).await;
        let stored = first_row(
            &db,
            "SELECT post_id, meta_key, meta_value FROM wp_write.wp_postmeta WHERE meta_key = 'zz_inserted'",
        )
        .await;
        apply(
            &db,
            &["DELETE FROM wp_write.wp_postmeta WHERE meta_key = 'zz_inserted'".into()],
        )
        .await;
        assert_eq!(
            stored,
            [
                Some("123".to_string()),
                Some("zz_inserted".to_string()),
                None
            ]
        );
    }
}

#[tokio::test]
#[ignore]
async fn deleting_old_logs_removes_only_that_sources_old_entries() {
    for (db, ctx, catalog) in write_env().await {
        let remaining = "SELECT message FROM wp_write.wp_woocommerce_log WHERE source = 'zz-retention' ORDER BY log_id";
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::DeleteLogsBefore {
                database: "wp_write".into(),
                days: 30,
                source: Some("zz-retention".into()),
            },
        )
        .await;
        assert_eq!(
            change.description,
            "Delete 2 log entries older than 30 days from zz-retention (no undo)"
        );
        assert!(change.undo.is_empty());
        apply(&db, &change.statements).await;
        let rs = db.fetch(remaining, TIMEOUT).await.unwrap();
        let messages: Vec<&str> = rs
            .rows
            .iter()
            .map(|r| crate::wp::rows::cell_str(&r[0]).unwrap())
            .collect();
        assert_eq!(messages, ["recent entry"]);
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM wp_write.wp_woocommerce_log WHERE source <> 'zz-retention'"
            )
            .await
            .as_deref(),
            Some("5"),
            "other sources keep their entries"
        );
        apply(
            &db,
            &["INSERT INTO wp_write.wp_woocommerce_log (timestamp, level, source, message, context) VALUES \
               (UTC_TIMESTAMP() - INTERVAL 400 DAY, 200, 'zz-retention', 'old entry one', NULL), \
               (UTC_TIMESTAMP() - INTERVAL 400 DAY, 200, 'zz-retention', 'old entry two', NULL)".into()],
        )
        .await;
    }
}

#[tokio::test]
#[ignore]
async fn a_failing_statement_rolls_back_the_whole_change() {
    for (db, _, _) in write_env().await {
        let err = db
            .apply(
                &[
                    "UPDATE wp_write.wp_wc_orders SET status = 'wc-x' WHERE id = 9105".into(),
                    "UPDATE wp_write.nope SET a = 1".into(),
                ],
                TIMEOUT,
            )
            .await;
        assert!(err.is_err());
        assert_eq!(
            scalar(
                &db,
                "SELECT status FROM wp_write.wp_wc_orders WHERE id = 9105"
            )
            .await
            .as_deref(),
            Some("wc-failed")
        );
    }
}

#[tokio::test]
#[ignore]
async fn snapshots_diff_and_compare() {
    use crate::wp::snapshot::{Scope, capture, diff};
    for (db, ctx, catalog) in write_env().await {
        let scope = Scope {
            entity: Some("order".into()),
            id: Some("9402".into()),
            include_related: true,
            option_pattern: Some("zz_snapshot_probe".into()),
            include_actions: true,
        };
        let before = capture(&db, &ctx, &catalog, "wp_write", &scope)
            .await
            .unwrap();
        assert_eq!(before["order#9402/field/status"], "wc-failed");
        assert_eq!(before["options/zz_snapshot_probe"], "before");
        assert_eq!(before["items/order_item#9402/meta/_qty"], "3");
        assert!(!before.contains_key("order#9402/meta/_snap"));

        apply(
            &db,
            &[
                "UPDATE wp_write.wp_wc_orders SET status = 'wc-cancelled' WHERE id = 9402".into(),
                "UPDATE wp_write.wp_options SET option_value = 'after' WHERE option_name = 'zz_snapshot_probe'".into(),
                "INSERT INTO wp_write.wp_wc_orders_meta (order_id, meta_key, meta_value) VALUES (9402, '_snap', 'x')".into(),
            ],
        )
        .await;
        let after = capture(&db, &ctx, &catalog, "wp_write", &scope)
            .await
            .unwrap();
        apply(
            &db,
            &[
                "UPDATE wp_write.wp_wc_orders SET status = 'wc-failed' WHERE id = 9402".into(),
                "UPDATE wp_write.wp_options SET option_value = 'before' WHERE option_name = 'zz_snapshot_probe'".into(),
                "DELETE FROM wp_write.wp_wc_orders_meta WHERE order_id = 9402 AND meta_key = '_snap'".into(),
            ],
        )
        .await;

        let changes: Vec<_> = diff(&before, &after)
            .into_iter()
            .map(|c| (c.path, c.before, c.after))
            .collect();
        assert_eq!(
            changes,
            [
                (
                    "options/zz_snapshot_probe".to_string(),
                    Some("before".to_string()),
                    Some("after".to_string())
                ),
                (
                    "order#9402/field/status".to_string(),
                    Some("wc-failed".to_string()),
                    Some("wc-cancelled".to_string())
                ),
                (
                    "order#9402/meta/_snap".to_string(),
                    None,
                    Some("x".to_string())
                ),
            ]
        );
    }
}

#[tokio::test]
#[ignore]
async fn compare_lists_differing_fields_and_one_sided_meta() {
    use crate::wp::snapshot::compare;
    for (db, ctx, catalog) in write_env().await {
        let rows = compare(&db, &ctx, &catalog, "wp_write", "order", "9401", "9403")
            .await
            .unwrap();
        let status = rows.iter().find(|r| r.path == "field/status").unwrap();
        assert_eq!(
            (status.a.as_deref(), status.b.as_deref()),
            (Some("wc-processing"), Some("wc-failed"))
        );
        let fees: Vec<_> = rows
            .iter()
            .filter(|r| r.path.starts_with("meta/_ppcp_paypal_fees/"))
            .map(|r| (r.path.as_str(), r.a.as_deref(), r.b.as_deref()))
            .collect();
        assert_eq!(
            fees,
            [
                (
                    "meta/_ppcp_paypal_fees/gross_amount/currency_code",
                    Some("EUR"),
                    None
                ),
                (
                    "meta/_ppcp_paypal_fees/gross_amount/value",
                    Some("49.90"),
                    None
                ),
                (
                    "meta/_ppcp_paypal_fees/net_amount/currency_code",
                    Some("EUR"),
                    None
                ),
                (
                    "meta/_ppcp_paypal_fees/net_amount/value",
                    Some("47.86"),
                    None
                ),
                (
                    "meta/_ppcp_paypal_fees/paypal_fee/currency_code",
                    Some("EUR"),
                    None
                ),
                (
                    "meta/_ppcp_paypal_fees/paypal_fee/value",
                    Some("2.04"),
                    None
                ),
            ]
        );
    }
}

// ---------- undo fidelity and plan refusals ----------

fn set_cell(
    table: &str,
    key_column: &str,
    key: &str,
    column: &str,
    value: Option<&str>,
) -> ChangeRequest {
    ChangeRequest::SetCell {
        database: "wp_write".into(),
        table: table.into(),
        key_column: key_column.into(),
        key: key.into(),
        column: column.into(),
        value: value.map(String::from),
    }
}

#[tokio::test]
#[ignore]
async fn undo_restores_an_empty_string_instead_of_null() {
    for db in sessions("wp_test").await {
        let read = "SELECT post_name FROM wp_write.wp_posts WHERE ID = 9601";
        assert_eq!(scalar(&db, read).await.as_deref(), Some(""));
        let change = row_plan(
            &db,
            set_cell("wp_posts", "ID", "9601", "post_name", Some("x")),
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, read).await.as_deref(), Some("x"));
        apply(&db, &change.undo).await;
        assert_eq!(
            scalar(&db, read).await.as_deref(),
            Some(""),
            "restored as '' and not NULL"
        );
    }
}

#[tokio::test]
#[ignore]
async fn undo_restores_a_null_cell_to_null() {
    for db in sessions("wp_test").await {
        let read = "SELECT email FROM wp_write.wp_wc_order_addresses WHERE id = 9602";
        assert_eq!(scalar(&db, read).await, None);
        let change = row_plan(
            &db,
            set_cell(
                "wp_wc_order_addresses",
                "id",
                "9602",
                "email",
                Some("ship@shop.test"),
            ),
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, read).await.as_deref(), Some("ship@shop.test"));
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, read).await, None, "restored as NULL");
    }
}

#[tokio::test]
#[ignore]
async fn editing_a_binary_cell_has_no_undo() {
    for db in sessions("wp_test").await {
        let change = row_plan(
            &db,
            ChangeRequest::SetCell {
                database: "databee_test".into(),
                table: "types".into(),
                key_column: "id".into(),
                key: "1".into(),
                column: "blob_data".into(),
                value: Some("x".into()),
            },
        )
        .await;
        assert!(change.undo.is_empty(), "{:?}", change.undo);
        assert!(
            change.description.contains("no undo"),
            "{}",
            change.description
        );
    }
}

#[tokio::test]
#[ignore]
async fn undo_of_a_key_column_change_finds_the_row_by_its_new_key() {
    for db in sessions("wp_test").await {
        let count = |name: &'static str| {
            let db = &db;
            async move {
                scalar(
                    db,
                    &format!(
                        "SELECT COUNT(*) FROM wp_write.wp_options WHERE option_name = '{name}'"
                    ),
                )
                .await
            }
        };
        let change = row_plan(
            &db,
            set_cell(
                "wp_options",
                "option_name",
                "zz_rename_me",
                "option_name",
                Some("zz_renamed"),
            ),
        )
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(count("zz_renamed").await.as_deref(), Some("1"));
        assert_eq!(count("zz_rename_me").await.as_deref(), Some("0"));
        apply(&db, &change.undo).await;
        assert_eq!(count("zz_rename_me").await.as_deref(), Some("1"));
        assert_eq!(count("zz_renamed").await.as_deref(), Some("0"));
    }
}

#[tokio::test]
#[ignore]
async fn set_meta_refuses_duplicate_rows() {
    for (db, ctx, catalog) in write_env().await {
        let result = plan_wp_change(
            &db,
            &ctx,
            &catalog,
            &WpChangeRequest::SetMeta {
                database: "wp_write".into(),
                entity: "user".into(),
                id: "9501".into(),
                key: "dup_key".into(),
                value: "c".into(),
            },
        )
        .await;
        let err = result.expect_err("two rows of one meta key");
        assert!(
            matches!(&err, AppError::InvalidInput(m) if m == "user #9501 has 2 rows of meta dup_key; edit them in the Database view"),
            "{err:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn delete_meta_of_a_missing_key_is_refused() {
    for (db, ctx, catalog) in write_env().await {
        let result = plan_wp_change(
            &db,
            &ctx,
            &catalog,
            &WpChangeRequest::DeleteMeta {
                database: "wp_write".into(),
                entity: "user".into(),
                id: "12".into(),
                key: "no_such_key".into(),
            },
        )
        .await;
        assert!(result.is_err(), "{result:?}");
    }
}

#[tokio::test]
#[ignore]
async fn log_retention_plans_a_fixed_cutoff_literal() {
    for (db, ctx, catalog) in write_env().await {
        let change = wp_plan(
            &db,
            &ctx,
            &catalog,
            WpChangeRequest::DeleteLogsBefore {
                database: "wp_write".into(),
                days: 0,
                source: None,
            },
        )
        .await;
        let sql = &change.statements[0];
        assert!(!sql.contains("UTC_TIMESTAMP"), "{sql}");
        assert!(
            sql.starts_with("DELETE FROM `wp_write`.`wp_woocommerce_log` WHERE `timestamp` < '"),
            "{sql}"
        );
    }
}

// ---------- legacy store (wp_legacy: orders in wp_posts, HPOS off) ----------

async fn legacy_env() -> Vec<(MySqlSession, crate::wp::context::WpContext, Catalog)> {
    let mut out = Vec::new();
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_legacy").await.unwrap().unwrap();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        out.push((db, ctx, catalog));
    }
    out
}

#[tokio::test]
#[ignore]
async fn legacy_store_is_detected_without_hpos() {
    for (_, ctx, _) in legacy_env().await {
        assert!(ctx.wc.is_some());
        assert!(!ctx.hpos());
    }
}

#[tokio::test]
#[ignore]
async fn legacy_user_document_loads_without_an_orders_relation() {
    for (db, ctx, catalog) in legacy_env().await {
        let docs = load_docs(&db, &ctx, &catalog, "wp_legacy", "user", &["7".into()])
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].relations.iter().all(|r| r.name != "orders"));
    }
}

#[tokio::test]
#[ignore]
async fn legacy_customer_view_lists_posts_based_orders() {
    for (db, ctx, catalog) in legacy_env().await {
        let EntityView::Customer(c) =
            entity_view(&db, &ctx, &catalog, "wp_legacy", "user", "7", false)
                .await
                .unwrap()
        else {
            panic!("expected customer view")
        };
        let mut ids: Vec<&str> = c.orders.iter().map(|o| o.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["501", "502"]);
    }
}

#[tokio::test]
#[ignore]
async fn legacy_order_document_counts_its_notes() {
    for (db, ctx, catalog) in legacy_env().await {
        let docs = load_docs(&db, &ctx, &catalog, "wp_legacy", "order", &["501".into()])
            .await
            .unwrap();
        let notes = docs[0]
            .relations
            .iter()
            .find(|r| r.name == "notes")
            .unwrap();
        assert_eq!(notes.count, 2);
    }
}

// ---------- detection fixtures (wp_custom, wp_multi, read-only) ----------

#[tokio::test]
#[ignore]
async fn a_custom_table_prefix_is_detected() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_custom").await.unwrap().unwrap();
        assert_eq!(ctx.prefix, "xy_");
        assert_eq!(ctx.site.name.as_deref(), Some("Custom Prefix Site"));
        assert_eq!(ctx.wc, None);
        assert!(ctx.sites.is_empty());
        assert!(ctx.has_table("options"));
    }
}

#[tokio::test]
#[ignore]
async fn a_multisite_network_lists_sub_sites_and_network_plugins() {
    for db in sessions("wp_test").await {
        let ctx = detect(&db, "wp_multi").await.unwrap().unwrap();
        assert_eq!(ctx.prefix, "wp_");
        assert_eq!(
            ctx.sites,
            [SiteInfo {
                blog_id: 2,
                prefix: "wp_2_".into(),
                domain: Some("shop.network.test".into()),
                path: Some("/".into()),
            }]
        );
        let network = ctx.site.network_plugins.unwrap();
        assert_eq!(network.source, "active_sitewide_plugins");
        assert_eq!(network.plugins, ["woocommerce"]);
        assert!(ctx.site.active_plugins.plugins.is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn single_sites_have_no_network_plugins_or_sub_sites() {
    for db in sessions("wp_test").await {
        for database in ["wp_test", "wp_legacy", "wp_custom"] {
            let ctx = detect(&db, database).await.unwrap().unwrap();
            assert_eq!(ctx.site.network_plugins, None, "{database}");
            assert!(ctx.sites.is_empty(), "{database}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn product_images_use_the_uploads_base_the_attachment_guid_proves() {
    for (db, ctx, _) in env().await {
        assert_eq!(ctx.site.uploads_url, None);
        let images = post_images(&db, &ctx, "wp_test", &["123".into(), "200".into()])
            .await
            .unwrap();
        assert_eq!(
            images,
            HashMap::from([(
                "123".to_string(),
                "https://shop.test/wp-content/uploads/2026/01/mug-150x150.jpg".to_string()
            )]),
            "the shirt has no featured image"
        );
    }
}

/// Read-only checks against a real WordPress database of your own (`DATABEE_WP_TEST`). Kept in a
/// module so `bun run test:db` can leave them out with `--skip real_data`.
mod real_data {
    use std::time::Duration;

    use crate::cli::{self, CliOutcome, Launch};
    use crate::db::Cell;
    use crate::db::Database;
    use crate::db::mysql::MySqlSession;
    use crate::decode::php;
    use crate::wp::context::WpContext;
    use crate::wp::detect;

    const TIMEOUT: Duration = Duration::from_secs(10);
    const PAGE: usize = 2000;

    /// The database named by `DATABEE_WP_TEST` (a `databee` connection URL), connected.
    async fn real_site() -> (MySqlSession, String) {
        let url = std::env::var("DATABEE_WP_TEST").unwrap_or_else(|_| {
            panic!("DATABEE_WP_TEST=mysql://user:pass@host:port/db must name a WordPress database")
        });
        let outcome = cli::parse(
            ["databee", url.as_str()],
            3306,
            &mut std::io::empty(),
            &|_| None,
        );
        let Ok(CliOutcome::Run {
            launch: Some(Launch { spec, password, .. }),
            ..
        }) = outcome
        else {
            panic!("DATABEE_WP_TEST is not a connection URL: {outcome:?}");
        };
        let database = spec
            .database
            .clone()
            .expect("DATABEE_WP_TEST names a database");
        let db = MySqlSession::connect(&spec, password.as_ref(), TIMEOUT, TIMEOUT)
            .await
            .unwrap();
        (db, database)
    }

    async fn real_context(db: &MySqlSession, database: &str) -> WpContext {
        detect(db, database)
            .await
            .unwrap()
            .expect("a WordPress database")
    }

    fn unhex(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[tokio::test]
    #[ignore]
    async fn every_serialized_value_round_trips() {
        let (db, database) = real_site().await;
        let ctx = real_context(&db, &database).await;
        let dialect = db.dialect();

        let mut sources = vec![
            ("options", "option_id", "option_value"),
            ("postmeta", "meta_id", "meta_value"),
            ("usermeta", "umeta_id", "meta_value"),
        ];
        if ctx.hpos() {
            sources.push(("wc_orders_meta", "id", "meta_value"));
        }
        if ctx.wc.is_some() {
            sources.push(("woocommerce_order_itemmeta", "meta_id", "meta_value"));
        }

        let (mut checked, mut lenient, mut broken, mut skipped) = (0, 0, 0, 0);
        for (table, id_column, column) in sources {
            let qualified = format!(
                "{}.{}",
                dialect.quote_ident(&database),
                dialect.quote_ident(&format!("{}{table}", ctx.prefix))
            );
            let (id, col) = (dialect.quote_ident(id_column), dialect.quote_ident(column));
            let mut offset = 0;
            loop {
                let sql = format!(
                    "SELECT HEX({col}) FROM {qualified} WHERE {col} REGEXP '^(a|O|s|i|b|d|C|E):' OR {col} = 'N;' \
                     ORDER BY {id} LIMIT {PAGE} OFFSET {offset}"
                );
                let rs = db.fetch(&sql, Duration::from_secs(60)).await.unwrap();
                assert!(
                    !rs.truncated,
                    "a page of {PAGE} rows stays under the fetch cap"
                );
                for row in &rs.rows {
                    let Cell::Text(hex) = &row[0] else {
                        skipped += 1;
                        continue;
                    };
                    let bytes = unhex(hex);
                    match php::parse(&bytes) {
                        Ok(parsed) => {
                            checked += 1;
                            assert_eq!(
                                php::serialize(&parsed),
                                bytes,
                                "round trip failed in {table}: {}",
                                String::from_utf8_lossy(&bytes)
                            );
                        }
                        Err(_) if php::parse_lenient(&bytes).is_ok() => lenient += 1,
                        Err(e) => {
                            broken += 1;
                            eprintln!(
                                "unparseable in {table} ({e}): {}",
                                String::from_utf8_lossy(&bytes)
                                    .chars()
                                    .take(120)
                                    .collect::<String>()
                            );
                        }
                    }
                }
                if rs.rows.len() < PAGE {
                    break;
                }
                offset += PAGE;
            }
        }
        eprintln!(
            "round-tripped {checked} values; {lenient} needed lenient parsing; {broken} unparseable; {skipped} cells skipped (not text)"
        );
        assert!(checked > 0);
    }

    /// Read-only smoke test of every WordPress-mode query against a real database.
    #[tokio::test]
    #[ignore]
    async fn wordpress_mode_queries_work_on_real_data() {
        use crate::wp::detail::{EntityView, entity_view, search};
        use crate::wp::entity::{Catalog, definitions};
        use crate::wp::lists::{self, ListQuery};
        use crate::wp::{dashboard, options_group};

        fn non_empty<T>(what: &str, items: &[T]) {
            assert!(!items.is_empty(), "the {what} list is empty on this site");
        }

        let (db, database) = real_site().await;
        let ctx = real_context(&db, &database).await;
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        let started = std::time::Instant::now();
        let lap = |what: &str| eprintln!("{what:<28} {:>6} ms", started.elapsed().as_millis());

        let d = dashboard::dashboard(&db, &ctx, &database).await.unwrap();
        lap("dashboard");
        eprintln!(
            "  insights: {:?}",
            d.insights.iter().map(|i| &i.text).collect::<Vec<_>>()
        );
        non_empty("dashboard tables", &d.tables);
        non_empty("dashboard recent orders", &d.recent_orders);
        non_empty(
            "entity browser",
            &dashboard::browser(&db, &ctx, &database).await.unwrap(),
        );
        lap("browser");

        let q = ListQuery::default();
        let orders = lists::orders(&db, &ctx, &database, "shop_order", &q)
            .await
            .unwrap();
        non_empty("orders", &orders.items);
        let subscriptions = lists::orders(&db, &ctx, &database, "shop_subscription", &q)
            .await
            .unwrap();
        non_empty("subscriptions", &subscriptions.items);
        let customers = lists::users(&db, &ctx, &database, &q, false).await.unwrap();
        non_empty("customers", &customers.items);
        let products = lists::products(&db, &ctx, &database, &q).await.unwrap();
        non_empty("products", &products.items);
        let content = lists::content(&db, &ctx, &database, &q).await.unwrap();
        non_empty("content", &content.items);
        let actions = lists::actions(&db, &ctx, &database, &q).await.unwrap();
        non_empty("scheduled actions", &actions.items);
        let sessions = lists::sessions(&db, &ctx, &database, &q).await.unwrap();
        non_empty("sessions", &sessions.items);
        lap("lists");

        let order_id = &orders.items[0].id;
        let EntityView::Order(order) =
            entity_view(&db, &ctx, &catalog, &database, "order", order_id, false)
                .await
                .unwrap()
        else {
            panic!("expected an order view for #{order_id}");
        };
        lap("order view");
        eprintln!(
            "  order #{}: {} items, {} timeline events",
            order.doc.id,
            order.items.len(),
            order.timeline.len()
        );
        match order.payment.and_then(|p| p.transaction_id) {
            Some(tx) => {
                let hits = search(&db, &ctx, &database, &tx).await.unwrap();
                assert!(
                    hits.iter()
                        .any(|h| h.entity == "order" && h.id == order.doc.id),
                    "search by transaction id {tx}"
                );
                lap("search transaction id");
            }
            None => eprintln!("  order #{} has no transaction id", order.doc.id),
        }

        entity_view(
            &db,
            &ctx,
            &catalog,
            &database,
            "user",
            &customers.items[0].id,
            false,
        )
        .await
        .unwrap();
        lap("customer view");
        entity_view(
            &db,
            &ctx,
            &catalog,
            &database,
            "post",
            &products.items[0].id,
            false,
        )
        .await
        .unwrap();
        lap("product view");
        non_empty(
            "woocommerce options",
            &options_group(&db, &ctx, &catalog, &database, "woocommerce")
                .await
                .unwrap(),
        );
        lap("options group");
    }
}
