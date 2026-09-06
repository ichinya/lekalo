//! The separate idempotency and retry declarations (issue #24).
//!
//! Retry safety is never inferred from idempotency: an operation may be
//! key-idempotent yet unsafe to retry past a commit, and vice versa. The
//! two declarations are typed independently and validated for
//! contradictions only at the operation level.

use std::fmt;

/// The closed idempotency mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum IdempotencyMode {
    /// The operation declares no results, so idempotency cannot apply.
    NotApplicable,
    /// Re-execution may duplicate effects; never advertise as safe
    /// retry.
    NotGuaranteed,
    /// The operation and effect semantics are repeat-safe without any
    /// key.
    Intrinsic,
    /// An exact typed request key is mandatory and durably recorded.
    KeyRequired,
    /// A request key may be supplied; its policy is declared.
    KeyOptional,
}

impl IdempotencyMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::NotGuaranteed => "not_guaranteed",
            Self::Intrinsic => "intrinsic",
            Self::KeyRequired => "key_required",
            Self::KeyOptional => "key_optional",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "not_applicable" => Some(Self::NotApplicable),
            "not_guaranteed" => Some(Self::NotGuaranteed),
            "intrinsic" => Some(Self::Intrinsic),
            "key_required" => Some(Self::KeyRequired),
            "key_optional" => Some(Self::KeyOptional),
            _ => None,
        }
    }
}

impl fmt::Display for IdempotencyMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// How a duplicate request with the same key is handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DuplicatePolicy {
    /// Return the prior result; no new effects.
    ReplayResult,
    /// Reject the duplicate with the typed conflict outcome.
    RejectSameKey,
    /// Join the in-flight request.
    JoinInFlight,
}

impl DuplicatePolicy {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ReplayResult => "replay_result",
            Self::RejectSameKey => "reject_same_key",
            Self::JoinInFlight => "join_in_flight",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "replay_result" => Some(Self::ReplayResult),
            "reject_same_key" => Some(Self::RejectSameKey),
            "join_in_flight" => Some(Self::JoinInFlight),
            _ => None,
        }
    }
}

/// The closed idempotency declaration. A replay with the same key and
/// the same canonical request digest returns the prior result and
/// creates no new effects; the same key with a different digest is a
/// typed conflict (`same_key_different_request` is the fixed value
/// `conflict`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Idempotency {
    pub(crate) mode: IdempotencyMode,
    pub(crate) key_field: Option<crate::effects::FieldName>,
    pub(crate) scope: crate::scenario::id::NamespacedId,
    pub(crate) record: IdempotencyRecord,
    pub(crate) duplicate: DuplicatePolicy,
}

impl Idempotency {
    /// The declared mode.
    pub const fn mode(&self) -> IdempotencyMode {
        self.mode
    }

    /// The exact request-key field reference, when declared.
    pub fn key_field(&self) -> Option<&crate::effects::FieldName> {
        self.key_field.as_ref()
    }

    /// The typed scope of the key record.
    pub fn scope(&self) -> &crate::scenario::id::NamespacedId {
        &self.scope
    }

    /// Whether the key record is durable.
    pub const fn record(&self) -> IdempotencyRecord {
        self.record
    }

    /// The duplicate policy.
    pub const fn duplicate(&self) -> DuplicatePolicy {
        self.duplicate
    }
}

/// Whether key records are durable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum IdempotencyRecord {
    /// The key record survives crashes.
    Durable,
    /// No record is kept.
    None,
}

impl IdempotencyRecord {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Durable => "durable",
            Self::None => "none",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "durable" => Some(Self::Durable),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// The closed retry-safety classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RetrySafety {
    /// A repeat cannot duplicate or violate effects.
    Safe,
    /// A repeat is safe only with the named proof.
    Conditional,
    /// Never automatically retried.
    Unsafe,
}

impl RetrySafety {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Conditional => "conditional",
            Self::Unsafe => "unsafe",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "safe" => Some(Self::Safe),
            "conditional" => Some(Self::Conditional),
            "unsafe" => Some(Self::Unsafe),
            _ => None,
        }
    }
}

/// The condition a retry must carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RetryCondition {
    /// Unconditional.
    None,
    /// The declared idempotency key.
    IdempotencyKey,
    /// A reconciliation proof.
    Reconciliation,
    /// A human decision.
    ManualOnly,
}

impl RetryCondition {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::IdempotencyKey => "idempotency_key",
            Self::Reconciliation => "reconciliation",
            Self::ManualOnly => "manual_only",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "none" => Some(Self::None),
            "idempotency_key" => Some(Self::IdempotencyKey),
            "reconciliation" => Some(Self::Reconciliation),
            "manual_only" => Some(Self::ManualOnly),
            _ => None,
        }
    }
}

/// The failure phase a retry declaration covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RetryPhase {
    /// The failure happened before the local commit.
    BeforeCommit,
    /// Post-commit transport loss: the result is unknown, and blind
    /// replay is forbidden.
    AfterCommitUnknown,
    /// An external effect partially failed.
    ExternalPartial,
}

impl RetryPhase {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::BeforeCommit => "before_commit",
            Self::AfterCommitUnknown => "after_commit_unknown",
            Self::ExternalPartial => "external_partial",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "before_commit" => Some(Self::BeforeCommit),
            "after_commit_unknown" => Some(Self::AfterCommitUnknown),
            "external_partial" => Some(Self::ExternalPartial),
            _ => None,
        }
    }
    /// The registry rank; canonical phase order.
    pub const fn rank(self) -> u8 {
        match self {
            Self::BeforeCommit => 0,
            Self::AfterCommitUnknown => 1,
            Self::ExternalPartial => 2,
        }
    }
}

/// The closed retry declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Retry {
    pub(crate) safety: RetrySafety,
    pub(crate) condition: RetryCondition,
    pub(crate) phases: Vec<RetryPhase>,
}

impl Retry {
    /// The safety classification.
    pub const fn safety(&self) -> RetrySafety {
        self.safety
    }

    /// The required condition.
    pub const fn condition(&self) -> RetryCondition {
        self.condition
    }

    /// The covered phases, deduplicated in registry order.
    pub fn phases(&self) -> &[RetryPhase] {
        &self.phases
    }
}
