//! Value counts: how often each value of one column occurs in a table, most frequent first.

use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::db::ColumnEquals;
use crate::db::browse::equals_sql;
use crate::document::Ctx;
use crate::error::{AppError, AppResult};
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::state::AppState;
use crate::value::{ResultValues, Value};

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MAX_LIMIT: u32 = 500;
const VALUE_COLUMN: usize = 0;
const COUNT_COLUMN: usize = 1;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub(crate) struct ValueCount {
    /// A group has no row, so the value has no provenance.
    pub value: Value,
    #[ts(type = "number")]
    pub count: u64,
}

pub(crate) struct CountsRequest<'a> {
    pub table: &'a str,
    pub column: &'a str,
    pub equals: &'a [ColumnEquals],
    /// Capped at `MAX_LIMIT`.
    pub limit: u32,
}

/// Ties come in value order. Runs with the query timeout, at most `QUERY_TIMEOUT`.
pub(crate) async fn count_values(
    session: &Session,
    database: &str,
    request: &CountsRequest<'_>,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<Vec<ValueCount>> {
    let CountsRequest {
        table,
        column,
        equals,
        ..
    } = request;
    let ctx = Ctx::load(session, database, sources, timeout.min(QUERY_TIMEOUT)).await?;
    if ctx.schema.table(table).is_none() {
        return Err(AppError::InvalidInput(format!(
            "No table {table} in {database}"
        )));
    }
    let has_column = |name: &str| {
        ctx.schema
            .columns(table)
            .iter()
            .any(|c| c.eq_ignore_ascii_case(name))
    };
    if let Some(missing) = std::iter::once(*column)
        .chain(equals.iter().map(|c| c.column.as_str()))
        .find(|name| !has_column(name))
    {
        return Err(AppError::InvalidInput(format!(
            "No column {missing} in {table}"
        )));
    }
    let rows = ctx.rows(table, &counts_sql(&ctx, request)).await?;
    let values = ResultValues::new(&rows.result);
    Ok((0..rows.len())
        .map(|row| ValueCount {
            value: values.value(row, VALUE_COLUMN, None),
            count: rows.result.rows[row][COUNT_COLUMN]
                .as_text()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
        })
        .collect())
}

fn counts_sql(ctx: &Ctx<'_>, request: &CountsRequest<'_>) -> String {
    let col = ctx.ident(request.column);
    let filter = if request.equals.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", equals_sql(ctx.dialect, request.equals))
    };
    format!(
        "SELECT {col}, COUNT(*) FROM {}{filter} GROUP BY {col} ORDER BY COUNT(*) DESC, {col} LIMIT {}",
        ctx.table_ref(request.table),
        request.limit.clamp(1, MAX_LIMIT)
    )
}

/// `equals` is a structured filter (`column = value` matches).
#[tauri::command]
pub async fn value_counts(
    state: State<'_, AppState>,
    session_id: String,
    database: String,
    table: String,
    column: String,
    equals: Vec<ColumnEquals>,
    limit: u32,
) -> AppResult<Vec<ValueCount>> {
    let session = state.sessions.get(&session_id)?;
    let timeout = state.settings.get().query_timeout();
    let request = CountsRequest {
        table: &table,
        column: &column,
        equals: &equals,
        limit,
    };
    count_values(&session, &database, &request, state.rules(), timeout).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Cell;
    use crate::document::testing::{Rig, num, result, shop, text};
    use crate::error::ErrorKind;
    use crate::rules::PresetCatalog;
    use crate::rules::ProjectsStore;
    use crate::test_support::{connect_to, servers, spec};
    use crate::value::ValueKind;

    const TIMEOUT: Duration = Duration::from_secs(30);
    const ENUM_STATUS: &str = "[[kind]]\ncolumns = [\"orders.status\"]\nenum = true\n";
    const UNFILTERED: &str = "SELECT `status`, COUNT(*) FROM `wp`.`f_orders` GROUP BY `status` ORDER BY COUNT(*) DESC, `status` LIMIT 20";

    fn counts(rows: Vec<Vec<Cell>>) -> crate::db::ResultSet {
        result(&["status", "COUNT(*)"], rows)
    }

    fn equals(column: &str, value: Option<&str>) -> ColumnEquals {
        ColumnEquals {
            column: column.into(),
            value: value.map(String::from),
        }
    }

    fn request<'a>(column: &'a str, equals: &'a [ColumnEquals], limit: u32) -> CountsRequest<'a> {
        CountsRequest {
            table: "f_orders",
            column,
            equals,
            limit,
        }
    }

    fn value(raw: Cell, kind: ValueKind) -> Value {
        Value {
            raw,
            kind,
            source: None,
            style: None,
            secondary: None,
        }
    }

    async fn run(
        rig: &Rig,
        request: &CountsRequest<'_>,
        timeout: Duration,
    ) -> AppResult<Vec<ValueCount>> {
        count_values(&rig.session, "wp", request, rig.sources(), timeout).await
    }

    #[tokio::test]
    async fn values_are_grouped_and_counted_with_their_kinds_and_no_provenance() {
        let rig = Rig::new(
            shop().on(
                "GROUP BY",
                counts(vec![
                    vec![text("wc-paid"), num("3")],
                    vec![Cell::Null, num("2")],
                    vec![text(""), num("1")],
                ]),
            ),
            ENUM_STATUS,
        );

        let found = run(&rig, &request("status", &[], 20), TIMEOUT)
            .await
            .unwrap();

        assert_eq!(rig.db.queries(), [UNFILTERED]);
        assert_eq!(
            found,
            [
                ValueCount {
                    value: value(text("wc-paid"), ValueKind::Enum),
                    count: 3
                },
                ValueCount {
                    value: value(Cell::Null, ValueKind::Null),
                    count: 2
                },
                ValueCount {
                    value: value(text(""), ValueKind::Empty),
                    count: 1
                },
            ]
        );
    }

    #[tokio::test]
    async fn equals_filters_become_a_where_with_is_null_for_none() {
        let rig = Rig::new(shop().on("GROUP BY", counts(Vec::new())), "");
        let filter = [equals("currency", Some("EUR")), equals("total", None)];

        let found = run(&rig, &request("status", &filter, 20), TIMEOUT)
            .await
            .unwrap();

        assert!(found.is_empty());
        assert_eq!(
            rig.db.queries(),
            [
                "SELECT `status`, COUNT(*) FROM `wp`.`f_orders` WHERE `currency` = 'EUR' AND `total` IS NULL GROUP BY `status` ORDER BY COUNT(*) DESC, `status` LIMIT 20"
            ]
        );
    }

    #[tokio::test]
    async fn a_filter_on_the_counted_column_is_allowed() {
        let rig = Rig::new(shop().on("GROUP BY", counts(Vec::new())), "");
        let filter = [equals("status", Some("wc-paid"))];

        run(&rig, &request("status", &filter, 20), TIMEOUT)
            .await
            .unwrap();

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT `status`, COUNT(*) FROM `wp`.`f_orders` WHERE `status` = 'wc-paid' GROUP BY `status` ORDER BY COUNT(*) DESC, `status` LIMIT 20"
            ]
        );
    }

    #[tokio::test]
    async fn the_limit_is_clamped_between_one_and_the_maximum() {
        for (asked, sent) in [
            (0, 1),
            (1, 1),
            (20, 20),
            (500, 500),
            (501, 500),
            (u32::MAX, 500),
        ] {
            let rig = Rig::new(shop().on("GROUP BY", counts(Vec::new())), "");

            run(&rig, &request("status", &[], asked), TIMEOUT)
                .await
                .unwrap();

            assert_eq!(
                rig.db.queries(),
                [format!(
                    "SELECT `status`, COUNT(*) FROM `wp`.`f_orders` GROUP BY `status` ORDER BY COUNT(*) DESC, `status` LIMIT {sent}"
                )],
                "limit {asked}"
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_table_or_column_is_invalid_input_and_runs_no_query() {
        let missing_table = CountsRequest {
            table: "f_missing",
            ..request("status", &[], 20)
        };
        let filter = [equals("nope", Some("x"))];
        for (request, message) in [
            (missing_table, "No table f_missing in wp"),
            (request("nope", &[], 20), "No column nope in f_orders"),
            (request("status", &filter, 20), "No column nope in f_orders"),
        ] {
            let rig = Rig::new(shop().on("GROUP BY", counts(Vec::new())), "");

            let err = run(&rig, &request, TIMEOUT).await.unwrap_err();

            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{message}");
            assert_eq!(err.to_string(), message);
            assert!(rig.db.queries().is_empty(), "{message}");
        }
    }

    #[tokio::test]
    async fn column_names_match_regardless_of_case() {
        let rig = Rig::new(shop().on("GROUP BY", counts(Vec::new())), "");
        let filter = [equals("CURRENCY", Some("EUR"))];

        run(&rig, &request("STATUS", &filter, 20), TIMEOUT)
            .await
            .unwrap();

        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn a_count_that_is_not_a_number_is_zero() {
        let rig = Rig::new(
            shop().on("GROUP BY", counts(vec![vec![text("a"), Cell::Null]])),
            "",
        );

        let found = run(&rig, &request("status", &[], 20), TIMEOUT)
            .await
            .unwrap();

        assert_eq!(found[0].count, 0);
    }

    #[tokio::test]
    async fn the_query_runs_with_the_settings_timeout_but_at_most_five_seconds() {
        for (settings, used) in [(30, 5), (5, 5), (2, 2)] {
            let rig = Rig::new(shop().on("GROUP BY", counts(Vec::new())), "");

            run(
                &rig,
                &request("status", &[], 20),
                Duration::from_secs(settings),
            )
            .await
            .unwrap();

            assert_eq!(
                rig.db.fetch_timeouts(),
                [Duration::from_secs(used)],
                "settings {settings}s"
            );
        }
    }

    async fn statuses(
        session: &Session,
        server: &str,
        equals: &[ColumnEquals],
        limit: u32,
        sources: RuleSources<'_>,
    ) -> Vec<(String, u64)> {
        let request = CountsRequest {
            table: "wp_wc_orders",
            column: "status",
            equals,
            limit,
        };
        count_values(session, "wp_test", &request, sources, TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("{server}: {e:?}"))
            .into_iter()
            .map(|c| {
                (
                    c.value.raw.as_text().unwrap_or_default().to_string(),
                    c.count,
                )
            })
            .collect()
    }

    fn pairs(expected: &[(&str, u64)]) -> Vec<(String, u64)> {
        expected.iter().map(|(s, n)| (s.to_string(), *n)).collect()
    }

    #[tokio::test]
    #[ignore]
    async fn counting_wc_order_statuses_matches_the_fixture_on_every_server() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = PresetCatalog::open(dir.path());
        let projects = ProjectsStore::open(dir.path());
        let sources = RuleSources {
            presets: &catalog,
            projects: &projects,
        };
        let by_type = [equals("type", Some("shop_order"))];
        let no_email = [equals("billing_email", None)];
        for (host, port) in servers() {
            let server = format!("{host}:{port}");
            let db = connect_to(&host, port, "wp_test").await;
            let session = Session::new(
                "it".into(),
                spec(&host, port, "wp_test"),
                format!("it-{port}"),
                Box::new(db),
            );

            let all = statuses(&session, &server, &[], 20, sources).await;
            let limited = statuses(&session, &server, &[], 2, sources).await;
            let shop_orders = statuses(&session, &server, &by_type, 20, sources).await;
            let without_email = statuses(&session, &server, &no_email, 20, sources).await;

            assert_eq!(
                all,
                pairs(&[
                    ("wc-active", 1),
                    ("wc-failed", 1),
                    ("wc-processing", 1),
                    ("wc-refunded", 1)
                ]),
                "{server}"
            );
            assert_eq!(
                limited,
                pairs(&[("wc-active", 1), ("wc-failed", 1)]),
                "{server}"
            );
            assert_eq!(
                shop_orders,
                pairs(&[("wc-failed", 1), ("wc-processing", 1)]),
                "{server}"
            );
            assert_eq!(without_email, pairs(&[("wc-refunded", 1)]), "{server}");
        }
    }
}
