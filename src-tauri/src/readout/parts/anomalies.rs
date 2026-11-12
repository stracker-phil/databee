use futures_util::future::join_all;

use super::pinned::query_address;
use crate::address::{Address, Target};
use crate::error::AppResult;
use crate::query::Ctx;
use crate::readout::{AnomalyFix, AnomalyLine};
use crate::rules::actions::ActionScope;
use crate::rules::queries::{Anomaly, AnomalyOpen};

pub async fn anomalies(ctx: &Ctx<'_>) -> AppResult<Vec<AnomalyLine>> {
    let rules: Vec<(&str, &Anomaly)> = ctx
        .rules
        .applied
        .presets
        .iter()
        .flat_map(|p| p.anomalies.iter().map(|a| (p.name.as_str(), a)))
        .collect();
    let lines = join_all(
        rules
            .into_iter()
            .map(|(preset, rule)| anomaly(ctx, preset, rule)),
    )
    .await;
    Ok(lines.into_iter().flatten().collect())
}

/// The line of an anomaly that shows: its value is a non-zero number, or its query failed.
async fn anomaly(ctx: &Ctx<'_>, preset: &str, rule: &Anomaly) -> Option<AnomalyLine> {
    let (count, error) = match ctx.fetch(&ctx.preset_sql(&rule.sql)).await {
        Ok(result) => {
            let count = result
                .rows
                .first()
                .and_then(|row| row.first()?.as_text().map(str::to_string));
            if !count.as_deref().is_some_and(is_non_zero) {
                return None;
            }
            (count, None)
        }
        Err(e) => (None, Some(e.to_string())),
    };
    let (address, filter) = anomaly_open(ctx, rule.open.as_ref());
    Some(AnomalyLine {
        id: rule.id.clone(),
        preset: preset.to_string(),
        text: ctx.rules.applied.expand(&rule.text),
        count,
        address,
        filter,
        fix: rule
            .fix
            .as_deref()
            .and_then(|fix| anomaly_fix(ctx, preset, fix)),
        error,
    })
}

fn is_non_zero(text: &str) -> bool {
    text.trim().parse::<f64>().is_ok_and(|n| n != 0.0)
}

fn anomaly_open(ctx: &Ctx<'_>, open: Option<&AnomalyOpen>) -> (Option<Address>, Option<String>) {
    match open {
        Some(AnomalyOpen::Pinned { pinned }) => (Some(query_address(ctx, pinned)), None),
        Some(AnomalyOpen::Table { table, r#where }) => {
            let Some(table) = ctx.rules.applied.existing_table(table) else {
                return (None, None);
            };
            let address = ctx.address(Target::Table {
                table,
                filter: Vec::new(),
            });
            (
                Some(address),
                r#where.as_deref().map(|w| ctx.rules.applied.expand(w)),
            )
        }
        None => (None, None),
    }
}

fn anomaly_fix(ctx: &Ctx<'_>, preset: &str, fix: &str) -> Option<AnomalyFix> {
    let action =
        ctx.rules.actions.iter().find(|a| {
            a.preset == preset && a.rule.name == fix && a.rule.scope == ActionScope::Table
        })?;
    Some(AnomalyFix {
        action: fix.to_string(),
        address: ctx.address(Target::Table {
            table: action.rule.table.clone(),
            filter: Vec::new(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, FakeDatabase};
    use crate::error::AppError;
    use crate::query::testing::{num, result, shop, text};
    use crate::readout::parts::tests::{one, table};
    use crate::readout::testing::Rig;

    const ANOMALIES: &str = r#"
[[pinned]]
id = "flagged"
sql = "SELECT 1"

[[action]]
name = "purge"
table = "orders"
scope = "table"
delete = { where = "status = 'old'" }

[[anomaly]]
id = "zero"
sql = "SELECT 'z0' FROM {p}orders"
text = "zero"

[[anomaly]]
id = "decimal_zero"
sql = "SELECT 'z00' FROM {p}orders"
text = "decimal zero"

[[anomaly]]
id = "null"
sql = "SELECT 'nul' FROM {p}orders"
text = "null"

[[anomaly]]
id = "no_row"
sql = "SELECT 'nor' FROM {p}orders"
text = "no row"

[[anomaly]]
id = "words"
sql = "SELECT 'txt' FROM {p}orders"
text = "words"

[[anomaly]]
id = "many"
sql = "SELECT 'many' FROM {p}orders"
text = "{p}orders flagged"
open = { pinned = "flagged" }
fix = "purge"

[[anomaly]]
id = "tbl"
sql = "SELECT 'tbl' FROM {p}orders"
text = "tbl"
open = { table = "orders", where = "status = 'x'" }

[[anomaly]]
id = "ghost"
sql = "SELECT 'ghost' FROM {p}orders"
text = "ghost"
open = { table = "ghosts", where = "a = 1" }

[[anomaly]]
id = "bad"
sql = "SELECT 'bad' FROM {p}orders"
text = "bad"
open = { table = "orders" }
"#;

    const OTHER: &str = r#"
[preset]
name = "aaa"
marker_tables = ["people"]

[[action]]
name = "purge"
table = "people"
scope = "table"
delete = { where = "name = ''" }
"#;

    fn anomaly_db() -> FakeDatabase {
        shop()
            .on("'z0'", one(num("0")))
            .on("'z00'", one(text("0.00")))
            .on("'nul'", one(Cell::Null))
            .on("'nor'", result(&["c"], Vec::new()))
            .on("'txt'", one(text("n/a")))
            .on("'many'", one(num("3")))
            .on("'tbl'", one(num("2")))
            .on("'ghost'", one(num("4")))
            .failing("'bad'", AppError::InvalidInput("boom".into()))
    }

    async fn anomaly_lines() -> Vec<AnomalyLine> {
        let rig = Rig::new(anomaly_db(), ANOMALIES, &[("aaa.toml", OTHER)]);
        let ctx = rig.ctx().await;
        anomalies(&ctx).await.unwrap()
    }

    fn line<'a>(lines: &'a [AnomalyLine], id: &str) -> &'a AnomalyLine {
        lines
            .iter()
            .find(|l| l.id == id)
            .unwrap_or_else(|| panic!("no anomaly line {id}"))
    }

    #[tokio::test]
    async fn anomalies_with_zero_null_no_row_or_non_numeric_values_stay_hidden() {
        let lines = anomaly_lines().await;

        let ids: Vec<&str> = lines.iter().map(|l| l.id.as_str()).collect();

        assert_eq!(ids, ["many", "tbl", "ghost", "bad"]);
    }

    #[tokio::test]
    async fn a_non_zero_anomaly_shows_its_count_prefixed_text_pinned_address_and_fix() {
        let rig = Rig::new(anomaly_db(), ANOMALIES, &[("aaa.toml", OTHER)]);
        let ctx = rig.ctx().await;

        let lines = anomalies(&ctx).await.unwrap();

        assert_eq!(
            line(&lines, "many"),
            &AnomalyLine {
                id: "many".into(),
                preset: "doc".into(),
                text: "f_orders flagged".into(),
                count: Some("3".into()),
                address: Some(ctx.address(Target::Query {
                    id: "flagged".into()
                })),
                filter: None,
                fix: Some(AnomalyFix {
                    action: "purge".into(),
                    address: table(&ctx, "f_orders"),
                }),
                error: None,
            }
        );
    }

    #[tokio::test]
    async fn an_anomaly_query_runs_qualified_with_the_database_and_prefix() {
        let rig = Rig::new(anomaly_db(), ANOMALIES, &[("aaa.toml", OTHER)]);
        let ctx = rig.ctx().await;

        anomalies(&ctx).await.unwrap();

        assert!(
            rig.db
                .queries()
                .contains(&"SELECT 'many' FROM `wp`.f_orders".to_string())
        );
    }

    #[tokio::test]
    async fn an_anomaly_opening_a_table_carries_its_address_and_filter() {
        let rig = Rig::new(anomaly_db(), ANOMALIES, &[("aaa.toml", OTHER)]);
        let ctx = rig.ctx().await;

        let lines = anomalies(&ctx).await.unwrap();

        let tbl = line(&lines, "tbl");
        assert_eq!(tbl.address, Some(table(&ctx, "f_orders")));
        assert_eq!(tbl.filter.as_deref(), Some("status = 'x'"));
        assert_eq!(tbl.fix, None);
    }

    #[tokio::test]
    async fn an_anomaly_opening_a_missing_table_has_no_address_or_filter() {
        let lines = anomaly_lines().await;

        let ghost = line(&lines, "ghost");

        assert_eq!(ghost.count.as_deref(), Some("4"));
        assert_eq!((ghost.address.clone(), ghost.filter.clone()), (None, None));
    }

    #[tokio::test]
    async fn an_anomaly_whose_query_fails_shows_the_error_instead_of_a_count() {
        let rig = Rig::new(anomaly_db(), ANOMALIES, &[("aaa.toml", OTHER)]);
        let ctx = rig.ctx().await;

        let lines = anomalies(&ctx).await.unwrap();

        let bad = line(&lines, "bad");
        assert_eq!(bad.count, None);
        assert_eq!(bad.error.as_deref(), Some("boom"));
        assert_eq!(bad.address, Some(table(&ctx, "f_orders")));
    }

    #[tokio::test]
    async fn a_fix_resolves_the_action_of_the_anomalys_own_preset_not_another_with_the_same_name() {
        let rig = Rig::new(anomaly_db(), ANOMALIES, &[("aaa.toml", OTHER)]);
        let ctx = rig.ctx().await;

        let lines = anomalies(&ctx).await.unwrap();

        assert_eq!(
            ctx.rules.applied.preset_names().len(),
            2,
            "both presets apply"
        );
        assert_eq!(
            line(&lines, "many").fix.as_ref().map(|f| f.address.clone()),
            Some(table(&ctx, "f_orders"))
        );
    }
}
