//! Issue #23: the portable Scenario IR for multi-target conformance
//! tests.
//!
//! One closed, target-neutral behavioral scenario document: explicit
//! stable identities (`scenario_id`, every `step_id`), ordered
//! `given`/`when`/`then` arrays, closed typed values and references, a
//! data-flow-validated assertion set, set-like tags and namespaced
//! metadata, typed backend bindings that reference native tests and
//! fake-reference evaluations as data, an optional source-map sidecar
//! reference, and a derived coverage vector. Deterministic canonical
//! bytes (compact JSON, byte-sorted keys, sorted set-like collections,
//! untouched behavioral order) make one scenario byte-identical on
//! every host.
//!
//! Boundaries: this module is pure contract and validation — no
//! execution, no process, no network, no runner, no evaluator. Runtime
//! execution belongs to #107 (fake reference), #47/#56 (native
//! runners), and #56/#91 (aggregation); neutral evidence production to
//! #31/#107; process and protocol conformance to #27/#31. Generic
//! symbol, kind, and visibility resolution stays with #12; error-union
//! membership with #62; effect meaning with #14; authorization with
//! #25; transactions with #24. The graph `verifies` contribution to #13
//! is carried as plain typed data
//! ([`CoverageVector`]), never as graph mutation.
//!
//! Determinism and denial: every construction bound
//! ([`version`]) rejects with an explicit registered diagnostic before
//! any partial IR exists; canonical bytes are bounded and identical for
//! the same validated scenario regardless of map insertion order,
//! locale, timezone, host, or line endings.

pub mod action;
pub mod assertion;
pub mod binding;
pub mod canonical;
pub mod coverage;
pub mod diagnostic;
pub mod id;
pub mod precondition;
pub mod reference;
pub mod source_map;
pub mod step;
pub mod value;
pub mod version;
pub(crate) mod wire;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use serde_json::Value as Json;

pub use action::InvokeAction;
pub use assertion::Assertion;
pub use binding::{Backend, Binding, Mode, ProfilePin, ProtocolPin};
pub use coverage::{CoverageEntry, CoverageVector, OutcomeKind};
pub use precondition::{FieldPredicate, IdAlgorithm, Precondition};
pub use reference::{Ref, RefKind, RefTarget};
pub use source_map::SourceMapRef;
pub use step::{GivenStep, ReplayExpect, ReplayMeta, ThenStep, ValueOrRef, WhenStep};
pub use value::TypedValue;
pub use version::{FAMILY, IDENTITY, IR_IDENTITY, SCHEMA_VERSION, SOURCE_MAP_IDENTITY, VERSION};

/// The exact accepted source Model contract pin of one scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelPin {
    /// The accepted Model v0.1.0 contract.
    V0_1_0,
    /// The accepted Model v1.0.0 contract.
    V1_0_0,
}

impl ModelPin {
    /// The exact wire text of the Model version.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelPin::V0_1_0 => "0.1.0",
            ModelPin::V1_0_0 => "1.0.0",
        }
    }
}

/// The bound source Model contract: exact version plus digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRef {
    /// The accepted Model contract version.
    pub version: ModelPin,
    /// The digest of the exact source Model document.
    pub digest: Sha256Digest,
}

/// The bound compiled IR contract: the one accepted #8 identity plus
/// the digest of the exact canonical IR payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrRef {
    /// The digest of the exact canonical IR payload.
    pub digest: Sha256Digest,
}

impl ModelRef {
    /// The exact wire text of the pinned Model version.
    pub fn version_text(&self) -> &'static str {
        self.version.as_str()
    }
}

/// One closed scalar metadata value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataValue {
    /// The single null value.
    Null,
    /// True or false.
    Boolean(bool),
    /// A signed 64-bit integer.
    Integer(i64),
    /// A bounded UTF-8 string.
    String(String),
}

/// One finished Scenario IR: immutable, deterministically ordered, and
/// safe to share across threads. Every query is side-effect free.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioIr {
    /// The stable project identity.
    pub(crate) project_id: id::SemanticId,
    /// The stable scenario identity.
    pub(crate) scenario_id: id::SemanticId,
    /// The explicit scenario contract version.
    pub(crate) scenario_version: crate::lockfile::types::SemVer,
    /// The bounded human summary.
    pub(crate) summary: String,
    /// The bound compiled IR contract.
    pub(crate) ir_ref: IrRef,
    /// The bound source Model contract.
    pub(crate) model_ref: ModelRef,
    /// The ordered preconditions.
    pub(crate) given: Vec<GivenStep>,
    /// The ordered actions (at least one).
    pub(crate) when: Vec<WhenStep>,
    /// The ordered assertions (at least one).
    pub(crate) then: Vec<ThenStep>,
    /// The sorted backend bindings.
    pub(crate) bindings: Vec<Binding>,
    /// The sorted tags.
    pub(crate) tags: Vec<id::Tag>,
    /// The key-sorted namespaced metadata.
    pub(crate) metadata: Vec<(String, MetadataValue)>,
    /// The optional source-map sidecar reference.
    pub(crate) source_map: Option<SourceMapRef>,
    /// The derived coverage vector.
    pub(crate) coverage: CoverageVector,
}

impl ScenarioIr {
    /// Normalize one wire document into a validated Scenario IR, or
    /// return the typed rejection set. Pure: no source, model, cache,
    /// report, network, process, or target access of any kind.
    pub fn from_value(json: &Json) -> Result<ScenarioIr, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// sorted set-like collections, exact behavioral order, no trailing
    /// LF), or a typed refusal beyond the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::scenario_bytes(self)
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &id::SemanticId {
        &self.project_id
    }

    /// The stable scenario identity.
    pub fn scenario_id(&self) -> &id::SemanticId {
        &self.scenario_id
    }

    /// The explicit scenario contract version.
    pub fn scenario_version(&self) -> &crate::lockfile::types::SemVer {
        &self.scenario_version
    }

    /// The bounded human summary.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// The bound compiled IR contract.
    pub fn ir_ref(&self) -> &IrRef {
        &self.ir_ref
    }

    /// The bound source Model contract.
    pub fn model_ref(&self) -> &ModelRef {
        &self.model_ref
    }

    /// The ordered preconditions.
    pub fn given(&self) -> &[GivenStep] {
        &self.given
    }

    /// The ordered actions.
    pub fn when(&self) -> &[WhenStep] {
        &self.when
    }

    /// The ordered assertions.
    pub fn then(&self) -> &[ThenStep] {
        &self.then
    }

    /// The sorted backend bindings.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The sorted tags.
    pub fn tags(&self) -> &[id::Tag] {
        &self.tags
    }

    /// The key-sorted namespaced metadata.
    pub fn metadata(&self) -> &[(String, MetadataValue)] {
        &self.metadata
    }

    /// The optional source-map sidecar reference.
    pub fn source_map(&self) -> Option<&SourceMapRef> {
        self.source_map.as_ref()
    }

    /// The derived coverage vector: stable typed data for later #13
    /// contribution offers; it never mutates any graph.
    pub fn coverage(&self) -> &CoverageVector {
        &self.coverage
    }
}
