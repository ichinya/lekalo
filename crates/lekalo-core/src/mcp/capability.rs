//! The deterministic `lekalo/mcp-capabilities/v0.8.0` manifest: the exact
//! seven tools, the five resource families, the pinned protocol profile,
//! the schema/diagnostic pins, and the recorded limits.
//!
//! `lekalo mcp describe --json` emits exactly these bytes without reading
//! a project or starting a server; the JSON key order is fixed and the
//! serialization is the compact canonical envelope of the manifest.

use super::resources::{templates, FAMILY_KEYS};
use super::tools::CATALOG;
use super::version::{
    CONFIG_IDENTITY, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_PAGE_SIZE, DEFAULT_TIMEOUT_MS,
    GRAPH_QUERY_IDENTITY, IDENTITY, MAX_CONCURRENT_CALLS, MAX_FRAME_BYTES, MAX_NESTING_DEPTH,
    MAX_QUEUED_REQUESTS, MAX_RESPONSE_BYTES_CEILING, MAX_SEMANTIC_ROOTS, MAX_WATCHED_IDENTITIES,
    PAGE_SIZE_CEILING, PROTOCOL_PROFILE, SCHEMA_VERSION, TIMEOUT_MS_CEILING, VERSION,
};

/// Build the capabilities manifest as a `serde_json::Value` with a fixed
/// member order (registry entries and catalogs are already sorted).
pub fn manifest() -> serde_json::Value {
    let tools: Vec<serde_json::Value> = CATALOG
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "title": tool.title,
                "input": {
                    "identity": format!("dev.lekalo.mcp-tool-{}-input@{VERSION}", tool.slug),
                    "discriminator": format!("lekalo/mcp-tool-{}-input/v{VERSION}", tool.slug),
                },
                "output": {
                    "identity": format!("dev.lekalo.mcp-tool-{}-output@{VERSION}", tool.slug),
                    "discriminator": format!("lekalo/mcp-tool-{}-output/v{VERSION}", tool.slug),
                },
            })
        })
        .collect();
    let resources: Vec<serde_json::Value> = FAMILY_KEYS
        .iter()
        .zip(templates().iter())
        .map(|(key, template)| {
            serde_json::json!({
                "kind": key,
                "uriTemplate": template.uri_template,
                "description": family_description(key),
            })
        })
        .collect();
    let codes: Vec<String> = [
        "LEK-MCP-001",
        "LEK-MCP-002",
        "LEK-MCP-003",
        "LEK-MCP-004",
        "LEK-MCP-005",
        "LEK-MCP-006",
        "LEK-MCP-007",
        "LEK-MCP-008",
        "LEK-MCP-009",
        "LEK-MCP-010",
        "LEK-MCP-011",
    ]
    .iter()
    .map(|code| (*code).to_owned())
    .collect();
    serde_json::json!({
        "schemaVersion": SCHEMA_VERSION,
        "identity": IDENTITY,
        "protocol": {
            "name": "mcp",
            "transport": "stdio",
            "profile": PROTOCOL_PROFILE,
            "frameDelimiter": "lf",
        },
        "server": {
            "name": "lekalo",
            "readOnly": true,
        },
        "limits": {
            "frameBytes": MAX_FRAME_BYTES,
            "maxResponseBytes": DEFAULT_MAX_RESPONSE_BYTES,
            "maxResponseBytesCeiling": MAX_RESPONSE_BYTES_CEILING,
            "timeoutMs": DEFAULT_TIMEOUT_MS,
            "timeoutMsCeiling": TIMEOUT_MS_CEILING,
            "concurrentCalls": MAX_CONCURRENT_CALLS,
            "queuedRequests": MAX_QUEUED_REQUESTS,
            "semanticRoots": MAX_SEMANTIC_ROOTS,
            "pageSize": DEFAULT_PAGE_SIZE,
            "pageSizeCeiling": PAGE_SIZE_CEILING,
            "watchedIdentities": MAX_WATCHED_IDENTITIES,
            "nestingDepth": MAX_NESTING_DEPTH,
        },
        "tools": tools,
        "resources": resources,
        "pins": {
            "diagnostic": crate::diagnostics::version::SCHEMA_VERSION,
            "diagnosticRegistry": format!("dev.lekalo.diagnostic-registry@{}", registry_version()),
            "serverConfig": CONFIG_IDENTITY,
            "graphQuery": GRAPH_QUERY_IDENTITY,
        },
        "diagnosticCodes": codes,
    })
}

fn registry_version() -> &'static str {
    // The MCP registry is a developer contract; the pin is constant.
    crate::diagnostics::version::MCP_REGISTRY_VERSION
}

fn family_description(key: &str) -> &'static str {
    match key {
        "project-summary" => {
            "Project identity, schema pins, admitted scope, revision digest, and counts."
        }
        "module-list" => {
            "Deterministic bounded page of authorized module metadata with an opaque cursor."
        }
        "symbol-metadata" => "The authorized inspect projection of one symbol; never raw source.",
        "target-status" => {
            "Selected lock/profile/inventory observations with separate unavailable states."
        }
        "diagnostics" => {
            "Safe stable diagnostics and scoped evidence counts; no raw logs or history."
        }
        _ => "",
    }
}

/// The compact canonical envelope bytes of the describe receipt:
/// `{"status":"valid","capabilities":<manifest>}`.
pub fn describe_json() -> String {
    format!(
        "{{\"status\":\"valid\",\"capabilities\":{}}}",
        serde_json::to_string(&manifest()).expect("capabilities serialize")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagnosticRegistry;
    use crate::mcp::version::IDENTITY;

    #[test]
    fn manifest_is_the_exact_closed_catalog() {
        let document = manifest();
        assert_eq!(document["schemaVersion"], "lekalo/mcp-capabilities/v0.8.0");
        assert_eq!(document["identity"], IDENTITY);
        assert_eq!(document["protocol"]["profile"], "2025-11-25");
        assert_eq!(document["server"]["readOnly"], true);
        let tools = document["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 7);
        assert_eq!(tools[0]["name"], "lekalo_inspect");
        assert_eq!(tools[6]["name"], "lekalo_trace_export");
        let resources = document["resources"].as_array().expect("resources");
        assert_eq!(resources.len(), 5);
        let codes = document["diagnosticCodes"].as_array().expect("codes");
        assert_eq!(codes.len(), 11);
        assert_eq!(codes[0], "LEK-MCP-001");
        assert_eq!(codes[10], "LEK-MCP-011");
        assert_eq!(
            document["pins"]["diagnosticRegistry"],
            "dev.lekalo.diagnostic-registry@0.8.0"
        );
    }

    #[test]
    fn describe_envelope_is_deterministic() {
        assert_eq!(describe_json(), describe_json());
        let parsed: serde_json::Value = serde_json::from_str(&describe_json()).expect("json");
        assert_eq!(parsed["status"], "valid");
        assert_eq!(parsed["capabilities"]["identity"], IDENTITY);
    }

    #[test]
    fn registry_pin_resolves() {
        DiagnosticRegistry::mcp_successor().expect("mcp registry");
        assert_eq!(registry_version(), "0.8.0");
    }
}
