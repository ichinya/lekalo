//! Canonical JSON encoding of the #120 family (the custody identity
//! projection).
//!
//! Canonical bytes are compact UTF-8 JSON with object keys sorted by
//! Unicode code points, array order preserved, and JSON primitive
//! encoding. This is the exact encoding the reference checker and the
//! subject-profile projection use; it is deterministic and stable.

use serde_json::Value as Json;

/// Compare two strings by Unicode code points (the declared
/// `unicode-code-point-lexicographic` strategy).
pub fn compare_unicode_code_points(left: &str, right: &str) -> std::cmp::Ordering {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    for index in 0..left.len().min(right.len()) {
        let delta = (u32::from(left[index])).cmp(&u32::from(right[index]));
        if delta != std::cmp::Ordering::Equal {
            return delta;
        }
    }
    left.len().cmp(&right.len())
}

/// Encode one JSON value into its canonical bytes: compact, object
/// keys sorted by code points, arrays in order.
pub fn canonical(value: &Json) -> String {
    match value {
        Json::Array(items) => {
            let joined: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", joined.join(","))
        }
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| compare_unicode_code_points(left, right));
            let joined: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", joined.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_are_code_point_sorted_and_arrays_preserved() {
        let value = json!({"b": 1, "a": [{"z": true, "a": null}], "c": "x"});
        assert_eq!(
            canonical(&value),
            r#"{"a":[{"a":null,"z":true}],"b":1,"c":"x"}"#
        );
    }

    #[test]
    fn code_point_order_matches_the_declared_strategy() {
        assert_eq!(
            compare_unicode_code_points("a", "b"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_unicode_code_points("ab", "b"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_unicode_code_points("abc", "ab"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_unicode_code_points("same", "same"),
            std::cmp::Ordering::Equal
        );
        // A non-BMP character sorts above every BMP character.
        assert_eq!(
            compare_unicode_code_points("\u{1F600}", "\u{FFFD}"),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn primitives_encode_as_json() {
        assert_eq!(canonical(&json!(null)), "null");
        assert_eq!(canonical(&json!(true)), "true");
        assert_eq!(canonical(&json!(0)), "0");
        assert_eq!(canonical(&json!(-3)), "-3");
        assert_eq!(canonical(&json!("a\"b")), "\"a\\\"b\"");
        assert_eq!(canonical(&Json::Array(vec![])), "[]");
        assert_eq!(canonical(&Json::Object(serde_json::Map::new())), "{}");
    }
}
