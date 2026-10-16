use super::*;

fn run(
    service: &mut WatchService,
    root: &Path,
    policy: &CapturePolicy,
    id: u64,
) -> super::super::super::WatchResult {
    let captured = snapshot::capture_with_policy(root, policy).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let outcome = service.run_revision(
        &captured,
        scratch.path(),
        &RunContext {
            run_id: id,
            generation: id,
            full_rescan: id == 1,
            cancel: &AtomicBool::new(false),
        },
    );
    assert!(!outcome.stale);
    let value = serde_json::to_value(&outcome.result).unwrap();
    assert!(crate::watch::result_value_is_wellformed(&value), "{value}");
    service.commit(outcome.staged);
    outcome.result
}

#[test]
fn negotiated_planner_edit_executes_only_affected_target_partitions_and_retains_diagnostics() {
    let root = project_selective(true);
    let policy = policy(root.path());
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".into()],
        ..WatchRequest::default()
    })
    .unwrap();
    let control = root.path().join("src/control.txt");
    std::fs::write(&control, "unrelated-invalid").unwrap();
    let baseline = run(&mut service, root.path(), &policy, 1);
    assert_eq!(
        baseline.status,
        Status::Invalid,
        "{:?}",
        baseline.diagnostics
    );
    assert_eq!(baseline.pins.adapters[0].protocol_version, "0.8.0");
    let source = root.path().join("lekalo/modules/planner/entities.yaml");
    let original = std::fs::read_to_string(&source).unwrap();
    std::fs::write(&source, original.replace("version: 1", "version: 2")).unwrap();
    let edited = run(&mut service, root.path(), &policy, 2);
    let work = &edited.pins.adapters[0].work;
    assert!(work
        .iter()
        .any(|w| w.module.as_deref() == Some("planner") && w.state == "executed"));
    assert!(work
        .iter()
        .any(|w| w.module.as_deref() == Some("unrelated") && w.state == "reused"));
    assert!(work
        .iter()
        .any(|w| w.scope == "global" && w.state == "executed"));
    assert_eq!(edited.status, Status::Invalid);
    assert_eq!(
        serde_json::to_value(&edited.diagnostics).unwrap(),
        serde_json::to_value(&baseline.diagnostics).unwrap(),
        "untouched failing adapter partition must survive planner-only edits"
    );
    // Independent clean service over the settled snapshot, without reuse.
    let mut clean = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".into()],
        no_cache: true,
        ..WatchRequest::default()
    })
    .unwrap();
    let oracle = run(&mut clean, root.path(), &policy, 1);
    assert_eq!(edited.status, oracle.status);
    assert_eq!(
        serde_json::to_value(&edited.diagnostics).unwrap(),
        serde_json::to_value(&oracle.diagnostics).unwrap()
    );
    // A target-source change has unknown ownership and reruns every partition.
    std::fs::write(&control, "ok").unwrap();
    let recovered = run(&mut service, root.path(), &policy, 3);
    assert_eq!(
        recovered.status,
        Status::Valid,
        "{:?}",
        recovered.diagnostics
    );
    assert!(recovered.pins.adapters[0]
        .work
        .iter()
        .all(|w| w.state == "executed"));
    assert!(recovered.diagnostics.is_empty());
    assert!(!root.path().join(".lekalo/observed").exists());
}

#[test]
fn generated_only_save_executes_no_adapter_exchange_and_wrong_echo_is_rejected() {
    let root = project_selective(true);
    let mut policy = policy(root.path());
    policy.generate_check = true;
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".into()],
        stages: crate::watch::request::WatchStages {
            validate: true,
            generate_check: true,
            impact: false,
        },
        ..WatchRequest::default()
    })
    .unwrap();
    let baseline = run(&mut service, root.path(), &policy, 1);
    assert_eq!(baseline.status, Status::Valid, "{:?}", baseline.diagnostics);
    let launched = crate::target_protocol::transport::launches();
    std::fs::create_dir_all(root.path().join(".lekalo/generated")).unwrap();
    std::fs::write(
        root.path().join(".lekalo/generated/probe.ts"),
        "// synthetic orphan",
    )
    .unwrap();
    let drift = run(&mut service, root.path(), &policy, 2);
    assert_eq!(
        crate::target_protocol::transport::launches(),
        launched,
        "generated drift cannot even describe an adapter"
    );
    assert!(drift
        .stages
        .iter()
        .any(|s| s.stage == "target-scan" && s.state == "reused"));
    assert_eq!(
        drift.status,
        Status::Invalid,
        "an orphan without ownership manifest is explicit drift refusal"
    );
    assert!(drift
        .stages
        .iter()
        .any(|s| s.stage == "generate-check" && s.state == "unavailable"));
    std::fs::remove_file(root.path().join(".lekalo/generated/probe.ts")).unwrap();
    std::fs::write(root.path().join("src/control.txt"), "bad-echo").unwrap();
    let rejected = run(&mut service, root.path(), &policy, 3);
    assert_eq!(rejected.status, Status::Unavailable);
    assert!(rejected
        .diagnostics
        .iter()
        .any(|d| d.id() == "target.response-invalid"));
    assert!(rejected
        .stages
        .iter()
        .any(|s| s.stage == "target-scan" && s.coverage == "partial"));
}

#[test]
fn accumulated_failed_removal_partitions_refuse_before_exceeding_wire_bound() {
    let root = project_selective(true);
    let policy = policy(root.path());
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".into()],
        ..WatchRequest::default()
    })
    .unwrap();
    assert_eq!(
        run(&mut service, root.path(), &policy, 1).status,
        Status::Valid
    );
    let state = service.accepted.targets.get_mut("watch-fixture").unwrap();
    let template = state.partitions["planner"].clone();
    for index in 0..crate::watch::request::limits::MAX_TARGET_PARTITIONS {
        state
            .partitions
            .insert(format!("removed-{index}"), template.clone());
    }
    let source = root.path().join("lekalo/modules/planner/entities.yaml");
    let original = std::fs::read_to_string(&source).unwrap();
    std::fs::write(source, original.replace("version: 1", "version: 2")).unwrap();
    let refused = run(&mut service, root.path(), &policy, 2);
    assert_eq!(refused.status, Status::Denied);
    assert!(refused
        .diagnostics
        .iter()
        .any(|d| d.id() == "watch.resource-limit"));
    assert!(
        refused.pins.adapters.is_empty(),
        "no oversized target work array is published"
    );
}

#[test]
fn legacy_adapters_keep_visible_full_fallback_and_receive_no_selection() {
    let root = project();
    let policy = policy(root.path());
    let mut service = WatchService::new(WatchRequest {
        targets: vec!["watch-fixture".into()],
        ..WatchRequest::default()
    })
    .unwrap();
    let result = run(&mut service, root.path(), &policy, 1);
    assert_eq!(result.status, Status::Valid);
    assert!(result.pins.adapters[0].work.is_empty());
    assert!(result
        .stages
        .iter()
        .any(|r| r.reason.as_deref() == Some("adapter-full-scope")));
}
