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
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(n.into()))
                    .ok_or_else(|| E::custom("number"))
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
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|_| ())
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
