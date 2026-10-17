//! Turns stored text (PHP-serialized or JSON) into a generic tree the UI renders as-is.

use serde::Serialize;
use ts_rs::TS;

use super::php::{self, PhpKey, PhpValue};

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Null,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Ref,
    Enum,
    Custom,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Null => "null",
            NodeKind::Bool => "bool",
            NodeKind::Int => "int",
            NodeKind::Float => "float",
            NodeKind::String => "string",
            NodeKind::Array => "array",
            NodeKind::Object => "object",
            NodeKind::Ref => "ref",
            NodeKind::Enum => "enum",
            NodeKind::Custom => "custom",
        }
    }
}

/// How a child is keyed: array keys, or the visibility of an object property.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum KeyKind {
    Int,
    String,
    Public,
    Protected,
    Private,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Php,
    Json,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ValueNode {
    pub kind: NodeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(optional = nullable)]
    pub children: Vec<ChildNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

/// Children are addressed by their position (edits send index paths), so equal display keys
/// stay distinct.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[ts(rename = "ValueChild")]
#[serde(rename_all = "camelCase")]
pub struct ChildNode {
    /// The display key: a property name without its visibility mangling.
    pub key: String,
    pub key_kind: KeyKind,
    /// The class that declares a private property.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub declaring_class: Option<String>,
    /// The stored key bytes as hex when they are not valid UTF-8.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub raw_hex: Option<String>,
    pub node: ValueNode,
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecodedValue {
    pub format: Format,
    pub root: Option<ValueNode>,
    pub warning: Option<String>,
    pub error: Option<String>,
}

impl DecodedValue {
    fn new(format: Format, root: Option<ValueNode>) -> Self {
        Self {
            format,
            root,
            warning: None,
            error: None,
        }
    }
}

const MAX_NESTED_DECODE: usize = 3;

fn leaf(kind: NodeKind, text: impl Into<String>) -> ValueNode {
    ValueNode {
        kind,
        text: Some(text.into()),
        class: None,
        children: Vec::new(),
        note: None,
    }
}

fn container(kind: NodeKind, children: Vec<ChildNode>) -> ValueNode {
    ValueNode {
        kind,
        text: None,
        class: None,
        children,
        note: None,
    }
}

fn child(key: impl Into<String>, key_kind: KeyKind, node: ValueNode) -> ChildNode {
    ChildNode {
        key: key.into(),
        key_kind,
        declaring_class: None,
        raw_hex: None,
        node,
    }
}

/// Decodes text if it is PHP-serialized or a JSON object/array; `None` for plain values.
pub fn decode(text: &str) -> Option<DecodedValue> {
    decode_at_depth(text, 0)
}

/// The format `text` looks stored in: PHP-serialized by `php::looks_serialized`, JSON when it
/// starts like an object or array. Whether it parses is up to the caller.
pub fn format_of(text: &str) -> Option<Format> {
    let trimmed = text.trim();
    if php::looks_serialized(trimmed) {
        return Some(Format::Php);
    }
    let json = trimmed.starts_with('{') || trimmed.starts_with('[');
    json.then_some(Format::Json)
}

fn decode_at_depth(text: &str, depth: usize) -> Option<DecodedValue> {
    let trimmed = text.trim();
    if format_of(trimmed)? == Format::Php {
        return Some(match php::parse(trimmed.as_bytes()) {
            Ok(v) => DecodedValue::new(Format::Php, Some(php_node(&v, depth))),
            Err(strict) => match php::parse_lenient(trimmed.as_bytes()) {
                Ok(v) => DecodedValue {
                    warning: Some(format!(
                        "Stored string lengths are wrong ({strict}); decoded leniently. \
                         PHP's unserialize() would fail on this value."
                    )),
                    ..DecodedValue::new(Format::Php, Some(php_node(&v, depth)))
                },
                Err(_) => DecodedValue {
                    error: Some(strict.to_string()),
                    ..DecodedValue::new(Format::Php, None)
                },
            },
        });
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .map(|v| DecodedValue::new(Format::Json, Some(json_node(&v, depth))))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn key_node(key: &PhpKey, in_object: bool, node: ValueNode) -> ChildNode {
    let PhpKey::Str(raw) = key else {
        return child(key_text(key), KeyKind::Int, node);
    };
    let text = String::from_utf8_lossy(raw).into_owned();
    let raw_hex = std::str::from_utf8(raw).is_err().then(|| hex(raw));
    if !in_object {
        return ChildNode {
            raw_hex,
            ..child(text, KeyKind::String, node)
        };
    }
    // Mangled property names: "\0*\0name" (protected), "\0Class\0name" (private).
    let (key, key_kind, declaring_class) =
        match text.strip_prefix('\0').and_then(|r| r.split_once('\0')) {
            Some(("*", name)) => (name.to_string(), KeyKind::Protected, None),
            Some((class, name)) => (name.to_string(), KeyKind::Private, Some(class.to_string())),
            None => (text, KeyKind::Public, None),
        };
    ChildNode {
        declaring_class,
        raw_hex,
        ..child(key, key_kind, node)
    }
}

fn key_text(key: &PhpKey) -> String {
    match key {
        PhpKey::Int(i) => i.to_string(),
        PhpKey::Str(s) => String::from_utf8_lossy(s).into_owned(),
    }
}

fn entries(entries: &[(PhpKey, PhpValue)], in_object: bool, depth: usize) -> Vec<ChildNode> {
    entries
        .iter()
        .map(|(k, v)| key_node(k, in_object, php_node(v, depth)))
        .collect()
}

fn string_node(raw: &[u8], depth: usize) -> ValueNode {
    match std::str::from_utf8(raw) {
        Ok(s) => {
            let mut node = leaf(NodeKind::String, s);
            // Serialized data stored inside a serialized string is common in WooCommerce.
            if depth < MAX_NESTED_DECODE
                && let Some(DecodedValue {
                    root: Some(inner), ..
                }) = decode_at_depth(s, depth + 1)
            {
                node.note = Some("contains encoded data".into());
                node.children = vec![child("decoded", KeyKind::String, inner)];
            }
            node
        }
        Err(_) => {
            let mut node = leaf(NodeKind::String, String::from_utf8_lossy(raw));
            node.note = Some(format!("invalid UTF-8 ({} bytes)", raw.len()));
            node
        }
    }
}

fn php_node(value: &PhpValue, depth: usize) -> ValueNode {
    match value {
        PhpValue::Null => leaf(NodeKind::Null, "null"),
        PhpValue::Bool(b) => leaf(NodeKind::Bool, b.to_string()),
        PhpValue::Int(i) => leaf(NodeKind::Int, i.to_string()),
        PhpValue::Float(f) => leaf(NodeKind::Float, f.clone()),
        PhpValue::Str(s) => string_node(s, depth),
        PhpValue::Array(items) => container(NodeKind::Array, entries(items, false, depth)),
        PhpValue::Object { class, props } => ValueNode {
            class: Some(String::from_utf8_lossy(class).into_owned()),
            ..container(NodeKind::Object, entries(props, true, depth))
        },
        PhpValue::Custom { class, data } => ValueNode {
            class: Some(String::from_utf8_lossy(class).into_owned()),
            ..leaf(NodeKind::Custom, String::from_utf8_lossy(data))
        },
        PhpValue::Ref { strong, index } => leaf(
            NodeKind::Ref,
            format!("{} #{index}", if *strong { "&ref" } else { "ref" }),
        ),
        PhpValue::Enum(name) => leaf(NodeKind::Enum, String::from_utf8_lossy(name)),
    }
}

fn json_node(value: &serde_json::Value, depth: usize) -> ValueNode {
    use serde_json::Value;
    match value {
        Value::Null => leaf(NodeKind::Null, "null"),
        Value::Bool(b) => leaf(NodeKind::Bool, b.to_string()),
        Value::Number(n) if n.is_f64() => leaf(NodeKind::Float, n.to_string()),
        Value::Number(n) => leaf(NodeKind::Int, n.to_string()),
        Value::String(s) => string_node(s.as_bytes(), depth),
        Value::Array(items) => container(
            NodeKind::Array,
            items
                .iter()
                .enumerate()
                .map(|(i, v)| child(i.to_string(), KeyKind::Int, json_node(v, depth)))
                .collect(),
        ),
        Value::Object(map) => container(
            NodeKind::Object,
            map.iter()
                .map(|(k, v)| child(k.clone(), KeyKind::String, json_node(v, depth)))
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
        assert_eq!(node.kind, NodeKind::Array);
        assert_eq!(keys(&node), ["enabled", "title"]);
        assert_eq!(node.children[1].node.text.as_deref(), Some("PayPal"));
    }

    #[test]
    fn object_property_visibility_is_shown() {
        let node = root("O:3:\"Foo\":2:{s:6:\"\0*\0bar\";i:1;s:8:\"\0Foo\0baz\";i:2;}");
        assert_eq!(node.class.as_deref(), Some("Foo"));
        assert_eq!(node.children[0].key, "bar");
        assert_eq!(node.children[0].key_kind, KeyKind::Protected);
        assert_eq!(node.children[1].key_kind, KeyKind::Private);
    }

    #[test]
    fn protected_and_public_properties_with_one_name_are_distinguishable() {
        let node = root("O:3:\"Foo\":2:{s:6:\"\0*\0bar\";i:1;s:3:\"bar\";i:2;}");
        assert_eq!(keys(&node), ["bar", "bar"]);
        assert_eq!(node.children[0].key_kind, KeyKind::Protected);
        assert_eq!(node.children[1].key_kind, KeyKind::Public);
        assert_eq!(node.children[0].declaring_class, None);
    }

    #[test]
    fn private_properties_carry_their_declaring_class() {
        let node = root("O:3:\"Foo\":1:{s:8:\"\0Foo\0bar\";i:1;}");
        assert_eq!(node.children[0].key, "bar");
        assert_eq!(node.children[0].key_kind, KeyKind::Private);
        assert_eq!(node.children[0].declaring_class.as_deref(), Some("Foo"));
    }

    #[test]
    fn nested_decoding_stops_after_three_levels() {
        let wrap = |inner: &str| format!(r#"a:1:{{s:1:"k";s:{}:"{}";}}"#, inner.len(), inner);
        let l4 = r#"a:1:{s:1:"k";i:1;}"#;
        let outer = wrap(&wrap(&wrap(&wrap(l4))));
        let mut node = root(&outer);
        for level in 1..=3 {
            let string = &node.children[0].node;
            assert_eq!(string.children[0].key, "decoded", "level {level}");
            node = string.children[0].node.clone();
        }
        let deepest = &node.children[0].node;
        assert_eq!(deepest.kind, NodeKind::String);
        assert_eq!(deepest.text.as_deref(), Some(l4));
        assert!(deepest.children.is_empty());
        assert_eq!(deepest.note, None);
    }

    #[test]
    fn invalid_utf8_string_values_carry_a_note() {
        let node = php_node(&PhpValue::Str(vec![0xff, 0xfe]), 0);
        assert_eq!(node.kind, NodeKind::String);
        assert_eq!(node.text.as_deref(), Some("\u{fffd}\u{fffd}"));
        assert_eq!(node.note.as_deref(), Some("invalid UTF-8 (2 bytes)"));
    }

    #[test]
    fn invalid_utf8_keys_carry_their_bytes_as_hex() {
        let node = php_node(
            &PhpValue::Array(vec![(PhpKey::Str(vec![0xff, 0x41]), PhpValue::Null)]),
            0,
        );
        assert_eq!(node.children[0].key, "\u{fffd}A");
        assert_eq!(node.children[0].raw_hex.as_deref(), Some("ff41"));
        let valid = root(r#"a:1:{s:1:"k";N;}"#);
        assert_eq!(valid.children[0].raw_hex, None);
    }

    #[test]
    fn json_numbers_keep_their_text_and_kind() {
        let node = root("[1.0, 1, 2.50]");
        let numbers: Vec<_> = node
            .children
            .iter()
            .map(|c| (c.node.kind, c.node.text.as_deref()))
            .collect();
        assert_eq!(
            numbers,
            [
                (NodeKind::Float, Some("1.0")),
                (NodeKind::Int, Some("1")),
                (NodeKind::Float, Some("2.5")),
            ]
        );
    }

    #[test]
    fn custom_ref_and_enum_values_become_opaque_leaves() {
        let custom = root(r#"C:11:"ArrayObject":3:{abc}"#);
        assert_eq!(
            custom,
            ValueNode {
                class: Some("ArrayObject".into()),
                ..leaf(NodeKind::Custom, "abc")
            }
        );
        let refs = root(r#"a:2:{i:0;r:2;i:1;R:2;}"#);
        assert_eq!(refs.children[0].node, leaf(NodeKind::Ref, "ref #2"));
        assert_eq!(refs.children[1].node, leaf(NodeKind::Ref, "&ref #2"));
        assert_eq!(
            root(r#"E:11:"Suit:Hearts";"#),
            leaf(NodeKind::Enum, "Suit:Hearts")
        );
    }

    #[test]
    fn nodes_serialize_with_lowercase_kinds_and_camel_case_fields() {
        let decoded = decode("O:3:\"Foo\":1:{s:6:\"\0*\0bar\";i:1;}").unwrap();
        assert_eq!(
            serde_json::to_value(&decoded).unwrap(),
            serde_json::json!({
                "format": "php",
                "root": {
                    "kind": "object",
                    "class": "Foo",
                    "children": [{
                        "key": "bar",
                        "keyKind": "protected",
                        "node": {"kind": "int", "text": "1"}
                    }]
                },
                "warning": null,
                "error": null
            })
        );
        assert_eq!(
            serde_json::to_value(decode("[null]").unwrap()).unwrap(),
            serde_json::json!({
                "format": "json",
                "root": {
                    "kind": "array",
                    "children": [{
                        "key": "0",
                        "keyKind": "int",
                        "node": {"kind": "null", "text": "null"}
                    }]
                },
                "warning": null,
                "error": null
            })
        );
    }

    #[test]
    fn node_kinds_have_lowercase_names() {
        for (kind, name) in [
            (NodeKind::Null, "null"),
            (NodeKind::Bool, "bool"),
            (NodeKind::Int, "int"),
            (NodeKind::Float, "float"),
            (NodeKind::String, "string"),
            (NodeKind::Array, "array"),
            (NodeKind::Object, "object"),
            (NodeKind::Ref, "ref"),
            (NodeKind::Enum, "enum"),
            (NodeKind::Custom, "custom"),
        ] {
            assert_eq!(kind.as_str(), name);
            assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!(name));
        }
    }

    #[test]
    fn json_keeps_key_order() {
        let node = root(r#"{"z":1,"a":[true,null]}"#);
        assert_eq!(decode(r#"{"z":1}"#).unwrap().format, Format::Json);
        assert_eq!(keys(&node), ["z", "a"]);
        assert_eq!(node.children[1].node.children[0].node.kind, NodeKind::Bool);
    }

    #[test]
    fn nested_serialized_strings_are_decoded_too() {
        let inner = r#"a:1:{s:1:"k";i:1;}"#;
        let outer = format!(r#"a:1:{{s:4:"data";s:{}:"{}";}}"#, inner.len(), inner);
        let node = root(&outer);
        let data = &node.children[0].node;
        assert_eq!(data.note.as_deref(), Some("contains encoded data"));
        assert_eq!(data.children[0].node.kind, NodeKind::Array);
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
