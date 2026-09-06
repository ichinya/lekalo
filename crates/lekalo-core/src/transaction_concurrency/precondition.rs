//! Optimistic and pessimistic preconditions, unique invariants, and
//! capability requirements (issue #24).

use std::fmt;

use crate::effects::FieldName;

use super::identity::{DistributedProtocol, ErrorRef, EtagRef, OrderKey, ResourceRef};
use super::isolation::IsolationLevel;
use super::lock::{LockAcquisition, LockMode, LockScope, LockTimeout};

/// One optimistic compare-and-swap precondition on a version token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionPrecondition {
    pub(crate) resource: ResourceRef,
    pub(crate) token_field: FieldName,
    pub(crate) supplied_by: TokenSupply,
    pub(crate) mismatch: ErrorRef,
}

impl VersionPrecondition {
    /// The guarded resource.
    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    /// The guarded token field.
    pub fn token_field(&self) -> &FieldName {
        &self.token_field
    }

    /// Where the expected token comes from.
    pub const fn supplied_by(&self) -> TokenSupply {
        self.supplied_by
    }

    /// The typed conflict outcome reference.
    pub fn mismatch(&self) -> &ErrorRef {
        &self.mismatch
    }
}

/// One optimistic If-Match precondition on an opaque ETag token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EtagPrecondition {
    pub(crate) resource: ResourceRef,
    pub(crate) etag_ref: EtagRef,
    pub(crate) supplied_by: TokenSupply,
    pub(crate) mismatch: ErrorRef,
}

impl EtagPrecondition {
    /// The guarded resource.
    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    /// The typed reference to the token's carrying field.
    pub fn etag_ref(&self) -> &EtagRef {
        &self.etag_ref
    }

    /// Where the expected token comes from.
    pub const fn supplied_by(&self) -> TokenSupply {
        self.supplied_by
    }

    /// The typed conflict outcome reference.
    pub fn mismatch(&self) -> &ErrorRef {
        &self.mismatch
    }
}

/// Where an optimistic token comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TokenSupply {
    /// Carried on the request input.
    InputRef,
    /// Produced by a prior read.
    PriorReadRef,
}

impl TokenSupply {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::InputRef => "input_ref",
            Self::PriorReadRef => "prior_read_ref",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "input_ref" => Some(Self::InputRef),
            "prior_read_ref" => Some(Self::PriorReadRef),
            _ => None,
        }
    }
}

impl fmt::Display for TokenSupply {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// One pessimistic lock requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockRequirement {
    pub(crate) resource: ResourceRef,
    pub(crate) scope: LockScope,
    pub(crate) mode: LockMode,
    pub(crate) acquisition: LockAcquisition,
    pub(crate) order_key: OrderKey,
    pub(crate) timeout: LockTimeout,
}

impl LockRequirement {
    /// The locked resource selector.
    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    /// The selector scope.
    pub const fn scope(&self) -> LockScope {
        self.scope
    }

    /// The lock mode.
    pub const fn mode(&self) -> LockMode {
        self.mode
    }

    /// The acquisition point.
    pub const fn acquisition(&self) -> LockAcquisition {
        self.acquisition
    }

    /// The deterministic canonical acquisition key.
    pub fn order_key(&self) -> &OrderKey {
        &self.order_key
    }

    /// The timeout policy.
    pub const fn timeout(&self) -> LockTimeout {
        self.timeout
    }
}

/// The closed precondition union: optimistic version, optimistic ETag,
/// or pessimistic lock. Compare is always `exact` and the check always
/// participates in the local commit (`at_commit`); the wire carries
/// those fixed values and the normalizer enforces them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Precondition {
    /// Optimistic version compare-and-swap.
    Version(VersionPrecondition),
    /// Optimistic ETag / If-Match.
    Etag(EtagPrecondition),
    /// Pessimistic lock.
    Lock(LockRequirement),
}

impl Precondition {
    /// The kind key of this precondition.
    pub const fn kind_key(&self) -> &'static str {
        match self {
            Self::Version(_) => "version",
            Self::Etag(_) => "etag",
            Self::Lock(_) => "lock",
        }
    }

    /// The guarded resource selector.
    pub fn resource(&self) -> &ResourceRef {
        match self {
            Self::Version(precondition) => &precondition.resource,
            Self::Etag(precondition) => &precondition.resource,
            Self::Lock(precondition) => &precondition.resource,
        }
    }
}

/// The finite v1 predicate vocabulary of a unique invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvariantPredicate {
    /// The predicate field is not null.
    FieldNotNull(FieldName),
    /// The predicate field equals one closed typed literal.
    FieldEquals { field: FieldName, literal: Literal },
}

impl InvariantPredicate {
    /// The predicate field.
    pub fn field(&self) -> &FieldName {
        match self {
            Self::FieldNotNull(field) => field,
            Self::FieldEquals { field, .. } => field,
        }
    }
}

/// One closed typed literal for predicate use. No floats, no raw
/// objects, no attacker-controlled shapes.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Literal {
    /// True or false.
    Boolean(bool),
    /// A bounded signed integer.
    Integer(i64),
    /// A bounded lowercase identifier-like string.
    Text(String),
}

impl Literal {
    /// The exact wire spelling used by the canonical writer.
    pub fn to_wire(&self) -> String {
        match self {
            Self::Boolean(flag) => flag.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Text(text) => text.clone(),
        }
    }
}

/// One unique invariant under concurrency. Key fields, the predicate,
/// and the conflict outcome are typed, closed, and bounded; the v1
/// predicate vocabulary is finite and never an expression language.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invariant {
    pub(crate) invariant_id: crate::scenario::id::NamespacedId,
    pub(crate) resource: ResourceRef,
    pub(crate) key_fields: Vec<FieldName>,
    pub(crate) predicate: InvariantPredicate,
    pub(crate) violation: ErrorRef,
    pub(crate) enforcement: Enforcement,
}

impl Invariant {
    /// The validated invariant identifier.
    pub fn invariant_id(&self) -> &crate::scenario::id::NamespacedId {
        &self.invariant_id
    }

    /// The guarded resource selector.
    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    /// The uniqueness key fields in canonical byte order.
    pub fn key_fields(&self) -> &[FieldName] {
        &self.key_fields
    }

    /// The finite predicate.
    pub const fn predicate(&self) -> &InvariantPredicate {
        &self.predicate
    }

    /// The typed violation outcome reference.
    pub fn violation(&self) -> &ErrorRef {
        &self.violation
    }

    /// The declared enforcement mechanism.
    pub const fn enforcement(&self) -> Enforcement {
        self.enforcement
    }
}

/// The declared enforcement mechanism of one invariant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Enforcement {
    /// Optimistic compare-and-swap checks.
    Optimistic,
    /// Pessimistic locks.
    Pessimistic,
    /// A database-level constraint.
    DatabaseConstraint,
    /// Any of the above, decided by the target profile.
    Either,
}

impl Enforcement {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Optimistic => "optimistic",
            Self::Pessimistic => "pessimistic",
            Self::DatabaseConstraint => "database_constraint",
            Self::Either => "either",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "optimistic" => Some(Self::Optimistic),
            "pessimistic" => Some(Self::Pessimistic),
            "database_constraint" => Some(Self::DatabaseConstraint),
            "either" => Some(Self::Either),
            _ => None,
        }
    }
}

/// The closed capability identifiers (plus the typed
/// distributed-protocol form). The exact registry and handshake belong
/// to #27/#28; profile composition to #29.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CapabilityId {
    /// Local atomic all-or-nothing groups.
    TransactionAtomicGroup,
    /// Local rollback on failure.
    TransactionRollback,
    /// A specific isolation level.
    Isolation(IsolationLevel),
    /// Shared locks.
    LockShared,
    /// Exclusive locks.
    LockExclusive,
    /// Key-scoped locks.
    LockKey,
    /// Range-scoped locks.
    LockRange,
    /// Version compare-and-swap.
    ConcurrencyCompareAndSet,
    /// ETag If-Match.
    ConcurrencyEtagIfMatch,
    /// Unique constraints under concurrency.
    InvariantUniqueConcurrent,
    /// Durable idempotency keys.
    IdempotencyDurableKey,
    /// Replay of recorded results.
    IdempotencyReplay,
    /// Compensation of external effects.
    ExternalCompensation,
    /// An accepted distributed-protocol capability.
    DistributedProtocol(DistributedProtocol),
}

impl CapabilityId {
    /// The exact wire spelling.
    pub fn to_wire(&self) -> String {
        match self {
            Self::TransactionAtomicGroup => "transaction.atomic_group".to_owned(),
            Self::TransactionRollback => "transaction.rollback".to_owned(),
            Self::Isolation(level) => format!("isolation.{}", level.key()),
            Self::LockShared => "lock.shared".to_owned(),
            Self::LockExclusive => "lock.exclusive".to_owned(),
            Self::LockKey => "lock.key".to_owned(),
            Self::LockRange => "lock.range".to_owned(),
            Self::ConcurrencyCompareAndSet => "concurrency.compare_and_set".to_owned(),
            Self::ConcurrencyEtagIfMatch => "concurrency.etag_if_match".to_owned(),
            Self::InvariantUniqueConcurrent => "invariant.unique_concurrent".to_owned(),
            Self::IdempotencyDurableKey => "idempotency.durable_key".to_owned(),
            Self::IdempotencyReplay => "idempotency.replay".to_owned(),
            Self::ExternalCompensation => "external.compensation".to_owned(),
            Self::DistributedProtocol(protocol) => protocol.as_str().to_owned(),
        }
    }

    /// Parse one wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "transaction.atomic_group" => return Some(Self::TransactionAtomicGroup),
            "transaction.rollback" => return Some(Self::TransactionRollback),
            "lock.shared" => return Some(Self::LockShared),
            "lock.exclusive" => return Some(Self::LockExclusive),
            "lock.key" => return Some(Self::LockKey),
            "lock.range" => return Some(Self::LockRange),
            "concurrency.compare_and_set" => return Some(Self::ConcurrencyCompareAndSet),
            "concurrency.etag_if_match" => return Some(Self::ConcurrencyEtagIfMatch),
            "invariant.unique_concurrent" => return Some(Self::InvariantUniqueConcurrent),
            "idempotency.durable_key" => return Some(Self::IdempotencyDurableKey),
            "idempotency.replay" => return Some(Self::IdempotencyReplay),
            "external.compensation" => return Some(Self::ExternalCompensation),
            _ => {}
        }
        if let Some(level) = text.strip_prefix("isolation.") {
            return IsolationLevel::from_key(level).map(Self::Isolation);
        }
        if text.starts_with("distributed.") {
            return DistributedProtocol::parse(text)
                .ok()
                .map(Self::DistributedProtocol);
        }
        None
    }
}

/// The required guarantee level of one capability requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequirementLevel {
    /// The capability must be fully present.
    Full,
    /// A documented partial implementation may satisfy the requirement.
    Partial,
}

impl RequirementLevel {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "full" => Some(Self::Full),
            "partial" => Some(Self::Partial),
            _ => None,
        }
    }
}

/// One typed capability requirement record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRequirement {
    pub(crate) requirement_id: crate::scenario::id::NamespacedId,
    pub(crate) capability: CapabilityId,
    pub(crate) minimum: RequirementLevel,
    pub(crate) reason: String,
}

impl CapabilityRequirement {
    /// The validated requirement identifier.
    pub fn requirement_id(&self) -> &crate::scenario::id::NamespacedId {
        &self.requirement_id
    }

    /// The closed capability.
    pub const fn capability(&self) -> &CapabilityId {
        &self.capability
    }

    /// The minimum guarantee.
    pub const fn minimum(&self) -> RequirementLevel {
        self.minimum
    }

    /// The bounded human reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}
