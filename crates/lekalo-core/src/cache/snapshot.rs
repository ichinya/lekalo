//! The cache-owned snapshot of one decoded source document (issue #20).
//!
//! The parsed-fragment payload is a typed, closed mirror of the spanned
//! document tree the loader's decode produced — every id, scalar, node,
//! and byte/line/column position — so a cache hit reconstructs the exact
//! `Document` without re-running the strict frontend. The mirror adds no
//! semantics: it stores what decode already produced, and nothing else.
//! Source text never appears (positions and resolved scalars only, which
//! the loader already derives from the bytes it re-reads every run).

use serde::{Deserialize, Serialize};

use crate::loader::error::SpanPos;
use crate::loader::frontends::{MapEntry, Node, Scalar, Span, Value};
use crate::loader::project_docs::{Definition, DocKind, Document};

/// The mirror of one span.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SpanMirror {
    #[serde(rename = "sb")]
    pub(crate) start_byte: usize,
    #[serde(rename = "sl")]
    pub(crate) start_line: usize,
    #[serde(rename = "sc")]
    pub(crate) start_column: usize,
    #[serde(rename = "eb")]
    pub(crate) end_byte: usize,
    #[serde(rename = "el")]
    pub(crate) end_line: usize,
    #[serde(rename = "ec")]
    pub(crate) end_column: usize,
}

/// The mirror of one resolved scalar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub(crate) enum ScalarMirror {
    Null,
    Bool { value: bool },
    Int { value: String },
    Float { value: f64 },
    Str { value: String },
}

/// The mirror of one node value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub(crate) enum ValueMirror {
    Scalar { scalar: ScalarMirror },
    Seq { items: Vec<NodeMirror> },
    Map { entries: Vec<MapEntryMirror> },
}

/// The mirror of one spanned node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct NodeMirror {
    pub(crate) span: SpanMirror,
    pub(crate) value: ValueMirror,
}

/// The mirror of one mapping entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct MapEntryMirror {
    pub(crate) key: String,
    pub(crate) key_span: SpanMirror,
    pub(crate) value: NodeMirror,
}

/// The mirror of one decoded definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct DefinitionMirror {
    pub(crate) id: String,
    pub(crate) id_span: SpanMirror,
    pub(crate) node: NodeMirror,
    pub(crate) span: SpanMirror,
}

/// The mirror of one decoded document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct DocumentMirror {
    pub(crate) path: String,
    pub(crate) kind: DocKindMirror,
    pub(crate) version: String,
    pub(crate) version_span: SpanMirror,
    pub(crate) definitions: Vec<DefinitionMirror>,
}

/// The closed document-kind mirror vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DocKindMirror {
    Project,
    Module,
    Symbol,
}

impl From<DocKind> for DocKindMirror {
    fn from(kind: DocKind) -> Self {
        match kind {
            DocKind::Project => Self::Project,
            DocKind::Module => Self::Module,
            DocKind::Symbol => Self::Symbol,
        }
    }
}

impl From<DocKindMirror> for DocKind {
    fn from(kind: DocKindMirror) -> Self {
        match kind {
            DocKindMirror::Project => Self::Project,
            DocKindMirror::Module => Self::Module,
            DocKindMirror::Symbol => Self::Symbol,
        }
    }
}

fn span_mirror(span: Span) -> SpanMirror {
    SpanMirror {
        start_byte: span.start.byte,
        start_line: span.start.line,
        start_column: span.start.column,
        end_byte: span.end.byte,
        end_line: span.end.line,
        end_column: span.end.column,
    }
}

fn span_restore(mirror: SpanMirror) -> Span {
    Span {
        start: SpanPos {
            byte: mirror.start_byte,
            line: mirror.start_line,
            column: mirror.start_column,
        },
        end: SpanPos {
            byte: mirror.end_byte,
            line: mirror.end_line,
            column: mirror.end_column,
        },
    }
}

fn scalar_mirror(scalar: &Scalar) -> ScalarMirror {
    match scalar {
        Scalar::Null => ScalarMirror::Null,
        Scalar::Bool(value) => ScalarMirror::Bool { value: *value },
        Scalar::Int(value) => ScalarMirror::Int {
            value: value.to_string(),
        },
        Scalar::Float(value) => ScalarMirror::Float { value: *value },
        Scalar::Str(value) => ScalarMirror::Str {
            value: value.clone(),
        },
    }
}

fn scalar_restore(mirror: ScalarMirror) -> Scalar {
    match mirror {
        ScalarMirror::Null => Scalar::Null,
        ScalarMirror::Bool { value } => Scalar::Bool(value),
        ScalarMirror::Int { value } => Scalar::Int(
            value
                .parse()
                .expect("i128 round-trips through its decimal serialization"),
        ),
        ScalarMirror::Float { value } => Scalar::Float(value),
        ScalarMirror::Str { value } => Scalar::Str(value),
    }
}

fn node_mirror(node: &Node) -> NodeMirror {
    let value = match &node.value {
        Value::Scalar(scalar) => ValueMirror::Scalar {
            scalar: scalar_mirror(scalar),
        },
        Value::Seq(items) => ValueMirror::Seq {
            items: items.iter().map(node_mirror).collect(),
        },
        Value::Map(entries) => ValueMirror::Map {
            entries: entries
                .iter()
                .map(|entry| MapEntryMirror {
                    key: entry.key.clone(),
                    key_span: span_mirror(entry.key_span),
                    value: node_mirror(&entry.value),
                })
                .collect(),
        },
    };
    NodeMirror {
        span: span_mirror(node.span),
        value,
    }
}

fn node_restore(mirror: NodeMirror) -> Node {
    let value = match mirror.value {
        ValueMirror::Scalar { scalar } => Value::Scalar(scalar_restore(scalar)),
        ValueMirror::Seq { items } => Value::Seq(items.into_iter().map(node_restore).collect()),
        ValueMirror::Map { entries } => Value::Map(
            entries
                .into_iter()
                .map(|entry| MapEntry {
                    key: entry.key,
                    key_span: span_restore(entry.key_span),
                    value: node_restore(entry.value),
                })
                .collect(),
        ),
    };
    Node {
        span: span_restore(mirror.span),
        value,
    }
}

/// The lossless mirror of one decoded document.
pub(crate) fn document_mirror(document: &Document) -> DocumentMirror {
    DocumentMirror {
        path: document.path.clone(),
        kind: document.kind.into(),
        version: document.version.clone(),
        version_span: span_mirror(document.version_span),
        definitions: document
            .definitions
            .iter()
            .map(|definition| DefinitionMirror {
                id: definition.id.clone(),
                id_span: span_mirror(definition.id_span),
                node: node_mirror(&definition.node),
                span: span_mirror(definition.span),
            })
            .collect(),
    }
}

/// Reconstruct the exact decoded document from its mirror.
pub(crate) fn document_restore(mirror: DocumentMirror) -> Document {
    Document {
        path: mirror.path,
        kind: mirror.kind.into(),
        version: mirror.version,
        version_span: span_restore(mirror.version_span),
        definitions: mirror
            .definitions
            .into_iter()
            .map(|definition| Definition {
                id: definition.id,
                id_span: span_restore(definition.id_span),
                node: node_restore(definition.node),
                span: span_restore(definition.span),
            })
            .collect(),
    }
}

/// The span-stripped mirror used only for the normalized-content digest:
/// two spellings with one data tree share it.
fn spanless_value_mirror(node: &Node) -> SpanlessNode<'_> {
    let value = match &node.value {
        Value::Scalar(scalar) => SpanlessValue::Scalar {
            scalar: scalar_mirror(scalar),
        },
        Value::Seq(items) => SpanlessValue::Seq {
            items: items.iter().map(spanless_value_mirror).collect(),
        },
        Value::Map(entries) => SpanlessValue::Map {
            entries: entries
                .iter()
                .map(|entry| SpanlessEntry {
                    key: entry.key.as_str(),
                    value: spanless_value_mirror(&entry.value),
                })
                .collect(),
        },
    };
    SpanlessNode { value }
}

#[derive(Serialize)]
struct SpanlessNode<'a> {
    value: SpanlessValue<'a>,
}

#[derive(Serialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
enum SpanlessValue<'a> {
    Scalar { scalar: ScalarMirror },
    Seq { items: Vec<SpanlessNode<'a>> },
    Map { entries: Vec<SpanlessEntry<'a>> },
}

#[derive(Serialize)]
struct SpanlessEntry<'a> {
    key: &'a str,
    value: SpanlessNode<'a>,
}

/// The normalized-content digest: SHA-256 over the canonical serialization
/// of the document tree without spans. The raw digest stays the validity
/// authority; the normalized digest is derived evidence only.
pub(crate) fn normalized_digest(document: &Document) -> String {
    let views: Vec<SpanlessNode> = document
        .definitions
        .iter()
        .map(|definition| spanless_value_mirror(&definition.node))
        .collect();
    super::canonical::sha256_digest(&super::canonical::canonical_bytes(&views))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_round_trip_preserves_the_document() {
        let text = "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"planner\",\"kind\":\"module\",\"version\":1}]}";
        let index = crate::loader::source::LineIndex::new(text);
        let parsed = crate::loader::frontends::json::parse(text, &index).expect("parses");
        let document = crate::loader::project_docs::decode(
            "lekalo/modules/planner/module.yaml",
            DocKind::Module,
            &parsed,
        )
        .expect("decodes");
        let mirror = document_mirror(&document);
        let restored = document_restore(mirror.clone());
        assert_eq!(restored.path, document.path);
        assert_eq!(restored.version, document.version);
        assert_eq!(restored.definitions.len(), document.definitions.len());
        assert_eq!(restored.definitions[0].id, document.definitions[0].id);
        assert_eq!(
            restored.definitions[0].id_span.start.byte,
            document.definitions[0].id_span.start.byte
        );
        assert_eq!(document_mirror(&restored), mirror);
    }

    #[test]
    fn normalized_digest_ignores_spans_but_not_content() {
        let text = "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"planner\"}]}";
        let index = crate::loader::source::LineIndex::new(text);
        let parsed = crate::loader::frontends::json::parse(text, &index).expect("parses");
        let first = crate::loader::project_docs::decode(
            "lekalo/modules/planner/module.yaml",
            DocKind::Module,
            &parsed,
        )
        .expect("decodes");
        let other_text = "{\"definitions\":[{\"id\":\"planner\"}],\"schema_version\":\"1.0.0\"}";
        let other_index = crate::loader::source::LineIndex::new(other_text);
        let other_parsed =
            crate::loader::frontends::json::parse(other_text, &other_index).expect("parses");
        let second = crate::loader::project_docs::decode(
            "lekalo/modules/planner/module.yaml",
            DocKind::Module,
            &other_parsed,
        )
        .expect("decodes");
        assert_eq!(normalized_digest(&first), normalized_digest(&second));
    }
}
