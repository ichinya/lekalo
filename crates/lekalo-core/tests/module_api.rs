//! Issue #113 core integration tests: the pure module API boundary
//! surface over the committed fixture corpus. Every test is offline and
//! deterministic: loader -> compile -> module_api, no cache, no targets.

use lekalo_core::loader::LoadSelection;
use lekalo_core::module_api::{
    check, consumer_snapshot, export_public, plan_consumer_context, resolve, resolve_lock,
    FindingClass,
};
use lekalo_core::result::Status;

/// Load and compile one fixture project (pure; no cache involvement).
fn compile(project: &str) -> lekalo_core::ir::Compilation {
    let model = lekalo_core::loader::normalize_model(&LoadSelection {
        project: Some(project.to_owned()),
    })
    .expect("fixture project loads");
    lekalo_core::ir::compile(&model).expect("fixture project compiles")
}

fn attachment(project: &str) -> Vec<u8> {
    std::fs::read(format!("{project}/module-api.json")).expect("attachment fixture")
}

const REFERENCE: &str = "tests/fixtures/module-api/reference/tasks";

#[test]
fn reference_boundary_is_advisory_pass_and_complete() {
    let compilation = compile(REFERENCE);
    let verdict = check(&attachment(REFERENCE), &compilation, false, None).expect("check runs");
    assert_eq!(verdict.report.assessment, "pass");
    assert!(verdict.report.complete);
    assert!(verdict.evidence.is_empty());
    assert_eq!(verdict.class, FindingClass::Boundary);
    // Strict mode on the same clean project stays pass.
    let strict = check(&attachment(REFERENCE), &compilation, true, None).expect("strict runs");
    assert_eq!(strict.report.assessment, "pass");
}

#[test]
fn visibility_probes_reject_unexported_cross_module_references() {
    let visibility = "tests/fixtures/module-api/visibility";
    let compilation = compile(visibility);
    let verdict = check(&attachment(visibility), &compilation, true, None).expect("check runs");
    assert_eq!(verdict.report.assessment, "fail");
    let rules: Vec<&str> = verdict.evidence.iter().map(|e| e.rule).collect();
    assert!(rules.contains(&"module-api.symbol-not-exported"));
    assert!(rules.contains(&"module-api.public-private-exposure"));
    // The exact reported symbol is the unexported one; the module-private
    // planner helper is never implicated.
    let exported: Vec<&str> = verdict
        .evidence
        .iter()
        .filter_map(|e| e.export.as_deref())
        .collect();
    assert!(exported.contains(&"tasks.task_state"));
    assert!(verdict
        .evidence
        .iter()
        .all(|e| !e.subject.contains("planner.internal_note")));
}

#[test]
fn visibility_module_filter_scopes_owned_findings() {
    let visibility = "tests/fixtures/module-api/visibility";
    let compilation = compile(visibility);
    let scoped = check(&attachment(visibility), &compilation, false, Some("tasks"))
        .expect("scoped check runs");
    assert!(scoped
        .evidence
        .iter()
        .all(|e| e.producer.as_deref() != Some("planner")));
}

#[test]
fn cross_owner_writes_need_exact_grant_scope() {
    let writes = "tests/fixtures/module-api/writes";
    let compilation = compile(writes);
    let bytes = attachment(writes);
    let verdict = check(&bytes, &compilation, true, None).expect("check runs");
    let write_findings: Vec<_> = verdict
        .evidence
        .iter()
        .filter(|e| e.rule == "module-api.cross-module-write-forbidden")
        .collect();
    assert_eq!(write_findings.len(), 1);
    assert_eq!(write_findings[0].consumer.as_deref(), Some("planner"));
    assert_eq!(write_findings[0].producer.as_deref(), Some("tasks"));
    assert!(write_findings[0].path[0].starts_with("operation:planner.edit_task_cmd|"));

    // The exact narrow grant (update, entity-wide) admits the write.
    let mut narrow_attachment = attachment(writes);
    let mut grant: serde_json::Value =
        serde_json::from_slice(&narrow_attachment).expect("attachment json");
    grant["modules"][0]["grants"] = serde_json::json!([{
        "grant": "planner-narrow-update",
        "consumer": "planner",
        "aggregate": "tasks.task",
        "operations": ["update"],
        "fields": ["*"],
        "rationale": "planner owns the scheduling state",
    }]);
    narrow_attachment = serde_json::to_vec(&grant).expect("grant serializes");
    let narrowed = check(&narrow_attachment, &compilation, true, None).expect("check runs");
    assert_eq!(narrowed.report.assessment, "pass");

    // A field-only grant never covers an entity-wide write.
    let mut field_attachment = attachment(writes);
    let mut field: serde_json::Value =
        serde_json::from_slice(&field_attachment).expect("attachment json");
    field["modules"][0]["grants"] = serde_json::json!([{
        "grant": "planner-narrow-update",
        "consumer": "planner",
        "aggregate": "tasks.task",
        "operations": ["update"],
        "fields": ["title"],
        "rationale": "planner owns the scheduling state",
    }]);
    field_attachment = serde_json::to_vec(&field).expect("field serializes");
    let bypass = check(&field_attachment, &compilation, true, None).expect("check runs");
    assert!(bypass
        .evidence
        .iter()
        .any(|e| e.rule == "module-api.cross-module-write-forbidden"));
}

#[test]
fn portability_refuses_target_specific_public_closure() {
    let portability = "tests/fixtures/module-api/portability";
    let compilation = compile(portability);
    let verdict = check(&attachment(portability), &compilation, true, None).expect("check runs");
    assert!(verdict
        .evidence
        .iter()
        .any(|e| e.rule == "module-api.public-target-exposure"));
    // The projection itself is structural; the certified-bundle refusal is
    // the command layer's boundary gate (see the CLI integration tests).
}

#[test]
fn certified_export_is_stable_and_closed() {
    let compilation = compile(REFERENCE);
    let resolved = resolve(&attachment(REFERENCE), &compilation).expect("resolves");
    let bundle = export_public(&resolved, &compilation, "tasks").expect("bundle");
    assert_eq!(bundle.api_version.as_deref(), Some("1.2.0"));
    assert_eq!(bundle.exports.len(), 9);
    assert!(bundle.exports.iter().all(|e| e.export != "tasks.task"));
    // Deterministic digest: an equivalent resolve/export round-trips.
    let again = export_public(
        &resolve(&attachment(REFERENCE), &compilation).expect("resolves"),
        &compilation,
        "tasks",
    )
    .expect("bundle");
    assert_eq!(bundle.digest, again.digest);
    // The digest excludes its own digest member: re-parsing and
    // re-digesting the canonical projection is stable.
    let canonical = lekalo_core::module_api::wire::canonical(&bundle);
    assert!(canonical.contains(&format!("\"digest\":\"{}\"", bundle.digest)));
}

#[test]
fn attachment_decode_rejects_hostile_shapes() {
    let compilation = compile(REFERENCE);
    let good = attachment(REFERENCE);
    let text = String::from_utf8(good.clone()).expect("utf8");
    // Duplicate JSON keys are rejected before typed decoding.
    let duplicated = text.replace(
        "\"schemaVersion\":",
        "\"schemaVersion\": \"lekalo/module-api/v0.8.0\", \"schemaVersion\":",
    );
    assert!(resolve(duplicated.as_bytes(), &compilation).is_err());
    // Unknown family versions fail closed as unsupported.
    let future = text.replace("v0.8.0", "v9.9.9");
    let error = resolve(future.as_bytes(), &compilation).expect_err("unsupported");
    assert!(error
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.version-unsupported"));
    // Contradictory pins: an export naming a foreign module.
    let foreign = text.replace(
        "\"export\": \"tasks.task_id\"",
        "\"export\": \"planner.focus\"",
    );
    assert!(resolve(foreign.as_bytes(), &compilation).is_err());
}

#[test]
fn consumer_snapshot_joins_declared_and_resolved_uses() {
    let compilation = compile(REFERENCE);
    let resolved = resolve(&attachment(REFERENCE), &compilation).expect("resolves");
    let snapshot = consumer_snapshot(&resolved, &compilation, "tasks").expect("snapshot");
    let planner = snapshot
        .consumers
        .iter()
        .find(|c| c.consumer == "planner")
        .expect("planner row");
    assert!(planner.declared);
    assert!(planner
        .uses
        .iter()
        .any(|u| u.origin == "declared" && u.export == "tasks.task_view"));
    assert!(planner.uses.iter().any(|u| u.origin == "resolved"));
    // External completeness stays honestly unknown.
    assert!(!snapshot.complete);
}

#[test]
fn consumer_locks_detect_digest_and_dependency_changes() {
    let compilation = compile(REFERENCE);
    let resolved = resolve(&attachment(REFERENCE), &compilation).expect("resolves");
    let lock = resolve_lock(&resolved, &compilation, "tasks", "planner").expect("lock");
    assert_eq!(lock.consumer, "planner");
    assert!(!lock.digest.is_empty());
    // The same resolution is fresh.
    assert!(lekalo_core::module_api::check_lock(&lock, &lock).is_empty());
    // A changed digest is stale; the evidence carries expected/actual.
    let mut stale = lock.clone();
    stale.digest =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
    let evidence = lekalo_core::module_api::check_lock(&lock, &stale);
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].rule, "module-api.lock-stale");
    assert_eq!(evidence[0].detail, "lock-digest");
}

#[test]
fn public_diff_classifies_breaking_changes_and_affected_consumers() {
    let before = std::fs::read("tests/fixtures/module-api/diff/golden-bundle-before.json")
        .expect("before bundle");
    let after = std::fs::read("tests/fixtures/module-api/diff/golden-bundle-after.json")
        .expect("after bundle");
    let consumers_before =
        std::fs::read("tests/fixtures/module-api/diff/golden-consumers-before.json")
            .expect("before snapshot");
    let consumers_after =
        std::fs::read("tests/fixtures/module-api/diff/golden-consumers-after.json")
            .expect("after snapshot");
    let (report, evidence) = lekalo_core::module_api::compare_public(
        &before,
        &after,
        Some(&consumers_before),
        Some(&consumers_after),
    )
    .expect("diff runs");
    // The planner range accepts the candidate resolution, so the verdict
    // is the source break itself; strict denial rides the version-policy
    // violation instead of a phantom consumer incompatibility.
    assert_eq!(report.verdict, "breaking");
    // The exact breaking change.
    let removed = report
        .changes
        .iter()
        .find(|c| c.reason == "field-removed")
        .expect("field removal");
    assert_eq!(removed.export, "tasks.task_view");
    assert_eq!(removed.member.as_deref(), Some("state"));
    assert_eq!(removed.class, "source-breaking");
    // The exact affected consumer with its policy, plus the unaffected
    // control.
    let planner = report
        .consumers
        .iter()
        .find(|c| c.consumer == "planner")
        .expect("planner");
    assert_eq!(planner.status, "affected");
    assert_eq!(planner.reason.as_deref(), Some("policy-accepted"));
    let provider = report
        .consumers
        .iter()
        .find(|c| c.consumer == "provider")
        .expect("provider");
    assert_eq!(provider.status, "unaffected");
    assert_eq!(report.coverage.known, 1);
    assert_eq!(report.version_policy.expected.as_deref(), Some("major"));
    assert!(evidence
        .iter()
        .any(|e| e.rule == "module-api.version-policy-violation"));
}

#[test]
fn public_context_needs_only_the_bundle() {
    let bundle = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json")
        .expect("standalone bundle");
    let capsule = plan_consumer_context(
        &bundle,
        "planner",
        &["tasks.task_view".to_owned(), "tasks.get_task".to_owned()],
        4000,
    )
    .expect("capsule");
    assert!(capsule.fits);
    assert_eq!(capsule.estimator.id, "module-context-estimator");
    assert!(capsule
        .exports
        .iter()
        .any(|e| e.export == "tasks.task_view"));
    // The tiny budget keeps the required facts and reports fits:false.
    let tiny = plan_consumer_context(
        &bundle,
        "planner",
        &["tasks.task_view".to_owned(), "tasks.get_task".to_owned()],
        16,
    )
    .expect("tiny capsule");
    assert!(!tiny.fits);
    assert_eq!(tiny.gaps, vec!["budget-exceeded".to_owned()]);
    assert!(tiny.required_cost > 16);
    // Deterministic: the same inputs produce the same digest.
    let again = plan_consumer_context(
        &bundle,
        "planner",
        &["tasks.task_view".to_owned(), "tasks.get_task".to_owned()],
        4000,
    )
    .expect("capsule");
    assert_eq!(capsule.digest, again.digest);
    // Exact selectors only: no short names, no duplicates, bounded budget.
    assert!(plan_consumer_context(&bundle, "planner", &["tasks.task".to_owned()], 4000).is_err());
    assert!(plan_consumer_context(
        &bundle,
        "planner",
        &["tasks.task_view".to_owned(), "tasks.task_view".to_owned()],
        4000
    )
    .is_err());
    assert!(plan_consumer_context(&bundle, "planner", &["tasks.task_view".to_owned()], 0).is_err());
}

#[test]
fn status_classes_follow_the_registered_rules() {
    // Invalid input projects to the invalid status class through the
    // diagnostic set; the boundary evidence stays registry-owned.
    let set = lekalo_core::module_api::diagnostic::invalid("probe");
    assert!(set
        .as_slice()
        .iter()
        .all(|diagnostic| diagnostic.code() == "LEK-MODULE-001"));
    let unsupported = lekalo_core::module_api::diagnostic::unsupported_version("v9");
    assert!(unsupported
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.version-unsupported"));
    let _ = Status::UnsupportedVersion;
}

#[test]
fn version_policy_evaluates_bumps_independently() {
    use lekalo_core::module_api::{ApiVersion, RequiredBump};
    let v = |text: &str| ApiVersion::parse(text).expect("version");
    // Stable majors: breaking demands major; satisfied major is compliant.
    assert_eq!(
        RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("2.0.0")), true, true, false),
        RequiredBump::Major
    );
    assert_eq!(
        RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("3.0.0")), true, true, false),
        RequiredBump::None
    );
    // Pre-1.0 pins minor-breaking behavior explicitly.
    assert_eq!(
        RequiredBump::evaluate(Some(&v("0.3.0")), Some(&v("0.3.0")), true, true, false),
        RequiredBump::Minor
    );
    // Identical bundles never demand a bump (finding 9).
    assert_eq!(
        RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.2.0")), false, false, false),
        RequiredBump::None
    );
    // Additive surface on stable majors needs a minor, not a patch (finding 11).
    assert_eq!(
        RequiredBump::evaluate(Some(&v("1.2.0")), Some(&v("1.2.1")), true, false, true),
        RequiredBump::Minor
    );
}

/// The r1 review probes: every fixed finding carries its own regression.

#[test]
fn module_private_symbols_never_enter_certified_bundles() {
    // Finding 1: an exported shape whose field reaches a module-private
    // symbol is an exposure finding, and the bundle refuses.
    let leak = "tests/fixtures/module-api/leak";
    let compilation = compile(leak);
    let bytes = attachment(leak);
    let strict = check(&bytes, &compilation, true, None).expect("check runs");
    let exposure = strict
        .evidence
        .iter()
        .find(|e| e.rule == "module-api.public-private-exposure")
        .expect("private closure exposure");
    assert_eq!(exposure.member.as_deref(), Some("tasks.secret"));
    assert_eq!(
        exposure.path.last().map(String::as_str),
        Some("tasks.secret")
    );
    let resolved = resolve(&bytes, &compilation).expect("resolves");
    assert!(export_public(&resolved, &compilation, "tasks").is_err());
}

#[test]
fn unattached_producers_cannot_vacate_the_boundary() {
    // Finding 2: dropping the producer from the attachment turns its
    // references into boundary findings, never a silent pass.
    let leak = "tests/fixtures/module-api/leak";
    let compilation = compile(leak);
    let mut value: serde_json::Value =
        serde_json::from_slice(&attachment(leak)).expect("attachment json");
    value["modules"]
        .as_array_mut()
        .unwrap()
        .retain(|module| module["moduleId"] != "tasks");
    let bytes = serde_json::to_vec(&value).expect("serializes");
    let verdict = check(&bytes, &compilation, true, None).expect("check runs");
    let finding = verdict
        .evidence
        .iter()
        .find(|e| e.rule == "module-api.symbol-not-exported")
        .expect("unattached producer finding");
    assert_eq!(finding.detail, "unattached-producer");
    assert_eq!(finding.producer.as_deref(), Some("tasks"));
}

#[test]
fn unknown_module_scopes_are_invalid() {
    // Finding 14: an unknown --module scope matches nothing and used to
    // pass silently; it is invalid input.
    let compilation = compile(REFERENCE);
    let error = match check(&attachment(REFERENCE), &compilation, false, Some("ghost")) {
        Err(set) => set,
        Ok(_) => panic!("an unknown --module scope must be invalid"),
    };
    assert!(error
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.input-invalid"));
}

#[test]
fn declared_kinds_and_import_uses_are_contradiction_checked() {
    // Finding 16: a mislabeled export kind and an unknown import use are
    // invalid input, not silent relabelings.
    let compilation = compile(REFERENCE);
    let good = attachment(REFERENCE);
    let mut mislabeled: serde_json::Value = serde_json::from_slice(&good).expect("json");
    for module in mislabeled["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "tasks" {
            for entry in module["exports"].as_array_mut().unwrap().iter_mut() {
                if entry["export"] == "tasks.task_view" {
                    entry["kind"] = "entity".into();
                }
            }
        }
    }
    let error = resolve(
        &serde_json::to_vec(&mislabeled).expect("serializes"),
        &compilation,
    )
    .expect_err("kind contradiction");
    assert!(error
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.input-invalid"));

    // An import use naming a non-export is invalid.
    let mut unknown_use: serde_json::Value = serde_json::from_slice(&good).expect("json");
    for module in unknown_use["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "planner" {
            module["imports"] = serde_json::json!([
                { "moduleId": "tasks", "uses": ["tasks.nonexistent"] }
            ]);
        }
    }
    let error = resolve(
        &serde_json::to_vec(&unknown_use).expect("serializes"),
        &compilation,
    )
    .expect_err("unknown import use");
    assert!(error
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.input-invalid"));
}

#[test]
fn declared_zero_use_consumers_stay_in_scope() {
    // Finding 17: an empty declared uses keeps subscriptions and range
    // policies in scope; it is no longer consumer-use-bound.
    let compilation = compile(REFERENCE);
    let mut value: serde_json::Value =
        serde_json::from_slice(&attachment(REFERENCE)).expect("json");
    for module in value["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "tasks" {
            module["consumers"][0]["uses"] = serde_json::json!([]);
        }
    }
    let bytes = serde_json::to_vec(&value).expect("serializes");
    let resolved = resolve(&bytes, &compilation).expect("zero-use consumer resolves");
    let snapshot = consumer_snapshot(&resolved, &compilation, "tasks").expect("snapshot");
    let planner = snapshot
        .consumers
        .iter()
        .find(|c| c.consumer == "planner")
        .expect("declared row retained");
    assert!(planner
        .uses
        .iter()
        .any(|use_row| use_row.origin == "resolved"));
}

#[test]
fn identical_bundles_diff_compatible() {
    // Finding 9: a no-op re-export demands no bump and never diffs as
    // breaking.
    let bundle =
        std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").expect("bundle");
    let (report, evidence) = lekalo_core::module_api::compare_public(&bundle, &bundle, None, None)
        .expect("identical diff runs");
    assert!(report.changes.is_empty());
    assert_eq!(report.verdict, "compatible");
    assert_eq!(report.version_policy.decision, "compliant");
    assert!(evidence.is_empty());
}

#[test]
fn snapshots_bind_every_pin() {
    // Finding 12: a snapshot minted for another project or revision is
    // invalid input, never a silent participant.
    let before =
        std::fs::read("tests/fixtures/module-api/diff/golden-bundle-before.json").expect("bundle");
    let snapshot =
        std::fs::read("tests/fixtures/module-api/diff/golden-consumers-before.json").expect("snap");
    let mut tampered: serde_json::Value = serde_json::from_slice(&snapshot).expect("json");
    tampered["pins"]["apiDigest"] = serde_json::Value::String(format!("sha256:{}", "0".repeat(64)));
    let bytes = serde_json::to_vec(&tampered).expect("serializes");
    let error =
        lekalo_core::module_api::compare_public(&before, &before, Some(&bytes), Some(&bytes))
            .expect_err("tampered snapshot");
    assert!(error
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.input-invalid"));
}

/// Seal one hand-authored bundle document with the digest domain the
/// projection uses (digest member and definition revisions excluded).
fn seal_bundle(value: &mut serde_json::Value) {
    value["digest"] = serde_json::Value::String(String::new());
    for export in value["exports"].as_array_mut().unwrap().iter_mut() {
        export["version"] = serde_json::Value::from(0);
        // The wire projection skips empty arrays and nulls; the digest
        // domain is computed over the decoded projection, so the sealed
        // spelling must drop them too.
        let empty_keys: Vec<String> = export
            .as_object()
            .expect("export object")
            .iter()
            .filter(|(_, v)| v.as_array().is_some_and(Vec::is_empty) || v.is_null())
            .map(|(k, _)| k.clone())
            .collect();
        for key in empty_keys {
            export.as_object_mut().unwrap().remove(&key);
        }
    }
    let canonical = lekalo_core::module_api::wire::canonical(&*value);
    let digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            format!("lekalo/module-public-contract/1\n{canonical}").as_bytes()
        )
    );
    value["digest"] = digest.into();
}

#[test]
fn dependency_changes_propagate_to_transitive_consumers() {
    // Finding 19: a consumer of a wrapper export is affected when a
    // transitive public dependency changes.
    let bundle_with_dep = |digest: &str| -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": "lekalo/module-public-contract/v0.8.0",
            "identity": "dev.lekalo.module-public-contract@0.8.0",
            "producer": { "project": "flow", "module": "tasks" },
            "apiVersion": "1.0.0",
            "digest": "",
            "pins": {
                "modelVersion": "0.2.16",
                "irDigest": format!("sha256:{}", "a".repeat(64)),
                "sourceRevision": { "state": "unknown" }
            },
            "exports": [
                {
                    "export": "tasks.wrapper",
                    "kind": "value-object",
                    "version": 0,
                    "fields": [
                        { "name": "inner", "required": true,
                          "type": { "ref": "tasks.inner" } }
                    ]
                },
                {
                    "export": "tasks.inner",
                    "kind": "value-object",
                    "version": 0,
                    "fields": [
                        { "name": "thing", "required": true,
                          "type": { "ref": "dep.thing" } }
                    ]
                }
            ],
            "publicDependencies": [
                {
                    "moduleId": "dep",
                    "digest": digest,
                    "apiVersion": null,
                    "exports": ["dep.thing"]
                }
            ],
            "ports": [],
            "capabilities": [],
            "deprecations": []
        })
    };
    let mut before = bundle_with_dep(&format!("sha256:{}", "1".repeat(64)));
    let mut after = bundle_with_dep(&format!("sha256:{}", "2".repeat(64)));
    seal_bundle(&mut before);
    seal_bundle(&mut after);
    let snap_with = |api_digest: &str| -> serde_json::Value {
        let mut snapshot = serde_json::json!({
            "schemaVersion": "lekalo/module-api-consumers/v0.8.0",
            "identity": "dev.lekalo.module-api-consumers@0.8.0",
            "producer": { "project": "flow", "module": "tasks" },
            "pins": {
                "project": "flow",
                "modelVersion": "0.2.16",
                "irDigest": format!("sha256:{}", "a".repeat(64)),
                "apiDigest": api_digest
            },
            "consumers": [
                {
                    "consumer": "watcher",
                    "declared": true,
                    "mode": "source",
                    "policy": null,
                    "uses": [
                        { "export": "tasks.wrapper", "member": null,
                          "origin": "declared", "sensitivity": "whole-shape",
                          "transitive": false, "edges": [] }
                    ]
                }
            ],
            "complete": false,
            "digest": ""
        });
        snapshot["digest"] = serde_json::Value::String(String::new());
        let canonical = lekalo_core::module_api::wire::canonical(&snapshot);
        snapshot["digest"] = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(
                format!("lekalo/module-api-consumers/1\n{canonical}").as_bytes()
            )
        )
        .into();
        snapshot
    };
    let before_bytes = serde_json::to_vec(&before).expect("serializes");
    let after_bytes = serde_json::to_vec(&after).expect("serializes");
    let consumers_before =
        serde_json::to_vec(&snap_with(before["digest"].as_str().expect("digest")))
            .expect("serializes");
    let consumers_after = serde_json::to_vec(&snap_with(after["digest"].as_str().expect("digest")))
        .expect("serializes");
    let (report, _) = lekalo_core::module_api::compare_public(
        &before_bytes,
        &after_bytes,
        Some(&consumers_before),
        Some(&consumers_after),
    )
    .expect("diff runs");
    assert_eq!(report.verdict, "breaking");
    let watcher = report
        .consumers
        .iter()
        .find(|c| c.consumer == "watcher")
        .expect("watcher row");
    assert_eq!(
        watcher.status, "affected",
        "the transitive dependency break is exact"
    );
}

#[test]
fn deprecation_windows_are_enforced_at_the_diff_seam() {
    // Finding 18: the parsed deprecation surface has reachable emit
    // sites: a removal due that never happened is a policy violation.
    let mut after = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").expect("bundle"),
    )
    .expect("json");
    after["apiVersion"] = "1.3.0".into();
    after["deprecations"] = serde_json::json!([
        {
            "export": "tasks.get_task",
            "introducedIn": "1.0.0",
            "removedIn": "1.2.0",
            "replacement": "tasks.list_tasks",
            "hint": "migrate to the list view"
        }
    ]);
    seal_bundle(&mut after);
    let before = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json")
        .expect("bundle bytes");
    let after_bytes = serde_json::to_vec(&after).expect("serializes");
    let (report, evidence) =
        lekalo_core::module_api::compare_public(&before, &after_bytes, None, None)
            .expect("diff runs");
    assert!(
        evidence
            .iter()
            .any(|finding| finding.rule == "module-api.deprecation-window-violation"),
        "the closed removal window is a violation"
    );
    assert_eq!(report.verdict, "policy-denied");
}

#[test]
fn diagnostic_sets_partition_per_registry_statuses() {
    // Finding 4: a mixed evidence pool never collapses to the
    // registry-invariant set; each status keeps only its allowed rules,
    // and over-limit pools truncate deterministically.
    use lekalo_core::module_api::diagnostic::{self, Evidence};
    use lekalo_core::result::Status;
    let boundary = |subject: &str| {
        let mut finding = Evidence::new(
            "module-api.symbol-not-exported",
            subject,
            "unexported-cross-module-reference",
        );
        finding.export = Some("tasks.secret".to_owned());
        finding
    };
    let gap = Evidence::new(
        "module-api.evidence-incomplete",
        "tasks",
        "public-closure-gap",
    );
    let mixed = vec![boundary("planner.focus"), gap];
    let invalid = diagnostic::set(&mixed, Status::Invalid).expect("partitioned invalid set");
    assert!(invalid
        .as_slice()
        .iter()
        .all(|d| d.id() != "module-api.evidence-incomplete"));
    assert!(invalid
        .as_slice()
        .iter()
        .any(|d| d.id() == "module-api.symbol-not-exported"));
    let valid = diagnostic::set(&mixed, Status::Valid).expect("partitioned valid set");
    assert_eq!(valid.as_slice().len(), 2);
    // Over 256 rows: the set truncates instead of collapsing.
    let floods: Vec<Evidence> = (0..300)
        .map(|index| boundary(&format!("planner.focus_{index}")))
        .collect();
    let truncated = diagnostic::set(&floods, Status::Invalid).expect("truncated set");
    assert!(truncated.as_slice().len() <= 256);
    // The denied seam carries only denied-allowed rules plus the marker.
    let mut stale = Evidence::new("module-api.lock-stale", "planner", "lock-digest");
    stale.expected = Some("sha256:a".to_owned());
    stale.actual = Some("sha256:b".to_owned());
    let denied = diagnostic::denied_set(
        &[boundary("planner.focus"), stale],
        "tasks::planner",
        "lock-policy-denied",
    )
    .expect("denied set");
    assert!(denied
        .as_slice()
        .iter()
        .any(|d| d.id() == "module-api.policy-denied"));
    assert!(denied
        .as_slice()
        .iter()
        .all(|d| d.id() != "module-api.symbol-not-exported"));
}

#[test]
fn public_export_survives_deep_and_wide_dependency_graphs() {
    // Finding 3 + the second reviewer's stack overflow: memoized bundles
    // and an explicit worklist keep legal DAGs linear and the stack flat.
    // The selection grammar admits invocation-relative lowercase paths
    // only, so the synthetic project is generated under the (ignored)
    // crate target directory.
    let root_rel = "target/module-api-dag-probe";
    let _ = std::fs::remove_dir_all(root_rel);
    let root = std::path::Path::new(root_rel);
    let modules_dir = root.join("lekalo/modules");
    std::fs::create_dir_all(&modules_dir).expect("modules dir");
    std::fs::write(
        root.join("lekalo/project.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: flow\n    kind: project\n    version: 1\n    description: \"synthetic diamond\"\n    derived_from:\n      - \"TASKS-REQ-001\"\n",
    )
    .expect("project");
    // A 24-module diamond: layers fan out and reconverge, so the old
    // per-dependency recomputation was exponential; plus a 120-long
    // chain, which the old recursion walked frame by frame.
    const WIDE: usize = 4;
    for index in 0..WIDE {
        for layer in 0..3 {
            let id = format!("m{layer}x{index}");
            let dir = modules_dir.join(&id);
            std::fs::create_dir_all(&dir).expect("module dir");
            // The reconvergence: every top module pins the whole middle
            // layer, and every middle module its own bottom module, so
            // per-dependency recomputation re-walked shared cones.
            let imports: Vec<String> = match layer {
                0 => Vec::new(),
                1 => vec![format!("m0x{index}")],
                _ => (0..WIDE).map(|peer| format!("m1x{peer}")).collect(),
            };
            let import_lines = if imports.is_empty() {
                String::new()
            } else {
                format!(
                    "    imports:\n{}",
                    imports
                        .iter()
                        .map(|import| format!("      - {import}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            std::fs::write(
                dir.join("module.yaml"),
                format!(
                    "schema_version: \"0.2.16\"\ndefinitions:\n  - id: {id}\n    kind: module\n    version: 1\n    description: \"layer\"\n{import_lines}\n"
                ),
            )
            .expect("module");
            let field_type = match layer {
                0 => format!("{id}.id"),
                1 => format!("m0x{index}.v"),
                _ => format!("m1x{index}.v"),
            };
            let scalar = if layer == 0 {
                format!(
                    "  - id: {id}.id\n    kind: scalar\n    version: 1\n    description: \"leaf domain\"\n    base: string\n"
                )
            } else {
                String::new()
            };
            std::fs::write(
                dir.join("entities.yaml"),
                format!(
                    "schema_version: \"0.2.16\"\ndefinitions:\n{scalar}  - id: {id}.v\n    kind: value-object\n    version: 1\n    description: \"node\"\n    fields:\n      - name: carry\n        type: \"{field_type}\"\n        required: true\n"
                ),
            )
            .expect("types");
        }
    }
    let chain_len = 120;
    for index in 0..chain_len {
        let id = format!("c{index}");
        let dir = modules_dir.join(&id);
        std::fs::create_dir_all(&dir).expect("module dir");
        let import_lines = if index + 1 < chain_len {
            format!("    imports:\n      - c{}\n", index + 1)
        } else {
            String::new()
        };
        std::fs::write(
            dir.join("module.yaml"),
            format!(
                "schema_version: \"0.2.16\"\ndefinitions:\n  - id: {id}\n    kind: module\n    version: 1\n    description: \"chain\"\n{import_lines}"
            ),
        )
        .expect("module");
        let (field_type, scalar) = if index + 1 < chain_len {
            (format!("c{}.v", index + 1), String::new())
        } else {
            (
                format!("{id}.id"),
                format!(
                    "  - id: {id}.id\n    kind: scalar\n    version: 1\n    description: \"leaf domain\"\n    base: string\n"
                ),
            )
        };
        std::fs::write(
            dir.join("entities.yaml"),
            format!(
                "schema_version: \"0.2.16\"\ndefinitions:\n{scalar}  - id: {id}.v\n    kind: value-object\n    version: 1\n    description: \"link\"\n    fields:\n      - name: next\n        type: \"{field_type}\"\n        required: true\n"
            ),
        )
        .expect("types");
    }
    let mut modules = Vec::new();
    for index in 0..WIDE {
        for layer in 0..3 {
            modules.push(format!("m{layer}x{index}"));
        }
    }
    for index in 0..chain_len {
        modules.push(format!("c{index}"));
    }
    modules.sort();
    // Every shape-reachable local symbol must be exported (the boundary
    // fix itself): the leaf modules also publish their scalar domain.
    let attachment = serde_json::json!({
        "schemaVersion": "lekalo/module-api/v0.8.0",
        "identity": "dev.lekalo.module-api@0.8.0",
        "project": "flow",
        "modules": modules
            .iter()
            .map(|id| {
                let mut exports = vec![serde_json::json!({
                    "export": format!("{id}.v"), "kind": "value-object",
                    "since": null, "aliasOf": null
                })];
                if id.starts_with("m0x") || id == "c119" {
                    exports.push(serde_json::json!({
                        "export": format!("{id}.id"), "kind": "scalar",
                        "since": null, "aliasOf": null
                    }));
                }
                serde_json::json!({
                    "moduleId": id,
                    "apiVersion": null,
                    "exports": exports,
                    "consumers": [], "ports": [], "capabilities": [],
                    "grants": [], "deprecations": []
                })
            })
            .collect::<Vec<_>>()
    });
    let attachment_path = root.join("module-api.json");
    std::fs::write(
        &attachment_path,
        serde_json::to_vec_pretty(&attachment).expect("serializes"),
    )
    .expect("attachment");
    let started = std::time::Instant::now();
    let compilation = compile(root_rel);
    let resolved = resolve(
        &std::fs::read(&attachment_path).expect("attachment"),
        &compilation,
    )
    .expect("resolves");
    let diamond = export_public(&resolved, &compilation, "m2x0").expect("diamond exports");
    let chain = export_public(&resolved, &compilation, "c0").expect("chain exports");
    let elapsed = started.elapsed();
    assert_eq!(diamond.exports.len(), 1);
    assert_eq!(chain.exports.len(), 1);
    assert!(
        elapsed.as_secs() < 60,
        "the public export must stay linear on legal DAGs, took {elapsed:?}"
    );
    let _ = std::fs::remove_dir_all(root_rel);
}

#[test]
fn error_exports_carry_registry_facts() {
    // Finding 22: kind "error" exports are reachable and carry the typed
    // #62 registry facts (public payload only).
    let errors = "tests/fixtures/module-api/errors";
    let compilation = compile(errors);
    let bytes = attachment(errors);
    let verdict = check(&bytes, &compilation, true, None).expect("check runs");
    assert_eq!(verdict.report.assessment, "pass");
    let resolved = resolve(&bytes, &compilation).expect("resolves");
    let bundle = export_public(&resolved, &compilation, "planner").expect("bundle");
    let error_export = bundle
        .exports
        .iter()
        .find(|export| export.kind == "error")
        .expect("error export");
    assert_eq!(error_export.export, "planner.task_not_found");
    assert_eq!(error_export.code.as_deref(), Some("LEK-ERR-005"));
    assert_eq!(error_export.retryable, Some(false));
}

#[test]
fn every_public_field_flip_is_visible_to_the_diff() {
    // Finding 6: a fixture that flips every public semantic field
    // independently must report a change with the documented class.
    let base = serde_json::json!({
        "schemaVersion": "lekalo/module-public-contract/v0.8.0",
        "identity": "dev.lekalo.module-public-contract@0.8.0",
        "producer": { "project": "flow", "module": "tasks" },
        "apiVersion": "2.0.0",
        "digest": "",
        "pins": {
            "modelVersion": "0.2.16",
            "irDigest": format!("sha256:{}", "a".repeat(64)),
            "sourceRevision": { "state": "unknown" }
        },
        "exports": [
            { "export": "tasks.level", "kind": "scalar", "version": 0,
              "base": "uuid" },
            { "export": "tasks.state", "kind": "enum", "version": 0,
              "values": ["open"] },
            { "export": "tasks.record", "kind": "entity", "version": 0,
              "fields": [{ "name": "id", "required": true,
                           "type": { "ref": "tasks.level" } }],
              "identity": ["id"] },
            { "export": "tasks.pick", "kind": "command", "version": 0,
              "input": [{ "name": "id", "required": true,
                          "type": { "ref": "tasks.level" } }],
              "effects": [{ "entity": "tasks.record",
                            "operation": "update", "emits": [] }] },
            { "export": "tasks.read", "kind": "query", "version": 0,
              "reads": ["tasks.record"],
              "returns": { "ref": "tasks.record" } },
            { "export": "tasks.made", "kind": "event", "version": 0,
              "payload": [{ "name": "id", "required": true,
                            "type": { "ref": "tasks.level" } }] },
            { "export": "tasks.call", "kind": "endpoint", "version": 0,
              "invokes": "tasks.pick", "method": "post", "path": "/pick" },
            { "export": "tasks.guard", "kind": "policy", "version": 0,
              "operations": ["tasks.pick"], "decision": "allow" }
        ],
        "publicDependencies": [],
        "ports": [{ "port": "tasks.api",
                    "operations": ["tasks.pick", "tasks.read"] }],
        "capabilities": [{ "capability": "http", "obligation": "required",
                           "owner": null, "version": null }],
        "deprecations": []
    });
    let mut base_sealed = base.clone();
    seal_bundle(&mut base_sealed);
    let base_bytes = serde_json::to_vec(&base_sealed).expect("serializes");
    let flip = |mutate: &dyn Fn(&mut serde_json::Value)| -> Vec<String> {
        let mut after = base.clone();
        mutate(&mut after);
        seal_bundle(&mut after);
        let (report, _) = lekalo_core::module_api::compare_public(
            &base_bytes,
            &serde_json::to_vec(&after).expect("serializes"),
            None,
            None,
        )
        .expect("diff runs");
        report
            .changes
            .iter()
            .map(|change| change.reason.to_owned())
            .collect()
    };
    // Scalar base.
    let reasons = flip(&|value| value["exports"][0]["base"] = "string".into());
    assert!(reasons.contains(&"base-changed".to_owned()), "{reasons:?}");
    // Enum values: added and removed.
    let reasons = flip(&|value| {
        value["exports"][1]["values"] = serde_json::json!(["open", "done"]);
    });
    assert!(
        reasons.contains(&"enum-value-added".to_owned()),
        "{reasons:?}"
    );
    let reasons = flip(&|value| {
        value["exports"][1]["values"] = serde_json::json!([]);
    });
    assert!(
        reasons.contains(&"enum-value-removed".to_owned()),
        "{reasons:?}"
    );
    // Entity identity and fields.
    let reasons = flip(&|value| {
        value["exports"][2]["identity"] = serde_json::json!([]);
    });
    assert!(
        reasons.contains(&"identity-changed".to_owned()),
        "{reasons:?}"
    );
    let reasons = flip(&|value| {
        value["exports"][2]["fields"] = serde_json::json!([]);
    });
    assert!(reasons.contains(&"field-removed".to_owned()), "{reasons:?}");
    // Command input and effect obligations.
    let reasons = flip(&|value| {
        value["exports"][3]["input"] = serde_json::json!([]);
    });
    assert!(reasons.contains(&"field-removed".to_owned()), "{reasons:?}");
    let reasons = flip(&|value| {
        value["exports"][3]["effects"] = serde_json::json!([]);
    });
    assert!(
        reasons.contains(&"effect-removed".to_owned()),
        "{reasons:?}"
    );
    let reasons = flip(&|value| {
        value["exports"][3]["effects"][0]["emits"] = serde_json::json!(["tasks.made"]);
    });
    assert!(
        reasons.contains(&"effect-emits-changed".to_owned()),
        "{reasons:?}"
    );
    // Query reads and result.
    let reasons = flip(&|value| {
        value["exports"][4]["reads"] = serde_json::json!([]);
    });
    assert!(reasons.contains(&"reads-changed".to_owned()), "{reasons:?}");
    let reasons = flip(&|value| {
        value["exports"][4]["returns"] = serde_json::json!({ "ref": "tasks.level" });
    });
    assert!(
        reasons.contains(&"result-type-changed".to_owned()),
        "{reasons:?}"
    );
    // Event payload.
    let reasons = flip(&|value| {
        value["exports"][5]["payload"] = serde_json::json!([]);
    });
    assert!(reasons.contains(&"field-removed".to_owned()), "{reasons:?}");
    // Endpoint route and operation.
    let reasons = flip(&|value| {
        value["exports"][6]["method"] = "get".into();
        value["exports"][6]["path"] = "/picks".into();
    });
    assert!(reasons.contains(&"route-changed".to_owned()), "{reasons:?}");
    let reasons = flip(&|value| {
        value["exports"][6]["invokes"] = "tasks.read".into();
    });
    assert!(
        reasons.contains(&"operation-changed".to_owned()),
        "{reasons:?}"
    );
    // Policy decision and governed operations.
    let reasons = flip(&|value| {
        value["exports"][7]["decision"] = "deny".into();
    });
    assert!(
        reasons.contains(&"decision-changed".to_owned()),
        "{reasons:?}"
    );
    let reasons = flip(&|value| {
        value["exports"][7]["operations"] = serde_json::json!([]);
    });
    assert!(
        reasons.contains(&"operations-changed".to_owned()),
        "{reasons:?}"
    );
    // Bundle-level ports and capabilities.
    let reasons = flip(&|value| value["ports"] = serde_json::json!([]));
    assert!(reasons.contains(&"port-removed".to_owned()), "{reasons:?}");
    let reasons = flip(&|value| value["capabilities"] = serde_json::json!([]));
    assert!(
        reasons.contains(&"capability-removed".to_owned()),
        "{reasons:?}"
    );
}

#[test]
fn locks_flag_policy_api_version_and_extra_pins() {
    // Finding 15: one freshness comparison — policy, selection, api
    // version, and symmetric dependency pins.
    use lekalo_core::module_api::{check_lock, ConsumerLock};
    let lock = serde_json::json!({
        "schemaVersion": "lekalo/module-api-lock/v0.8.0",
        "identity": "dev.lekalo.module-api-lock@0.8.0",
        "consumer": "watcher",
        "producer": { "project": "flow", "module": "tasks" },
        "apiVersion": "1.0.0",
        "digest": format!("sha256:{}", "a".repeat(64)),
        "policy": { "type": "version-range", "lower": "1.0.0",
                    "lowerInclusive": true, "upper": "2.0.0",
                    "upperInclusive": false },
        "publicDependencies": [
            { "moduleId": "dep",
              "digest": format!("sha256:{}", "b".repeat(64)),
              "apiVersion": null, "exports": [] }
        ],
        "selected": ["tasks.read"]
    });
    let parse = |value: &serde_json::Value| -> ConsumerLock {
        serde_json::from_value(value.clone()).expect("lock shape")
    };
    let current = parse(&lock);
    assert!(check_lock(&current, &current).is_empty());
    // A changed policy is a resolution change.
    let mut changed = lock.clone();
    changed["policy"] = serde_json::json!({ "type": "exact-version", "version": "1.0.0" });
    let evidence = check_lock(&current, &parse(&changed));
    assert!(evidence
        .iter()
        .any(|finding| finding.detail == "lock-policy"));
    // A changed independent API version is a resolution change.
    let mut changed = lock.clone();
    changed["apiVersion"] = "1.1.0".into();
    let evidence = check_lock(&current, &parse(&changed));
    assert!(
        evidence
            .iter()
            .any(|finding| finding.detail == "lock-api-version"),
        "{:?}",
        evidence
            .iter()
            .map(|f| f.detail.as_str())
            .collect::<Vec<_>>()
    );
    // Symmetric dependency pins: an extra supplied pin and a missing pin
    // are both flagged.
    let mut changed = lock.clone();
    changed["publicDependencies"][0]["moduleId"] = "other".into();
    let evidence = check_lock(&current, &parse(&changed));
    assert!(
        evidence
            .iter()
            .any(|finding| finding.detail == "lock-dependency-missing"),
        "{:?}",
        evidence
            .iter()
            .map(|f| f.detail.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        evidence
            .iter()
            .any(|finding| finding.detail == "lock-dependency-extra"),
        "{:?}",
        evidence
            .iter()
            .map(|f| f.detail.as_str())
            .collect::<Vec<_>>()
    );
    let mut changed = lock.clone();
    changed["publicDependencies"] = serde_json::json!([]);
    let evidence = check_lock(&current, &parse(&changed));
    assert!(
        evidence
            .iter()
            .any(|finding| finding.detail == "lock-dependency-missing"),
        "{:?}",
        evidence
            .iter()
            .map(|f| f.detail.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn revision_bumps_preserve_public_identity() {
    // Finding 21: the digest domain excludes the integer definition
    // revision, so a private revision bump pins no consumer.
    let bundle = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").expect("bundle"),
    )
    .expect("json");
    let original_digest = bundle["digest"].clone();
    let mut bumped = bundle;
    for export in bumped["exports"].as_array_mut().unwrap().iter_mut() {
        export["version"] = serde_json::Value::from(export["version"].as_u64().unwrap_or(0) + 7);
    }
    seal_bundle(&mut bumped);
    assert_eq!(bumped["digest"], original_digest);
}
