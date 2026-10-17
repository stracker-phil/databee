//! The readout's parts. Each reads with the readout's `Ctx`; a line whose query fails carries
//! the error instead of its result.

use futures_util::future::join_all;

use super::sql::{counted, in_database, limited, single_statement};
use super::{
    AnomalyFix, AnomalyLine, NEW_ROWS, NewObjects, PINNED_ROWS, PinnedLine, PinnedResult,
    ReadoutRow, RecentLine, StatusValue, SummaryValue,
};
use crate::address::{Address, Target};
use crate::db::ResultSet;
use crate::document::Ctx;
use crate::document::peek;
use crate::error::AppResult;
use crate::lens::annotate_query;
use crate::rules::actions::ActionScope;
use crate::rules::effective::{AppliedEntity, RuleSources};
use crate::rules::queries::{Anomaly, AnomalyOpen, Home};
use crate::saved::SavedEntry;
use crate::store::recent_objects::RecentObject;
use crate::value::{ResultValues, Value, ValueKind};

pub async fn status_values(
    ctx: &Ctx<'_>,
    sources: RuleSources<'_>,
    entries: &[SavedEntry],
) -> AppResult<Vec<StatusValue>> {
    let shown = entries
        .iter()
        .filter(|e| matches!(e.pin, Some(Home::Value)));
    Ok(join_all(shown.map(|e| status_value(ctx, sources, e))).await)
}

async fn status_value(ctx: &Ctx<'_>, sources: RuleSources<'_>, entry: &SavedEntry) -> StatusValue {
    let mut line = StatusValue {
        id: entry.id.clone(),
        title: entry.title.clone(),
        sql: entry.sql.clone(),
        address: query_address(ctx, &entry.id),
        value: None,
        error: None,
    };
    match saved_query(ctx, sources, &entry.sql, str::to_string).await {
        Ok(result) => line.value = first_value(&result),
        Err(error) => line.error = Some(error),
    }
    line
}

pub async fn pinned(
    ctx: &Ctx<'_>,
    sources: RuleSources<'_>,
    entries: &[SavedEntry],
) -> AppResult<Vec<PinnedLine>> {
    let shown = entries
        .iter()
        .filter(|e| matches!(e.pin, Some(Home::Count | Home::Rows { .. })));
    Ok(join_all(shown.map(|e| pinned_line(ctx, sources, e))).await)
}

async fn pinned_line(ctx: &Ctx<'_>, sources: RuleSources<'_>, entry: &SavedEntry) -> PinnedLine {
    let result = match entry.pin {
        Some(Home::Rows { limit }) => pinned_rows(ctx, sources, entry, limit).await,
        _ => pinned_count(ctx, sources, entry).await,
    };
    let (result, error) = match result {
        Ok(result) => (Some(result), None),
        Err(error) => (None, Some(error)),
    };
    PinnedLine {
        id: entry.id.clone(),
        title: entry.title.clone(),
        sql: entry.sql.clone(),
        origin: entry.origin.clone(),
        address: query_address(ctx, &entry.id),
        result,
        error,
    }
}

async fn pinned_count(
    ctx: &Ctx<'_>,
    sources: RuleSources<'_>,
    entry: &SavedEntry,
) -> Result<PinnedResult, String> {
    let result = saved_query(ctx, sources, &entry.sql, counted).await?;
    let count = result
        .rows
        .first()
        .and_then(|row| row.first()?.as_text()?.parse().ok())
        .unwrap_or(0);
    Ok(PinnedResult::Count { count })
}

async fn pinned_rows(
    ctx: &Ctx<'_>,
    sources: RuleSources<'_>,
    entry: &SavedEntry,
    limit: Option<u32>,
) -> Result<PinnedResult, String> {
    let limit = limit.unwrap_or(PINNED_ROWS);
    let mut result = saved_query(ctx, sources, &entry.sql, |s| limited(s, limit)).await?;
    // A query with a LIMIT of its own keeps it, which may be larger.
    result.rows.truncate(limit as usize);
    let values = ResultValues::new(&result);
    let rows = (0..result.rows.len()).map(|row| {
        (0..result.columns.len())
            .map(|col| values.value(row, col, None))
            .collect()
    });
    Ok(PinnedResult::Rows {
        columns: result.columns.iter().map(|c| c.name.clone()).collect(),
        rows: readout_rows(rows),
    })
}

/// The result of a saved query's only statement, shaped by `shape`, in the readout's database;
/// its columns get kinds by their origin, as query results do.
async fn saved_query(
    ctx: &Ctx<'_>,
    sources: RuleSources<'_>,
    sql: &str,
    shape: impl FnOnce(&str) -> String,
) -> Result<ResultSet, String> {
    let statement = single_statement(sql)?;
    let sql = in_database(ctx.dialect, &ctx.database, &shape(statement));
    let mut result = ctx.fetch(&sql).await.map_err(|e| e.to_string())?;
    annotate_query(
        ctx.session,
        &ctx.database,
        ctx.lens.clone(),
        &mut result,
        Some(sources),
    )
    .await;
    Ok(result)
}

fn first_value(result: &ResultSet) -> Option<Value> {
    if result.rows.is_empty() || result.columns.is_empty() {
        return None;
    }
    Some(ResultValues::new(result).value(0, 0, None))
}

fn query_address(ctx: &Ctx<'_>, id: &str) -> Address {
    ctx.address(Target::Query { id: id.to_string() })
}

/// Each row with the address of its primary-key value, which the line opens.
fn readout_rows(rows: impl IntoIterator<Item = Vec<Value>>) -> Vec<ReadoutRow> {
    rows.into_iter()
        .map(|values| {
            let address = values.iter().find_map(|v| match &v.kind {
                ValueKind::PrimaryKey { address } => Some(address.clone()),
                _ => None,
            });
            ReadoutRow { address, values }
        })
        .collect()
}

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

pub async fn new_objects(ctx: &Ctx<'_>) -> AppResult<Vec<NewObjects>> {
    let entities = ctx
        .rules
        .entities
        .iter()
        .filter_map(|e| Some((e, e.entity.created.as_deref()?)));
    Ok(join_all(entities.map(|(e, created)| newest(ctx, e, created))).await)
}

async fn newest(ctx: &Ctx<'_>, applied: &AppliedEntity, created: &str) -> NewObjects {
    let entity = &applied.entity;
    let columns = entity_columns(ctx, applied, created);
    let sql = newest_sql(ctx, applied, &columns, created);
    let mut line = NewObjects {
        entity: entity.name.clone(),
        preset: applied.preset.clone(),
        table: entity.table.clone(),
        sql: sql.clone(),
        columns,
        rows: Vec::new(),
        error: None,
    };
    match ctx.rows(&entity.table, &sql).await {
        Ok(rows) => line.rows = readout_rows(rows.values(&|_, _| None)),
        Err(e) => line.error = Some(e.to_string()),
    }
    line
}

/// The newest `NEW_ROWS` rows of the entity by `created`, where its `filter` holds.
fn newest_sql(ctx: &Ctx<'_>, applied: &AppliedEntity, columns: &[String], created: &str) -> String {
    let entity = &applied.entity;
    let filter = entity
        .filter
        .as_deref()
        .map(|f| format!(" WHERE {}", ctx.preset_sql(f)))
        .unwrap_or_default();
    let list = columns
        .iter()
        .map(|c| ctx.ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT {list} FROM {}{filter} ORDER BY {} DESC LIMIT {NEW_ROWS}",
        ctx.table_ref(&entity.table),
        ctx.ident(created)
    )
}

/// The key, the summary names that are columns, and `created`, each once, in schema spelling.
fn entity_columns(ctx: &Ctx<'_>, applied: &AppliedEntity, created: &str) -> Vec<String> {
    let entity = &applied.entity;
    let existing = ctx.schema.columns(&entity.table);
    let mut columns: Vec<String> = Vec::new();
    let wanted = ctx
        .primary_key(&entity.table)
        .iter()
        .map(String::as_str)
        .chain(entity.summary.iter().map(String::as_str))
        .chain([created]);
    for name in wanted {
        let Some(column) = existing.iter().find(|c| c.eq_ignore_ascii_case(name)) else {
            continue;
        };
        if !columns.contains(column) {
            columns.push(column.clone());
        }
    }
    columns
}

pub async fn recent(ctx: &Ctx<'_>, recent: &[RecentObject]) -> AppResult<Vec<RecentLine>> {
    let addresses = recent.iter().filter_map(|r| {
        let address = Address::parse(&r.address).ok()?;
        let Target::Row { table, key } = address.target else {
            return None;
        };
        Some((ctx.row_address(&table, &key), r.at))
    });
    Ok(join_all(addresses.map(|(address, at)| recent_line(ctx, address, at))).await)
}

async fn recent_line(ctx: &Ctx<'_>, address: Address, at: i64) -> RecentLine {
    let (summary, error) = match peek::peek(ctx, &address).await {
        Ok(peek) => {
            let summary = peek
                .values
                .into_iter()
                .map(|(column, value)| SummaryValue { column, value })
                .collect();
            (summary, None)
        }
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    RecentLine {
        address,
        at,
        summary,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Rig;
    use super::*;
    use crate::db::{Cell, FakeDatabase};
    use crate::document::testing::{num, result, shop, text};
    use crate::error::AppError;
    use crate::rules::project::{Project, SavedQuery};
    use crate::saved;

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

    fn one(cell: Cell) -> ResultSet {
        result(&["c"], vec![vec![cell]])
    }

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

    fn table(ctx: &Ctx<'_>, name: &str) -> Address {
        ctx.address(Target::Table {
            table: name.into(),
            filter: Vec::new(),
        })
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

    fn saved_query(id: &str, sql: &str, pin: Option<Home>) -> SavedQuery {
        SavedQuery {
            id: id.into(),
            title: id.to_uppercase(),
            sql: sql.into(),
            pin,
        }
    }

    fn entries(ctx: &Ctx<'_>, queries: Vec<SavedQuery>) -> Vec<SavedEntry> {
        let project = Project {
            saved: queries,
            ..Project::new("local", "wp")
        };
        saved::list(&project, &[], &ctx.rules)
    }

    fn text_of(value: &Option<Value>) -> Option<String> {
        value
            .as_ref()
            .and_then(|v| v.raw.as_text().map(str::to_string))
    }

    #[tokio::test]
    async fn status_values_read_the_first_cell_of_each_value_pin() {
        let db = shop()
            .on("AS v1", one(text("9.3.0")))
            .on("AS v2", result(&["v"], Vec::new()))
            .failing("AS v3", AppError::InvalidInput("boom".into()));
        let rig = Rig::new(db, "", &[]);
        let ctx = rig.ctx().await;
        let list = entries(
            &ctx,
            vec![
                saved_query("v1", "SELECT 1 AS v1 FROM {p}orders", Some(Home::Value)),
                saved_query("v2", "SELECT 1 AS v2 FROM {p}orders", Some(Home::Value)),
                saved_query("v3", "SELECT 1 AS v3 FROM {p}orders", Some(Home::Value)),
                saved_query("c", "SELECT 1 FROM {p}orders", Some(Home::Count)),
                saved_query("none", "SELECT 1 FROM {p}orders", None),
            ],
        );

        let lines = status_values(&ctx, rig.sources(), &list).await.unwrap();

        let ids: Vec<&str> = lines.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["v1", "v2", "v3"]);
        assert_eq!(text_of(&lines[0].value).as_deref(), Some("9.3.0"));
        assert_eq!(
            lines[0].address,
            ctx.address(Target::Query { id: "v1".into() })
        );
        assert_eq!(
            (lines[1].value.clone(), lines[1].error.clone()),
            (None, None)
        );
        assert_eq!(lines[2].value, None);
        assert_eq!(lines[2].error.as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn a_pinned_count_counts_the_rows_of_its_statement() {
        let rig = Rig::new(shop().on("AS c1", one(num("7"))), "", &[]);
        let ctx = rig.ctx().await;
        let list = entries(
            &ctx,
            vec![saved_query(
                "c1",
                "SELECT 1 AS c1 FROM {p}orders;",
                Some(Home::Count),
            )],
        );

        let lines = pinned(&ctx, rig.sources(), &list).await.unwrap();

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].result, Some(PinnedResult::Count { count: 7 }));
        assert_eq!(lines[0].error, None);
        assert_eq!(
            lines[0].address,
            ctx.address(Target::Query { id: "c1".into() })
        );
        assert_eq!(
            rig.db.queries().last().map(String::as_str),
            Some("USE `wp`;\nSELECT COUNT(*) FROM (\nSELECT 1 AS c1 FROM f_orders\n) AS counted")
        );
    }

    fn orders(n: usize) -> ResultSet {
        let rows = (1..=n)
            .map(|i| vec![num(&i.to_string()), text("10.00")])
            .collect();
        result(&["id", "total"], rows)
    }

    #[tokio::test]
    async fn pinned_rows_without_a_limit_show_five_rows() {
        let rig = Rig::new(shop().on("AS r1", orders(7)), "", &[]);
        let ctx = rig.ctx().await;
        let list = entries(
            &ctx,
            vec![saved_query(
                "r1",
                "SELECT id, total AS r1 FROM {p}orders",
                Some(Home::Rows { limit: None }),
            )],
        );

        let lines = pinned(&ctx, rig.sources(), &list).await.unwrap();

        let Some(PinnedResult::Rows { columns, rows }) = &lines[0].result else {
            panic!("{:?}", lines[0]);
        };
        assert_eq!(columns, &["id", "total"]);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].values[0].raw.as_text(), Some("1"));
        assert_eq!(
            rig.db.queries().last().map(String::as_str),
            Some("USE `wp`;\nSELECT id, total AS r1 FROM f_orders LIMIT 5")
        );
    }

    #[tokio::test]
    async fn pinned_rows_use_their_own_limit() {
        let rig = Rig::new(shop().on("AS r2", orders(7)), "", &[]);
        let ctx = rig.ctx().await;
        let list = entries(
            &ctx,
            vec![saved_query(
                "r2",
                "SELECT id, total AS r2 FROM {p}orders",
                Some(Home::Rows { limit: Some(2) }),
            )],
        );

        let lines = pinned(&ctx, rig.sources(), &list).await.unwrap();

        let Some(PinnedResult::Rows { rows, .. }) = &lines[0].result else {
            panic!("{:?}", lines[0]);
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rig.db.queries().last().map(String::as_str),
            Some("USE `wp`;\nSELECT id, total AS r2 FROM f_orders LIMIT 2")
        );
    }

    #[tokio::test]
    async fn pinned_lines_that_cannot_run_carry_their_error_and_leave_the_others() {
        let db = shop()
            .on("AS ok", one(num("1")))
            .failing("AS bad", AppError::InvalidInput("boom".into()));
        let rig = Rig::new(db, "", &[]);
        let ctx = rig.ctx().await;
        let list = entries(
            &ctx,
            vec![
                saved_query("bad", "SELECT 1 AS bad FROM {p}orders", Some(Home::Count)),
                saved_query("two", "SELECT 1; SELECT 2", Some(Home::Count)),
                saved_query("blank", "  ;", Some(Home::Rows { limit: None })),
                saved_query("ok", "SELECT 1 AS ok FROM {p}orders", Some(Home::Count)),
                saved_query("value", "SELECT 1", Some(Home::Value)),
            ],
        );

        let lines = pinned(&ctx, rig.sources(), &list).await.unwrap();

        let outcome: Vec<(&str, Option<&str>, bool)> = lines
            .iter()
            .map(|l| (l.id.as_str(), l.error.as_deref(), l.result.is_some()))
            .collect();
        assert_eq!(
            outcome,
            [
                ("bad", Some("boom"), false),
                (
                    "two",
                    Some("Only a query with one statement shows on the readout"),
                    false
                ),
                ("blank", Some("The query is empty"), false),
                ("ok", None, true),
            ]
        );
    }

    const ENTITIES: &str = r#"
[[entity]]
name = "order"
table = "orders"
summary = ["status", "total", "nope"]
created = "created_gmt"

[[entity]]
name = "open_order"
table = "orders"
filter = "status = 'open'"
summary = ["status"]
created = "created_gmt"

[[entity]]
name = "person"
table = "people"
"#;

    fn newest_orders() -> FakeDatabase {
        let columns = ["id", "status", "created_gmt"];
        let rows = vec![vec![
            num("2"),
            text("open"),
            Cell::Temporal("2026-01-02 00:00:00".into()),
        ]];
        shop()
            .on("WHERE status = 'open'", result(&columns, rows))
            .on("`total`", orders(2))
    }

    #[tokio::test]
    async fn new_objects_list_the_newest_five_rows_of_each_entity_with_a_created_column() {
        let rig = Rig::new(newest_orders(), ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let lines = new_objects(&ctx).await.unwrap();

        let names: Vec<&str> = lines.iter().map(|l| l.entity.as_str()).collect();
        assert_eq!(names, ["open_order", "order"]);
        assert_eq!(
            lines[1].sql,
            "SELECT `id`, `status`, `total`, `created_gmt` FROM `wp`.`f_orders` \
             ORDER BY `created_gmt` DESC LIMIT 5"
        );
        assert_eq!(lines[1].columns, ["id", "status", "total", "created_gmt"]);
        assert_eq!(
            (lines[1].preset.as_str(), lines[1].table.as_str()),
            ("doc", "f_orders")
        );
    }

    #[tokio::test]
    async fn new_objects_of_a_filtered_entity_apply_its_filter() {
        let rig = Rig::new(newest_orders(), ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let lines = new_objects(&ctx).await.unwrap();

        assert_eq!(
            lines[0].sql,
            "SELECT `id`, `status`, `created_gmt` FROM `wp`.`f_orders` WHERE status = 'open' \
             ORDER BY `created_gmt` DESC LIMIT 5"
        );
        assert_eq!(lines[0].error, None);
        assert_eq!(lines[0].rows.len(), 1);
        assert_eq!(lines[0].rows[0].values[0].raw.as_text(), Some("2"));
        assert_eq!(
            lines[0].rows[0].address,
            Some(ctx.row_address("f_orders", "2"))
        );
    }

    #[tokio::test]
    async fn a_new_objects_line_whose_query_fails_carries_the_error() {
        let db = shop().failing(
            "FROM `wp`.`f_orders`",
            AppError::InvalidInput("boom".into()),
        );
        let rig = Rig::new(db, ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let lines = new_objects(&ctx).await.unwrap();

        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert_eq!(line.error.as_deref(), Some("boom"), "{}", line.entity);
            assert!(line.rows.is_empty());
        }
    }

    #[tokio::test]
    async fn recent_keeps_only_row_addresses_and_carries_the_error_of_an_unreadable_row() {
        let rig = Rig::new(shop(), "", &[]);
        let ctx = rig.ctx().await;
        let row = ctx.row_address("f_orders", "1");
        let recent_objects = [
            RecentObject {
                address: "not an address".into(),
                at: 5,
            },
            RecentObject {
                address: table(&ctx, "f_orders").to_string(),
                at: 4,
            },
            RecentObject {
                address: row.to_string(),
                at: 3,
            },
        ];

        let lines = recent(&ctx, &recent_objects).await.unwrap();

        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].address.clone(), lines[0].at), (row, 3));
        assert!(lines[0].summary.is_empty());
        assert!(lines[0].error.is_some());
    }
}
