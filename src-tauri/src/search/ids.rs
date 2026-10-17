//! `#<id>`: the rows with that key across the entity tables, and the rows that point at them.

use futures_util::future::join_all;

use super::hits::{self, Place};
use super::{PLAIN_TABLE_ROWS, PaletteHit, SECTION_CAP};
use crate::document::{Ctx, Own, linked, peek};

/// Found rows whose referencing rows are read.
const LINKED_ROWS: usize = 5;

pub async fn search(ctx: &Ctx<'_>, id: &str) -> Vec<PaletteHit> {
    let tables = tables(ctx);
    let loaded = join_all(tables.iter().map(|table| linked::row(ctx, table, id))).await;
    let found: Vec<Own> = tables
        .iter()
        .zip(loaded)
        .filter_map(|(table, loaded)| {
            loaded
                .map_err(|e| log::debug!("`#{id}` skipped `{table}`: {e}"))
                .ok()
        })
        .collect();
    let section = format!("objects with id {id}");
    let objects = join_all(
        found
            .iter()
            .take(SECTION_CAP)
            .map(|own| object(ctx, own, &section)),
    );
    let mut out: Vec<PaletteHit> = objects.await.into_iter().flatten().collect();
    let groups = join_all(
        found
            .iter()
            .take(LINKED_ROWS)
            .map(|own| linked::referencing(ctx, own)),
    )
    .await;
    let groups: Vec<_> = groups.into_iter().flatten().collect();
    out.extend(hits::from_groups(
        ctx,
        &groups,
        &format!("rows that reference {id}"),
    ));
    out
}

/// The entity tables with a single-column key, in rule order; without entities, the small ones.
fn tables(ctx: &Ctx<'_>) -> Vec<String> {
    let mut tables: Vec<String> = Vec::new();
    if ctx.rules.entities.is_empty() {
        tables.extend(
            ctx.schema
                .tables
                .iter()
                .filter(|t| !t.is_view && t.rows_estimate.unwrap_or(0) < PLAIN_TABLE_ROWS)
                .map(|t| t.name.clone()),
        );
    } else {
        for entity in &ctx.rules.entities {
            if !tables.contains(&entity.entity.table) {
                tables.push(entity.entity.table.clone());
            }
        }
    }
    tables.retain(|t| ctx.single_key(t).is_some());
    tables
}

async fn object(ctx: &Ctx<'_>, own: &Own, section: &str) -> Option<PaletteHit> {
    let key = own.rows.key(0)?;
    let summary = peek::values(ctx, own).await;
    let fields = own.rows.values(&|_, _| None).into_iter().next()?;
    let fields: Vec<_> = own.rows.names().into_iter().zip(fields).collect();
    let place = Place {
        section,
        source: own.table().to_string(),
        table: own.table(),
        skip: &[],
        summary: &[],
        matched: None,
    };
    let mut hit = hits::row(ctx, &place, &key, &fields);
    hit.detail = summary.into_iter().map(|(_, value)| value).collect();
    Some(hit)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::TableInfo;
    use crate::document::testing::{Rig, shop};
    use crate::schema::DatabaseSchema;

    /// The tables `#` reads in the shop database without entities, after `adjust` changed what
    /// the schema says about them.
    async fn read_without_entities(adjust: fn(&mut DatabaseSchema)) -> Vec<String> {
        let rig = Rig::new(shop(), "");
        let mut ctx = rig.ctx().await;
        let mut schema = (*ctx.schema).clone();
        adjust(&mut schema);
        ctx.schema = Arc::new(schema);
        let mut tables = tables(&ctx);
        tables.sort();
        tables
    }

    fn table_mut<'s>(schema: &'s mut DatabaseSchema, name: &str) -> &'s mut TableInfo {
        schema.tables.iter_mut().find(|t| t.name == name).unwrap()
    }

    #[tokio::test]
    async fn without_entities_every_table_with_a_single_column_key_is_read() {
        let read = read_without_entities(|_| {}).await;

        assert_eq!(read, ["f_log", "f_ordermeta", "f_orders", "f_people"]);
    }

    #[tokio::test]
    async fn without_entities_views_are_not_read() {
        let read =
            read_without_entities(|schema| table_mut(schema, "f_orders").is_view = true).await;

        assert_eq!(read, ["f_log", "f_ordermeta", "f_people"]);
    }

    #[tokio::test]
    async fn without_entities_tables_estimated_at_a_million_rows_are_not_read() {
        let read = read_without_entities(|schema| {
            table_mut(schema, "f_log").rows_estimate = Some(PLAIN_TABLE_ROWS);
            table_mut(schema, "f_people").rows_estimate = Some(PLAIN_TABLE_ROWS - 1);
            table_mut(schema, "f_orders").rows_estimate = Some(u64::MAX);
        })
        .await;

        assert_eq!(read, ["f_ordermeta", "f_people"]);
    }

    #[tokio::test]
    async fn without_entities_tables_with_a_composite_key_are_not_read() {
        let read = read_without_entities(|schema| {
            let keys = schema.keys.get_mut("f_ordermeta").unwrap();
            keys.primary_key = vec!["order_id".into(), "k".into()];
        })
        .await;

        assert_eq!(read, ["f_log", "f_orders", "f_people"]);
    }
}
