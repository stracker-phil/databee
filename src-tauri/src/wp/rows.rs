//! Cell readers for WordPress query results.

use super::lists::Facet;
use crate::db::{Cell, ResultSet};

/// The stored text: NULL (and binary) is `None`, an empty string stays `Some("")`.
pub fn cell_str(cell: &Cell) -> Option<&str> {
    match cell {
        Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => Some(s),
        _ => None,
    }
}

/// `cell_str` as an owned string, keeping `""`.
pub(crate) fn string(cell: &Cell) -> Option<String> {
    cell_str(cell).map(String::from)
}

/// For display: blank is `None`.
pub(crate) fn text(cell: &Cell) -> Option<String> {
    string(cell).filter(|s| !s.is_empty())
}

/// Counts and other unsigned integers.
pub(crate) fn count(cell: &Cell) -> Option<u64> {
    cell_str(cell)?.trim().parse().ok()
}

pub(crate) fn num(cell: &Cell) -> Option<f64> {
    cell_str(cell)?.trim().parse().ok()
}

/// Exact for any `i64`; `"12.0"`-style text is truncated through `f64`.
pub(crate) fn int(cell: &Cell) -> Option<i64> {
    let s = cell_str(cell)?.trim();
    s.parse::<i64>()
        .ok()
        .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
}

/// Case-insensitive.
pub fn column_index(rs: &ResultSet, name: &str) -> Option<usize> {
    rs.columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
}

/// `(value, count)` pairs from two columns, highest count first. The label is the value.
pub(crate) fn facets(rs: &ResultSet, key_col: usize, count_col: usize) -> Vec<Facet> {
    let mut out: Vec<Facet> = rs
        .rows
        .iter()
        .filter_map(|r| {
            let value = text(&r[key_col])?;
            Some(Facet {
                label: value.clone(),
                count: count(&r[count_col]).unwrap_or(0),
                value,
            })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.count));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnMeta;

    fn t(s: &str) -> Cell {
        Cell::Text(s.into())
    }

    fn n(s: &str) -> Cell {
        Cell::Number(s.into())
    }

    fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
        ResultSet {
            columns: columns
                .iter()
                .map(|name| ColumnMeta {
                    name: (*name).into(),
                    type_name: "TEXT".into(),
                    origin: None,
                })
                .collect(),
            rows,
            ..ResultSet::default()
        }
    }

    fn facet(value: &str, count: u64) -> Facet {
        Facet {
            value: value.into(),
            label: value.into(),
            count,
        }
    }

    #[test]
    fn cell_str_keeps_empty_text_apart_from_null() {
        assert_eq!(cell_str(&t("")), Some(""));
        assert_eq!(cell_str(&Cell::Null), None);
        assert_eq!(cell_str(&n("5")), Some("5"));
        assert_eq!(
            cell_str(&Cell::Temporal("2024-01-02".into())),
            Some("2024-01-02")
        );
        assert_eq!(cell_str(&Cell::Json("{}".into())), Some("{}"));
    }

    #[test]
    fn cell_str_is_none_for_binary_and_long_text() {
        let bytes = Cell::Bytes {
            len: 2,
            hex_preview: "00ff".into(),
        };
        let long = Cell::LongText {
            len: 9,
            preview: "abc".into(),
        };
        assert_eq!(cell_str(&bytes), None);
        assert_eq!(cell_str(&long), None);
    }

    #[test]
    fn string_keeps_empty_text() {
        assert_eq!(string(&t("")), Some(String::new()));
        assert_eq!(string(&t("a")), Some("a".to_string()));
        assert_eq!(string(&Cell::Null), None);
    }

    #[test]
    fn text_treats_empty_and_blank_as_none() {
        assert_eq!(text(&t("")), None);
        assert_eq!(text(&Cell::Null), None);
        assert_eq!(text(&t("a")), Some("a".to_string()));
    }

    #[test]
    fn text_keeps_whitespace_only_text() {
        assert_eq!(text(&t("  ")), Some("  ".to_string()));
    }

    #[test]
    fn int_is_exact_beyond_f64_precision() {
        assert_eq!(int(&n("9007199254740993")), Some(9_007_199_254_740_993));
        assert_eq!(int(&n("-9223372036854775808")), Some(i64::MIN));
    }

    #[test]
    fn int_truncates_decimal_text_and_trims() {
        for (input, expected) in [
            ("12.0", Some(12)),
            ("12.9", Some(12)),
            (" 7 ", Some(7)),
            ("-3", Some(-3)),
            ("abc", None),
            ("", None),
        ] {
            assert_eq!(int(&t(input)), expected, "{input:?}");
        }
        assert_eq!(int(&Cell::Null), None);
    }

    #[test]
    fn num_trims_whitespace_and_rejects_non_numbers() {
        assert_eq!(num(&t(" 12.5 ")), Some(12.5));
        assert_eq!(num(&n("-0.25")), Some(-0.25));
        assert_eq!(num(&t("x")), None);
        assert_eq!(num(&Cell::Null), None);
    }

    #[test]
    fn count_reads_unsigned_integers() {
        assert_eq!(count(&n("18446744073709551615")), Some(u64::MAX));
        assert_eq!(count(&n(" 3 ")), Some(3));
        assert_eq!(count(&n("-1")), None);
        assert_eq!(count(&n("1.0")), None);
        assert_eq!(count(&Cell::Null), None);
    }

    #[test]
    fn column_index_ignores_case_and_misses_return_none() {
        let rs = result(&["ID", "Post_Title"], vec![]);
        assert_eq!(column_index(&rs, "id"), Some(0));
        assert_eq!(column_index(&rs, "POST_TITLE"), Some(1));
        assert_eq!(column_index(&rs, "missing"), None);
    }

    #[test]
    fn facets_pair_value_with_count_highest_first() {
        let rs = result(
            &["status", "n"],
            vec![
                vec![t("wc-pending"), n("2")],
                vec![t("wc-completed"), n("10")],
                vec![t("wc-failed"), n("5")],
            ],
        );
        assert_eq!(
            facets(&rs, 0, 1),
            vec![
                facet("wc-completed", 10),
                facet("wc-failed", 5),
                facet("wc-pending", 2)
            ]
        );
    }

    #[test]
    fn facets_skip_blank_values_and_default_unreadable_counts_to_zero() {
        let rs = result(
            &["status", "n"],
            vec![
                vec![Cell::Null, n("4")],
                vec![t(""), n("4")],
                vec![t("a"), Cell::Null],
                vec![t("b"), n("1")],
            ],
        );
        assert_eq!(facets(&rs, 0, 1), vec![facet("b", 1), facet("a", 0)]);
    }

    #[test]
    fn facets_keep_row_order_for_equal_counts() {
        let rs = result(
            &["k", "n"],
            vec![vec![t("b"), n("1")], vec![t("a"), n("1")]],
        );
        assert_eq!(facets(&rs, 0, 1), vec![facet("b", 1), facet("a", 1)]);
    }
}
