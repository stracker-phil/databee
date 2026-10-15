//! WordPress lens against the `wp_test` fixture (HPOS with sync) on both servers.

use std::time::Duration;

use databee_lib::connection::{ConnectionSpec, Secret};
use databee_lib::db::mysql::MySqlSession;
use databee_lib::wp::context::HposMode;
use databee_lib::wp::entity::{Catalog, definitions};
use databee_lib::wp::{detect, expand, load_docs, options_overview};

async fn sessions() -> Vec<MySqlSession> {
    let servers = std::env::var("DATABEE_TEST_SERVERS")
        .unwrap_or_else(|_| "127.0.0.1:33306,127.0.0.1:33307".into());
    let mut out = Vec::new();
    for s in servers.split(',') {
        let (host, port) = s.rsplit_once(':').unwrap();
        let spec = ConnectionSpec {
            name: None,
            host: host.into(),
            port: port.parse().unwrap(),
            socket: None,
            user: "root".into(),
            database: Some("wp_test".into()),
        };
        out.push(
            MySqlSession::connect(&spec, Some(&Secret::new("root")), Duration::from_secs(10))
                .await
                .unwrap(),
        );
    }
    out
}

#[tokio::test]
#[ignore]
async fn detects_woocommerce_with_hpos_sync() {
    for db in sessions().await {
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
async fn loads_an_order_document_with_meta_links_and_relation_counts() {
    for db in sessions().await {
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
        assert_eq!(customer.link.as_ref().unwrap().entity, "user");

        let ppcp = order.meta.iter().find(|g| g.name == "ppcp").unwrap();
        assert!(ppcp.private);
        assert_eq!(ppcp.entries.len(), 2);

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
    for db in sessions().await {
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

        let user = load_docs(&db, &ctx, &catalog, "wp_test", "user", &["12".into()])
            .await
            .unwrap();
        assert!(user[0].fields.iter().all(|f| f.name != "user_pass"));
    }
}

#[tokio::test]
#[ignore]
async fn options_overview_reports_groups_and_expired_transients() {
    for db in sessions().await {
        let ctx = detect(&db, "wp_test").await.unwrap().unwrap();
        let o = options_overview(&db, &ctx, "wp_test").await.unwrap();
        assert_eq!(o.total_count, 10);
        assert!(
            o.groups
                .iter()
                .any(|g| g.prefix == "woocommerce" && g.count == 4)
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
