use super::{ParseError, PhpKey, PhpValue, is_php_float};

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

    /// Only the form `serialize()` writes (`5`, `-5`, never `+5`, `05` or `-0`), so a
    /// parsed value always serializes back to the same bytes.
    fn int_until(&mut self, end: u8) -> Result<i64, ParseError> {
        let start = self.pos;
        let raw = self.until(end)?;
        std::str::from_utf8(raw)
            .ok()
            .and_then(|s| s.parse::<i64>().ok().filter(|i| i.to_string() == s))
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

    /// `N:"bytes"` followed by `terminator`; the length prefix has already been read up to
    /// the colon. The terminator is left for the caller.
    fn quoted(&mut self, len: usize, terminator: u8) -> Result<Vec<u8>, ParseError> {
        self.expect(b'"')?;
        let (input, start) = (self.input, self.pos);
        let closes_at = |i: usize| {
            input.get(start + i) == Some(&b'"') && input.get(start + i + 1) == Some(&terminator)
        };
        if closes_at(len) {
            self.pos = start + len + 1;
            return Ok(input[start..start + len].to_vec());
        }
        if self.mode == Mode::Strict {
            return Err(ParseError {
                offset: start,
                message: format!("string length {len} does not match its content"),
            });
        }
        // Lenient: the closing quote nearest to the declared length. The whole parse must
        // still consume the input, so a wrong pick fails instead of yielding a wrong tree.
        let end = (0..input.len() - start)
            .filter(|&i| closes_at(i))
            .min_by_key(|&i| i.abs_diff(len))
            .ok_or_else(|| self.error("unterminated string"))?;
        self.pos = start + end + 1;
        Ok(input[start..start + end].to_vec())
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
                let start = self.pos;
                let raw = self.until(b';')?;
                let text = std::str::from_utf8(raw)
                    .ok()
                    .filter(|t| is_php_float(t))
                    .ok_or(ParseError {
                        offset: start,
                        message: "invalid float".into(),
                    })?;
                PhpValue::Float(text.to_string())
            }
            b's' => {
                self.expect(b':')?;
                let len = self.count_until(b':')?;
                let s = self.quoted(len, b';')?;
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
                let class = self.quoted(len, b':')?;
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
                let class = self.quoted(len, b':')?;
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
                let start = self.pos;
                let index = self.int_until(b';')?;
                if index < 1 {
                    return Err(ParseError {
                        offset: start,
                        message: "references count from 1".into(),
                    });
                }
                PhpValue::Ref {
                    strong: tag == b'R',
                    index,
                }
            }
            b'E' => {
                self.expect(b':')?;
                let len = self.count_until(b':')?;
                let name = self.quoted(len, b';')?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::php::serialize;

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
    fn non_canonical_ints_and_bools_are_rejected_in_both_modes() {
        for raw in [
            "i:+1;",
            "i:05;",
            "i:-0;",
            "i:;",
            "b:2;",
            "b:+1;",
            "b:01;",
            "a:+1:{i:0;N;}",
        ] {
            assert!(parse(raw.as_bytes()).is_err(), "strict {raw}");
            assert!(parse_lenient(raw.as_bytes()).is_err(), "lenient {raw}");
        }
    }

    #[test]
    fn non_php_floats_are_rejected() {
        for raw in [
            "d:abc;",
            "d:inf;",
            "d:nan;",
            "d:infinity;",
            "d:;",
            "d:.;",
            "d:1e;",
            "d:1.2.3;",
        ] {
            assert!(parse(raw.as_bytes()).is_err(), "strict {raw}");
            assert!(parse_lenient(raw.as_bytes()).is_err(), "lenient {raw}");
        }
    }

    #[test]
    fn custom_objects_with_a_wrong_payload_length_fail() {
        for raw in [
            r#"C:3:"Foo":5:{ab}"#,
            r#"C:3:"Foo":1:{ab}"#,
            r#"C:3:"Foo":2:{ab"#,
            r#"C:4:"Foo":0:{}"#,
        ] {
            assert!(parse(raw.as_bytes()).is_err(), "{raw}");
        }
        assert_eq!(
            parse(br#"C:3:"Foo":2:{ab}"#).unwrap(),
            PhpValue::Custom {
                class: b"Foo".to_vec(),
                data: b"ab".to_vec()
            }
        );
    }

    #[test]
    fn references_must_count_from_one() {
        for raw in ["r:-1;", "r:0;", "R:0;", "R:-1;", "r:+1;"] {
            assert!(parse(raw.as_bytes()).is_err(), "strict {raw}");
            assert!(parse_lenient(raw.as_bytes()).is_err(), "lenient {raw}");
        }
        assert_eq!(
            parse(b"r:1;").unwrap(),
            PhpValue::Ref {
                strong: false,
                index: 1
            }
        );
    }

    #[test]
    fn enums_with_an_invalid_length_fail() {
        for raw in [
            r#"E:5:"Suit:Hearts";"#,
            r#"E:3:"abcd";"#,
            r#"E:-1:"x";"#,
            r#"E:x:"x";"#,
        ] {
            assert!(parse(raw.as_bytes()).is_err(), "{raw}");
        }
    }

    #[test]
    fn duplicate_array_keys_are_kept_in_order() {
        let raw = br#"a:4:{s:1:"a";i:1;s:1:"a";i:2;i:7;i:3;i:7;i:4;}"#;
        let parsed = parse(raw).unwrap();
        assert_eq!(
            parsed,
            PhpValue::Array(vec![
                (key("a"), PhpValue::Int(1)),
                (key("a"), PhpValue::Int(2)),
                (PhpKey::Int(7), PhpValue::Int(3)),
                (PhpKey::Int(7), PhpValue::Int(4)),
            ])
        );
        assert_eq!(serialize(&parsed), raw);
    }

    #[test]
    fn lenient_mode_never_turns_a_misplaced_closing_quote_into_a_wrong_tree() {
        let expected = PhpValue::Array(vec![(key("k"), s("x\";y"))]);
        assert_eq!(
            parse(br#"a:1:{s:1:"k";s:4:"x";y";}"#).unwrap(),
            expected,
            "correct length"
        );
        assert_eq!(
            parse_lenient(br#"a:1:{s:1:"k";s:9:"x";y";}"#).unwrap(),
            expected,
            "declared too long"
        );
        let too_short = br#"a:1:{s:1:"k";s:2:"x";y";}"#;
        assert!(parse(too_short).is_err());
        if let Ok(v) = parse_lenient(too_short) {
            assert_eq!(v, expected, "declared too short");
        }
    }
}
