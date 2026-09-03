//! Strict, span-producing JSON and YAML frontends feeding one spanned tree.
//!
//! Both frontends build the same [`Node`] tree over the original document
//! bytes. Selection is deterministic: the first non-whitespace byte `{` or
//! `[` selects the strict JSON frontend; anything else selects the single
//! YAML 1.2-core block frontend. A JSON failure never falls back to YAML.

pub mod json;
pub mod yaml;

use super::error::Diagnostic;
pub use super::error::Span;

/// Maximum syntax nesting depth accepted in one document.
pub const MAX_NESTING_DEPTH: usize = 64;

/// Maximum syntax nodes accepted in one document.
pub const MAX_NODES_PER_DOCUMENT: usize = 100_000;

/// A resolved scalar value. Only the closed accepted surface exists: core
/// null/true/false, base-10 integers, and strings. Floats and YAML-1.1 or
/// non-decimal numeric forms are rejected by the YAML frontend; the JSON
/// frontend accepts finite JSON numbers as floats.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
}

impl Scalar {
    /// The scalar's canonical type name used in diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Int(_) | Self::Float(_) => "number",
            Self::Str(_) => "string",
        }
    }
}

/// One spanned syntax node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub span: Span,
    pub value: Value,
}

/// Container or scalar content of a node.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Scalar(Scalar),
    Seq(Vec<Node>),
    /// Insertion-ordered mapping with per-entry key spans.
    Map(Vec<MapEntry>),
}

/// One mapping entry: resolved string key, its span, and the value node.
#[derive(Clone, Debug, PartialEq)]
pub struct MapEntry {
    pub key: String,
    pub key_span: Span,
    pub value: Node,
}

impl Node {
    pub fn scalar(scalar: Scalar, span: Span) -> Self {
        Self {
            span,
            value: Value::Scalar(scalar),
        }
    }

    /// Look up a direct mapping entry.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.value {
            Value::Map(entries) => entries
                .iter()
                .find(|entry| entry.key == key)
                .map(|entry| &entry.value),
            _ => None,
        }
    }

    /// Look up a mapping entry together with its key span.
    pub fn get_entry(&self, key: &str) -> Option<&MapEntry> {
        match &self.value {
            Value::Map(entries) => entries.iter().find(|entry| entry.key == key),
            _ => None,
        }
    }

    /// Mutable lookup of a direct sequence item slice.
    pub fn as_seq_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.value {
            Value::Seq(items) => Some(items),
            _ => None,
        }
    }

    /// Mutable lookup of a direct mapping entry value by key.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Node> {
        match &mut self.value {
            Value::Map(entries) => entries
                .iter_mut()
                .find(|entry| entry.key == key)
                .map(|entry| &mut entry.value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Scalar(Scalar::Str(text)) => Some(text),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&[MapEntry]> {
        match &self.value {
            Value::Map(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_seq(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Seq(items) => Some(items),
            _ => None,
        }
    }
}

/// Outcome of parsing one document.
#[derive(Debug)]
pub enum Parsed {
    /// The document carried content (the whole tree root).
    Root(Box<Node>),
    /// The document carried no content at all (empty or comments only).
    Empty,
}

/// Select a frontend by the first non-whitespace byte and parse the text.
///
/// Counts bytes only for `{` and `[` selection: space, tab, CR, and LF are
/// skipped; every other byte selects the YAML frontend.
pub fn parse_document(
    text: &str,
    index: &super::source::LineIndex,
) -> Result<Parsed, Vec<Diagnostic>> {
    let first = text
        .bytes()
        .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'));
    match first {
        Some(b'{') | Some(b'[') => json::parse(text, index),
        _ => yaml::parse(text, index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::source::LineIndex;

    fn parse_ok(text: &str) -> Parsed {
        let index = LineIndex::new(text);
        parse_document(text, &index).unwrap()
    }

    #[test]
    fn frontend_selection_picks_json_only_for_leading_brace_or_bracket() {
        assert!(matches!(parse_ok("{\"a\":1}\n"), Parsed::Root(_)));
        assert!(matches!(parse_ok("  [1]\n"), Parsed::Root(_)));
        assert!(matches!(parse_ok("a: 1\n"), Parsed::Root(_)));
        assert!(matches!(parse_ok("# c\na: 1\n"), Parsed::Root(_)));
        assert!(matches!(parse_ok(""), Parsed::Empty));
    }

    #[test]
    fn json_failure_never_falls_back_to_yaml() {
        let text = "{\"a\":1,}\n";
        let index = LineIndex::new(text);
        let error = parse_document(text, &index).unwrap_err();
        assert_eq!(error[0].code, "loader.json-parse");
    }
}
