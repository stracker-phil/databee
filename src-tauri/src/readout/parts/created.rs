use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};

use futures_util::future::join_all;

use crate::document::summary::{self, RowRef};
use crate::document::{Selection, Summary, moment};
use crate::query::{Ctx, Rows};
use crate::readout::{CREATED_ROWS, CreatedEntity, CreatedItem, RecentlyCreated};
use crate::rules::effective::AppliedEntity;
use crate::value::{RowKey, Value};

/// The newest `CREATED_ROWS` rows over every entity with a `created` column, each row once: one
/// query per entity (its newest rows, where its `filter` holds), then the summaries of the rows
/// that made it, one batch per table.
pub async fn recently_created(ctx: &Ctx<'_>) -> RecentlyCreated {
    let sources: Vec<(&AppliedEntity, &str, Selection<'_>)> = ctx
        .rules
        .entities
        .iter()
        .filter_map(|applied| {
            let created = applied.entity.created.as_deref()?;
            Some((applied, created, newest(ctx, applied, created)))
        })
        .collect();
    let read = join_all(sources.iter().map(|(_, _, selection)| selection.run(ctx))).await;
    let mut candidates: Vec<Candidate<'_>> = Vec::new();
    for ((applied, created, _), result) in sources.iter().zip(&read) {
        if let Ok((rows, entities)) = result {
            candidates.extend(candidates_of(rows, entities, applied, created));
        }
    }
    let window = window(candidates);
    let summaries = window_summaries(ctx, &window).await;
    let items: Vec<CreatedItem> = window
        .into_iter()
        .zip(summaries)
        .map(|(candidate, summary)| CreatedItem {
            entity: candidate.entity.entity.name.clone(),
            table: candidate.rows.table.clone(),
            address: ctx.key_address(&candidate.rows.table, &candidate.key),
            created: candidate.created,
            summary,
        })
        .collect();
    let entities = sources
        .iter()
        .zip(&read)
        .map(|((applied, _, selection), result)| {
            let entity = &applied.entity;
            let in_window = items
                .iter()
                .filter(|i| i.entity == entity.name && i.table == entity.table)
                .count();
            CreatedEntity {
                entity: entity.name.clone(),
                preset: applied.preset.clone(),
                table: entity.table.clone(),
                sql: selection.sql.clone(),
                count_in_window: in_window as u32,
                error: result.as_ref().err().map(ToString::to_string),
            }
        })
        .collect();
    RecentlyCreated {
        items,
        entities,
        error: None,
    }
}

/// The entity's newest `CREATED_ROWS` rows by `created`, where its `filter` holds.
fn newest<'r>(ctx: &'r Ctx<'_>, applied: &AppliedEntity, created: &str) -> Selection<'r> {
    let entity = &applied.entity;
    let condition = entity
        .filter
        .as_deref()
        .map(|f| format!("({})", ctx.preset_sql(f)))
        .unwrap_or_default();
    let order = format!(
        " ORDER BY {} DESC LIMIT {CREATED_ROWS}",
        ctx.column(Some("t"), created)
    );
    Selection::new(ctx, &entity.table, &condition, &order)
}

/// A row of an entity's newest rows, with the entity its document shows.
struct Candidate<'r> {
    rows: &'r Rows,
    row: usize,
    key: RowKey,
    entity: &'r AppliedEntity,
    created: Value,
}

fn candidates_of<'r>(
    rows: &'r Rows,
    entities: &'r [Option<AppliedEntity>],
    queried: &'r AppliedEntity,
    created: &str,
) -> Vec<Candidate<'r>> {
    let Some(col) = rows.column(created) else {
        return Vec::new();
    };
    (0..rows.len())
        .filter_map(|row| {
            Some(Candidate {
                rows,
                row,
                key: rows.key(row)?,
                entity: entities[row].as_ref().unwrap_or(queried),
                created: rows.row_values(row, &|_, _| None).swap_remove(col),
            })
        })
        .collect()
}

/// Newest first, each row once, at most `CREATED_ROWS`.
fn window(mut candidates: Vec<Candidate<'_>>) -> Vec<Candidate<'_>> {
    candidates.sort_by(|a, b| newest_first(&a.created, &b.created));
    let mut seen = HashSet::new();
    candidates.retain(|c| seen.insert((c.rows.table.clone(), c.key.clone())));
    candidates.truncate(CREATED_ROWS);
    candidates
}

/// By moment; values without one (zero dates, text that is no time) last, by their text.
fn newest_first(a: &Value, b: &Value) -> Ordering {
    match (moment(a), moment(b)) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => b.raw.as_text().cmp(&a.raw.as_text()),
    }
}

async fn window_summaries(ctx: &Ctx<'_>, window: &[Candidate<'_>]) -> Vec<Summary> {
    let mut by_table: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, candidate) in window.iter().enumerate() {
        by_table.entry(&candidate.rows.table).or_default().push(i);
    }
    let batches = join_all(by_table.into_values().map(|members| async move {
        let items: Vec<RowRef<'_>> = members
            .iter()
            .map(|&i| RowRef {
                rows: window[i].rows,
                row: window[i].row,
                entity: Some(window[i].entity),
            })
            .collect();
        members.into_iter().zip(summary::of_rows(ctx, &items).await)
    }))
    .await;
    let mut out = vec![Summary::default(); window.len()];
    for (i, summary) in batches.into_iter().flatten() {
        out[i] = summary;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, FakeDatabase, ResultSet};
    use crate::error::AppError;
    use crate::query::testing::{num, result, shop, text};
    use crate::readout::testing::Rig;

    const ENTITIES: &str = r#"
[[entity]]
name = "open_order"
table = "orders"
filter = "status = 'open'"
summary = ["status"]
created = "created_gmt"

[[entity]]
name = "order"
table = "orders"
summary = ["status", "total"]
created = "created_gmt"

[[entity]]
name = "entry"
table = "log"
created = "at_gmt"

[[entity]]
name = "person"
table = "people"
"#;

    fn order(id: &str, status: &str, created: &str, open: &str) -> Vec<Cell> {
        vec![
            num(id),
            text(status),
            text("10.00"),
            text("EUR"),
            Cell::Temporal(created.into()),
            num(open),
            num("1"),
        ]
    }

    fn orders_with_tests(rows: Vec<Vec<Cell>>) -> ResultSet {
        result(
            &[
                "id",
                "status",
                "total",
                "currency",
                "created_gmt",
                "__databee_entity_0",
                "__databee_entity_1",
            ],
            rows,
        )
    }

    fn log_entry(id: &str, at: &str) -> ResultSet {
        result(
            &["log_id", "order_id", "at_gmt", "msg", "__databee_entity_0"],
            vec![vec![
                num(id),
                num("1"),
                Cell::Temporal(at.into()),
                text("hi"),
                num("1"),
            ]],
        )
    }

    fn created_db() -> FakeDatabase {
        shop()
            .on(
                "WHERE (status = 'open')",
                orders_with_tests(vec![order("2", "open", "2026-01-02 00:00:00", "1")]),
            )
            .on(
                "`f_orders` AS t ORDER BY",
                orders_with_tests(vec![
                    order("3", "done", "2026-01-03 00:00:00", "0"),
                    order("2", "open", "2026-01-02 00:00:00", "1"),
                    order("1", "done", "0000-00-00 00:00:00", "0"),
                ]),
            )
            .on("`f_log`", log_entry("9", "2026-01-02 12:00:00"))
    }

    fn timeline(created: &RecentlyCreated) -> Vec<(&str, String, Option<&str>)> {
        created
            .items
            .iter()
            .map(|i| {
                (
                    i.entity.as_str(),
                    i.address.to_string(),
                    i.created.raw.as_text(),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn rows_created_last_merge_newest_first_across_entities_each_row_once() {
        let rig = Rig::new(created_db(), ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let created = recently_created(&ctx).await;

        assert_eq!(created.error, None);
        assert_eq!(
            timeline(&created),
            [
                (
                    "order",
                    ctx.row_address("f_orders", "3").to_string(),
                    Some("2026-01-03 00:00:00")
                ),
                (
                    "entry",
                    ctx.row_address("f_log", "9").to_string(),
                    Some("2026-01-02 12:00:00")
                ),
                (
                    "open_order",
                    ctx.row_address("f_orders", "2").to_string(),
                    Some("2026-01-02 00:00:00")
                ),
                (
                    "order",
                    ctx.row_address("f_orders", "1").to_string(),
                    Some("0000-00-00 00:00:00")
                ),
            ]
        );
    }

    #[tokio::test]
    async fn each_created_row_shows_the_summary_of_the_entity_its_document_shows() {
        let rig = Rig::new(created_db(), ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let created = recently_created(&ctx).await;

        let names: Vec<Vec<&str>> = created
            .items
            .iter()
            .map(|i| i.summary.values.iter().map(|v| v.name.as_str()).collect())
            .collect();
        assert_eq!(
            names,
            [
                vec!["status", "total"],
                vec!["order_id", "at_gmt", "msg"],
                vec!["status"],
                vec!["status", "total"],
            ]
        );
    }

    #[tokio::test]
    async fn each_entity_with_a_created_column_keeps_its_sql_and_count_in_the_window() {
        let rig = Rig::new(created_db(), ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let created = recently_created(&ctx).await;

        let entities: Vec<(&str, &str, u32)> = created
            .entities
            .iter()
            .map(|e| (e.entity.as_str(), e.table.as_str(), e.count_in_window))
            .collect();
        assert_eq!(
            entities,
            [
                ("open_order", "f_orders", 1),
                ("order", "f_orders", 2),
                ("entry", "f_log", 1),
            ]
        );
        assert_eq!(
            created.entities[0].sql,
            "SELECT t.*, (status = 'open') AS `__databee_entity_0`, 1 AS `__databee_entity_1` \
             FROM `wp`.`f_orders` AS t WHERE (status = 'open') ORDER BY t.`created_gmt` DESC LIMIT 20"
        );
        assert_eq!(
            created.entities[1].sql,
            "SELECT t.*, (status = 'open') AS `__databee_entity_0`, 1 AS `__databee_entity_1` \
             FROM `wp`.`f_orders` AS t ORDER BY t.`created_gmt` DESC LIMIT 20"
        );
    }

    #[tokio::test]
    async fn the_timeline_keeps_the_newest_twenty_rows() {
        let rows = (1..=25)
            .map(|i| {
                order(
                    &i.to_string(),
                    "done",
                    &format!("2026-01-01 00:00:{i:02}"),
                    "0",
                )
            })
            .collect();
        let db = shop()
            .on("WHERE (status = 'open')", orders_with_tests(Vec::new()))
            .on("`f_orders` AS t ORDER BY", orders_with_tests(rows))
            .on("`f_log`", log_entry("9", "2025-01-01 00:00:00"));
        let rig = Rig::new(db, ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let created = recently_created(&ctx).await;

        assert_eq!(created.items.len(), CREATED_ROWS);
        assert_eq!(created.items[0].address, ctx.row_address("f_orders", "25"));
        assert_eq!(created.items[19].address, ctx.row_address("f_orders", "6"));
    }

    #[tokio::test]
    async fn an_entity_whose_query_fails_carries_the_error_and_the_others_stand() {
        let db = shop()
            .failing("`f_orders`", AppError::InvalidInput("boom".into()))
            .on("`f_log`", log_entry("9", "2026-01-02 12:00:00"));
        let rig = Rig::new(db, ENTITIES, &[]);
        let ctx = rig.ctx().await;

        let created = recently_created(&ctx).await;

        let errors: Vec<(&str, Option<&str>)> = created
            .entities
            .iter()
            .map(|e| (e.entity.as_str(), e.error.as_deref()))
            .collect();
        assert_eq!(
            errors,
            [
                ("open_order", Some("boom")),
                ("order", Some("boom")),
                ("entry", None)
            ]
        );
        assert_eq!(created.items.len(), 1);
    }
}
