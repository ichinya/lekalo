//! Transaction fault tests use a simulated capability host. The separate CLI
//! acceptance gate qualifies actual confined native execution.
use super::*;
use serde_json::json;

struct Host {
    program: std::path::PathBuf,
    expected: Vec<Observation>,
    calls: std::cell::Cell<usize>,
}
impl VerificationHost for Host {
    fn resolve_program(&self, _: &str) -> Option<std::path::PathBuf> {
        Some(self.program.clone())
    }
    fn run(&self, _: &Path, _: &[String], _: &Path) -> Result<(Option<i32>, String), String> {
        panic!("mutable host must not run")
    }
    fn run_immutable(
        &self,
        _: &Path,
        _: &[String],
        _: &Path,
    ) -> Result<(Option<i32>, String), String> {
        self.calls.set(self.calls.get() + 1);
        Ok((Some(0), serde_json::to_string(&self.expected).unwrap()))
    }
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &to.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}
fn fixture() -> (tempfile::TempDir, TransitionPlan, Host) {
    let root = tempfile::tempdir().unwrap();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/managed/planner"),
        root.path(),
    );
    let mut policy: serde_json::Value =
        serde_json::from_slice(&required(root.path(), "ownership.json").unwrap()).unwrap();
    policy["modules"][0]["mode"] = json!("observed");
    mg::fs::write_file(root.path(), mg::source::PROJECT_PATH, &bytes(&policy)).unwrap();
    let mut symbols = vec![];
    let coverage: serde_json::Value =
        serde_json::from_slice(&required(root.path(), "coverage.json").unwrap()).unwrap();
    let expected: Vec<Observation> = coverage["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| Observation {
            scenario: row["id"].as_str().unwrap().to_owned(),
            output: row["sample"]["value"]["output"].clone(),
            error: json!(null),
            state: json!(null),
            effects: vec![],
            authorization: json!(null),
            transaction: json!(null),
        })
        .collect();
    for (name, kind, signature) in [
        (
            "complete_task",
            "command",
            json!({"inputs":[{"name":"done","type":"planner.boolean","required":true},{"name":"id","type":"planner.task_id","required":true}],"output":null,"reads":[]}),
        ),
        (
            "focus_task",
            "query",
            json!({"inputs":[],"output":"planner.task","reads":["planner.task"]}),
        ),
    ] {
        let path = format!("src/{name}.mjs");
        let body = format!("// simulated transaction fixture: {name}\n");
        mg::fs::write_file(root.path(), &path, body.as_bytes()).unwrap();
        symbols.push(
            json!({"id":format!("planner.{name}"),"kind":kind,"source":{"path":path,"line":1},
            "fingerprint":mg::digest(body.as_bytes()),"signature":signature,"effects":[]}),
        );
    }
    let declaration = json!({"schemaVersion":"lekalo/contracted-declaration/v0.4.0",
        "adapter":{"id":"ownership-fixture","version":"1.0.0","digest":null},"project":"planner_fixture",
        "revision":mg::digest(b"reviewed-maintained-fixture"),"artifacts":[],"symbols":symbols});
    mg::fs::write_file(root.path(), "declaration.json", &bytes(&declaration)).unwrap();
    mg::fs::write_file(root.path(), "runner.mjs", b"// simulated host\n").unwrap();
    mg::fs::write_file(root.path(), "runtime", b"test-runtime").unwrap();
    let request = Request {
        schema_version: format!("lekalo/ownership-transition-request/v{VERSION}"),
        module: "planner".into(),
        symbol: None,
        to: mg::Mode::Contracted,
        inputs: vec!["runner.mjs".into()],
        coverage: None,
        declaration: Some("declaration.json".into()),
        unknown_behavior: vec![],
        runner: Runner {
            program: "node".into(),
            digest: mg::digest(b"test-runtime"),
            args: vec!["runner.mjs".into()],
            expected: expected.clone(),
        },
    };
    let saved = plan(root.path(), request).unwrap();
    let host = Host {
        program: root.path().join("runtime"),
        expected,
        calls: std::cell::Cell::new(0),
    };
    (root, saved, host)
}
fn all_match(root: &Path, plan: &TransitionPlan, restored: bool) -> bool {
    plan.edits
        .iter()
        .all(|edit| matches(root, edit, restored).unwrap())
}

#[test]
fn assignment_capacity_refuses_new_scope_but_allows_the_last_slot_and_replacement() {
    for (count, replacement) in [
        (MAX_FILES - 1, false),
        (MAX_FILES, false),
        (MAX_FILES, true),
    ] {
        let (root, prepared, host) = fixture();
        let mut existing = OwnershipState {
            assignments: (0..count)
                .map(|index| Assignment {
                    module: "archived".to_owned(),
                    symbol: Some(format!("archived.scope_{index}")),
                    mode: mg::Mode::Contracted,
                    transition: mg::digest(b"retained scoped history"),
                    artifacts: vec![],
                })
                .collect(),
            ..OwnershipState::default()
        };
        if replacement {
            // A canonical policy can declare an observed scope; replacing it
            // consumes no additional assignment slot.
            existing.assignments[count - 1] = Assignment {
                module: "planner".to_owned(),
                symbol: None,
                mode: mg::Mode::Observed,
                transition: mg::digest(b"observed policy"),
                artifacts: vec![],
            };
        }
        let before = bytes(&existing);
        mg::fs::write_file(root.path(), STATE, &before).unwrap();
        assert_eq!(state(root.path()).unwrap().assignments.len(), count);
        let result = plan(root.path(), prepared.request);
        if count == MAX_FILES && !replacement {
            assert!(
                result.is_err(),
                "a 513th assignment must refuse during preparation"
            );
            assert_eq!(required(root.path(), STATE).unwrap(), before);
            assert!(read(root.path(), PENDING).unwrap().is_none());
            assert!(!root.path().join(format!("{HOME}/receipts")).exists());
            assert_eq!(host.calls.get(), 0);
        } else {
            let accepted = result.unwrap();
            apply(root.path(), &accepted, &accepted.plan_digest, &host).unwrap();
            assert_eq!(state(root.path()).unwrap().assignments.len(), MAX_FILES);
            rollback(
                root.path(),
                &accepted.plan_digest,
                Some(&accepted.plan_digest),
            )
            .unwrap();
            assert_eq!(required(root.path(), STATE).unwrap(), before);
        }
    }
}

#[test]
fn every_publication_boundary_recovers_to_before_or_committed_after() {
    let mut cuts = 0;
    for boundary in 0..16 {
        let (root, plan, host) = fixture();
        STOP.with(|stop| stop.set(Some(boundary)));
        let result = apply(root.path(), &plan, &plan.plan_digest, &host);
        STOP.with(|stop| stop.set(None));
        if result.is_ok() {
            assert!(all_match(root.path(), &plan, false));
            break;
        }
        cuts += 1;
        assert!(barrier(root.path()));
        assert!(crate::loader::load_validated_root(root.path().to_path_buf()).is_err());
        assert!(crate::project_writer::WriterLock::mutation(
            root.path(),
            crate::project_writer::MutationFamily::Change
        )
        .is_err());
        let before = read(root.path(), STATE).unwrap();
        recovery(root.path(), None).unwrap();
        assert_eq!(
            read(root.path(), STATE).unwrap(),
            before,
            "preview is read-only"
        );
        let recovered = recovery(root.path(), Some(&plan.plan_digest)).unwrap();
        assert!(!barrier(root.path()));
        assert!(all_match(
            root.path(),
            &plan,
            recovered.phase != "committed"
        ));
    }
    assert!(
        cuts >= 6,
        "all publication/commit/cleanup boundaries exercised"
    );
}

#[test]
fn interrupted_restore_resumes_with_source_grant_until_last_file() {
    for boundary in 0..5 {
        let (root, plan, host) = fixture();
        apply(root.path(), &plan, &plan.plan_digest, &host).unwrap();
        STOP.with(|stop| stop.set(Some(boundary)));
        let result = rollback(root.path(), &plan.plan_digest, Some(&plan.plan_digest));
        STOP.with(|stop| stop.set(None));
        if result.is_err() {
            recovery(root.path(), Some(&plan.plan_digest)).unwrap();
        }
        assert!(all_match(root.path(), &plan, true));
        assert!(!barrier(root.path()));
    }
}

#[test]
fn recovery_of_a_grant_only_crash_survives_another_interruption() {
    let (root, plan, host) = fixture();
    STOP.with(|stop| stop.set(Some(0)));
    assert!(apply(root.path(), &plan, &plan.plan_digest, &host).is_err());
    assert!(read(root.path(), PENDING).unwrap().is_none());
    STOP.with(|stop| stop.set(Some(0)));
    assert!(recovery(root.path(), Some(&plan.plan_digest)).is_err());
    assert!(all_match(root.path(), &plan, true));
    assert!(
        barrier(root.path()),
        "cache cleanup remains discoverable after source grant removal"
    );
    recovery(root.path(), Some(&plan.plan_digest)).unwrap();
    assert!(!barrier(root.path()));
    assert!(all_match(root.path(), &plan, true));
}

#[test]
fn changed_journal_or_snapshot_cannot_authorize_restore() {
    for journal_attack in [true, false] {
        let (root, plan, host) = fixture();
        STOP.with(|stop| stop.set(Some(2)));
        assert!(apply(root.path(), &plan, &plan.plan_digest, &host).is_err());
        let protected = read(root.path(), STATE).unwrap();
        if journal_attack {
            let path = journal_path(&plan.plan_digest).unwrap();
            let mut journal: Journal = decode(&required(root.path(), &path).unwrap()).unwrap();
            journal.verification.runner_digest = mg::digest(b"forged");
            mg::fs::write_file(root.path(), &path, &bytes(&journal)).unwrap();
        } else {
            let pin = plan
                .inputs
                .iter()
                .find_map(|row| row.digest.as_ref())
                .unwrap();
            mg::fs::write_file(root.path(), &object_path(pin).unwrap(), b"corrupt").unwrap();
        }
        assert!(recovery(root.path(), Some(&plan.plan_digest)).is_err());
        assert_eq!(read(root.path(), STATE).unwrap(), protected);
        assert!(barrier(root.path()));
    }
}

#[test]
fn cache_only_marker_never_grants_restoration_writes() {
    let (root, plan, host) = fixture();
    apply(root.path(), &plan, &plan.plan_digest, &host).unwrap();
    let mut journal = load_journal(root.path(), &plan.plan_digest).unwrap();
    journal.phase = "restoring".into();
    mg::fs::write_file(
        root.path(),
        &journal_path(&plan.plan_digest).unwrap(),
        &bytes(&journal),
    )
    .unwrap();
    pending_marker(root.path(), &plan.plan_digest).unwrap();
    assert!(recovery(root.path(), Some(&plan.plan_digest)).is_err());
    assert!(all_match(root.path(), &plan, false));
}

#[test]
fn self_consistent_forged_before_images_cannot_match_live_canonical_intent() {
    let (root, original, host) = fixture();
    let receipt = apply(root.path(), &original, &original.plan_digest, &host).unwrap();
    let mut files = std::collections::BTreeMap::new();
    for pin in &original.inputs {
        files.insert(
            pin.path.clone(),
            pin.digest
                .as_deref()
                .map(|pin| object(root.path(), pin).unwrap()),
        );
    }
    files.insert(
        "runner.mjs".into(),
        Some(b"// fabricated baseline\n".to_vec()),
    );
    let baseline = verification::snapshot(&files).unwrap();
    let forged = plan(baseline.path(), original.request.clone()).unwrap();
    assert_ne!(
        forged
            .edits
            .iter()
            .find(|edit| edit.path == STATE)
            .unwrap()
            .after,
        original
            .edits
            .iter()
            .find(|edit| edit.path == STATE)
            .unwrap()
            .after,
        "canonical intent binds the preimage closure"
    );
    for value in files.values().flatten() {
        retain(root.path(), value).unwrap();
    }
    let mut measurement = receipt.verification.clone();
    measurement.plan_digest = forged.plan_digest.clone();
    let journal = Journal {
        schema_version: format!("lekalo/ownership-journal/v{VERSION}"),
        phase: "publishing".into(),
        plan: forged.clone(),
        verification: measurement.clone(),
    };
    mg::fs::write_file(
        root.path(),
        &journal_path(&forged.plan_digest).unwrap(),
        &bytes(&journal),
    )
    .unwrap();
    let mut fake_receipt = receipt;
    fake_receipt.plan_digest = forged.plan_digest.clone();
    fake_receipt.rollback = forged.plan_digest.clone();
    fake_receipt.files = forged
        .edits
        .iter()
        .map(|edit| Pin {
            path: edit.path.clone(),
            digest: edit.after.clone(),
        })
        .collect();
    fake_receipt.verification = measurement;
    mg::fs::write_file(
        root.path(),
        &receipt_path(&forged.plan_digest).unwrap(),
        &bytes(&fake_receipt),
    )
    .unwrap();
    assert!(
        load_journal(root.path(), &forged.plan_digest).is_ok(),
        "attack is internally re-derivable"
    );
    assert!(rollback(root.path(), &forged.plan_digest, Some(&forged.plan_digest)).is_err());
    assert!(all_match(root.path(), &original, false));
    assert_eq!(
        required(root.path(), "runner.mjs").unwrap(),
        b"// simulated host\n"
    );
}

#[test]
fn apply_reexecutes_and_exact_rollback_allows_reapply_with_receipt_lineage() {
    let (root, plan, host) = fixture();
    verify(root.path(), &plan, &host).unwrap();
    apply(root.path(), &plan, &plan.plan_digest, &host).unwrap();
    assert_eq!(
        host.calls.get(),
        4,
        "caller verification is not publication authority"
    );
    rollback(root.path(), &plan.plan_digest, Some(&plan.plan_digest)).unwrap();
    let receipt = apply(root.path(), &plan, &plan.plan_digest, &host).unwrap();
    assert_eq!(host.calls.get(), 6);
    let previous: Receipt =
        decode(&object(root.path(), receipt.previous_receipt.as_deref().unwrap()).unwrap())
            .unwrap();
    assert_eq!(previous.state, "rolled-back");
    assert!(previous.previous_receipt.is_some());
}

#[test]
fn consumed_source_drift_invalidates_replay_and_current_trace() {
    let (root, plan, host) = fixture();
    apply(root.path(), &plan, &plan.plan_digest, &host).unwrap();
    mg::fs::write_file(root.path(), "src/complete_task.mjs", b"user edit\n").unwrap();
    assert!(apply(root.path(), &plan, &plan.plan_digest, &host).is_err());
    assert!(!trace(root.path(), &plan.plan_digest).unwrap().current);
    assert_eq!(host.calls.get(), 2, "stale replay refuses before execution");
    assert_eq!(
        required(root.path(), "src/complete_task.mjs").unwrap(),
        b"user edit\n"
    );
}
