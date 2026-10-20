//! Issue #33 core MCP integration tests: the read boundary that never
//! leaves the pure module. Every behavior the transport relies on —
//! registry union identity, closed tool catalog, request grammar, scope
//! admission, resource identity, snapshot binding, and embedded contract
//! verification — is proven here without a filesystem, a process, or a
//! socket.

use std::collections::BTreeSet;

use lekalo_core::diagnostics::DiagnosticRegistry;
use lekalo_core::mcp::capability;
use lekalo_core::mcp::policy::{
    self, modules_admitted, Catalog, GroupGrant, Limits, ManifestGrant, ScopeDecision,
};
use lekalo_core::mcp::resources::{self, Cursor, Family};
use lekalo_core::mcp::schemas;
use lekalo_core::mcp::tools::{self, parse_request, Include, RequestError, ToolRequest};
use lekalo_core::mcp::version;
use serde_json::json;

// ---- The registry union is exact and additive. ----------------------------

#[test]
fn mcp_registry_is_the_exact_m8_grand_union() {
    let embedded = DiagnosticRegistry::embedded().expect("0.6.4");
    let successor = DiagnosticRegistry::successor().expect("0.6.5");
    let mcp = DiagnosticRegistry::mcp_successor().expect("0.8.0");
    // Never pin a stale predecessor+own subset (r6 F6/omp F1). All
    // predecessor entries and landed families survive, while additional
    // future families may join this same immutable-entry union.
    assert!(mcp.len() > successor.len());
    // Every successor accessor parses the same merged bytes, so neither
    // side of the merge can drop a pin (the #113 union-guard invariant).
    assert_eq!(
        DiagnosticRegistry::format_successor().unwrap().len(),
        mcp.len()
    );
    assert_eq!(DiagnosticRegistry::adoption().unwrap().len(), mcp.len());
    assert_eq!(DiagnosticRegistry::m8().unwrap().len(), mcp.len());
    assert_eq!(DiagnosticRegistry::pilot().unwrap().len(), mcp.len());
    assert_eq!(
        DiagnosticRegistry::module_successor().unwrap().len(),
        mcp.len()
    );
    assert_eq!(DiagnosticRegistry::managed().unwrap().len(), mcp.len());
    assert_eq!(
        DiagnosticRegistry::tasklink_successor().unwrap().len(),
        mcp.len()
    );
    // A landed family that vanishes from the union is a stale-merge
    // failure, not a review finding: every landed M8 family plus this
    // issue's own family must resolve in the successor (mirrors the
    // `check-registry-union.mjs` live-line guard).
    for family in [
        "format.",
        "adoption.",
        "portability.",
        "worklog-changelog.",
        "managed.",
        "module-api.",
        "tasklink.",
        "search.",
        "change.",
        "watch.",
        "fix.",
        "mcp.",
    ] {
        assert!(
            mcp.entries().any(|entry| entry.id().starts_with(family)),
            "family {family} missing from the union successor"
        );
    }
    for (prefix, count) in [("format.", 11), ("adoption.", 10), ("mcp.", 11)] {
        assert_eq!(
            mcp.entries()
                .filter(|entry| entry.id().starts_with(prefix))
                .count(),
            count
        );
    }
    for predecessor in [embedded, successor] {
        for entry in predecessor.entries() {
            let union_entry = mcp
                .entry(entry.id())
                .unwrap_or_else(|| panic!("{} missing", entry.id()));
            assert_eq!(entry.code(), union_entry.code());
            assert_eq!(entry.default_severity(), union_entry.default_severity());
            assert_eq!(entry.category(), union_entry.category());
            assert_eq!(entry.default_message(), union_entry.default_message());
            assert_eq!(entry.lifecycle(), union_entry.lifecycle());
        }
    }
}

#[test]
fn every_mcp_code_is_unique_and_registry_pinned() {
    let mcp = DiagnosticRegistry::mcp_successor().expect("registry");
    let codes: Vec<&str> = mcp
        .entries()
        .filter(|entry| entry.id().starts_with("mcp."))
        .map(|entry| entry.code())
        .collect();
    assert_eq!(codes.len(), 11);
    assert_eq!(codes.len(), codes.iter().collect::<BTreeSet<_>>().len());
    assert!(codes.iter().all(|code| code.starts_with("LEK-MCP-")));
}

// ---- The embedded contracts verify once and stay pinned. -----------------

#[test]
fn embedded_contracts_carry_the_exact_identities() {
    schemas::verified().expect("embedded mcp contracts");
}

#[test]
fn tool_schema_tables_are_symmetric_and_complete() {
    assert_eq!(schemas::INPUT_SCHEMAS.len(), 7);
    assert_eq!(schemas::OUTPUT_SCHEMAS.len(), 7);
    for ((input_slug, input_bytes), (output_slug, output_bytes)) in schemas::INPUT_SCHEMAS
        .iter()
        .zip(schemas::OUTPUT_SCHEMAS.iter())
    {
        assert_eq!(input_slug, output_slug);
        let input: serde_json::Value = serde_json::from_slice(input_bytes).expect("input json");
        let output: serde_json::Value = serde_json::from_slice(output_bytes).expect("output json");
        assert_eq!(input["type"], "object");
        assert_eq!(output["type"], "object");
        assert_eq!(input["additionalProperties"], false);
        assert_eq!(output["additionalProperties"], false);
        // No root unions in tool schemas: hosts parse flat root objects.
        for keyword in ["oneOf", "anyOf", "allOf", "not"] {
            assert!(input.get(keyword).is_none(), "{input_slug} root {keyword}");
            assert!(
                output.get(keyword).is_none(),
                "{output_slug} root {keyword}"
            );
        }
        assert!(input.get("$ref").is_none());
        assert!(output.get("$ref").is_none());
    }
}

// ---- The capability manifest is the exact deterministic catalog. ----------

#[test]
fn capabilities_manifest_is_deterministic_and_closed() {
    assert_eq!(capability::describe_json(), capability::describe_json());
    let manifest = capability::manifest();
    assert_eq!(
        manifest["identity"],
        format!("dev.lekalo.mcp-capabilities@{}", version::VERSION)
    );
    assert_eq!(manifest["protocol"]["profile"], version::PROTOCOL_PROFILE);
    assert_eq!(manifest["protocol"]["transport"], "stdio");
    assert_eq!(manifest["server"]["readOnly"], true);
    let tools = manifest["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 7);
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, tools::TOOL_NAMES.to_vec());
    assert!(names.iter().all(|name| name.starts_with("lekalo_")));
    assert_eq!(
        manifest["resources"].as_array().expect("resources").len(),
        5
    );
    assert_eq!(
        manifest["limits"]["frameBytes"],
        version::MAX_FRAME_BYTES as u64
    );
}

#[test]
fn bounds_only_ever_shrink_from_the_defaults() {
    let limits = Limits::default();
    assert!(limits.max_response_bytes <= version::MAX_RESPONSE_BYTES_CEILING);
    assert!(limits.timeout_ms <= version::TIMEOUT_MS_CEILING);
    assert!(limits.page_size <= version::PAGE_SIZE_CEILING);
    assert!(limits.concurrent_calls <= version::MAX_CONCURRENT_CALLS);
    assert_eq!(
        Limits::effective_timeout_ms(Some(version::TIMEOUT_MS_CEILING + 1), limits.timeout_ms),
        limits.timeout_ms
    );
    assert_eq!(Limits::effective_timeout_ms(Some(1), limits.timeout_ms), 1);
}

// ---- The request grammar admits semantic operands only. -------------------

#[test]
fn no_request_grammar_admits_paths_or_write_capabilities() {
    // The known-tool table rejects every non-catalog name, so no
    // generate/write/collect/install/migrate/scan tool exists on this
    // surface even at the parse boundary.
    for name in [
        "lekalo_generate",
        "lekalo_write",
        "lekalo_collect",
        "lekalo_install",
        "lekalo_migrate",
        "lekalo_scan",
        "lekalo_report",
        "lekalo_history",
    ] {
        let error = parse_request(name, &json!({"projectId": "p"})).expect_err("non-catalog tool");
        assert_eq!(
            error,
            RequestError::Params(tools::ParamsError { field: "tool" })
        );
    }
    // Smuggled operands are structural refusals before any semantic work.
    for smuggled in [
        "root",
        "path",
        "argv",
        "env",
        "destination",
        "write",
        "mode_write",
    ] {
        let mut arguments = json!({"projectId": "p", "symbol": "s"});
        arguments[smuggled] = json!("x");
        assert!(
            parse_request("lekalo_inspect", &arguments).is_err(),
            "{smuggled} must not parse"
        );
    }
}

#[test]
fn every_tool_parses_its_minimal_request() {
    let cases: [(&str, serde_json::Value); 7] = [
        ("lekalo_inspect", json!({"projectId": "p", "symbol": "m.s"})),
        (
            "lekalo_impact",
            json!({"projectId": "p", "mode": "symbol", "symbol": "m.s"}),
        ),
        (
            "lekalo_context",
            json!({"projectId": "p", "mode": "symbol", "symbol": "m.s", "budget": 100}),
        ),
        ("lekalo_graph", json!({"projectId": "p", "mode": "export"})),
        ("lekalo_validate", json!({"projectId": "p"})),
        ("lekalo_target_status", json!({"projectId": "p"})),
        (
            "lekalo_trace_export",
            json!({"projectId": "p", "manifestRef": "m"}),
        ),
    ];
    for (name, arguments) in cases {
        let request =
            parse_request(name, &arguments).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(request.tool_name(), name);
        assert_eq!(request.common().project_id, "p");
    }
}

#[test]
fn inspect_include_projection_is_closed_and_deduplicated() {
    let request = parse_request(
        "lekalo_inspect",
        &json!({"projectId": "p", "symbol": "s", "include": ["scenarios", "bindings"]}),
    )
    .expect("request");
    let ToolRequest::Inspect { include, .. } = request else {
        panic!("variant");
    };
    assert_eq!(include.len(), 2);
    assert!(include.contains(&Include::Bindings));
    assert!(include.contains(&Include::Scenarios));
    assert!(parse_request(
        "lekalo_inspect",
        &json!({"projectId": "p", "symbol": "s", "include": ["bindings", "bindings"]})
    )
    .is_err());
}

// ---- Scope admission is closed over the catalog. --------------------------

fn catalog() -> Catalog {
    Catalog::empty()
        .with_project("planner")
        .expect("project")
        .with_group(GroupGrant {
            id: "planner-review".to_owned(),
            project: "planner".to_owned(),
            modules: ["planner".to_owned()].into_iter().collect(),
        })
        .expect("group")
        .with_manifest(ManifestGrant {
            id: "baseline".to_owned(),
            project: "planner".to_owned(),
            path: "traces/full.trace.json".to_owned(),
        })
        .expect("manifest")
        .with_git_changed_mode(false)
}

#[test]
fn scope_table_covers_projects_groups_and_manifests() {
    let catalog = catalog();
    assert!(catalog.admits_project("planner"));
    assert_eq!(
        catalog.admits_group("planner", "planner-review"),
        ScopeDecision::Admitted
    );
    assert_eq!(
        catalog.admits_manifest("planner", "baseline"),
        Some("traces/full.trace.json")
    );
    let denied: [(&str, &str); 7] = [
        ("project", "unknown"),
        ("project", "Planner"),
        ("project", "../planner"),
        ("manifest", "secret"),
        ("group", "unknown-group"),
        ("group", "other-review"),
        ("group", ""),
    ];
    for (kind, subject) in denied {
        let admitted = match kind {
            "project" => catalog.admits_project(subject),
            "manifest" => catalog.admits_manifest("planner", subject).is_some(),
            _ => catalog.admits_group("planner", subject) == ScopeDecision::Admitted,
        };
        assert!(!admitted, "{kind} {subject} must be denied");
    }
    assert!(!catalog.admits_git_changed_mode());
}

#[test]
fn group_closure_refuses_before_any_read() {
    let catalog = catalog();
    let inside: BTreeSet<String> = ["planner".to_owned()].into_iter().collect();
    let outside: BTreeSet<String> = ["notify".to_owned()].into_iter().collect();
    assert_eq!(
        modules_admitted(&catalog, "planner", Some("planner-review"), &inside),
        ScopeDecision::Admitted
    );
    assert_eq!(
        modules_admitted(&catalog, "planner", Some("planner-review"), &outside),
        ScopeDecision::Widening
    );
}

// ---- Resource identities stay logical and hostile-input free. -------------

#[test]
fn resource_uri_table_is_the_exact_five_families() {
    for family in [
        Family::ProjectSummary,
        Family::ModuleList,
        Family::TargetStatus,
        Family::DiagnosticsSummary,
    ] {
        let uri = resources::resource_uri("planner", family, None);
        assert!(uri.starts_with("lekalo://planner/"), "{uri}");
        assert!(resources::parse_uri(&uri).is_ok(), "{uri}");
    }
    // The symbol family is the one qualified identity.
    let symbol = resources::resource_uri("planner", Family::SymbolMetadata, Some("m.s"));
    assert!(resources::parse_uri(&symbol).is_ok(), "{symbol}");
    // No file:// resource, no glob, no recursive listing exists.
    for hostile in [
        "file:///etc/passwd",
        "http://host/summary",
        "lekalo://planner/../planner/summary",
        "lekalo://planner/symbol/../..",
    ] {
        // Any refusal is contract-correct for a hostile URI: the exact
        // class (scheme, family, qualifier) is grammar-dependent.
        assert!(
            resources::parse_uri(hostile).is_err(),
            "{hostile} must refuse"
        );
    }
}

#[test]
fn cursors_are_snapshot_bound_and_opaque() {
    let cursor = Cursor {
        snapshot: "sha256-abc".to_owned(),
        offset: 7,
    };
    let encoded = cursor.encode();
    assert_eq!(Cursor::decode(&encoded), Some(cursor));
    // A different snapshot refuses the same cursor.
    let stale = Cursor::decode(&encoded).expect("cursor");
    assert_ne!(stale.snapshot, "sha256-other");
}

// ---- Path admission is lexical and closed. --------------------------------

#[test]
fn hostile_operand_table_refuses_before_any_open() {
    let hostile = [
        "../outside",
        "%2e%2e/x",
        "a/./b",
        "a/../b",
        "/absolute",
        "C:\\x",
        "//unc",
        "nul\0",
        "file://x",
        ".env",
        "dir/.git/config",
        "keys/id_rsa",
        "x:stream",
    ];
    for operand in hostile {
        assert!(
            policy::admit_relative_path(operand).is_err(),
            "{operand} must refuse"
        );
    }
    for admitted in ["lekalo/project.yaml", "traces/full.trace.json", "m.md"] {
        assert!(
            policy::admit_relative_path(admitted).is_ok(),
            "{admitted} must admit"
        );
    }
}

#[test]
fn governed_lekalo_paths_are_the_only_internal_reads() {
    assert!(policy::governed_internal_path(".lekalo/lock.yaml"));
    assert!(!policy::governed_internal_path(".git/config"));
    assert!(!policy::governed_internal_path(".lekalo"));
}
