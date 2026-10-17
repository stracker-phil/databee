//! Rule actions of the built-in presets against the Docker fixtures (`dev/fixture.sql`), planned,
//! applied and undone on MariaDB and MySQL. Rows are only written in `wp_write`, and every test
//! starts by putting the rows it touches into a known state, so a run can follow any earlier one.

use std::collections::BTreeMap;
use std::time::Duration;

use tempfile::{TempDir, tempdir};

use super::{ObjectActionKind, for_table, plan};
use crate::address::Target;
use crate::db::changes::Plan;
use crate::document::context::Ctx;
use crate::document::open;
use crate::error::AppError;
use crate::rules::catalog::PresetCatalog;
use crate::rules::effective::RuleSources;
use crate::rules::project::ProjectsStore;
use crate::session::Session;
use crate::test_support::{connect_to, servers, spec};

const TIMEOUT: Duration = Duration::from_secs(10);
const DB: &str = "wp_write";
const ACTIONS: &str = "`wp_write`.`wp_actionscheduler_actions`";

type Snapshot = Vec<Vec<Option<String>>>;

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

/// One session on `wp_write` per fixture server, with the server's name.
async fn sessions() -> Vec<(String, Session)> {
    let mut out = Vec::new();
    for (host, port) in servers() {
        let db = connect_to(&host, port, DB).await;
        let session = Session::new(
            "it".into(),
            spec(&host, port, DB),
            format!("it-{port}"),
            Box::new(db),
        );
        out.push((format!("{host}:{port}"), session));
    }
    out
}

async fn ctx<'a>(session: &'a Session, rig: &Rig) -> Ctx<'a> {
    Ctx::load(session, DB, rig.sources(), TIMEOUT)
        .await
        .unwrap()
}

async fn run(ctx: &Ctx<'_>, statements: &[&str]) {
    let statements: Vec<String> = statements.iter().map(|s| s.to_string()).collect();
    ctx.session.db.apply(&statements, TIMEOUT).await.unwrap();
}

async fn snapshot(ctx: &Ctx<'_>, sql: &str) -> Snapshot {
    ctx.fetch(sql)
        .await
        .unwrap()
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.as_text().map(str::to_string))
                .collect()
        })
        .collect()
}

fn no_params() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn param(name: &str, value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(name.to_string(), value.to_string())])
}

async fn apply(ctx: &Ctx<'_>, plan: &Plan) {
    ctx.session
        .db
        .apply(&plan.statements, TIMEOUT)
        .await
        .unwrap();
}

async fn undo(ctx: &Ctx<'_>, plan: &Plan) {
    ctx.session.db.apply(&plan.undo, TIMEOUT).await.unwrap();
}

async fn reset_failed_action(ctx: &Ctx<'_>) {
    let sql = format!(
        "UPDATE {ACTIONS} SET status = 'failed', claim_id = 0, \
         scheduled_date_gmt = '2026-01-01 00:00:00' WHERE action_id = 1"
    );
    run(ctx, &[sql.as_str()]).await;
}

const RETRY_STATE: &str = "SELECT status, scheduled_date_gmt, claim_id FROM \
                           wp_write.wp_actionscheduler_actions WHERE action_id = 1";

#[tokio::test]
#[ignore]
async fn retry_on_a_failed_action_plans_an_update_with_undo() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        reset_failed_action(&ctx).await;
        let address = ctx.row_address("wp_actionscheduler_actions", "1");

        let plan = plan(&ctx, &address, "retry", &no_params())
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));

        assert_eq!(
            plan.description, "retry: Update 1 wp_actionscheduler_actions row",
            "{server}"
        );
        assert_eq!(
            plan.statements,
            [format!(
                "UPDATE {ACTIONS} SET `claim_id` = '0', `scheduled_date_gmt` = UTC_TIMESTAMP(), \
                 `status` = 'pending' WHERE `action_id` IN ('1')"
            )],
            "{server}"
        );
        assert_eq!(
            plan.undo,
            [format!(
                "UPDATE {ACTIONS} SET `claim_id` = '0', `scheduled_date_gmt` = '2026-01-01 00:00:00', \
                 `status` = 'failed' WHERE `action_id` = '1'"
            )],
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn applying_retry_and_undoing_it_restores_status_schedule_and_claim() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        reset_failed_action(&ctx).await;
        let before = snapshot(&ctx, RETRY_STATE).await;
        let address = ctx.row_address("wp_actionscheduler_actions", "1");
        let plan = plan(&ctx, &address, "retry", &no_params()).await.unwrap();

        apply(&ctx, &plan).await;
        let applied = snapshot(&ctx, RETRY_STATE).await;
        undo(&ctx, &plan).await;
        let restored = snapshot(&ctx, RETRY_STATE).await;

        assert_eq!(applied[0][0].as_deref(), Some("pending"), "{server}");
        assert_eq!(applied[0][2].as_deref(), Some("0"), "{server}");
        assert_ne!(applied[0][1], before[0][1], "{server}");
        assert_eq!(restored, before, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn retry_is_listed_only_for_the_actions_it_applies_to() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        reset_failed_action(&ctx(&session, &rig).await).await;
        let slug = session.slug();
        let document = |id: &str| format!("databee://{slug}/{DB}/wp_actionscheduler_actions/{id}");

        let failed = open(&session, &document("1"), None, rig.sources(), TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));
        let complete = open(&session, &document("2"), None, rig.sources(), TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));

        let retry = failed.actions.iter().find(|a| a.name == "retry");
        let retry = retry.unwrap_or_else(|| panic!("{server}: {:?}", failed.actions));
        assert_eq!(retry.kind, ObjectActionKind::Update, "{server}");
        assert!(retry.undo, "{server}");
        assert!(retry.params.is_empty(), "{server}");
        assert!(
            complete.actions.iter().all(|a| a.name != "retry"),
            "{server}: {:?}",
            complete.actions
        );
    }
}

#[tokio::test]
#[ignore]
async fn planning_retry_on_a_completed_action_is_refused() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        let address = ctx.row_address("wp_actionscheduler_actions", "2");

        let err = plan(&ctx, &address, "retry", &no_params())
            .await
            .unwrap_err();

        assert!(
            matches!(&err, AppError::InvalidInput(m) if m.contains("does not apply")),
            "{server}: {err:?}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn set_status_on_an_hpos_order_writes_the_order_and_its_posts_copy_and_undo_restores_both() {
    const ORDER: &str =
        "SELECT status, date_updated_gmt FROM wp_write.wp_wc_orders WHERE id = 4711";
    const POST: &str = "SELECT post_status, post_modified, post_modified_gmt \
                        FROM wp_write.wp_posts WHERE ID = 4711";
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        run(
            &ctx,
            &[
                "UPDATE wp_write.wp_wc_orders SET status = 'wc-processing', \
                 date_updated_gmt = '2026-01-01 10:00:00' WHERE id = 4711",
                "UPDATE wp_write.wp_posts SET post_status = 'wc-processing', \
                 post_modified = '2026-01-01 11:00:00', post_modified_gmt = '2026-01-01 10:00:00' \
                 WHERE ID = 4711",
            ],
        )
        .await;
        let order_before = snapshot(&ctx, ORDER).await;
        let post_before = snapshot(&ctx, POST).await;
        let address = ctx.row_address("wp_wc_orders", "4711");

        let plan = plan(
            &ctx,
            &address,
            "set_status",
            &param("status", "wc-completed"),
        )
        .await
        .unwrap_or_else(|e| panic!("{server}: {e:?}"));
        apply(&ctx, &plan).await;
        let order_applied = snapshot(&ctx, ORDER).await;
        let post_applied = snapshot(&ctx, POST).await;
        undo(&ctx, &plan).await;

        assert_eq!(
            plan.description, "set_status: Update 1 wp_wc_orders row, 1 wp_posts row",
            "{server}"
        );
        assert_eq!(
            plan.statements,
            [
                "UPDATE `wp_write`.`wp_wc_orders` SET `date_updated_gmt` = UTC_TIMESTAMP(), \
                 `status` = 'wc-completed' WHERE `id` IN ('4711')",
                "UPDATE `wp_write`.`wp_posts` SET `post_modified` = NOW(), \
                 `post_modified_gmt` = UTC_TIMESTAMP(), `post_status` = 'wc-completed' \
                 WHERE `ID` IN ('4711')",
            ],
            "{server}"
        );
        assert_eq!(
            order_applied[0][0].as_deref(),
            Some("wc-completed"),
            "{server}"
        );
        assert_eq!(
            post_applied[0][0].as_deref(),
            Some("wc-completed"),
            "{server}"
        );
        assert_eq!(snapshot(&ctx, ORDER).await, order_before, "{server}");
        assert_eq!(snapshot(&ctx, POST).await, post_before, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn delete_expired_transients_through_a_table_address_deletes_by_key_and_undo_reinserts() {
    const EXPIRED: &str = "SELECT option_name FROM wp_write.wp_options \
                           WHERE option_name LIKE '%zz\\_exp' ORDER BY option_name";
    const ALL: &str = "SELECT * FROM wp_write.wp_options \
                       WHERE option_name <> 'zz_autoload_probe' ORDER BY option_id";
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        run(
            &ctx,
            &[
                "DELETE FROM wp_write.wp_options WHERE option_name LIKE '%zz\\_exp' \
                 OR option_name LIKE '%zz\\_live'",
                "INSERT INTO wp_write.wp_options (option_name, option_value, autoload) VALUES \
                 ('_transient_zz_exp', 'v', 'no'), ('_transient_timeout_zz_exp', '1000', 'no'), \
                 ('_transient_zz_live', 'v', 'no'), ('_transient_timeout_zz_live', '4102444800', 'no')",
            ],
        )
        .await;
        let before = snapshot(&ctx, ALL).await;
        let address = ctx.address(Target::Table {
            table: "wp_options".into(),
            filter: Vec::new(),
        });

        let actions = for_table(&ctx, "wp_options").await;
        let plan = plan(&ctx, &address, "delete_expired_transients", &no_params())
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));
        let listed = actions
            .iter()
            .find(|a| a.name == "delete_expired_transients");
        apply(&ctx, &plan).await;
        let after = snapshot(&ctx, ALL).await;
        let gone = snapshot(&ctx, EXPIRED).await;
        undo(&ctx, &plan).await;

        let listed = listed.unwrap_or_else(|| panic!("{server}: {actions:?}"));
        assert_eq!(listed.kind, ObjectActionKind::Delete, "{server}");
        assert!(listed.undo, "{server}");
        assert_eq!(plan.statements.len(), 1, "{server}");
        assert!(
            plan.statements[0]
                .starts_with("DELETE FROM `wp_write`.`wp_options` WHERE `option_id` IN ("),
            "{server}: {}",
            plan.statements[0]
        );
        assert!(plan.undo.len() >= 2, "{server}: {:?}", plan.undo);
        assert!(gone.is_empty(), "{server}: {gone:?}");
        assert!(after.len() < before.len(), "{server}");
        assert!(
            after
                .iter()
                .any(|row| row[1].as_deref() == Some("_transient_zz_live")),
            "{server}: unexpired transients stay"
        );
        assert_eq!(snapshot(&ctx, ALL).await, before, "{server}");
    }
}

const PROBE_STATE: &str =
    "SELECT option_id, autoload FROM wp_write.wp_options WHERE option_name = 'zz_autoload_probe'";

async fn reset_autoload_probe(ctx: &Ctx<'_>) -> String {
    run(
        ctx,
        &["UPDATE wp_write.wp_options SET autoload = 'yes' WHERE option_name = 'zz_autoload_probe'"],
    )
    .await;
    snapshot(ctx, PROBE_STATE).await[0][0].clone().unwrap()
}

#[tokio::test]
#[ignore]
async fn set_autoload_plans_an_update_of_the_option_with_undo_to_the_old_value() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        let id = reset_autoload_probe(&ctx).await;
        let address = ctx.row_address("wp_options", &id);

        let plan = plan(&ctx, &address, "set_autoload", &param("autoload", "no"))
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"));

        assert_eq!(
            plan.description, "set_autoload: Update 1 wp_options row",
            "{server}"
        );
        assert_eq!(
            plan.statements,
            [format!(
                "UPDATE `wp_write`.`wp_options` SET `autoload` = 'no' WHERE `option_id` IN ('{id}')"
            )],
            "{server}"
        );
        assert_eq!(
            plan.undo,
            [format!(
                "UPDATE `wp_write`.`wp_options` SET `autoload` = 'yes' WHERE `option_id` = '{id}'"
            )],
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn applying_set_autoload_and_undoing_it_restores_the_options_autoload() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        let id = reset_autoload_probe(&ctx).await;
        let before = snapshot(&ctx, PROBE_STATE).await;
        let address = ctx.row_address("wp_options", &id);
        let plan = plan(&ctx, &address, "set_autoload", &param("autoload", "no"))
            .await
            .unwrap();

        apply(&ctx, &plan).await;
        let applied = snapshot(&ctx, PROBE_STATE).await;
        undo(&ctx, &plan).await;
        let restored = snapshot(&ctx, PROBE_STATE).await;

        assert_eq!(applied[0][1].as_deref(), Some("no"), "{server}");
        assert_eq!(restored, before, "{server}");
    }
}

#[tokio::test]
#[ignore]
async fn set_autoload_refuses_a_value_that_is_not_one_of_the_stored_autoload_values() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        let id = reset_autoload_probe(&ctx).await;
        let address = ctx.row_address("wp_options", &id);

        let err = plan(&ctx, &address, "set_autoload", &param("autoload", "maybe"))
            .await
            .unwrap_err();

        assert!(
            matches!(&err, AppError::InvalidInput(m) if m == "`maybe` is not one of the choices of `autoload`"),
            "{server}: {err:?}"
        );
        assert_eq!(
            snapshot(&ctx, PROBE_STATE).await[0][1].as_deref(),
            Some("yes"),
            "{server}"
        );
    }
}

#[tokio::test]
#[ignore]
async fn delete_logs_before_has_no_undo_and_its_description_carries_the_preview_count() {
    let rig = Rig::new();
    for (server, session) in sessions().await {
        let ctx = ctx(&session, &rig).await;
        let address = ctx.address(Target::Table {
            table: "wp_woocommerce_log".into(),
            filter: Vec::new(),
        });
        let expected = snapshot(
            &ctx,
            "SELECT COUNT(*) FROM wp_write.wp_woocommerce_log \
             WHERE timestamp < UTC_TIMESTAMP() - INTERVAL 30 DAY AND source = 'zz-retention'",
        )
        .await[0][0]
            .clone()
            .unwrap();

        let actions = for_table(&ctx, "wp_woocommerce_log").await;
        let plan = plan(
            &ctx,
            &address,
            "delete_logs_before",
            &param("source", "zz-retention"),
        )
        .await
        .unwrap_or_else(|e| panic!("{server}: {e:?}"));

        let listed = actions.iter().find(|a| a.name == "delete_logs_before");
        let listed = listed.unwrap_or_else(|| panic!("{server}: {actions:?}"));
        assert_eq!(listed.kind, ObjectActionKind::Sql, "{server}");
        assert!(!listed.undo, "{server}");
        assert_eq!(
            plan.description,
            format!(
                "delete_logs_before: free SQL on wp_woocommerce_log (no undo; preview_sql: {expected})"
            ),
            "{server}"
        );
        assert_eq!(
            plan.statements,
            [
                "DELETE FROM `wp_write`.wp_woocommerce_log WHERE timestamp < UTC_TIMESTAMP() \
              - INTERVAL 30 DAY AND ('zz-retention' IS NULL OR source = 'zz-retention')"
            ],
            "{server}"
        );
        assert!(plan.undo.is_empty(), "{server}");
    }
}
