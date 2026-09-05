//! Issue #22 core tests for the neutral trace manifest: the committed
//! golden contract, the partial semantics, the invalid fixture matrix,
//! canonical determinism under permutation, occurrence safety, and the
//! derived forward/reverse queries.

use lekalo_core::trace::{Completeness, QuerySelection, TraceManifest};

const GOLDEN: &str = include_str!("../../../tests/fixtures/trace/golden/planner.trace.json");
const GOLDEN_DIGEST: &str =
    "sha256:b0fabdc6f2bc55f392e912989579ce8b56b41547d7a4782f06acfda41a3a5031";
const FULL: &str = include_str!("../../../tests/fixtures/trace/full.trace.json");
const PARTIAL: &str = include_str!("../../../tests/fixtures/trace/partial.trace.json");
const INVALID_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/trace/invalid"
);

fn detail_of(bytes: &[u8]) -> String {
    let error = lekalo_core::trace::TraceManifest::parse(bytes)
        .err()
        .expect("fixture must be rejected");
    let diagnostic = error.as_slice()[0].clone();
    let value = serde_json::to_value(&diagnostic).expect("diagnostic json");
    value["data"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn all_invalid_fixtures() -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<(String, Vec<u8>)> = std::fs::read_dir(INVALID_DIR)
        .expect("invalid fixture directory")
        .map(|entry| {
            let path = entry.expect("entry").path();
            let name = path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            let bytes = std::fs::read(&path).expect("fixture bytes");
            (name, bytes)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

#[test]
fn golden_parses_and_canonical_bytes_are_byte_identical_to_the_committed_file() {
    let manifest = TraceManifest::parse(GOLDEN.as_bytes()).expect("golden parses");
    let canonical = manifest.canonical_bytes().expect("canonical bytes");
    assert_eq!(canonical, GOLDEN.trim_end_matches('\n'), "canonical bytes");
    assert_eq!(manifest.digest().expect("digest"), GOLDEN_DIGEST);

    // The pretty input file is accepted and normalizes onto the exact
    // same canonical bytes: input spelling never leaks into the export.
    let from_input = TraceManifest::parse(FULL.as_bytes()).expect("input parses");
    assert_eq!(
        from_input.canonical_bytes().expect("canonical bytes"),
        canonical,
        "input and golden canonicalize identically"
    );
    assert_eq!(from_input.digest().expect("digest"), GOLDEN_DIGEST);
}

#[test]
fn golden_report_and_chain_coverage_match_the_committed_contract() {
    let manifest = TraceManifest::parse(GOLDEN.as_bytes()).expect("golden parses");
    let report = manifest.report();
    assert_eq!(report.manifest_id, "planner-trace-full");
    assert_eq!(report.project_ref, "planner");
    assert_eq!(report.completeness, Completeness::Full);
    assert_eq!(report.node_count, 9);
    assert_eq!(report.relation_count, 9);
    assert_eq!(report.gap_count, 0);
    assert!(report.uncovered_sinks.is_empty(), "full covers every sink");
}

#[test]
fn partial_manifest_keeps_every_missing_link_visible() {
    let manifest = TraceManifest::parse(PARTIAL.as_bytes()).expect("partial parses");
    let report = manifest.report();
    assert_eq!(report.completeness, Completeness::Partial);
    assert_eq!(report.gap_count, 2);
    assert_eq!(
        report.uncovered_sinks,
        vec!["requirement:PLANNER-REQ-003".to_owned()]
    );
    let gaps = &manifest.manifest().gaps;
    assert_eq!(gaps[0].gap_kind.as_str(), "missing-gate");
    assert_eq!(gaps[0].status.as_str(), "candidate");
    assert_eq!(gaps[1].gap_kind.as_str(), "stale-revision");
}

#[test]
fn every_invalid_fixture_is_rejected_with_its_typed_detail() {
    let expected: &[(&str, &str)] = &[
        ("absolute-path.json", "path-invalid"),
        ("artifact-missing-digest.json", "node-invalid"),
        ("backslash-path.json", "path-invalid"),
        ("bad-ir-version.json", "contract-ref-invalid"),
        ("bad-model-version.json", "contract-ref-invalid"),
        ("bad-occurrence.json", "relation-invalid"),
        ("bad-revision.json", "identity-invalid"),
        ("bom-prefix.json", "bom-prefix"),
        ("confirmed-policy-inferred.json", "confirmed-policy"),
        ("confirmed-policy-no-evidence.json", "confirmed-policy"),
        ("confirmed-policy-revision.json", "confirmed-policy"),
        ("dangling-endpoint.json", "dangling-endpoint"),
        ("dangling-evidence.json", "dangling-evidence"),
        ("device-path.json", "path-invalid"),
        ("duplicate-identity.json", "duplicate-identity"),
        ("duplicate-json-key.json", "duplicate-key"),
        ("duplicate-node-id.json", "duplicate-node-id"),
        ("duplicate-relation-tuple.json", "duplicate-relation"),
        ("endpoints-illegal.json", "endpoints-illegal"),
        ("foreign-identity.json", "node-invalid"),
        ("full-with-gaps.json", "full-with-gaps"),
        ("full-with-uncovered-sink.json", "full-with-uncovered-sink"),
        ("gap-confirmed-status.json", "gap-invalid"),
        ("gap-dangling-anchor.json", "dangling-gap-anchor"),
        ("malformed-digest.json", "contract-ref-invalid"),
        ("malformed-semantic-id.json", "node-invalid"),
        ("manifest-id-uppercase.json", "identity-invalid"),
        ("over-limit-nodes.json", "over-limit"),
        ("partial-without-gap.json", "partial-without-gap"),
        ("relation-id-mismatch.json", "relation-invalid"),
        ("trailing-bytes.json", "trailing-bytes"),
        ("traversal-path.json", "path-invalid"),
        ("unknown-external-system.json", "schema-invalid"),
        ("unknown-node-key.json", "schema-invalid"),
        ("unknown-top-key.json", "schema-invalid"),
        ("wrong-identity.json", "schema-invalid"),
        ("wrong-schema-version.json", "schema-invalid"),
    ];
    let fixtures = all_invalid_fixtures();
    assert_eq!(fixtures.len(), expected.len(), "fixture inventory");
    for ((name, bytes), (expected_name, expected_detail)) in fixtures.iter().zip(expected) {
        assert_eq!(name, expected_name, "fixture order");
        assert!(
            TraceManifest::parse(bytes).is_err(),
            "{name} must be rejected"
        );
        assert_eq!(&detail_of(bytes), expected_detail, "{name} detail");
    }
}

#[test]
fn canonical_bytes_are_stable_under_input_permutation() {
    let mut value: serde_json::Value = serde_json::from_str(FULL).expect("json");
    // Reverse every array the manifest carries, then reverse the
    // set-like references inside nodes and relations.
    for key in ["nodes", "relations", "gaps"] {
        let items = value[key].as_array().expect("array").clone();
        value[key] = serde_json::Value::Array(items.into_iter().rev().collect());
    }
    for node in value["nodes"].as_array_mut().expect("nodes") {
        if let Some(refs) = node.get_mut("externalRefs").and_then(|v| v.as_array_mut()) {
            let reversed: Vec<serde_json::Value> = refs.iter().rev().cloned().collect();
            *refs = reversed;
        }
    }
    for relation in value["relations"].as_array_mut().expect("relations") {
        if let Some(refs) = relation
            .get_mut("evidenceRefs")
            .and_then(|v| v.as_array_mut())
        {
            let reversed: Vec<serde_json::Value> = refs.iter().rev().cloned().collect();
            *refs = reversed;
        }
    }
    let permuted = serde_json::to_string(&value).expect("serialize");
    let manifest = TraceManifest::parse(permuted.as_bytes()).expect("permuted parses");
    assert_eq!(
        manifest.canonical_bytes().expect("canonical bytes"),
        GOLDEN.trim_end_matches('\n'),
        "permutation never changes the canonical bytes"
    );
    assert_eq!(manifest.digest().expect("digest"), GOLDEN_DIGEST);
}

#[test]
fn occurrence_safe_bindings_stay_distinct_and_answer_queries() {
    let manifest = TraceManifest::parse(GOLDEN.as_bytes()).expect("golden parses");
    let bindings: Vec<_> = manifest
        .relations()
        .filter(|relation| relation.relation_kind.as_str() == "binds")
        .collect();
    assert_eq!(bindings.len(), 2, "same pair, distinct occurrences");
    assert_ne!(bindings[0].occurrence, bindings[1].occurrence);
    assert_ne!(bindings[0].relation_id, bindings[1].relation_id);

    let artifacts = manifest
        .query(&QuerySelection::ArtifactsFor(
            "planner.focus_task".to_owned(),
        ))
        .expect("query");
    assert_eq!(artifacts.len(), 2, "both occurrences answer");
    assert_eq!(artifacts[0].id, "apps-api-focus-task");
    assert_eq!(artifacts[1].id, "apps-api-focus-task");
}

#[test]
fn reverse_and_forward_queries_answer_from_the_normalized_index() {
    let manifest = TraceManifest::parse(GOLDEN.as_bytes()).expect("golden parses");

    let requirements = manifest
        .query(&QuerySelection::RequirementsFor(
            "planner.focus_task".to_owned(),
        ))
        .expect("query");
    let ids: Vec<&str> = requirements.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(
        ids,
        ["PLANNER-REQ-001", "PLANNER-REQ-002"],
        "one symbol, two requirements"
    );

    let symbols = manifest
        .query(&QuerySelection::SymbolsFor("PLANNER-REQ-001".to_owned()))
        .expect("query");
    let ids: Vec<&str> = symbols.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(
        ids,
        ["planner.focus_task", "planner.switch_focus"],
        "one requirement, two symbols"
    );

    let tests = manifest
        .query(&QuerySelection::TestsFor("planner.focus_task".to_owned()))
        .expect("query");
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].id, "node.focus-task-switch");
    assert_eq!(tests[0].relation.as_str(), "verifies");

    let gates = manifest
        .query(&QuerySelection::GatesFor(
            "node.focus-task-switch".to_owned(),
        ))
        .expect("query");
    assert_eq!(gates.len(), 1);
    assert_eq!(gates[0].id, "hlv.gate.focus");

    let diagnostics = manifest
        .query(&QuerySelection::DiagnosticsFor("hlv.gate.focus".to_owned()))
        .expect("query");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].id, "hlv.diag.focus");
}

#[test]
fn external_original_ids_survive_canonical_export_verbatim() {
    let manifest = TraceManifest::parse(GOLDEN.as_bytes()).expect("golden parses");
    assert!(manifest
        .manifest()
        .nodes
        .iter()
        .any(|node| { node.path.as_deref() == Some("apps/api/src/planner/focus-task.ts") }));
    for hostile in [
        "https://example.com/x.ts",
        "trailing/space/ ",
        "..",
        "a/./b",
        "spec~1/focus.ts",
    ] {
        let mut value: serde_json::Value = serde_json::from_str(FULL).expect("json");
        for node in value["nodes"].as_array_mut().expect("nodes") {
            if node["nodeKind"] == "artifact" {
                node["path"] = serde_json::json!(hostile);
            }
        }
        let bytes = serde_json::to_vec(&value).expect("serialize");
        assert!(
            TraceManifest::parse(&bytes).is_err(),
            "rejected: {hostile:?}"
        );
    }
}

#[test]
fn known_but_unmatched_queries_are_empty_never_errors() {
    let manifest = TraceManifest::parse(GOLDEN.as_bytes()).expect("golden parses");
    let rows = manifest
        .query(&QuerySelection::ArtifactsFor(
            "planner.switch_focus".to_owned(),
        ))
        .expect("query");
    assert!(rows.is_empty(), "a symbol with no binding answers empty");
}

#[test]
fn trace_module_constants_match_the_recorded_owner_decisions() {
    assert_eq!(
        lekalo_core::trace::version::SCHEMA_VERSION,
        "lekalo/trace-manifest/v1.0.0"
    );
    assert_eq!(
        lekalo_core::trace::version::IDENTITY,
        "dev.lekalo.trace-manifest@1.0.0"
    );
    assert_eq!(
        lekalo_core::trace::io_failure("file-missing").as_slice()[0].id(),
        "loader.io"
    );
}
