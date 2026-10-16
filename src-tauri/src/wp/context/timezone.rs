//! The site's time zone and the conversion between UTC instants and site wall-clock time.

use chrono::{DateTime, Duration, FixedOffset, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Serialize, Serializer};

/// The site's time zone: a named zone with DST from `timezone_string`, or the fixed
/// `gmt_offset` WordPress uses when no zone is chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiteZone {
    Named(Tz),
    Fixed(FixedOffset),
}

/// What WordPress assumes with neither option set: `gmt_offset` 0.
impl Default for SiteZone {
    fn default() -> Self {
        SiteZone::Fixed(Utc.fix())
    }
}

impl SiteZone {
    pub fn offset_at(&self, utc: DateTime<Utc>) -> FixedOffset {
        match self {
            SiteZone::Named(tz) => tz.offset_from_utc_datetime(&utc.naive_utc()).fix(),
            SiteZone::Fixed(offset) => *offset,
        }
    }

    pub fn to_local(&self, utc: DateTime<Utc>) -> NaiveDateTime {
        utc.naive_utc() + self.offset_at(utc)
    }

    /// The instant of a local wall-clock time. An ambiguous time (DST ends) takes the
    /// earlier instant; a skipped one (DST starts) uses the offset from before the gap.
    pub fn to_utc(&self, local: NaiveDateTime) -> DateTime<Utc> {
        let tz = match self {
            SiteZone::Fixed(offset) => return (local - *offset).and_utc(),
            SiteZone::Named(tz) => tz,
        };
        if let Some(instant) = tz.from_local_datetime(&local).earliest() {
            return instant.to_utc();
        }
        let before_gap = self.offset_at(local.and_utc() - Duration::days(1));
        (local - before_gap).and_utc()
    }
}

/// The stored value: the zone name, or the offset as `+05:30`.
impl Serialize for SiteZone {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            SiteZone::Named(tz) => serializer.serialize_str(tz.name()),
            SiteZone::Fixed(offset) => serializer.collect_str(offset),
        }
    }
}

/// `timezone_string` when it names a zone, else `gmt_offset` (hours, may be fractional),
/// with the option it came from.
pub(super) fn site_zone(
    timezone_string: Option<&str>,
    gmt_offset: Option<&str>,
) -> (SiteZone, &'static str) {
    if let Some(tz) = timezone_string.and_then(|name| name.trim().parse::<Tz>().ok()) {
        return (SiteZone::Named(tz), "timezone_string");
    }
    let zone = gmt_offset
        .and_then(|hours| hours.trim().parse::<f64>().ok())
        .and_then(|hours| FixedOffset::east_opt((hours * 3600.0).round() as i32))
        .map_or_else(SiteZone::default, SiteZone::Fixed);
    (zone, "gmt_offset")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn local(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn named_zone_converts_local_times_to_utc_and_back() {
        let berlin = SiteZone::Named(chrono_tz::Europe::Berlin);
        for (local_time, instant) in [
            ("2026-07-01 12:00:00", "2026-07-01T10:00:00Z"),
            ("2026-01-15 12:00:00", "2026-01-15T11:00:00Z"),
            ("2026-03-29 02:30:00", "2026-03-29T01:30:00Z"),
            ("2026-10-25 02:30:00", "2026-10-25T00:30:00Z"),
        ] {
            assert_eq!(
                berlin.to_utc(local(local_time)),
                utc(instant),
                "{local_time}"
            );
        }
        for (instant, local_time) in [
            ("2026-07-01T10:00:00Z", "2026-07-01 12:00:00"),
            ("2026-01-15T11:00:00Z", "2026-01-15 12:00:00"),
            ("2026-10-25T01:30:00Z", "2026-10-25 02:30:00"),
        ] {
            assert_eq!(
                berlin.to_local(utc(instant)),
                local(local_time),
                "{instant}"
            );
        }
    }
}
