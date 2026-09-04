//! Stable loader statuses, diagnostics, and failure envelopes (issue #7).

use std::borrow::Cow;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Maximum diagnostics collected within one loader phase before it stops.
///
/// The bound keeps hostile inputs bounded; it is documented in
/// `docs/loader.md` and asserted by fixtures.
pub const MAX_PHASE_DIAGNOSTICS: usize = 100;

/// Maximum Unicode scalar values of an import token echoed in the
/// `data.import` field of `loader.import-invalid` and
/// `loader.path-escape` diagnostics.
///
/// Hostile tokens would otherwise scale the failure envelope with input
/// size; the bound is fixed, documented in `docs/loader.md`, and asserted
/// by unit and subprocess regressions.
pub const MAX_IMPORT_ECHO_CHARS: usize = 64;

/// Marker appended to an echoed import token truncated at
/// [`MAX_IMPORT_ECHO_CHARS`].
pub const IMPORT_ECHO_ELLIPSIS: &str = "…";

/// Echo an import token inside diagnostic data: verbatim while it fits
/// [`MAX_IMPORT_ECHO_CHARS`], otherwise its first
/// [`MAX_IMPORT_ECHO_CHARS`] Unicode scalar values plus
/// [`IMPORT_ECHO_ELLIPSIS`].
pub(crate) fn bounded_import_echo(token: &str) -> Cow<'_, str> {
    // Any token of at most MAX_IMPORT_ECHO_CHARS bytes also has at most
    // that many scalar values, so it always fits: no allocation.
    if token.len() <= MAX_IMPORT_ECHO_CHARS {
        return Cow::Borrowed(token);
    }
    let mut chars = token.chars();
    let prefix: String = chars.by_ref().take(MAX_IMPORT_ECHO_CHARS).collect();
    if chars.next().is_none() {
        // Multibyte token over the byte bound but within the scalar bound.
        return Cow::Borrowed(token);
    }
    let mut echo = prefix;
    echo.push_str(IMPORT_ECHO_ELLIPSIS);
    Cow::Owned(echo)
}

/// A position inside one source document: half-open byte offset into the
/// original file plus 1-based line and Unicode-scalar column.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SpanPos {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
}

/// A half-open source range: `start` inclusive, `end` exclusive.
///
/// Bytes are 0-based offsets into the original file bytes; lines are 1-based;
/// columns are 1-based Unicode-scalar positions; CRLF counts as one break.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Span {
    pub start: SpanPos,
    pub end: SpanPos,
}

impl Span {
    pub const fn new(start: SpanPos, end: SpanPos) -> Self {
        Self { start, end }
    }

    pub const EMPTY_EOF: Self = Self {
        start: SpanPos {
            byte: 0,
            line: 1,
            column: 1,
        },
        end: SpanPos {
            byte: 0,
            line: 1,
            column: 1,
        },
    };
}

/// One typed loader or pass-through structure diagnostic.
///
/// `path` is a logical project-relative POSIX path; `span` points into that
/// document; `data` carries stable structured details. Free-form operating
/// system messages, absolute paths, and timestamps never appear here.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub path: Option<String>,
    pub span: Option<Box<Span>>,
    pub data: Option<serde_json::Value>,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            path: None,
            span: None,
            data: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(Box::new(span));
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Sort key `(path, startByte, code, data)` from the issue brief.
    fn sort_key(&self) -> (String, usize, String, String) {
        (
            self.path.clone().unwrap_or_default(),
            self.span.as_ref().map_or(0, |span| span.start.byte),
            self.code.clone(),
            self.data
                .as_ref()
                .map(serde_json::Value::to_string)
                .unwrap_or_default(),
        )
    }
}

impl Serialize for Diagnostic {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("code", &self.code)?;
        if let Some(path) = &self.path {
            map.serialize_entry("path", path)?;
        }
        if let Some(span) = &self.span {
            map.serialize_entry("span", span)?;
        }
        if let Some(data) = &self.data {
            map.serialize_entry("data", data)?;
        }
        map.end()
    }
}

/// Sort diagnostics by `(path, startByte, code, data)` and keep at most
/// [`MAX_PHASE_DIAGNOSTICS`].
pub(crate) fn finalize_diagnostics(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by_key(|diagnostic| diagnostic.sort_key());
    diagnostics.truncate(MAX_PHASE_DIAGNOSTICS);
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_sort_by_path_byte_code_data_and_respect_the_bound() {
        let span = Span {
            start: SpanPos {
                byte: 9,
                line: 2,
                column: 3,
            },
            end: SpanPos {
                byte: 12,
                line: 2,
                column: 6,
            },
        };
        let mut diagnostics = vec![
            Diagnostic::new("loader.json-parse").with_span(span),
            Diagnostic::new("loader.encoding").with_path("b.yaml"),
            Diagnostic::new("loader.json-parse").with_span(span),
            Diagnostic::new("loader.document-shape"),
        ];
        diagnostics = finalize_diagnostics(diagnostics);
        let rendered: Vec<String> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        assert_eq!(
            rendered,
            [
                "loader.document-shape",
                "loader.json-parse",
                "loader.json-parse",
                "loader.encoding"
            ]
        );
        for count in 0..MAX_PHASE_DIAGNOSTICS + 5 {
            diagnostics.push(Diagnostic::new("loader.io").with_path(format!("{count:04}")));
        }
        let bounded = finalize_diagnostics(diagnostics);
        assert_eq!(bounded.len(), MAX_PHASE_DIAGNOSTICS);
    }

    #[test]
    fn diagnostic_json_omits_absent_fields_in_declaration_order() {
        let diagnostic = Diagnostic::new("loader.import-missing")
            .with_path("lekalo/modules/planner/module.yaml")
            .with_data(serde_json::json!({ "module": "ghost" }));
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert_eq!(
            json,
            "{\"code\":\"loader.import-missing\",\"path\":\"lekalo/modules/planner/module.yaml\",\"data\":{\"module\":\"ghost\"}}"
        );
    }

    #[test]
    fn import_echo_is_verbatim_within_the_bound() {
        assert_eq!(bounded_import_echo("m2"), "m2");
        assert_eq!(bounded_import_echo("../core"), "../core");
        let exact = "a".repeat(MAX_IMPORT_ECHO_CHARS);
        assert_eq!(bounded_import_echo(&exact), exact.as_str());
    }

    #[test]
    fn import_echo_truncates_hostile_tokens_at_the_bound() {
        let hostile = "z".repeat(1_000_000);
        let expected = format!(
            "{}{IMPORT_ECHO_ELLIPSIS}",
            "z".repeat(MAX_IMPORT_ECHO_CHARS)
        );
        assert_eq!(bounded_import_echo(&hostile), expected);
        // Multibyte tokens never split a scalar value.
        let multibyte = "é".repeat(MAX_IMPORT_ECHO_CHARS + 7);
        let expected = format!(
            "{}{IMPORT_ECHO_ELLIPSIS}",
            "é".repeat(MAX_IMPORT_ECHO_CHARS)
        );
        assert_eq!(bounded_import_echo(&multibyte), expected);
    }
}
