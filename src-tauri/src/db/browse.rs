//! The paged SELECT behind the table view and the filter typed above it.

use super::{AppliedFilter, BrowseRequest, ColumnEquals, Dialect, FilterKind, id_filter, lexer};
use crate::error::{AppError, AppResult};

/// Largest page served; the UI asks for its row limit and pages by the `limit` it gets back.
const MAX_PAGE_ROWS: u32 = 500;

/// Page size the caller gets; the query fetches one more row to detect a next page.
pub fn page_limit(req: &BrowseRequest) -> u32 {
    req.limit.clamp(1, MAX_PAGE_ROWS)
}

/// Resolves the typed filter: ID shorthand against the primary key, or raw SQL.
fn applied_filter(req: &BrowseRequest, dialect: Dialect) -> AppResult<Option<AppliedFilter>> {
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
            sql: ids.to_sql(pk, dialect),
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

/// The request's structured `where` as one condition. Values are always quoted strings: MySQL
/// compares them as strings for text columns and coerces them for numeric ones.
pub fn columns_filter(req: &BrowseRequest, dialect: Dialect) -> Option<AppliedFilter> {
    if req.r#where.is_empty() {
        return None;
    }
    Some(AppliedFilter {
        kind: FilterKind::Columns,
        sql: equals_sql(dialect, &req.r#where),
        label: render_columns(&req.r#where, str::to_string, str::to_string),
    })
}

/// One column of the first row matching `row`.
pub fn select_column_sql(
    dialect: Dialect,
    database: &str,
    table: &str,
    column: &str,
    row: &[ColumnEquals],
) -> String {
    format!(
        "SELECT {} FROM {} WHERE {} LIMIT 1",
        dialect.quote_ident(column),
        dialect.table_ref(database, table),
        equals_sql(dialect, row)
    )
}

/// `column = 'value' AND …`, quoted for `dialect`; a `None` value is `IS NULL`.
pub fn equals_sql(dialect: Dialect, columns: &[ColumnEquals]) -> String {
    render_columns(
        columns,
        |c| dialect.quote_ident(c),
        |v| dialect.sql_string(v),
    )
}

/// `equals_sql` with every column qualified as `alias.column`.
pub fn aliased_equals_sql(dialect: Dialect, alias: &str, columns: &[ColumnEquals]) -> String {
    render_columns(
        columns,
        |c| format!("{alias}.{}", dialect.quote_ident(c)),
        |v| dialect.sql_string(v),
    )
}

fn render_columns(
    columns: &[ColumnEquals],
    column: impl Fn(&str) -> String,
    value: impl Fn(&str) -> String,
) -> String {
    columns
        .iter()
        .map(|c| match &c.value {
            Some(v) => format!("{} = {}", column(&c.column), value(v)),
            None => format!("{} IS NULL", column(&c.column)),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// The `SELECT` a new query tab starts with for a table, one clause per line.
pub fn select_all_sql(dialect: Dialect, database: &str, table: &str, limit: u32) -> String {
    format!(
        "SELECT *\nFROM {}\nLIMIT {limit};",
        dialect.table_ref(database, table)
    )
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
    let filter = applied_filter(req, dialect)?;
    let mut conditions = Vec::new();
    if let Some(columns) = columns_filter(req, dialect) {
        conditions.push(format!("({})", columns.sql));
    }
    if let Some(filter) = &filter {
        // The newline ends a trailing `#` or `-- ` comment before the closing parenthesis.
        conditions.push(format!("({}\n)", filter.sql));
    }
    if !conditions.is_empty() {
        sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
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
    use crate::error::ErrorKind;

    fn req() -> BrowseRequest {
        BrowseRequest {
            database: "site".into(),
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
            "SELECT * FROM `site`.`wp_posts` LIMIT 101 OFFSET 200"
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
            "SELECT * FROM `site`.`wp_posts` WHERE (post_type = 'page'\n) ORDER BY `post_date` DESC LIMIT 101 OFFSET 0"
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
                "SELECT * FROM `site`.`wp_posts` WHERE (ID > 0 # note\n) LIMIT 101 OFFSET 0",
            ),
            (
                "ID > 0 -- x",
                None,
                "SELECT * FROM `site`.`wp_posts` WHERE (ID > 0 -- x\n) LIMIT 101 OFFSET 0",
            ),
            (
                "ID > 0 # note",
                Some("ID"),
                "SELECT * FROM `site`.`wp_posts` WHERE (ID > 0 # note\n) ORDER BY `ID` ASC LIMIT 101 OFFSET 0",
            ),
            (
                "ID > 0 -- x",
                Some("ID"),
                "SELECT * FROM `site`.`wp_posts` WHERE (ID > 0 -- x\n) ORDER BY `ID` ASC LIMIT 101 OFFSET 0",
            ),
        ] {
            assert_eq!(sql_for(filter, order_by).unwrap().0, expected, "{filter}");
        }
    }

    #[test]
    fn unterminated_comment_or_quote_in_filter_is_rejected() {
        for filter in ["ID > 0 /* x", "post_title = 'x"] {
            let err = sql_for(filter, None).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{filter}");
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
            "SELECT * FROM `site`.`wp_posts` WHERE (1\n) ORDER BY `a``b` ASC LIMIT 101 OFFSET 0"
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
            "SELECT * FROM `site`.`wp_posts` WHERE (`I``D` = 3229\n) LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn requested_limit_is_clamped_to_the_page_bounds() {
        for (limit, page, fetched) in [(0, 1, 2), (u32::MAX, 500, 501)] {
            let r = BrowseRequest { limit, ..req() };
            assert_eq!(page_limit(&r), page, "{limit}");
            assert_eq!(
                browse_sql(&r, Dialect::MYSQL).unwrap().0,
                format!("SELECT * FROM `site`.`wp_posts` LIMIT {fetched} OFFSET 0"),
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
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
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
            "SELECT * FROM `site`.`wp_posts` WHERE ((`ID` BETWEEN 3220 AND 3225 OR `ID` = 3229)\n) LIMIT 101 OFFSET 0"
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
            "SELECT * FROM `site`.`wp_posts` WHERE (3229\n) LIMIT 101 OFFSET 0"
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
            applied_filter(&request, Dialect::MYSQL)
                .unwrap()
                .unwrap()
                .kind,
            FilterKind::Sql
        );
    }

    fn eq(column: &str, value: Option<&str>) -> ColumnEquals {
        ColumnEquals {
            column: column.into(),
            value: value.map(Into::into),
        }
    }

    fn where_sql(r#where: Vec<ColumnEquals>, dialect: Dialect) -> String {
        browse_sql(&BrowseRequest { r#where, ..req() }, dialect)
            .unwrap()
            .0
    }

    #[test]
    fn structured_where_renders_a_quoted_string_comparison() {
        assert_eq!(
            where_sql(vec![eq("ID", Some("007"))], Dialect::MYSQL),
            "SELECT * FROM `site`.`wp_posts` WHERE (`ID` = '007') LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn structured_where_with_no_value_renders_is_null() {
        assert_eq!(
            where_sql(vec![eq("post_parent", None)], Dialect::MYSQL),
            "SELECT * FROM `site`.`wp_posts` WHERE (`post_parent` IS NULL) LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn structured_where_entries_are_joined_with_and_in_one_group() {
        assert_eq!(
            where_sql(
                vec![eq("ID", Some("7")), eq("post_parent", None)],
                Dialect::MYSQL
            ),
            "SELECT * FROM `site`.`wp_posts` WHERE (`ID` = '7' AND `post_parent` IS NULL) LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn structured_where_values_are_escaped_per_dialect() {
        let no_backslash = Dialect {
            backslash_escapes: false,
        };
        for (dialect, expected) in [
            (
                Dialect::MYSQL,
                r"SELECT * FROM `site`.`wp_posts` WHERE (`c` = 'a''b\\c') LIMIT 101 OFFSET 0",
            ),
            (
                no_backslash,
                r"SELECT * FROM `site`.`wp_posts` WHERE (`c` = 'a''b\c') LIMIT 101 OFFSET 0",
            ),
        ] {
            assert_eq!(
                where_sql(vec![eq("c", Some(r"a'b\c"))], dialect),
                expected,
                "{dialect:?}"
            );
        }
    }

    #[test]
    fn equals_sql_joins_columns_and_renders_null_as_is_null() {
        for (columns, expected) in [
            (vec![], ""),
            (vec![eq("ID", Some("7"))], "`ID` = '7'"),
            (vec![eq("post_parent", None)], "`post_parent` IS NULL"),
            (
                vec![
                    eq("meta_key", Some("_k")),
                    eq("meta_value", None),
                    eq("we`ird", Some("")),
                ],
                "`meta_key` = '_k' AND `meta_value` IS NULL AND `we``ird` = ''",
            ),
        ] {
            assert_eq!(equals_sql(Dialect::MYSQL, &columns), expected, "{expected}");
        }
    }

    #[test]
    fn select_column_sql_reads_one_column_of_the_first_matching_row() {
        let row = [eq("meta_id", Some("7")), eq("site", None)];

        assert_eq!(
            select_column_sql(Dialect::MYSQL, "wp", "wp_postmeta", "meta_key", &row),
            "SELECT `meta_key` FROM `wp`.`wp_postmeta` WHERE `meta_id` = '7' AND `site` IS NULL LIMIT 1"
        );
    }

    #[test]
    fn equals_sql_escapes_values_per_dialect() {
        let no_backslash = Dialect {
            backslash_escapes: false,
        };
        let columns = [eq("c", Some(r"a'b\c"))];

        assert_eq!(equals_sql(Dialect::MYSQL, &columns), r"`c` = 'a''b\\c'");
        assert_eq!(equals_sql(no_backslash, &columns), r"`c` = 'a''b\c'");
    }

    #[test]
    fn structured_where_comes_before_the_free_text_filter() {
        let sql = browse_sql(
            &BrowseRequest {
                r#where: vec![eq("ID", Some("7"))],
                filter: Some("post_type = 'page' # note".into()),
                ..req()
            },
            Dialect::MYSQL,
        )
        .unwrap()
        .0;
        assert_eq!(
            sql,
            "SELECT * FROM `site`.`wp_posts` WHERE (`ID` = '7') AND (post_type = 'page' # note\n) LIMIT 101 OFFSET 0"
        );
    }

    #[test]
    fn columns_filter_carries_quoted_sql_and_a_raw_label() {
        let applied = columns_filter(
            &BrowseRequest {
                r#where: vec![eq("ID", Some("007")), eq("post_parent", None)],
                ..req()
            },
            Dialect::MYSQL,
        )
        .unwrap();
        assert_eq!(applied.kind, FilterKind::Columns);
        assert_eq!(applied.sql, "`ID` = '007' AND `post_parent` IS NULL");
        assert_eq!(applied.label, "ID = 007 AND post_parent IS NULL");
    }

    #[test]
    fn columns_filter_is_none_without_where() {
        assert!(columns_filter(&req(), Dialect::MYSQL).is_none());
    }

    #[test]
    fn select_all_sql_puts_each_clause_on_its_own_line() {
        assert_eq!(
            select_all_sql(Dialect::MYSQL, "db", "t", 100),
            "SELECT *\nFROM `db`.`t`\nLIMIT 100;"
        );
        assert_eq!(
            select_all_sql(Dialect::MYSQL, "d`b", "t`x", 5),
            "SELECT *\nFROM `d``b`.`t``x`\nLIMIT 5;"
        );
    }

    #[test]
    fn browse_request_where_defaults_to_empty_and_reads_null_values() {
        let base = r#""database":"d","table":"t","limit":10"#;
        let without: BrowseRequest = serde_json::from_str(&format!("{{{base}}}")).unwrap();
        assert!(without.r#where.is_empty());
        let with: BrowseRequest = serde_json::from_str(&format!(
            r#"{{{base},"where":[{{"column":"ID","value":null}}]}}"#
        ))
        .unwrap();
        assert_eq!(with.r#where, vec![eq("ID", None)]);
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
