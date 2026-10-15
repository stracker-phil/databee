//! Primary-key shorthand for the WHERE field: `3229`, `3229, 3227`, `3220-3229` or a mix
//! become `pk = …`, `pk IN (…)`, `pk BETWEEN … AND …`. Anything else is passed through as SQL.

use super::quote_ident;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Single(u64),
    Range(u64, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdFilter {
    pub parts: Vec<Part>,
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
                let (a, b) = (a.trim().parse::<u64>().ok()?, b.trim().parse::<u64>().ok()?);
                parts.push(if a <= b {
                    Part::Range(a, b)
                } else {
                    Part::Range(b, a)
                });
            }
            None => parts.push(Part::Single(token.parse().ok()?)),
        }
    }
    Some(IdFilter { parts })
}

impl IdFilter {
    pub fn to_sql(&self, pk: &str) -> String {
        let col = quote_ident(pk);
        let singles: Vec<String> = self
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Single(n) => Some(n.to_string()),
                Part::Range(..) => None,
            })
            .collect();
        let mut clauses: Vec<String> = self
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Range(a, b) => Some(format!("{col} BETWEEN {a} AND {b}")),
                Part::Single(_) => None,
            })
            .collect();
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
