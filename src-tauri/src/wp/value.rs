//! Turns stored text (PHP-serialized or JSON) into a generic tree the UI renders as-is.

use serde::Serialize;

use super::php::{self, PhpKey, PhpValue};

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueNode {
    /// null, bool, int, float, string, array, object, ref, enum, custom
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ChildNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChildNode {
    pub key: String,
    /// int, string, public, protected, private
    pub key_kind: &'static str,
    pub node: ValueNode,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DecodedValue {
    /// php or json
    pub format: &'static str,
    pub root: Option<ValueNode>,
    pub warning: Option<String>,
    pub error: Option<String>,
}

const MAX_NESTED_DECODE: usize = 3;

fn leaf(kind: &'static str, text: impl Into<String>) -> ValueNode {
    ValueNode {
        kind,
        text: Some(text.into()),
        class: None,
        children: Vec::new(),
        note: None,
    }
}

fn container(kind: &'static str, children: Vec<ChildNode>) -> ValueNode {
    ValueNode {
        kind,
        text: None,
        class: None,
        children,
        note: None,
    }
}

/// Decodes text if it is PHP-serialized or a JSON object/array; `None` for plain values.
pub fn decode(text: &str) -> Option<DecodedValue> {
    decode_at_depth(text, 0)
}

fn decode_at_depth(text: &str, depth: usize) -> Option<DecodedValue> {
    let trimmed = text.trim();
    if php::looks_serialized(trimmed) {
        return Some(match php::parse(trimmed.as_bytes()) {
            Ok(v) => DecodedValue {
                format: "php",
                root: Some(php_node(&v, depth)),
                warning: None,
                error: None,
            },
            Err(strict) => match php::parse_lenient(trimmed.as_bytes()) {
                Ok(v) => DecodedValue {
                    format: "php",
                    root: Some(php_node(&v, depth)),
                    warning: Some(format!(
                        "Stored string lengths are wrong ({strict}); decoded leniently. \
                         PHP's unserialize() would fail on this value."
                    )),
                    error: None,
                },
                Err(_) => DecodedValue {
                    format: "php",
                    root: None,
                    warning: None,
                    error: Some(strict.to_string()),
                },
            },
        });
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .map(|v| DecodedValue {
                format: "json",
                root: Some(json_node(&v, depth)),
                warning: None,
                error: None,
            });
    }
    None
}

fn key_of(key: &PhpKey, in_object: bool) -> (String, &'static str) {
    match key {
        PhpKey::Int(i) => (i.to_string(), "int"),
        PhpKey::Str(raw) => {
            let s = String::from_utf8_lossy(raw).into_owned();
            if !in_object {
                return (s, "string");
            }
            // Mangled property names: "\0*\0name" (protected), "\0Class\0name" (private).
            match s.strip_prefix('\0').and_then(|r| r.split_once('\0')) {
                Some(("*", name)) => (name.to_string(), "protected"),
                Some((_, name)) => (name.to_string(), "private"),
                None => (s, "public"),
            }
        }
    }
}

fn entries(entries: &[(PhpKey, PhpValue)], in_object: bool, depth: usize) -> Vec<ChildNode> {
    entries
        .iter()
        .map(|(k, v)| {
            let (key, key_kind) = key_of(k, in_object);
            ChildNode {
                key,
                key_kind,
                node: php_node(v, depth),
            }
        })
        .collect()
}

fn string_node(raw: &[u8], depth: usize) -> ValueNode {
    match std::str::from_utf8(raw) {
        Ok(s) => {
            let mut node = leaf("string", s);
            // Serialized data stored inside a serialized string is common in WooCommerce.
            if depth < MAX_NESTED_DECODE
                && let Some(DecodedValue {
                    root: Some(inner), ..
                }) = decode_at_depth(s, depth + 1)
            {
                node.note = Some("contains encoded data".into());
                node.children = vec![ChildNode {
                    key: "decoded".into(),
                    key_kind: "string",
                    node: inner,
                }];
            }
            node
        }
        Err(_) => {
            let mut node = leaf("string", String::from_utf8_lossy(raw));
            node.note = Some(format!("invalid UTF-8 ({} bytes)", raw.len()));
            node
        }
    }
}

fn php_node(value: &PhpValue, depth: usize) -> ValueNode {
    match value {
        PhpValue::Null => leaf("null", "null"),
        PhpValue::Bool(b) => leaf("bool", b.to_string()),
        PhpValue::Int(i) => leaf("int", i.to_string()),
        PhpValue::Float(f) => leaf("float", f.clone()),
        PhpValue::Str(s) => string_node(s, depth),
        PhpValue::Array(items) => container("array", entries(items, false, depth)),
        PhpValue::Object { class, props } => ValueNode {
            class: Some(String::from_utf8_lossy(class).into_owned()),
            ..container("object", entries(props, true, depth))
        },
        PhpValue::Custom { class, data } => ValueNode {
            class: Some(String::from_utf8_lossy(class).into_owned()),
            ..leaf("custom", String::from_utf8_lossy(data))
        },
        PhpValue::Ref { strong, index } => leaf(
            "ref",
            format!("{} #{index}", if *strong { "&ref" } else { "ref" }),
        ),
        PhpValue::Enum(name) => leaf("enum", String::from_utf8_lossy(name)),
    }
}

fn json_node(value: &serde_json::Value, depth: usize) -> ValueNode {
    use serde_json::Value;
    match value {
        Value::Null => leaf("null", "null"),
        Value::Bool(b) => leaf("bool", b.to_string()),
        Value::Number(n) if n.is_f64() => leaf("float", n.to_string()),
        Value::Number(n) => leaf("int", n.to_string()),
        Value::String(s) => string_node(s.as_bytes(), depth),
        Value::Array(items) => container(
            "array",
            items
                .iter()
                .enumerate()
                .map(|(i, v)| ChildNode {
                    key: i.to_string(),
                    key_kind: "int",
                    node: json_node(v, depth),
                })
                .collect(),
        ),
        Value::Object(map) => container(
            "object",
            map.iter()
                .map(|(k, v)| ChildNode {
                    key: k.clone(),
                    key_kind: "string",
                    node: json_node(v, depth),
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(text: &str) -> ValueNode {
        decode(text).unwrap().root.unwrap()
    }

    fn keys(node: &ValueNode) -> Vec<&str> {
        node.children.iter().map(|c| c.key.as_str()).collect()
    }

    #[test]
    fn plain_text_is_not_decoded() {
        assert_eq!(decode("yes"), None);
        assert_eq!(decode("4.2.0"), None);
    }

    #[test]
    fn php_arrays_become_ordered_trees() {
        let node = root(r#"a:2:{s:7:"enabled";s:3:"yes";s:5:"title";s:6:"PayPal";}"#);
        assert_eq!(node.kind, "array");
        assert_eq!(keys(&node), ["enabled", "title"]);
        assert_eq!(node.children[1].node.text.as_deref(), Some("PayPal"));
    }

    #[test]
    fn object_property_visibility_is_shown() {
        let node = root("O:3:\"Foo\":2:{s:6:\"\0*\0bar\";i:1;s:8:\"\0Foo\0baz\";i:2;}");
        assert_eq!(node.class.as_deref(), Some("Foo"));
        assert_eq!(node.children[0].key, "bar");
        assert_eq!(node.children[0].key_kind, "protected");
        assert_eq!(node.children[1].key_kind, "private");
    }

    #[test]
    fn json_keeps_key_order() {
        let node = root(r#"{"z":1,"a":[true,null]}"#);
        assert_eq!(decode(r#"{"z":1}"#).unwrap().format, "json");
        assert_eq!(keys(&node), ["z", "a"]);
        assert_eq!(node.children[1].node.children[0].node.kind, "bool");
    }

    #[test]
    fn nested_serialized_strings_are_decoded_too() {
        let inner = r#"a:1:{s:1:"k";i:1;}"#;
        let outer = format!(r#"a:1:{{s:4:"data";s:{}:"{}";}}"#, inner.len(), inner);
        let node = root(&outer);
        let data = &node.children[0].node;
        assert_eq!(data.note.as_deref(), Some("contains encoded data"));
        assert_eq!(data.children[0].node.kind, "array");
    }

    #[test]
    fn wrong_lengths_decode_leniently_with_a_warning() {
        let decoded = decode(r#"a:1:{s:4:"name";s:5:"Müller";}"#).unwrap();
        assert!(decoded.warning.unwrap().contains("lengths"));
        assert_eq!(
            decoded.root.unwrap().children[0].node.text.as_deref(),
            Some("Müller")
        );
    }

    #[test]
    fn broken_values_report_an_error_instead_of_a_tree() {
        let decoded = decode("a:2:{i:0;s:1:\"x\";").unwrap();
        assert_eq!(decoded.root, None);
        assert!(decoded.error.is_some());
    }
}
