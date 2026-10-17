//! Which text cells hold PHP-serialized data or JSON, with a short preview.

use std::collections::HashMap;

use super::SerializedCell;
use crate::db::{Cell, ResultSet};
use crate::decode::php;
use crate::decode::value::{Format, format_of};

const SCALAR_PREVIEW_CHARS: usize = 40;

/// The format and a short preview when the decoder reads `text`: `decode::value::format_of`,
/// then a parse whose result is discarded (no tree is built).
pub fn serialized(text: &str) -> Option<(Format, String)> {
    let trimmed = text.trim();
    match format_of(trimmed)? {
        Format::Php => {
            let bytes = trimmed.as_bytes();
            let parses = php::parse(bytes).is_ok() || php::parse_lenient(bytes).is_ok();
            parses.then(|| (Format::Php, php_preview(trimmed)))
        }
        Format::Json => {
            serde_json::from_str::<serde::de::IgnoredAny>(trimmed).ok()?;
            let preview = if trimmed.starts_with('{') {
                "{…}"
            } else {
                "[…]"
            };
            Some((Format::Json, preview.to_string()))
        }
    }
}

/// Arrays and objects (`a:`, `O:`, `C:`) keep their header up to the opening brace
/// (`a:3:{…}`, `O:8:"WC_Order":2:{…}`), which already says how many entries follow; scalars are
/// short enough to show as stored.
fn php_preview(text: &str) -> String {
    let container = ["a:", "O:", "C:"].iter().any(|p| text.starts_with(p));
    if container && let Some(open) = text.find('{') {
        return format!("{}…}}", &text[..=open]);
    }
    shorten(text, SCALAR_PREVIEW_CHARS)
}

fn shorten(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// Fills `result.serialized`. Runs before the session moves the full texts into its cell cache,
/// because a long value's preview is cut and no longer parses.
pub fn find_serialized(result: &mut ResultSet) {
    let full: HashMap<(usize, usize), &str> = result
        .full_texts
        .iter()
        .map(|f| ((f.row, f.col), f.text.as_str()))
        .collect();
    let mut found = Vec::new();
    for (row, cells) in result.rows.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            let text = match cell {
                Cell::Text(text) | Cell::Json(text) => Some(text.as_str()),
                Cell::LongText { .. } => full.get(&(row, col)).copied(),
                _ => None,
            };
            let Some((format, preview)) = text.and_then(serialized) else {
                continue;
            };
            found.push(SerializedCell {
                row,
                col,
                format,
                preview,
            });
        }
    }
    result.serialized = found;
}
