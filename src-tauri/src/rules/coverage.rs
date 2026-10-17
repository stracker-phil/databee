//! Reference coverage: of the source rows a reference covers, how many find a target row, how many
//! hold "no reference" (NULL or `0`) and which values point nowhere (docs/presets.md, "Coverage").
//!
//! A reference is named by the `ResolvedReference` that `table_references` returned (from, to and
//! `by`): it works for declared, project, preset and inferred references alike, and is checked
//! against the current lens, so every table and column in the SQL is one the schema has.

use std::time::Duration;

use serde::Serialize;
use ts_rs::TS;

use super::condition::Condition;
use super::effective::{self, EffectiveRules, RuleSources};
use super::types::Match;
use crate::db::{Dialect, ResultSet, escape_like};
use crate::error::{AppError, AppResult};
use crate::lens::{self, ColumnsRef, ResolvedReference};
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;
use crate::value::ReferenceSource;

/// Distinct unresolved values the UI can show as examples.
const SAMPLE_LIMIT: usize = 10;

#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// Source rows the reference applies to (its `when` holds).
    pub total: u64,
    /// Rows with a target row.
    pub resolved: u64,
    /// Rows whose value is NULL or `0`: no reference, not an orphan.
    pub zero_or_null: u64,
    /// Up to `SAMPLE_LIMIT` distinct values without a target; composite values joined by `,`.
    pub sample_unresolved: Vec<String>,
    /// The count hit the query timeout: the counts are 0. When only the sample did, the counts
    /// stand and the sample is empty.
    pub timed_out: bool,
}

/// One reference in SQL terms: real tables, paired columns and the rule's conditions.
#[derive(Debug, Clone)]
struct CoverageQuery<'a> {
    pub database: &'a str,
    pub from: &'a ColumnsRef,
    pub to: &'a ColumnsRef,
    /// On the source row.
    pub when: &'a [Condition],
    /// On the target row.
    pub to_when: &'a [Condition],
    /// The key of a table whose column references itself (shared emails, external ids): a row
    /// counts as resolved only when another row matches, not by finding itself.
    pub self_key: &'a [String],
}

impl CoverageQuery<'_> {
    /// One pass over the source rows: the total, the "no reference" rows and the resolved rows.
    /// `EXISTS` instead of a `LEFT JOIN` count, so a target that matches several rows (any
    /// non-key column) counts its source row once.
    fn counts_sql(&self, dialect: Dialect) -> String {
        format!(
            "SELECT COUNT(*) AS total, \
             COALESCE(SUM({none}), 0) AS zero_or_null, \
             COALESCE(SUM(CASE WHEN {none} THEN 0 WHEN {exists} THEN 1 ELSE 0 END), 0) AS resolved \
             FROM {from} AS s{filter}",
            none = self.no_reference(dialect),
            exists = self.exists(dialect),
            from = dialect.table_ref(self.database, &self.from.table),
            filter = where_clause(&self.source_filter(dialect)),
        )
    }

    /// Distinct values of source rows that hold a reference but find no target.
    fn sample_sql(&self, dialect: Dialect) -> String {
        let values: Vec<String> = self
            .from
            .columns
            .iter()
            .map(|c| format!("CAST({} AS CHAR)", column(dialect, "s", c)))
            .collect();
        let mut filter = self.source_filter(dialect);
        filter.push(format!("NOT ({})", self.no_reference(dialect)));
        filter.push(format!("NOT {}", self.exists(dialect)));
        format!(
            "SELECT DISTINCT CONCAT_WS(',', {values}) AS value FROM {from} AS s{filter} LIMIT {SAMPLE_LIMIT}",
            values = values.join(", "),
            from = dialect.table_ref(self.database, &self.from.table),
            filter = where_clause(&filter),
        )
    }

    fn source_filter(&self, dialect: Dialect) -> Vec<String> {
        conditions(dialect, "s", self.when)
    }

    /// NULL in any column (a composite key with a NULL part is not checked, as in SQL), or a
    /// single column whose text is `0` (how WordPress stores "no target"), as `valueAt` reads it.
    fn no_reference(&self, dialect: Dialect) -> String {
        let mut parts: Vec<String> = self
            .from
            .columns
            .iter()
            .map(|c| format!("{} IS NULL", column(dialect, "s", c)))
            .collect();
        if let [single] = self.from.columns.as_slice() {
            parts.push(format!(
                "CAST({} AS BINARY) = '0'",
                column(dialect, "s", single)
            ));
        }
        format!("({})", parts.join(" OR "))
    }

    /// Compares with the column's collation, as following the reference does (its address
    /// filters the target by `=`), so the counts match what a click finds.
    fn exists(&self, dialect: Dialect) -> String {
        let mut on: Vec<String> = self
            .from
            .columns
            .iter()
            .zip(&self.to.columns)
            .map(|(from, to)| {
                format!(
                    "{} = {}",
                    column(dialect, "t", to),
                    column(dialect, "s", from)
                )
            })
            .collect();
        on.extend(conditions(dialect, "t", self.to_when));
        if !self.self_key.is_empty() {
            let same: Vec<String> = self
                .self_key
                .iter()
                .map(|k| format!("{} = {}", column(dialect, "t", k), column(dialect, "s", k)))
                .collect();
            on.push(format!("NOT ({})", same.join(" AND ")));
        }
        format!(
            "EXISTS (SELECT 1 FROM {} AS t WHERE {})",
            dialect.table_ref(self.database, &self.to.table),
            on.join(" AND ")
        )
    }
}

fn where_clause(filter: &[String]) -> String {
    if filter.is_empty() {
        return String::new();
    }
    format!(" WHERE {}", filter.join(" AND "))
}

fn column(dialect: Dialect, alias: &str, name: &str) -> String {
    format!("{alias}.{}", dialect.quote_ident(name))
}

fn conditions(dialect: Dialect, alias: &str, conditions: &[Condition]) -> Vec<String> {
    conditions
        .iter()
        .map(|c| match_sql(dialect, &column(dialect, alias, &c.column), &c.rule))
        .collect()
}

/// A condition in SQL, as exact as `Matcher` is in Rust: case-sensitive whatever the collation,
/// NULL matching nothing (not even `not`). `=`, IN and LIKE test the plain column first so an
/// index on it still narrows the rows; the binary test then makes them exact. MySQL rejects a
/// binary operand to REGEXP, so a regular expression turns case-sensitivity on with `(?-i)`
/// instead; it runs in the server's dialect (ICU on MySQL, PCRE on MariaDB), which agrees with
/// Rust's on common patterns.
fn match_sql(dialect: Dialect, column: &str, rule: &Match) -> String {
    match rule {
        Match::Eq(value) => exact(column, "=", &dialect.sql_string(value)),
        // Never true, and NULL on NULL, so `not` of it holds on every non-NULL value as in Rust.
        Match::In(values) if values.is_empty() => format!("({column} <> {column})"),
        Match::In(values) => {
            let list: Vec<String> = values.iter().map(|v| dialect.sql_string(v)).collect();
            exact(column, "IN", &format!("({})", list.join(", ")))
        }
        Match::Glob { glob } => {
            let like = escape_like(glob).replace('*', "%");
            exact(column, "LIKE", &dialect.like(&like))
        }
        Match::Like { like } => exact(column, "LIKE", &dialect.like(like)),
        Match::Regexp { regexp } => {
            let pattern = dialect.sql_string(&format!("(?-i){regexp}"));
            format!("({column} REGEXP {pattern})")
        }
        Match::Not { not } => format!("NOT {}", match_sql(dialect, column, not)),
    }
}

fn exact(column: &str, op: &str, operand: &str) -> String {
    format!("({column} {op} {operand} AND CAST({column} AS BINARY) {op} {operand})")
}

/// The coverage of `reference` in `database`; read-only queries on the metadata pool, each
/// bounded by `timeout` (a timed-out query is killed on the server).
pub async fn reference_coverage(
    session: &Session,
    database: &str,
    reference: &ResolvedReference,
    sources: RuleSources<'_>,
    timeout: Duration,
) -> AppResult<Coverage> {
    let schema = schema::database(session, database).await?;
    let rules = effective::effective(session, database, &schema, sources);
    let lens = lens::build(
        schema.clone(),
        &session.slug(),
        database,
        Some(rules.clone()),
    );
    if !lens
        .references_from(&reference.from.table)
        .contains(reference)
    {
        return Err(AppError::InvalidInput(format!(
            "`{database}` has no such reference from `{}` to `{}`",
            reference.from.table, reference.to.table
        )));
    }
    let (when, to_when) = rule_conditions(&rules, reference);
    let self_key = self_key_columns(&schema, reference);
    let query = CoverageQuery {
        database,
        from: &reference.from,
        to: &reference.to,
        when,
        to_when,
        self_key,
    };
    let dialect = session.db.dialect();
    let counts = match session.db.fetch(&query.counts_sql(dialect), timeout).await {
        Ok(result) => result,
        Err(AppError::Timeout { .. }) => {
            return Ok(Coverage {
                timed_out: true,
                ..Coverage::default()
            });
        }
        Err(e) => return Err(e),
    };
    let mut coverage = read_counts(&counts)?;
    if coverage.total == coverage.resolved + coverage.zero_or_null {
        return Ok(coverage);
    }
    match session.db.fetch(&query.sample_sql(dialect), timeout).await {
        Ok(result) => coverage.sample_unresolved = read_sample(&result),
        Err(AppError::Timeout { .. }) => coverage.timed_out = true,
        Err(e) => return Err(e),
    }
    Ok(coverage)
}

/// The `when` and `to_when` of a project or preset rule; declared and inferred references have
/// none.
fn rule_conditions<'r>(
    rules: &'r EffectiveRules,
    reference: &ResolvedReference,
) -> (&'r [Condition], &'r [Condition]) {
    if !matches!(
        reference.by,
        ReferenceSource::Project { .. } | ReferenceSource::Preset { .. }
    ) {
        return (&[], &[]);
    }
    let rule = rules.references.iter().find(|r| {
        r.by == reference.by
            && r.to.table == reference.to.table
            && reference.to.columns == [r.to.column.clone()]
    });
    rule.map_or((&[], &[]), |r| (&r.when, &r.to_when))
}

fn self_key_columns<'s>(schema: &'s DatabaseSchema, reference: &ResolvedReference) -> &'s [String] {
    if reference.from != reference.to {
        return &[];
    }
    schema
        .keys(&reference.from.table)
        .map_or(&[], |k| k.primary_key.as_slice())
}

fn read_counts(result: &ResultSet) -> AppResult<Coverage> {
    let row = result
        .rows
        .first()
        .ok_or_else(|| AppError::InvalidInput("the coverage query returned no row".into()))?;
    let number = |name: &str| -> u64 {
        result
            .columns
            .iter()
            .position(|c| c.name == name)
            .and_then(|i| row.get(i)?.as_text()?.parse().ok())
            .unwrap_or(0)
    };
    Ok(Coverage {
        total: number("total"),
        resolved: number("resolved"),
        zero_or_null: number("zero_or_null"),
        ..Coverage::default()
    })
}

fn read_sample(result: &ResultSet) -> Vec<String> {
    result
        .rows
        .iter()
        .filter_map(|row| Some(row.first()?.as_text()?.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::db::{
        Cell, CellLimit, ColumnInfo, ColumnMeta, Database, FakeDatabase, ForeignKey, TableStructure,
    };
    use crate::rules::ProjectsStore;
    use crate::rules::catalog::PresetCatalog;
    use crate::rules::condition::Matcher;
    use crate::rules::testing::reference;
    use crate::rules::types::{Conditions, Reference};
    use crate::test_support::{connect_to, servers, spec};

    const NO_BACKSLASH: Dialect = Dialect {
        backslash_escapes: false,
    };
    const TIMEOUT: Duration = Duration::from_secs(10);

    fn cond(column: &str, rule: Match) -> Condition {
        Condition {
            column: column.into(),
            matcher: Matcher::compile(&rule).unwrap(),
            rule,
        }
    }

    fn cols(table: &str, columns: &[&str]) -> ColumnsRef {
        ColumnsRef::new(table, columns.iter().map(|c| c.to_string()).collect())
    }

    fn query<'a>(from: &'a ColumnsRef, to: &'a ColumnsRef) -> CoverageQuery<'a> {
        CoverageQuery {
            database: "shop",
            from,
            to,
            when: &[],
            to_when: &[],
            self_key: &[],
        }
    }

    fn not(rule: Match) -> Match {
        Match::Not {
            not: Box::new(rule),
        }
    }

    #[test]
    fn match_sql_is_exact_and_escapes_per_dialect() {
        let cases = [
            (
                Match::Eq("a".into()),
                "(c = 'a' AND CAST(c AS BINARY) = 'a')",
                "(c = 'a' AND CAST(c AS BINARY) = 'a')",
            ),
            (
                Match::Eq("it's".into()),
                "(c = 'it''s' AND CAST(c AS BINARY) = 'it''s')",
                "(c = 'it''s' AND CAST(c AS BINARY) = 'it''s')",
            ),
            (
                Match::Eq(r"a\b".into()),
                r"(c = 'a\\b' AND CAST(c AS BINARY) = 'a\\b')",
                r"(c = 'a\b' AND CAST(c AS BINARY) = 'a\b')",
            ),
            (
                Match::In(vec!["a".into(), "b".into()]),
                "(c IN ('a', 'b') AND CAST(c AS BINARY) IN ('a', 'b'))",
                "(c IN ('a', 'b') AND CAST(c AS BINARY) IN ('a', 'b'))",
            ),
            (Match::In(Vec::new()), "(c <> c)", "(c <> c)"),
            (
                Match::Glob {
                    glob: "abc*".into(),
                },
                "(c LIKE 'abc%' AND CAST(c AS BINARY) LIKE 'abc%')",
                r"(c LIKE 'abc%' ESCAPE '\' AND CAST(c AS BINARY) LIKE 'abc%' ESCAPE '\')",
            ),
            (
                Match::Glob {
                    glob: r"a%b_c\d*".into(),
                },
                r"(c LIKE 'a\\%b\\_c\\\\d%' AND CAST(c AS BINARY) LIKE 'a\\%b\\_c\\\\d%')",
                r"(c LIKE 'a\%b\_c\\d%' ESCAPE '\' AND CAST(c AS BINARY) LIKE 'a\%b\_c\\d%' ESCAPE '\')",
            ),
            (
                Match::Like {
                    like: r"a\_%".into(),
                },
                r"(c LIKE 'a\\_%' AND CAST(c AS BINARY) LIKE 'a\\_%')",
                r"(c LIKE 'a\_%' ESCAPE '\' AND CAST(c AS BINARY) LIKE 'a\_%' ESCAPE '\')",
            ),
            (
                Match::Regexp {
                    regexp: "^[0-9]+$".into(),
                },
                "(c REGEXP '(?-i)^[0-9]+$')",
                "(c REGEXP '(?-i)^[0-9]+$')",
            ),
            (
                Match::Regexp {
                    regexp: r"\d+".into(),
                },
                r"(c REGEXP '(?-i)\\d+')",
                r"(c REGEXP '(?-i)\d+')",
            ),
            (
                not(Match::Eq("a".into())),
                "NOT (c = 'a' AND CAST(c AS BINARY) = 'a')",
                "NOT (c = 'a' AND CAST(c AS BINARY) = 'a')",
            ),
            (
                not(not(Match::Eq("a".into()))),
                "NOT NOT (c = 'a' AND CAST(c AS BINARY) = 'a')",
                "NOT NOT (c = 'a' AND CAST(c AS BINARY) = 'a')",
            ),
            (not(Match::In(Vec::new())), "NOT (c <> c)", "NOT (c <> c)"),
        ];
        for (rule, mysql, no_backslash) in cases {
            assert_eq!(match_sql(Dialect::MYSQL, "c", &rule), mysql, "{rule:?}");
            assert_eq!(
                match_sql(NO_BACKSLASH, "c", &rule),
                no_backslash,
                "{rule:?} without backslash escapes"
            );
        }
    }

    #[test]
    fn counts_sql_of_a_plain_foreign_key_counts_total_none_and_resolved_in_one_pass() {
        let from = cols("books", &["author_id"]);
        let to = cols("authors", &["id"]);

        assert_eq!(
            query(&from, &to).counts_sql(Dialect::MYSQL),
            "SELECT COUNT(*) AS total, \
             COALESCE(SUM((s.`author_id` IS NULL OR CAST(s.`author_id` AS BINARY) = '0')), 0) AS zero_or_null, \
             COALESCE(SUM(CASE WHEN (s.`author_id` IS NULL OR CAST(s.`author_id` AS BINARY) = '0') THEN 0 \
             WHEN EXISTS (SELECT 1 FROM `shop`.`authors` AS t WHERE t.`id` = s.`author_id`) THEN 1 ELSE 0 END), 0) AS resolved \
             FROM `shop`.`books` AS s"
        );
    }

    #[test]
    fn sample_sql_of_a_plain_foreign_key_lists_distinct_values_that_hold_a_reference_without_a_target()
     {
        let from = cols("books", &["author_id"]);
        let to = cols("authors", &["id"]);

        assert_eq!(
            query(&from, &to).sample_sql(Dialect::MYSQL),
            "SELECT DISTINCT CONCAT_WS(',', CAST(s.`author_id` AS CHAR)) AS value \
             FROM `shop`.`books` AS s \
             WHERE NOT ((s.`author_id` IS NULL OR CAST(s.`author_id` AS BINARY) = '0')) \
             AND NOT EXISTS (SELECT 1 FROM `shop`.`authors` AS t WHERE t.`id` = s.`author_id`) LIMIT 10"
        );
    }

    #[test]
    fn composite_columns_pair_up_check_null_on_every_part_and_skip_the_zero_check() {
        let from = cols("order_items", &["order_id", "sku"]);
        let to = cols("stock", &["oid", "code"]);

        let query = query(&from, &to);

        assert_eq!(
            query.counts_sql(Dialect::MYSQL),
            "SELECT COUNT(*) AS total, \
             COALESCE(SUM((s.`order_id` IS NULL OR s.`sku` IS NULL)), 0) AS zero_or_null, \
             COALESCE(SUM(CASE WHEN (s.`order_id` IS NULL OR s.`sku` IS NULL) THEN 0 \
             WHEN EXISTS (SELECT 1 FROM `shop`.`stock` AS t WHERE t.`oid` = s.`order_id` AND t.`code` = s.`sku`) THEN 1 ELSE 0 END), 0) AS resolved \
             FROM `shop`.`order_items` AS s"
        );
        assert_eq!(
            query.sample_sql(Dialect::MYSQL),
            "SELECT DISTINCT CONCAT_WS(',', CAST(s.`order_id` AS CHAR), CAST(s.`sku` AS CHAR)) AS value \
             FROM `shop`.`order_items` AS s \
             WHERE NOT ((s.`order_id` IS NULL OR s.`sku` IS NULL)) \
             AND NOT EXISTS (SELECT 1 FROM `shop`.`stock` AS t WHERE t.`oid` = s.`order_id` AND t.`code` = s.`sku`) LIMIT 10"
        );
    }

    #[test]
    fn when_filters_the_source_rows_and_to_when_filters_the_target_inside_exists() {
        let from = cols("threads", &["author"]);
        let to = cols("people", &["person_id"]);
        let when = [cond("kind", Match::Eq("a".into()))];
        let to_when = [cond("active", Match::Eq("1".into()))];
        let query = CoverageQuery {
            when: &when,
            to_when: &to_when,
            ..query(&from, &to)
        };

        assert_eq!(
            query.counts_sql(Dialect::MYSQL),
            "SELECT COUNT(*) AS total, \
             COALESCE(SUM((s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0')), 0) AS zero_or_null, \
             COALESCE(SUM(CASE WHEN (s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0') THEN 0 \
             WHEN EXISTS (SELECT 1 FROM `shop`.`people` AS t WHERE t.`person_id` = s.`author` \
             AND (t.`active` = '1' AND CAST(t.`active` AS BINARY) = '1')) THEN 1 ELSE 0 END), 0) AS resolved \
             FROM `shop`.`threads` AS s WHERE (s.`kind` = 'a' AND CAST(s.`kind` AS BINARY) = 'a')"
        );
        assert_eq!(
            query.sample_sql(Dialect::MYSQL),
            "SELECT DISTINCT CONCAT_WS(',', CAST(s.`author` AS CHAR)) AS value \
             FROM `shop`.`threads` AS s \
             WHERE (s.`kind` = 'a' AND CAST(s.`kind` AS BINARY) = 'a') \
             AND NOT ((s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0')) \
             AND NOT EXISTS (SELECT 1 FROM `shop`.`people` AS t WHERE t.`person_id` = s.`author` \
             AND (t.`active` = '1' AND CAST(t.`active` AS BINARY) = '1')) LIMIT 10"
        );
    }

    #[test]
    fn several_conditions_are_joined_with_and_in_the_dialect_of_the_server() {
        let from = cols("notes", &["v"]);
        let to = cols("threads", &["thread_id"]);
        let when = [
            cond("k", Match::Glob { glob: "_x*".into() }),
            cond("flag", not(Match::Eq("0".into()))),
        ];
        let query = CoverageQuery {
            when: &when,
            ..query(&from, &to)
        };

        assert!(
            query.counts_sql(NO_BACKSLASH).ends_with(
                r"FROM `shop`.`notes` AS s WHERE (s.`k` LIKE '\_x%' ESCAPE '\' AND CAST(s.`k` AS BINARY) LIKE '\_x%' ESCAPE '\') AND NOT (s.`flag` = '0' AND CAST(s.`flag` AS BINARY) = '0')"
            ),
            "{}",
            query.counts_sql(NO_BACKSLASH)
        );
    }

    #[test]
    fn self_key_excludes_the_row_itself_from_its_targets() {
        let from = cols("orders", &["billing_email"]);
        let to = cols("orders", &["billing_email"]);
        let self_key = ["id".to_string()];
        let composite = ["a".to_string(), "b".to_string()];

        let single = CoverageQuery {
            self_key: &self_key,
            ..query(&from, &to)
        };
        let multi = CoverageQuery {
            self_key: &composite,
            ..query(&from, &to)
        };

        assert!(
            single.counts_sql(Dialect::MYSQL).contains(
                "EXISTS (SELECT 1 FROM `shop`.`orders` AS t WHERE t.`billing_email` = s.`billing_email` AND NOT (t.`id` = s.`id`))"
            ),
            "{}",
            single.counts_sql(Dialect::MYSQL)
        );
        assert!(
            multi.sample_sql(Dialect::MYSQL).contains(
                "WHERE t.`billing_email` = s.`billing_email` AND NOT (t.`a` = s.`a` AND t.`b` = s.`b`))"
            ),
            "{}",
            multi.sample_sql(Dialect::MYSQL)
        );
    }

    #[test]
    fn identifiers_are_quoted_and_the_tables_are_database_qualified() {
        let from = cols("t`b", &["c`d"]);
        let to = cols("u", &["e"]);
        let query = CoverageQuery {
            database: "d`b",
            ..query(&from, &to)
        };

        assert_eq!(
            query.sample_sql(Dialect::MYSQL),
            "SELECT DISTINCT CONCAT_WS(',', CAST(s.`c``d` AS CHAR)) AS value \
             FROM `d``b`.`t``b` AS s \
             WHERE NOT ((s.`c``d` IS NULL OR CAST(s.`c``d` AS BINARY) = '0')) \
             AND NOT EXISTS (SELECT 1 FROM `d``b`.`u` AS t WHERE t.`e` = s.`c``d`) LIMIT 10"
        );
    }

    fn structure(
        columns: &[&str],
        primary_key: &str,
        foreign_keys: Vec<ForeignKey>,
    ) -> TableStructure {
        let column = |name: &&str| ColumnInfo {
            name: name.to_string(),
            column_type: "bigint".into(),
            nullable: false,
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
            foreign_keys,
            ddl: None,
        }
    }

    /// `wp` with a declared foreign key (`books.author_id` to `authors.id`) and the tables the
    /// rules below refer to under the prefix `f_`.
    fn shop() -> FakeDatabase {
        let books_author = ForeignKey {
            name: "books_author".into(),
            columns: vec!["author_id".into()],
            ref_table: "authors".into(),
            ref_columns: vec!["id".into()],
        };
        FakeDatabase::new()
            .with_database("wp")
            .with_table("wp", "authors", structure(&["id"], "id", Vec::new()))
            .with_table(
                "wp",
                "books",
                structure(&["id", "author_id"], "id", vec![books_author]),
            )
            .with_table(
                "wp",
                "f_threads",
                structure(&["thread_id", "author", "kind"], "thread_id", Vec::new()),
            )
            .with_table(
                "wp",
                "f_people",
                structure(&["person_id", "active"], "person_id", Vec::new()),
            )
    }

    fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: columns
                .iter()
                .map(|c| ColumnMeta {
                    name: c.to_string(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            returns_rows: true,
            ..ResultSet::default()
        }
    }

    fn counts(total: u64, zero_or_null: u64, resolved: u64) -> ResultSet {
        let number = |n: u64| Cell::Number(n.to_string());
        result(
            &["total", "zero_or_null", "resolved"],
            vec![vec![number(total), number(zero_or_null), number(resolved)]],
        )
    }

    fn sample(values: &[&str]) -> ResultSet {
        result(
            &["value"],
            values
                .iter()
                .map(|v| vec![Cell::Text(v.to_string())])
                .collect(),
        )
    }

    fn books_author() -> ResolvedReference {
        ResolvedReference {
            from: cols("books", &["author_id"]),
            to: cols("authors", &["id"]),
            by: ReferenceSource::ForeignKey {
                constraint: "books_author".into(),
            },
        }
    }

    const LINKS: &str = r#"
[preset]
name = "links"
marker_tables = ["threads"]

[[reference]]
from = "threads.author"
when = { kind = "a" }
to = "people.person_id"
to_when = { active = "1" }
"#;

    struct Rig {
        dir: TempDir,
        db: Arc<FakeDatabase>,
        session: Session,
        catalog: PresetCatalog,
        projects: ProjectsStore,
    }

    impl Rig {
        fn new(db: FakeDatabase) -> Self {
            let dir = tempdir().unwrap();
            let db = Arc::new(db);
            let session = Session::new(
                "s".into(),
                spec("127.0.0.1", 3306, "wp"),
                "local".into(),
                Box::new(db.clone()),
            );
            let catalog = PresetCatalog::with_built_in(dir.path().join("presets"), &[]);
            let projects = ProjectsStore::open(dir.path());
            Self {
                dir,
                db,
                session,
                catalog,
                projects,
            }
        }

        fn add_preset(&self, src: &str) {
            let presets = self.dir.path().join("presets");
            fs::create_dir_all(&presets).unwrap();
            fs::write(presets.join("links.toml"), src).unwrap();
        }

        fn sources(&self) -> RuleSources<'_> {
            RuleSources {
                presets: &self.catalog,
                projects: &self.projects,
            }
        }

        async fn outgoing(&self, table: &str) -> Vec<ResolvedReference> {
            lens::table_references(&self.session, "wp", table, Some(self.sources()))
                .await
                .unwrap()
                .outgoing
        }

        async fn coverage(&self, reference: &ResolvedReference) -> AppResult<Coverage> {
            reference_coverage(&self.session, "wp", reference, self.sources(), TIMEOUT).await
        }
    }

    #[tokio::test]
    async fn a_reference_the_lens_does_not_have_is_invalid_input_and_runs_no_query() {
        let rig = Rig::new(shop());
        let unknown = ResolvedReference {
            by: ReferenceSource::ForeignKey {
                constraint: "no_such_key".into(),
            },
            ..books_author()
        };

        let err = rig.coverage(&unknown).await.unwrap_err();

        assert_eq!(err.kind(), crate::error::ErrorKind::InvalidInput);
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn a_count_that_times_out_reports_timed_out_with_zero_counts_and_skips_the_sample() {
        let rig =
            Rig::new(shop().failing("COUNT(*)", AppError::timeout(Duration::from_millis(200))));

        let coverage = rig.coverage(&books_author()).await.unwrap();

        assert_eq!(
            coverage,
            Coverage {
                timed_out: true,
                ..Coverage::default()
            }
        );
        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn a_sample_that_times_out_keeps_the_counts() {
        let rig = Rig::new(shop().on("COUNT(*)", counts(5, 2, 1)).failing(
            "SELECT DISTINCT",
            AppError::timeout(Duration::from_millis(200)),
        ));

        let coverage = rig.coverage(&books_author()).await.unwrap();

        assert_eq!(
            coverage,
            Coverage {
                total: 5,
                resolved: 1,
                zero_or_null: 2,
                sample_unresolved: Vec::new(),
                timed_out: true,
            }
        );
    }

    #[tokio::test]
    async fn counts_and_sample_come_from_the_results() {
        let rig = Rig::new(
            shop()
                .on("COUNT(*)", counts(5, 2, 1))
                .on("SELECT DISTINCT", sample(&["98", "99"])),
        );

        let coverage = rig.coverage(&books_author()).await.unwrap();

        assert_eq!(
            coverage,
            Coverage {
                total: 5,
                resolved: 1,
                zero_or_null: 2,
                sample_unresolved: vec!["98".into(), "99".into()],
                timed_out: false,
            }
        );
        assert_eq!(rig.db.queries().len(), 2);
    }

    #[tokio::test]
    async fn the_sample_is_skipped_when_every_row_resolves_or_holds_no_reference() {
        let rig = Rig::new(shop().on("COUNT(*)", counts(3, 1, 2)));

        let coverage = rig.coverage(&books_author()).await.unwrap();

        assert_eq!(
            coverage,
            Coverage {
                total: 3,
                resolved: 2,
                zero_or_null: 1,
                sample_unresolved: Vec::new(),
                timed_out: false,
            }
        );
        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn another_error_of_the_count_is_returned() {
        let rig = Rig::new(shop().failing("COUNT(*)", AppError::Cancelled));

        let err = rig.coverage(&books_author()).await.unwrap_err();

        assert_eq!(err.kind(), crate::error::ErrorKind::Cancelled);
    }

    #[tokio::test]
    async fn a_preset_reference_uses_the_when_and_to_when_of_its_rule() {
        let rig = Rig::new(shop().on("COUNT(*)", counts(4, 1, 3)));
        rig.add_preset(LINKS);
        let preset_rule = rig
            .outgoing("f_threads")
            .await
            .into_iter()
            .find(|r| matches!(r.by, ReferenceSource::Preset { .. }))
            .unwrap();

        let coverage = rig.coverage(&preset_rule).await.unwrap();

        assert_eq!(
            coverage,
            Coverage {
                total: 4,
                resolved: 3,
                zero_or_null: 1,
                sample_unresolved: Vec::new(),
                timed_out: false,
            }
        );
        assert_eq!(
            rig.db.queries(),
            ["SELECT COUNT(*) AS total, \
                 COALESCE(SUM((s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0')), 0) AS zero_or_null, \
                 COALESCE(SUM(CASE WHEN (s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0') THEN 0 \
                 WHEN EXISTS (SELECT 1 FROM `wp`.`f_people` AS t WHERE t.`person_id` = s.`author` \
                 AND (t.`active` = '1' AND CAST(t.`active` AS BINARY) = '1')) THEN 1 ELSE 0 END), 0) AS resolved \
                 FROM `wp`.`f_threads` AS s WHERE (s.`kind` = 'a' AND CAST(s.`kind` AS BINARY) = 'a')"]
        );
    }

    #[tokio::test]
    async fn a_project_reference_uses_the_when_of_its_rule() {
        let rig = Rig::new(shop().on("COUNT(*)", counts(2, 0, 2)));
        rig.projects
            .update("local", "wp", |p| {
                p.prefix = Some("f_".into());
                p.references = vec![Reference {
                    when: Conditions::from([("kind".to_string(), Match::Eq("b".into()))]),
                    ..reference("mine", "threads.author", "people.person_id")
                }];
                Ok(())
            })
            .await
            .unwrap();
        let project_rule = rig
            .outgoing("f_threads")
            .await
            .into_iter()
            .find(|r| matches!(r.by, ReferenceSource::Project { .. }))
            .unwrap();

        rig.coverage(&project_rule).await.unwrap();

        assert_eq!(
            rig.db.queries(),
            ["SELECT COUNT(*) AS total, \
                 COALESCE(SUM((s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0')), 0) AS zero_or_null, \
                 COALESCE(SUM(CASE WHEN (s.`author` IS NULL OR CAST(s.`author` AS BINARY) = '0') THEN 0 \
                 WHEN EXISTS (SELECT 1 FROM `wp`.`f_people` AS t WHERE t.`person_id` = s.`author`) THEN 1 ELSE 0 END), 0) AS resolved \
                 FROM `wp`.`f_threads` AS s WHERE (s.`kind` = 'b' AND CAST(s.`kind` AS BINARY) = 'b')"]
        );
    }

    async fn fixture_session(host: &str, port: u16, database: &str) -> Session {
        let db = connect_to(host, port, database).await;
        Session::new(
            "it".into(),
            spec(host, port, database),
            format!("it-{port}"),
            Box::new(db),
        )
    }

    /// The draft presets as user presets, as the effective-rules integration test has them.
    fn draft_sources(dir: &TempDir) -> (PresetCatalog, ProjectsStore) {
        let presets = dir.path().join("presets");
        fs::create_dir_all(&presets).unwrap();
        fs::write(
            presets.join("wordpress.toml"),
            include_str!("fixtures/wordpress.draft.toml"),
        )
        .unwrap();
        fs::write(
            presets.join("woocommerce.toml"),
            include_str!("fixtures/woocommerce.draft.toml"),
        )
        .unwrap();
        (
            PresetCatalog::with_built_in(presets, &[]),
            ProjectsStore::open(dir.path()),
        )
    }

    async fn find(
        session: &Session,
        database: &str,
        table: &str,
        sources: RuleSources<'_>,
        pick: impl Fn(&ResolvedReference) -> bool,
    ) -> ResolvedReference {
        let outgoing = lens::table_references(session, database, table, Some(sources))
            .await
            .unwrap()
            .outgoing;
        outgoing
            .iter()
            .find(|r| pick(r))
            .unwrap_or_else(|| panic!("no such reference in {outgoing:?}"))
            .clone()
    }

    fn preset_rule(preset: &str, id: &str) -> impl Fn(&ResolvedReference) -> bool {
        let by = ReferenceSource::Preset {
            preset: preset.into(),
            id: id.into(),
        };
        move |r| r.by == by
    }

    #[tokio::test]
    #[ignore]
    async fn an_inferred_reference_counts_resolved_none_and_unresolved_rows() {
        let dir = tempdir().unwrap();
        let (catalog, projects) = draft_sources(&dir);
        let sources = RuleSources {
            presets: &catalog,
            projects: &projects,
        };
        for (host, port) in servers() {
            let session = fixture_session(&host, port, "databee_test").await;
            let rule = find(&session, "databee_test", "cov_children", sources, |r| {
                r.to.table == "cov_parents"
            })
            .await;

            let mut coverage =
                reference_coverage(&session, "databee_test", &rule, sources, TIMEOUT)
                    .await
                    .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

            coverage.sample_unresolved.sort();
            assert_eq!(
                coverage,
                Coverage {
                    total: 5,
                    resolved: 1,
                    zero_or_null: 2,
                    sample_unresolved: vec!["98".into(), "99".into()],
                    timed_out: false,
                },
                "{host}:{port}"
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn a_declared_foreign_key_counts_its_null_as_no_reference() {
        let dir = tempdir().unwrap();
        let (catalog, projects) = draft_sources(&dir);
        let sources = RuleSources {
            presets: &catalog,
            projects: &projects,
        };
        for (host, port) in servers() {
            let session = fixture_session(&host, port, "databee_test").await;
            let rule = find(&session, "databee_test", "books", sources, |r| {
                matches!(&r.by, ReferenceSource::ForeignKey { constraint } if constraint == "books_author")
            })
            .await;

            let coverage = reference_coverage(&session, "databee_test", &rule, sources, TIMEOUT)
                .await
                .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));

            assert_eq!(
                coverage,
                Coverage {
                    total: 3,
                    resolved: 2,
                    zero_or_null: 1,
                    sample_unresolved: Vec::new(),
                    timed_out: false,
                },
                "{host}:{port}"
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn the_draft_presets_cover_the_wordpress_fixture() {
        let dir = tempdir().unwrap();
        let (catalog, projects) = draft_sources(&dir);
        let sources = RuleSources {
            presets: &catalog,
            projects: &projects,
        };
        for (host, port) in servers() {
            let session = fixture_session(&host, port, "wp_test").await;
            let thumbnail = find(
                &session,
                "wp_test",
                "wp_postmeta",
                sources,
                preset_rule("wordpress", "postmeta.meta_value?meta_key=_thumbnail_id"),
            )
            .await;
            let customer = find(
                &session,
                "wp_test",
                "wp_wc_orders",
                sources,
                preset_rule("woocommerce", "wc_orders.customer_id"),
            )
            .await;

            let thumbnail = reference_coverage(&session, "wp_test", &thumbnail, sources, TIMEOUT)
                .await
                .unwrap_or_else(|e| panic!("{host}:{port}: {e:?}"));
            let customer = reference_coverage(&session, "wp_test", &customer, sources, TIMEOUT)
                .await
                .unwrap();

            assert_eq!(
                thumbnail,
                Coverage {
                    total: 1,
                    resolved: 1,
                    ..Coverage::default()
                },
                "{host}:{port}"
            );
            assert_eq!(
                customer,
                Coverage {
                    total: 4,
                    resolved: 2,
                    zero_or_null: 2,
                    ..Coverage::default()
                },
                "{host}:{port}"
            );
        }
    }

    #[tokio::test]
    #[ignore]
    async fn a_count_blocked_by_a_table_lock_times_out() {
        let dir = tempdir().unwrap();
        let (catalog, projects) = draft_sources(&dir);
        let sources = RuleSources {
            presets: &catalog,
            projects: &projects,
        };
        for (host, port) in servers() {
            let session = fixture_session(&host, port, "databee_test").await;
            let rule = find(&session, "databee_test", "cov_children", sources, |r| {
                r.to.table == "cov_parents"
            })
            .await;
            let locker = connect_to(&host, port, "databee_test").await;
            locker
                .execute(
                    "LOCK TABLES databee_test.cov_children WRITE",
                    10,
                    CellLimit::NONE,
                    TIMEOUT,
                )
                .await
                .unwrap();

            let blocked = reference_coverage(
                &session,
                "databee_test",
                &rule,
                sources,
                Duration::from_millis(200),
            )
            .await;
            let unlocked = locker
                .execute("UNLOCK TABLES", 10, CellLimit::NONE, TIMEOUT)
                .await;

            unlocked.unwrap();
            assert_eq!(
                blocked.unwrap_or_else(|e| panic!("{host}:{port}: {e:?}")),
                Coverage {
                    timed_out: true,
                    ..Coverage::default()
                },
                "{host}:{port}"
            );
        }
    }
}
