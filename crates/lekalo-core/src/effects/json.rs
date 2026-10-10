//! Decode JSON with member uniqueness checked before any map insertion.
//! Serde supplies decoded keys, so escaped spellings share one identity.
//! The evidence envelope is a closed family input, so a smuggled
//! duplicate member is a refusal, never a silent last-wins.

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use serde_json::Value;

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
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
        let mut map_copy = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if map_copy.contains_key(&key) {
                *self.0 = true;
                return Err(serde::de::Error::custom("duplicate key"));
            }
            let value: Value = map.next_value()?;
            map_copy.insert(key, value);
        }
        Ok(Value::Object(map_copy))
    }
    fn visit_seq<M: SeqAccess<'de>>(self, mut seq: M) -> Result<Value, M::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element::<Value>()? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
}
