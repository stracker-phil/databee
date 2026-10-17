//! Entity shorthand: `order 4711` names the row with key 4711 of a table that defines the entity
//! `order`. A table only for the entity (no `filter`) is tried before filtered ones; when several
//! hold the key, the first wins and the others are alternatives.

use futures_util::future::join_all;

use super::context::Ctx;
use crate::address::Address;
use crate::error::{AppError, AppResult};
use crate::rules::effective::AppliedEntity;

pub struct Named {
    pub address: Address,
    pub alternatives: Vec<Address>,
}

/// `<entity> <key>`, or `None` when `text` is not of that shape.
pub fn parse(text: &str) -> Option<(&str, &str)> {
    let (entity, key) = text.trim().split_once(char::is_whitespace)?;
    let key = key.trim();
    (!entity.is_empty() && !key.is_empty()).then_some((entity, key))
}

pub async fn resolve(ctx: &Ctx<'_>, entity: &str, key: &str) -> AppResult<Named> {
    let mut candidates: Vec<&AppliedEntity> = ctx
        .rules
        .entities
        .iter()
        .filter(|e| e.entity.name == entity)
        .collect();
    if candidates.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "No entity `{entity}` in this database"
        )));
    }
    candidates.sort_by_key(|e| e.entity.filter.is_some());
    let held = join_all(candidates.iter().map(|e| holds(ctx, e, key))).await;
    let mut found = candidates
        .iter()
        .zip(held)
        .filter(|(_, held)| *held)
        .map(|(e, _)| ctx.row_address(&e.entity.table, key));
    let Some(address) = found.next() else {
        return Err(AppError::InvalidInput(format!("No {entity} {key}")));
    };
    Ok(Named {
        address,
        alternatives: found.collect(),
    })
}

/// Whether `entity`'s table has a row with `key` on which its filter holds.
async fn holds(ctx: &Ctx<'_>, entity: &AppliedEntity, key: &str) -> bool {
    let table = &entity.entity.table;
    let Some(column) = ctx.single_key(table) else {
        return false;
    };
    let filter = entity
        .entity
        .filter
        .as_ref()
        .map(|f| format!(" AND ({})", ctx.preset_sql(f)))
        .unwrap_or_default();
    let sql = format!(
        "SELECT 1 FROM {} WHERE {} = {}{filter} LIMIT 1",
        ctx.table_ref(table),
        ctx.ident(column),
        ctx.literal(key)
    );
    match ctx.fetch(&sql).await {
        Ok(result) => !result.rows.is_empty(),
        Err(e) => {
            log::warn!("{} {key} not looked up in {table}: {e}", entity.entity.name);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ResultSet, TableStructure};
    use crate::document::testing::{Rig, num, result, shop};

    const FILTERED_ORDER: &str = r#"
[[entity]]
name = "order"
table = "orders"
filter = "status = 'open'"
"#;

    const UNFILTERED_ORDER_IN_LOG: &str = r#"
[[entity]]
name = "order"
table = "log"
"#;

    fn found() -> ResultSet {
        result(&["1"], vec![vec![num("1")]])
    }

    fn missing() -> ResultSet {
        result(&["1"], Vec::new())
    }

    fn message(err: AppError) -> String {
        match err {
            AppError::InvalidInput(message) => message,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn an_entity_no_table_defines_is_invalid_input() {
        let rig = Rig::new(shop(), FILTERED_ORDER);
        let ctx = rig.ctx().await;

        let err = resolve(&ctx, "x", "1").await.err().unwrap();

        assert_eq!(message(err), "No entity `x` in this database");
        assert_eq!(rig.db.queries(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_key_no_candidate_holds_is_named_in_the_error() {
        let rig = Rig::new(shop().on("SELECT 1 FROM", missing()), FILTERED_ORDER);
        let ctx = rig.ctx().await;

        let err = resolve(&ctx, "order", "999").await.err().unwrap();

        assert_eq!(message(err), "No order 999");
        assert_eq!(
            rig.db.queries(),
            ["SELECT 1 FROM `wp`.`f_orders` WHERE `id` = '999' AND (status = 'open') LIMIT 1"]
        );
    }

    #[tokio::test]
    async fn the_unfiltered_table_is_the_address_whatever_the_rule_order() {
        let orders = [
            format!("{FILTERED_ORDER}{UNFILTERED_ORDER_IN_LOG}"),
            format!("{UNFILTERED_ORDER_IN_LOG}{FILTERED_ORDER}"),
        ];
        for rules in orders {
            let db = shop().on("`f_orders`", found()).on("`f_log`", found());
            let rig = Rig::new(db, &rules);
            let ctx = rig.ctx().await;

            let named = resolve(&ctx, "order", "1").await.ok().unwrap();

            assert_eq!(named.address, ctx.row_address("f_log", "1"), "{rules}");
            assert_eq!(
                named.alternatives,
                [ctx.row_address("f_orders", "1")],
                "{rules}"
            );
        }
    }

    #[tokio::test]
    async fn a_candidate_without_a_single_column_key_is_skipped_unqueried() {
        let pairs = TableStructure {
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: vec!["a".into(), "b".into()],
            foreign_keys: Vec::new(),
            ddl: None,
        };
        let rules = r#"
[[entity]]
name = "order"
table = "pairs"

[[entity]]
name = "order"
table = "orders"
"#;
        let rig = Rig::new(
            shop()
                .with_table("wp", "f_pairs", pairs)
                .on("`f_orders`", found()),
            rules,
        );
        let ctx = rig.ctx().await;

        let named = resolve(&ctx, "order", "1").await.ok().unwrap();

        assert_eq!(named.address, ctx.row_address("f_orders", "1"));
        assert!(named.alternatives.is_empty());
        assert_eq!(
            rig.db.queries(),
            ["SELECT 1 FROM `wp`.`f_orders` WHERE `id` = '1' LIMIT 1"]
        );
    }
}
