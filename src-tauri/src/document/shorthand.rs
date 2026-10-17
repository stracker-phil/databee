//! Entity shorthand: `order 4711` names the row with key 4711 of a table that defines the entity
//! `order`. A table only for the entity (no `filter`) is tried before filtered ones; when several
//! hold the key, the first wins and the others are alternatives.

use futures_util::future::join_all;

use super::context::Ctx;
use super::own::entity_filter;
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
        .map(|f| format!(" AND ({})", entity_filter(ctx, f)))
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
