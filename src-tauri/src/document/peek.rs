//! A peek: the summary that says which row a reference points at, without building its
//! document. One query for the row, plus one per meta table when its summary needs meta keys.
//! A table address that is not one row (a reference to a non-key column) peeks at the set: how
//! many rows match and the first few, or the row itself when only one matches.

use serde::Serialize;
use ts_rs::TS;

use super::build::canonical;
use super::own::{self, Own, Selection};
use super::summary::{self, RowRef, Summary};
use crate::address::{Address, Target};
use crate::db::ColumnEquals;
use crate::db::browse::aliased_equals_sql;
use crate::error::AppResult;
use crate::query::{Ctx, KeyOrder, Rows};
use crate::rules::effective::AppliedEntity;
use crate::value::RowKey;

/// Rows a set peek shows.
pub const MATCH_ROWS: usize = 5;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Peek {
    /// The peeked row's address, or the set's as given.
    pub address: Address,
    /// The single-column primary key that row addresses name, e.g. `ID` of `wp_users`.
    pub key_column: Option<String>,
    /// The row's summary; empty for a set.
    pub summary: Summary,
    /// For an address that selects no row or several: how many match and the first ones.
    pub matches: Option<PeekMatches>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct PeekMatches {
    #[ts(type = "number")]
    pub total: u64,
    /// The first `MATCH_ROWS` by primary key. The row the reference starts from is not known
    /// here, so a reference to its own table may list it.
    pub rows: Vec<PeekMatch>,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
pub struct PeekMatch {
    pub address: Address,
    pub key: RowKey,
    pub summary: Summary,
}

pub async fn peek(ctx: &Ctx<'_>, address: &Address) -> AppResult<Peek> {
    if let Target::Table { table, filter } = &address.target
        && own::row_filter(ctx, address).is_err()
    {
        return set(ctx, address, table, filter).await;
    }
    let (own, filter) = own::resolve(ctx, address).await?;
    let address = canonical(ctx, own.table(), filter);
    Ok(row(ctx, &own, address).await)
}

async fn row(ctx: &Ctx<'_>, own: &Own, address: Address) -> Peek {
    Peek {
        address,
        key_column: ctx.single_key(own.table()).map(str::to_string),
        summary: summary::of_own(ctx, own).await,
        matches: None,
    }
}

async fn set(
    ctx: &Ctx<'_>,
    address: &Address,
    table: &str,
    filter: &[ColumnEquals],
) -> AppResult<Peek> {
    own::existing(ctx, table)?;
    let condition = aliased_equals_sql(ctx.dialect, "t", filter);
    let order = KeyOrder::primary(ctx, table).sql(ctx, Some("t"));
    let tail = format!(" ORDER BY {order} LIMIT {}", MATCH_ROWS + 1);
    let (rows, mut entities) = Selection::new(ctx, table, &condition, &tail)
        .run(ctx)
        .await?;
    if rows.len() == 1 {
        let address = rows
            .key(0)
            .map_or_else(|| address.clone(), |key| ctx.key_address(table, &key));
        let own = Own {
            rows,
            entity: entities.remove(0),
        };
        return Ok(row(ctx, &own, address).await);
    }
    let total = if rows.len() > MATCH_ROWS {
        let condition = if condition.is_empty() {
            String::new()
        } else {
            format!(" WHERE {condition}")
        };
        ctx.count(&format!(
            "SELECT COUNT(*) FROM {} AS t{condition}",
            ctx.table_ref(table)
        ))
        .await?
    } else {
        rows.len() as u64
    };
    Ok(Peek {
        address: address.clone(),
        key_column: ctx.single_key(table).map(str::to_string),
        summary: Summary::default(),
        matches: Some(PeekMatches {
            total,
            rows: matches(ctx, &rows, &entities).await,
        }),
    })
}

async fn matches(ctx: &Ctx<'_>, rows: &Rows, entities: &[Option<AppliedEntity>]) -> Vec<PeekMatch> {
    let keyed: Vec<(usize, RowKey)> = (0..rows.len().min(MATCH_ROWS))
        .filter_map(|row| Some((row, rows.key(row)?)))
        .collect();
    let items: Vec<RowRef<'_>> = keyed
        .iter()
        .map(|(row, _)| RowRef {
            rows,
            row: *row,
            entity: entities[*row].as_ref(),
        })
        .collect();
    let summaries = summary::of_rows(ctx, &items).await;
    keyed
        .into_iter()
        .zip(summaries)
        .map(|((_, key), summary)| PeekMatch {
            address: ctx.key_address(&rows.table, &key),
            key,
            summary,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ResultSet};
    use crate::error::AppError;
    use crate::query::testing::{Rig, count, num, result, shop, text};
    use crate::value::{KeyPart, Provenance, ValueKind};

    const SUMMARY: &str = r#"
[[entity]]
name = "order"
table = "orders"
summary = ["status", "_note", "_missing"]
"#;

    const META: &str = "FROM `wp`.`f_ordermeta`";

    fn key(column: &str, value: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: column.into(),
            value: value.into(),
        }])
    }

    fn names(summary: &Summary) -> Vec<&str> {
        summary.values.iter().map(|v| v.name.as_str()).collect()
    }

    fn orders(ids: &[&str]) -> ResultSet {
        let rows = ids
            .iter()
            .map(|id| vec![num(id), text("open"), num("1")])
            .collect();
        result(&["id", "status", "__databee_entity_0"], rows)
    }

    fn open_orders(ctx: &Ctx<'_>) -> Address {
        ctx.address(Target::Table {
            table: "f_orders".into(),
            filter: vec![ColumnEquals {
                column: "status".into(),
                value: Some("open".into()),
            }],
        })
    }

    #[tokio::test]
    async fn an_entity_summary_gives_column_and_meta_values_with_their_sources() {
        let order = result(
            &[
                "id",
                "status",
                "total",
                "currency",
                "created_gmt",
                "__databee_entity_0",
            ],
            vec![vec![
                num("1"),
                text("open"),
                text("10.00"),
                text("EUR"),
                text("2023-11-14"),
                num("1"),
            ]],
        );
        let meta = result(
            &["meta_id", "order_id", "k", "v"],
            vec![vec![num("8"), num("1"), text("_note"), text("gift")]],
        );
        let rig = Rig::new(shop().on("AS t WHERE", order).on(META, meta), SUMMARY);
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &ctx.row_address("f_orders", "1")).await.unwrap();

        assert_eq!(peek.address, ctx.row_address("f_orders", "1"));
        assert_eq!(peek.key_column.as_deref(), Some("id"));
        assert_eq!(peek.matches, None);
        assert_eq!(names(&peek.summary), ["status", "_note", "_missing"]);
        let status = &peek.summary.values[0].value;
        assert_eq!(status.raw, text("open"));
        assert_eq!(
            status.source,
            Some(Provenance {
                table: "f_orders".into(),
                column: "status".into(),
                row: key("id", "1"),
            })
        );
        let note = &peek.summary.values[1].value;
        assert_eq!(note.raw, text("gift"));
        assert_eq!(
            note.source,
            Some(Provenance {
                table: "f_ordermeta".into(),
                column: "v".into(),
                row: key("meta_id", "8"),
            })
        );
        let missing = &peek.summary.values[2].value;
        assert_eq!(
            (&missing.raw, &missing.kind),
            (&Cell::Null, &ValueKind::Null)
        );
        assert_eq!(missing.source, None);
        assert_eq!(
            rig.db.queries()[1],
            "SELECT * FROM `wp`.`f_ordermeta` WHERE `order_id` IN ('1') \
             AND `k` IN ('_note', '_missing') ORDER BY `meta_id`"
        );
    }

    #[tokio::test]
    async fn a_summary_of_columns_reads_no_meta() {
        let rules = SUMMARY.replace(r#"["status", "_note", "_missing"]"#, r#"["status"]"#);
        let rig = Rig::new(shop().on("AS t WHERE", orders(&["1"])), &rules);
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &ctx.row_address("f_orders", "1")).await.unwrap();

        assert_eq!(names(&peek.summary), ["status"]);
        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn a_table_without_a_single_column_key_is_invalid_input() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;

        let err = peek(&ctx, &ctx.row_address("f_unknown", "1"))
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
    }

    #[tokio::test]
    async fn a_set_address_that_one_row_matches_peeks_at_that_row() {
        let rules = SUMMARY.replace(r#"["status", "_note", "_missing"]"#, r#"["status"]"#);
        let rig = Rig::new(shop().on("AS t WHERE", orders(&["4"])), &rules);
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &open_orders(&ctx)).await.unwrap();

        assert_eq!(peek.address, ctx.row_address("f_orders", "4"));
        assert_eq!(peek.matches, None);
        assert_eq!(names(&peek.summary), ["status"]);
        assert_eq!(
            rig.db.queries(),
            [
                "SELECT t.*, 1 AS `__databee_entity_0` FROM `wp`.`f_orders` AS t \
                 WHERE t.`status` = 'open' ORDER BY t.`id` LIMIT 6"
            ]
        );
    }

    #[tokio::test]
    async fn a_set_address_that_no_row_matches_has_no_matches_and_no_error() {
        let rig = Rig::new(shop().on("AS t WHERE", orders(&[])), SUMMARY);
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &open_orders(&ctx)).await.unwrap();

        assert_eq!(peek.address, open_orders(&ctx));
        assert_eq!(peek.summary, Summary::default());
        assert_eq!(
            peek.matches,
            Some(PeekMatches {
                total: 0,
                rows: Vec::new()
            })
        );
        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn a_set_address_that_several_rows_match_lists_them_with_their_summaries() {
        let rules = SUMMARY.replace(r#"["status", "_note", "_missing"]"#, r#"["status"]"#);
        let rig = Rig::new(shop().on("AS t WHERE", orders(&["2", "3"])), &rules);
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &open_orders(&ctx)).await.unwrap();

        assert_eq!(peek.key_column.as_deref(), Some("id"));
        let matches = peek.matches.unwrap();
        assert_eq!(matches.total, 2);
        let listed: Vec<(Address, RowKey, Vec<&str>)> = matches
            .rows
            .iter()
            .map(|m| (m.address.clone(), m.key.clone(), names(&m.summary)))
            .collect();
        assert_eq!(
            listed,
            [
                (
                    ctx.row_address("f_orders", "2"),
                    key("id", "2"),
                    vec!["status"]
                ),
                (
                    ctx.row_address("f_orders", "3"),
                    key("id", "3"),
                    vec!["status"]
                ),
            ]
        );
        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn beyond_the_listed_rows_a_set_counts_all_matches() {
        let rig = Rig::new(
            shop()
                .on("SELECT COUNT(*)", count(42))
                .on("AS t WHERE", orders(&["1", "2", "3", "4", "5", "6"])),
            SUMMARY
                .replace(r#"["status", "_note", "_missing"]"#, r#"["status"]"#)
                .as_str(),
        );
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &open_orders(&ctx)).await.unwrap();

        let matches = peek.matches.unwrap();
        assert_eq!((matches.total, matches.rows.len()), (42, MATCH_ROWS));
        assert_eq!(
            rig.db.queries()[1],
            "SELECT COUNT(*) FROM `wp`.`f_orders` AS t WHERE t.`status` = 'open'"
        );
    }
}
