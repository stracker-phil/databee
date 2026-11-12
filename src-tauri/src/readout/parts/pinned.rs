use futures_util::future::join_all;

use crate::address::{Address, Target};
use crate::db::ResultSet;
use crate::db::lexer::read_tables;
use crate::error::AppResult;
use crate::query::Ctx;
use crate::readout::sql::{counted, in_database, limited, single_statement};
use crate::readout::{PINNED_ROWS, PinnedLine, PinnedResult, ReadoutRow, StatusValue};
use crate::rules::annotate::annotate_query;
use crate::rules::effective::RuleSources;
use crate::rules::queries::Home;
use crate::saved::SavedEntry;
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
        tables: read_tables(&entry.sql),
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

pub(super) fn query_address(ctx: &Ctx<'_>, id: &str) -> Address {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::query::testing::{num, result, shop, text};
    use crate::readout::parts::tests::one;
    use crate::readout::testing::Rig;
    use crate::rules::project::{Project, SavedQuery};
    use crate::saved;

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
        assert_eq!(lines[0].tables, ["f_orders"]);
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
}
