use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Sandbox {
    root: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let root = std::env::current_dir()
            .unwrap()
            .join("target/fix-core")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fix/project"),
            &root,
        );
        Self { root }
    }
    fn project(&self) -> String {
        self.root
            .strip_prefix(std::env::current_dir().unwrap())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }
    fn prepare(&self) -> PreparedFix {
        prepare(&PrepareOptions {
            project: Some(&self.project()),
            selection: Selection::Safe,
            policy: None,
            allow_unsafe: false,
            allow_breaking: false,
        })
        .unwrap()
    }
    fn sources(&self) -> BTreeMap<String, Vec<u8>> {
        fn walk(path: &Path, root: &Path, rows: &mut BTreeMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, rows);
                } else {
                    rows.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                        std::fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut rows = BTreeMap::new();
        walk(&self.root.join("lekalo"), &self.root, &mut rows);
        rows
    }
    fn pin(&self) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(".lekalo/change-history-pin.json")).ok()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn consent<'a>(id: &'a str) -> Confirmation<'a> {
    Confirmation {
        expect_plan: Some(id),
        allow_unsafe: false,
        allow_breaking: false,
        safe_only: true,
    }
}
fn golden(name: &str, value: Json) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fix/golden")
        .join(name);
    let expected: Json = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value, expected);
}

#[test]
fn safe_preview_commit_reapply_and_audit_golden() {
    let sandbox = Sandbox::new();
    let before = sandbox.sources();
    let prepared = sandbox.prepare();
    assert_eq!(
        prepared.preview().unwrap(),
        sandbox.prepare().preview().unwrap()
    );
    assert_eq!(sandbox.sources(), before, "preview is read-only");
    assert!(!sandbox.root.join(".lekalo").exists());
    assert!(!prepared.is_no_op());
    let id = prepared.plan_id();
    let receipt = apply(&prepared, &consent(&id)).unwrap().unwrap();
    let after = sandbox.sources();
    let pin = sandbox.pin();
    let replay = apply(&prepared, &consent(&id)).unwrap().unwrap();
    assert_eq!(sandbox.sources(), after);
    assert_eq!(sandbox.pin(), pin);
    let history = change::receipt::History::decode(
        &std::fs::read(sandbox.root.join(change::version::HISTORY_LOGICAL_PATH)).unwrap(),
    )
    .unwrap();
    assert!(history.records[0].receipt["operations"][0]["detail"]
        .as_str()
        .unwrap()
        .contains("core.canonical-format@1.0.0;sha256:"));
    assert!(sandbox.prepare().is_no_op(), "re-discovery is idempotent");
    golden(
        "safe.json",
        json!({"state": receipt.state, "class": "safe", "semanticEqual": prepared.change.semantic_diff.equal, "sourceChanged": before != after, "reapplyState": replay.state, "auditRecords": history.records.len(), "postValidation": "passed"}),
    );
}

#[test]
fn safe_option_intent_is_sealed_and_same_command_replays_without_writes() {
    let policy = br#"{"schemaVersion":"lekalo/change-policy/v0.8.0","identity":"dev.lekalo.change-policy@0.8.0","allowBreakingClasses":[]}"#;
    let changed_policy = [policy.as_slice(), b"\n"].concat();
    for (policy, allow_breaking) in [
        (Some(policy.as_slice()), false),
        (None, true),
        (Some(policy.as_slice()), true),
    ] {
        let sandbox = Sandbox::new();
        let project = sandbox.project();
        let options = PrepareOptions {
            project: Some(&project),
            selection: Selection::Diagnostic("LEK-FORMAT-001"),
            policy,
            allow_unsafe: false,
            allow_breaking,
        };
        let prepared = prepare(&options).unwrap();
        assert_eq!(prepared.change.policy_digest(), policy.map(digest));
        assert_eq!(prepared.change.allow_breaking, allow_breaking);
        let id = prepared.plan_id();
        let confirmation = Confirmation {
            expect_plan: Some(&id),
            allow_unsafe: false,
            allow_breaking,
            safe_only: false,
        };
        let mismatch = Confirmation {
            allow_breaking: !allow_breaking,
            ..confirmation.clone()
        };
        assert!(matches!(
            apply(&prepared, &mismatch),
            Err(FixError::Rule {
                id: "fix.conflict",
                ..
            })
        ));
        let committed = run(&options, Some(&confirmation));
        assert_eq!(committed.status(), Status::Valid);
        let committed: Json = serde_json::from_str(&committed.to_json_string()).unwrap();
        assert_eq!(committed["state"], "committed");
        assert_eq!(
            committed["receipt"]["policy"]["policyDigest"],
            json!(policy.map(digest))
        );
        assert_eq!(
            committed["receipt"]["policy"]["allowBreaking"],
            allow_breaking
        );
        let after = sandbox.sources();
        let pin = sandbox.pin();
        let replay = run(&options, Some(&confirmation));
        assert_eq!(replay.status(), Status::Valid);
        let replay: Json = serde_json::from_str(&replay.to_json_string()).unwrap();
        assert_eq!(replay["state"], "replayed");
        assert_eq!(replay["receipt"]["replayed"], true);
        assert_eq!(sandbox.sources(), after);
        assert_eq!(sandbox.pin(), pin);
        for (policy, allow_breaking) in [
            (Some(changed_policy.as_slice()), allow_breaking),
            (policy, !allow_breaking),
        ] {
            let changed_options = PrepareOptions {
                policy,
                allow_breaking,
                ..options.clone()
            };
            let changed_confirmation = Confirmation {
                allow_breaking,
                ..confirmation.clone()
            };
            assert_eq!(
                run(&changed_options, Some(&changed_confirmation)).diagnostics()[0].id(),
                "fix.conflict"
            );
            assert_eq!(sandbox.sources(), after);
            assert_eq!(sandbox.pin(), pin);
        }
    }
}

#[test]
fn safe_options_refuse_undeclared_acknowledgements_and_invalid_policies() {
    let sandbox = Sandbox::new();
    let project = sandbox.project();
    let options = PrepareOptions {
        project: Some(&project),
        selection: Selection::Diagnostic("LEK-FORMAT-001"),
        policy: None,
        allow_unsafe: true,
        allow_breaking: false,
    };
    let before = sandbox.sources();
    assert!(matches!(
        prepare(&options),
        Err(FixError::Rule {
            id: "fix.class-denied",
            ..
        })
    ));
    assert_eq!(
        run(&options, None).diagnostics()[0].id(),
        "fix.class-denied"
    );
    let invalid_policy = br#"{"schemaVersion":"lekalo/change-policy/v0.8.0","identity":"dev.lekalo.change-policy@0.8.0","force":true}"#;
    let invalid = PrepareOptions {
        policy: Some(invalid_policy),
        allow_unsafe: false,
        ..options
    };
    assert!(matches!(
        prepare(&invalid),
        Err(FixError::Change(ChangeError::PolicyInvalid {
            detail: "unknown-member"
        }))
    ));
    assert_eq!(run(&invalid, None).status(), Status::Invalid);
    assert_eq!(sandbox.sources(), before);
    assert!(!sandbox.root.join(".lekalo").exists());
    let prepared = sandbox.prepare();
    let id = prepared.plan_id();
    apply(&prepared, &consent(&id)).unwrap();
    let after = sandbox.sources();
    let pin = sandbox.pin();
    assert_eq!(run(&invalid, Some(&consent(&id))).status(), Status::Invalid);
    let undeclared = Confirmation {
        allow_unsafe: true,
        ..consent(&id)
    };
    assert_eq!(
        apply(&prepared, &undeclared)
            .unwrap_err()
            .into_result()
            .diagnostics()[0]
            .id(),
        "fix.class-denied"
    );
    assert_eq!(sandbox.sources(), after);
    assert_eq!(sandbox.pin(), pin);
}

#[test]
fn stale_fingerprint_blocks_both_imported_and_sealed_fixes_golden() {
    let sandbox = Sandbox::new();
    let prepared = sandbox.prepare();
    let recipe = serde_json::to_vec(prepared.recipe()).unwrap();
    let path = sandbox.root.join("lekalo/project.yaml");
    let mut changed = std::fs::read(&path).unwrap();
    changed.extend_from_slice(b"\n");
    std::fs::write(&path, changed).unwrap();
    let current = sandbox.sources();
    let error = prepare(&PrepareOptions {
        project: Some(&sandbox.project()),
        selection: Selection::Recipe(&recipe),
        policy: None,
        allow_unsafe: false,
        allow_breaking: false,
    })
    .unwrap_err()
    .into_result();
    assert_eq!(error.diagnostics()[0].id(), "fix.precondition-failed");
    let id = prepared.plan_id();
    let error = apply(&prepared, &consent(&id)).unwrap_err().into_result();
    golden(
        "stale.json",
        json!({"reasonCodes": error.reason_codes().iter().map(|r| r.as_str()).collect::<Vec<_>>(), "sourcePreserved": sandbox.sources() == current, "auditAbsent": !sandbox.root.join(change::version::HISTORY_LOGICAL_PATH).exists()}),
    );
    // No-op plans must also recheck the source snapshot, never silently succeed.
    let fresh = sandbox.prepare();
    let id = fresh.plan_id();
    apply(&fresh, &consent(&id)).unwrap();
    let noop = sandbox.prepare();
    assert!(noop.is_no_op());
    let mut changed = std::fs::read(&path).unwrap();
    changed.extend_from_slice(b"\n");
    std::fs::write(&path, changed).unwrap();
    assert!(apply(&noop, &consent(&noop.plan_id())).is_err());
}

#[test]
fn conflict_and_confirmation_are_read_only_golden() {
    let sandbox = Sandbox::new();
    let prepared = sandbox.prepare();
    let before = sandbox.sources();
    let error = apply(&prepared, &consent(&format!("sha256:{}", "0".repeat(64))))
        .unwrap_err()
        .into_result();
    golden(
        "conflict.json",
        json!({"reasonCodes": error.reason_codes().iter().map(|r| r.as_str()).collect::<Vec<_>>(), "sourcePreserved": sandbox.sources() == before, "auditAbsent": !sandbox.root.join(change::version::HISTORY_LOGICAL_PATH).exists()}),
    );
    let mut missing = consent("");
    missing.expect_plan = None;
    assert_eq!(
        apply(&prepared, &missing)
            .unwrap_err()
            .into_result()
            .diagnostics()[0]
            .id(),
        "fix.confirmation-required"
    );
}

#[test]
fn failed_post_validation_rolls_back_sources_history_and_custody_golden() {
    let sandbox = Sandbox::new();
    // Establish genuine nonempty custody/history, then introduce a new format
    // opportunity. Rollback must restore existing ledger bytes, not delete it.
    let first = sandbox.prepare();
    apply(&first, &consent(&first.plan_id())).unwrap();
    let path = sandbox.root.join("lekalo/project.yaml");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(b"\n");
    std::fs::write(path, bytes).unwrap();
    let prepared = sandbox.prepare();
    let before = sandbox.sources();
    let pin = sandbox.pin();
    let history = std::fs::read(sandbox.root.join(change::version::HISTORY_LOGICAL_PATH)).unwrap();
    let id = prepared.plan_id();
    let error = change::apply_with_validation(
        &prepared.change,
        &change::ApplyOptions {
            idempotency_key: "rollback-probe",
            expect_plan: Some(&id),
            allow_breaking: false,
        },
        &|load| {
            assert!(
                crate::format::fix_candidates(load).unwrap().is_empty(),
                "real replacements happened before oracle"
            );
            assert_ne!(sandbox.sources(), before);
            Err(ChangeError::RequestInvalid {
                detail: "forced-post-validation",
            })
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ChangeError::CommitFailed {
            detail: "post-validation",
            ..
        }
    ));
    golden(
        "rollback.json",
        json!({"state": "rolled-back", "sourceRestored": sandbox.sources() == before, "historyRestored": std::fs::read(sandbox.root.join(change::version::HISTORY_LOGICAL_PATH)).unwrap() == history, "custodyRestored": sandbox.pin() == pin, "pending": change::pending_transactions(Some(&sandbox.project())).unwrap().len()}),
    );
    assert!(crate::loader::load_with_snapshot(&LoadSelection {
        project: Some(sandbox.project())
    })
    .is_ok());
}

fn external_recipe() -> Json {
    let mut recipe = serde_json::to_value(canonical_recipe()).unwrap();
    recipe["id"] = json!("adapter.node-typescript.add-field");
    recipe["owner"] = json!("adapter.node-typescript");
    recipe["target"] = json!("node-typescript");
    recipe["class"] = json!("breaking");
    recipe["diagnosticCodes"] = json!(["LEK-SEM-009"]);
    recipe["postValidation"] = json!("validate");
    recipe["scope"] = json!({"operations": ["entity-field:add"], "subjects": ["tasks.task"]});
    recipe["operation"] = json!({"kind": "model-change", "request": {"schemaVersion": "lekalo/change-request/v0.8.0", "operations": [{"operation": "entity-field:add", "entity": "tasks.task", "field": {"name": "extra", "type": {"ref": "tasks.text"}}}]}});
    recipe
}
#[test]
fn adapter_semantic_fix_repairs_its_origin_and_replays_with_version_custody() {
    let sandbox = Sandbox::new();
    let path = sandbox.root.join("lekalo/modules/tasks/entities.yaml");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, original.replace("      - title", "      - extra")).unwrap();
    let load = crate::loader::load_with_snapshot(&LoadSelection {
        project: Some(sandbox.project()),
    })
    .unwrap();
    assert!(validation_codes(&load)
        .unwrap()
        .contains(&"LEK-SEM-009".to_owned()));
    let mut recipe = external_recipe();
    recipe["preconditions"]["sourceFingerprint"] = json!(fingerprint(&load));
    let bytes = serde_json::to_vec(&recipe).unwrap();
    let project = sandbox.project();
    let options = PrepareOptions {
        project: Some(&project),
        selection: Selection::Recipe(&bytes),
        policy: None,
        allow_unsafe: false,
        allow_breaking: true,
    };
    let prepared = prepare(&options).unwrap();
    let id = prepared.plan_id();
    assert!(!prepared.change.semantic_diff.equal);
    assert_eq!(
        apply(&prepared, &consent(&id))
            .unwrap_err()
            .into_result()
            .diagnostics()[0]
            .id(),
        "fix.class-denied"
    );
    let confirmation = Confirmation {
        expect_plan: Some(&id),
        allow_unsafe: false,
        allow_breaking: true,
        safe_only: false,
    };
    let receipt = apply(&prepared, &confirmation).unwrap().unwrap();
    assert_eq!(receipt.state, "committed");
    assert_eq!(receipt.operations[0].operation, "entity-field:add");
    assert_eq!(receipt.operations[1].operation, "fix:apply");
    let load = crate::loader::load_with_snapshot(&LoadSelection {
        project: Some(sandbox.project()),
    })
    .unwrap();
    assert!(!validation_codes(&load)
        .unwrap()
        .contains(&"LEK-SEM-009".to_owned()));
    let after = sandbox.sources();
    let pin = sandbox.pin();
    let replay = run(&options, Some(&confirmation));
    assert_eq!(replay.status(), Status::Valid);
    assert_eq!(
        serde_json::from_str::<Json>(&replay.to_json_string()).unwrap()["state"],
        "replayed"
    );
    assert_eq!(sandbox.sources(), after);
    assert_eq!(sandbox.pin(), pin);
    recipe["version"] = json!("2.0.0");
    let changed = serde_json::to_vec(&recipe).unwrap();
    let changed_options = PrepareOptions {
        selection: Selection::Recipe(&changed),
        ..options
    };
    assert_eq!(
        run(&changed_options, Some(&confirmation)).diagnostics()[0].id(),
        "fix.conflict"
    );
}

#[test]
fn adapter_namespace_and_exact_semantic_scope_cannot_be_bypassed() {
    let recipe = external_recipe();
    Recipe::decode(&serde_json::to_vec(&recipe).unwrap()).unwrap();
    for (pointer, value) in [
        ("/id", json!("core.fake")),
        ("/target", json!("php-laravel")),
        ("/scope/operations/0", json!("entity-field:remove")),
        ("/scope/subjects/0", json!("tasks.foreign")),
        (
            "/operation/request/operations/0/entity",
            json!("tasks.foreign"),
        ),
        ("/class", json!("safe")),
    ] {
        let mut bad = recipe.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            Recipe::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut raw = recipe.clone();
    raw["operation"] = json!({"kind": "raw-patch", "path": "lekalo/project.yaml", "text": "x"});
    assert!(Recipe::decode(&serde_json::to_vec(&raw).unwrap()).is_err());
}

#[test]
fn unsafe_breaking_and_manual_only_never_enter_bulk_safe() {
    let sandbox = Sandbox::new();
    let mut prepared = sandbox.prepare();
    for class in [Class::Unsafe, Class::Breaking, Class::ManualOnly] {
        prepared.recipe.class = class;
        prepared.change.allow_breaking = class == Class::Breaking;
        let id = prepared.plan_id();
        assert!(confirm(&prepared, &consent(&id)).is_err());
        let explicit = Confirmation {
            expect_plan: Some(&id),
            allow_unsafe: class == Class::Unsafe,
            allow_breaking: class == Class::Breaking,
            safe_only: false,
        };
        assert_eq!(
            confirm(&prepared, &explicit).is_ok(),
            class != Class::ManualOnly
        );
    }
    let mut manual = external_recipe();
    manual["class"] = json!("manual-only");
    manual["operation"] = json!({"kind": "manual-only"});
    manual["postValidation"] = json!("manual-only");
    manual["scope"] = json!({"operations": [], "subjects": []});
    let bytes = serde_json::to_vec(&manual).unwrap();
    let options = PrepareOptions {
        project: Some("does-not-exist"),
        selection: Selection::Recipe(&bytes),
        policy: None,
        allow_unsafe: false,
        allow_breaking: true,
    };
    assert_eq!(run(&options, None).status(), Status::Valid);
    assert_eq!(
        run(&options, Some(&consent("none"))).diagnostics()[0].id(),
        "fix.manual-only"
    );
    assert!(!sandbox.root.join(".lekalo").exists());
}

#[test]
fn recipe_decoding_is_closed_versioned_bounded_and_duplicate_refusing() {
    let bytes = serde_json::to_vec(&canonical_recipe()).unwrap();
    Recipe::decode(&bytes).unwrap();
    for bad in [
        b"{}".to_vec(),
        b"{\"id\":\"x\",\"id\":\"y\"}".to_vec(),
        vec![b' '; MAX_RECIPE_BYTES + 1],
    ] {
        assert!(Recipe::decode(&bad).is_err());
    }
    let original: Json = serde_json::from_slice(&bytes).unwrap();
    for (key, value) in [
        ("version", json!("2.0.0")),
        ("schemaVersion", json!("lekalo/fix-recipe/v9")),
        ("patch", json!("raw")),
        ("diagnosticCodes", json!(["LEK-REF-014"])),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        assert!(
            Recipe::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{key}"
        );
    }
    let result = run(
        &PrepareOptions {
            project: None,
            selection: Selection::Diagnostic("LEK-REF-014"),
            policy: None,
            allow_unsafe: false,
            allow_breaking: false,
        },
        None,
    );
    assert_eq!(result.diagnostics()[0].id(), "fix.recipe-unavailable");
}
