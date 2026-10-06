//! The closed `format-report.v0.8.0` wire: the command payload, its file
//! rows, and the summary invariants (issue #94).
//!
//! Object member order in the emitted JSON follows the canonical writer
//! (byte-sorted keys, compact); the schema is order-insensitive and the
//! gates assert the semantic invariants (`selected = changed +
//! unchanged`, no duplicate rows, byte-sorted paths, zero applied files
//! in check mode).

use crate::format::policy::canonical_json;

/// The exact wire discriminator of the report contract.
pub const SCHEMA_VERSION: &str = "lekalo/format-report/v0.8.0";
/// The exact report identity.
pub const IDENTITY: &str = "dev.lekalo.format-report@0.8.0";

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
    pub schema_version: &'static str,
    pub identity: &'static str,
    pub mode: &'static str,
    /// A completed plan always reports complete state; an incomplete plan
    /// never reports equality or zero changes as known.
    pub complete: bool,
    pub policy: PolicyRef,
    pub model: VersionRef,
    pub ir: VersionRef,
    pub scope: Scope,
    pub files: Vec<FileRow>,
    pub summary: Summary,
}

/// The pinned policy the plan was built under.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PolicyRef {
    pub version: String,
    pub digest: String,
}

/// One admitted contract version the plan bound.
#[derive(Clone, Debug, serde::Serialize)]
pub struct VersionRef {
    pub version: String,
}

/// The resolved scope selector.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Scope {
    pub kind: String,
    /// Null for the whole-source scope; the schema requires the member.
    pub selector: Option<String>,
}

/// The closed summary counts.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Summary {
    pub selected: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub applied: usize,
    #[serde(rename = "lossCount")]
    pub loss_count: usize,
}

impl Report {
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
