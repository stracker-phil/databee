//! Pattern groups: rows a `[[pattern]]` rule selects by SQL. Rules with the same name and table
//! on the row's table are one group (their conditions OR'ed); a rule applies when its
//! `owner_when` holds on the row and none of its `{owner.…}`/`{meta.…}` values is NULL or `''`.

use std::collections::HashMap;

use futures_util::future::join_all;

use super::meta::MetaValues;
use super::own::Own;
use super::{Group, GroupLink, GroupRow, GroupShape, ROWS_CAP};
use crate::query::{Ctx, KeyOrder, Rows};
use crate::rules::condition;
use crate::rules::effective::RulePattern;
use crate::rules::placeholder::{Placeholder, render};

pub struct PatternGroup {
    pub group: Group,
    /// The rule's `order` column, whose timestamps go into `events`.
    pub order: Option<String>,
}

pub async fn groups(ctx: &Ctx<'_>, own: &Own, meta: &MetaValues) -> Vec<PatternGroup> {
    let merged = merge(ctx, own.table());
    join_all(merged.iter().map(|rules| group(ctx, own, meta, rules))).await
}

/// The row's patterns, merged by name and table, in rule order.
fn merge<'r>(ctx: &'r Ctx<'_>, owner: &str) -> Vec<Vec<&'r RulePattern>> {
    let mut merged: Vec<Vec<&RulePattern>> = Vec::new();
    for pattern in ctx.rules.patterns_of(owner) {
        let same = |rules: &&mut Vec<&RulePattern>| {
            rules[0].rule.name == pattern.rule.name && rules[0].rule.table == pattern.rule.table
        };
        match merged.iter_mut().find(same) {
            Some(rules) => rules.push(pattern),
            None => merged.push(vec![pattern]),
        }
    }
    merged
}

async fn group(
    ctx: &Ctx<'_>,
    own: &Own,
    meta: &MetaValues,
    rules: &[&RulePattern],
) -> PatternGroup {
    let first = &rules[0].rule;
    let table = &first.table;
    let order = first.order.clone();
    let sql = condition_sql(ctx, own, meta, rules);
    let via = GroupLink::Pattern {
        rule: first.name.clone(),
        sql: sql.clone(),
    };
    let mut group = Group::new(
        format!("pattern:{}:{}", first.name, table),
        table,
        via,
        GroupShape::Rows,
    );
    group.columns = ctx.schema.columns(table).to_vec();
    if sql.is_empty() {
        return PatternGroup { group, order };
    }
    let key_order = KeyOrder::primary(ctx, table).sql(ctx, None);
    let order_sql = match &order {
        Some(column) => format!("{}, {key_order}", ctx.ident(column)),
        None => key_order,
    };
    let cap = first
        .limit
        .map_or(ROWS_CAP, |limit| ROWS_CAP.min(limit as usize));
    match ctx.capped(table, None, &sql, &order_sql, cap).await {
        Ok(capped) => {
            group.columns = capped.rows.names();
            group.total = capped.total;
            group.capped = capped.capped;
            group.rows = group_rows(&capped.rows, &group.columns);
        }
        Err(e) => group = group.failed(e),
    }
    PatternGroup { group, order }
}

/// The conditions of the rules that apply to the row, OR'ed; empty when none does.
fn condition_sql(ctx: &Ctx<'_>, own: &Own, meta: &MetaValues, rules: &[&RulePattern]) -> String {
    rules
        .iter()
        .filter(|p| condition::holds(&p.owner_when, |c| own.rows.text(0, c)))
        .filter_map(|p| {
            render(&ctx.preset_sql(&p.rule.r#where), |placeholder| {
                value(ctx, own, meta, placeholder)
            })
            .ok()
        })
        .map(|sql| format!("({})", sql.trim()))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn group_rows(rows: &Rows, columns: &[String]) -> Vec<GroupRow> {
    let no_currency = |_: usize, _: &str| None;
    rows.values(&no_currency)
        .into_iter()
        .enumerate()
        .map(|(row, values)| GroupRow {
            key: rows.key(row),
            fields: columns.iter().cloned().zip(values).collect(),
        })
        .collect()
}

/// The groups, and the `order` column of each by group id.
pub fn split(found: Vec<PatternGroup>) -> (Vec<Group>, HashMap<String, Option<String>>) {
    let orders = found
        .iter()
        .map(|p| (p.group.id.clone(), p.order.clone()))
        .collect();
    let groups = found.into_iter().map(|p| p.group).collect();
    (groups, orders)
}

/// A value placeholder as an SQL literal. `None` skips the rule.
fn value(
    ctx: &Ctx<'_>,
    own: &Own,
    meta: &MetaValues,
    placeholder: Placeholder<'_>,
) -> Option<String> {
    let literal = |text: &str| (!text.is_empty()).then(|| ctx.literal(text));
    match placeholder {
        Placeholder::Id => {
            let id = own.id(ctx)?;
            match id.parse::<u64>() {
                Ok(number) => Some(number.to_string()),
                Err(_) => literal(id),
            }
        }
        Placeholder::Owner(column) => literal(own.text(column)?),
        Placeholder::Meta(key) => literal(meta.get(key)?.raw.as_text()?),
        Placeholder::Prefix | Placeholder::NetworkPrefix | Placeholder::Param(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Cell, ResultSet};
    use crate::query::testing::{Rig, count, num, result, shop, text};

    use crate::document::meta;
    use crate::document::testing::order;
    use crate::error::AppError;
    use crate::value::{KeyPart, RowKey};

    const LOG: &str = "FROM `wp`.`f_log`";

    fn log_rows(n: usize) -> ResultSet {
        let rows = (1..=n)
            .map(|i| {
                vec![
                    num(&i.to_string()),
                    num("1"),
                    Cell::Temporal("2023-11-14 22:13:20".into()),
                    text(&format!("m{i}")),
                ]
            })
            .collect();
        result(&["log_id", "order_id", "at_gmt", "msg"], rows)
    }

    fn rule(name: &str, table: &str, where_sql: &str, extra: &str) -> String {
        format!(
            "[[pattern]]\nname = \"{name}\"\nowner = \"orders\"\ntable = \"{table}\"\nwhere = \"{where_sql}\"\n{extra}\n"
        )
    }

    fn log_rule(where_sql: &str) -> String {
        rule("audit", "log", where_sql, "")
    }

    async fn groups_of(rig: &Rig, id: &str, status: Cell) -> Vec<PatternGroup> {
        let ctx = rig.ctx().await;
        let own = order(&ctx, id, status, text(""));
        let meta = MetaValues::new(&meta::load(&ctx, &own, &[]).await);
        groups(&ctx, &own, &meta).await
    }

    fn log_queries(rig: &Rig) -> Vec<String> {
        rig.db
            .queries()
            .into_iter()
            .filter(|sql| sql.contains("`f_log`"))
            .collect()
    }

    fn log_key(id: &str) -> Option<RowKey> {
        Some(RowKey(vec![KeyPart {
            column: "log_id".into(),
            value: id.into(),
        }]))
    }

    #[tokio::test]
    async fn a_rule_runs_with_the_id_as_a_number_and_owner_values_as_literals() {
        let rig = Rig::new(
            shop().on(LOG, log_rows(2)),
            &log_rule("order_id = {id} AND msg = {owner.status}"),
        );

        let found = groups_of(&rig, "1", text("it's")).await;

        assert_eq!(
            log_queries(&rig),
            [
                "SELECT * FROM `wp`.`f_log` WHERE (order_id = 1 AND msg = 'it''s') ORDER BY `log_id` LIMIT 21"
            ]
        );
        assert_eq!(found.len(), 1);
        let group = &found[0].group;
        assert_eq!(group.id, "pattern:audit:f_log");
        assert_eq!(group.table, "f_log");
        assert_eq!(group.shape, GroupShape::Rows);
        assert_eq!(
            group.via,
            GroupLink::Pattern {
                rule: "audit".into(),
                sql: "(order_id = 1 AND msg = 'it''s')".into()
            }
        );
        assert_eq!(group.columns, ["log_id", "order_id", "at_gmt", "msg"]);
        assert_eq!((group.total, group.capped, &group.error), (2, false, &None));
        let keys: Vec<Option<RowKey>> = group.rows.iter().map(|r| r.key.clone()).collect();
        assert_eq!(keys, [log_key("1"), log_key("2")]);
        let names: Vec<&str> = group.rows[1]
            .fields
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(names, group.columns);
        assert_eq!(group.rows[1].fields[3].1.raw, text("m2"));
        assert_eq!(found[0].order, None);
    }

    #[tokio::test]
    async fn a_key_that_is_not_a_number_is_quoted() {
        let rig = Rig::new(shop().on(LOG, log_rows(0)), &log_rule("order_id = {id}"));

        groups_of(&rig, "a'b", text("open")).await;

        assert_eq!(
            log_queries(&rig),
            ["SELECT * FROM `wp`.`f_log` WHERE (order_id = 'a''b') ORDER BY `log_id` LIMIT 21"]
        );
    }

    #[tokio::test]
    async fn prefix_placeholders_are_filled_in_and_qualified_with_the_database() {
        let rig = Rig::new(
            shop().on(LOG, log_rows(0)),
            &log_rule("EXISTS (SELECT 1 FROM {p}people) AND 1 = (SELECT 1 FROM {np}people)"),
        );

        let found = groups_of(&rig, "1", text("open")).await;

        assert_eq!(
            found[0].group.via,
            GroupLink::Pattern {
                rule: "audit".into(),
                sql: "(EXISTS (SELECT 1 FROM `wp`.f_people) AND 1 = (SELECT 1 FROM `wp`.f_people))"
                    .into()
            }
        );
    }

    #[tokio::test]
    async fn a_meta_value_is_a_literal_and_an_empty_one_skips_its_rule() {
        let meta_rows = result(
            &["meta_id", "order_id", "k", "v"],
            vec![
                vec![num("1"), num("1"), text("_ref"), text("A-1")],
                vec![num("2"), num("1"), text("_empty"), text("")],
            ],
        );
        let rules = format!(
            "{}{}",
            log_rule("msg = {meta._ref}"),
            log_rule("msg = {meta._empty}")
        );
        let rig = Rig::new(
            shop()
                .on("FROM `wp`.`f_ordermeta`", meta_rows)
                .on(LOG, log_rows(0)),
            &rules,
        );

        groups_of(&rig, "1", text("open")).await;

        assert_eq!(
            log_queries(&rig),
            ["SELECT * FROM `wp`.`f_log` WHERE (msg = 'A-1') ORDER BY `log_id` LIMIT 21"]
        );
    }

    #[tokio::test]
    async fn a_rule_whose_placeholder_has_no_value_is_skipped_without_a_query() {
        let cases = [
            ("msg = {owner.status}", Cell::Null),
            ("msg = {owner.status}", text("")),
            ("msg = {owner.nope}", text("open")),
            ("msg = {meta._missing}", text("open")),
        ];
        for (where_sql, status) in cases {
            let rig = Rig::new(shop().on(LOG, log_rows(2)), &log_rule(where_sql));

            let found = groups_of(&rig, "1", status.clone()).await;

            let group = &found[0].group;
            assert!(log_queries(&rig).is_empty(), "{where_sql} {status:?}");
            assert!(group.rows.is_empty(), "{where_sql} {status:?}");
            assert_eq!(
                (&group.via, group.total, &group.error),
                (
                    &GroupLink::Pattern {
                        rule: "audit".into(),
                        sql: String::new()
                    },
                    0,
                    &None
                ),
                "{where_sql} {status:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_rule_applies_only_where_its_owner_condition_holds() {
        for (status, runs) in [("paid", true), ("open", false)] {
            let rig = Rig::new(
                shop().on(LOG, log_rows(1)),
                &rule(
                    "audit",
                    "log",
                    "order_id = {id}",
                    "owner_when = { status = \"paid\" }",
                ),
            );

            let found = groups_of(&rig, "1", text(status)).await;

            assert_eq!(log_queries(&rig).len(), usize::from(runs), "{status}");
            assert_eq!(found[0].group.rows.len(), usize::from(runs), "{status}");
        }
    }

    #[tokio::test]
    async fn rules_with_the_same_name_on_a_table_are_one_group_of_or_ed_conditions() {
        let rules = [
            log_rule("order_id = {id}"),
            log_rule("msg = {owner.status}"),
            log_rule("msg = {meta._missing}"),
        ]
        .concat();
        let rig = Rig::new(shop().on(LOG, log_rows(1)), &rules);

        let found = groups_of(&rig, "1", text("open")).await;

        assert_eq!(found.len(), 1);
        assert_eq!(
            log_queries(&rig),
            [
                "SELECT * FROM `wp`.`f_log` WHERE (order_id = 1) OR (msg = 'open') ORDER BY `log_id` LIMIT 21"
            ]
        );
        assert_eq!(
            found[0].group.via,
            GroupLink::Pattern {
                rule: "audit".into(),
                sql: "(order_id = 1) OR (msg = 'open')".into()
            }
        );
    }

    #[tokio::test]
    async fn rules_on_another_table_or_under_another_name_are_separate_groups_in_rule_order() {
        let rules = [
            log_rule("order_id = {id}"),
            rule("audit", "people", "person_id = {id}", ""),
            rule("trail", "log", "order_id = {id}", ""),
        ]
        .concat();
        let rig = Rig::new(
            shop()
                .on(
                    "FROM `wp`.`f_people`",
                    result(&["person_id", "name"], Vec::new()),
                )
                .on(LOG, log_rows(1)),
            &rules,
        );

        let found = groups_of(&rig, "1", text("open")).await;

        let ids: Vec<&str> = found.iter().map(|p| p.group.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "pattern:audit:f_log",
                "pattern:audit:f_people",
                "pattern:trail:f_log"
            ]
        );
    }

    #[tokio::test]
    async fn rules_of_another_owner_do_not_show() {
        let rules = rule("audit", "log", "order_id = 1", "")
            .replace("owner = \"orders\"", "owner = \"people\"");
        let rig = Rig::new(shop().on(LOG, log_rows(1)), &rules);

        assert!(groups_of(&rig, "1", text("open")).await.is_empty());
        assert!(log_queries(&rig).is_empty());
    }

    #[tokio::test]
    async fn a_group_beyond_its_cap_keeps_the_cap_and_counts_all_matches() {
        for (limit, cap) in [(None, 20), (Some(5), 5), (Some(50), 20)] {
            let extra = limit.map(|n| format!("limit = {n}")).unwrap_or_default();
            let rig = Rig::new(
                shop()
                    .on("SELECT COUNT(*)", count(99))
                    .on(LOG, log_rows(30)),
                &rule("audit", "log", "order_id = {id}", &extra),
            );

            let found = groups_of(&rig, "1", text("open")).await;

            let group = &found[0].group;
            assert_eq!(
                log_queries(&rig),
                [
                    format!(
                        "SELECT * FROM `wp`.`f_log` WHERE (order_id = 1) ORDER BY `log_id` LIMIT {}",
                        cap + 1
                    ),
                    "SELECT COUNT(*) FROM `wp`.`f_log` WHERE (order_id = 1)".to_string(),
                ],
                "{limit:?}"
            );
            assert_eq!(group.rows.len(), cap, "{limit:?}");
            assert_eq!((group.capped, group.total), (true, 99), "{limit:?}");
        }
    }

    #[tokio::test]
    async fn the_order_column_sorts_first_and_is_reported_for_events() {
        let rig = Rig::new(
            shop().on(LOG, log_rows(1)),
            &rule("audit", "log", "order_id = {id}", "order = \"at_gmt\""),
        );

        let found = groups_of(&rig, "1", text("open")).await;

        assert_eq!(
            log_queries(&rig),
            [
                "SELECT * FROM `wp`.`f_log` WHERE (order_id = 1) ORDER BY `at_gmt`, `log_id` LIMIT 21"
            ]
        );
        assert_eq!(found[0].order.as_deref(), Some("at_gmt"));
    }

    #[tokio::test]
    async fn a_failing_rule_marks_its_group_and_keeps_the_order_column() {
        let rig = Rig::new(
            shop().failing(LOG, AppError::Cancelled),
            &rule("audit", "log", "order_id = {id}", "order = \"at_gmt\""),
        );

        let found = groups_of(&rig, "1", text("open")).await;

        assert!(found[0].group.error.is_some());
        assert!(found[0].group.rows.is_empty());
        assert_eq!(found[0].order.as_deref(), Some("at_gmt"));
    }
}
