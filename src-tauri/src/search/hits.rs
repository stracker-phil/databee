use std::collections::{BTreeMap, HashSet};

use futures_util::future::join_all;

use super::{PaletteHit, SECTION_CAP};
use crate::address::Address;
use crate::db::Cell;
use crate::document::summary::{self, RowRef, Summary};
use crate::document::{Group, GroupLink};
use crate::query::{Ctx, Rows};
use crate::rules::effective::{AppliedEntity, MetaRef};
use crate::value::{KeyPart, RowKey, Value, ValueKind};

type Fields = Vec<(String, Value)>;

fn field<'f>(fields: &'f Fields, name: &str) -> Option<&'f Value> {
    fields
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

/// How the rows of one table become hits.
pub struct Place<'a> {
    pub section: &'a str,
    pub source: String,
    pub table: &'a str,
    /// A matched column, shown first unless the summary shows it.
    pub matched: Option<&'a str>,
}

/// The hit of one row: its key as the title, the matched value and the row's summary as detail.
pub fn hit(
    ctx: &Ctx<'_>,
    place: &Place<'_>,
    key: &RowKey,
    fields: &Fields,
    summary: Summary,
) -> PaletteHit {
    let summarized = |name: &str| {
        summary
            .values
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(name))
    };
    let mut detail: Vec<Value> = place
        .matched
        .filter(|name| !summarized(name))
        .and_then(|name| field(fields, name))
        .into_iter()
        .cloned()
        .collect();
    detail.extend(summary.values.into_iter().map(|v| v.value));
    let address = ctx.key_address(place.table, key);
    PaletteHit {
        section: place.section.to_string(),
        title: key
            .0
            .iter()
            .map(|part| key_value(fields, part, &address))
            .collect(),
        address,
        source: place.source.clone(),
        detail,
    }
}

/// The row's own key value, or the bare key when the row's columns leave it out.
fn key_value(fields: &Fields, part: &KeyPart, address: &Address) -> Value {
    field(fields, &part.column)
        .cloned()
        .unwrap_or_else(|| Value {
            raw: Cell::Text(part.value.clone()),
            kind: ValueKind::PrimaryKey {
                address: address.clone(),
            },
            source: None,
            style: None,
            secondary: None,
        })
}

fn fields_of(rows: &Rows) -> Vec<Fields> {
    let names = rows.names();
    rows.values(&|_, _| None)
        .into_iter()
        .map(|values| names.iter().cloned().zip(values).collect())
        .collect()
}

/// One hit per row of `rows` with a key, at most `SECTION_CAP`, each with its entity's summary;
/// rows read without their `entities` have them tested.
pub async fn from_rows(
    ctx: &Ctx<'_>,
    rows: &Rows,
    entities: Option<Vec<Option<AppliedEntity>>>,
    section: &str,
    matched: Option<&str>,
) -> Vec<PaletteHit> {
    let place = Place {
        section,
        source: matched.map_or_else(
            || rows.table.clone(),
            |column| format!("{}.{column}", rows.table),
        ),
        table: &rows.table,
        matched,
    };
    let keyed: Vec<(usize, RowKey)> = (0..rows.len())
        .filter_map(|row| Some((row, rows.key(row)?)))
        .take(SECTION_CAP)
        .collect();
    if keyed.is_empty() {
        return Vec::new();
    }
    let entities = match entities {
        Some(entities) => entities,
        None => summary::entities(ctx, rows).await,
    };
    let items: Vec<RowRef<'_>> = keyed
        .iter()
        .map(|(row, _)| RowRef {
            rows,
            row: *row,
            entity: entities[*row].as_ref(),
        })
        .collect();
    let summaries = summary::of_rows(ctx, &items).await;
    let fields = fields_of(rows);
    keyed
        .iter()
        .zip(summaries)
        .map(|((row, key), summary)| hit(ctx, &place, key, &fields[*row], summary))
        .collect()
}

/// The rows of incoming reference and pattern groups, named by the referencing column. Group
/// rows may hold only some columns, so each table's rows are read again for their summaries.
pub async fn from_groups(ctx: &Ctx<'_>, groups: &[Group], section: &str) -> Vec<PaletteHit> {
    let mut found: Vec<(Place<'_>, &RowKey, &Fields)> = Vec::new();
    let mut seen = HashSet::new();
    for group in groups {
        let source = match &group.via {
            GroupLink::Incoming { reference, .. } => {
                format!("{}.{}", group.table, reference.from.columns.join(","))
            }
            GroupLink::Pattern { .. } => group.table.clone(),
            _ => continue,
        };
        for row in &group.rows {
            let Some(key) = &row.key else {
                continue;
            };
            if found.len() < SECTION_CAP
                && seen.insert(ctx.key_address(&group.table, key).to_string())
            {
                let place = Place {
                    section,
                    source: source.clone(),
                    table: &group.table,
                    matched: None,
                };
                found.push((place, key, &row.fields));
            }
        }
    }
    let mut keys: BTreeMap<&str, Vec<RowKey>> = BTreeMap::new();
    for (place, key, _) in &found {
        keys.entry(place.table).or_default().push((*key).clone());
    }
    let summaries: BTreeMap<&str, _> =
        join_all(keys.iter().map(|(table, keys)| async move {
            (*table, summary::of_keys(ctx, table, keys).await)
        }))
        .await
        .into_iter()
        .collect();
    found
        .into_iter()
        .map(|(place, key, fields)| {
            let summary = summaries
                .get(place.table)
                .and_then(|s| s.get(key).cloned())
                .unwrap_or_default();
            hit(ctx, &place, key, fields, summary)
        })
        .collect()
}

/// Rows of a `[[meta]]` table: each opens its owner row (titled by the `fk` value), or the meta
/// row itself without an owner; the detail is the stored key and value.
pub fn from_meta(ctx: &Ctx<'_>, rows: &Rows, meta: &MetaRef, section: &str) -> Vec<PaletteHit> {
    fields_of(rows)
        .iter()
        .enumerate()
        .filter_map(|(i, fields)| {
            let detail: Vec<Value> = [&meta.key, &meta.value]
                .iter()
                .filter_map(|name| field(fields, name).cloned())
                .collect();
            let (address, title, source) = match owner_of(ctx, meta, rows, i, fields) {
                Some(owner) => owner,
                None => {
                    let key = rows.key(i)?;
                    let own = ctx.key_address(&meta.table, &key);
                    let title = key.0.iter().map(|p| key_value(fields, p, &own)).collect();
                    (own, title, meta.table.clone())
                }
            };
            Some(PaletteHit {
                section: section.to_string(),
                address,
                source,
                title,
                detail,
            })
        })
        .take(SECTION_CAP)
        .collect()
}

/// The owner row of meta row `i`, its `fk` value as the owner's key, and the owner table.
fn owner_of(
    ctx: &Ctx<'_>,
    meta: &MetaRef,
    rows: &Rows,
    i: usize,
    fields: &Fields,
) -> Option<(Address, Vec<Value>, String)> {
    let (owner, fk) = (meta.owner.as_ref()?, meta.fk.as_ref()?);
    ctx.single_key(owner)?;
    let id = rows.text(i, fk)??;
    let address = ctx.row_address(owner, id);
    let mut title = field(fields, fk)?.clone();
    title.kind = ValueKind::PrimaryKey {
        address: address.clone(),
    };
    Some((address, vec![title], owner.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Target;
    use crate::db::ColumnEquals;
    use crate::query::testing::{Rig, shop};

    fn key(parts: &[(&str, &str)]) -> RowKey {
        RowKey(
            parts
                .iter()
                .map(|(column, value)| KeyPart {
                    column: column.to_string(),
                    value: value.to_string(),
                })
                .collect(),
        )
    }

    #[tokio::test]
    async fn a_single_part_key_is_a_row_address_and_a_composite_key_filters_every_part() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;

        assert_eq!(
            ctx.key_address("f_orders", &key(&[("id", "5")])),
            ctx.row_address("f_orders", "5")
        );
        assert_eq!(
            ctx.key_address("f_pairs", &key(&[("a", "1"), ("b", "2")])),
            ctx.address(Target::Table {
                table: "f_pairs".into(),
                filter: vec![
                    ColumnEquals {
                        column: "a".into(),
                        value: Some("1".into()),
                    },
                    ColumnEquals {
                        column: "b".into(),
                        value: Some("2".into()),
                    },
                ],
            })
        );
    }
}
