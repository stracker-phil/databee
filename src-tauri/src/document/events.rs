//! `events`: every timestamp of the document as one row, sorted. The kind rules decide what is a
//! timestamp; pattern rows count by the rule's `order` column only. A value whose zone is not
//! stored (`local`, or none) sorts by its wall-clock text; zero dates are no time and left out.

use std::collections::HashMap;

use super::meta;
use super::{Group, GroupLink, GroupRow, GroupShape};
use crate::rules::kinds::Zone;
use crate::value::{Value, ValueKind, utc_seconds};

/// `orders`: the `order` column of each pattern group, by group id.
pub fn group(groups: &[Group], orders: &HashMap<String, Option<String>>) -> Group {
    let mut events: Vec<(i64, GroupRow)> = Vec::new();
    for group in groups {
        for row in &group.rows {
            for (name, value) in timestamps(group, row, orders) {
                let Some(at) = moment(value) else {
                    continue;
                };
                let key = value.source.as_ref().map(|s| s.row.clone());
                events.push((
                    at,
                    GroupRow {
                        key,
                        fields: vec![(name, value.clone())],
                    },
                ));
            }
        }
    }
    events.sort_by_key(|(at, _)| *at);
    let mut group = Group::new("events".into(), "", GroupLink::Events, GroupShape::Rows);
    group.total = events.len() as u64;
    group.rows = events.into_iter().map(|(_, row)| row).collect();
    group
}

/// The timestamps of one row with the name they show under: a column, or the meta key.
fn timestamps<'g>(
    group: &Group,
    row: &'g GroupRow,
    orders: &HashMap<String, Option<String>>,
) -> Vec<(String, &'g Value)> {
    let is_time = |value: &Value| matches!(value.kind, ValueKind::Timestamp { .. });
    match &group.via {
        GroupLink::Meta { .. } => meta::key_value(row)
            .filter(|(_, value)| is_time(value))
            .map(|(key, value)| (key.to_string(), value))
            .into_iter()
            .collect(),
        GroupLink::Pattern { .. } => {
            let Some(Some(order)) = orders.get(&group.id) else {
                return Vec::new();
            };
            row.fields
                .iter()
                .filter(|(name, v)| name.eq_ignore_ascii_case(order) && is_time(v))
                .map(|(name, v)| (name.clone(), v))
                .collect()
        }
        GroupLink::Events => Vec::new(),
        GroupLink::Own | GroupLink::Incoming { .. } => row
            .fields
            .iter()
            .filter(|(_, v)| is_time(v))
            .map(|(name, v)| (name.clone(), v))
            .collect(),
    }
}

/// The value's moment in Unix seconds; a time without a stored zone counts as UTC.
pub fn moment(value: &Value) -> Option<i64> {
    let ValueKind::Timestamp { utc, .. } = value.kind else {
        return None;
    };
    utc.or_else(|| utc_seconds(value.raw.as_text()?, Some(Zone::Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Cell;
    use crate::value::{KeyPart, Provenance, RowKey};

    const ZERO: &str = "0000-00-00 00:00:00";

    fn key(table: &str, id: &str) -> RowKey {
        RowKey(vec![KeyPart {
            column: format!("{table}_id"),
            value: id.into(),
        }])
    }

    fn source(table: &str, column: &str, id: &str) -> Provenance {
        Provenance {
            table: table.into(),
            column: column.into(),
            row: key(table, id),
        }
    }

    fn value(raw: Cell, kind: ValueKind, source: Option<Provenance>) -> Value {
        Value {
            raw,
            kind,
            source,
            style: None,
            secondary: None,
        }
    }

    fn at(utc: i64, table: &str, column: &str, id: &str) -> Value {
        value(
            Cell::Temporal("shown".into()),
            ValueKind::Timestamp {
                utc: Some(utc),
                zone: Some(Zone::Utc),
            },
            Some(source(table, column, id)),
        )
    }

    fn zero_date(table: &str, column: &str, id: &str) -> Value {
        value(
            Cell::Temporal(ZERO.into()),
            ValueKind::Timestamp {
                utc: None,
                zone: Some(Zone::Utc),
            },
            Some(source(table, column, id)),
        )
    }

    fn plain(raw: &str) -> Value {
        value(Cell::Text(raw.into()), ValueKind::Text, None)
    }

    fn group(id: &str, via: GroupLink, rows: Vec<Vec<(&str, Value)>>) -> Group {
        let mut group = Group::new(id.into(), "t", via, GroupShape::Rows);
        group.rows = rows
            .into_iter()
            .enumerate()
            .map(|(i, fields)| GroupRow {
                key: Some(key("t", &i.to_string())),
                fields: fields
                    .into_iter()
                    .map(|(name, v)| (name.to_string(), v))
                    .collect(),
            })
            .collect();
        group
    }

    fn meta_link() -> GroupLink {
        GroupLink::Meta {
            fk: "order_id".into(),
            key: "k".into(),
            value: "v".into(),
        }
    }

    fn pattern_link() -> GroupLink {
        GroupLink::Pattern {
            rule: "audit".into(),
            sql: "1".into(),
        }
    }

    fn names(events: &Group) -> Vec<&str> {
        events.rows.iter().map(|r| r.fields[0].0.as_str()).collect()
    }

    fn no_orders() -> HashMap<String, Option<String>> {
        HashMap::new()
    }

    fn events_of(groups: &[Group], orders: &HashMap<String, Option<String>>) -> Group {
        super::group(groups, orders)
    }

    #[test]
    fn events_are_one_group_with_no_table_of_its_own() {
        let events = events_of(&[], &no_orders());

        assert_eq!(events.id, "events");
        assert_eq!(events.table, "");
        assert_eq!(events.via, GroupLink::Events);
        assert_eq!(events.shape, GroupShape::Rows);
        assert_eq!((events.total, events.rows.len()), (0, 0));
    }

    #[test]
    fn timestamps_of_every_group_are_sorted_by_moment() {
        let groups = [
            group(
                "own",
                GroupLink::Own,
                vec![vec![
                    ("created", at(300, "orders", "created", "1")),
                    ("note", plain("hello")),
                    ("paid", at(100, "orders", "paid", "1")),
                ]],
            ),
            group(
                "meta",
                meta_link(),
                vec![vec![
                    ("k", plain("_shipped")),
                    ("v", at(200, "ordermeta", "v", "5")),
                ]],
            ),
        ];

        let events = events_of(&groups, &no_orders());

        assert_eq!(names(&events), ["paid", "_shipped", "created"]);
        assert_eq!(events.total, 3);
        assert_eq!(events.rows[1].key, Some(key("ordermeta", "5")));
        assert_eq!(
            events.rows[1].fields[0].1.source,
            Some(source("ordermeta", "v", "5"))
        );
    }

    #[test]
    fn events_at_the_same_moment_keep_the_document_order() {
        let groups = [group(
            "own",
            GroupLink::Own,
            vec![vec![
                ("b", at(100, "orders", "b", "1")),
                ("a", at(100, "orders", "a", "1")),
            ]],
        )];

        assert_eq!(names(&events_of(&groups, &no_orders())), ["b", "a"]);
    }

    #[test]
    fn zero_dates_are_no_time_and_left_out() {
        let groups = [group(
            "own",
            GroupLink::Own,
            vec![vec![
                ("closed", zero_date("orders", "closed", "1")),
                ("created", at(100, "orders", "created", "1")),
            ]],
        )];

        let events = events_of(&groups, &no_orders());

        assert_eq!(names(&events), ["created"]);
        assert_eq!(events.total, 1);
    }

    #[test]
    fn a_value_without_a_stored_zone_sorts_by_its_wall_clock_text() {
        let wall_clock = |text: &str, zone| {
            value(
                Cell::Temporal(text.into()),
                ValueKind::Timestamp { utc: None, zone },
                Some(source("orders", "at", "1")),
            )
        };
        let groups = [group(
            "own",
            GroupLink::Own,
            vec![vec![
                ("late", at(1_700_000_100, "orders", "late", "1")),
                (
                    "local",
                    wall_clock("2023-11-14 22:13:20", Some(Zone::Local)),
                ),
                ("none", wall_clock("2023-11-14 22:13:30", None)),
                ("unreadable", wall_clock("sometime", Some(Zone::Local))),
                ("zero", wall_clock(ZERO, Some(Zone::Local))),
            ]],
        )];

        assert_eq!(
            names(&events_of(&groups, &no_orders())),
            ["local", "none", "late"]
        );
    }

    #[test]
    fn a_meta_row_counts_by_its_value_under_the_name_of_its_key() {
        let groups = [group(
            "meta",
            meta_link(),
            vec![
                vec![("k", at(500, "ordermeta", "k", "1")), ("v", plain("x"))],
                vec![("k", plain("_paid")), ("v", at(400, "ordermeta", "v", "2"))],
            ],
        )];

        let events = events_of(&groups, &no_orders());

        assert_eq!(names(&events), ["_paid"]);
    }

    #[test]
    fn a_pattern_row_counts_by_the_order_column_of_its_rule_only() {
        let groups = [group(
            "pattern:audit:log",
            pattern_link(),
            vec![vec![
                ("logged", at(300, "log", "logged", "1")),
                ("edited", at(100, "log", "edited", "1")),
            ]],
        )];
        let cases = [
            ("the order column", Some(Some("LOGGED")), vec!["logged"]),
            ("a rule without order", Some(None), vec![]),
            ("a group without a rule entry", None, vec![]),
        ];
        for (name, order, expected) in cases {
            let mut orders = no_orders();
            if let Some(order) = order {
                orders.insert("pattern:audit:log".to_string(), order.map(str::to_string));
            }

            assert_eq!(names(&events_of(&groups, &orders)), expected, "{name}");
        }
    }

    #[test]
    fn an_events_group_does_not_feed_itself() {
        let earlier = events_of(
            &[group(
                "own",
                GroupLink::Own,
                vec![vec![("created", at(100, "orders", "created", "1"))]],
            )],
            &no_orders(),
        );

        let events = events_of(&[earlier], &no_orders());

        assert!(events.rows.is_empty());
    }
}
