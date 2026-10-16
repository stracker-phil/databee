//! Edits one leaf of a stored PHP-serialized or JSON value. Paths are child indexes as the
//! value tree lists them, so equal display keys (a protected and a public `bar`, int `1` and
//! string `"1"`) stay distinct.

use serde::Deserialize;

use super::php::{self, PhpValue, is_php_float};
use crate::error::{AppError, AppResult};

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum NewValue {
    String(String),
    Int(String),
    Float(String),
    Bool(bool),
    Null,
}

fn php_new(v: &NewValue) -> AppResult<PhpValue> {
    Ok(match v {
        NewValue::String(s) => PhpValue::Str(s.as_bytes().to_vec()),
        NewValue::Int(s) => PhpValue::Int(
            s.trim()
                .parse()
                .map_err(|_| AppError::InvalidInput(format!("'{s}' is not an integer")))?,
        ),
        NewValue::Float(s) => {
            let s = s.trim();
            if !is_php_float(s) {
                return Err(AppError::InvalidInput(format!("'{s}' is not a PHP float")));
            }
            PhpValue::Float(s.to_string())
        }
        NewValue::Bool(b) => PhpValue::Bool(*b),
        NewValue::Null => PhpValue::Null,
    })
}

fn no_child(index: usize) -> AppError {
    AppError::InvalidInput(format!("No child #{index} in the value"))
}

/// The value tree shows serialized data stored inside a string as the string's only child.
const DECODED_CHILD: usize = 0;

fn set_php(value: &mut PhpValue, path: &[usize], new: &NewValue) -> AppResult<()> {
    let Some((&index, rest)) = path.split_first() else {
        *value = php_new(new)?;
        return Ok(());
    };
    match value {
        PhpValue::Array(items) | PhpValue::Object { props: items, .. } => {
            let (_, child) = items.get_mut(index).ok_or_else(|| no_child(index))?;
            set_php(child, rest, new)
        }
        PhpValue::Str(s) if index == DECODED_CHILD => {
            let inner = String::from_utf8(s.clone())
                .map_err(|_| AppError::InvalidInput("Nested value is not UTF-8".into()))?;
            *s = edit_serialized(&inner, rest, new)?.into_bytes();
            Ok(())
        }
        _ => Err(no_child(index)),
    }
}

fn contains_ref(value: &PhpValue) -> bool {
    match value {
        PhpValue::Ref { .. } => true,
        PhpValue::Array(items) | PhpValue::Object { props: items, .. } => {
            items.iter().any(|(_, v)| contains_ref(v))
        }
        _ => false,
    }
}

/// Whether an `r:`/`R:` reference sits at or after the value at `path` in serialization
/// order. References count values in that order, so replacing a subtree can shift them.
/// A nested serialized string has its own numbering.
fn ref_at_or_after(value: &PhpValue, path: &[usize]) -> bool {
    let Some((&index, rest)) = path.split_first() else {
        return contains_ref(value);
    };
    let (PhpValue::Array(items) | PhpValue::Object { props: items, .. }) = value else {
        return false;
    };
    let Some((_, child)) = items.get(index) else {
        return false;
    };
    ref_at_or_after(child, rest) || items[index + 1..].iter().any(|(_, v)| contains_ref(v))
}

fn set_json(value: &mut serde_json::Value, path: &[usize], new: &NewValue) -> AppResult<()> {
    let Some((&index, rest)) = path.split_first() else {
        *value = match new {
            NewValue::String(s) => serde_json::Value::String(s.clone()),
            NewValue::Int(s) | NewValue::Float(s) => {
                serde_json::from_str::<serde_json::Number>(s.trim())
                    .map(serde_json::Value::Number)
                    .map_err(|_| AppError::InvalidInput(format!("'{s}' is not a number")))?
            }
            NewValue::Bool(b) => serde_json::Value::Bool(*b),
            NewValue::Null => serde_json::Value::Null,
        };
        return Ok(());
    };
    let child = match value {
        serde_json::Value::Object(map) => map.values_mut().nth(index),
        serde_json::Value::Array(items) => items.get_mut(index),
        serde_json::Value::String(s) if index == DECODED_CHILD => {
            *s = edit_serialized(s, rest, new)?;
            return Ok(());
        }
        _ => None,
    }
    .ok_or_else(|| no_child(index))?;
    set_json(child, rest, new)
}

/// Replaces the value at `path` and re-serializes, keeping whitespace around the value. PHP
/// output has correct byte lengths and leaves untouched parts byte-identical; JSON is
/// re-encoded (same key order, normalized spacing and escapes).
pub fn edit_serialized(raw: &str, path: &[usize], new: &NewValue) -> AppResult<String> {
    let core = raw.trim();
    let lead = &raw[..raw.len() - raw.trim_start().len()];
    let tail = &raw[lead.len() + core.len()..];
    let edited = if php::looks_serialized(core) {
        edit_php(core, path, new)?
    } else {
        edit_json(core, path, new)?
    };
    Ok(format!("{lead}{edited}{tail}"))
}

fn edit_php(raw: &str, path: &[usize], new: &NewValue) -> AppResult<String> {
    let mut v = php::parse(raw.as_bytes())
        .map_err(|e| AppError::InvalidInput(format!("Cannot edit: {e}")))?;
    if ref_at_or_after(&v, path) {
        return Err(AppError::InvalidInput(
            "Cannot edit: PHP references (r:/R:) at or after this value would point elsewhere"
                .into(),
        ));
    }
    set_php(&mut v, path, new)?;
    String::from_utf8(php::serialize(&v))
        .map_err(|_| AppError::InvalidInput("Result is not UTF-8".into()))
}

fn edit_json(raw: &str, path: &[usize], new: &NewValue) -> AppResult<String> {
    let mut v: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| AppError::InvalidInput(format!("Cannot edit: {e}")))?;
    set_json(&mut v, path, new)?;
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_php_values_with_correct_lengths() {
        let raw = r#"a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";}"#;
        let out = edit_serialized(raw, &[1], &NewValue::String("PayPal Checkout".into())).unwrap();
        assert_eq!(
            out,
            r#"a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:15:"PayPal Checkout";}"#
        );
        let out = edit_serialized(raw, &[0], &NewValue::String("nö".into())).unwrap();
        assert!(out.contains(r#"s:3:"nö""#), "lengths count bytes: {out}");
    }

    #[test]
    fn edits_nested_and_typed_values() {
        let raw = r#"a:1:{s:4:"fees";a:2:{s:5:"gross";d:95.4;s:4:"live";b:0;}}"#;
        let out = edit_serialized(raw, &[0, 1], &NewValue::Bool(true)).unwrap();
        assert_eq!(
            out,
            r#"a:1:{s:4:"fees";a:2:{s:5:"gross";d:95.4;s:4:"live";b:1;}}"#
        );
        let out = edit_serialized(raw, &[0, 0], &NewValue::Float("100.5".into())).unwrap();
        assert!(out.contains("d:100.5;"));
        let err = edit_serialized(raw, &[0, 0], &NewValue::Int("x".into())).unwrap_err();
        assert_eq!(err.kind(), "invalid_input");
    }

    #[test]
    fn out_of_range_index_is_invalid_input() {
        let php = r#"a:1:{s:1:"k";i:1;}"#;
        let cases: [(&str, &[usize]); 5] = [
            (php, &[1]),
            (php, &[0, 0]),
            ("[1,2]", &[2]),
            (r#"{"a":1}"#, &[1]),
            (r#"{"a":1}"#, &[0, 0]),
        ];
        for (raw, path) in cases {
            let err = edit_serialized(raw, path, &NewValue::Null).expect_err(raw);
            assert_eq!(err.kind(), "invalid_input", "{raw} {path:?}");
        }
    }

    #[test]
    fn equal_display_keys_are_told_apart_by_index() {
        let raw = "O:3:\"Foo\":2:{s:6:\"\0*\0bar\";i:1;s:3:\"bar\";i:2;}";
        let out = edit_serialized(raw, &[1], &NewValue::Int("9".into())).unwrap();
        assert_eq!(out, "O:3:\"Foo\":2:{s:6:\"\0*\0bar\";i:1;s:3:\"bar\";i:9;}");
    }

    #[test]
    fn int_key_and_string_key_with_the_same_text_are_distinct() {
        let raw = r#"a:2:{i:1;s:1:"a";s:1:"1";s:1:"b";}"#;
        let out = edit_serialized(raw, &[1], &NewValue::String("z".into())).unwrap();
        assert_eq!(out, r#"a:2:{i:1;s:1:"a";s:1:"1";s:1:"z";}"#);
        let out = edit_serialized(raw, &[0], &NewValue::String("y".into())).unwrap();
        assert_eq!(out, r#"a:2:{i:1;s:1:"y";s:1:"1";s:1:"b";}"#);
    }

    #[test]
    fn edits_serialized_data_inside_strings() {
        let inner = r#"a:1:{s:1:"k";i:1;}"#;
        let raw = format!(r#"a:1:{{s:4:"data";s:{}:"{}";}}"#, inner.len(), inner);
        let out = edit_serialized(&raw, &[0, 0, 0], &NewValue::Int("42".into())).unwrap();
        let new_inner = r#"a:1:{s:1:"k";i:42;}"#;
        assert_eq!(
            out,
            format!(
                r#"a:1:{{s:4:"data";s:{}:"{}";}}"#,
                new_inner.len(),
                new_inner
            )
        );
    }

    #[test]
    fn edits_protected_object_properties() {
        let raw = "O:3:\"Foo\":1:{s:6:\"\0*\0bar\";i:1;}";
        let out = edit_serialized(raw, &[0], &NewValue::Int("2".into())).unwrap();
        assert_eq!(out, "O:3:\"Foo\":1:{s:6:\"\0*\0bar\";i:2;}");
    }

    #[test]
    fn float_edits_accept_only_the_syntax_php_reads() {
        let raw = r#"a:1:{s:1:"f";d:1;}"#;
        for text in ["1.5", "-1.0E+25", "INF", "-INF", "NAN"] {
            let out = edit_serialized(raw, &[0], &NewValue::Float(text.into()))
                .unwrap_or_else(|e| panic!("{text}: {e:?}"));
            assert_eq!(out, format!(r#"a:1:{{s:1:"f";d:{text};}}"#), "{text}");
        }
        for text in ["inf", "nan", "infinity"] {
            let err = edit_serialized(raw, &[0], &NewValue::Float(text.into())).expect_err(text);
            assert_eq!(err.kind(), "invalid_input", "{text}");
        }
    }

    #[test]
    fn whitespace_around_a_serialized_value_is_kept() {
        let out =
            edit_serialized("  a:1:{s:1:\"k\";i:1;}\n", &[0], &NewValue::Int("2".into())).unwrap();
        assert_eq!(out, "  a:1:{s:1:\"k\";i:2;}\n");
    }

    #[test]
    fn edits_are_refused_when_a_reference_sits_at_or_after_the_value() {
        let ref_after = r#"a:2:{s:1:"a";i:1;s:1:"b";r:2;}"#;
        for index in [0, 1] {
            let err =
                edit_serialized(ref_after, &[index], &NewValue::Int("5".into())).expect_err("ref");
            assert_eq!(err.kind(), "invalid_input", "{index}");
        }
        let ref_before = r#"a:2:{s:1:"a";r:1;s:1:"b";i:1;}"#;
        let out = edit_serialized(ref_before, &[1], &NewValue::Int("5".into())).unwrap();
        assert_eq!(out, r#"a:2:{s:1:"a";r:1;s:1:"b";i:5;}"#);
    }

    #[test]
    fn edits_json_keeping_key_order() {
        let out = edit_serialized(
            r#"{"z":1,"a":{"b":"x"}}"#,
            &[1, 0],
            &NewValue::String("y".into()),
        )
        .unwrap();
        assert_eq!(out, r#"{"z":1,"a":{"b":"y"}}"#);
        let out = edit_serialized(
            r#"{ "z": 1, "a": 2, "m": 3 }"#,
            &[2],
            &NewValue::Int("7".into()),
        )
        .unwrap();
        assert_eq!(out, r#"{"z":1,"a":2,"m":7}"#);
        let out = edit_serialized("[1,2]", &[1], &NewValue::Int("5".into())).unwrap();
        assert_eq!(out, "[1,5]");
    }
}
