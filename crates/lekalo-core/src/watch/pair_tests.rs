//! Pair admission and refusal producer regressions, not just isolated
//! inventory limits or hand-authored positive contract vectors.
use super::*;

fn records(prefix: &str, count: usize) -> BTreeMap<String, FileRecord> {
    (0..count)
        .map(|i| {
            let bytes = i.to_string().into_bytes();
            (
                format!("{prefix}/{i}.txt"),
                FileRecord {
                    sha256: format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
                    len: bytes.len() as u64,
                    bytes: Some(bytes),
                },
            )
        })
        .collect()
}

#[test]
fn pair_limits_include_both_rename_sides_for_generated_and_target_paths() {
    for generated in [false, true] {
        for count in [2048, 2049] {
            let mut old = InputSnapshot::default();
            let mut new = InputSnapshot::default();
            if generated {
                old.generated = records(".lekalo/generated/old", count);
                new.generated = records(".lekalo/generated/new", count);
            } else {
                old.files = records("src/old", count);
                new.files = records("src/new", count);
            }
            assert!(budget::admit(&old).is_ok());
            assert!(budget::admit(&new).is_ok());
            let pair = diff(&old, &new);
            assert_eq!(pair.len(), count * 2);
            assert_eq!(pair.admit().is_ok(), count == 2048);
            if !generated {
                assert_eq!(pair.semantic_rows().count(), count * 2);
                assert_eq!(crate::target_protocol::watch::MAX_CHANGED_PATHS, 4096);
            }
        }
    }
}

#[test]
fn over_limit_pair_refuses_before_materialization_and_cannot_commit() {
    let root = project();
    let mut service = WatchService::new(WatchRequest::default()).unwrap();
    run(&mut service, root.path(), 1);
    let accepted = service.accepted().unwrap().revision.clone();
    service.accepted.generated = records(".lekalo/generated/old", 2050)
        .into_iter()
        .map(|(path, record)| (path, (record.sha256, record.len)))
        .collect();
    let mut current = snapshot::capture(root.path(), false).unwrap();
    current.generated = records(".lekalo/generated/new", 2050);
    let scratch = tempfile::tempdir().unwrap();
    let outcome = service.run_revision(
        &current,
        scratch.path(),
        &RunContext {
            run_id: 2,
            generation: 1,
            full_rescan: true,
            cancel: &AtomicBool::new(false),
        },
    );
    assert_eq!(outcome.status, Status::Denied);
    assert!(outcome.staged.revision.is_empty());
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    assert!(outcome.result.changed_inputs.is_empty());
    let value = serde_json::to_value(&outcome.result).unwrap();
    assert_eq!(value["diagnostics"][0]["id"], "watch.resource-limit");
    assert_eq!(value["diagnostics"][0]["data"]["count"], 4100);
    assert_eq!(value["diagnostics"][0]["data"]["limit"], "changed-inputs");
    assert!(result_value_is_wellformed(&value), "{value}");
    assert!(outcome
        .result
        .stages
        .iter()
        .filter(|s| s.state != "not-requested")
        .all(|s| s.coverage == "partial"));
    let event = events::WatchEvent::run_completed(
        3,
        "pair-test",
        2,
        1,
        &outcome.source_revision,
        outcome.status,
        outcome.result,
    );
    assert!(events::WatchEvent::decode(&event.to_json_line()).is_ok());
    service.commit(outcome.staged);
    assert_eq!(service.accepted().unwrap().revision, accepted);
}

#[test]
fn generated_security_and_io_refusals_preserve_schema_and_classification() {
    for path in [
        ".lekalo/generated/link",
        ".lekalo/generated/Transient File",
        "src/UPPER",
        "",
        "../escape",
    ] {
        for error in [
            CaptureError::Refused {
                code: "structure.path-link",
                path: path.into(),
            },
            CaptureError::Io { path: path.into() },
        ] {
            let refusal = error.domain_result();
            let expected = if matches!(error, CaptureError::Io { .. }) {
                "loader.io"
            } else {
                "structure.path-link"
            };
            assert_eq!(refusal.diagnostics()[0].id(), expected);
            let event = events::WatchEvent::recovery_update(
                1,
                "capture-test",
                "inventory-mismatch",
                None,
                Some(&refusal),
            );
            assert!(
                events::WatchEvent::decode(&event.to_json_line()).is_ok(),
                "{}",
                event.to_json_line()
            );
            assert!(
                event.value()["diagnostics"][0].get("source").is_none(),
                "never invent a diagnostic location"
            );
        }
    }
    // The loader wire producer bounds tokens to 256 bytes; do not
    // truncate a longer inventory path into a misleading location.
    let long = CaptureError::Io {
        path: format!("src/{}/file", "x".repeat(256)),
    }
    .domain_result();
    let event = events::WatchEvent::recovery_update(
        1,
        "capture-test",
        "inventory-mismatch",
        None,
        Some(&long),
    );
    assert!(event.value()["diagnostics"][0].get("source").is_none());
    assert!(events::WatchEvent::decode(&event.to_json_line()).is_ok());
    let ordinary = CaptureError::Io {
        path: "lekalo/project.yaml".into(),
    }
    .domain_result();
    let event = events::WatchEvent::recovery_update(
        1,
        "capture-test",
        "inventory-mismatch",
        None,
        Some(&ordinary),
    );
    assert_eq!(
        event.value()["diagnostics"][0]["source"]["path"],
        "lekalo/project.yaml"
    );
    assert!(events::WatchEvent::decode(&event.to_json_line()).is_ok());
}

#[test]
fn modified_membership_keeps_current_impact_and_old_invalidation_evidence() {
    let root = project();
    let mut service = WatchService::new(WatchRequest {
        stages: request::WatchStages {
            validate: true,
            impact: true,
            generate_check: false,
        },
        ..WatchRequest::default()
    })
    .unwrap();
    run(&mut service, root.path(), 1);
    let path = root.path().join("lekalo/modules/planner/entities.yaml");
    let original = std::fs::read_to_string(&path).unwrap();
    for (id, text, expected) in [
        (2, format!("{original}  - id: planner.extra\n    kind: scalar\n    version: 1\n    base: string\n"), 2),
        (3, original.clone(), 1),
        (4, original.replace("planner.title", "planner.newtitle"), 1),
    ] {
        std::fs::write(&path, text).unwrap();
        let result = run(&mut service, root.path(), id);
        assert_eq!(result.status, Status::Valid);
        let impact = result.impact.unwrap();
        assert_eq!(impact["roots"].as_array().unwrap().len(), expected);
        if id == 3 {assert!(result.capsule_invalidations.contains(&"planner.extra".into()));}
        if id == 4 {assert!(result.capsule_invalidations.contains(&"planner.title".into()));}
    }
}
