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

/// The engine's real outbox states: the evidence projection mirrors the
/// delivery engine exactly, so every durable state (including the two
/// uncertainty states) is representable and nothing else is admitted.
const OUTBOX_STATES: [&str; 9] = [
    "pending",
    "leased",
    "accepted_pending_ack",
    "delivered",
    "retry_wait",
    "conflicted",
    "cancelled",
    "dead_letter",
    "unknown_delivery",
];

/// One bounded `token` per the evidence schema: 1..=256 Unicode
/// scalars. JSON Schema string lengths count characters, never bytes,
/// so the runtime bound is char-based and schema-valid Unicode input
/// is never refused for its UTF-8 size.
fn is_evidence_token(text: &str) -> bool {
    let chars = text.chars().count();
    (1..=256).contains(&chars)
}

/// One qualified symbol per the evidence schema (the same closed shape
/// the public portability rows use). Everything the summary may echo is
/// constrained to this grammar first: free text never reaches stdout.
fn is_evidence_symbol(text: &str) -> bool {
    text.chars().count() <= 191 && wire::is_qualified_symbol(text)
}

/// One `sha256:` digest per the evidence schema.
fn is_evidence_digest(text: &str) -> bool {
    wire::Digest::parse(text).is_some()
}

/// Parse and admit one evidence document against the attachment and
/// the live compilation.
///
/// Admission is schema-faithful in both directions: every closed
/// schema constraint (symbols, tokens, digests, enums, bounds) is
/// mirrored exactly, so a schema-valid document is never refused for
/// shape and a schema-invalid document is never admitted. On top of
/// the shape, the joined semantics are admitted too: the scenario must
/// be declared in the compilation, decision references must be
/// internally consistent (an allowed outcome cannot carry an
/// unsupported capability or a denied authorization), an allowed
/// decision may never touch the protected planning fields, step ids
/// are unique, and coverage observations cannot claim more than the
/// recorded steps. The origin must be one the environment can actually
/// stand behind: `native` claims are admitted only when
/// `qualified_native` is asserted by the caller (the
/// qualified-environment contract), and `native` always implies the
/// `wall-clock-native` clock policy. A forged native claim without
/// qualification is one registered failure, never a silent pass.
pub fn admit_evidence(
    bytes: &[u8],
    attachment: &Attachment,
    compilation: &crate::ir::Compilation,
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
    // The closed evidence schema has no nullable member anywhere: every
    // present member is a typed string, number, array, or object. An
    // explicit `null` is therefore schema-invalid in every position —
    // an absent member stays the only way to omit one, and the runtime
    // never conflates the two the way a bare serde `Option` would.
    if value_fills_null(&value) {
        return Err(evidence_invalid("evidence-shape", None));
    }
    let evidence: Evidence =
        serde_json::from_value(value).map_err(|_| evidence_invalid("evidence-shape", None))?;
    if evidence.schema_version != version::EVIDENCE_SCHEMA_VERSION
        || evidence.identity != version::EVIDENCE_IDENTITY
    {
        // Same-family different-version evidence is the accepted exit-5
        // class, coherent with the attachment family convention; a
        // malformed identity pair under the accepted discriminator
        // stays plain invalid (exit 1).
        const FAMILY_STEM: &str = "lekalo/task-links-evidence/v";
        let version_skew = evidence.schema_version != version::EVIDENCE_SCHEMA_VERSION
            && evidence
                .schema_version
                .strip_prefix(FAMILY_STEM)
                .is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
                });
        if version_skew {
            return Err(evidence_invalid("evidence-version-unsupported", None));
        }
        return Err(evidence_invalid("evidence-identity", None));
    }
    // Public evidence ids are qualified symbols before anything can
    // ever echo them: unconstrained strings are refused here, so no
    // private marker can ride into inspect/portability output.
    if !is_evidence_symbol(&evidence.evidence_id) || !is_evidence_symbol(&evidence.scenario_id) {
        return Err(evidence_invalid("evidence-symbol", None));
    }
    if evidence.attachment_revision != attachment.attachment_revision().as_str() {
        return Err(evidence_invalid(
            "evidence-stale-pin",
            Some(attachment.project_id()),
        ));
    }
    if evidence.model_ref.schema_version != version::MODEL_VERSION
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
    // The contribution switches are read, not decorative: evidence
    // witnesses scenario runs, so with the scenarios contribution
    // disabled the attachment declares no evidence surface at all.
    if !attachment.contributions().scenarios {
        return Err(evidence_invalid("contribution-disabled", None));
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
    // The scenario must be declared by the compilation: evidence can
    // never witness an invented scenario.
    let declared_scenarios: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        compilation
            .project
            .definitions
            .iter()
            .filter_map(|definition| match definition {
                crate::ir::Definition::Scenario(scenario) => Some((
                    scenario.id.as_str().to_owned(),
                    scenario
                        .covers
                        .iter()
                        .map(|symbol| symbol.as_str().to_owned())
                        .collect::<std::collections::BTreeSet<String>>(),
                )),
                _ => None,
            })
            .collect();
    if !declared_scenarios.contains_key(&evidence.scenario_id) {
        return Err(evidence_invalid("evidence-scenario-unresolved", None));
    }
    if evidence.steps.is_empty() || evidence.steps.len() > version::MAX_STEPS {
        return Err(evidence_invalid("evidence-steps", None));
    }
    if evidence.observations.len() > version::MAX_OBSERVATIONS {
        return Err(evidence_invalid("evidence-observations", None));
    }
    let mut seen_steps = std::collections::BTreeSet::new();
    for step in &evidence.steps {
        if !is_evidence_token(&step.step_id) || !seen_steps.insert(step.step_id.as_str()) {
            return Err(evidence_invalid("evidence-step-token", None));
        }
        if !STEP_COMMANDS.contains(&step.command.as_str())
            || !STEP_OUTCOMES.contains(&step.outcome.as_str())
        {
            return Err(evidence_invalid("evidence-step-token", None));
        }
        if let Some(attribution) = &step.attribution {
            if !matches!(
                attribution.actor.as_str(),
                "identity.user" | "system.job" | "provider.event"
            ) || !is_evidence_token(&attribution.workspace_id)
            {
                return Err(evidence_invalid("evidence-step-token", None));
            }
            for token in [
                &attribution.installation_id,
                &attribution.provider_id,
                &attribution.object_type,
                &attribution.external_id,
                &attribution.event_identity,
                &attribution.causation_id,
                &attribution.correlation_id,
                &attribution.source_revision,
            ]
            .into_iter()
            .flatten()
            {
                if !is_evidence_token(token) {
                    return Err(evidence_invalid("evidence-step-token", None));
                }
            }
        }
        if let Some(intent) = &step.intent {
            // A refused or uncertain step produced no delivery: a
            // delivered state or a receipt digest on a step whose
            // outcome is neither applied nor replayed is an invented
            // receipt, and this admission refuses it.
            if (intent.state.as_str() == "delivered" || intent.receipt_digest.is_some())
                && !matches!(step.outcome.as_str(), "applied" | "replayed")
            {
                return Err(evidence_invalid("evidence-receipt-invented", None));
            }
            if !OUTBOX_STATES.contains(&intent.state.as_str())
                || !is_evidence_token(&intent.intent_id)
                || intent.attempt.is_some_and(|attempt| attempt > 64)
            {
                return Err(evidence_invalid("evidence-step-token", None));
            }
            for token in [
                &intent.link_id,
                &intent.idempotency_key,
                &intent.lease_token,
            ]
            .into_iter()
            .flatten()
            {
                if !is_evidence_token(token) {
                    return Err(evidence_invalid("evidence-step-token", None));
                }
            }
            if let Some(digest) = &intent.receipt_digest {
                if !is_evidence_digest(digest) {
                    return Err(evidence_invalid("evidence-step-token", None));
                }
            }
        }
        if let Some(decision) = &step.decision {
            // The decided action is a qualified symbol per the schema,
            // never free text.
            if let Some(action) = &decision.action {
                if !is_evidence_symbol(action) {
                    return Err(evidence_invalid("evidence-step-token", None));
                }
            }
            // An allowed decision is a WITNESSED decision: it must
            // carry both independent observations (capability and
            // authorization), because a self-declared allow without
            // observations is exactly the fabricated authority this
            // admission refuses.
            if decision.outcome == "allowed"
                && (decision.capability_outcome.is_none()
                    || decision.authorization_outcome.is_none())
            {
                return Err(evidence_invalid("evidence-observation-missing", None));
            }
            if !is_decision_consistent(decision)
                || decision
                    .fields
                    .iter()
                    .any(|field| !is_evidence_token(field))
            {
                return Err(evidence_invalid("evidence-step-token", None));
            }
            if decision.fields.len() > 64 {
                return Err(evidence_invalid("evidence-step-token", None));
            }
            // An allowed decision never touches the protected planning
            // fields, live or in evidence.
            if decision.outcome == "allowed"
                && decision
                    .fields
                    .iter()
                    .any(|field| validate::PROTECTED_PLANNING_FIELDS.contains(&field.as_str()))
            {
                return Err(evidence_invalid("evidence-protected-field", None));
            }
            for digest in [&decision.policy_revision, &decision.capability_revision]
                .into_iter()
                .flatten()
            {
                if !is_evidence_digest(digest) {
                    return Err(evidence_invalid("evidence-step-token", None));
                }
            }
        }
        // The recorded detail is bounded free text; it is never echoed
        // into any public surface. The bound is schema-correct: 512
        // Unicode scalars, not bytes.
        if let Some(detail) = &step.detail {
            if detail.chars().count() > 512 {
                return Err(evidence_invalid("evidence-step-token", None));
            }
        }
    }
    for observation in &evidence.observations {
        if !OBSERVATION_KINDS.contains(&observation.kind.as_str())
            || !is_evidence_token(&observation.subject)
        {
            return Err(evidence_invalid("evidence-observation", None));
        }
        if let Some(token) = &observation.value {
            if !is_evidence_token(token) {
                return Err(evidence_invalid("evidence-observation", None));
            }
        }
        if observation.count.is_some_and(|count| count > 1_000_000) {
            return Err(evidence_invalid("evidence-observation", None));
        }
        // Coverage cannot claim more than the recorded steps prove.
        if observation.kind == "coverage"
            && observation
                .count
                .is_some_and(|count| count > evidence.steps.len() as u64)
        {
            return Err(evidence_invalid("evidence-coverage", None));
        }
        if let Some(digest) = &observation.digest {
            if !is_evidence_digest(digest) {
                return Err(evidence_invalid("evidence-observation", None));
            }
        }
    }
    // The run witnesses the scenario it names: at least one successful
    // (applied or replayed) step must execute a symbol the declared
    // scenario covers. A run of refused or unrelated steps proves
    // nothing about its scenario, whatever id it borrows.
    let covers = &declared_scenarios[&evidence.scenario_id];
    let executed: std::collections::BTreeSet<String> = evidence
        .steps
        .iter()
        .filter(|step| matches!(step.outcome.as_str(), "applied" | "replayed"))
        .map(|step| step.command.clone())
        .collect();
    if !executed.iter().any(|command| covers.contains(command)) {
        return Err(evidence_invalid("evidence-scenario-unwitnessed", None));
    }
    Ok(EvidenceSummary {
        // The producer-supplied id never reaches a public surface: the
        // admitted projection carries an opaque synthetic reference
        // derived from the exact admitted bytes, so even a
        // grammar-valid confidential marker cannot ride through inspect
        // or portability output. The id is still grammar-checked above
        // (garbage refuses); admission and echo are separate powers.
        // The digest segment is prefixed with a fixed alphabetic
        // token: every public symbol segment must begin with a letter,
        // and a bare hex digest starts with a digit for 10/16 of all
        // serializations (including semantically irrelevant whitespace
        // differences), which would emit schema-invalid output.
        evidence_id: format!(
            "tasklinks.evidence.admitted.ev{}",
            &crate::digest::sha256_hex(bytes)[..16]
        ),
        origin: evidence.origin,
        scenario_id: evidence.scenario_id,
        steps: evidence.steps.len() as u64,
        executed,
        pins_match: true,
    })
}

/// Whether one decision reference is internally consistent: the
/// composed outcome must follow from the capability and authorization
/// observations, exactly like the engine's own decision function.
fn is_decision_consistent(decision: &EvidenceDecisionRef) -> bool {
    if !matches!(
        decision.outcome.as_str(),
        "allowed" | "denied" | "unsupported" | "indeterminate"
    ) {
        return false;
    }
    if let Some(capability) = &decision.capability_outcome {
        if !matches!(
            capability.as_str(),
            "supported" | "partial" | "unsupported" | "unknown"
        ) {
            return false;
        }
    }
    if let Some(authorization) = &decision.authorization_outcome {
        if !matches!(authorization.as_str(), "allowed" | "denied") {
            return false;
        }
    }
    match decision.outcome.as_str() {
        // Allowed requires both observations to permit.
        "allowed" => {
            decision.authorization_outcome.as_deref() != Some("denied")
                && matches!(
                    decision.capability_outcome.as_deref(),
                    None | Some("supported") | Some("partial")
                )
        }
        // Denied is a known denial.
        "denied" => decision.authorization_outcome.as_deref() == Some("denied"),
        // Unsupported is a capability refusal.
        "unsupported" => decision.capability_outcome.as_deref() == Some("unsupported"),
        // Indeterminate means undecidable input (unknown capability).
        "indeterminate" => decision.capability_outcome.as_deref() == Some("unknown"),
        _ => false,
    }
}

fn evidence_invalid(detail: &'static str, subject: Option<&str>) -> Failure {
    diagnostic::rule_invalid(diagnostic::EVIDENCE_INVALID, detail, subject)
}

/// Whether any JSON member anywhere in one document carries an explicit
/// `null`: the closed evidence schema admits no nullable member, so one
/// explicit null is a schema-invalid document.
fn value_fills_null(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Array(items) => items.iter().any(value_fills_null),
        serde_json::Value::Object(map) => map.values().any(value_fills_null),
        _ => false,
    }
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

    fn decision(outcome: &str, capability: Option<&str>, authorization: Option<&str>) -> bool {
        is_decision_consistent(&EvidenceDecisionRef {
            outcome: outcome.to_owned(),
            action: None,
            capability_outcome: capability.map(str::to_owned),
            authorization_outcome: authorization.map(str::to_owned),
            fields: vec![],
            policy_revision: None,
            capability_revision: None,
        })
    }

    #[test]
    fn decision_consistency_mirrors_the_engine_decision_function() {
        assert!(decision("allowed", Some("supported"), Some("allowed")));
        assert!(decision("allowed", Some("partial"), Some("allowed")));
        // A fabricated receipt: allowed with an unsupported capability
        // or a denied authorization contradicts the engine.
        assert!(!decision("allowed", Some("unsupported"), Some("allowed")));
        assert!(!decision("allowed", Some("supported"), Some("denied")));
        assert!(decision("denied", Some("supported"), Some("denied")));
        assert!(!decision("denied", Some("supported"), Some("allowed")));
        assert!(decision(
            "unsupported",
            Some("unsupported"),
            Some("allowed")
        ));
        assert!(!decision("unsupported", Some("supported"), Some("allowed")));
        assert!(decision("indeterminate", Some("unknown"), Some("allowed")));
        assert!(!decision(
            "indeterminate",
            Some("supported"),
            Some("allowed")
        ));
    }

    #[test]
    fn evidence_symbols_reject_everything_unqualified() {
        assert!(is_evidence_symbol("tasklinks.evidence.two_providers"));
        assert!(!is_evidence_symbol("SYNTHETIC-PRIVATE-MARKER"));
        assert!(!is_evidence_symbol("no-dashes-allowed"));
        assert!(!is_evidence_symbol(""));
        assert!(!is_evidence_symbol("a.b.c.d.e"));
        let long = format!("a.{}", "b".repeat(200));
        assert!(!is_evidence_symbol(&long), "over the 191-byte bound");
        assert!(!is_evidence_digest("sha256:zz"));
        assert!(is_evidence_digest(&format!("sha256:{}", "a".repeat(64))));
    }

    #[test]
    fn schema_and_runtime_bounds_count_characters_not_bytes() {
        // JSON Schema maxLength counts characters: a 256-character
        // token of two-byte scalars is schema-valid, so the runtime
        // must admit it too (256 chars = 512 UTF-8 bytes).
        let two_byte = "é".repeat(256);
        assert_eq!(two_byte.len(), 512);
        assert!(is_evidence_token(&two_byte));
        // One character over the bound refuses, byte length regardless.
        assert!(!is_evidence_token(&"é".repeat(257)));
        assert!(!is_evidence_token(&"a".repeat(257)));
        // The 512-character detail bound behaves the same way.
        assert!("é".repeat(512).chars().count() <= 512);
        // A schema-valid confidential marker is symbol-legal: that is
        // exactly why admission must not double as an echo license.
        assert!(is_evidence_symbol(
            "private_consumer_name.synthetic_secret_52"
        ));
    }
}
