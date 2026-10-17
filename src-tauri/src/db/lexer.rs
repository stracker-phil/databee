//! A tiny MySQL-aware scanner: knows strings, quoted identifiers, variables and comments, so
//! `;` and keywords inside them are ignored. It doesn't try to parse SQL.
//!
//! Known limits, inherited by `split_statements` and so by `run_script`'s result count:
//! backslash is always an escape inside strings (`NO_BACKSLASH_ESCAPES` is ignored), `"` always
//! quotes a string (`ANSI_QUOTES` is ignored), and `DELIMITER` is not supported.

use std::ops::Range;

/// `Word` is a keyword or unquoted identifier; `Variable` (`@name`, `@@name`, `@'quoted'`) never
/// matches a keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Number,
    Variable,
    Quoted,
    Comment,
    Semicolon,
    Open,
    Close,
    Other,
}

/// `start` is the byte offset of `text` in the scanned SQL. `depth` is the parenthesis nesting;
/// `(` and `)` carry the depth outside them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece<'a> {
    kind: Kind,
    pub text: &'a str,
    start: usize,
    pub depth: usize,
}

impl Piece<'_> {
    pub fn is_keyword(&self, keyword: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(keyword)
    }

    pub fn is_comment(&self) -> bool {
        self.kind == Kind::Comment
    }

    pub fn is_open(&self) -> bool {
        self.kind == Kind::Open
    }

    pub fn is_symbol(&self, symbol: &str) -> bool {
        self.kind == Kind::Other && self.text == symbol
    }

    /// Byte offsets of the piece in the scanned SQL.
    pub fn span(&self) -> Range<usize> {
        self.start..self.start + self.text.len()
    }
}

struct Scanner<'a> {
    sql: &'a str,
    pos: usize,
    depth: usize,
    in_executable_comment: bool,
    terminated: bool,
}

/// Pieces of `sql` in order, whitespace skipped.
fn scan(sql: &str) -> Scanner<'_> {
    Scanner {
        sql,
        pos: 0,
        depth: 0,
        in_executable_comment: false,
        terminated: true,
    }
}

impl<'a> Iterator for Scanner<'a> {
    type Item = Piece<'a>;

    fn next(&mut self) -> Option<Piece<'a>> {
        let bytes = self.sql.as_bytes();
        self.pos += bytes[self.pos..]
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count();
        if self.pos >= bytes.len() {
            if self.in_executable_comment {
                self.in_executable_comment = false;
                self.terminated = false;
            }
            return None;
        }
        let start = self.pos;
        let (kind, end) = self.lex(start);
        self.pos = end;
        let depth = match kind {
            Kind::Open => {
                self.depth += 1;
                self.depth - 1
            }
            Kind::Close => {
                self.depth = self.depth.saturating_sub(1);
                self.depth
            }
            _ => self.depth,
        };
        Some(Piece {
            kind,
            text: &self.sql[start..end],
            start,
            depth,
        })
    }
}

impl Scanner<'_> {
    /// The kind and end offset of the piece starting at `i`.
    fn lex(&mut self, i: usize) -> (Kind, usize) {
        let bytes = self.sql.as_bytes();
        let next = bytes.get(i + 1).copied();
        match bytes[i] {
            b'\'' | b'"' | b'`' => (Kind::Quoted, self.quoted_end(i)),
            b'@' => (Kind::Variable, self.variable_end(i)),
            b'#' => (Kind::Comment, line_end(bytes, i)),
            b'-' if next == Some(b'-') && bytes.get(i + 2).is_none_or(u8::is_ascii_whitespace) => {
                (Kind::Comment, line_end(bytes, i))
            }
            b'*' if self.in_executable_comment && next == Some(b'/') => {
                self.in_executable_comment = false;
                (Kind::Comment, i + 2)
            }
            b'/' if next == Some(b'*') => (Kind::Comment, self.block_comment_end(i)),
            b';' => (Kind::Semicolon, i + 1),
            b'(' => (Kind::Open, i + 1),
            b')' => (Kind::Close, i + 1),
            c if c.is_ascii_digit() => (Kind::Number, word_end(bytes, i)),
            c if is_word_byte(c) => (Kind::Word, word_end(bytes, i)),
            _ => (Kind::Other, i + 1),
        }
    }

    fn quoted_end(&mut self, start: usize) -> usize {
        skip_quoted(self.sql.as_bytes(), start).unwrap_or_else(|| {
            self.terminated = false;
            self.sql.len()
        })
    }

    fn variable_end(&mut self, start: usize) -> usize {
        let bytes = self.sql.as_bytes();
        let mut i = start + 1;
        if bytes.get(i) == Some(&b'@') {
            i += 1;
        }
        if matches!(bytes.get(i), Some(b'\'' | b'"' | b'`')) {
            return self.quoted_end(i);
        }
        i + bytes[i..]
            .iter()
            .take_while(|&&b| is_word_byte(b) || b == b'.')
            .count()
    }

    /// For `/*!NNNNN` and `/*M!NNNNNN` only the prefix is a comment; the body is lexed as code
    /// until its `*/`.
    fn block_comment_end(&mut self, start: usize) -> usize {
        let bytes = self.sql.as_bytes();
        let body = match &bytes[start + 2..] {
            [b'!', ..] => Some(start + 3),
            [b'M', b'!', ..] => Some(start + 4),
            _ => None,
        };
        if let Some(body) = body {
            self.in_executable_comment = true;
            return body
                + bytes[body..]
                    .iter()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
        }
        match self.sql[start + 2..].find("*/") {
            Some(len) => start + 2 + len + 2,
            None => {
                self.terminated = false;
                bytes.len()
            }
        }
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

fn word_end(bytes: &[u8], start: usize) -> usize {
    start
        + bytes[start..]
            .iter()
            .take_while(|&&b| is_word_byte(b))
            .count()
}

/// The index after the closing quote, or `None` when the quote is never closed.
fn skip_quoted(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = bytes[start];
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
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn line_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |p| start + p + 1)
}

/// Splits a script into statements, dropping comment-only ones and surrounding whitespace.
pub fn split_statements(sql: &str) -> Vec<&str> {
    statement_ranges(sql)
        .into_iter()
        .map(|range| &sql[range])
        .collect()
}

/// The byte ranges `split_statements` returns.
pub fn statement_ranges(sql: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut has_code = false;
    for piece in scan(sql) {
        match piece.kind {
            Kind::Semicolon => {
                if has_code {
                    ranges.push(trimmed(sql, start..piece.start));
                }
                start = piece.start + 1;
                has_code = false;
            }
            Kind::Comment => {}
            _ => has_code = true,
        }
    }
    if has_code {
        ranges.push(trimmed(sql, start..sql.len()));
    }
    ranges
}

fn trimmed(sql: &str, range: Range<usize>) -> Range<usize> {
    let text = &sql[range.clone()];
    let start = range.start + (text.len() - text.trim_start().len());
    start..start + text.trim().len()
}

/// Bare words (keywords and unquoted identifiers), in order.
pub fn words(statement: &str) -> Vec<Piece<'_>> {
    scan(statement).filter(|p| p.kind == Kind::Word).collect()
}

/// Every piece, comments included, in order.
pub fn pieces(statement: &str) -> Vec<Piece<'_>> {
    scan(statement).collect()
}

/// False when the text ends inside a quote or a comment.
pub fn is_terminated(sql: &str) -> bool {
    let mut scanner = scan(sql);
    scanner.by_ref().for_each(drop);
    scanner.terminated
}

/// True when the text holds nothing but whitespace and comments.
pub fn is_blank(sql: &str) -> bool {
    scan(sql).all(|p| p.kind == Kind::Comment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_texts(sql: &str) -> Vec<String> {
        words(sql)
            .iter()
            .map(|w| w.text.to_ascii_uppercase())
            .collect()
    }

    #[test]
    fn open_quotes_and_block_comments_are_unterminated() {
        for sql in ["'abc", "\"abc", "`abc", "/* x", r"'abc\"] {
            assert!(!is_terminated(sql), "{sql}");
        }
    }

    #[test]
    fn line_comments_and_plain_text_are_terminated() {
        for sql in ["a -- x", "a # x", "a 'b'", "/* x */"] {
            assert!(is_terminated(sql), "{sql}");
        }
    }

    #[test]
    fn only_whitespace_and_comments_is_blank() {
        for (sql, blank) in [(" -- x\n /* y */ ", true), ("", true), ("--x", false)] {
            assert_eq!(is_blank(sql), blank, "{sql}");
        }
    }

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
        assert_eq!(word_texts("SELECT 1--1 FROM t"), ["SELECT", "FROM", "T"]);
    }

    fn kinds(sql: &str) -> Vec<(Kind, &str)> {
        scan(sql).map(|p| (p.kind, p.text)).collect()
    }

    #[test]
    fn empty_script_has_no_statements() {
        assert_eq!(split_statements(""), Vec::<&str>::new());
    }

    #[test]
    fn unclosed_executable_comment_is_unterminated() {
        assert!(!is_terminated("/*!40101 SET"));
    }

    #[test]
    fn unclosed_quote_is_one_piece_to_the_end() {
        assert_eq!(
            kinds("a 'bc; d"),
            [(Kind::Word, "a"), (Kind::Quoted, "'bc; d")]
        );
    }

    #[test]
    fn executable_comment_body_is_code() {
        assert_eq!(
            split_statements("/*!40101 SET NAMES utf8 */;"),
            ["/*!40101 SET NAMES utf8 */"]
        );
        assert_eq!(
            word_texts("/*!40101 SET NAMES utf8 */;"),
            ["SET", "NAMES", "UTF8"]
        );
        assert_eq!(word_texts("/*M!100100 SELECT 1 */"), ["SELECT"]);
    }

    #[test]
    fn backslash_inside_backticks_is_literal() {
        assert_eq!(split_statements("SELECT `a\\`; SELECT 2").len(), 2);
    }

    #[test]
    fn trailing_double_dash_is_a_comment() {
        assert!(is_blank("--"));
        assert_eq!(split_statements("SELECT 1 --"), ["SELECT 1 --"]);
    }

    #[test]
    fn crlf_line_endings_split_cleanly() {
        assert_eq!(
            split_statements("SELECT 1 -- x\r\n; SELECT 2\r\n"),
            ["SELECT 1 -- x", "SELECT 2"]
        );
    }

    #[test]
    fn non_ascii_text_is_lexed_by_bytes_without_breaking_splits() {
        assert_eq!(split_statements("SELECT 'é;ü'; SELECT 2").len(), 2);
        assert_eq!(kinds("é'x'"), [(Kind::Word, "é"), (Kind::Quoted, "'x'")]);
    }

    #[test]
    fn identifiers_and_numbers_are_single_pieces() {
        for (sql, kind) in [
            ("$col", Kind::Word),
            ("größe", Kind::Word),
            ("1abc", Kind::Number),
        ] {
            assert_eq!(kinds(sql), [(kind, sql)], "{sql}");
        }
    }

    #[test]
    fn variables_are_single_pieces_and_never_keywords() {
        for sql in ["@where", "@@session.sql_mode", "@'q'", "@`q`"] {
            assert_eq!(kinds(sql), [(Kind::Variable, sql)], "{sql}");
        }
        let piece = scan("@where").next().unwrap();
        assert!(!piece.is_keyword("WHERE"));
    }

    #[test]
    fn parentheses_carry_the_outer_depth() {
        let depths: Vec<_> = scan("a (b) c").map(|p| (p.text, p.depth)).collect();
        assert_eq!(depths, [("a", 0), ("(", 0), ("b", 1), (")", 0), ("c", 0)]);
    }

    #[test]
    fn words_skip_quoted_content() {
        assert_eq!(
            word_texts("delete from `where` where name = 'where'"),
            ["DELETE", "FROM", "WHERE", "NAME"]
        );
    }
}
