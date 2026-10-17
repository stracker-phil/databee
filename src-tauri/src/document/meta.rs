//! Meta rows: the rows of each `[[meta]]` table whose `fk` holds the row's key, in stored order
//! (the meta table's primary key), so write order stays visible.

use std::collections::HashMap;

use super::context::{Capped, Ctx, CurrencyLookup, Rows};
use super::own::Own;
use super::{Group, GroupLink, GroupRow, GroupShape, META_CAP};
use crate::address::Target;
use crate::db::ColumnEquals;
use crate::error::AppResult;
use crate::rules::effective::MetaRef;
use crate::value::Value;

pub struct MetaRows {
    pub meta: MetaRef,
    pub fk: String,
    pub id: String,
    pub loaded: AppResult<Capped>,
    /// Rows of `needed` keys beyond the cap, so header, patterns and currencies still find them.
    pub beyond_cap: Option<Rows>,
}

pub async fn load(ctx: &Ctx<'_>, own: &Own, needed: &[String]) -> Vec<MetaRows> {
    let Some(id) = own.id(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for meta in ctx
        .rules
        .meta
        .iter()
        .filter(|m| m.owner.as_deref() == Some(own.table()))
    {
        let Some(fk) = meta.fk.clone() else {
            continue;
        };
        let filter = format!("{} = {}", ctx.ident(&fk), ctx.literal(id));
        let order = ctx.key_order(&meta.table, None);
        let loaded = ctx
            .capped(&meta.table, None, &filter, &order, META_CAP)
            .await;
        let beyond_cap = match &loaded {
            Ok(capped) if capped.capped && !needed.is_empty() => {
                needed_rows(ctx, meta, &filter, &order, needed).await
            }
            _ => None,
        };
        out.push(MetaRows {
            meta: meta.clone(),
            fk,
            id: id.to_string(),
            loaded,
            beyond_cap,
        });
    }
    out
}

async fn needed_rows(
    ctx: &Ctx<'_>,
    meta: &MetaRef,
    filter: &str,
    order: &str,
    needed: &[String],
) -> Option<Rows> {
    let keys: Vec<String> = needed.iter().map(|k| ctx.literal(k)).collect();
    let sql = format!(
        "SELECT * FROM {} WHERE {filter} AND {} IN ({}) ORDER BY {order}",
        ctx.table_ref(&meta.table),
        ctx.ident(&meta.key),
        keys.join(", ")
    );
    match ctx.rows(&meta.table, &sql).await {
        Ok(rows) => Some(rows),
        Err(e) => {
            log::warn!("Meta keys of {} beyond the cap not read: {e}", meta.table);
            None
        }
    }
}

/// The first value stored under each key, over all meta tables of the row.
#[derive(Default)]
pub struct MetaValues(HashMap<String, Value>);

impl MetaValues {
    pub fn new(meta: &[MetaRows]) -> Self {
        let mut values = Self::default();
        for rows in meta {
            let loaded = rows.loaded.as_ref().ok().map(|c| &c.rows);
            for found in loaded.into_iter().chain(&rows.beyond_cap) {
                values.add(&rows.meta, found);
            }
        }
        values
    }

    fn add(&mut self, meta: &MetaRef, rows: &Rows) {
        let Some(value_col) = rows.column(&meta.value) else {
            return;
        };
        let no_currency = |_: usize, _: &str| None;
        let values = rows.values(&no_currency);
        for (row, cells) in values.into_iter().enumerate() {
            let Some(Some(key)) = rows.text(row, &meta.key) else {
                continue;
            };
            if !self.0.contains_key(key) {
                self.0.insert(key.to_string(), cells[value_col].clone());
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
}

/// The stored key and value of a meta group row.
pub fn key_value(row: &GroupRow) -> Option<(&str, &Value)> {
    let [(_, key), (_, value)] = row.fields.as_slice() else {
        return None;
    };
    Some((key.raw.as_text()?, value))
}

pub fn group(ctx: &Ctx<'_>, rows: &MetaRows, currency: CurrencyLookup<'_>) -> Group {
    let meta = &rows.meta;
    let via = GroupLink::Meta {
        fk: rows.fk.clone(),
        key: meta.key.clone(),
        value: meta.value.clone(),
    };
    let mut group = Group::new(meta.table.clone(), &meta.table, via, GroupShape::Meta);
    group.columns = vec![meta.key.clone(), meta.value.clone()];
    group.rows_address = Some(ctx.address(Target::Table {
        table: meta.table.clone(),
        filter: vec![ColumnEquals {
            column: rows.fk.clone(),
            value: Some(rows.id.clone()),
        }],
    }));
    let capped = match &rows.loaded {
        Ok(capped) => capped,
        Err(e) => return group.failed(e),
    };
    let found = &capped.rows;
    let (Some(key_col), Some(value_col)) = (found.column(&meta.key), found.column(&meta.value))
    else {
        return group.failed(format!(
            "{} has no {} or {} column",
            meta.table, meta.key, meta.value
        ));
    };
    group.total = capped.total;
    group.capped = capped.capped;
    group.rows = found
        .values(currency)
        .into_iter()
        .enumerate()
        .map(|(row, cells)| GroupRow {
            key: found.key(row),
            fields: vec![
                (meta.key.clone(), cells[key_col].clone()),
                (meta.value.clone(), cells[value_col].clone()),
            ],
        })
        .collect();
    group
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ResultSet};
    use crate::document::build;
    use crate::document::context::testing::{Rig, count, num, order, result, shop, text};
    use crate::error::AppError;
    use crate::value::{KeyPart, RowKey, ValueKind};

    const ROWS: &str = "FROM `wp`.`f_ordermeta`";
    const FIRST_PAGE: &str =
        "SELECT * FROM `wp`.`f_ordermeta` WHERE `order_id` = '1' ORDER BY `meta_id` LIMIT 51";
    const COUNT: &str = "SELECT COUNT(*) FROM `wp`.`f_ordermeta` WHERE `order_id` = '1'";

    fn meta_rows(rows: &[(u32, &str, &str)]) -> ResultSet {
        let rows = rows
            .iter()
            .map(|(id, key, value)| vec![num(&id.to_string()), num("1"), text(key), text(value)])
            .collect();
        result(&["meta_id", "order_id", "k", "v"], rows)
    }

    fn crowded() -> ResultSet {
        let rows: Vec<(u32, String)> = (1..=51).map(|i| (i, format!("k{i}"))).collect();
        let rows: Vec<(u32, &str, &str)> =
            rows.iter().map(|(i, k)| (*i, k.as_str(), "x")).collect();
        meta_rows(&rows)
    }

    fn row_key(id: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: "meta_id".into(),
            value: id.into(),
        }])
    }

    #[tokio::test]
    async fn meta_rows_of_the_owner_are_read_by_fk_in_key_order() {
        let rig = Rig::new(
            shop().on(ROWS, meta_rows(&[(1, "_a", "x"), (2, "_b", "y")])),
            "",
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let loaded = load(&ctx, &own, &[]).await;

        assert_eq!(rig.db.queries(), [FIRST_PAGE]);
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            (loaded[0].fk.as_str(), loaded[0].id.as_str()),
            ("order_id", "1")
        );
        let capped = loaded[0].loaded.as_ref().unwrap();
        assert_eq!(
            (capped.rows.len(), capped.total, capped.capped),
            (2, 2, false)
        );
        assert!(loaded[0].beyond_cap.is_none());
    }

    #[tokio::test]
    async fn a_row_without_a_key_value_reads_no_meta() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;
        let own = Own {
            rows: ctx.annotate("f_orders", result(&["id"], vec![vec![Cell::Null]])),
            entity: None,
        };

        assert!(load(&ctx, &own, &[]).await.is_empty());
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn only_the_meta_tables_of_the_rows_table_are_read() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;
        let person = Own {
            rows: ctx.annotate("f_people", result(&["person_id"], vec![vec![num("1")]])),
            entity: None,
        };

        assert!(load(&ctx, &person, &["_a".into()]).await.is_empty());
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn beyond_the_cap_the_needed_keys_are_read_in_a_second_query() {
        let rig = Rig::new(
            shop()
                .on("AND `k` IN", meta_rows(&[(99, "_currency", "EUR")]))
                .on("SELECT COUNT(*)", count(80))
                .on(ROWS, crowded()),
            "",
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));
        let needed = ["_currency".to_string(), "_paid".to_string()];

        let loaded = load(&ctx, &own, &needed).await;

        assert_eq!(
            rig.db.queries(),
            [
                FIRST_PAGE,
                COUNT,
                "SELECT * FROM `wp`.`f_ordermeta` WHERE `order_id` = '1' AND `k` IN ('_currency', '_paid') ORDER BY `meta_id`",
            ]
        );
        let capped = loaded[0].loaded.as_ref().unwrap();
        assert_eq!(
            (capped.rows.len(), capped.total, capped.capped),
            (50, 80, true)
        );
        assert_eq!(loaded[0].beyond_cap.as_ref().map(Rows::len), Some(1));
        let values = MetaValues::new(&loaded);
        assert_eq!(values.get("_currency").unwrap().raw, text("EUR"));
        assert_eq!(values.get("k1").unwrap().raw, text("x"));
        assert!(values.get("_paid").is_none());
    }

    #[tokio::test]
    async fn beyond_the_cap_without_needed_keys_only_counts() {
        let rig = Rig::new(
            shop().on("SELECT COUNT(*)", count(80)).on(ROWS, crowded()),
            "",
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let loaded = load(&ctx, &own, &[]).await;

        assert_eq!(rig.db.queries(), [FIRST_PAGE, COUNT]);
        assert!(loaded[0].beyond_cap.is_none());
    }

    #[tokio::test]
    async fn within_the_cap_the_needed_keys_need_no_second_query() {
        let rig = Rig::new(shop().on(ROWS, meta_rows(&[(1, "_currency", "EUR")])), "");
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let loaded = load(&ctx, &own, &["_currency".into()]).await;

        assert_eq!(rig.db.queries(), [FIRST_PAGE]);
        assert!(loaded[0].beyond_cap.is_none());
        assert_eq!(
            MetaValues::new(&loaded).get("_currency").unwrap().raw,
            text("EUR")
        );
    }

    #[tokio::test]
    async fn a_failed_needed_keys_query_keeps_the_page_that_was_read() {
        let rig = Rig::new(
            shop()
                .failing("AND `k` IN", AppError::Cancelled)
                .on("SELECT COUNT(*)", count(80))
                .on(ROWS, crowded()),
            "",
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let loaded = load(&ctx, &own, &["_currency".into()]).await;

        assert!(loaded[0].loaded.is_ok());
        assert!(loaded[0].beyond_cap.is_none());
    }

    #[tokio::test]
    async fn the_first_value_stored_under_a_key_wins() {
        let rig = Rig::new(
            shop().on(
                ROWS,
                meta_rows(&[(1, "_a", "first"), (2, "_a", "second"), (3, "_b", "other")]),
            ),
            "",
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let values = MetaValues::new(&load(&ctx, &own, &[]).await);

        assert_eq!(values.get("_a").unwrap().raw, text("first"));
        assert_eq!(values.get("_b").unwrap().raw, text("other"));
        assert!(values.get("_c").is_none());
    }

    #[tokio::test]
    async fn a_failed_meta_read_contributes_no_values() {
        let rig = Rig::new(shop().failing(ROWS, AppError::Cancelled), "");
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let values = MetaValues::new(&load(&ctx, &own, &["_a".into()]).await);

        assert!(values.get("_a").is_none());
    }

    #[tokio::test]
    async fn the_group_shows_key_and_value_rows_and_addresses_them_by_the_fk() {
        let rig = Rig::new(
            shop().on(ROWS, meta_rows(&[(10, "_a", "x"), (11, "_b", "y")])),
            "",
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));
        let loaded = load(&ctx, &own, &[]).await;

        let group = group(&ctx, &loaded[0], &|_, _| None);

        assert_eq!(group.id, "f_ordermeta");
        assert_eq!(group.table, "f_ordermeta");
        assert_eq!(
            group.via,
            GroupLink::Meta {
                fk: "order_id".into(),
                key: "k".into(),
                value: "v".into()
            }
        );
        assert_eq!(group.shape, GroupShape::Meta);
        assert_eq!(group.columns, ["k", "v"]);
        assert_eq!((group.total, group.capped, group.error), (2, false, None));
        let keys: Vec<Option<RowKey>> = group.rows.iter().map(|r| r.key.clone()).collect();
        assert_eq!(keys, [Some(row_key("10")), Some(row_key("11"))]);
        let fields: Vec<Vec<&str>> = group
            .rows
            .iter()
            .map(|r| {
                r.fields
                    .iter()
                    .map(|(_, v)| v.raw.as_text().unwrap())
                    .collect()
            })
            .collect();
        assert_eq!(fields, [["_a", "x"], ["_b", "y"]]);
        assert_eq!(
            group.rows_address,
            Some(ctx.address(Target::Table {
                table: "f_ordermeta".into(),
                filter: vec![ColumnEquals {
                    column: "order_id".into(),
                    value: Some("1".into())
                }],
            }))
        );
    }

    #[tokio::test]
    async fn the_group_of_a_failed_read_carries_the_error_and_still_links_its_rows() {
        let rig = Rig::new(shop().failing(ROWS, AppError::Cancelled), "");
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));
        let loaded = load(&ctx, &own, &[]).await;

        let group = group(&ctx, &loaded[0], &|_, _| None);

        assert!(group.error.is_some());
        assert!(group.rows.is_empty());
        assert!(group.rows_address.is_some());
    }

    #[tokio::test]
    async fn the_group_of_a_table_without_the_key_or_value_column_fails_by_name() {
        let without_value = result(&["meta_id", "order_id", "k"], Vec::new());
        let rig = Rig::new(shop().on(ROWS, without_value), "");
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));
        let loaded = load(&ctx, &own, &[]).await;

        let group = group(&ctx, &loaded[0], &|_, _| None);

        assert_eq!(
            group.error.as_deref(),
            Some("f_ordermeta has no k or v column")
        );
    }

    const MONEY: &str = r#"
[[kind]]
meta_keys = ["_amount"]
in = ["ordermeta"]
money = { currency = ["currency", "_currency"] }
"#;

    /// The currency of the `_amount` meta value of order 1, whose row has `own_currency`.
    async fn amount_currency(
        own_currency: Cell,
        meta: &[(u32, &str, &str)],
    ) -> Option<crate::value::Value> {
        let columns = ["id", "status", "total", "currency", "created_gmt"];
        let order_row = vec![
            num("1"),
            text("open"),
            text("10.00"),
            own_currency,
            Cell::Temporal("2023-11-14 22:13:20".into()),
        ];
        let rig = Rig::new(
            shop()
                .on("AS t WHERE", result(&columns, vec![order_row]))
                .on(ROWS, meta_rows(meta)),
            MONEY,
        );
        let ctx = rig.ctx().await;
        let address = ctx.row_address("f_orders", "1");

        let document = build::document(&ctx, &address).await.unwrap();

        let group = document
            .groups
            .iter()
            .find(|g| matches!(g.via, GroupLink::Meta { .. }))
            .unwrap();
        let amount = group
            .rows
            .iter()
            .find(|r| r.fields[0].1.raw.as_text() == Some("_amount"))
            .unwrap();
        match &amount.fields[1].1.kind {
            ValueKind::Money { currency } => currency.as_deref().cloned(),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_meta_amount_takes_its_currency_from_a_column_of_the_owner_row() {
        let currency = amount_currency(
            text("EUR"),
            &[(1, "_amount", "5.00"), (2, "_currency", "USD")],
        )
        .await
        .unwrap();

        assert_eq!(currency.raw, text("EUR"));
        let source = currency.source.unwrap();
        assert_eq!(
            (source.table.as_str(), source.column.as_str()),
            ("f_orders", "currency")
        );
    }

    #[tokio::test]
    async fn a_meta_amount_falls_back_to_a_sibling_meta_key_when_the_owner_has_no_currency() {
        let currency =
            amount_currency(text(""), &[(1, "_amount", "5.00"), (2, "_currency", "USD")])
                .await
                .unwrap();

        assert_eq!(currency.raw, text("USD"));
        let source = currency.source.unwrap();
        assert_eq!(
            (source.table.as_str(), source.column.as_str()),
            ("f_ordermeta", "v")
        );
        assert_eq!(source.row, row_key("2"));
    }

    #[tokio::test]
    async fn a_meta_amount_without_any_currency_has_none() {
        assert_eq!(
            amount_currency(Cell::Null, &[(1, "_amount", "5.00")]).await,
            None
        );
    }
}
