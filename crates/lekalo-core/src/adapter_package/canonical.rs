//! Canonical JSON serialization for the adapter package documents
//! (issue #32).
//!
//! The canonical form is the accepted workspace spelling: compact UTF-8
//! JSON, object keys in unsigned UTF-8 byte order, semantic arrays in
//! their documented sorted order, no floats, no duplicate keys, no
//! trailing whitespace, exactly one trailing LF when a file on disk
//! carries the document. Digest domains always exclude that trailing LF
//! and any self-referential digest member, mirroring the lock digest.

use serde_json::{Map, Value as Json};

/// Canonicalize one already-parsed JSON value into bytes (no trailing
/// LF). The input must carry no duplicate keys — parsing through
/// [`serde_json`] with the standard `Value` already collapses them, so
/// the document reader rejects duplicate-key bytes before canonicalizing.
pub(crate) fn canonical_bytes(value: &Json) -> Vec<u8> {
    let mut out = String::new();
    write_value(value, &mut out);
    out.into_bytes()
}

/// Canonical bytes with exactly one trailing LF (the on-disk form).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn canonical_file_bytes(value: &Json) -> Vec<u8> {
    let mut bytes = canonical_bytes(value);
    bytes.push(b'\n');
    bytes
}

/// Recursively remove one top-level member by name (the self-referential
/// digest exclusion).
pub(crate) fn without_member(value: &Json, member: &str) -> Json {
    match value {
        Json::Object(map) => {
            let mut next = Map::new();
            for (key, item) in map {
                if key == member {
                    continue;
                }
                next.insert(key.clone(), without_member(item, member));
            }
            Json::Object(next)
        }
        Json::Array(items) => Json::Array(
            items
                .iter()
                .map(|item| without_member(item, member))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn write_value(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Json::Number(number) => out.push_str(&number.to_string()),
        Json::String(text) => {
            out.push_str(&serde_json::to_string(text).expect("string serialization cannot fail"));
        }
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(
                    &serde_json::to_string(key).expect("string serialization cannot fail"),
                );
                out.push(':');
                write_value(&map[*key], out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_sort_by_unsigned_bytes() {
        let value = json!({ "b": 1, "a": { "z": 2, "A": 3 } });
        let bytes = canonical_bytes(&value);
        assert_eq!(bytes, br#"{"a":{"A":3,"z":2},"b":1}"#);
    }

    #[test]
    fn arrays_keep_their_order() {
        let value = json!({ "items": [3, 1, 2] });
        assert_eq!(canonical_bytes(&value), br#"{"items":[3,1,2]}"#);
    }

    #[test]
    fn member_removal_is_recursive_and_total() {
        let value = json!({ "manifestDigest": "sha256:00", "keep": 1, "nested": { "manifestDigest": "sha256:01", "x": 2 } });
        let stripped = without_member(&value, "manifestDigest");
        assert_eq!(
            canonical_bytes(&stripped),
            br#"{"keep":1,"nested":{"x":2}}"#
        );
    }

    #[test]
    fn file_bytes_carry_exactly_one_lf() {
        let value = json!({ "a": 1 });
        let bytes = canonical_file_bytes(&value);
        assert_eq!(bytes, b"{\"a\":1}\n");
    }
}
