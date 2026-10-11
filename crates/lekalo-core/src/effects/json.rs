//! Decode JSON with member uniqueness checked before any map insertion.
//! Serde supplies decoded keys, so escaped spellings share one identity.
//! The evidence envelope is a closed family input, so a smuggled
//! duplicate member is a refusal, never a silent last-wins — at EVERY
//! nesting depth: the same duplicate-aware seed decodes nested map and
//! sequence values, so a duplicate buried inside one entry cannot
//! bypass the check the top-level map gets.

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use serde_json::{Number, Value};

pub(super) fn parse(bytes: &[u8]) -> Result<Value, crate::diagnostics::DiagnosticSet> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| super::diagnostic::input_invalid_detail("envelope-json"))?;
    let mut duplicate = false;
    let mut decoder = serde_json::Deserializer::from_str(text);
    let result = Checked(&mut duplicate)
        .deserialize(&mut decoder)
        .and_then(|value| decoder.end().map(|()| value));
    result.map_err(|_| {
        super::diagnostic::input_invalid_detail(if duplicate {
            "envelope-duplicate-key"
        } else {
            "envelope-json"
        })
    })
}

struct Checked<'a>(&'a mut bool);

impl<'de> DeserializeSeed<'de> for Checked<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        decoder.deserialize_any(self)
    }
}

impl<'de, 'a> Visitor<'de> for Checked<'a> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value with unique decoded members")
    }
    // Scalars pass through exactly like serde_json's own Value visitor;
    // only maps and sequences recurse through the duplicate-aware seed.
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<Value, E> {
        match serde_json::Number::from_i128(value) {
            Some(number) => Ok(Value::Number(number)),
            None => Err(serde::de::Error::custom("number out of range")),
        }
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<Value, E> {
        match serde_json::Number::from_u128(value) {
            Some(number) => Ok(Value::Number(number)),
            None => Err(serde::de::Error::custom("number out of range")),
        }
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Value, E> {
        Ok(Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
        let mut map_copy = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if map_copy.contains_key(&key) {
                *self.0 = true;
                return Err(serde::de::Error::custom("duplicate key"));
            }
            // Nested values decode through the same duplicate-aware
            // seed: only the top-level map was checked before, so a
            // nested duplicate key was silently last-wins.
            let value: Value = map.next_value_seed(Checked(self.0))?;
            map_copy.insert(key, value);
        }
        Ok(Value::Object(map_copy))
    }
    fn visit_seq<M: SeqAccess<'de>>(self, mut seq: M) -> Result<Value, M::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Checked(self.0))? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
}
