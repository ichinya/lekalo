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

use crate::classification::policy::{DestinationKind, PolicyAttachment, SinkName};
use crate::classification::resolve::{Resolution, ResolvedKind};
use crate::classification::types::{DataKind, SubjectPath};
use crate::diagnostics::DiagnosticSet;
use crate::effects::EffectGraph;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

use super::diagnostic;
use super::report::Report;
use super::types::{
    Confidence, Finding, Flow, Gate, GateReason, Provenance, Severity, SinkKind, TenantRelation,
    UnknownFlow, UnknownReason,
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
    /// The bound compilation (the tenant-key join evidence of the
    /// tenant-relation projection).
    pub compilation: &'a crate::ir::Compilation,
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
    /// The transport endpoint-actor bindings (issue #70 seam, plan
    /// §5.1): empty on this branch — the exposure rule then emits
    /// nothing, and never fails closed on the absence of observed data.
    pub endpoint_exposures: &'a [EndpointExposure],
}

/// One transport endpoint-actor binding handed to the analysis: the
/// endpoint symbol, its closed actor, and the subject its success
/// response resolves to (the binding resolves the operation result;
/// the analysis classifies it).
#[derive(Clone, Debug)]
pub struct EndpointExposure {
    /// The endpoint semantic id.
    pub endpoint: String,
    /// The closed actor of the binding.
    pub actor: EndpointActor,
    /// The subject the success response exposes.
    pub result_subject: SubjectPath,
}

/// The closed endpoint-actor vocabulary (the transport `actor` axis).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointActor {
    /// `public` — unauthenticated exposure.
    Public,
    /// Every authenticated actor.
    Authenticated,
}

impl EndpointActor {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Authenticated => "authenticated",
        }
    }
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
    let mut has_unknowns = false;
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
                // Unknown is never safe (plan §4.3): the flow is
                // recorded as unknown, its rule finding fires, and the
                // verdict denies — never a silent skip.
                unknowns.push(UnknownFlow {
                    source: subject.clone(),
                    reason: UnknownReason::UnresolvedSubject,
                });
                findings.push(Finding {
                    rule_id: "dataflow.unknown-flow".to_owned(),
                    severity: Severity::Error,
                    subject: subject.as_str().to_owned(),
                    detail: "unresolved-subject".to_owned(),
                });
                has_unknowns = true;
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
        let tenant_relation = tenant_relation_of(inputs, &subject, kind, sink_kind);
        let gate = gate_outcome(inputs, &subject, &kind, sink_kind, confidence, resolved);
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
        // Rule findings over the flow. An unknown tenant relation is
        // treated as crossing (plan §4.4): scopes that cannot be derived
        // are never silently `same`.
        if tenant_relation != TenantRelation::Same && kind.cross_tenant_forbidden_by_default() {
            findings.push(Finding {
                rule_id: "dataflow.tenant-crossing".to_owned(),
                severity: Severity::Error,
                subject: subject.as_str().to_owned(),
                detail: if tenant_relation == TenantRelation::Crossing {
                    "cross-tenant-flow".to_owned()
                } else {
                    "tenant-relation-unknown".to_owned()
                },
            });
        }
        // Hard rule: secret values never enter diagnostics, traces, or
        // evidence — those sinks observe every operation unconditionally,
        // so a credential-kind subject is above their ceiling wherever
        // it flows.
        if kind == DataKind::Credential {
            for sink in [SinkName::Diagnostics, SinkName::Traces] {
                if let Some(ceiling) = inputs.policy.sink(sink) {
                    if DataKind::Credential.rank() > ceiling.max_kind().rank() {
                        findings.push(Finding {
                            rule_id: "classification.sink-ceiling-exceeded".to_owned(),
                            severity: Severity::Error,
                            subject: subject.as_str().to_owned(),
                            detail: format!("sink-{}", sink.as_str()),
                        });
                        break;
                    }
                }
            }
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
    // Detected (observed) edges are analyzed too: adapter-reported
    // effects are never invisible to the analysis. They project flows
    // with `observed` provenance, degraded confidence, and incomplete
    // project inputs (plan §4.3: observed never promotes to canonical).
    for edge in inputs.graph.detected() {
        let resource = edge.key().subject().resource().as_str();
        let subject = match SubjectPath::parse(resource) {
            Ok(subject) => subject,
            Err(_) => continue,
        };
        let resolved = inputs.classification.resolve(&subject);
        let kind = match resolved {
            ResolvedKind::Classified(kind) => kind,
            ResolvedKind::Unclassified => {
                unknowns.push(UnknownFlow {
                    source: subject.clone(),
                    reason: UnknownReason::UnresolvedSubject,
                });
                findings.push(Finding {
                    rule_id: "dataflow.unknown-flow".to_owned(),
                    severity: Severity::Error,
                    subject: subject.as_str().to_owned(),
                    detail: "unresolved-subject".to_owned(),
                });
                has_unknowns = true;
                continue;
            }
        };
        inputs_complete = false;
        let sink_kind = sink_kind_of(edge.key().kind().key());
        let tenant_relation = tenant_relation_of(inputs, &subject, kind, sink_kind);
        let gate = gate_outcome(
            inputs,
            &subject,
            &kind,
            sink_kind,
            Confidence::Unknown,
            resolved,
        );
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
            provenance: Provenance::Observed,
            confidence: Confidence::Unknown,
            tenant_relation,
            classification: kind,
            gate: gate_wire,
        });
        // A sensitive flow resting on observed evidence is a finding
        // (plan §4.3), and the project-wide incompleteness rule fires.
        if kind.is_sensitive() {
            findings.push(Finding {
                rule_id: "dataflow.low-confidence-sensitive".to_owned(),
                severity: Severity::Error,
                subject: subject.as_str().to_owned(),
                detail: Confidence::Unknown.as_str().to_owned(),
            });
        }
        findings.push(Finding {
            rule_id: "dataflow.observed-incomplete".to_owned(),
            severity: Severity::Error,
            subject: subject.as_str().to_owned(),
            detail: "observed-edge".to_owned(),
        });
        // Observed credential flows keep the unconditional ceiling.
        if kind == DataKind::Credential {
            for sink in [SinkName::Diagnostics, SinkName::Traces] {
                if let Some(ceiling) = inputs.policy.sink(sink) {
                    if DataKind::Credential.rank() > ceiling.max_kind().rank() {
                        findings.push(Finding {
                            rule_id: "classification.sink-ceiling-exceeded".to_owned(),
                            severity: Severity::Error,
                            subject: subject.as_str().to_owned(),
                            detail: format!("sink-{}", sink.as_str()),
                        });
                        break;
                    }
                }
            }
        }
    }
    // Declared edge coverage of the event-publish and external sinks
    // through extended-effects is consumed by the CLI validation hook;
    // this module projects what the graph itself carries so analysis
    // stays a pure function of the pinned compilation plus inputs.

    // The public-endpoint exposure rule (plan §5.1, the acceptance
    // case) over the transport bindings handed to this run.
    findings.extend(exposure_findings(
        inputs.classification,
        inputs.endpoint_exposures,
    ));

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
        has_unknowns,
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

/// The public-endpoint exposure evaluation over one resolution and the
/// supplied endpoint-actor bindings: a public actor whose success
/// response resolves to a kind above public is a finding unless an
/// approved declassification grant lowers the subject to public.
pub fn exposure_findings(
    resolution: &crate::classification::Resolution,
    exposures: &[EndpointExposure],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for exposure in exposures {
        if exposure.actor != EndpointActor::Public {
            continue;
        }
        // Unclassified on a public endpoint counts as sensitive (the
        // resolution widens unclassified to the most restrictive rank,
        // and unknown is never safe): it is an exposure finding unless
        // an approved grant covers the subject explicitly.
        let kind = match resolution.resolve(&exposure.result_subject) {
            ResolvedKind::Classified(kind) => kind,
            ResolvedKind::Unclassified => DataKind::Credential,
        };
        if kind.rank() <= DataKind::Public.rank() {
            continue;
        }
        // Only a fully valid grant clears the exposure: a real strict
        // lowering to public, never self-approved, not expired. The
        // structural role/approval checks run in validation; here the
        // grant must at least not be the grant's own approval.
        let lowered_to_public = resolution
            .grants(&exposure.result_subject)
            .iter()
            .any(|grant| {
                grant.from_kind() == kind
                    && grant.to_kind() == DataKind::Public
                    && grant.from_kind().declassifiable()
                    && grant.approved_by().as_str() != grant.id().as_str()
            });
        if lowered_to_public {
            continue;
        }
        findings.push(Finding {
            rule_id: "dataflow.exposed-private-field".to_owned(),
            severity: Severity::Error,
            subject: exposure.result_subject.as_str().to_owned(),
            detail: "public-endpoint".to_owned(),
        });
    }
    findings
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
        if let Ok(built) = crate::diagnostics::normalize::build(&finding.rule_id, None, None, data)
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

/// The closed sink kind of one effect-kind key: every declared
/// effect-kind key maps to its honest sink kind — never a silent
/// collapse into `storage-write`.
fn sink_kind_of(kind_key: &str) -> SinkKind {
    match kind_key {
        "read" => SinkKind::EndpointResponse,
        "create" | "update" | "delete" | "write-field" => SinkKind::StorageWrite,
        "emit-event" => SinkKind::EventPublish,
        "enqueue-job" => SinkKind::ExternalCall,
        "external-call" => SinkKind::ExternalCall,
        "cache-read" | "cache-write" | "cache-invalidate" => SinkKind::CacheWrite,
        "publish-output" => SinkKind::Publication,
        "audit-log" => SinkKind::Log,
        "transaction-boundary" => SinkKind::StorageWrite,
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

/// The tenant relation of one subject/kind pair: the join of the IR
/// entity's declared tenant-key field (the query-model tenancy
/// declaration shape of plan §4.4) with the kind's policy cross-tenant
/// mode. With no declared tenant evidence the relation is `unknown` —
/// never silently `same` (review F-7): no evidence, no assertion of
/// safety.
fn tenant_relation_of(
    inputs: &Inputs<'_>,
    subject: &SubjectPath,
    kind: DataKind,
    sink_kind: SinkKind,
) -> TenantRelation {
    // Evidence: the source entity declares a tenant key field (the
    // query-model tenancy shape of plan §4.4), so the record set is
    // tenant-partitioned and every access carries a tenant scope.
    let partitioned = inputs
        .compilation
        .project
        .definitions
        .iter()
        .any(|definition| {
            definition.id().as_str() == subject.semantic_id()
                && matches!(
                    definition,
                    crate::ir::Definition::Entity(entity) if has_tenant_key(entity)
                )
        });
    if !partitioned {
        // No tenant evidence at all: unknown. Never silently `same` —
        // no evidence, no assertion of safety.
        return TenantRelation::Unknown;
    }
    // Tenant-partitioned source. A declared effect on the entity
    // (storage/event edge of the project's own commands and queries)
    // executes inside the tenant scope when the kind's policy grants
    // the `tenant` actor: the relation is provably `same`.
    let Some(rule) = inputs.policy.rule(kind) else {
        return TenantRelation::Unknown;
    };
    let tenant_scoped_access = matches!(
        sink_kind,
        SinkKind::StorageWrite | SinkKind::EndpointResponse | SinkKind::EventPublish
    );
    let tenant_actor = rule
        .readers()
        .iter()
        .chain(rule.writers().iter())
        .any(|actor| actor.dimension() == crate::classification::policy::ScopeDimension::Tenant);
    if tenant_scoped_access && tenant_actor {
        return match rule.cross_tenant() {
            crate::classification::CrossTenantMode::Reviewed => TenantRelation::Same,
            // Forbidden kinds stay conservative: tenant-scoped storage
            // and query-response access through a tenant actor is
            // provable (the project's own commands and queries execute
            // inside the tenant scope), but any publication-shaped sink
            // has no sink-actor bindings yet, so its relation stays
            // unknown — never silently `same`.
            crate::classification::CrossTenantMode::Forbidden => {
                if matches!(
                    sink_kind,
                    SinkKind::StorageWrite | SinkKind::EndpointResponse
                ) {
                    TenantRelation::Same
                } else {
                    TenantRelation::Unknown
                }
            }
        };
    }
    // Access outside the tenant scope (external/publication sinks):
    // the relation cannot be derived.
    TenantRelation::Unknown
}

/// Whether one entity declares a tenant-key field (the plan §4.4
/// tenant-filter declaration: the field the tenant filter must
/// constrain).
fn has_tenant_key(entity: &crate::ir::EntityDef) -> bool {
    entity.fields.iter().any(|field| {
        field.name.as_str() == "tenant_id"
            || field.name.as_str() == "tenant"
            || field.name.as_str().ends_with("_tenant_id")
    })
}

/// The gate decision for one flow to one sink. Gated sinks —
/// external-call, publication, public-endpoint response, export, and
/// cache — evaluate the plan §3.2 rules: the kind's policy row must
/// declare a destination that accepts the sink (missing-destination),
/// consent-bearing kinds require an approval record (missing-approval),
/// and a destination the kind never declares is forbidden
/// (destination-forbidden). Policy sinks evaluate their ceilings.
fn gate_outcome(
    inputs: &Inputs<'_>,
    subject: &SubjectPath,
    kind: &DataKind,
    sink_kind: SinkKind,
    confidence: Confidence,
    resolved: ResolvedKind,
) -> Option<GateOutcome> {
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
    if sink_kind.is_gated() {
        // The destination the sink consumes (plan §3.2) and the kind's
        // declared destination set.
        let Some(kind_rule) = inputs.policy.rule(*kind) else {
            return Some(GateOutcome {
                required: true,
                satisfied: false,
                reason: GateReason::UnknownFlow,
            });
        };
        let declared: Vec<DestinationKind> = kind_rule.destinations().to_vec();
        let required_destination = gated_destination_of(sink_kind);
        match required_destination {
            None => {
                return Some(GateOutcome {
                    required: true,
                    satisfied: false,
                    reason: GateReason::MissingDestination,
                });
            }
            Some(destination) if !declared.contains(&destination) => {
                return Some(GateOutcome {
                    required: true,
                    satisfied: false,
                    reason: GateReason::DestinationForbidden,
                });
            }
            _ => {}
        }
        // Consent-bearing kinds require an approval record; grants are
        // structural until #26 wires review verification.
        if kind_rule.consent_required() && inputs.grants_of(subject).is_empty() {
            return Some(GateOutcome {
                required: true,
                satisfied: false,
                reason: GateReason::MissingApproval,
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

/// The policy destination a gated sink consumes.
fn gated_destination_of(sink_kind: SinkKind) -> Option<DestinationKind> {
    match sink_kind {
        SinkKind::ExternalCall => Some(DestinationKind::InternalService),
        SinkKind::Publication => Some(DestinationKind::MessageBus),
        SinkKind::CacheWrite => Some(DestinationKind::InternalService),
        _ => None,
    }
}

#[cfg(test)]
mod sink_kind_tests {
    use super::super::types::SinkKind;
    use super::sink_kind_of;

    /// Every declared effect-kind key maps to its honest sink kind:
    /// gated kinds survive, they never collapse into storage-write
    /// (the review's F-5 dead-machinery finding).
    #[test]
    fn every_effect_kind_maps_to_its_honest_sink_kind() {
        assert_eq!(sink_kind_of("read"), SinkKind::EndpointResponse);
        assert_eq!(sink_kind_of("create"), SinkKind::StorageWrite);
        assert_eq!(sink_kind_of("update"), SinkKind::StorageWrite);
        assert_eq!(sink_kind_of("delete"), SinkKind::StorageWrite);
        assert_eq!(sink_kind_of("write-field"), SinkKind::StorageWrite);
        assert_eq!(sink_kind_of("emit-event"), SinkKind::EventPublish);
        assert_eq!(sink_kind_of("external-call"), SinkKind::ExternalCall);
        assert_eq!(sink_kind_of("enqueue-job"), SinkKind::ExternalCall);
        assert_eq!(sink_kind_of("publish-output"), SinkKind::Publication);
        assert_eq!(sink_kind_of("audit-log"), SinkKind::Log);
        assert_eq!(sink_kind_of("cache-read"), SinkKind::CacheWrite);
        assert_eq!(sink_kind_of("cache-write"), SinkKind::CacheWrite);
        assert_eq!(sink_kind_of("cache-invalidate"), SinkKind::CacheWrite);
        // The closed vocabulary stays total: an unknown key fails
        // closed into the conservative default rather than panicking.
        assert_eq!(sink_kind_of("something-else"), SinkKind::StorageWrite);
    }
}

impl Inputs<'_> {
    /// Whether the declared graph is the only input (observed sections
    /// present would set this false before analysis).
    const fn inputs_declared_complete(&self) -> bool {
        true
    }

    /// The declared declassification grants of one subject (the
    /// approval records the consent rule consults).
    fn grants_of(&self, subject: &SubjectPath) -> &[crate::classification::Declassification] {
        self.classification.grants(subject)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::GateReason;

    #[test]
    fn gate_rules_cover_every_failure_reason() {
        for (reason, rule) in [
            (
                GateReason::MissingDestination,
                Some("dataflow.missing-destination"),
            ),
            (
                GateReason::MissingApproval,
                Some("dataflow.missing-approval"),
            ),
            (
                GateReason::DestinationForbidden,
                Some("dataflow.destination-forbidden"),
            ),
            (GateReason::UnknownFlow, Some("dataflow.unknown-flow")),
            (
                GateReason::LowConfidence,
                Some("dataflow.low-confidence-sensitive"),
            ),
            (
                GateReason::SinkCeilingExceeded,
                Some("classification.sink-ceiling-exceeded"),
            ),
            (
                GateReason::UnclassifiedSubject,
                Some("classification.unclassified-sensitive-sink"),
            ),
            (
                GateReason::InputsIncomplete,
                Some("dataflow.observed-incomplete"),
            ),
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
    model_json: &str,
    attachment: &crate::classification::Attachment,
    policy: &crate::classification::PolicyAttachment,
    resolution: &crate::classification::Resolution,
    endpoint_exposures: &[EndpointExposure],
) -> Result<(crate::dataflow::Report, crate::diagnostics::DiagnosticSet), DiagnosticSet> {
    // Custody: the attachment binds the exact compilation (project,
    // Model bytes, and IR bytes).
    crate::classification::validate::validate_custody(
        attachment,
        policy,
        &compilation.project,
        model_json,
    )?;
    crate::classification::validate::validate_subjects(attachment, compilation)?;
    // Grant validity: the report never rests on an invalid grant
    // (kind-rule-missing, self-approval, expiry); violations are
    // terminal for the report surface.
    crate::classification::validate_policy_and_grants(
        attachment,
        policy,
        resolution,
        &compilation.project,
    )?;
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
    let policy_ref =
        crate::lockfile::types::Sha256Digest::parse(&crate::classification::policy_digest(policy)?)
            .map_err(|_| diagnostic::document_invalid("policy-ref", None))?;
    let analysis = analyze(&Inputs {
        project_id: &project_id,
        model_ref: (compilation.project.model_version.as_str(), &model_digest),
        ir_ref: (crate::ir::VERSION, &ir_digest),
        graph: &graph,
        compilation,
        classification: resolution,
        classification_ref: &classification_ref,
        policy,
        policy_ref: &policy_ref,
        generated_by: GENERATED_BY,
        report_revision: REPORT_REVISION,
        endpoint_exposures,
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
) -> (
    crate::lockfile::types::Sha256Digest,
    crate::lockfile::types::Sha256Digest,
) {
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

#[cfg(test)]
mod exposure_tests {
    use super::*;
    use crate::classification::Attachment;

    const ATTACHMENT: &str = r#"{
      "schemaVersion": "lekalo/data-classification/v0.4.0",
      "identity": "dev.lekalo.data-classification@0.4.0",
      "attachmentRevision": "1.0.0",
      "projectId": "clinic",
      "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
      "irRef": {"irVersion": "0.2.16", "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
      "defaults": {"profile": "strict", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"},
      "classifications": [
        {"subject": "core.entity.user/email", "kind": "personal"}
      ],
      "declassifications": [],
      "openQuestions": []
    }"#;

    fn resolution() -> crate::classification::Resolution {
        let attachment = Attachment::parse(ATTACHMENT.as_bytes()).expect("parses");
        crate::classification::Resolution::build(&attachment)
    }

    fn exposure_of(subject: &str) -> EndpointExposure {
        EndpointExposure {
            endpoint: "core.endpoint.list_users".to_owned(),
            actor: EndpointActor::Public,
            result_subject: SubjectPath::parse(subject).expect("subject"),
        }
    }

    #[test]
    fn a_public_endpoint_exposing_a_personal_field_is_a_finding() {
        let resolution = resolution();
        let exposures = vec![exposure_of("core.entity.user/email")];
        let findings = super::exposure_findings(&resolution, &exposures);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "dataflow.exposed-private-field");
        assert!(findings[0].severity.is_error());
    }

    #[test]
    fn authenticated_actors_never_expose_and_unresolved_subjects_fail_closed() {
        let resolution = resolution();
        let mut authenticated = exposure_of("core.entity.user/email");
        authenticated.actor = EndpointActor::Authenticated;
        let unresolved = exposure_of("core.entity.user");
        let findings = super::exposure_findings(&resolution, &[authenticated, unresolved]);
        // An authenticated actor never triggers the exposure rule, but
        // a public endpoint whose result subject never resolved is the
        // unsafe-unknown state: unclassified counts as sensitive and
        // fails closed (exactly one finding, on the unresolved subject).
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].rule_id, "dataflow.exposed-private-field");
        assert_eq!(findings[0].subject, "core.entity.user");
    }

    #[test]
    fn an_approved_lowering_to_public_clears_the_exposure() {
        let attachment = Attachment::parse(
            r#"{
      "schemaVersion": "lekalo/data-classification/v0.4.0",
      "identity": "dev.lekalo.data-classification@0.4.0",
      "attachmentRevision": "1.1.0",
      "projectId": "clinic",
      "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
      "irRef": {"irVersion": "0.2.16", "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
      "defaults": {"profile": "strict", "unclassifiedFields": "internal", "unclassifiedPayloads": "confidential"},
      "classifications": [
        {"subject": "core.entity.user/email", "kind": "personal"}
      ],
      "declassifications": [
        {
          "id": "grant.core.email-public@1.0.0",
          "subject": "core.entity.user/email",
          "fromKind": "personal",
          "toKind": "public",
          "approvedBy": "review-2025-09-003",
          "justification": "Published directory listing."
        }
      ],
      "openQuestions": []
    }"#
            .as_bytes(),
        )
        .expect("parses");
        let resolution = crate::classification::Resolution::build(&attachment);
        let exposures = vec![exposure_of("core.entity.user/email")];
        let findings = super::exposure_findings(&resolution, &exposures);
        assert!(findings.is_empty(), "the grant clears the exposure");
    }
}
