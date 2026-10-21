//! Bounded, typed default literals owned by Model/IR 0.8.0.
use crate::loader::frontends::{Node, Scalar, Value};
use serde_json::Value as Json;

/// A JSON literal with the same closed limits as the Change request owner:
/// four levels, 64 array items, 16 object members, 64-byte keys, 8192-byte
/// strings and signed 64-bit integers. Null is distinct from no default.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultValue(Json);

impl DefaultValue {
    /// Validate a literal before admitting it into the typed IR.
    pub fn from_json(value: Json) -> Option<Self> {
        fn valid(value: &Json, depth: usize) -> bool {
            depth <= 4
                && match value {
                    Json::Null | Json::Bool(_) => true,
                    Json::Number(n) => n.as_i64().is_some(),
                    Json::String(s) => s.len() <= 8192,
                    Json::Array(a) => a.len() <= 64 && a.iter().all(|v| valid(v, depth + 1)),
                    Json::Object(o) => {
                        o.len() <= 16 && o.iter().all(|(k, v)| k.len() <= 64 && valid(v, depth + 1))
                    }
                }
        }
        valid(&value, 1).then_some(Self(value))
    }

    /// The admitted literal, without losing the null/absence distinction.
    pub fn as_json(&self) -> &Json {
        &self.0
    }

    /// Compact JSON with recursively byte-sorted object keys.
    pub fn canonical_json(&self) -> String {
        fn canonical(value: &Json) -> String {
            match value {
                Json::Array(items) => format!(
                    "[{}]",
                    items.iter().map(canonical).collect::<Vec<_>>().join(",")
                ),
                Json::Object(entries) => {
                    let mut entries: Vec<_> = entries.iter().collect();
                    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                    format!(
                        "{{{}}}",
                        entries
                            .into_iter()
                            .map(|(k, v)| format!(
                                "{}:{}",
                                serde_json::to_string(k).expect("string"),
                                canonical(v)
                            ))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
                _ => serde_json::to_string(value).expect("JSON literal"),
            }
        }
        canonical(&self.0)
    }

    pub(crate) fn from_node(node: &Node) -> Option<Self> {
        fn convert(node: &Node, depth: usize) -> Option<Json> {
            if depth > 4 {
                return None;
            }
            Some(match &node.value {
                Value::Scalar(Scalar::Null) => Json::Null,
                Value::Scalar(Scalar::Bool(b)) => Json::Bool(*b),
                Value::Scalar(Scalar::Int(i)) => Json::from(i64::try_from(*i).ok()?),
                Value::Scalar(Scalar::Str(s)) if s.len() <= 8192 => Json::String(s.clone()),
                Value::Seq(items) if items.len() <= 64 => Json::Array(
                    items
                        .iter()
                        .map(|n| convert(n, depth + 1))
                        .collect::<Option<_>>()?,
                ),
                Value::Map(entries) if entries.len() <= 16 => {
                    let mut map = serde_json::Map::new();
                    for entry in entries {
                        if entry.key.len() > 64 || map.contains_key(&entry.key) {
                            return None;
                        }
                        map.insert(entry.key.clone(), convert(&entry.value, depth + 1)?);
                    }
                    Json::Object(map)
                }
                _ => return None,
            })
        }
        Self::from_json(convert(node, 1)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_literals_preserve_null_and_canonicalize_recursive_objects() {
        let value = DefaultValue::from_json(json!({"z": [null, true], "a": {"b": 1}})).unwrap();
        assert_eq!(
            value.canonical_json(),
            "{\"a\":{\"b\":1},\"z\":[null,true]}"
        );
        assert_eq!(
            DefaultValue::from_json(Json::Null).unwrap().as_json(),
            &Json::Null
        );
    }

    #[test]
    fn default_literal_bounds_are_closed_before_admission() {
        for value in [json!(1.5), json!(u64::MAX), json!([[[[1]]]]), json!([])] {
            // Empty collections are valid JSON literals; four nested arrays
            // plus their scalar are beyond the four-level literal owner.
            if value == json!([]) {
                assert!(DefaultValue::from_json(value).is_some());
            } else {
                assert!(DefaultValue::from_json(value).is_none());
            }
        }
        assert!(DefaultValue::from_json(json!(vec![0; 65])).is_none());
        assert!(DefaultValue::from_json(json!("x".repeat(8193))).is_none());
        assert!(DefaultValue::from_json(json!({("x".repeat(65)): true})).is_none());
        assert!(DefaultValue::from_json(json!([[[[]]]])).is_some());
    }
}
