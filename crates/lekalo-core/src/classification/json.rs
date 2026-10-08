//! Decode JSON with member uniqueness checked before any map insertion.
//! Serde supplies decoded keys, so escaped spellings share one identity.
//! The transport crate (MCP stdio, issue #33) reuses the same
//! duplicate-refusing deserializer for every inbound frame and every
//! backend-output parse; a duplicate member is refused, never last-key-wins.

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use serde_json::Value;

pub(super) fn parse(text: &str) -> Result<Value, crate::diagnostics::DiagnosticSet> {
    parse_checked(text).map_err(|failure| match failure {
        ParseFailure::Duplicate => super::diagnostic::document_invalid("duplicate-key", None),
        ParseFailure::Malformed => super::diagnostic::document_invalid("malformed-json", None),
    })
}

/// Why one JSON document failed the checked decode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseFailure {
    /// Two members of one object decoded to the same key.
    Duplicate,
    /// The document is not valid JSON (or not valid UTF-8 text).
    Malformed,
}

/// The checked decode over `&str` bytes: `Ok(())` only when the document
/// carries a unique JSON value.
pub fn parse_checked(text: &str) -> Result<Value, ParseFailure> {
    let mut duplicate = false;
    let mut decoder = serde_json::Deserializer::from_str(text);
    let result = Checked(&mut duplicate)
        .deserialize(&mut decoder)
        .and_then(|value| decoder.end().map(|()| value));
    result.map_err(|_| {
        if duplicate {
            ParseFailure::Duplicate
        } else {
            ParseFailure::Malformed
        }
    })
}

struct Checked<'a>(&'a mut bool);

impl<'de> DeserializeSeed<'de> for Checked<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        decoder.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Checked<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value with unique decoded members")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
        let mut result = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if result.contains_key(&key) {
                *self.0 = true;
                return Err(serde::de::Error::custom("duplicate-key"));
            }
            let value = map.next_value_seed(Checked(self.0))?;
            result.insert(key, value);
        }
        Ok(Value::Object(result))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Value, S::Error> {
        let mut result = Vec::new();
        while let Some(value) = seq.next_element_seed(Checked(self.0))? {
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
    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
}
