use std::collections::{HashMap, HashSet};

use super::marks::{changed_fields, mark, meta_key};
use super::{DocDelta, FlatDocument, FlatGroup, FlatRow};
use crate::document::GroupShape;

/// The differences between row `left` and row `right` of one table, `left` as the old side:
/// groups match by id, the own row column by column, meta rows by key (the n-th row of a key
/// with the n-th), other rows by position.
pub fn compare_documents(left: &FlatDocument, right: &FlatDocument) -> DocDelta {
    let mut delta = DocDelta::default();
    for group in &right.groups {
        let Some(before) = left.group(&group.id) else {
            continue;
        };
        let pairs = match group.shape {
            GroupShape::Meta => pairs_by_meta_key(before, group),
            GroupShape::Fields | GroupShape::Rows => pairs_by_position(before, group),
        };
        for pair in pairs {
            match pair {
                Pair::Both(old_row, new_row) => changed_fields(group, old_row, new_row, &mut delta),
                Pair::Right(row) if !before.capped => delta.added.push(mark(group, row)),
                Pair::Left(row) if !group.capped => delta.removed.push(mark(before, row)),
                Pair::Right(_) | Pair::Left(_) => {}
            }
        }
    }
    delta.sort();
    delta
}

enum Pair<'a> {
    Both(&'a FlatRow, &'a FlatRow),
    Left(&'a FlatRow),
    Right(&'a FlatRow),
}

fn pairs_by_position<'a>(left: &'a FlatGroup, right: &'a FlatGroup) -> Vec<Pair<'a>> {
    let shared = left.rows.len().min(right.rows.len());
    let both = left
        .rows
        .iter()
        .zip(&right.rows)
        .map(|(l, r)| Pair::Both(l, r));
    both.chain(left.rows[shared..].iter().map(Pair::Left))
        .chain(right.rows[shared..].iter().map(Pair::Right))
        .collect()
}

fn pairs_by_meta_key<'a>(left: &'a FlatGroup, right: &'a FlatGroup) -> Vec<Pair<'a>> {
    let occurrences = |group: &'a FlatGroup| {
        let mut seen: HashMap<String, usize> = HashMap::new();
        group
            .rows
            .iter()
            .map(|row| {
                let key = meta_key(row).unwrap_or_default();
                let n = seen.entry(key.clone()).or_default();
                *n += 1;
                ((key, *n), row)
            })
            .collect::<Vec<_>>()
    };
    let lefts = occurrences(left);
    let rights = occurrences(right);
    let by_key: HashMap<&(String, usize), &FlatRow> = lefts.iter().map(|(k, r)| (k, *r)).collect();
    let right_keys: HashSet<&(String, usize)> = rights.iter().map(|(k, _)| k).collect();
    let mut pairs: Vec<Pair> = rights
        .iter()
        .map(|(k, row)| match by_key.get(k) {
            Some(old_row) => Pair::Both(old_row, row),
            None => Pair::Right(row),
        })
        .collect();
    pairs.extend(
        lefts
            .iter()
            .filter(|(k, _)| !right_keys.contains(k))
            .map(|(_, row)| Pair::Left(row)),
    );
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::diff::ValueChange;
    use crate::document::diff::fixtures::*;

    #[test]
    fn compare_aligns_meta_rows_by_key_and_names_both_rows_as_sources() {
        let left = meta_doc(vec![meta_row("1", "a", "1"), meta_row("2", "b", "2")]);
        let right = meta_doc(vec![meta_row("11", "b", "3"), meta_row("12", "c", "4")]);

        let delta = compare_documents(&left, &right);

        assert_eq!(
            delta.changed,
            vec![ValueChange {
                group: "meta".into(),
                label: "b".into(),
                source: prov("ordermeta", "v", "meta_id", "11"),
                old_source: prov("ordermeta", "v", "meta_id", "2"),
                old: text("2"),
                new: text("3"),
            }]
        );
        assert_eq!(labels(&delta.added), ["c"]);
        assert_eq!(delta.added[0].key, Some(key("meta_id", "12")));
        assert_eq!(labels(&delta.removed), ["a"]);
        assert_eq!(delta.removed[0].key, Some(key("meta_id", "1")));
    }

    #[test]
    fn compare_pairs_duplicate_meta_keys_by_occurrence() {
        let left = meta_doc(vec![meta_row("1", "x", "1"), meta_row("2", "x", "2")]);
        let right = meta_doc(vec![
            meta_row("11", "x", "1"),
            meta_row("12", "x", "5"),
            meta_row("13", "x", "6"),
        ]);

        let delta = compare_documents(&left, &right);

        assert_eq!(delta.changed.len(), 1);
        assert_eq!(
            delta.changed[0].old_source,
            prov("ordermeta", "v", "meta_id", "2")
        );
        assert_eq!(
            delta.changed[0].source,
            prov("ordermeta", "v", "meta_id", "12")
        );
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.added[0].key, Some(key("meta_id", "13")));
        assert!(delta.removed.is_empty());
    }

    #[test]
    fn compare_aligns_rows_groups_by_position() {
        let left = rows_doc(vec![order("1", "a"), order("2", "b")], false);
        let right = rows_doc(vec![order("11", "z")], false);

        let delta = compare_documents(&left, &right);

        assert_eq!(delta.changed.len(), 2);
        assert_eq!(delta.changed[0].label, "id");
        assert_eq!(delta.changed[0].old_source, prov("orders", "id", "id", "1"));
        assert_eq!(delta.changed[0].source, prov("orders", "id", "id", "11"));
        assert_eq!(delta.changed[1].label, "status");
        assert_eq!(delta.changed[1].old, text("a"));
        assert_eq!(delta.changed[1].new, text("z"));
        assert!(delta.added.is_empty());
        assert_eq!(labels(&delta.removed), ["id 2"]);
    }

    #[test]
    fn compare_marks_surplus_right_rows_as_added() {
        let left = rows_doc(vec![order("1", "a")], false);
        let right = rows_doc(vec![order("1", "a"), order("2", "b")], false);

        let delta = compare_documents(&left, &right);

        assert!(delta.changed.is_empty() && delta.removed.is_empty());
        assert_eq!(labels(&delta.added), ["id 2"]);
    }
}
