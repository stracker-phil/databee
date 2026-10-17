//! `Ctx`: the session, the schema, the effective rules and their lens of one database, and the
//! bounded reads built on them.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;

use super::Rows;
use crate::address::{Address, Target};
use crate::db::{ColumnEquals, Dialect, ResultSet};
use crate::error::AppResult;
use crate::lens::{self, Lens};
use crate::rules::effective::{self, EffectiveRules, RuleSources};
use crate::rules::lens::build;
use crate::rules::placeholder::qualify_prefixes;
use crate::schema::DatabaseSchema;
use crate::session::Session;
use crate::value::{self, RowKey};

pub struct Ctx<'a> {
    pub session: &'a Session,
    pub database: String,
    pub schema: Arc<DatabaseSchema>,
    pub rules: Arc<EffectiveRules>,
    pub lens: Arc<dyn Lens>,
    pub dialect: Dialect,
    pub timeout: Duration,
}

impl<'a> Ctx<'a> {
    pub async fn load(
        session: &'a Session,
        database: &str,
        sources: RuleSources<'_>,
        timeout: Duration,
    ) -> AppResult<Self> {
        let (schema, rules) = effective::load(session, database, sources).await?;
        let lens = Arc::from(build(
            schema.clone(),
            &session.slug(),
            database,
            Some(rules.clone()),
        ));
        Ok(Self {
            session,
            database: database.to_string(),
            schema,
            rules,
            lens,
            dialect: session.db.dialect(),
            timeout,
        })
    }

    pub fn table_ref(&self, table: &str) -> String {
        self.dialect.table_ref(&self.database, table)
    }

    pub fn ident(&self, name: &str) -> String {
        self.dialect.quote_ident(name)
    }

    /// `alias.name`, or `name` alone without an alias.
    pub fn column(&self, alias: Option<&str>, name: &str) -> String {
        match alias {
            Some(alias) => format!("{alias}.{}", self.ident(name)),
            None => self.ident(name),
        }
    }

    /// Preset SQL with its prefixes filled in and its tables qualified with this database.
    pub fn preset_sql(&self, sql: &str) -> String {
        let qualifier = format!("{}.", self.ident(&self.database));
        self.rules
            .applied
            .expand(&qualify_prefixes(sql, &qualifier))
    }

    pub fn literal(&self, value: &str) -> String {
        self.dialect.sql_string(value)
    }

    pub fn primary_key(&self, table: &str) -> &[String] {
        self.schema
            .keys(table)
            .map_or(&[], |k| k.primary_key.as_slice())
    }

    pub fn single_key(&self, table: &str) -> Option<&str> {
        self.schema.single_key(table)
    }

    pub fn address(&self, target: Target) -> Address {
        Address::new(self.session.slug(), self.database.clone(), target)
    }

    pub fn row_address(&self, table: &str, key: &str) -> Address {
        self.address(Target::Row {
            table: table.to_string(),
            key: key.to_string(),
        })
    }

    /// A single-part key as a row address, a composite key as a filter on every part.
    pub fn key_address(&self, table: &str, key: &RowKey) -> Address {
        if let [only] = key.0.as_slice() {
            return self.row_address(table, &only.value);
        }
        let filter = key
            .0
            .iter()
            .map(|part| ColumnEquals {
                column: part.column.clone(),
                value: Some(part.value.clone()),
            })
            .collect();
        self.address(Target::Table {
            table: table.to_string(),
            filter,
        })
    }

    pub async fn fetch(&self, sql: &str) -> AppResult<ResultSet> {
        self.session.db.fetch(sql, self.timeout).await
    }

    /// Rows of `table` read with `sql` (every column of the table, under its own name).
    pub async fn rows(&self, table: &str, sql: &str) -> AppResult<Rows> {
        let result = self.fetch(sql).await?;
        Ok(self.annotate(table, result))
    }

    pub fn annotate(&self, table: &str, mut result: ResultSet) -> Rows {
        lens::annotate_table(self.lens.as_ref(), table, &mut result);
        let keys = value::row_keys(&result, self.primary_key(table));
        Rows {
            table: table.to_string(),
            result,
            keys,
        }
    }

    /// At most `cap` rows of `table` (as `alias`) where `filter` holds, in `order`, and how many
    /// hold in all: a count runs only when there are more.
    pub async fn capped(
        &self,
        table: &str,
        alias: Option<&str>,
        filter: &str,
        order: &str,
        cap: usize,
    ) -> AppResult<Capped> {
        let from = self.from(table, alias);
        let sql = format!(
            "SELECT * FROM {from} WHERE {filter} ORDER BY {order} LIMIT {}",
            cap + 1
        );
        let result = self.fetch(&sql).await?;
        self.finish_capped(table, &from, filter, result, cap).await
    }

    /// `capped` for several filters on one table, in key order: one query for the rows of all of
    /// them, then a count for each that has more.
    pub async fn capped_each(
        &self,
        table: &str,
        alias: &str,
        filters: &[String],
        cap: usize,
    ) -> Vec<AppResult<Capped>> {
        let order = KeyOrder::primary(self, table);
        let results = match self
            .rows_per_filter(table, alias, filters, &order, cap + 1)
            .await
        {
            Ok(results) => results,
            Err(e) => return filters.iter().map(|_| Err(e.clone())).collect(),
        };
        let from = self.from(table, Some(alias));
        let finished = results
            .into_iter()
            .zip(filters)
            .map(|(result, filter)| self.finish_capped(table, &from, filter, result, cap));
        join_all(finished).await
    }

    async fn finish_capped(
        &self,
        table: &str,
        from: &str,
        filter: &str,
        mut result: ResultSet,
        cap: usize,
    ) -> AppResult<Capped> {
        let capped = result.rows.len() > cap;
        result.rows.truncate(cap);
        let total = if capped {
            self.count(&format!("SELECT COUNT(*) FROM {from} WHERE {filter}"))
                .await?
        } else {
            result.rows.len() as u64
        };
        Ok(Capped {
            rows: self.annotate(table, result),
            total,
            capped,
        })
    }

    /// At most `limit` rows of `table` (as `alias`) for each of `filters`, one result per filter.
    /// Several filters run as one `UNION ALL`, so the statement count is one per table.
    pub async fn rows_per_filter(
        &self,
        table: &str,
        alias: &str,
        filters: &[String],
        order: &KeyOrder<'_>,
        limit: usize,
    ) -> AppResult<Vec<ResultSet>> {
        let from = self.from(table, Some(alias));
        let inner = order.sql(self, Some(alias));
        if let [filter] = filters {
            let sql = format!("SELECT * FROM {from} WHERE {filter} ORDER BY {inner} LIMIT {limit}");
            return Ok(vec![self.fetch(&sql).await?]);
        }
        let part = self.ident(PART_COLUMN);
        let branches: Vec<String> = filters
            .iter()
            .enumerate()
            .map(|(i, filter)| {
                format!(
                    "(SELECT {alias}.*, {i} AS {part} FROM {from} WHERE {filter} \
                     ORDER BY {inner} LIMIT {limit})"
                )
            })
            .collect();
        let outer = order.sql(self, None);
        let outer = if order.columns.is_empty() {
            part
        } else {
            format!("{part}, {outer}")
        };
        let sql = format!("{} ORDER BY {outer}", branches.join(" UNION ALL "));
        Ok(split_parts(self.fetch(&sql).await?, filters.len()))
    }

    fn from(&self, table: &str, alias: Option<&str>) -> String {
        match alias {
            Some(alias) => format!("{} AS {alias}", self.table_ref(table)),
            None => self.table_ref(table),
        }
    }

    pub async fn count(&self, sql: &str) -> AppResult<u64> {
        let result = self.fetch(sql).await?;
        let count = result
            .rows
            .first()
            .and_then(|row| row.first()?.as_text()?.parse().ok());
        Ok(count.unwrap_or(0))
    }
}

/// The column a `UNION ALL` of `rows_per_filter` tells its filters apart by.
const PART_COLUMN: &str = "__databee_part";

/// Rows sorted by key columns; unsorted without any.
pub struct KeyOrder<'k> {
    pub columns: &'k [String],
    pub descending: bool,
}

impl<'k> KeyOrder<'k> {
    /// Ascending by the primary key of `table`.
    pub fn primary(ctx: &'k Ctx<'_>, table: &str) -> Self {
        Self {
            columns: ctx.primary_key(table),
            descending: false,
        }
    }

    /// The columns qualified with `alias`, for `ORDER BY`; `1` without any.
    pub fn sql(&self, ctx: &Ctx<'_>, alias: Option<&str>) -> String {
        if self.columns.is_empty() {
            return "1".into();
        }
        let direction = if self.descending { " DESC" } else { "" };
        self.columns
            .iter()
            .map(|c| format!("{}{direction}", ctx.column(alias, c)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The rows of a `UNION ALL` by the part number in their last column, without that column.
fn split_parts(mut result: ResultSet, parts: usize) -> Vec<ResultSet> {
    let rows = std::mem::take(&mut result.rows);
    result.columns.pop();
    let mut out = vec![result; parts];
    for mut row in rows {
        let part = row.pop().and_then(|c| c.as_text()?.parse::<usize>().ok());
        if let Some(target) = part.and_then(|i| out.get_mut(i)) {
            target.rows.push(row);
        }
    }
    for part in &mut out {
        part.total_rows = part.rows.len() as u64;
    }
    out
}

pub struct Capped {
    pub rows: Rows,
    pub total: u64,
    pub capped: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Cell;
    use crate::error::AppError;
    use crate::query::testing::{Rig, count, num, result, shop, text};

    fn log_rows(n: usize) -> ResultSet {
        let rows = (1..=n)
            .map(|i| vec![num(&i.to_string()), num("1"), text("")])
            .collect();
        result(&["log_id", "order_id", "msg"], rows)
    }

    #[tokio::test]
    async fn capped_below_the_cap_reads_once_and_totals_the_rows() {
        let rig = Rig::new(shop().on("FROM `wp`.`f_log`", log_rows(2)), "");
        let ctx = rig.ctx().await;

        let capped = ctx
            .capped("f_log", None, "`order_id` = '1'", "`log_id`", 3)
            .await
            .unwrap();

        assert_eq!(
            rig.db.queries(),
            ["SELECT * FROM `wp`.`f_log` WHERE `order_id` = '1' ORDER BY `log_id` LIMIT 4"]
        );
        assert_eq!(
            (capped.rows.len(), capped.total, capped.capped),
            (2, 2, false)
        );
    }

    #[tokio::test]
    async fn capped_beyond_the_cap_keeps_the_cap_and_counts_all_matches() {
        let rig = Rig::new(
            shop()
                .on("SELECT COUNT(*)", count(9))
                .on("FROM `wp`.`f_log`", log_rows(4)),
            "",
        );
        let ctx = rig.ctx().await;

        let capped = ctx
            .capped("f_log", None, "`order_id` = '1'", "`log_id`", 3)
            .await
            .unwrap();

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT * FROM `wp`.`f_log` WHERE `order_id` = '1' ORDER BY `log_id` LIMIT 4",
                "SELECT COUNT(*) FROM `wp`.`f_log` WHERE `order_id` = '1'",
            ]
        );
        assert_eq!(
            (capped.rows.len(), capped.total, capped.capped),
            (3, 9, true)
        );
    }

    #[tokio::test]
    async fn capped_aliases_the_table_in_both_queries() {
        let rig = Rig::new(
            shop()
                .on("SELECT COUNT(*)", count(9))
                .on("FROM `wp`.`f_log`", log_rows(4)),
            "",
        );
        let ctx = rig.ctx().await;

        ctx.capped("f_log", Some("s"), "s.`order_id` = '1'", "s.`log_id`", 3)
            .await
            .unwrap();

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT * FROM `wp`.`f_log` AS s WHERE s.`order_id` = '1' ORDER BY s.`log_id` LIMIT 4",
                "SELECT COUNT(*) FROM `wp`.`f_log` AS s WHERE s.`order_id` = '1'",
            ]
        );
    }

    fn log_parts(parts: &[usize]) -> ResultSet {
        let rows = parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                vec![
                    num(&(i + 1).to_string()),
                    num("1"),
                    text(""),
                    num(&part.to_string()),
                ]
            })
            .collect();
        result(&["log_id", "order_id", "msg", "__databee_part"], rows)
    }

    fn order_filters() -> Vec<String> {
        vec!["s.`order_id` = '1'".into(), "s.`order_id` = '2'".into()]
    }

    const UNION: &str = "(SELECT s.*, 0 AS `__databee_part` FROM `wp`.`f_log` AS s WHERE s.`order_id` = '1' ORDER BY s.`log_id` LIMIT 4) \
UNION ALL (SELECT s.*, 1 AS `__databee_part` FROM `wp`.`f_log` AS s WHERE s.`order_id` = '2' ORDER BY s.`log_id` LIMIT 4) \
ORDER BY `__databee_part`, `log_id`";

    #[tokio::test]
    async fn capped_each_reads_all_filters_of_a_table_in_one_statement() {
        let rig = Rig::new(shop().on("UNION ALL", log_parts(&[0, 0, 1])), "");
        let ctx = rig.ctx().await;

        let each = ctx.capped_each("f_log", "s", &order_filters(), 3).await;

        assert_eq!(rig.db.queries(), [UNION]);
        let shape: Vec<_> = each
            .iter()
            .map(|c| {
                let c = c.as_ref().ok().unwrap();
                (c.rows.names(), c.rows.len(), c.total, c.capped)
            })
            .collect();
        let columns = vec!["log_id".to_string(), "order_id".into(), "msg".into()];
        assert_eq!(
            shape,
            [(columns.clone(), 2, 2, false), (columns, 1, 1, false)]
        );
    }

    #[tokio::test]
    async fn capped_each_counts_only_the_filter_beyond_the_cap() {
        let rig = Rig::new(
            shop()
                .on("SELECT COUNT(*)", count(9))
                .on("UNION ALL", log_parts(&[0, 0, 0, 0, 1])),
            "",
        );
        let ctx = rig.ctx().await;

        let each = ctx.capped_each("f_log", "s", &order_filters(), 3).await;

        assert_eq!(
            rig.db.queries(),
            [
                UNION,
                "SELECT COUNT(*) FROM `wp`.`f_log` AS s WHERE s.`order_id` = '1'"
            ]
        );
        let shape: Vec<_> = each
            .iter()
            .map(|c| {
                let c = c.as_ref().ok().unwrap();
                (c.rows.len(), c.total, c.capped)
            })
            .collect();
        assert_eq!(shape, [(3, 9, true), (1, 1, false)]);
    }

    #[tokio::test]
    async fn capped_each_gives_every_filter_the_error_of_a_failing_statement() {
        let rig = Rig::new(
            shop().failing("UNION ALL", AppError::InvalidInput("boom".into())),
            "",
        );
        let ctx = rig.ctx().await;

        let each = ctx.capped_each("f_log", "s", &order_filters(), 3).await;

        assert_eq!(each.len(), 2);
        for c in &each {
            assert!(
                matches!(c.as_ref().err(), Some(AppError::InvalidInput(m)) if m == "boom"),
                "{:?}",
                c.as_ref().err()
            );
        }
    }

    #[tokio::test]
    async fn a_count_without_a_number_is_zero() {
        let rig = Rig::new(
            shop()
                .on("empty", result(&["c"], Vec::new()))
                .on("null", result(&["c"], vec![vec![Cell::Null]])),
            "",
        );
        let ctx = rig.ctx().await;

        assert_eq!(ctx.count("SELECT 'empty'").await.unwrap(), 0);
        assert_eq!(ctx.count("SELECT 'null'").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn key_order_lists_the_key_with_the_alias_and_is_one_without_a_key() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;

        let order = |table| KeyOrder::primary(&ctx, table);

        assert_eq!(order("f_log").sql(&ctx, None), "`log_id`");
        assert_eq!(order("f_log").sql(&ctx, Some("s")), "s.`log_id`");
        assert_eq!(order("f_unknown").sql(&ctx, None), "1");
    }
}
