//! Assembles a document: the own row, meta, incoming references and patterns, then `exists` for
//! every reference value, the header and `events`.

use std::collections::HashMap;

use futures_util::future::join;

use super::context::{Ctx, non_empty};
use super::meta::{self, MetaValues};
use super::own::{self, Own};
use super::{
    Document, Group, GroupLink, GroupRow, GroupShape, actions, events, exists, incoming, patterns,
};
use crate::address::{Address, Target};
use crate::db::ColumnEquals;
use crate::error::{AppError, AppResult};
use crate::rules::kinds::ValueRule;
use crate::rules::placeholder::{Placeholder, scan};
use crate::value::Value;

pub async fn document(ctx: &Ctx<'_>, address: &Address) -> AppResult<Document> {
    let (table, filter) = own::row_filter(ctx, address)?;
    if ctx.schema.table(&table).is_none() {
        return Err(AppError::InvalidInput(format!(
            "`{}` has no table `{table}`",
            ctx.database
        )));
    }
    let own = own::load(ctx, &table, &filter).await?;
    let meta_rows = meta::load(ctx, &own, &needed_meta_keys(ctx, &own)).await;
    let meta_values = MetaValues::new(&meta_rows);
    let plain = own.rows.values(&|_, _| None).remove(0);
    let currency = currency_lookup(&own, &plain, &meta_values);
    let own_values = own.rows.values(&currency).remove(0);
    let mut groups = vec![own_group(&own, &own_values)];
    groups.extend(
        meta_rows
            .iter()
            .map(|rows| meta::group(ctx, rows, &currency)),
    );
    let (incoming, found) = join(
        incoming::groups(ctx, &own),
        patterns::groups(ctx, &own, &meta_values),
    )
    .await;
    groups.extend(incoming);
    let (pattern_groups, orders) = patterns::split(found);
    groups.extend(pattern_groups);
    unique_ids(&mut groups);
    let mut header = header(&own, &groups[0].rows[0].fields, &groups, &meta_values);
    exists::check(ctx, &mut header, &mut groups).await;
    groups.push(events::group(&groups, &orders));
    let actions = actions::for_row(ctx, &own).await;
    Ok(Document {
        address: canonical(ctx, &table, filter),
        entity: own.entity.as_ref().map(|e| e.entity.name.clone()),
        header,
        groups,
        alternatives: Vec::new(),
        actions,
    })
}

/// A money value without its currency in its own row looks in the document's row (the owner of
/// its meta rows), then in its meta keys.
pub(super) fn currency_lookup<'a>(
    own: &'a Own,
    plain: &'a [Value],
    meta: &'a MetaValues,
) -> impl Fn(usize, &str) -> Option<Value> + 'a {
    move |_, name| {
        let own_value = own.rows.column(name).map(|col| &plain[col]);
        own_value
            .filter(|v| non_empty(v))
            .or_else(|| meta.get(name).filter(|v| non_empty(v)))
            .cloned()
    }
}

fn own_group(own: &Own, values: &[Value]) -> Group {
    let table = own.table();
    let mut group = Group::new("own".into(), table, GroupLink::Own, GroupShape::Fields);
    group.columns = own.rows.names();
    group.total = 1;
    group.rows = vec![GroupRow {
        key: own.rows.key(0),
        fields: group
            .columns
            .iter()
            .cloned()
            .zip(values.iter().cloned())
            .collect(),
    }];
    group
}

/// Meta keys the document reads by name besides showing them: the header, the patterns'
/// `{meta.…}` and money currencies. Read beyond the cap of the meta group.
pub(super) fn needed_meta_keys(ctx: &Ctx<'_>, own: &Own) -> Vec<String> {
    let mut names: Vec<&str> = Vec::new();
    if let Some(entity) = &own.entity {
        for name in &entity.entity.summary {
            names.push(name);
        }
    }
    for pattern in &ctx.rules.patterns {
        if pattern.rule.owner != own.table() {
            continue;
        }
        for (_, placeholder) in scan(&pattern.rule.r#where) {
            if let Placeholder::Meta(key) = placeholder {
                names.push(key);
            }
        }
    }
    for rule in &ctx.rules.kinds {
        if let ValueRule::Money(money) = &rule.value {
            for name in &money.currency {
                names.push(name);
            }
        }
    }
    let mut keys: Vec<String> = Vec::new();
    for name in names {
        if own.rows.column(name).is_none() && !keys.iter().any(|k| k == name) {
            keys.push(name.to_string());
        }
    }
    keys
}

/// The entity's summary: the row's own value of a column, else the first meta row of the key.
fn header(
    own: &Own,
    own_fields: &[(String, Value)],
    groups: &[Group],
    meta: &MetaValues,
) -> Vec<(String, Value)> {
    let Some(entity) = &own.entity else {
        return Vec::new();
    };
    entity
        .entity
        .summary
        .iter()
        .filter_map(|name| {
            let column = own_fields
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(name));
            let value = column
                .map(|(_, v)| v.clone())
                .or_else(|| meta_group_value(groups, name))
                .or_else(|| meta.get(name).cloned())?;
            Some((name.clone(), value))
        })
        .collect()
}

fn meta_group_value(groups: &[Group], key: &str) -> Option<Value> {
    groups
        .iter()
        .filter(|g| matches!(g.via, GroupLink::Meta { .. }))
        .flat_map(|g| &g.rows)
        .filter_map(meta::key_value)
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
}

/// Two rules may give the same id (a reference to this table and its meta copy): later ones
/// get `#2`, `#3`.
fn unique_ids(groups: &mut [Group]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for group in groups {
        let count = seen.entry(group.id.clone()).or_default();
        *count += 1;
        if *count > 1 {
            group.id = format!("{}#{count}", group.id);
        }
    }
}

fn canonical(ctx: &Ctx<'_>, table: &str, filter: Vec<ColumnEquals>) -> Address {
    if let ([column], [only]) = (ctx.primary_key(table), filter.as_slice())
        && column.eq_ignore_ascii_case(&only.column)
        && let Some(key) = &only.value
    {
        return ctx.row_address(table, key);
    }
    ctx.address(Target::Table {
        table: table.to_string(),
        filter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str) -> Group {
        Group::new(id.into(), "t", GroupLink::Own, GroupShape::Rows)
    }

    #[test]
    fn groups_with_the_same_id_are_numbered_from_the_second() {
        let mut groups = vec![group("a"), group("b"), group("a"), group("a"), group("b")];

        unique_ids(&mut groups);

        let ids: Vec<&str> = groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "a#2", "a#3", "b#2"]);
    }
}
