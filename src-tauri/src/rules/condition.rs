//! Conditions (`when`, `to_when`) and value matches, evaluated in memory over the rows of a page,
//! so a conditional rule costs no query per row. Comparisons are exact (case-sensitive), whatever
//! the column's collation, so a rule means the same on every server.

use regex::Regex;

use super::types::{Conditions, Match};

/// A `Match` ready to test values. LIKE patterns and regular expressions are compiled once.
#[derive(Debug, Clone)]
pub enum Matcher {
    Eq(String),
    In(Vec<String>),
    Glob(String),
    Pattern(Regex),
    Not(Box<Matcher>),
}

impl Matcher {
    pub fn compile(rule: &Match) -> Result<Self, String> {
        match rule {
            Match::Eq(value) => Ok(Self::Eq(value.clone())),
            Match::In(values) => Ok(Self::In(values.clone())),
            Match::Glob { glob } => Ok(Self::Glob(glob.clone())),
            Match::Like { like } => Self::like(like),
            Match::Regexp { regexp } => Self::regexp(regexp),
            Match::Not { not } => Ok(Self::Not(Box::new(Self::compile(not)?))),
        }
    }

    /// SQL LIKE: `%` is any run of characters, `_` one character, `\` escapes the next.
    pub fn like(pattern: &str) -> Result<Self, String> {
        Self::regexp(&like_regex(pattern))
    }

    /// Unanchored, as SQL `REGEXP`.
    pub fn regexp(pattern: &str) -> Result<Self, String> {
        Regex::new(pattern)
            .map(Self::Pattern)
            .map_err(|e| format!("invalid regular expression `{pattern}`: {e}"))
    }

    /// NULL (and a value only known as a preview) matches nothing, not even a negation, as
    /// `NULL = 'x'` and `NOT (NULL = 'x')` are both not true in SQL.
    pub fn matches(&self, value: Option<&str>) -> bool {
        value.is_some_and(|text| self.matches_text(text))
    }

    fn matches_text(&self, text: &str) -> bool {
        match self {
            Self::Eq(value) => value == text,
            Self::In(values) => values.iter().any(|v| v == text),
            Self::Glob(glob) => glob_match(glob, text),
            Self::Pattern(regex) => regex.is_match(text),
            Self::Not(inner) => !inner.matches_text(text),
        }
    }

    /// The value an address filter can carry: only `=` narrows a target address.
    pub fn equals(&self) -> Option<&str> {
        match self {
            Self::Eq(value) => Some(value),
            _ => None,
        }
    }
}

/// One compiled `column = match` of a condition list. `rule` keeps the match as written, so
/// coverage can say it in SQL (a compiled LIKE and REGEXP look the same).
#[derive(Debug, Clone)]
pub struct Condition {
    pub column: String,
    pub matcher: Matcher,
    pub rule: Match,
}

/// All of `conditions`, compiled; the error names the column.
pub fn compile(conditions: &Conditions) -> Result<Vec<Condition>, String> {
    conditions
        .iter()
        .map(|(column, rule)| {
            let matcher = Matcher::compile(rule).map_err(|e| format!("`{column}`: {e}"))?;
            Ok(Condition {
                column: column.clone(),
                matcher,
                rule: rule.clone(),
            })
        })
        .collect()
}

/// Whether every condition holds on a row. `value` gives a column's text, `None` for NULL; a
/// column the row does not have fails its condition.
pub fn holds<'a>(
    conditions: &[Condition],
    value: impl Fn(&str) -> Option<Option<&'a str>>,
) -> bool {
    conditions.iter().all(|c| match value(&c.column) {
        Some(text) => c.matcher.matches(text),
        None => false,
    })
}

/// `*` matches any run of characters (also none); everything else is literal.
pub fn glob_match(glob: &str, text: &str) -> bool {
    let (glob, text) = (glob.as_bytes(), text.as_bytes());
    let (mut g, mut t) = (0, 0);
    // The last `*` seen and the text position it was tried at, for backtracking.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if g < glob.len() && glob[g] == b'*' {
            star = Some((g, t));
            g += 1;
        } else if g < glob.len() && glob[g] == text[t] {
            g += 1;
            t += 1;
        } else if let Some((star_g, star_t)) = star {
            g = star_g + 1;
            t = star_t + 1;
            star = Some((star_g, star_t + 1));
        } else {
            return false;
        }
    }
    glob[g..].iter().all(|b| *b == b'*')
}

/// Column names compare ASCII case-insensitively, as MySQL does.
pub fn column_glob_match(glob: &str, column: &str) -> bool {
    glob_match(&glob.to_ascii_lowercase(), &column.to_ascii_lowercase())
}

fn like_regex(pattern: &str) -> String {
    let mut out = String::from("(?s)^");
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' => out.push_str(".*"),
            '_' => out.push('.'),
            '\\' => {
                let escaped = chars.next().unwrap_or('\\');
                out.push_str(&regex::escape(&escaped.to_string()));
            }
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::preset;

    fn matcher(rule: Match) -> Matcher {
        Matcher::compile(&rule).unwrap()
    }

    fn not(rule: Match) -> Match {
        Match::Not {
            not: Box::new(rule),
        }
    }

    fn glob(pattern: &str) -> Match {
        Match::Glob {
            glob: pattern.into(),
        }
    }

    fn like(pattern: &str) -> Match {
        Match::Like {
            like: pattern.into(),
        }
    }

    fn regexp(pattern: &str) -> Match {
        Match::Regexp {
            regexp: pattern.into(),
        }
    }

    #[test]
    fn each_match_operator_decides_a_text() {
        let cases = [
            (Match::Eq("a".into()), "a", true),
            (Match::Eq("a".into()), "A", false),
            (Match::In(vec!["a".into(), "b".into()]), "b", true),
            (Match::In(vec!["a".into(), "b".into()]), "c", false),
            (glob("_who*"), "_whoami", true),
            (glob("_who*"), "who", false),
            (glob("*"), "", true),
            (glob("a*c"), "abbc", true),
            (glob("a*c"), "abd", false),
            (glob("a*b*c"), "aXbYbZc", true),
            (like("wa%"), "warm", true),
            (like("wa%"), "cold", false),
            (like("a_c"), "abc", true),
            (like("a_c"), "ac", false),
            (like("a.c"), "abc", false),
            (like("a.c"), "a.c", true),
            (like("a%"), "a\nb", true),
            (like(r"50\%"), "50%", true),
            (like(r"50\%"), "500", false),
            (like("WA%"), "warm", false),
            (regexp("^[0-9]+$"), "123", true),
            (regexp("^[0-9]+$"), "12a", false),
            (regexp("ab"), "xaby", true),
            (not(Match::Eq("a".into())), "b", true),
            (not(Match::Eq("a".into())), "a", false),
            (not(not(Match::Eq("a".into()))), "a", true),
            (not(regexp("^x")), "xy", false),
        ];
        for (rule, text, expected) in cases {
            let label = format!("{rule} on {text:?}");
            assert_eq!(matcher(rule).matches(Some(text)), expected, "{label}");
        }
    }

    #[test]
    fn null_matches_nothing_not_even_a_negation() {
        let rules = [
            Match::Eq("a".into()),
            Match::In(vec!["a".into()]),
            glob("*"),
            like("%"),
            regexp(""),
            not(Match::Eq("a".into())),
        ];
        for rule in rules {
            let label = rule.to_string();
            assert!(!matcher(rule).matches(None), "{label}");
        }
    }

    #[test]
    fn only_an_equality_narrows_a_target_address() {
        assert_eq!(matcher(Match::Eq("a".into())).equals(), Some("a"));
        for rule in [
            Match::In(vec!["a".into()]),
            glob("a"),
            like("a"),
            regexp("a"),
            not(Match::Eq("a".into())),
        ] {
            let label = rule.to_string();
            assert_eq!(matcher(rule).equals(), None, "{label}");
        }
    }

    #[test]
    fn an_invalid_regular_expression_does_not_compile() {
        let error = Matcher::compile(&regexp("(")).unwrap_err();

        assert!(
            error.starts_with("invalid regular expression `(`"),
            "{error}"
        );
        assert!(Matcher::compile(&not(regexp("("))).is_err());
    }

    #[test]
    fn an_invalid_regular_expression_is_a_load_error_in_a_reference_and_a_style() {
        let reference = "[preset]\nname = \"t\"\n[[reference]]\nfrom = \"a.b\"\nwhen = { c = { regexp = \"(\" } }\nto = \"d.e\"\n";
        let style = "[preset]\nname = \"t\"\n[[style]]\ncolumns = [\"a.b\"]\nmatch = \"REGEXP\"\nvalue = \"(\"\ncolor = \"red\"\n";

        let reference_error = preset::parse("t.toml", reference).unwrap_err();
        let style_error = preset::parse("t.toml", style).unwrap_err();

        assert!(
            reference_error
                .to_string()
                .contains("invalid regular expression `(`"),
            "{reference_error}"
        );
        assert_eq!(style_error.field.as_deref(), Some("value"));
        assert_eq!(style_error.rule.map(|r| r.section), Some("style"));
    }

    #[test]
    fn conditions_hold_when_every_column_matches() {
        let conditions = compile(&Conditions::from([
            ("k".to_string(), Match::Eq("x".into())),
            ("n".to_string(), regexp("^[0-9]+$")),
        ]))
        .unwrap();
        let row = |k: Option<&'static str>, n: Option<&'static str>| {
            move |column: &str| match column {
                "k" => Some(k),
                "n" => Some(n),
                _ => None,
            }
        };

        assert!(holds(&conditions, row(Some("x"), Some("12"))));
        assert!(!holds(&conditions, row(Some("y"), Some("12"))));
        assert!(!holds(&conditions, row(Some("x"), Some("ab"))));
        assert!(!holds(&conditions, row(Some("x"), None)));
    }

    #[test]
    fn a_condition_on_a_column_the_row_lacks_fails_and_no_conditions_hold() {
        let conditions = compile(&Conditions::from([(
            "k".to_string(),
            Match::Eq("x".into()),
        )]))
        .unwrap();

        assert!(!holds(&conditions, |_: &str| None));
        assert!(holds(&[], |_: &str| None));
    }

    #[test]
    fn compiling_names_the_column_of_a_failing_condition() {
        let error = compile(&Conditions::from([("k".to_string(), regexp("("))])).unwrap_err();

        assert!(
            error.starts_with("`k`: invalid regular expression"),
            "{error}"
        );
    }

    #[test]
    fn column_globs_ignore_letter_case() {
        assert!(column_glob_match("*_GMT", "post_date_gmt"));
        assert!(column_glob_match("id", "ID"));
        assert!(!column_glob_match("*_gmt", "post_date"));
    }
}
