//! Lossless parser/serializer for PHP's `serialize()` format. Never executes anything:
//! objects are kept as class name + properties. Strings are bytes because `s:N` counts
//! bytes, and real databases contain broken UTF-8 and wrong lengths.

mod parse;
mod serialize;

pub use parse::{parse, parse_lenient};
pub use serialize::serialize;

use std::borrow::Cow;
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

impl PhpValue {
    /// The entry of an array or object by key text; int keys match their decimal form.
    pub fn get(&self, key: &str) -> Option<&PhpValue> {
        let (PhpValue::Array(entries) | PhpValue::Object { props: entries, .. }) = self else {
            return None;
        };
        entries.iter().find(|(k, _)| k.is(key)).map(|(_, v)| v)
    }

    /// Strings (lossy UTF-8), ints and floats as text.
    pub fn as_text(&self) -> Option<Cow<'_, str>> {
        match self {
            PhpValue::Str(s) => Some(String::from_utf8_lossy(s)),
            PhpValue::Int(i) => Some(Cow::Owned(i.to_string())),
            PhpValue::Float(f) => Some(Cow::Borrowed(f)),
            _ => None,
        }
    }
}

impl PhpKey {
    fn is(&self, key: &str) -> bool {
        match self {
            PhpKey::Int(i) => i.to_string() == key,
            PhpKey::Str(s) => s == key.as_bytes(),
        }
    }
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

/// Mirrors a strict `is_serialized()`, plus `C:` objects: a cheap check before
/// trying to parse.
pub fn looks_serialized(text: &str) -> bool {
    let t = text.trim();
    if t == "N;" {
        return true;
    }
    let b = t.as_bytes();
    if b.len() < 4 || b[1] != b':' || !matches!(b[b.len() - 1], b';' | b'}') {
        return false;
    }
    match b[0] {
        b's' => b[b.len() - 2] == b'"' && counted_prefix(&b[2..]),
        b'a' | b'O' | b'E' | b'C' => counted_prefix(&b[2..]),
        b'b' | b'i' | b'd' => {
            let body = &b[2..b.len() - 1];
            !body.is_empty()
                && body
                    .iter()
                    .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'E' | b'+' | b'-'))
        }
        _ => false,
    }
}

/// `<digits>:` as after `a:`, `O:` or `s:`.
fn counted_prefix(rest: &[u8]) -> bool {
    let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && rest.get(digits) == Some(&b':')
}

/// The float syntax `unserialize()` reads after `d:`. Rust spellings (`inf`, `nan`,
/// `infinity`) would produce a value PHP rejects.
pub fn is_php_float(s: &str) -> bool {
    if matches!(s, "INF" | "-INF" | "NAN") {
        return true;
    }
    fn unsigned(p: &str) -> &str {
        p.strip_prefix(['+', '-']).unwrap_or(p)
    }
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    let (mantissa, exponent) = match unsigned(s).split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(unsigned(e))),
        None => (unsigned(s), None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((int, frac)) => digits(int) && digits(frac) && !(int.is_empty() && frac.is_empty()),
        None => !mantissa.is_empty() && digits(mantissa),
    };
    mantissa_ok && exponent.is_none_or(|e| !e.is_empty() && digits(e))
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
    fn detects_serialized_looking_text() {
        assert!(looks_serialized("a:0:{}"));
        assert!(looks_serialized(" s:3:\"abc\"; "));
        assert!(looks_serialized("N;"));
        assert!(!looks_serialized("yes"));
        assert!(!looks_serialized("{\"a\":1}"));
    }

    #[test]
    fn looks_serialized_follows_strict_is_serialized() {
        for (text, expected) in [
            ("a:0:{}", true),
            ("s:3:\"abc\";", true),
            ("i:5;", true),
            ("b:1;", true),
            ("N;", true),
            ("C:3:\"Foo\":0:{}", true),
            ("a: note;", false),
            ("i:abc;", false),
            ("s:3:\"abc\"", false),
            ("x:1;", false),
            ("i:", false),
        ] {
            assert_eq!(looks_serialized(text), expected, "{text}");
        }
    }

    #[test]
    fn get_finds_array_and_object_entries_by_key_text() {
        let array = PhpValue::Array(vec![
            (key("a"), PhpValue::Int(1)),
            (PhpKey::Int(5), s("five")),
        ]);
        assert_eq!(array.get("a"), Some(&PhpValue::Int(1)));
        assert_eq!(array.get("5"), Some(&s("five")));
        assert_eq!(array.get("b"), None);
        assert_eq!(array.get("6"), None);

        let object = parse(br#"O:3:"Foo":1:{s:1:"x";b:1;}"#).unwrap();
        assert_eq!(object.get("x"), Some(&PhpValue::Bool(true)));

        for scalar in [PhpValue::Null, PhpValue::Int(1), s("a")] {
            assert_eq!(scalar.get("a"), None, "{scalar:?}");
        }
    }

    #[test]
    fn as_text_covers_strings_ints_and_floats_only() {
        assert_eq!(s("hi").as_text().as_deref(), Some("hi"));
        assert_eq!(
            PhpValue::Str(vec![0xff]).as_text().as_deref(),
            Some("\u{fffd}")
        );
        assert_eq!(PhpValue::Int(-5).as_text().as_deref(), Some("-5"));
        assert_eq!(
            PhpValue::Float("1.0E+25".into()).as_text().as_deref(),
            Some("1.0E+25")
        );
        for value in [
            PhpValue::Null,
            PhpValue::Bool(true),
            PhpValue::Array(vec![]),
            PhpValue::Enum(b"A:B".to_vec()),
        ] {
            assert_eq!(value.as_text(), None, "{value:?}");
        }
    }
}
