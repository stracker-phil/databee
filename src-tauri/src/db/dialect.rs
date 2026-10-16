//! SQL spelling that differs between servers.

/// A user's own `SET sql_mode` on the editor connection is picked up only on reconnect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialect {
    pub backslash_escapes: bool,
}

impl Dialect {
    pub const MYSQL: Dialect = Dialect {
        backslash_escapes: true,
    };

    /// Backtick-quoted, with inner backticks doubled.
    pub fn quote_ident(&self, name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    /// Follows the server's `sql_mode`: `NO_BACKSLASH_ESCAPES` turns backslashes into
    /// plain characters, so the session reads it at connect.
    pub fn from_sql_mode(sql_mode: &str) -> Dialect {
        Dialect {
            backslash_escapes: !sql_mode
                .split(',')
                .any(|mode| mode.trim().eq_ignore_ascii_case("NO_BACKSLASH_ESCAPES")),
        }
    }

    pub fn sql_string(&self, value: &str) -> String {
        if !self.backslash_escapes {
            return format!("'{}'", value.replace('\'', "''"));
        }
        let mut quoted = String::with_capacity(value.len() + 2);
        quoted.push('\'');
        for c in value.chars() {
            match c {
                '\\' => quoted.push_str("\\\\"),
                '\'' => quoted.push_str("''"),
                '\0' => quoted.push_str("\\0"),
                '\n' => quoted.push_str("\\n"),
                '\r' => quoted.push_str("\\r"),
                '\x1a' => quoted.push_str("\\Z"),
                c => quoted.push(c),
            }
        }
        quoted.push('\'');
        quoted
    }

    /// A LIKE operand from a pattern that escapes `%`, `_` and `\` with `\` (see
    /// `escape_like`). Without backslash escapes the server has no default LIKE escape
    /// character, so the clause names it.
    pub fn like(&self, pattern: &str) -> String {
        let literal = self.sql_string(pattern);
        if self.backslash_escapes {
            literal
        } else {
            format!("{literal} ESCAPE '\\'")
        }
    }

    /// Matches `value` anywhere, taken literally.
    pub fn like_contains(&self, value: &str) -> String {
        self.like(&format!("%{}%", escape_like(value)))
    }

    /// `database`.`table`, both parts quoted.
    pub fn table_ref(&self, database: &str, table: &str) -> String {
        format!("{}.{}", self.quote_ident(database), self.quote_ident(table))
    }

    pub fn system_schemas(&self) -> &'static [&'static str] {
        &["information_schema", "mysql", "performance_schema", "sys"]
    }
}

/// Makes `%`, `_` and `\` literal in a LIKE pattern used with `Dialect::like`.
pub fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers_and_escapes_backticks() {
        assert_eq!(Dialect::MYSQL.quote_ident("wp_posts"), "`wp_posts`");
        assert_eq!(Dialect::MYSQL.quote_ident("we`ird"), "`we``ird`");
    }

    #[test]
    fn table_ref_quotes_both_parts() {
        assert_eq!(Dialect::MYSQL.table_ref("d`b", "t`b"), "`d``b`.`t``b`");
    }

    #[test]
    fn sql_string_doubles_quotes_and_escapes_backslashes() {
        for (input, expected) in [
            ("plain", "'plain'"),
            ("it's", "'it''s'"),
            (r"a\b", r"'a\\b'"),
            (r"\'", r"'\\'''"),
            ("", "''"),
        ] {
            assert_eq!(Dialect::MYSQL.sql_string(input), expected, "{input}");
        }
    }

    const NO_BACKSLASH: Dialect = Dialect {
        backslash_escapes: false,
    };

    #[test]
    fn without_backslash_escapes_only_quotes_are_doubled() {
        assert_eq!(NO_BACKSLASH.sql_string(r"a\b'c"), r"'a\b''c'");
        assert_eq!(Dialect::MYSQL.sql_string(r"a\b'c"), r"'a\\b''c'");
    }

    #[test]
    fn control_characters_are_escaped_only_with_backslash_escapes() {
        let value = "\0\n\r\x1a";
        assert_eq!(Dialect::MYSQL.sql_string(value), r"'\0\n\r\Z'");
        assert_eq!(NO_BACKSLASH.sql_string(value), "'\0\n\r\x1a'");
    }

    #[test]
    fn sql_mode_decides_backslash_escapes() {
        for (mode, backslash_escapes) in [
            ("", true),
            ("ONLY_FULL_GROUP_BY", true),
            ("STRICT_TRANS_TABLES,NO_BACKSLASH_ESCAPES", false),
            ("no_backslash_escapes", false),
            ("ONLY_FULL_GROUP_BY, No_Backslash_Escapes", false),
        ] {
            assert_eq!(
                Dialect::from_sql_mode(mode).backslash_escapes,
                backslash_escapes,
                "{mode}"
            );
        }
    }

    #[test]
    fn like_contains_escapes_wildcards_and_names_the_escape_character_when_needed() {
        let value = r"a%b_c\d";
        assert_eq!(Dialect::MYSQL.like_contains(value), r"'%a\\%b\\_c\\\\d%'");
        assert_eq!(
            NO_BACKSLASH.like_contains(value),
            r"'%a\%b\_c\\d%' ESCAPE '\'"
        );
    }

    #[test]
    fn like_passes_an_escaped_pattern_through() {
        assert_eq!(
            Dialect::MYSQL.like(r"\_transient\_%"),
            r"'\\_transient\\_%'"
        );
        assert_eq!(
            NO_BACKSLASH.like(r"\_transient\_%"),
            r"'\_transient\_%' ESCAPE '\'"
        );
    }
}
