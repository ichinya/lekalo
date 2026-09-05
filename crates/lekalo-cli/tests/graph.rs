//! Issue #13 CLI tests for the `lekalo graph` handoff: the hermetic
//! planner fixture, byte-identical export determinism, show/callers/path
//! probes on the accepted 0/1 envelope, the spans sidecar, and the
//! unknown-node failure class.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/graph/planner";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run the real lekalo binary")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn export(project: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["--json", "graph", "export", "--project", "."];
    args.extend_from_slice(extra);
    lekalo_in(project, &args)
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(FIXTURE)
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

/// The canonical export is byte-identical across runs and matches the
/// pinned golden graph bytes inside the success envelope.
#[test]
fn export_is_byte_identical_and_matches_the_golden() {
    let project = fixture_path();
    let first = export(&project, &[]);
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let second = export(&project, &[]);
    assert_eq!(exit_code(&second), 0);
    assert_eq!(stdout_text(&first), stdout_text(&second));

    let golden = std::fs::read_to_string(workspace_path(
        "tests/fixtures/graph/golden/planner.graph.json",
    ))
    .expect("golden graph");
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&first).trim()).expect("envelope json");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["graph"]["identity"], "dev.lekalo.graph@1.0.0");
    assert_eq!(document["graph"]["schemaVersion"], "lekalo/graph/v1.0.0");
    assert_eq!(document["graph"]["modelVersion"], "1.0.0");
    assert_eq!(document["graph"]["metadata"]["nodeCount"], 24);
    assert_eq!(document["graph"]["metadata"]["edgeCount"], 32);
    let rendered = serde_json::to_string(&document["graph"]).expect("graph bytes");
    assert_eq!(rendered, golden, "export must match the pinned golden");
}

/// The export carries every direct relation the fixture declares and none
/// of the registered-but-not-emitted ones.
#[test]
fn export_contains_only_emitted_core_relations() {
    let project = fixture_path();
    let output = export(&project, &[]);
    assert_eq!(exit_code(&output), 0);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    let edges = document["graph"]["edges"].as_array().expect("edges");
    let mut relations: Vec<&str> = edges
        .iter()
        .map(|edge| edge["relation"].as_str().expect("relation"))
        .collect();
    relations.sort();
    relations.dedup();
    assert_eq!(
        relations,
        vec![
            "accepts",
            "authorizes",
            "derived_from",
            "emits",
            "exposes",
            "reads",
            "references",
            "requires",
            "returns"
        ]
    );
    // Repeated identical references at different sites stay separate edges.
    let accepts: Vec<_> = edges
        .iter()
        .filter(|edge| {
            edge["relation"] == "accepts"
                && edge["from"] == "operation:planner.focus_task"
                && edge["to"] == "type:planner.task_id"
        })
        .collect();
    assert_eq!(accepts.len(), 2);
    assert_eq!(accepts[0]["occurrence"], 0);
    assert_eq!(accepts[1]["occurrence"], 1);
}

/// `graph show` answers from the indexes: direct dependencies and
/// dependents in canonical order on the valid envelope.
#[test]
fn show_reports_direct_dependencies_and_dependents() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &["--json", "graph", "show", "planner.task", "--project", "."],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope json");
    assert_eq!(document["graph"]["node"]["id"], "entity:planner.task");
    assert_eq!(document["graph"]["node"]["kind"], "entity");
    assert_eq!(document["graph"]["node"]["module"], "planner");
    let dependencies = document["graph"]["dependencies"].as_array().expect("deps");
    assert_eq!(dependencies.len(), 5);
    assert_eq!(dependencies[0]["to"], "type:planner.due_date");
    let dependents = document["graph"]["dependents"].as_array().expect("deps");
    assert_eq!(dependents.len(), 3);
    assert_eq!(dependents[0]["from"], "effect:planner.create_task");
}

/// `graph callers` is the reverse view; `--transitive` walks the closure.
#[test]
fn callers_report_the_reverse_view() {
    let project = fixture_path();
    let direct = lekalo_in(
        &project,
        &[
            "--json",
            "graph",
            "callers",
            "planner.task_focused",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&direct), 0, "{}", stderr_text(&direct));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&direct).trim()).expect("envelope json");
    let callers = document["graph"]["callers"].as_array().expect("callers");
    assert_eq!(callers.len(), 3);
    assert_eq!(document["graph"]["complete"], true);

    let transitive = lekalo_in(
        &project,
        &[
            "--json",
            "graph",
            "callers",
            "planner.text",
            "--transitive",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&transitive), 0);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&transitive).trim()).expect("envelope json");
    assert_eq!(document["graph"]["complete"], true);
    let callers = document["graph"]["callers"].as_array().expect("callers");
    assert!(callers.len() > 3, "the closure exceeds the direct view");
}

/// `graph path` finds the deterministic shortest path; disconnected nodes
/// are an explicit graph.path-not-found failure, and an unknown node is
/// graph.unknown-node.
#[test]
fn path_answers_are_explicit() {
    let project = fixture_path();
    let found = lekalo_in(
        &project,
        &[
            "--json",
            "graph",
            "path",
            "planner.api_focus",
            "planner.task_focused",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&found), 0, "{}", stderr_text(&found));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&found).trim()).expect("envelope json");
    let path = &document["graph"];
    assert_eq!(path["from"], "endpoint:planner.api_focus");
    assert_eq!(path["to"], "event:planner.task_focused");
    assert_eq!(path["length"], 2);
    assert_eq!(path["confidence"], "canonical");
    assert_eq!(path["nodes"][0], "endpoint:planner.api_focus");
    assert_eq!(path["nodes"][2], "event:planner.task_focused");

    let missing = lekalo_in(
        &project,
        &[
            "--json",
            "graph",
            "path",
            "target-binding:planner.binding_node",
            "planner.task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&missing), 1);
    assert!(stderr_text(&missing).contains("graph.path-not-found"));

    let unknown = lekalo_in(
        &project,
        &[
            "--json",
            "graph",
            "show",
            "planner.nonesuch",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&unknown), 1);
    let text = stderr_text(&unknown);
    assert!(text.contains("graph.unknown-node"));
    assert!(text.contains("LEK-GRAPH-007"));
}

/// `--spans` attaches the declaration sidecar from the #8 source map;
/// the semantic graph bytes stay identical with and without it.
#[test]
fn spans_sidecar_is_additive_and_kind_aware() {
    let project = fixture_path();
    let plain = export(&project, &[]);
    let with_spans = export(&project, &["--spans"]);
    assert_eq!(exit_code(&with_spans), 0);
    let plain: serde_json::Value =
        serde_json::from_str(stdout_text(&plain).trim()).expect("envelope json");
    let spans: serde_json::Value =
        serde_json::from_str(stdout_text(&with_spans).trim()).expect("envelope json");
    assert_eq!(plain["graph"], spans["graph"]);
    let nodes = spans["spans"]["nodes"].as_array().expect("sidecar");
    assert_eq!(nodes.len(), 22, "every node but the two requirements");
    let by_id: std::collections::HashMap<&str, &serde_json::Value> = nodes
        .iter()
        .map(|node| (node["id"].as_str().expect("id"), node))
        .collect();
    let project_span = by_id["project:planner"]["span"]["path"]
        .as_str()
        .expect("path");
    assert!(project_span.ends_with("project.yaml"));
    let module_span = by_id["module:planner"]["span"]["path"]
        .as_str()
        .expect("path");
    assert!(module_span.ends_with("modules/planner/module.yaml"));
    assert!(!by_id.contains_key("requirement:PLANNER-REQ-001"));
}

/// The human and JSON projections carry the same result on the same
/// envelope: the human summary names the graph identity and counts.
#[test]
fn human_projection_matches_the_json_envelope() {
    let project = fixture_path();
    let output = lekalo_in(&project, &["graph", "export", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    let stdout = stdout_text(&output);
    assert!(
        stdout.contains("built graph dev.lekalo.graph@1.0.0: 24 nodes, 32 edges"),
        "human summary: {stdout}"
    );
}

/// The version custody probe: the binary reports the prospective product
/// version in both projections.
#[test]
fn version_reports_the_prospective_product_version() {
    let project = fixture_path();
    let human = lekalo_in(&project, &["--version"]);
    assert_eq!(exit_code(&human), 0);
    assert_eq!(stdout_text(&human).trim(), "lekalo 0.1.12");
    let json = lekalo_in(&project, &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.1.12\"\n}"
    );
}
