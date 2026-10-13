//! Differential evidence for session-only outgoing-edge fragments.
use super::build;
use super::build::{build_reusing, GraphReuse, GraphWork};
use crate::ir::{CompiledProject, Definition, EntityDef};

fn project() -> CompiledProject {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/graph/planner")
        .canonicalize()
        .unwrap();
    let model = crate::loader::normalize_captured_root(&root).unwrap();
    crate::ir::compile(&model).unwrap().project
}

fn differential(project: &CompiledProject, old: &GraphReuse) -> (GraphReuse, GraphWork) {
    let full = build(project);
    let incremental = build_reusing(project, old);
    match (full, incremental) {
        (Ok(full), Ok((next, cache, counts))) => {
            assert_eq!(
                next.to_canonical_json().unwrap(),
                full.to_canonical_json().unwrap()
            );
            // Exercise both indexes, not only the exported edge inventory.
            let filter = super::EdgeFilter::default();
            for node in full.nodes() {
                assert_eq!(
                    next.outgoing(node.id(), &filter),
                    full.outgoing(node.id(), &filter)
                );
                assert_eq!(
                    next.incoming(node.id(), &filter),
                    full.incoming(node.id(), &filter)
                );
            }
            (cache, counts)
        }
        (Err(full), Err(next)) => {
            assert_eq!(
                serde_json::to_value(next.as_slice()).unwrap(),
                serde_json::to_value(full.as_slice()).unwrap()
            );
            (GraphReuse::default(), GraphWork::default())
        }
        _ => panic!("incremental graph differs from clean terminal class"),
    }
}

#[test]
fn unchanged_and_local_edits_reuse_actual_outgoing_fragments() {
    let mut project = project();
    let (old, first) = differential(&project, &GraphReuse::default());
    assert_eq!(first.reused_units, 0);
    let (_, settled) = differential(&project, &old);
    assert_eq!(settled.emitted_units, 0);
    assert_eq!(settled.reused_units, first.emitted_units);
    let scalar = project
        .definitions
        .iter_mut()
        .find_map(|d| match d {
            Definition::Scalar(s) if s.id.as_str() == "planner.text" => Some(s),
            _ => None,
        })
        .unwrap();
    scalar.common.version += 1;
    let (_, edited) = differential(&project, &old);
    assert_eq!(edited.emitted_units, 1);
    assert_eq!(edited.reused_units + 1, first.emitted_units);
}

#[test]
fn kind_changes_rebind_unchanged_inbound_sources() {
    let mut project = project();
    let (old, _) = differential(&project, &GraphReuse::default());
    let target = project
        .definitions
        .iter_mut()
        .find(|d| d.id().as_str() == "planner.text")
        .unwrap();
    let Definition::Scalar(scalar) = target.clone() else {
        panic!("scalar fixture")
    };
    *target = Definition::Entity(EntityDef {
        id: scalar.id,
        common: scalar.common,
        fields: Vec::new(),
        identity: Vec::new(),
    });
    let (_, counts) = differential(&project, &old);
    assert_eq!(counts.emitted_units, 2); // target + planner.task's type site
    assert!(counts.reused_units > counts.emitted_units);
}

#[test]
fn deletion_refuses_unresolved_edges_and_prunes_last_requirement_owner() {
    let original = project();
    let (old, _) = differential(&original, &GraphReuse::default());
    let mut broken = original.clone();
    broken
        .definitions
        .retain(|d| d.id().as_str() != "planner.text");
    assert!(build_reusing(&broken, &old).is_err());
    differential(&broken, &old);
    let mut pruned = original;
    pruned
        .definitions
        .retain(|d| d.id().as_str() != "planner.count_focused");
    let (graph, _, _) = build_reusing(&pruned, &old).unwrap();
    assert!(graph.resolve("PLANNER-REQ-002").is_none());
    differential(&pruned, &old);
}

#[test]
fn cycles_and_duplicate_input_still_run_global_policy() {
    let original = project();
    let (old, _) = differential(&original, &GraphReuse::default());
    let mut cycle = original.clone();
    let notify = cycle
        .modules
        .iter()
        .find(|m| m.id.as_str() == "notify")
        .unwrap()
        .id
        .clone();
    cycle
        .modules
        .iter_mut()
        .find(|m| m.id.as_str() == "planner")
        .unwrap()
        .imports
        .push(notify);
    assert!(build_reusing(&cycle, &old).is_err());
    differential(&cycle, &old);
    let mut duplicate = original;
    duplicate.definitions.push(duplicate.definitions[0].clone());
    assert!(build_reusing(&duplicate, &old).is_err());
    differential(&duplicate, &old);
}
