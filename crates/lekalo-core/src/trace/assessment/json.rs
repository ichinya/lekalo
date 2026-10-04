//! Reject duplicate decoded keys, including escaped spellings, before map
//! insertion. Serde's recursion limit bounds the document depth.

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use serde_json::Value;

pub(super) fn parse(text: &str) -> Result<Value, crate::diagnostics::DiagnosticSet> {
    let mut decoder = serde_json::Deserializer::from_str(text);
    Checked
        .deserialize(&mut decoder)
        .and_then(|value| decoder.end().map(|()| value))
        .map_err(|_| super::diagnostic::invalid("malformed-or-duplicate-json"))
}

struct Checked;

impl<'de> DeserializeSeed<'de> for Checked {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        decoder.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Checked {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a closed JSON document with unique decoded keys")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
        let mut result = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if result.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate-key"));
            }
            let value = map.next_value_seed(Checked)?;
            if value.is_null() {
                return Err(serde::de::Error::custom("null-not-permitted"));
            }
            result.insert(key, value);
        }
        Ok(Value::Object(result))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Value, S::Error> {
        let mut result = Vec::new();
        while let Some(value) = seq.next_element_seed(Checked)? {
            result.push(value);
        }
        Ok(Value::Array(result))
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Value, E> {
        Ok(value.into())
    }
}
