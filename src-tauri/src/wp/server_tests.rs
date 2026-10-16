//! Integration tests against real servers; every one is `#[ignore]`
//! (`cargo test --lib -- --ignored wp::`).
//!
//! - The lens against the Docker fixtures (`bun run test:db`): reads go to `wp_test` (HPOS with
//!   sync), edits to its copy `wp_write`, and `wp_legacy` is a posts-based store (HPOS off).
//! - Read-only checks against a real WordPress database of your own, skipped unless
//!   `DATABEE_WP_TEST=mysql://user:pass@host:port/db` is set:
//!
//!   DATABEE_WP_TEST=mysql://root:root@127.0.0.1:52958/db cargo test --lib -- --ignored --nocapture every_serialized_value_round_trips

use std::collections::HashMap;
use std::time::Duration;

use crate::cli::{self, CliOutcome, Launch};
use crate::db::Cell;
use crate::db::Database;
use crate::db::mysql::MySqlSession;
use crate::decode::php;
use crate::test_support::sessions;
use crate::wp::context::{HposMode, SiteInfo, SiteZone};
use crate::wp::dashboard::{Granularity, action_histogram, browser, dashboard};
use crate::wp::detail::{EntityView, EventKind, EventLevel, entity_view, search};
use crate::wp::entity::{Catalog, definitions};
use crate::wp::lists::{self, ListQuery};
use crate::wp::{WpList, detect, expand, load_docs, options_overview, post_images};

const TIMEOUT: Duration = Duration::from_secs(10);

/// The database named by `DATABEE_WP_TEST` (a `databee` connection URL), connected.
async fn real_site() -> Option<(MySqlSession, String)> {
    let url = std::env::var("DATABEE_WP_TEST").ok()?;
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
    Some((db, database))
}

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
        assert!(d.hpos_drift.is_some());
        assert!(
            d.tables
                .iter()
                .any(|t| t.name == "wp_woocommerce_log" && t.category == "logs")
        );
        assert!(
            d.insights
                .iter()
                .any(|i| i.text.contains("scheduled actions failed"))
        );
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
    for (db, ctx, _) in env().await {
        let asc = lists::orders(
            &db,
            &ctx,
            "wp_test",
            "shop_order",
            &lists::ListQuery {
                sort: Some("total".into()),
                descending: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let desc = lists::orders(
            &db,
            &ctx,
            "wp_test",
            "shop_order",
            &lists::ListQuery {
                sort: Some("total".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(asc.items.len(), 2);
        assert_eq!(asc.items.first().unwrap().id, desc.items.last().unwrap().id);
        assert_ne!(
            asc.items.first().unwrap().id,
            desc.items.first().unwrap().id
        );
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
        let facts = crate::wp::peek::peek(&db, &ctx, &catalog, "wp_test", "order", "4711")
            .await
            .unwrap();
        let labels: Vec<&str> = facts.iter().map(|f| f.label.as_str()).collect();
        assert!(
            labels.contains(&"Status") && labels.contains(&"Total"),
            "{labels:?}"
        );
        assert!(facts.len() <= 5);
        let total = facts.iter().find(|f| f.label == "Total").unwrap();
        assert_eq!(
            (total.kind, total.value.as_str()),
            (crate::wp::peek::FactKind::Money, "€49.90")
        );
        let user = crate::wp::peek::peek(&db, &ctx, &catalog, "wp_test", "user", "1")
            .await
            .unwrap();
        assert!(user.iter().any(|f| f.label == "Email"));
        let buyer = crate::wp::peek::peek(&db, &ctx, &catalog, "wp_test", "user", "12")
            .await
            .unwrap();
        let labels: Vec<&str> = buyer.iter().map(|f| f.label.as_str()).collect();
        assert!(
            labels.contains(&"Orders") && labels.contains(&"Spent"),
            "{labels:?}"
        );
        assert_eq!(
            labels.iter().position(|l| *l == "Orders"),
            Some(1),
            "{labels:?}"
        );
        assert!(
            crate::wp::peek::peek(&db, &ctx, &catalog, "wp_test", "order", "999999")
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
        let groups = crate::wp::peek::find_value(&db, &ctx, "wp_test", "66M12696JX411763D")
            .await
            .unwrap();
        let sources: Vec<&str> = groups.iter().map(|g| g.source.as_str()).collect();
        assert!(sources.contains(&"WooCommerce log"), "{sources:?}");
        assert!(sources.iter().any(|s| s.ends_with("meta")), "{sources:?}");
        let meta = groups.iter().find(|g| g.source.ends_with("meta")).unwrap();
        assert!(meta.count >= 1);
        assert_eq!(meta.hits[0].column, "meta_value");
        assert!(meta.hits[0].entity.is_some());
        assert!(
            crate::wp::peek::find_value(&db, &ctx, "wp_test", "no-such-value-anywhere")
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

#[tokio::test]
#[ignore]
async fn edits_apply_and_undo_cleanly() {
    use crate::db::changes::{ChangeRequest as R, plan as plan_row};
    use crate::wp::changes::{WpChangeRequest as C, plan};
    for (db, ctx, catalog) in write_env().await {
        let p = |c: C| {
            let (db, ctx, catalog) = (&db, &ctx, &catalog);
            async move { plan(db, ctx, catalog, &c).await.unwrap() }
        };
        let row = |c: R| {
            let db = &db;
            async move { plan_row(db, &c, TIMEOUT).await.unwrap() }
        };
        let d = || "wp_write".to_string();

        // Order status in wc_orders and the synced posts copy.
        let change = p(C::SetOrderStatus {
            database: d(),
            id: "4711".into(),
            status: "completed".into(),
        })
        .await;
        assert_eq!(change.statements.len(), 2, "HPOS table and posts copy");
        apply(&db, &change.statements).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT status FROM wp_write.wp_wc_orders WHERE id = 4711"
            )
            .await
            .as_deref(),
            Some("wc-completed")
        );
        assert_eq!(
            scalar(
                &db,
                "SELECT post_status FROM wp_write.wp_posts WHERE ID = 4711"
            )
            .await
            .as_deref(),
            Some("wc-completed")
        );
        apply(&db, &change.undo).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT status FROM wp_write.wp_wc_orders WHERE id = 4711"
            )
            .await
            .as_deref(),
            Some("wc-processing")
        );

        // New meta key, then undo removes it.
        let change = p(C::SetMeta {
            database: d(),
            entity: "order".into(),
            id: "4711".into(),
            key: "_debug_flag".into(),
            value: "1".into(),
        })
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, "SELECT meta_value FROM wp_write.wp_wc_orders_meta WHERE order_id = 4711 AND meta_key = '_debug_flag'").await.as_deref(), Some("1"));
        apply(&db, &change.undo).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT COUNT(*) FROM wp_write.wp_wc_orders_meta WHERE meta_key = '_debug_flag'"
            )
            .await
            .as_deref(),
            Some("0")
        );

        // Serialized option edited via a path keeps valid lengths.
        let raw = scalar(&db, "SELECT option_value FROM wp_write.wp_options WHERE option_name = 'woocommerce_ppcp-settings'").await.unwrap();
        let edited = crate::decode::edit::edit_serialized(
            &raw,
            &[1],
            &crate::decode::edit::NewValue::String("PayPal ✓".into()),
        )
        .unwrap();
        let change = row(R::SetCell {
            database: d(),
            table: "wp_options".into(),
            key_column: "option_name".into(),
            key: "woocommerce_ppcp-settings".into(),
            column: "option_value".into(),
            value: Some(edited),
        })
        .await;
        apply(&db, &change.statements).await;
        let stored = scalar(&db, "SELECT option_value FROM wp_write.wp_options WHERE option_name = 'woocommerce_ppcp-settings'").await.unwrap();
        assert!(
            crate::decode::php::parse(stored.as_bytes()).is_ok(),
            "{stored}"
        );
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, "SELECT option_value FROM wp_write.wp_options WHERE option_name = 'woocommerce_ppcp-settings'").await.unwrap(), raw);

        // Expired transients: delete and restore.
        let change = p(C::DeleteExpiredTransients { database: d() }).await;
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

        // Autoload keeps the legacy yes/no style of this install.
        let change = p(C::SetAutoload {
            database: d(),
            option: "woocommerce_ppcp-settings".into(),
            on: false,
        })
        .await;
        assert!(change.statements[0].contains("autoload = 'no'"));

        // Retrying a failed action makes it pending again.
        let change = p(C::RetryAction {
            database: d(),
            id: "1".into(),
        })
        .await;
        apply(&db, &change.statements).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT status FROM wp_write.wp_actionscheduler_actions WHERE action_id = 1"
            )
            .await
            .as_deref(),
            Some("pending")
        );
        apply(&db, &change.undo).await;
        assert_eq!(
            scalar(
                &db,
                "SELECT status FROM wp_write.wp_actionscheduler_actions WHERE action_id = 1"
            )
            .await
            .as_deref(),
            Some("failed")
        );

        // Deleting a row restores it exactly.
        let change = row(R::DeleteRow {
            database: d(),
            table: "wp_postmeta".into(),
            key_column: "meta_id".into(),
            key: "1".into(),
        })
        .await;
        let before = scalar(&db, "SELECT CONCAT(post_id, meta_key, meta_value) FROM wp_write.wp_postmeta WHERE meta_id = 1").await;
        apply(&db, &change.statements).await;
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, "SELECT CONCAT(post_id, meta_key, meta_value) FROM wp_write.wp_postmeta WHERE meta_id = 1").await, before);

        // A failing statement rolls back the whole change.
        let err = db
            .apply(
                &[
                    "UPDATE wp_write.wp_wc_orders SET status = 'wc-x' WHERE id = 4713".into(),
                    "UPDATE wp_write.nope SET a = 1".into(),
                ],
                TIMEOUT,
            )
            .await;
        assert!(err.is_err());
        assert_eq!(
            scalar(
                &db,
                "SELECT status FROM wp_write.wp_wc_orders WHERE id = 4713"
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
    use crate::wp::snapshot::{Scope, capture, compare, diff};
    for (db, ctx, catalog) in write_env().await {
        let scope = Scope {
            entity: Some("order".into()),
            id: Some("4713".into()),
            include_related: true,
            // The edit test changes ppcp options concurrently; watch one nothing else touches.
            option_pattern: Some("blogname".into()),
            include_actions: true,
        };
        apply(
            &db,
            &["DELETE FROM wp_write.wp_wc_orders_meta WHERE meta_key = '_snap'".into()],
        )
        .await;
        let before = capture(&db, &ctx, &catalog, "wp_write", &scope)
            .await
            .unwrap();
        assert_eq!(before["order#4713/field/status"], "wc-failed");
        assert_eq!(before["options/blogname"], "Test Shop");
        assert!(
            before
                .keys()
                .any(|k| k.starts_with("items/order_item#3/meta/_qty"))
        );
        apply(&db, &["UPDATE wp_write.wp_wc_orders_meta SET meta_value = meta_value WHERE id = 0".into(), "INSERT INTO wp_write.wp_wc_orders_meta (order_id, meta_key, meta_value) VALUES (4713, '_snap', 'x')".into()]).await;
        let after = capture(&db, &ctx, &catalog, "wp_write", &scope)
            .await
            .unwrap();
        let changes = diff(&before, &after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "order#4713/meta/_snap");
        assert_eq!(changes[0].after.as_deref(), Some("x"));
        apply(
            &db,
            &["DELETE FROM wp_write.wp_wc_orders_meta WHERE meta_key = '_snap'".into()],
        )
        .await;

        let rows = compare(&db, &ctx, &catalog, "wp_write", "order", "4711", "4713")
            .await
            .unwrap();
        let status = rows.iter().find(|r| r.path == "field/status").unwrap();
        assert_eq!(
            (status.a.as_deref(), status.b.as_deref()),
            (Some("wc-processing"), Some("wc-failed"))
        );
        assert!(
            rows.iter()
                .any(|r| r.path.starts_with("meta/_ppcp_paypal_fees/") && r.b.is_none())
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
) -> crate::db::changes::ChangeRequest {
    crate::db::changes::ChangeRequest::SetCell {
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
    use crate::db::changes::plan;
    for db in sessions("wp_test").await {
        let read = "SELECT post_name FROM wp_write.wp_posts WHERE ID = 201";
        assert_eq!(scalar(&db, read).await.as_deref(), Some(""));
        let change = plan(
            &db,
            &set_cell("wp_posts", "ID", "201", "post_name", Some("x")),
            TIMEOUT,
        )
        .await
        .unwrap();
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
    use crate::db::changes::plan;
    for db in sessions("wp_test").await {
        let id = scalar(
            &db,
            "SELECT id FROM wp_write.wp_wc_order_addresses WHERE order_id = 4711 AND address_type = 'shipping'",
        )
        .await
        .unwrap();
        let read = format!("SELECT email FROM wp_write.wp_wc_order_addresses WHERE id = {id}");
        assert_eq!(scalar(&db, &read).await, None);
        let change = plan(
            &db,
            &set_cell(
                "wp_wc_order_addresses",
                "id",
                &id,
                "email",
                Some("ship@shop.test"),
            ),
            TIMEOUT,
        )
        .await
        .unwrap();
        apply(&db, &change.statements).await;
        assert_eq!(scalar(&db, &read).await.as_deref(), Some("ship@shop.test"));
        apply(&db, &change.undo).await;
        assert_eq!(scalar(&db, &read).await, None, "restored as NULL");
    }
}

#[tokio::test]
#[ignore]
async fn editing_a_binary_cell_has_no_undo() {
    use crate::db::changes::{ChangeRequest, plan};
    for db in sessions("wp_test").await {
        let change = plan(
            &db,
            &ChangeRequest::SetCell {
                database: "databee_test".into(),
                table: "types".into(),
                key_column: "id".into(),
                key: "1".into(),
                column: "blob_data".into(),
                value: Some("x".into()),
            },
            TIMEOUT,
        )
        .await
        .unwrap();
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
    use crate::db::changes::plan;
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
        let change = plan(
            &db,
            &set_cell(
                "wp_options",
                "option_name",
                "zzz_mystery_setting",
                "option_name",
                Some("zzz_renamed"),
            ),
            TIMEOUT,
        )
        .await
        .unwrap();
        apply(&db, &change.statements).await;
        assert_eq!(count("zzz_renamed").await.as_deref(), Some("1"));
        assert_eq!(count("zzz_mystery_setting").await.as_deref(), Some("0"));
        apply(&db, &change.undo).await;
        assert_eq!(count("zzz_mystery_setting").await.as_deref(), Some("1"));
        assert_eq!(count("zzz_renamed").await.as_deref(), Some("0"));
    }
}

#[tokio::test]
#[ignore]
async fn set_meta_refuses_duplicate_rows() {
    use crate::wp::changes::{WpChangeRequest, plan};
    for (db, ctx, catalog) in write_env().await {
        apply(
            &db,
            &["INSERT INTO wp_write.wp_usermeta (user_id, meta_key, meta_value) VALUES (12, 'billing_city', 'Graz')".into()],
        )
        .await;
        let result = plan(
            &db,
            &ctx,
            &catalog,
            &WpChangeRequest::SetMeta {
                database: "wp_write".into(),
                entity: "user".into(),
                id: "12".into(),
                key: "billing_city".into(),
                value: "Linz".into(),
            },
        )
        .await;
        apply(
            &db,
            &["DELETE FROM wp_write.wp_usermeta WHERE user_id = 12 AND meta_key = 'billing_city' AND meta_value = 'Graz'".into()],
        )
        .await;
        let err = result.expect_err("two rows of one meta key");
        let message = err.to_string();
        assert!(
            message.contains("billing_city") && message.contains('2'),
            "{message}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn delete_meta_of_a_missing_key_is_refused() {
    use crate::wp::changes::{WpChangeRequest, plan};
    for (db, ctx, catalog) in write_env().await {
        let result = plan(
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
    use crate::wp::changes::{WpChangeRequest, plan};
    for (db, ctx, catalog) in write_env().await {
        let change = plan(
            &db,
            &ctx,
            &catalog,
            &WpChangeRequest::DeleteLogsBefore {
                database: "wp_write".into(),
                days: 0,
                source: None,
            },
        )
        .await
        .unwrap();
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

// ---------- real data (DATABEE_WP_TEST, read-only) ----------

#[tokio::test]
#[ignore]
async fn every_serialized_value_round_trips() {
    let Some((db, database)) = real_site().await else {
        eprintln!("DATABEE_WP_TEST not set; skipping");
        return;
    };
    let ctx = detect(&db, &database)
        .await
        .unwrap()
        .expect("a WordPress database");

    let mut sources = vec![
        ("options", "option_value"),
        ("postmeta", "meta_value"),
        ("usermeta", "meta_value"),
    ];
    if ctx.hpos() {
        sources.push(("wc_orders_meta", "meta_value"));
    }
    if ctx.wc.is_some() {
        sources.push(("woocommerce_order_itemmeta", "meta_value"));
    }

    let (mut checked, mut lenient, mut broken) = (0, 0, 0);
    for (table, column) in sources {
        let sql = format!(
            "SELECT `{column}` FROM `{database}`.`{}{table}` WHERE `{column}` REGEXP '^(a|O|s|i|b|d|C|E):' OR `{column}` = 'N;'",
            ctx.prefix
        );
        let rs = db.fetch(&sql, Duration::from_secs(60)).await.unwrap();
        for row in &rs.rows {
            let Cell::Text(value) = &row[0] else { continue };
            match php::parse(value.as_bytes()) {
                Ok(parsed) => {
                    checked += 1;
                    assert_eq!(
                        php::serialize(&parsed),
                        value.as_bytes(),
                        "round trip failed in {table}: {value}"
                    );
                }
                Err(_) if php::parse_lenient(value.as_bytes()).is_ok() => lenient += 1,
                Err(e) => {
                    broken += 1;
                    eprintln!(
                        "unparseable in {table} ({e}): {}",
                        value.chars().take(120).collect::<String>()
                    );
                }
            }
        }
    }
    eprintln!(
        "round-tripped {checked} values; {lenient} needed lenient parsing; {broken} unparseable"
    );
    assert!(checked > 0);
}

/// Read-only smoke test of every WordPress-mode query against a real database.
#[tokio::test]
#[ignore]
async fn wordpress_mode_queries_work_on_real_data() {
    use crate::wp::detail::{entity_view, search};
    use crate::wp::entity::{Catalog, definitions};
    use crate::wp::lists::{self, ListQuery};
    use crate::wp::{dashboard, options_group};

    let Some((db, database)) = real_site().await else {
        return;
    };
    let ctx = detect(&db, &database)
        .await
        .unwrap()
        .expect("a WordPress database");
    let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
    let started = std::time::Instant::now();
    let lap = |what: &str| eprintln!("{what:<28} {:>6} ms", started.elapsed().as_millis());

    let d = dashboard::dashboard(&db, &ctx, &database).await.unwrap();
    lap("dashboard");
    eprintln!(
        "  insights: {:?}",
        d.insights.iter().map(|i| &i.text).collect::<Vec<_>>()
    );
    dashboard::browser(&db, &ctx, &database).await.unwrap();
    lap("browser");
    let q = ListQuery::default();
    let orders = lists::orders(&db, &ctx, &database, "shop_order", &q)
        .await
        .unwrap();
    lists::orders(&db, &ctx, &database, "shop_subscription", &q)
        .await
        .unwrap();
    let customers = lists::users(&db, &ctx, &database, &q, false).await.unwrap();
    let products = lists::products(&db, &ctx, &database, &q).await.unwrap();
    lists::content(&db, &ctx, &database, &q).await.unwrap();
    if ctx.has_table("actionscheduler_actions") {
        lists::actions(&db, &ctx, &database, &q).await.unwrap();
    }
    if ctx.has_table("woocommerce_sessions") {
        lists::sessions(&db, &ctx, &database, &q).await.unwrap();
    }
    lap("lists");
    if let Some(o) = orders.items.first() {
        let v = entity_view(&db, &ctx, &catalog, &database, "order", &o.id, false)
            .await
            .unwrap();
        lap("order view");
        if let crate::wp::detail::EntityView::Order(o) = v {
            eprintln!(
                "  order #{}: {} items, {} timeline events",
                o.doc.id,
                o.items.len(),
                o.timeline.len()
            );
            if let Some(tx) = o.payment.and_then(|p| p.transaction_id) {
                let hits = search(&db, &ctx, &database, &tx).await.unwrap();
                assert!(
                    hits.iter().any(|h| h.entity == "order" && h.id == o.doc.id),
                    "search by transaction id"
                );
                lap("search transaction id");
            }
        }
    }
    if let Some(c) = customers.items.first() {
        entity_view(&db, &ctx, &catalog, &database, "user", &c.id, false)
            .await
            .unwrap();
        lap("customer view");
    }
    if let Some(p) = products.items.first() {
        entity_view(&db, &ctx, &catalog, &database, "post", &p.id, false)
            .await
            .unwrap();
        lap("product view");
    }
    options_group(&db, &ctx, &catalog, &database, "woocommerce")
        .await
        .unwrap();
    lap("options group");
}
