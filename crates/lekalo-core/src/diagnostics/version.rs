//! Diagnostic contract version identity (issue #11).
//!
//! The diagnostic schema version and the diagnostic registry version are
//! independent of the product release, the Model/IR/protocol contract
//! versions, the lock wire, and the resolver algorithm version. Reusing the
//! strict SemVer mechanics of issue #9 does not widen its registry.

use serde::Serialize;

/// The exact wire discriminator of the diagnostic item contract.
pub const SCHEMA_VERSION: &str = "lekalo/diagnostic/v0.2.16";

/// The exact wire discriminator of the diagnostic registry contract.
pub const REGISTRY_SCHEMA_VERSION: &str = "lekalo/diagnostic-registry/v0.4.0";

/// The embedded diagnostic registry identity.
pub const REGISTRY_IDENTITY: &str = "dev.lekalo.diagnostic-registry@0.4.0";

/// The current diagnostic registry version. The integrated chain is
/// additive end to end: 1.14.0 (issue #29) -> 1.16.0 (issue #39, the
/// reserved observed.* family on the integrated M2 line) -> 1.18.0
/// (issue #30, the reserved `implementation.*` family; 1.17.0 stays
/// reserved by its parallel owner and is not part of this line) ->
/// 1.19.0 (issue #40, the contracted.* family) -> 1.20.0 (issue #42,
/// the reserved `bindings.*` family: proposal-unknown, ambiguous, and
/// plan-mismatch) -> 1.21.0 (issue #64, the reserved `query.*` family
/// `storage.*` family LEK-STO-001..007) -> 1.24.0 (issue #97, the
/// reserved `init.bootstrap.*` family LEK-INIT-006..009; 1.23.0 stays
/// reserved by its parallel owner and is not part of this line) ->
/// 0.2.16 (issue #66, the reserved `expression.*` family
/// LEK-EXPR-001..009) -> 0.3.2 (issue #48, the reserved `native-gate.*`
/// family LEK-NGT-001..014) -> 0.4.0 (issues #85, #70, #45, #117, #69,
/// and #87: the `nfr.*` family LEK-NFR-001..013, the reserved
/// `transport.*` family LEK-TRN-001..009, the additive
/// `storage.profile-*` family LEK-SEP-001..005, the
/// `storage.migration-*` planning family, and the `classification.*`
/// family LEK-CLS-001..012 with the `dataflow.*` family
/// LEK-DFL-001..009, all at the reserved 0.4.0 product generation;
/// issue #45 ships no new diagnostic families).
pub const REGISTRY_VERSION: &str = "0.4.0";

/// The closed diagnostic schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSchemaVersion {
    /// `lekalo/diagnostic/v0.2.16`.
    Current,
}

impl DiagnosticSchemaVersion {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        SCHEMA_VERSION
    }
}

impl Serialize for DiagnosticSchemaVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
