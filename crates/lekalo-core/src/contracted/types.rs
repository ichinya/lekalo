//! The typed contracted-mode model (issue #40).
//!
//! The conformed-binding registry records which canonical symbols a
//! native target implements, the source locations of the maintained
//! handlers, the typed signature and effect declarations the code
//! claims, the ownership manifest of every generated support artifact,
//! and the attached native tests. Source code stays the owner of the
//! implementation; the registry is verified conformance evidence, never
//! a copy of it.

use serde::{Deserialize, Serialize};

use super::version;

// ---------------------------------------------------------------------------
// Closed vocabularies
// ---------------------------------------------------------------------------

/// The closed definition kinds the registry can bind; the same kinds the
/// observed scan can record and the conformance checks can classify.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolKind {
    Command,
    Query,
    Effect,
    Endpoint,
    Entity,
}

impl SymbolKind {
    /// The closed kinds in canonical (Model kind-stem) order.
    pub const KEYS: [Self; 5] = [
        Self::Command,
        Self::Query,
        Self::Effect,
        Self::Endpoint,
        Self::Entity,
    ];

    /// The wire spelling (the Model `kind` word).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Query => "query",
            Self::Effect => "effect",
            Self::Endpoint => "endpoint",
            Self::Entity => "entity",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|kind| kind.key() == text)
    }
}

/// The closed drift-state of one conformed binding after the last
/// conformance run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftState {
    /// The recorded conformance evidence still matches the tree.
    Conformant,
    /// The source fingerprint changed: the binding is unverified.
    Stale,
    /// No fingerprint evidence exists to compare.
    Unknown,
}

impl DriftState {
    /// The closed states in canonical order.
    pub const KEYS: [Self; 3] = [Self::Conformant, Self::Stale, Self::Unknown];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Conformant => "conformant",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.key() == text)
    }
}

/// The closed support-artifact kind. Exactly the support outputs the
/// issue lets Lekalo generate for a maintained implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportKind {
    /// OpenAPI fragment projected from the bound endpoints.
    Openapi,
    /// DTO/type/schema fragment for the native target.
    Types,
    /// Native test skeleton projected from a Scenario IR step set.
    TestSkeleton,
    /// Binding manifest fragment for the target adapter.
    BindingManifest,
    /// Conformance metadata emitted by the adapter.
    ConformanceMetadata,
    /// Migration-plan hint derived from a semantic diff.
    MigrationHint,
}

impl SupportKind {
    /// The closed kinds in canonical order.
    pub const KEYS: [Self; 6] = [
        Self::Openapi,
        Self::Types,
        Self::TestSkeleton,
        Self::BindingManifest,
        Self::ConformanceMetadata,
        Self::MigrationHint,
    ];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Openapi => "openapi",
            Self::Types => "types",
            Self::TestSkeleton => "test-skeleton",
            Self::BindingManifest => "binding-manifest",
            Self::ConformanceMetadata => "conformance-metadata",
            Self::MigrationHint => "migration-hint",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|kind| kind.key() == text)
    }
}

/// The closed lifecycle vocabulary of a support artifact, aligned with
/// the artifact-manifest lifecycle grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportLifecycle {
    /// Adapter-reproducible; drift or staleness blocks conformance.
    Generated,
    /// Emitted once for the maintainer to complete; never overwritten.
    Scaffolded,
    /// Recomputed and compared on every conformance run.
    Checked,
}

impl SupportLifecycle {
    /// The closed lifecycles in canonical order.
    pub const KEYS: [Self; 3] = [Self::Generated, Self::Scaffolded, Self::Checked];

    /// The wire spelling (the artifact-manifest lifecycle word).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Scaffolded => "scaffolded",
            Self::Checked => "checked",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS
            .iter()
            .copied()
            .find(|lifecycle| lifecycle.key() == text)
    }
}

// ---------------------------------------------------------------------------
// Wire records (serde field order is the canonical wire order)
// ---------------------------------------------------------------------------

/// The adapter that produced or verified a record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterIdentity {
    pub id: String,
    pub version: String,
    pub digest: Option<String>,
}

/// One maintained source location: a logical project-relative POSIX
/// path and an optional 1-based line. Physical bytes never enter the
/// registry.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceLocation {
    pub path: String,
    pub line: Option<u64>,
}

/// One typed effect declaration the maintained code claims.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeclaredEffect {
    /// The closed effect-kind key (the #14 effect-kind wire word).
    pub kind: String,
    /// The canonical subject: a semantic entity id, `entity.field`, or a
    /// typed `kind:id` resource reference.
    pub subject: String,
}

/// The typed signature claim of one maintained operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignatureEvidence {
    /// The canonical input field names, sorted, with requiredness.
    pub inputs: Vec<SignatureField>,
    /// The canonical output type expression, when the symbol returns.
    pub output: Option<String>,
    /// The canonical reads references, sorted and unique.
    pub reads: Vec<String>,
}

/// One input field of a signature claim.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SignatureField {
    pub name: String,
    pub r#type: String,
    pub required: bool,
}

/// The ownership manifest entry of one generated support artifact.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SupportArtifact {
    /// The canonical semantic owner of the artifact.
    pub symbol: String,
    /// The closed support-artifact kind.
    pub kind: SupportKind,
    /// The exact logical path inside the generated home.
    pub path: String,
    /// The closed lifecycle.
    pub lifecycle: SupportLifecycle,
    /// The SHA-256 of the artifact's exact observed bytes, when the
    /// lifecycle fingerprints content (generated/checked).
    pub digest: Option<String>,
}

/// The provenance of one conformed binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub adapter: String,
    pub revision: String,
}

/// One binding history entry: what happened and against which revision.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub event: HistoryEvent,
    pub revision: String,
}

/// The closed history event vocabulary of one binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryEvent {
    /// The binding was first recorded.
    Recorded,
    /// The adapter re-verified the binding conformant.
    Verified,
    /// The conformance run marked the binding stale.
    Staled,
    /// Attachments changed.
    Attached,
}

impl HistoryEvent {
    /// The closed events in canonical order.
    pub const KEYS: [Self; 4] = [Self::Recorded, Self::Verified, Self::Staled, Self::Attached];

    /// The wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Verified => "verified",
            Self::Staled => "staled",
            Self::Attached => "attached",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|event| event.key() == text)
    }
}

/// One conformed symbol: the maintained implementation bound to its
/// canonical contract, signature claim, declared effects, support
/// artifacts, and native tests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConformedSymbol {
    pub id: String,
    pub kind: SymbolKind,
    /// The logical project-relative source path of the maintained
    /// handler.
    pub source: SourceLocation,
    /// The SHA-256 of the source file's exact bytes at the last
    /// conformance run.
    pub fingerprint: Option<String>,
    /// The drift-state after the last conformance run.
    pub state: DriftState,
    /// The typed signature claim, when the kind carries one.
    pub signature: Option<SignatureEvidence>,
    /// The declared effects the code claims, sorted.
    pub effects: Vec<DeclaredEffect>,
    /// The attached verbatim native-test ids, sorted.
    pub native_tests: Vec<String>,
    /// The attached verbatim gate ids, sorted.
    pub gates: Vec<String>,
    pub provenance: Provenance,
    pub history: Vec<HistoryEntry>,
}

/// The persisted conformed-binding registry (canonical JSON, sorted
/// records).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConformedRegistry {
    pub schema_version: String,
    pub identity: String,
    pub project: String,
    pub mode: String,
    pub adapter: AdapterIdentity,
    pub revision: String,
    pub symbols: Vec<ConformedSymbol>,
    pub artifacts: Vec<SupportArtifact>,
}

impl ConformedRegistry {
    /// An empty registry for one project captured by one adapter.
    pub fn empty(project: &str, adapter: AdapterIdentity, revision: &str) -> Self {
        Self {
            schema_version: version::SCHEMA_VERSION.to_owned(),
            identity: version::IDENTITY.to_owned(),
            project: project.to_owned(),
            mode: version::MODE.to_owned(),
            adapter,
            revision: revision.to_owned(),
            symbols: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    /// The record of one symbol id, if present.
    pub fn symbol(&self, id: &str) -> Option<&ConformedSymbol> {
        self.symbols.iter().find(|record| record.id == id)
    }

    /// The module segment of a semantic id (the first dot-separated
    /// segment; the Model 1.0 one-segment module grammar).
    pub fn module_of(id: &str) -> Option<&str> {
        id.split('.').next().filter(|segment| !segment.is_empty())
    }
}

/// One adapter-declared conformed symbol to record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredSymbol {
    pub id: String,
    pub kind: SymbolKind,
    pub source: SourceLocation,
    pub fingerprint: Option<String>,
    pub signature: Option<SignatureEvidence>,
    pub effects: Vec<DeclaredEffect>,
}

/// The validated adapter declaration document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationDocument {
    pub adapter: AdapterIdentity,
    pub project: String,
    pub revision: String,
    pub symbols: Vec<DeclaredSymbol>,
    pub artifacts: Vec<SupportArtifact>,
}
