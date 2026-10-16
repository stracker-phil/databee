//! The paged SELECT behind the table view and the filter typed above it.

use super::{AppliedFilter, BrowseRequest, Dialect, FilterKind, id_filter, lexer};
use crate::error::{AppError, AppResult};

/// Matches the largest page the UI asks for.
const MAX_PAGE_ROWS: u32 = 500;

/// Page size the caller gets; the query fetches one more row to detect a next page.
pub fn page_limit(req: &BrowseRequest) -> u32 {
    req.limit.clamp(1, MAX_PAGE_ROWS)
}

/// Resolves the typed filter: ID shorthand against the primary key, or raw SQL.
fn applied_filter(req: &BrowseRequest) -> AppResult<Option<AppliedFilter>> {
    let Some(filter) = req.filter.as_deref().map(str::trim) else {
        return Ok(None);
    };
    if !lexer::is_terminated(filter) {
        return Err(AppError::InvalidInput(
            "unterminated quote or comment in filter".into(),
        ));
    }
    if lexer::is_blank(filter) {
        return Ok(None);
    }
    if let (Some(pk), Some(ids)) = (req.primary_key.as_deref(), id_filter::parse_ids(filter)) {
        return Ok(Some(AppliedFilter {
            kind: FilterKind::Ids,
            sql: ids.to_sql(pk),
            label: ids.label(pk),
        }));
    }
    if lexer::split_statements(filter).len() != 1 {
        return Err(AppError::InvalidInput(
            "The filter must be a single condition".into(),
        ));
    }
    Ok(Some(AppliedFilter {
        kind: FilterKind::Sql,
        sql: filter.to_string(),
        label: filter.to_string(),
    }))
}

/// Builds the paged SELECT for the table view and returns the filter it applied.
/// Fetches one extra row to detect more pages.
pub fn browse_sql(
    req: &BrowseRequest,
    dialect: Dialect,
) -> AppResult<(String, Option<AppliedFilter>)> {
    let mut sql = format!(
        "SELECT * FROM {}",
        dialect.table_ref(&req.database, &req.table)
    );
    let filter = applied_filter(req)?;
    if let Some(filter) = &filter {
        // The newline ends a trailing `#` or `-- ` comment before the closing parenthesis.
        sql.push_str(&format!(" WHERE ({}\n)", filter.sql));
    }
    if let Some(col) = &req.order_by {
        let dir = if req.descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(" ORDER BY {} {dir}", dialect.quote_ident(col)));
    }
    sql.push_str(&format!(
        " LIMIT {} OFFSET {}",
        u64::from(page_limit(req)) + 1,
        req.offset
    ));
    Ok((sql, filter))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> BrowseRequest {
        BrowseRequest {
            database: "wordpress".into(),
            table: "wp_posts".into(),
            limit: 100,
            ..Default::default()
        }
    }

    #[test]
    fn browse_fetches_one_extra_row_for_paging() {
        assert_eq!(
            browse_sql(
                &BrowseRequest {
                    offset: 200,
                    ..req()
                },
                Dialect::MYSQL
            )
            .unwrap()
            .0,
            "SELECT * FROM `wordpress`.`wp_posts` LIMIT 101 OFFSET 200"
        );
    }

    #[test]
    fn browse_applies_filter_and_order() {
        let sql = browse_sql(
            &BrowseRequest {
                order_by: Some("post_date".into()),
                descending: true,
                filter: Some(" post_type = 'page' ".into()),
                ..req()
            },
            Dialect::MYSQL,
        )
        .unwrap()
        .0;
        assert_eq!(
            sql,
            "SELECT * FROM `wordpress`.`wp_posts` WHERE (post_type = 'page'\n) ORDER BY `post_date` DESC LIMIT 101 OFFSET 0"
        );
    }

    fn sql_for(filter: &str, order_by: Option<&str>) -> AppResult<(String, Option<AppliedFilter>)> {
        browse_sql(
            &BrowseRequest {
                filter: Some(filter.into()),
                order_by: order_by.map(Into::into),
                ..req()
            },
            Dialect::MYSQL,
        )
    }

    #[test]
    fn trailing_line_comment_in_filter_does_not_swallow_the_limit() {
        for (filter, order_by, expected) in [
            (
                "ID > 0 # note",
                None,
                "SELECT * FROM `wordpress`.`wp_posts` WHERE (ID > 0 # note\n) LIMIT 101 OFFSET 0",
            ),
            (
                "ID > 0 -- x",
                None,
                "SELECT * FROM `wordpress`.`wp_posts` WHERE (ID > 0 -- x\n) LIMIT 101 OFFSET 0",
            ),
            (
                "ID > 0 # note",
                Some("ID"),
                "SELECT * FROM `wordpress`.`wp_posts` WHERE (ID > 0 # note\n) ORDER BY `ID` ASC LIMIT 101 OFFSET 0",
            ),
            (
                "ID > 0 -- x",
                Some("ID"),
                "SELECT * FROM `wordpress`.`wp_posts` WHERE (ID > 0 -- x\n) ORDER BY `ID` ASC LIMIT 101 OFFSET 0",
            ),
        ] {
            assert_eq!(sql_for(filter, order_by).unwrap().0, expected, "{filter}");
        }
    }

    #[test]
    fn unterminated_comment_or_quote_in_filter_is_rejected() {
        for filter in ["ID > 0 /* x", "post_title = 'x"] {
            let err = sql_for(filter, None).unwrap_err();
            assert_eq!(err.kind(), "invalid_input", "{filter}");
        }
    }

    #[test]
    fn comment_only_filter_behaves_like_no_filter() {
        let (expected, _) = browse_sql(&req(), Dialect::MYSQL).unwrap();
        for filter in ["-- note", "# x", "/* x */"] {
            let (sql, applied) = sql_for(filter, None).unwrap();
            assert_eq!(sql, expected, "{filter}");
            assert!(applied.is_none(), "{filter}");
        }
    }

    #[test]
    fn order_by_column_backticks_are_doubled() {
        assert_eq!(
            sql_for("1", Some("a`b")).unwrap().0,
            "SELECT * FROM `wordpress`.`wp_posts` WHERE (1\n) ORDER BY `a``b` ASC LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn primary_key_backticks_are_doubled_in_the_id_condition() {
        let (sql, _) = browse_sql(
            &BrowseRequest {
                filter: Some("3229".into()),
                primary_key: Some("I`D".into()),
                ..req()
            },
            Dialect::MYSQL,
        )
        .unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM `wordpress`.`wp_posts` WHERE (`I``D` = 3229\n) LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn requested_limit_is_clamped_to_the_page_bounds() {
        for (limit, page, fetched) in [(0, 1, 2), (u32::MAX, 500, 501)] {
            let r = BrowseRequest { limit, ..req() };
            assert_eq!(page_limit(&r), page, "{limit}");
            assert_eq!(
                browse_sql(&r, Dialect::MYSQL).unwrap().0,
                format!("SELECT * FROM `wordpress`.`wp_posts` LIMIT {fetched} OFFSET 0"),
                "{limit}"
            );
        }
    }

    #[test]
    fn filter_kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(FilterKind::Ids).unwrap(),
            serde_json::json!("ids")
        );
        assert_eq!(
            serde_json::to_value(FilterKind::Sql).unwrap(),
            serde_json::json!("sql")
        );
    }

    #[test]
    fn browse_rejects_filters_with_extra_statements() {
        let err = browse_sql(
            &BrowseRequest {
                filter: Some("1; DROP TABLE wp_users".into()),
                ..req()
            },
            Dialect::MYSQL,
        )
        .unwrap_err();
        assert_eq!(err.kind(), "invalid_input");
    }

    #[test]
    fn id_shorthand_with_primary_key_filters_by_ids() {
        let with_pk = BrowseRequest {
            filter: Some("3220-3225, 3229".into()),
            primary_key: Some("ID".into()),
            ..req()
        };
        let (sql, applied) = browse_sql(&with_pk, Dialect::MYSQL).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM `wordpress`.`wp_posts` WHERE ((`ID` BETWEEN 3220 AND 3225 OR `ID` = 3229)\n) LIMIT 101 OFFSET 0"
        );
        let applied = applied.unwrap();
        assert_eq!(applied.kind, FilterKind::Ids);
        assert_eq!(applied.label, "ID in (3220–3225, 3229)");
    }

    #[test]
    fn id_shorthand_without_primary_key_is_sql_as_typed() {
        let without = BrowseRequest {
            filter: Some("3229".into()),
            primary_key: None,
            ..req()
        };
        let (sql, applied) = browse_sql(&without, Dialect::MYSQL).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM `wordpress`.`wp_posts` WHERE (3229\n) LIMIT 101 OFFSET 0"
        );
        assert_eq!(applied.unwrap().kind, FilterKind::Sql);
    }

    #[test]
    fn real_sql_is_never_mistaken_for_ids() {
        let request = BrowseRequest {
            filter: Some("ID > 3229".into()),
            primary_key: Some("ID".into()),
            ..req()
        };
        assert_eq!(
            applied_filter(&request).unwrap().unwrap().kind,
            FilterKind::Sql
        );
    }

    #[test]
    fn blank_filter_is_ignored() {
        let sql = browse_sql(
            &BrowseRequest {
                filter: Some("  ".into()),
                ..req()
            },
            Dialect::MYSQL,
        )
        .unwrap()
        .0;
        assert!(!sql.contains("WHERE"));
    }
}
