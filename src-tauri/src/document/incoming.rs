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
use crate::rules::condition::{self, Condition, conditions_sql};
use crate::rules::effective::{RuleReference, inferred_id};
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
    let rule = ctx.rules.rule_for(&reference);
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
        if !other.from.is(&reference.from.table, column) {
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
        parts.extend(conditions_sql(ctx.dialect, ALIAS, &rule.when));
    }
    for when in &incoming.earlier {
        let held = conditions_sql(ctx.dialect, ALIAS, when).join(" AND ");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ResultSet};
    use crate::document::testing::{Rig, count, num, order, result, shop, text};
    use crate::error::AppError;

    const LOG_REFERENCES: &str = r#"
[[reference]]
from = "log.order_id"
to = "orders.id"

[[reference]]
from = "log.msg"
to = "orders.status"
"#;

    const PEOPLE_REFERENCE: &str = r#"
[[reference]]
from = "people.name"
to = "orders.status"
"#;

    fn log_parts(parts: &[(usize, usize)]) -> ResultSet {
        let rows = parts
            .iter()
            .map(|(part, log_id)| {
                vec![
                    num(&log_id.to_string()),
                    num("1"),
                    text(""),
                    text("open"),
                    num(&part.to_string()),
                ]
            })
            .collect();
        result(
            &["log_id", "order_id", "at_gmt", "msg", "__databee_part"],
            rows,
        )
    }

    fn from_column<'g>(groups: &'g [Group], column: &str) -> &'g Group {
        groups
            .iter()
            .find(|g| {
                matches!(&g.via, GroupLink::Incoming { reference, .. }
                    if reference.from.columns == [column])
            })
            .unwrap_or_else(|| panic!("no group from {column}"))
    }

    fn first_cells(group: &Group) -> Vec<Cell> {
        group
            .rows
            .iter()
            .map(|row| row.fields[0].1.raw.clone())
            .collect()
    }

    #[tokio::test]
    async fn references_from_one_table_share_one_statement_and_keep_their_own_rows() {
        let mut parts: Vec<(usize, usize)> = (1..=21).map(|i| (0, i)).collect();
        parts.push((1, 5));
        let rig = Rig::new(
            shop()
                .on("SELECT COUNT(*)", count(30))
                .on("UNION ALL", log_parts(&parts)),
            LOG_REFERENCES,
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let groups = groups(&ctx, &own).await;

        assert_eq!(
            rig.db.queries(),
            [
                "(SELECT s.*, 0 AS `__databee_part` FROM `wp`.`f_log` AS s WHERE s.`order_id` = '1' ORDER BY s.`log_id` LIMIT 21) \
                 UNION ALL (SELECT s.*, 1 AS `__databee_part` FROM `wp`.`f_log` AS s WHERE s.`msg` = 'open' ORDER BY s.`log_id` LIMIT 21) \
                 ORDER BY `__databee_part`, `log_id`",
                "SELECT COUNT(*) FROM `wp`.`f_log` AS s WHERE s.`order_id` = '1'",
            ]
        );
        let by_order = from_column(&groups, "order_id");
        assert_eq!(
            (by_order.rows.len(), by_order.total, by_order.capped),
            (20, 30, true)
        );
        let expected: Vec<Cell> = (1..=20).map(|i| num(&i.to_string())).collect();
        assert_eq!(first_cells(by_order), expected);
        let by_msg = from_column(&groups, "msg");
        assert_eq!(
            (by_msg.rows.len(), by_msg.total, by_msg.capped),
            (1, 1, false)
        );
        assert_eq!(first_cells(by_msg), [num("5")]);
    }

    #[tokio::test]
    async fn a_failing_shared_statement_fails_every_group_of_its_table_only() {
        let failure = AppError::InvalidInput("boom".into());
        let rig = Rig::new(
            shop().failing("`f_log`", failure.clone()).on(
                "`f_people`",
                result(&["person_id", "name"], vec![vec![num("7"), text("open")]]),
            ),
            &format!("{LOG_REFERENCES}{PEOPLE_REFERENCE}"),
        );
        let ctx = rig.ctx().await;
        let own = order(&ctx, "1", text("open"), text(""));

        let groups = groups(&ctx, &own).await;

        for column in ["order_id", "msg"] {
            let group = from_column(&groups, column);
            assert_eq!(group.error, Some(failure.to_string()), "{column}");
            assert!(group.rows.is_empty(), "{column}");
        }
        let people = from_column(&groups, "name");
        assert_eq!(people.error, None);
        assert_eq!((people.rows.len(), people.total), (1, 1));
    }

    #[tokio::test]
    async fn a_reference_whose_target_is_empty_in_the_row_reads_nothing() {
        for status in [Cell::Null, text("")] {
            let rig = Rig::new(shop(), PEOPLE_REFERENCE);
            let ctx = rig.ctx().await;
            let own = order(&ctx, "1", status.clone(), text(""));

            let groups = groups(&ctx, &own).await;

            let people = from_column(&groups, "name");
            assert_eq!(
                (
                    people.rows.len(),
                    people.total,
                    people.capped,
                    &people.error
                ),
                (0, 0, false, &None),
                "{status:?}"
            );
            assert_eq!(rig.db.queries(), Vec::<String>::new(), "{status:?}");
        }
    }
}
