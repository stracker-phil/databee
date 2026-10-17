//! Group columns (a reference's `columns`): columns of the source table, and keys of the source's
//! `[[meta]]` table pivoted in with one query per group, each value sourced from its meta row.

use std::collections::HashMap;

use super::GroupRow;
use super::context::{Ctx, Rows, non_empty};
use crate::db::Cell;
use crate::rules::effective::MetaRef;
use crate::value::{Value, ValueKind};

/// The group rows of `rows` with `columns` as fields; a meta key a row does not have is NULL.
pub async fn rows(ctx: &Ctx<'_>, rows: &Rows, columns: &[String]) -> Vec<GroupRow> {
    let no_currency = |_: usize, _: &str| None;
    let values = rows.values(&no_currency);
    let keys: Vec<&String> = columns
        .iter()
        .filter(|c| rows.column(c).is_none())
        .collect();
    let pivot = if keys.is_empty() {
        None
    } else {
        Pivot::load(ctx, rows, &values, &keys).await
    };
    (0..rows.len())
        .map(|row| GroupRow {
            key: rows.key(row),
            fields: columns
                .iter()
                .map(|name| {
                    let value = match rows.column(name) {
                        Some(col) => values[row][col].clone(),
                        None => pivot
                            .as_ref()
                            .and_then(|p| p.value(rows, row, name))
                            .unwrap_or_else(null),
                    };
                    (name.clone(), value)
                })
                .collect(),
        })
        .collect()
}

fn null() -> Value {
    Value {
        raw: Cell::Null,
        kind: ValueKind::Null,
        source: None,
        style: None,
        secondary: None,
    }
}

/// Meta values of the group's rows by owner key and meta key; the first row of a key wins.
struct Pivot {
    owner_key: String,
    values: HashMap<(String, String), Value>,
}

impl Pivot {
    async fn load(
        ctx: &Ctx<'_>,
        rows: &Rows,
        owner_values: &[Vec<Value>],
        keys: &[&String],
    ) -> Option<Pivot> {
        let meta = meta_of(ctx, &rows.table)?;
        let fk = meta.fk.as_deref()?;
        let owner_key = ctx.single_key(&rows.table)?.to_string();
        let mut owner_rows: HashMap<&str, usize> = HashMap::new();
        let mut ids: Vec<&str> = Vec::new();
        for row in 0..rows.len() {
            if let Some(Some(id)) = rows.text(row, &owner_key) {
                owner_rows.entry(id).or_insert(row);
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return None;
        }
        let found = fetch_meta(ctx, meta, fk, &ids, keys).await?;
        let owner_row = |meta_row: usize| -> Option<usize> {
            let id = found.text(meta_row, fk)??;
            owner_rows.get(id).copied()
        };
        let owner_currency = |meta_row: usize, name: &str| -> Option<Value> {
            let col = rows.column(name)?;
            let value = &owner_values[owner_row(meta_row)?][col];
            non_empty(value).then(|| value.clone())
        };
        let value_col = found.column(&meta.value)?;
        let mut values = HashMap::new();
        for (row, cells) in found.values(&owner_currency).into_iter().enumerate() {
            let (Some(Some(id)), Some(Some(key))) =
                (found.text(row, fk), found.text(row, &meta.key))
            else {
                continue;
            };
            values
                .entry((id.to_string(), key.to_string()))
                .or_insert_with(|| cells[value_col].clone());
        }
        Some(Pivot { owner_key, values })
    }

    fn value(&self, rows: &Rows, row: usize, key: &str) -> Option<Value> {
        let id = rows.text(row, &self.owner_key)??;
        self.values.get(&(id.to_string(), key.to_string())).cloned()
    }
}

/// The rows of `meta` under `keys` for the owners `ids`, in stored order.
async fn fetch_meta(
    ctx: &Ctx<'_>,
    meta: &MetaRef,
    fk: &str,
    ids: &[&str],
    keys: &[&String],
) -> Option<Rows> {
    let ids: Vec<String> = ids.iter().map(|id| ctx.literal(id)).collect();
    let names: Vec<String> = keys.iter().map(|k| ctx.literal(k)).collect();
    let sql = format!(
        "SELECT * FROM {} WHERE {} IN ({}) AND {} IN ({}) ORDER BY {}",
        ctx.table_ref(&meta.table),
        ctx.ident(fk),
        ids.join(", "),
        ctx.ident(&meta.key),
        names.join(", "),
        ctx.key_order(&meta.table, None)
    );
    match ctx.rows(&meta.table, &sql).await {
        Ok(found) => Some(found),
        Err(e) => {
            log::warn!("Group columns from {} not read: {e}", meta.table);
            None
        }
    }
}

fn meta_of<'r>(ctx: &'r Ctx<'_>, owner: &str) -> Option<&'r MetaRef> {
    ctx.rules.owned_meta(owner).next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::testing::{Rig, num, result, shop, text};
    use crate::value::{KeyPart, Provenance, RowKey};

    #[tokio::test]
    async fn a_meta_key_some_rows_lack_is_null_there_and_sourced_from_the_meta_row_elsewhere() {
        let meta = result(
            &["meta_id", "order_id", "k", "v"],
            vec![vec![num("10"), num("2"), text("_paid"), text("yes")]],
        );
        let rig = Rig::new(shop().on("`f_ordermeta`", meta), "");
        let ctx = rig.ctx().await;
        let orders = ctx.annotate(
            "f_orders",
            result(
                &["id", "status"],
                vec![vec![num("1"), text("open")], vec![num("2"), text("open")]],
            ),
        );
        let columns = ["id".to_string(), "_paid".to_string()];

        let rows = rows(&ctx, &orders, &columns).await;

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT * FROM `wp`.`f_ordermeta` WHERE `order_id` IN ('1', '2') AND `k` IN ('_paid') ORDER BY `meta_id`"
            ]
        );
        let paid: Vec<_> = rows.iter().map(|r| &r.fields[1]).collect();
        assert_eq!(paid[0].0, "_paid");
        assert_eq!(
            (&paid[0].1.raw, &paid[0].1.kind),
            (&Cell::Null, &ValueKind::Null)
        );
        assert_eq!(paid[1].1.raw, text("yes"));
        assert_eq!(
            paid[1].1.source,
            Some(Provenance {
                table: "f_ordermeta".into(),
                column: "v".into(),
                row: RowKey(vec![KeyPart {
                    column: "meta_id".into(),
                    value: "10".into(),
                }]),
            })
        );
        assert_eq!(rows[0].fields[0].1.raw, num("1"));
    }
}
