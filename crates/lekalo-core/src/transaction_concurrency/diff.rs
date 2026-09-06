//! The pure semantic diff of two same-family transaction-concurrency
//! attachments (issue #24).
//!
//! The comparison never reads source, wire bytes, paths, clocks, or the
//! filesystem: both sides are already validated attachments. Diff paths
//! are deterministic and byte-sorted; every changed path carries one
//! closed class. Weakening a required guarantee (transaction mode,
//! atomic group coverage, isolation, locks, CAS/ETag, unique invariants,
//! idempotency/replay, compensation) is `breaking`; additions of new
//! independent operations or cases are `non-breaking`; capability-policy
//! mapping changes that leave every guarantee unchanged are
//! `policy-change`. Invalid inputs (mismatched family identity, or mixed
//! Model/IR/effect-graph revisions) are the typed error set, never a
//! guessed classification.

use super::effect_group::{BoundarySubject, OperationContract, TransactionMode};
use super::operation::{Idempotency, IdempotencyMode, Retry, RetrySafety};
use super::precondition::{Invariant, Precondition};
use super::TransactionConcurrencyAttachment;
use crate::diagnostics::DiagnosticSet;

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or weakened.
    Breaking,
    /// An addition or descriptive change under the evolution policy.
    NonBreaking,
    /// A capability-policy mapping change with unchanged guarantees.
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

/// One changed path with its classification.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    class: DiffClass,
}

impl DiffPath {
    /// The deterministic path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// One finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically identical.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths in deterministic byte order.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two validated attachments of the same contract family.
pub fn compare(
    base: &TransactionConcurrencyAttachment,
    candidate: &TransactionConcurrencyAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(super::diagnostic::input_invalid(
            "diff-project-mismatch",
            None,
        ));
    }
    if base.model_ref().version().as_str() != candidate.model_ref().version().as_str()
        || base.model_ref().digest().as_str() != candidate.model_ref().digest().as_str()
        || base.ir_digest().as_str() != candidate.ir_digest().as_str()
        || base.effect_graph_digest().map(|digest| digest.as_str())
            != candidate
                .effect_graph_digest()
                .map(|digest| digest.as_str())
    {
        return Err(super::diagnostic::input_invalid(
            "diff-mixed-revision",
            None,
        ));
    }
    let mut paths: Vec<DiffPath> = Vec::new();
    compare_operations(base, candidate, &mut paths);
    compare_invariants(base, candidate, &mut paths);
    compare_cases(base, candidate, &mut paths);
    compare_requirements(base, candidate, &mut paths);
    paths.sort();
    let equal = paths.is_empty();
    Ok(DiffResult { equal, paths })
}

/// Compare the operation contracts.
fn compare_operations(
    base: &TransactionConcurrencyAttachment,
    candidate: &TransactionConcurrencyAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for base_operation in base.operations() {
        let Some(candidate_operation) = candidate
            .operations()
            .iter()
            .find(|operation| operation.operation_ref() == base_operation.operation_ref())
        else {
            // A removed operation removes its required guarantees when
            // it was required; otherwise it is an independent removal.
            let class = if base_operation.transaction() == TransactionMode::Required {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            };
            paths.push(DiffPath {
                path: format!("operations/{}", base_operation.operation_ref().as_str()),
                class,
            });
            continue;
        };
        let prefix = format!("operations/{}", base_operation.operation_ref().as_str());
        if base_operation.transaction() != candidate_operation.transaction() {
            let class = transaction_change(
                base_operation.transaction(),
                candidate_operation.transaction(),
            );
            paths.push(DiffPath {
                path: format!("{prefix}/transaction"),
                class,
            });
        }
        if base_operation.isolation() != candidate_operation.isolation() {
            // Weakening (the new level no longer satisfies the old
            // requirement) is breaking; strengthening is not.
            let class = match candidate_operation
                .isolation()
                .satisfies(base_operation.isolation())
            {
                Some(true) => DiffClass::NonBreaking,
                _ => DiffClass::Breaking,
            };
            paths.push(DiffPath {
                path: format!("{prefix}/isolationRequirement"),
                class,
            });
        }
        compare_groups(base_operation, candidate_operation, &prefix, paths);
        compare_preconditions(base_operation, candidate_operation, &prefix, paths);
        compare_boundaries(base_operation, candidate_operation, &prefix, paths);
        if base_operation.idempotency() != candidate_operation.idempotency() {
            let class = idempotency_change(
                base_operation.idempotency(),
                candidate_operation.idempotency(),
            );
            paths.push(DiffPath {
                path: format!("{prefix}/idempotency"),
                class,
            });
        }
        if base_operation.retry() != candidate_operation.retry() {
            let class = retry_change(base_operation.retry(), candidate_operation.retry());
            paths.push(DiffPath {
                path: format!("{prefix}/retry"),
                class,
            });
        }
        if canonical_refs(base_operation.capability_refs())
            != canonical_refs(candidate_operation.capability_refs())
        {
            paths.push(DiffPath {
                path: format!("{prefix}/capabilityRequirements"),
                class: DiffClass::PolicyChange,
            });
        }
    }
    for candidate_operation in candidate.operations() {
        if !base
            .operations()
            .iter()
            .any(|operation| operation.operation_ref() == candidate_operation.operation_ref())
        {
            paths.push(DiffPath {
                path: format!(
                    "operations/{}",
                    candidate_operation.operation_ref().as_str()
                ),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// A required-to-weaker mode change is breaking; a forbidden-to-stronger
/// change adds a guarantee without removing one.
fn transaction_change(base: TransactionMode, candidate: TransactionMode) -> DiffClass {
    match (base, candidate) {
        (TransactionMode::Forbidden, TransactionMode::Optional)
        | (TransactionMode::Forbidden, TransactionMode::Required) => DiffClass::NonBreaking,
        _ => DiffClass::Breaking,
    }
}

/// Compare the atomic groups of one operation.
fn compare_groups(
    base: &OperationContract,
    candidate: &OperationContract,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for base_group in base.groups() {
        match candidate
            .groups()
            .iter()
            .find(|group| group.group_id() == base_group.group_id())
        {
            None => {
                // Removing coverage from a required operation is
                // breaking; optional groups are descriptive.
                let class = if base.transaction() == TransactionMode::Required {
                    DiffClass::Breaking
                } else {
                    DiffClass::NonBreaking
                };
                paths.push(DiffPath {
                    path: format!("{}/atomicEffectGroups/{}", prefix, base_group.group_id()),
                    class,
                });
            }
            Some(candidate_group) => {
                let added = candidate_group
                    .effect_refs()
                    .iter()
                    .any(|effect| !base_group.effect_refs().contains(effect));
                let removed = base_group
                    .effect_refs()
                    .iter()
                    .any(|effect| !candidate_group.effect_refs().contains(effect));
                if added || removed {
                    // Any membership change to a required group's
                    // guarantee is breaking; optional groups are
                    // descriptive.
                    let class = if base.transaction() == TransactionMode::Required {
                        DiffClass::Breaking
                    } else {
                        DiffClass::NonBreaking
                    };
                    paths.push(DiffPath {
                        path: format!(
                            "{}/atomicEffectGroups/{}/effectRefs",
                            prefix,
                            base_group.group_id()
                        ),
                        class,
                    });
                }
                if base_group.commit_boundary() != candidate_group.commit_boundary() {
                    paths.push(DiffPath {
                        path: format!(
                            "{}/atomicEffectGroups/{}/commitBoundary",
                            prefix,
                            base_group.group_id()
                        ),
                        class: DiffClass::NonBreaking,
                    });
                }
            }
        }
    }
    for candidate_group in candidate.groups() {
        if !base
            .groups()
            .iter()
            .any(|group| group.group_id() == candidate_group.group_id())
        {
            paths.push(DiffPath {
                path: format!(
                    "{}/atomicEffectGroups/{}",
                    prefix,
                    candidate_group.group_id()
                ),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// Compare the preconditions of one operation (resource-keyed).
fn compare_preconditions(
    base: &OperationContract,
    candidate: &OperationContract,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for (base_key, base_precondition) in precondition_keys(base.preconditions()) {
        match candidate
            .preconditions()
            .iter()
            .find(|item| precondition_key(item) == base_key)
        {
            None => {
                // Removing a declared concurrency control removes a
                // guarantee.
                paths.push(DiffPath {
                    path: format!("{prefix}/preconditions/{base_key}"),
                    class: DiffClass::Breaking,
                });
            }
            Some(candidate_precondition) => {
                if !precondition_equal(base_precondition, candidate_precondition) {
                    // A changed conflict outcome or order key changes
                    // conflict behavior.
                    paths.push(DiffPath {
                        path: format!("{prefix}/preconditions/{base_key}"),
                        class: DiffClass::Breaking,
                    });
                }
            }
        }
    }
    for (candidate_key, _) in precondition_keys(candidate.preconditions()) {
        if !base
            .preconditions()
            .iter()
            .any(|item| precondition_key(item) == candidate_key)
        {
            paths.push(DiffPath {
                path: format!("{prefix}/preconditions/{candidate_key}"),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// The keyed view of a precondition list.
fn precondition_keys(preconditions: &[Precondition]) -> Vec<(String, &Precondition)> {
    let mut keys: Vec<(String, &Precondition)> = preconditions
        .iter()
        .map(|item| (precondition_key(item), item))
        .collect();
    keys.sort_by(|left, right| left.0.cmp(&right.0));
    keys
}

/// The stable key of one precondition.
fn precondition_key(precondition: &Precondition) -> String {
    match precondition {
        Precondition::Version(version) => format!("version|{}", version.resource().as_str()),
        Precondition::Etag(etag) => format!(
            "etag|{}|{}",
            etag.resource().as_str(),
            etag.etag_ref().as_str()
        ),
        Precondition::Lock(lock) => format!(
            "lock|{}|{}|{}",
            lock.resource().as_str(),
            lock.scope().key(),
            lock.mode().key()
        ),
    }
}

/// Full equality of one precondition beyond its key.
fn precondition_equal(base: &Precondition, candidate: &Precondition) -> bool {
    match (base, candidate) {
        (Precondition::Version(left), Precondition::Version(right)) => {
            left.token_field() == right.token_field()
                && left.supplied_by() == right.supplied_by()
                && left.mismatch().as_str() == right.mismatch().as_str()
        }
        (Precondition::Etag(left), Precondition::Etag(right)) => {
            left.supplied_by() == right.supplied_by()
                && left.mismatch().as_str() == right.mismatch().as_str()
        }
        (Precondition::Lock(left), Precondition::Lock(right)) => {
            left.acquisition() == right.acquisition()
                && left.order_key().as_str() == right.order_key().as_str()
                && left.timeout() == right.timeout()
        }
        _ => false,
    }
}

/// Compare the failure boundaries of one operation (id-keyed).
fn compare_boundaries(
    base: &OperationContract,
    candidate: &OperationContract,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for base_boundary in base.failure_boundaries() {
        match candidate
            .failure_boundaries()
            .iter()
            .find(|boundary| boundary.boundary_id() == base_boundary.boundary_id())
        {
            None => {
                // Removing a compensation boundary removes a recovery
                // guarantee.
                paths.push(DiffPath {
                    path: format!(
                        "{}/failureBoundaries/{}",
                        prefix,
                        base_boundary.boundary_id().as_str()
                    ),
                    class: if base_boundary.kind()
                        == super::effect_group::BoundaryKind::Compensation
                        || base_boundary.compensation_ref().is_some()
                    {
                        DiffClass::Breaking
                    } else {
                        DiffClass::NonBreaking
                    },
                });
            }
            Some(candidate_boundary) => {
                if boundary_subject(base_boundary) != boundary_subject(candidate_boundary)
                    || boundary_compensation(base_boundary)
                        != boundary_compensation(candidate_boundary)
                {
                    paths.push(DiffPath {
                        path: format!(
                            "{}/failureBoundaries/{}",
                            prefix,
                            base_boundary.boundary_id().as_str()
                        ),
                        class: DiffClass::Breaking,
                    });
                }
            }
        }
    }
    for candidate_boundary in candidate.failure_boundaries() {
        if !base
            .failure_boundaries()
            .iter()
            .any(|boundary| boundary.boundary_id() == candidate_boundary.boundary_id())
        {
            paths.push(DiffPath {
                path: format!(
                    "{}/failureBoundaries/{}",
                    prefix,
                    candidate_boundary.boundary_id().as_str()
                ),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// The subject spelling of one boundary.
fn boundary_subject(boundary: &super::effect_group::FailureBoundary) -> String {
    match boundary.subject() {
        BoundarySubject::Group(group) => group.as_str().to_owned(),
        BoundarySubject::Effect(effect) => effect.as_str(),
        BoundarySubject::Operation(operation) => operation.as_str().to_owned(),
    }
}

/// The compensation spelling of one boundary, if any.
fn boundary_compensation(boundary: &super::effect_group::FailureBoundary) -> String {
    boundary
        .compensation_ref()
        .map(|reference| reference.as_str().to_owned())
        .unwrap_or_default()
}

/// Idempotency weakening is breaking; strengthening is not.
fn idempotency_change(base: &Idempotency, candidate: &Idempotency) -> DiffClass {
    let weakened = match (base.mode(), candidate.mode()) {
        (
            IdempotencyMode::KeyRequired,
            IdempotencyMode::KeyOptional
            | IdempotencyMode::Intrinsic
            | IdempotencyMode::NotGuaranteed
            | IdempotencyMode::NotApplicable,
        ) => true,
        (IdempotencyMode::Intrinsic, IdempotencyMode::NotGuaranteed)
        | (IdempotencyMode::Intrinsic, IdempotencyMode::NotApplicable) => true,
        _ => base.record() != candidate.record() || base.duplicate() != candidate.duplicate(),
    };
    if weakened {
        DiffClass::Breaking
    } else {
        DiffClass::NonBreaking
    }
}

/// Retry-safety weakening is breaking; strengthening is not.
fn retry_change(base: &Retry, candidate: &Retry) -> DiffClass {
    let weakened = matches!(
        (base.safety(), candidate.safety()),
        (
            RetrySafety::Safe,
            RetrySafety::Conditional | RetrySafety::Unsafe
        ) | (RetrySafety::Conditional, RetrySafety::Unsafe)
    );
    if weakened {
        DiffClass::Breaking
    } else {
        DiffClass::NonBreaking
    }
}

/// Compare the invariants (id-keyed).
fn compare_invariants(
    base: &TransactionConcurrencyAttachment,
    candidate: &TransactionConcurrencyAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for base_invariant in base.invariants() {
        match candidate
            .invariants()
            .iter()
            .find(|invariant| invariant.invariant_id() == base_invariant.invariant_id())
        {
            None => {
                paths.push(DiffPath {
                    path: format!("invariants/{}", base_invariant.invariant_id().as_str()),
                    class: DiffClass::Breaking,
                });
            }
            Some(candidate_invariant) => {
                if invariant_semantics(base_invariant) != invariant_semantics(candidate_invariant) {
                    paths.push(DiffPath {
                        path: format!("invariants/{}", base_invariant.invariant_id().as_str()),
                        class: DiffClass::Breaking,
                    });
                }
            }
        }
    }
    for candidate_invariant in candidate.invariants() {
        if !base
            .invariants()
            .iter()
            .any(|invariant| invariant.invariant_id() == candidate_invariant.invariant_id())
        {
            paths.push(DiffPath {
                path: format!("invariants/{}", candidate_invariant.invariant_id().as_str()),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// The semantic facts of one invariant: the constraint, not its
/// enforcement mechanism (which may change freely).
fn invariant_semantics(invariant: &Invariant) -> (String, String, Vec<String>, String, String) {
    (
        invariant.resource().as_str().to_owned(),
        invariant
            .key_fields()
            .iter()
            .map(|field| field.as_str().to_owned())
            .collect(),
        match invariant.predicate() {
            super::precondition::InvariantPredicate::FieldNotNull(field) => {
                vec!["field_not_null".to_owned(), field.as_str().to_owned()]
            }
            super::precondition::InvariantPredicate::FieldEquals { field, literal } => {
                vec![
                    "field_equals".to_owned(),
                    field.as_str().to_owned(),
                    literal.to_wire(),
                ]
            }
        },
        invariant.violation().as_str().to_owned(),
        invariant.enforcement().key().to_owned(),
    )
}

/// Compare the concurrency cases (id-keyed). Cases are test vectors,
/// not guarantees: any change is non-breaking.
fn compare_cases(
    base: &TransactionConcurrencyAttachment,
    candidate: &TransactionConcurrencyAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for base_case in base.cases() {
        let exists = candidate
            .cases()
            .iter()
            .any(|case| case.case_id() == base_case.case_id());
        if !exists {
            paths.push(DiffPath {
                path: format!("concurrencyCases/{}", base_case.case_id().as_str()),
                class: DiffClass::NonBreaking,
            });
        }
    }
    for candidate_case in candidate.cases() {
        let exists = base
            .cases()
            .iter()
            .any(|case| case.case_id() == candidate_case.case_id());
        if !exists {
            paths.push(DiffPath {
                path: format!("concurrencyCases/{}", candidate_case.case_id().as_str()),
                class: DiffClass::NonBreaking,
            });
        }
    }
}

/// Compare the capability-requirement records. Requirement records are
/// target-policy mapping: their changes are policy changes, never
/// contract-guarantee changes.
fn compare_requirements(
    base: &TransactionConcurrencyAttachment,
    candidate: &TransactionConcurrencyAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for base_requirement in base.capability_requirements() {
        let changed = candidate
            .capability_requirements()
            .iter()
            .find(|requirement| requirement.requirement_id() == base_requirement.requirement_id())
            .map(|requirement| {
                requirement.capability().to_wire() != base_requirement.capability().to_wire()
                    || requirement.minimum() != base_requirement.minimum()
            })
            .unwrap_or(true);
        if changed {
            paths.push(DiffPath {
                path: format!(
                    "capabilityRequirements/{}",
                    base_requirement.requirement_id().as_str()
                ),
                class: DiffClass::PolicyChange,
            });
        }
    }
    for candidate_requirement in candidate.capability_requirements() {
        if !base.capability_requirements().iter().any(|requirement| {
            requirement.requirement_id() == candidate_requirement.requirement_id()
        }) {
            paths.push(DiffPath {
                path: format!(
                    "capabilityRequirements/{}",
                    candidate_requirement.requirement_id().as_str()
                ),
                class: DiffClass::PolicyChange,
            });
        }
    }
}

/// The sorted spelling set of one reference list.
fn canonical_refs(refs: &[crate::scenario::id::NamespacedId]) -> Vec<&str> {
    let mut sorted: Vec<&str> = refs.iter().map(|reference| reference.as_str()).collect();
    sorted.sort();
    sorted
}
