//! WordPress edits as reviewable SQL, planned into the session's plans like row edits
//! (`db::changes`): each planner fetches the current state, then a pure builder turns it into
//! a `Plan` with the statements that undo it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::fetch;
use super::rows::{self, text};
use super::site::WpContext;
use super::sql::table_ref;
use crate::db::changes::{Plan, reinsert_sql};
use crate::db::{Database, Dialect, ResultSet};
use crate::error::{AppError, AppResult};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WpChangeRequest {
    #[serde(rename_all = "camelCase")]
    SetAutoload {
        database: String,
        option: String,
        on: bool,
    },
    #[serde(rename_all = "camelCase")]
    DeleteExpiredTransients { database: String },
    /// Retention for `woocommerce_log`: drop entries older than `days` (optionally one source).
    #[serde(rename_all = "camelCase")]
    DeleteLogsBefore {
        database: String,
        days: u32,
        #[serde(default)]
        #[ts(optional = nullable)]
        source: Option<String>,
    },
}

impl WpChangeRequest {
    pub fn database(&self) -> &str {
        match self {
            WpChangeRequest::SetAutoload { database, .. }
            | WpChangeRequest::DeleteExpiredTransients { database }
            | WpChangeRequest::DeleteLogsBefore { database, .. } => database,
        }
    }
}

/// `cutoff` is a literal fixed at plan time, so the applied DELETE removes exactly the
/// entries that were counted.
pub fn logs_before_filter(dialect: Dialect, cutoff: &str, source: Option<&str>) -> String {
    let mut f = format!("`timestamp` < {}", dialect.sql_string(cutoff));
    if let Some(s) = source {
        f.push_str(&format!(" AND `source` = {}", dialect.sql_string(s)));
    }
    f
}

fn first_text(rs: &ResultSet) -> Option<Option<String>> {
    rs.rows.first().map(|r| text(&r[0]))
}

/// Fixed at plan time, so the review shows exactly the rows the DELETE removes.
async fn log_cutoff(session: &dyn Database, days: u32) -> AppResult<String> {
    let rs = fetch(
        session,
        &format!("SELECT UTC_TIMESTAMP() - INTERVAL {days} DAY"),
    )
    .await?;
    first_text(&rs).flatten().ok_or_else(|| AppError::Database {
        message: "The server returned no time for UTC_TIMESTAMP()".into(),
        code: None,
    })
}

pub async fn plan(
    session: &dyn Database,
    ctx: &WpContext,
    req: &WpChangeRequest,
) -> AppResult<Plan> {
    let dialect = session.dialect();
    match req {
        WpChangeRequest::SetAutoload {
            database,
            option,
            on,
        } => {
            let t = table_ref(dialect, database, ctx, "options");
            let sql = format!(
                "SELECT autoload FROM {t} WHERE option_name = {}",
                dialect.sql_string(option)
            );
            let before = first_text(&fetch(session, &sql).await?)
                .flatten()
                .ok_or_else(|| AppError::InvalidInput(format!("Option {option} not found")))?;
            Ok(build_set_autoload(
                dialect, database, &t, option, *on, &before,
            ))
        }
        WpChangeRequest::DeleteExpiredTransients { database } => {
            let t = table_ref(dialect, database, ctx, "options");
            let overview = super::options_overview(session, ctx, database).await?;
            let names = expired_transient_rows(&overview);
            if names.is_empty() {
                return Err(AppError::InvalidInput(
                    "There are no expired transients".into(),
                ));
            }
            let filter = option_names_filter(dialect, &names);
            let rows = fetch(session, &format!("SELECT * FROM {t} WHERE {filter}")).await?;
            Ok(build_delete_expired_transients(
                dialect, database, &t, &names, &rows,
            ))
        }
        WpChangeRequest::DeleteLogsBefore {
            database,
            days,
            source,
        } => {
            if !ctx.has_table("woocommerce_log") {
                return Err(AppError::InvalidInput(
                    "This site has no woocommerce_log table".into(),
                ));
            }
            let t = table_ref(dialect, database, ctx, "woocommerce_log");
            let cutoff = log_cutoff(session, *days).await?;
            let filter = logs_before_filter(dialect, &cutoff, source.as_deref());
            let count = fetch(session, &format!("SELECT COUNT(*) FROM {t} WHERE {filter}")).await?;
            let n = count
                .rows
                .first()
                .and_then(|r| rows::count(&r[0]))
                .unwrap_or(0);
            build_delete_logs_before(database, &t, &filter, *days, source.as_deref(), n)
        }
    }
}

/// `before` is the stored autoload value; its style (`yes`/`no` or the WordPress 6.6
/// `on`/`off` family) decides the new value's spelling.
pub fn build_set_autoload(
    dialect: Dialect,
    database: &str,
    table: &str,
    option: &str,
    on: bool,
    before: &str,
) -> Plan {
    let modern = !matches!(before, "yes" | "no");
    let value = match (modern, on) {
        (true, true) => "on",
        (true, false) => "off",
        (false, true) => "yes",
        (false, false) => "no",
    };
    let name = dialect.sql_string(option);
    Plan {
        database: database.to_string(),
        description: format!("Autoload of {option} → {value}"),
        statements: vec![format!(
            "UPDATE {table} SET autoload = '{value}' WHERE option_name = {name}"
        )],
        undo: vec![format!(
            "UPDATE {table} SET autoload = {} WHERE option_name = {name}",
            dialect.sql_string(before)
        )],
    }
}

/// The option names of every expired transient: its value row and its timeout row.
fn expired_transient_rows(overview: &super::options::OptionsOverview) -> Vec<String> {
    overview
        .transients
        .iter()
        .filter(|tr| tr.expired)
        .flat_map(|tr| {
            let (value, timeout) = if tr.site_wide {
                ("_site_transient_", "_site_transient_timeout_")
            } else {
                ("_transient_", "_transient_timeout_")
            };
            [
                format!("{value}{}", tr.name),
                format!("{timeout}{}", tr.name),
            ]
        })
        .collect()
}

fn option_names_filter(dialect: Dialect, names: &[String]) -> String {
    let list = names
        .iter()
        .map(|n| dialect.sql_string(n))
        .collect::<Vec<_>>()
        .join(", ");
    format!("option_name IN ({list})")
}

/// `names` are the value and timeout rows of the expired transients; `rows` their stored rows.
pub fn build_delete_expired_transients(
    dialect: Dialect,
    database: &str,
    table: &str,
    names: &[String],
    rows: &ResultSet,
) -> Plan {
    Plan {
        database: database.to_string(),
        description: format!("Delete {} expired transients", names.len() / 2),
        statements: vec![format!(
            "DELETE FROM {table} WHERE {}",
            option_names_filter(dialect, names)
        )],
        undo: reinsert_sql(dialect, table, rows, &[]),
    }
}

/// `filter` comes from `logs_before_filter`; `count` is how many entries it matches.
pub fn build_delete_logs_before(
    database: &str,
    table: &str,
    filter: &str,
    days: u32,
    source: Option<&str>,
    count: u64,
) -> AppResult<Plan> {
    if count == 0 {
        return Err(AppError::InvalidInput(format!(
            "No log entries are older than {days} days"
        )));
    }
    let from = source.map(|s| format!(" from {s}")).unwrap_or_default();
    Ok(Plan {
        database: database.to_string(),
        description: format!("Delete {count} log entries older than {days} days{from} (no undo)"),
        statements: vec![format!("DELETE FROM {table} WHERE {filter}")],
        undo: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ColumnMeta, FakeDatabase};
    use crate::error::ErrorKind;
    use crate::wp::testing::site;

    const D: Dialect = Dialect::MYSQL;
    const OPTIONS: &str = "`wp`.`wp_options`";

    fn cell(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn rs(cols: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: cols
                .iter()
                .map(|c| ColumnMeta {
                    name: c.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            returns_rows: true,
            ..Default::default()
        }
    }

    #[test]
    fn log_retention_filter_targets_age_and_source() {
        assert_eq!(
            logs_before_filter(Dialect::MYSQL, "2026-09-16 12:00:00", None),
            "`timestamp` < '2026-09-16 12:00:00'"
        );
        assert_eq!(
            logs_before_filter(
                Dialect::MYSQL,
                "2026-10-09 12:00:00",
                Some("woocommerce-paypal-payments")
            ),
            "`timestamp` < '2026-10-09 12:00:00' AND `source` = 'woocommerce-paypal-payments'"
        );
    }

    #[test]
    fn every_request_names_its_database() {
        let requests = [
            WpChangeRequest::SetAutoload {
                database: "wp".into(),
                option: "o".into(),
                on: true,
            },
            WpChangeRequest::DeleteExpiredTransients {
                database: "wp".into(),
            },
            WpChangeRequest::DeleteLogsBefore {
                database: "wp".into(),
                days: 30,
                source: None,
            },
        ];

        for request in requests {
            assert_eq!(request.database(), "wp", "{request:?}");
        }
    }

    #[test]
    fn requests_use_camel_case_tags_and_fields_and_default_the_log_source() {
        let logs: WpChangeRequest = serde_json::from_value(serde_json::json!({
            "type": "deleteLogsBefore",
            "database": "wp",
            "days": 30,
        }))
        .unwrap();
        let autoload = WpChangeRequest::SetAutoload {
            database: "wp".into(),
            option: "o".into(),
            on: true,
        };

        assert_eq!(
            logs,
            WpChangeRequest::DeleteLogsBefore {
                database: "wp".into(),
                days: 30,
                source: None,
            }
        );
        assert_eq!(
            serde_json::to_value(&autoload).unwrap(),
            serde_json::json!({"type": "setAutoload", "database": "wp", "option": "o", "on": true})
        );
    }

    #[test]
    fn autoload_keeps_the_spelling_style_of_the_stored_value() {
        for (before, on, value) in [
            ("yes", false, "no"),
            ("no", true, "yes"),
            ("on", false, "off"),
            ("off", true, "on"),
            ("auto-on", false, "off"),
            ("auto-off", true, "on"),
        ] {
            let plan = build_set_autoload(D, "wp", OPTIONS, "foo", on, before);

            assert_eq!(
                plan,
                Plan {
                    database: "wp".into(),
                    description: format!("Autoload of foo → {value}"),
                    statements: vec![format!(
                        "UPDATE {OPTIONS} SET autoload = '{value}' WHERE option_name = 'foo'"
                    )],
                    undo: vec![format!(
                        "UPDATE {OPTIONS} SET autoload = '{before}' WHERE option_name = 'foo'"
                    )],
                },
                "{before} -> on={on}"
            );
        }
    }

    #[test]
    fn expired_transients_are_deleted_by_one_statement_naming_value_and_timeout_rows() {
        let names = vec![
            "_transient_a".to_string(),
            "_transient_timeout_a".to_string(),
        ];
        let rows = rs(
            &["option_id", "option_name", "option_value"],
            vec![
                vec![Cell::Number("1".into()), cell("_transient_a"), cell("x")],
                vec![
                    Cell::Number("2".into()),
                    cell("_transient_timeout_a"),
                    cell("100"),
                ],
            ],
        );

        let plan = build_delete_expired_transients(D, "wp", OPTIONS, &names, &rows);

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Delete 1 expired transients".into(),
                statements: vec![format!(
                    "DELETE FROM {OPTIONS} WHERE option_name IN ('_transient_a', '_transient_timeout_a')"
                )],
                undo: vec![
                    format!(
                        "INSERT INTO {OPTIONS} (`option_id`, `option_name`, `option_value`) VALUES ('1', '_transient_a', 'x')"
                    ),
                    format!(
                        "INSERT INTO {OPTIONS} (`option_id`, `option_name`, `option_value`) VALUES ('2', '_transient_timeout_a', '100')"
                    ),
                ],
            }
        );
    }

    #[test]
    fn log_retention_without_matching_entries_is_refused() {
        let err = build_delete_logs_before("wp", "t", "f", 30, None, 0).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "No log entries are older than 30 days");
    }

    #[test]
    fn log_retention_deletes_by_the_given_filter_and_cannot_be_undone() {
        let plan = build_delete_logs_before(
            "wp",
            "`wp`.`wp_woocommerce_log`",
            "`timestamp` < 'c'",
            30,
            None,
            5,
        )
        .unwrap();
        let sourced = build_delete_logs_before("wp", "t", "f", 7, Some("paypal"), 2).unwrap();

        assert_eq!(
            plan,
            Plan {
                database: "wp".into(),
                description: "Delete 5 log entries older than 30 days (no undo)".into(),
                statements: vec![
                    "DELETE FROM `wp`.`wp_woocommerce_log` WHERE `timestamp` < 'c'".into()
                ],
                undo: Vec::new(),
            }
        );
        assert_eq!(
            sourced.description,
            "Delete 2 log entries older than 7 days from paypal (no undo)"
        );
    }

    #[tokio::test]
    async fn planning_an_autoload_change_reads_the_stored_value_first() {
        let (ctx, _) = site(&["options"]);
        let db = FakeDatabase::new().on(
            "SELECT autoload FROM `wp`.`wp_options` WHERE option_name = 'foo'",
            rs(&["autoload"], vec![vec![cell("yes")]]),
        );
        let request = WpChangeRequest::SetAutoload {
            database: "wp".into(),
            option: "foo".into(),
            on: false,
        };

        let plan = plan(&db, &ctx, &request).await.unwrap();

        assert_eq!(
            plan,
            build_set_autoload(D, "wp", OPTIONS, "foo", false, "yes")
        );
    }

    #[tokio::test]
    async fn planning_an_autoload_change_of_a_missing_option_is_refused() {
        let (ctx, _) = site(&["options"]);
        let db = FakeDatabase::new().on("SELECT autoload", rs(&["autoload"], vec![]));
        let request = WpChangeRequest::SetAutoload {
            database: "wp".into(),
            option: "foo".into(),
            on: true,
        };

        let err = plan(&db, &ctx, &request).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "Option foo not found");
    }

    #[tokio::test]
    async fn planning_a_log_cleanup_needs_the_log_table() {
        let (ctx, _) = site(&["options"]);
        let db = FakeDatabase::new();
        let request = WpChangeRequest::DeleteLogsBefore {
            database: "wp".into(),
            days: 30,
            source: None,
        };

        let err = plan(&db, &ctx, &request).await.unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "This site has no woocommerce_log table");
    }
}
