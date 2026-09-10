//! The typed observed-mode model (issues #39 and #42).
//!
//! One project carries at most one observed index. Every fact it records
//! is either **confirmed data** (an explicit or confirmed binding the user
//! declared) or **inferred data** (an adapter scan mapping), and the two
//! are never interchangeable: promotion requires the confirmed kinds,
//! inferred facts never become canonical by themselves, and missing
//! evidence is `unknown`, never the absence of behavior.
//!
//! Issue #42 adds the binding-registry surface: per-binding native
//! candidate sets that keep an ambiguous mapping visible instead of
//! silently resolved, the declared target/profile of the producing scan,
//! and native test bindings with their own freshness state.

use serde::{Deserialize, Serialize};

use super::version;

// ---------------------------------------------------------------------------
// Closed vocabularies
// ---------------------------------------------------------------------------

/// The closed definition kind a scan can observe; exactly the Model
/// definition kinds promotion can materialize.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolKind {
    Scalar,
    Enum,
    ValueObject,
    Entity,
    Command,
    Query,
    Event,
    Policy,
    Effect,
    Endpoint,
}

impl SymbolKind {
    /// The closed kinds in canonical (Model kind-stem) order.
    pub const KEYS: [Self; 10] = [
        Self::Scalar,
        Self::Enum,
        Self::ValueObject,
        Self::Entity,
        Self::Command,
        Self::Query,
        Self::Event,
        Self::Policy,
        Self::Effect,
        Self::Endpoint,
    ];

    /// The wire spelling (the Model `kind` word).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Enum => "enum",
            Self::ValueObject => "value-object",
            Self::Entity => "entity",
            Self::Command => "command",
            Self::Query => "query",
            Self::Event => "event",
            Self::Policy => "policy",
            Self::Effect => "effect",
            Self::Endpoint => "endpoint",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|kind| kind.key() == text)
    }

    /// The canonical module kind stem whose document owns this kind.
    pub const fn stem(self) -> &'static str {
        match self {
            Self::Scalar | Self::Enum | Self::ValueObject | Self::Entity => "entities",
            Self::Command | Self::Effect => "commands",
            Self::Query => "queries",
            Self::Event => "events",
            Self::Policy => "policies",
            Self::Endpoint => "endpoints",
        }
    }

    /// Whether promotion of this kind is supported by this issue. Policy
    /// definitions need `applies_to`/`decision` decisions that observed
    /// evidence cannot carry without over-inference, so they refuse.
    pub const fn promotable(self) -> bool {
        !matches!(self, Self::Policy)
    }
}

/// The closed binding status. `Explicit` and `Confirmed` are user-owned
/// facts; `Inferred` is adapter-owned and never promotion-eligible.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingStatus {
    Explicit,
    Confirmed,
    Inferred,
}

impl BindingStatus {
    /// The closed statuses in canonical order.
    pub const KEYS: [Self; 3] = [Self::Explicit, Self::Confirmed, Self::Inferred];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Confirmed => "confirmed",
            Self::Inferred => "inferred",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// The closed binding currency: `current` means the last fingerprint or
/// scan still matches, `stale` means the source moved or changed, and
/// `unknown` means no evidence exists to compare (missing evidence is
/// unknown, never the absence of behavior).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingState {
    Current,
    Stale,
    Unknown,
}

impl BindingState {
    /// The closed states in canonical order.
    pub const KEYS: [Self; 3] = [Self::Current, Self::Stale, Self::Unknown];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// The closed provenance origin of one recorded fact, shared with the
/// #22 trace vocabulary semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Stated directly by the user (an explicit binding).
    Declared,
    /// Observed by the registered adapter scan.
    Observed,
    /// Derived by the adapter without user confirmation.
    Inferred,
}

impl Origin {
    /// The closed origins in canonical order.
    pub const KEYS: [Self; 3] = [Self::Declared, Self::Observed, Self::Inferred];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Observed => "observed",
            Self::Inferred => "inferred",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// The closed confidence vocabulary: words, never numbers (the ADR-0014
/// owner decision), so no decimal canonicalization rule is needed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Exact,
    High,
    Medium,
    Low,
    Unknown,
}

impl Confidence {
    /// The closed confidences in canonical order.
    pub const KEYS: [Self; 5] = [
        Self::Exact,
        Self::High,
        Self::Medium,
        Self::Low,
        Self::Unknown,
    ];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// The closed evidence reference role of one observed outbound edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceRole {
    /// A read of a query/entity surface.
    Read,
    /// A create write.
    Create,
    /// An update write.
    Update,
    /// A delete write.
    Delete,
    /// An event emission.
    Emit,
    /// An external call.
    Call,
    /// A plain reference with no effect semantics.
    Reference,
}

impl ReferenceRole {
    /// The closed roles in canonical order.
    pub const KEYS: [Self; 7] = [
        Self::Read,
        Self::Create,
        Self::Update,
        Self::Delete,
        Self::Emit,
        Self::Call,
        Self::Reference,
    ];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Emit => "emit",
            Self::Call => "call",
            Self::Reference => "reference",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// The closed history event vocabulary of one binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryEvent {
    /// The binding was first recorded from a scan.
    Recorded,
    /// The user created an explicit binding.
    Bound,
    /// The user confirmed an inferred mapping.
    Confirmed,
    /// The adapter re-resolved the binding after a source move.
    Moved,
    /// A scan or fingerprint check marked the binding stale.
    Staled,
    /// The symbol was promoted into the canonical model.
    Promoted,
    /// Attachments changed.
    Attached,
}

impl HistoryEvent {
    /// The closed events in canonical order.
    pub const KEYS: [Self; 7] = [
        Self::Recorded,
        Self::Bound,
        Self::Confirmed,
        Self::Moved,
        Self::Staled,
        Self::Promoted,
        Self::Attached,
    ];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Bound => "bound",
            Self::Confirmed => "confirmed",
            Self::Moved => "moved",
            Self::Staled => "staled",
            Self::Promoted => "promoted",
            Self::Attached => "attached",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

// ---------------------------------------------------------------------------
// Wire records (serde field order is the canonical wire order)
// ---------------------------------------------------------------------------

/// The adapter that produced a scan or recorded a fact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterIdentity {
    pub id: String,
    pub version: String,
    pub digest: Option<String>,
}

/// One observed source location: a logical project-relative POSIX path
/// and an optional 1-based line. Physical bytes never enter the index.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceLocation {
    pub path: String,
    pub line: Option<u64>,
}

/// One observed outbound evidence edge.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReferenceEvidence {
    pub target: String,
    pub role: ReferenceRole,
    pub confidence: Confidence,
}

/// One observed entity/value-object field.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FieldEvidence {
    pub name: String,
    pub r#type: String,
    pub required: bool,
}

/// One observed enum value.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ValueEvidence {
    pub value: String,
    pub description: Option<String>,
}

/// The bounded evidence of one symbol: what the adapter saw, with the
/// confidence it claims. Missing members mean unknown, never absence.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub signature: Option<String>,
    pub references: Vec<ReferenceEvidence>,
    pub fields: Vec<FieldEvidence>,
    pub identity_fields: Vec<String>,
    pub base: Option<String>,
    pub values: Vec<ValueEvidence>,
}

/// The provenance of one recorded fact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub origin: Origin,
    pub confidence: Confidence,
    pub adapter: String,
    pub revision: String,
}

/// One binding history entry: what happened, against which revision, and
/// the optional path transition.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub event: HistoryEvent,
    pub revision: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// The typed receipt of one promotion (the authority brownfield-adoption
/// evidence: explicit adoption, review, and provenance).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromotionReceipt {
    pub plan: String,
    pub revision: String,
    pub adapter: String,
}

/// One observed symbol with its binding, evidence, provenance,
/// attachments, and history. The issue #42 wire adds `candidates`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolRecord {
    pub id: String,
    pub kind: SymbolKind,
    pub stable_key: Option<String>,
    pub location: Option<SourceLocation>,
    pub fingerprint: Option<String>,
    pub status: BindingStatus,
    pub state: BindingState,
    pub promoted: bool,
    pub promotion: Option<PromotionReceipt>,
    pub evidence: Evidence,
    pub provenance: Provenance,
    pub native_tests: Vec<String>,
    pub gates: Vec<String>,
    pub history: Vec<HistoryEntry>,
    /// Every plausible native symbol the adapter recorded for this
    /// semantic id (issue #42): an ambiguous adapter names all of them
    /// instead of silently picking one. Empty when the mapping was
    /// unambiguous or the binding is user-owned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CandidateRecord>,
}

/// One native symbol candidate of one binding (issue #42): a plausible
/// native identity the adapter saw, with the confidence it claims. The
/// user resolves an ambiguous set by naming exactly one candidate.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CandidateRecord {
    pub native: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub confidence: Confidence,
}

/// One observed endpoint bound to a symbol record. Endpoints are the
/// `exposes` relation rows of the binding registry (issue #42).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EndpointRecord {
    pub id: String,
    pub method: String,
    pub path: String,
    pub symbol: String,
}

/// The closed schema-record kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaKind {
    Input,
    Output,
    Payload,
}

impl SchemaKind {
    /// The closed kinds in canonical order.
    pub const KEYS: [Self; 3] = [Self::Input, Self::Output, Self::Payload];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Payload => "payload",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// One observed schema record bound to a symbol: an opaque payload
/// shape pinned by its exact digest.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SchemaRecord {
    pub id: String,
    pub kind: SchemaKind,
    pub symbol: String,
    pub digest: Option<String>,
}

/// One native test binding (issue #42): the `verifies` relation rows of
/// the binding registry. The id is the verbatim external test identity;
/// freshness is re-fingerprinted by the audit like every binding.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TestBindingRecord {
    pub id: String,
    pub symbol: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub state: BindingState,
}

/// The persisted observed index (canonical JSON, sorted records).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedIndex {
    pub schema_version: String,
    pub identity: String,
    pub project: String,
    pub mode: String,
    pub adapter: AdapterIdentity,
    pub revision: String,
    pub symbols: Vec<SymbolRecord>,
    pub endpoints: Vec<EndpointRecord>,
    pub schemas: Vec<SchemaRecord>,
    /// The target the producing scan declared (issue #42), if any. Set
    /// once: a later scan naming a different target refuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The adapter profile the producing scan declared (issue #42).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The native test bindings of the producing scan (issue #42),
    /// sorted by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_bindings: Vec<TestBindingRecord>,
}

impl ObservedIndex {
    /// An empty index for one project captured by one adapter scan.
    pub fn empty(project: &str, adapter: AdapterIdentity, revision: &str) -> Self {
        Self {
            schema_version: version::SCHEMA_VERSION.to_owned(),
            identity: version::INDEX_IDENTITY.to_owned(),
            project: project.to_owned(),
            mode: version::MODE.to_owned(),
            adapter,
            revision: revision.to_owned(),
            symbols: Vec::new(),
            endpoints: Vec::new(),
            schemas: Vec::new(),
            target: None,
            profile: None,
            test_bindings: Vec::new(),
        }
    }

    /// The record of one symbol id, if present.
    pub fn symbol(&self, id: &str) -> Option<&SymbolRecord> {
        self.symbols.iter().find(|record| record.id == id)
    }

    /// The module segment of a semantic id (the first dot-separated
    /// segment; the Model 1.0 one-segment module grammar).
    pub fn module_of(id: &str) -> Option<&str> {
        id.split('.').next().filter(|segment| !segment.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Adapter scan input documents
// ---------------------------------------------------------------------------

/// One scanned symbol: the adapter's claim about existing code, with the
/// optional candidate set issue #42 adds for ambiguous mappings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanSymbol {
    pub id: String,
    pub kind: SymbolKind,
    pub stable_key: Option<String>,
    pub location: Option<SourceLocation>,
    pub fingerprint: Option<String>,
    pub mapping: Confidence,
    pub evidence: Evidence,
    pub candidates: Vec<CandidateRecord>,
}

/// One scanned endpoint bound to a scanned symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanEndpoint {
    pub id: String,
    pub method: String,
    pub path: String,
    pub symbol: String,
}

/// One scanned schema record bound to a scanned symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanSchema {
    pub id: String,
    pub kind: SchemaKind,
    pub symbol: String,
    pub digest: Option<String>,
}

/// One scanned native test binding (issue #42).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ScanTestBinding {
    pub id: String,
    pub symbol: String,
    pub path: String,
    pub fingerprint: Option<String>,
}

/// The validated adapter scan document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanDocument {
    pub adapter: AdapterIdentity,
    pub project: String,
    pub revision: String,
    pub symbols: Vec<ScanSymbol>,
    pub endpoints: Vec<ScanEndpoint>,
    pub schemas: Vec<ScanSchema>,
    pub target: Option<String>,
    pub profile: Option<String>,
    pub test_bindings: Vec<ScanTestBinding>,
}
