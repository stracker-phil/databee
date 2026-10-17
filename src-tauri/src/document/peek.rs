//! A peek: the few values that say which row a reference points at, without building its
//! document. One query for the row, plus the meta queries its entity's summary needs.

use serde::Serialize;
use ts_rs::TS;

use super::build::{currency_lookup, needed_meta_keys};
use super::context::Ctx;
use super::meta::{self, MetaValues};
use super::own::{self, Own};
use crate::address::Address;
use crate::db::Cell;
use crate::error::{AppError, AppResult};
use crate::value::{Value, ValueKind};

/// Columns shown for a row whose table has no entity summary.
const PLAIN_COLUMNS: usize = 5;

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct Peek {
    /// The single-column primary key that row addresses name, e.g. `ID` of `wp_users`.
    pub key_column: Option<String>,
    /// The entity's `summary`, else the first five columns that are not part of the key.
    pub values: Vec<(String, Value)>,
}

pub async fn peek(ctx: &Ctx<'_>, address: &Address) -> AppResult<Peek> {
    let (table, filter) = own::row_filter(ctx, address)?;
    if ctx.schema.table(&table).is_none() {
        return Err(AppError::InvalidInput(format!(
            "`{}` has no table `{table}`",
            ctx.database
        )));
    }
    let own = own::load(ctx, &table, &filter).await?;
    let summary: &[String] = own.entity.as_ref().map_or(&[], |e| &e.entity.summary);
    let values = if summary.is_empty() {
        plain_columns(ctx, &own)
    } else {
        summary_values(ctx, &own, summary).await
    };
    Ok(Peek {
        key_column: ctx.single_key(&table).map(str::to_string),
        values,
    })
}

/// The row's values without currency lookups.
fn plain_row(own: &Own) -> Vec<Value> {
    own.rows.values(&|_, _| None).remove(0)
}

/// A summary name that is neither a column nor a stored meta key still shows, as NULL.
fn missing() -> Value {
    Value {
        raw: Cell::Null,
        kind: ValueKind::Null,
        source: None,
        style: None,
        secondary: None,
    }
}

fn plain_columns(ctx: &Ctx<'_>, own: &Own) -> Vec<(String, Value)> {
    let key = ctx.primary_key(own.table());
    let in_key = |name: &str| key.iter().any(|k| k.eq_ignore_ascii_case(name));
    own.rows
        .names()
        .into_iter()
        .zip(plain_row(own))
        .filter(|(name, _)| !in_key(name))
        .take(PLAIN_COLUMNS)
        .collect()
}

/// The summary as the document header reads it; meta rows only when a name is not a column.
async fn summary_values(ctx: &Ctx<'_>, own: &Own, summary: &[String]) -> Vec<(String, Value)> {
    let needs_meta = summary.iter().any(|name| own.rows.column(name).is_none());
    let meta_rows = if needs_meta {
        meta::load(ctx, own, &needed_meta_keys(ctx, own)).await
    } else {
        Vec::new()
    };
    let meta_values = MetaValues::new(&meta_rows);
    let plain = plain_row(own);
    let currency = currency_lookup(own, &plain, &meta_values);
    let values = own.rows.values(&currency).remove(0);
    summary
        .iter()
        .map(|name| {
            let value = match own.rows.column(name) {
                Some(col) => values[col].clone(),
                None => meta_values.get(name).cloned().unwrap_or_else(missing),
            };
            (name.clone(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::context::testing::{Rig, num, result, shop, text};
    use crate::value::{KeyPart, Provenance, RowKey};

    const SUMMARY: &str = r#"
[[entity]]
name = "order"
table = "orders"
summary = ["status", "_note", "_missing"]
"#;

    fn key(column: &str, value: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: column.into(),
            value: value.into(),
        }])
    }

    fn names(peek: &Peek) -> Vec<&str> {
        peek.values.iter().map(|(name, _)| name.as_str()).collect()
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
            vec![
                vec![num("7"), num("1"), text("_other"), text("x")],
                vec![num("8"), num("1"), text("_note"), text("gift")],
            ],
        );
        let rig = Rig::new(
            shop()
                .on("AS t WHERE", order)
                .on("FROM `wp`.`f_ordermeta`", meta),
            SUMMARY,
        );
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &ctx.row_address("f_orders", "1")).await.unwrap();

        assert_eq!(peek.key_column.as_deref(), Some("id"));
        assert_eq!(names(&peek), ["status", "_note", "_missing"]);
        let (_, status) = &peek.values[0];
        assert_eq!(status.raw, text("open"));
        assert_eq!(
            status.source,
            Some(Provenance {
                table: "f_orders".into(),
                column: "status".into(),
                row: key("id", "1"),
            })
        );
        let (_, note) = &peek.values[1];
        assert_eq!(note.raw, text("gift"));
        assert_eq!(
            note.source,
            Some(Provenance {
                table: "f_ordermeta".into(),
                column: "v".into(),
                row: key("meta_id", "8"),
            })
        );
        let (_, missing) = &peek.values[2];
        assert_eq!(missing.raw, Cell::Null);
        assert_eq!(missing.kind, ValueKind::Null);
        assert_eq!(missing.source, None);
    }

    #[tokio::test]
    async fn a_summary_of_columns_reads_no_meta() {
        let order = result(
            &["id", "status", "__databee_entity_0"],
            vec![vec![num("1"), text("open"), num("1")]],
        );
        let rules = SUMMARY.replace(r#"["status", "_note", "_missing"]"#, r#"["status"]"#);
        let rig = Rig::new(shop().on("AS t WHERE", order), &rules);
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &ctx.row_address("f_orders", "1")).await.unwrap();

        assert_eq!(names(&peek), ["status"]);
        assert_eq!(rig.db.queries().len(), 1);
    }

    #[tokio::test]
    async fn without_a_summary_the_first_five_columns_outside_the_key_are_shown() {
        let order = result(
            &["id", "a", "b", "c", "d", "e", "f"],
            vec![vec![
                num("1"),
                text("1"),
                text("2"),
                text("3"),
                text("4"),
                text("5"),
                text("6"),
            ]],
        );
        let rig = Rig::new(shop().on("AS t WHERE", order), "");
        let ctx = rig.ctx().await;

        let peek = peek(&ctx, &ctx.row_address("f_orders", "1")).await.unwrap();

        assert_eq!(peek.key_column.as_deref(), Some("id"));
        assert_eq!(names(&peek), ["a", "b", "c", "d", "e"]);
        assert_eq!(peek.values[0].1.raw, text("1"));
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
}
