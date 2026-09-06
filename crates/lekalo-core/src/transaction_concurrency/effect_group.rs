//! Operation contracts, atomic effect groups, and failure boundaries
//! (issue #24).

use crate::effects::TransactionGroupId;

use super::identity::EffectRef;
use super::precondition::Precondition;

/// The closed transaction semantics of one operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TransactionMode {
    /// Every declared atomic group commits all-or-nothing inside one
    /// local transactional boundary; an error or abort exposes zero
    /// committed effects from that group. Never a claim about external
    /// calls.
    Required,
    /// The implementation may use a transaction, but no atomic guarantee
    /// is promised: effects are potentially partial and evidence must
    /// state whether an actual group was atomic.
    Optional,
    /// No declared transaction boundary or group may be used;
    /// non-transactional, autocommit, and external effects remain
    /// independently observable.
    Forbidden,
}

impl TransactionMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Forbidden => "forbidden",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "required" => Some(Self::Required),
            "optional" => Some(Self::Optional),
            "forbidden" => Some(Self::Forbidden),
            _ => None,
        }
    }
}

/// The declared commit boundary of one atomic group relative to its
/// external effects. A declaration of sequencing, never a claim that an
/// external call is transactional.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CommitBoundary {
    /// The group commits before any external effect of the operation.
    BeforeExternalEffects,
    /// The group commits after the declared external effects.
    AfterExternalEffects,
}

impl CommitBoundary {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::BeforeExternalEffects => "before_external_effects",
            Self::AfterExternalEffects => "after_external_effects",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "before_external_effects" => Some(Self::BeforeExternalEffects),
            "after_external_effects" => Some(Self::AfterExternalEffects),
            _ => None,
        }
    }
}

/// One local all-or-nothing atomic effect group. Membership order is
/// semantic and is preserved exactly; scope and atomicity are the closed
/// v1 values (`local`, `all_or_nothing`); the only failure policy is
/// `abort`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicEffectGroup {
    pub(crate) group_id: TransactionGroupId,
    pub(crate) effect_refs: Vec<EffectRef>,
    pub(crate) commit_boundary: CommitBoundary,
}

impl AtomicEffectGroup {
    /// The validated group identifier (the #14 grammar).
    pub fn group_id(&self) -> &TransactionGroupId {
        &self.group_id
    }

    /// The declared effect membership in declared order.
    pub fn effect_refs(&self) -> &[EffectRef] {
        &self.effect_refs
    }

    /// The declared commit boundary.
    pub const fn commit_boundary(&self) -> CommitBoundary {
        self.commit_boundary
    }
}

/// What one failure boundary partitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BoundaryKind {
    /// A local atomic group: commits all-or-nothing or not at all.
    AtomicGroup,
    /// A non-atomic effect that can remain observable after a local
    /// rollback.
    NonAtomicEffect,
    /// A declared compensating recovery action.
    Compensation,
}

impl BoundaryKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::AtomicGroup => "atomic_group",
            Self::NonAtomicEffect => "non_atomic_effect",
            Self::Compensation => "compensation",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "atomic_group" => Some(Self::AtomicGroup),
            "non_atomic_effect" => Some(Self::NonAtomicEffect),
            "compensation" => Some(Self::Compensation),
            _ => None,
        }
    }
}

/// The subject of one failure boundary: a group, an effect, or a
/// compensating operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundarySubject {
    /// The atomic group this boundary covers.
    Group(TransactionGroupId),
    /// The effect this boundary declares non-atomic.
    Effect(EffectRef),
    /// The compensating operation reference.
    Operation(crate::effects::OperationId),
}

/// One explicit partial-failure boundary. Partial success is a
/// first-class degraded result, never atomic success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureBoundary {
    pub(crate) boundary_id: crate::scenario::id::NamespacedId,
    pub(crate) kind: BoundaryKind,
    pub(crate) subject: BoundarySubject,
    pub(crate) compensation_ref: Option<crate::effects::OperationId>,
    /// Compensation boundaries always declare `recovery_required`.
    pub(crate) recovery_declared: bool,
}

impl FailureBoundary {
    /// The validated boundary identifier.
    pub fn boundary_id(&self) -> &crate::scenario::id::NamespacedId {
        &self.boundary_id
    }

    /// The partition kind.
    pub const fn kind(&self) -> BoundaryKind {
        self.kind
    }

    /// The typed subject.
    pub const fn subject(&self) -> &BoundarySubject {
        &self.subject
    }

    /// The typed compensating operation reference, when declared.
    pub fn compensation_ref(&self) -> Option<&crate::effects::OperationId> {
        self.compensation_ref.as_ref()
    }

    /// Whether the recovery-required outcome is declared.
    pub const fn recovery_declared(&self) -> bool {
        self.recovery_declared
    }
}

/// One operation's full transaction-concurrency contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationContract {
    pub(crate) operation_ref: crate::effects::OperationId,
    pub(crate) operation_version: crate::lockfile::types::SemVer,
    pub(crate) transaction: TransactionMode,
    pub(crate) isolation: super::isolation::IsolationLevel,
    pub(crate) groups: Vec<AtomicEffectGroup>,
    pub(crate) preconditions: Vec<Precondition>,
    pub(crate) failure_boundaries: Vec<FailureBoundary>,
    pub(crate) idempotency: super::operation::Idempotency,
    pub(crate) retry: super::operation::Retry,
    pub(crate) capability_refs: Vec<crate::scenario::id::NamespacedId>,
}

impl OperationContract {
    /// The typed qualified operation reference (`operation:<semantic>`).
    pub fn operation_ref(&self) -> &crate::effects::OperationId {
        &self.operation_ref
    }

    /// The source semantic version of the operation (not a contract
    /// version).
    pub fn operation_version(&self) -> &crate::lockfile::types::SemVer {
        &self.operation_version
    }

    /// The closed transaction semantics.
    pub const fn transaction(&self) -> TransactionMode {
        self.transaction
    }

    /// The closed isolation requirement.
    pub const fn isolation(&self) -> super::isolation::IsolationLevel {
        self.isolation
    }

    /// The declared atomic groups in declared order.
    pub fn groups(&self) -> &[AtomicEffectGroup] {
        &self.groups
    }

    /// The declared preconditions in declared order.
    pub fn preconditions(&self) -> &[Precondition] {
        &self.preconditions
    }

    /// The declared failure boundaries in declared order.
    pub fn failure_boundaries(&self) -> &[FailureBoundary] {
        &self.failure_boundaries
    }

    /// The separate idempotency declaration.
    pub const fn idempotency(&self) -> &super::operation::Idempotency {
        &self.idempotency
    }

    /// The separate retry-safety declaration.
    pub const fn retry(&self) -> &super::operation::Retry {
        &self.retry
    }

    /// The referenced capability-requirement ids in declared order.
    pub fn capability_refs(&self) -> &[crate::scenario::id::NamespacedId] {
        &self.capability_refs
    }
}
