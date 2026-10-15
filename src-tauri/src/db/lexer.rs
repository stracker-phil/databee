//! A tiny MySQL-aware scanner: knows strings, quoted identifiers and comments, so `;`
//! and keywords inside them are ignored. It doesn't try to parse SQL.

#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Word(&'a str),
    Semicolon(usize),
    Other,
}

fn scan(sql: &str) -> Vec<Piece<'_>> {
    let bytes = sql.as_bytes();
    let mut pieces = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' | b'"' | b'`' => {
                i = skip_quoted(bytes, i, c);
                pieces.push(Piece::Other);
            }
            b'#' => i = skip_line(bytes, i),
            b'-' if bytes.get(i + 1) == Some(&b'-')
                && bytes.get(i + 2).is_none_or(|b| b.is_ascii_whitespace()) =>
            {
                i = skip_line(bytes, i)
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = sql[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| i + 2 + end + 2);
            }
            b';' => {
                pieces.push(Piece::Semicolon(i));
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                pieces.push(Piece::Word(&sql[start..i]));
            }
            _ => {
                if !c.is_ascii_whitespace() {
                    pieces.push(Piece::Other);
                }
                i += 1;
            }
        }
    }
    pieces
}

fn skip_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && quote != b'`' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            // Doubled quote is an escaped quote.
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn skip_line(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |p| start + p + 1)
}

/// Splits a script into statements, dropping empty ones and surrounding whitespace.
pub fn split_statements(sql: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut start = 0;
    for piece in scan(sql) {
        if let Piece::Semicolon(pos) = piece {
            statements.push(&sql[start..pos]);
            start = pos + 1;
        }
    }
    statements.push(&sql[start..]);
    statements
        .into_iter()
        .map(str::trim)
        .filter(|s| scan(s).iter().any(|p| !matches!(p, Piece::Semicolon(_))))
        .collect()
}

/// Bare words (keywords and unquoted identifiers) in upper case, in order.
pub fn words(statement: &str) -> Vec<String> {
    scan(statement)
        .into_iter()
        .filter_map(|p| match p {
            Piece::Word(w) => Some(w.to_ascii_uppercase()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_top_level_semicolons() {
        assert_eq!(
            split_statements("SELECT 1; SELECT 2 ;\n"),
            ["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn ignores_semicolons_in_strings_identifiers_and_comments() {
        let sql = "SELECT 'a;b', \"c;d\", `e;f` -- x;y\n FROM t /* ; */; SELECT 2 # ;";
        let parts = split_statements(sql);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].ends_with("FROM t /* ; */"));
        assert_eq!(parts[1], "SELECT 2 # ;");
    }

    #[test]
    fn handles_escaped_quotes() {
        assert_eq!(split_statements(r"SELECT 'it''s; \'x;'; SELECT 3").len(), 2);
    }

    #[test]
    fn comment_only_fragments_are_not_statements() {
        assert_eq!(split_statements("SELECT 1; -- done\n"), ["SELECT 1"]);
    }

    #[test]
    fn double_dash_without_space_is_not_a_comment() {
        assert_eq!(words("SELECT 1--1 FROM t"), ["SELECT", "FROM", "T"]);
    }

    #[test]
    fn words_skip_quoted_content() {
        assert_eq!(
            words("delete from `where` where name = 'where'"),
            ["DELETE", "FROM", "WHERE", "NAME"]
        );
    }
}
