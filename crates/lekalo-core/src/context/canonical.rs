//! Canonical serialization of the context capsule (issue #17).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in
//! unsigned UTF-8 byte order, sections in selection-rank order, and the
//! manifest in selection-walk order. The bytes are path-independent: no
//! physical root, raw source, timestamp, host, locale, or target
//! implementation body ever enters them. The CLI adds exactly one trailing
//! LF; these functions never do.

/// A quoted JSON string value (the writer adds the surrounding quotes).
pub(crate) fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    crate::loader::canonical::write_json_string(text, &mut out);
    out
}

/// An unsigned integer value.
pub(crate) fn number(value: u64) -> String {
    value.to_string()
}

/// A boolean value.
pub(crate) fn boolean(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

/// A JSON array from an ordered iterator.
pub(crate) fn array(items: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
    out
}

/// Emit one object with byte-sorted keys; a duplicated key is a
/// programming error and panics in debug builds.
pub(crate) fn object(mut fields: Vec<(&'static str, String)>) -> String {
    fields.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(key);
        out.push_str("\":");
        out.push_str(value);
    }
    out.push('}');
    out
}
