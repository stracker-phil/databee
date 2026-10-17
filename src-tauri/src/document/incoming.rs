//! Incoming reference groups: the rows whose reference points at this row, in the lens's order
//! (rule references in precedence order, then declared foreign keys, then same-name inference).
//!
//! A source row belongs to a reference only when the lens would give its cell that reference: a
//! conditional rule that comes first takes the rows its `when` holds on, and a rule after an
//! unconditional one never decides a row.

use std::collections::BTreeMap;

use futures_util::future::join_all;

use super::context::{Capped, Ctx};
use super::own::Own;
use super::pivot;
use super::{Group, GroupLink, GroupShape, ROWS_CAP};
use crate::address::{Address, Target};
use crate::db::ColumnEquals;
use crate::error::AppResult;
use crate::lens::ResolvedReference;
use crate::rules::condition::{self, Condition};
use crate::rules::coverage::conditions;
use crate::rules::effective::{RuleReference, inferred_id};
use crate::rules::types::ColumnRef;
use crate::value::ReferenceSource;

const ALIAS: &str = "s";

/// One incoming reference with what decides its source rows.
struct Incoming<'r> {
    reference: ResolvedReference,
    rule: Option<&'r RuleReference>,
    /// The `when` of conditional rules on the same column that take their rows first.
    earlier: Vec<&'r [Condition]>,
    /// The row's values of the target columns, paired with the source columns.
    values: Vec<(String, String)>,
}

pub async fn groups(ctx: &Ctx<'_>, own: &Own) -> Vec<Group> {
    let incoming: Vec<Incoming<'_>> = ctx
        .lens
        .references_to(own.table())
        .into_iter()
        .filter(|r| !is_meta_of(ctx, r, own.table()))
        .filter_map(|r| plan(ctx, own, r))
        .collect();
    let rows = source_rows(ctx, own, &incoming).await;
    join_all(
        incoming
            .iter()
            .zip(rows)
            .map(|(i, rows)| group(ctx, own, i, rows)),
    )
    .await
}

/// The capped source rows of each reference that can point at the row, one query per source
/// table; `None` for a reference whose target columns are empty in the row.
async fn source_rows(
    ctx: &Ctx<'_>,
    own: &Own,
    incoming: &[Incoming<'_>],
) -> Vec<Option<AppResult<Capped>>> {
    let mut by_table: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, reference) in incoming.iter().enumerate() {
        if has_values(reference) {
            by_table
                .entry(&reference.reference.from.table)
                .or_default()
                .push(i);
        }
    }
    let queries = by_table.into_iter().map(|(table, members)| async move {
        let filters: Vec<String> = members
            .iter()
            .map(|&i| filter_sql(ctx, own, &incoming[i]))
            .collect();
        let rows = ctx.capped_each(table, ALIAS, &filters, ROWS_CAP).await;
        members.into_iter().zip(rows)
    });
    let mut out: Vec<Option<AppResult<Capped>>> = incoming.iter().map(|_| None).collect();
    for (i, rows) in join_all(queries).await.into_iter().flatten() {
        out[i] = Some(rows);
    }
    out
}

/// The `fk → owner` reference of a meta table, which the meta group shows.
fn is_meta_of(ctx: &Ctx<'_>, reference: &ResolvedReference, owner: &str) -> bool {
    let [column] = reference.from.columns.as_slice() else {
        return false;
    };
    ctx.rules.meta.iter().any(|m| {
        m.owner.as_deref() == Some(owner)
            && m.table == reference.from.table
            && m.fk
                .as_ref()
                .is_some_and(|fk| fk.eq_ignore_ascii_case(column))
    })
}

/// `None` when the reference cannot point at this row: its `to_when` does not hold on it, or an
/// earlier unconditional rule on the same column decides every source row.
fn plan<'r>(ctx: &'r Ctx<'_>, own: &Own, reference: ResolvedReference) -> Option<Incoming<'r>> {
    let rule = find_rule(ctx, &reference);
    if let Some(rule) = rule
        && !condition::holds(&rule.to_when, |c| own.rows.text(0, c))
    {
        return None;
    }
    let earlier = match &reference.by {
        ReferenceSource::ForeignKey { .. } => Vec::new(),
        _ => earlier_conditions(ctx, &reference, rule)?,
    };
    let values = reference
        .from
        .columns
        .iter()
        .zip(&reference.to.columns)
        .map(|(from, to)| Some((from.clone(), own.text(to)?.to_string())))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    Some(Incoming {
        reference,
        rule,
        earlier,
        values,
    })
}

fn find_rule<'r>(ctx: &'r Ctx<'_>, reference: &ResolvedReference) -> Option<&'r RuleReference> {
    let ([from], [to]) = (
        reference.from.columns.as_slice(),
        reference.to.columns.as_slice(),
    ) else {
        return None;
    };
    ctx.rules.references.iter().find(|r| {
        r.by == reference.by
            && same_column(&r.from, &reference.from.table, from)
            && same_column(&r.to, &reference.to.table, to)
    })
}

fn same_column(column: &ColumnRef, table: &str, name: &str) -> bool {
    column.table == table && column.column.eq_ignore_ascii_case(name)
}

/// The conditional rules on the source column before `rule` (before any unconditional one for an
/// inferred reference); `None` when an unconditional rule comes first.
fn earlier_conditions<'r>(
    ctx: &'r Ctx<'_>,
    reference: &ResolvedReference,
    rule: Option<&'r RuleReference>,
) -> Option<Vec<&'r [Condition]>> {
    let [column] = reference.from.columns.as_slice() else {
        return Some(Vec::new());
    };
    let mut earlier = Vec::new();
    for other in &ctx.rules.references {
        if rule.is_some_and(|r| r.group == other.group) {
            break;
        }
        if !same_column(&other.from, &reference.from.table, column) {
            continue;
        }
        if other.when.is_empty() {
            return None;
        }
        earlier.push(other.when.as_slice());
    }
    Some(earlier)
}

fn group_id(reference: &ResolvedReference) -> String {
    match &reference.by {
        ReferenceSource::Preset { id, .. } | ReferenceSource::Project { id } => id.clone(),
        ReferenceSource::ForeignKey { constraint } => constraint.clone(),
        ReferenceSource::SameName { column, .. } => inferred_id(&reference.from.table, column),
    }
}

async fn group(
    ctx: &Ctx<'_>,
    own: &Own,
    incoming: &Incoming<'_>,
    rows: Option<AppResult<Capped>>,
) -> Group {
    let reference = &incoming.reference;
    let source = &reference.from.table;
    let via = GroupLink::Incoming {
        reference: reference.clone(),
        label: incoming.rule.and_then(|r| r.label.clone()),
    };
    let mut group = Group::new(group_id(reference), source, via, GroupShape::Rows);
    group.columns = columns(ctx, source, incoming.rule);
    group.rows_address = rows_address(ctx, own, incoming);
    match rows {
        Some(Ok(capped)) => {
            group.total = capped.total;
            group.capped = capped.capped;
            group.rows = pivot::rows(ctx, &capped.rows, &group.columns).await;
            group
        }
        Some(Err(e)) => group.failed(e),
        None => group,
    }
}

/// The row has a non-empty value in every target column, so source rows can point at it.
fn has_values(incoming: &Incoming<'_>) -> bool {
    !incoming.values.is_empty() && incoming.values.iter().all(|(_, v)| !v.is_empty())
}

/// The rule's `columns`, else every column of the source table.
fn columns(ctx: &Ctx<'_>, source: &str, rule: Option<&RuleReference>) -> Vec<String> {
    let table_columns = ctx.schema.columns(source);
    match rule {
        Some(rule) if !rule.columns.is_empty() => rule
            .columns
            .iter()
            .map(|name| {
                table_columns
                    .iter()
                    .find(|c| c.eq_ignore_ascii_case(name))
                    .unwrap_or(name)
                    .clone()
            })
            .collect(),
        _ => table_columns.to_vec(),
    }
}

fn filter_sql(ctx: &Ctx<'_>, own: &Own, incoming: &Incoming<'_>) -> String {
    let source = &incoming.reference.from.table;
    let mut parts: Vec<String> = incoming
        .values
        .iter()
        .map(|(column, value)| {
            format!(
                "{} = {}",
                ctx.column(Some(ALIAS), column),
                ctx.literal(value)
            )
        })
        .collect();
    if let Some(rule) = incoming.rule {
        parts.extend(conditions(ctx.dialect, ALIAS, &rule.when));
    }
    for when in &incoming.earlier {
        let held = conditions(ctx.dialect, ALIAS, when).join(" AND ");
        parts.push(format!("NOT COALESCE(({held}), FALSE)"));
    }
    if source == own.table()
        && let Some(itself) = own_key_sql(ctx, own)
    {
        parts.push(format!("NOT ({itself})"));
    }
    parts.join(" AND ")
}

/// A column that references its own table (shared emails) must not find the row itself.
fn own_key_sql(ctx: &Ctx<'_>, own: &Own) -> Option<String> {
    let key = ctx.primary_key(own.table());
    if key.is_empty() {
        return None;
    }
    let parts = key
        .iter()
        .map(|k| {
            Some(format!(
                "{} = {}",
                ctx.column(Some(ALIAS), k),
                ctx.literal(own.text(k)?)
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join(" AND "))
}

/// The source rows as a structured filter, when `=` conditions select exactly them.
fn rows_address(ctx: &Ctx<'_>, own: &Own, incoming: &Incoming<'_>) -> Option<Address> {
    let source = &incoming.reference.from.table;
    if !incoming.earlier.is_empty() || !has_values(incoming) || source == own.table() {
        return None;
    }
    let mut filter: Vec<ColumnEquals> = incoming
        .values
        .iter()
        .map(|(column, value)| ColumnEquals {
            column: column.clone(),
            value: Some(value.clone()),
        })
        .collect();
    for c in incoming.rule.iter().flat_map(|r| &r.when) {
        filter.push(ColumnEquals {
            column: c.column.clone(),
            value: Some(c.matcher.equals()?.to_string()),
        });
    }
    Some(ctx.address(Target::Table {
        table: source.clone(),
        filter,
    }))
}
