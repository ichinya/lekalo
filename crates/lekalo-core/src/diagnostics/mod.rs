//! Stable machine-readable diagnostic contract (issue #11).
//!
//! This module owns the closed diagnostic wire (`lekalo/diagnostic/v1.0.0`),
//! the embedded rule registry (`dev.lekalo.diagnostic-registry@1.0.0`),
//! deterministic normalization, and the human projection. Producers never
//! assemble a [`Diagnostic`] by hand: they go through the registry-backed
//! constructor so every emitted rule matches its registered identity, code,
//! category, severity, message, and closed data fields.
//!
//! Privacy: a diagnostic carries only logical project-relative paths,
//! bounded structured data, and registry-approved text. Raw operating-system
//! messages, absolute paths, provider output, and timestamps never enter.

pub mod id;
pub mod normalize;
pub mod provider;
pub mod registry;
pub mod render;
pub mod types;
pub mod version;

use std::collections::BTreeMap;

use serde::Serialize;

pub use id::{DiagnosticCode, DiagnosticId, FixId, MessageId, OriginalCode, ProviderNamespace};
pub use normalize::{DiagnosticSet, SetError};
pub use registry::{DiagnosticRegistry, RegistryEntry, RegistryError};
pub use types::{
    Category, DataFieldType, DataValue, FixApplicability, LocationRequirement, NestedValue, Scalar,
    Severity,
};
pub use version::DiagnosticSchemaVersion;

/// One closed source location: logical path plus optional half-open range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) range: Option<Range>,
}

/// A half-open source range with 0-based bytes and 1-based line/column.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// One end of a [`Range`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
}

/// A related location: registered relation, message id, and location.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RelatedLocation {
    pub(crate) relation: DiagnosticId,
    pub(crate) message_id: MessageId,
    pub(crate) location: SourceLocation,
}

/// The closed data payload: registry-declared keys with typed values.
pub use types::DataObject;
/// One immediate-to-root cause link (non-recursive in v1).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiagnosticCause {
    pub(crate) id: DiagnosticId,
    pub(crate) code: DiagnosticCode,
    pub(crate) message_id: MessageId,
    pub(crate) message: String,
    pub(crate) data: DataObject,
}

/// An inert suggested fix: advice only, never an executable operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SuggestedFix {
    pub(crate) fix_id: FixId,
    pub(crate) applicability: FixApplicability,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<SourceLocation>,
}

/// Provider metadata: key-sorted map of namespaced bounded original codes.
pub type ProviderMetadata = BTreeMap<String, BTreeMap<String, OriginalCode>>;

/// The stable machine-readable diagnostic (wire `lekalo/diagnostic/v1.0.0`).
///
/// Fields are private by contract: producers construct diagnostics through
/// [`build`], which resolves the registry entry and validates every field.
/// The type is deliberately `Serialize` without a general-purpose
/// `Deserialize`, so library callers cannot forge core diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub(crate) schema_version: DiagnosticSchemaVersion,
    pub(crate) registry_version: String,
    pub(crate) id: DiagnosticId,
    pub(crate) code: DiagnosticCode,
    pub(crate) severity: Severity,
    pub(crate) category: Category,
    pub(crate) message_id: MessageId,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<SourceLocation>,
    pub(crate) data: DataObject,
    pub(crate) related_locations: Vec<RelatedLocation>,
    pub(crate) causes: Vec<DiagnosticCause>,
    pub(crate) fixes: Vec<SuggestedFix>,
    pub(crate) metadata: ProviderMetadata,
}

impl Diagnostic {
    /// The dotted rule id (also the stable derived reason code).
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// The immutable `LEK-SUBSYSTEM-NNN` code.
    pub fn code(&self) -> &str {
        self.code.as_str()
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn category(&self) -> Category {
        self.category
    }

    /// The rendered registry default message (stable English in v1).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The bounded structured data of this diagnostic.
    pub fn data(&self) -> &DataObject {
        &self.data
    }

    /// The logical path, when the diagnostic is located.
    pub fn path(&self) -> Option<&str> {
        self.source.as_ref()?.path.as_deref()
    }

    /// The start position, when the diagnostic carries a span.
    pub fn start(&self) -> Option<Position> {
        Some(self.source.as_ref()?.range?.start)
    }
}

/// A validated machine key that ignores the rendered message: two
/// diagnostics with identical machine fields are exact duplicates.
pub(crate) fn machine_eq(left: &Diagnostic, right: &Diagnostic) -> bool {
    left.id == right.id
        && left.code == right.code
        && left.severity == right.severity
        && left.category == right.category
        && left.message_id == right.message_id
        && left.symbol == right.symbol
        && left.source == right.source
        && left.data == right.data
        && left.related_locations == right.related_locations
        && left.causes == right.causes
        && left.fixes == right.fixes
        && left.metadata == right.metadata
}
