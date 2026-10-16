use serde::Serialize;

/// A value as the UI displays it. Numbers stay strings so 64-bit IDs and decimals
/// survive the trip to JavaScript unchanged.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "t", content = "v", rename_all = "camelCase")]
pub enum Cell {
    Null,
    Number(String),
    Text(String),
    Temporal(String),
    Json(String),
    #[serde(rename_all = "camelCase")]
    Bytes {
        len: usize,
        hex_preview: String,
    },
}

const HEX_PREVIEW_BYTES: usize = 16;

/// Converts a raw text-protocol value into a `Cell`, guided by the column type name
/// (as reported by sqlx, e.g. `BIGINT UNSIGNED`, `VARBINARY`, `DATETIME`).
pub fn decode(type_name: &str, raw: Option<&[u8]>) -> Cell {
    let Some(raw) = raw else {
        return Cell::Null;
    };
    let family = family(type_name);
    match family {
        Family::Number => text(raw).map_or_else(|| bytes(raw), Cell::Number),
        // Kept as text: WordPress uses zero dates like 0000-00-00 00:00:00.
        Family::Temporal => text(raw).map_or_else(|| bytes(raw), Cell::Temporal),
        Family::Json => text(raw).map_or_else(|| bytes(raw), Cell::Json),
        Family::Bit => bytes(raw),
        // Binary flag is also set for *_bin collations, so readable UTF-8 is still text.
        Family::Binary => readable_text(raw).map_or_else(|| bytes(raw), Cell::Text),
        Family::Text => text(raw).map_or_else(|| bytes(raw), Cell::Text),
    }
}

enum Family {
    Number,
    Temporal,
    Json,
    Bit,
    Binary,
    Text,
}

fn family(type_name: &str) -> Family {
    let base = type_name.split_whitespace().next().unwrap_or("");
    match base {
        "BOOLEAN" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" | "FLOAT"
        | "DOUBLE" | "DECIMAL" | "YEAR" => Family::Number,
        "DATE" | "TIME" | "DATETIME" | "TIMESTAMP" => Family::Temporal,
        "JSON" => Family::Json,
        "BIT" => Family::Bit,
        "BINARY" | "VARBINARY" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB" | "GEOMETRY" => {
            Family::Binary
        }
        _ => Family::Text,
    }
}

fn text(raw: &[u8]) -> Option<String> {
    std::str::from_utf8(raw).ok().map(str::to_owned)
}

fn readable_text(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    s.chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .then(|| s.to_owned())
}

fn bytes(raw: &[u8]) -> Cell {
    use std::fmt::Write;
    let preview = &raw[..raw.len().min(HEX_PREVIEW_BYTES)];
    // One buffer for the whole preview; this runs for every binary cell of a result set.
    let mut hex_preview = String::with_capacity(2 * preview.len());
    for b in preview {
        let _ = write!(hex_preview, "{b:02x}");
    }
    Cell::Bytes {
        len: raw.len(),
        hex_preview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_is_null_for_every_type() {
        assert_eq!(decode("VARCHAR", None), Cell::Null);
        assert_eq!(decode("BIGINT", None), Cell::Null);
    }

    #[test]
    fn numbers_keep_full_precision() {
        assert_eq!(
            decode("BIGINT UNSIGNED", Some(b"18446744073709551615")),
            Cell::Number("18446744073709551615".into())
        );
        assert_eq!(
            decode("DECIMAL", Some(b"49.90")),
            Cell::Number("49.90".into())
        );
    }

    #[test]
    fn zero_dates_are_preserved() {
        assert_eq!(
            decode("DATETIME", Some(b"0000-00-00 00:00:00")),
            Cell::Temporal("0000-00-00 00:00:00".into())
        );
    }

    #[test]
    fn binary_columns_with_readable_utf8_are_text() {
        assert_eq!(
            decode("VARBINARY", Some(b"hello")),
            Cell::Text("hello".into())
        );
    }

    #[test]
    fn real_binary_data_shows_length_and_hex_preview() {
        let raw: Vec<u8> = (0u8..40).collect();
        assert_eq!(
            decode("BLOB", Some(&raw)),
            Cell::Bytes {
                len: 40,
                hex_preview: "000102030405060708090a0b0c0d0e0f".into()
            }
        );
    }

    #[test]
    fn bit_values_are_bytes() {
        assert!(matches!(
            decode("BIT", Some(&[1])),
            Cell::Bytes { len: 1, .. }
        ));
    }

    #[test]
    fn json_columns_are_tagged() {
        assert_eq!(
            decode("JSON", Some(b"{\"a\":1}")),
            Cell::Json("{\"a\":1}".into())
        );
    }

    #[test]
    fn invalid_utf8_in_a_text_column_is_bytes() {
        assert_eq!(
            decode("VARCHAR", Some(&[0xff, 0xfe])),
            Cell::Bytes {
                len: 2,
                hex_preview: "fffe".into()
            }
        );
    }

    #[test]
    fn binary_columns_treat_control_bytes_as_binary_but_keep_whitespace() {
        assert_eq!(
            decode("VARBINARY", Some(b"a\x01b")),
            Cell::Bytes {
                len: 3,
                hex_preview: "610162".into()
            }
        );
        assert_eq!(
            decode("VARBINARY", Some(b"a\tb\nc\r")),
            Cell::Text("a\tb\nc\r".into())
        );
    }

    #[test]
    fn empty_binary_values_are_empty_text() {
        for type_name in ["BLOB", "VARBINARY"] {
            assert_eq!(
                decode(type_name, Some(b"")),
                Cell::Text(String::new()),
                "{type_name}"
            );
        }
    }

    #[test]
    fn time_year_and_boolean_decode_by_family() {
        let cases = [
            (
                "TIME",
                &b"838:59:59"[..],
                Cell::Temporal("838:59:59".into()),
            ),
            ("TIME", b"-12:00:00", Cell::Temporal("-12:00:00".into())),
            ("YEAR", b"2024", Cell::Number("2024".into())),
            ("BOOLEAN", b"1", Cell::Number("1".into())),
        ];
        for (type_name, raw, expected) in cases {
            assert_eq!(
                decode(type_name, Some(raw)),
                expected,
                "{type_name} {raw:?}"
            );
        }
    }

    #[test]
    fn geometry_is_bytes_and_unknown_types_are_text() {
        assert_eq!(
            decode("GEOMETRY", Some(&[0, 0, 0, 0, 1])),
            Cell::Bytes {
                len: 5,
                hex_preview: "0000000001".into()
            }
        );
        assert_eq!(
            decode("SOMETHING_NEW", Some(b"abc")),
            Cell::Text("abc".into())
        );
    }

    #[test]
    fn hex_preview_is_lowercase_and_zero_padded() {
        assert_eq!(
            decode("VARBINARY", Some(&[0x00, 0x0f, 0xab, 0xff])),
            Cell::Bytes {
                len: 4,
                hex_preview: "000fabff".into()
            }
        );
    }

    #[test]
    fn hex_preview_is_capped_at_sixteen_bytes_but_len_is_full() {
        let raw = [0u8; 17];
        assert_eq!(
            decode("BLOB", Some(&raw)),
            Cell::Bytes {
                len: 17,
                hex_preview: "00".repeat(16)
            }
        );
    }

    #[test]
    fn serializes_with_type_tag() {
        assert_eq!(
            serde_json::to_value(Cell::Number("1".into())).unwrap(),
            json!({"t": "number", "v": "1"})
        );
        assert_eq!(
            serde_json::to_value(Cell::Null).unwrap(),
            json!({"t": "null"})
        );
    }
}
