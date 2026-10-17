//! The WordPress dashboard.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use serde::Serialize;
use ts_rs::TS;

use super::changes::WpChangeRequest;
use super::fetch;
use super::rows::{Facet, count, facets, text};
use super::site::WpContext;
use super::sql::{autoload_on_sql, table_ref};
use crate::db::Database;
use crate::db::TableInfo;
use crate::error::{AppError, AppResult};

pub const AUTOLOAD_HEALTHY_BYTES: u64 = 800 * 1024;
const DAYS: i64 = 14;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct TableTile {
    pub name: String,
    pub rows: u64,
    pub bytes: u64,
    /// core, woocommerce, action_scheduler, logs, plugins
    pub category: &'static str,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct DayBucket {
    pub date: String,
    pub completed: u64,
    pub processing: u64,
    pub failed: u64,
    pub other: u64,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum Granularity {
    Day,
    Hour,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct OrderStats {
    pub total: u64,
    pub today: u64,
    pub by_status: Vec<Facet>,
    pub days: Vec<DayBucket>,
    /// `day` (14 days) or `hour` (last 24 h, used when fewer than 3 days have orders).
    pub granularity: Granularity,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActionStats {
    pub total: u64,
    pub by_status: Vec<Facet>,
    pub overdue: u64,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum InsightLevel {
    Info,
    Warning,
    Danger,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
/// A reviewed change an alert offers; it goes through the pending tray.
pub struct InsightAction {
    pub label: String,
    pub change: WpChangeRequest,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub level: InsightLevel,
    pub text: String,
    pub action: Option<InsightAction>,
}

fn change(label: &str, change: WpChangeRequest) -> Option<InsightAction> {
    Some(InsightAction {
        label: label.into(),
        change,
    })
}

/// Rows dated in `[start, end)` count towards the bucket `key`.
struct Span {
    key: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

const SQL_DATETIME: &str = "%Y-%m-%d %H:%M:%S";

/// The `DAYS` UTC days ending with `today`, keyed `YYYY-MM-DD`. The site's zone is not read.
fn day_spans(today: NaiveDate) -> Vec<Span> {
    let midnight = |date: NaiveDate| date.and_time(NaiveTime::MIN).and_utc();
    (0..DAYS)
        .rev()
        .map(|back| {
            let date = today - Duration::days(back);
            Span {
                key: date.format("%Y-%m-%d").to_string(),
                start: midnight(date),
                end: midnight(date + Duration::days(1)),
            }
        })
        .collect()
}

/// The 24 UTC hours ending with the current one, keyed by their start.
fn hour_spans(now: DateTime<Utc>) -> Vec<Span> {
    let current = now
        .date_naive()
        .and_hms_opt(now.hour(), 0, 0)
        .expect("the hour of a valid time is a valid time")
        .and_utc();
    (0..24)
        .rev()
        .map(|back| {
            let start = current - Duration::hours(back);
            Span {
                key: start.format("%Y-%m-%d %H:%M").to_string(),
                start,
                end: start + Duration::hours(1),
            }
        })
        .collect()
}

/// A `CASE` naming the span a UTC `DATETIME` column falls into, plus the `WHERE` condition
/// that covers all spans.
fn span_sql(column: &str, spans: &[Span]) -> (String, String) {
    let at = |t: &DateTime<Utc>| format!("'{}'", t.format(SQL_DATETIME));
    let cases: String = spans
        .iter()
        .map(|s| {
            format!(
                " WHEN {column} >= {} AND {column} < {} THEN '{}'",
                at(&s.start),
                at(&s.end),
                s.key
            )
        })
        .collect();
    let range = match (spans.first(), spans.last()) {
        (Some(first), Some(last)) => {
            format!(
                "{column} >= {} AND {column} < {}",
                at(&first.start),
                at(&last.end)
            )
        }
        _ => "FALSE".to_string(),
    };
    (format!("CASE{cases} END"), range)
}

/// The server's clock, so "today" doesn't depend on this machine's.
async fn server_now(session: &dyn Database) -> AppResult<DateTime<Utc>> {
    let rs = fetch(session, "SELECT UTC_TIMESTAMP()").await?;
    rs.rows
        .first()
        .and_then(|r| text(&r[0]))
        .and_then(|t| NaiveDateTime::parse_from_str(&t, SQL_DATETIME).ok())
        .map(|t| t.and_utc())
        .ok_or_else(|| AppError::Database {
            message: "The server returned no time for UTC_TIMESTAMP()".into(),
            code: None,
        })
}

/// 24 hourly buckets ending with the current UTC hour, labelled `HH:00`.
pub fn hour_buckets(rows: &[(String, String, u64)], now: DateTime<Utc>) -> Vec<DayBucket> {
    hour_spans(now)
        .into_iter()
        .map(|span| {
            let mut bucket = DayBucket {
                date: span.start.format("%H:00").to_string(),
                completed: 0,
                processing: 0,
                failed: 0,
                other: 0,
            };
            for (_, status, count) in rows.iter().filter(|(d, _, _)| *d == span.key) {
                match status.trim_start_matches("wc-") {
                    "completed" => bucket.completed += count,
                    "processing" => bucket.processing += count,
                    "failed" | "cancelled" => bucket.failed += count,
                    _ => bucket.other += count,
                }
            }
            bucket
        })
        .collect()
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub orders: Option<OrderStats>,
    pub autoload_bytes: u64,
    pub autoload_healthy: bool,
    pub transients_expired: u64,
    pub actions: Option<ActionStats>,
    /// Orders whose posts copy differs from `wc_orders` (only with HPOS sync on).
    pub hpos_drift: Option<u64>,
    pub tables: Vec<TableTile>,
    pub insights: Vec<Insight>,
}

const CORE_TABLES: &[&str] = &[
    "posts",
    "postmeta",
    "options",
    "users",
    "usermeta",
    "comments",
    "commentmeta",
    "terms",
    "termmeta",
    "term_taxonomy",
    "term_relationships",
    "links",
    "blogs",
    "site",
    "sitemeta",
];

pub fn table_category(ctx: &WpContext, table: &str) -> &'static str {
    let name = table.strip_prefix(&ctx.prefix).unwrap_or(table);
    // Core first: `blogs` contains "log".
    if CORE_TABLES.contains(&name) {
        "core"
    } else if name.starts_with("actionscheduler_") {
        "action_scheduler"
    } else if name.contains("log") {
        "logs"
    } else if name.starts_with("wc_") || name.starts_with("woocommerce_") {
        "woocommerce"
    } else {
        "plugins"
    }
}

pub fn tiles(ctx: &WpContext, tables: &[TableInfo]) -> Vec<TableTile> {
    let mut tiles: Vec<TableTile> = tables
        .iter()
        .filter(|t| !t.is_view)
        .map(|t| TableTile {
            name: t.name.clone(),
            rows: t.rows_estimate.unwrap_or(0),
            bytes: t.size_bytes.unwrap_or(0),
            category: table_category(ctx, &t.name),
        })
        .collect();
    tiles.sort_by_key(|a| std::cmp::Reverse(a.rows));
    tiles
}

/// Fills all `DAYS` days ending today (UTC) from (date, status, count) rows.
pub fn day_buckets(rows: &[(String, String, u64)], today: NaiveDate) -> Vec<DayBucket> {
    (0..DAYS)
        .rev()
        .map(|back| {
            let date = (today - Duration::days(back))
                .format("%Y-%m-%d")
                .to_string();
            let mut bucket = DayBucket {
                date: date.clone(),
                completed: 0,
                processing: 0,
                failed: 0,
                other: 0,
            };
            for (_, status, count) in rows.iter().filter(|(d, _, _)| *d == date) {
                match status.trim_start_matches("wc-") {
                    "completed" => bucket.completed += count,
                    "processing" => bucket.processing += count,
                    "failed" | "cancelled" => bucket.failed += count,
                    _ => bucket.other += count,
                }
            }
            bucket
        })
        .collect()
}

pub fn insights(d: &Dashboard, db: &str) -> Vec<Insight> {
    let mut out = Vec::new();
    let total_rows: u64 = d.tables.iter().map(|t| t.rows).sum();
    if let Some(top) = d.tables.first().filter(|_| total_rows > 10_000) {
        let share = top.rows * 100 / total_rows.max(1);
        if share >= 40 {
            let hint = if top.category == "logs" {
                " Consider a retention rule."
            } else {
                ""
            };
            out.push(Insight {
                level: InsightLevel::Warning,
                text: format!("{} holds {share}% of all rows.{hint}", top.name),
                action: if top.category == "logs" && top.name.ends_with("woocommerce_log") {
                    change(
                        "Set retention",
                        WpChangeRequest::DeleteLogsBefore {
                            database: db.into(),
                            days: 30,
                            source: None,
                        },
                    )
                } else {
                    None
                },
            });
        }
    }
    if !d.autoload_healthy {
        out.push(Insight {
            level: InsightLevel::Warning,
            text: format!(
                "Autoloaded options weigh {} KB; every request loads them.",
                d.autoload_bytes / 1024
            ),
            action: None,
        });
    }
    if let Some(a) = &d.actions {
        let failed = a
            .by_status
            .iter()
            .find(|f| f.value == "failed")
            .map_or(0, |f| f.count);
        if failed > 0 {
            out.push(Insight {
                level: InsightLevel::Danger,
                text: format!("{failed} scheduled actions failed."),
                action: None,
            });
        }
        if a.overdue > 0 {
            out.push(Insight {
                level: InsightLevel::Warning,
                text: format!(
                    "{} pending actions are overdue by more than 10 minutes.",
                    a.overdue
                ),
                action: None,
            });
        }
    }
    if let Some(drift) = d.hpos_drift.filter(|n| *n > 0) {
        out.push(Insight {
            level: InsightLevel::Danger,
            text: format!("{drift} orders differ between wc_orders and their posts copy."),
            action: None,
        });
    }
    if d.transients_expired > 0 {
        out.push(Insight {
            level: InsightLevel::Info,
            text: format!(
                "{} expired transients can be cleaned up.",
                d.transients_expired
            ),
            action: change(
                "Clean up",
                WpChangeRequest::DeleteExpiredTransients {
                    database: db.into(),
                },
            ),
        });
    }
    out
}

pub async fn dashboard(session: &dyn Database, ctx: &WpContext, db: &str) -> AppResult<Dashboard> {
    let dialect = session.dialect();
    let options = table_ref(dialect, db, ctx, "options");
    let autoload = fetch(
        session,
        &format!(
            "SELECT SUM(LENGTH(option_value)) FROM {options} WHERE {}",
            autoload_on_sql("autoload")
        ),
    )
    .await?;
    let autoload_bytes = autoload
        .rows
        .first()
        .and_then(|r| count(&r[0]))
        .unwrap_or(0);
    let expired = fetch(
        session,
        &format!(
            "SELECT COUNT(*) FROM {options} WHERE (option_name LIKE {} OR option_name LIKE {}) \
             AND CAST(option_value AS UNSIGNED) < UNIX_TIMESTAMP()",
            dialect.like(r"\_transient\_timeout\_%"),
            dialect.like(r"\_site\_transient\_timeout\_%"),
        ),
    )
    .await?;

    let (orders, hpos_drift) = if ctx.woocommerce() {
        let now = server_now(session).await?;
        let today = now.date_naive();
        let (table, status_col, date_col, type_filter) = if ctx.hpos() {
            (
                table_ref(dialect, db, ctx, "wc_orders"),
                "status",
                "date_created_gmt",
                "type = 'shop_order'",
            )
        } else {
            (
                table_ref(dialect, db, ctx, "posts"),
                "post_status",
                "post_date_gmt",
                "post_type = 'shop_order'",
            )
        };
        let by_status = fetch(
            session,
            &format!("SELECT {status_col}, COUNT(*) FROM {table} WHERE {type_filter} GROUP BY {status_col}"),
        )
        .await?;
        let (day_case, day_range) = span_sql(date_col, &day_spans(today));
        let days = fetch(
            session,
            &format!(
                "SELECT {day_case}, {status_col}, COUNT(*) FROM {table} WHERE {type_filter} \
                 AND {day_range} GROUP BY 1, 2"
            ),
        )
        .await?;
        let day_rows: Vec<(String, String, u64)> = days
            .rows
            .iter()
            .filter_map(|r| Some((text(&r[0])?, text(&r[1])?, count(&r[2])?)))
            .collect();
        let mut buckets = day_buckets(&day_rows, today);
        let mut granularity = Granularity::Day;
        let active_days = buckets
            .iter()
            .filter(|b| b.completed + b.processing + b.failed + b.other > 0)
            .count();
        if active_days < 3 {
            let (hour_case, hour_range) = span_sql(date_col, &hour_spans(now));
            let hours = fetch(
                session,
                &format!(
                    "SELECT {hour_case}, {status_col}, COUNT(*) FROM {table} WHERE {type_filter} \
                     AND {hour_range} GROUP BY 1, 2"
                ),
            )
            .await?;
            let hour_rows: Vec<(String, String, u64)> = hours
                .rows
                .iter()
                .filter_map(|r| Some((text(&r[0])?, text(&r[1])?, count(&r[2])?)))
                .collect();
            let today_total = buckets
                .last()
                .map_or(0, |b| b.completed + b.processing + b.failed + b.other);
            buckets = hour_buckets(&hour_rows, now);
            // keep "today" from the day view
            buckets.push(DayBucket {
                date: "__today".into(),
                completed: today_total,
                processing: 0,
                failed: 0,
                other: 0,
            });
            granularity = Granularity::Hour;
        }
        let today_count = if granularity == Granularity::Hour {
            buckets.pop().map_or(0, |b| b.completed)
        } else {
            buckets
                .last()
                .map_or(0, |b| b.completed + b.processing + b.failed + b.other)
        };
        let status_facets = facets(&by_status, 0, 1);
        let stats = OrderStats {
            total: status_facets
                .iter()
                .filter(|f| f.value != "trash")
                .map(|f| f.count)
                .sum(),
            today: today_count,
            by_status: status_facets,
            days: buckets,
            granularity,
        };
        // The posts copies only have to match while data sync is on.
        let drift = if ctx.hpos() {
            let rs = fetch(
                session,
                &format!(
                    "SELECT COUNT(*) FROM {o} o LEFT JOIN {p} p ON p.ID = o.id WHERE o.type IN ('shop_order', 'shop_order_refund') \
                     AND (p.ID IS NULL OR p.post_type <> o.type OR p.post_modified_gmt <> o.date_updated_gmt) \
                     AND EXISTS (SELECT 1 FROM {options} WHERE option_name = 'woocommerce_custom_orders_table_data_sync_enabled' AND option_value = 'yes')",
                    o = table_ref(dialect, db, ctx, "wc_orders"),
                    p = table_ref(dialect, db, ctx, "posts"),
                ),
            )
            .await?;
            rs.rows.first().and_then(|r| count(&r[0]))
        } else {
            None
        };
        (Some(stats), drift)
    } else {
        (None, None)
    };

    let actions = if ctx.has_table("actionscheduler_actions") {
        let table = table_ref(dialect, db, ctx, "actionscheduler_actions");
        let by_status = fetch(
            session,
            &format!("SELECT status, COUNT(*) FROM {table} GROUP BY status"),
        )
        .await?;
        let status_facets = facets(&by_status, 0, 1);
        let overdue = fetch(
            session,
            &format!(
                "SELECT COUNT(*) FROM {table} WHERE status = 'pending' AND scheduled_date_gmt < UTC_TIMESTAMP() - INTERVAL 10 MINUTE \
                 AND scheduled_date_gmt > '1000-01-01'"
            ),
        )
        .await?;
        Some(ActionStats {
            total: status_facets.iter().map(|f| f.count).sum(),
            by_status: status_facets,
            overdue: overdue.rows.first().and_then(|r| count(&r[0])).unwrap_or(0),
        })
    } else {
        None
    };

    let mut dash = Dashboard {
        orders,
        autoload_bytes,
        autoload_healthy: autoload_bytes < AUTOLOAD_HEALTHY_BYTES,
        transients_expired: expired.rows.first().and_then(|r| count(&r[0])).unwrap_or(0),
        actions,
        hpos_drift,
        tables: tiles(ctx, &session.tables(db).await?),
        insights: Vec::new(),
    };
    dash.insights = insights(&dash, db);
    Ok(dash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::testing::site;

    fn ctx() -> WpContext {
        site(&["actionscheduler_actions"]).0
    }

    fn table(name: &str, rows: u64) -> TableInfo {
        TableInfo {
            name: name.into(),
            is_view: false,
            rows_estimate: Some(rows),
            size_bytes: Some(rows * 100),
            comment: String::new(),
        }
    }

    #[test]
    fn tables_are_categorized() {
        let c = ctx();
        assert_eq!(table_category(&c, "wp_woocommerce_log"), "logs");
        assert_eq!(
            table_category(&c, "wp_actionscheduler_logs"),
            "action_scheduler"
        );
        assert_eq!(table_category(&c, "wp_wc_orders_meta"), "woocommerce");
        assert_eq!(table_category(&c, "wp_postmeta"), "core");
        assert_eq!(table_category(&c, "wp_yoast_indexable"), "plugins");
    }

    #[test]
    fn day_buckets_cover_fourteen_days_with_status_groups() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let rows = vec![
            ("2026-10-15".to_string(), "wc-completed".to_string(), 3),
            ("2026-10-15".to_string(), "wc-failed".to_string(), 1),
            ("2026-10-14".to_string(), "wc-on-hold".to_string(), 2),
            ("2026-09-01".to_string(), "wc-completed".to_string(), 9),
        ];
        let b = day_buckets(&rows, today);
        assert_eq!(b.len(), 14);
        assert_eq!(b[0].date, "2026-10-02");
        assert_eq!((b[13].completed, b[13].failed), (3, 1));
        assert_eq!(b[12].other, 2);
    }

    #[test]
    fn insights_flag_dominant_log_tables_and_problems() {
        let c = ctx();
        let mut d = Dashboard {
            orders: None,
            autoload_bytes: 900 * 1024,
            autoload_healthy: false,
            transients_expired: 11,
            actions: Some(ActionStats {
                total: 10,
                by_status: vec![Facet {
                    value: "failed".into(),
                    label: "Failed".into(),
                    count: 3,
                }],
                overdue: 0,
            }),
            hpos_drift: Some(2),
            tables: tiles(
                &c,
                &[
                    table("wp_woocommerce_log", 44_000),
                    table("wp_postmeta", 20_000),
                    table("wp_options", 5_000),
                ],
            ),
            insights: vec![],
        };
        d.insights = insights(&d, "wp_test");
        let texts: Vec<&str> = d.insights.iter().map(|i| i.text.as_str()).collect();
        assert!(
            texts[0].starts_with(
                "wp_woocommerce_log holds 63% of all rows. Consider a retention rule."
            )
        );
        assert!(texts.iter().any(|t| t.contains("900 KB")));
        assert!(
            texts
                .iter()
                .any(|t| t.contains("3 scheduled actions failed"))
        );
        assert!(texts.iter().any(|t| t.contains("2 orders differ")));
        assert!(texts.iter().any(|t| t.contains("11 expired transients")));

        let retention = d.insights[0].action.as_ref().unwrap();
        assert_eq!(retention.label, "Set retention");
        assert!(matches!(
            retention.change,
            WpChangeRequest::DeleteLogsBefore { days: 30, .. }
        ));
        let cleanup = d
            .insights
            .iter()
            .find(|i| i.text.contains("expired transients"))
            .unwrap();
        assert!(matches!(
            cleanup.action.as_ref().unwrap().change,
            WpChangeRequest::DeleteExpiredTransients { .. }
        ));
    }

    #[test]
    fn insight_levels_serialize_to_their_wire_strings() {
        for (level, wire) in [
            (InsightLevel::Info, "info"),
            (InsightLevel::Warning, "warning"),
            (InsightLevel::Danger, "danger"),
        ] {
            assert_eq!(serde_json::to_value(level).unwrap(), wire);
        }
    }

    #[test]
    fn hour_buckets_cover_the_last_day_by_hour() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-16T09:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rows = vec![
            (
                "2026-10-16 09:00".to_string(),
                "wc-processing".to_string(),
                2,
            ),
            (
                "2026-10-15 10:00".to_string(),
                "wc-completed".to_string(),
                1,
            ),
            (
                "2026-10-15 09:00".to_string(),
                "wc-completed".to_string(),
                7,
            ),
        ];
        let b = hour_buckets(&rows, now);
        assert_eq!(b.len(), 24);
        assert_eq!(b[0].date, "10:00");
        assert_eq!(b[0].completed, 1);
        assert_eq!(b[23].date, "09:00");
        assert_eq!(b[23].processing, 2);
        assert_eq!(
            b.iter().map(|x| x.completed).sum::<u64>(),
            1,
            "09:00 yesterday is outside the window"
        );
    }

    fn at(rfc3339: &str) -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn hour_buckets_cross_midnight_in_utc() {
        let rows = vec![
            ("2026-10-15 01:00".to_string(), "wc-failed".to_string(), 2),
            (
                "2026-10-16 00:00".to_string(),
                "wc-cancelled".to_string(),
                3,
            ),
            ("2026-10-15 00:00".to_string(), "wc-failed".to_string(), 9),
        ];

        let b = hour_buckets(&rows, at("2026-10-16T00:15:00Z"));

        assert_eq!((b[0].date.as_str(), b[0].failed), ("01:00", 2));
        assert_eq!((b[23].date.as_str(), b[23].failed), ("00:00", 3));
        assert_eq!(b.iter().map(|x| x.failed).sum::<u64>(), 5);
    }
}
