//! Lossless parser/serializer for PHP's `serialize()` format. Never executes anything:
//! objects are kept as class name + properties. Strings are bytes because `s:N` counts
//! bytes, and real databases contain broken UTF-8 and wrong lengths.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PhpKey {
    Int(i64),
    Str(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhpValue {
    Null,
    Bool(bool),
    Int(i64),
    /// Raw text (`0.1`, `INF`, `1.0E+25`) so re-serializing never drifts.
    Float(String),
    Str(Vec<u8>),
    Array(Vec<(PhpKey, PhpValue)>),
    Object {
        class: Vec<u8>,
        props: Vec<(PhpKey, PhpValue)>,
    },
    /// `C:` objects implementing Serializable: opaque payload.
    Custom {
        class: Vec<u8>,
        data: Vec<u8>,
    },
    /// `r:` (value reference) or `R:` (variable reference) to the Nth value.
    Ref {
        strong: bool,
        index: i64,
    },
    /// PHP 8.1 enum case, e.g. `Suit:Hearts`.
    Enum(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

const MAX_DEPTH: usize = 128;
/// Caps preallocation from declared counts, which can't be trusted.
const MAX_PREALLOC: usize = 1024;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Strict,
    /// Ignores `s:N` lengths and finds the closing `";` instead. Repairs data that was
    /// re-encoded (e.g. latin1 → utf8) after serialization.
    Lenient,
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    mode: Mode,
    depth: usize,
}

/// Parses a complete serialized value; trailing bytes are an error.
pub fn parse(input: &[u8]) -> Result<PhpValue, ParseError> {
    parse_with(input, Mode::Strict)
}

/// Like `parse`, but tolerates wrong string lengths.
pub fn parse_lenient(input: &[u8]) -> Result<PhpValue, ParseError> {
    parse_with(input, Mode::Lenient)
}

fn parse_with(input: &[u8], mode: Mode) -> Result<PhpValue, ParseError> {
    let mut p = Parser {
        input,
        pos: 0,
        mode,
        depth: 0,
    };
    let value = p.value()?;
    if p.pos != input.len() {
        return Err(p.error("unexpected data after the value"));
    }
    Ok(value)
}

/// Mirrors WordPress `is_serialized()`: a cheap check before trying to parse.
pub fn looks_serialized(text: &str) -> bool {
    let t = text.trim();
    if t == "N;" {
        return true;
    }
    let b = t.as_bytes();
    b.len() >= 4
        && b[1] == b':'
        && matches!(b[0], b'a' | b'O' | b's' | b'b' | b'i' | b'd' | b'C' | b'E')
        && matches!(b[b.len() - 1], b';' | b'}')
}

impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(format!("expected '{}'", byte as char)))
        }
    }

    /// Reads up to (excluding) `end` and consumes `end`.
    fn until(&mut self, end: u8) -> Result<&[u8], ParseError> {
        let start = self.pos;
        let len = self.input[start..]
            .iter()
            .position(|&b| b == end)
            .ok_or_else(|| self.error(format!("missing '{}'", end as char)))?;
        self.pos = start + len + 1;
        Ok(&self.input[start..start + len])
    }

    fn int_until(&mut self, end: u8) -> Result<i64, ParseError> {
        let start = self.pos;
        let raw = self.until(end)?;
        std::str::from_utf8(raw)
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or(ParseError {
                offset: start,
                message: "invalid integer".into(),
            })
    }

    fn count_until(&mut self, end: u8) -> Result<usize, ParseError> {
        let start = self.pos;
        let n = self.int_until(end)?;
        usize::try_from(n).map_err(|_| ParseError {
            offset: start,
            message: "negative length".into(),
        })
    }

    /// `N:"bytes"` — the length prefix has already been read up to the colon.
    fn quoted(&mut self, len: usize) -> Result<Vec<u8>, ParseError> {
        self.expect(b'"')?;
        let start = self.pos;
        let fits = start + len < self.input.len() && self.input[start + len] == b'"';
        if fits {
            self.pos = start + len + 1;
            return Ok(self.input[start..start + len].to_vec());
        }
        if self.mode == Mode::Strict {
            return Err(ParseError {
                offset: start,
                message: format!("string length {len} does not match its content"),
            });
        }
        // Lenient: the string ends at the first `"` followed by a valid terminator.
        let rest = &self.input[start..];
        let end = (0..rest.len())
            .find(|&i| rest[i] == b'"' && matches!(rest.get(i + 1), Some(b';' | b':') | None))
            .ok_or_else(|| self.error("unterminated string"))?;
        self.pos = start + end + 1;
        Ok(rest[..end].to_vec())
    }

    fn value(&mut self) -> Result<PhpValue, ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        let tag = self.peek().ok_or_else(|| self.error("unexpected end"))?;
        self.pos += 1;
        let value = match tag {
            b'N' => {
                self.expect(b';')?;
                PhpValue::Null
            }
            b'b' => {
                self.expect(b':')?;
                let v = self.int_until(b';')?;
                match v {
                    0 => PhpValue::Bool(false),
                    1 => PhpValue::Bool(true),
                    _ => return Err(self.error("boolean must be 0 or 1")),
                }
            }
            b'i' => {
                self.expect(b':')?;
                PhpValue::Int(self.int_until(b';')?)
            }
            b'd' => {
                self.expect(b':')?;
                let raw = self.until(b';')?;
                let text =
                    String::from_utf8(raw.to_vec()).map_err(|_| self.error("invalid float"))?;
                PhpValue::Float(text)
            }
            b's' => {
                self.expect(b':')?;
                let len = self.count_until(b':')?;
                let s = self.quoted(len)?;
                self.expect(b';')?;
                PhpValue::Str(s)
            }
            b'a' => {
                self.expect(b':')?;
                let n = self.count_until(b':')?;
                PhpValue::Array(self.entries(n)?)
            }
            b'O' => {
                self.expect(b':')?;
                let len = self.count_until(b':')?;
                let class = self.quoted(len)?;
                self.expect(b':')?;
                let n = self.count_until(b':')?;
                PhpValue::Object {
                    class,
                    props: self.entries(n)?,
                }
            }
            b'C' => {
                self.expect(b':')?;
                let len = self.count_until(b':')?;
                let class = self.quoted(len)?;
                self.expect(b':')?;
                let n = self.count_until(b':')?;
                self.expect(b'{')?;
                let start = self.pos;
                if start + n >= self.input.len() || self.input[start + n] != b'}' {
                    return Err(self.error("custom object length does not match"));
                }
                self.pos = start + n + 1;
                PhpValue::Custom {
                    class,
                    data: self.input[start..start + n].to_vec(),
                }
            }
            b'r' | b'R' => {
                self.expect(b':')?;
                PhpValue::Ref {
                    strong: tag == b'R',
                    index: self.int_until(b';')?,
                }
            }
            b'E' => {
                self.expect(b':')?;
                let len = self.count_until(b':')?;
                let name = self.quoted(len)?;
                self.expect(b';')?;
                PhpValue::Enum(name)
            }
            other => {
                self.pos -= 1;
                return Err(self.error(format!("unknown type '{}'", other as char)));
            }
        };
        self.depth -= 1;
        Ok(value)
    }

    fn entries(&mut self, n: usize) -> Result<Vec<(PhpKey, PhpValue)>, ParseError> {
        self.expect(b'{')?;
        let mut out = Vec::with_capacity(n.min(MAX_PREALLOC));
        for _ in 0..n {
            let key = match self.value()? {
                PhpValue::Int(i) => PhpKey::Int(i),
                PhpValue::Str(s) => PhpKey::Str(s),
                _ => return Err(self.error("array keys must be int or string")),
            };
            out.push((key, self.value()?));
        }
        self.expect(b'}')?;
        Ok(out)
    }
}

/// Serializes back to PHP's format. `serialize(&parse(x)?) == x` for valid input.
pub fn serialize(value: &PhpValue) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(&mut out, value);
    out
}

fn write_str(out: &mut Vec<u8>, tag: &str, s: &[u8]) {
    out.extend_from_slice(format!("{tag}:{}:\"", s.len()).as_bytes());
    out.extend_from_slice(s);
    out.push(b'"');
}

fn write_entries(out: &mut Vec<u8>, entries: &[(PhpKey, PhpValue)]) {
    out.extend_from_slice(format!("{}:{{", entries.len()).as_bytes());
    for (k, v) in entries {
        match k {
            PhpKey::Int(i) => out.extend_from_slice(format!("i:{i};").as_bytes()),
            PhpKey::Str(s) => {
                write_str(out, "s", s);
                out.push(b';');
            }
        }
        write_value(out, v);
    }
    out.push(b'}');
}

fn write_value(out: &mut Vec<u8>, value: &PhpValue) {
    match value {
        PhpValue::Null => out.extend_from_slice(b"N;"),
        PhpValue::Bool(b) => out.extend_from_slice(if *b { b"b:1;" } else { b"b:0;" }),
        PhpValue::Int(i) => out.extend_from_slice(format!("i:{i};").as_bytes()),
        PhpValue::Float(f) => out.extend_from_slice(format!("d:{f};").as_bytes()),
        PhpValue::Str(s) => {
            write_str(out, "s", s);
            out.push(b';');
        }
        PhpValue::Array(entries) => {
            out.extend_from_slice(b"a:");
            write_entries(out, entries);
        }
        PhpValue::Object { class, props } => {
            write_str(out, "O", class);
            out.push(b':');
            write_entries(out, props);
        }
        PhpValue::Custom { class, data } => {
            write_str(out, "C", class);
            out.extend_from_slice(format!(":{}:{{", data.len()).as_bytes());
            out.extend_from_slice(data);
            out.push(b'}');
        }
        PhpValue::Ref { strong, index } => out
            .extend_from_slice(format!("{}:{index};", if *strong { 'R' } else { 'r' }).as_bytes()),
        PhpValue::Enum(name) => {
            write_str(out, "E", name);
            out.push(b';');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> PhpValue {
        PhpValue::Str(v.as_bytes().to_vec())
    }

    fn key(v: &str) -> PhpKey {
        PhpKey::Str(v.as_bytes().to_vec())
    }

    #[test]
    fn parses_scalars() {
        assert_eq!(parse(b"N;").unwrap(), PhpValue::Null);
        assert_eq!(parse(b"b:1;").unwrap(), PhpValue::Bool(true));
        assert_eq!(parse(b"i:-42;").unwrap(), PhpValue::Int(-42));
        assert_eq!(parse(b"d:0.1;").unwrap(), PhpValue::Float("0.1".into()));
        assert_eq!(parse(b"s:5:\"hello\";").unwrap(), s("hello"));
    }

    #[test]
    fn string_lengths_count_bytes_not_characters() {
        assert_eq!(parse("s:2:\"ü\";".as_bytes()).unwrap(), s("ü"));
    }

    #[test]
    fn parses_nested_arrays_with_mixed_keys_in_order() {
        let v = parse(br#"a:2:{s:7:"enabled";s:3:"yes";i:0;a:1:{i:5;b:0;}}"#).unwrap();
        assert_eq!(
            v,
            PhpValue::Array(vec![
                (key("enabled"), s("yes")),
                (
                    PhpKey::Int(0),
                    PhpValue::Array(vec![(PhpKey::Int(5), PhpValue::Bool(false))])
                ),
            ])
        );
    }

    #[test]
    fn keeps_objects_as_data() {
        let v = parse(br#"O:8:"stdClass":1:{s:1:"a";i:1;}"#).unwrap();
        assert_eq!(
            v,
            PhpValue::Object {
                class: b"stdClass".to_vec(),
                props: vec![(key("a"), PhpValue::Int(1))]
            }
        );
    }

    #[test]
    fn round_trips_exactly() {
        let samples: &[&[u8]] = &[
            br#"a:4:{s:4:"ppcp";s:0:"";s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";s:5:"price";d:49.9;}"#,
            br#"O:11:"WC_DateTime":3:{s:4:"date";s:26:"2026-01-01 00:00:00.000000";s:13:"timezone_type";i:3;s:8:"timezone";s:3:"UTC";}"#,
            br#"a:2:{i:0;r:1;i:1;R:2;}"#,
            br#"C:11:"ArrayObject":21:{x:i:0;a:0:{};m:a:0:{}}"#,
            br#"E:11:"Suit:Hearts";"#,
            "a:1:{s:4:\"name\";s:7:\"Müller\";}".as_bytes(),
            b"d:INF;",
            "O:3:\"Foo\":1:{s:6:\"\0*\0bar\";N;}".as_bytes(),
        ];
        for raw in samples {
            let parsed =
                parse(raw).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(raw)));
            assert_eq!(serialize(&parsed), *raw, "{}", String::from_utf8_lossy(raw));
        }
    }

    #[test]
    fn invalid_utf8_inside_strings_is_preserved() {
        let raw = b"s:2:\"\xff\xfe\";";
        assert_eq!(serialize(&parse(raw).unwrap()), raw);
    }

    #[test]
    fn wrong_string_length_is_reported_with_position() {
        let err = parse(r#"a:1:{s:3:"key";s:5:"Müller";}"#.as_bytes()).unwrap_err();
        assert!(err.message.contains("does not match"), "{err}");
        assert!(err.offset > 0);
    }

    #[test]
    fn lenient_mode_repairs_wrong_string_lengths() {
        let v = parse_lenient(r#"a:1:{s:4:"name";s:5:"Müller";}"#.as_bytes()).unwrap();
        assert_eq!(v, PhpValue::Array(vec![(key("name"), s("Müller"))]));
    }

    #[test]
    fn truncated_or_garbage_input_errors_instead_of_panicking() {
        for raw in [
            &b"a:2:{i:0;"[..],
            b"s:999999999:\"x\";",
            b"a:-1:{}",
            b"x:1;",
            b"i:abc;",
            b"",
            b"N;trailing",
        ] {
            assert!(parse(raw).is_err(), "{}", String::from_utf8_lossy(raw));
        }
    }

    #[test]
    fn huge_declared_counts_do_not_allocate_upfront() {
        assert!(parse(b"a:4000000000:{}").is_err());
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let raw = "a:1:{i:0;".repeat(200) + "N;" + &"}".repeat(200);
        assert!(parse(raw.as_bytes()).is_err());
    }

    #[test]
    fn detects_serialized_looking_text() {
        assert!(looks_serialized("a:0:{}"));
        assert!(looks_serialized(" s:3:\"abc\"; "));
        assert!(looks_serialized("N;"));
        assert!(!looks_serialized("yes"));
        assert!(!looks_serialized("{\"a\":1}"));
    }
}
