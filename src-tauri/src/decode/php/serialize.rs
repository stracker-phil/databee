use super::{PhpKey, PhpValue};

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
    use crate::decode::php::parse;

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
    fn canonical_ints_at_the_i64_edges_round_trip() {
        for raw in ["i:0;", "i:9223372036854775807;", "i:-9223372036854775808;"] {
            assert_eq!(
                serialize(&parse(raw.as_bytes()).unwrap()),
                raw.as_bytes(),
                "{raw}"
            );
        }
    }

    #[test]
    fn php_float_syntax_parses_and_round_trips() {
        for text in ["-INF", "NAN", "INF", "1.0E+25", "-1.5e-3", ".5", "5.", "0"] {
            let raw = format!("d:{text};");
            let parsed = parse(raw.as_bytes()).unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert_eq!(parsed, PhpValue::Float(text.into()), "{raw}");
            assert_eq!(serialize(&parsed), raw.as_bytes(), "{raw}");
        }
    }

    #[test]
    fn private_properties_round_trip() {
        let raw = "O:3:\"Foo\":1:{s:8:\"\0Foo\0bar\";i:1;}";
        let parsed = parse(raw.as_bytes()).unwrap();
        assert_eq!(
            parsed,
            PhpValue::Object {
                class: b"Foo".to_vec(),
                props: vec![(PhpKey::Str(b"\0Foo\0bar".to_vec()), PhpValue::Int(1))]
            }
        );
        assert_eq!(serialize(&parsed), raw.as_bytes());
    }
}
