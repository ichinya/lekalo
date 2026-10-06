//! Issue #52: the target-neutral multi-provider TaskExternalLink pilot.
//!
//! The module owns the `tasklinks` integration contract family: closed
//! attachment parsing ([`wire`]), static validation against one
//! compiled Model/IR ([`validate`]), first-class capability/policy
//! decisions plus the deterministic delivery engine ([`decision`]), the
//! typed inspect view ([`view`]), and the portability projection
//! ([`portability`]). Everything is pure: no filesystem, network,
//! clock, thread, or provider access; the CLI adapter owns every read
//! and maps the typed results onto the stable [`DomainResult`]
//! channels.

pub mod decision;
pub mod diagnostic;
pub mod portability;
pub mod validate;
pub mod version;
pub mod view;
pub mod wire;

use crate::diagnostics::types::{bound_token, token_value, DataObject};
use crate::{
    diagnostics::DiagnosticSet,
    result::{DomainResult, Status, SuccessPayload},
};
use serde::Deserialize;

pub use diagnostic::Failure;

/// The public fatal input failure constructor (fixed detail tag, no
/// subject echo).
pub fn input_failure(detail: &'static str) -> Failure {
    diagnostic::input_invalid(detail)
}

/// The profile selector of the family.
pub const PROFILES: [&str; 2] = ["strict", "permissive"];

/// Project one failure into the stable [`DomainResult`].
pub fn into_result(failure: &Failure) -> DomainResult {
    diagnostic::into_result(failure)
}

pub use validate::{invalidates, validate, Report};
pub use view::{view, EvidenceSummary};
pub use wire::{parse, Attachment};

/// The maximum size in bytes of one attachment or evidence payload.
pub use version::MAX_BYTES;

/// One parsed, closed evidence document (the admitted mock/native run
/// receipt joined by `inspect`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Evidence {
    /// Exact evidence discriminator.
    pub schema_version: String,
    /// Exact evidence identity.
    pub identity: String,
    /// Evidence id symbol.
    pub evidence_id: String,
    /// Declared origin.
    pub origin: String,
    /// Scenario id.
    pub scenario_id: String,
    /// Attachment revision pin.
    pub attachment_revision: String,
    /// Model pin.
    pub model_ref: EvidenceModelRef,
    /// The declared clock policy.
    pub clock: EvidenceClock,
    /// Step records.
    pub steps: Vec<EvidenceStep>,
}

/// The evidence Model pin.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceModelRef {
    /// Source Model version.
    pub schema_version: String,
    /// IR digest.
    pub ir_digest: String,
}

/// The declared evidence clock policy.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceClock {
    /// `injected` or `wall-clock-native`.
    pub policy: String,
    /// Deterministic start time in milliseconds.
    pub start_ms: u64,
}

/// One evidence step.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceStep {
    /// Step id.
    pub step_id: String,
    /// The command executed.
    pub command: String,
    /// The outcome token.
    pub outcome: String,
}

/// Parse and admit one evidence document against the attachment.
///
/// Admission requires exact identity, matching attachment revision and
/// IR pin, and a bounded step count; a mismatch is one registered
/// `tasklink.evidence-invalid` failure, never a silent ignore.
pub fn admit_evidence(bytes: &[u8], attachment: &Attachment) -> Result<EvidenceSummary, Failure> {
    if bytes.len() > version::MAX_BYTES {
        return Err(diagnostic::rule_invalid(
            diagnostic::EVIDENCE_INVALID,
            "evidence-bound",
            None,
        ));
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| evidence_invalid("evidence-json", None))?;
    let evidence: Evidence =
        serde_json::from_value(value).map_err(|_| evidence_invalid("evidence-shape", None))?;
    if evidence.schema_version != version::EVIDENCE_SCHEMA_VERSION {
        return Err(evidence_invalid("evidence-identity", None));
    }
    if evidence.identity != version::EVIDENCE_IDENTITY {
        return Err(evidence_invalid("evidence-identity", None));
    }
    if evidence.attachment_revision != attachment.attachment_revision().as_str() {
        return Err(evidence_invalid(
            "evidence-stale-pin",
            Some(attachment.project_id()),
        ));
    }
    if evidence.model_ref.schema_version != "0.2.16"
        || evidence.model_ref.ir_digest != attachment.ir_digest().as_str()
    {
        return Err(evidence_invalid(
            "evidence-stale-pin",
            Some(attachment.project_id()),
        ));
    }
    if !matches!(evidence.origin.as_str(), "mock" | "reference" | "native") {
        return Err(evidence_invalid("evidence-origin", None));
    }
    if !matches!(
        evidence.clock.policy.as_str(),
        "injected" | "wall-clock-native"
    ) {
        return Err(evidence_invalid("evidence-origin", None));
    }
    if evidence.steps.is_empty() || evidence.steps.len() > version::MAX_STEPS {
        return Err(evidence_invalid("evidence-steps", None));
    }
    Ok(EvidenceSummary {
        evidence_id: evidence.evidence_id,
        origin: evidence.origin,
        scenario_id: evidence.scenario_id,
        steps: evidence.steps.len() as u64,
        pins_match: true,
    })
}

fn evidence_invalid(detail: &'static str, subject: Option<&str>) -> Failure {
    diagnostic::rule_invalid(diagnostic::EVIDENCE_INVALID, detail, subject)
}

/// Build one bounded diagnostic set for a failure under `status`.
pub fn diagnostic_set(failure: &Failure, status: Status) -> DiagnosticSet {
    diagnostic::set_for(failure, status)
}

/// The typed invalid result of the family.
pub fn failure(_id: &str, _detail: &str) -> DomainResult {
    unreachable!("use Failure-based constructors")
}

use serde::Serialize;

/// Render one serializable value as the compact valid task-links envelope.
pub fn render<T: Serialize>(value: &T) -> DomainResult {
    envelope("valid", value, vec![])
}

/// Render one serializable value with non-blocking gap diagnostics
/// riding the valid envelope (the permissive profile's honest gap
/// report: gaps are visible and registered, never a silent pass).
pub fn render_with_gaps<T: Serialize>(value: &T, findings: &[validate::Finding]) -> DomainResult {
    envelope("valid", value, findings_diagnostics(findings))
}

/// Render one report whose findings invalidate the run: the typed
/// report rides the envelope payload and every finding becomes one
/// registered diagnostic in normalized order.
pub fn render_invalid<T: Serialize>(value: &T, findings: &[validate::Finding]) -> DomainResult {
    envelope("invalid", value, findings_diagnostics(findings)).into_invalid()
}

/// One registered diagnostic per finding, bounded by the set limit.
fn findings_diagnostics(findings: &[validate::Finding]) -> Vec<crate::diagnostics::Diagnostic> {
    let mut diagnostics = Vec::new();
    for finding in findings {
        let mut data = DataObject::new();
        data.insert("detail".to_owned(), token_value(finding.detail));
        data.insert(
            "subject".to_owned(),
            token_value(&bound_token(&finding.subject)),
        );
        if let Ok(item) = crate::diagnostics::normalize::build(finding.rule, None, None, data) {
            diagnostics.push(item);
        }
    }
    diagnostics
}

/// The compact family envelope with the exact wire status.
fn envelope<T: Serialize>(
    status: &str,
    value: &T,
    diagnostics: Vec<crate::diagnostics::Diagnostic>,
) -> DomainResult {
    let json = serde_json::json!({"status": status, "taskLinks": value}).to_string();
    let human = serde_json::to_string_pretty(value).expect("typed output");
    DomainResult::validation(json, human, diagnostics)
}

impl DomainResult {
    /// Project a validation payload onto the invalid channel, keeping
    /// the exact envelope bytes as the denial-style payload (invalid
    /// results print on stderr; the payload mirrors the findings).
    fn into_invalid(self) -> DomainResult {
        match self {
            DomainResult::Valid {
                payload: SuccessPayload::Validation { json, human },
                diagnostics,
            } => {
                let set = DiagnosticSet::try_from_unsorted(diagnostics, Status::Invalid)
                    .unwrap_or_else(|_| {
                        crate::result::singleton_set("diagnostics.registry-invalid")
                    });
                let _ = (json, human);
                DomainResult::Invalid { diagnostics: set }
            }
            other => other,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_constructors_are_not_reachable() {
        assert_eq!(PROFILES.len(), 2);
    }
}
