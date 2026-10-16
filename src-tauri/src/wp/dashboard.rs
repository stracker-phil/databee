//! The WordPress dashboard and the counts shown in the entity browser.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use serde::Serialize;
use ts_rs::TS;

use super::changes::WpChangeRequest;
use super::context::{HposMode, SiteZone, WpContext};
use super::lists::{self, ActionSummary, Facet, HookFailures, ListQuery, OrderSummary};
use super::rows::{count, facets, text};
use super::sql::{autoload_on_sql, table_ref};
use super::{WpList, fetch};
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
    pub recent_failed: Vec<ActionSummary>,
    /// Failed actions grouped by hook, newest first.
    pub by_hook: Vec<HookFailures>,
    pub overdue: u64,
}

/// One button per alert: open a filtered list, or queue a reviewed change.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Navigate,
    Change,
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
pub struct InsightAction {
    pub label: String,
    pub kind: ActionKind,
    pub list: Option<WpList>,
    pub status: Option<&'static str>,
    pub change: Option<WpChangeRequest>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub level: InsightLevel,
    pub text: String,
    pub action: Option<InsightAction>,
}

fn go(label: &str, list: WpList, status: Option<&'static str>) -> Option<InsightAction> {
    Some(InsightAction {
        label: label.into(),
        kind: ActionKind::Navigate,
        list: Some(list),
        status,
        change: None,
    })
}

fn change(label: &str, change: WpChangeRequest) -> Option<InsightAction> {
    Some(InsightAction {
        label: label.into(),
        kind: ActionKind::Change,
        list: None,
        status: None,
        change: Some(change),
    })
}

/// Rows dated in `[start, end)` count towards the bucket `key`.
struct Span {
    key: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

const SQL_DATETIME: &str = "%Y-%m-%d %H:%M:%S";

/// The `DAYS` local days of the site ending with `today`, keyed `YYYY-MM-DD`.
fn day_spans(today: NaiveDate, zone: &SiteZone) -> Vec<Span> {
    let midnight = |date: NaiveDate| zone.to_utc(date.and_time(NaiveTime::MIN));
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

/// The 24 local hours ending with the current one, keyed by their UTC start, which stays
/// unique when DST repeats an hour.
fn hour_spans(now: DateTime<Utc>, zone: &SiteZone) -> Vec<Span> {
    let offset = zone.offset_at(now);
    let local = now.naive_utc() + offset;
    let local_hour = local
        .date()
        .and_hms_opt(local.hour(), 0, 0)
        .expect("the hour of a valid time is a valid time");
    let current = (local_hour - offset).and_utc();
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

/// 24 hourly buckets ending with the current local hour, labelled `HH:00` in the site zone.
pub fn hour_buckets(
    rows: &[(String, String, u64)],
    now: DateTime<Utc>,
    zone: &SiteZone,
) -> Vec<DayBucket> {
    hour_spans(now, zone)
        .into_iter()
        .map(|span| {
            let mut bucket = DayBucket {
                date: zone.to_local(span.start).format("%H:00").to_string(),
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
    pub recent_orders: Vec<OrderSummary>,
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

/// Fills all `DAYS` days ending today (site zone) from (date, status, count) rows.
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
            action: go("Review", WpList::Settings, None),
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
                action: go("Review", WpList::Actions, Some("failed")),
            });
        }
        if a.overdue > 0 {
            out.push(Insight {
                level: InsightLevel::Warning,
                text: format!(
                    "{} pending actions are overdue by more than 10 minutes.",
                    a.overdue
                ),
                action: go("Review", WpList::Actions, Some("pending")),
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

    let (orders, recent_orders, hpos_drift) = match &ctx.wc {
        Some(wc) => {
            let now = server_now(session).await?;
            let today = ctx.site.zone.to_local(now).date();
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
            let (day_case, day_range) = span_sql(date_col, &day_spans(today, &ctx.site.zone));
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
                let (hour_case, hour_range) = span_sql(date_col, &hour_spans(now, &ctx.site.zone));
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
                buckets = hour_buckets(&hour_rows, now, &ctx.site.zone);
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
            let recent = lists::orders(
                session,
                ctx,
                db,
                "shop_order",
                &ListQuery {
                    limit: 8,
                    ..Default::default()
                },
            )
            .await?;
            let drift = if wc.hpos == HposMode::OnWithSync {
                let rs = fetch(
                    session,
                    &format!(
                        "SELECT COUNT(*) FROM {o} o LEFT JOIN {p} p ON p.ID = o.id WHERE o.type IN ('shop_order', 'shop_order_refund') \
                         AND (p.ID IS NULL OR p.post_type <> o.type OR p.post_modified_gmt <> o.date_updated_gmt)",
                        o = table_ref(dialect, db, ctx, "wc_orders"),
                        p = table_ref(dialect, db, ctx, "posts"),
                    ),
                )
                .await?;
                rs.rows.first().and_then(|r| count(&r[0]))
            } else {
                None
            };
            (Some(stats), recent.items, drift)
        }
        None => (None, Vec::new(), None),
    };

    let actions = if ctx.has_table("actionscheduler_actions") {
        let table = table_ref(dialect, db, ctx, "actionscheduler_actions");
        let by_status = fetch(
            session,
            &format!("SELECT status, COUNT(*) FROM {table} GROUP BY status"),
        )
        .await?;
        let status_facets = facets(&by_status, 0, 1);
        let failed = lists::actions(
            session,
            ctx,
            db,
            &ListQuery {
                status: Some("failed".into()),
                limit: 5,
                ..Default::default()
            },
        )
        .await?;
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
            recent_failed: failed.items,
            by_hook: lists::failed_by_hook(session, ctx, db)
                .await?
                .into_iter()
                .take(6)
                .collect(),
            overdue: overdue.rows.first().and_then(|r| count(&r[0])).unwrap_or(0),
        })
    } else {
        None
    };

    let mut dash = Dashboard {
        orders,
        recent_orders,
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

// ---------- entity browser counts ----------

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct BrowserSection {
    pub list: WpList,
    pub label: &'static str,
    pub count: u64,
    pub breakdown: Vec<Facet>,
}

pub async fn browser(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
) -> AppResult<Vec<BrowserSection>> {
    let dialect = session.dialect();
    let section = |list, label, breakdown: Vec<Facet>| BrowserSection {
        list,
        label,
        count: breakdown.iter().map(|f| f.count).sum(),
        breakdown,
    };
    let mut out = Vec::new();
    if ctx.wc.is_some() {
        let (table, status, ty) = if ctx.hpos() {
            (table_ref(dialect, db, ctx, "wc_orders"), "status", "type")
        } else {
            (
                table_ref(dialect, db, ctx, "posts"),
                "post_status",
                "post_type",
            )
        };
        for (order_type, list, label) in [
            ("shop_order", WpList::Orders, "Orders"),
            ("shop_subscription", WpList::Subscriptions, "Subscriptions"),
        ] {
            let rs = fetch(
                session,
                &format!(
                    "SELECT {status}, COUNT(*) FROM {table} WHERE {ty} = {} GROUP BY {status}",
                    dialect.sql_string(order_type)
                ),
            )
            .await?;
            let s = section(list, label, facets(&rs, 0, 1));
            if list == WpList::Orders || s.count > 0 {
                out.push(s);
            }
        }
    }
    let users = fetch(
        session,
        &format!(
            "SELECT COUNT(*) FROM {}",
            table_ref(dialect, db, ctx, "users")
        ),
    )
    .await?;
    out.push(BrowserSection {
        list: WpList::Users,
        label: "Users",
        count: users.rows.first().and_then(|r| count(&r[0])).unwrap_or(0),
        breakdown: Vec::new(),
    });
    if ctx.wc.is_some() {
        let rs = fetch(
            session,
            &format!(
                "SELECT post_status, COUNT(*) FROM {} WHERE post_type = 'product' AND post_status NOT IN ('trash', 'auto-draft') GROUP BY post_status",
                table_ref(dialect, db, ctx, "posts")
            ),
        )
        .await?;
        out.push(section(WpList::Products, "Products", facets(&rs, 0, 1)));
    }
    if ctx.has_table("actionscheduler_actions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT status, COUNT(*) FROM {} GROUP BY status",
                table_ref(dialect, db, ctx, "actionscheduler_actions")
            ),
        )
        .await?;
        out.push(section(
            WpList::Actions,
            "Scheduled actions",
            facets(&rs, 0, 1),
        ));
    }
    if ctx.has_table("woocommerce_sessions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT IF(session_expiry < UNIX_TIMESTAMP(), 'expired', 'active'), COUNT(*) FROM {} GROUP BY 1",
                table_ref(dialect, db, ctx, "woocommerce_sessions")
            ),
        )
        .await?;
        out.push(section(WpList::Sessions, "Sessions", facets(&rs, 0, 1)));
    }
    let hidden = lists::HIDDEN_POST_TYPES
        .iter()
        .map(|h| dialect.sql_string(h))
        .collect::<Vec<_>>()
        .join(", ");
    let rs = fetch(
        session,
        &format!(
            "SELECT post_type, COUNT(*) FROM {} WHERE post_type NOT IN ({hidden}) AND post_status <> 'auto-draft' GROUP BY post_type",
            table_ref(dialect, db, ctx, "posts")
        ),
    )
    .await?;
    out.push(section(WpList::Content, "Content", facets(&rs, 0, 1)));
    let rs = fetch(
        session,
        &format!(
            "SELECT COUNT(*) FROM {}",
            table_ref(dialect, db, ctx, "options")
        ),
    )
    .await?;
    out.push(BrowserSection {
        list: WpList::Settings,
        label: "Options",
        count: rs.rows.first().and_then(|r| count(&r[0])).unwrap_or(0),
        breakdown: Vec::new(),
    });
    Ok(out)
}

/// Action Scheduler statuses per day, folded into the `DayBucket` series; `labels` names the
/// stored statuses behind each series.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct ActionHistogram {
    pub days: Vec<DayBucket>,
    pub labels: SeriesLabels,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct SeriesLabels {
    pub completed: String,
    pub processing: String,
    pub failed: String,
    pub other: String,
}

pub async fn action_histogram(
    session: &dyn Database,
    ctx: &WpContext,
    db: &str,
    hook: Option<&str>,
) -> AppResult<ActionHistogram> {
    let dialect = session.dialect();
    let hook_filter = hook
        .map(|h| format!(" AND hook = {}", dialect.sql_string(h)))
        .unwrap_or_default();
    let today = ctx.site.zone.to_local(server_now(session).await?).date();
    let (day_case, day_range) = span_sql("scheduled_date_gmt", &day_spans(today, &ctx.site.zone));
    let rs = fetch(
        session,
        &format!(
            "SELECT {day_case}, status, COUNT(*) FROM {} WHERE {day_range}{hook_filter} GROUP BY 1, 2",
            table_ref(dialect, db, ctx, "actionscheduler_actions"),
        ),
    )
    .await?;
    let mut others: Vec<String> = Vec::new();
    let rows: Vec<(String, String, u64)> = rs
        .rows
        .iter()
        .filter_map(|r| {
            let stored = text(&r[1])?;
            let series = match stored.as_str() {
                "complete" => "completed",
                "pending" | "in-progress" => "processing",
                "failed" => "failed",
                _ => {
                    if !others.contains(&stored) {
                        others.push(stored.clone());
                    }
                    "other"
                }
            };
            Some((text(&r[0])?, series.to_string(), count(&r[2])?))
        })
        .collect();
    others.sort();
    Ok(ActionHistogram {
        days: day_buckets(&rows, today),
        labels: SeriesLabels {
            completed: "complete".into(),
            processing: "pending, in-progress".into(),
            failed: "failed".into(),
            other: if others.is_empty() {
                "other".into()
            } else {
                others.join(", ")
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> WpContext {
        WpContext {
            prefix: "wp_".into(),
            version: None,
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: Default::default(),
        }
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
            recent_orders: vec![],
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
                recent_failed: vec![],
                by_hook: vec![],
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
        assert_eq!(
            (retention.label.as_str(), retention.kind),
            ("Set retention", ActionKind::Change)
        );
        assert!(matches!(
            retention.change,
            Some(WpChangeRequest::DeleteLogsBefore { days: 30, .. })
        ));
        let failed = d
            .insights
            .iter()
            .find(|i| i.text.contains("actions failed"))
            .unwrap();
        let a = failed.action.as_ref().unwrap();
        assert_eq!(
            (a.kind, a.list, a.status),
            (ActionKind::Navigate, Some(WpList::Actions), Some("failed"))
        );
        let cleanup = d
            .insights
            .iter()
            .find(|i| i.text.contains("expired transients"))
            .unwrap();
        assert!(matches!(
            cleanup.action.as_ref().unwrap().change,
            Some(WpChangeRequest::DeleteExpiredTransients { .. })
        ));
    }

    #[test]
    fn insight_actions_and_levels_serialize_to_their_wire_strings() {
        for (kind, wire) in [
            (ActionKind::Navigate, "navigate"),
            (ActionKind::Change, "change"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), wire);
        }
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
        let b = hour_buckets(&rows, now, &SiteZone::default());
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
    fn hour_buckets_are_labelled_in_the_named_site_zone() {
        let zone = SiteZone::Named(chrono_tz::Europe::Berlin);
        let rows = vec![
            (
                "2026-10-16 09:00".to_string(),
                "wc-completed".to_string(),
                4,
            ),
            (
                "2026-10-16 10:00".to_string(),
                "wc-completed".to_string(),
                9,
            ),
        ];
        let b = hour_buckets(&rows, at("2026-10-16T09:30:00Z"), &zone);
        assert_eq!(b.len(), 24);
        assert_eq!(b[23].date, "11:00");
        assert_eq!(b[23].completed, 4);
        assert_eq!(b[0].date, "12:00");
        assert_eq!(
            b.iter().map(|x| x.completed).sum::<u64>(),
            4,
            "10:00 UTC is in the future"
        );
    }

    #[test]
    fn hour_buckets_follow_a_half_hour_offset() {
        let zone = SiteZone::Fixed(chrono::FixedOffset::east_opt(19800).unwrap());
        let rows = vec![("2026-10-16 09:30".to_string(), "wc-failed".to_string(), 2)];
        let b = hour_buckets(&rows, at("2026-10-16T09:45:00Z"), &zone);
        assert_eq!(b[23].date, "15:00");
        assert_eq!(b[23].failed, 2);

        let unmatched = vec![("2026-10-16 09:00".to_string(), "wc-failed".to_string(), 2)];
        let b = hour_buckets(&unmatched, at("2026-10-16T09:45:00Z"), &zone);
        assert_eq!(b.iter().map(|x| x.failed).sum::<u64>(), 0);
    }
}
