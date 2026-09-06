//! Issue #17 library tests for the context capsule: deterministic
//! selection over the hermetic planner fixture through the accepted loader
//! seam, the closed section vocabulary, the budget walk and its exact
//! truncation metadata, the changed scope, the closed gap vocabulary, the
//! estimator identity, privacy, and the repository-scan benchmark proxy.
//!
//! The capsule is a pure function of the compilation, the scope, the
//! budget, and the spans flag: every test below re-plans and compares
//! whole bytes or exact numbers, never arrival order.

use lekalo_core::context::{plan, Capsule, CapsuleScope};
use lekalo_core::ir::compile;
use lekalo_core::loader::{normalize_model, LoadSelection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/context/planner";

/// The permissive v1 budget: the recorded bound itself.
const MAX_BUDGET: u64 = lekalo_core::context::version::MAX_BUDGET_TOKENS;

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}
fn compile_fixture(name: &str) -> lekalo_core::ir::Compilation {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let selection = LoadSelection {
        project: Some(name.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("{name}: load failed: {}", outcome.to_json_string()),
    };
    std::env::set_current_dir(original).expect("restore cwd");
    match compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!(
            "{name}: IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn planner() -> lekalo_core::ir::Compilation {
    compile_fixture(FIXTURE)
}

fn symbol_capsule(compilation: &lekalo_core::ir::Compilation, budget: u64) -> Capsule {
    plan(
        &CapsuleScope::Symbol("planner.focus_task".to_owned()),
        budget,
        false,
        compilation,
    )
    .expect("capsule plans")
}

#[test]
fn planning_is_deterministic_byte_for_byte() {
    let compilation = planner();
    let first = symbol_capsule(&compilation, 5_000);
    let second = symbol_capsule(&compilation, 5_000);
    assert_eq!(first.to_canonical_json(), second.to_canonical_json());
    assert_eq!(first.to_markdown(), second.to_markdown());
    // The same compilation loaded twice produces the same bytes.
    let reloaded = planner();
    let third = symbol_capsule(&reloaded, 5_000);
    assert_eq!(first.to_canonical_json(), third.to_canonical_json());
}

#[test]
fn sections_follow_the_protection_order_and_carry_the_expected_facts() {
    let compilation = planner();
    let capsule = symbol_capsule(&compilation, MAX_BUDGET);
    let json = capsule.to_canonical_json();
    let document: serde_json::Value = serde_json::from_str(&json).expect("canonical json");
    let sections = document["sections"].as_object().expect("sections object");
    // The canonical JSON keys are byte-sorted (the #13/#14 discipline);
    // the protection order shows in the Markdown section sequence.
    let rank = |key: &str| {
        [
            "symbol",
            "policies",
            "effects",
            "dependencies",
            "scenarios",
            "public-impact",
            "bindings",
            "types",
            "closure",
        ]
        .iter()
        .position(|candidate| *candidate == key)
        .expect("closed section key")
    };
    let markdown = capsule.to_markdown();
    let mut positions: Vec<(usize, &str)> = Vec::new();
    for key in [
        "symbol",
        "policies",
        "effects",
        "dependencies",
        "scenarios",
        "public-impact",
        "bindings",
        "types",
        "closure",
    ] {
        if let Some(found) = markdown.find(&format!("## {key}\n")) {
            positions.push((found, key));
        }
    }
    let mut ranked = positions.clone();
    ranked.sort_by_key(|(position, key)| (*position, rank(key)));
    assert_eq!(
        positions, ranked,
        "markdown sections appear in protection order"
    );
    assert!(
        positions.len() >= 8,
        "the rich fixture covers most sections"
    );

    // The root card carries the canonical contract.
    let roots = sections["symbol"].as_array().expect("symbol facts");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0]["id"], "operation:planner.focus_task");
    assert_eq!(roots[0]["kind"], "operation");
    assert_eq!(roots[0]["subkind"], "command");
    assert_eq!(roots[0]["description"], "Focus one task");
    assert_eq!(roots[0]["contract"]["input"][0]["name"], "task_id");
    assert_eq!(
        roots[0]["contract"]["input"][0]["type"],
        serde_json::json!({"ref": "planner.task_id"})
    );
    assert_eq!(roots[0]["contract"]["input"][0]["required"], true);

    // The applying policy, with its closed decision.
    let policies = sections["policies"].as_array().expect("policy facts");
    assert_eq!(policies[0]["id"], "policy:planner.deny_bulk_focus");
    assert_eq!(policies[0]["decision"], "deny");
    assert_eq!(policies[0]["appliesTo"][0], "planner.focus_task");

    // The declared effect radius of the root operation.
    let effects = sections["effects"].as_array().expect("effect facts");
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["kind"], "create");
    assert_eq!(effects[0]["operation"], "operation:planner.focus_task");
    assert_eq!(
        effects[0]["resource"],
        serde_json::json!({"id": "planner.task", "kind": "canonical"})
    );
    assert_eq!(effects[1]["kind"], "emit-event");
    assert_eq!(effects[1]["resource"]["id"], "planner.task_focused");

    // Direct dependencies, scenarios, public impact, bindings, types.
    let dependencies = sections["dependencies"]
        .as_array()
        .expect("dependency facts");
    assert!(
        dependencies
            .iter()
            .any(|edge| edge["relation"] == "accepts" && edge["to"] == "type:planner.task_id"),
        "accepts edge expected"
    );
    let scenarios = sections["scenarios"].as_array().expect("scenario facts");
    assert_eq!(scenarios[0]["id"], "scenario:planner.focus_flow");
    assert_eq!(scenarios[0]["covers"][0], "planner.focus_task");
    let impact = sections["public-impact"].as_array().expect("impact facts");
    assert!(
        impact
            .iter()
            .any(|edge| edge["relation"] == "exposes"
                && edge["from"] == "endpoint:planner.api_focus"),
        "exposes edge expected"
    );
    let bindings = sections["bindings"].as_array().expect("binding facts");
    assert_eq!(bindings[0]["id"], "target-binding:planner.binding_node");
    assert_eq!(bindings[0]["target"], "node-typescript");
    let types = sections["types"].as_array().expect("type facts");
    assert!(types
        .iter()
        .any(|card| card["id"] == "type:planner.task_id"));
}

#[test]
fn every_fact_fits_when_the_budget_equals_the_minimum_requirement() {
    let compilation = planner();
    let unbounded = symbol_capsule(&compilation, MAX_BUDGET);
    assert!(unbounded.fits());
    assert_eq!(unbounded.excluded_count(), 0);
    assert_eq!(
        unbounded.estimated_tokens(),
        unbounded.minimum_required_tokens()
    );
    let exact = symbol_capsule(&compilation, unbounded.minimum_required_tokens());
    assert!(exact.fits());
    assert_eq!(exact.estimated_tokens(), exact.minimum_required_tokens());
    // Same facts in, same facts out: the manifest and the emitted
    // sections match the permissive capsule (only the recorded limit
    // differs in the metadata).
    let exact_json = exact.to_canonical_json();
    let unbounded_json = unbounded.to_canonical_json();
    let exact_doc: serde_json::Value = serde_json::from_str(&exact_json).expect("canonical json");
    let unbounded_doc: serde_json::Value =
        serde_json::from_str(&unbounded_json).expect("canonical json");
    assert_eq!(exact_doc["manifest"], unbounded_doc["manifest"]);
    assert_eq!(exact_doc["sections"], unbounded_doc["sections"]);
    assert_eq!(
        exact_doc["budget"]["estimated"],
        serde_json::json!(exact.minimum_required_tokens())
    );
}

#[test]
fn an_exhausted_budget_is_explicit_metadata_never_an_error() {
    let compilation = planner();
    let full = symbol_capsule(&compilation, MAX_BUDGET);
    let tight_budget = full.minimum_required_tokens() - 1;
    let tight = symbol_capsule(&compilation, tight_budget);
    assert!(!tight.fits());
    assert_eq!(tight.candidate_count(), full.candidate_count());
    assert_eq!(
        tight.minimum_required_tokens(),
        full.minimum_required_tokens()
    );
    assert!(tight.estimated_tokens() < tight.minimum_required_tokens());
    assert_eq!(
        tight.included_count() + tight.excluded_count(),
        tight.candidate_count()
    );
    // Every excluded row names the budget, and every included row counts.
    let json = tight.to_canonical_json();
    let document: serde_json::Value = serde_json::from_str(&json).expect("canonical json");
    let excluded = document["manifest"]["excluded"]
        .as_array()
        .expect("excluded");
    assert!(!excluded.is_empty());
    for row in excluded {
        assert_eq!(row["reason"], "budget");
    }
    // The Markdown carries the same truncation metadata.
    let markdown = tight.to_markdown();
    assert!(markdown.contains("fits false"));
    assert!(markdown.contains("## excluded"));
    // The emitted capsule respects the limit exactly.
    assert!(tight.estimated_tokens() <= tight_budget);
}

#[test]
fn zero_and_over_range_budgets_are_fatal_input_violations() {
    let compilation = planner();
    for budget in [0, lekalo_core::context::version::MAX_BUDGET_TOKENS + 1] {
        let outcome = plan(
            &CapsuleScope::Symbol("planner.focus_task".to_owned()),
            budget,
            false,
            &compilation,
        );
        let set = outcome.expect_err("budget must be rejected");
        let rendered = serde_json::to_string(&set).expect("set serializes");
        assert!(rendered.contains("graph.input-invalid"), "{rendered}");
    }
}

#[test]
fn unknown_roots_reuse_the_registered_graph_rule() {
    let compilation = planner();
    let outcome = plan(
        &CapsuleScope::Symbol("no.such_symbol".to_owned()),
        5_000,
        false,
        &compilation,
    );
    let set = outcome.expect_err("unknown root must fail");
    let rendered = serde_json::to_string(&set).expect("set serializes");
    assert!(rendered.contains("graph.unknown-node"), "{rendered}");
    // The hostile echo is bounded and control-clean on the accepted wire.
    let hostile = "9".repeat(8_192);
    let outcome = plan(&CapsuleScope::Symbol(hostile), 5_000, false, &compilation);
    let set = outcome.expect_err("hostile root must fail");
    let rendered = serde_json::to_string(&set).expect("set serializes");
    assert!(rendered.len() < 8_192, "echo must be bounded");
    assert!(!rendered.contains('\u{0}'));
}

#[test]
fn the_changed_scope_covers_every_changed_symbol_exactly_once() {
    let compilation = planner();
    let capsule = plan(
        &CapsuleScope::Changed(vec![
            "planner.focus_task".to_owned(),
            "planner.edit_task_cmd".to_owned(),
            "planner.focus_task".to_owned(),
        ]),
        MAX_BUDGET,
        false,
        &compilation,
    )
    .expect("capsule plans");
    assert_eq!(capsule.mode(), "changed");
    assert_eq!(capsule.roots().len(), 2, "duplicates collapse");
    assert_eq!(capsule.roots()[0], "operation:planner.edit_task_cmd");
    assert_eq!(capsule.roots()[1], "operation:planner.focus_task");
    let document: serde_json::Value =
        serde_json::from_str(&capsule.to_canonical_json()).expect("canonical json");
    let symbols = document["sections"]["symbol"]
        .as_array()
        .expect("symbol facts");
    assert_eq!(symbols.len(), 2);
    let single = plan(
        &CapsuleScope::Symbol("planner.focus_task".to_owned()),
        MAX_BUDGET,
        false,
        &compilation,
    )
    .expect("capsule plans");
    assert_eq!(single.mode(), "symbol");
}

#[test]
fn gaps_are_the_closed_vocabulary_with_standing_confidence_honesty() {
    let compilation = planner();
    let capsule = symbol_capsule(&compilation, MAX_BUDGET);
    let document: serde_json::Value =
        serde_json::from_str(&capsule.to_canonical_json()).expect("canonical json");
    let names: Vec<&str> = document["gaps"]
        .as_array()
        .expect("gaps")
        .iter()
        .map(|gap| gap["gap"].as_str().expect("gap id"))
        .collect();
    // The standing gaps: the accepted Model cannot declare error
    // contracts, and the CLI path attaches no detected evidence.
    assert!(names.contains(&"error-contracts-unrepresentable"));
    assert!(names.contains(&"detected-effects-absent"));
    // A command with no declared effects is named explicitly.
    let silent = plan(
        &CapsuleScope::Symbol("notify.purge_cache_cmd".to_owned()),
        MAX_BUDGET,
        false,
        &compilation,
    )
    .expect("capsule plans");
    let document: serde_json::Value =
        serde_json::from_str(&silent.to_canonical_json()).expect("canonical json");
    let gaps = document["gaps"].as_array().expect("gaps");
    let names: Vec<&str> = gaps
        .iter()
        .map(|gap| gap["gap"].as_str().expect("gap id"))
        .collect();
    assert!(names.contains(&"no-effects"));
    let no_effects = gaps
        .iter()
        .find(|gap| gap["gap"] == "no-effects")
        .expect("no-effects row");
    assert_eq!(
        no_effects["symbols"],
        serde_json::json!(["operation:notify.purge_cache_cmd"])
    );
}

#[test]
fn the_estimator_pins_its_identity_version_and_digest() {
    let compilation = planner();
    let capsule = symbol_capsule(&compilation, MAX_BUDGET);
    let document: serde_json::Value =
        serde_json::from_str(&capsule.to_canonical_json()).expect("canonical json");
    let estimator = &document["estimator"];
    assert_eq!(estimator["identity"], "dev.lekalo.estimator.chars-4@1.0.0");
    assert_eq!(estimator["version"], "1.0.0");
    assert_eq!(
        estimator["digest"].as_str().expect("digest").len(),
        "sha256:".len() + 64
    );
    // Every manifest row carries a positive token estimate.
    for row in document["manifest"]["included"].as_array().expect("rows") {
        assert!(row["tokens"].as_u64().expect("tokens") >= 1);
    }
}

#[test]
fn spans_are_an_opt_in_logical_path_sidecar_only() {
    let compilation = planner();
    let default_capsule = symbol_capsule(&compilation, MAX_BUDGET);
    assert!(!default_capsule.to_canonical_json().contains("\"spans\""));
    let capsule = plan(
        &CapsuleScope::Symbol("planner.focus_task".to_owned()),
        MAX_BUDGET,
        true,
        &compilation,
    )
    .expect("capsule plans");
    let json = capsule.to_canonical_json();
    let document: serde_json::Value = serde_json::from_str(&json).expect("canonical json");
    let spans = document["spans"].as_array().expect("spans sidecar");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0]["id"], "operation:planner.focus_task");
    let path = spans[0]["span"]["path"].as_str().expect("logical path");
    assert!(path.starts_with("lekalo/"), "{path}");
    assert!(!path.contains('\\'), "logical paths only: {path}");
    assert!(!path.contains(".env"));
}

#[test]
fn capsule_bytes_never_carry_private_or_absolute_data() {
    let compilation = planner();
    let capsule = plan(
        &CapsuleScope::Changed(vec![
            "planner.focus_task".to_owned(),
            "planner.count_focused".to_owned(),
        ]),
        MAX_BUDGET,
        true,
        &compilation,
    )
    .expect("capsule plans");
    for rendered in [capsule.to_canonical_json(), capsule.to_markdown()] {
        // No Windows drive spellings, no UNC prefixes, no .env references,
        // no NUL controls.
        assert!(!rendered.contains("C:\\") && !rendered.contains("C:/"));
        assert!(!rendered.contains(r"\\?\"));
        assert!(!rendered.contains(".env"));
        assert!(!rendered.contains('\u{0}'));
    }
}

#[test]
fn the_capsule_is_far_smaller_than_the_whole_repository_scan() {
    // The benchmark proxy required by the issue: the capsule's estimated
    // token count against the token-equivalent of the whole canonical
    // graph export (the deterministic whole-repository projection).
    let compilation = planner();
    let graph = lekalo_core::graph::build(&compilation.project).expect("graph builds");
    let export = graph.to_canonical_json().expect("export renders");
    let scan_tokens = lekalo_core::context::estimate::tokens(&export);
    let capsule = symbol_capsule(&compilation, MAX_BUDGET);
    let capsule_tokens = capsule.estimated_tokens();
    assert!(
        capsule_tokens * 4 < scan_tokens,
        "capsule {capsule_tokens} tokens must be far below the {scan_tokens}-token repository scan"
    );
}
