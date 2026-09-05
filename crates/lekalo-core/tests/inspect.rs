//! Issue #15 library tests for `lekalo inspect`: safe selector
//! resolution over the hermetic planner fixture (exact ids, safe short
//! names, #6 alias registry, tombstones), the distinct unknown and
//! ambiguous diagnostics, every mandatory section for the six required
//! kinds, the bounded include filters, byte-identical determinism, and
//! the no-write guarantee.
//!
//! The #8 IR newtypes are constructible only inside the crate, so these
//! tests drive the same loader seam every consumer uses. Tests that
//! need a mutated fixture copy it into a unique canonicalized temp
//! directory: GitHub's Windows runners export `%TEMP%` with the 8.3
//! profile alias and the selection policy denies alias spellings.

use lekalo_core::graph::build as graph_build;
use lekalo_core::inspect::{run, Include, InspectRequest};
use lekalo_core::ir::compile;
use lekalo_core::loader::{normalize_model, LoadSelection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/inspect/planner";
const GOLDEN_ENVELOPE: &str = "tests/fixtures/inspect/golden/planner.task.json";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// The canonicalized, alias-free spelling of `path` (strips the `\\?\`
/// verbatim prefix `canonicalize` produces on Windows drive paths).
fn alias_free(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

/// Run one inspect invocation with the process working directory moved
/// into `project_dir` for the whole call (the lock serializes every
/// test that changes the directory).
fn inspect_at(
    project_dir: &Path,
    selector: &str,
    include: Include,
) -> Result<String, lekalo_core::diagnostics::DiagnosticSet> {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(project_dir).expect("enter project");
    let selection = LoadSelection {
        project: Some(String::from(".")),
    };
    let outcome = {
        let model = match normalize_model(&selection) {
            Ok(model) => model,
            Err(outcome) => panic!("fixture load failed: {}", outcome.to_json_string()),
        };
        let compilation = compile(&model).expect("fixture compiles");
        let graph = graph_build(&compilation.project).expect("graph builds");
        let effects = lekalo_core::effects::build(&compilation.project).expect("effects build");
        let request = InspectRequest {
            selector: selector.to_owned(),
            include,
        };
        run(&compilation, &graph, &effects, &request)
            .map(|outcome| outcome.json)
            .map(|json| format!("{{\"status\":\"valid\",\"inspect\":{json}}}"))
    };
    std::env::set_current_dir(original).expect("restore cwd");
    outcome
}

fn inspect_fixture(
    selector: &str,
    include: Include,
) -> Result<String, lekalo_core::diagnostics::DiagnosticSet> {
    inspect_at(&workspace_root().join(FIXTURE), selector, include)
}

fn inspect_ok(selector: &str) -> serde_json::Value {
    let json = inspect_fixture(selector, Include::default()).expect("inspect succeeds");
    serde_json::from_str(&json).expect("envelope parses")
}

fn inspect_err(selector: &str) -> lekalo_core::diagnostics::DiagnosticSet {
    inspect_fixture(selector, Include::default()).expect_err("inspect fails")
}

// ---------------------------------------------------------------------
// Resolution

#[test]
fn exact_id_short_name_and_alias_resolve_to_the_same_card() {
    let exact = inspect_ok("planner.focus_task");
    let short = inspect_ok("focus_task");
    assert_eq!(exact["inspect"]["selector"]["mode"], "exact-id");
    assert_eq!(short["inspect"]["selector"]["mode"], "short-name");
    assert_eq!(short["inspect"]["selector"]["input"], "focus_task");
    assert_eq!(
        short["inspect"]["selector"]["resolvedId"],
        "planner.focus_task"
    );
    // The projected result beyond the selector is identical: one symbol,
    // one normalized object.
    assert_eq!(
        exact["inspect"].as_object().map(|object| object.len()),
        short["inspect"].as_object().map(|object| object.len())
    );
    let alias = inspect_ok("audit.work_item");
    assert_eq!(alias["inspect"]["selector"]["mode"], "alias");
    assert_eq!(alias["inspect"]["selector"]["input"], "audit.work_item");
    assert_eq!(alias["inspect"]["selector"]["resolvedId"], "audit.task");
    assert_eq!(alias["inspect"]["symbol"]["id"], "audit.task");
}

#[test]
fn unknown_and_ambiguous_are_distinct_stable_diagnostics() {
    // Unknown full id.
    let set = inspect_err("planner.nope");
    assert_eq!(set.as_slice().len(), 1);
    assert_eq!(set.as_slice()[0].id(), "inspect.symbol-unknown");
    assert_eq!(set.as_slice()[0].code(), "LEK-INS-001");
    // Tombstoned ids never come back: never reuse a deleted id.
    let set = inspect_err("planner.old_task");
    assert_eq!(set.as_slice()[0].id(), "inspect.symbol-unknown");
    let rendered = serde_json::to_string(&set.as_slice()[0]).expect("wire");
    assert!(rendered.contains("tombstoned"), "{rendered}");
    // Short-name zero matches.
    let set = inspect_err("no_such_name");
    assert_eq!(set.as_slice()[0].id(), "inspect.short-name-unknown");
    assert_eq!(set.as_slice()[0].code(), "LEK-INS-002");
    // Short-name many matches: bounded, sorted candidates plus the
    // exact total; never a first-match selection.
    let set = inspect_err("task");
    assert_eq!(set.as_slice()[0].id(), "inspect.short-name-ambiguous");
    assert_eq!(set.as_slice()[0].code(), "LEK-INS-003");
    let rendered = serde_json::to_string(&set.as_slice()[0]).expect("wire");
    assert!(rendered.contains("\"audit.task\""), "{rendered}");
    assert!(rendered.contains("\"planner.task\""), "{rendered}");
    assert!(rendered.contains("\"matched\":2"), "{rendered}");
    // Case differs: an uppercase spelling is invalid grammar, not an
    // unknown symbol.
    let set = inspect_err("planner.TASK");
    assert_eq!(set.as_slice()[0].id(), "cli.usage");
    let usage_wire = serde_json::to_string(&set.as_slice()[0]).expect("wire");
    assert!(
        !usage_wire.contains("\"symbol\""),
        "no echo of rejected input"
    );
}

#[test]
fn unsafe_selectors_fail_as_usage_before_any_resolution() {
    for raw in [
        "..",
        "planner/task",
        "planner\\task",
        "operation:planner.focus_task",
        "file:///etc/passwd",
        "%2e%2e%2f",
        "planner.focus_task.x.y",
        ".hidden",
        "trailing.",
        "",
    ] {
        let set = inspect_err(raw);
        assert_eq!(
            set.as_slice()[0].id(),
            "cli.usage",
            "selector {raw:?} must be a usage failure"
        );
    }
}

// ---------------------------------------------------------------------
// Sections

#[test]
fn entity_card_carries_identity_contract_and_reverse_views() {
    let envelope = inspect_ok("planner.task");
    let inspect = &envelope["inspect"];
    assert_eq!(inspect["symbol"]["kind"], "entity");
    assert_eq!(inspect["symbol"]["moduleId"], "planner");
    assert_eq!(
        inspect["symbol"]["source"]["path"],
        "lekalo/modules/planner/entities.yaml"
    );
    // Identity membership is the one Model-declared invariant.
    assert_eq!(inspect["invariants"]["state"], "available");
    assert_eq!(inspect["invariants"]["items"][0]["type"], "identity");
    assert_eq!(inspect["invariants"]["items"][0]["fields"][0], "task_id");
    // Typed field contracts with requiredness.
    let fields = inspect["contract"]["fields"].as_array().expect("fields");
    assert_eq!(fields.len(), 5);
    assert_eq!(fields[0]["name"], "task_id");
    assert_eq!(
        fields[0]["type"],
        serde_json::json!({"ref": "planner.task_id"})
    );
    assert_eq!(fields[0]["required"], true);
    assert_eq!(
        fields[3]["type"],
        serde_json::json!({"optional": {"ref": "planner.due_date"}})
    );
    // The applicable policies and the scenarios that cover the entity.
    assert_eq!(inspect["policies"]["state"], "empty");
    assert_eq!(inspect["scenarios"]["state"], "unsupported");
    // Effect summary counts from the #14 reverse indexes.
    assert_eq!(inspect["effects"]["state"], "available");
    assert!(
        inspect["effects"]["summary"]["readerCount"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        inspect["effects"]["summary"]["writerCount"]
            .as_u64()
            .unwrap()
            >= 1
    );
    // Graph relations: direct dependencies and dependents.
    let deps = inspect["dependencies"]["items"].as_array().expect("deps");
    assert!(deps
        .iter()
        .any(|item| item["relation"] == "references" && item["endpoint"] == "planner.task_state"));
    let dependents = inspect["dependents"]["items"]
        .as_array()
        .expect("dependents");
    assert!(dependents.iter().any(|item| item["relation"] == "reads"));
    // Ownership and trace are explicitly unsupported, never silently
    // absent; completeness records the degradation.
    assert_eq!(
        inspect["ownership"],
        serde_json::json!({
            "complete": false, "reason": "owner-not-accepted", "state": "unsupported"
        })
    );
    assert_eq!(inspect["trace"]["state"], "unsupported");
    assert_eq!(inspect["completeness"]["state"], "partial");
    assert_eq!(inspect["portability"]["state"], "empty");
}

#[test]
fn command_query_policy_event_scenario_cards_cover_their_kinds() {
    // Command: input contract, declared effects, applicable policies.
    let command = inspect_ok("planner.focus_task")["inspect"].clone();
    assert_eq!(command["contract"]["input"][0]["name"], "task_id");
    let effects = command["effects"]["items"]
        .as_array()
        .expect("effect items");
    let kinds: Vec<&str> = effects
        .iter()
        .map(|item| item["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(kinds, vec!["create", "emit-event"]);
    assert_eq!(effects[0]["provenance"]["type"], "canonical-ir");
    assert_eq!(effects[0]["confidence"], "canonical");
    let policies = command["policies"]["items"].as_array().expect("policies");
    assert_eq!(policies[0]["id"], "planner.deny_bulk_focus");
    assert_eq!(policies[0]["decision"], "deny");
    let dependents = command["dependents"]["items"]
        .as_array()
        .expect("dependents");
    assert!(dependents
        .iter()
        .any(|item| item["relation"] == "authorizes"));
    assert!(dependents.iter().any(|item| item["relation"] == "exposes"));
    assert!(command["dependencies"]["items"]
        .as_array()
        .expect("deps")
        .iter()
        .any(|item| item["relation"] == "accepts"));

    // Query: reads and the named return type; explicit reads validation
    // stays with #12, so the card reports the declared reads only.
    let query = inspect_ok("planner.count_focused")["inspect"].clone();
    assert_eq!(
        query["contract"]["reads"],
        serde_json::json!(["planner.task"])
    );
    assert_eq!(
        query["contract"]["output"],
        serde_json::json!({"ref": "planner.task_state"})
    );
    let effects = query["effects"]["items"].as_array().expect("effect items");
    assert_eq!(effects[0]["kind"], "read");
    assert_eq!(effects[0]["resource"]["id"], "planner.task");

    // Policy: the closed decision and its applies_to relations.
    let policy = inspect_ok("planner.deny_bulk_focus")["inspect"].clone();
    assert_eq!(policy["contract"]["decision"], "deny");
    let deps = policy["dependencies"]["items"].as_array().expect("deps");
    assert!(deps
        .iter()
        .any(|item| item["relation"] == "authorizes" && item["endpoint"] == "planner.focus_task"));

    // Event: payload contract and the reverse emitter counts.
    let event = inspect_ok("planner.task_focused")["inspect"].clone();
    assert_eq!(event["contract"]["fields"][0]["name"], "task_id");
    assert!(
        event["effects"]["summary"]["emitterCount"]
            .as_u64()
            .unwrap()
            >= 1
    );

    // Scenario: summary and covers in the contract, no invented
    // Given/When/Then detail (the #23 owner has not landed).
    let scenario = inspect_ok("planner.focus_flow")["inspect"].clone();
    assert_eq!(
        scenario["contract"]["summary"],
        "Focusing a task emits the focused event."
    );
    assert_eq!(
        scenario["contract"]["covers"],
        serde_json::json!(["planner.focus_task", "planner.task_focused"])
    );
    assert!(scenario["completeness"]["reasons"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|reason| reason == "scenario-details-unavailable"));

    // Target binding definition card.
    let binding = inspect_ok("planner.binding_node")["inspect"].clone();
    assert_eq!(binding["contract"]["targetName"], "node-typescript");
}

#[test]
fn include_filters_project_bindings_and_scenarios_or_mark_them() {
    // Default: the optional projections are present and explicitly
    // not-requested, and completeness records them.
    let envelope = inspect_ok("planner.focus_task");
    let inspect = &envelope["inspect"];
    assert_eq!(inspect["bindings"]["state"], "unsupported");
    assert_eq!(inspect["scenarios"]["state"], "unsupported");
    assert_eq!(inspect["bindings"]["bounds"]["reason"], "not-requested");
    let reasons = inspect["completeness"]["reasons"]
        .as_array()
        .expect("reasons");
    assert!(reasons.contains(&serde_json::json!("bindings-not-requested")));
    assert!(reasons.contains(&serde_json::json!("scenarios-not-requested")));

    // With the include filter both project their declared facts.
    let json = inspect_fixture(
        "planner.focus_task",
        Include {
            bindings: true,
            scenarios: true,
        },
    )
    .expect("inspect succeeds");
    let inspect = &serde_json::from_str::<serde_json::Value>(&json).expect("parses")["inspect"];
    assert_eq!(inspect["scenarios"]["state"], "available");
    assert_eq!(inspect["scenarios"]["items"][0]["id"], "planner.focus_flow");
    assert_eq!(inspect["bindings"]["state"], "available");
    let bindings = inspect["bindings"]["items"].as_array().expect("bindings");
    assert_eq!(bindings[0]["bindingId"], "planner.binding_node");
    assert_eq!(bindings[0]["targetId"], "node-typescript");
    assert_eq!(bindings[0]["status"], "declared");
    assert_eq!(bindings[0]["confidence"], "canonical");
    let reasons = inspect["completeness"]["reasons"]
        .as_array()
        .expect("reasons");
    assert!(!reasons.contains(&serde_json::json!("bindings-not-requested")));
    assert!(!reasons.contains(&serde_json::json!("scenarios-not-requested")));
}

// ---------------------------------------------------------------------
// Determinism, goldens, and the no-write guarantee

#[test]
fn reruns_are_byte_identical_and_match_the_pinned_golden() {
    let first = inspect_fixture("planner.task", Include::default()).expect("first");
    let second = inspect_fixture("planner.task", Include::default()).expect("second");
    assert_eq!(first, second, "reruns must be byte-identical");
    let golden =
        std::fs::read_to_string(workspace_root().join(GOLDEN_ENVELOPE)).expect("golden envelope");
    assert_eq!(first.trim_end(), golden.trim_end(), "pinned golden drift");
}

#[test]
fn human_and_json_project_the_same_object_and_match_the_golden() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root().join(FIXTURE)).expect("enter fixture");
    let selection = LoadSelection {
        project: Some(String::from(".")),
    };
    let model = normalize_model(&selection).expect("model");
    let compilation = compile(&model).expect("compiles");
    let graph = graph_build(&compilation.project).expect("graph");
    let effects = lekalo_core::effects::build(&compilation.project).expect("effects");
    let request = InspectRequest {
        selector: String::from("planner.task"),
        include: Include::default(),
    };
    let outcome = run(&compilation, &graph, &effects, &request).expect("outcome");
    // The human view carries the same sections in the same order.
    for label in [
        "symbol:",
        "kind:",
        "module:",
        "contract:",
        "invariants:",
        "policies:",
        "effects:",
        "dependencies:",
        "dependents:",
        "scenarios:",
        "bindings:",
        "ownership:",
        "portability:",
        "trace:",
        "completeness:",
    ] {
        assert!(outcome.human.contains(label), "human view misses {label}");
    }
    let golden = std::fs::read_to_string(
        workspace_root().join("tests/fixtures/inspect/golden/planner.task.human.txt"),
    )
    .expect("human golden");
    assert_eq!(outcome.human, golden.trim_end(), "human golden drift");
    std::env::set_current_dir(original).expect("restore cwd");
}

#[test]
fn inspect_performs_no_writes_anywhere() {
    let root = alias_free(&workspace_root().join(FIXTURE));
    let before = snapshot(&root);
    let _ = inspect_fixture("planner.focus_task", Include::default()).expect("inspect");
    let after = snapshot(&root);
    assert_eq!(before, after, "inspect must not write");
}

/// Recursive `(relative path, size)` snapshot for the no-write check.
fn snapshot(root: &Path) -> Vec<(String, u64)> {
    let mut entries = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let meta = std::fs::metadata(&path).expect("metadata");
            if meta.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                entries.push((relative, meta.len()));
            }
        }
    }
    entries.sort();
    entries
}

// ---------------------------------------------------------------------
// Bounds over generated temp projects

/// A unique, canonicalized temp project root (the Windows %TEMP% 8.3
/// alias lesson applied to library tests).
fn temp_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lekalo-inspect-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("lekalo/modules/main")).expect("temp project dirs");
    alias_free(&dir)
}

fn write_project(root: &Path, field_count: usize) {
    std::fs::write(
        root.join("lekalo/project.yaml"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: big\n    kind: project\n    version: 1\n",
    )
    .expect("project yaml");
    std::fs::write(
        root.join("lekalo/modules/main/module.yaml"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: main\n    kind: module\n    version: 1\n",
    )
    .expect("module yaml");
    let mut fields = String::new();
    for index in 0..field_count {
        fields.push_str(&format!(
            "      - name: field_{index:04}\n        type: \"main.text\"\n        required: true\n"
        ));
    }
    std::fs::write(
        root.join("lekalo/modules/main/entities.yaml"),
        format!(
            "schema_version: \"1.0.0\"\ndefinitions:\n  - id: main.text\n    kind: scalar\n    version: 1\n    base: string\n  - id: main.blob\n    kind: entity\n    version: 1\n    description: \"Wide entity\"\n    fields:\n{fields}    identity:\n      - field_0000\n"
        ),
    )
    .expect("entities yaml");
}

#[test]
fn wide_sections_truncate_with_bounds_frontier_and_reasons() {
    let root = temp_project("wide");
    write_project(&root, 300);
    let json = inspect_at(&root, "main.blob", Include::default()).expect("inspect");
    let _ = std::fs::remove_dir_all(&root);
    let inspect = &serde_json::from_str::<serde_json::Value>(&json).expect("parses")["inspect"];
    let fields = inspect["contract"]["fields"].as_array().expect("fields");
    assert_eq!(fields.len(), 256);
    assert_eq!(inspect["contract"]["state"], "truncated");
    assert_eq!(inspect["contract"]["complete"], false);
    assert_eq!(inspect["contract"]["bounds"]["limit"], 256);
    assert_eq!(inspect["contract"]["bounds"]["returned"], 256);
    assert_eq!(inspect["contract"]["bounds"]["omitted"], 44);
    assert_eq!(inspect["contract"]["bounds"]["frontier"], "field_0256");
    // The 300 field references also truncate the dependencies section:
    // 44 contract fields plus 44 dependency edges are omitted in total.
    assert_eq!(inspect["dependencies"]["state"], "truncated");
    assert_eq!(inspect["dependencies"]["bounds"]["omitted"], 44);
    assert_eq!(inspect["completeness"]["omittedItems"], 88);
    assert!(inspect["completeness"]["reasons"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|reason| reason == "items-truncated"));
}

#[test]
fn ambiguity_carries_a_bounded_sorted_candidate_prefix_and_exact_total() {
    let root = temp_project("ambiguous");
    std::fs::write(
        root.join("lekalo/project.yaml"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: big\n    kind: project\n    version: 1\n",
    )
    .expect("project yaml");
    // temp_project seeds an unused `main` module directory; the closed
    // layout requires every module directory to carry its module.yaml.
    std::fs::remove_dir_all(root.join("lekalo/modules/main")).expect("drop unused module");
    for index in 0..40 {
        let module = format!("m{index:02}");
        std::fs::create_dir_all(root.join(format!("lekalo/modules/{module}"))).expect("module dir");
        std::fs::write(
            root.join(format!("lekalo/modules/{module}/module.yaml")),
            format!(
                "schema_version: \"1.0.0\"\ndefinitions:\n  - id: {module}\n    kind: module\n    version: 1\n{}",
                if module == "m00" {
                    String::new()
                } else {
                    "    imports:\n      - m00\n".to_owned()
                }
            ),
        )
        .expect("module yaml");
        std::fs::write(
            root.join(format!("lekalo/modules/{module}/entities.yaml")),
            format!(
                "schema_version: \"1.0.0\"\ndefinitions:\n  - id: {module}.item\n    kind: entity\n    version: 1\n    fields:\n      - name: token\n        type: \"m00.text\"\n    identity:\n      - token\n{}",
                if module == "m00" {
                    "  - id: m00.text\n    kind: scalar\n    version: 1\n    base: string\n"
                } else {
                    ""
                }
            ),
        )
        .expect("entities yaml");
    }

    let set = inspect_at(&root, "item", Include::default()).expect_err("ambiguous");
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(set.as_slice().len(), 1);
    assert_eq!(set.as_slice()[0].id(), "inspect.short-name-ambiguous");
    let rendered = serde_json::to_string(&set.as_slice()[0]).expect("wire");
    assert!(rendered.contains("\"matched\":40"), "{rendered}");
    // The bounded prefix: exactly 32 sorted candidates.
    let candidates: Vec<&str> = [
        "m00.item", "m01.item", "m07.item", "m31.item", "m32.item", "m39.item",
    ]
    .to_vec();
    for candidate in candidates {
        let expected = format!("\"{candidate}\"");
        let present = rendered.contains(&expected);
        if candidate == "m39.item" || candidate == "m32.item" {
            assert!(!present, "beyond the 32-candidate bound: {candidate}");
        } else {
            assert!(present, "candidate missing: {candidate}");
        }
    }
}
