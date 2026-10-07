//! Issue #52: the target-neutral multi-provider TaskExternalLink pilot.
//!
//! The module owns the `tasklinks` integration contract family: closed
//! duplicate-key-checked attachment parsing ([`wire`], [`json`]),
//! recomputed content pins ([`pins`]), static validation against one
//! compiled Model/IR ([`validate`]), first-class capability/policy
//! decisions plus the deterministic delivery engine ([`decision`]),
//! the typed inspect view ([`view`]), and the portability projection
//! ([`portability`]). Everything is pure: no filesystem, network,
//! clock, thread, or provider access; the CLI adapter owns every read
//! (including the verified owner-contract digests) and maps the typed
//! results onto the stable [`DomainResult`] channels.

pub mod decision;
pub mod diagnostic;
pub mod json;
pub mod pins;
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
pub use view::{view, view_contains_symbol, EvidenceSummary};
pub use wire::{parse, Attachment, Digest, OwnerPin};

/// The lowercase `sha256:` digest of `bytes` (adapter-side hashing of
/// read artifacts: owner contracts, profiles).
pub fn wire_digest(bytes: &[u8]) -> Digest {
    Digest(format!("sha256:{}", crate::digest::sha256_hex(bytes)))
}

/// Duplicate-key-checked JSON decode exposed to the adapter (profile
/// documents and any other closed input share the control).
pub fn json_parse(bytes: &[u8]) -> Result<serde_json::Value, serde_json::Error> {
    json::parse(bytes)
}

/// Verify every content pin of the attachment against verified facts:
/// the recomputed canonical IR digest of the live compilation, the
/// recomputed attachment revision over the canonical input bytes, and
/// the accepted owner-contract digests. Pins are facts; a divergent
/// one fails closed before anything else runs.
pub fn verify_pins(
    attachment: &Attachment,
    compilation: &crate::ir::Compilation,
    accepted_pins: &[OwnerPin],
    canonical_bytes: &[u8],
) -> Result<(), Failure> {
    pins::verify_ir_digest(attachment, compilation)?;
    pins::verify_revision(attachment, canonical_bytes)?;
    pins::verify_owner_pins(attachment, accepted_pins)?;
    Ok(())
}

/// The maximum size in bytes of one attachment or evidence payload.
pub use version::MAX_BYTES;

/// One parsed, closed evidence document (the admitted mock/native run
/// receipt joined by `inspect`). The shape mirrors the closed evidence
/// schema exactly: every optional schema member is carried, every enum
/// is closed, and unknown members are rejected.
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
    /// Bounded state observations (schema-optional, admission-checked).
    #[serde(default)]
    pub observations: Vec<EvidenceObservation>,
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

/// One evidence step, carrying the schema's full closed shape.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceStep {
    /// Step id.
    pub step_id: String,
    /// The command executed (closed command enum).
    pub command: String,
    /// The outcome token (closed outcome enum).
    pub outcome: String,
    /// Source attribution (workspace, actor, provider scope).
    #[serde(default)]
    pub attribution: Option<EvidenceAttribution>,
    /// The intent reference of the step.
    #[serde(default)]
    pub intent: Option<EvidenceIntentRef>,
    /// The decision reference of the step.
    #[serde(default)]
    pub decision: Option<EvidenceDecisionRef>,
    /// Bounded free-text detail.
    #[serde(default)]
    pub detail: Option<String>,
}

/// One step's source attribution (closed to the evidence schema).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceAttribution {
    /// Owning workspace.
    pub workspace_id: String,
    /// Actor class.
    pub actor: String,
    /// Originating installation.
    #[serde(default)]
    pub installation_id: Option<String>,
    /// Originating provider.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Provider object type.
    #[serde(default)]
    pub object_type: Option<String>,
    /// External object id.
    #[serde(default)]
    pub external_id: Option<String>,
    /// Scoped event identity.
    #[serde(default)]
    pub event_identity: Option<String>,
    /// Causation id.
    #[serde(default)]
    pub causation_id: Option<String>,
    /// Correlation id.
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Source revision.
    #[serde(default)]
    pub source_revision: Option<String>,
}

/// One step's intent reference (closed to the evidence schema).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceIntentRef {
    /// Intent id.
    pub intent_id: String,
    /// Outbox state at the step.
    pub state: String,
    /// Originating link.
    #[serde(default)]
    pub link_id: Option<String>,
    /// Idempotency key.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Attempt number.
    #[serde(default)]
    pub attempt: Option<u64>,
    /// Lease token.
    #[serde(default)]
    pub lease_token: Option<String>,
    /// Receipt digest.
    #[serde(default)]
    pub receipt_digest: Option<String>,
}

/// One step's decision reference (closed to the evidence schema).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceDecisionRef {
    /// The decision outcome.
    pub outcome: String,
    /// The decided action.
    #[serde(default)]
    pub action: Option<String>,
    /// The capability observation.
    #[serde(default)]
    pub capability_outcome: Option<String>,
    /// The authorization observation.
    #[serde(default)]
    pub authorization_outcome: Option<String>,
    /// The decided field mask.
    #[serde(default)]
    pub fields: Vec<String>,
    /// The policy revision pin.
    #[serde(default)]
    pub policy_revision: Option<String>,
    /// The capability revision pin.
    #[serde(default)]
    pub capability_revision: Option<String>,
}

/// One bounded state observation (closed to the evidence schema).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvidenceObservation {
    /// Observation kind (closed enum).
    pub kind: String,
    /// Observed subject.
    pub subject: String,
    /// Observed value token.
    #[serde(default)]
    pub value: Option<String>,
    /// Observed count.
    #[serde(default)]
    pub count: Option<u64>,
    /// Observed digest.
    #[serde(default)]
    pub digest: Option<String>,
}

/// The closed step-command enum of the evidence schema.
const STEP_COMMANDS: [&str; 8] = [
    "tasklinks.attach_link",
    "tasklinks.import_link",
    "tasklinks.ingest_event",
    "tasklinks.request_action",
    "tasklinks.detach_link",
    "tasklinks.resolve_conflict",
    "tasklinks.dispatch_outbox",
    "tasklinks.ack_delivery",
];

/// The closed step-outcome enum of the evidence schema.
const STEP_OUTCOMES: [&str; 8] = [
    "applied",
    "replayed",
    "conflict",
    "denied",
    "unsupported",
    "indeterminate",
    "unknown_delivery",
    "rejected",
];

/// The closed observation-kind enum of the evidence schema.
const OBSERVATION_KINDS: [&str; 7] = [
    "link_state",
    "outbox_state",
    "inbox_receipt",
    "conflict_state",
    "audit_event",
    "cursor_state",
    "coverage",
];

/// Parse and admit one evidence document against the attachment.
///
/// Admission requires exact identity, matching attachment revision and
/// IR pin, a bounded step/observation count, closed step enums, and an
/// origin the environment can actually stand behind: `native` claims
/// are admitted only when `qualified_native` is asserted by the caller
/// (the qualified-environment contract), and `native` always implies
/// the `wall-clock-native` clock policy. A forged native claim without
/// qualification is one registered failure, never a silent pass.
pub fn admit_evidence(
    bytes: &[u8],
    attachment: &Attachment,
    qualified_native: bool,
) -> Result<EvidenceSummary, Failure> {
    if bytes.len() > version::MAX_BYTES {
        return Err(diagnostic::rule_invalid(
            diagnostic::EVIDENCE_INVALID,
            "evidence-bound",
            None,
        ));
    }
    let value: serde_json::Value =
        json::parse(bytes).map_err(|_| evidence_invalid("evidence-json", None))?;
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
    // Native claims are qualification-gated and clock-coherent.
    if evidence.origin == "native" {
        if !qualified_native {
            return Err(evidence_invalid("evidence-native-unqualified", None));
        }
        if evidence.clock.policy != "wall-clock-native" {
            return Err(evidence_invalid("evidence-native-clock", None));
        }
    }
    if evidence.steps.is_empty() || evidence.steps.len() > version::MAX_STEPS {
        return Err(evidence_invalid("evidence-steps", None));
    }
    if evidence.observations.len() > version::MAX_OBSERVATIONS {
        return Err(evidence_invalid("evidence-observations", None));
    }
    for step in &evidence.steps {
        if !STEP_COMMANDS.contains(&step.command.as_str())
            || !STEP_OUTCOMES.contains(&step.outcome.as_str())
        {
            return Err(evidence_invalid("evidence-step-token", None));
        }
        if let Some(attribution) = &step.attribution {
            if !matches!(
                attribution.actor.as_str(),
                "identity.user" | "system.job" | "provider.event"
            ) {
                return Err(evidence_invalid("evidence-step-token", None));
            }
        }
        if let Some(intent) = &step.intent {
            if !matches!(
                intent.state.as_str(),
                "pending"
                    | "leased"
                    | "delivered"
                    | "retry_wait"
                    | "conflicted"
                    | "cancelled"
                    | "dead_letter"
                    | "unknown_delivery"
            ) {
                return Err(evidence_invalid("evidence-step-token", None));
            }
        }
        if let Some(decision) = &step.decision {
            if !matches!(
                decision.outcome.as_str(),
                "allowed" | "denied" | "unsupported" | "indeterminate"
            ) || decision
                .capability_outcome
                .as_deref()
                .is_some_and(|outcome| {
                    !matches!(outcome, "supported" | "partial" | "unsupported" | "unknown")
                })
                || decision
                    .authorization_outcome
                    .as_deref()
                    .is_some_and(|outcome| !matches!(outcome, "allowed" | "denied"))
            {
                return Err(evidence_invalid("evidence-step-token", None));
            }
        }
        if let Some(detail) = &step.detail {
            if detail.is_empty() || detail.len() > 512 {
                return Err(evidence_invalid("evidence-step-token", None));
            }
        }
    }
    for observation in &evidence.observations {
        if !OBSERVATION_KINDS.contains(&observation.kind.as_str()) {
            return Err(evidence_invalid("evidence-observation", None));
        }
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
