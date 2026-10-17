//! What every part of a document reads with: the session, the schema, the effective rules and
//! their lens, and annotated rows with provenance.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;

use crate::address::{Address, Target};
use crate::db::{Cell, Dialect, ResultSet};
use crate::error::AppResult;
use crate::lens::{self, Lens};
use crate::rules::effective::{self, EffectiveRules, RuleSources};
use crate::rules::placeholder::qualify_prefixes;
use crate::schema::DatabaseSchema;
use crate::session::Session;
use crate::value::{self, ColumnKind, Provenance, ResultValues, RowKey, Value, ValueKind};

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
        let lens = Arc::from(lens::build(
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
        let order = KeyOrder {
            columns: self.primary_key(table),
            descending: false,
        };
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
        let inner = order.sql(|c| self.column(Some(alias), c));
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
        let outer = order.sql(|c| self.ident(c));
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

    /// The key columns of `table`, qualified with `alias`, for `ORDER BY`; `1` without a key.
    pub fn key_order(&self, table: &str, alias: Option<&str>) -> String {
        let key = self.primary_key(table);
        if key.is_empty() {
            return "1".into();
        }
        key.iter()
            .map(|k| self.column(alias, k))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The column a `UNION ALL` of `rows_per_filter` tells its filters apart by.
const PART_COLUMN: &str = "__databee_part";

/// Rows sorted by key columns; unsorted without any.
pub struct KeyOrder<'k> {
    pub columns: &'k [String],
    pub descending: bool,
}

impl KeyOrder<'_> {
    fn sql(&self, column: impl Fn(&str) -> String) -> String {
        if self.columns.is_empty() {
            return "1".into();
        }
        let direction = if self.descending { " DESC" } else { "" };
        self.columns
            .iter()
            .map(|c| format!("{}{direction}", column(c)))
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

/// An annotated result of one table, with each row's key.
pub struct Rows {
    pub table: String,
    pub result: ResultSet,
    /// Empty when the table has no primary key.
    pub keys: Vec<RowKey>,
}

/// A currency candidate for a row by name: a column of the owner row or a meta key of the
/// document.
pub type CurrencyLookup<'a> = &'a dyn Fn(usize, &str) -> Option<Value>;

impl Rows {
    pub fn len(&self) -> usize {
        self.result.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.result.rows.is_empty()
    }

    pub fn column(&self, name: &str) -> Option<usize> {
        self.result
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn names(&self) -> Vec<String> {
        self.result.columns.iter().map(|c| c.name.clone()).collect()
    }

    pub fn cell(&self, row: usize, name: &str) -> Option<&Cell> {
        self.result.rows.get(row)?.get(self.column(name)?)
    }

    /// `None` when the table has no such column, `Some(None)` for NULL or binary data.
    pub fn text(&self, row: usize, name: &str) -> Option<Option<&str>> {
        self.cell(row, name).map(Cell::as_text)
    }

    pub fn key(&self, row: usize) -> Option<RowKey> {
        self.keys.get(row).cloned()
    }

    /// Every value of every row, sourced from this table. Money without a currency in its own
    /// row takes the first of its names that `currency` finds.
    pub fn values(&self, currency: CurrencyLookup<'_>) -> Vec<Vec<Value>> {
        let values = ResultValues::new(&self.result);
        (0..self.len())
            .map(|row| {
                (0..self.result.columns.len())
                    .map(|col| {
                        let mut value = values.value(row, col, self.provenance(row, col));
                        self.add_currency(&mut value, row, col, currency);
                        value
                    })
                    .collect()
            })
            .collect()
    }

    fn provenance(&self, row: usize, col: usize) -> Option<Provenance> {
        Some(Provenance {
            table: self.table.clone(),
            column: self.result.columns[col].name.clone(),
            row: self.keys.get(row)?.clone(),
        })
    }

    fn add_currency(
        &self,
        value: &mut Value,
        row: usize,
        col: usize,
        currency: CurrencyLookup<'_>,
    ) {
        let ValueKind::Money { currency: found } = &mut value.kind else {
            return;
        };
        if found.is_some() {
            return;
        }
        let names = match self.kind(row, col) {
            Some(ColumnKind::Money { currency }) => currency,
            _ => return,
        };
        *found = names
            .iter()
            .find_map(|name| currency(row, name))
            .map(Box::new);
    }

    fn kind(&self, row: usize, col: usize) -> Option<&ColumnKind> {
        let cell = self
            .result
            .cell_kinds
            .iter()
            .find(|c| c.row == row && c.col == col);
        cell.map(|c| &c.kind).or_else(|| self.result.kinds.get(col))
    }
}

/// A currency value only counts with text in it.
pub fn non_empty(value: &Value) -> bool {
    value.raw.as_text().is_some_and(|text| !text.is_empty())
}

#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;

    use tempfile::{TempDir, tempdir};

    use super::Ctx;
    use crate::db::{Cell, ColumnInfo, ColumnMeta, FakeDatabase, ResultSet, TableStructure};
    use crate::document::own::Own;
    use crate::rules::ProjectsStore;
    use crate::rules::catalog::PresetCatalog;
    use crate::rules::effective::RuleSources;
    use crate::session::Session;
    use crate::test_support::spec;

    pub const TIMEOUT: Duration = Duration::from_secs(5);

    /// The preset every document test starts from: orders owning `ordermeta`, under the prefix `f_`.
    pub const BASE: &str = r#"
[preset]
name = "doc"
marker_tables = ["orders"]

[[meta]]
table = "ordermeta"
owner = "orders"
fk = "order_id"
key = "k"
value = "v"
"#;

    pub fn text(value: &str) -> Cell {
        Cell::Text(value.into())
    }

    pub fn num(value: &str) -> Cell {
        Cell::Number(value.into())
    }

    pub fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: columns
                .iter()
                .map(|name| ColumnMeta {
                    name: name.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            returns_rows: true,
            ..ResultSet::default()
        }
    }

    pub fn count(total: u64) -> ResultSet {
        result(&["COUNT(*)"], vec![vec![num(&total.to_string())]])
    }

    fn structure(columns: &[&str], primary_key: &str) -> TableStructure {
        let column = |name: &&str| ColumnInfo {
            name: name.to_string(),
            column_type: "text".into(),
            nullable: true,
            default: None,
            key: String::new(),
            extra: String::new(),
            comment: String::new(),
            auto_increment: false,
            generated: false,
        };
        TableStructure {
            columns: columns.iter().map(column).collect(),
            indexes: Vec::new(),
            primary_key: vec![primary_key.into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    /// The database `wp` with `f_orders`, `f_ordermeta`, `f_log` and `f_people`.
    pub fn shop() -> FakeDatabase {
        FakeDatabase::new()
            .with_database("wp")
            .with_table(
                "wp",
                "f_orders",
                structure(&["id", "status", "total", "currency", "created_gmt"], "id"),
            )
            .with_table(
                "wp",
                "f_ordermeta",
                structure(&["meta_id", "order_id", "k", "v"], "meta_id"),
            )
            .with_table(
                "wp",
                "f_log",
                structure(&["log_id", "order_id", "at_gmt", "msg"], "log_id"),
            )
            .with_table(
                "wp",
                "f_people",
                structure(&["person_id", "name"], "person_id"),
            )
    }

    /// A session over a fake database, with `BASE` plus `rules` as the one preset.
    pub struct Rig {
        _dir: TempDir,
        pub db: Arc<FakeDatabase>,
        pub session: Session,
        catalog: PresetCatalog,
        projects: ProjectsStore,
    }

    impl Rig {
        pub fn new(db: FakeDatabase, rules: &str) -> Self {
            let dir = tempdir().unwrap();
            let presets = dir.path().join("presets");
            fs::create_dir_all(&presets).unwrap();
            fs::write(presets.join("doc.toml"), format!("{BASE}\n{rules}")).unwrap();
            let db = Arc::new(db);
            let session = Session::new(
                "s".into(),
                spec("127.0.0.1", 3306, "wp"),
                "local".into(),
                Box::new(db.clone()),
            );
            let catalog = PresetCatalog::with_built_in(presets, &[]);
            let projects = ProjectsStore::open(dir.path());
            Self {
                _dir: dir,
                db,
                session,
                catalog,
                projects,
            }
        }

        pub fn sources(&self) -> RuleSources<'_> {
            RuleSources {
                presets: &self.catalog,
                projects: &self.projects,
            }
        }

        pub async fn ctx(&self) -> Ctx<'_> {
            Ctx::load(&self.session, "wp", self.sources(), TIMEOUT)
                .await
                .unwrap()
        }
    }

    pub fn order(ctx: &Ctx<'_>, id: &str, status: Cell, currency: Cell) -> Own {
        let row = vec![
            num(id),
            status,
            text("10.00"),
            currency,
            Cell::Temporal("2023-11-14 22:13:20".into()),
        ];
        let columns = ["id", "status", "total", "currency", "created_gmt"];
        Own {
            rows: ctx.annotate("f_orders", result(&columns, vec![row])),
            entity: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Rig, count, num, result, shop, text};
    use super::*;
    use crate::error::AppError;
    use crate::value::{ColumnKind, Provenance, RowKey};

    fn log_rows(n: usize) -> ResultSet {
        let rows = (1..=n)
            .map(|i| vec![num(&i.to_string()), num("1"), text("")])
            .collect();
        result(&["log_id", "order_id", "msg"], rows)
    }

    fn eur() -> Value {
        Value {
            raw: text("EUR"),
            kind: ValueKind::Text,
            source: None,
            style: None,
            secondary: None,
        }
    }

    fn money_rows(amount: Cell, currency: Cell) -> Rows {
        let mut result = result(&["amount", "currency"], vec![vec![amount, currency]]);
        result.kinds = vec![
            ColumnKind::Money {
                currency: vec!["currency".into(), "_currency".into()],
            },
            ColumnKind::Text,
        ];
        Rows {
            table: "t".into(),
            result,
            keys: Vec::new(),
        }
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

        assert_eq!(ctx.key_order("f_log", None), "`log_id`");
        assert_eq!(ctx.key_order("f_log", Some("s")), "s.`log_id`");
        assert_eq!(ctx.key_order("f_unknown", None), "1");
    }

    #[test]
    fn rows_tell_a_missing_column_from_a_null_cell() {
        let rows = money_rows(Cell::Null, text("EUR"));

        assert_eq!(rows.text(0, "amount"), Some(None));
        assert_eq!(rows.text(0, "CURRENCY"), Some(Some("EUR")));
        assert_eq!(rows.text(0, "other"), None);
        assert_eq!(rows.names(), ["amount", "currency"]);
    }

    #[test]
    fn money_without_a_currency_in_its_row_takes_the_first_name_the_lookup_finds() {
        let rows = money_rows(text("10.00"), text(""));
        let asked = std::cell::RefCell::new(Vec::new());
        let lookup = |row: usize, name: &str| {
            asked.borrow_mut().push((row, name.to_string()));
            (name == "_currency").then(eur)
        };

        let values = rows.values(&lookup);

        assert_eq!(
            values[0][0].kind,
            ValueKind::Money {
                currency: Some(Box::new(eur()))
            }
        );
        assert_eq!(
            asked.into_inner(),
            [(0, "currency".to_string()), (0, "_currency".to_string())]
        );
    }

    #[test]
    fn money_with_a_currency_in_its_row_never_asks_the_lookup() {
        let mut rows = money_rows(text("10.00"), text("USD"));
        rows.keys = vec![RowKey(Vec::new())];
        let lookup = |_: usize, _: &str| -> Option<Value> { panic!("the row has a currency") };

        let values = rows.values(&lookup);

        let ValueKind::Money { currency } = &values[0][0].kind else {
            panic!("{:?}", values[0][0].kind);
        };
        let currency = currency.as_ref().unwrap();
        assert_eq!(currency.raw, text("USD"));
        assert_eq!(
            currency.source,
            Some(Provenance {
                table: "t".into(),
                column: "currency".into(),
                row: RowKey(Vec::new()),
            })
        );
    }

    #[test]
    fn money_whose_currency_nobody_knows_has_none() {
        let rows = money_rows(text("10.00"), Cell::Null);

        let values = rows.values(&|_, _| None);

        assert_eq!(values[0][0].kind, ValueKind::Money { currency: None });
        assert_eq!(values[0][1].kind, ValueKind::Null);
    }

    #[test]
    fn a_value_that_is_not_money_ignores_the_lookup() {
        let mut rows = money_rows(text("10.00"), text(""));
        rows.result.kinds[0] = ColumnKind::Text;

        let values = rows.values(&|_, _| Some(eur()));

        assert_eq!(values[0][0].kind, ValueKind::Text);
    }

    #[test]
    fn only_text_counts_as_a_currency() {
        let mut empty = eur();
        empty.raw = text("");
        let mut null = eur();
        null.raw = Cell::Null;

        assert!(non_empty(&eur()));
        assert!(!non_empty(&empty));
        assert!(!non_empty(&null));
    }
}
