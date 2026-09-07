//! Pure semantic comparison of two same-family attachments (issue #26).
//!
//! The comparison answers one question per changed path with one closed
//! class: **breaking** (a declared guarantee was removed or weakened),
//! **non-breaking** (an addition or a strengthening under the evolution
//! policy), and **policy-change** (capability, inclusion, or target
//! mapping that changes policy without changing guarantees). Invalid
//! inputs — foreign identities or mixed Model/IR/effect revisions — are
//! the typed error set, never a guessed classification. Paths are
//! deterministic and byte-sorted.

use crate::diagnostics::DiagnosticSet;

use super::contract::{
    Approval, CallSafety, Classification, Consistency, DeadLetter, DeduplicationMode, Delivery,
    JobIdempotencyMode, Ordering,
};
use super::diagnostic;
use super::ExtendedEffectsAttachment;

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or weakened.
    Breaking,
    /// An addition or descriptive change under the evolution policy.
    NonBreaking,
    /// A capability, inclusion, or target mapping change with unchanged
    /// guarantees.
    PolicyChange,
}

impl DiffClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
        }
    }
}

/// One changed path with its class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    class: DiffClass,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// The finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two same-family attachments. Pure and read-only.
pub fn compare(
    base: &ExtendedEffectsAttachment,
    candidate: &ExtendedEffectsAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::input_invalid("diff-project-mismatch", None));
    }
    if base.model_ref() != candidate.model_ref()
        || base.ir_digest().as_str() != candidate.ir_digest().as_str()
        || base.effect_graph_digest().map(|digest| digest.as_str())
            != candidate
                .effect_graph_digest()
                .map(|digest| digest.as_str())
    {
        return Err(diagnostic::input_invalid("diff-mixed-revision", None));
    }
    let mut paths: Vec<DiffPath> = Vec::new();
    compare_contracts(
        &kind_contracts(base),
        &kind_contracts(candidate),
        &mut paths,
    );
    compare_cases(base, candidate, &mut paths);
    compare_requirements(base, candidate, &mut paths);
    paths.sort();
    let equal = paths.is_empty();
    Ok(DiffResult { equal, paths })
}

/// One erased contract pairing: the kind prefix plus the typed views.
fn kind_contracts(
    attachment: &ExtendedEffectsAttachment,
) -> Vec<(&'static str, super::ContractView<'_>)> {
    let mut contracts: Vec<(&'static str, super::ContractView<'_>)> = Vec::new();
    for event in attachment.events() {
        contracts.push(("events", super::ContractView::Event(event)));
    }
    for job in attachment.jobs() {
        contracts.push(("jobs", super::ContractView::Job(job)));
    }
    for call in attachment.calls() {
        contracts.push(("calls", super::ContractView::Call(call)));
    }
    for cache in attachment.caches() {
        contracts.push(("cacheContracts", super::ContractView::Cache(cache)));
    }
    for publication in attachment.publications() {
        contracts.push((
            "publications",
            super::ContractView::Publication(publication),
        ));
    }
    contracts
}

/// Compare the contract collections kind by kind.
fn compare_contracts(
    base: &[(&'static str, super::ContractView<'_>)],
    candidate: &[(&'static str, super::ContractView<'_>)],
    paths: &mut Vec<DiffPath>,
) {
    for (kind, base_contract) in base {
        let matching = candidate
            .iter()
            .filter(|(candidate_kind, _)| *candidate_kind == *kind)
            .find(|(_, contract)| contract.ref_text() == base_contract.ref_text());
        match matching {
            Some((_, candidate_contract)) => {
                compare_one(kind, base_contract, candidate_contract, paths);
            }
            None => paths.push(DiffPath {
                path: format!("{kind}/{}", base_contract.ref_text()),
                class: DiffClass::Breaking,
            }),
        }
    }
    for (kind, candidate_contract) in candidate {
        let exists = base
            .iter()
            .filter(|(base_kind, _)| *base_kind == *kind)
            .any(|(_, contract)| contract.ref_text() == candidate_contract.ref_text());
        if !exists {
            paths.push(DiffPath {
                path: format!("{kind}/{}", candidate_contract.ref_text()),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// Compare one contract pair field by field.
fn compare_one(
    kind: &str,
    base: &super::ContractView<'_>,
    candidate: &super::ContractView<'_>,
    paths: &mut Vec<DiffPath>,
) {
    let prefix = |field: &str| format!("{kind}/{}/{field}", base.ref_text());
    let record = |paths: &mut Vec<DiffPath>, field: &str, class: DiffClass| {
        paths.push(DiffPath {
            path: prefix(field),
            class,
        });
    };
    // Common members.
    if base.common().contract_version.as_str() != candidate.common().contract_version.as_str() {
        record(paths, "contractVersion", DiffClass::NonBreaking);
    }
    if base.effect_text() != candidate.effect_text() {
        record(paths, "effectRef", DiffClass::Breaking);
    }
    if base.common().security_gate.is_some() != candidate.common().security_gate.is_some() {
        record(
            paths,
            "securityGate",
            if candidate.common().security_gate.is_none() {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            },
        );
    }
    if base.common().contributions != candidate.common().contributions
        || base.common().portability != candidate.common().portability
        || base.common().target_profile != candidate.common().target_profile
    {
        record(paths, "inclusion", DiffClass::PolicyChange);
    }
    if base.common().capability_refs != candidate.common().capability_refs {
        record(paths, "capabilityRequirementRefs", DiffClass::PolicyChange);
    }
    match (base, candidate) {
        (super::ContractView::Event(base), super::ContractView::Event(candidate)) => {
            compare_events(base, candidate, &prefix, paths);
        }
        (super::ContractView::Job(base), super::ContractView::Job(candidate)) => {
            compare_jobs(base, candidate, &prefix, paths);
        }
        (super::ContractView::Call(base), super::ContractView::Call(candidate)) => {
            compare_calls(base, candidate, &prefix, paths);
        }
        (super::ContractView::Cache(base), super::ContractView::Cache(candidate)) => {
            compare_caches(base, candidate, &prefix, paths);
        }
        (super::ContractView::Publication(base), super::ContractView::Publication(candidate)) => {
            compare_publications(base, candidate, &prefix, paths);
        }
        _ => unreachable!("matched contracts share their kind"),
    }
}

/// Event-specific field comparison.
fn compare_events(
    base: &super::EventContract,
    candidate: &super::EventContract,
    prefix: &impl Fn(&str) -> String,
    paths: &mut Vec<DiffPath>,
) {
    let record = |paths: &mut Vec<DiffPath>, field: &str, class: DiffClass| {
        paths.push(DiffPath {
            path: prefix(field),
            class,
        });
    };
    if base.event_version.as_str() != candidate.event_version.as_str() {
        record(paths, "eventVersion", DiffClass::NonBreaking);
    }
    if base.schema_digest.as_str() != candidate.schema_digest.as_str() {
        record(paths, "schemaDigest", DiffClass::Breaking);
    }
    if base.delivery != candidate.delivery {
        let class = match (base.delivery, candidate.delivery) {
            (Delivery::Durable, Delivery::Local) => DiffClass::Breaking,
            (Delivery::Local, Delivery::Durable) => DiffClass::NonBreaking,
            _ => DiffClass::PolicyChange,
        };
        record(paths, "delivery", class);
    }
    if base.ordering != candidate.ordering {
        let rank = |ordering: Ordering| match ordering {
            Ordering::Unordered => 0,
            Ordering::PerKey => 1,
            Ordering::Total => 2,
        };
        let class = if rank(candidate.ordering) < rank(base.ordering) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "ordering", class);
    }
    if base.deduplication.mode != candidate.deduplication.mode {
        let rank = |mode: DeduplicationMode| match mode {
            DeduplicationMode::None => 0,
            DeduplicationMode::Key => 1,
            DeduplicationMode::DurableKey => 2,
        };
        let class = if rank(candidate.deduplication.mode) < rank(base.deduplication.mode) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "deduplication", class);
    }
    if base.deduplication.key_field != candidate.deduplication.key_field {
        record(paths, "deduplicationKey", DiffClass::PolicyChange);
    }
    if base.correlation_id != candidate.correlation_id
        || base.causation_id != candidate.causation_id
    {
        let class = if base.correlation_id.is_some() && candidate.correlation_id.is_none()
            || base.causation_id.is_some() && candidate.causation_id.is_none()
        {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "traceIds", class);
    }
}

/// Job-specific field comparison.
fn compare_jobs(
    base: &super::JobContract,
    candidate: &super::JobContract,
    prefix: &impl Fn(&str) -> String,
    paths: &mut Vec<DiffPath>,
) {
    let record = |paths: &mut Vec<DiffPath>, field: &str, class: DiffClass| {
        paths.push(DiffPath {
            path: prefix(field),
            class,
        });
    };
    if base.payload_digest.as_str() != candidate.payload_digest.as_str() {
        record(paths, "payloadDigest", DiffClass::Breaking);
    }
    if base.queue_class != candidate.queue_class {
        record(paths, "queueClass", DiffClass::PolicyChange);
    }
    if base.retry.max_attempts != candidate.retry.max_attempts {
        let class = if candidate.retry.max_attempts < base.retry.max_attempts {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "retry", class);
    } else if base.retry.backoff != candidate.retry.backoff
        || base.retry.cap_millis != candidate.retry.cap_millis
    {
        record(paths, "retry", DiffClass::NonBreaking);
    }
    if base.idempotency.mode != candidate.idempotency.mode {
        let rank = |mode: JobIdempotencyMode| match mode {
            JobIdempotencyMode::None => 0,
            JobIdempotencyMode::Key => 1,
            JobIdempotencyMode::Intrinsic => 2,
        };
        let class = if rank(candidate.idempotency.mode) < rank(base.idempotency.mode) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "idempotency", class);
    } else if base.idempotency.key_field != candidate.idempotency.key_field {
        record(paths, "idempotencyKey", DiffClass::PolicyChange);
    }
    if base.timeout_millis != candidate.timeout_millis {
        record(paths, "timeoutMillis", DiffClass::PolicyChange);
    }
    if base.dead_letter != candidate.dead_letter {
        let rank = |letter: DeadLetter| match letter {
            DeadLetter::None => 0,
            DeadLetter::Park => 1,
            DeadLetter::Escalate => 2,
        };
        let class = if rank(candidate.dead_letter) < rank(base.dead_letter) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "deadLetter", class);
    }
}

/// External-call field comparison.
fn compare_calls(
    base: &super::CallContract,
    candidate: &super::CallContract,
    prefix: &impl Fn(&str) -> String,
    paths: &mut Vec<DiffPath>,
) {
    let record = |paths: &mut Vec<DiffPath>, field: &str, class: DiffClass| {
        paths.push(DiffPath {
            path: prefix(field),
            class,
        });
    };
    if base.provider != candidate.provider {
        record(paths, "providerContract", DiffClass::PolicyChange);
    }
    if base.request_digest.as_str() != candidate.request_digest.as_str()
        || base.response_digest.as_str() != candidate.response_digest.as_str()
    {
        record(paths, "schema", DiffClass::Breaking);
    }
    if base.error_refs != candidate.error_refs {
        let class = if candidate
            .error_refs
            .iter()
            .any(|error| !base.error_refs.contains(error))
        {
            DiffClass::NonBreaking
        } else {
            DiffClass::Breaking
        };
        record(paths, "errorRefs", class);
    }
    if base.timeout_millis != candidate.timeout_millis {
        record(paths, "timeoutMillis", DiffClass::PolicyChange);
    }
    if base.retry.max_attempts != candidate.retry.max_attempts {
        let class = if candidate.retry.max_attempts < base.retry.max_attempts {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "retry", class);
    } else if base.retry.safety != candidate.retry.safety {
        let rank = |safety: CallSafety| match safety {
            CallSafety::Unsafe => 0,
            CallSafety::ConditionalIdempotent => 1,
            CallSafety::Safe => 2,
        };
        let class = if rank(candidate.retry.safety) < rank(base.retry.safety) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "retry", class);
    }
    if base.classification != candidate.classification {
        let rank = |classification: Classification| match classification {
            Classification::Read => 0,
            Classification::Write => 1,
            Classification::Destructive => 2,
        };
        let class = if rank(candidate.classification) < rank(base.classification) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "classification", class);
    }
    if base.compensation != candidate.compensation {
        record(
            paths,
            "compensationRef",
            if candidate.compensation.is_none() {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            },
        );
    }
}

/// Cache field comparison.
fn compare_caches(
    base: &super::CacheContract,
    candidate: &super::CacheContract,
    prefix: &impl Fn(&str) -> String,
    paths: &mut Vec<DiffPath>,
) {
    let record = |paths: &mut Vec<DiffPath>, field: &str, class: DiffClass| {
        paths.push(DiffPath {
            path: prefix(field),
            class,
        });
    };
    if base.key.key_version.as_str() != candidate.key.key_version.as_str()
        || base.key.key_fields != candidate.key.key_fields
    {
        record(paths, "keyContract", DiffClass::Breaking);
    }
    if base.operations != candidate.operations {
        let class = if candidate
            .operations
            .iter()
            .any(|operation| !base.operations.contains(operation))
        {
            DiffClass::NonBreaking
        } else {
            DiffClass::Breaking
        };
        record(paths, "operations", class);
    }
    if base.consistency != candidate.consistency {
        let rank = |consistency: Consistency| match consistency {
            Consistency::Eventual => 0,
            Consistency::ReadYourWrites => 1,
            Consistency::Strong => 2,
        };
        let class = if rank(candidate.consistency) < rank(base.consistency) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "consistency", class);
    }
    if base.freshness != candidate.freshness {
        let weaker = |freshness: Option<super::CacheFreshness>| match freshness {
            None => 0,
            Some(freshness) => freshness.ttl_millis.is_some() as usize,
        };
        let class = if weaker(candidate.freshness) < weaker(base.freshness) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "freshness", class);
    }
}

/// Publication field comparison.
fn compare_publications(
    base: &super::PublicationContract,
    candidate: &super::PublicationContract,
    prefix: &impl Fn(&str) -> String,
    paths: &mut Vec<DiffPath>,
) {
    let record = |paths: &mut Vec<DiffPath>, field: &str, class: DiffClass| {
        paths.push(DiffPath {
            path: prefix(field),
            class,
        });
    };
    if base.destination != candidate.destination {
        record(paths, "destination", DiffClass::Breaking);
    }
    if base.consent.approval != candidate.consent.approval {
        let rank = |approval: Approval| match approval {
            Approval::Draft => 0,
            Approval::Preview => 1,
            Approval::Approved => 2,
        };
        let class = if rank(candidate.consent.approval) < rank(base.consent.approval) {
            DiffClass::Breaking
        } else {
            DiffClass::NonBreaking
        };
        record(paths, "consent", class);
    }
    if base.consent.approver != candidate.consent.approver {
        record(
            paths,
            "approverRef",
            if candidate.consent.approver.is_none() {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            },
        );
    }
    if base.snapshot.revision_field != candidate.snapshot.revision_field {
        record(paths, "snapshot", DiffClass::Breaking);
    }
}

/// Partial-failure case comparison: any content change is a semantic
/// change of the declared proof.
fn compare_cases(
    base: &ExtendedEffectsAttachment,
    candidate: &ExtendedEffectsAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for base_case in base.cases() {
        match candidate
            .cases()
            .iter()
            .find(|case| case.case_id.as_str() == base_case.case_id.as_str())
        {
            None => paths.push(DiffPath {
                path: format!("partialFailureCases/{}", base_case.case_id.as_str()),
                class: DiffClass::Breaking,
            }),
            Some(matched) if matched != base_case => paths.push(DiffPath {
                path: format!("partialFailureCases/{}", base_case.case_id.as_str()),
                class: DiffClass::Breaking,
            }),
            Some(_) => {}
        }
    }
    for candidate_case in candidate.cases() {
        if !base
            .cases()
            .iter()
            .any(|case| case.case_id.as_str() == candidate_case.case_id.as_str())
        {
            paths.push(DiffPath {
                path: format!("partialFailureCases/{}", candidate_case.case_id.as_str()),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// Capability-record comparison: policy, never guarantees.
fn compare_requirements(
    base: &ExtendedEffectsAttachment,
    candidate: &ExtendedEffectsAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for base_requirement in base.capability_requirements() {
        match candidate
            .capability_requirements()
            .iter()
            .find(|requirement| {
                requirement.requirement_id.as_str() == base_requirement.requirement_id.as_str()
            }) {
            None => paths.push(DiffPath {
                path: format!(
                    "capabilityRequirements/{}",
                    base_requirement.requirement_id.as_str()
                ),
                class: DiffClass::PolicyChange,
            }),
            Some(matched) if matched != base_requirement => paths.push(DiffPath {
                path: format!(
                    "capabilityRequirements/{}",
                    base_requirement.requirement_id.as_str()
                ),
                class: DiffClass::PolicyChange,
            }),
            Some(_) => {}
        }
    }
    for candidate_requirement in candidate.capability_requirements() {
        if !base.capability_requirements().iter().any(|requirement| {
            requirement.requirement_id.as_str() == candidate_requirement.requirement_id.as_str()
        }) {
            paths.push(DiffPath {
                path: format!(
                    "capabilityRequirements/{}",
                    candidate_requirement.requirement_id.as_str()
                ),
                class: DiffClass::PolicyChange,
            });
        }
    }
}
