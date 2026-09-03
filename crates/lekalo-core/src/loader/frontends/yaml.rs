//! YAML 1.2-core, single-document, block-style frontend with spans.
//!
//! The accepted surface is closed: block mappings/sequences; plain,
//! single-quoted, double-quoted, and block (literal/folded) scalars;
//! comments; core `null`/`true`/`false`; base-10 integer tokens. Directives,
//! document markers, multi-document streams, anchors, aliases, tags, merge
//! keys, non-string mapping keys, flow style, and special numeric forms are
//! rejected with stable codes. Event positions are converted to byte offsets
//! against the original document.

use super::{MapEntry, Node, Parsed, Scalar, Value, MAX_NESTING_DEPTH, MAX_NODES_PER_DOCUMENT};
use crate::loader::error::{Diagnostic, Span, SpanPos};
use crate::loader::source::LineIndex;
use saphyr_parser::{Event, Parser, ScalarStyle, Span as EventSpan, StrInput};

/// Maps character offsets (saphyr's `str`-input index space) to byte offsets.
struct ByteMap {
    offsets: Vec<usize>,
}

impl ByteMap {
    fn new(text: &str) -> Self {
        let mut offsets = Vec::with_capacity(text.chars().count() + 1);
        for (byte_index, _) in text.char_indices() {
            offsets.push(byte_index);
        }
        offsets.push(text.len());
        Self { offsets }
    }

    fn byte(&self, char_index: usize) -> usize {
        self.offsets
            .get(char_index)
            .copied()
            .unwrap_or(self.offsets[self.offsets.len() - 1])
    }

    fn span(&self, span: &EventSpan) -> Span {
        Span::new(
            self.position(span.start.line(), span.start.col(), span.start.index()),
            self.position(span.end.line(), span.end.col(), span.end.index()),
        )
    }

    /// Saphyr lines are 1-based and columns are 0-based character columns;
    /// the wire format wants 1-based columns.
    fn position(&self, line: usize, column: usize, char_index: usize) -> SpanPos {
        SpanPos {
            byte: self.byte(char_index),
            line,
            column: column + 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CollectionKind {
    Mapping,
    Sequence,
}

/// Walk state for one document parse. The parser is owned so key/value
/// pairing can consume events directly.
struct Builder<'text> {
    bytes: &'text [u8],
    map: ByteMap,
    parser: Parser<'text, StrInput<'text>>,
    nodes: usize,
    depth: usize,
    diagnostics: Vec<Diagnostic>,
}

/// Outcome of one subtree build.
enum Built {
    Node(Node),
    /// A diagnostic was recorded; the whole walk stops.
    Rejected,
}

impl<'text> Builder<'text> {
    fn new(text: &'text str) -> Self {
        Self {
            bytes: text.as_bytes(),
            map: ByteMap::new(text),
            parser: Parser::new_from_str(text),
            nodes: 0,
            depth: 0,
            diagnostics: Vec::new(),
        }
    }

    fn next_event(
        &mut self,
    ) -> Option<Result<(Event<'text>, EventSpan), saphyr_parser::ScanError>> {
        self.parser.next_event()
    }

    fn reject(&mut self, code: &'static str, span: Span, detail: &str) {
        self.diagnostics.push(
            Diagnostic::new(code)
                .with_span(span)
                .with_data(serde_json::json!({ "detail": detail })),
        );
    }

    fn limit(&mut self, span: Span, detail: &str) {
        self.diagnostics.push(
            Diagnostic::new("loader.limit-exceeded")
                .with_span(span)
                .with_data(serde_json::json!({ "limit": detail })),
        );
    }

    /// Flow collections are detected positionally: their event span starts
    /// exactly at the `{` or `[` token.
    fn is_flow(&self, span: &EventSpan) -> bool {
        matches!(
            self.bytes.get(self.map.byte(span.start.index())),
            Some(b'{') | Some(b'[')
        )
    }

    /// Resolve one scalar event against the closed accepted surface.
    fn scalar_node(&mut self, event: &Event<'text>, span: &EventSpan) -> Built {
        let wire_span = self.map.span(span);
        let Event::Scalar(raw, style, anchor_id, tag) = event else {
            unreachable!("scalar_node called with a non-scalar event");
        };
        if *anchor_id != 0 {
            self.reject("loader.yaml-unsupported", wire_span, "anchor");
            return Built::Rejected;
        }
        if tag.is_some() {
            self.reject("loader.yaml-unsupported", wire_span, "tag");
            return Built::Rejected;
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES_PER_DOCUMENT {
            self.limit(wire_span, "nodes-per-document");
            return Built::Rejected;
        }
        let value = match style {
            ScalarStyle::Plain => {
                if raw == "<<" {
                    // The merge key form; a quoted "<<" stays a string.
                    self.reject("loader.yaml-unsupported", wire_span, "merge-key");
                    return Built::Rejected;
                }
                match resolve_plain(raw) {
                    Ok(scalar) => scalar,
                    Err(detail) => {
                        self.reject("loader.yaml-unsupported", wire_span, detail);
                        return Built::Rejected;
                    }
                }
            }
            ScalarStyle::SingleQuoted
            | ScalarStyle::DoubleQuoted
            | ScalarStyle::Literal
            | ScalarStyle::Folded => Scalar::Str(raw.to_string()),
        };
        Built::Node(Node::scalar(value, wire_span))
    }

    /// Build one block collection subtree; consumes through its End event.
    fn build_collection(
        &mut self,
        start: &EventSpan,
        kind: CollectionKind,
    ) -> Result<Built, saphyr_parser::ScanError> {
        let start_span = self.map.span(start);
        self.nodes += 1;
        if self.nodes > MAX_NODES_PER_DOCUMENT {
            self.limit(start_span, "nodes-per-document");
            return Ok(Built::Rejected);
        }
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            self.limit(start_span, "nesting-depth");
            return Ok(Built::Rejected);
        }
        let mut entries: Vec<MapEntry> = Vec::new();
        let mut items: Vec<Node> = Vec::new();
        loop {
            let Some(next) = self.next_event() else {
                self.reject("loader.yaml-parse", start_span, "truncated-stream");
                return Ok(Built::Rejected);
            };
            let (event, span) = next?;
            match kind {
                CollectionKind::Mapping => match event {
                    Event::MappingEnd => {
                        return Ok(self.close_collection(start_span, kind, entries, items));
                    }
                    Event::Scalar(..) => {
                        let key_span = self.map.span(&span);
                        let key_node = match self.scalar_node(&event, &span) {
                            Built::Rejected => return Ok(Built::Rejected),
                            Built::Node(node) => node,
                        };
                        let key = match &key_node.value {
                            Value::Scalar(Scalar::Str(text)) => text.clone(),
                            _ => {
                                self.reject("loader.yaml-unsupported", key_span, "non-string-key");
                                return Ok(Built::Rejected);
                            }
                        };
                        // The next event is this key's value.
                        let value = match self.build_value()? {
                            Built::Rejected => return Ok(Built::Rejected),
                            Built::Node(node) => node,
                        };
                        if let Some(existing) = entries.iter().find(|entry| entry.key == key) {
                            self.diagnostics.push(
                                Diagnostic::new("loader.duplicate-key")
                                    .with_span(key_span)
                                    .with_data(serde_json::json!({
                                        "key": key,
                                        "firstSpan": span_json(existing.key_span),
                                        "duplicateSpan": span_json(key_span),
                                    })),
                            );
                            return Ok(Built::Rejected);
                        }
                        entries.push(MapEntry {
                            key,
                            key_span,
                            value,
                        });
                    }
                    Event::MappingStart(..) | Event::SequenceStart(..) | Event::Alias(..) => {
                        self.reject(
                            "loader.yaml-unsupported",
                            self.map.span(&span),
                            "non-string-key",
                        );
                        return Ok(Built::Rejected);
                    }
                    _ => {
                        self.reject(
                            "loader.yaml-unsupported",
                            self.map.span(&span),
                            "unexpected-event",
                        );
                        return Ok(Built::Rejected);
                    }
                },
                CollectionKind::Sequence => match event {
                    Event::SequenceEnd => {
                        return Ok(self.close_collection(start_span, kind, entries, items));
                    }
                    Event::Scalar(..) => match self.scalar_node(&event, &span) {
                        Built::Rejected => return Ok(Built::Rejected),
                        Built::Node(node) => items.push(node),
                    },
                    Event::MappingStart(..) | Event::SequenceStart(..) => {
                        if self.is_flow(&span) {
                            self.reject(
                                "loader.yaml-unsupported",
                                self.map.span(&span),
                                "flow-style",
                            );
                            return Ok(Built::Rejected);
                        }
                        let child_kind = if matches!(event, Event::MappingStart(..)) {
                            CollectionKind::Mapping
                        } else {
                            CollectionKind::Sequence
                        };
                        match self.build_collection(&span, child_kind)? {
                            Built::Rejected => return Ok(Built::Rejected),
                            Built::Node(node) => items.push(node),
                        }
                    }
                    Event::Alias(..) => {
                        self.reject("loader.yaml-unsupported", self.map.span(&span), "alias");
                        return Ok(Built::Rejected);
                    }
                    _ => {
                        self.reject(
                            "loader.yaml-unsupported",
                            self.map.span(&span),
                            "unexpected-event",
                        );
                        return Ok(Built::Rejected);
                    }
                },
            }
        }
    }

    /// Build the value event following a mapping key.
    fn build_value(&mut self) -> Result<Built, saphyr_parser::ScanError> {
        let Some(next) = self.next_event() else {
            return Ok(Built::Rejected);
        };
        let (event, span) = next?;
        match event {
            Event::Scalar(..) => Ok(self.scalar_node(&event, &span)),
            Event::MappingStart(..) => {
                if self.is_flow(&span) {
                    self.reject(
                        "loader.yaml-unsupported",
                        self.map.span(&span),
                        "flow-style",
                    );
                    return Ok(Built::Rejected);
                }
                self.build_collection(&span, CollectionKind::Mapping)
            }
            Event::SequenceStart(..) => {
                if self.is_flow(&span) {
                    self.reject(
                        "loader.yaml-unsupported",
                        self.map.span(&span),
                        "flow-style",
                    );
                    return Ok(Built::Rejected);
                }
                self.build_collection(&span, CollectionKind::Sequence)
            }
            Event::Alias(..) => {
                self.reject("loader.yaml-unsupported", self.map.span(&span), "alias");
                Ok(Built::Rejected)
            }
            _ => {
                self.reject(
                    "loader.yaml-unsupported",
                    self.map.span(&span),
                    "unexpected-event",
                );
                Ok(Built::Rejected)
            }
        }
    }

    fn close_collection(
        &mut self,
        start_span: Span,
        kind: CollectionKind,
        entries: Vec<MapEntry>,
        items: Vec<Node>,
    ) -> Built {
        self.depth -= 1;
        // The End event's span is empty at EOF for block collections; extend
        // the node span to the last contained position when that happens.
        Built::Node(Node {
            span: start_span,
            value: match kind {
                CollectionKind::Mapping => Value::Map(entries),
                CollectionKind::Sequence => Value::Seq(items),
            },
        })
    }

    /// Drive the event stream: stream guard, document-count guard,
    /// document-marker guard, then tree construction.
    fn walk(&mut self) -> Result<Option<Node>, saphyr_parser::ScanError> {
        match self.next_event() {
            Some(Ok((Event::StreamStart, _))) => {}
            Some(Ok(_)) | None => {
                self.reject("loader.yaml-parse", Span::EMPTY_EOF, "no-stream-start");
                return Ok(None);
            }
            Some(Err(error)) => return Err(error),
        }
        let mut documents = 0usize;
        let mut root: Option<Node> = None;
        while let Some(next) = self.next_event() {
            let (event, span) = next?;
            match event {
                Event::StreamEnd => break,
                Event::DocumentStart(explicit) => {
                    documents += 1;
                    if explicit {
                        self.reject(
                            "loader.yaml-unsupported",
                            self.map.span(&span),
                            "document-marker",
                        );
                        return Ok(None);
                    }
                    if documents > 1 {
                        self.reject(
                            "loader.yaml-unsupported",
                            self.map.span(&span),
                            "multi-document",
                        );
                        return Ok(None);
                    }
                }
                Event::DocumentEnd => {
                    // The implicit EOF end is an empty span; explicit `---`
                    // and `...` markers carry a non-empty span.
                    let wire = self.map.span(&span);
                    if wire.start.byte != wire.end.byte {
                        self.reject("loader.yaml-unsupported", wire, "document-marker");
                        return Ok(None);
                    }
                }
                Event::MappingStart(..) | Event::SequenceStart(..) => {
                    if self.is_flow(&span) {
                        self.reject(
                            "loader.yaml-unsupported",
                            self.map.span(&span),
                            "flow-style",
                        );
                        return Ok(None);
                    }
                    let kind = if matches!(event, Event::MappingStart(..)) {
                        CollectionKind::Mapping
                    } else {
                        CollectionKind::Sequence
                    };
                    match self.build_collection(&span, kind)? {
                        Built::Rejected => return Ok(None),
                        Built::Node(node) => {
                            if root.is_some() {
                                self.reject("loader.yaml-unsupported", node.span, "multi-root");
                                return Ok(None);
                            }
                            root = Some(node);
                        }
                    }
                }
                Event::Scalar(..) => {
                    // A bare scalar document: parseable, but the pipeline
                    // requires a mapping root; keep the node for that shape
                    // diagnostic to point at.
                    match self.scalar_node(&event, &span) {
                        Built::Rejected => return Ok(None),
                        Built::Node(node) => {
                            if root.is_some() {
                                self.reject("loader.yaml-unsupported", node.span, "multi-root");
                                return Ok(None);
                            }
                            root = Some(node);
                        }
                    }
                }
                Event::Alias(..) => {
                    self.reject("loader.yaml-unsupported", self.map.span(&span), "alias");
                    return Ok(None);
                }
                Event::Nothing | Event::StreamStart | Event::SequenceEnd | Event::MappingEnd => {}
            }
        }
        if self.diagnostics.is_empty() {
            Ok(root)
        } else {
            Ok(None)
        }
    }
}

/// Resolve a plain scalar against the closed accepted surface.
/// `Err(detail)` marks numeric forms outside the surface.
fn resolve_plain(raw: &str) -> Result<Scalar, &'static str> {
    match raw {
        "" | "~" | "null" | "Null" | "NULL" => return Ok(Scalar::Null),
        "true" | "True" | "TRUE" => return Ok(Scalar::Bool(true)),
        "false" | "False" | "FALSE" => return Ok(Scalar::Bool(false)),
        _ => {}
    }
    let digits = raw.strip_prefix(['-', '+']).unwrap_or(raw);
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return raw
            .parse::<i128>()
            .map(Scalar::Int)
            .map_err(|_| "integer-out-of-range");
    }
    if is_special_numeric(raw) {
        return Err("numeric-form");
    }
    Ok(Scalar::Str(raw.to_owned()))
}

/// Core-float, infinity, NaN, hex/octal/binary, underscore-digit, and
/// YAML-1.1 sexagesimal-like numeric forms are outside the closed surface.
fn is_special_numeric(raw: &str) -> bool {
    let body = raw.strip_prefix(['-', '+']).unwrap_or(raw);
    if matches!(body, ".inf" | ".Inf" | ".INF" | ".nan" | ".NaN" | ".NAN") {
        return true;
    }
    if body.starts_with("0x")
        || body.starts_with("0X")
        || body.starts_with("0o")
        || body.starts_with("0b")
    {
        return true;
    }
    // Core float: digits with a dot or an exponent.
    let has_dot = body.contains('.');
    let has_exponent = body.contains(['e', 'E']);
    if (has_dot || has_exponent)
        && body.bytes().all(|byte| {
            byte.is_ascii_digit()
                || byte == b'.'
                || byte == b'e'
                || byte == b'E'
                || byte == b'+'
                || byte == b'-'
        })
        && body.bytes().any(|byte| byte.is_ascii_digit())
    {
        return true;
    }
    // YAML-1.1 underscore digit groups (`1_000`).
    if body.contains('_') && body.bytes().any(|byte| byte.is_ascii_digit()) {
        return body
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_');
    }
    false
}

fn span_json(span: Span) -> serde_json::Value {
    serde_json::json!({
        "start": { "byte": span.start.byte, "line": span.start.line, "column": span.start.column },
        "end": { "byte": span.end.byte, "line": span.end.line, "column": span.end.column },
    })
}

/// Parse a single-document, block-style YAML document.
pub fn parse(text: &str, _index: &LineIndex) -> Result<Parsed, Vec<Diagnostic>> {
    let mut builder = Builder::new(text);
    let outcome = builder.walk();
    let mut diagnostics = std::mem::take(&mut builder.diagnostics);
    match outcome {
        Err(error) => {
            let marker = error.marker();
            let position = builder
                .map
                .position(marker.line(), marker.col(), marker.index());
            diagnostics.insert(
                0,
                Diagnostic::new("loader.yaml-parse").with_span(Span {
                    start: position,
                    end: position,
                }),
            );
            Err(diagnostics)
        }
        Ok(Some(root)) => {
            if diagnostics.is_empty() {
                Ok(Parsed::Root(Box::new(root)))
            } else {
                Err(diagnostics)
            }
        }
        Ok(None) => {
            if diagnostics.is_empty() {
                Ok(Parsed::Empty)
            } else {
                Err(diagnostics)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::source::LineIndex;
    fn root_of(text: &str) -> Node {
        let index = LineIndex::new(text);
        match parse(text, &index).expect("parse") {
            Parsed::Root(node) => *node,
            Parsed::Empty => panic!("empty"),
        }
    }

    fn first_code(text: &str) -> String {
        let index = LineIndex::new(text);
        parse(text, &index)
            .expect_err("expected rejection")
            .remove(0)
            .code
    }

    #[test]
    fn block_mapping_builds_spanned_tree_with_resolved_scalars() {
        let root = root_of("a: 1\nb: yes\nc: d\n");
        let entries = root.as_map().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].value.as_str(), None); // 1 is an Int
        assert!(matches!(
            &entries[0].value.value,
            Value::Scalar(Scalar::Int(1))
        ));
        assert!(matches!(
            &entries[1].value.value,
            Value::Scalar(Scalar::Str(text)) if text == "yes"
        ));
        assert_eq!(entries[2].value.as_str(), Some("d"));
        // Key span points at `a`, value span at `1`.
        assert_eq!(entries[0].key_span.start.byte, 0);
        assert_eq!(entries[0].value.span.start.byte, 3);
    }

    #[test]
    fn multibyte_keys_produce_byte_offsets_and_scalar_columns() {
        let text = "été: xéé\n";
        let root = root_of(text);
        let entries = root.as_map().unwrap();
        assert_eq!(entries[0].key, "été");
        assert_eq!(entries[0].key_span.start.byte, 0);
        assert_eq!(entries[0].key_span.end.byte, 5); // bytes of "été"
        assert_eq!(entries[0].key_span.start.column, 1);
        assert_eq!(entries[0].value.span.start.byte, 7);
    }

    #[test]
    fn closed_surface_rejections_use_stable_codes() {
        let cases = [
            ("a: &x 1\nb: *x\n", "loader.yaml-unsupported"),
            ("a: !!str 5\n", "loader.yaml-unsupported"),
            ("{a: 1}\n", "loader.yaml-unsupported"),
            ("---\na: 1\n", "loader.yaml-unsupported"),
            ("a: 1\n---\nb: 2\n", "loader.yaml-unsupported"),
            ("a: 1.5\n", "loader.yaml-unsupported"),
            ("a: .inf\n", "loader.yaml-unsupported"),
            ("a: 0x1A\n", "loader.yaml-unsupported"),
            ("a: 1_000\n", "loader.yaml-unsupported"),
            ("1: a\n", "loader.yaml-unsupported"),
            ("true: a\n", "loader.yaml-unsupported"),
            ("a:\n  <<: b\n", "loader.yaml-unsupported"),
            ("a: 1\na: 2\n", "loader.duplicate-key"),
            ("a:\n\tb: 2\n", "loader.yaml-parse"),
        ];
        for (text, code) in cases {
            assert_eq!(first_code(text), code, "input {text:?}");
        }
    }

    #[test]
    fn quoted_scalars_never_resolve_and_block_scalars_are_accepted() {
        let root = root_of("a: '1.5'\nb: \"true\"\nc: |\n  line1\n  line2\n");
        let entries = root.as_map().unwrap();
        assert_eq!(entries[0].value.as_str(), Some("1.5"));
        assert_eq!(entries[1].value.as_str(), Some("true"));
        assert_eq!(entries[2].value.as_str(), Some("line1\nline2\n"));
    }

    #[test]
    fn crlf_and_comments_are_accepted() {
        let root = root_of("# header\r\na: 1 # trailing\r\n");
        assert_eq!(root.as_map().unwrap().len(), 1);
    }

    #[test]
    fn empty_and_comment_only_documents_are_empty() {
        let index = LineIndex::new("# just a comment\n");
        assert!(matches!(
            parse("# just a comment\n", &index),
            Ok(Parsed::Empty)
        ));
        let index = LineIndex::new("");
        assert!(matches!(parse("", &index), Ok(Parsed::Empty)));
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let mut text = String::from("root:\n");
        for level in 0..70 {
            text.push_str(&" ".repeat(level + 1));
            text.push_str(&format!("k{level}:\n"));
        }
        text.push_str(&" ".repeat(71));
        text.push_str("leaf: 1\n");
        assert_eq!(first_code(&text), "loader.limit-exceeded");
    }
}
