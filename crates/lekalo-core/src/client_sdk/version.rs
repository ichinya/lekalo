//! Issue #72 client-SDK contract identity and hard denial limits.
//!
//! The client-SDK contract is its own family: independent of the product
//! release, of the source Model and IR versions, of the transport-http,
//! error-contract, query-model, authorization, and OpenAPI families, and
//! of the diagnostic registry. The first generation lands at the current
//! transport-http generation (0.4.0) — the SDK projection is derived
//! from the transport attachment and shares its wire generation. The
//! limits below are the v1 constants; every over-bound input rejects
//! with an explicit registered diagnostic instead of truncating by
//! arrival order.

/// The client-SDK contract family identifier.
pub const FAMILY: &str = "dev.lekalo.client-sdk";

/// The exact client-SDK contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.client-sdk@0.4.0";

/// The exact wire discriminator of the client-SDK projection contract.
pub const SCHEMA_VERSION: &str = "lekalo/client-sdk/v0.4.0";

/// The exact compatibility-metadata contract identity one client
/// artifact carries.
pub const COMPATIBILITY_IDENTITY: &str = "dev.lekalo.client-sdk-compatibility@0.4.0";

/// The exact consumer-binding index contract identity.
pub const INDEX_IDENTITY: &str = "dev.lekalo.client-sdk-index@0.4.0";

/// The exact IR identity every projection binds (`irRef`).
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact Model version every projection binds (`modelRef`).
pub const MODEL_VERSION: &str = "0.2.16";

/// The closed generation modes of one client artifact set.
pub const MODE_GENERATED: &str = "generated";
/// The read-only maintained-client mode.
pub const MODE_CHECKED: &str = "checked";

/// The maximum number of operations one projection may carry.
pub const MAX_OPERATIONS: usize = 2048;

/// The maximum number of named types one projection may carry.
pub const MAX_TYPES: usize = 4096;

/// The maximum number of error variants one projection may carry.
pub const MAX_ERRORS: usize = 256;

/// The maximum number of declared consumers one index may carry.
pub const MAX_CONSUMERS: usize = 256;

/// The maximum number of affected-client entries one impact analysis
/// may return.
pub const MAX_AFFECTED: usize = 1024;

/// The maximum length of one generated identifier in any language.
pub const MAX_IDENTIFIER: usize = 128;

/// The maximum canonical payload size of one projection.
pub const MAX_CANONICAL_BYTES: usize = 8 * 1024 * 1024;

/// The maximum number of automatic attempts per operation (the first
/// call plus bounded retries; a declared budget beyond this bound is a
/// refusal, never an unbounded loop).
pub const MAX_ATTEMPTS: u32 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_independent() {
        assert_eq!(SCHEMA_VERSION, "lekalo/client-sdk/v0.4.0");
        assert_eq!(IDENTITY, "dev.lekalo.client-sdk@0.4.0");
        assert_ne!(IDENTITY, crate::transport_http::IDENTITY);
        assert_ne!(COMPATIBILITY_IDENTITY, IDENTITY);
        assert_ne!(INDEX_IDENTITY, IDENTITY);
    }

    #[test]
    fn limits_are_bounded() {
        assert_eq!(MAX_OPERATIONS, 2048);
        assert_eq!(MAX_ATTEMPTS, 5);
        assert_eq!(MAX_CANONICAL_BYTES, 8 * 1024 * 1024);
        assert_eq!(MODE_GENERATED, "generated");
        assert_eq!(MODE_CHECKED, "checked");
    }
}
