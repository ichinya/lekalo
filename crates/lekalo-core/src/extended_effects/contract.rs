//! The five extended-effect contract kinds (issue #26): events, jobs,
//! external calls, cache, and publications.
//!
//! Every kind shares one closed common member set — contract reference,
//! source version, exact #14 effect binding, sensitivity classification
//! with its security review gate hint, target-neutrality declaration,
//! the explicit contribution matrix, and capability-requirement
//! references — and adds the kind-specific vocabulary the issue defines.
//! Everything is a closed, bounded, typed value; nothing is inferred.

use crate::effects::{FieldName, OperationId};
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::NamespacedId;

use super::identity::{EffectRef, ErrorRef, ProviderContract};

/// The privacy/security classification of one effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Sensitivity {
    /// No special handling required.
    Standard,
    /// The effect touches sensitive data: the security review gate hint
    /// is mandatory.
    Sensitive,
}

impl Sensitivity {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Sensitive => "sensitive",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "standard" => Some(Self::Standard),
            "sensitive" => Some(Self::Sensitive),
            _ => None,
        }
    }
}

/// The target-neutrality declaration of one contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Portability {
    /// The contract is target-neutral; no target profile is referenced.
    Portable,
    /// The runtime-specific details live only in the referenced target
    /// profile.
    TargetSpecific,
}

impl Portability {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::TargetSpecific => "target-specific",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "portable" => Some(Self::Portable),
            "target-specific" => Some(Self::TargetSpecific),
            _ => None,
        }
    }
}

/// The explicit inclusion matrix: each surface receives the contract
/// only when its flag is true. Nothing is ever contributed implicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contributions {
    /// The #14 effect-graph contribution.
    pub graph: bool,
    /// The #16 impact-analysis contribution.
    pub impact: bool,
    /// The #17 context-capsule contribution.
    pub context: bool,
    /// The #23 scenario contribution.
    pub scenarios: bool,
}

impl Contributions {
    /// The wire flags in canonical member order.
    pub(crate) const KEYS: [&'static str; 4] = ["graph", "impact", "context", "scenarios"];
}

/// The security review gate hint a sensitive effect automatically
/// requires: an opaque typed pointer to the owning review contract.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SecurityGate {
    review_ref: NamespacedId,
}

impl SecurityGate {
    pub(crate) fn new(review_ref: NamespacedId) -> Self {
        Self { review_ref }
    }

    /// The review contract reference.
    pub fn review_ref(&self) -> &str {
        self.review_ref.as_str()
    }
}

/// The shared member set of every contract kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Common {
    pub(crate) contract_ref: NamespacedId,
    pub(crate) contract_version: SemVer,
    pub(crate) effect: EffectRef,
    pub(crate) sensitivity: Sensitivity,
    pub(crate) security_gate: Option<SecurityGate>,
    pub(crate) portability: Portability,
    pub(crate) target_profile: Option<NamespacedId>,
    pub(crate) contributions: Contributions,
    pub(crate) capability_refs: Vec<NamespacedId>,
}

/// The closed event delivery vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Delivery {
    /// In-process, transactional-outbox delivery with no persistence
    /// guarantee beyond the local commit.
    Local,
    /// Persisted delivery that survives process death.
    Durable,
}

impl Delivery {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Durable => "durable",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Self::Local),
            "durable" => Some(Self::Durable),
            _ => None,
        }
    }
}

/// The closed event ordering vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Ordering {
    /// No ordering guarantee.
    Unordered,
    /// Ordering is guaranteed within one partition key.
    PerKey,
    /// Total ordering across every consumer.
    Total,
}

impl Ordering {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Unordered => "unordered",
            Self::PerKey => "per_key",
            Self::Total => "total",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "unordered" => Some(Self::Unordered),
            "per_key" => Some(Self::PerKey),
            "total" => Some(Self::Total),
            _ => None,
        }
    }
}

/// The closed event deduplication declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deduplication {
    pub(crate) mode: DeduplicationMode,
    pub(crate) key_field: Option<FieldName>,
}

/// The closed deduplication modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeduplicationMode {
    /// Duplicates are possible and never suppressed.
    None,
    /// Bounded-window suppression keyed by one payload field.
    Key,
    /// Durable, unbounded suppression keyed by one payload field.
    DurableKey,
}

impl DeduplicationMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Key => "key",
            Self::DurableKey => "durable_key",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "key" => Some(Self::Key),
            "durable_key" => Some(Self::DurableKey),
            _ => None,
        }
    }
}

/// One event contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventContract {
    pub(crate) common: Common,
    pub(crate) event_version: SemVer,
    pub(crate) schema_digest: Sha256Digest,
    pub(crate) delivery: Delivery,
    pub(crate) ordering: Ordering,
    pub(crate) deduplication: Deduplication,
    pub(crate) correlation_id: Option<FieldName>,
    pub(crate) causation_id: Option<FieldName>,
}

impl EventContract {
    /// The contract reference.
    pub fn contract_ref(&self) -> &str {
        self.common.contract_ref.as_str()
    }

    /// The exact bound effect.
    pub(crate) const fn common(&self) -> &Common {
        &self.common
    }

    /// The declared event schema version.
    pub fn event_version(&self) -> &str {
        self.event_version.as_str()
    }

    /// The delivery guarantee.
    pub const fn delivery(&self) -> Delivery {
        self.delivery
    }

    /// The ordering guarantee.
    pub const fn ordering(&self) -> Ordering {
        self.ordering
    }

    /// The deduplication declaration.
    pub const fn deduplication(&self) -> &Deduplication {
        &self.deduplication
    }

    /// The correlation payload field, when declared.
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_ref().map(|field| field.as_str())
    }

    /// The causation payload field, when declared.
    pub fn causation_id(&self) -> Option<&str> {
        self.causation_id.as_ref().map(|field| field.as_str())
    }
}

/// The closed job retry declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobRetry {
    pub(crate) max_attempts: u8,
    pub(crate) backoff: Backoff,
    pub(crate) cap_millis: Option<u64>,
}

/// The closed backoff vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Backoff {
    /// Retries fire immediately; no ceiling applies.
    None,
    /// Fixed-delay retries with a declared ceiling.
    Fixed,
    /// Exponential retries with a declared ceiling.
    Exponential,
}

impl Backoff {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fixed => "fixed",
            Self::Exponential => "exponential",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "fixed" => Some(Self::Fixed),
            "exponential" => Some(Self::Exponential),
            _ => None,
        }
    }
}

/// The closed job idempotency declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobIdempotency {
    pub(crate) mode: JobIdempotencyMode,
    pub(crate) key_field: Option<FieldName>,
}

/// The closed job idempotency modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum JobIdempotencyMode {
    /// The job is not idempotent; retries are forbidden.
    None,
    /// Idempotency is keyed by one payload field.
    Key,
    /// The effect itself is idempotent by construction.
    Intrinsic,
}

impl JobIdempotencyMode {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Key => "key",
            Self::Intrinsic => "intrinsic",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "key" => Some(Self::Key),
            "intrinsic" => Some(Self::Intrinsic),
            _ => None,
        }
    }
}

/// The closed dead-letter vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeadLetter {
    /// Failed jobs are dropped after the retry budget.
    None,
    /// Failed jobs are parked in a dead-letter queue.
    Park,
    /// Failed jobs escalate to an operator surface.
    Escalate,
}

impl DeadLetter {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Park => "park",
            Self::Escalate => "escalate",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "park" => Some(Self::Park),
            "escalate" => Some(Self::Escalate),
            _ => None,
        }
    }
}

/// One job contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobContract {
    pub(crate) common: Common,
    pub(crate) payload_digest: Sha256Digest,
    pub(crate) queue_class: NamespacedId,
    pub(crate) retry: JobRetry,
    pub(crate) idempotency: JobIdempotency,
    pub(crate) timeout_millis: u64,
    pub(crate) dead_letter: DeadLetter,
}

impl JobContract {
    /// The contract reference.
    pub fn contract_ref(&self) -> &str {
        self.common.contract_ref.as_str()
    }

    /// The exact bound effect.
    pub(crate) const fn common(&self) -> &Common {
        &self.common
    }

    /// The declared queue class.
    pub fn queue_class(&self) -> &str {
        self.queue_class.as_str()
    }

    /// The retry declaration.
    pub const fn retry(&self) -> &JobRetry {
        &self.retry
    }

    /// The idempotency declaration.
    pub const fn idempotency(&self) -> &JobIdempotency {
        &self.idempotency
    }

    /// The declared timeout in milliseconds.
    pub const fn timeout_millis(&self) -> u64 {
        self.timeout_millis
    }

    /// The dead-letter policy.
    pub const fn dead_letter(&self) -> DeadLetter {
        self.dead_letter
    }
}

/// The closed external-call retry safety vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CallSafety {
    /// Retries are always safe.
    Safe,
    /// Retries are safe only under the provider's idempotency protocol.
    ConditionalIdempotent,
    /// Retries are unsafe; every re-attempt is manual.
    Unsafe,
}

impl CallSafety {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::ConditionalIdempotent => "conditional_idempotent",
            Self::Unsafe => "unsafe",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "safe" => Some(Self::Safe),
            "conditional_idempotent" => Some(Self::ConditionalIdempotent),
            "unsafe" => Some(Self::Unsafe),
            _ => None,
        }
    }
}

/// The closed external-call classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Classification {
    /// The call only reads provider state.
    Read,
    /// The call mutates provider state.
    Write,
    /// The call can destroy provider state irreversibly.
    Destructive,
}

impl Classification {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "destructive" => Some(Self::Destructive),
            _ => None,
        }
    }

    /// Whether the call mutates or destroys provider state.
    pub(crate) const fn mutates(self) -> bool {
        matches!(self, Self::Write | Self::Destructive)
    }
}

/// The closed external-call retry declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallRetry {
    pub(crate) max_attempts: u8,
    pub(crate) safety: CallSafety,
}

/// One external-call contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallContract {
    pub(crate) common: Common,
    pub(crate) provider: ProviderContract,
    pub(crate) request_digest: Sha256Digest,
    pub(crate) response_digest: Sha256Digest,
    pub(crate) error_refs: Vec<ErrorRef>,
    pub(crate) timeout_millis: u64,
    pub(crate) retry: CallRetry,
    pub(crate) classification: Classification,
    pub(crate) compensation: Option<OperationId>,
}

impl CallContract {
    /// The contract reference.
    pub fn contract_ref(&self) -> &str {
        self.common.contract_ref.as_str()
    }

    /// The exact bound effect.
    pub(crate) const fn common(&self) -> &Common {
        &self.common
    }

    /// The declared provider capability contract.
    pub fn provider(&self) -> &str {
        self.provider.as_str()
    }

    /// The closed typed error vocabulary, canonically sorted.
    pub fn error_refs(&self) -> impl Iterator<Item = &str> {
        self.error_refs.iter().map(|error| error.as_str())
    }

    /// The declared timeout in milliseconds.
    pub const fn timeout_millis(&self) -> u64 {
        self.timeout_millis
    }

    /// The retry declaration.
    pub const fn retry(&self) -> &CallRetry {
        &self.retry
    }

    /// The read/write/destructive classification.
    pub const fn classification(&self) -> Classification {
        self.classification
    }

    /// The compensating operation reference, when declared.
    pub fn compensation(&self) -> Option<&str> {
        self.compensation
            .as_ref()
            .map(|operation| operation.as_str())
    }
}

/// The closed cache operation vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CacheOperation {
    /// The cache is read.
    Read,
    /// The cache is written.
    Write,
    /// The cache is invalidated.
    Invalidate,
}

impl CacheOperation {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Invalidate => "invalidate",
        }
    }

    /// The #14 effect-kind key the operation binds to.
    pub(crate) const fn effect_kind(self) -> &'static str {
        match self {
            Self::Read => "cache-read",
            Self::Write => "cache-write",
            Self::Invalidate => "cache-invalidate",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "invalidate" => Some(Self::Invalidate),
            _ => None,
        }
    }
}

/// The closed cache consistency vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Consistency {
    /// Eventual consistency with no freshness bound.
    Eventual,
    /// A process observes its own writes.
    ReadYourWrites,
    /// Every observer sees the latest write.
    Strong,
}

impl Consistency {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Eventual => "eventual",
            Self::ReadYourWrites => "read_your_writes",
            Self::Strong => "strong",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "eventual" => Some(Self::Eventual),
            "read_your_writes" => Some(Self::ReadYourWrites),
            "strong" => Some(Self::Strong),
            _ => None,
        }
    }
}

/// The cache key contract: one exact version and the sorted key fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheKeyContract {
    pub(crate) key_version: SemVer,
    pub(crate) key_fields: Vec<FieldName>,
}

impl CacheKeyContract {
    /// The declared key version.
    pub fn key_version(&self) -> &str {
        self.key_version.as_str()
    }

    /// The sorted key fields.
    pub fn key_fields(&self) -> impl Iterator<Item = &str> {
        self.key_fields.iter().map(|field| field.as_str())
    }
}

/// The declared freshness window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheFreshness {
    pub(crate) ttl_millis: Option<u64>,
    pub(crate) max_stale_millis: Option<u64>,
}

/// One cache contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheContract {
    pub(crate) common: Common,
    pub(crate) key: CacheKeyContract,
    pub(crate) operations: Vec<CacheOperation>,
    pub(crate) consistency: Consistency,
    pub(crate) freshness: Option<CacheFreshness>,
}

impl CacheContract {
    /// The contract reference.
    pub fn contract_ref(&self) -> &str {
        self.common.contract_ref.as_str()
    }

    /// The exact bound effect.
    pub(crate) const fn common(&self) -> &Common {
        &self.common
    }

    /// The key contract.
    pub const fn key(&self) -> &CacheKeyContract {
        &self.key
    }

    /// The declared operations, canonically sorted.
    pub fn operations(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.operations.iter().map(|operation| operation.key())
    }

    /// The declared consistency level.
    pub const fn consistency(&self) -> Consistency {
        self.consistency
    }

    /// The declared freshness window, when declared.
    pub const fn freshness(&self) -> Option<&CacheFreshness> {
        self.freshness.as_ref()
    }
}

/// The closed publication approval vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Approval {
    /// Draft: internal authoring only.
    Draft,
    /// Preview: visible to a bounded preview audience.
    Preview,
    /// Approved: publication is consented by a named authority.
    Approved,
}

impl Approval {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Preview => "preview",
            Self::Approved => "approved",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "draft" => Some(Self::Draft),
            "preview" => Some(Self::Preview),
            "approved" => Some(Self::Approved),
            _ => None,
        }
    }
}

/// The explicit opt-in and approval contract of one publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Consent {
    pub(crate) approval: Approval,
    pub(crate) approver: Option<NamespacedId>,
}

impl Consent {
    /// The declared approval level.
    pub const fn approval(&self) -> Approval {
        self.approval
    }

    /// The approving authority reference, when approved.
    pub fn approver(&self) -> Option<&str> {
        self.approver.as_ref().map(|reference| reference.as_str())
    }
}

/// The immutable snapshot declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub(crate) revision_field: FieldName,
}

impl Snapshot {
    /// The immutable revision identifier field.
    pub fn revision_field(&self) -> &str {
        self.revision_field.as_str()
    }
}

/// One publication contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationContract {
    pub(crate) common: Common,
    pub(crate) destination: NamespacedId,
    pub(crate) consent: Consent,
    pub(crate) snapshot: Snapshot,
}

impl PublicationContract {
    /// The contract reference.
    pub fn contract_ref(&self) -> &str {
        self.common.contract_ref.as_str()
    }

    /// The exact bound effect.
    pub(crate) const fn common(&self) -> &Common {
        &self.common
    }

    /// The typed publication destination.
    pub fn destination(&self) -> &str {
        self.destination.as_str()
    }

    /// The explicit opt-in and approval contract.
    pub const fn consent(&self) -> &Consent {
        &self.consent
    }

    /// The immutable snapshot declaration.
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}
