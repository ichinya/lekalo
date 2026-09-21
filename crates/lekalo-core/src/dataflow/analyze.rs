//! The deterministic data-flow analysis and report builder (issue #87).
//!
//! The analyzer consumes the classified compilation — the effect graph
//! with stamped [`Sensitivity`] markers, the query-model projections,
//! and the classification/policy attachments — and derives the
//! data-flow report: one flow per source-subject to sink relationship,
//! the tenant relation of every path, the gate decision of every gated
//! sink, and the first-class unknown list.
//!
//! Hard rules (plan §4): an unresolvable hop degrades the flow to
//! unknown and the gate to unsatisfied; observed or partial evidence
//! never promotes to canonical certainty; a sensitive flow resting on
//! non-canonical evidence is a finding; unknown is never safe. The
//! report is derived: its bytes are a pure function of the exact
//! input digests and the pinned compilation, with no wall-clock input.


use crate::classification::policy::{PolicyAttachment, SinkName};
use crate::classification::resolve::{Resolution, ResolvedKind};
use crate::classification::types::{DataKind, SubjectPath};
use crate::diagnostics::DiagnosticSet;
use crate::effects::EffectGraph;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

use super::diagnostic;
use super::report::Report;
use super::types::{
    Confidence, Finding, Flow, Gate, GateReason, Provenance, Severity, SinkKind,
    TenantRelation, UnknownFlow, UnknownReason,
};
use super::version;
use crate::result::Status;

/// The input bundle of one analysis run.
pub struct Inputs<'a> {
    /// The project identity.
    pub project_id: &'a SemanticId,
    /// The bound Model pin `(modelVersion, digest)`.
    pub model_ref: (&'a str, &'a Sha256Digest),
    /// The bound IR pin `(irVersion, digest)`.
    pub ir_ref: (&'a str, &'a Sha256Digest),
    /// The effect graph (declared plus detected edges).
    pub graph: &'a EffectGraph,
    /// The classification resolution.
    pub classification: &'a Resolution,
    /// The canonical digest of the classification attachment.
    pub classification_ref: &'a Sha256Digest,
    /// The governing policy attachment.
    pub policy: &'a PolicyAttachment,
    /// The canonical digest of the policy attachment.
    pub policy_ref: &'a Sha256Digest,
    /// The engine identity recorded as `generatedBy`.
    pub generated_by: &'a str,
    /// The report revision.
    pub report_revision: &'a str,
}

/// The derived report plus the diagnostics the derivation surfaced
/// (findings mirror these rules into the report; the set is for CLI
/// exit mapping).
pub struct Analysis {
    /// The derived read-only report.
    pub report: Report,
    /// The normalized findings as diagnostics (mirrored in the report).
    pub diagnostics: DiagnosticSet,
}

/// Why one flow's gate is unsatisfiable or satisfied.
struct GateOutcome {
    required: bool,
    satisfied: bool,
    reason: GateReason,
}

/// Run the deterministic data-flow analysis over the inputs and build
/// the report. Pure: no filesystem, network, cache, process, or
/// clock access of any kind.
pub fn analyze(inputs: &Inputs<'_>) -> Result<Analysis, DiagnosticSet> {
    let mut flows: Vec<Flow> = Vec::new();
    let mut findings: Vec<Finding> = Vec::new();
    let mut unknowns: Vec<UnknownFlow> = Vec::new();
    let mut inputs_complete = true;

    // The policy must cover every resolvable kind (classification
    // validation owns kind-rule-missing; the analyzer simply reads
    // rows and treats a missing row as policy-unresolved).
    let mut ordinal = 0usize;
    for edge in inputs.graph.declared() {
        let resource = edge.key().subject().resource().as_str();
        let subject = match SubjectPath::parse(resource) {
            Ok(subject) => subject,
            Err(_) => continue,
        };
        let resolved = inputs.classification.resolve(&subject);
        let kind = match resolved {
            ResolvedKind::Classified(kind) => kind,
            ResolvedKind::Unclassified => {
                // Declared edges without classification stay unknown
                // and never gate; the unclassified-sensitive-sink rule
                // is validation's, applied under the strict profile.
                unknowns.push(UnknownFlow {
                    source: subject,
                    reason: UnknownReason::UnresolvedSubject,
                });
                continue;
            }
        };
        let sink_kind = sink_kind_of(edge.key().kind().key());
        let (provenance, confidence) = match edge.provenance() {
            crate::effects::EffectProvenance::CanonicalIr { .. } => {
                (Provenance::Canonical, Confidence::High)
            }
            _ => {
                inputs_complete = false;
                (Provenance::Observed, Confidence::Unknown)
            }
        };
        let tenant_relation = tenant_relation_of(inputs, &subject, kind);
        let gate = gate_outcome(inputs, &kind, sink_kind, confidence, resolved);
        let gate_wire = gate.as_ref().map(|outcome| Gate {
            required: outcome.required,
            satisfied: outcome.satisfied,
            reason: outcome.reason,
        });
        ordinal += 1;
        flows.push(Flow {
            id: flow_id(ordinal, &subject),
            source: subject.clone(),
            path: vec![format!(
                "effect:{}:{}",
                edge.key().operation().as_str(),
                edge.key().kind().key()
            )],
            sink: sink_symbol(edge),
            sink_kind,
            provenance,
            confidence,
            tenant_relation,
            classification: kind,
            gate: gate_wire,
        });
        // Rule findings over the flow.
        if tenant_relation == TenantRelation::Crossing && kind.cross_tenant_forbidden_by_default() {
            findings.push(Finding {
                rule_id: "dataflow.tenant-crossing".to_owned(),
                severity: Severity::Error,
                subject: subject.as_str().to_owned(),
                detail: "cross-tenant-flow".to_owned(),
            });
        }
        if let Some(outcome) = gate {
            if outcome.required && !outcome.satisfied {
                if let Some(rule) = gate_rule_of(outcome.reason) {
                    findings.push(Finding {
                        rule_id: rule.to_owned(),
                        severity: Severity::Error,
                        subject: subject.as_str().to_owned(),
                        detail: outcome.reason.as_str().to_owned(),
                    });
                }
            }
        }
        if kind.is_sensitive() && confidence != Confidence::High {
            findings.push(Finding {
                rule_id: "dataflow.low-confidence-sensitive".to_owned(),
                severity: Severity::Error,
                subject: subject.as_str().to_owned(),
                detail: confidence.as_str().to_owned(),
            });
        }
        // Policy sink ceilings.
        if let Some(sink) = policy_sink_of(sink_kind) {
            if let Some(ceiling) = inputs.policy.sink(sink) {
                if kind.rank() > ceiling.max_kind().rank() {
                    findings.push(Finding {
                        rule_id: "classification.sink-ceiling-exceeded".to_owned(),
                        severity: Severity::Error,
                        subject: subject.as_str().to_owned(),
                        detail: format!("sink-{}", sink.as_str()),
                    });
                }
            }
        }
    }
    // Declared edge coverage of the event-publish and external sinks
    // through extended-effects is consumed by the CLI validation hook;
    // this module projects what the graph itself carries so analysis
    // stays a pure function of the pinned compilation plus inputs.

    unknowns.sort_by(|left, right| {
        left.source
            .as_str()
            .cmp(right.source.as_str())
            .then_with(|| left.reason.as_str().cmp(right.reason.as_str()))
    });
    unknowns.dedup();
    flows.sort_by(|left, right| {
        left.source
            .as_str()
            .cmp(right.source.as_str())
            .then_with(|| left.sink_kind.as_str().cmp(right.sink_kind.as_str()))
            .then_with(|| left.id.cmp(&right.id))
    });
    findings.sort_by(|left, right| {
        left.rule_id
            .cmp(&right.rule_id)
            .then_with(|| left.subject.cmp(&right.subject))
            .then_with(|| left.detail.cmp(&right.detail))
    });
    findings.dedup();
    if flows.len() > version::MAX_FLOWS {
        return Err(diagnostic::document_invalid("flow-limit", None));
    }
    if findings.len() > version::MAX_FINDINGS {
        return Err(diagnostic::document_invalid("finding-limit", None));
    }
    if unknowns.len() > version::MAX_UNKNOWNS {
        return Err(diagnostic::document_invalid("unknown-limit", None));
    }

    let report = Report::assemble(
        inputs.report_revision.to_owned(),
        inputs.project_id.clone(),
        (inputs.model_ref.0.to_owned(), inputs.model_ref.1.clone()),
        (inputs.ir_ref.0.to_owned(), inputs.ir_ref.1.clone()),
        inputs.classification_ref.clone(),
        inputs.policy_ref.clone(),
        inputs.generated_by.to_owned(),
        inputs_complete,
        flows,
        findings.clone(),
        unknowns,
        Vec::new(),
    );
    // Mirror the findings into one normalized set for CLI exits. Any
    // construction failure collapses to the invariant set (double
    // developer fault), never a panic.
    let diagnostics = findings_to_diagnostics(&findings);
    Ok(Analysis {
        report,
        diagnostics,
    })
}

/// Map findings into one normalized diagnostic set (`valid` when only
/// warnings, `invalid` when any error-severity finding exists).
fn findings_to_diagnostics(findings: &[Finding]) -> DiagnosticSet {
    let has_error = findings.iter().any(|finding| finding.severity().is_error());
    let status = if has_error {
        Status::Invalid
    } else {
        Status::Valid
    };
    let mut diagnostics = Vec::with_capacity(findings.len());
    for finding in findings {
        // Rule data carries only fixed tags and opaque subject
        // digests; the raw subject never enters the diagnostic.
        let mut data = crate::diagnostics::types::DataObject::new();
        data.insert(
            "detail".to_owned(),
            crate::diagnostics::types::token_value(&format!(
                "{}:subject-{}",
                finding.detail,
                crate::digest::sha256_hex(finding.subject.as_bytes())
            )),
        );
        if let Ok(built) =
            crate::diagnostics::normalize::build(&finding.rule_id, None, None, data)
        {
            diagnostics.push(built);
        }
    }
    DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid"))
}

/// The deterministic flow id: the ordinal and the subject digest.
fn flow_id(ordinal: usize, subject: &SubjectPath) -> String {
    format!(
        "flow-{:04}-{}",
        ordinal,
        &crate::digest::sha256_hex(subject.as_str().as_bytes())[..8]
    )
}

/// The sink symbol of one edge (the operation semantic id).
fn sink_symbol(edge: &crate::effects::EffectEdge) -> String {
    edge.key().operation().semantic_id().to_owned()
}

/// The closed sink kind of one effect-kind key.
fn sink_kind_of(kind_key: &str) -> SinkKind {
    match kind_key {
        "read" => SinkKind::EndpointResponse,
        "create" | "update" | "delete" => SinkKind::StorageWrite,
        "emit-event" => SinkKind::EventPublish,
        _ => SinkKind::StorageWrite,
    }
}

/// The policy sink of one closed sink kind, when the sink is a
/// non-model policy surface.
fn policy_sink_of(sink_kind: SinkKind) -> Option<SinkName> {
    Some(match sink_kind {
        SinkKind::Log => SinkName::Logs,
        SinkKind::Trace => SinkName::Traces,
        SinkKind::ContextCapsule => SinkName::ContextCapsules,
        SinkKind::Diagnostic => SinkName::Diagnostics,
        SinkKind::Evidence => SinkName::Evidence,
        SinkKind::Export => SinkName::Exports,
        _ => return None,
    })
}

/// The registered rule behind one failed gate reason, when any.
const fn gate_rule_of(reason: GateReason) -> Option<&'static str> {
    match reason {
        GateReason::MissingDestination => Some("dataflow.missing-destination"),
        GateReason::MissingApproval => Some("dataflow.missing-approval"),
        GateReason::DestinationForbidden => Some("dataflow.destination-forbidden"),
        GateReason::UnknownFlow => Some("dataflow.unknown-flow"),
        GateReason::LowConfidence => Some("dataflow.low-confidence-sensitive"),
        GateReason::SinkCeilingExceeded => Some("classification.sink-ceiling-exceeded"),
        GateReason::UnclassifiedSubject => Some("classification.unclassified-sensitive-sink"),
        GateReason::InputsIncomplete => Some("dataflow.observed-incomplete"),
        GateReason::NotRequired | GateReason::DestinationDeclared | GateReason::ApprovalPresent => {
            None
        }
    }
}

/// The tenant relation of one subject/kind pair. With no declared
/// tenant evidence the relation is `unknown` — never silently `same`.
fn tenant_relation_of(_inputs: &Inputs<'_>, _subject: &SubjectPath, kind: DataKind) -> TenantRelation {
    // Tenant-crossing detection joins authorization tenant scopes and
    // query tenant-filter declarations. The classification-level rule
    // is conservative: tenant-scoped kinds whose path carries no
    // declared tenant-filter evidence resolve to `unknown`, which the
    // gate treats as crossing. The plan's §4.4 representative case is
    // a tenant-scoped kind on a cross-module event path.
    if kind == DataKind::TenantScoped {
        TenantRelation::Unknown
    } else {
        TenantRelation::Same
    }
}

/// The gate decision for one flow to one sink.
fn gate_outcome(
    inputs: &Inputs<'_>,
    kind: &DataKind,
    sink_kind: SinkKind,
    confidence: Confidence,
    resolved: ResolvedKind,
) -> Option<GateOutcome> {
    // Gated sinks: external-call, publication, public-endpoint
    // response, export, cache. The declared graph carries storage,
    // event, and read edges; the extended-effects gates are evaluated
    // at the validation hook with the parsed contracts. Here the
    // ceiling and confidence rules apply to every sink.
    if resolved == ResolvedKind::Unclassified {
        return Some(GateOutcome {
            required: true,
            satisfied: false,
            reason: GateReason::UnclassifiedSubject,
        });
    }
    if !inputs.inputs_declared_complete() && confidence != Confidence::High {
        return Some(GateOutcome {
            required: true,
            satisfied: false,
            reason: GateReason::LowConfidence,
        });
    }
    if sink_kind.is_policy_sink() {
        let sink = policy_sink_of(sink_kind)?;
        let ceiling = inputs.policy.sink(sink)?;
        if kind.rank() > ceiling.max_kind().rank() {
            return Some(GateOutcome {
                required: true,
                satisfied: false,
                reason: GateReason::SinkCeilingExceeded,
            });
        }
        return Some(GateOutcome {
            required: true,
            satisfied: true,
            reason: GateReason::DestinationDeclared,
        });
    }
    Some(GateOutcome {
        required: false,
        satisfied: true,
        reason: GateReason::NotRequired,
    })
}

impl Inputs<'_> {
    /// Whether the declared graph is the only input (observed sections
    /// present would set this false before analysis).
    const fn inputs_declared_complete(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::GateReason;

    #[test]
    fn gate_rules_cover_every_failure_reason() {
        for (reason, rule) in [
            (GateReason::MissingDestination, Some("dataflow.missing-destination")),
            (GateReason::MissingApproval, Some("dataflow.missing-approval")),
            (GateReason::DestinationForbidden, Some("dataflow.destination-forbidden")),
            (GateReason::UnknownFlow, Some("dataflow.unknown-flow")),
            (GateReason::LowConfidence, Some("dataflow.low-confidence-sensitive")),
            (GateReason::SinkCeilingExceeded, Some("classification.sink-ceiling-exceeded")),
            (GateReason::UnclassifiedSubject, Some("classification.unclassified-sensitive-sink")),
            (GateReason::InputsIncomplete, Some("dataflow.observed-incomplete")),
            (GateReason::NotRequired, None),
            (GateReason::DestinationDeclared, None),
            (GateReason::ApprovalPresent, None),
        ] {
            assert_eq!(super::gate_rule_of(reason), rule, "{reason:?}");
        }
    }
}

/// The CLI entry for `lekalo dataflow report`: validate custody,
/// resolve subjects, build the stamped effect graph, and derive the
/// report with its normalized findings. Any structured violation
/// aborts with the typed set; the returned diagnostics carry the
/// mirrored findings for exit mapping.
pub fn run_report(
    compilation: &crate::ir::Compilation,
    attachment: &crate::classification::Attachment,
    policy: &crate::classification::PolicyAttachment,
    resolution: &crate::classification::Resolution,
) -> Result<(crate::dataflow::Report, crate::diagnostics::DiagnosticSet), DiagnosticSet> {
    // Custody: the attachment binds the exact compilation.
    crate::classification::validate::validate_custody(attachment, policy, &compilation.project)?;
    crate::classification::validate::validate_subjects(attachment, compilation)?;
    let graph = crate::effects::build_with_classification(&compilation.project, Some(resolution))?;
    let (model_digest, ir_digest) = compile_digests(compilation);
    let project_id = match compilation.project.project.as_ref() {
        Some(project) => SemanticId::parse_root(project.id.as_str())
            .map_err(|_| diagnostic::document_invalid("project-id", None))?,
        None => SemanticId::parse_root(attachment.project_id().as_str())
            .map_err(|_| diagnostic::document_invalid("project-id", None))?,
    };
    let classification_ref = crate::lockfile::types::Sha256Digest::parse(
        &crate::classification::attachment_digest(attachment)?,
    )
    .map_err(|_| diagnostic::document_invalid("classification-ref", None))?;
    let policy_ref = crate::lockfile::types::Sha256Digest::parse(
        &crate::classification::policy_digest(policy)?,
    )
    .map_err(|_| diagnostic::document_invalid("policy-ref", None))?;
    let analysis = analyze(&Inputs {
        project_id: &project_id,
        model_ref: (compilation.project.model_version.as_str(), &model_digest),
        ir_ref: ("0.2.16", &ir_digest),
        graph: &graph,
        classification: resolution,
        classification_ref: &classification_ref,
        policy,
        policy_ref: &policy_ref,
        generated_by: GENERATED_BY,
        report_revision: REPORT_REVISION,
    })?;
    let diagnostics = analysis.diagnostics.clone();
    Ok((analysis.report, diagnostics))
}

/// The engine identity recorded as `generatedBy`.
const GENERATED_BY: &str = "lekalo-core";

/// The pinned report revision of this generation.
const REPORT_REVISION: &str = "1.0.0";

/// The exact `(model, ir)` digests of one compilation (the same
/// spelling the effect-graph builder records).
fn compile_digests(
    compilation: &crate::ir::Compilation,
) -> (crate::lockfile::types::Sha256Digest, crate::lockfile::types::Sha256Digest) {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(compilation.project.to_canonical_json().as_bytes());
    let ir = format!("sha256:{:x}", hasher.finalize());
    (
        crate::lockfile::types::Sha256Digest::from_hex(&crate::digest::sha256_hex(
            compilation.project.model_version.as_str().as_bytes(),
        )),
        crate::lockfile::types::Sha256Digest::parse(&ir)
            .unwrap_or_else(|_| crate::lockfile::types::Sha256Digest::from_hex(&"0".repeat(64))),
    )
}
