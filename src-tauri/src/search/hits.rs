//! Hits from rows: the row's address and key, and the values that say which row it is.

use std::collections::HashSet;

use super::{PaletteHit, SECTION_CAP};
use crate::address::{Address, Target};
use crate::db::{Cell, ColumnEquals};
use crate::document::{Ctx, Group, GroupLink, Rows};
use crate::rules::effective::MetaRef;
use crate::rules::types::Entity;
use crate::value::{KeyPart, RowKey, Value, ValueKind};

/// Values shown for a row without an entity summary.
const PLAIN_VALUES: usize = 4;

type Fields = Vec<(String, Value)>;

pub fn address(ctx: &Ctx<'_>, table: &str, key: &RowKey) -> Address {
    if let [only] = key.0.as_slice() {
        return ctx.row_address(table, &only.value);
    }
    let filter = key
        .0
        .iter()
        .map(|part| ColumnEquals {
            column: part.column.clone(),
            value: Some(part.value.clone()),
        })
        .collect();
    ctx.address(Target::Table {
        table: table.to_string(),
        filter,
    })
}

/// The entity a table's rows are shown with: its unfiltered one, else its first.
pub fn table_entity<'r>(ctx: &'r Ctx<'_>, table: &str) -> Option<&'r Entity> {
    let mut entities = ctx
        .rules
        .entities
        .iter()
        .filter(|e| e.entity.table == table);
    let first = entities.clone().next()?;
    let unfiltered = entities.find(|e| e.entity.filter.is_none());
    Some(&unfiltered.unwrap_or(first).entity)
}

fn field<'f>(fields: &'f Fields, name: &str) -> Option<&'f Value> {
    fields
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

fn shown(value: &Value) -> bool {
    !matches!(value.kind, ValueKind::Null | ValueKind::Empty)
}

/// How the rows of one table become hits; `skip` columns stay out of the plain values.
pub struct Place<'a> {
    pub section: &'a str,
    pub source: String,
    pub table: &'a str,
    pub skip: &'a [String],
    pub summary: &'a [String],
    /// A matched column, shown first.
    pub matched: Option<&'a str>,
}

pub fn row(ctx: &Ctx<'_>, place: &Place<'_>, key: &RowKey, fields: &Fields) -> PaletteHit {
    let in_key = |name: &str| key.0.iter().any(|p| p.column.eq_ignore_ascii_case(name));
    let skipped = |name: &str| place.skip.iter().any(|s| s.eq_ignore_ascii_case(name));
    let mut detail: Vec<Value> = place
        .matched
        .and_then(|name| field(fields, name))
        .into_iter()
        .cloned()
        .collect();
    let named: Vec<&str> = place.summary.iter().map(String::as_str).collect();
    if named.is_empty() {
        let plain = fields
            .iter()
            .filter(|(name, value)| !in_key(name) && !skipped(name) && shown(value))
            .filter(|(name, _)| Some(name.as_str()) != place.matched)
            .take(PLAIN_VALUES)
            .map(|(_, v)| v.clone());
        detail.extend(plain);
    } else {
        let summary = named
            .into_iter()
            .filter(|name| Some(*name) != place.matched)
            .filter_map(|name| field(fields, name).cloned());
        detail.extend(summary);
    }
    let address = address(ctx, place.table, key);
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

/// One hit per row of `rows` with a key, at most `SECTION_CAP`.
pub fn from_rows(
    ctx: &Ctx<'_>,
    rows: &Rows,
    section: &str,
    entity: Option<&Entity>,
    matched: Option<&str>,
) -> Vec<PaletteHit> {
    let place = Place {
        section,
        source: matched.map_or_else(
            || rows.table.clone(),
            |column| format!("{}.{column}", rows.table),
        ),
        table: &rows.table,
        skip: &[],
        summary: entity.map_or(&[], |e| &e.summary),
        matched,
    };
    fields_of(rows)
        .iter()
        .enumerate()
        .filter_map(|(i, fields)| Some(row(ctx, &place, &rows.key(i)?, fields)))
        .take(SECTION_CAP)
        .collect()
}

/// The rows of incoming reference and pattern groups, named by the referencing column.
pub fn from_groups(ctx: &Ctx<'_>, groups: &[Group], section: &str) -> Vec<PaletteHit> {
    let mut hits = Vec::new();
    for group in groups {
        let (source, skip) = match &group.via {
            GroupLink::Incoming { reference, .. } => {
                let columns = &reference.from.columns;
                (
                    format!("{}.{}", group.table, columns.join(",")),
                    columns.clone(),
                )
            }
            GroupLink::Pattern { .. } => (group.table.clone(), Vec::new()),
            _ => continue,
        };
        let place = Place {
            section,
            source,
            table: &group.table,
            skip: &skip,
            summary: &[],
            matched: None,
        };
        hits.extend(
            group
                .rows
                .iter()
                .filter_map(|r| Some(row(ctx, &place, r.key.as_ref()?, &r.fields))),
        );
    }
    let mut seen = HashSet::new();
    hits.retain(|hit| seen.insert(hit.address.to_string()));
    hits.truncate(SECTION_CAP);
    hits
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
                    let own = address(ctx, &meta.table, &key);
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
