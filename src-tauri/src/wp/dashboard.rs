//! The WordPress dashboard and the counts shown in the entity browser.

use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;

use super::changes::ChangeRequest;
use super::context::{HposMode, WpContext};
use super::lists::{
    self, ActionSummary, Facet, HookFailures, ListQuery, OrderSummary, num, status_label, text,
};
use super::{fetch, sql_string};
use crate::db::mysql::MySqlSession;
use crate::db::{TableInfo, quote_ident};
use crate::error::AppResult;

pub const AUTOLOAD_HEALTHY_BYTES: u64 = 800 * 1024;
const DAYS: i64 = 14;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TableTile {
    pub name: String,
    pub rows: u64,
    pub bytes: u64,
    /// core, woocommerce, action_scheduler, logs, plugins
    pub category: &'static str,
    /// Owning plugin from the registry, e.g. `WooCommerce`; `None` when unknown.
    pub owner: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayBucket {
    pub date: String,
    pub completed: u64,
    pub processing: u64,
    pub failed: u64,
    pub other: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrderStats {
    pub total: u64,
    pub today: u64,
    pub by_status: Vec<Facet>,
    pub days: Vec<DayBucket>,
    /// `day` (14 days) or `hour` (last 24 h, used when fewer than 3 days have orders).
    pub granularity: &'static str,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
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
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InsightAction {
    pub label: String,
    /// `navigate` or `change`
    pub kind: &'static str,
    pub list: Option<&'static str>,
    pub status: Option<&'static str>,
    pub change: Option<ChangeRequest>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    /// info, warning, danger
    pub level: &'static str,
    pub text: String,
    pub action: Option<InsightAction>,
}

fn go(label: &str, list: &'static str, status: Option<&'static str>) -> Option<InsightAction> {
    Some(InsightAction {
        label: label.into(),
        kind: "navigate",
        list: Some(list),
        status,
        change: None,
    })
}

fn change(label: &str, change: ChangeRequest) -> Option<InsightAction> {
    Some(InsightAction {
        label: label.into(),
        kind: "change",
        list: None,
        status: None,
        change: Some(change),
    })
}

/// 24 hourly buckets ending with the current hour, labelled `HH:00`.
pub fn hour_buckets(rows: &[(String, String, u64)], now: chrono::DateTime<Utc>) -> Vec<DayBucket> {
    (0..24)
        .rev()
        .map(|back| {
            let at = now - Duration::hours(back);
            let key = at.format("%Y-%m-%d %H:00").to_string();
            let mut bucket = DayBucket {
                date: at.format("%H:00").to_string(),
                completed: 0,
                processing: 0,
                failed: 0,
                other: 0,
            };
            for (_, status, count) in rows.iter().filter(|(d, _, _)| *d == key) {
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

#[derive(Serialize, Debug, Clone, PartialEq)]
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
    if name.starts_with("actionscheduler_") {
        "action_scheduler"
    } else if name.contains("log") {
        "logs"
    } else if name.starts_with("wc_") || name.starts_with("woocommerce_") {
        "woocommerce"
    } else if CORE_TABLES.contains(&name) {
        "core"
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
            owner: super::plugins::for_table(t.name.strip_prefix(&ctx.prefix).unwrap_or(&t.name))
                .map(|p| p.name.clone()),
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
                level: "warning",
                text: format!("{} holds {share}% of all rows.{hint}", top.name),
                action: if top.category == "logs" && top.name.ends_with("woocommerce_log") {
                    change(
                        "Set retention",
                        ChangeRequest::DeleteLogsBefore {
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
            level: "warning",
            text: format!(
                "Autoloaded options weigh {} KB; every request loads them.",
                d.autoload_bytes / 1024
            ),
            action: go("Review", "settings", None),
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
                level: "danger",
                text: format!("{failed} scheduled actions failed."),
                action: go("Review", "actions", Some("failed")),
            });
        }
        if a.overdue > 0 {
            out.push(Insight {
                level: "warning",
                text: format!(
                    "{} pending actions are overdue by more than 10 minutes.",
                    a.overdue
                ),
                action: go("Review", "actions", Some("pending")),
            });
        }
    }
    if let Some(drift) = d.hpos_drift.filter(|n| *n > 0) {
        out.push(Insight {
            level: "danger",
            text: format!("{drift} orders differ between wc_orders and their posts copy."),
            action: None,
        });
    }
    if d.transients_expired > 0 {
        out.push(Insight {
            level: "info",
            text: format!(
                "{} expired transients can be cleaned up.",
                d.transients_expired
            ),
            action: change(
                "Clean up",
                ChangeRequest::DeleteExpiredTransients {
                    database: db.into(),
                },
            ),
        });
    }
    out
}

fn t(db: &str, ctx: &WpContext, table: &str) -> String {
    format!("{}.{}", quote_ident(db), quote_ident(&ctx.table(table)))
}

pub async fn dashboard(session: &MySqlSession, ctx: &WpContext, db: &str) -> AppResult<Dashboard> {
    let options = t(db, ctx, "options");
    let autoload = fetch(
        session,
        &format!("SELECT SUM(LENGTH(option_value)) FROM {options} WHERE autoload IN ('yes', 'on', 'auto', 'auto-on')"),
    )
    .await?;
    let autoload_bytes = autoload.rows.first().and_then(|r| num(&r[0])).unwrap_or(0);
    let expired = fetch(
        session,
        &format!(
            "SELECT COUNT(*) FROM {options} WHERE (option_name LIKE '\\_transient\\_timeout\\_%' \
             OR option_name LIKE '\\_site\\_transient\\_timeout\\_%') AND CAST(option_value AS UNSIGNED) < UNIX_TIMESTAMP()"
        ),
    )
    .await?;

    let (orders, recent_orders, hpos_drift) = match &ctx.wc {
        Some(wc) => {
            let (table, status_col, date_col, type_filter) = if ctx.hpos() {
                (
                    t(db, ctx, "wc_orders"),
                    "status",
                    "date_created_gmt",
                    "type = 'shop_order'",
                )
            } else {
                (
                    t(db, ctx, "posts"),
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
            let days = fetch(
                session,
                &format!(
                    "SELECT DATE({date_col}), {status_col}, COUNT(*) FROM {table} WHERE {type_filter} \
                     AND {date_col} >= UTC_DATE() - INTERVAL {} DAY GROUP BY DATE({date_col}), {status_col}",
                    DAYS - 1
                ),
            )
            .await?;
            let day_rows: Vec<(String, String, u64)> = days
                .rows
                .iter()
                .filter_map(|r| Some((text(&r[0])?, text(&r[1])?, num(&r[2])?)))
                .collect();
            let today = Utc::now().date_naive();
            let mut buckets = day_buckets(&day_rows, today);
            let mut granularity = "day";
            let active_days = buckets
                .iter()
                .filter(|b| b.completed + b.processing + b.failed + b.other > 0)
                .count();
            if active_days < 3 {
                let hours = fetch(
                    session,
                    &format!(
                        "SELECT DATE_FORMAT({date_col}, '%Y-%m-%d %H:00'), {status_col}, COUNT(*) FROM {table} WHERE {type_filter} \
                         AND {date_col} >= UTC_TIMESTAMP() - INTERVAL 24 HOUR GROUP BY 1, 2"
                    ),
                )
                .await?;
                let hour_rows: Vec<(String, String, u64)> = hours
                    .rows
                    .iter()
                    .filter_map(|r| Some((text(&r[0])?, text(&r[1])?, num(&r[2])?)))
                    .collect();
                let today_total = buckets
                    .last()
                    .map_or(0, |b| b.completed + b.processing + b.failed + b.other);
                buckets = hour_buckets(&hour_rows, Utc::now());
                // keep "today" from the day view
                buckets.push(DayBucket {
                    date: "__today".into(),
                    completed: today_total,
                    processing: 0,
                    failed: 0,
                    other: 0,
                });
                granularity = "hour";
            }
            let today_count = if granularity == "hour" {
                buckets.pop().map_or(0, |b| b.completed)
            } else {
                buckets
                    .last()
                    .map_or(0, |b| b.completed + b.processing + b.failed + b.other)
            };
            let mut facets: Vec<Facet> = by_status
                .rows
                .iter()
                .filter_map(|r| {
                    let v = text(&r[0])?;
                    Some(Facet {
                        label: status_label(&v),
                        count: num(&r[1]).unwrap_or(0),
                        value: v,
                    })
                })
                .collect();
            facets.sort_by_key(|a| std::cmp::Reverse(a.count));
            let stats = OrderStats {
                total: facets
                    .iter()
                    .filter(|f| f.value != "trash")
                    .map(|f| f.count)
                    .sum(),
                today: today_count,
                by_status: facets,
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
                        o = t(db, ctx, "wc_orders"),
                        p = t(db, ctx, "posts"),
                    ),
                )
                .await?;
                rs.rows.first().and_then(|r| num(&r[0]))
            } else {
                None
            };
            (Some(stats), recent.items, drift)
        }
        None => (None, Vec::new(), None),
    };

    let actions = if ctx.has_table("actionscheduler_actions") {
        let table = t(db, ctx, "actionscheduler_actions");
        let by_status = fetch(
            session,
            &format!("SELECT status, COUNT(*) FROM {table} GROUP BY status"),
        )
        .await?;
        let mut facets: Vec<Facet> = by_status
            .rows
            .iter()
            .filter_map(|r| {
                let v = text(&r[0])?;
                Some(Facet {
                    label: status_label(&v),
                    count: num(&r[1]).unwrap_or(0),
                    value: v,
                })
            })
            .collect();
        facets.sort_by_key(|a| std::cmp::Reverse(a.count));
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
            total: facets.iter().map(|f| f.count).sum(),
            by_status: facets,
            recent_failed: failed.items,
            by_hook: lists::failed_by_hook(session, ctx, db)
                .await?
                .into_iter()
                .take(6)
                .collect(),
            overdue: overdue.rows.first().and_then(|r| num(&r[0])).unwrap_or(0),
        })
    } else {
        None
    };

    let mut dash = Dashboard {
        orders,
        recent_orders,
        autoload_bytes,
        autoload_healthy: autoload_bytes < AUTOLOAD_HEALTHY_BYTES,
        transients_expired: expired.rows.first().and_then(|r| num(&r[0])).unwrap_or(0),
        actions,
        hpos_drift,
        tables: tiles(ctx, &session.tables(db).await?),
        insights: Vec::new(),
    };
    dash.insights = insights(&dash, db);
    Ok(dash)
}

// ---------- entity browser counts ----------

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSection {
    /// orders, subscriptions, customers, products, actions, sessions, content, settings
    pub list: &'static str,
    pub label: &'static str,
    pub count: u64,
    pub breakdown: Vec<Facet>,
}

pub async fn browser(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
) -> AppResult<Vec<BrowserSection>> {
    let facet_rows = |rs: &crate::db::ResultSet| -> Vec<Facet> {
        let mut f: Vec<Facet> = rs
            .rows
            .iter()
            .filter_map(|r| {
                let v = text(&r[0])?;
                Some(Facet {
                    label: status_label(&v),
                    count: num(&r[1]).unwrap_or(0),
                    value: v,
                })
            })
            .collect();
        f.sort_by_key(|a| std::cmp::Reverse(a.count));
        f
    };
    let section = |list, label, breakdown: Vec<Facet>| BrowserSection {
        list,
        label,
        count: breakdown.iter().map(|f| f.count).sum(),
        breakdown,
    };
    let mut out = Vec::new();
    if ctx.wc.is_some() {
        let (table, status, ty) = if ctx.hpos() {
            (t(db, ctx, "wc_orders"), "status", "type")
        } else {
            (t(db, ctx, "posts"), "post_status", "post_type")
        };
        for (order_type, list, label) in [
            ("shop_order", "orders", "Orders"),
            ("shop_subscription", "subscriptions", "Subscriptions"),
        ] {
            let rs = fetch(
                session,
                &format!(
                    "SELECT {status}, COUNT(*) FROM {table} WHERE {ty} = {} GROUP BY {status}",
                    sql_string(order_type)
                ),
            )
            .await?;
            let s = section(list, label, facet_rows(&rs));
            if list == "orders" || s.count > 0 {
                out.push(s);
            }
        }
    }
    let users = fetch(
        session,
        &format!("SELECT COUNT(*) FROM {}", t(db, ctx, "users")),
    )
    .await?;
    out.push(BrowserSection {
        list: "customers",
        label: if ctx.wc.is_some() {
            "Customers"
        } else {
            "Users"
        },
        count: users.rows.first().and_then(|r| num(&r[0])).unwrap_or(0),
        breakdown: Vec::new(),
    });
    if ctx.wc.is_some() {
        let rs = fetch(
            session,
            &format!(
                "SELECT post_status, COUNT(*) FROM {} WHERE post_type = 'product' AND post_status NOT IN ('trash', 'auto-draft') GROUP BY post_status",
                t(db, ctx, "posts")
            ),
        )
        .await?;
        out.push(section("products", "Products", facet_rows(&rs)));
    }
    if ctx.has_table("actionscheduler_actions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT status, COUNT(*) FROM {} GROUP BY status",
                t(db, ctx, "actionscheduler_actions")
            ),
        )
        .await?;
        out.push(section("actions", "Scheduled actions", facet_rows(&rs)));
    }
    if ctx.has_table("woocommerce_sessions") {
        let rs = fetch(
            session,
            &format!(
                "SELECT IF(session_expiry < UNIX_TIMESTAMP(), 'expired', 'active'), COUNT(*) FROM {} GROUP BY 1",
                t(db, ctx, "woocommerce_sessions")
            ),
        )
        .await?;
        out.push(section("sessions", "Sessions", facet_rows(&rs)));
    }
    let hidden = lists::HIDDEN_POST_TYPES
        .iter()
        .map(|h| sql_string(h))
        .collect::<Vec<_>>()
        .join(", ");
    let rs = fetch(
        session,
        &format!(
            "SELECT post_type, COUNT(*) FROM {} WHERE post_type NOT IN ({hidden}) AND post_status <> 'auto-draft' GROUP BY post_type",
            t(db, ctx, "posts")
        ),
    )
    .await?;
    out.push(section("content", "Content", facet_rows(&rs)));
    let rs = fetch(
        session,
        &format!("SELECT COUNT(*) FROM {}", t(db, ctx, "options")),
    )
    .await?;
    out.push(BrowserSection {
        list: "settings",
        label: "Settings & options",
        count: rs.rows.first().and_then(|r| num(&r[0])).unwrap_or(0),
        breakdown: Vec::new(),
    });
    Ok(out)
}

/// Pending/complete/failed per day for the Action Scheduler histogram.
pub async fn action_histogram(
    session: &MySqlSession,
    ctx: &WpContext,
    db: &str,
    hook: Option<&str>,
) -> AppResult<Vec<DayBucket>> {
    let hook_filter = hook
        .map(|h| format!(" AND hook = {}", sql_string(h)))
        .unwrap_or_default();
    let rs = fetch(
        session,
        &format!(
            "SELECT DATE(scheduled_date_gmt), status, COUNT(*) FROM {} WHERE scheduled_date_gmt >= UTC_DATE() - INTERVAL {} DAY \
             AND scheduled_date_gmt < UTC_DATE() + INTERVAL 1 DAY{hook_filter} GROUP BY 1, 2",
            t(db, ctx, "actionscheduler_actions"),
            DAYS - 1
        ),
    )
    .await?;
    let rows: Vec<(String, String, u64)> = rs
        .rows
        .iter()
        .filter_map(|r| {
            let status = match text(&r[1])?.as_str() {
                "complete" => "completed".to_string(),
                "pending" | "in-progress" => "processing".to_string(),
                "failed" => "failed".to_string(),
                other => other.to_string(),
            };
            Some((text(&r[0])?, status, num(&r[2])?))
        })
        .collect();
    Ok(day_buckets(&rows, Utc::now().date_naive()))
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
            ("Set retention", "change")
        );
        assert!(matches!(
            retention.change,
            Some(ChangeRequest::DeleteLogsBefore { days: 30, .. })
        ));
        let failed = d
            .insights
            .iter()
            .find(|i| i.text.contains("actions failed"))
            .unwrap();
        let a = failed.action.as_ref().unwrap();
        assert_eq!(
            (a.kind, a.list, a.status),
            ("navigate", Some("actions"), Some("failed"))
        );
        let cleanup = d
            .insights
            .iter()
            .find(|i| i.text.contains("expired transients"))
            .unwrap();
        assert!(matches!(
            cleanup.action.as_ref().unwrap().change,
            Some(ChangeRequest::DeleteExpiredTransients { .. })
        ));
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
}
