//! Owner-published data obtained from the selected external provider.
use super::super::{diagnostic, provider};
use crate::diagnostics::DiagnosticSet;
use crate::transaction_concurrency::SnapshotSupport;
/// A published major version and its capability answers.
#[derive(Clone, Debug)]
pub struct MatrixRow {
    pub major: u32,
    pub answers: Vec<(String, SnapshotSupport)>,
}
fn support(value: &str) -> Option<SnapshotSupport> {
    match value {
        "full" => Some(SnapshotSupport::Full),
        "partial" => Some(SnapshotSupport::Partial),
        "unsupported" => Some(SnapshotSupport::Unsupported),
        "unknown" => Some(SnapshotSupport::Unknown),
        _ => None,
    }
}
pub(crate) fn answers(
    value: &serde_json::Value,
) -> Result<Vec<(String, SnapshotSupport)>, DiagnosticSet> {
    let fail = || diagnostic::rule_invalid(diagnostic::MAPPING_INVALID, "provider-matrix", None);
    value
        .as_array()
        .ok_or_else(fail)?
        .iter()
        .map(|row| {
            Ok((
                row[0].as_str().ok_or_else(fail)?.to_owned(),
                support(row[1].as_str().ok_or_else(fail)?).ok_or_else(fail)?,
            ))
        })
        .collect()
}
/// Read the complete published version matrix. Missing providers fail closed.
pub fn rows() -> Result<Vec<MatrixRow>, DiagnosticSet> {
    let value = provider::call("storage.matrix", serde_json::json!({}))?;
    let fail = || diagnostic::rule_invalid(diagnostic::MAPPING_INVALID, "provider-matrix", None);
    value["rows"]
        .as_array()
        .ok_or_else(fail)?
        .iter()
        .map(|row| {
            Ok(MatrixRow {
                major: u32::try_from(row["major"].as_u64().ok_or_else(fail)?)
                    .map_err(|_| fail())?,
                answers: answers(&row["answers"])?,
            })
        })
        .collect()
}
/// Find a published major without clamping unsupported versions.
pub fn row_for(major: u32) -> Option<MatrixRow> {
    rows().ok()?.into_iter().find(|row| row.major == major)
}
/// Query a single capability; absent or unavailable answers stay unknown.
pub fn answer(major: u32, capability: &str) -> Option<SnapshotSupport> {
    row_for(major)?
        .answers
        .into_iter()
        .find(|(id, _)| id == capability)
        .map(|(_, support)| support)
}
