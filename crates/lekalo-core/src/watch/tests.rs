use super::*;
use std::sync::atomic::AtomicBool;
#[path = "fault_tests.rs"]
mod faults;
#[path = "pair_tests.rs"]
mod pairs;

fn project() -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/watch/project");
    let snapshot = snapshot::capture(&source, false).unwrap();
    let temp = tempfile::tempdir().unwrap();
    materialize(&snapshot, temp.path()).unwrap();
    temp
}
fn run(service: &mut WatchService, root: &Path, id: u64) -> WatchResult {
    let captured = snapshot::capture(root, service.request.stages.generate_check).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let outcome = service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: id,
            generation: id - 1,
            full_rescan: false,
            cancel: &AtomicBool::new(false),
        },
    );
    assert!(!outcome.stale);
    let value = serde_json::to_value(&outcome.result).unwrap();
    assert!(result_value_is_wellformed(&value), "{value}");
    service.commit(outcome.staged);
    outcome.result
}
fn clean(root: &Path) -> serde_json::Value {
    let model = crate::loader::normalize_captured_root(root).unwrap();
    let compiled = crate::ir::compile(&model).unwrap();
    let profile = crate::validator::ValidationProfile::embedded_default().unwrap();
    let rows = match crate::validator::validate(&compiled, profile) {
        Ok(report) => report.diagnostics().as_slice().to_vec(),
        Err(set) => set.as_slice().to_vec(),
    };
    serde_json::to_value(rows).unwrap()
}
fn edit_planner(root: &Path) {
    let path = root.join("lekalo/modules/planner/entities.yaml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, text.replace("version: 1", "version: 2")).unwrap();
}
#[test]
fn selective_subjects_and_parse_ir_work_are_measured_not_filtered() {
    let root = project();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let first = run(&mut service, root.path(), 1);
    assert_eq!(first.status, Status::Valid);
    let first_counts = first.pipeline_counts.unwrap();
    assert_eq!(first_counts.decoded_documents, 5);
    assert_eq!(
        (
            first_counts.emitted_graph_units,
            first_counts.reused_graph_units
        ),
        (5, 0)
    );
    crate::validator::take_selected_visits();
    edit_planner(root.path());
    let next = run(&mut service, root.path(), 2);
    // Instrumentation at the validator rule-subject seam proves these
    // are execution counts, not module-filtered diagnostic rows.
    let visits = crate::validator::take_selected_visits();
    assert_eq!(visits.len(), 2); // one selected unit + mandatory global recursion
    assert_eq!(
        visits[0],
        (false, BTreeSet::from(["planner.title".to_owned()]))
    );
    assert_eq!(visits[1], (true, BTreeSet::new()));
    assert_eq!(next.affected.scope, "selective");
    assert_eq!(next.affected.modules, ["planner"]);
    assert_eq!(
        next.work
            .iter()
            .find(|unit| unit.module == "planner")
            .unwrap()
            .state,
        "executed"
    );
    assert_eq!(
        next.work
            .iter()
            .find(|unit| unit.module == "unrelated")
            .unwrap()
            .state,
        "reused"
    );
    let counts = next.pipeline_counts.unwrap();
    assert_eq!(
        (counts.emitted_graph_units, counts.reused_graph_units),
        (1, 4)
    );
    assert_eq!(counts.graph_cycle_passes, 1);
    assert_eq!(counts.assembled_graph_nodes, 5);
    assert_eq!((counts.decoded_documents, counts.reused_documents), (1, 4));
    assert_eq!(
        (counts.decoded_definitions, counts.reused_definitions),
        (1, 4)
    );
    assert_eq!(
        serde_json::to_value(next.diagnostics).unwrap(),
        clean(root.path())
    );
    let settled = run(&mut service, root.path(), 3);
    assert_eq!(
        crate::validator::take_selected_visits(),
        vec![(true, BTreeSet::new())]
    );
    assert_eq!(settled.counts.executed_units, 0);
    assert_eq!(settled.counts.reused_units, 2);
    let settled_counts = settled.pipeline_counts.unwrap();
    assert_eq!(
        (
            settled_counts.emitted_graph_units,
            settled_counts.reused_graph_units
        ),
        (0, 5)
    );
}
#[test]
fn unrelated_diagnostics_survive_multiple_selective_runs() {
    let root = project();
    std::fs::write(root.path().join("lekalo/modules/unrelated/entities.yaml"),"schema_version: \"0.2.16\"\ndefinitions:\n  - id: unrelated.name\n    kind: scalar\n    version: 1\n    base: string\n  - id: unrelated.record\n    kind: entity\n    version: 1\n    fields:\n      - name: name\n        type: unrelated.name\n        required: true\n    identity:\n      - absent\n").unwrap();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let baseline = run(&mut service, root.path(), 1);
    assert_eq!(baseline.status, Status::Invalid);
    assert!(baseline
        .diagnostics
        .iter()
        .any(|d| d.id() == "semantic.entity-identity-field-missing"));
    edit_planner(root.path());
    for id in [2, 3] {
        let result = run(&mut service, root.path(), id);
        assert_eq!(
            serde_json::to_value(result.diagnostics).unwrap(),
            clean(root.path())
        );
    }
}
#[test]
fn delete_rename_and_ir_ordinals_match_clean_compiler() {
    let root = project();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    run(&mut service, root.path(), 1);
    std::fs::remove_file(root.path().join("lekalo/modules/planner/entities.yaml")).unwrap();
    let deleted = run(&mut service, root.path(), 2);
    assert!(deleted
        .capsule_invalidations
        .contains(&"planner.title".to_owned()));
    assert_eq!(
        serde_json::to_value(deleted.diagnostics).unwrap(),
        clean(root.path())
    );
    std::fs::rename(
        root.path().join("lekalo/modules/unrelated"),
        root.path().join("lekalo/modules/moved"),
    )
    .unwrap();
    let renamed = run(&mut service, root.path(), 3);
    assert!(renamed
        .changed_inputs
        .iter()
        .any(|row| row.rename_of.is_some()));
    let model = crate::loader::normalize_captured_root(root.path()).unwrap();
    let full = crate::ir::compile(&model).unwrap();
    let (incremental, _, counts) =
        compile::Fragments::compile(&model, &service.accepted.fragments).unwrap();
    assert_eq!(counts.decoded_definitions, 0);
    assert_eq!(incremental.project, full.project);
    assert_eq!(incremental.source_map.to_json(), full.source_map.to_json());
}
#[test]
fn governed_paths_named_target_are_not_ignored_and_output_is_hashed() {
    assert_eq!(
        snapshot::classify_event_path("lekalo/modules/target/entities.yaml"),
        snapshot::EventClass::Semantic
    );
    for ignored in [
        "target/debug/lekalo.exe",
        ".git/index",
        ".lekalo/cache/cache.sqlite",
        "node_modules/pkg/code.js",
    ] {
        assert_eq!(
            snapshot::classify_event_path(ignored),
            snapshot::EventClass::Ignored
        );
    }
    let root = project();
    std::fs::create_dir_all(root.path().join(".lekalo/generated")).unwrap();
    std::fs::write(
        root.path().join(".lekalo/generated/model.rs"),
        "// generated code\n",
    )
    .unwrap();
    let generated = snapshot::capture(root.path(), true).unwrap();
    assert_eq!(generated.generated.len(), 1);
    let core = snapshot::capture(root.path(), false).unwrap();
    assert!(core.generated.is_empty());
    assert_eq!(core.source_revision(), generated.source_revision());
}
#[test]
fn cancellation_and_capture_refusal_cannot_replace_accepted_state() {
    let root = project();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    run(&mut service, root.path(), 1);
    let previous = service.accepted.revision.clone();
    edit_planner(root.path());
    let captured = snapshot::capture(root.path(), false).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let cancelled = service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: 2,
            generation: 1,
            full_rescan: false,
            cancel: &AtomicBool::new(true),
        },
    );
    assert!(cancelled.stale);
    assert_eq!(service.accepted.revision, previous);
    std::fs::write(
        root.path().join("lekalo/modules/planner/entities.yaml"),
        "invalid [",
    )
    .unwrap();
    let invalid = run(&mut service, root.path(), 3);
    assert_eq!(invalid.status, Status::Invalid);
    assert_eq!(service.accepted.revision, previous);
    std::fs::create_dir_all(root.path().join(".lekalo/cache/migrations")).unwrap();
    std::fs::write(
        root.path().join(".lekalo/cache/migrations/active.lock"),
        "active",
    )
    .unwrap();
    let error = snapshot::capture(root.path(), false).unwrap_err();
    assert!(error
        .domain_result()
        .diagnostics()
        .iter()
        .any(|d| d.id() == "versioning.recovery-required"));
}
#[test]
fn mixed_component_replacement_preserves_independent_status_admissions() {
    let invalid = crate::DomainResult::from(&crate::artifacts::ArtifactFailure::ManifestInvalid);
    let version = crate::DomainResult::from(
        &crate::artifacts::ArtifactFailure::UnsupportedSchemaVersion {
            found: "lekalo/artifact-manifest/v99.0.0".to_owned(),
        },
    );
    let rows: Vec<_> = invalid
        .diagnostics()
        .iter()
        .chain(version.diagnostics())
        .cloned()
        .collect();
    assert!(
        crate::diagnostics::DiagnosticSet::try_from_unsorted(
            rows.clone(),
            Status::UnsupportedVersion
        )
        .is_err(),
        "single-command status admission remains strict"
    );
    let normalized = normalize_diagnostics(rows, Status::UnsupportedVersion);
    assert_eq!(normalized.len(), 2);
    assert!(normalized
        .iter()
        .any(|row| row.id() == "lock.schema-invalid"));
    assert!(normalized
        .iter()
        .any(|row| row.id() == "lock.unsupported-schema-version"));
}

#[test]
fn retired_model_refusal_preserves_clean_status_and_diagnostics() {
    let root = project();
    for path in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/unrelated/module.yaml",
        "lekalo/modules/unrelated/entities.yaml",
    ] {
        let path = root.path().join(path);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, text.replace("0.2.16", "0.1.0")).unwrap();
    }
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let result = run(&mut service, root.path(), 1);
    let clean = crate::loader::normalize_captured_root(root.path()).unwrap_err();
    assert_eq!(result.status, Status::UnsupportedVersion);
    assert_eq!(result.status, clean.status());
    assert_eq!(result.pins.model_version, None);
    assert_eq!(
        serde_json::to_value(result.diagnostics).unwrap(),
        serde_json::to_value(clean.diagnostics()).unwrap()
    );
}

#[test]
fn inventory_projection_and_streaming_hashes_keep_exact_digest_spelling() {
    let root = project();
    std::fs::create_dir_all(root.path().join(".lekalo/generated")).unwrap();
    std::fs::write(
        root.path().join(".lekalo/generated/model.rs"),
        "// synthetic\n",
    )
    .unwrap();
    let snapshot = snapshot::capture(root.path(), true).unwrap();
    let mut parts = Vec::new();
    for path in &snapshot.directories {
        if !path.starts_with(snapshot::GENERATED_ROOT) {
            parts.push(format!("dir\0{path}\n"));
        }
    }
    for (path, record) in &snapshot.files {
        parts.push(format!("file\0{path}\0{}\n", record.sha256));
    }
    assert_eq!(
        snapshot.source_revision(),
        format!(
            "sha256:{}",
            crate::digest::sha256_hex(parts.join("").as_bytes())
        )
    );
    let generated: Vec<_> = snapshot
        .generated
        .iter()
        .map(|(path, record)| format!("{path}\0{}\n", record.sha256))
        .collect();
    assert_eq!(
        snapshot.generated_digest(),
        format!(
            "sha256:{}",
            crate::digest::sha256_hex(generated.join("").as_bytes())
        )
    );
    let mut view = snapshot.without_bytes();
    assert!(view
        .files
        .values()
        .chain(view.generated.values())
        .all(|record| record.bytes.is_none()));
    assert!(view.same_inventory(&snapshot));
    assert_eq!(view.source_revision(), snapshot.source_revision());
    view.files.values_mut().next().unwrap().len += 1;
    assert!(!view.same_inventory(&snapshot));
}

#[cfg(unix)]
#[test]
fn no_follow_capture_refuses_links() {
    let root = project();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("lekalo/modules/linked")).unwrap();
    assert!(matches!(
        snapshot::capture(root.path(), false),
        Err(CaptureError::Refused {
            code: "structure.path-link",
            ..
        })
    ));
}
