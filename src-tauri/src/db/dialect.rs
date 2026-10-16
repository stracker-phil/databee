//! SQL spelling that differs between servers.

#[derive(Clone, Copy, Debug)]
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

    pub fn sql_string(&self, value: &str) -> String {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
    }

    /// `database`.`table`, both parts quoted.
    pub fn table_ref(&self, database: &str, table: &str) -> String {
        format!("{}.{}", self.quote_ident(database), self.quote_ident(table))
    }

    pub fn system_schemas(&self) -> &'static [&'static str] {
        &["information_schema", "mysql", "performance_schema", "sys"]
    }
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
}
