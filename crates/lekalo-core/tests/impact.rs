//! Issue #16 library tests for the impact analyzer: the direct and
//! transitive radius over the hermetic planner fixture, the
//! mandatory-public closure that depth limits can never hide, the closed
//! risk vector, rename and field-removal regression cases, the typed
//! changed-input handoff, strict-profile denial, and canonical digest
//! stability.
//!
//! The #8 IR newtypes are constructible only inside the crate, so these
//! integration tests drive the same loader seam every consumer uses.

use lekalo_core::effects::build as build_effects;
use lekalo_core::graph::build as build_graph;
use lekalo_core::impact::{
    analyze, ChangedInput, ChangedInputSet, EntryEvidence, FileChange, ImpactFailure,
    ImpactProfile, ImpactRequest, MemberChange, MemberSeed, Scope,
};
use lekalo_core::loader::{normalize_model, LoadSelection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PLANNER: &str = "tests/fixtures/impact/planner";
const RENAME: &str = "tests/fixtures/impact/rename";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn load(
    fixture: &str,
) -> (
    lekalo_core::ir::CompiledProject,
    lekalo_core::graph::DependencyGraph,
    lekalo_core::effects::EffectGraph,
) {
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let selection = LoadSelection {
        project: Some(fixture.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("{fixture}: load failed: {}", outcome.to_json_string()),
    };
    std::env::set_current_dir(original).expect("restore cwd");
    let compilation = match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!(
            "{fixture}: IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    let graph = build_graph(&compilation.project).expect("graph builds");
    let effects = build_effects(&compilation.project).expect("effects build");
    (compilation.project, graph, effects)
}

fn planner() -> (
    lekalo_core::ir::CompiledProject,
    lekalo_core::graph::DependencyGraph,
    lekalo_core::effects::EffectGraph,
) {
    load(PLANNER)
}

fn rename() -> (
    lekalo_core::ir::CompiledProject,
    lekalo_core::graph::DependencyGraph,
    lekalo_core::effects::EffectGraph,
) {
    load(RENAME)
}

fn symbol_request(root: &str) -> ImpactRequest {
    ImpactRequest::for_symbol(root).expect("valid root")
}

fn subjects(section: &lekalo_core::impact::ImpactSection) -> Vec<String> {
    section
        .items
        .iter()
        .map(|item| item.subject.as_str().to_owned())
        .collect()
}

fn changed_entry(symbol: &str, path: &str) -> ChangedInput {
    ChangedInput::new(
        vec![symbol.to_owned()],
        Vec::new(),
        Some((path, None)),
        FileChange::Modified,
        EntryEvidence::Canonical,
    )
    .expect("valid entry")
}

#[test]
fn entity_radius_covers_effects_queries_scenarios_and_bindings() {
    let (project, graph, effects) = planner();
    let request = symbol_request("planner.task");
    let result = analyze(&project, &graph, &effects, &request, None).expect("impact");

    // Roots: the entity itself.
    assert_eq!(result.roots().len(), 1);
    assert_eq!(result.roots()[0].as_str(), "entity:planner.task");

    // Direct dependents: the effect definitions and queries that
    // reference or read the entity.
    let direct = subjects(result.direct());
    assert!(direct.contains(&"effect:planner.create_task".to_owned()));
    assert!(direct.contains(&"operation:planner.count_focused".to_owned()));

    // Transitive: commands, endpoint, scenario reach through the direct
    // layer.
    let transitive = subjects(result.transitive());
    assert!(transitive.contains(&"operation:planner.focus_task".to_owned()));
    assert!(transitive.contains(&"endpoint:planner.api_focus".to_owned()));
    assert!(transitive.contains(&"scenario:planner.focus_flow".to_owned()));

    // The module-scope binding of the planner module is projected.
    let targets: Vec<String> = result
        .targets()
        .items
        .iter()
        .map(|item| item.binding.as_str().to_owned())
        .collect();
    assert!(targets.contains(&"target-binding:planner.binding_node".to_owned()));

    // The event emission of create_task surfaces as an effect fact.
    let effects_risk = result
        .risks()
        .items
        .iter()
        .find(|risk| risk.dimension.key() == "effects")
        .expect("effects risk fires");
    assert!(effects_risk
        .subject_refs
        .iter()
        .any(|subject| subject.as_str() == "effect:planner.create_task"));

    // Every item explains itself with a path and at least one reason.
    for section in [
        result.direct(),
        result.transitive(),
        result.mandatory_public(),
    ] {
        for item in &section.items {
            assert!(
                !item.path_refs.is_empty(),
                "item {} has a path",
                item.subject
            );
            assert!(
                !item.reason_refs.is_empty(),
                "item {} has a reason",
                item.subject
            );
        }
    }
    // Missing detected-effect evidence degrades completeness explicitly:
    // the result is never claimed complete on unavailable evidence.
    assert!(!result.completeness().complete);
    assert_eq!(result.completeness().state.key(), "incomplete");
    assert!(result
        .completeness()
        .reason_refs
        .contains(&"impact.evidence-unknown".to_owned()));
    assert!(result
        .diagnostic_refs()
        .contains(&"impact.evidence-unknown".to_owned()));
}

#[test]
fn mandatory_public_closure_is_not_hidden_by_depth() {
    let (project, graph, effects) = planner();
    let request = symbol_request("planner.task")
        .with_depth(1)
        .expect("depth 1");
    let result = analyze(&project, &graph, &effects, &request, None).expect("impact");

    // With depth 1 the transitive section stays empty.
    assert_eq!(result.transitive().items.len(), 0);
    assert_eq!(result.transitive().summary.state.key(), "complete");

    // The public radius still reaches the endpoint, scenario, and policy
    // beyond the requested depth.
    let public = subjects(result.mandatory_public());
    assert!(public.contains(&"endpoint:planner.api_focus".to_owned()));
    assert!(public.contains(&"scenario:planner.focus_flow".to_owned()));
    for item in &result.mandatory_public().items {
        assert_eq!(item.scope, Scope::MandatoryPublic);
    }
    // The main sections stay complete at their depth; the public closure
    // is complete because the fixture graph is small.
    assert!(result.mandatory_public().summary.complete);
}

#[test]
fn unknown_symbol_is_an_explicit_invalid_result() {
    let (project, graph, effects) = planner();
    let request = ImpactRequest::for_symbol("planner.nonexistent").expect("grammar ok");
    let outcome = analyze(&project, &graph, &effects, &request, None);
    match outcome {
        Err(ImpactFailure::Invalid(set)) => {
            assert_eq!(set.reason_ids(), vec!["impact.symbol-unknown"]);
        }
        other => panic!("expected invalid failure, got {other:?}"),
    }
}

#[test]
fn rename_history_surfaces_migration_and_public_risks() {
    let (project, graph, effects) = rename();
    let request = symbol_request("planner.task");
    let result = analyze(&project, &graph, &effects, &request, None).expect("impact");

    let migration = result
        .risks()
        .items
        .iter()
        .find(|risk| risk.dimension.key() == "migration_data")
        .expect("migration risk fires for renamed entity");
    assert!(migration
        .reason_refs
        .contains(&"impact.reason.rename-history".to_owned()));
    assert!(migration.required);
    let public = result
        .risks()
        .items
        .iter()
        .find(|risk| risk.dimension.key() == "public_contract")
        .expect("public risk fires");
    assert!(public.required);
}

#[test]
fn field_removal_member_seed_drives_migration_risk() {
    let (project, graph, effects) = planner();
    let seed = MemberSeed::new("planner.task", "due", MemberChange::Removed).expect("valid seed");
    let entry = ChangedInput::new(
        vec!["planner.task".to_owned()],
        vec![seed],
        Some(("lekalo/modules/planner/entities.yaml", None)),
        FileChange::Modified,
        EntryEvidence::Canonical,
    )
    .expect("valid entry");
    let set = ChangedInputSet::from_entries(
        lekalo_core::impact::ChangedMode::Worktree,
        None,
        None,
        vec![entry],
    )
    .expect("valid set");
    let request = symbol_request("planner.task");
    let result = analyze(&project, &graph, &effects, &request, Some(&set)).expect("impact");
    let migration = result
        .risks()
        .items
        .iter()
        .find(|risk| risk.dimension.key() == "migration_data")
        .expect("field removal fires migration risk");
    assert!(migration
        .reason_refs
        .contains(&"impact.reason.field-removal".to_owned()));
    assert_eq!(result.input_mode().key(), "worktree");
    assert_eq!(result.changed_input_digest(), set.digest());
}

#[test]
fn unresolvable_changed_entries_degrade_completeness() {
    let (project, graph, effects) = planner();
    let unresolvable = ChangedInput::new(
        Vec::new(),
        Vec::new(),
        Some(("NOTES.md", None)),
        FileChange::Added,
        EntryEvidence::Canonical,
    )
    .expect("entry with no symbols carries unknown evidence");
    let resolved = changed_entry("planner.task", "lekalo/modules/planner/entities.yaml");
    let set = ChangedInputSet::from_entries(
        lekalo_core::impact::ChangedMode::Worktree,
        Some("a".repeat(40)),
        None,
        vec![unresolvable, resolved],
    )
    .expect("valid set");
    let request = symbol_request("planner.task");
    let result = analyze(&project, &graph, &effects, &request, Some(&set)).expect("impact");

    assert_eq!(result.base_revision_ref(), Some("a".repeat(40).as_str()));
    assert!(!result.completeness().complete);
    assert_eq!(result.completeness().state.key(), "incomplete");
    let ids: Vec<String> = result
        .warnings()
        .iter()
        .map(|warning| warning.id().to_owned())
        .collect();
    assert!(ids.contains(&"impact.changed-input-incomplete".to_owned()));
    assert!(result
        .diagnostic_refs()
        .contains(&"impact.changed-input-incomplete".to_owned()));
}

#[test]
fn effect_change_traverses_the_effect_radius() {
    let (project, graph, effects) = planner();
    let request = symbol_request("planner.create_task");
    let result = analyze(&project, &graph, &effects, &request, None).expect("impact");

    // The command that owns the effect and the endpoint above it are in
    // the radius.
    let direct = subjects(result.direct());
    assert!(direct.contains(&"operation:planner.focus_task".to_owned()));
    let transitive = subjects(result.transitive());
    assert!(transitive.contains(&"endpoint:planner.api_focus".to_owned()));

    // The create/write and the emitted event surface as effect facts.
    let effects_risk = result
        .risks()
        .items
        .iter()
        .find(|risk| risk.dimension.key() == "effects")
        .expect("effects risk fires");
    assert!(effects_risk
        .reason_refs
        .contains(&"impact.reason.effect-write".to_owned()));
    assert!(effects_risk
        .reason_refs
        .contains(&"impact.reason.event-emission".to_owned()));
}

#[test]
fn strict_profile_denies_when_required_authorization_evidence_is_unknown() {
    let (project, graph, effects) = rename();
    // The rename fixture's command has no policy mapping: authorization
    // evidence is unknown, so strict must deny and default must not.
    let request = symbol_request("planner.focus_task").with_profile(ImpactProfile::Strict);
    let outcome = analyze(&project, &graph, &effects, &request, None);
    match outcome {
        Err(ImpactFailure::Denied(set)) => {
            assert_eq!(set.reason_ids(), vec!["impact.gate-blocked"]);
        }
        other => panic!("expected denied failure, got {other:?}"),
    }

    let (project, graph, effects) = rename();
    let request = symbol_request("planner.focus_task").with_profile(ImpactProfile::Default);
    let result = analyze(&project, &graph, &effects, &request, None).expect("default profile");
    let authorization = result
        .gates()
        .items
        .iter()
        .find(|gate| gate.gate_id == "impact.gate.authorization")
        .expect("authorization gate listed");
    assert_eq!(authorization.state.key(), "unknown");
}

#[test]
fn canonical_bytes_and_digest_are_stable_and_repeatable() {
    let (project, graph, effects) = planner();
    let request = symbol_request("planner.task");
    let first = analyze(&project, &graph, &effects, &request, None).expect("impact");
    let second = analyze(&project, &graph, &effects, &request, None).expect("impact");

    let bytes_first = first.to_canonical_json().expect("canonical bytes");
    let bytes_second = second.to_canonical_json().expect("canonical bytes");
    assert_eq!(bytes_first, bytes_second);
    assert_eq!(first.digest(), second.digest());
    assert!(first.digest().starts_with("sha256:"));
    assert_eq!(first.fingerprint(), second.fingerprint());

    // The canonical bytes are compact JSON without a trailing newline and
    // carry the frozen key order.
    assert!(bytes_first.starts_with("{\"schemaVersion\":"));
    assert!(!bytes_first.ends_with('\n'));
    // The canonical bytes carry the frozen schema key order (checked on
    // the raw bytes: a JSON parser would sort object keys).
    let expected_order = [
        "\"schemaVersion\":",
        "\"identity\":",
        "\"algorithm\":",
        "\"modelVersion\":",
        "\"project\":",
        "\"input\":",
        "\"request\":",
        "\"roots\":",
        "\"direct\":",
        "\"transitive\":",
        "\"mandatoryPublic\":",
        "\"risks\":",
        "\"targets\":",
        "\"artifacts\":",
        "\"scenarios\":",
        "\"tests\":",
        "\"gates\":",
        "\"explanations\":",
        "\"evidence\":",
        "\"completeness\":",
        "\"diagnosticRefs\":",
        "\"digest\":",
    ];
    let mut cursor = 0usize;
    for key in expected_order {
        let position = bytes_first[cursor..]
            .find(key)
            .unwrap_or_else(|| panic!("key {key} missing or out of order"));
        cursor += position + key.len();
    }
}
#[test]
fn symbol_mode_result_determinism_survives_reordering() {
    // Multi-root requests normalize root order: the radius, digest, and
    // fingerprint are identical for any root permutation.
    let (project, graph, effects) = planner();
    let forward =
        ImpactRequest::for_symbols(&["planner.task", "planner.task_id"]).expect("valid roots");
    let backward =
        ImpactRequest::for_symbols(&["planner.task_id", "planner.task"]).expect("valid roots");
    let forward = analyze(&project, &graph, &effects, &forward, None).expect("impact");
    let backward = analyze(&project, &graph, &effects, &backward, None).expect("impact");
    assert_eq!(forward.digest(), backward.digest());
    assert_eq!(forward.roots(), backward.roots());
}
