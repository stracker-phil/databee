//! Placeholders in preset SQL: `{p}` and `{np}` (prefixes, as text), `{id}`, `{owner.<column>}`,
//! `{meta.<key>}` and `{param.<name>}` (SQL literals). Braces with any other content are SQL,
//! e.g. the regexp quantifier in `'^[0-9a-f]{32}$'`, and stay as written.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder<'a> {
    Prefix,
    NetworkPrefix,
    Id,
    Owner(&'a str),
    Meta(&'a str),
    Param(&'a str),
}

impl Placeholder<'_> {
    fn parse(inner: &str) -> Option<Placeholder<'_>> {
        match inner {
            "p" => Some(Placeholder::Prefix),
            "np" => Some(Placeholder::NetworkPrefix),
            "id" => Some(Placeholder::Id),
            _ => Self::parse_scoped(inner),
        }
    }

    /// `owner.<column>`, `meta.<key>`, `param.<name>`.
    fn parse_scoped(inner: &str) -> Option<Placeholder<'_>> {
        let (scope, name) = inner.split_once('.')?;
        if name.is_empty() {
            return None;
        }
        match scope {
            "owner" => Some(Placeholder::Owner(name)),
            "meta" => Some(Placeholder::Meta(name)),
            "param" => Some(Placeholder::Param(name)),
            _ => None,
        }
    }
}

/// Every placeholder in `sql`, with its byte range, in order.
pub fn scan(sql: &str) -> Vec<(std::ops::Range<usize>, Placeholder<'_>)> {
    let mut found = Vec::new();
    let mut rest = 0;
    while let Some(open) = sql[rest..].find('{').map(|i| rest + i) {
        let Some(close) = sql[open + 1..].find(['{', '}']).map(|i| open + 1 + i) else {
            break;
        };
        // An unclosed brace: the next one starts the candidate.
        if sql.as_bytes()[close] == b'{' {
            rest = close;
            continue;
        }
        if let Some(p) = Placeholder::parse(&sql[open + 1..close]) {
            found.push((open..close + 1, p));
        }
        rest = close + 1;
    }
    found
}

/// Which placeholders a piece of SQL may use besides the prefixes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Allowed<'a> {
    pub id: bool,
    pub owner: bool,
    pub meta: bool,
    /// The declared parameter names, when parameters are allowed at all.
    pub params: Option<&'a [String]>,
}

/// The first placeholder `sql` may not use, as a message.
pub fn check(sql: &str, allowed: Allowed<'_>) -> Result<(), String> {
    for (range, placeholder) in scan(sql) {
        let ok = match placeholder {
            Placeholder::Prefix | Placeholder::NetworkPrefix => true,
            Placeholder::Id => allowed.id,
            Placeholder::Owner(_) => allowed.owner,
            Placeholder::Meta(_) => allowed.meta,
            Placeholder::Param(name) => match allowed.params {
                Some(names) if names.iter().any(|n| n == name) => true,
                Some(_) => return Err(format!("`{{param.{name}}}` names no declared param")),
                None => false,
            },
        };
        if !ok {
            return Err(format!("`{}` is not available here", &sql[range]));
        }
    }
    Ok(())
}

/// `sql` with `{p}` and `{np}` replaced; literal placeholders stay for their consumer.
pub fn with_prefixes(sql: &str, prefix: &str, network_prefix: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut last = 0;
    for (range, placeholder) in scan(sql) {
        let text = match placeholder {
            Placeholder::Prefix => prefix,
            Placeholder::NetworkPrefix => network_prefix,
            _ => continue,
        };
        out.push_str(&sql[last..range.start]);
        out.push_str(text);
        last = range.end;
    }
    out.push_str(&sql[last..]);
    out
}

/// `sql` with `qualifier` (such as `` `db`. ``) before every `{p}` and `{np}` outside quotes, so
/// the tables they start resolve in one database whatever the connection's default is.
pub fn qualify_prefixes(sql: &str, qualifier: &str) -> String {
    let starts: Vec<usize> = scan(sql)
        .into_iter()
        .filter(|(_, p)| matches!(p, Placeholder::Prefix | Placeholder::NetworkPrefix))
        .map(|(range, _)| range.start)
        .collect();
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + starts.len() * qualifier.len());
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if quote.is_none() && starts.contains(&i) {
            out.push_str(qualifier);
        }
        match quote {
            None if matches!(b, b'\'' | b'"' | b'`') => quote = Some(b),
            Some(q) if b == q && bytes.get(i + 1) == Some(&q) => {
                out.push_str(&sql[i..i + 2]);
                i += 2;
                continue;
            }
            Some(q) if b == q => quote = None,
            Some(q) if b == b'\\' && q != b'`' && i + 1 < bytes.len() => {
                let next = sql[i + 1..].chars().next().map_or(1, char::len_utf8);
                out.push_str(&sql[i..i + 1 + next]);
                i += 1 + next;
                continue;
            }
            _ => {}
        }
        let len = sql[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&sql[i..i + len]);
        i += len;
    }
    out
}

/// `sql` with every placeholder replaced by what `value` gives for it; `None` as soon as one has
/// no value, so the caller can skip the SQL.
pub fn render(sql: &str, value: impl Fn(Placeholder<'_>) -> Option<String>) -> Option<String> {
    let mut out = String::with_capacity(sql.len());
    let mut last = 0;
    for (range, placeholder) in scan(sql) {
        out.push_str(&sql[last..range.start]);
        out.push_str(&value(placeholder)?);
        last = range.end;
    }
    out.push_str(&sql[last..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(sql: &str) -> Vec<Placeholder<'_>> {
        scan(sql).into_iter().map(|(_, p)| p).collect()
    }

    #[test]
    fn scan_finds_every_placeholder_kind_in_order() {
        assert_eq!(
            placeholders("{p}a {np}b {id} {owner.total} {meta._key} {param.days}"),
            [
                Placeholder::Prefix,
                Placeholder::NetworkPrefix,
                Placeholder::Id,
                Placeholder::Owner("total"),
                Placeholder::Meta("_key"),
                Placeholder::Param("days"),
            ]
        );
    }

    #[test]
    fn scan_reports_the_byte_range_of_each_placeholder() {
        assert_eq!(
            scan("a{p}b{id}"),
            [(1..4, Placeholder::Prefix), (5..9, Placeholder::Id)]
        );
    }

    #[test]
    fn braces_with_other_content_are_not_placeholders() {
        for sql in [
            "x REGEXP '^[0-9a-f]{32}$'",
            "{unknown}",
            "{param.}",
            "{other.name}",
            "{}",
            "{p",
            "p}",
        ] {
            assert!(placeholders(sql).is_empty(), "{sql}");
        }
    }

    #[test]
    fn an_unclosed_brace_does_not_hide_the_next_placeholder() {
        assert_eq!(scan("x { {p}"), [(4..7, Placeholder::Prefix)]);
    }

    #[test]
    fn with_prefixes_replaces_only_the_prefix_placeholders() {
        let sql = "SELECT * FROM {p}posts JOIN {np}users WHERE id = {id} AND k = {meta._k} \
                   AND o = {owner.c} AND n = {param.n} AND h REGEXP '^[0-9a-f]{32}$'";
        assert_eq!(
            with_prefixes(sql, "wp_2_", "wp_"),
            "SELECT * FROM wp_2_posts JOIN wp_users WHERE id = {id} AND k = {meta._k} \
             AND o = {owner.c} AND n = {param.n} AND h REGEXP '^[0-9a-f]{32}$'"
        );
    }

    #[test]
    fn with_prefixes_accepts_empty_prefixes_and_plain_sql() {
        assert_eq!(with_prefixes("SELECT {p}a", "", ""), "SELECT a");
        assert_eq!(with_prefixes("SELECT 1", "wp_", "wp_"), "SELECT 1");
    }

    const DB: &str = "`wp`.";

    #[test]
    fn qualify_prefixes_qualifies_both_prefixes_outside_quotes() {
        assert_eq!(
            qualify_prefixes(
                "SELECT * FROM {p}posts JOIN {np}users ON {p}posts.a = 1",
                DB
            ),
            "SELECT * FROM `wp`.{p}posts JOIN `wp`.{np}users ON `wp`.{p}posts.a = 1"
        );
    }

    #[test]
    fn qualify_prefixes_leaves_prefixes_inside_quotes_alone() {
        for sql in [
            "SELECT '{p}x'",
            "SELECT \"{p}x\"",
            "SELECT `{p}x`",
            "SELECT 'a ''{p}'' b'",
            "SELECT 'it\\'s {p}x'",
            "SELECT \"say \\\"{np}\\\"\"",
            "SELECT 'a\\\\' '{p}'",
        ] {
            assert_eq!(qualify_prefixes(sql, DB), sql, "{sql}");
        }
    }

    #[test]
    fn qualify_prefixes_resumes_after_a_quote_closes() {
        for (sql, qualified) in [
            ("SELECT 'a' FROM {p}t", "SELECT 'a' FROM `wp`.{p}t"),
            ("SELECT 'a\\'' FROM {p}t", "SELECT 'a\\'' FROM `wp`.{p}t"),
            ("SELECT 'a\\\\' FROM {p}t", "SELECT 'a\\\\' FROM `wp`.{p}t"),
            ("SELECT 'a''' FROM {p}t", "SELECT 'a''' FROM `wp`.{p}t"),
            ("SELECT `a\\` FROM {p}t", "SELECT `a\\` FROM `wp`.{p}t"),
            ("SELECT 'ä' FROM {p}t", "SELECT 'ä' FROM `wp`.{p}t"),
        ] {
            assert_eq!(qualify_prefixes(sql, DB), qualified, "{sql}");
        }
    }

    #[test]
    fn qualify_prefixes_ignores_braces_that_are_not_prefixes() {
        let sql = "SELECT {id}, '{param.x}' FROM t WHERE h REGEXP '^[0-9a-f]{32}$'";

        assert_eq!(qualify_prefixes(sql, DB), sql);
    }

    fn known(placeholder: Placeholder<'_>) -> Option<String> {
        match placeholder {
            Placeholder::Prefix => Some("wp_".into()),
            Placeholder::Id => Some("7".into()),
            Placeholder::Meta("_key") => Some("'k'".into()),
            _ => None,
        }
    }

    #[test]
    fn render_replaces_every_placeholder_with_its_value() {
        assert_eq!(
            render(
                "SELECT * FROM {p}t WHERE id = {id} AND k = {meta._key}",
                known
            ),
            Some("SELECT * FROM wp_t WHERE id = 7 AND k = 'k'".to_string())
        );
    }

    #[test]
    fn render_gives_none_when_one_placeholder_has_no_value() {
        assert_eq!(render("id = {id} AND c = {owner.total}", known), None);
        assert_eq!(render("{meta.other}", known), None);
    }

    #[test]
    fn render_keeps_braces_that_are_not_placeholders() {
        assert_eq!(
            render("h REGEXP '^[0-9a-f]{32}$' AND id = {id}", known),
            Some("h REGEXP '^[0-9a-f]{32}$' AND id = 7".to_string())
        );
        assert_eq!(render("SELECT 1", |_| None), Some("SELECT 1".to_string()));
    }

    #[test]
    fn render_does_not_rescan_substituted_text() {
        let value = |placeholder: Placeholder<'_>| match placeholder {
            Placeholder::Id => Some("{p}".to_string()),
            _ => None,
        };

        assert_eq!(render("{id}", value), Some("{p}".to_string()));
    }

    #[test]
    fn check_allows_prefixes_everywhere_and_the_rest_only_when_enabled() {
        assert_eq!(check("{p}t {np}u", Allowed::default()), Ok(()));
        assert_eq!(
            check("{id}", Allowed::default()),
            Err("`{id}` is not available here".to_string())
        );
        assert_eq!(
            check("{owner.c}", Allowed::default()),
            Err("`{owner.c}` is not available here".to_string())
        );
        assert_eq!(
            check("{meta.k}", Allowed::default()),
            Err("`{meta.k}` is not available here".to_string())
        );
        let everything = Allowed {
            id: true,
            owner: true,
            meta: true,
            params: None,
        };
        assert_eq!(check("{id} {owner.c} {meta.k}", everything), Ok(()));
    }

    #[test]
    fn check_matches_params_against_the_declared_names() {
        let names = vec!["days".to_string()];
        let allowed = Allowed {
            params: Some(&names),
            ..Allowed::default()
        };
        assert_eq!(check("{param.days}", allowed), Ok(()));
        assert_eq!(
            check("{param.hours}", allowed),
            Err("`{param.hours}` names no declared param".to_string())
        );
        assert_eq!(
            check("{param.days}", Allowed::default()),
            Err("`{param.days}` is not available here".to_string())
        );
    }
}
