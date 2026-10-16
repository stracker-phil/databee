//! Read-only property check against a real WordPress database:
//! every value that parses must re-serialize to the exact same bytes.
//!
//!   DATABEE_WP_TEST=mysql://root:root@127.0.0.1:52958/db cargo test --test php_roundtrip -- --ignored --nocapture

use std::time::Duration;

use databee_lib::connection::{ConnectionSpec, Secret};
use databee_lib::db::Cell;
use databee_lib::db::Database;
use databee_lib::db::mysql::MySqlSession;
use databee_lib::wp::{detect, php};

#[tokio::test]
#[ignore]
async fn every_serialized_value_round_trips() {
    let Ok(raw) = std::env::var("DATABEE_WP_TEST") else {
        eprintln!("DATABEE_WP_TEST not set; skipping");
        return;
    };
    let url = url::Url::parse(&raw).unwrap();
    let database = url.path().trim_start_matches('/').to_string();
    let spec = ConnectionSpec {
        name: None,
        host: url.host_str().unwrap().into(),
        port: url.port().unwrap_or(3306),
        socket: None,
        user: url.username().into(),
        database: Some(database.clone()),
    };
    let password = url.password().map(Secret::new);
    let db = MySqlSession::connect(
        &spec,
        password.as_ref(),
        Duration::from_secs(10),
        Duration::from_secs(10),
    )
    .await
    .unwrap();
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
    use databee_lib::wp::detail::{entity_view, search};
    use databee_lib::wp::entity::{Catalog, definitions};
    use databee_lib::wp::lists::{self, ListQuery};
    use databee_lib::wp::{dashboard, options_group};

    let Ok(raw) = std::env::var("DATABEE_WP_TEST") else {
        return;
    };
    let url = url::Url::parse(&raw).unwrap();
    let database = url.path().trim_start_matches('/').to_string();
    let spec = ConnectionSpec {
        name: None,
        host: url.host_str().unwrap().into(),
        port: url.port().unwrap_or(3306),
        socket: None,
        user: url.username().into(),
        database: Some(database.clone()),
    };
    let password = url.password().map(Secret::new);
    let db = MySqlSession::connect(
        &spec,
        password.as_ref(),
        Duration::from_secs(10),
        Duration::from_secs(10),
    )
    .await
    .unwrap();
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
        if let databee_lib::wp::detail::EntityView::Order(o) = v {
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
