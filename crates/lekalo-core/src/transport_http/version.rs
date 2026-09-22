//! Issue #70 transport-http contract identity and hard denial limits.
//!
//! The transport-http contract is its own family: independent of the
//! product release, of the source Model and IR versions, of the
//! error-contract, query-model, and authorization families, and of
//! the diagnostic registry. The M4 project line is 0.4.x, so the
//! family's first generation is 0.4.0. The limits below are the
//! owner-approved v1 constants (ADR-0042); every normalization
//! rejects with an explicit registered diagnostic instead of
//! truncating by arrival order.

/// The transport-http contract family identifier.
pub const FAMILY: &str = "dev.lekalo.transport-http";

/// The exact transport-http contract version.
pub const VERSION: &str = "0.4.0";

/// The exact contract identity: family and version joined with `@`.
pub const IDENTITY: &str = "dev.lekalo.transport-http@0.4.0";

/// The exact wire discriminator of the transport-http contract.
pub const SCHEMA_VERSION: &str = "lekalo/transport-http/v0.4.0";

/// The exact IR identity this attachment binds (`irRef`).
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact Model version this attachment binds (`modelRef`).
pub const MODEL_VERSION: &str = "0.2.16";

/// The closed wire dialect of v1 attachments.
pub const WIRE_DIALECT: &str = "lekalo-http-wire/v1";

/// The closed request/response content type of v1 attachments.
pub const CONTENT_TYPE: &str = "application/json";

/// The closed error envelope of v1 attachments.
pub const ERROR_ENVELOPE: &str = "canonical-v1";

/// The maximum number of endpoints one attachment may carry.
pub const MAX_ENDPOINTS: usize = 2048;

/// The maximum number of parameters one endpoint may declare.
pub const MAX_PARAMS: usize = 64;

/// The maximum number of error-map entries one endpoint may carry.
pub const MAX_ERROR_MAP: usize = 256;

/// The maximum number of security schemes one attachment may declare.
pub const MAX_SCHEMES: usize = 64;

/// The maximum number of auth schemes one endpoint may require.
pub const MAX_AUTH_SCHEMES: usize = 4;

/// The maximum number of capability declarations one endpoint may carry.
pub const MAX_CAPABILITIES: usize = 3;

/// The maximum number of scenario references one endpoint may declare.
pub const MAX_SCENARIOS: usize = 32;

/// The maximum number of body/response fields one binding may declare.
pub const MAX_FIELDS: usize = 64;

/// The maximum number of response headers one success may declare.
pub const MAX_HEADERS: usize = 16;

/// The maximum number of correlation headers.
pub const MAX_CORRELATION_HEADERS: usize = 4;

/// The maximum number of tags one endpoint may declare.
pub const MAX_TAGS: usize = 16;

/// The maximum size in bytes of one canonical attachment payload.
pub const MAX_CANONICAL_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_parts_are_consistent() {
        assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
        assert!(SCHEMA_VERSION.ends_with(VERSION));
        assert!(IR_IDENTITY.starts_with("dev.lekalo.ir@"));
    }

    #[test]
    fn bounds_match_the_recorded_owner_decisions() {
        assert_eq!(MAX_ENDPOINTS, 2048);
        assert_eq!(MAX_PARAMS, 64);
        assert_eq!(MAX_ERROR_MAP, 256);
        assert_eq!(MAX_SCHEMES, 64);
        assert_eq!(MAX_AUTH_SCHEMES, 4);
        assert_eq!(MAX_CAPABILITIES, 3);
        assert_eq!(MAX_SCENARIOS, 32);
        assert_eq!(MAX_FIELDS, 64);
        assert_eq!(MAX_HEADERS, 16);
        assert_eq!(MAX_CORRELATION_HEADERS, 4);
        assert_eq!(MAX_TAGS, 16);
        assert_eq!(MAX_CANONICAL_BYTES, 1024 * 1024);
    }
}
