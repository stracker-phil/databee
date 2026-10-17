//! A row's summary: the one rule behind the document header, peeks, palette hits and the readout
//! (docs/architecture.md, "Summaries"). Rows of one table are summarized together, so the meta
//! keys they need are read in one query per meta table, never per row.

use std::collections::HashMap;

use futures_util::future::join_all;
use serde::Serialize;
use ts_rs::TS;

use super::meta::MetaValues;
use super::non_empty;
use super::own::{self, Own, Selection};
use crate::db::browse::aliased_equals_sql;
use crate::db::{Cell, ColumnEquals};
use crate::query::{Ctx, CurrencyLookup, KeyOrder, Rows};
use crate::rules::effective::{AppliedEntity, MetaRef};
use crate::rules::kinds::ValueRule;
use crate::value::{self, RowKey, Value, ValueKind};

/// Values of a row whose entity has no summary.
pub const PLAIN_VALUES: usize = 4;

const ALIAS: &str = "t";

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct SummaryValue {
    /// The column as the table spells it, or the meta key.
    pub name: String,
    pub value: Value,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct Summary {
    pub values: Vec<SummaryValue>,
}

/// The rule. With `summary` names: each in order, a column of the row (in any case), else a meta
/// key, else NULL, so a configured name never silently drops out. Without: the first
/// `PLAIN_VALUES` columns outside `key` that are neither NULL nor empty, in schema order.
pub fn rule(
    names: &[String],
    values: &[Value],
    key: &[String],
    summary: &[String],
    meta: &dyn Fn(&str) -> Option<Value>,
) -> Summary {
    let column = |name: &str| names.iter().position(|n| n.eq_ignore_ascii_case(name));
    let values = if summary.is_empty() {
        names
            .iter()
            .zip(values)
            .filter(|(name, value)| {
                !key.iter().any(|k| k.eq_ignore_ascii_case(name)) && shown(value)
            })
            .take(PLAIN_VALUES)
            .map(|(name, value)| SummaryValue {
                name: name.clone(),
                value: value.clone(),
            })
            .collect()
    } else {
        summary
            .iter()
            .map(|name| match column(name) {
                Some(col) => SummaryValue {
                    name: names[col].clone(),
                    value: values[col].clone(),
                },
                None => SummaryValue {
                    name: name.clone(),
                    value: meta(name).unwrap_or_else(missing),
                },
            })
            .collect()
    };
    Summary { values }
}

fn shown(value: &Value) -> bool {
    !matches!(&value.raw, Cell::Null) && value.raw.as_text() != Some("")
}

fn missing() -> Value {
    Value {
        raw: Cell::Null,
        kind: ValueKind::Null,
        source: None,
        style: None,
        secondary: None,
    }
}

/// The header of a document, from the values and meta rows it has read anyway.
pub fn of_document(ctx: &Ctx<'_>, own: &Own, values: &[Value], meta: &MetaValues) -> Summary {
    rule(
        &own.rows.names(),
        values,
        ctx.primary_key(own.table()),
        summary_names(own.entity.as_ref()),
        &|key| meta.get(key).cloned(),
    )
}

pub async fn of_own(ctx: &Ctx<'_>, own: &Own) -> Summary {
    let item = RowRef {
        rows: &own.rows,
        row: 0,
        entity: own.entity.as_ref(),
    };
    of_rows(ctx, &[item]).await.pop().unwrap_or_default()
}

/// One row of a `Rows` with its entity.
#[derive(Clone, Copy)]
pub struct RowRef<'a> {
    pub rows: &'a Rows,
    pub row: usize,
    pub entity: Option<&'a AppliedEntity>,
}

fn summary_names(entity: Option<&AppliedEntity>) -> &[String] {
    entity.map_or(&[], |e| &e.entity.summary)
}

/// The summaries of rows of one table. Only rows whose summary names a meta key, or shows money
/// without a currency in its row, read meta rows: those of all such rows in one query per meta
/// table.
pub async fn of_rows(ctx: &Ctx<'_>, items: &[RowRef<'_>]) -> Vec<Summary> {
    let Some(first) = items.first() else {
        return Vec::new();
    };
    let table = first.rows.table.as_str();
    let key = ctx.primary_key(table);
    let plain: Vec<Vec<Value>> = items
        .iter()
        .map(|item| item.rows.row_values(item.row, &|_, _| None))
        .collect();
    let without_meta: Vec<Summary> = items
        .iter()
        .zip(&plain)
        .map(|(item, plain)| {
            let currency = |_: usize, name: &str| column_value(item.rows, plain, name);
            let values = item.rows.row_values(item.row, &currency);
            let names = summary_names(item.entity);
            rule(&item.rows.names(), &values, key, names, &|_| None)
        })
        .collect();
    let has_meta = ctx.rules.owned_meta(table).next().is_some();
    let owners: Vec<Option<&str>> = items
        .iter()
        .zip(&without_meta)
        .map(|(item, summary)| {
            let id = item.rows.text(item.row, ctx.single_key(table)?)??;
            (has_meta && needs_meta(item, summary)).then_some(id)
        })
        .collect();
    let mut ids: Vec<&str> = owners.iter().flatten().copied().collect();
    if ids.is_empty() {
        return without_meta;
    }
    ids.sort_unstable();
    ids.dedup();
    let meta = MetaBatch::load(ctx, table, &ids, &needed_keys(ctx, items)).await;
    items
        .iter()
        .zip(plain)
        .zip(owners)
        .zip(without_meta)
        .map(|(((item, plain), owner), summary)| {
            let Some(owner) = owner else {
                return summary;
            };
            let stored = |name: &str| meta.value(owner, name, &|_, _| None).filter(non_empty);
            let currency = |_: usize, name: &str| {
                column_value(item.rows, &plain, name).or_else(|| stored(name))
            };
            let values = item.rows.row_values(item.row, &currency);
            let names = summary_names(item.entity);
            rule(&item.rows.names(), &values, key, names, &|name| {
                meta.value(owner, name, &currency)
            })
        })
        .collect()
}

/// A currency candidate of the row itself: its non-empty column `name`.
fn column_value(rows: &Rows, plain: &[Value], name: &str) -> Option<Value> {
    rows.column(name)
        .map(|col| &plain[col])
        .filter(|v| non_empty(v))
        .cloned()
}

fn needs_meta(item: &RowRef<'_>, summary: &Summary) -> bool {
    let names = summary_names(item.entity);
    names.iter().any(|name| item.rows.column(name).is_none())
        || summary
            .values
            .iter()
            .any(|v| matches!(v.value.kind, ValueKind::Money { currency: None }))
}

/// The summary names that are no column, and every money currency name that is none.
fn needed_keys(ctx: &Ctx<'_>, items: &[RowRef<'_>]) -> Vec<String> {
    let rows = items[0].rows;
    let summary = items.iter().flat_map(|item| summary_names(item.entity));
    let currencies = ctx.rules.kinds.iter().flat_map(|rule| match &rule.value {
        ValueRule::Money(money) => money.currency.as_slice(),
        _ => &[],
    });
    let mut keys: Vec<String> = Vec::new();
    for name in summary.chain(currencies) {
        if rows.column(name).is_none() && !keys.contains(name) {
            keys.push(name.clone());
        }
    }
    keys
}

/// The meta rows of several owner rows under some keys; the first stored row of a key wins, the
/// meta tables in rule order, then each in key order (as a document reads them).
struct MetaBatch {
    tables: Vec<Rows>,
    /// `(owner id, key)` → `(table, row, value column)`.
    first: HashMap<(String, String), (usize, usize, usize)>,
}

impl MetaBatch {
    async fn load(ctx: &Ctx<'_>, owner: &str, ids: &[&str], keys: &[String]) -> Self {
        let metas: Vec<&MetaRef> = ctx.rules.owned_meta(owner).collect();
        let read = join_all(metas.iter().map(|meta| read_meta(ctx, meta, ids, keys))).await;
        let mut batch = Self {
            tables: Vec::new(),
            first: HashMap::new(),
        };
        for (meta, rows) in metas.into_iter().zip(read) {
            let (Some(rows), Some(fk)) = (rows, meta.fk.as_deref()) else {
                continue;
            };
            let Some(value_col) = rows.column(&meta.value) else {
                continue;
            };
            let table = batch.tables.len();
            for row in 0..rows.len() {
                let (Some(Some(id)), Some(Some(key))) =
                    (rows.text(row, fk), rows.text(row, &meta.key))
                else {
                    continue;
                };
                batch
                    .first
                    .entry((id.to_string(), key.to_string()))
                    .or_insert((table, row, value_col));
            }
            batch.tables.push(rows);
        }
        batch
    }

    fn value(&self, owner: &str, key: &str, currency: CurrencyLookup<'_>) -> Option<Value> {
        let &(table, row, col) = self.first.get(&(owner.to_string(), key.to_string()))?;
        self.tables[table]
            .row_values(row, currency)
            .into_iter()
            .nth(col)
    }
}

async fn read_meta(ctx: &Ctx<'_>, meta: &MetaRef, ids: &[&str], keys: &[String]) -> Option<Rows> {
    let fk = meta.fk.as_ref()?;
    if keys.is_empty() {
        return None;
    }
    let list = |values: &mut dyn Iterator<Item = &str>| {
        values
            .map(|v| ctx.literal(v))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let sql = format!(
        "SELECT * FROM {} WHERE {} IN ({}) AND {} IN ({}) ORDER BY {}",
        ctx.table_ref(&meta.table),
        ctx.ident(fk),
        list(&mut ids.iter().copied()),
        ctx.ident(&meta.key),
        list(&mut keys.iter().map(String::as_str)),
        KeyOrder::primary(ctx, &meta.table).sql(ctx, None),
    );
    match ctx.rows(&meta.table, &sql).await {
        Ok(rows) => Some(rows),
        Err(e) => {
            log::warn!("Summary meta keys of {} not read: {e}", meta.table);
            None
        }
    }
}

/// The entity of each row, as its document picks it: the first whose filter holds. One query
/// tests the filters by key when the table has filtered entities; none without.
pub async fn entities(ctx: &Ctx<'_>, rows: &Rows) -> Vec<Option<AppliedEntity>> {
    let all: Vec<&AppliedEntity> = ctx.rules.entities_of(&rows.table).collect();
    let unfiltered = all
        .iter()
        .find(|e| e.entity.filter.is_none())
        .map(|e| (*e).clone());
    let fallback = vec![unfiltered.clone(); rows.len()];
    if all.iter().all(|e| e.entity.filter.is_none()) || rows.keys.is_empty() {
        return fallback;
    }
    let key = ctx.primary_key(&rows.table);
    let columns: Vec<String> = key.iter().map(|c| ctx.column(Some(ALIAS), c)).collect();
    let sql = format!(
        "SELECT {}{} FROM {} AS {ALIAS} WHERE {}",
        columns.join(", "),
        own::entity_tests(ctx, &all),
        ctx.table_ref(&rows.table),
        keys_condition(ctx, &rows.keys),
    );
    let mut result = match ctx.fetch(&sql).await {
        Ok(result) => result,
        Err(e) => {
            log::debug!("Entities of `{}` not tested: {e}", rows.table);
            return fallback;
        }
    };
    let found = own::take_entities(&mut result, &all);
    let by_key: HashMap<RowKey, Option<AppliedEntity>> = value::row_keys(&result, key)
        .into_iter()
        .zip(found)
        .collect();
    (0..rows.len())
        .map(|row| match rows.key(row).and_then(|k| by_key.get(&k)) {
            Some(entity) => entity.clone(),
            None => unfiltered.clone(),
        })
        .collect()
}

/// The summaries of the rows of `table` with `keys`, read again in full with their entities in
/// one query: rows of a document group may hold only some columns.
pub async fn of_keys(ctx: &Ctx<'_>, table: &str, keys: &[RowKey]) -> HashMap<RowKey, Summary> {
    if keys.is_empty() {
        return HashMap::new();
    }
    let selection = Selection::new(ctx, table, &keys_condition(ctx, keys), "");
    let (rows, entities) = match selection.run(ctx).await {
        Ok(found) => found,
        Err(e) => {
            log::debug!("Summaries of `{table}` not read: {e}");
            return HashMap::new();
        }
    };
    let items: Vec<RowRef<'_>> = (0..rows.len())
        .map(|row| RowRef {
            rows: &rows,
            row,
            entity: entities[row].as_ref(),
        })
        .collect();
    let summaries = of_rows(ctx, &items).await;
    (0..rows.len())
        .zip(summaries)
        .filter_map(|(row, summary)| Some((rows.key(row)?, summary)))
        .collect()
}

/// `(k = …) OR (…)` over the alias `t`, one term per key.
fn keys_condition(ctx: &Ctx<'_>, keys: &[RowKey]) -> String {
    keys.iter()
        .map(|key| {
            let parts: Vec<ColumnEquals> = key
                .0
                .iter()
                .map(|part| ColumnEquals {
                    column: part.column.clone(),
                    value: Some(part.value.clone()),
                })
                .collect();
            format!("({})", aliased_equals_sql(ctx.dialect, ALIAS, &parts))
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{build, peek};
    use crate::query::testing::{Rig, num, result, shop, text};

    const META: &str = "FROM `wp`.`f_ordermeta`";

    const MONEY: &str = r#"
[[entity]]
name = "order"
table = "orders"
summary = ["STATUS", "total", "_amount", "_gone"]

[[kind]]
columns = ["orders.total"]
money = { currency = ["currency", "_currency"] }

[[kind]]
meta_keys = ["_amount"]
in = ["ordermeta"]
money = { currency = ["currency", "_currency"] }
"#;

    fn plain(raw: Cell) -> Value {
        let kind = match &raw {
            Cell::Null => ValueKind::Null,
            _ => ValueKind::Text,
        };
        Value {
            raw,
            kind,
            source: None,
            style: None,
            secondary: None,
        }
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    fn shape(summary: &Summary) -> Vec<(&str, Option<&str>)> {
        summary
            .values
            .iter()
            .map(|v| (v.name.as_str(), v.value.raw.as_text()))
            .collect()
    }

    fn currency_of(value: &Value) -> Option<&str> {
        match &value.kind {
            ValueKind::Money { currency } => currency.as_ref()?.raw.as_text(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_summary_shows_each_name_in_order_from_a_column_a_meta_key_or_as_null() {
        let names = strings(&["id", "Status", "total"]);
        let values = vec![plain(num("1")), plain(text("open")), plain(text("9.00"))];
        let meta = |key: &str| (key == "_note").then(|| plain(text("gift")));

        let summary = rule(
            &names,
            &values,
            &strings(&["id"]),
            &strings(&["_note", "status", "_gone", "total"]),
            &meta,
        );

        assert_eq!(
            shape(&summary),
            [
                ("_note", Some("gift")),
                ("Status", Some("open")),
                ("_gone", None),
                ("total", Some("9.00")),
            ]
        );
        assert_eq!(summary.values[2].value.kind, ValueKind::Null);
    }

    #[test]
    fn a_summary_name_that_is_a_column_never_reads_meta() {
        let names = strings(&["id", "status"]);
        let values = vec![plain(num("1")), plain(Cell::Null)];
        let meta = |_: &str| -> Option<Value> { panic!("status is a column") };

        let summary = rule(&names, &values, &[], &strings(&["status"]), &meta);

        assert_eq!(shape(&summary), [("status", None)]);
    }

    #[test]
    fn without_a_summary_four_filled_columns_outside_the_key_show_in_schema_order() {
        let names = strings(&["id", "a", "b", "c", "d", "e", "f", "g"]);
        let values = vec![
            plain(num("1")),
            plain(text("1")),
            plain(Cell::Null),
            plain(text("")),
            plain(text("2")),
            plain(text("3")),
            plain(text("4")),
            plain(text("5")),
        ];

        let summary = rule(&names, &values, &strings(&["ID"]), &[], &|_| None);

        assert_eq!(
            shape(&summary),
            [
                ("a", Some("1")),
                ("d", Some("2")),
                ("e", Some("3")),
                ("f", Some("4"))
            ]
        );
    }

    fn order_row(id: &str, currency: &str) -> Vec<Cell> {
        vec![
            num(id),
            text("open"),
            text("10.00"),
            text(currency),
            text("2023-11-14"),
            num("1"),
        ]
    }

    fn order_rows(rows: Vec<Vec<Cell>>) -> crate::db::ResultSet {
        result(
            &[
                "id",
                "status",
                "total",
                "currency",
                "created_gmt",
                "__databee_entity_0",
            ],
            rows,
        )
    }

    fn meta_rows(rows: &[(&str, &str, &str, &str)]) -> crate::db::ResultSet {
        let rows = rows
            .iter()
            .map(|(id, owner, k, v)| vec![num(id), num(owner), text(k), text(v)])
            .collect();
        result(&["meta_id", "order_id", "k", "v"], rows)
    }

    async fn summaries_of(rig: &Rig, ids: &[&str]) -> Vec<Summary> {
        let ctx = rig.ctx().await;
        let condition: Vec<String> = ids.iter().map(|id| format!("t.`id` = '{id}'")).collect();
        let (rows, entities) = Selection::new(&ctx, "f_orders", &condition.join(" OR "), "")
            .run(&ctx)
            .await
            .unwrap();
        let items: Vec<RowRef<'_>> = (0..rows.len())
            .map(|row| RowRef {
                rows: &rows,
                row,
                entity: entities[row].as_ref(),
            })
            .collect();
        of_rows(&ctx, &items).await
    }

    #[tokio::test]
    async fn money_takes_its_currency_from_the_row_else_from_a_meta_key() {
        let rig = Rig::new(
            shop()
                .on(
                    META,
                    meta_rows(&[
                        ("1", "1", "_amount", "5.00"),
                        ("2", "2", "_amount", "6.00"),
                        ("3", "2", "_currency", "USD"),
                    ]),
                )
                .on(
                    "AS t WHERE",
                    order_rows(vec![order_row("1", "EUR"), order_row("2", "")]),
                ),
            MONEY,
        );

        let summaries = summaries_of(&rig, &["1", "2"]).await;

        let currencies: Vec<(Option<&str>, Option<&str>)> = summaries
            .iter()
            .map(|s| {
                (
                    currency_of(&s.values[1].value),
                    currency_of(&s.values[2].value),
                )
            })
            .collect();
        assert_eq!(
            currencies,
            [(Some("EUR"), Some("EUR")), (Some("USD"), Some("USD"))]
        );
        assert_eq!(
            shape(&summaries[1]),
            [
                ("status", Some("open")),
                ("total", Some("10.00")),
                ("_amount", Some("6.00")),
                ("_gone", None),
            ]
        );
    }

    #[tokio::test]
    async fn the_meta_keys_of_many_rows_are_read_in_one_query() {
        let rig = Rig::new(
            shop()
                .on(META, meta_rows(&[("1", "1", "_amount", "5.00")]))
                .on(
                    "AS t WHERE",
                    order_rows(vec![order_row("1", "EUR"), order_row("2", "EUR")]),
                ),
            MONEY,
        );

        summaries_of(&rig, &["1", "2"]).await;

        let queries = rig.db.queries();
        assert_eq!(queries.len(), 2, "{queries:?}");
        assert_eq!(
            queries[1],
            "SELECT * FROM `wp`.`f_ordermeta` WHERE `order_id` IN ('1', '2') \
             AND `k` IN ('_amount', '_gone', '_currency') ORDER BY `meta_id`"
        );
    }

    #[tokio::test]
    async fn rows_whose_summary_and_money_need_no_meta_read_none() {
        let rules = MONEY.replace(
            r#"["STATUS", "total", "_amount", "_gone"]"#,
            r#"["status", "total"]"#,
        );
        let rig = Rig::new(
            shop().on("AS t WHERE", order_rows(vec![order_row("1", "EUR")])),
            &rules,
        );

        let summaries = summaries_of(&rig, &["1"]).await;

        assert_eq!(rig.db.queries().len(), 1);
        assert_eq!(currency_of(&summaries[0].values[1].value), Some("EUR"));
    }

    const FILTERED: &str = r#"
[[entity]]
name = "paid"
table = "orders"
filter = "status = 'paid'"
summary = ["total"]

[[entity]]
name = "order"
table = "orders"
summary = ["status"]
"#;

    fn found_orders() -> crate::db::ResultSet {
        result(
            &["id", "status", "total"],
            vec![
                vec![num("1"), text("paid"), text("5.00")],
                vec![num("2"), text("open"), text("6.00")],
            ],
        )
    }

    #[tokio::test]
    async fn each_found_row_takes_the_entity_its_document_shows() {
        let tests = result(
            &["id", "__databee_entity_0", "__databee_entity_1"],
            vec![
                vec![num("1"), num("1"), num("1")],
                vec![num("2"), num("0"), num("1")],
            ],
        );
        let rig = Rig::new(shop().on("__databee_entity_0", tests), FILTERED);
        let ctx = rig.ctx().await;
        let rows = ctx.annotate("f_orders", found_orders());

        let found = entities(&ctx, &rows).await;

        let names: Vec<Option<&str>> = found
            .iter()
            .map(|e| e.as_ref().map(|e| e.entity.name.as_str()))
            .collect();
        assert_eq!(names, [Some("paid"), Some("order")]);
        assert_eq!(
            rig.db.queries(),
            [
                "SELECT t.`id`, (status = 'paid') AS `__databee_entity_0`, 1 AS `__databee_entity_1` \
                 FROM `wp`.`f_orders` AS t WHERE (t.`id` = '1') OR (t.`id` = '2')"
            ]
        );
    }

    #[tokio::test]
    async fn a_table_without_filtered_entities_needs_no_entity_query() {
        let rules = FILTERED.replace("filter = \"status = 'paid'\"\n", "");
        let rig = Rig::new(shop(), &rules);
        let ctx = rig.ctx().await;
        let rows = ctx.annotate("f_orders", found_orders());

        let found = entities(&ctx, &rows).await;

        assert!(
            found
                .iter()
                .all(|e| e.as_ref().unwrap().entity.name == "paid")
        );
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn the_document_header_and_the_peek_of_a_row_show_the_same_summary() {
        let order = order_rows(vec![order_row("2", "")]);
        let meta = meta_rows(&[
            ("3", "2", "_currency", "USD"),
            ("4", "2", "_amount", "6.00"),
        ]);
        let db = || {
            shop()
                .on(META, meta.clone())
                .on("AS t WHERE", order.clone())
        };
        let (one, two) = (Rig::new(db(), MONEY), Rig::new(db(), MONEY));
        let (ctx, other) = (one.ctx().await, two.ctx().await);
        let address = ctx.row_address("f_orders", "2");

        let document = build::document(&ctx, &address).await.unwrap();
        let peek = peek::peek(&other, &address).await.unwrap();

        assert_eq!(document.header, peek.summary);
        assert_eq!(currency_of(&peek.summary.values[2].value), Some("USD"));
    }
}
