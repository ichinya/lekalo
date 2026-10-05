//! Deny-unknown decoder for the deliberately small schema vocabulary used here.
//! It is not a general JSON Schema implementation; Ajv remains the release oracle.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::fmt;

pub fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("unique-key JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Unique, E> {
                // All literal numbers must have been normalized exactly first.
                Err(E::custom("integer"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Unique, A::Error> {
                let mut out = vec![];
                while let Some(Unique(v)) = seq.next_element()? {
                    out.push(v);
                }
                Ok(Unique(out.into()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(de::Error::custom("duplicate"));
                    }
                    let Unique(value) = map.next_value()?;
                    out.insert(key, value);
                }
                Ok(Unique(out.into()))
            }
        }
        d.deserialize_any(JsonVisitor)
    }
}
pub fn decode(bytes: &[u8]) -> Result<Value, ()> {
    serde_json::from_slice::<Unique>(&normalize_integers(bytes)?)
        .map(|v| v.0)
        .map_err(|_| ())
}
// These five families only admit bounded integer numbers. Normalize their raw
// decimal/exponent spellings before serde can round a fraction to an integer.
// Strings are copied verbatim; serde still owns structure and duplicate keys.
fn normalize_integers(bytes: &[u8]) -> Result<Vec<u8>, ()> {
    if bytes.len() > super::MAX_BYTES {
        return Err(());
    }
    let mut out = Vec::with_capacity(bytes.len());
    let (mut quoted, mut escaped, mut i) = (false, false, 0);
    while i < bytes.len() {
        let b = bytes[i];
        if quoted {
            out.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
            i += 1;
        } else if b == b'"' {
            quoted = true;
            out.push(b);
            i += 1;
        } else if b == b'-' || b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || b".eE+-".contains(&bytes[i])) {
                i += 1;
            }
            out.extend_from_slice(integer_literal(&bytes[start..i])?.to_string().as_bytes());
        } else {
            out.push(b);
            i += 1;
        }
        if out.len() > super::MAX_BYTES {
            return Err(());
        }
    }
    Ok(out)
}
fn integer_literal(raw: &[u8]) -> Result<i64, ()> {
    const MAXIMUM: u64 = 9_007_199_254_740_991;
    let negative = raw.first() == Some(&b'-');
    let raw = if negative { &raw[1..] } else { raw };
    let (mantissa, exponent) = match raw.iter().position(|b| b"eE".contains(b)) {
        Some(i) => {
            let exp = &raw[i + 1..];
            let minus = exp.first() == Some(&b'-');
            let digits = if matches!(exp.first(), Some(b'-' | b'+')) {
                &exp[1..]
            } else {
                exp
            };
            if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
                return Err(());
            }
            // Saturation is safe: a nonzero bounded input cannot compensate an
            // exponent this large. Zero is handled after grammar validation.
            let exp = digits.iter().fold(0i64, |n, b| {
                n.saturating_mul(10).saturating_add((b - b'0') as i64)
            });
            (&raw[..i], if minus { -exp } else { exp })
        }
        None => (raw, 0),
    };
    let (integer, fraction) = match mantissa.iter().position(|b| *b == b'.') {
        Some(i) => {
            let fraction = &mantissa[i + 1..];
            if fraction.is_empty() || !fraction.iter().all(u8::is_ascii_digit) {
                return Err(());
            }
            (&mantissa[..i], fraction)
        }
        None => (mantissa, &[][..]),
    };
    if integer.is_empty()
        || !integer.iter().all(u8::is_ascii_digit)
        || (integer.len() > 1 && integer[0] == b'0')
    {
        return Err(());
    }
    let mut digits = integer.to_vec();
    digits.extend_from_slice(fraction);
    let Some(first) = digits.iter().position(|b| *b != b'0') else {
        return Ok(0);
    };
    let last = digits.iter().rposition(|b| *b != b'0').unwrap();
    let shift = exponent
        .saturating_sub(fraction.len() as i64)
        .saturating_add((digits.len() - last - 1) as i64);
    if !(0..=16).contains(&shift) || (last - first + 1) as i64 + shift > 16 {
        return Err(());
    }
    let mut magnitude = digits[first..=last]
        .iter()
        .fold(0u64, |n, b| n * 10 + (b - b'0') as u64);
    for _ in 0..shift {
        magnitude *= 10;
    }
    if magnitude > MAXIMUM {
        return Err(());
    }
    Ok(if negative {
        -(magnitude as i64)
    } else {
        magnitude as i64
    })
}
fn schema(family: &str) -> Result<Value, ()> {
    let bytes = match family {
        "baseline" => {
            include_str!("../../../../contracts/framework-lift-baseline.schema.v0.6.4.json")
        }
        "task" => include_str!("../../../../contracts/framework-lift-task.schema.v0.6.4.json"),
        "campaign" => {
            include_str!("../../../../contracts/framework-lift-campaign.schema.v0.6.4.json")
        }
        "arm" => include_str!("../../../../contracts/framework-lift-arm.schema.v0.6.4.json"),
        "result" => include_str!("../../../../contracts/framework-lift-result.schema.v0.6.4.json"),
        _ => return Err(()),
    };
    serde_json::from_str(bytes).map_err(|_| ())
}
pub fn unknown_metrics() -> Value {
    let s = schema("arm").expect("embedded schema");
    s["$defs"]["metrics"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| (k.clone(), serde_json::json!({"state":"unknown"})))
        .collect::<serde_json::Map<String, Value>>()
        .into()
}
pub fn validate(family: &str, value: &Value) -> Result<(), ()> {
    let root = schema(family)?;
    check(&root, &root, value, 0)
}
fn check(root: &Value, s: &Value, v: &Value, depth: usize) -> Result<(), ()> {
    if depth > 64 {
        return Err(());
    }
    if let Some(r) = s["$ref"].as_str() {
        return check(
            root,
            &root["$defs"][r.strip_prefix("#/$defs/").ok_or(())?],
            v,
            depth + 1,
        );
    }
    if let Some(choices) = s["oneOf"].as_array() {
        return if choices
            .iter()
            .filter(|c| check(root, c, v, depth + 1).is_ok())
            .count()
            == 1
        {
            Ok(())
        } else {
            Err(())
        };
    }
    if let Some(c) = s.get("const") {
        if c != v {
            return Err(());
        }
    }
    if let Some(e) = s["enum"].as_array() {
        if !e.contains(v) {
            return Err(());
        }
    }
    match s["type"].as_str() {
        Some("object") => {
            let o = v.as_object().ok_or(())?;
            let props = s["properties"].as_object().ok_or(())?;
            if o.len() != props.len() {
                return Err(());
            }
            for (k, rule) in props {
                check(root, rule, o.get(k).ok_or(())?, depth + 1)?;
            }
        }
        Some("array") => {
            let a = v.as_array().ok_or(())?;
            if a.len() < s["minItems"].as_u64().unwrap_or(0) as usize
                || a.len() > s["maxItems"].as_u64().unwrap_or(0) as usize
            {
                return Err(());
            }
            for (i, x) in a.iter().enumerate() {
                if a[..i].contains(x) {
                    return Err(());
                }
                check(root, &s["items"], x, depth + 1)?;
            }
        }
        Some("string") => {
            let x = v.as_str().ok_or(())?;
            if x.len() < s["minLength"].as_u64().unwrap_or(0) as usize
                || x.len() > s["maxLength"].as_u64().unwrap_or(usize::MAX as u64) as usize
            {
                return Err(());
            }
            if let Some(p) = s["pattern"].as_str() {
                let valid = match p {
                    "^[a-z][a-z0-9-]*$" => token(x),
                    "^sha256:[0-9a-f]{64}$" => x.strip_prefix("sha256:").is_some_and(|x| {
                        x.len() == 64
                            && x.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    }),
                    "^[0-9a-f]{40}$" => {
                        x.len() == 40
                            && x.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    }
                    "^[A-Z]{3}$" => x.len() == 3 && x.bytes().all(|b| b.is_ascii_uppercase()),
                    "^[a-zA-Z0-9][a-zA-Z0-9._/-]*$" => {
                        !x.is_empty()
                            && (x.as_bytes()[0].is_ascii_alphabetic()
                                || x.as_bytes()[0].is_ascii_digit())
                            && x.bytes().all(|b| {
                                b.is_ascii_alphabetic()
                                    || b.is_ascii_digit()
                                    || b"._/-".contains(&b)
                            })
                    }
                    _ => false,
                };
                if !valid {
                    return Err(());
                }
            }
        }
        Some("integer") => {
            let x = v.as_i64().ok_or(())?;
            if x < s["minimum"].as_i64().ok_or(())? || x > s["maximum"].as_i64().ok_or(())? {
                return Err(());
            }
        }
        Some("number") => {
            let x = v.as_f64().ok_or(())?;
            if !x.is_finite()
                || x < s["minimum"].as_f64().ok_or(())?
                || x > s["maximum"].as_f64().ok_or(())?
            {
                return Err(());
            }
        }
        Some("boolean") => {
            if !v.is_boolean() {
                return Err(());
            }
        }
        None => {
            if s.get("const").is_none() && s.get("enum").is_none() {
                return Err(());
            }
        }
        _ => return Err(()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn signed_integral_lexemes_round_trip_through_the_exact_decoder() {
        for n in -12_800..=12_800i64 {
            for literal in [
                format!("{n}.0"),
                format!("{n}e0"),
                format!("{}e-2", n * 100),
            ] {
                assert_eq!(decode(literal.as_bytes()), Ok(json!(n)), "{literal}");
            }
        }
    }

    #[test]
    fn safe_integer_endpoints_and_zero_canonicalize_without_float_rounding() {
        for literal in [
            "9007199254740991.0",
            "9007199254740991e0",
            "90071992547409910e-1",
            "9007199254740991000e-3",
        ] {
            assert_eq!(
                decode(literal.as_bytes()),
                Ok(json!(9_007_199_254_740_991u64))
            );
        }
        for literal in [
            "-0.0",
            "0e999999999999999999999999",
            "0e-999999999999999999999999",
        ] {
            assert_eq!(decode(literal.as_bytes()), Ok(json!(0)));
        }
    }

    #[test]
    fn fractions_nonfinite_oversized_and_malformed_literals_are_never_repaired() {
        for literal in [
            "1.5",
            "1.000000000000000000001",
            "9007199254740991.1",
            "1e-999",
            "1e309",
            "9007199254740992.0",
            "-9007199254740992e0",
            "01.0",
            "1.",
            "1e",
            "1e+",
            "NaN",
            "Infinity",
            "1+2",
            "--1",
            "+1",
        ] {
            assert!(decode(literal.as_bytes()).is_err(), "{literal}");
        }
    }

    #[test]
    fn normalization_preserves_strings_and_duplicate_key_and_structure_refusals() {
        let encoded = br#"{"s":"1.0 -2e4 \"9007199254740992\" \\1.5","n":1.0}"#;
        assert_eq!(
            decode(encoded),
            Ok(json!({"s":"1.0 -2e4 \"9007199254740992\" \\1.5","n":1}))
        );
        for encoded in [
            &br#"{"n":1.0,"\u006e":1e0}"#[..],
            &br#"{"n":1e0}[]"#[..],
            &br#"{"n":1e0,}"#[..],
            &br#"{"s":"unterminated 1.0}"#[..],
        ] {
            assert!(decode(encoded).is_err());
        }
    }
}
