//! Canonical serialization: byte-ordered keys, semantic-ID ordering, and
//! deterministic compact JSON.

use super::frontends::{Node, Scalar, Value};

/// A canonical value: mapping entries are kept pre-sorted by raw UTF-8 key
/// bytes.
#[derive(Clone, Debug, PartialEq)]
pub enum Canonical {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    Seq(Vec<Canonical>),
    Map(Vec<(String, Canonical)>),
}

impl Canonical {
    /// Convert a spanned node tree, recursively ordering object keys.
    pub fn from_node(node: &Node) -> Self {
        match &node.value {
            Value::Scalar(scalar) => Self::from_scalar(scalar),
            Value::Seq(items) => Self::Seq(items.iter().map(Self::from_node).collect()),
            Value::Map(entries) => {
                let mut pairs: Vec<(String, Canonical)> = entries
                    .iter()
                    .map(|entry| (entry.key.clone(), Self::from_node(&entry.value)))
                    .collect();
                pairs.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
                pairs.dedup_by(|left, right| left.0 == right.0);
                Self::Map(pairs)
            }
        }
    }

    fn from_scalar(scalar: &Scalar) -> Self {
        match scalar {
            Scalar::Null => Self::Null,
            Scalar::Bool(value) => Self::Bool(*value),
            Scalar::Int(value) => Self::Int(*value),
            Scalar::Float(value) => Self::Float(*value),
            Scalar::Str(text) => Self::Str(text.clone()),
        }
    }

    /// Serialize compactly (no whitespace) onto `out`.
    pub fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Int(value) => out.push_str(&value.to_string()),
            Self::Float(value) => {
                let rendered = format!("{value:?}");
                out.push_str(&rendered);
            }
            Self::Str(text) => write_json_string(text, out),
            Self::Seq(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Self::Map(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_json_string(key, out);
                    out.push(':');
                    value.write_json(out);
                }
                out.push('}');
            }
        }
    }

    /// The full compact JSON bytes.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }
}

/// JSON string escaping: RFC 8259 mandatory escapes; all other Unicode
/// scalars are emitted verbatim as UTF-8.
pub(crate) fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::frontends::{MapEntry, Span};

    fn span() -> Span {
        Span::EMPTY_EOF
    }

    #[test]
    fn keys_are_ordered_by_utf8_bytes_recursively() {
        let node = Node {
            span: span(),
            value: Value::Map(vec![
                MapEntry {
                    key: "zeta".to_owned(),
                    key_span: span(),
                    value: Node::scalar(Scalar::Int(1), span()),
                },
                MapEntry {
                    key: "alpha".to_owned(),
                    key_span: span(),
                    value: Node {
                        span: span(),
                        value: Value::Map(vec![MapEntry {
                            key: "b".to_owned(),
                            key_span: span(),
                            value: Node::scalar(Scalar::Str("é".to_owned()), span()),
                        }]),
                    },
                },
            ]),
        };
        let canonical = Canonical::from_node(&node);
        assert_eq!(canonical.to_json(), "{\"alpha\":{\"b\":\"é\"},\"zeta\":1}");
    }

    #[test]
    fn strings_escape_control_characters_deterministically() {
        let node = Node::scalar(Scalar::Str("a\"b\\c\nd\te\u{0001}f".to_owned()), span());
        let canonical = Canonical::from_node(&node);
        assert_eq!(canonical.to_json(), "\"a\\\"b\\\\c\\nd\\te\\u0001f\"");
    }
}
