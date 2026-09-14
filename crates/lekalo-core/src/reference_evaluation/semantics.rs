//! The deterministic reference-semantics kernel (issue #107).
//!
//! This module owns every behavioral decision of the reference
//! evaluator over the closed #63 predicate/value AST: literal
//! projection, operand resolution against input, prior row, and the
//! evaluation clock, exact-spelling equality, chronological ordering
//! with offset normalization, calendar duration arithmetic, and the
//! deterministic ID derivation of the #23 `given` ID sources. Nothing
//! here reads the filesystem, the wall clock, randomness, or the
//! network; the same inputs produce byte-identical decisions on every
//! host.
//!
//! Unsupported operand shapes never guess: they return one fixed
//! reason token which the executor records as the `unsupported`
//! outcome of the step.

use std::collections::BTreeMap;

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::invariant_transition::expr::PredicateNode;
use crate::invariant_transition::state::{Duration, DurationUnit, ValueNode};
use crate::scenario::value::TypedValue;

/// The closed reason vocabulary of the reference semantics. Every
/// token is stable wire data of the trace contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Reason {
    /// A referenced row field was never established.
    FieldUnset,
    /// A referenced input field was not supplied.
    InputUnset,
    /// The two operands have no closed ordering together.
    Incomparable,
    /// An operand kind cannot take part in the operation.
    IncompatibleKind,
    /// A normalized literal denotes no runtime-representable value.
    DatetimeRange,
    /// An aggregate reference names no IR entity.
    AggregateUnknown,
    /// The #63 contract binds no row field to state identifiers.
    StateFieldUnbound,
    /// A one-active invariant was declared without its bound.
    MaxActiveMissing,
}
impl Reason {
    /// The exact wire token.
    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::FieldUnset => "field-unset",
            Self::InputUnset => "input-unset",
            Self::Incomparable => "incomparable",
            Self::IncompatibleKind => "incompatible-kind",
            Self::DatetimeRange => "datetime-out-of-range",
            Self::AggregateUnknown => "aggregate-unknown",
            Self::StateFieldUnbound => "state-field-unbound",
            Self::MaxActiveMissing => "max-active-missing",
        }
    }
}

/// One resolved operand: a closed typed value of the runtime state.
pub(crate) type Value = TypedValue;

/// The operand-resolution context of one predicate evaluation.
pub(crate) struct Context<'a> {
    /// The transition input fields, resolved to typed values and keyed
    /// by field name.
    pub input: &'a BTreeMap<String, Value>,
    /// The prior row fields of the state-space entity, when a target
    /// row exists.
    pub row: Option<&'a BTreeMap<String, Value>>,
    /// The state-space entity the predicate is evaluated against.
    pub entity: &'a str,
    /// The evaluation clock (canonical UTC datetime).
    pub clock: &'a str,
}

/// Project one declared literal node into a runtime value. Enum
/// members run as their exact member text; the declared enum type
/// reference is recorded by the trace layer, not the value.
pub(crate) fn literal(node: &ValueNode) -> Result<Value, Reason> {
    match node {
        ValueNode::Null => Ok(Value::Null),
        ValueNode::Boolean(flag) => Ok(Value::Boolean(*flag)),
        ValueNode::Integer(number) => Ok(Value::Integer(*number)),
        ValueNode::String(text) => Ok(Value::String(text.clone())),
        ValueNode::Decimal(text) => Ok(Value::Decimal(text.clone())),
        ValueNode::Date(text) => Ok(Value::Date(text.clone())),
        ValueNode::DateTime(text) => normalize_datetime(text).map(Value::Datetime),
        ValueNode::Uuid(text) => Ok(Value::Uuid(text.clone())),
        ValueNode::Uri(text) => Ok(Value::Uri(text.clone())),
        ValueNode::EnumMember { value, .. } => Ok(Value::String(value.clone())),
        ValueNode::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(literal(item)?);
            }
            Ok(Value::List(values))
        }
        ValueNode::Object(entries) => {
            let mut pairs = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                pairs.push((
                    crate::scenario::id::FieldName::parse(key)
                        .map_err(|_| Reason::IncompatibleKind)?,
                    literal(item)?,
                ));
            }
            Ok(Value::Object(pairs))
        }
        // References are resolved by `resolve`, never projected as
        // literals.
        ValueNode::Field { .. }
        | ValueNode::Input { .. }
        | ValueNode::Prior { .. }
        | ValueNode::Now => Err(Reason::IncompatibleKind),
    }
}

/// Resolve one value node against the evaluation context.
pub(crate) fn resolve(node: &ValueNode, context: &Context<'_>) -> Result<Value, Reason> {
    match node {
        ValueNode::Field { entity, field } => {
            if let Some(entity) = entity {
                if entity.as_str() != context.entity {
                    return Err(Reason::IncompatibleKind);
                }
            }
            let row = context.row.ok_or(Reason::FieldUnset)?;
            row.get(field.as_str()).cloned().ok_or(Reason::FieldUnset)
        }
        ValueNode::Input { field } => context
            .input
            .get(field.as_str())
            .cloned()
            .ok_or(Reason::InputUnset),
        ValueNode::Prior { field } => {
            let row = context.row.ok_or(Reason::FieldUnset)?;
            row.get(field.as_str()).cloned().ok_or(Reason::FieldUnset)
        }
        ValueNode::Now => Ok(Value::Datetime(context.clock.to_owned())),
        other => literal(other),
    }
}

/// Evaluate one predicate against the context. The result is the
/// deterministic boolean decision, or the fixed unsupported reason.
pub(crate) fn evaluate(node: &PredicateNode, context: &Context<'_>) -> Result<bool, Reason> {
    match node {
        PredicateNode::Equal { left, right } => {
            Ok(resolve(left, context)? == resolve(right, context)?)
        }
        PredicateNode::NotEqual { left, right } => {
            Ok(resolve(left, context)? != resolve(right, context)?)
        }
        PredicateNode::IsNull { operand } => Ok(resolve(operand, context)? == Value::Null),
        PredicateNode::NotNull { operand } => Ok(resolve(operand, context)? != Value::Null),
        PredicateNode::InSet { operand, values } => {
            let operand = resolve(operand, context)?;
            let mut set = Vec::with_capacity(values.len());
            for value in values {
                set.push(literal(value)?);
            }
            Ok(set.contains(&operand))
        }
        PredicateNode::All { from, predicate } => {
            // As with `any`: the collection operand is validated
            // through the common member helper, and the #63 AST has
            // no member-bound variable inside a quantified predicate:
            // references keep resolving against input, prior row, and
            // `now`. The rule is therefore the literal one — an empty
            // collection is vacuously true without evaluating the
            // predicate, and a nonempty collection takes the member
            // predicate decision in the same context (issue #107
            // correction 2).
            let collection = resolve(from, context)?;
            if members(&collection)?.is_empty() {
                return Ok(true);
            }
            Ok(evaluate(predicate, context)?)
        }
        PredicateNode::Any { from, predicate } => {
            // As with `all`: no member binding exists, so `any` over
            // an empty collection is false and otherwise the member
            // predicate decision in the same context.
            let collection = resolve(from, context)?;
            if members(&collection)?.is_empty() {
                return Ok(false);
            }
            Ok(evaluate(predicate, context)?)
        }
        PredicateNode::And { operands } => {
            for operand in operands {
                if !evaluate(operand, context)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        PredicateNode::Or { operands } => {
            for operand in operands {
                if evaluate(operand, context)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        PredicateNode::Before { left, right } => Ok(compare(
            &resolve(left, context)?,
            &resolve(right, context)?,
        )? == std::cmp::Ordering::Less),
        PredicateNode::After { left, right } => Ok(compare(
            &resolve(left, context)?,
            &resolve(right, context)?,
        )? == std::cmp::Ordering::Greater),
        PredicateNode::Within { left, duration } => {
            // The span is compared numerically against the elapsed
            // whole-second distance between the operand and the
            // evaluation clock: the temporary bounds are never
            // serialized or reparsed, so only the operands themselves
            // must be representable wire datetimes. Endpoints stay
            // inclusive and fraction spellings order numerically at
            // exact whole-second boundary distance (issue #107
            // correction 3).
            let left = resolve(left, context)?;
            let Value::Datetime(left_text) = &left else {
                return Err(Reason::Incomparable);
            };
            let (left_days, left_second, left_fraction) =
                datetime_parts(left_text).ok_or(Reason::IncompatibleKind)?;
            let (now_days, now_second, now_fraction) =
                datetime_parts(context.clock).ok_or(Reason::IncompatibleKind)?;
            let span = duration_seconds(duration);
            let delta = (left_days * 86_400 + left_second) - (now_days * 86_400 + now_second);
            let fraction = align(&left_fraction, &now_fraction, false);
            let at_or_after_lower =
                delta > -span || (delta == -span && fraction != std::cmp::Ordering::Less);
            let at_or_before_upper =
                delta < span || (delta == span && fraction != std::cmp::Ordering::Greater);
            Ok(at_or_after_lower && at_or_before_upper)
        }
        PredicateNode::Count { from, min, max } => {
            let collection = resolve(from, context)?;
            let count = members(&collection)?.len() as i64;
            Ok(count >= *min && count <= *max)
        }
        PredicateNode::MemberOf { operand, set } => {
            let operand = resolve(operand, context)?;
            let set = resolve(set, context)?;
            Ok(members(&set)?.contains(&operand))
        }
    }
}

/// The member view of one collection operand.
fn members(value: &Value) -> Result<Vec<Value>, Reason> {
    match value {
        Value::List(items) => Ok(items.clone()),
        Value::Null => Ok(Vec::new()),
        _ => Err(Reason::IncompatibleKind),
    }
}

/// The closed total ordering over comparable operands. Equality of
/// spellings is exact (`PartialEq`); ordering is by value where a
/// chronological or numeric order exists, and by unsigned UTF-8 bytes
/// for strings, UUIDs, and URIs. Booleans order false before true.
/// Null and mismatched kinds are incomparable.
pub(crate) fn compare(left: &Value, right: &Value) -> Result<std::cmp::Ordering, Reason> {
    use std::cmp::Ordering;
    match (left, right) {
        (Value::Null, Value::Null) => Ok(Ordering::Equal),
        (Value::Boolean(a), Value::Boolean(b)) => Ok(a.cmp(b)),
        (Value::Integer(a), Value::Integer(b)) => Ok(a.cmp(b)),
        (Value::Integer(a), Value::Decimal(b)) => decimal_of_integer(a, b),
        (Value::Decimal(a), Value::Integer(b)) => Ok(decimal_of_integer(b, a)?.reverse()),
        (Value::Decimal(a), Value::Decimal(b)) => Ok(compare_decimal(a, b)),
        (Value::String(a), Value::String(b))
        | (Value::Uuid(a), Value::Uuid(b))
        | (Value::Uri(a), Value::Uri(b)) => Ok(a.as_bytes().cmp(b.as_bytes())),
        (Value::Date(a), Value::Date(b)) => Ok(a.as_bytes().cmp(b.as_bytes())),
        (Value::Datetime(a), Value::Datetime(b)) => compare_datetime(a, b),
        _ => Err(Reason::Incomparable),
    }
}

/// Compare one integer against one canonical decimal.
fn decimal_of_integer(integer: &i64, decimal: &str) -> Result<std::cmp::Ordering, Reason> {
    let text = integer.to_string();
    Ok(compare_decimal(&text, decimal))
}

/// Numeric comparison of two canonical decimals via aligned
/// integer/fraction digit strings.
pub(crate) fn compare_decimal(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (left_sign, left_int, left_frac) = split_decimal(left);
    let (right_sign, right_int, right_frac) = split_decimal(right);
    let sign_order = left_sign.cmp(&right_sign);
    if sign_order != Ordering::Equal {
        return sign_order;
    }
    let magnitude = compare_magnitude(&left_int, &left_frac, &right_int, &right_frac);
    if left_sign == std::cmp::Ordering::Less {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Split one canonical decimal into sign, integer digits, and
/// fraction digits.
fn split_decimal(text: &str) -> (std::cmp::Ordering, String, String) {
    use std::cmp::Ordering;
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (Ordering::Less, rest),
        None => (Ordering::Greater, text),
    };
    match digits.split_once('.') {
        Some((integer, fraction)) => (sign, integer.to_owned(), fraction.to_owned()),
        None => (sign, digits.to_owned(), String::new()),
    }
}

/// Compare magnitudes by aligned integer and zero-padded fraction
/// digits.
fn compare_magnitude(
    left_int: &str,
    left_frac: &str,
    right_int: &str,
    right_frac: &str,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let int = align(left_int, right_int, true);
    if int != Ordering::Equal {
        return int;
    }
    align(left_frac, right_frac, false)
}

/// Compare two digit strings after left-padding (integers) or
/// right-padding (fractions) to equal length.
fn align(left: &str, right: &str, left_pad: bool) -> std::cmp::Ordering {
    let width = left.len().max(right.len());
    let (left, right) = if left_pad {
        (format!("{left:0>width$}", width = width), {
            format!("{right:0>width$}", width = width)
        })
    } else {
        (format!("{left:0<width$}", width = width), {
            format!("{right:0<width$}", width = width)
        })
    };
    left.cmp(&right)
}

/// Normalize one #63 timestamp literal (which may carry an explicit
/// offset) into the canonical UTC `Z` spelling of the runtime value.
pub(crate) fn normalize_datetime(text: &str) -> Result<String, Reason> {
    let date = &text[..10];
    let time_and_zone = &text[11..];
    if text.as_bytes().get(10) != Some(&b'T') {
        return Err(Reason::IncompatibleKind);
    }
    let (time, zone) = match time_and_zone.find(['Z', '+', '-']) {
        Some(index) => (&time_and_zone[..index], &time_and_zone[index..]),
        None => return Err(Reason::IncompatibleKind),
    };
    let (whole, fraction) = match time.split_once('.') {
        Some((whole, fraction)) => {
            // The whole-second clock is parsed separately from the
            // fraction, which survives offset normalization with its
            // exact precision (issue #107 correction 1).
            if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Reason::IncompatibleKind);
            }
            (whole, Some(fraction))
        }
        None => (time, None),
    };
    let mut parts = [0i64; 3];
    for (slot, piece) in parts.iter_mut().zip(whole.split(':')) {
        *slot = piece.parse::<i64>().map_err(|_| Reason::IncompatibleKind)?;
    }
    let [mut hour, mut minute, _second] = parts;
    // Normalize the offset onto the civil time.
    let mut day_shift = 0i64;
    if zone != "Z" {
        let sign = if zone.starts_with('-') { -1i64 } else { 1i64 };
        let offset = &zone[1..];
        let (offset_hour, offset_minute) =
            offset.split_once(':').ok_or(Reason::IncompatibleKind)?;
        let offset_hour: i64 = offset_hour.parse().map_err(|_| Reason::IncompatibleKind)?;
        let offset_minute: i64 = offset_minute
            .parse()
            .map_err(|_| Reason::IncompatibleKind)?;
        // UTC = local - offset.
        minute -= sign * offset_minute;
        hour -= sign * offset_hour;
        while minute < 0 {
            minute += 60;
            hour -= 1;
        }
        while minute >= 60 {
            minute -= 60;
            hour += 1;
        }
        while hour < 0 {
            hour += 24;
            day_shift -= 1;
        }
        while hour >= 24 {
            hour -= 24;
            day_shift += 1;
        }
    }
    // `second` needs no offset arithmetic beyond whole-minute zones,
    // which the #63 grammar guarantees.
    // Rebuild the civil date after the day shift.
    let (year, month, day) = civil_of_date(date).ok_or(Reason::IncompatibleKind)?;
    let days = days_from_civil(year, month, day) + day_shift;
    let (year, month, day) = civil_from_days(days);
    // The runtime typed-value contract admits exactly four-digit
    // years `0001..=9999`: an offset normalization that rolls out of
    // that range (e.g. `9999-12-31T23:00:00.1-02:00` to year 10000,
    // or `0001-01-01T00:00:00.1+01:00` to year 0) denotes no
    // canonical UTC value and is refused as a typed unsupported
    // outcome before any write or event (issue #107 correction 3).
    if !(1..=9999).contains(&year) {
        return Err(Reason::DatetimeRange);
    }
    let mut text = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{_second:02}");
    if let Some(fraction) = fraction {
        text.push('.');
        text.push_str(fraction);
    }
    text.push('Z');
    Ok(text)
}

/// Structurally compare two canonical UTC datetimes.
fn compare_datetime(left: &str, right: &str) -> Result<std::cmp::Ordering, Reason> {
    let (left_days, left_second, left_fraction) =
        datetime_parts(left).ok_or(Reason::IncompatibleKind)?;
    let (right_days, right_second, right_fraction) =
        datetime_parts(right).ok_or(Reason::IncompatibleKind)?;
    let days = left_days.cmp(&right_days);
    if days != std::cmp::Ordering::Equal {
        return Ok(days);
    }
    let second = left_second.cmp(&right_second);
    if second != std::cmp::Ordering::Equal {
        return Ok(second);
    }
    // Fraction digits order numerically: right-pad the exact
    // serialized spellings to a common width so `.5`, `.50`, an
    // absent fraction, and `.0` denote one and the same instant,
    // while unequal fractions keep their chronological order and
    // every stored spelling keeps its exact precision (issue #107
    // correction 2).
    Ok(align(&left_fraction, &right_fraction, false))
}

/// The comparable tuple of one canonical UTC datetime: epoch days,
/// second of day, and fraction digits.
fn datetime_parts(text: &str) -> Option<(i64, i64, String)> {
    let date = &text.get(..10)?;
    let rest = &text.get(11..)?;
    let zone = rest.strip_suffix('Z')?;
    let (clock, fraction) = match zone.find('.') {
        Some(index) => (&zone[..index], zone[index + 1..].to_owned()),
        None => (zone, String::new()),
    };
    let mut second_of_day = 0i64;
    for piece in clock.split(':') {
        second_of_day = second_of_day * 60 + piece.parse::<i64>().ok()?;
    }
    let (year, month, day) = civil_of_date(date)?;
    Some((days_from_civil(year, month, day), second_of_day, fraction))
}

/// Parse `YYYY-MM-DD` into its civil parts.
fn civil_of_date(date: &str) -> Option<(i64, u32, u32)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

/// Real Gregorian month lengths (shared with the #23 value grammar).
fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
    }
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days from the civil date (Howard Hinnant's algorithm); epoch is
/// 1970-01-01 = day 0.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Civil date from days since the epoch.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// The whole-second span of one duration.
fn duration_seconds(duration: &Duration) -> i64 {
    let amount = duration.amount();
    match duration.unit() {
        DurationUnit::Seconds => amount,
        DurationUnit::Minutes => amount.saturating_mul(60),
        DurationUnit::Hours => amount.saturating_mul(3_600),
        DurationUnit::Days => amount.saturating_mul(86_400),
    }
}

/// Resolve one bounded member path into a referenced value: object
/// fields by name, list items by decimal index, one to sixteen
/// segments.
pub(crate) fn member_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = match current {
            Value::Object(entries) => entries
                .iter()
                .find(|(name, _)| name.as_str() == segment)
                .map(|(_, value)| value)?,
            Value::List(items) => {
                let index: usize = segment.parse().ok()?;
                items.get(index)?
            }
            _ => return None,
        };
    }
    Some(current)
}

/// Derive one deterministic ID of a #23 ID source. Sequence sources
/// derive `{seed}-{index}`; UUIDv4 sources derive the lowercase
/// hyphenated v4 spelling of SHA-256(`seed:index`), with the version
/// and variant bits forced per RFC 4122.
pub(crate) fn derive_id(
    seed: &str,
    algorithm: crate::scenario::precondition::IdAlgorithm,
    index: u64,
) -> Value {
    match algorithm {
        crate::scenario::precondition::IdAlgorithm::Sequence => {
            Value::String(format!("{seed}-{index}"))
        }
        crate::scenario::precondition::IdAlgorithm::UuidV4 => {
            let mut hasher = Sha256::new();
            hasher.update(seed.as_bytes());
            hasher.update(b":");
            hasher.update(index.to_string().as_bytes());
            let digest: [u8; 32] = hasher.finalize().into();
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&digest[..16]);
            // Force the RFC 4122 version (4) and variant (10xx) bits;
            // the identity is the derived digest with exactly these
            // two bits pinned, everything else untouched.
            bytes[6] = (bytes[6] & 0x0f) | 0x40;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            let mut hex = String::with_capacity(32);
            for byte in bytes.iter() {
                let _ = write!(hex, "{byte:02x}");
            }
            Value::Uuid(format!(
                "{}-{}-{}-{}-{}",
                &hex[..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..32],
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_compare_numerically() {
        assert_eq!(compare_decimal("1.5", "1.25"), std::cmp::Ordering::Greater);
        assert_eq!(compare_decimal("1", "1.0"), std::cmp::Ordering::Equal);
        assert_eq!(compare_decimal("-1", "1"), std::cmp::Ordering::Less);
        assert_eq!(compare_decimal("-0.5", "-1"), std::cmp::Ordering::Greater);
        assert_eq!(compare_decimal("10", "9"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn datetime_offsets_normalize_to_utc() {
        assert_eq!(
            normalize_datetime("2026-09-05T10:20:30+03:00").expect("normalizes"),
            "2026-09-05T07:20:30Z"
        );
        assert_eq!(
            normalize_datetime("2026-09-05T10:20:30-05:00").expect("normalizes"),
            "2026-09-05T15:20:30Z"
        );
        assert_eq!(
            normalize_datetime("2026-09-05T10:20:30Z").expect("normalizes"),
            "2026-09-05T10:20:30Z"
        );
    }

    #[test]
    fn day_boundaries_shift_correctly() {
        assert_eq!(
            normalize_datetime("2026-09-05T01:00:00+03:00").expect("normalizes"),
            "2026-09-04T22:00:00Z"
        );
        assert_eq!(
            normalize_datetime("2026-12-31T23:00:00-02:00").expect("normalizes"),
            "2027-01-01T01:00:00Z"
        );
    }

    /// An offset normalization that rolls out of the four-digit
    /// runtime range denotes no canonical value and is refused as the
    /// typed `datetime-out-of-range` reason, while the ordinary
    /// in-range rollover keeps executing (issue #107 correction 3,
    /// review B3.1).
    #[test]
    fn out_of_range_offset_normalization_is_refused() {
        assert_eq!(
            normalize_datetime("9999-12-31T23:00:00.1-02:00"),
            Err(Reason::DatetimeRange)
        );
        assert_eq!(
            normalize_datetime("0001-01-01T00:00:00.1+01:00"),
            Err(Reason::DatetimeRange)
        );
        // Exactly at the edges without a shift: still representable.
        assert_eq!(
            normalize_datetime("9999-12-31T23:59:59.1+02:00").expect("edge normalizes"),
            "9999-12-31T21:59:59.1Z"
        );
        assert_eq!(
            normalize_datetime("0001-01-01T00:00:00.1-02:00").expect("edge normalizes"),
            "0001-01-01T02:00:00.1Z"
        );
        // The in-range year-rollover control keeps working.
        assert_eq!(
            normalize_datetime("2026-01-01T00:00:00.1+01:00").expect("control normalizes"),
            "2025-12-31T23:00:00.1Z"
        );
    }

    #[test]
    fn fractional_seconds_normalize_and_keep_precision() {
        assert_eq!(
            normalize_datetime("2026-09-08T12:00:00.123Z").expect("normalizes"),
            "2026-09-08T12:00:00.123Z"
        );
        // An offset timestamp keeps its exact fraction after the UTC
        // shift, including across day boundaries.
        assert_eq!(
            normalize_datetime("2026-09-08T15:30:00.456+03:00").expect("normalizes"),
            "2026-09-08T12:30:00.456Z"
        );
        assert_eq!(
            normalize_datetime("2026-12-31T23:59:59.999999-02:00").expect("normalizes"),
            "2027-01-01T01:59:59.999999Z"
        );
        assert!(normalize_datetime("2026-09-08T12:00:00.Z").is_err());
    }

    #[test]
    fn fractional_datetimes_order_chronologically() {
        assert_eq!(
            compare_datetime(
                &normalize_datetime("2026-09-08T12:00:00.1235Z").expect("normalizes"),
                &normalize_datetime("2026-09-08T12:00:00.123Z").expect("normalizes"),
            )
            .expect("comparable"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn fractions_order_chronologically() {
        assert_eq!(
            compare_datetime("2026-09-05T10:20:30.5Z", "2026-09-05T10:20:30.25Z")
                .expect("comparable"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_datetime("2026-09-05T10:20:30Z", "2026-09-05T10:20:30.1Z").expect("comparable"),
            std::cmp::Ordering::Less
        );
    }

    /// Equal instants spelled with different fraction widths compare
    /// Equal: `.5`, `.50`, an absent fraction, and `.0` are one
    /// instant, while unequal fractions keep their numeric order
    /// (issue #107 correction 2).
    #[test]
    fn equal_instant_fraction_spellings_compare_equal() {
        use std::cmp::Ordering;
        let same_day = |left: &str, right: &str| {
            compare_datetime(
                &format!("2026-09-05T10:20:30{left}"),
                &format!("2026-09-05T10:20:30{right}"),
            )
            .expect("comparable")
        };
        assert_eq!(same_day(".5Z", ".50Z"), Ordering::Equal);
        assert_eq!(same_day(".50Z", ".5Z"), Ordering::Equal);
        assert_eq!(same_day(".123Z", ".1230Z"), Ordering::Equal);
        assert_eq!(same_day("Z", ".0Z"), Ordering::Equal);
        assert_eq!(same_day("Z", ".000Z"), Ordering::Equal);
        assert_eq!(same_day(".0Z", "Z"), Ordering::Equal);
        // Unequal fractions keep their chronological order at the
        // same second, in both directions.
        assert_eq!(same_day(".5Z", ".51Z"), Ordering::Less);
        assert_eq!(same_day(".51Z", ".5Z"), Ordering::Greater);
        assert_eq!(same_day("Z", ".1Z"), Ordering::Less);
        assert_eq!(same_day(".1Z", "Z"), Ordering::Greater);
        assert_eq!(same_day(".1235Z", ".123Z"), Ordering::Greater);
        assert_eq!(same_day(".123Z", ".1235Z"), Ordering::Less);
        // Cross-day and cross-second orderings stay intact next to
        // equal spellings.
        assert_eq!(
            compare_datetime("2026-09-05T10:20:30.5Z", "2026-09-06T10:20:30.4Z")
                .expect("comparable"),
            Ordering::Less
        );
        assert_eq!(
            compare_datetime("2026-09-05T10:20:31Z", "2026-09-05T10:20:30.9Z").expect("comparable"),
            Ordering::Greater
        );
    }

    #[test]
    fn id_derivation_is_seed_and_index_stable() {
        let one = derive_id(
            "seed-a",
            crate::scenario::precondition::IdAlgorithm::Sequence,
            1,
        );
        let two = derive_id(
            "seed-a",
            crate::scenario::precondition::IdAlgorithm::Sequence,
            1,
        );
        assert_eq!(one, two);
        assert_eq!(one, Value::String("seed-a-1".to_owned()));
        let left = derive_id("s", crate::scenario::precondition::IdAlgorithm::UuidV4, 1);
        let right = derive_id("s", crate::scenario::precondition::IdAlgorithm::UuidV4, 2);
        assert_ne!(left, right);
        let text = match left {
            Value::Uuid(text) => text,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(text.len(), 36);
        assert!(text.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn member_paths_walk_objects_and_lists() {
        let value = Value::List(vec![
            Value::Object(vec![(
                crate::scenario::id::FieldName::parse("inner").expect("field"),
                Value::Integer(7),
            )]),
            Value::Null,
        ]);
        let found = member_path(&value, "0.inner").expect("resolves");
        assert_eq!(found, &Value::Integer(7));
        assert!(member_path(&value, "1.inner").is_none());
        assert!(member_path(&value, "5").is_none());
    }

    #[test]
    fn epoch_clock_is_canonical() {
        assert_eq!(
            crate::reference_evaluation::version::EPOCH_CLOCK,
            "1970-01-01T00:00:00Z"
        );
    }
}
