//! SQL fragments shared by the WordPress screens. Pure string builders, no I/O.

use super::context::WpContext;
use crate::db::Dialect;

/// Order statuses that count as paid in customer and order totals.
pub const PAID_STATUSES: &[&str] = &["wc-completed", "wc-processing"];

/// `options.autoload` values that load on every request: `yes` before WordPress 6.6,
/// `on`/`auto`/`auto-on` since.
pub const AUTOLOAD_ON: &[&str] = &["yes", "on", "auto", "auto-on"];

/// `col IN ('wc-completed', 'wc-processing')`.
pub fn paid_statuses_sql(col: &str) -> String {
    format!("{col} IN ({})", quoted_list(PAID_STATUSES))
}

/// `col IN ('yes', 'on', 'auto', 'auto-on')`.
pub fn autoload_on_sql(col: &str) -> String {
    format!("{col} IN ({})", quoted_list(AUTOLOAD_ON))
}

fn quoted_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|v| format!("'{v}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The quoted, prefixed WordPress table `name` in `db`.
pub fn table_ref(dialect: Dialect, db: &str, ctx: &WpContext, name: &str) -> String {
    dialect.table_ref(db, &ctx.table(name))
}

/// `col = n` when `term` is an unsigned integer: the ID match that leads a search group.
fn id_eq(col: &str, term: &str) -> Option<String> {
    term.parse::<u64>().ok().map(|n| format!("{col} = {n}"))
}

/// `col = n OR ` when `term` is an unsigned integer, otherwise empty; for search groups
/// written by hand.
pub fn id_match(col: &str, term: &str) -> String {
    id_eq(col, term).map(|eq| eq + " OR ").unwrap_or_default()
}

/// A `WHERE` clause: conditions joined by `AND`, then one search group of `OR`ed
/// alternatives (an ID match first, when there is one).
pub struct Filter {
    dialect: Dialect,
    all: Vec<String>,
    id: Option<String>,
    any: Vec<String>,
}

impl Filter {
    pub fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            all: Vec::new(),
            id: None,
            any: Vec::new(),
        }
    }

    pub fn and(mut self, cond: impl Into<String>) -> Self {
        self.all.push(cond.into());
        self
    }

    /// Adds `col LIKE '%term%'` per column to the search group.
    pub fn search_any(mut self, cols: &[&str], term: &str) -> Self {
        let pattern = self.dialect.like_contains(term);
        self.any
            .extend(cols.iter().map(|col| format!("{col} LIKE {pattern}")));
        self
    }

    /// Leads the search group with `col = n` when `term` is an unsigned integer.
    pub fn id_or(mut self, col: &str, term: &str) -> Self {
        self.id = id_eq(col, term);
        self
    }

    /// `""` without conditions, otherwise ` WHERE a AND (b OR c)`.
    pub fn sql(&self) -> String {
        let mut conds = self.all.clone();
        let group: Vec<&str> = self
            .id
            .iter()
            .chain(&self.any)
            .map(String::as_str)
            .collect();
        if !group.is_empty() {
            conds.push(format!("({})", group.join(" OR ")));
        }
        if conds.is_empty() {
            return String::new();
        }
        format!(" WHERE {}", conds.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(prefix: &str) -> WpContext {
        WpContext {
            version: None,
            prefix: prefix.into(),
            sites: vec![],
            wc: None,
            site: Default::default(),
            tables: Default::default(),
        }
    }

    #[test]
    fn empty_filter_has_no_where_clause() {
        assert_eq!(Filter::new(Dialect::MYSQL).sql(), "");
    }

    #[test]
    fn and_conditions_are_joined_with_and() {
        let sql = Filter::new(Dialect::MYSQL)
            .and("p.post_type = 'shop_order'")
            .and("p.post_status = 'publish'")
            .sql();
        assert_eq!(
            sql,
            " WHERE p.post_type = 'shop_order' AND p.post_status = 'publish'"
        );
    }

    #[test]
    fn search_group_follows_all_and_conditions_with_numeric_id_first() {
        let sql = Filter::new(Dialect::MYSQL)
            .search_any(&["o.billing_email", "o.note"], "42")
            .and("o.status = 'wc-completed'")
            .id_or("o.id", "42")
            .and("o.type = 'shop_order'")
            .sql();
        assert_eq!(
            sql,
            " WHERE o.status = 'wc-completed' AND o.type = 'shop_order' AND \
             (o.id = 42 OR o.billing_email LIKE '%42%' OR o.note LIKE '%42%')"
        );
    }

    #[test]
    fn non_numeric_term_adds_no_id_clause() {
        for term in ["abc", "-5", "1.5", "", " 7", "99999999999999999999"] {
            let sql = Filter::new(Dialect::MYSQL)
                .id_or("o.id", term)
                .search_any(&["o.note"], term)
                .sql();
            assert_eq!(
                sql,
                format!(
                    " WHERE (o.note LIKE {})",
                    Dialect::MYSQL.like_contains(term)
                ),
                "{term:?}"
            );
        }
    }

    #[test]
    fn id_or_without_search_any_still_forms_a_group() {
        let sql = Filter::new(Dialect::MYSQL).id_or("o.id", "7").sql();
        assert_eq!(sql, " WHERE (o.id = 7)");
    }

    #[test]
    fn non_numeric_id_or_alone_leaves_the_filter_empty() {
        assert_eq!(Filter::new(Dialect::MYSQL).id_or("o.id", "abc").sql(), "");
    }

    #[test]
    fn search_escapes_like_wildcards_for_the_dialect() {
        let no_backslash = Dialect::from_sql_mode("NO_BACKSLASH_ESCAPES");
        assert_eq!(
            Filter::new(Dialect::MYSQL)
                .search_any(&["t.c"], "a_b")
                .sql(),
            r" WHERE (t.c LIKE '%a\\_b%')"
        );
        assert_eq!(
            Filter::new(no_backslash).search_any(&["t.c"], "a_b").sql(),
            r" WHERE (t.c LIKE '%a\_b%' ESCAPE '\')"
        );
    }

    #[test]
    fn paid_statuses_sql_lists_completed_and_processing() {
        assert_eq!(
            paid_statuses_sql("o.status"),
            "o.status IN ('wc-completed', 'wc-processing')"
        );
    }

    #[test]
    fn autoload_on_sql_lists_every_loading_value() {
        assert_eq!(
            autoload_on_sql("autoload"),
            "autoload IN ('yes', 'on', 'auto', 'auto-on')"
        );
    }

    #[test]
    fn id_match_leads_with_an_or_only_for_unsigned_integers() {
        for (term, expected) in [
            ("42", "p.ID = 42 OR "),
            ("0", "p.ID = 0 OR "),
            ("007", "p.ID = 7 OR "),
            ("abc", ""),
            ("-1", ""),
            ("4.2", ""),
            ("", ""),
            ("12 ", ""),
        ] {
            assert_eq!(id_match("p.ID", term), expected, "{term:?}");
        }
    }

    #[test]
    fn table_ref_quotes_database_and_prefixed_table() {
        assert_eq!(
            table_ref(Dialect::MYSQL, "shop", &ctx("wp_"), "options"),
            "`shop`.`wp_options`"
        );
        assert_eq!(
            table_ref(Dialect::MYSQL, "my`db", &ctx("wp_2_"), "posts"),
            "`my``db`.`wp_2_posts`"
        );
    }
}
