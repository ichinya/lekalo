//! Issue #26: the closed, versioned extended-effects attachment.
//!
//! One independent, immutable contract family declaring the
//! non-database effect semantics of a project: event contracts
//! (schema and version, local/durable delivery, ordering and
//! deduplication, correlation and causation fields), job contracts
//! (payload, queue class, retry and backoff, idempotency, timeout and
//! dead-letter policy), external-call contracts (provider capability,
//! request/response/error contract, timeout and retry, read/write/
//! destructive classification, compensation), cache contracts (key
//! contract, read/write/invalidate operations, consistency and
//! freshness), and publication contracts (destination, explicit opt-in
//! and approval, sensitivity, immutable revisioned snapshots) — plus
//! typed adapter-capability requirement records and deterministic
//! partial-failure cases keyed to Scenario IR documents.
//!
//! Boundaries: this module is pure declaration and validation — no
//! runtime execution, no queue or provider implementation, no adapter
//! call, no transaction semantics (#24 owns commit and concurrency), no
//! authorization (#25 owns consent enforcement), and no report or cache
//! surface. Effect meaning stays with #14 (the attachment only
//! contributes typed bindings keyed by exact EffectIds); scenario step
//! identity and execution stay with #23 and the owner harnesses; error
//! identity stays with #62 (only opaque typed refs are carried);
//! capability registries, negotiation, and profile resolution stay
//! with #27/#28/#29.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON with
//! byte-sorted keys; set-like collections normalize to sorted form
//! while schedules keep their declared partial order as canonically
//! ordered nodes. Every bound and every semantic contradiction rejects
//! with an explicit registered diagnostic and no partial result.

pub mod canonical;
pub mod capability;
pub mod case;
pub mod contract;
pub(crate) mod diagnostic;
pub mod diff;
pub mod identity;
pub mod version;
pub(crate) mod wire;

pub use capability::{
    map_capabilities, Capability, CapabilityDecision, CapabilityProfile, CapabilityRequirement,
    CapabilitySnapshot, RequirementLevel, RequirementVerdict, SnapshotSupport,
};
pub use case::{
    CaseNode, CaseStep, ExpectedOutcome, Fault, NodeKind, Outcome, PartialFailureCase, ScenarioRef,
};
pub use contract::{
    Approval, Backoff, CacheContract, CacheFreshness, CacheKeyContract, CacheOperation,
    CallContract, CallRetry, CallSafety, Classification, Consent, Consistency, Contributions,
    DeadLetter, Deduplication, DeduplicationMode, Delivery, EventContract, JobContract,
    JobIdempotency, JobIdempotencyMode, JobRetry, Ordering, Portability, PublicationContract,
    SecurityGate, Sensitivity, Snapshot,
};
pub use diff::{compare, DiffClass, DiffPath, DiffResult};
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::effects::EffectGraph;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

/// The bound source Model contract: exact accepted version plus digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelPin {
    /// The accepted Model contract version.
    pub(crate) version: crate::scenario::ModelPin,
    /// The digest of the exact source Model document.
    pub(crate) digest: Sha256Digest,
}

impl ModelPin {
    /// The accepted Model contract version.
    pub const fn version(&self) -> &crate::scenario::ModelPin {
        &self.version
    }

    /// The digest of the exact source Model document.
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

/// One finished extended-effects attachment: immutable, deterministically
/// ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtendedEffectsAttachment {
    project_id: SemanticId,
    model_ref: ModelPin,
    ir_digest: Sha256Digest,
    effect_graph_digest: Option<Sha256Digest>,
    events: Vec<EventContract>,
    jobs: Vec<JobContract>,
    calls: Vec<CallContract>,
    caches: Vec<CacheContract>,
    publications: Vec<PublicationContract>,
    cases: Vec<PartialFailureCase>,
    capability_requirements: Vec<CapabilityRequirement>,
}

impl ExtendedEffectsAttachment {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        effect_graph_digest: Option<Sha256Digest>,
        events: Vec<EventContract>,
        jobs: Vec<JobContract>,
        calls: Vec<CallContract>,
        caches: Vec<CacheContract>,
        publications: Vec<PublicationContract>,
        cases: Vec<PartialFailureCase>,
        capability_requirements: Vec<CapabilityRequirement>,
    ) -> Self {
        Self {
            project_id,
            model_ref,
            ir_digest,
            effect_graph_digest,
            events,
            jobs,
            calls,
            caches,
            publications,
            cases,
            capability_requirements,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// sorted set-like collections, no trailing LF), or a typed refusal
    /// beyond the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::attachment_bytes(self)
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound source Model contract.
    pub const fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// The optional bound effect-graph digest.
    pub fn effect_graph_digest(&self) -> Option<&Sha256Digest> {
        self.effect_graph_digest.as_ref()
    }

    /// Every event contract, canonically ordered by contract ref.
    pub fn events(&self) -> &[EventContract] {
        &self.events
    }

    /// Every job contract, canonically ordered by contract ref.
    pub fn jobs(&self) -> &[JobContract] {
        &self.jobs
    }

    /// Every external-call contract, canonically ordered by contract ref.
    pub fn calls(&self) -> &[CallContract] {
        &self.calls
    }

    /// Every cache contract, canonically ordered by contract ref.
    pub fn caches(&self) -> &[CacheContract] {
        &self.caches
    }

    /// Every publication contract, canonically ordered by contract ref.
    pub fn publications(&self) -> &[PublicationContract] {
        &self.publications
    }

    /// Every partial-failure case, canonically ordered by case id.
    pub fn cases(&self) -> &[PartialFailureCase] {
        &self.cases
    }

    /// Every capability requirement record, canonically ordered by
    /// requirement id.
    pub fn capability_requirements(&self) -> &[CapabilityRequirement] {
        &self.capability_requirements
    }

    /// The typed effect-binding projection keyed by exact #14 EffectId:
    /// plain data the graph, impact, context, and scenario surfaces may
    /// display — never a graph mutation. Inclusion stays exactly as
    /// declared: nothing enters a surface implicitly.
    pub fn effect_bindings(&self) -> Vec<EffectBinding> {
        let mut bindings = Vec::new();
        for contract in self.all_contracts() {
            bindings.push(EffectBinding {
                contract_ref: contract.ref_text().to_owned(),
                effect: contract.effect_text(),
                kind: contract.kind_key(),
                graph: contract.common().contributions.graph,
                impact: contract.common().contributions.impact,
                context: contract.common().contributions.context,
                scenarios: contract.common().contributions.scenarios,
            });
        }
        bindings.sort_by(|left, right| {
            (&left.contract_ref[..], &left.effect[..])
                .cmp(&(&right.contract_ref[..], &right.effect[..]))
        });
        bindings
    }

    /// Cross-validate this attachment against the accepted #14 effect
    /// graph it binds: every graph-contributed contract reference must
    /// resolve to a declared edge of its operation with the bound kind,
    /// and the bound graph digest (when declared) must match. Pure and
    /// read-only; never mutates the graph.
    pub fn validate_against_graph(&self, graph: &EffectGraph) -> Result<(), DiagnosticSet> {
        if let Some(bound) = &self.effect_graph_digest {
            if bound.as_str() != graph.ir_digest() {
                return Err(diagnostic::input_invalid("graph-digest-mismatch", None));
            }
        }
        for contract in self.all_contracts() {
            if !contract.common().contributions.graph {
                continue;
            }
            let effect = contract.common().effect.as_str();
            let edges: Vec<String> = graph
                .operation_edges(contract.common().effect.operation())
                .iter()
                .map(|edge| edge.key().to_canonical_string())
                .collect();
            if !edges.contains(&effect) {
                return Err(diagnostic::input_invalid(
                    "unresolved-effect-ref",
                    Some(contract.ref_text()),
                ));
            }
        }
        Ok(())
    }

    /// Every contract in canonical kind order, as an erased view.
    pub(crate) fn all_contracts(&self) -> Vec<ContractView<'_>> {
        let mut views = Vec::new();
        for event in &self.events {
            views.push(ContractView::Event(event));
        }
        for job in &self.jobs {
            views.push(ContractView::Job(job));
        }
        for call in &self.calls {
            views.push(ContractView::Call(call));
        }
        for cache in &self.caches {
            views.push(ContractView::Cache(cache));
        }
        for publication in &self.publications {
            views.push(ContractView::Publication(publication));
        }
        views
    }

    /// The full semantic self-check: identity uniqueness, per-family
    /// rules, retry-idempotency coherence, consent, review gates,
    /// capability coverage, and case schedule integrity.
    pub(crate) fn semantic_self_check(&self) -> Result<(), DiagnosticSet> {
        self.check_identities()?;
        self.check_common_rules()?;
        self.check_events()?;
        self.check_jobs()?;
        self.check_calls()?;
        self.check_caches()?;
        self.check_publications()?;
        self.check_capability_coverage()?;
        self.check_cases()?;
        Ok(())
    }

    /// Attachment-wide identity uniqueness across every namespace.
    fn check_identities(&self) -> Result<(), DiagnosticSet> {
        let views = self.all_contracts();
        let mut refs: Vec<&str> = views.iter().map(|contract| contract.ref_text()).collect();
        refs.extend(self.cases.iter().map(|case| case.case_id.as_str()));
        refs.extend(
            self.capability_requirements
                .iter()
                .map(|requirement| requirement.requirement_id.as_str()),
        );
        refs.sort_unstable();
        let total = refs.len();
        refs.dedup();
        if refs.len() != total {
            return Err(diagnostic::input_invalid("duplicate-contract-ref", None));
        }
        Ok(())
    }

    /// Rules shared by every contract kind.
    fn check_common_rules(&self) -> Result<(), DiagnosticSet> {
        if self.all_contracts().is_empty() {
            return Err(diagnostic::input_invalid("no-contracts", None));
        }
        for contract in self.all_contracts() {
            // Target-specific details live only in their target profile.
            match contract.common().portability {
                Portability::TargetSpecific if contract.common().target_profile.is_none() => {
                    return Err(diagnostic::input_invalid(
                        "target-profile-missing",
                        Some(contract.ref_text()),
                    ));
                }
                Portability::Portable if contract.common().target_profile.is_some() => {
                    return Err(diagnostic::input_invalid(
                        "target-profile-forbidden",
                        Some(contract.ref_text()),
                    ));
                }
                _ => {}
            }
            // A sensitive effect automatically requires its gate hint
            // and the review-gate capability record.
            if contract.common().sensitivity == Sensitivity::Sensitive
                && contract.common().security_gate.is_none()
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::GATE_REQUIRED,
                    "sensitive-without-gate",
                    Some(contract.ref_text()),
                ));
            }
            if contract.common().sensitivity == Sensitivity::Sensitive {
                self.require_capability(contract.ref_text(), Capability::ReviewGate)?;
            }
        }
        Ok(())
    }

    /// Event-family rules.
    fn check_events(&self) -> Result<(), DiagnosticSet> {
        for event in &self.events {
            let dedup = &event.deduplication;
            let key_needed = matches!(
                dedup.mode,
                DeduplicationMode::Key | DeduplicationMode::DurableKey
            );
            if key_needed != dedup.key_field.is_some() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::EVENT_INVALID,
                    "dedup-key-mismatch",
                    Some(event.contract_ref()),
                ));
            }
            match event.delivery {
                Delivery::Durable => {
                    self.require_capability(event.contract_ref(), Capability::DurableDelivery)?;
                }
                Delivery::Local => {}
            }
            match event.ordering {
                Ordering::Total => {
                    self.require_capability(event.contract_ref(), Capability::OrderingTotal)?;
                }
                Ordering::PerKey => {
                    self.require_capability(event.contract_ref(), Capability::OrderingPerKey)?;
                }
                Ordering::Unordered => {}
            }
            if dedup.mode == DeduplicationMode::DurableKey {
                self.require_capability(event.contract_ref(), Capability::DurableDedup)?;
            }
        }
        Ok(())
    }

    /// Job-family rules. Retries never violate idempotency: a retrying
    /// job must be idempotent by key or by construction.
    fn check_jobs(&self) -> Result<(), DiagnosticSet> {
        for job in &self.jobs {
            let retrying = job.retry.max_attempts > 1;
            let idempotent = !matches!(job.idempotency.mode, JobIdempotencyMode::None);
            if retrying && !idempotent {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "retry-without-idempotency",
                    Some(job.contract_ref()),
                ));
            }
            if retrying
                && job.idempotency.mode == JobIdempotencyMode::Key
                && job.idempotency.key_field.is_none()
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "idempotency-key-missing",
                    Some(job.contract_ref()),
                ));
            }
            if job.dead_letter != DeadLetter::None {
                self.require_capability(job.contract_ref(), Capability::DeadLetter)?;
            }
            if job.retry.backoff != Backoff::None && job.retry.cap_millis.is_none() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::JOB_INVALID,
                    "backoff-cap-missing",
                    Some(job.contract_ref()),
                ));
            }
            if job.retry.backoff == Backoff::None && job.retry.cap_millis.is_some() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::JOB_INVALID,
                    "backoff-cap-forbidden",
                    Some(job.contract_ref()),
                ));
            }
        }
        Ok(())
    }

    /// External-call rules. Write and destructive calls declare typed
    /// compensation, and errors never collapse to a catch-all.
    fn check_calls(&self) -> Result<(), DiagnosticSet> {
        for call in &self.calls {
            if call.classification.mutates() {
                let Some(compensation) = &call.compensation else {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CALL_INVALID,
                        "missing-compensation",
                        Some(call.contract_ref()),
                    ));
                };
                if compensation.as_str() == call.common.effect.operation().as_str() {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CALL_INVALID,
                        "self-compensation",
                        Some(call.contract_ref()),
                    ));
                }
                self.require_capability(call.contract_ref(), Capability::CallCompensation)?;
            }
            if call.retry.max_attempts > 1 && call.retry.safety == CallSafety::Unsafe {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "unsafe-retry",
                    Some(call.contract_ref()),
                ));
            }
        }
        Ok(())
    }

    /// Cache-family rules: the bound effect kind must be one of the
    /// declared operations, and consistency guarantees carry capability
    /// records.
    fn check_caches(&self) -> Result<(), DiagnosticSet> {
        for cache in &self.caches {
            let bound_kind = cache.common.effect.kind_key();
            let declared = cache
                .operations
                .iter()
                .any(|operation| operation.effect_kind() == bound_kind);
            if !declared {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CACHE_INVALID,
                    "operation-kind-mismatch",
                    Some(cache.contract_ref()),
                ));
            }
            match cache.consistency {
                Consistency::Strong => {
                    self.require_capability(cache.contract_ref(), Capability::StrongConsistency)?;
                }
                Consistency::ReadYourWrites => {
                    self.require_capability(cache.contract_ref(), Capability::ReadYourWrites)?;
                }
                Consistency::Eventual => {}
            }
            // Consistency guarantees are meaningless for a cache the
            // process never reads back.
            if cache.consistency != Consistency::Eventual
                && !cache.operations.contains(&CacheOperation::Read)
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CACHE_INVALID,
                    "consistency-without-read",
                    Some(cache.contract_ref()),
                ));
            }
        }
        Ok(())
    }

    /// Publication rules. Publication is never implicit: approved
    /// publication names its authority, and the immutable snapshot is
    /// always revisioned.
    fn check_publications(&self) -> Result<(), DiagnosticSet> {
        for publication in &self.publications {
            self.require_capability(publication.contract_ref(), Capability::PublishApproval)?;
            self.require_capability(publication.contract_ref(), Capability::RevisionedSnapshot)?;
            if publication.consent.approval == Approval::Approved
                && publication.consent.approver.is_none()
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CONSENT_MISSING,
                    "approved-without-approver",
                    Some(publication.contract_ref()),
                ));
            }
        }
        Ok(())
    }

    /// One requirement record for the capability must exist and be
    /// referenced by the named contract.
    fn require_capability(
        &self,
        contract_ref: &str,
        capability: Capability,
    ) -> Result<(), DiagnosticSet> {
        let wire = capability.to_wire();
        let record_for = |reference: &str| {
            self.capability_requirements
                .iter()
                .find(|requirement| requirement.requirement_id.as_str() == reference)
        };
        let declared = self
            .capability_requirements
            .iter()
            .any(|requirement| requirement.capability.to_wire() == wire);
        if !declared {
            return Err(diagnostic::rule_invalid(
                diagnostic::CAPABILITY_MISSING,
                "missing-capability",
                Some(&wire),
            ));
        }
        let referenced = self
            .all_contracts()
            .iter()
            .filter(|contract| contract.ref_text() == contract_ref)
            .any(|contract| {
                contract
                    .common()
                    .capability_refs
                    .iter()
                    .filter_map(|reference| record_for(reference.as_str()))
                    .any(|requirement| requirement.capability.to_wire() == wire)
            });
        if !referenced {
            return Err(diagnostic::rule_invalid(
                diagnostic::CAPABILITY_MISSING,
                "capability-unreferenced",
                Some(contract_ref),
            ));
        }
        Ok(())
    }

    /// Every capability reference (contract or case) resolves to a
    /// declared requirement record.
    fn check_capability_coverage(&self) -> Result<(), DiagnosticSet> {
        let resolve = |reference: &str| {
            self.capability_requirements
                .iter()
                .any(|requirement| requirement.requirement_id.as_str() == reference)
        };
        for contract in self.all_contracts() {
            for reference in &contract.common().capability_refs {
                if !resolve(reference.as_str()) {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CAPABILITY_MISSING,
                        "unresolved-capability-ref",
                        Some(reference.as_str()),
                    ));
                }
            }
        }
        for case in &self.cases {
            for reference in &case.capability_refs {
                if !resolve(reference.as_str()) {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CAPABILITY_MISSING,
                        "unresolved-capability-ref",
                        Some(reference.as_str()),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Case integrity: steps reference declared contracts, the schedule
    /// covers each step exactly once with valid joins and acyclicity,
    /// and every scheduled contract declares one consistent outcome.
    fn check_cases(&self) -> Result<(), DiagnosticSet> {
        let views = self.all_contracts();
        let contracts: Vec<&str> = views.iter().map(|contract| contract.ref_text()).collect();
        for case in &self.cases {
            // Every step references a declared contract.
            for step in &case.steps {
                if !contracts.contains(&step.contract_ref.as_str()) {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CASE_INVALID,
                        "unknown-step",
                        Some(step.contract_ref.as_str()),
                    ));
                }
            }
            // Schedule node integrity.
            let mut scheduled: Vec<&str> = Vec::new();
            for node in &case.schedule {
                for join in &node.joins {
                    if !case
                        .schedule
                        .iter()
                        .any(|candidate| candidate.node_id.as_str() == join.as_str())
                    {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "unknown-join",
                            Some(node.node_id.as_str()),
                        ));
                    }
                }
                if let Some(step_id) = &node.step_id {
                    let Some(step) = case
                        .steps
                        .iter()
                        .find(|candidate| candidate.step_id.as_str() == step_id.as_str())
                    else {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "unknown-step",
                            Some(step_id.as_str()),
                        ));
                    };
                    if scheduled.contains(&step.contract_ref.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "duplicate-step-schedule",
                            Some(step.contract_ref.as_str()),
                        ));
                    }
                    scheduled.push(step.contract_ref.as_str());
                }
            }
            // Every step is scheduled.
            for step in &case.steps {
                if !case.schedule.iter().any(|node| {
                    node.step_id.as_ref().map(|step_id| step_id.as_str())
                        == Some(step.step_id.as_str())
                }) {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CASE_INVALID,
                        "unscheduled-step",
                        Some(step.step_id.as_str()),
                    ));
                }
            }
            // Acyclicity over the join partial order.
            if schedule_has_cycle(case) {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "cyclic-schedule",
                    Some(case.case_id.as_str()),
                ));
            }
            // One consistent outcome per scheduled contract; a
            // disturbed step never reports plain success.
            for outcome in &case.outcomes {
                let Some(step) = case.steps.iter().find(|candidate| {
                    candidate.contract_ref.as_str() == outcome.contract_ref.as_str()
                }) else {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CASE_INVALID,
                        "unknown-outcome",
                        Some(outcome.contract_ref.as_str()),
                    ));
                };
                if step.fault.disturbs() && outcome.outcome == Outcome::Success {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CASE_INVALID,
                        "fault-outcome-mismatch",
                        Some(outcome.contract_ref.as_str()),
                    ));
                }
            }
            for scheduled_ref in &scheduled {
                if !case
                    .outcomes
                    .iter()
                    .any(|outcome| outcome.contract_ref.as_str() == *scheduled_ref)
                {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CASE_INVALID,
                        "missing-outcome",
                        Some(scheduled_ref),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Depth-first cycle detection over the join partial order. Iterative
/// so no schedule bound can overflow the stack; a join that points at a
/// node still in progress is a back edge and proves a cycle.
fn schedule_has_cycle(case: &PartialFailureCase) -> bool {
    enum Mark {
        InProgress,
        Done,
    }
    let node_of = |id: &str| {
        case.schedule
            .iter()
            .find(|node| node.node_id.as_str() == id)
    };
    let mut marks: Vec<(&str, Mark)> = Vec::new();
    for root in &case.schedule {
        if marks
            .iter()
            .any(|(id, mark)| *id == root.node_id.as_str() && matches!(mark, Mark::Done))
        {
            continue;
        }
        if marks.iter().any(|(id, _)| *id == root.node_id.as_str()) {
            continue;
        }
        marks.push((root.node_id.as_str(), Mark::InProgress));
        let mut stack: Vec<(&str, usize)> = vec![(root.node_id.as_str(), 0)];
        while let Some((current, index)) = stack.pop() {
            let Some(owner) = node_of(current) else {
                continue;
            };
            if index < owner.joins.len() {
                stack.push((current, index + 1));
                let join = owner.joins[index].as_str();
                if marks
                    .iter()
                    .any(|(id, mark)| *id == join && matches!(mark, Mark::InProgress))
                {
                    return true;
                }
                if !marks.iter().any(|(id, _)| *id == join) {
                    marks.push((join, Mark::InProgress));
                    stack.push((join, 0));
                }
            } else if let Some(entry) = marks.iter_mut().find(|(id, _)| *id == current) {
                entry.1 = Mark::Done;
            }
        }
    }
    false
}

/// The typed effect-binding projection of one contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectBinding {
    /// The contract reference.
    pub contract_ref: String,
    /// The exact #14 EffectId spelling.
    pub effect: String,
    /// The bound #14 effect-kind key.
    pub kind: &'static str,
    /// Whether the effect graph receives this contract.
    pub graph: bool,
    /// Whether impact analysis receives this contract.
    pub impact: bool,
    /// Whether context capsules receive this contract.
    pub context: bool,
    /// Whether scenario coverage receives this contract.
    pub scenarios: bool,
}

/// An erased view over one contract of any kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContractView<'a> {
    Event(&'a EventContract),
    Job(&'a JobContract),
    Call(&'a CallContract),
    Cache(&'a CacheContract),
    Publication(&'a PublicationContract),
}

impl ContractView<'_> {
    /// The shared common members of the viewed contract.
    pub(crate) const fn common(&self) -> &contract::Common {
        match self {
            Self::Event(event) => &event.common,
            Self::Job(job) => &job.common,
            Self::Call(call) => &call.common,
            Self::Cache(cache) => &cache.common,
            Self::Publication(publication) => &publication.common,
        }
    }

    /// The contract reference text.
    pub(crate) fn ref_text(&self) -> &str {
        match self {
            Self::Event(contract) => contract.contract_ref(),
            Self::Job(contract) => contract.contract_ref(),
            Self::Call(contract) => contract.contract_ref(),
            Self::Cache(contract) => contract.contract_ref(),
            Self::Publication(contract) => contract.contract_ref(),
        }
    }

    /// The canonical effect spelling.
    pub(crate) fn effect_text(&self) -> String {
        match self {
            Self::Event(contract) => contract.common().effect.as_str(),
            Self::Job(contract) => contract.common().effect.as_str(),
            Self::Call(contract) => contract.common().effect.as_str(),
            Self::Cache(contract) => contract.common().effect.as_str(),
            Self::Publication(contract) => contract.common().effect.as_str(),
        }
    }

    /// The bound effect-kind key.
    pub(crate) const fn kind_key(&self) -> &'static str {
        match self {
            Self::Event(contract) => contract.common().effect.kind_key(),
            Self::Job(contract) => contract.common().effect.kind_key(),
            Self::Call(contract) => contract.common().effect.kind_key(),
            Self::Cache(contract) => contract.common().effect.kind_key(),
            Self::Publication(contract) => contract.common().effect.kind_key(),
        }
    }
}
