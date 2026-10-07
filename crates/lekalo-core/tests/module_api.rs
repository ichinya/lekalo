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

const FIXTURES: &str = "tests/fixtures/module-api";
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
        .filter(|e| e.export.is_some())
        .map(|e| e.export.as_deref().unwrap())
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
    assert_eq!(report.verdict, "policy-denied");
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
    let compilation = ();
    let _ = compilation;
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
        RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("2.0.0")), true, false),
        RequiredBump::Major
    );
    assert_eq!(
        RequiredBump::evaluate(Some(&v("2.0.0")), Some(&v("3.0.0")), true, false),
        RequiredBump::None
    );
    // Pre-1.0 pins minor-breaking behavior explicitly.
    assert_eq!(
        RequiredBump::evaluate(Some(&v("0.3.0")), Some(&v("0.3.0")), true, false),
        RequiredBump::Minor
    );
}
