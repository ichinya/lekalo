//! Issue #24: the closed, versioned transaction-concurrency attachment.
//!
//! One independent, immutable contract family declaring transaction
//! (`required|optional|forbidden`) semantics, local atomic effect
//! groups, optimistic version/ETag and pessimistic lock preconditions,
//! closed isolation requirements, unique invariants, separate
//! idempotency and retry declarations, explicit partial-failure
//! boundaries, typed compensation references, capability requirement
//! records, and deterministic concurrency race cases keyed to Scenario
//! IR documents.
//!
//! Boundaries: this module is pure declaration and validation — no
//! runtime transaction execution, no adapter implementation, no
//! external-call discovery, no profile resolution, no report
//! persistence. Effect meaning stays with #14 (the attachment only
//! contributes typed group membership keyed by exact EffectIds);
//! scenario step identity, reachability, and execution stay with #23
//! and the owner harnesses (#31/#47/#56/#107); error-contract identity
//! stays with #62 (only opaque typed refs are carried); capability
//! registries, negotiation, and profile resolution stay with #27/#28/#29.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON with
//! byte-sorted keys; set-like collections normalize to sorted form while
//! semantically ordered membership, schedules, and boundaries keep their
//! declared order. Every bound and every semantic contradiction rejects
//! with an explicit registered diagnostic and no partial result.

pub mod canonical;
pub mod capability;
pub mod diff;
pub use diff::{compare, DiffClass, DiffPath, DiffResult};
pub(crate) mod diagnostic;
pub mod effect_group;
pub mod identity;
pub mod isolation;
pub mod lock;
pub mod operation;
pub mod precondition;
pub mod scenario;
pub mod version;
pub(crate) mod wire;

use crate::diagnostics::DiagnosticSet;
use crate::effects::{EffectGraph, OperationId};
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

pub use capability::{
    map_capabilities, CapabilityDecision, CapabilityProfile, CapabilitySnapshot,
    RequirementVerdict, SnapshotSupport,
};
pub use effect_group::{
    AtomicEffectGroup, BoundaryKind, BoundarySubject, CommitBoundary, FailureBoundary,
    OperationContract, TransactionMode,
};
pub use isolation::IsolationLevel;
pub use lock::{LockAcquisition, LockMode, LockScope, LockTimeout};
pub use operation::{
    DuplicatePolicy, Idempotency, IdempotencyMode, IdempotencyRecord, Retry, RetryCondition,
    RetryPhase, RetrySafety,
};
pub use precondition::{
    CapabilityId, CapabilityRequirement, Enforcement, EtagPrecondition, Invariant,
    InvariantPredicate, Literal, LockRequirement, Precondition, RequirementLevel, TokenSupply,
    VersionPrecondition,
};
pub use scenario::{
    Barrier, ConcurrencyCase, ExpectedOutcome, Invocation, OutcomeExpectation, Participant,
    ScenarioRef, ScheduleNode,
};
pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

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

/// One finished transaction-concurrency attachment: immutable,
/// deterministically ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionConcurrencyAttachment {
    project_id: SemanticId,
    model_ref: ModelPin,
    ir_digest: Sha256Digest,
    effect_graph_digest: Option<Sha256Digest>,
    operations: Vec<OperationContract>,
    invariants: Vec<Invariant>,
    cases: Vec<ConcurrencyCase>,
    capability_requirements: Vec<CapabilityRequirement>,
}

impl TransactionConcurrencyAttachment {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's normalized order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        project_id: SemanticId,
        model_ref: ModelPin,
        ir_digest: Sha256Digest,
        effect_graph_digest: Option<Sha256Digest>,
        operations: Vec<OperationContract>,
        invariants: Vec<Invariant>,
        cases: Vec<ConcurrencyCase>,
        capability_requirements: Vec<CapabilityRequirement>,
    ) -> Self {
        Self {
            project_id,
            model_ref,
            ir_digest,
            effect_graph_digest,
            operations,
            invariants,
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
    /// sorted set-like collections, exact declared order, no trailing
    /// LF), or a typed refusal beyond the payload bound.
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

    /// Every operation contract, canonically ordered by operation ref.
    pub fn operations(&self) -> &[OperationContract] {
        &self.operations
    }

    /// Every invariant, canonically ordered by invariant id.
    pub fn invariants(&self) -> &[Invariant] {
        &self.invariants
    }

    /// Every concurrency case, canonically ordered by case id.
    pub fn cases(&self) -> &[ConcurrencyCase] {
        &self.cases
    }

    /// Every capability requirement record, canonically ordered by
    /// requirement id.
    pub fn capability_requirements(&self) -> &[CapabilityRequirement] {
        &self.capability_requirements
    }

    /// The typed `TransactionGroupMembership` projection keyed by exact
    /// #14 EffectId and group: plain data #14 may display, never a graph
    /// mutation.
    pub fn group_membership(&self) -> Vec<TransactionGroupMembership> {
        let mut members = Vec::new();
        for operation in &self.operations {
            for group in &operation.groups {
                for effect in &group.effect_refs {
                    members.push(TransactionGroupMembership {
                        group_id: group.group_id.as_str().to_owned(),
                        operation: operation.operation_ref.as_str().to_owned(),
                        effect: effect.as_str(),
                    });
                }
            }
        }
        members.sort_by(|left, right| {
            (&left.group_id[..], &left.effect[..]).cmp(&(&right.group_id[..], &right.effect[..]))
        });
        members
    }

    /// Cross-validate this attachment against the accepted #14 effect
    /// graph it binds: every group effect reference must resolve to a
    /// declared edge of its operation, and every external declared edge
    /// of an attached operation must carry a compensating non-atomic
    /// failure boundary when the operation declares `required` or
    /// `optional`. Pure and read-only; never mutates the graph.
    pub fn validate_against_graph(&self, graph: &EffectGraph) -> Result<(), DiagnosticSet> {
        if let Some(bound) = &self.effect_graph_digest {
            if bound.as_str() != graph.ir_digest() {
                return Err(diagnostic::input_invalid("graph-digest-mismatch", None));
            }
        }
        for operation in &self.operations {
            let edges: Vec<String> = graph
                .operation_edges(&operation.operation_ref)
                .iter()
                .map(|edge| edge.key().to_canonical_string())
                .collect();
            for group in &operation.groups {
                for effect in &group.effect_refs {
                    if !edges.iter().any(|edge| *edge == effect.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::GROUP_OVERLAP,
                            "unresolved-effect-ref",
                            Some(&effect.as_str()),
                        ));
                    }
                }
            }
            if operation.transaction != TransactionMode::Forbidden {
                for effect in external_edges(graph, &operation.operation_ref) {
                    let covered = operation.failure_boundaries.iter().any(|boundary| {
                        matches!(&boundary.subject, BoundarySubject::Effect(subject) if subject.as_str() == effect)
                            && boundary.compensation_ref.is_some()
                    });
                    if !covered {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::PARTIAL_UNACKNOWLEDGED,
                            "uncovered-external-effect",
                            Some(&effect),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// The full semantic self-check: cross-operation identity
    /// uniqueness, group legality, compensation coverage, precondition
    /// consistency, idempotency/retry contradictions, capability
    /// coverage, and race-case schedule integrity.
    pub(crate) fn semantic_self_check(&self) -> Result<(), DiagnosticSet> {
        self.check_identities()?;
        self.check_groups()?;
        self.check_preconditions()?;
        self.check_idempotency_and_retry()?;
        self.check_capability_coverage()?;
        self.check_cases()?;
        Ok(())
    }

    /// Attachment-wide identity uniqueness.
    fn check_identities(&self) -> Result<(), DiagnosticSet> {
        let mut operation_refs: Vec<&str> = self
            .operations
            .iter()
            .map(|operation| operation.operation_ref.as_str())
            .collect();
        operation_refs.sort();
        operation_refs.dedup();
        if operation_refs.len() != self.operations.len() {
            return Err(diagnostic::input_invalid("duplicate-operation-ref", None));
        }
        let mut group_ids: Vec<&str> = self
            .operations
            .iter()
            .flat_map(|operation| operation.groups.iter())
            .map(|group| group.group_id.as_str())
            .collect();
        group_ids.sort_unstable();
        let group_total = group_ids.len();
        group_ids.dedup();
        if group_ids.len() != group_total {
            return Err(diagnostic::input_invalid("duplicate-group-id", None));
        }
        let mut boundary_ids: Vec<&str> = self
            .operations
            .iter()
            .flat_map(|operation| operation.failure_boundaries.iter())
            .map(|boundary| boundary.boundary_id.as_str())
            .collect();
        boundary_ids.sort_unstable();
        let boundary_total = boundary_ids.len();
        boundary_ids.dedup();
        if boundary_ids.len() != boundary_total {
            return Err(diagnostic::input_invalid("duplicate-boundary-id", None));
        }
        let mut requirement_ids: Vec<&str> = self
            .capability_requirements
            .iter()
            .map(|record| record.requirement_id.as_str())
            .collect();
        requirement_ids.sort_unstable();
        let requirement_total = requirement_ids.len();
        requirement_ids.dedup();
        if requirement_ids.len() != requirement_total {
            return Err(diagnostic::input_invalid("duplicate-requirement-id", None));
        }
        let mut invariant_ids: Vec<&str> = self
            .invariants
            .iter()
            .map(|record| record.invariant_id.as_str())
            .collect();
        invariant_ids.sort_unstable();
        let invariant_total = invariant_ids.len();
        invariant_ids.dedup();
        if invariant_ids.len() != invariant_total {
            return Err(diagnostic::input_invalid("duplicate-invariant-id", None));
        }
        let mut case_ids: Vec<&str> = self
            .cases
            .iter()
            .map(|record| record.case_id.as_str())
            .collect();
        case_ids.sort_unstable();
        let case_total = case_ids.len();
        case_ids.dedup();
        if case_ids.len() != case_total {
            return Err(diagnostic::input_invalid("duplicate-case-id", None));
        }
        Ok(())
    }

    /// Group legality per transaction mode and compensation coverage.
    fn check_groups(&self) -> Result<(), DiagnosticSet> {
        for operation in &self.operations {
            match operation.transaction {
                TransactionMode::Required => {
                    if operation.groups.is_empty() {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::GROUP_MISSING,
                            "required-without-group",
                            Some(operation.operation_ref.as_str()),
                        ));
                    }
                }
                TransactionMode::Forbidden => {
                    if !operation.groups.is_empty() {
                        return Err(diagnostic::input_invalid(
                            "forbidden-group-declared",
                            Some(operation.operation_ref.as_str()),
                        ));
                    }
                }
                TransactionMode::Optional => {}
            }
            let mut seen: Vec<String> = Vec::new();
            for group in &operation.groups {
                for effect in &group.effect_refs {
                    if seen.contains(&effect.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::GROUP_OVERLAP,
                            "duplicate-effect-ref",
                            Some(&effect.as_str()),
                        ));
                    }
                    seen.push(effect.as_str());
                }
            }
            for boundary in &operation.failure_boundaries {
                if let BoundarySubject::Effect(effect) = &boundary.subject {
                    if effect.is_external()
                        && operation.transaction != TransactionMode::Forbidden
                        && boundary.compensation_ref.is_none()
                    {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::PARTIAL_UNACKNOWLEDGED,
                            "missing-compensation",
                            Some(&effect.as_str()),
                        ));
                    }
                }
            }
            if operation.groups.iter().any(|group| group.commit_boundary == CommitBoundary::AfterExternalEffects)
                && !operation.failure_boundaries.iter().any(|boundary| {
                    matches!(&boundary.subject, BoundarySubject::Effect(effect) if effect.is_external())
                })
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "commit-boundary-without-external",
                    Some(operation.operation_ref.as_str()),
                ));
            }
        }
        Ok(())
    }

    /// Lock order consistency and distinct-resource bounds.
    fn check_preconditions(&self) -> Result<(), DiagnosticSet> {
        let mut order_keys: Vec<&str> = Vec::new();
        let mut lock_resources: Vec<&str> = Vec::new();
        for operation in &self.operations {
            for precondition in &operation.preconditions {
                if let Precondition::Lock(lock) = precondition {
                    if order_keys.contains(&lock.order_key.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::PRECONDITION_INVALID,
                            "duplicate-lock-order-key",
                            Some(lock.order_key.as_str()),
                        ));
                    }
                    order_keys.push(lock.order_key.as_str());
                    let identity = format!(
                        "{}|{}|{}",
                        lock.resource.as_str(),
                        lock.scope.key(),
                        lock.mode.key()
                    );
                    if lock_resources.contains(&identity.as_str()) {
                        return Err(diagnostic::input_invalid(
                            "duplicate-precondition",
                            Some(&identity),
                        ));
                    }
                    lock_resources.push(lock.resource.as_str());
                }
            }
        }
        lock_resources.sort_unstable();
        lock_resources.dedup();
        if lock_resources.len() > version::MAX_LOCK_RESOURCES {
            return Err(diagnostic::input_invalid("lock-resource-limit", None));
        }
        Ok(())
    }

    /// Idempotency and retry contradictions (never inferred, always
    /// declared).
    fn check_idempotency_and_retry(&self) -> Result<(), DiagnosticSet> {
        for operation in &self.operations {
            let idempotency = &operation.idempotency;
            let key_needed = matches!(
                idempotency.mode,
                IdempotencyMode::KeyRequired | IdempotencyMode::KeyOptional
            );
            if key_needed != idempotency.key_field.is_some() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "key-ref-mismatch",
                    Some(operation.operation_ref.as_str()),
                ));
            }
            if idempotency.mode == IdempotencyMode::KeyRequired
                && idempotency.record != IdempotencyRecord::Durable
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "key-requires-durable-record",
                    Some(operation.operation_ref.as_str()),
                ));
            }
            if matches!(
                idempotency.mode,
                IdempotencyMode::NotApplicable | IdempotencyMode::NotGuaranteed
            ) && idempotency.record != IdempotencyRecord::None
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "record-without-guarantee",
                    Some(operation.operation_ref.as_str()),
                ));
            }
            let retry = &operation.retry;
            let condition_legal = match retry.safety {
                RetrySafety::Safe => retry.condition == RetryCondition::None,
                RetrySafety::Conditional => matches!(
                    retry.condition,
                    RetryCondition::IdempotencyKey | RetryCondition::Reconciliation
                ),
                RetrySafety::Unsafe => retry.condition == RetryCondition::ManualOnly,
            };
            if !condition_legal {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "safety-condition-mismatch",
                    Some(operation.operation_ref.as_str()),
                ));
            }
            if retry.condition == RetryCondition::IdempotencyKey && !key_needed {
                return Err(diagnostic::rule_invalid(
                    diagnostic::RETRY_CONFLICT,
                    "condition-without-key",
                    Some(operation.operation_ref.as_str()),
                ));
            }
        }
        Ok(())
    }

    /// Every declared guarantee must carry its capability requirement
    /// record reference.
    fn check_capability_coverage(&self) -> Result<(), DiagnosticSet> {
        for operation in &self.operations {
            let mut required: Vec<CapabilityId> = Vec::new();
            if operation.transaction == TransactionMode::Required {
                required.push(CapabilityId::TransactionAtomicGroup);
                required.push(CapabilityId::TransactionRollback);
            }
            for precondition in &operation.preconditions {
                match precondition {
                    Precondition::Version(_) => {
                        required.push(CapabilityId::ConcurrencyCompareAndSet)
                    }
                    Precondition::Etag(_) => required.push(CapabilityId::ConcurrencyEtagIfMatch),
                    Precondition::Lock(lock) => {
                        required.push(match lock.mode {
                            LockMode::Shared => CapabilityId::LockShared,
                            LockMode::Exclusive => CapabilityId::LockExclusive,
                        });
                        match lock.scope {
                            LockScope::Key => required.push(CapabilityId::LockKey),
                            LockScope::Range => required.push(CapabilityId::LockRange),
                            LockScope::Entity => {}
                        }
                    }
                }
            }
            if operation.isolation != IsolationLevel::None {
                required.push(CapabilityId::Isolation(operation.isolation));
            }
            if operation.idempotency.mode == IdempotencyMode::KeyRequired {
                required.push(CapabilityId::IdempotencyDurableKey);
                required.push(CapabilityId::IdempotencyReplay);
            }
            if operation
                .failure_boundaries
                .iter()
                .any(|boundary| boundary.compensation_ref.is_some())
            {
                required.push(CapabilityId::ExternalCompensation);
            }
            for capability in required {
                let spelling = capability.to_wire();
                let covered = operation.capability_refs.iter().any(|reference| {
                    self.capability_requirements.iter().any(|record| {
                        record.requirement_id.as_str() == reference.as_str()
                            && record.capability.to_wire() == spelling
                    })
                });
                if !covered {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CAPABILITY_MISSING,
                        "missing-required-capability",
                        Some(&spelling),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Race-case schedule integrity: unique identities, resolvable
    /// references, exact scheduling, and acyclic partial orders.
    fn check_cases(&self) -> Result<(), DiagnosticSet> {
        for case in &self.cases {
            let mut participants: Vec<&str> = case
                .participants
                .iter()
                .map(|participant| participant.participant_id.as_str())
                .collect();
            participants.sort_unstable();
            let participant_total = participants.len();
            participants.dedup();
            if participants.len() != participant_total {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "duplicate-participant",
                    Some(case.case_id.as_str()),
                ));
            }
            for invocation in &case.invocations {
                let participant = case
                    .participants
                    .iter()
                    .find(|candidate| {
                        candidate.participant_id.as_str() == invocation.participant_id.as_str()
                    })
                    .ok_or_else(|| {
                        diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "unknown-participant",
                            Some(case.case_id.as_str()),
                        )
                    })?;
                if participant.step_id.as_str() != invocation.step_id.as_str() {
                    return Err(diagnostic::rule_invalid(
                        diagnostic::CASE_INVALID,
                        "invocation-step-mismatch",
                        Some(case.case_id.as_str()),
                    ));
                }
            }
            let mut invocation_ids: Vec<&str> = case
                .invocations
                .iter()
                .map(|invocation| invocation.invocation_id.as_str())
                .collect();
            invocation_ids.sort_unstable();
            let invocation_total = invocation_ids.len();
            invocation_ids.dedup();
            if invocation_ids.len() != invocation_total {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "duplicate-invocation",
                    Some(case.case_id.as_str()),
                ));
            }
            let mut scheduled: Vec<&str> = Vec::new();
            for node in &case.schedule {
                if let Some(invocation) = &node.invocation {
                    if !case
                        .invocations
                        .iter()
                        .any(|candidate| candidate.invocation_id.as_str() == invocation.as_str())
                    {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "unknown-invocation",
                            Some(case.case_id.as_str()),
                        ));
                    }
                    if scheduled.contains(&invocation.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "duplicate-schedule",
                            Some(case.case_id.as_str()),
                        ));
                    }
                    scheduled.push(invocation.as_str());
                }
            }
            if scheduled.len() != case.invocations.len() {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "unscheduled-invocation",
                    Some(case.case_id.as_str()),
                ));
            }
            let node_ids: Vec<&str> = case
                .schedule
                .iter()
                .map(|node| node.node_id.as_str())
                .collect();
            if node_ids.len()
                != node_ids
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "duplicate-node",
                    Some(case.case_id.as_str()),
                ));
            }
            for node in &case.schedule {
                for join in &node.joins {
                    if !node_ids.contains(&join.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "unknown-join-ref",
                            Some(case.case_id.as_str()),
                        ));
                    }
                }
            }
            if schedule_has_cycle(&case.schedule) {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "cyclic-schedule",
                    Some(case.case_id.as_str()),
                ));
            }
            for barrier in &case.barriers {
                for wait in &barrier.waits_for {
                    if !node_ids.contains(&wait.as_str()) {
                        return Err(diagnostic::rule_invalid(
                            diagnostic::CASE_INVALID,
                            "unknown-barrier-ref",
                            Some(case.case_id.as_str()),
                        ));
                    }
                }
            }
            let mut outcome_participants: Vec<&str> = case
                .expected_outcomes
                .iter()
                .map(|outcome| outcome.participant_id.as_str())
                .collect();
            outcome_participants.sort_unstable();
            let outcome_total = outcome_participants.len();
            outcome_participants.dedup();
            if outcome_participants.len() != outcome_total
                || outcome_participants.len() != participant_total
            {
                return Err(diagnostic::rule_invalid(
                    diagnostic::CASE_INVALID,
                    "outcome-mismatch",
                    Some(case.case_id.as_str()),
                ));
            }
        }
        Ok(())
    }
}

/// The typed transaction-group membership of one effect: plain data #14
/// may display. Atomicity meaning stays with #24; the graph never owns
/// it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TransactionGroupMembership {
    /// The group identifier.
    pub group_id: String,
    /// The acting operation reference.
    pub operation: String,
    /// The canonical EffectId spelling.
    pub effect: String,
}

/// Whether the declared partial order contains a cycle.
fn schedule_has_cycle(schedule: &[ScheduleNode]) -> bool {
    let mut pending: Vec<&ScheduleNode> = schedule.iter().collect();
    let mut done: Vec<&str> = Vec::new();
    let mut progress = true;
    while progress && !pending.is_empty() {
        progress = false;
        pending.retain(|node| {
            let ready = node.joins.iter().all(|join| done.contains(&join.as_str()));
            if ready {
                done.push(node.node_id.as_str());
                progress = true;
                false
            } else {
                true
            }
        });
    }
    !pending.is_empty()
}

/// The canonical spellings of every external declared edge of one
/// operation.
fn external_edges(graph: &EffectGraph, operation: &OperationId) -> Vec<String> {
    let mut external = Vec::new();
    for edge in graph.operation_edges(operation) {
        let key = edge.key();
        let kind_key = crate::effects::kind::EFFECT_KIND_KEYS[key.kind().rank() as usize];
        let is_external = matches!(
            kind_key,
            "external-call"
                | "enqueue-job"
                | "publish-output"
                | "cache-read"
                | "cache-write"
                | "cache-invalidate"
        );
        if is_external {
            external.push(key.to_canonical_string());
        }
    }
    external.sort();
    external.dedup();
    external
}
