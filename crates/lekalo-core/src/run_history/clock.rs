//! Strict UTC timestamp handling for the local run history (issue #121).
//!
//! The accepted spelling is exactly RFC 3339 UTC with an optional
//! one-to-nine digit fraction: `YYYY-MM-DDTHH:MM:SS[.f{1,9}]Z`. Records
//! render with a fixed three-digit millisecond fraction, so the byte
//! order of `recordedAt` strings is identical to their chronological
//! order — the stable retention and list order `(recordedAt, runId)`
//! never needs a secondary parse.

use crate::expressions::types::{civil_from_days, days_from_civil};

/// The seconds of one civil day.
const SECONDS_PER_DAY: i64 = 86_400;

/// One parsed instant: seconds since the Unix epoch plus the sub-second
/// nanoseconds carried by the fraction (0 when absent).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Instant {
    pub(crate) seconds: i64,
    pub(crate) nanos: u32,
}

/// The day-of-year length check for one month.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Parse the strict run-history timestamp spelling.
pub(crate) fn parse(text: &str) -> Option<Instant> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || bytes.len() > 30 {
        return None;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    if bytes.len() == 20 {
        if bytes[19] != b'Z' {
            return None;
        }
    } else if bytes[19] != b'.' || bytes[bytes.len() - 1] != b'Z' {
        return None;
    }
    let digit = |range: std::ops::Range<usize>| -> Option<i64> {
        let mut value: i64 = 0;
        for byte in bytes[range].iter() {
            if !byte.is_ascii_digit() {
                return None;
            }
            value = value * 10 + i64::from(byte - b'0');
        }
        Some(value)
    };
    let year = digit(0..4)?;
    let month = digit(5..7)?;
    let day = digit(8..10)?;
    let hour = digit(11..13)?;
    let minute = digit(14..16)?;
    let second = digit(17..19)?;
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    if day < 1 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut nanos: u32 = 0;
    if bytes.len() > 20 {
        let fraction = &bytes[20..bytes.len() - 1];
        if fraction.is_empty() || fraction.len() > 9 {
            return None;
        }
        let mut scale = 100_000_000_u32;
        for byte in fraction.iter() {
            if !byte.is_ascii_digit() {
                return None;
            }
            nanos += u32::from(byte - b'0') * scale;
            scale /= 10;
        }
    }
    Some(Instant {
        seconds: days_from_civil(year, month, day) * SECONDS_PER_DAY
            + hour * 3600
            + minute * 60
            + second,
        nanos,
    })
}

/// Render the fixed three-digit-millisecond record spelling.
pub(crate) fn render_millis(instant: Instant) -> String {
    let days = instant.seconds.div_euclid(SECONDS_PER_DAY);
    let time = instant.seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year,
        month,
        day,
        time / 3600,
        (time % 3600) / 60,
        time % 60,
        instant.nanos / 1_000_000
    )
}

/// Render the strict no-fraction spelling used by scope creation dates.
pub(crate) fn render_seconds(instant: Instant) -> String {
    let days = instant.seconds.div_euclid(SECONDS_PER_DAY);
    let time = instant.seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        day,
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// The wall-clock source of the store. Production reads the system
/// clock; tests inject a deterministic sequence.
pub(crate) trait Clock: std::fmt::Debug {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The production system clock.
#[derive(Debug)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Instant {
            seconds: i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
            nanos: duration.subsec_nanos(),
        }
    }
}

/// The opaque identifier source. Production derives random 128-bit
/// tokens from the SQLite PRNG; tests inject a deterministic sequence.
pub(crate) trait Ids: std::fmt::Debug {
    /// The next opaque 32-character lowercase hex token.
    fn next_id(&mut self) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_the_documented_shapes() {
        assert!(parse("2026-09-30T12:00:00Z").is_some());
        assert!(parse("2026-09-30T12:00:00.123Z").is_some());
        assert!(parse("2026-09-30T12:00:00.123456789Z").is_some());
        assert!(parse("2028-02-29T00:00:00Z").is_some());
    }

    #[test]
    fn parse_rejects_offsets_leap_seconds_and_bad_dates() {
        assert!(parse("2026-09-30T12:00:00+01:00").is_none());
        assert!(parse("2026-09-30T12:00:60Z").is_none());
        assert!(parse("2026-02-30T00:00:00Z").is_none());
        assert!(parse("2026-13-01T00:00:00Z").is_none());
        assert!(parse("2026-09-30T12:00:00.Z").is_none());
        assert!(parse("2026-09-30T12:00:00.1234567890Z").is_none());
        assert!(parse("26-09-30T12:00:00Z").is_none());
    }

    #[test]
    fn rendered_record_order_is_chronological() {
        let early = parse("2026-09-30T12:00:00Z").expect("early");
        let late = parse("2026-09-30T12:00:00.001Z").expect("late");
        let rendered_early = render_millis(early);
        let rendered_late = render_millis(late);
        assert!(rendered_early < rendered_late);
        assert_eq!(
            render_millis(parse(&rendered_early).expect("round trip")),
            rendered_early
        );
    }

    #[test]
    fn second_render_round_trips() {
        let instant = parse("2026-09-30T12:00:00Z").expect("instant");
        assert_eq!(render_seconds(instant), "2026-09-30T12:00:00Z");
    }
}
