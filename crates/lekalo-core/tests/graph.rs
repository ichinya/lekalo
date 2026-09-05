//! Issue #13 library tests for the dependency graph: construction over the
//! hermetic planner fixture through the accepted loader seam, the emission
//! table, bounded traversal and slices, module boundaries, shortest paths,
//! registry extension, filters, and canonical export stability.
//!
//! The #8 IR newtypes are deliberately constructible only inside the crate,
//! so these integration tests drive the same loader seam every consumer
//! uses; the forbidden-cycle policy is exercised in the internal unit tests
//! of `graph::cycle`, where hand-assembled graphs are crate-visible.

use lekalo_core::graph::{
    build, build_with_registry, cycle, slice, traverse, Confidence, DependencyGraph, Direction,
    EdgeFilter, GraphRegistry, NodeId, NodeKindId, RelationSelection, TraversalSpec,
    MAX_FILTER_TERMS,
};
use lekalo_core::ir::{compile, CompiledProject};
use lekalo_core::loader::{normalize_model, LoadSelection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FIXTURE: &str = "tests/fixtures/graph/planner";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn compile_fixture(name: &str) -> CompiledProject {
    let selection = LoadSelection {
        project: Some(name.to_owned()),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("{name}: load failed: {}", outcome.to_json_string()),
    };
    match compile(&model) {
        Ok(compilation) => compilation.project,
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

fn planner() -> CompiledProject {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let project = compile_fixture(FIXTURE);
    std::env::set_current_dir(original).expect("restore cwd");
    project
}

fn ids(graph: &DependencyGraph) -> Vec<String> {
    graph
        .nodes()
        .iter()
        .map(|node| node.id().as_str().to_owned())
        .collect()
}

fn edge_keys(graph: &DependencyGraph) -> Vec<String> {
    graph
        .edges()
        .iter()
        .map(|edge| edge.key().to_canonical_string())
        .collect()
}

#[test]
fn construction_is_deterministic_and_canonical() {
    let project = planner();
    let first = build(&project).expect("graph builds");
    let second = build(&project).expect("graph builds");
    assert_eq!(ids(&first), ids(&second));
    assert_eq!(edge_keys(&first), edge_keys(&second));
    // Nodes sort by kind rank, then module, then semantic id.
    let order = ids(&first);
    assert_eq!(order[0], "project:planner");
    let module_slice = &order[1..3];
    assert_eq!(module_slice, &["module:notify", "module:planner"]);
    // Requirements sort last by their uppercase ids.
    assert_eq!(order[order.len() - 1], "requirement:PLANNER-REQ-002");
}

#[test]
fn every_core_node_kind_is_projected_with_subkinds() {
    let graph = build(&planner()).expect("graph builds");
    assert_eq!(graph.nodes().len(), 24);
    assert_eq!(
        graph
            .nodes()
            .iter()
            .filter(|node| node.kind() == NodeKindId::PROJECT)
            .count(),
        1
    );
    assert_eq!(
        graph
            .nodes()
            .iter()
            .filter(|node| node.kind() == NodeKindId::MODULE)
            .count(),
        2
    );
    assert_eq!(
        graph
            .nodes()
            .iter()
            .filter(|node| node.kind() == NodeKindId::TYPE)
            .count(),
        7
    );
    assert_eq!(
        graph
            .nodes()
            .iter()
            .filter(|node| node.kind() == NodeKindId::REQUIREMENT)
            .count(),
        2
    );
    let command = graph.resolve("planner.focus_task").expect("command node");
    assert_eq!(command.kind(), NodeKindId::OPERATION);
    assert_eq!(command.subkind(), Some("command"));
    let query = graph.resolve("planner.count_focused").expect("query node");
    assert_eq!(query.subkind(), Some("query"));
}

#[test]
fn emission_table_maps_every_accepted_ir_surface() {
    let graph = build(&planner()).expect("graph builds");
    let keys = edge_keys(&graph);
    let has = |needle: &str| keys.iter().any(|key| key == needle);
    // Module import -> requires (cross-module).
    assert!(has("module:notify|requires|module:planner|0"));
    // Entity type leaves -> references; repeated types keep distinct sites.
    assert!(has("entity:planner.task|references|type:planner.task_id|0"));
    // Command input -> accepts; command effects -> references.
    assert!(has(
        "operation:planner.focus_task|accepts|type:planner.task_id|0"
    ));
    assert!(has(
        "operation:planner.focus_task|accepts|type:planner.task_id|1"
    ));
    assert!(has(
        "operation:planner.focus_task|references|effect:planner.create_task|0"
    ));
    // Query reads -> reads; query returns -> returns.
    assert!(has(
        "operation:planner.count_focused|reads|entity:planner.task|0"
    ));
    assert!(has(
        "operation:planner.count_focused|returns|type:planner.task_state|0"
    ));
    // Policy applies-to -> authorizes; endpoint invokes -> exposes.
    assert!(has(
        "policy:planner.deny_bulk_focus|authorizes|operation:planner.focus_task|0"
    ));
    assert!(has(
        "endpoint:planner.api_focus|exposes|operation:planner.focus_task|0"
    ));
    // Declared effect emits -> emits; effect entity -> references.
    assert!(has(
        "effect:planner.create_task|emits|event:planner.task_focused|0"
    ));
    // Scenario covers -> references until #23 defines verifies.
    assert!(has(
        "scenario:planner.focus_flow|references|operation:planner.focus_task|0"
    ));
    // Requirement provenance -> derived_from, project included.
    assert!(has(
        "project:planner|derived_from|requirement:PLANNER-REQ-001|0"
    ));
    assert!(has(
        "operation:planner.count_focused|derived_from|requirement:PLANNER-REQ-002|1"
    ));
    // The relations without accepted typed owners are never emitted.
    for edge in graph.edges() {
        assert_ne!(edge.key().relation().key(), "writes");
        assert_ne!(edge.key().relation().key(), "implements");
        assert_ne!(edge.key().relation().key(), "verifies");
        assert_eq!(edge.confidence(), Confidence::Canonical);
    }
}

#[test]
fn repeated_references_stay_separate_edges() {
    let graph = build(&planner()).expect("graph builds");
    let accepts: Vec<_> = graph
        .edges()
        .iter()
        .filter(|edge| {
            edge.key().relation() == lekalo_core::graph::RelationKindId::ACCEPTS
                && edge.key().from().as_str() == "operation:planner.focus_task"
                && edge.key().to().as_str() == "type:planner.task_id"
        })
        .collect();
    assert_eq!(accepts.len(), 2);
    assert_eq!(accepts[0].key().occurrence().get(), 0);
    assert_eq!(accepts[1].key().occurrence().get(), 1);
}

#[test]
fn traversal_is_bounded_deterministic_and_filtered() {
    let graph = build(&planner()).expect("graph builds");
    let root = graph.resolve_id("entity:planner.task").unwrap().clone();

    let forward = graph
        .transitive(&root, &TraversalSpec::new(Direction::Forward))
        .expect("forward closure");
    assert!(forward.complete());

    assert!(forward
        .nodes()
        .contains(&NodeId::from_qualified("type:planner.task_id").unwrap()));

    let reverse = graph
        .transitive(&root, &TraversalSpec::new(Direction::Reverse))
        .expect("reverse closure");
    assert!(reverse
        .nodes()
        .contains(&NodeId::from_qualified("entity:planner.task").unwrap()));

    // Filtered reverse queries answer from the precomputed incoming index.
    let filter = EdgeFilter::new()
        .with_relation_keys(&GraphRegistry::core(), &["derived_from"])
        .expect("registered relation");
    let requirement = graph.resolve_id("PLANNER-REQ-001").unwrap().clone();
    let dependents = graph.reverse_dependencies(&requirement, &filter);
    assert_eq!(dependents.len(), 3);

    // Unknown nodes and unregistered relations fail closed.
    let unknown = NodeId::from_qualified("entity:planner.nonesuch").unwrap();
    assert!(graph
        .transitive(&unknown, &TraversalSpec::new(Direction::Forward))
        .is_err());
    assert!(EdgeFilter::new()
        .with_relation_keys(&GraphRegistry::core(), &["writes"])
        .is_ok());
    assert!(EdgeFilter::new()
        .with_relation_keys(&GraphRegistry::core(), &["vendor.example/rel"])
        .is_err());
}

#[test]
fn shortest_path_and_module_boundary_are_deterministic() {
    let graph = build(&planner()).expect("graph builds");
    let from = graph.resolve_id("planner.api_focus").unwrap().clone();
    let to = graph.resolve_id("planner.task_focused").unwrap().clone();
    let path = graph
        .shortest_path(&from, &to, &traverse::PathSpec::new())
        .expect("a path exists");
    assert_eq!(path.length(), 2);
    assert_eq!(path.confidence(), Confidence::Canonical);
    assert_eq!(path.nodes()[0], from);
    assert_eq!(path.nodes()[2], to);

    // No path is an explicit failure, never an empty success.
    let source = graph.resolve_id("planner.binding_node").unwrap().clone();
    assert!(graph
        .shortest_path(&source, &to, &traverse::PathSpec::new())
        .is_err());

    let boundary = graph.module_boundary("planner").expect("module exists");
    assert!(!boundary.internal().is_empty());
    assert!(!boundary.outbound().is_empty());
    assert!(!boundary.inbound().is_empty());
    assert!(graph.module_boundary("nonesuch").is_err());
}

#[test]
fn slices_report_truncation_explicitly() {
    let graph = build(&planner()).expect("graph builds");
    let root = graph.resolve_id("planner.task").unwrap().clone();
    let spec = slice::SliceSpec::new(Direction::Forward)
        .with_max_nodes(2)
        .expect("bound");
    let bounded = graph
        .slice(std::slice::from_ref(&root), &spec)
        .expect("slice");
    assert!(!bounded.complete());
    assert_eq!(
        bounded.truncation(),
        Some(slice::TruncationReason::NodeBound)
    );

    let full = graph
        .slice(&[root], &slice::SliceSpec::new(Direction::Forward))
        .expect("slice");
    assert!(full.complete());
    assert_eq!(full.truncation(), None);
    assert_eq!(full.nodes().len(), 6);
}

#[test]
fn registry_extensions_are_closed_and_versioned() {
    let registry = GraphRegistry::core()
        .with_extension_kind(lekalo_core::graph::ExtensionKind {
            key: "vendor.example/widget".to_owned(),
            version: "1.0.0".to_owned(),
        })
        .expect("valid extension kind");
    let registry = registry
        .with_extension_relation(lekalo_core::graph::ExtensionRelation {
            key: "vendor.example/bundles".to_owned(),
            version: "1.0.0".to_owned(),
            acyclic: false,
        })
        .expect("valid extension relation");
    assert_eq!(
        registry.relation("vendor.example/bundles"),
        Some(RelationSelection::Extension(
            "vendor.example/bundles".to_owned()
        ))
    );
    // Unregistered extension keys fail closed.
    assert!(registry.relation("vendor.example/other").is_none());
    // Malformed records are refused.
    assert!(GraphRegistry::core()
        .with_extension_kind(lekalo_core::graph::ExtensionKind {
            key: "nowhere".to_owned(),
            version: "1.0.0".to_owned(),
        })
        .is_err());
    assert!(GraphRegistry::core()
        .with_extension_relation(lekalo_core::graph::ExtensionRelation {
            key: "vendor.example/x".to_owned(),
            version: "not-semver".to_owned(),
            acyclic: false,
        })
        .is_err());
    let graph = build_with_registry(&planner(), &registry).expect("extended registry builds");
    assert_eq!(graph.registry().extension_kind_keys().len(), 1);
}

#[test]
fn canonical_export_is_stable_and_closed() {
    let graph = build(&planner()).expect("graph builds");
    let first = graph.to_canonical_json().expect("export");
    let second = build(&planner())
        .expect("graph builds")
        .to_canonical_json()
        .expect("export");
    assert_eq!(first, second);
    assert!(
        !first.contains('\n'),
        "compact, no insignificant whitespace"
    );
    let document: serde_json::Value = serde_json::from_str(&first).expect("valid json");
    assert_eq!(document["schemaVersion"], "lekalo/graph/v1.0.0");
    assert_eq!(document["identity"], "dev.lekalo.graph@1.0.0");
    assert_eq!(document["project"], "planner");
    assert_eq!(document["metadata"]["nodeCount"], 24);
    assert_eq!(document["metadata"]["edgeCount"], 32);
}

#[test]
fn filter_terms_are_bounded() {
    let registry = GraphRegistry::core();
    let terms: Vec<&str> = (0..MAX_FILTER_TERMS + 1).map(|_| "requires").collect();
    assert!(EdgeFilter::new()
        .with_relation_keys(&registry, &terms)
        .is_err());
}

#[test]
fn cycle_detection_finds_nothing_on_acyclic_graphs() {
    let graph = build(&planner()).expect("graph builds");
    assert!(cycle::find_all(&graph).is_empty());
}

#[test]
fn resolve_precedence_is_deterministic() {
    let graph = build(&planner()).expect("graph builds");
    // `planner` names a module here; the bare id resolves to the module.
    let module = graph.resolve("planner").expect("module wins");
    assert_eq!(module.kind(), NodeKindId::MODULE);
    // The exact qualified form reaches the project node directly.
    let project = graph.resolve("project:planner").expect("project");
    assert_eq!(project.kind(), NodeKindId::PROJECT);
    let requirement = graph.resolve("PLANNER-REQ-001").expect("requirement");
    assert_eq!(requirement.kind(), NodeKindId::REQUIREMENT);
}
