use serde::Serialize;
use ts_rs::TS;

/// A value as the UI displays it. Numbers stay strings so 64-bit IDs and decimals
/// survive the trip to JavaScript unchanged.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
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
    /// Text longer than the cell limit; the full value comes from `fetch_cell`.
    LongText {
        /// In characters.
        len: u64,
        preview: String,
    },
}

impl Cell {
    /// The stored text; `None` for NULL, binary and long text cells, which only carry a preview.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Cell::Number(s) | Cell::Text(s) | Cell::Temporal(s) | Cell::Json(s) => Some(s),
            Cell::Null | Cell::Bytes { .. } | Cell::LongText { .. } => None,
        }
    }
}

/// The most characters a text cell carries across IPC; longer text becomes a `LongText` preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellLimit(pub usize);

impl CellLimit {
    /// For internal reads (documents, change planning) that need every value in full.
    pub const NONE: CellLimit = CellLimit(usize::MAX);
}

const HEX_PREVIEW_BYTES: usize = 16;

/// Converts a raw text-protocol value into a `Cell` of its column's family; text beyond
/// `limit` characters becomes a `LongText` preview.
pub(crate) fn decode(family: Family, raw: Option<&[u8]>, limit: CellLimit) -> Cell {
    match decode_full(family, raw) {
        Cell::Text(text) => shorten(text, limit),
        cell => cell,
    }
}

fn decode_full(family: Family, raw: Option<&[u8]>) -> Cell {
    let Some(raw) = raw else {
        return Cell::Null;
    };
    match family {
        Family::Number => text(raw).map_or_else(|| bytes(raw), Cell::Number),
        // Kept as text: zero dates like 0000-00-00 00:00:00 are stored values.
        Family::Temporal => text(raw).map_or_else(|| bytes(raw), Cell::Temporal),
        Family::Json => text(raw).map_or_else(|| bytes(raw), Cell::Json),
        Family::Bit => bytes(raw),
        // Binary flag is also set for *_bin collations, so readable UTF-8 is still text.
        Family::Binary => readable_text(raw).map_or_else(|| bytes(raw), Cell::Text),
        Family::Text => text(raw).map_or_else(|| bytes(raw), Cell::Text),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Number,
    Temporal,
    Json,
    Bit,
    Binary,
    Text,
}

/// The family of a column type name as reported by sqlx, e.g. `BIGINT UNSIGNED`, `VARBINARY`.
pub(crate) fn family(type_name: &str) -> Family {
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

fn shorten(text: String, limit: CellLimit) -> Cell {
    // Byte length bounds the character count, so most cells skip the count.
    if text.len() <= limit.0 {
        return Cell::Text(text);
    }
    let len = text.chars().count();
    if len <= limit.0 {
        return Cell::Text(text);
    }
    let cut = text
        .char_indices()
        .nth(limit.0)
        .map_or(text.len(), |(i, _)| i);
    Cell::LongText {
        len: len as u64,
        preview: text[..cut].to_owned(),
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

    fn decode_unlimited(family: Family, raw: Option<&[u8]>) -> Cell {
        decode(family, raw, CellLimit::NONE)
    }

    #[test]
    fn null_is_null_for_every_type() {
        assert_eq!(decode_unlimited(family("VARCHAR"), None), Cell::Null);
        assert_eq!(decode_unlimited(family("BIGINT"), None), Cell::Null);
    }

    #[test]
    fn numbers_keep_full_precision() {
        assert_eq!(
            decode_unlimited(family("BIGINT UNSIGNED"), Some(b"18446744073709551615")),
            Cell::Number("18446744073709551615".into())
        );
        assert_eq!(
            decode_unlimited(family("DECIMAL"), Some(b"49.90")),
            Cell::Number("49.90".into())
        );
    }

    #[test]
    fn zero_dates_are_preserved() {
        assert_eq!(
            decode_unlimited(family("DATETIME"), Some(b"0000-00-00 00:00:00")),
            Cell::Temporal("0000-00-00 00:00:00".into())
        );
    }

    #[test]
    fn binary_columns_with_readable_utf8_are_text() {
        assert_eq!(
            decode_unlimited(family("VARBINARY"), Some(b"hello")),
            Cell::Text("hello".into())
        );
    }

    #[test]
    fn real_binary_data_shows_length_and_hex_preview() {
        let raw: Vec<u8> = (0u8..40).collect();
        assert_eq!(
            decode_unlimited(family("BLOB"), Some(&raw)),
            Cell::Bytes {
                len: 40,
                hex_preview: "000102030405060708090a0b0c0d0e0f".into()
            }
        );
    }

    #[test]
    fn bit_values_are_bytes() {
        assert!(matches!(
            decode_unlimited(family("BIT"), Some(&[1])),
            Cell::Bytes { len: 1, .. }
        ));
    }

    #[test]
    fn json_columns_are_tagged() {
        assert_eq!(
            decode_unlimited(family("JSON"), Some(b"{\"a\":1}")),
            Cell::Json("{\"a\":1}".into())
        );
    }

    #[test]
    fn invalid_utf8_in_a_text_column_is_bytes() {
        assert_eq!(
            decode_unlimited(family("VARCHAR"), Some(&[0xff, 0xfe])),
            Cell::Bytes {
                len: 2,
                hex_preview: "fffe".into()
            }
        );
    }

    #[test]
    fn binary_columns_treat_control_bytes_as_binary_but_keep_whitespace() {
        assert_eq!(
            decode_unlimited(family("VARBINARY"), Some(b"a\x01b")),
            Cell::Bytes {
                len: 3,
                hex_preview: "610162".into()
            }
        );
        assert_eq!(
            decode_unlimited(family("VARBINARY"), Some(b"a\tb\nc\r")),
            Cell::Text("a\tb\nc\r".into())
        );
    }

    #[test]
    fn empty_binary_values_are_empty_text() {
        for type_name in ["BLOB", "VARBINARY"] {
            assert_eq!(
                decode_unlimited(family(type_name), Some(b"")),
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
                decode_unlimited(family(type_name), Some(raw)),
                expected,
                "{type_name} {raw:?}"
            );
        }
    }

    #[test]
    fn geometry_is_bytes_and_unknown_types_are_text() {
        assert_eq!(
            decode_unlimited(family("GEOMETRY"), Some(&[0, 0, 0, 0, 1])),
            Cell::Bytes {
                len: 5,
                hex_preview: "0000000001".into()
            }
        );
        assert_eq!(
            decode_unlimited(family("SOMETHING_NEW"), Some(b"abc")),
            Cell::Text("abc".into())
        );
    }

    #[test]
    fn hex_preview_is_lowercase_and_zero_padded() {
        assert_eq!(
            decode_unlimited(family("VARBINARY"), Some(&[0x00, 0x0f, 0xab, 0xff])),
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
            decode_unlimited(family("BLOB"), Some(&raw)),
            Cell::Bytes {
                len: 17,
                hex_preview: "00".repeat(16)
            }
        );
    }

    #[test]
    fn text_up_to_the_limit_stays_text_and_longer_text_becomes_a_preview() {
        let limit = CellLimit(5);
        assert_eq!(
            decode(Family::Text, Some(b"hello"), limit),
            Cell::Text("hello".into())
        );
        assert_eq!(
            decode(Family::Text, Some(b"hello!"), limit),
            Cell::LongText {
                len: 6,
                preview: "hello".into()
            }
        );
    }

    #[test]
    fn limit_counts_characters_and_the_preview_is_cut_at_a_char_boundary() {
        let limit = CellLimit(3);
        assert_eq!(
            decode(Family::Text, Some("äöü".as_bytes()), limit),
            Cell::Text("äöü".into())
        );
        assert_eq!(
            decode(Family::Text, Some("äöüß€".as_bytes()), limit),
            Cell::LongText {
                len: 5,
                preview: "äöü".into()
            }
        );
    }

    #[test]
    fn zero_limit_leaves_an_empty_preview() {
        assert_eq!(
            decode(Family::Text, Some(b"ab"), CellLimit(0)),
            Cell::LongText {
                len: 2,
                preview: String::new()
            }
        );
        assert_eq!(
            decode(Family::Text, Some(b""), CellLimit(0)),
            Cell::Text(String::new())
        );
    }

    #[test]
    fn readable_binary_text_is_cut_but_unreadable_binary_stays_bytes() {
        assert_eq!(
            decode(Family::Binary, Some(b"abcdef"), CellLimit(2)),
            Cell::LongText {
                len: 6,
                preview: "ab".into()
            }
        );
        assert_eq!(
            decode(Family::Binary, Some(b"a\x01cdef"), CellLimit(2)),
            Cell::Bytes {
                len: 6,
                hex_preview: "610163646566".into()
            }
        );
    }

    #[test]
    fn numbers_temporals_and_json_are_never_cut() {
        let long = "1".repeat(50);
        for (family, cell) in [
            (Family::Number, Cell::Number(long.clone())),
            (Family::Temporal, Cell::Temporal(long.clone())),
            (Family::Json, Cell::Json(long.clone())),
        ] {
            assert_eq!(
                decode(family, Some(long.as_bytes()), CellLimit(5)),
                cell,
                "{family:?}"
            );
        }
    }

    #[test]
    fn no_limit_never_cuts() {
        let long = "x".repeat(100_000);
        assert_eq!(
            decode(Family::Text, Some(long.as_bytes()), CellLimit::NONE),
            Cell::Text(long)
        );
    }

    #[test]
    fn long_text_serializes_with_camel_case_fields() {
        assert_eq!(
            serde_json::to_value(Cell::LongText {
                len: 6,
                preview: "hello".into()
            })
            .unwrap(),
            json!({"t": "longText", "v": {"len": 6, "preview": "hello"}})
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
