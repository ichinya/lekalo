//! Typed identifiers and scalar grammars of the NFR contracts (issue
//! #85).
//!
//! Every wire string becomes one closed typed value before it can
//! exist in an attachment or an evidence set. Constraint ids reuse the
//! dotted lowercase segment shape of semantic ids without inheriting
//! their segment count; scope references reuse the accepted #22
//! semantic grammar; gate and run references reuse the accepted
//! namespaced grammar ([`NamespacedId`]); dates are canonical
//! `YYYY-MM-DD` calendar dates that order lexically; decimals are
//! canonical unsigned decimal strings compared exactly, never through
//! floats.

use std::cmp::Ordering;
use std::fmt;

use crate::scenario::id::IdError;

/// A validated constraint identifier: two to eight dot-separated
/// segments (`planner.nfr.api-focus-p95`), each segment lowercase
/// alphanumeric with `_`/`-`, at most 192 bytes. Constraint ids are
/// stable across revisions; the revision lives in `validity`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ConstraintId(String);

impl ConstraintId {
    /// Validate and keep the exact constraint id text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > 192 {
            return Err(IdError::Length);
        }
        let segments: Vec<&str> = text.split('.').collect();
        if segments.len() < 2 || segments.len() > 8 {
            return Err(IdError::Shape);
        }
        for (index, segment) in segments.iter().enumerate() {
            let ok = !segment.is_empty()
                && segment.len() <= 64
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                && (index > 0 || segment.as_bytes()[0].is_ascii_lowercase());
            if !ok {
                return Err(IdError::Shape);
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConstraintId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A validated canonical calendar date (`YYYY-MM-DD`). Two dates
/// compare in chronological order through their exact text, so no
/// clock and no parsing beyond construction is ever needed.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct IsoDate(String);

impl IsoDate {
    /// Validate and keep the exact date text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || text.as_bytes()[4] != b'-' || text.as_bytes()[7] != b'-' {
            return Err(IdError::Shape);
        }
        if !bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        }) {
            return Err(IdError::Shape);
        }
        let year: u32 = text[..4].parse().map_err(|_| IdError::Shape)?;
        let month: u32 = text[5..7].parse().map_err(|_| IdError::Shape)?;
        let day: u32 = text[8..10].parse().map_err(|_| IdError::Shape)?;
        if month == 0 || month > 12 || day == 0 {
            return Err(IdError::Shape);
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let max_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            _ => return Err(IdError::Shape),
        };
        if day > max_day {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IsoDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A validated canonical unsigned decimal (`0`, `250`, `99.9`): no
/// sign, no exponent, no trailing dot, no leading zeros beyond the one
/// zero itself, at most 32 digits. Comparison is exact and
/// string-based; a measured value is never coerced through a float.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Decimal(String);

impl Decimal {
    /// Validate and keep the exact decimal text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 64 {
            return Err(IdError::Length);
        }
        let (integer, fraction) = match text.split_once('.') {
            Some((integer, fraction)) => (integer, Some(fraction)),
            None => (text, None),
        };
        let ok_integer = if integer.len() == 1 {
            integer.as_bytes()[0].is_ascii_digit()
        } else {
            !integer.is_empty()
                && integer.as_bytes()[0] != b'0'
                && integer.bytes().all(|byte| byte.is_ascii_digit())
        };
        let ok_fraction = match fraction {
            None => true,
            Some(fraction) => {
                !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
            }
        };
        if !ok_integer || !ok_fraction {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The exact numeric comparison of two decimals.
    pub fn cmp_value(&self, other: &Self) -> Ordering {
        cmp_decimal(self.0.as_str(), other.0.as_str())
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Exact decimal comparison over canonical spellings: longer integer
/// parts are larger; equal-length integer parts compare textually;
/// fractions compare digit by digit with the shorter run padded by
/// implicit zeros.
pub(crate) fn cmp_decimal(left: &str, right: &str) -> Ordering {
    let (left_integer, left_fraction) = split_decimal(left);
    let (right_integer, right_fraction) = split_decimal(right);
    let by_integer = left_integer
        .len()
        .cmp(&right_integer.len())
        .then_with(|| left_integer.cmp(right_integer));
    if by_integer.is_ne() {
        return by_integer;
    }
    let left = left_fraction.trim_end_matches('0');
    let right = right_fraction.trim_end_matches('0');
    left.cmp(right)
}

fn split_decimal(text: &str) -> (&str, &str) {
    match text.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (text, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::id::NamespacedId;

    #[test]
    fn constraint_ids_are_dotted_lowercase() {
        assert!(ConstraintId::parse("planner.nfr.api-focus-p95").is_ok());
        assert!(ConstraintId::parse("planner.cost.monthly").is_ok());
        assert!(ConstraintId::parse("a.b").is_ok());
        assert!(ConstraintId::parse("planner").is_err());
        assert!(ConstraintId::parse("Planner.nfr.x").is_err());
        assert!(ConstraintId::parse("planner..x").is_err());
        assert!(ConstraintId::parse("1lanner.nfr.x").is_err());
        assert!(ConstraintId::parse(&"a.b".repeat(100)).is_err());
    }

    #[test]
    fn dates_are_real_calendar_dates_in_canonical_spelling() {
        assert!(IsoDate::parse("2026-09-10").is_ok());
        assert!(IsoDate::parse("2024-02-29").is_ok());
        assert!(IsoDate::parse("2026-02-29").is_err());
        assert!(IsoDate::parse("2026-13-01").is_err());
        assert!(IsoDate::parse("2026-00-10").is_err());
        assert!(IsoDate::parse("2026-04-31").is_err());
        assert!(IsoDate::parse("2026-9-10").is_err());
        assert!(IsoDate::parse("20260910").is_err());
        assert_eq!(
            IsoDate::parse("2026-09-10").unwrap(),
            IsoDate::parse("2026-09-10").unwrap()
        );
        assert!(
            IsoDate::parse("2026-09-10").unwrap() < IsoDate::parse("2026-12-10").unwrap(),
            "dates order chronologically through their text"
        );
    }

    #[test]
    fn decimals_are_canonical_and_exactly_comparable() {
        assert!(Decimal::parse("250").is_ok());
        assert!(Decimal::parse("99.9").is_ok());
        assert!(Decimal::parse("0").is_ok());
        assert!(Decimal::parse("0.5").is_ok());
        assert!(Decimal::parse("-1").is_err());
        assert!(Decimal::parse("1e3").is_err());
        assert!(Decimal::parse("01").is_err());
        assert!(Decimal::parse("1.").is_err());
        assert!(Decimal::parse(".5").is_err());
        assert!(Decimal::parse("").is_err());
        assert!(Decimal::parse(&"9".repeat(65)).is_err());
        let small = Decimal::parse("99.9").unwrap();
        let large = Decimal::parse("250").unwrap();
        assert_eq!(small.cmp_value(&large), Ordering::Less);
        assert_eq!(large.cmp_value(&small), Ordering::Greater);
        assert_eq!(
            Decimal::parse("0.10")
                .unwrap()
                .cmp_value(&Decimal::parse("0.1").unwrap()),
            Ordering::Equal
        );
        assert_eq!(
            Decimal::parse("2")
                .unwrap()
                .cmp_value(&Decimal::parse("10").unwrap()),
            Ordering::Less
        );
        assert_eq!(
            Decimal::parse("99.95")
                .unwrap()
                .cmp_value(&Decimal::parse("99.9").unwrap()),
            Ordering::Greater
        );
    }

    #[test]
    fn namespaced_refs_reuse_the_scenario_grammar() {
        assert!(NamespacedId::parse("perf.gates/api-focus-p95").is_ok());
        assert!(NamespacedId::parse("adapters.node/runner-x").is_ok());
        assert!(NamespacedId::parse("no-slash").is_err());
        assert!(NamespacedId::parse("perf.gates/").is_err());
    }
}
