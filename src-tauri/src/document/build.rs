//! Assembles a document: the own row, meta, incoming references and patterns, then `exists` for
//! every reference value, the header and `events`.

use std::collections::HashMap;

use futures_util::future::join;

use super::meta::{self, MetaValues};
use super::non_empty;
use super::own::{self, Own};
use super::{
    Document, Group, GroupLink, GroupRow, GroupShape, actions, events, exists, incoming, patterns,
    summary,
};
use crate::address::{Address, Target};
use crate::db::ColumnEquals;
use crate::error::AppResult;
use crate::query::Ctx;
use crate::rules::kinds::ValueRule;
use crate::rules::placeholder::{Placeholder, scan};
use crate::value::Value;

pub async fn document(ctx: &Ctx<'_>, address: &Address) -> AppResult<Document> {
    let (own, filter) = own::resolve(ctx, address).await?;
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
    let (referencing, orders) = incoming_and_patterns(ctx, &own, &meta_values).await;
    groups.extend(referencing);
    unique_ids(&mut groups);
    let stored = MetaValues::with_currency(&meta_rows, &currency);
    let mut header = summary::of_document(ctx, &own, &own_values, &stored);
    exists::check(ctx, &mut header.values, &mut groups).await;
    groups.push(events::group(&groups, &orders));
    let actions = actions::for_row(ctx, &own).await;
    Ok(Document {
        address: canonical(ctx, own.table(), filter),
        entity: own.entity.as_ref().map(|e| e.entity.name.clone()),
        header,
        groups,
        alternatives: Vec::new(),
        actions,
    })
}

/// The incoming reference and pattern groups of a row without its document; meta rows are read
/// only when a pattern of the table needs them.
pub async fn referencing(ctx: &Ctx<'_>, own: &Own) -> Vec<Group> {
    let has_patterns = ctx.rules.patterns_of(own.table()).next().is_some();
    let meta_rows = if has_patterns {
        meta::load(ctx, own, &needed_meta_keys(ctx, own)).await
    } else {
        Vec::new()
    };
    incoming_and_patterns(ctx, own, &MetaValues::new(&meta_rows))
        .await
        .0
}

/// Incoming groups, then pattern groups, and the patterns' event orders.
async fn incoming_and_patterns(
    ctx: &Ctx<'_>,
    own: &Own,
    meta_values: &MetaValues,
) -> (Vec<Group>, HashMap<String, Option<String>>) {
    let (mut groups, found) = join(
        incoming::groups(ctx, own),
        patterns::groups(ctx, own, meta_values),
    )
    .await;
    let (pattern_groups, orders) = patterns::split(found);
    groups.extend(pattern_groups);
    (groups, orders)
}

/// A money value without its currency in its own row looks in the document's row (the owner of
/// its meta rows), then in its meta keys.
fn currency_lookup<'a>(
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

/// Meta keys the document reads by name besides showing them: the summary, the patterns'
/// `{meta.…}` and money currencies. Read beyond the cap of the meta group.
fn needed_meta_keys(ctx: &Ctx<'_>, own: &Own) -> Vec<String> {
    let mut names: Vec<&str> = Vec::new();
    if let Some(entity) = &own.entity {
        for name in &entity.entity.summary {
            names.push(name);
        }
    }
    for pattern in ctx.rules.patterns_of(own.table()) {
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

pub(super) fn canonical(ctx: &Ctx<'_>, table: &str, filter: Vec<ColumnEquals>) -> Address {
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
