use super::*;

#[test]
fn successor_model_is_explicitly_refused_not_emitted_under_false_legacy_pins() {
    let root = project();
    for path in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/unrelated/module.yaml",
        "lekalo/modules/unrelated/entities.yaml",
    ] {
        let file = root.path().join(path);
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(file, text.replace("0.2.16", "0.8.0")).unwrap();
    }
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let result = run(&mut service, root.path(), 1);
    assert_eq!(result.status, Status::Unsupported);
    assert!(result.pins.model_version.is_none());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.id() == "watch.selective-unavailable"));
    assert!(service.accepted().is_none());
}

#[test]
fn captured_runs_are_immutable_and_rapid_supersession_cannot_lose_diagnostics() {
    let root = project();
    let unrelated = root.path().join("lekalo/modules/unrelated/entities.yaml");
    std::fs::write(unrelated, "schema_version: \"0.2.16\"\ndefinitions:\n  - id: unrelated.name\n    kind: scalar\n    version: 1\n    base: string\n  - id: unrelated.record\n    kind: entity\n    version: 1\n    fields:\n      - name: name\n        type: unrelated.name\n        required: true\n    identity:\n      - absent\n").unwrap();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let baseline = run(&mut service, root.path(), 1);
    let accepted = service.accepted().unwrap().revision.clone();
    let planner = root.path().join("lekalo/modules/planner/entities.yaml");
    let source = std::fs::read_to_string(&planner).unwrap();
    edit_planner(root.path());
    let captured = snapshot::capture(root.path(), false).unwrap();
    // The live tree now has different, invalid bytes. No stage may
    // reread them through the original root during the captured run.
    std::fs::write(&planner, "schema_version: [\n").unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let intermediate = service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: 2,
            generation: 1,
            full_rescan: false,
            cancel: &AtomicBool::new(false),
        },
    );
    assert_eq!(intermediate.source_revision, captured.source_revision());
    assert_eq!(
        serde_json::to_value(&intermediate.result.diagnostics).unwrap(),
        serde_json::to_value(&baseline.diagnostics).unwrap()
    );
    // Simulate the host's precommit revalidation: the captured inventory
    // differs, so the result is superseded and NEVER committed.
    assert!(!snapshot::capture(root.path(), false)
        .unwrap()
        .same_inventory(&captured));
    assert_eq!(service.accepted().unwrap().revision, accepted);
    // A transient parse failure must retain the unrelated partition.
    let invalid = run(&mut service, root.path(), 3);
    assert!(invalid
        .diagnostics
        .iter()
        .any(|d| d.id() == "semantic.entity-identity-field-missing"));
    assert!(invalid.diagnostics.len() > baseline.diagnostics.len());
    assert_eq!(service.accepted().unwrap().revision, accepted);
    // A burst has one final captured candidate; prior diagnostics must
    // neither disappear nor accumulate duplicate/stale parser findings.
    for version in [3, 4, 5, 6] {
        std::fs::write(
            &planner,
            source.replace("version: 1", &format!("version: {version}")),
        )
        .unwrap();
    }
    let settled = run(&mut service, root.path(), 4);
    assert_eq!(
        serde_json::to_value(&settled.diagnostics).unwrap(),
        clean(root.path())
    );
    assert_eq!(settled.diagnostics.len(), baseline.diagnostics.len());
    assert!(settled
        .work
        .iter()
        .any(|unit| unit.module == "unrelated" && unit.state == "reused"));
}

#[test]
fn dependent_units_execute_but_unrelated_rules_do_not_and_global_changes_fall_back() {
    let root = project();
    let reporter = root.path().join("lekalo/modules/reporter");
    std::fs::create_dir(&reporter).unwrap();
    std::fs::write(reporter.join("module.yaml"), "schema_version: \"0.2.16\"\ndefinitions:\n  - id: reporter\n    kind: module\n    version: 1\n    imports:\n      - planner\n").unwrap();
    std::fs::write(reporter.join("entities.yaml"), "schema_version: \"0.2.16\"\ndefinitions:\n  - id: reporter.entry\n    kind: entity\n    version: 1\n    fields:\n      - name: title\n        type: planner.title\n        required: true\n    identity:\n      - title\n").unwrap();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    let baseline = run(&mut service, root.path(), 1);
    assert_eq!(baseline.status, Status::Valid, "{:?}", baseline.diagnostics);
    crate::validator::take_selected_visits();
    edit_planner(root.path());
    let result = run(&mut service, root.path(), 2);
    assert_eq!(
        result.affected.modules,
        ["planner", "reporter"],
        "{:?}: {:?}",
        result.affected,
        result.diagnostics
    );
    let visits = crate::validator::take_selected_visits();
    assert_eq!(visits.len(), 3); // planner, reporter, mandatory recursion
    let subjects: BTreeSet<_> = visits
        .into_iter()
        .flat_map(|(_, symbols)| symbols)
        .collect();
    assert_eq!(
        subjects,
        BTreeSet::from(["planner.title".to_owned(), "reporter.entry".to_owned()])
    );
    assert_eq!(
        serde_json::to_value(result.diagnostics).unwrap(),
        clean(root.path())
    );
    let project = root.path().join("lekalo/project.yaml");
    let mut bytes = std::fs::read(&project).unwrap();
    bytes.push(b'\n');
    std::fs::write(project, bytes).unwrap();
    let fallback = run(&mut service, root.path(), 3);
    assert_eq!(fallback.affected.reason, Some("project-input"));
    let subjects: BTreeSet<_> = crate::validator::take_selected_visits()
        .into_iter()
        .flat_map(|(_, symbols)| symbols)
        .collect();
    assert!(
        subjects.contains("unrelated.name"),
        "global input change really executes the full fallback"
    );
}
