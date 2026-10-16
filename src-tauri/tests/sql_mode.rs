//! Changes the servers' global `sql_mode`, so it is its own test binary: Cargo runs binaries
//! one after another and no other test sees the altered mode.

use std::time::Duration;

use databee_lib::connection::{ConnectionSpec, Secret};
use databee_lib::db::Cell;
use databee_lib::db::mysql::MySqlSession;
use databee_lib::wp::changes::{ChangeRequest, plan};
use databee_lib::wp::detect;
use databee_lib::wp::entity::{Catalog, cell_str, definitions};

const TIMEOUT: Duration = Duration::from_secs(10);

fn servers() -> Vec<(String, u16)> {
    let servers = std::env::var("DATABEE_TEST_SERVERS")
        .unwrap_or_else(|_| "127.0.0.1:33306,127.0.0.1:33307".into());
    servers
        .split(',')
        .map(|s| {
            let (host, port) = s.rsplit_once(':').unwrap();
            (host.to_string(), port.parse().unwrap())
        })
        .collect()
}

async fn connect_to(host: &str, port: u16, database: &str) -> MySqlSession {
    let spec = ConnectionSpec {
        name: None,
        host: host.into(),
        port,
        socket: None,
        user: "root".into(),
        database: Some(database.into()),
    };
    MySqlSession::connect(&spec, Some(&Secret::new("root")), TIMEOUT, TIMEOUT)
        .await
        .unwrap()
}

async fn scalar(db: &MySqlSession, sql: &str) -> Option<String> {
    let rs = db.fetch(sql, TIMEOUT).await.unwrap();
    rs.rows.first().and_then(|r| match &r[0] {
        Cell::Null => None,
        c => Some(cell_str(c).unwrap_or_default().to_string()),
    })
}

/// Puts the server's global `sql_mode` back when dropped, also after a failed assertion.
struct SqlModeGuard {
    host: String,
    port: u16,
    original: String,
}

impl Drop for SqlModeGuard {
    fn drop(&mut self) {
        let (host, port) = (self.host.clone(), self.port);
        let restore = format!("SET GLOBAL sql_mode = '{}'", self.original);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let admin = connect_to(&host, port, "wp_write").await;
                    admin.apply(&[restore], TIMEOUT).await.unwrap();
                });
        })
        .join()
        .unwrap();
    }
}

#[tokio::test]
#[ignore]
async fn no_backslash_escapes_writes_keep_a_single_backslash() {
    for (host, port) in servers() {
        let admin = connect_to(&host, port, "wp_write").await;
        let original = scalar(&admin, "SELECT @@GLOBAL.sql_mode")
            .await
            .unwrap_or_default();
        let _restore = SqlModeGuard {
            host: host.clone(),
            port,
            original,
        };
        admin
            .apply(
                &["SET GLOBAL sql_mode = CONCAT_WS(',', NULLIF(@@GLOBAL.sql_mode, ''), 'NO_BACKSLASH_ESCAPES')"
                    .to_string()],
                TIMEOUT,
            )
            .await
            .unwrap();

        let db = connect_to(&host, port, "wp_write").await;
        assert!(
            !db.dialect().backslash_escapes,
            "{host}:{port}: session ignored NO_BACKSLASH_ESCAPES"
        );
        let ctx = detect(&db, "wp_write").await.unwrap().unwrap();
        let catalog = Catalog::new(&definitions(None).unwrap(), &ctx);
        let change = plan(
            &db,
            Some(&ctx),
            Some(&catalog),
            &ChangeRequest::SetMeta {
                database: "wp_write".into(),
                entity: "order".into(),
                id: "4711".into(),
                key: "_backslash_probe".into(),
                value: r"a\b".into(),
            },
        )
        .await
        .unwrap();
        db.apply(&change.statements, TIMEOUT).await.unwrap();
        let read = "SELECT meta_value FROM wp_write.wp_wc_orders_meta WHERE order_id = 4711 AND meta_key = '_backslash_probe'";
        let stored = scalar(&db, read).await;
        db.apply(&change.undo, TIMEOUT).await.unwrap();

        assert_eq!(stored.as_deref(), Some(r"a\b"), "{host}:{port}");
        assert_eq!(scalar(&db, read).await, None, "{host}:{port}: undo");
    }
}
