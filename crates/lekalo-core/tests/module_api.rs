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
    // r6 review finding (devin F4): scoping to the producer keeps the
    // consumer-side violations of that producer's boundary — the
    // producer-side reviewer sees who crosses it — instead of hiding
    // them behind a consumer-owns-the-fix convention.
    assert!(
        scoped.evidence.iter().any(|e| {
            e.rule == "module-api.symbol-not-exported"
                && e.consumer.as_deref() == Some("planner")
                && e.producer.as_deref() == Some("tasks")
        }),
        "producer scope retains consumer-side violations: {:?}",
        scoped.evidence
    );
    // Scoping to the violating consumer keeps its own findings too.
    let consumer_scoped = check(
        &attachment(visibility),
        &compilation,
        false,
        Some("planner"),
    )
    .expect("consumer-scoped check runs");
    assert!(consumer_scoped.evidence.iter().any(|e| {
        e.rule == "module-api.symbol-not-exported"
            && e.consumer.as_deref() == Some("planner")
            && e.producer.as_deref() == Some("tasks")
    }));
    // An unrelated scope is refused, never a silent empty pass
    // (finding 14).
    assert!(check(&attachment(visibility), &compilation, false, Some("other")).is_err());
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

/// The r3 review probes: every fixed finding carries its own regression.
///
/// Copy one fixture project into an ignored target-directory scratch the
/// selection grammar accepts (invocation-relative lowercase path).
fn scratch_fixture(rel: &str, name: &str) -> String {
    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).expect("scratch dir");
        for entry in std::fs::read_dir(from).expect("fixture dir") {
            let entry = entry.expect("fixture entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("fixture type").is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).expect("fixture copy");
            }
        }
    }
    let target = format!("target/module-api-r3/{name}");
    let _ = std::fs::remove_dir_all(&target);
    copy_dir(std::path::Path::new(rel), std::path::Path::new(&target));
    target
}

#[test]
fn r3_scoped_module_check_keeps_import_undeclared() {
    // r3 finding 1: the scoped module's own undeclared import never
    // vanishes under --module filtering, and it blocks certification.
    // The probe mirrors the review: an endpoint invoking a foreign
    // operation while the consumer declares no import (endpoint invokes
    // are identity references, so the loader itself admits them).
    let project = scratch_fixture(REFERENCE, "d1-scope");
    let module_yaml =
        std::fs::read_to_string(format!("{project}/lekalo/modules/planner/module.yaml"))
            .expect("module yaml");
    let stripped = module_yaml.replace("    imports:\n      - tasks\n", "");
    assert_ne!(
        stripped.len(),
        module_yaml.len(),
        "the import declaration is present in the fixture"
    );
    std::fs::write(
        format!("{project}/lekalo/modules/planner/module.yaml"),
        stripped,
    )
    .expect("write");
    // Localize the planner shapes so the loader's type-reference import
    // rule stays out of the probe's way.
    let entities =
        std::fs::read_to_string(format!("{project}/lekalo/modules/planner/entities.yaml"))
            .expect("entities");
    std::fs::write(
        format!("{project}/lekalo/modules/planner/entities.yaml"),
        entities.replace("\"tasks.title\"", "\"planner.text\"").replace(
            "definitions:\n",
            "definitions:\n  - id: planner.text\n    kind: scalar\n    version: 1\n    description: \"local text\"\n    base: string\n",
        ),
    )
    .expect("write");
    let commands =
        std::fs::read_to_string(format!("{project}/lekalo/modules/planner/commands.yaml"))
            .expect("commands");
    std::fs::write(
        format!("{project}/lekalo/modules/planner/commands.yaml"),
        commands.replace("\"tasks.title\"", "\"planner.text\""),
    )
    .expect("write");
    std::fs::write(
        format!("{project}/lekalo/modules/planner/bindings.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: planner.tasks_endpoint\n    kind: endpoint\n    version: 1\n    description: \"Foreign list endpoint\"\n    invokes: \"tasks.list_tasks\"\n    method: GET\n    path: \"/tasks\"\n",
    )
    .expect("write");
    let compilation = compile(&project);
    let bytes = attachment(&project);
    let scoped = check(&bytes, &compilation, false, Some("planner")).expect("scoped check");
    let finding = scoped
        .evidence
        .iter()
        .find(|e| e.rule == "module-api.import-undeclared")
        .expect("the scoped module keeps its own undeclared import");
    assert_eq!(finding.consumer.as_deref(), Some("planner"));
    assert_eq!(finding.producer.as_deref(), Some("tasks"));
    assert_eq!(finding.subject, "planner.tasks_endpoint");
    let strict = check(&bytes, &compilation, true, Some("planner")).expect("strict scoped");
    assert_eq!(strict.class, FindingClass::Invalid);
}

/// One minimal two-module project: entities, a scalar, and optional
/// effects/commands referencing the other module.
fn write_cycle_project(root: &str, planner_write: bool, tasks_write: bool) -> Vec<u8> {
    let modules = std::path::Path::new(root).join("lekalo/modules");
    std::fs::create_dir_all(modules.join("tasks")).expect("tasks dir");
    std::fs::create_dir_all(modules.join("planner")).expect("planner dir");
    std::fs::write(
        std::path::Path::new(root).join("lekalo/project.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: flow\n    kind: project\n    version: 1\n    description: \"synthetic\"\n    derived_from:\n      - \"TASKS-REQ-001\"\n",
    )
    .expect("project");
    std::fs::write(
        modules.join("tasks/module.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks\n    kind: module\n    version: 1\n    description: \"tasks\"\n",
    )
    .expect("tasks module");
    std::fs::write(
        modules.join("planner/module.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: planner\n    kind: module\n    version: 1\n    description: \"planner\"\n",
    )
    .expect("planner module");
    std::fs::write(
        modules.join("tasks/entities.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.title\n    kind: scalar\n    version: 1\n    description: \"title\"\n    base: string\n  - id: tasks.task\n    kind: entity\n    version: 1\n    description: \"task\"\n    fields:\n      - name: title\n        type: \"tasks.title\"\n        required: true\n    identity:\n      - title\n",
    )
    .expect("tasks entities");
    // Effects live in commands.yaml (the IR kind-placement rule).
    std::fs::write(
        modules.join("tasks/commands.yaml"),
        format!(
            "schema_version: \"0.2.16\"\ndefinitions:\n{}",
            if tasks_write {
                "  - id: tasks.touch\n    kind: effect\n    version: 1\n    description: \"touches focus\"\n    operation: create\n    entity: \"planner.focus\"\n"
            } else {
                ""
            }
        ),
    )
    .expect("tasks commands");
    std::fs::write(
        modules.join("planner/entities.yaml"),
        "schema_version: \"0.2.16\"\ndefinitions:\n  - id: planner.text\n    kind: scalar\n    version: 1\n    description: \"local text\"\n    base: string\n  - id: planner.focus\n    kind: entity\n    version: 1\n    description: \"focus\"\n    fields:\n      - name: title\n        type: \"planner.text\"\n        required: true\n    identity:\n      - title\n",
    )
    .expect("planner entities");
    std::fs::write(
        modules.join("planner/commands.yaml"),
        format!(
            "schema_version: \"0.2.16\"\ndefinitions:\n{}",
            if planner_write {
                "  - id: planner.edit\n    kind: effect\n    version: 1\n    description: \"edits task\"\n    operation: update\n    entity: \"tasks.task\"\n"
            } else {
                ""
            }
        ),
    )
    .expect("planner commands");
    serde_json::to_vec(&serde_json::json!({
        "schemaVersion": "lekalo/module-api/v0.8.0",
        "identity": "dev.lekalo.module-api@0.8.0",
        "project": "flow",
        "modules": [
            {
                "moduleId": "tasks",
                "apiVersion": "1.0.0",
                "exports": [
                    { "export": "tasks.title", "kind": "scalar", "since": null, "aliasOf": null },
                    { "export": "tasks.task", "kind": "entity", "since": null, "aliasOf": null }
                ],
                "operationErrors": [],
                "queryFacts": [],
                "consumers": [], "ports": [], "capabilities": [],
                "grants": [], "deprecations": []
            },
            {
                "moduleId": "planner",
                "apiVersion": null,
                "exports": [
                    { "export": "planner.focus", "kind": "entity", "since": null, "aliasOf": null }
                ],
                "operationErrors": [],
                "queryFacts": [],
                "consumers": [], "ports": [], "capabilities": [],
                "grants": [], "deprecations": []
            }
        ]
    }))
    .expect("attachment serializes")
}

#[test]
fn r3_write_only_dependency_cycles_are_visible() {
    // r3 finding 2: mutual effect-entity writes with no import declared
    // anywhere are a reference-projection cycle and two undeclared
    // imports — never a clean pass.
    let root = "target/module-api-r3/d2-write-cycle";
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root).expect("root");
    let bytes = write_cycle_project(root, true, true);
    let compilation = compile(root);
    let verdict = check(&bytes, &compilation, true, None).expect("check runs");
    assert_eq!(verdict.class, FindingClass::Invalid);
    assert!(verdict.evidence.iter().any(|e| {
        e.rule == "module-api.dependency-cycle" && e.detail == "reference-dependency-scc"
    }));
    let undeclared: Vec<_> = verdict
        .evidence
        .iter()
        .filter(|e| e.rule == "module-api.import-undeclared")
        .collect();
    assert!(
        undeclared.len() >= 2,
        "both write edges are undeclared imports"
    );
    let cycle = verdict
        .report
        .cycles
        .iter()
        .find(|cycle| cycle.projection == "references")
        .expect("the reference projection witness is reported");
    assert_eq!(cycle.path.first().map(String::as_str), Some("planner"));
    assert_eq!(cycle.path.last().map(String::as_str), Some("planner"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn r3_public_dependency_cycles_carry_the_path() {
    // r3 finding 3: a public dependency cycle is the registered
    // dependency-cycle rule with its exact path, invalid under strict
    // (exit 1), never an unavailable gap or a bare input-invalid set.
    // The mutual exported effect-entity obligations pin each other's
    // module: planner -> tasks and tasks -> planner.
    let root = "target/module-api-r3/d3-public-cycle";
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root).expect("root");
    let mut bytes = write_cycle_project(root, true, true);
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    value["modules"][0]["exports"] = serde_json::json!([
        { "export": "tasks.title", "kind": "scalar", "since": null, "aliasOf": null },
        { "export": "tasks.task", "kind": "entity", "since": null, "aliasOf": null },
        { "export": "tasks.touch", "kind": "effect", "since": null, "aliasOf": null }
    ]);
    value["modules"][1]["exports"] = serde_json::json!([
        { "export": "planner.text", "kind": "scalar", "since": null, "aliasOf": null },
        { "export": "planner.focus", "kind": "entity", "since": null, "aliasOf": null },
        { "export": "planner.edit", "kind": "effect", "since": null, "aliasOf": null }
    ]);
    bytes = serde_json::to_vec(&value).expect("serializes");
    std::fs::write(format!("{root}/module-api.json"), &bytes).expect("attachment");
    let compilation = compile(root);
    let verdict = check(&bytes, &compilation, true, None).expect("check runs");
    let cycle = verdict
        .evidence
        .iter()
        .find(|e| e.rule == "module-api.dependency-cycle" && e.detail == "public-dependency-scc")
        .expect("the public cycle is a registered finding with a path");
    assert!(cycle.path.len() >= 3, "the cycle path is the active path");
    assert_eq!(verdict.class, FindingClass::Invalid);
    assert!(verdict
        .report
        .cycles
        .iter()
        .any(|cycle| cycle.projection == "public"));
    // The core certified export refuses the cyclic closure (the scoped
    // refusal carries the boundary findings; the public-cycle rule is
    // already the check-side witness above).
    let resolved = resolve(&bytes, &compilation).expect("resolves");
    assert!(export_public(&resolved, &compilation, "tasks").is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn r3_public_identity_ignores_private_edits_and_set_order() {
    // r3 finding 4: a private field rename or an export-allowlist reorder
    // never churns the public identity.
    let compilation = compile(REFERENCE);
    let base = export_public(
        &resolve(&attachment(REFERENCE), &compilation).expect("resolves"),
        &compilation,
        "tasks",
    )
    .expect("bundle");
    // (a) private edit: rename the internal field of the non-exported
    // tasks.task entity.
    let project = scratch_fixture(REFERENCE, "d4-private-edit");
    let entities = std::fs::read_to_string(format!("{project}/lekalo/modules/tasks/entities.yaml"))
        .expect("entities");
    std::fs::write(
        format!("{project}/lekalo/modules/tasks/entities.yaml"),
        entities.replace("internal_note", "secret_note"),
    )
    .expect("write");
    let edited = compile(&project);
    let edited_bundle = export_public(
        &resolve(&attachment(&project), &edited).expect("resolves"),
        &edited,
        "tasks",
    )
    .expect("bundle");
    assert_eq!(
        base.digest, edited_bundle.digest,
        "a private edit is not a public identity change"
    );
    // (b) export allowlist reorder.
    let mut value: serde_json::Value =
        serde_json::from_slice(&attachment(REFERENCE)).expect("json");
    for module in value["modules"].as_array_mut().unwrap().iter_mut() {
        if module["moduleId"] == "tasks" {
            module["exports"].as_array_mut().unwrap().reverse();
        }
    }
    let reordered = export_public(
        &resolve(
            &serde_json::to_vec(&value).expect("serializes"),
            &compilation,
        )
        .expect("resolves"),
        &compilation,
        "tasks",
    )
    .expect("bundle");
    assert_eq!(
        base.digest, reordered.digest,
        "an allowlist reorder is not a public identity change"
    );
}

#[test]
fn r3_grant_invariants_bind_through_their_owner() {
    // r3 finding 12: a shared-write grant whose invariant obligation
    // dangles never authorizes the write — it is invalid input.
    let compilation = compile("tests/fixtures/module-api/writes");
    let mut value: serde_json::Value =
        serde_json::from_slice(&attachment("tests/fixtures/module-api/writes")).expect("json");
    value["modules"][0]["grants"] = serde_json::json!([{
        "grant": "planner-update",
        "consumer": "planner",
        "aggregate": "tasks.task",
        "operations": ["update"],
        "fields": ["*"],
        "invariant": "tasks.nonexistent_invariant",
        "rationale": "probe"
    }]);
    let error = resolve(
        &serde_json::to_vec(&value).expect("serializes"),
        &compilation,
    )
    .expect_err("a dangling invariant refuses the grant");
    assert!(error
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.input-invalid"));
}

#[test]
fn r3_alias_declarations_bind_and_resolve() {
    // r3 finding 10: cyclic, dangling, and semantically incompatible
    // aliases are invalid input, never silently dropped renames.
    let compilation = compile(REFERENCE);
    let base: serde_json::Value = serde_json::from_slice(&attachment(REFERENCE)).expect("json");
    let attachment = |alias_of: serde_json::Value| -> Vec<u8> {
        let mut value = base.clone();
        for module in value["modules"].as_array_mut().unwrap().iter_mut() {
            if module["moduleId"] == "tasks" {
                for entry in module["exports"].as_array_mut().unwrap().iter_mut() {
                    if entry["export"] == "tasks.title" {
                        entry["aliasOf"] = alias_of.clone();
                    }
                }
            }
        }
        serde_json::to_vec(&value).expect("serializes")
    };
    // A dangling target.
    let error = resolve(
        &attachment(serde_json::json!("tasks.nonexistent")),
        &compilation,
    )
    .expect_err("dangling alias");
    assert!(error
        .as_slice()
        .iter()
        .any(|d| d.id() == "module-api.input-invalid"));
    // A cycle: task_id -> title -> task_id.
    let cyclic = {
        let mut value: serde_json::Value =
            serde_json::from_slice(&attachment(serde_json::json!("tasks.task_id"))).expect("json");
        for module in value["modules"].as_array_mut().unwrap().iter_mut() {
            if module["moduleId"] == "tasks" {
                for entry in module["exports"].as_array_mut().unwrap().iter_mut() {
                    if entry["export"] == "tasks.task_id" {
                        entry["aliasOf"] = serde_json::json!("tasks.title");
                    }
                }
            }
        }
        serde_json::to_vec(&value).expect("serializes")
    };
    let error = resolve(&cyclic, &compilation).expect_err("alias cycle");
    assert!(error
        .as_slice()
        .iter()
        .any(|d| d.id() == "module-api.input-invalid"));
    // An incompatible scalar meaning: title (string) -> task_id (uuid).
    let error = resolve(
        &attachment(serde_json::json!("tasks.task_id")),
        &compilation,
    )
    .expect_err("alias semantics");
    assert!(error
        .as_slice()
        .iter()
        .any(|d| d.id() == "module-api.input-invalid"));
    // Accepted alias evidence is preserved in the public surface and
    // visible to the diff (r3 finding 10: never silently dropped).
    let bundle =
        std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").expect("bundle");
    let mut before: serde_json::Value = serde_json::from_slice(&bundle).expect("json");
    let mut after = before.clone();
    let title = after["exports"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|export| export["export"] == "tasks.title")
        .unwrap();
    title["aliasOf"] = serde_json::json!("tasks.task_id");
    seal_bundle(&mut before);
    seal_bundle(&mut after);
    let (report, _) = lekalo_core::module_api::compare_public(
        &serde_json::to_vec(&before).expect("serializes"),
        &serde_json::to_vec(&after).expect("serializes"),
        None,
        None,
    )
    .expect("diff runs");
    assert!(report
        .changes
        .iter()
        .any(|change| change.reason == "alias-changed"));
}

#[test]
fn r3_missing_owner_evidence_blocks_strict_and_certification() {
    // r3 finding 14: exported operations without their declared error
    // unions or query facts are required evidence gaps; strict checks go
    // unavailable and certified exports refuse.
    let project = scratch_fixture(REFERENCE, "c14-owner-evidence");
    let compilation = compile(&project);
    let mut value: serde_json::Value = serde_json::from_slice(&attachment(&project)).expect("json");
    for module in value["modules"].as_array_mut().unwrap().iter_mut() {
        module.as_object_mut().unwrap().remove("operationErrors");
        module.as_object_mut().unwrap().remove("queryFacts");
    }
    let bytes = serde_json::to_vec(&value).expect("serializes");
    let verdict = check(&bytes, &compilation, false, None).expect("advisory check");
    assert_eq!(verdict.report.assessment, "indeterminate");
    assert!(verdict
        .report
        .gaps
        .contains(&"owner-evidence-missing".to_owned()));
    assert!(verdict.evidence.iter().any(|e| {
        e.rule == "module-api.evidence-incomplete" && e.detail == "owner-evidence-missing"
    }));
    let strict = check(&bytes, &compilation, true, None).expect("strict check");
    assert_eq!(strict.class, FindingClass::Evidence);
    let resolved = resolve(&bytes, &compilation).expect("resolves");
    let error = export_public(&resolved, &compilation, "tasks").expect_err("refuses");
    assert!(error
        .as_slice()
        .iter()
        .any(|d| d.id() == "module-api.evidence-incomplete"));
}

#[test]
fn r3_resealed_schema_invalid_bundles_refuse() {
    // r3 finding 1: a recomputed checksum never admits a bundle the
    // published closed schema rejects.
    let bundle =
        std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").expect("bundle");
    let base: serde_json::Value = serde_json::from_slice(&bundle).expect("json");
    // A made-up export kind.
    let mut made_up = base.clone();
    made_up["exports"][0]["kind"] = serde_json::json!("made-up");
    seal_bundle(&mut made_up);
    assert!(lekalo_core::module_api::compare_public(
        &serde_json::to_vec(&made_up).expect("serializes"),
        &bundle,
        None,
        None,
    )
    .is_err());
    // A required nullable member deleted (apiVersion).
    let mut no_version = base.clone();
    no_version.as_object_mut().unwrap().remove("apiVersion");
    seal_bundle(&mut no_version);
    assert!(
        lekalo_core::module_api::parse_bundle(&serde_json::to_vec(&no_version).expect("s"))
            .is_err()
    );
    // A required defaulted member deleted (ports).
    let mut no_ports = base.clone();
    no_ports.as_object_mut().unwrap().remove("ports");
    seal_bundle(&mut no_ports);
    assert!(
        lekalo_core::module_api::parse_bundle(&serde_json::to_vec(&no_ports).expect("s")).is_err()
    );
    // A member outside the export's discriminated branch (a scalar
    // carrying a command input member).
    let mut wrong_member = base.clone();
    let scalar = wrong_member["exports"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|export| export["kind"] == "scalar")
        .expect("scalar export");
    scalar["input"] = serde_json::json!([
        { "name": "probe", "required": true, "type": { "ref": "tasks.title" } }
    ]);
    seal_bundle(&mut wrong_member);
    assert!(
        lekalo_core::module_api::parse_bundle(&serde_json::to_vec(&wrong_member).expect("s"))
            .is_err()
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
    // Missing consumer snapshots are explicit unknown coverage (r3
    // finding 7/codex), never a silent pass.
    assert!(evidence.iter().any(|finding| {
        finding.rule == "module-api.evidence-incomplete"
            && finding.gaps == vec!["consumer-snapshots-missing".to_owned()]
    }));
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
/// projection uses: the digest member, the freshness pins, and the
/// integer definition revisions excluded, and every set-like collection
/// canonically ordered (r3 finding 4: the identity is order-insensitive).
fn seal_bundle(value: &mut serde_json::Value) {
    let original_pins = value["pins"].clone();
    value["digest"] = serde_json::Value::String(String::new());
    value["pins"] = serde_json::json!({
        "modelVersion": "",
        "irDigest": "",
        "sourceRevision": { "state": "unknown" }
    });
    let exports = value["exports"].as_array_mut().expect("exports array");
    for export in exports.iter_mut() {
        export["version"] = serde_json::Value::from(0);
        // The wire projection skips empty arrays and nulls — except the
        // per-kind required `errors` union of commands and queries (r4
        // review finding 4/codex), which always serializes — so the
        // sealed spelling must drop them too.
        let operation = matches!(export["kind"].as_str(), Some("command") | Some("query"));
        let empty_keys: Vec<String> = export
            .as_object()
            .expect("export object")
            .iter()
            .filter(|(k, v)| {
                (v.as_array().is_some_and(Vec::is_empty) && !(operation && k.as_str() == "errors"))
                    || v.is_null()
            })
            .map(|(k, _)| k.clone())
            .collect();
        for key in empty_keys {
            export.as_object_mut().unwrap().remove(&key);
        }
    }
    exports.sort_by(|left, right| {
        left["export"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["export"].as_str().unwrap_or_default())
    });
    for key in [
        "publicDependencies",
        "ports",
        "capabilities",
        "deprecations",
    ] {
        if let Some(items) = value[key].as_array_mut() {
            let sort_key = match key {
                "publicDependencies" => "moduleId",
                "ports" => "port",
                "capabilities" => "capability",
                _ => "export",
            };
            items.sort_by(|left, right| {
                left[sort_key]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right[sort_key].as_str().unwrap_or_default())
            });
        }
    }
    if let Some(dependencies) = value["publicDependencies"].as_array_mut() {
        for dependency in dependencies {
            if let Some(symbols) = dependency["exports"].as_array_mut() {
                symbols.sort_by(|left, right| {
                    left.as_str()
                        .unwrap_or_default()
                        .cmp(right.as_str().unwrap_or_default())
                });
            }
        }
    }
    if let Some(ports) = value["ports"].as_array_mut() {
        for port in ports {
            if let Some(operations) = port["operations"].as_array_mut() {
                operations.sort_by(|left, right| {
                    left.as_str()
                        .unwrap_or_default()
                        .cmp(right.as_str().unwrap_or_default())
                });
            }
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
    // The emitted bundle carries its real freshness pins: only the digest
    // domain excludes them.
    value["pins"] = original_pins;
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
                            "operation": "update", "emits": [] }],
              "errors": [] },
            { "export": "tasks.read", "kind": "query", "version": 0,
              "reads": ["tasks.record"],
              "returns": { "ref": "tasks.record" },
              "errors": [],
              "cardinality": "one", "consistency": "strong",
              "pagination": "none" },
            { "export": "tasks.made", "kind": "event", "version": 0,
              "payload": [{ "name": "id", "required": true,
                            "type": { "ref": "tasks.level" } }] },
            { "export": "tasks.call", "kind": "endpoint", "version": 0,
              "invokes": "tasks.pick", "method": "POST", "path": "/pick" },
            { "export": "tasks.guard", "kind": "policy", "version": 0,
              "operations": ["tasks.pick"], "decision": "allow" },
            { "export": "tasks.boom", "kind": "error", "version": 0,
              "code": "LEK-ERR-005", "retryable": false,
              "payload": [{ "name": "task", "required": true,
                            "type": { "ref": "tasks.level" } }] }
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
        value["exports"][6]["method"] = "GET".into();
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
    // r3 finding 8/codex: the capability version pin is public semantics.
    let reasons = flip(&|value| value["capabilities"][0]["version"] = "2.0.0".into());
    assert!(
        reasons.contains(&"capability-version-changed".to_owned()),
        "{reasons:?}"
    );
    // r3 finding 8/codex: error payload types are compared surface.
    let reasons = flip(&|value| {
        let error = value["exports"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|export| export["kind"] == "error")
            .unwrap();
        error["payload"][0]["type"] = serde_json::json!({ "ref": "tasks.state" });
    });
    assert!(reasons.contains(&"type-changed".to_owned()), "{reasons:?}");
    // r3 findings 4 and 9/codex: member-level additive surface and
    // output-breaking optionality drive the version policy.
    let policy_flip = |mutate: &dyn Fn(&mut serde_json::Value),
                       version: &str|
     -> (Vec<String>, String, Option<String>) {
        let mut after = base.clone();
        mutate(&mut after);
        after["apiVersion"] = version.into();
        seal_bundle(&mut after);
        let (report, _) = lekalo_core::module_api::compare_public(
            &base_bytes,
            &serde_json::to_vec(&after).expect("serializes"),
            None,
            None,
        )
        .expect("diff runs");
        (
            report.changes.iter().map(|c| c.reason.clone()).collect(),
            report.version_policy.decision.clone(),
            report.version_policy.expected.clone(),
        )
    };
    // An optional output field addition is additive surface: a patch
    // bump violates the documented minor requirement.
    let (reasons, decision, expected) = policy_flip(
        &|value| {
            value["exports"][2]["fields"] = serde_json::json!([
                { "name": "id", "required": true, "type": { "ref": "tasks.level" } },
                { "name": "notes", "required": false, "type": { "ref": "tasks.level" } }
            ]);
        },
        "2.0.1",
    );
    assert!(reasons.contains(&"field-added".to_owned()), "{reasons:?}");
    assert_eq!(decision, "violation");
    assert_eq!(expected.as_deref(), Some("minor"));
    // A required output member becoming optional is a break: only a
    // major bump satisfies it.
    let (reasons, decision, expected) = policy_flip(
        &|value| {
            value["exports"][2]["fields"] = serde_json::json!([
                { "name": "id", "required": false, "type": { "ref": "tasks.level" } }
            ]);
        },
        "2.1.0",
    );
    assert!(
        reasons.contains(&"required-output-became-optional".to_owned()),
        "{reasons:?}"
    );
    assert_eq!(decision, "violation");
    assert_eq!(expected.as_deref(), Some("major"));
    // r3 finding 5: a new deprecation declaration is visible surface.
    let (reasons, decision, expected) = policy_flip(
        &|value| {
            value["deprecations"] = serde_json::json!([
                { "export": "tasks.read", "introducedIn": "2.0.0", "removedIn": null,
                  "replacement": null, "hint": "migrate" }
            ]);
        },
        "2.0.0",
    );
    assert!(
        reasons.contains(&"deprecation-added".to_owned()),
        "{reasons:?}"
    );
    assert_eq!(decision, "violation");
    assert_eq!(expected.as_deref(), Some("minor"));
}

#[test]
fn r6_required_additions_on_non_command_kinds_are_never_additive() {
    // r6 review finding (devin F2): a required field added to a
    // value-object is a real break regardless of anchor — source-breaking
    // when consumers construct the shape in input position (a command
    // input member references it), wire-breaking when the shape is
    // output-only — and must never be classified bare `additive`, which
    // would exclude it from the hard-impact export set and under-claim
    // the required bump.
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
            { "export": "tasks.key", "kind": "scalar", "version": 0,
              "base": "uuid" },
            { "export": "tasks.input_vo", "kind": "value-object", "version": 0,
              "fields": [{ "name": "key", "required": true,
                           "type": { "ref": "tasks.key" } }] },
            { "export": "tasks.output_vo", "kind": "value-object", "version": 0,
              "fields": [{ "name": "key", "required": true,
                           "type": { "ref": "tasks.key" } }] },
            { "export": "tasks.run", "kind": "command", "version": 0,
              "input": [{ "name": "payload", "required": true,
                          "type": { "ref": "tasks.input_vo" } }],
              "effects": [],
              "errors": [] },
            { "export": "tasks.read", "kind": "query", "version": 0,
              "reads": [],
              "returns": { "ref": "tasks.output_vo" },
              "errors": [],
              "cardinality": "one", "consistency": "strong",
              "pagination": "none" }
        ],
        "publicDependencies": [],
        "ports": [{ "port": "tasks.api",
                    "operations": ["tasks.run", "tasks.read"] }],
        "capabilities": [{ "capability": "http", "obligation": "required",
                           "owner": null, "version": null }],
        "deprecations": []
    });
    let mut base_sealed = base.clone();
    seal_bundle(&mut base_sealed);
    let base_bytes = serde_json::to_vec(&base_sealed).expect("serializes");
    let diff = |mutate: &dyn Fn(&mut serde_json::Value)| {
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
    };
    let member = |report: &lekalo_core::module_api::DiffReport| {
        report
            .changes
            .iter()
            .find(|change| change.member.as_deref() == Some("extra"))
            .expect("required addition is a reported change")
            .clone()
    };
    // Input-anchored: the command constructs tasks.input_vo, so the
    // required addition is a source break for constructing consumers.
    let report = diff(&|value| {
        value["exports"][1]["fields"]
            .as_array_mut()
            .expect("input vo fields")
            .push(serde_json::json!({ "name": "extra", "required": true,
                                       "type": { "ref": "tasks.key" } }));
    });
    let change = member(&report);
    assert_eq!(change.class, "source-breaking", "{change:?}");
    assert_eq!(change.reason, "required-input-added", "{change:?}");
    assert_eq!(report.verdict, "breaking");
    assert_eq!(report.version_policy.expected.as_deref(), Some("major"));
    // Output-only: never constructed, so the required addition is a
    // wire-visible, policy-dependent break — never bare additive.
    let report = diff(&|value| {
        value["exports"][2]["fields"]
            .as_array_mut()
            .expect("output vo fields")
            .push(serde_json::json!({ "name": "extra", "required": true,
                                       "type": { "ref": "tasks.key" } }));
    });
    let change = member(&report);
    assert_eq!(change.class, "wire-breaking", "{change:?}");
    assert_eq!(change.reason, "required-field-added", "{change:?}");
    assert_eq!(report.verdict, "breaking");
    assert_eq!(report.version_policy.expected.as_deref(), Some("major"));
    // Control: an optional addition on the same output-only shape stays
    // additive surface.
    let report = diff(&|value| {
        value["exports"][2]["fields"]
            .as_array_mut()
            .expect("output vo fields")
            .push(serde_json::json!({ "name": "extra", "required": false,
                                       "type": { "ref": "tasks.key" } }));
    });
    let change = member(&report);
    assert_eq!(change.class, "additive", "{change:?}");
    assert_eq!(change.reason, "field-added", "{change:?}");
}

#[test]
fn r6_lock_admission_enforces_the_published_schema() {
    // r6 review finding (omp F2): the published lock schema is the exact
    // admission grammar in both directions. A schema-invalid lock is
    // refused input-invalid before any staleness comparison, and a
    // duplicate `moduleId` pin can never mask a stale dependency through
    // last-wins folding in `check_lock`'s supplied-deps map.
    use lekalo_core::module_api::{check_lock, parse_lock, ConsumerLock};
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
            { "moduleId": "shared",
              "digest": format!("sha256:{}", "b".repeat(64)),
              "apiVersion": null,
              "exports": ["shared.task_id"] }
        ],
        "selected": ["tasks.read"]
    });
    let refuses = |value: &serde_json::Value, detail: &str| {
        let error = parse_lock(&serde_json::to_vec(value).expect("serializes"))
            .expect_err("schema-invalid lock refuses");
        assert!(
            error.as_slice().iter().any(|diagnostic| {
                diagnostic.id() == "module-api.input-invalid"
                    && matches!(
                        diagnostic.data().get("detail"),
                        Some(lekalo_core::diagnostics::DataValue::Token(found))
                            if found == detail
                    )
            }),
            "expected refusal {detail}: {:?}",
            error.as_slice()
        );
    };
    // Control: the well-formed lock parses and is fresh against itself.
    let current: ConsumerLock = serde_json::from_value(lock.clone()).expect("lock shape");
    assert!(parse_lock(&serde_json::to_vec(&lock).expect("serializes")).is_ok());
    assert!(check_lock(&current, &current).is_empty());
    // L1: duplicate dependency exports.
    let mut mutated = lock.clone();
    mutated["publicDependencies"][0]["exports"] =
        serde_json::json!(["shared.task_id", "shared.task_id"]);
    refuses(&mutated, "lock-dependency-exports");
    // L2: empty dependency exports.
    let mut mutated = lock.clone();
    mutated["publicDependencies"][0]["exports"] = serde_json::json!([]);
    refuses(&mutated, "lock-dependency-exports");
    // L5: dependency apiVersion outside the SemVer pattern.
    let mut mutated = lock.clone();
    mutated["publicDependencies"][0]["apiVersion"] = "1.x".into();
    refuses(&mutated, "lock-dependency-api-version");
    // L8: an export id beyond the 192-byte bound.
    let mut mutated = lock.clone();
    mutated["publicDependencies"][0]["exports"] =
        serde_json::json!([format!("shared.{}", "x".repeat(400))]);
    refuses(&mutated, "lock-dependency-exports");
    // L9: duplicate moduleId pins. The stale-first ordering used to fold
    // into the fresh pin and report a fresh lock (exit 0); admission now
    // refuses the document before comparison.
    let mut masked = lock.clone();
    masked["publicDependencies"] = serde_json::json!([
        { "moduleId": "shared",
          "digest": format!("sha256:{}", "c".repeat(64)),
          "apiVersion": null,
          "exports": ["shared.task_id"] },
        { "moduleId": "shared",
          "digest": format!("sha256:{}", "b".repeat(64)),
          "apiVersion": null,
          "exports": ["shared.task_id"] }
    ]);
    refuses(&masked, "duplicate-lock-dependency");
    // The masking refusal is admission, not comparison: a single fresh
    // pin still compares fresh, and a single drifted pin still reports
    // the dependency-digest staleness (controls for the control).
    let current: ConsumerLock = serde_json::from_value(lock.clone()).expect("lock shape");
    let mut stale_pin = lock.clone();
    stale_pin["publicDependencies"][0]["digest"] = format!("sha256:{}", "d".repeat(64)).into();
    let stale: ConsumerLock = serde_json::from_value(stale_pin).expect("lock shape");
    assert!(check_lock(&current, &stale)
        .iter()
        .any(|finding| finding.detail == "lock-dependency-digest"));
}

#[test]
fn r8_review_version_range_policy_upper_key_is_required_nullable() {
    // r8 review finding 2 (MINOR): the `version-range` arm of the
    // published versionPolicy grammar declares `upper` required-nullable,
    // but `VersionPolicyWire::VersionRange.upper` decodes an absent key
    // and an explicit `null` identically to `None`, so only the presence
    // layer can enforce the schema's `required` list. A consumer
    // declaration (attachment), a consumer snapshot, and a lock policy
    // that omit `upper` must refuse with `required-member-missing` —
    // never decode as an unbounded range.
    use lekalo_core::module_api::{parse_lock, parse_snapshot, resolve};

    let detail = |error: &lekalo_core::diagnostics::DiagnosticSet, want: &str| {
        error.as_slice().iter().any(|diagnostic| {
            diagnostic.id() == "module-api.input-invalid"
                && matches!(
                    diagnostic.data().get("detail"),
                    Some(lekalo_core::diagnostics::DataValue::Token(found))
                        if found == want
                )
        })
    };

    // Attachment seam: a declared consumer policy without `upper`.
    let compilation = compile(REFERENCE);
    let mut attachment: serde_json::Value =
        serde_json::from_slice(&attachment(REFERENCE)).expect("attachment fixture");
    attachment["modules"][0]["consumers"][0]["policy"] = serde_json::json!({
        "type": "version-range", "lower": "1.0.0",
        "lowerInclusive": true, "upperInclusive": false
    });
    let error = resolve(
        &serde_json::to_vec(&attachment).expect("serializes"),
        &compilation,
    )
    .expect_err("absent upper refuses");
    assert!(
        detail(&error, "required-member-missing"),
        "expected required-member-missing: {:?}",
        error.as_slice()
    );
    // Control: an explicitly present `upper` — null (unbounded) or a
    // bound — is the schema's legal spelling and stays admitted.
    for upper in [serde_json::Value::Null, serde_json::json!("2.0.0")] {
        attachment["modules"][0]["consumers"][0]["policy"]["upper"] = upper;
        resolve(
            &serde_json::to_vec(&attachment).expect("serializes"),
            &compilation,
        )
        .expect("present upper admits");
    }

    // Snapshot seam: a recorded consumer policy without `upper`.
    let snapshot = std::fs::read("tests/fixtures/module-api/diff/golden-consumers-before.json")
        .expect("snapshot fixture");
    let mut mutated: serde_json::Value = serde_json::from_slice(&snapshot).expect("json");
    assert!(parse_snapshot(&snapshot).is_ok(), "fixture control parses");
    let policy = mutated["consumers"][0]["policy"]
        .as_object_mut()
        .expect("policy");
    assert_eq!(
        policy.get("type").and_then(|kind| kind.as_str()),
        Some("version-range")
    );
    policy.remove("upper");
    let error = parse_snapshot(&serde_json::to_vec(&mutated).expect("serializes"))
        .expect_err("absent upper refuses");
    assert!(
        detail(&error, "required-member-missing"),
        "expected required-member-missing: {:?}",
        error.as_slice()
    );

    // Lock seam: the top-level lock policy without `upper`.
    let mut lock = serde_json::json!({
        "schemaVersion": "lekalo/module-api-lock/v0.8.0",
        "identity": "dev.lekalo.module-api-lock@0.8.0",
        "consumer": "planner",
        "producer": { "project": "tasks", "module": "tasks" },
        "apiVersion": null,
        "digest": format!("sha256:{}", "a".repeat(64)),
        "policy": {
            "type": "version-range", "lower": "1.0.0",
            "lowerInclusive": true, "upper": "2.0.0", "upperInclusive": false
        },
        "publicDependencies": [],
        "selected": ["tasks.task_view"]
    });
    assert!(parse_lock(&serde_json::to_vec(&lock).expect("serializes")).is_ok());
    lock["policy"]
        .as_object_mut()
        .expect("policy")
        .remove("upper");
    let error = parse_lock(&serde_json::to_vec(&lock).expect("serializes"))
        .expect_err("absent upper refuses");
    assert!(
        detail(&error, "required-member-missing"),
        "expected required-member-missing: {:?}",
        error.as_slice()
    );
    lock["policy"]["upper"] = serde_json::Value::Null;
    assert!(parse_lock(&serde_json::to_vec(&lock).expect("serializes")).is_ok());
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
fn r3_policy_incompatible_consumers_deny_without_shape_hits() {
    // r3 findings 6/codex and 6/devin: policy evaluation is an
    // independent obligation — a consumer whose declared resolution can
    // never resolve again (an exact digest pin on the before bundle) is
    // affected and strict-denied even when none of its used exports
    // changed shape (an additive-only change elsewhere moves the digest).
    let bundle_with_dep = |_field: &str, digest: &str| -> serde_json::Value {
        serde_json::json!({
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
                { "export": "tasks.inner", "kind": "value-object", "version": 0,
                  "fields": [{ "name": "thing", "required": true,
                               "type": { "ref": "dep.thing" } }] },
                { "export": "tasks.outer", "kind": "value-object", "version": 0,
                  "fields": [{ "name": "carry", "required": true,
                               "type": { "ref": "tasks.inner" } }] }
            ],
            "publicDependencies": [
                { "moduleId": "dep", "digest": digest, "apiVersion": null,
                  "exports": ["dep.thing"] }
            ],
            "ports": [], "capabilities": [], "deprecations": []
        })
    };
    let mut before = bundle_with_dep("thing", &format!("sha256:{}", "1".repeat(64)));
    // Additive-only change: a new optional field on inner + a minor bump.
    let mut after = bundle_with_dep("thing", &format!("sha256:{}", "1".repeat(64)));
    after["exports"][0]["fields"] = serde_json::json!([
        { "name": "thing", "required": true, "type": { "ref": "dep.thing" } },
        { "name": "note", "required": false, "type": { "ref": "dep.thing" } }
    ]);
    after["apiVersion"] = "2.1.0".into();
    seal_bundle(&mut before);
    seal_bundle(&mut after);
    let snap_with = |api_digest: &str, digest: &str| -> serde_json::Value {
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
                { "consumer": "pinned", "declared": true, "mode": "source",
                  "policy": { "type": "exact-digest", "digest": digest },
                  "uses": [
                    { "export": "tasks.outer", "member": null,
                      "origin": "declared", "sensitivity": "whole-shape",
                      "transitive": false, "edges": [] }
                  ] },
                { "consumer": "ranger", "declared": true, "mode": "source",
                  "policy": { "type": "version-range", "lower": "2.0.0",
                              "lowerInclusive": true, "upper": "3.0.0",
                              "upperInclusive": false },
                  "uses": [
                    { "export": "tasks.inner", "member": null,
                      "origin": "declared", "sensitivity": "whole-shape",
                      "transitive": false, "edges": [] }
                  ] }
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
    let before_digest = before["digest"].as_str().expect("digest").to_owned();
    let after_digest = after["digest"].as_str().expect("digest").to_owned();
    let before_bytes = serde_json::to_vec(&before).expect("serializes");
    let after_bytes = serde_json::to_vec(&after).expect("serializes");
    let consumers_before =
        serde_json::to_vec(&snap_with(&before_digest, &before_digest)).expect("serializes");
    let consumers_after =
        serde_json::to_vec(&snap_with(&after_digest, &before_digest)).expect("serializes");
    let (report, evidence) = lekalo_core::module_api::compare_public(
        &before_bytes,
        &after_bytes,
        Some(&consumers_before),
        Some(&consumers_after),
    )
    .expect("diff runs");
    let pinned = report
        .consumers
        .iter()
        .find(|c| c.consumer == "pinned")
        .expect("pinned row");
    assert_eq!(pinned.status, "affected");
    assert_eq!(pinned.reason.as_deref(), Some("policy-incompatible"));
    assert!(pinned.changes.is_empty(), "no used shape changed");
    let ranger = report
        .consumers
        .iter()
        .find(|c| c.consumer == "ranger")
        .expect("ranger row");
    assert_eq!(ranger.status, "compatible");
    assert!(evidence
        .iter()
        .any(|f| f.rule == "module-api.consumer-incompatible"
            && f.consumer.as_deref() == Some("pinned")));
    // r3 finding 3/codex: the context capsule materializes the required
    // local closure and records the unmaterialized foreign signatures as
    // explicit gaps, never a complete claim without them.
    let capsule =
        plan_consumer_context(&before_bytes, "watcher", &["tasks.outer".to_owned()], 4000)
            .expect("capsule");
    let exports: Vec<&str> = capsule.exports.iter().map(|f| f.export.as_str()).collect();
    assert!(
        exports.contains(&"tasks.inner"),
        "the DTO closure is materialized"
    );
    assert!(capsule
        .gaps
        .contains(&"foreign-signature-not-materialized".to_owned()));
    assert!(capsule
        .omissions
        .iter()
        .any(|omission| omission.subject == "dep.thing"));
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

/// One named tamper case over a JSON document.
type Tamper<'a> = (&'a str, Box<dyn Fn(&mut serde_json::Value) + 'a>);

/// Seal one consumer snapshot over its canonical bytes (the exact digest
/// domain of the family).
fn seal_snapshot(value: &mut serde_json::Value) {
    let original = value.clone();
    value["digest"] = serde_json::Value::String(String::new());
    let canonical = lekalo_core::module_api::wire::canonical(value);
    let digest = format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(
            format!("lekalo/module-api-consumers/1\n{canonical}").as_bytes()
        )
    );
    *value = original;
    value["digest"] = serde_json::Value::String(digest);
}

/// The reference snapshot as JSON, resealed after a mutation.
fn resealed_snapshot(mutate: &dyn Fn(&mut serde_json::Value)) -> Vec<u8> {
    let compilation = compile(REFERENCE);
    let resolved = resolve(&attachment(REFERENCE), &compilation).expect("resolves");
    let snapshot = consumer_snapshot(&resolved, &compilation, "tasks").expect("snapshot");
    let mut value = serde_json::to_value(&snapshot).expect("snapshot serializes");
    mutate(&mut value);
    seal_snapshot(&mut value);
    serde_json::to_vec(&value).expect("snapshot bytes")
}

#[test]
fn r4_digest_valid_snapshots_still_pass_grammar_admission() {
    // r4 finding 3/codex + finding 1/devin: a correctly resealed snapshot
    // with a grammar violation — row ids, vocabularies, policies, bounds
    // — is invalid input; a checksum proves canonical bytes, never input
    // validity.
    let cases: Vec<Tamper> = vec![
        (
            "bad-consumer-id",
            Box::new(|value| {
                value["consumers"][0]["consumer"] = "BAD ID".into();
            }),
        ),
        (
            "bad-mode",
            Box::new(|value| {
                value["consumers"][0]["mode"] = "carrier-pigeon".into();
            }),
        ),
        (
            "bad-origin",
            Box::new(|value| {
                value["consumers"][0]["uses"][0]["origin"] = "divined".into();
            }),
        ),
        (
            "bad-sensitivity",
            Box::new(|value| {
                value["consumers"][0]["uses"][0]["sensitivity"] = "vibes".into();
            }),
        ),
        (
            "bad-use-export",
            Box::new(|value| {
                value["consumers"][0]["uses"][0]["export"] = "!!".into();
            }),
        ),
        (
            "bad-member",
            Box::new(|value| {
                value["consumers"][0]["uses"][0]["member"] = serde_json::json!("Not A Member");
            }),
        ),
        (
            "bad-edge-key",
            Box::new(|value| {
                value["consumers"][0]["uses"][0]["edges"] = serde_json::json!(["!! bad edge"]);
            }),
        ),
        (
            "malformed-policy",
            Box::new(|value| {
                value["consumers"][0]["policy"] = serde_json::json!({
                    "type": "exact-version", "version": "garbage"
                });
            }),
        ),
        (
            "inverted-policy-range",
            Box::new(|value| {
                value["consumers"][0]["policy"] = serde_json::json!({
                    "type": "version-range", "lower": "9.0.0", "lowerInclusive": true,
                    "upper": "1.0.0", "upperInclusive": true
                });
            }),
        ),
        (
            "edge-bound",
            Box::new(|value| {
                value["consumers"][0]["uses"][0]["edges"] = serde_json::Value::Array(
                    std::iter::repeat(serde_json::json!("a->b"))
                        .take(300)
                        .collect(),
                );
            }),
        ),
    ];
    for (label, mutate) in cases {
        let bytes = resealed_snapshot(&mutate);
        let error = lekalo_core::module_api::parse_snapshot(&bytes)
            .expect_err(label)
            .as_slice()[0]
            .code()
            .to_string();
        assert_eq!(error, "LEK-MODULE-001", "{label}: {error}");
    }
    // The untouched control still parses.
    let bytes = resealed_snapshot(&|_| {});
    assert!(lekalo_core::module_api::parse_snapshot(&bytes).is_ok());
}

#[test]
fn r4_snapshot_uses_must_name_bound_bundle_exports() {
    // r4 finding 3/codex: selected-export membership is enforced against
    // the bound bundle at the diff seam — a use of a symbol the producer
    // never exported is invalid inventory.
    let before = std::fs::read("tests/fixtures/module-api/diff/golden-bundle-before.json").unwrap();
    let after = std::fs::read("tests/fixtures/module-api/diff/golden-bundle-after.json").unwrap();
    let forged = resealed_snapshot(&|value| {
        value["consumers"][0]["uses"] = serde_json::json!([{
            "export": "tasks.nonexistent",
            "member": null, "origin": "declared", "sensitivity": "whole-shape",
            "transitive": false, "edges": []
        }]);
        value["pins"]["apiDigest"] = serde_json::to_value(
            serde_json::from_slice::<serde_json::Value>(&before).unwrap()["digest"].clone(),
        )
        .unwrap();
    });
    let error = lekalo_core::module_api::compare_public(
        &before,
        &after,
        Some(&forged),
        Some(&std::fs::read("tests/fixtures/module-api/diff/golden-consumers-after.json").unwrap()),
    )
    .expect_err("forged use refuses");
    assert_eq!(error.as_slice()[0].code().to_string(), "LEK-MODULE-001");
}

#[test]
fn r4_standalone_bundles_require_per_kind_owner_evidence() {
    // r4 finding 4/codex: the standalone decoder refuses a resealed
    // bundle that drops the query owner facts or the explicit (possibly
    // empty) error union of an exported operation — the same required
    // semantics the project export enforces.
    let bundle: serde_json::Value = serde_json::from_slice(
        &std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap(),
    )
    .unwrap();
    let query_without_facts = |value: &mut serde_json::Value| {
        for export in value["exports"].as_array_mut().unwrap().iter_mut() {
            if export["kind"] == "query" {
                for member in ["cardinality", "consistency", "pagination"] {
                    export.as_object_mut().unwrap().remove(member);
                }
            }
        }
    };
    let operation_without_union = |value: &mut serde_json::Value| {
        for export in value["exports"].as_array_mut().unwrap().iter_mut() {
            if matches!(export["kind"].as_str(), Some("command") | Some("query")) {
                export.as_object_mut().unwrap().remove("errors");
            }
        }
    };
    for label in ["query-owner-facts", "operation-errors"] {
        let mut value = bundle.clone();
        if label == "query-owner-facts" {
            query_without_facts(&mut value);
        } else {
            operation_without_union(&mut value);
        }
        seal_bundle(&mut value);
        let error = lekalo_core::module_api::parse_bundle(&serde_json::to_vec(&value).unwrap())
            .expect_err(label);
        assert_eq!(
            error.as_slice()[0].code().to_string(),
            "LEK-MODULE-001",
            "{label}"
        );
    }
}

#[test]
fn r4_attachment_admission_enforces_the_published_grammar() {
    // r4 finding 2/codex: every schema constraint — ID patterns,
    // capability versions, bounds — is attachment admission, so a
    // schema-invalid attachment never becomes accepted policy or output.
    let compilation = compile(REFERENCE);
    let base: serde_json::Value = serde_json::from_slice(&attachment(REFERENCE)).unwrap();
    let cases: Vec<Tamper> = vec![
        (
            "bad-port-id",
            Box::new(|value| {
                value["modules"][0]["ports"][0]["port"] = "!".into();
            }),
        ),
        (
            "bad-capability-id",
            Box::new(|value| {
                value["modules"][0]["capabilities"][0]["capability"] = "!nope".into();
            }),
        ),
        (
            "bad-capability-version",
            Box::new(|value| {
                value["modules"][0]["capabilities"][0]["version"] = "garbage".into();
            }),
        ),
        (
            "bad-capability-owner",
            Box::new(|value| {
                value["modules"][0]["capabilities"][0]["owner"] = "!! owner".into();
            }),
        ),
        (
            "bad-consumer-id",
            Box::new(|value| {
                value["modules"][0]["consumers"][0]["consumer"] = "BAD ID".into();
            }),
        ),
        (
            "bad-grant-id",
            Box::new(|value| {
                value["modules"][0]["grants"] = serde_json::json!([{
                    "grant": "!", "consumer": "planner", "aggregate": "tasks.task",
                    "operations": ["update"], "fields": ["*"],
                    "invariant": "", "rationale": "shared scheduling"
                }]);
            }),
        ),
        (
            "bad-import-use-id",
            Box::new(|value| {
                value["modules"][1]["imports"] = serde_json::json!([
                    { "moduleId": "tasks", "uses": ["tasks.!!"] }
                ]);
            }),
        ),
    ];
    for (label, mutate) in cases {
        let mut value = base.clone();
        mutate(&mut value);
        let error = resolve(&serde_json::to_vec(&value).unwrap(), &compilation).expect_err(label);
        assert_eq!(
            error.as_slice()[0].code().to_string(),
            "LEK-MODULE-001",
            "{label}"
        );
    }
    // The intact attachment still resolves, and its certified bundle
    // round-trips the full incoming admission (r4 finding 2/codex: the
    // outgoing projection is certified against the same grammar).
    let resolved = resolve(&attachment(REFERENCE), &compilation).expect("resolves");
    let bundle = export_public(&resolved, &compilation, "tasks").expect("exports");
    let bytes = serde_json::to_vec(&bundle).unwrap();
    assert!(lekalo_core::module_api::parse_bundle(&bytes).is_ok());
}

#[test]
fn r4_producer_project_and_pin_patterns_are_admission() {
    // r4 finding 2/devin: producer.project, the model-version pin, and a
    // known source-revision value obey their published patterns even in a
    // digest-resealed bundle.
    let bundle: serde_json::Value = serde_json::from_slice(
        &std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap(),
    )
    .unwrap();
    let cases: Vec<Tamper> = vec![
        (
            "bad-producer-project",
            Box::new(|value| {
                value["producer"]["project"] = "!!!".into();
            }),
        ),
        (
            "bad-model-version",
            Box::new(|value| {
                value["pins"]["modelVersion"] = "not-a-version".into();
            }),
        ),
        (
            "bad-source-revision",
            Box::new(|value| {
                value["pins"]["sourceRevision"] =
                    serde_json::json!({ "state": "known", "value": "XYZ" });
            }),
        ),
    ];
    for (label, mutate) in cases {
        let mut value = bundle.clone();
        mutate(&mut value);
        seal_bundle(&mut value);
        let error = lekalo_core::module_api::parse_bundle(&serde_json::to_vec(&value).unwrap())
            .expect_err(label);
        assert_eq!(error.as_slice()[0].code().to_string(), "LEK-MODULE-001");
    }
}

#[test]
fn r4_context_preserves_errors_effects_and_capability_obligations() {
    // r4 finding 5/codex: the capsule materializes the reachable error
    // closure and preserves emitted event ids and full capability
    // obligations — lossless public facts, never counts or bare names.
    let base = serde_json::json!({
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
            { "export": "tasks.level", "kind": "scalar", "version": 0, "base": "uuid" },
            { "export": "tasks.public_error", "kind": "error", "version": 0,
              "code": "PUBLIC_ERROR", "retryable": false,
              "payload": [{ "name": "task", "required": true,
                            "type": { "ref": "tasks.level" } }] },
            { "export": "tasks.do_thing", "kind": "command", "version": 0,
              "input": [{ "name": "level", "required": true,
                          "type": { "ref": "tasks.level" } }],
              "effects": [{ "entity": "tasks.level",
                            "operation": "update",
                            "emits": ["tasks.level"] }],
              "errors": ["tasks.public_error"] }
        ],
        "publicDependencies": [],
        "ports": [{ "port": "tasks.api", "operations": ["tasks.do_thing"] }],
        "capabilities": [
            { "capability": "http", "obligation": "required",
              "owner": "platform", "version": "2.1.0" }
        ],
        "deprecations": []
    });
    let mut sealed = base.clone();
    seal_bundle(&mut sealed);
    let capsule = plan_consumer_context(
        &serde_json::to_vec(&sealed).unwrap(),
        "planner",
        &["tasks.do_thing".to_owned()],
        4000,
    )
    .expect("capsule");
    // The reachable public error export is a materialized required fact.
    let error_fact = capsule
        .exports
        .iter()
        .find(|fact| fact.export == "tasks.public_error")
        .expect("error closure materialized");
    assert!(error_fact.signature.contains("PUBLIC_ERROR"));
    assert!(error_fact.signature.contains("task!"));
    // The command's effect obligation preserves its emitted event ids.
    let command = capsule
        .exports
        .iter()
        .find(|fact| fact.export == "tasks.do_thing")
        .expect("command fact");
    assert_eq!(command.errors, vec!["tasks.public_error".to_owned()]);
    assert_eq!(command.effects.len(), 1);
    assert_eq!(command.effects[0].emits, vec!["tasks.level".to_owned()]);
    // The capability obligation is retained with every typed member.
    assert_eq!(command.capabilities.len(), 1);
    assert_eq!(command.capabilities[0].capability, "http");
    assert_eq!(command.capabilities[0].obligation, "required");
    assert_eq!(command.capabilities[0].owner.as_deref(), Some("platform"));
    assert_eq!(command.capabilities[0].version.as_deref(), Some("2.1.0"));
}

#[test]
fn r4_error_code_changes_reach_operation_consumers() {
    // r4 finding 6/codex: export.errors are edges of the shared reference
    // collector — a changed public error code reaches every consumer of
    // the operation that can raise it.
    let before = serde_json::json!({
        "schemaVersion": "lekalo/module-public-contract/v0.8.0",
        "identity": "dev.lekalo.module-public-contract@0.8.0",
        "producer": { "project": "flow", "module": "tasks" },
        "apiVersion": "1.2.0",
        "digest": "",
        "pins": {
            "modelVersion": "0.2.16",
            "irDigest": format!("sha256:{}", "a".repeat(64)),
            "sourceRevision": { "state": "unknown" }
        },
        "exports": [
            { "export": "tasks.public_error", "kind": "error", "version": 0,
              "code": "PUBLIC_ERROR", "retryable": false,
              "payload": [] },
            { "export": "tasks.get_thing", "kind": "query", "version": 0,
              "reads": [], "errors": ["tasks.public_error"],
              "cardinality": "one", "consistency": "strong",
              "pagination": "none" }
        ],
        "publicDependencies": [],
        "ports": [],
        "capabilities": [],
        "deprecations": []
    });
    let mut before_sealed = before.clone();
    seal_bundle(&mut before_sealed);
    let mut after = before.clone();
    after["exports"][0]["code"] = "CHANGED_ERROR".into();
    after["apiVersion"] = "2.0.0".into();
    seal_bundle(&mut after);
    let snapshot = serde_json::json!({
        "schemaVersion": "lekalo/module-api-consumers/v0.8.0",
        "identity": "dev.lekalo.module-api-consumers@0.8.0",
        "producer": { "project": "flow", "module": "tasks" },
        "pins": {
            "project": "flow",
            "modelVersion": "0.2.16",
            "irDigest": format!("sha256:{}", "a".repeat(64)),
            "apiDigest": before_sealed["digest"].clone()
        },
        "consumers": [{
            "consumer": "planner", "declared": true, "mode": "source",
            "policy": null,
            "uses": [{
                "export": "tasks.get_thing", "member": null,
                "origin": "declared", "sensitivity": "whole-shape",
                "transitive": false, "edges": []
            }]
        }],
        "complete": false,
        "digest": ""
    });
    let mut before_snapshot = snapshot.clone();
    seal_snapshot(&mut before_snapshot);
    let mut after_snapshot = snapshot.clone();
    after_snapshot["pins"]["apiDigest"] = after["digest"].clone();
    seal_snapshot(&mut after_snapshot);
    let (report, _) = lekalo_core::module_api::compare_public(
        &serde_json::to_vec(&before_sealed).unwrap(),
        &serde_json::to_vec(&after).unwrap(),
        Some(&serde_json::to_vec(&before_snapshot).unwrap()),
        Some(&serde_json::to_vec(&after_snapshot).unwrap()),
    )
    .expect("diff runs");
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.reason == "error-code-changed"),
        "the error code change is recorded"
    );
    let planner = report
        .consumers
        .iter()
        .find(|consumer| consumer.consumer == "planner")
        .expect("planner row");
    assert_eq!(
        planner.status, "affected",
        "the operation consumer is affected through its error union"
    );
    assert!(planner.changes.contains(&"tasks.get_thing".to_owned()));
}

#[test]
fn r4_port_operation_removal_breaks_the_callable_surface() {
    // r4 finding 7/codex: removing an operation from the callable surface
    // is a wire break of that operation, so its consumers are exactly
    // affected — never an exact unaffected claim.
    let before = serde_json::json!({
        "schemaVersion": "lekalo/module-public-contract/v0.8.0",
        "identity": "dev.lekalo.module-public-contract@0.8.0",
        "producer": { "project": "flow", "module": "tasks" },
        "apiVersion": "1.2.0",
        "digest": "",
        "pins": {
            "modelVersion": "0.2.16",
            "irDigest": format!("sha256:{}", "a".repeat(64)),
            "sourceRevision": { "state": "unknown" }
        },
        "exports": [
            { "export": "tasks.get_thing", "kind": "query", "version": 0,
              "reads": [], "errors": [],
              "cardinality": "one", "consistency": "strong",
              "pagination": "none" }
        ],
        "publicDependencies": [],
        "ports": [{ "port": "tasks.api", "operations": ["tasks.get_thing"] }],
        "capabilities": [],
        "deprecations": []
    });
    let mut before_sealed = before.clone();
    seal_bundle(&mut before_sealed);
    let mut after = before.clone();
    after["ports"] = serde_json::json!([]);
    after["apiVersion"] = "2.0.0".into();
    seal_bundle(&mut after);
    let (report, _) = lekalo_core::module_api::compare_public(
        &serde_json::to_vec(&before_sealed).unwrap(),
        &serde_json::to_vec(&after).unwrap(),
        None,
        None,
    )
    .expect("diff runs");
    let removal = report
        .changes
        .iter()
        .find(|change| change.reason == "port-operation-removed")
        .expect("the callable-surface removal is explicit");
    assert_eq!(removal.class, "wire-breaking");
    assert_eq!(removal.export, "tasks.get_thing");
    assert_eq!(report.verdict, "breaking");
}

#[test]
fn r4_required_capability_addition_is_an_explicit_evidence_gap() {
    // r4 finding 7/codex: a new required capability has no operation
    // mapping in the bundle, so its exact consumer impact is explicitly
    // unknown — recorded as a coverage reason and a required evidence gap.
    let before = serde_json::json!({
        "schemaVersion": "lekalo/module-public-contract/v0.8.0",
        "identity": "dev.lekalo.module-public-contract@0.8.0",
        "producer": { "project": "flow", "module": "tasks" },
        "apiVersion": "1.2.0",
        "digest": "",
        "pins": {
            "modelVersion": "0.2.16",
            "irDigest": format!("sha256:{}", "a".repeat(64)),
            "sourceRevision": { "state": "unknown" }
        },
        "exports": [
            { "export": "tasks.get_thing", "kind": "query", "version": 0,
              "reads": [], "errors": [],
              "cardinality": "one", "consistency": "strong",
              "pagination": "none" }
        ],
        "publicDependencies": [],
        "ports": [{ "port": "tasks.api", "operations": ["tasks.get_thing"] }],
        "capabilities": [],
        "deprecations": []
    });
    let mut before_sealed = before.clone();
    seal_bundle(&mut before_sealed);
    let mut after = before.clone();
    after["capabilities"] = serde_json::json!([
        { "capability": "audit.sink", "obligation": "required",
          "owner": "compliance", "version": "1.0.0" }
    ]);
    after["apiVersion"] = "1.3.0".into();
    seal_bundle(&mut after);
    let (report, evidence) = lekalo_core::module_api::compare_public(
        &serde_json::to_vec(&before_sealed).unwrap(),
        &serde_json::to_vec(&after).unwrap(),
        None,
        None,
    )
    .expect("diff runs");
    assert!(report
        .coverage
        .unknown_reasons
        .contains(&"required-capability-impact-unknown".to_owned()));
    assert!(evidence
        .iter()
        .any(|finding| finding.rule == "module-api.evidence-incomplete"
            && finding
                .gaps
                .contains(&"required-capability-impact-unknown".to_owned())));
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.reason == "required-capability-added"
                && change.class == "behavioral")
    );
}

#[test]
fn r4_version_downgrades_are_violations() {
    // r4 finding 8/codex: backwards version movement is rejected
    // independently of the classification — decreasing major/minor/patch
    // and an unchanged-surface downgrade are all violations.
    let bundle_with_version = |version: &str, code: &str| -> Vec<u8> {
        let mut value = serde_json::json!({
            "schemaVersion": "lekalo/module-public-contract/v0.8.0",
            "identity": "dev.lekalo.module-public-contract@0.8.0",
            "producer": { "project": "flow", "module": "tasks" },
            "apiVersion": version,
            "digest": "",
            "pins": {
                "modelVersion": "0.2.16",
                "irDigest": format!("sha256:{}", "a".repeat(64)),
                "sourceRevision": { "state": "unknown" }
            },
            "exports": [
                { "export": "tasks.public_error", "kind": "error", "version": 0,
                  "code": code, "retryable": false, "payload": [] },
                { "export": "tasks.get_thing", "kind": "query", "version": 0,
                  "reads": [], "errors": ["tasks.public_error"],
                  "cardinality": "one", "consistency": "strong",
                  "pagination": "none" }
            ],
            "publicDependencies": [],
            "ports": [],
            "capabilities": [],
            "deprecations": []
        });
        seal_bundle(&mut value);
        serde_json::to_vec(&value).unwrap()
    };
    let base = bundle_with_version("1.2.0", "PUBLIC_ERROR");
    for (label, candidate, code) in [
        ("major-downgrade", "0.1.0", "CHANGED_ERROR"),
        ("minor-downgrade", "1.1.0", "CHANGED_ERROR"),
        ("patch-downgrade", "1.1.9", "PUBLIC_ERROR"),
        ("unchanged-surface-downgrade", "0.9.9", "PUBLIC_ERROR"),
    ] {
        let after = bundle_with_version(candidate, code);
        let (report, evidence) =
            lekalo_core::module_api::compare_public(&base, &after, None, None).expect("diff runs");
        assert_eq!(
            report.version_policy.decision, "violation",
            "{label}: {:?}",
            report.version_policy
        );
        assert_eq!(
            report.version_policy.detail, "version-moved-backwards",
            "{label}"
        );
        assert!(
            evidence
                .iter()
                .any(|finding| finding.rule == "module-api.version-policy-violation"),
            "{label}"
        );
    }
    // A strictly increased major with the same break is compliant.
    let fixed = bundle_with_version("2.0.0", "CHANGED_ERROR");
    let (report, _) =
        lekalo_core::module_api::compare_public(&base, &fixed, None, None).expect("diff runs");
    assert_eq!(report.version_policy.decision, "compliant");
}

#[test]
fn r4_mapping_conformance_compares_independent_evidence() {
    // r4 finding 10/codex: two independently authored mapping-evidence
    // documents against one public digest prove the implementation
    // conformance; divergences and unsupported required capabilities are
    // typed evidence gaps, and malformed rows are invalid input.
    let bundle = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap();
    let in_process =
        std::fs::read("tests/fixtures/module-api/mapping/in-process/mapping-evidence.json")
            .unwrap();
    let service =
        std::fs::read("tests/fixtures/module-api/mapping/service/mapping-evidence.json").unwrap();
    let (conformance, evidence) =
        lekalo_core::module_api::check_mapping_conformance(&bundle, &in_process, &service)
            .expect("conformance runs");
    assert!(conformance.conforms);
    assert!(evidence.is_empty());
    assert_eq!(conformance.observations, 4);

    let seal = |value: &mut serde_json::Value| {
        let original = value.clone();
        value["digest"] = serde_json::Value::String(String::new());
        let canonical = lekalo_core::module_api::wire::canonical(value);
        let digest = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(
                format!("lekalo/module-mapping-evidence/1\n{canonical}").as_bytes()
            )
        );
        *value = original;
        value["digest"] = serde_json::Value::String(digest);
    };
    // A divergent observation: the read/write meaning flipped.
    let mut divergent: serde_json::Value = serde_json::from_slice(&service).unwrap();
    divergent["observations"][0]["access"] = "write".into();
    seal(&mut divergent);
    let (report, evidence) = lekalo_core::module_api::check_mapping_conformance(
        &bundle,
        &in_process,
        &serde_json::to_vec(&divergent).unwrap(),
    )
    .expect("divergence runs");
    assert!(!report.conforms);
    assert!(report
        .divergences
        .iter()
        .any(|row| row.aspect == "access" && row.operation == "tasks.get_task"));
    assert!(evidence
        .iter()
        .all(|finding| finding.rule == "module-api.evidence-incomplete"));
    // An unsupported required capability diverges explicitly.
    let mut unsupported: serde_json::Value = serde_json::from_slice(&service).unwrap();
    unsupported["unsupported"] = serde_json::json!(["http"]);
    seal(&mut unsupported);
    let (report, _) = lekalo_core::module_api::check_mapping_conformance(
        &bundle,
        &in_process,
        &serde_json::to_vec(&unsupported).unwrap(),
    )
    .expect("unsupported runs");
    assert!(!report.conforms);
    assert!(report
        .divergences
        .iter()
        .any(|row| row.aspect == "required-capability-unsupported"));
    // A missing observation is a coverage divergence.
    let mut missing: serde_json::Value = serde_json::from_slice(&service).unwrap();
    missing["observations"].as_array_mut().unwrap().pop();
    seal(&mut missing);
    let (report, _) = lekalo_core::module_api::check_mapping_conformance(
        &bundle,
        &in_process,
        &serde_json::to_vec(&missing).unwrap(),
    )
    .expect("missing runs");
    assert!(!report.conforms);
    assert!(report
        .divergences
        .iter()
        .any(|row| row.aspect == "membership"));
    // A wrong digest binding is a divergence, and a malformed document is
    // invalid input.
    let mut rebound: serde_json::Value = serde_json::from_slice(&service).unwrap();
    rebound["publicDigest"] = format!("sha256:{}", "0".repeat(64)).into();
    seal(&mut rebound);
    let (report, _) = lekalo_core::module_api::check_mapping_conformance(
        &bundle,
        &in_process,
        &serde_json::to_vec(&rebound).unwrap(),
    )
    .expect("rebound runs");
    assert!(!report.conforms);
    assert!(report
        .divergences
        .iter()
        .any(|row| row.aspect == "digest-binding"));
    let mut malformed: serde_json::Value = serde_json::from_slice(&service).unwrap();
    malformed["implementation"] = "quantum".into();
    seal(&mut malformed);
    assert!(lekalo_core::module_api::check_mapping_conformance(
        &bundle,
        &in_process,
        &serde_json::to_vec(&malformed).unwrap(),
    )
    .is_err());
}

#[test]
fn r4_review_mapping_roles_bind_to_their_positional_argument() {
    // r4 review finding (devin F2 / codex 2): each positional evidence
    // argument must carry its own role. Swapped but otherwise valid,
    // digest-sealed documents are refused as `mapping-implementation-role`
    // instead of certifying a report with inverted implementation labels,
    // and an identical-implementation pair is refused the same way.
    let bundle = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap();
    let in_process =
        std::fs::read("tests/fixtures/module-api/mapping/in-process/mapping-evidence.json")
            .unwrap();
    let service =
        std::fs::read("tests/fixtures/module-api/mapping/service/mapping-evidence.json").unwrap();
    let refuses_role = |error: &lekalo_core::diagnostics::DiagnosticSet| {
        error.as_slice().iter().any(|diagnostic| {
            diagnostic.id() == "module-api.input-invalid"
                && matches!(
                    diagnostic.data().get("detail"),
                    Some(lekalo_core::diagnostics::DataValue::Token(detail))
                        if detail == "mapping-implementation-role"
                )
        })
    };
    // Swapped flag order: both documents are valid and sealed, but each
    // occupies the other implementation's positional role.
    let swapped =
        lekalo_core::module_api::check_mapping_conformance(&bundle, &service, &in_process)
            .expect_err("swapped roles are refused");
    assert!(refuses_role(&swapped));
    // An identical-implementation pair is refused by the same binding.
    let identical =
        lekalo_core::module_api::check_mapping_conformance(&bundle, &in_process, &in_process)
            .expect_err("identical roles are refused");
    assert!(refuses_role(&identical));
    // The correctly ordered golden pair still certifies.
    let (conformance, _) =
        lekalo_core::module_api::check_mapping_conformance(&bundle, &in_process, &service)
            .expect("ordered roles conform");
    assert!(conformance.conforms);
}

#[test]
fn r4_review_union_registry_and_boundary_rule_set_are_pinned() {
    // r4 review F1: the 0.8.0 successor must be the union of the live M8
    // format/adoption/portability/worklog-changelog/managed/tasklink
    // registry, this issue's module-api family, and every family merged
    // since (#33 mcp.*). Every successor accessor parses the same merged
    // bytes and every landed family resolves, so neither side of the
    // merge can drop a pin (r8: 592 = 578 + 14 = 570 + 8 + 14; r6 merge
    // with #33: 603 = 592 + 11).
    let module = lekalo_core::diagnostics::DiagnosticRegistry::module_successor().unwrap();
    let format = lekalo_core::diagnostics::DiagnosticRegistry::format_successor().unwrap();
    let adoption = lekalo_core::diagnostics::DiagnosticRegistry::adoption().unwrap();
    let m8 = lekalo_core::diagnostics::DiagnosticRegistry::m8().unwrap();
    let pilot = lekalo_core::diagnostics::DiagnosticRegistry::pilot().unwrap();
    let mcp = lekalo_core::diagnostics::DiagnosticRegistry::mcp_successor().unwrap();
    assert!(
        module.len()
            > lekalo_core::diagnostics::DiagnosticRegistry::successor()
                .unwrap()
                .len()
    );
    assert_eq!(format.len(), module.len());
    assert_eq!(adoption.len(), module.len());
    assert_eq!(m8.len(), module.len());
    assert_eq!(pilot.len(), module.len());
    assert_eq!(mcp.len(), module.len());
    for family in [
        "module-api.",
        "format.",
        "adoption.",
        "portability.",
        "worklog-changelog.",
        "managed.",
        "tasklink.",
        "mcp.",
    ] {
        assert!(
            module.entries().any(|entry| entry.id().starts_with(family)),
            "family {family} missing from the union successor"
        );
    }
    // r4 review finding (devin F3/codex 5): the strict boundary-rule set
    // lives once (`is_boundary_rule`) and the meta table may not drift
    // from the union registry.
    let six = [
        "module-api.symbol-not-exported",
        "module-api.import-undeclared",
        "module-api.public-private-exposure",
        "module-api.public-target-exposure",
        "module-api.cross-module-write-forbidden",
        "module-api.dependency-cycle",
    ];
    for rule in six {
        assert!(lekalo_core::module_api::validate::is_boundary_rule(rule));
    }
    let registered: Vec<&str> = module
        .entries()
        .map(|entry| entry.id())
        .filter(|id| id.starts_with("module-api."))
        .collect();
    assert_eq!(registered.len(), 14);
    for id in &registered {
        let (code, severity, category) = lekalo_core::module_api::diagnostic::meta(id);
        let entry = module.entry(id).unwrap();
        assert_eq!(code, entry.code(), "meta code drift for {id}");
        assert_eq!(
            severity,
            entry.default_severity().as_str(),
            "meta severity drift for {id}"
        );
        assert_eq!(
            category,
            entry.category().as_str(),
            "meta category drift for {id}"
        );
    }
    // No registered module-api rule outside the six is silently a
    // boundary rule: the strict set stays exact.
    for id in &registered {
        assert_eq!(
            lekalo_core::module_api::validate::is_boundary_rule(id),
            six.contains(id),
            "boundary set drift for {id}"
        );
    }
}

#[test]
fn r5_source_revision_states_are_closed_and_keep_their_wire_shape() {
    use lekalo_core::module_api::wire::State;
    let bytes = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for revision in [
        serde_json::json!({ "state": "unknown" }),
        serde_json::json!({ "state": "known", "value": "deadbeefdeadbeef" }),
    ] {
        let state: State<String> = serde_json::from_value(revision.clone()).unwrap();
        assert_eq!(serde_json::to_value(state).unwrap(), revision);
        let mut bundle = original.clone();
        bundle["pins"]["sourceRevision"] = revision;
        let parsed =
            lekalo_core::module_api::public::parse_bundle(&serde_json::to_vec(&bundle).unwrap())
                .expect("valid provenance state needs no public digest reseal");
        assert_eq!(parsed.digest, original["digest"].as_str().unwrap());
    }
    for (revision, expected_detail) in [
        (
            serde_json::json!({ "state": "unknown", "value": "deadbeefdeadbeef" }),
            "closed-shape",
        ),
        (
            serde_json::json!({ "state": "unknown", "value": null }),
            "closed-shape",
        ),
        (
            serde_json::json!({ "state": "unknown", "extra": 1 }),
            "closed-shape",
        ),
        (
            serde_json::json!({ "state": "known", "value": "deadbeefdeadbeef", "extra": 1 }),
            "closed-shape",
        ),
        (
            serde_json::json!({ "state": "known" }),
            "required-member-missing",
        ),
    ] {
        assert!(serde_json::from_value::<State<String>>(revision.clone()).is_err());
        let mut bundle = original.clone();
        bundle["pins"]["sourceRevision"] = revision;
        let error =
            lekalo_core::module_api::public::parse_bundle(&serde_json::to_vec(&bundle).unwrap())
                .expect_err("extra or missing state members refuse before digest validation");
        assert!(error.as_slice().iter().any(|diagnostic| {
            diagnostic.id() == "module-api.input-invalid"
                && matches!(
                    diagnostic.data().get("detail"),
                    Some(lekalo_core::diagnostics::DataValue::Token(detail))
                        if detail == expected_detail
                )
        }));
    }
}

#[test]
fn r5_mapping_roles_require_distinct_port_surfaces() {
    let bundle = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap();
    let local = std::fs::read("tests/fixtures/module-api/mapping/in-process/mapping-evidence.json")
        .unwrap();
    let remote =
        std::fs::read("tests/fixtures/module-api/mapping/service/mapping-evidence.json").unwrap();
    let seal = |bytes: &[u8], port: &str| {
        let mut document = lekalo_core::module_api::parse_mapping_evidence(bytes).unwrap();
        document.port = port.to_owned();
        document.digest.clear();
        document.digest = lekalo_core::module_api::wire::digest(
            lekalo_core::module_api::wire::DIGEST_DOMAIN_MAPPING,
            &lekalo_core::module_api::wire::canonical(&document),
        );
        serde_json::to_vec(&document).unwrap()
    };
    // Both symmetric forgeries retain their proper positional role and
    // valid digest. Each document passes standalone admission, but two
    // labels over one port cannot certify the two-mechanism contract.
    for (in_process, service) in [
        (local.clone(), seal(&remote, "tasks.inprocess")),
        (seal(&local, "tasks.api"), remote.clone()),
    ] {
        lekalo_core::module_api::parse_mapping_evidence(&in_process).unwrap();
        lekalo_core::module_api::parse_mapping_evidence(&service).unwrap();
        let error =
            lekalo_core::module_api::check_mapping_conformance(&bundle, &in_process, &service)
                .expect_err("same-port role pair refuses");
        assert!(error.as_slice().iter().any(|diagnostic| {
            diagnostic.id() == "module-api.input-invalid"
                && matches!(
                    diagnostic.data().get("detail"),
                    Some(lekalo_core::diagnostics::DataValue::Token(detail))
                        if detail == "mapping-port-distinct"
                )
        }));
    }
    let (report, evidence) =
        lekalo_core::module_api::check_mapping_conformance(&bundle, &local, &remote).unwrap();
    assert!(report.conforms);
    assert_ne!(report.in_process, report.service);
    assert!(evidence.is_empty());
}

#[test]
fn r5_populated_public_dependency_fixture_preserves_exact_pins() {
    const PROJECT: &str = "tests/fixtures/module-api/public-dependencies";
    let compilation = compile(PROJECT);
    let bytes = attachment(PROJECT);
    let verdict = check(&bytes, &compilation, true, None).unwrap();
    assert_eq!(verdict.report.assessment, "pass");
    let resolved = resolve(&bytes, &compilation).unwrap();
    let shared = export_public(&resolved, &compilation, "shared").unwrap();
    let bundle = export_public(&resolved, &compilation, "tasks").unwrap();
    assert_eq!(bundle.public_dependencies.len(), 1);
    let dependency = &bundle.public_dependencies[0];
    assert_eq!(dependency.module_id, "shared");
    assert_eq!(dependency.digest, shared.digest);
    assert_eq!(dependency.api_version.as_deref(), Some("1.0.0"));
    assert_eq!(dependency.exports, ["shared.task_id"]);
    let golden_bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{PROJECT}/golden-bundle.json")).unwrap())
            .unwrap();
    assert_eq!(serde_json::to_value(&bundle).unwrap(), golden_bundle);
    let lock = resolve_lock(&resolved, &compilation, "tasks", "planner").unwrap();
    assert_eq!(lock.public_dependencies, bundle.public_dependencies);
    let supplied = lekalo_core::module_api::parse_lock(
        &std::fs::read(format!("{PROJECT}/golden-lock.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(lock, supplied);
    assert!(lekalo_core::module_api::check_lock(&lock, &supplied).is_empty());
    let mut stale = supplied.clone();
    stale.public_dependencies[0].digest = format!("sha256:{}", "0".repeat(64));
    let findings = lekalo_core::module_api::check_lock(&lock, &stale);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].detail, "lock-dependency-digest");
    assert_eq!(findings[0].path, ["shared"]);
    let capsule = plan_consumer_context(
        &serde_json::to_vec(&bundle).unwrap(),
        "planner",
        &["tasks.task_envelope".to_owned()],
        4000,
    )
    .unwrap();
    assert_eq!(capsule.dependencies, bundle.public_dependencies);
    assert_eq!(capsule.exports.len(), 2, "nested local shapes are retained");
    assert_eq!(capsule.gaps, ["foreign-signature-not-materialized"]);
    assert_eq!(capsule.omissions[0].subject, "shared.task_id");
    let golden_context: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{PROJECT}/golden-context.json")).unwrap())
            .unwrap();
    assert_eq!(serde_json::to_value(capsule).unwrap(), golden_context);
}

#[test]
fn r6_context_closure_bound_exhaustion_is_unavailable_not_invalid() {
    // r6 review finding (devin F3): a derived closure exceeding the
    // published fact-row bound is capacity exhaustion over admitted
    // input, not malformed input — it refuses with the typed
    // evidence-incomplete gap (unavailable/exit 4 through the CLI
    // failure router) so callers can distinguish "reduce scope and
    // retry" from "fix your input". The selection-size bound stays
    // input-invalid.
    let mut exports = vec![serde_json::json!({
        "export": "tasks.carrier", "kind": "value-object", "version": 0,
        "fields": [
            { "name": "left", "required": true, "type": { "ref": "tasks.s126" } },
            { "name": "right", "required": true, "type": { "ref": "tasks.s127" } }
        ]
    })];
    for index in 0..128u32 {
        exports.push(serde_json::json!({
            "export": format!("tasks.s{index:03}"), "kind": "scalar", "version": 0,
            "base": "uuid"
        }));
    }
    // Select 126 scalars plus the carrier (127 uses, within the
    // selection bound); the carrier pulls its two unselected scalar
    // leaves in, so the closure is 129 > MAX_CONTEXT_ROOTS (128).
    let mut uses: Vec<String> = (0..126u32)
        .map(|index| format!("tasks.s{index:03}"))
        .collect();
    uses.push("tasks.carrier".to_owned());
    let bundle = serde_json::json!({
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
        "exports": exports,
        "publicDependencies": [],
        "ports": [],
        "capabilities": [{ "capability": "http", "obligation": "required",
                           "owner": null, "version": null }],
        "deprecations": []
    });
    let mut sealed = bundle.clone();
    seal_bundle(&mut sealed);
    let bytes = serde_json::to_vec(&sealed).expect("serializes");
    let error = plan_consumer_context(&bytes, "planner", &uses, 1_000_000)
        .expect_err("closure beyond the bound refuses");
    assert!(
        error.as_slice().iter().any(|diagnostic| {
            diagnostic.id() == "module-api.evidence-incomplete"
                && matches!(
                    diagnostic.data().get("detail"),
                    Some(lekalo_core::diagnostics::DataValue::Token(detail))
                        if detail == "closure-bound-exhausted"
                )
        }),
        "exhaustion is the typed evidence gap: {:?}",
        error.as_slice()
    );
    assert!(
        !error
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.id() == "module-api.input-invalid"),
        "exhaustion is not malformed input: {:?}",
        error.as_slice()
    );
    // Control: selecting more roots than the bound stays malformed
    // input (the caller wrote an out-of-grammar selection).
    let oversized: Vec<String> = (0..=128u32)
        .map(|index| format!("tasks.s{index:03}"))
        .take(129)
        .collect();
    let error = plan_consumer_context(&bytes, "planner", &oversized, 1_000_000)
        .expect_err("oversized selection refuses");
    assert!(
        error.as_slice().iter().any(|diagnostic| {
            diagnostic.id() == "module-api.input-invalid"
                && matches!(
                    diagnostic.data().get("detail"),
                    Some(lekalo_core::diagnostics::DataValue::Token(detail))
                        if detail == "selection-bound"
                )
        }),
        "selection size is input-invalid: {:?}",
        error.as_slice()
    );
}

#[test]
fn r4_context_consumer_selectors_obey_the_grammar() {
    // r4 finding 9/codex: the pure context seam validates the consumer
    // selector against the capsule contract's own grammar.
    let bundle = std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").unwrap();
    for bad in ["!", "Bad_ID", "", "-leading-dash"] {
        assert!(
            plan_consumer_context(&bundle, bad, &["tasks.task_view".to_owned()], 4000).is_err(),
            "consumer {bad:?} refuses"
        );
    }
    assert!(
        plan_consumer_context(&bundle, "planner", &["tasks.task_view".to_owned()], 4000).is_ok()
    );
}
