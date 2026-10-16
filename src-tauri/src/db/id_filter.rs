//! Primary-key shorthand for the WHERE field: `3229`, `3229, 3227`, `3220-3229` or a mix
//! become `pk = …`, `pk IN (…)`, `pk BETWEEN … AND …`. Anything else is passed through as SQL.

use super::quote_ident;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Part {
    Single(u64),
    Range(u64, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdFilter {
    pub(super) parts: Vec<Part>,
}

/// Parses the shorthand; `None` when the input is not purely IDs, ranges and commas.
pub fn parse_ids(input: &str) -> Option<IdFilter> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for token in text.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        match token.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (parse_id(a)?, parse_id(b)?);
                parts.push(Part::Range(a.min(b), a.max(b)));
            }
            None => parts.push(Part::Single(parse_id(token)?)),
        }
    }
    Some(IdFilter { parts })
}

/// Digits only: `u64::from_str` would also accept a leading `+`, and overflow is `None`.
fn parse_id(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

impl IdFilter {
    pub fn to_sql(&self, pk: &str) -> String {
        let col = quote_ident(pk);
        let mut singles = Vec::new();
        let mut clauses = Vec::new();
        for part in &self.parts {
            match part {
                Part::Single(n) => singles.push(n.to_string()),
                Part::Range(a, b) => clauses.push(format!("{col} BETWEEN {a} AND {b}")),
            }
        }
        match singles.len() {
            0 => {}
            1 => clauses.push(format!("{col} = {}", singles[0])),
            _ => clauses.push(format!("{col} IN ({})", singles.join(", "))),
        }
        if clauses.len() == 1 {
            clauses.remove(0)
        } else {
            format!("({})", clauses.join(" OR "))
        }
    }

    /// Short human label for the chip next to the filter field.
    pub fn label(&self, pk: &str) -> String {
        let items: Vec<String> = self
            .parts
            .iter()
            .map(|p| match p {
                Part::Single(n) => n.to_string(),
                Part::Range(a, b) => format!("{a}–{b}"),
            })
            .collect();
        match self.parts.as_slice() {
            [Part::Single(n)] => format!("{pk} = {n}"),
            _ => format!("{pk} in ({})", items.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_maps_to_sql() {
        let cases = [
            ("3229", "`ID` = 3229", "ID = 3229"),
            ("3229, 3227", "`ID` IN (3229, 3227)", "ID in (3229, 3227)"),
            (
                "3220-3229",
                "`ID` BETWEEN 3220 AND 3229",
                "ID in (3220–3229)",
            ),
            (
                " 3220 - 3225 ,3229 ",
                "(`ID` BETWEEN 3220 AND 3225 OR `ID` = 3229)",
                "ID in (3220–3225, 3229)",
            ),
            (
                "1-3,7,9-10,12",
                "(`ID` BETWEEN 1 AND 3 OR `ID` BETWEEN 9 AND 10 OR `ID` IN (7, 12))",
                "ID in (1–3, 7, 9–10, 12)",
            ),
            ("9-5", "`ID` BETWEEN 5 AND 9", "ID in (5–9)"),
        ];
        for (input, sql, label) in cases {
            let f = parse_ids(input).unwrap_or_else(|| panic!("{input} should parse"));
            assert_eq!(f.to_sql("ID"), sql, "{input}");
            assert_eq!(f.label("ID"), label, "{input}");
        }
    }

    #[test]
    fn u64_max_parses_and_one_more_overflows() {
        assert_eq!(
            parse_ids("18446744073709551615").map(|f| f.to_sql("ID")),
            Some("`ID` = 18446744073709551615".to_string())
        );
        for input in [
            "18446744073709551616",
            "18446744073709551616-1",
            "1-18446744073709551616",
        ] {
            assert!(parse_ids(input).is_none(), "{input}");
        }
    }

    #[test]
    fn signs_and_chained_ranges_are_rejected() {
        for input in ["+5", "1-+5", "+1-5", "1-2-3"] {
            assert!(parse_ids(input).is_none(), "{input}");
        }
    }

    #[test]
    fn leading_zeros_are_normalized() {
        let f = parse_ids("007").unwrap();
        assert_eq!(f.to_sql("ID"), "`ID` = 7");
        assert_eq!(f.label("ID"), "ID = 7");
    }

    #[test]
    fn duplicate_ids_are_kept() {
        let f = parse_ids("5,5").unwrap();
        assert_eq!(f.to_sql("ID"), "`ID` IN (5, 5)");
        assert_eq!(f.label("ID"), "ID in (5, 5)");
    }

    #[test]
    fn anything_else_is_not_an_id_filter() {
        for input in [
            "",
            "  ",
            "post_type = 'page'",
            "3229 OR 1=1",
            "3229,",
            ",3229",
            "a-b",
            "3229 3227",
            "-5",
            "1.5",
        ] {
            assert!(parse_ids(input).is_none(), "{input:?} must pass through");
        }
    }
}
