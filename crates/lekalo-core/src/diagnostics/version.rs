//! Diagnostic contract version identity (issue #11).
//!
//! The diagnostic schema version and the diagnostic registry version are
//! independent of the product release, the Model/IR/protocol contract
//! versions, the lock wire, and the resolver algorithm version. Reusing the
//! strict SemVer mechanics of issue #9 does not widen its registry.

use serde::Serialize;

/// The exact wire discriminator of the diagnostic item contract.
pub const SCHEMA_VERSION: &str = "lekalo/diagnostic/v1.0.0";

/// The exact wire discriminator of the diagnostic registry contract.
pub const REGISTRY_SCHEMA_VERSION: &str = "lekalo/diagnostic-registry/v1.0.0";

/// The embedded diagnostic registry identity.
pub const REGISTRY_IDENTITY: &str = "dev.lekalo.diagnostic-registry@1.0.0";

/// The current diagnostic registry version.
pub const REGISTRY_VERSION: &str = "1.0.0";

/// The closed diagnostic schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSchemaVersion {
    /// `lekalo/diagnostic/v1.0.0`.
    V1_0_0,
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
