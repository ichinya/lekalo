//! The embedded issue #33 contract artifacts: exact bytes, exact
//! identities, and the parse-once trust boundary.
//!
//! The bytes are developer-owned contracts validated by the schema gates;
//! embedding them keeps the server and its describing schemas in one
//! immutable pin. `verify` re-checks the discriminator pairs of every
//! embedded artifact once per process and fails closed on drift, so a
//! mismatched regeneration can never serve.

use std::sync::LazyLock;

use serde_json::Value;

use super::version::{CONFIG_IDENTITY, GRAPH_QUERY_IDENTITY, IDENTITY, RESOURCE_IDENTITY, VERSION};

/// The seven input schemas, in catalog order.
pub const INPUT_SCHEMAS: [(&str, &[u8]); 7] = [
    (
        "inspect",
        include_bytes!("../../../../contracts/mcp-tool-inspect-input.schema.v0.8.0.json"),
    ),
    (
        "impact",
        include_bytes!("../../../../contracts/mcp-tool-impact-input.schema.v0.8.0.json"),
    ),
    (
        "context",
        include_bytes!("../../../../contracts/mcp-tool-context-input.schema.v0.8.0.json"),
    ),
    (
        "graph",
        include_bytes!("../../../../contracts/mcp-tool-graph-input.schema.v0.8.0.json"),
    ),
    (
        "validate",
        include_bytes!("../../../../contracts/mcp-tool-validate-input.schema.v0.8.0.json"),
    ),
    (
        "target-status",
        include_bytes!("../../../../contracts/mcp-tool-target-status-input.schema.v0.8.0.json"),
    ),
    (
        "trace-export",
        include_bytes!("../../../../contracts/mcp-tool-trace-export-input.schema.v0.8.0.json"),
    ),
];

/// The seven output schemas, in catalog order.
pub const OUTPUT_SCHEMAS: [(&str, &[u8]); 7] = [
    (
        "inspect",
        include_bytes!("../../../../contracts/mcp-tool-inspect-output.schema.v0.8.0.json"),
    ),
    (
        "impact",
        include_bytes!("../../../../contracts/mcp-tool-impact-output.schema.v0.8.0.json"),
    ),
    (
        "context",
        include_bytes!("../../../../contracts/mcp-tool-context-output.schema.v0.8.0.json"),
    ),
    (
        "graph",
        include_bytes!("../../../../contracts/mcp-tool-graph-output.schema.v0.8.0.json"),
    ),
    (
        "validate",
        include_bytes!("../../../../contracts/mcp-tool-validate-output.schema.v0.8.0.json"),
    ),
    (
        "target-status",
        include_bytes!("../../../../contracts/mcp-tool-target-status-output.schema.v0.8.0.json"),
    ),
    (
        "trace-export",
        include_bytes!("../../../../contracts/mcp-tool-trace-export-output.schema.v0.8.0.json"),
    ),
];

/// The capabilities manifest schema.
pub const CAPABILITIES_SCHEMA: &[u8] =
    include_bytes!("../../../../contracts/mcp-capabilities.schema.v0.8.0.json");

/// The trusted server-config schema.
pub const CONFIG_SCHEMA: &[u8] =
    include_bytes!("../../../../contracts/mcp-server-config.schema.v0.8.0.json");

/// The resource envelope schema.
pub const RESOURCE_SCHEMA: &[u8] =
    include_bytes!("../../../../contracts/mcp-resource.schema.v0.8.0.json");

/// The graph-query describing schema.
pub const GRAPH_QUERY_SCHEMA: &[u8] =
    include_bytes!("../../../../contracts/mcp-graph-query.schema.v0.8.0.json");

/// The expected `$id` identity of one embedded artifact.
fn expected_identity(bytes: &[u8]) -> Option<String> {
    let document: Value = serde_json::from_slice(bytes).ok()?;
    document.get("$id")?.as_str().map(str::to_owned)
}

/// The parse-once verification result over every embedded artifact.
pub struct Verified;

/// Verify the discriminator pairs of every embedded artifact; a drift is
/// a developer fault and callers fail closed instead of serving.
pub fn verified() -> Result<&'static Verified, String> {
    static VERIFIED: LazyLock<Result<Verified, String>> = LazyLock::new(|| {
        for (slug, bytes) in INPUT_SCHEMAS {
            let expected = format!("dev.lekalo.mcp-tool-{slug}-input@{VERSION}");
            let found = expected_identity(bytes)
                .ok_or_else(|| format!("mcp schema {slug}: unparseable"))?;
            if found != expected {
                return Err(format!("mcp schema {slug}: {found} != {expected}"));
            }
        }
        for (slug, bytes) in OUTPUT_SCHEMAS {
            let expected = format!("dev.lekalo.mcp-tool-{slug}-output@{VERSION}");
            let found = expected_identity(bytes)
                .ok_or_else(|| format!("mcp schema {slug}: unparseable"))?;
            if found != expected {
                return Err(format!("mcp schema {slug}: {found} != {expected}"));
            }
        }
        for (bytes, expected) in [
            (CAPABILITIES_SCHEMA, IDENTITY),
            (CONFIG_SCHEMA, CONFIG_IDENTITY),
            (RESOURCE_SCHEMA, RESOURCE_IDENTITY),
            (GRAPH_QUERY_SCHEMA, GRAPH_QUERY_IDENTITY),
        ] {
            let found = expected_identity(bytes)
                .ok_or_else(|| format!("mcp schema: unparseable (expected {expected})"))?;
            if found != expected {
                return Err(format!("mcp schema: {found} != {expected}"));
            }
        }
        Ok(Verified)
    });
    VERIFIED.as_ref().map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::version::{
        CONFIG_SCHEMA_VERSION, PROTOCOL_PROFILE, RESOURCE_SCHEMA_VERSION, SCHEMA_VERSION,
    };

    #[test]
    fn every_embedded_artifact_carries_the_exact_identity() {
        verified().expect("embedded mcp contracts verified");
    }

    #[test]
    fn tool_schema_pins_are_symmetric() {
        for ((input_slug, _), (output_slug, _)) in INPUT_SCHEMAS.iter().zip(OUTPUT_SCHEMAS.iter()) {
            assert_eq!(input_slug, output_slug);
        }
        assert_eq!(INPUT_SCHEMAS.len(), 7);
        assert_eq!(OUTPUT_SCHEMAS.len(), 7);
    }

    #[test]
    fn capabilities_schema_declares_the_pinned_profile() {
        let document: Value = serde_json::from_slice(CAPABILITIES_SCHEMA).expect("schema");
        assert_eq!(
            document["properties"]["protocol"]["properties"]["profile"]["const"],
            PROTOCOL_PROFILE
        );
        assert_eq!(
            document["properties"]["schemaVersion"]["const"],
            SCHEMA_VERSION
        );
        assert_eq!(document["properties"]["identity"]["const"], IDENTITY);
        let tools = document["properties"]["tools"]
            .as_object()
            .expect("tools schema");
        assert_eq!(tools["maxItems"], 7);
        assert_eq!(tools["minItems"], 7);
    }

    #[test]
    fn config_schema_declares_the_trusted_catalog_grammar() {
        let document: Value = serde_json::from_slice(CONFIG_SCHEMA).expect("schema");
        assert_eq!(
            document["properties"]["schemaVersion"]["const"],
            CONFIG_SCHEMA_VERSION
        );
        assert_eq!(document["properties"]["identity"]["const"], CONFIG_IDENTITY);
        assert_eq!(
            document["properties"]["projects"]["maxItems"],
            super::super::version::MAX_SEMANTIC_ROOTS
        );
    }

    #[test]
    fn resource_schema_declares_five_families() {
        let document: Value = serde_json::from_slice(RESOURCE_SCHEMA).expect("schema");
        assert_eq!(
            document["properties"]["schemaVersion"]["const"],
            RESOURCE_SCHEMA_VERSION
        );
        let kinds = document["properties"]["kind"]["enum"]
            .as_array()
            .expect("kind enum");
        assert_eq!(kinds.len(), 5);
        assert_eq!(
            document["properties"]["identity"]["const"],
            RESOURCE_IDENTITY
        );
    }
}
