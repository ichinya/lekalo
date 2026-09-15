//! The closed type system and runtime value model of the expression
//! family (issue #66).
//!
//! Six scalar types and homogeneous sets of them: `bool`, `int`
//! (bounded to ±2^53−1 so every target runtime is exact), `string`
//! (BMP-only UTF-8 so code-point order is byte order everywhere),
//! `datetime` (canonical UTC `YYYY-MM-DDTHH:MM:SSZ`, year
//! 0001–9999, no leap seconds), `duration` (integer seconds), and
//! `set:<scalar>`. Nullability is a property of declared references
//! only: a nullable reference may appear solely as the operand of
//! `is-null`/`not-null`, so no null value ever flows through an
//! operator. There are no floats, no records, no lists of
//! non-scalars, and no union types: the static checker decides every
//! operator combination, and the value model has one closed
//! representation per type.

use serde_json::{json, Value as Json};

use super::version::{MAX_DURATION_SECONDS, MAX_INT};

/// One scalar type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ScalarType {
    /// A boolean.
    Bool,
    /// A bounded signed integer.
    Int,
    /// A bounded BMP-only UTF-8 string.
    Str,
    /// A canonical UTC timestamp.
    DateTime,
    /// An integer-seconds duration.
    Duration,
}

impl ScalarType {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Str => "string",
            Self::DateTime => "datetime",
            Self::Duration => "duration",
        }
    }

    /// Parse one wire spelling.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "bool" => Some(Self::Bool),
            "int" => Some(Self::Int),
            "string" => Some(Self::Str),
            "datetime" => Some(Self::DateTime),
            "duration" => Some(Self::Duration),
            _ => None,
        }
    }

    /// Whether the scalar type participates in ordering operators.
    pub const fn ordered(self) -> bool {
        !matches!(self, Self::Bool)
    }
}

/// One expression type: a scalar or a homogeneous set of scalars.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ExprType {
    /// A scalar value.
    Scalar(ScalarType),
    /// A homogeneous, duplicate-free, sorted set.
    Set(ScalarType),
}

impl ExprType {
    /// Parse one wire spelling (`bool`, `set:int`, …).
    pub fn parse(key: &str) -> Option<Self> {
        if let Some(inner) = key.strip_prefix("set:") {
            return ScalarType::parse(inner).map(Self::Set);
        }
        ScalarType::parse(key).map(Self::Scalar)
    }

    /// The exact wire spelling.
    pub fn key(&self) -> String {
        match self {
            Self::Scalar(scalar) => scalar.key().to_owned(),
            Self::Set(scalar) => format!("set:{}", scalar.key()),
        }
    }

    /// The scalar type when this is a scalar.
    pub fn as_scalar(&self) -> Option<ScalarType> {
        match self {
            Self::Scalar(scalar) => Some(*scalar),
            Self::Set(_) => None,
        }
    }
}

/// One runtime scalar value.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Scalar {
    /// A boolean.
    Bool(bool),
    /// A bounded integer.
    Int(i64),
    /// A bounded string.
    Str(String),
    /// A UTC timestamp as seconds since the Unix epoch.
    DateTime(i64),
    /// A duration as integer seconds.
    Duration(i64),
}

impl Scalar {
    /// The static type of this value.
    pub fn ty(&self) -> ScalarType {
        match self {
            Self::Bool(_) => ScalarType::Bool,
            Self::Int(_) => ScalarType::Int,
            Self::Str(_) => ScalarType::Str,
            Self::DateTime(_) => ScalarType::DateTime,
            Self::Duration(_) => ScalarType::Duration,
        }
    }

    /// The canonical ordering key: booleans false < true, numbers by
    /// value, strings by code point (byte order on the UTF-8 form).
    pub fn cmp_key(&self) -> Scalar {
        match self {
            Self::Str(text) => Self::Str(text.clone()),
            other => other.clone(),
        }
    }

    /// The typed wire JSON: `datetime` stays a canonical string and
    /// everything else is native JSON.
    pub fn to_json(&self) -> Json {
        match self {
            Self::Bool(value) => json!(value),
            Self::Int(value) => json!(value),
            Self::Str(value) => json!(value),
            Self::DateTime(seconds) => json!(datetime_render(*seconds)),
            Self::Duration(seconds) => json!(seconds),
        }
    }
}

/// One closed runtime value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// A scalar value.
    Scalar(Scalar),
    /// A sorted, duplicate-free set of one scalar type.
    Set(Vec<Scalar>),
}

impl Value {
    /// The static type of this value.
    pub fn ty(&self) -> ExprType {
        match self {
            Self::Scalar(scalar) => ExprType::Scalar(scalar.ty()),
            Self::Set(items) => ExprType::Set(
                items
                    .first()
                    .map(|item| item.ty())
                    .unwrap_or(ScalarType::Str),
            ),
        }
    }

    /// The typed wire JSON; sets render as sorted arrays.
    pub fn to_json(&self) -> Json {
        match self {
            Self::Scalar(scalar) => scalar.to_json(),
            Self::Set(items) => Json::Array(items.iter().map(Scalar::to_json).collect()),
        }
    }
}

/// The seconds of one canonical day (the civil-calendar conversion
/// below is day-based).
const SECONDS_PER_DAY: i64 = 86_400;

/// Days from 1970-01-01 to the given civil date (Howard Hinnant's
/// `days_from_civil`; valid for the full year 0001–9999 range).
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_shift = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shift + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date of the given days since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shift = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_shift + 2) / 5 + 1;
    let month = if month_shift < 10 {
        month_shift + 3
    } else {
        month_shift - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
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

/// Validate a canonical UTC datetime spelling and return its seconds
/// since the Unix epoch. The accepted form is exactly
/// `YYYY-MM-DDTHH:MM:SSZ` with a four-digit year 0001–9999, zero
/// padding, no offsets, no fractions, and no leap second (`SS` ≤ 59).
pub(crate) fn datetime_parse(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' || bytes[19] != b'Z' {
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
    Some(days_from_civil(year, month, day) * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second)
}

/// Render canonical UTC datetime text from seconds since the epoch.
/// Values outside year 0001–9999 have no rendering.
pub(crate) fn datetime_render(seconds: i64) -> String {
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let time = seconds.rem_euclid(SECONDS_PER_DAY);
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

/// Whether seconds stay inside the renderable year range.
pub(crate) fn datetime_in_range(seconds: i64) -> bool {
    datetime_parse(&datetime_render(seconds)).is_some()
}

/// Whether a string is BMP-only UTF-8 with no unpaired surrogates and
/// no control characters below 0x20 (canonical text only). Rust
/// `String`s are always valid UTF-8; the check rejects code points
/// above U+FFFF (non-BMP) so target orderings and casings are
/// identical, and rejects C0 controls so canonical text stays
/// printable.
pub(crate) fn string_is_canonical(text: &str) -> bool {
    text.chars()
        .all(|c| (c as u32) <= 0xFFFF && c as u32 >= 0x20)
}

/// Validate one runtime integer against the family bound.
pub(crate) fn int_in_range(value: i64) -> bool {
    (-MAX_INT..=MAX_INT).contains(&value)
}

/// Validate one runtime duration against the family bound.
pub(crate) fn duration_in_range(value: i64) -> bool {
    (-MAX_DURATION_SECONDS..=MAX_DURATION_SECONDS).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_keys_round_trip() {
        for key in ["bool", "int", "string", "datetime", "duration"] {
            assert_eq!(ExprType::parse(key).expect("scalar").key(), key);
        }
        for key in ["set:bool", "set:int", "set:string"] {
            assert_eq!(ExprType::parse(key).expect("set").key(), key);
        }
        assert!(ExprType::parse("float").is_none());
        assert!(ExprType::parse("set:set:int").is_none());
        assert!(!ScalarType::Bool.ordered());
        assert!(ScalarType::Int.ordered());
    }

    #[test]
    fn datetime_round_trips_canonical_forms() {
        for text in [
            "0001-01-01T00:00:00Z",
            "1970-01-01T00:00:00Z",
            "2000-02-29T12:34:56Z",
            "9999-12-31T23:59:59Z",
        ] {
            let seconds = datetime_parse(text).expect("parses");
            assert_eq!(datetime_render(seconds), text);
        }
        assert_eq!(datetime_parse("1970-01-01T00:00:60Z"), None);
        assert_eq!(datetime_parse("2001-02-29T00:00:00Z"), None);
        assert_eq!(datetime_parse("1970-1-01T00:00:00Z"), None);
        assert_eq!(datetime_parse("1970-01-01 00:00:00Z"), None);
        assert_eq!(datetime_parse("1970-01-01T00:00:00+00:00"), None);
        assert_eq!(datetime_parse("10000-01-01T00:00:00Z"), None);
    }

    #[test]
    fn civil_conversions_agree_with_unix_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        // Symmetry over a dense sample including leap boundaries.
        for days in -25_567..=500_000i64 {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days, "day {days}");
        }
    }

    #[test]
    fn canonical_strings_reject_non_bmp_and_controls() {
        assert!(string_is_canonical("héllo wörld"));
        assert!(!string_is_canonical("emoji \u{1F600}"));
        assert!(!string_is_canonical("tab\t"));
        assert!(string_is_canonical("del \u{7F}"));
    }

    #[test]
    fn bounds_match_the_published_constants() {
        assert!(int_in_range(MAX_INT) && !int_in_range(MAX_INT + 1));
        assert!(int_in_range(-MAX_INT) && !int_in_range(-MAX_INT - 1));
        assert!(duration_in_range(MAX_DURATION_SECONDS));
        assert!(!duration_in_range(MAX_DURATION_SECONDS + 1));
    }
}
