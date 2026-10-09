//! Canonical bytes and digest of the composition contract (issue #51).
//!
//! Attachment canonical bytes are compact UTF-8 JSON with byte-sorted
//! object keys, canonical collections, and no trailing LF. The same
//! rule is proven independently by the Node contract gate.

use serde::Serialize;

/// The bare 64 lowercase hex SHA-256 of some bytes.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    crate::digest::sha256_hex(bytes)
}

/// The exact digest prefix of the composition identity.
pub const DIGEST_PREFIX: &str = "sha256:";

/// Serialize any value into compact JSON with byte-sorted object keys
/// (the round trip through `serde_json::Value`'s ordered map), which
/// is the attachment canonical form.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn canonical_value_bytes<T: Serialize>(value: &T) -> String {
    let dynamic = serde_json::to_value(value).expect("composition wire values serialize");
    serde_json::to_string(&dynamic).expect("canonical JSON bytes fit in memory")
}

use crate::diagnostics::DiagnosticSet;

/// The canonical digest over exact canonical UTF-8 bytes with the
/// documented domain prefix.
#[cfg_attr(not(test), allow(dead_code))]
const DOMAIN_PREFIX: &str = "lekalo/worklog-changelog-pilot";

/// The module-external unique-member JSON decode: the same closed
/// duplicate-key-refusing decoder `parse` uses, exposed to the module
/// root for the consumed-attachment seam.
pub(crate) fn canonical_parse_unique(
    text: &str,
) -> Result<serde_json::Value, crate::diagnostics::DiagnosticSet> {
    parse_unique(text)
}

/// The canonical digest of one composition document: the domain
/// prefix, one slash, then the SHA-256 of the exact canonical bytes.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn document_digest(canonical_bytes: &str) -> String {
    let mut bytes = Vec::with_capacity(DOMAIN_PREFIX.len() + 1 + canonical_bytes.len());
    bytes.extend_from_slice(DOMAIN_PREFIX.as_bytes());
    bytes.push(b'/');
    bytes.extend_from_slice(canonical_bytes.as_bytes());
    format!("{DIGEST_PREFIX}{}", sha256_hex(&bytes))
}

/// Decode JSON with member uniqueness checked before any map
/// insertion; escaped spellings share one identity.
pub(super) fn parse_unique(text: &str) -> Result<serde_json::Value, DiagnosticSet> {
    let mut duplicate = false;
    let mut decoder = serde_json::Deserializer::from_str(text);
    let result = Checked(&mut duplicate)
        .deserialize(&mut decoder)
        .and_then(|value| decoder.end().map(|()| value));
    result.map_err(|_| {
        super::diagnostic::input_invalid(if duplicate {
            "duplicate-key"
        } else {
            "malformed-json"
        })
    })
}

use serde::de::DeserializeSeed;

struct Checked<'a>(&'a mut bool);

impl<'de> serde::de::DeserializeSeed<'de> for Checked<'_> {
    type Value = serde_json::Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_any(self)
    }
}

impl<'de> serde::de::Visitor<'de> for Checked<'_> {
    type Value = serde_json::Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value with unique decoded members")
    }
    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut result = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if result.contains_key(&key) {
                *self.0 = true;
                return Err(serde::de::Error::custom("duplicate-key"));
            }
            let value = map.next_value_seed(Checked(self.0))?;
            result.insert(key, value);
        }
        Ok(serde_json::Value::Object(result))
    }
    fn visit_seq<S: serde::de::SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
        let mut result = Vec::new();
        while let Some(value) = seq.next_element_seed(Checked(self.0))? {
            result.push(value);
        }
        Ok(serde_json::Value::Array(result))
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Ok(serde_json::Value::from(value))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_some<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        decoder.deserialize_any(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_exact_sha256_with_the_domain_prefix() {
        let digest = document_digest("{}");
        let bare = &digest[DIGEST_PREFIX.len()..];
        assert_eq!(bare.len(), 64);
        assert!(digest.starts_with("sha256:"));
        // Deterministic across calls.
        assert_eq!(digest, document_digest("{}"));
    }

    #[test]
    fn duplicate_keys_are_rejected_before_map_insertion() {
        let error = parse_unique(r#"{"a":1,"a":2}"#).unwrap_err();
        let rendered = serde_json::to_string(&error.as_slice()[0]).unwrap();
        assert!(rendered.contains("duplicate-key"), "{rendered}");
    }

    #[test]
    fn keys_are_byte_sorted_and_compact() {
        #[derive(Serialize)]
        struct Probe {
            zeta: u32,
            alpha: &'static str,
        }
        assert_eq!(
            canonical_value_bytes(&Probe {
                zeta: 1,
                alpha: "a"
            }),
            r#"{"alpha":"a","zeta":1}"#
        );
    }
}
