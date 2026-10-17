//! The moment a timestamp value stands for, when its zone is stored. `src/lib/values.ts` ports
//! `utc_seconds`; tests keep both equal.

use chrono::{NaiveDate, NaiveDateTime};

use crate::rules::kinds::Zone;

/// Unix seconds of `text` read in `zone`. `None` for a zone that is not stored (`local`, or no
/// rule), for zero dates (`0000-00-00 …`, unix `0`) and for text that is no time.
pub fn utc_seconds(text: &str, zone: Option<Zone>) -> Option<i64> {
    match zone? {
        Zone::Unix => unix(text),
        Zone::Utc => datetime(text),
        Zone::Local => None,
    }
}

/// The leading run of digits, so `<unix>:<id>` values read as their time.
fn unix(text: &str) -> Option<i64> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let seconds: i64 = text[..end].parse().ok()?;
    (seconds != 0).then_some(seconds)
}

/// `YYYY-MM-DD HH:MM:SS` (fractions ignored) or `YYYY-MM-DD` at midnight. Zero dates fail to
/// parse, since month and day 0 do not exist.
fn datetime(text: &str) -> Option<i64> {
    if let Some(head) = text.get(..19)
        && let Ok(at) = NaiveDateTime::parse_from_str(head, "%Y-%m-%d %H:%M:%S")
    {
        return Some(at.and_utc().timestamp());
    }
    let date = NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()?;
    Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_zone_reads_the_leading_number() {
        let cases = [
            ("1700000000", Some(1_700_000_000)),
            ("1700000000:5", Some(1_700_000_000)),
            ("0", None),
            ("0:5", None),
            ("", None),
            ("abc", None),
            ("-5", None),
        ];
        for (text, expected) in cases {
            assert_eq!(utc_seconds(text, Some(Zone::Unix)), expected, "{text:?}");
        }
    }

    #[test]
    fn utc_zone_reads_datetimes_and_dates() {
        let cases = [
            ("2023-11-14 22:13:20", Some(1_700_000_000)),
            ("2023-11-14 22:13:20.123456", Some(1_700_000_000)),
            ("2023-11-14", Some(1_699_920_000)),
            ("1970-01-01 00:00:00", Some(0)),
            ("0000-00-00 00:00:00", None),
            ("0000-00-00", None),
            ("2023-13-45 10:00:00", None),
            ("not a time", None),
            ("", None),
            ("日本語日本語日本語", None),
        ];
        for (text, expected) in cases {
            assert_eq!(utc_seconds(text, Some(Zone::Utc)), expected, "{text:?}");
        }
    }

    #[test]
    fn a_zone_that_is_not_stored_gives_no_moment() {
        for (text, zone) in [
            ("2023-11-14 22:13:20", Some(Zone::Local)),
            ("1700000000", Some(Zone::Local)),
            ("2023-11-14 22:13:20", None),
            ("1700000000", None),
        ] {
            assert_eq!(utc_seconds(text, zone), None, "{text:?} {zone:?}");
        }
    }
}
