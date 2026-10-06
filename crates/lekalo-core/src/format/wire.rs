//! The closed `format-report.v0.8.0` wire: the command payload, its file
//! rows, and the summary invariants (issue #94).
//!
//! Object member order in the emitted JSON follows `serde_json`'s
//! byte-sorted maps; the schema is order-insensitive and the gates assert
//! the semantic invariants (`selected = changed + unchanged`, no duplicate
//! rows, byte-sorted paths, zero applied files in check mode).

use crate::format::policy::canonical_json;

/// One comment-loss row of one file.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FileLoss {
    pub line: usize,
    /// The fixed closed reason: `ambiguous-owner` or
    /// `unrepresentable-form`.
    pub reason: String,
}

/// One file row of the report.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FileRow {
    /// Logical project-relative POSIX path.
    pub path: String,
    /// The retained source syntax of the document.
    pub syntax: crate::format::render::Syntax,
    /// `changed` or `unchanged`.
    pub action: String,
    #[serde(rename = "bytesBefore")]
    pub bytes_before: usize,
    #[serde(rename = "bytesAfter")]
    pub bytes_after: usize,
    #[serde(rename = "sha256Before")]
    pub sha256_before: String,
    #[serde(rename = "sha256After")]
    pub sha256_after: String,
    /// Semantic equality of the verified candidate (always true for a
    /// planned row; the oracle refused the plan otherwise).
    pub equal: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub loss: Vec<FileLoss>,
    /// The authorized replacement text (editor rows only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

impl FileRow {
    pub fn is_changed(&self) -> bool {
        self.action == "changed"
    }
}

/// One aggregated loss row used by the human projection and the
/// `format.comments-loss` diagnostic.
#[derive(Clone, Debug)]
pub struct LossRow {
    pub path: String,
    pub line: usize,
    pub reason: String,
}

/// The closed report document.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Report {
    pub mode: &'static str,
    /// The policy contract version.
    #[serde(rename = "policyVersion")]
    pub policy_version: String,
    #[serde(rename = "policyDigest")]
    pub policy_digest: String,
    #[serde(rename = "modelVersion")]
    pub model_version: String,
    /// The IR contract version the equality oracle compiled against.
    #[serde(rename = "irVersion")]
    pub ir_version: String,
    #[serde(rename = "scopeKind")]
    pub scope_kind: String,
    #[serde(rename = "scopeSelector")]
    pub scope_selector: Option<String>,
    pub rows: Vec<FileRow>,
    pub selected: usize,
    pub changed: usize,
    pub applied: usize,
}

impl Report {
    pub fn unchanged_count(&self) -> usize {
        self.rows.iter().filter(|row| !row.is_changed()).count()
    }

    /// The exact compact report bytes (the payload document).
    pub fn to_json(&self) -> String {
        let value = serde_json::to_value(self).expect("report serializes");
        let mut out = String::new();
        canonical_json(&value, &mut out);
        out
    }
}

/// Wrap a report document into the command envelope spelling.
pub fn envelope(status: &str, report_json: &str) -> String {
    format!("{{\"status\":\"{status}\",\"format\":{report_json}}}")
}
