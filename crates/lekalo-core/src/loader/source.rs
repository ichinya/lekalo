//! Source document loading, encoding gate, and byte-to-position mapping.

use super::error::{Diagnostic, Span, SpanPos};
use std::path::Path;

/// Maximum bytes accepted for one source document (8 MiB).
pub const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

/// Maximum bytes accepted across all source documents of one load (64 MiB).
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// One read source document with validated encoding.
#[derive(Clone, Debug)]
pub struct Source {
    /// Logical project-relative POSIX path, e.g. `lekalo/project.yaml`.
    pub logical_path: String,
    pub bytes: Vec<u8>,
}

impl Source {
    /// Validate UTF-8 without BOM, NUL, or lone CR; accept LF and CRLF.
    ///
    /// All violations are `loader.encoding`; a non-UTF-8 document is
    /// rejected without ever losing the logical path in the diagnostic.
    pub fn validate_encoding(logical_path: &str, bytes: &[u8]) -> Result<String, Diagnostic> {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err(Diagnostic::new("loader.encoding")
                .with_path(logical_path)
                .with_data(serde_json::json!({ "reason": "byte-order-mark" })));
        }
        if bytes.contains(&0) {
            return Err(Diagnostic::new("loader.encoding")
                .with_path(logical_path)
                .with_data(serde_json::json!({ "reason": "nul-byte" })));
        }
        let mut rest = bytes;
        while let Some(position) = rest.iter().position(|byte| *byte == b'\r') {
            let consumed = bytes.len() - rest.len();
            if rest.get(position + 1) != Some(&b'\n') {
                let offset = consumed + position;
                return Err(Diagnostic::new("loader.encoding")
                    .with_path(logical_path)
                    .with_data(serde_json::json!({ "reason": "lone-cr", "byte": offset })));
            }
            rest = &rest[position + 2..];
        }
        let text = std::str::from_utf8(bytes).map_err(|error| {
            let valid = error.valid_up_to();
            Diagnostic::new("loader.encoding")
                .with_path(logical_path)
                .with_data(serde_json::json!({ "reason": "invalid-utf8", "byte": valid }))
        })?;
        Ok(text.to_owned())
    }
}

/// Precomputed line-start index for one document.
///
/// Positions are 1-based lines and 1-based Unicode-scalar columns; a CRLF
/// pair is one line break and the CR belongs to the ending line.
#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0usize];
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'\n' {
                line_starts.push(index + 1);
            }
            index += 1;
        }
        Self { line_starts }
    }

    /// Map a byte offset to a position. Offsets beyond the end of the
    /// document clamp to the end so that error spans at EOF stay total.
    pub fn position(&self, text: &str, byte: usize) -> SpanPos {
        let byte = byte.min(text.len());
        let line = match self.line_starts.binary_search(&byte) {
            Ok(exact) => exact + 1,
            Err(insertion) => insertion,
        };
        let line_start = self.line_starts[line - 1];
        let column = text[line_start..byte].chars().count() + 1;
        SpanPos { byte, line, column }
    }

    /// Build a half-open span between two byte offsets.
    pub fn span(&self, text: &str, start: usize, end: usize) -> Span {
        Span::new(self.position(text, start), self.position(text, end))
    }
}

/// Reject a read size over the per-document limit.
pub fn check_document_limit(logical_path: &str, size: usize) -> Result<(), Diagnostic> {
    if size > MAX_DOCUMENT_BYTES {
        return Err(Diagnostic::new("loader.limit-exceeded")
            .with_path(logical_path)
            .with_data(
                serde_json::json!({ "limit": "document-bytes", "max": MAX_DOCUMENT_BYTES }),
            ));
    }
    Ok(())
}

/// Reject an accumulated read size over the total limit.
pub fn check_total_limit(size: usize) -> Result<(), Diagnostic> {
    if size > MAX_TOTAL_BYTES {
        return Err(Diagnostic::new("loader.limit-exceeded")
            .with_data(serde_json::json!({ "limit": "total-bytes", "max": MAX_TOTAL_BYTES })));
    }
    Ok(())
}

/// Expand `~`-free `CARGO_MANIFEST_DIR`-style relative test helper: the
/// repository root used by CLI integration tests.
pub fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate lives under workspace/crates")
        .parent()
        .expect("workspace root above crates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_gate_rejects_bom_nul_lone_cr_and_invalid_utf8() {
        let cases: Vec<(Vec<u8>, &str)> = vec![
            (b"\xEF\xBB\xBF{}".to_vec(), "byte-order-mark"),
            (b"a\x00b".to_vec(), "nul-byte"),
            (b"a\rb\n".to_vec(), "lone-cr"),
            (b"\xFF\xFE".to_vec(), "invalid-utf8"),
        ];
        for (bytes, reason) in cases {
            let error = Source::validate_encoding("lekalo/project.yaml", &bytes).unwrap_err();
            assert_eq!(error.code, "loader.encoding");
            assert_eq!(error.path.as_deref(), Some("lekalo/project.yaml"));
            assert_eq!(error.data.unwrap()["reason"], reason);
        }
    }

    #[test]
    fn encoding_gate_accepts_lf_crlf_and_multibyte() {
        for text in ["a: 1\nb: 2\n", "a: 1\r\nb: 2\r\n", "clé: valeurs\n"] {
            assert!(Source::validate_encoding("d.yaml", text.as_bytes()).is_ok());
        }
    }

    #[test]
    fn line_index_counts_unicode_scalars_and_treats_crlf_as_one_break() {
        let text = "é: x\r\nsecond: y\n";
        let index = LineIndex::new(text);
        let start = index.position(text, 0);
        assert_eq!((start.line, start.column, start.byte), (1, 1, 0));
        let after = index.position(text, 7); // start of "second"
        assert_eq!((after.line, after.column, after.byte), (2, 1, 7));
        let mid = index.position(text, 15); // "y"
        assert_eq!((mid.line, mid.column, mid.byte), (2, 9, 15));
    }

    #[test]
    fn positions_beyond_the_end_clamp_instead_of_panicking() {
        let text = "ab";
        let index = LineIndex::new(text);
        let pos = index.position(text, 99);
        assert_eq!((pos.line, pos.column, pos.byte), (1, 3, 2));
        let span = index.span(text, 1, 99);
        assert_eq!(span.end.byte, 2);
    }

    #[test]
    fn size_limits_report_loader_limit_exceeded() {
        let error = check_document_limit("big.yaml", MAX_DOCUMENT_BYTES + 1).unwrap_err();
        assert_eq!(error.code, "loader.limit-exceeded");
        assert_eq!(error.data.unwrap()["limit"], "document-bytes");
        let error = check_total_limit(MAX_TOTAL_BYTES + 1).unwrap_err();
        assert_eq!(error.code, "loader.limit-exceeded");
        assert_eq!(error.data.unwrap()["limit"], "total-bytes");
        assert!(check_document_limit("ok.yaml", MAX_DOCUMENT_BYTES).is_ok());
        assert!(check_total_limit(MAX_TOTAL_BYTES).is_ok());
    }
}
