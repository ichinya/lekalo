//! Issue #33 MCP contract identity, the pinned protocol profile, and the
//! recorded admission bounds.
//!
//! The MCP family is its own contract family: independent of the product
//! release beyond the issue #33 implementation commit, of the Model/IR
//! contracts, of every preserved read-service contract it carries
//! (inspect `0.8.0`, impact/context/graph/trace `0.2.16`, doctor `0.3.2`,
//! validation `0.6.4`), and of the diagnostic registry identity. The
//! outer MCP protocol date is independent of the Lekalo contract
//! versions. Bounds are owner-approved proposal constants: caller
//! parameters and catalog configuration may reduce a limit, never raise
//! one.

/// The MCP capabilities contract family identifier.
pub const FAMILY: &str = "dev.lekalo.mcp";

/// The exact MCP contract version (the product version of this commit).
pub const VERSION: &str = "0.8.0";

/// The exact capabilities contract identity.
pub const IDENTITY: &str = "dev.lekalo.mcp-capabilities@0.8.0";

/// The exact wire discriminator of the capabilities contract.
pub const SCHEMA_VERSION: &str = "lekalo/mcp-capabilities/v0.8.0";

/// The exact server-config contract identity (trusted catalog).
pub const CONFIG_IDENTITY: &str = "dev.lekalo.mcp-server-config@0.8.0";

/// The exact wire discriminator of the server-config contract.
pub const CONFIG_SCHEMA_VERSION: &str = "lekalo/mcp-server-config/v0.8.0";

/// The exact resource-envelope contract identity.
pub const RESOURCE_IDENTITY: &str = "dev.lekalo.mcp-resource@0.8.0";

/// The exact wire discriminator of the resource envelope.
pub const RESOURCE_SCHEMA_VERSION: &str = "lekalo/mcp-resource/v0.8.0";

/// The exact identity of the graph-query describing schema.
pub const GRAPH_QUERY_IDENTITY: &str = "dev.lekalo.mcp-graph-query@0.8.0";

/// The pinned outer protocol profile (2025-11-25). A host advertising a
/// different date is not silently negotiated; compatibility with other
/// profiles needs an explicit adapter.
pub const PROTOCOL_PROFILE: &str = "2025-11-25";

/// The maximum size in bytes of one incoming JSON-RPC frame.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;

/// The default cap for one complete encoded JSON-RPC response.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// The maximum response cap any caller may configure.
pub const MAX_RESPONSE_BYTES_CEILING: usize = 4 * 1024 * 1024;

/// The default per-request deadline.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// The maximum deadline any caller may configure.
pub const TIMEOUT_MS_CEILING: u64 = 120_000;

/// The maximum number of admitted concurrent tool calls.
pub const MAX_CONCURRENT_CALLS: usize = 4;

/// The maximum number of queued requests.
pub const MAX_QUEUED_REQUESTS: usize = 16;

/// The maximum number of admitted semantic roots (projects).
pub const MAX_SEMANTIC_ROOTS: usize = 128;

/// The default resource/tool listing page size.
pub const DEFAULT_PAGE_SIZE: usize = 100;

/// The maximum page size any caller may configure.
pub const PAGE_SIZE_CEILING: usize = 500;

/// The maximum number of watched/subscribed resource identities.
pub const MAX_WATCHED_IDENTITIES: usize = 256;

/// The maximum JSON nesting depth of one admitted frame.
pub const MAX_NESTING_DEPTH: usize = 64;

/// The maximum number of diagnostic items one tool output carries.
pub const MAX_TOOL_DIAGNOSTICS: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_family_and_version() {
        assert_eq!(FAMILY, "dev.lekalo.mcp");
        assert_eq!(IDENTITY, format!("dev.lekalo.mcp-capabilities@{VERSION}"));
        assert_eq!(
            SCHEMA_VERSION,
            format!("lekalo/mcp-capabilities/v{VERSION}")
        );
        assert_eq!(
            CONFIG_IDENTITY,
            format!("dev.lekalo.mcp-server-config@{VERSION}")
        );
        assert_eq!(
            CONFIG_SCHEMA_VERSION,
            format!("lekalo/mcp-server-config/v{VERSION}")
        );
        assert_eq!(
            RESOURCE_IDENTITY,
            format!("dev.lekalo.mcp-resource@{VERSION}")
        );
        assert_eq!(
            RESOURCE_SCHEMA_VERSION,
            format!("lekalo/mcp-resource/v{VERSION}")
        );
        assert_eq!(
            GRAPH_QUERY_IDENTITY,
            format!("dev.lekalo.mcp-graph-query@{VERSION}")
        );
    }

    #[test]
    fn bounds_match_the_recorded_owner_proposals() {
        assert_eq!(PROTOCOL_PROFILE, "2025-11-25");
        assert_eq!(MAX_FRAME_BYTES, 262_144);
        assert_eq!(DEFAULT_MAX_RESPONSE_BYTES, 1_048_576);
        assert_eq!(MAX_RESPONSE_BYTES_CEILING, 4_194_304);
        assert_eq!(DEFAULT_TIMEOUT_MS, 30_000);
        assert_eq!(TIMEOUT_MS_CEILING, 120_000);
        assert_eq!(MAX_CONCURRENT_CALLS, 4);
        assert_eq!(MAX_QUEUED_REQUESTS, 16);
        assert_eq!(MAX_SEMANTIC_ROOTS, 128);
        assert_eq!(DEFAULT_PAGE_SIZE, 100);
        assert_eq!(PAGE_SIZE_CEILING, 500);
        assert_eq!(MAX_WATCHED_IDENTITIES, 256);
        assert_eq!(MAX_NESTING_DEPTH, 64);
    }
}
