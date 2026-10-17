//! Whether the targets of the document's reference values exist: one query per target table for
//! all values that point into it.

use std::collections::{BTreeMap, HashMap};

use futures_util::future::join_all;

use super::{Group, SummaryValue};
use crate::address::{Address, Target};
use crate::db::browse::equals_sql;
use crate::db::{ColumnEquals, ResultSet};
use crate::query::Ctx;
use crate::value::{Value, ValueKind};

/// The conditions that select a target row.
type Lookup = Vec<ColumnEquals>;

/// The rows found per table; `None` when the query failed, so `exists` stays unknown.
type Found = HashMap<String, Option<ResultSet>>;

pub async fn check(ctx: &Ctx<'_>, header: &mut [SummaryValue], groups: &mut [Group]) {
    let mut wanted: BTreeMap<String, Vec<Lookup>> = BTreeMap::new();
    for_each_value(header, groups, |value| {
        for (table, lookup) in targets(ctx, value).into_iter().flatten() {
            let lookups = wanted.entry(table).or_default();
            if !lookups.contains(&lookup) {
                lookups.push(lookup);
            }
        }
    });
    let queries = wanted
        .iter()
        .map(|(table, lookups)| async move { (table.clone(), fetch(ctx, table, lookups).await) });
    let found: Found = join_all(queries).await.into_iter().collect();
    for_each_value(header, groups, |value| {
        let each: Vec<Option<bool>> = targets(ctx, value)
            .into_iter()
            .map(|target| {
                let (table, lookup) = target?;
                Some(matches(found.get(&table)?.as_ref()?, &lookup))
            })
            .collect();
        if each.is_empty() {
            return;
        }
        if let ValueKind::Reference { exists: slot, .. } = &mut value.kind {
            *slot = verdict(&each);
        }
    });
}

/// Found in any target; missing only when every target was looked up.
fn verdict(each: &[Option<bool>]) -> Option<bool> {
    if each.contains(&Some(true)) {
        return Some(true);
    }
    let all_checked = !each.is_empty() && each.iter().all(Option::is_some);
    all_checked.then_some(false)
}

fn for_each_value(
    header: &mut [SummaryValue],
    groups: &mut [Group],
    mut f: impl FnMut(&mut Value),
) {
    for summary in header.iter_mut() {
        f(&mut summary.value);
    }
    for group in groups.iter_mut() {
        for row in &mut group.rows {
            for (_, value) in &mut row.fields {
                f(value);
            }
        }
    }
}

/// The lookup of each target of a reference value that nobody checked yet; `None` for a target
/// that cannot be looked up here (another database, no key, no filter).
fn targets(ctx: &Ctx<'_>, value: &Value) -> Vec<Option<(String, Lookup)>> {
    let ValueKind::Reference {
        targets,
        exists: None,
        ..
    } = &value.kind
    else {
        return Vec::new();
    };
    targets
        .iter()
        .map(|address| {
            (address.database == ctx.database)
                .then(|| lookup(ctx, address))
                .flatten()
        })
        .collect()
}

fn lookup(ctx: &Ctx<'_>, address: &Address) -> Option<(String, Lookup)> {
    match &address.target {
        Target::Row { table, key } => {
            Some((table.clone(), ctx.schema.row_filter(table, key).ok()?))
        }
        Target::Table { table, filter } if !filter.is_empty() => {
            Some((table.clone(), filter.clone()))
        }
        _ => None,
    }
}

async fn fetch(ctx: &Ctx<'_>, table: &str, lookups: &[Lookup]) -> Option<ResultSet> {
    let mut columns: Vec<&str> = Vec::new();
    for lookup in lookups.iter().flatten() {
        if !columns
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&lookup.column))
        {
            columns.push(&lookup.column);
        }
    }
    let tests: Vec<String> = lookups
        .iter()
        .map(|lookup| format!("({})", equals_sql(ctx.dialect, lookup)))
        .collect();
    let select: Vec<String> = columns.iter().map(|c| ctx.ident(c)).collect();
    let sql = format!(
        "SELECT DISTINCT {} FROM {} WHERE {}",
        select.join(", "),
        ctx.table_ref(table),
        tests.join(" OR ")
    );
    match ctx.fetch(&sql).await {
        Ok(result) => Some(result),
        Err(e) => {
            log::warn!("Reference targets in {table} not checked: {e}");
            None
        }
    }
}

/// Compared like the server's default collation does for keys and codes: ASCII case aside.
fn matches(found: &ResultSet, lookup: &Lookup) -> bool {
    let positions: Option<Vec<usize>> = lookup
        .iter()
        .map(|want| {
            found
                .columns
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(&want.column))
        })
        .collect();
    let Some(positions) = positions else {
        return false;
    };
    found.rows.iter().any(|row| {
        lookup.iter().zip(&positions).all(|(want, &i)| {
            match (row.get(i).and_then(|c| c.as_text()), &want.value) {
                (Some(have), Some(want)) => have.eq_ignore_ascii_case(want),
                (None, None) => true,
                _ => false,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ColumnEquals};
    use crate::document::{GroupLink, GroupRow, GroupShape};
    use crate::error::AppError;
    use crate::query::testing::{Rig, num, result, shop, text};
    use crate::value::ReferenceSource;

    const PEOPLE: &str = "FROM `wp`.`f_people`";
    const ORDERS: &str = "FROM `wp`.`f_orders`";

    fn reference(targets: Vec<Address>) -> Value {
        Value {
            raw: num("1"),
            kind: ValueKind::Reference {
                targets,
                by: ReferenceSource::SameName {
                    column: "person_id".into(),
                    indexed: false,
                },
                exists: None,
            },
            source: None,
            style: None,
            secondary: None,
        }
    }

    fn exists_of(value: &Value) -> Option<bool> {
        match &value.kind {
            ValueKind::Reference { exists, .. } => *exists,
            other => panic!("{other:?}"),
        }
    }

    fn group_of(values: Vec<Value>) -> Group {
        let mut group = Group::new("g".into(), "f_log", GroupLink::Own, GroupShape::Rows);
        group.rows = vec![GroupRow {
            key: None,
            fields: values.into_iter().map(|v| ("c".to_string(), v)).collect(),
        }];
        group
    }

    fn checked(group: &Group) -> Vec<Option<bool>> {
        group.rows[0]
            .fields
            .iter()
            .map(|(_, v)| exists_of(v))
            .collect()
    }

    fn people(ids: &[&str]) -> crate::db::ResultSet {
        result(&["person_id"], ids.iter().map(|id| vec![num(id)]).collect())
    }

    #[tokio::test]
    async fn all_values_pointing_into_one_table_share_a_single_query() {
        let rig = Rig::new(
            shop()
                .on(PEOPLE, people(&["7"]))
                .on(ORDERS, result(&["id"], vec![vec![num("1")]])),
            "",
        );
        let ctx = rig.ctx().await;
        let mut header = vec![SummaryValue {
            name: "owner".to_string(),
            value: reference(vec![ctx.row_address("f_people", "7")]),
        }];
        let mut groups = vec![group_of(vec![
            reference(vec![ctx.row_address("f_people", "8")]),
            reference(vec![ctx.row_address("f_orders", "1")]),
            reference(vec![ctx.row_address("f_people", "7")]),
        ])];

        check(&ctx, &mut header, &mut groups).await;

        let mut queries = rig.db.queries();
        queries.sort();
        assert_eq!(
            queries,
            [
                "SELECT DISTINCT `id` FROM `wp`.`f_orders` WHERE (`id` = '1')",
                "SELECT DISTINCT `person_id` FROM `wp`.`f_people` WHERE (`person_id` = '7') OR (`person_id` = '8')",
            ]
        );
        assert_eq!(exists_of(&header[0].value), Some(true));
        assert_eq!(checked(&groups[0]), [Some(false), Some(true), Some(true)]);
    }

    #[tokio::test]
    async fn keys_match_ignoring_ascii_case_like_the_servers_collation() {
        let rig = Rig::new(shop().on(PEOPLE, people(&["abc"])), "");
        let ctx = rig.ctx().await;
        let mut groups = vec![group_of(vec![
            reference(vec![ctx.row_address("f_people", "ABC")]),
            reference(vec![ctx.row_address("f_people", "abd")]),
        ])];

        check(&ctx, &mut [], &mut groups).await;

        assert_eq!(checked(&groups[0]), [Some(true), Some(false)]);
    }

    #[tokio::test]
    async fn a_value_with_several_targets_exists_when_any_target_does() {
        let rig = Rig::new(
            shop()
                .on(PEOPLE, people(&[]))
                .on(ORDERS, result(&["id"], vec![vec![num("1")]])),
            "",
        );
        let ctx = rig.ctx().await;
        let mut groups = vec![group_of(vec![
            reference(vec![
                ctx.row_address("f_people", "7"),
                ctx.row_address("f_orders", "1"),
            ]),
            reference(vec![
                ctx.row_address("f_people", "7"),
                ctx.row_address("f_orders", "2"),
            ]),
        ])];

        check(&ctx, &mut [], &mut groups).await;

        assert_eq!(checked(&groups[0]), [Some(true), Some(false)]);
    }

    #[tokio::test]
    async fn a_failed_query_leaves_only_the_values_of_that_table_unknown() {
        let rig = Rig::new(
            shop()
                .failing(PEOPLE, AppError::Cancelled)
                .on(ORDERS, result(&["id"], vec![vec![num("1")]])),
            "",
        );
        let ctx = rig.ctx().await;
        let mut groups = vec![group_of(vec![
            reference(vec![ctx.row_address("f_people", "7")]),
            reference(vec![ctx.row_address("f_orders", "1")]),
        ])];

        check(&ctx, &mut [], &mut groups).await;

        assert_eq!(checked(&groups[0]), [None, Some(true)]);
    }

    #[tokio::test]
    async fn a_value_that_was_already_checked_is_neither_queried_nor_changed() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;
        let mut known = reference(vec![ctx.row_address("f_people", "7")]);
        if let ValueKind::Reference { exists, .. } = &mut known.kind {
            *exists = Some(false);
        }
        let mut groups = vec![group_of(vec![known])];

        check(&ctx, &mut [], &mut groups).await;

        assert_eq!(checked(&groups[0]), [Some(false)]);
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn targets_that_cannot_be_looked_up_stay_unknown_without_a_query() {
        let rig = Rig::new(shop(), "");
        let ctx = rig.ctx().await;
        let table = |filter| {
            ctx.address(Target::Table {
                table: "f_people".into(),
                filter,
            })
        };
        let elsewhere = Address::new(
            ctx.session.slug(),
            "other",
            Target::Row {
                table: "f_people".into(),
                key: "7".into(),
            },
        );
        let cases = [
            ("another database", elsewhere),
            ("a table without a filter", table(Vec::new())),
            ("a table without a key", ctx.row_address("f_unknown", "7")),
        ];
        for (name, address) in cases {
            let mut groups = vec![group_of(vec![reference(vec![address])])];

            check(&ctx, &mut [], &mut groups).await;

            assert_eq!(checked(&groups[0]), [None], "{name}");
        }
        assert!(rig.db.queries().is_empty());
    }

    #[tokio::test]
    async fn a_table_target_is_looked_up_by_its_filter_and_null_matches_null() {
        let rig = Rig::new(
            shop().on(
                "FROM `wp`.`f_ordermeta`",
                result(&["k", "order_id"], vec![vec![text("_ext"), Cell::Null]]),
            ),
            "",
        );
        let ctx = rig.ctx().await;
        let filtered = |filter: Vec<(&str, Option<&str>)>| {
            ctx.address(Target::Table {
                table: "f_ordermeta".into(),
                filter: filter
                    .into_iter()
                    .map(|(column, value)| ColumnEquals {
                        column: column.into(),
                        value: value.map(String::from),
                    })
                    .collect(),
            })
        };
        let mut groups = vec![group_of(vec![
            reference(vec![filtered(vec![
                ("k", Some("_ext")),
                ("order_id", None),
            ])]),
            reference(vec![filtered(vec![("k", Some("_other"))])]),
        ])];

        check(&ctx, &mut [], &mut groups).await;

        assert_eq!(
            rig.db.queries(),
            [
                "SELECT DISTINCT `k`, `order_id` FROM `wp`.`f_ordermeta` WHERE (`k` = '_ext' AND `order_id` IS NULL) OR (`k` = '_other')"
            ]
        );
        assert_eq!(checked(&groups[0]), [Some(true), Some(false)]);
    }
}
