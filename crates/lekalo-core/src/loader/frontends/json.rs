//! Strict RFC 8259 JSON frontend with byte-precise spans.
//!
//! Rejects comments, trailing commas, duplicate keys (citing both spans),
//! lone surrogates, raw control characters, leading zeros, `+` signs, and
//! numbers that cannot be represented exactly. Parsing never falls back.

use super::{MapEntry, Node, Parsed, Scalar, Value, MAX_NESTING_DEPTH, MAX_NODES_PER_DOCUMENT};
use crate::loader::error::{Diagnostic, Span};
use crate::loader::source::LineIndex;

struct Parser<'text> {
    text: &'text str,
    bytes: &'text [u8],
    index: &'text LineIndex,
    position: usize,
    nodes: usize,
    diagnostics: Vec<Diagnostic>,
}

/// The failure channel: one parse error plus any duplicate-key diagnostics
/// collected before it.
struct Failed(Vec<Diagnostic>);

type ParseResult<T> = std::result::Result<T, Failed>;

impl<'text> Parser<'text> {
    fn new(text: &'text str, index: &'text LineIndex) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            index,
            position: 0,
            nodes: 0,
            diagnostics: Vec::new(),
        }
    }

    fn fail(&self, code: &'static str, start: usize, end: usize, detail: &str) -> Failed {
        Failed(vec![self.diagnostic(code, start, end, detail)])
    }

    fn diagnostic(&self, code: &str, start: usize, end: usize, detail: &str) -> Diagnostic {
        Diagnostic::new(code)
            .with_span(self.index.span(self.text, start, end.max(start)))
            .with_data(serde_json::json!({ "detail": detail }))
    }

    fn span(&self, start: usize, end: usize) -> Span {
        self.index.span(self.text, start, end.max(start))
    }

    fn skip_ws(&mut self) {
        while matches!(
            self.bytes.get(self.position),
            Some(b' ' | b'\t' | b'\n' | b'\r')
        ) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn count_node(&mut self, start: usize, end: usize) -> ParseResult<()> {
        self.nodes += 1;
        if self.nodes > MAX_NODES_PER_DOCUMENT {
            return Err(self.fail("loader.limit-exceeded", start, end, "nodes-per-document"));
        }
        Ok(())
    }

    fn parse_value(&mut self, depth: usize) -> ParseResult<Node> {
        if depth > MAX_NESTING_DEPTH {
            let position = self.position;
            return Err(self.fail("loader.limit-exceeded", position, position, "nesting-depth"));
        }
        self.skip_ws();
        let start = self.position;
        match self.peek() {
            Some(b'{') => self.parse_object(start, depth),
            Some(b'[') => self.parse_array(start, depth),
            Some(b'"') => self.parse_string_node(),
            Some(b't') => self.parse_literal("true", Scalar::Bool(true)),
            Some(b'f') => self.parse_literal("false", Scalar::Bool(false)),
            Some(b'n') => self.parse_literal("null", Scalar::Null),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.parse_number(),
            Some(_) => Err(self.fail("loader.json-parse", start, start + 1, "unexpected-byte")),
            None => Err(self.fail("loader.json-parse", start, start, "unexpected-end")),
        }
    }

    fn parse_object(&mut self, start: usize, depth: usize) -> ParseResult<Node> {
        self.count_node(start, start + 1)?;
        self.position += 1; // consume '{'
        let mut entries: Vec<MapEntry> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            let end = self.position + 1;
            self.position = end;
            return Ok(Node {
                span: self.span(start, end),
                value: Value::Map(entries),
            });
        }
        loop {
            self.skip_ws();
            let key_start = self.position;
            if self.peek() != Some(b'"') {
                return Err(self.fail(
                    "loader.json-parse",
                    key_start,
                    key_start + 1,
                    "key-expected",
                ));
            }
            let key_node = self.parse_string_node()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                let position = self.position;
                return Err(self.fail(
                    "loader.json-parse",
                    position,
                    position + 1,
                    "colon-expected",
                ));
            }
            self.position += 1;
            let value = self.parse_value(depth + 1)?;
            let key = match &key_node.value {
                Value::Scalar(Scalar::Str(text)) => text.clone(),
                _ => unreachable!("string parser always yields Str"),
            };
            if let Some(existing) = entries.iter().find(|entry| entry.key == key) {
                // Both spans: the first definition and this duplicate.
                self.diagnostics.push(
                    self.diagnostic(
                        "loader.duplicate-key",
                        key_node.span.start.byte,
                        key_node.span.end.byte,
                        "duplicate-object-key",
                    )
                    .with_data(serde_json::json!({
                        "key": key,
                        "firstSpan": span_json(existing.key_span),
                        "duplicateSpan": span_json(key_node.span),
                    })),
                );
            } else {
                entries.push(MapEntry {
                    key,
                    key_span: key_node.span,
                    value,
                });
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.position += 1;
                    self.skip_ws();
                    if self.peek() == Some(b'}') {
                        // Trailing comma.
                        let position = self.position;
                        return Err(self.fail(
                            "loader.json-parse",
                            position,
                            position + 1,
                            "trailing-comma",
                        ));
                    }
                }
                Some(b'}') => {
                    let end = self.position + 1;
                    self.position = end;
                    return Ok(Node {
                        span: self.span(start, end),
                        value: Value::Map(entries),
                    });
                }
                _ => {
                    let position = self.position;
                    return Err(self.fail(
                        "loader.json-parse",
                        position,
                        position + 1,
                        "comma-or-brace-expected",
                    ));
                }
            }
        }
    }

    fn parse_array(&mut self, start: usize, depth: usize) -> ParseResult<Node> {
        self.count_node(start, start + 1)?;
        self.position += 1; // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            let end = self.position + 1;
            self.position = end;
            return Ok(Node {
                span: self.span(start, end),
                value: Value::Seq(items),
            });
        }
        loop {
            let value = self.parse_value(depth + 1)?;
            items.push(value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.position += 1;
                    self.skip_ws();
                    if self.peek() == Some(b']') {
                        let position = self.position;
                        return Err(self.fail(
                            "loader.json-parse",
                            position,
                            position + 1,
                            "trailing-comma",
                        ));
                    }
                }
                Some(b']') => {
                    let end = self.position + 1;
                    self.position = end;
                    return Ok(Node {
                        span: self.span(start, end),
                        value: Value::Seq(items),
                    });
                }
                _ => {
                    let position = self.position;
                    return Err(self.fail(
                        "loader.json-parse",
                        position,
                        position + 1,
                        "comma-or-bracket-expected",
                    ));
                }
            }
        }
    }

    fn parse_string_node(&mut self) -> ParseResult<Node> {
        let start = self.position;
        let text = self.parse_string_body()?;
        let end = self.position;
        self.count_node(start, end)?;
        Ok(Node::scalar(Scalar::Str(text), self.span(start, end)))
    }

    /// Parse a JSON string starting at the opening quote; consumes through
    /// the closing quote.
    fn parse_string_body(&mut self) -> ParseResult<String> {
        let start = self.position;
        self.position += 1; // opening quote
        let mut decoded = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(self.fail(
                        "loader.json-parse",
                        start,
                        self.position,
                        "unterminated-string",
                    ))
                }
                Some(b'"') => {
                    self.position += 1;
                    return Ok(decoded);
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escape = self.position;
                    match self.peek() {
                        Some(b'"') => decoded.push('"'),
                        Some(b'\\') => decoded.push('\\'),
                        Some(b'/') => decoded.push('/'),
                        Some(b'b') => decoded.push('\u{0008}'),
                        Some(b'f') => decoded.push('\u{000C}'),
                        Some(b'n') => decoded.push('\n'),
                        Some(b'r') => decoded.push('\r'),
                        Some(b't') => decoded.push('\t'),
                        Some(b'u') => {
                            let high = self.parse_hex4()?;
                            let scalar = if (0xD800..0xDC00).contains(&high) {
                                // Require a low surrogate pair.
                                if self.bytes.get(self.position + 1) != Some(&b'\\')
                                    || self.bytes.get(self.position + 2) != Some(&b'u')
                                {
                                    return Err(self.fail(
                                        "loader.json-parse",
                                        escape,
                                        escape + 2,
                                        "unpaired-surrogate",
                                    ));
                                }
                                self.position += 2;
                                let low = self.parse_hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.fail(
                                        "loader.json-parse",
                                        escape,
                                        escape + 2,
                                        "unpaired-surrogate",
                                    ));
                                }
                                let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                                char::from_u32(combined).ok_or_else(|| {
                                    self.fail(
                                        "loader.json-parse",
                                        escape,
                                        escape + 2,
                                        "invalid-escape",
                                    )
                                })?
                            } else if (0xDC00..0xE000).contains(&high) {
                                return Err(self.fail(
                                    "loader.json-parse",
                                    escape,
                                    escape + 2,
                                    "unpaired-surrogate",
                                ));
                            } else {
                                char::from_u32(high).ok_or_else(|| {
                                    self.fail(
                                        "loader.json-parse",
                                        escape,
                                        escape + 2,
                                        "invalid-escape",
                                    )
                                })?
                            };
                            decoded.push(scalar);
                            continue;
                        }
                        _ => {
                            return Err(self.fail(
                                "loader.json-parse",
                                escape,
                                escape + 1,
                                "invalid-escape",
                            ))
                        }
                    }
                    self.position += 1;
                }
                Some(byte) if byte < 0x20 => {
                    return Err(self.fail(
                        "loader.json-parse",
                        self.position,
                        self.position + 1,
                        "raw-control-character",
                    ))
                }
                Some(_) => {
                    // Copy one UTF-8 scalar verbatim.
                    let rest = &self.text[self.position..];
                    let scalar = rest.chars().next().unwrap();
                    decoded.push(scalar);
                    self.position += scalar.len_utf8();
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> ParseResult<u32> {
        let start = self.position + 1;
        let slice = self
            .bytes
            .get(start..start + 4)
            .ok_or_else(|| self.fail("loader.json-parse", start, start + 4, "invalid-escape"))?;
        let text = std::str::from_utf8(slice)
            .ok()
            .and_then(|text| u32::from_str_radix(text, 16).ok())
            .ok_or_else(|| self.fail("loader.json-parse", start, start + 4, "invalid-escape"))?;
        self.position = start + 4;
        Ok(text)
    }

    fn parse_literal(&mut self, literal: &str, scalar: Scalar) -> ParseResult<Node> {
        let start = self.position;
        let end = start + literal.len();
        if self.bytes.get(start..end) != Some(literal.as_bytes()) {
            return Err(self.fail("loader.json-parse", start, start + 1, "invalid-literal"));
        }
        self.position = end;
        self.count_node(start, end)?;
        Ok(Node::scalar(scalar, self.span(start, end)))
    }

    fn parse_number(&mut self) -> ParseResult<Node> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        // Integer part: 0 | [1-9][0-9]*
        match self.peek() {
            Some(b'0') => self.position += 1,
            Some(byte) if byte.is_ascii_digit() => {
                while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                    self.position += 1;
                }
            }
            _ => {
                let position = self.position;
                return Err(self.fail(
                    "loader.json-parse",
                    position,
                    position + 1,
                    "invalid-number",
                ));
            }
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.position += 1;
            if !matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                let position = self.position;
                return Err(self.fail(
                    "loader.json-parse",
                    position,
                    position + 1,
                    "invalid-number",
                ));
            }
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            if !matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                let position = self.position;
                return Err(self.fail(
                    "loader.json-parse",
                    position,
                    position + 1,
                    "invalid-number",
                ));
            }
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        let end = self.position;
        let token = &self.text[start..end];
        let scalar = if is_float {
            let parsed: f64 = token
                .parse()
                .map_err(|_| self.fail("loader.json-parse", start, end, "invalid-number"))?;
            if !parsed.is_finite() {
                return Err(self.fail("loader.json-parse", start, end, "non-finite-number"));
            }
            Scalar::Float(parsed)
        } else {
            match token.parse::<i128>() {
                Ok(value) => Scalar::Int(value),
                Err(_) => {
                    // Never silently lossy: an integer that exceeds i128 is
                    // a parse-level rejection.
                    return Err(self.fail(
                        "loader.json-parse",
                        start,
                        end,
                        "number-not-representable",
                    ));
                }
            }
        };
        self.count_node(start, end)?;
        Ok(Node::scalar(scalar, self.span(start, end)))
    }
}

fn span_json(span: Span) -> serde_json::Value {
    serde_json::json!({
        "start": { "byte": span.start.byte, "line": span.start.line, "column": span.start.column },
        "end": { "byte": span.end.byte, "line": span.end.line, "column": span.end.column },
    })
}

/// Parse a whole JSON document; the root must be the entire input.
pub fn parse(text: &str, index: &LineIndex) -> std::result::Result<Parsed, Vec<Diagnostic>> {
    let mut parser = Parser::new(text, index);
    let root = parser.parse_value(1);
    match root {
        Err(Failed(diagnostics)) => Err(diagnostics),
        Ok(node) => {
            parser.skip_ws();
            if parser.position != parser.bytes.len() {
                let position = parser.position;
                return Err(vec![parser.diagnostic(
                    "loader.json-parse",
                    position,
                    position + 1,
                    "trailing-content",
                )]);
            }
            let mut diagnostics = parser.diagnostics;
            if !diagnostics.is_empty() {
                diagnostics
                    .sort_by_key(|diagnostic| diagnostic.span.as_ref().map(|s| s.start.byte));
                return Err(diagnostics);
            }
            Ok(Parsed::Root(Box::new(node)))
        }
    }
}
