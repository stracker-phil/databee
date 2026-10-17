//! The row itself: read with the filter of every entity of its table, so the row picks its entity
//! in the same query (the first entity whose filter holds; filtered ones come first).

use super::context::{Ctx, Rows};
use crate::address::{Address, Target};
use crate::db::ColumnEquals;
use crate::error::{AppError, AppResult};
use crate::rules::effective::AppliedEntity;
use crate::rules::placeholder::with_prefixes;

const ENTITY_COLUMN: &str = "__databee_entity_";

pub struct Own {
    pub rows: Rows,
    pub entity: Option<AppliedEntity>,
}

impl Own {
    pub fn table(&self) -> &str {
        &self.rows.table
    }

    /// The row's text in `column`: `None` without such a column or for NULL.
    pub fn text(&self, column: &str) -> Option<&str> {
        self.column_text(column).flatten()
    }

    /// `None` without such a column, `Some(None)` for NULL, as conditions test it.
    pub fn column_text(&self, column: &str) -> Option<Option<&str>> {
        self.rows.text(0, column)
    }

    /// The single-column key's value, for `{id}` and the `fk` of meta rows.
    pub fn id(&self, ctx: &Ctx<'_>) -> Option<&str> {
        self.text(ctx.single_key(self.table())?)
    }
}

/// The table and the `=` conditions that select the row of `address`: a single-column key, or
/// a filter on every column of a composite key.
pub fn row_filter(ctx: &Ctx<'_>, address: &Address) -> AppResult<(String, Vec<ColumnEquals>)> {
    match &address.target {
        Target::Row { table, key } => {
            let column = ctx.single_key(table).ok_or_else(|| {
                AppError::InvalidInput(format!("`{table}` has no single-column primary key"))
            })?;
            let filter = vec![ColumnEquals {
                column: column.to_string(),
                value: Some(key.clone()),
            }];
            Ok((table.clone(), filter))
        }
        Target::Table { table, filter } if covers_key(ctx.primary_key(table), filter) => {
            Ok((table.clone(), filter.clone()))
        }
        _ => Err(AppError::InvalidInput(
            "A document needs the address of one row".into(),
        )),
    }
}

fn covers_key(key: &[String], filter: &[ColumnEquals]) -> bool {
    !key.is_empty()
        && key
            .iter()
            .all(|k| filter.iter().any(|f| f.column.eq_ignore_ascii_case(k)))
}

pub async fn load(ctx: &Ctx<'_>, table: &str, filter: &[ColumnEquals]) -> AppResult<Own> {
    let entities: Vec<&AppliedEntity> = ctx
        .rules
        .entities
        .iter()
        .filter(|e| e.entity.table == table)
        .collect();
    let tests: Vec<String> = entities
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let test = match &e.entity.filter {
                Some(filter) => format!("({})", entity_filter(ctx, filter)),
                None => "1".into(),
            };
            format!(", {test} AS {}", ctx.ident(&format!("{ENTITY_COLUMN}{i}")))
        })
        .collect();
    let sql = format!(
        "SELECT t.*{} FROM {} AS t WHERE {} LIMIT 2",
        tests.concat(),
        ctx.table_ref(table),
        equals_sql(ctx, Some("t"), filter)
    );
    let mut result = ctx.fetch(&sql).await?;
    if result.rows.len() != 1 {
        return Err(AppError::InvalidInput(format!(
            "`{table}` has {} rows for this key",
            if result.rows.is_empty() {
                "no"
            } else {
                "several"
            }
        )));
    }
    let own_columns = result.columns.len() - entities.len();
    let row = result.rows.remove(0);
    let entity = row[own_columns..]
        .iter()
        .position(|cell| cell.as_text().is_some_and(|t| t != "0"))
        .map(|i| entities[i].clone());
    result.columns.truncate(own_columns);
    result.rows = vec![row[..own_columns].to_vec()];
    Ok(Own {
        rows: ctx.annotate(table, result),
        entity,
    })
}

/// An entity `filter` with the prefixes filled in, its tables qualified with the database.
pub fn entity_filter(ctx: &Ctx<'_>, filter: &str) -> String {
    let applied = &ctx.rules.applied;
    with_prefixes(
        &ctx.qualified(filter),
        &applied.prefix,
        &applied.network_prefix,
    )
}

/// `a = 'x' AND b IS NULL`, the columns qualified with `alias`.
pub fn equals_sql(ctx: &Ctx<'_>, alias: Option<&str>, filter: &[ColumnEquals]) -> String {
    filter
        .iter()
        .map(|f| match &f.value {
            Some(value) => format!("{} = {}", ctx.column(alias, &f.column), ctx.literal(value)),
            None => format!("{} IS NULL", ctx.column(alias, &f.column)),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, TableStructure};
    use crate::document::build;
    use crate::document::testing::{Rig, num, result, shop, text};

    const ORDER_COLUMNS: [&str; 5] = ["id", "status", "total", "currency", "created_gmt"];

    fn order_row(id: &str, status: &str) -> Vec<Cell> {
        vec![
            num(id),
            text(status),
            text("10.00"),
            text("EUR"),
            Cell::Temporal("2023-11-14 22:13:20".into()),
        ]
    }

    fn pairs() -> TableStructure {
        TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: vec!["a".into(), "b".into()],
            foreign_keys: Vec::new(),
            ddl: None,
        }
    }

    fn equals(column: &str, value: &str) -> ColumnEquals {
        ColumnEquals {
            column: column.into(),
            value: Some(value.into()),
        }
    }

    fn message(err: AppError) -> String {
        match err {
            AppError::InvalidInput(message) => message,
            other => panic!("{other:?}"),
        }
    }

    fn row(table: &str, key: &str) -> Target {
        Target::Row {
            table: table.into(),
            key: key.into(),
        }
    }

    fn rows_of(table: &str, filter: Vec<ColumnEquals>) -> Target {
        Target::Table {
            table: table.into(),
            filter,
        }
    }

    #[tokio::test]
    async fn an_address_that_selects_no_row_or_several_names_the_table() {
        let cases = [
            (
                Vec::new(),
                row("f_orders", "1"),
                "`f_orders` has no rows for this key",
            ),
            (
                vec![order_row("1", "open"), order_row("1", "open")],
                rows_of("f_orders", vec![equals("id", "1")]),
                "`f_orders` has several rows for this key",
            ),
        ];
        for (rows, target, expected) in cases {
            let rig = Rig::new(shop().on("AS t WHERE", result(&ORDER_COLUMNS, rows)), "");
            let ctx = rig.ctx().await;

            let err = build::document(&ctx, &ctx.address(target))
                .await
                .err()
                .unwrap();

            assert_eq!(message(err), expected);
        }
    }

    #[tokio::test]
    async fn a_row_address_on_a_composite_key_table_is_invalid_input() {
        let rig = Rig::new(shop().with_table("wp", "f_pairs", pairs()), "");
        let ctx = rig.ctx().await;

        let err = build::document(&ctx, &ctx.address(row("f_pairs", "1")))
            .await
            .err()
            .unwrap();

        assert_eq!(message(err), "`f_pairs` has no single-column primary key");
        assert_eq!(rig.db.queries(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_filter_on_every_column_of_a_composite_key_opens_the_row() {
        let pair = result(&["a", "b", "c"], vec![vec![num("1"), num("2"), text("x")]]);
        let rig = Rig::new(
            shop()
                .with_table("wp", "f_pairs", pairs())
                .on("`f_pairs` AS t", pair),
            "",
        );
        let ctx = rig.ctx().await;
        let filter = vec![equals("a", "1"), equals("b", "2")];

        let document = build::document(&ctx, &ctx.address(rows_of("f_pairs", filter.clone())))
            .await
            .ok()
            .unwrap();

        assert_eq!(document.address, ctx.address(rows_of("f_pairs", filter)));
        assert_eq!(
            rig.db.queries()[0],
            "SELECT t.* FROM `wp`.`f_pairs` AS t WHERE t.`a` = '1' AND t.`b` = '2' LIMIT 2"
        );
    }

    #[tokio::test]
    async fn a_filter_that_leaves_a_key_column_out_or_a_home_address_is_not_one_row() {
        let rig = Rig::new(shop().with_table("wp", "f_pairs", pairs()), "");
        let ctx = rig.ctx().await;

        for target in [
            rows_of("f_pairs", vec![equals("a", "1")]),
            rows_of("f_pairs", vec![equals("a", "1"), equals("c", "2")]),
            Target::Home,
        ] {
            let address = ctx.address(target.clone());

            let err = build::document(&ctx, &address).await.err().unwrap();

            assert_eq!(
                message(err),
                "A document needs the address of one row",
                "{target:?}"
            );
        }
        assert_eq!(rig.db.queries(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_filter_on_the_single_key_in_any_case_opens_at_the_row_address() {
        for column in ["id", "ID", "Id"] {
            let rig = Rig::new(
                shop()
                    .on(
                        "AS t WHERE",
                        result(&ORDER_COLUMNS, vec![order_row("5", "open")]),
                    )
                    .on(
                        "`f_ordermeta`",
                        result(&["meta_id", "order_id", "k", "v"], Vec::new()),
                    ),
                "",
            );
            let ctx = rig.ctx().await;
            let address = ctx.address(rows_of("f_orders", vec![equals(column, "5")]));

            let document = build::document(&ctx, &address).await.ok().unwrap();

            assert_eq!(
                document.address,
                ctx.row_address("f_orders", "5"),
                "{column}"
            );
        }
    }

    const PAID_AND_PLAIN: &str = r#"
[[entity]]
name = "order"
table = "orders"

[[entity]]
name = "paid"
table = "orders"
filter = "status = 'paid'"
"#;

    const PAID_ONLY: &str = r#"
[[entity]]
name = "paid"
table = "orders"
filter = "status = 'paid'"
"#;

    fn with_entity_tests(row: Vec<Cell>, tests: &[&str]) -> crate::db::ResultSet {
        let mut columns: Vec<String> = ORDER_COLUMNS.iter().map(ToString::to_string).collect();
        let mut row = row;
        for (i, test) in tests.iter().enumerate() {
            columns.push(format!("__databee_entity_{i}"));
            row.push(num(test));
        }
        let names: Vec<&str> = columns.iter().map(String::as_str).collect();
        result(&names, vec![row])
    }

    #[tokio::test]
    async fn the_row_takes_the_first_entity_whose_filter_holds() {
        let cases = [
            (PAID_AND_PLAIN, "paid", ["1", "1"].as_slice(), Some("paid")),
            (PAID_AND_PLAIN, "open", ["0", "1"].as_slice(), Some("order")),
            (PAID_ONLY, "paid", ["1"].as_slice(), Some("paid")),
            (PAID_ONLY, "open", ["0"].as_slice(), None),
        ];
        for (rules, status, tests, expected) in cases {
            let rig = Rig::new(
                shop().on(
                    "AS t WHERE",
                    with_entity_tests(order_row("1", status), tests),
                ),
                rules,
            );
            let ctx = rig.ctx().await;

            let own = load(&ctx, "f_orders", &[equals("id", "1")])
                .await
                .ok()
                .unwrap();

            assert_eq!(
                own.entity.map(|e| e.entity.name),
                expected.map(str::to_string),
                "{rules} {status}"
            );
            assert_eq!(own.rows.names(), ORDER_COLUMNS, "{rules} {status}");
        }
    }

    #[tokio::test]
    async fn every_entity_filter_is_tested_in_the_same_select() {
        let rig = Rig::new(
            shop().on(
                "AS t WHERE",
                with_entity_tests(order_row("1", "paid"), &["1", "1"]),
            ),
            PAID_AND_PLAIN,
        );
        let ctx = rig.ctx().await;

        load(&ctx, "f_orders", &[equals("id", "1")])
            .await
            .ok()
            .unwrap();

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT t.*, (status = 'paid') AS `__databee_entity_0`, 1 AS `__databee_entity_1` \
              FROM `wp`.`f_orders` AS t WHERE t.`id` = '1' LIMIT 2"
            ]
        );
    }
}
