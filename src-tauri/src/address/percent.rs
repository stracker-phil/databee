//! Percent-encoding of address parts. Everything but `A-Z a-z 0-9 - . _ ~` is escaped as UTF-8
//! bytes in uppercase hex, so the separators `/ ? & =` inside a value never split it.
//! `src/lib/address.ts` encodes the same set; the fixture test keeps both in step.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

const ESCAPED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub fn encode(text: &str) -> String {
    utf8_percent_encode(text, ESCAPED).to_string()
}

/// Strict: a `%` must start a two-digit hex escape and the bytes must be UTF-8. Characters
/// that should have been escaped but were typed raw (a space, `é`) are accepted as they are.
/// The error is the reason, for `Address::parse` to report.
pub fn decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    for (i, _) in text.match_indices('%') {
        let valid = bytes
            .get(i + 1..i + 3)
            .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit));
        if !valid {
            return Err(format!("broken percent escape in `{text}`"));
        }
    }
    percent_decode_str(text)
        .decode_utf8()
        .map(|decoded| decoded.into_owned())
        .map_err(|_| format!("`{text}` does not decode to UTF-8 text"))
}
