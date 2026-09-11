//! Issue #30 integration tests: the foreign/custom implementation
//! attachment decoded through the production wire seam, validated
//! against the compiled full-kinds fixture IR, and projected into the
//! deterministic per-target portability report. The semantic fixture
//! matrix under `tests/fixtures/implementation/semantic/` drives one
//! assertion per registered rule; the canonical-bytes and bounds
//! assertions pin the wire contract itself.
//!
//! Selectors are invocation-relative and the #4 grammar rejects
//! traversal, so the fixture-driven assertions run under one sequential
//! test that pins the process working directory to the workspace root
//! and restores it.

use lekalo_core::implementation::{self, ImplementationDocument, ImplementationKind, PortStatus};
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::lockfile::types::Sha256Digest;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FULL_KINDS: &str = "tests/fixtures/ir/valid-full-kinds";
const SEMANTIC: &str = "tests/fixtures/implementation/semantic";
const VALID_DIR: &str = "tests/fixtures/implementation/valid";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// The compiled full-kinds fixture project shared by every semantic
/// case; the fixture IR is never mutated by validation.
fn compilation() -> lekalo_core::ir::Compilation {
    let selection = LoadSelection {
        project: Some(FULL_KINDS.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("fixture load failed: {}", outcome.to_json_string()),
    };
    match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!(
            "fixture IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn semantic_dir() -> PathBuf {
    workspace_root().join(SEMANTIC)
}

fn read_json(path: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

/// The reason codes of one normalized set, in wire order.
fn reason_codes(set: &lekalo_core::diagnostics::DiagnosticSet) -> Vec<String> {
    set.as_slice()
        .iter()
        .map(|diagnostic| diagnostic.id().to_owned())
        .collect()
}

fn codes(expect: &serde_json::Value) -> Vec<String> {
    expect["reasonCodes"]
        .as_array()
        .expect("reasonCodes")
        .iter()
        .map(|code| code.as_str().expect("code").to_owned())
        .collect()
}

#[test]
fn implementation_suite_runs_from_the_workspace_root() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let result = std::panic::catch_unwind(|| {
        every_semantic_fixture_matches_its_registered_expectation();
        every_valid_fixture_decodes();
        the_issue_example_binds_three_targets_without_generation();
        canonical_bytes_are_deterministic_and_digest_bound();
        canonical_ir_digest_pins_the_attachment_to_one_revision();
        portability_reports_missing_project_targets_and_scenario_coverage();
        wire_rejects_hostile_and_unbounded_documents();
    });
    std::env::set_current_dir(original).expect("restore working dir");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn every_semantic_fixture_matches_its_registered_expectation() {
    let dir = semantic_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("semantic fixtures")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json") && !name.ends_with(".expect.json"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "semantic fixture matrix is present");

    let compiled = compilation();
    for name in names {
        let document = read_json(&dir.join(&name));
        let expect =
            read_json(&dir.join(format!("{}.expect.json", name.trim_end_matches(".json"))));
        let expected_status = expect["status"].as_str().expect("status");

        match ImplementationDocument::from_value(&document) {
            Err(set) => {
                // Document-level self-checks (selection rules, symbol
                // grammar) refuse at the wire seam.
                assert_eq!(
                    expected_status, "invalid",
                    "{name}: unexpected wire refusal"
                );
                assert_eq!(reason_codes(&set), codes(&expect), "{name}: reason codes");
            }
            Ok(attachment) => match implementation::validate(&attachment, &compiled) {
                Err(set) => {
                    assert_eq!(expected_status, "invalid", "{name}: unexpected invalid set");
                    assert_eq!(reason_codes(&set), codes(&expect), "{name}: reason codes");
                }
                Ok(report) => {
                    assert_eq!(expected_status, "valid", "{name}: unexpected valid outcome");
                    assert_eq!(
                        reason_codes(report.warnings()),
                        codes(&expect),
                        "{name}: warning codes"
                    );
                }
            },
        }
    }
}
fn every_valid_fixture_decodes() {
    let dir = workspace_root().join(VALID_DIR);
    for name in ["schedule.json", "planner-full.json"] {
        let document = read_json(&dir.join(name));
        ImplementationDocument::from_value(&document)
            .unwrap_or_else(|set| panic!("{name}: refused: {set:?}"));
    }
}

fn the_issue_example_binds_three_targets_without_generation() {
    let document = read_json(&workspace_root().join(VALID_DIR).join("schedule.json"));
    let attachment = ImplementationDocument::from_value(&document).expect("issue example decodes");
    assert_eq!(attachment.contracts().len(), 1);
    let contract = &attachment.contracts()[0];
    assert_eq!(contract.symbol(), "schedule.calculate");
    assert_eq!(contract.contract(), "schedule.calculator/v1");
    assert_eq!(contract.targets().len(), 3);
    assert_eq!(contract.targets()[0].kind(), ImplementationKind::Foreign);
    assert_eq!(contract.targets()[0].symbol(), Some("schedule.Calculate"));
    assert_eq!(
        contract.targets()[1].symbol(),
        Some("@example/core-domain#calculateSchedule")
    );
    assert_eq!(
        contract.targets()[2].symbol(),
        Some("App\\Schedule\\CalculateSchedule::__invoke")
    );
}

fn canonical_bytes_are_deterministic_and_digest_bound() {
    let document = read_json(&semantic_dir().join("planner-full.json"));
    let attachment = ImplementationDocument::from_value(&document).expect("decodes");
    let first = attachment.canonical_bytes().expect("canonical bytes");
    let second = attachment.canonical_bytes().expect("canonical bytes");
    assert_eq!(first, second, "canonical bytes are deterministic");
    assert!(!first.contains('\n'), "compact: no newlines");
    let digest = attachment.digest().expect("digest");
    let expected =
        Sha256Digest::from_hex(&lekalo_core::versioning::plan::sha256_hex(first.as_bytes()));
    assert_eq!(digest.as_str(), expected.as_str());
    // Round trip: the canonical bytes decode to an equal attachment.
    let round: serde_json::Value = serde_json::from_str(&first).expect("canonical parses");
    let reparsed = ImplementationDocument::from_value(&round).expect("canonical decodes");
    assert_eq!(reparsed, attachment);
}

fn canonical_ir_digest_pins_the_attachment_to_one_revision() {
    let compiled = compilation();
    let expected = implementation::ir_digest(&compiled.project);
    let document = read_json(&semantic_dir().join("planner-full.json"));
    let attachment = ImplementationDocument::from_value(&document).expect("decodes");
    assert_eq!(attachment.ir_digest().as_str(), expected.as_str());
}

fn portability_reports_missing_project_targets_and_scenario_coverage() {
    let compiled = compilation();
    let document = read_json(&semantic_dir().join("planner-full.json"));
    let attachment = ImplementationDocument::from_value(&document).expect("decodes");
    let report = implementation::validate(&attachment, &compiled)
        .expect("full fixture validates")
        .portability()
        .clone();

    assert_eq!(report.project, "planner");
    assert_eq!(report.symbols.len(), 3);

    let focus = report
        .symbols
        .iter()
        .find(|symbol| symbol.symbol == "planner.focus_task")
        .expect("focus entry");
    // The project binds node-typescript; go-service and php-laravel are
    // attachment-declared. The focus scenario covers the operation once
    // for every implementation.
    assert_eq!(focus.targets.len(), 3);
    assert_eq!(
        focus
            .targets
            .iter()
            .find(|row| row.target == "go-service")
            .expect("go row")
            .status,
        PortStatus::Unsupported
    );
    assert_eq!(
        focus
            .targets
            .iter()
            .find(|row| row.target == "node-typescript")
            .expect("node row")
            .status,
        PortStatus::Foreign
    );
    assert_eq!(focus.scenarios, vec!["planner.focus_flow".to_owned()]);

    let query = report
        .symbols
        .iter()
        .find(|symbol| symbol.symbol == "planner.count_focused")
        .expect("query entry");
    assert_eq!(
        query
            .targets
            .iter()
            .find(|row| row.target == "node-typescript")
            .expect("node row")
            .status,
        PortStatus::External
    );
    // No scenario covers the query.
    assert!(query.scenarios.is_empty());
    // The scenario suite never distinguishes implementations: the
    // covered foreign operation and an uncovered generated operation
    // differ only in the coverage data, never in the rules applied.
    let notify = report
        .symbols
        .iter()
        .find(|symbol| symbol.symbol == "notify.notify_user")
        .expect("notify entry");
    assert_eq!(
        notify
            .targets
            .iter()
            .find(|row| row.target == "php-laravel")
            .expect("php row")
            .status,
        PortStatus::Custom
    );
    assert!(
        notify
            .targets
            .iter()
            .any(|row| row.status == PortStatus::Missing),
        "notify misses the project's node-typescript binding"
    );
}

fn wire_rejects_hostile_and_unbounded_documents() {
    let base: serde_json::Value = read_json(&semantic_dir().join("planner-full.json"));
    // Unknown top-level members fail closed.
    let mut hostile = base.clone();
    hostile["sources"] = serde_json::json!("../../etc/passwd");
    assert!(ImplementationDocument::from_value(&hostile).is_err());
    // A traversal-looking foreign symbol is refused as symbol-invalid.
    hostile = base.clone();
    hostile["contracts"] = serde_json::json!([
        {
            "symbol": "planner.focus_task",
            "contract": "planner.focus/v1",
            "targets": [
                {"target": "node-typescript", "kind": "foreign", "symbol": "..\\..\\focus"}
            ]
        }
    ]);
    let set = ImplementationDocument::from_value(&hostile).expect_err("traversal refused");
    assert!(!reason_codes(&set).is_empty());
    assert!(reason_codes(&set)
        .iter()
        .all(|code| code == "implementation.symbol-invalid"));
    // Over-bound contract count fails closed.
    hostile = base.clone();
    let contract = hostile["contracts"][0].clone();
    hostile["contracts"] = serde_json::Value::Array(vec![contract; 257]);
    assert!(ImplementationDocument::from_value(&hostile).is_err());
}
