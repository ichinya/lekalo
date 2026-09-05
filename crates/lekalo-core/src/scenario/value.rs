//! Typed values of the Scenario IR (issue #23).
//!
//! A [`TypedValue`] is a closed recursive value with an explicit type
//! tag: null, boolean, signed 64-bit integer, bounded UTF-8 string,
//! canonical decimal-as-string, date, canonical UTC datetime, canonical
//! lowercase UUID, bounded absolute URI, list, and field-name-keyed
//! object. Floats, non-finite numbers, arbitrary JSON, unbounded maps,
//! implicit coercion, and locale- or timezone-dependent spellings do not
//! exist on the wire. Depth and fan-out are bounded
//! ([`super::version`]); every bound rejects with a typed diagnostic and
//! no partial value.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::id::FieldName;
use super::version;

/// One closed typed value. The exact canonical text of every literal is
/// preserved; spellings that are not already canonical are rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedValue {
    /// The single null value.
    Null,
    /// True or false.
    Boolean(bool),
    /// A signed 64-bit integer.
    Integer(i64),
    /// A bounded UTF-8 string of at least one code point.
    String(String),
    /// A canonical decimal: `-?(0|[1-9][0-9]*)(\.[0-9]*[1-9])?` — no
    /// exponent, no leading zero, no trailing zero fraction, no `-0`.
    Decimal(String),
    /// A calendar date `YYYY-MM-DD` with real month and day values.
    Date(String),
    /// A canonical UTC datetime `YYYY-MM-DDTHH:MM:SS[.fraction]Z`; no
    /// offsets, no leap seconds, no locale or host timezone.
    Datetime(String),
    /// A canonical lowercase hyphenated UUID.
    Uuid(String),
    /// A bounded absolute URI with no whitespace, credentials, or
    /// user information.
    Uri(String),
    /// An ordered list of typed values.
    List(Vec<TypedValue>),
    /// A map keyed by field names; canonical form sorts the keys by
    /// unsigned UTF-8 bytes.
    Object(Vec<(FieldName, TypedValue)>),
}

impl TypedValue {
    /// Normalize one JSON value into a typed value, or return the typed
    /// rejection set. `role` locates the value in diagnostics.
    pub(crate) fn from_json(
        json: &Json,
        role: &str,
        depth: usize,
    ) -> Result<TypedValue, DiagnosticSet> {
        if depth > version::MAX_TYPED_DEPTH {
            return Err(diagnostic::limit_set("typed-value-depth"));
        }
        let object = match json {
            Json::Object(map) if map.len() == 2 => map,
            _ => return Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
        };
        let kind = match object.get("type") {
            Some(Json::String(kind)) => kind.as_str(),
            _ => return Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
        };
        let Some(value) = object.get("value") else {
            return Err(diagnostic::input_invalid("typed-value-shape", Some(role)));
        };
        match kind {
            "null" => {
                if value.is_null() {
                    Ok(TypedValue::Null)
                } else {
                    Err(diagnostic::input_invalid("typed-value-shape", Some(role)))
                }
            }
            "boolean" => match value {
                Json::Bool(flag) => Ok(TypedValue::Boolean(*flag)),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "integer" => match value.as_i64() {
                Some(number) if value.is_i64() => Ok(TypedValue::Integer(number)),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "string" => match value.as_str() {
                Some(text) if string_within_bound(text) => Ok(TypedValue::String(text.to_owned())),
                _ => Err(diagnostic::limit_set("scalar-codepoints")),
            },
            "decimal" => match value.as_str() {
                Some(text) if canonical_decimal(text) => Ok(TypedValue::Decimal(text.to_owned())),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "date" => match value.as_str() {
                Some(text) if canonical_date(text) => Ok(TypedValue::Date(text.to_owned())),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "datetime" => match value.as_str() {
                Some(text) if canonical_datetime(text) => Ok(TypedValue::Datetime(text.to_owned())),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "uuid" => match value.as_str() {
                Some(text) if canonical_uuid(text) => Ok(TypedValue::Uuid(text.to_owned())),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "uri" => match value.as_str() {
                Some(text) if canonical_uri(text) => Ok(TypedValue::Uri(text.to_owned())),
                _ => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "list" => match value.as_array() {
                Some(items) => {
                    if items.len() > version::MAX_TYPED_ITEMS {
                        return Err(diagnostic::limit_set("typed-items"));
                    }
                    let mut values = Vec::with_capacity(items.len());
                    for (index, item) in items.iter().enumerate() {
                        let item_role = format!("{role}[{index}]");
                        values.push(TypedValue::from_json(item, &item_role, depth + 1)?);
                    }
                    Ok(TypedValue::List(values))
                }
                None => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            "object" => match value.as_object() {
                Some(map) => {
                    if map.len() > version::MAX_TYPED_ITEMS {
                        return Err(diagnostic::limit_set("typed-items"));
                    }
                    let mut entries = Vec::with_capacity(map.len());
                    for (key, item) in map {
                        let Ok(name) = FieldName::parse(key) else {
                            return Err(diagnostic::input_invalid("field-name", Some(role)));
                        };
                        let entry_role = format!("{role}.{key}");
                        entries.push((name, TypedValue::from_json(item, &entry_role, depth + 1)?));
                    }
                    entries.sort_by(|left, right| left.0.cmp(&right.0));
                    Ok(TypedValue::Object(entries))
                }
                None => Err(diagnostic::input_invalid("typed-value-shape", Some(role))),
            },
            _ => Err(diagnostic::input_invalid("typed-value-kind", Some(role))),
        }
    }

    /// The number of typed values reachable from this one, inclusive.
    pub(crate) fn count(&self) -> usize {
        match self {
            TypedValue::List(items) => items.iter().map(TypedValue::count).sum::<usize>() + 1,
            TypedValue::Object(entries) => {
                entries
                    .iter()
                    .map(|(_, value)| value.count())
                    .sum::<usize>()
                    + 1
            }
            _ => 1,
        }
    }
}

/// The scalar string bound: at least one and at most the closed code
/// point budget, with no control characters.
pub(crate) fn string_within_bound(text: &str) -> bool {
    let characters = text.chars().count();
    (1..=version::MAX_SCALAR_CODEPOINTS).contains(&characters)
        && !text.chars().any(char::is_control)
}

/// The canonical decimal spelling: no exponent, no leading zero, no
/// trailing zero fraction, no negative zero.
pub(crate) fn canonical_decimal(text: &str) -> bool {
    if text == "-0" {
        return false;
    }
    let digits = |segment: &str| !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit());
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (integral, fractional) = match rest.split_once('.') {
        Some((integral, fractional)) => (integral, Some(fractional)),
        None => (rest, None),
    };
    if !digits(integral) {
        return false;
    }
    if integral.len() > 1 && integral.starts_with('0') {
        return false;
    }
    if integral == "0" && negative {
        return false;
    }
    match fractional {
        Some(fractional) => digits(fractional) && !fractional.ends_with('0'),
        None => true,
    }
}

/// The canonical calendar date with real month and day values.
pub(crate) fn canonical_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !text[..4].bytes().all(|b| b.is_ascii_digit())
        || !text[5..7].bytes().all(|b| b.is_ascii_digit())
        || !text[8..10].bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        text[0..4].parse::<u32>(),
        text[5..7].parse::<u32>(),
        text[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return false;
    }
    day >= 1 && day <= days_in_month(year, month)
}

/// Real Gregorian month lengths, leap years included.
pub(crate) fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// The canonical UTC datetime: real date and civil time, `Z` suffix
/// only, zero to nine fractional digits, no offsets, no leap seconds.
pub(crate) fn canonical_datetime(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !bytes.ends_with(b"Z") {
        return false;
    }
    if !canonical_date(&text[..10]) {
        return false;
    }
    if bytes[10] != b'T' {
        return false;
    }
    let time = &text[11..text.len() - 1];
    let (clock, fraction) = match time.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (time, None),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() != 3 {
        return false;
    }
    for part in parts.iter() {
        if part.len() != 2 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    let (Ok(hour), Ok(minute), Ok(second)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    match fraction {
        Some(fraction) => {
            (1..=9).contains(&fraction.len()) && fraction.bytes().all(|b| b.is_ascii_digit())
        }
        None => true,
    }
}

/// The canonical lowercase hyphenated UUID.
pub(crate) fn canonical_uuid(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase() {
                    return false;
                }
            }
        }
    }
    true
}

/// The bounded absolute URI: scheme plus hierarchical part, no
/// whitespace or reserved delimiters, no user information.
pub(crate) fn canonical_uri(text: &str) -> bool {
    let characters = text.chars().count();
    if !(8..=2048).contains(&characters) {
        return false;
    }
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    let scheme_bytes = scheme.as_bytes();
    if scheme_bytes.is_empty()
        || !scheme_bytes[0].is_ascii_lowercase()
        || !scheme_bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'.' | b'-')
        })
    {
        return false;
    }
    if rest.is_empty() {
        return false;
    }
    if text.chars().any(|c| {
        c.is_control()
            || matches!(
                c,
                '<' | '>' | '"' | '{' | '}' | '|' | '\\' | '^' | '`' | ' '
            )
    }) {
        return false;
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    !authority.contains('@')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(json: Json) -> TypedValue {
        TypedValue::from_json(&json, "v", 0).expect("valid typed value")
    }

    fn err(json: Json) {
        assert!(
            TypedValue::from_json(&json, "v", 0).is_err(),
            "must reject {json}"
        );
    }

    #[test]
    fn scalars_round_trip_through_their_canonical_forms() {
        assert_eq!(ok(json!({"type": "null", "value": null})), TypedValue::Null);
        assert_eq!(
            ok(json!({"type": "boolean", "value": true})),
            TypedValue::Boolean(true)
        );
        assert_eq!(
            ok(json!({"type": "integer", "value": -5})),
            TypedValue::Integer(-5)
        );
        assert_eq!(
            ok(json!({"type": "string", "value": "привет"})),
            TypedValue::String("привет".to_owned())
        );
        assert_eq!(
            ok(json!({"type": "decimal", "value": "-12.05"})),
            TypedValue::Decimal("-12.05".to_owned())
        );
        assert_eq!(
            ok(json!({"type": "date", "value": "2026-09-05"})),
            TypedValue::Date("2026-09-05".to_owned())
        );
        assert_eq!(
            ok(json!({"type": "datetime", "value": "2026-09-05T10:20:30.5Z"})),
            TypedValue::Datetime("2026-09-05T10:20:30.5Z".to_owned())
        );
        assert_eq!(
            ok(json!({"type": "uuid", "value": "0de2b4f1-1a2b-4c3d-8e5f-6a7b8c9d0e1f"})),
            TypedValue::Uuid("0de2b4f1-1a2b-4c3d-8e5f-6a7b8c9d0e1f".to_owned())
        );
        assert_eq!(
            ok(json!({"type": "uri", "value": "https://dev.lekalo/scenario"})),
            TypedValue::Uri("https://dev.lekalo/scenario".to_owned())
        );
    }

    #[test]
    fn noncanonical_or_wrong_literals_are_rejected() {
        err(json!({"type": "decimal", "value": "1.50"}));
        err(json!({"type": "decimal", "value": "-0"}));
        err(json!({"type": "decimal", "value": "1e5"}));
        err(json!({"type": "decimal", "value": "01"}));
        err(json!({"type": "date", "value": "2026-02-30"}));
        err(json!({"type": "date", "value": "2026-13-01"}));
        err(json!({"type": "datetime", "value": "2026-09-05T10:20:30+03:00"}));
        err(json!({"type": "datetime", "value": "2026-09-05T24:00:00Z"}));
        err(json!({"type": "datetime", "value": "2026-09-05T10:20:60Z"}));
        err(json!({"type": "uuid", "value": "0DE2B4F1-1A2B-4C3D-8E5F-6A7B8C9D0E1F"}));
        err(json!({"type": "uri", "value": "https://user:pass@dev.lekalo/"}));
        err(json!({"type": "uri", "value": "not-a-uri"}));
        err(json!({"type": "string", "value": ""}));
        err(json!({"type": "float", "value": 1.5}));
        err(json!(1.5));
        err(json!({"type": "integer", "value": 1.0}));
    }

    #[test]
    fn lists_and_objects_bound_their_fanout_and_sort_object_keys() {
        let list = ok(json!({"type": "list", "value": [
            {"type": "integer", "value": 1},
            {"type": "list", "value": []}
        ]}));
        assert_eq!(list.count(), 3);
        let object = ok(json!({"type": "object", "value": {
            "b_field": {"type": "integer", "value": 2},
            "a_field": {"type": "integer", "value": 1}
        }}));
        match object {
            TypedValue::Object(entries) => {
                assert_eq!(entries[0].0.as_str(), "a_field");
                assert_eq!(entries[1].0.as_str(), "b_field");
            }
            _ => panic!("object expected"),
        }
    }

    #[test]
    fn depth_beyond_the_contract_limit_is_refused() {
        let mut value = json!({"type": "integer", "value": 1});
        for _ in 0..=version::MAX_TYPED_DEPTH {
            value = json!({"type": "list", "value": [value]});
        }
        assert!(TypedValue::from_json(&value, "v", 0).is_err());
    }
}
