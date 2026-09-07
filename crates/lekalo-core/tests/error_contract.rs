//! Issue #62 error-contract integration tests: the embedded registry's
//! canonical bytes, the adversarial fixture matrix, the project
//! validation seam, the typed outcome channel separation, the revision
//! diff classes, and the language-neutral projection vectors.
//!
//! Every registry variant is built through the validated `from_parts`
//! constructor or parsed from the committed canonical bytes; no test
//! writes anything and nothing outside the committed fixtures is read.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use lekalo_core::diagnostics::DataValue;
use lekalo_core::error_contract::{
    self, diff, mapping, normalize, Coverage, CoverageRef, EffectClass, ErrorCategory,
    ErrorContract, ErrorId, ErrorPayload, ErrorRef, ErrorRegistry, ErrorUnion, Idempotency,
    InfrastructureFailure, InfrastructureKind, MappingEntry, MessageTemplate, Messages,
    Observability, OperationErrorContract, OperationOutcome, Position, ProjectionForm,
    PublicPayload, RetryPolicy, SourceSpan, TargetMapping, Waiver,
};
use lekalo_core::ir;
use lekalo_core::loader::{normalize_model, LoadSelection, ModelVersion};

const PLANNER: &str = "tests/fixtures/model-v1/valid-planner";
const INVALID_ROOT: &str = "tests/fixtures/error-contract/invalid";

/// Serializes the tests that pin the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// Pin the process working directory to the workspace root: the
/// invocation-relative selectors are only valid from there.
fn pin_workspace_root() {
    std::env::set_current_dir(workspace_root()).expect("pin workspace root");
}

fn compile_planner() -> ir::CompiledProject {
    pin_workspace_root();
    let selection = LoadSelection {
        project: Some(PLANNER.to_owned()),
    };
    let model = normalize_model(&selection).expect("planner project loads");
    match ir::compile(&model) {
        Ok(compilation) => compilation.project,
        Err(_) => panic!("the planner fixture must compile"),
    }
}

fn registry() -> ErrorRegistry {
    ErrorRegistry::embedded()
        .expect("embedded registry is valid")
        .clone()
}

fn id(text: &str) -> ErrorId {
    ErrorId::new(text).expect("valid id")
}

fn source() -> SourceSpan {
    SourceSpan::new(
        "lekalo/modules/planner/commands.yaml",
        Position::new(0, 1, 1).expect("position"),
        Position::new(10, 1, 11).expect("position"),
    )
    .expect("valid source")
}

/// One fully valid extra contract, parameterized by identity, code, and
/// coverage.
fn ghost(id_text: &str, code: &str, coverage: Coverage) -> ErrorContract {
    ErrorContract::new(
        id_text,
        code,
        ErrorCategory::Domain,
        ErrorPayload::new(Vec::new()).expect("empty payload"),
        Messages::new(
            MessageTemplate::new("planner.error.ghost.public", Vec::new()).expect("template"),
            None,
        )
        .expect("messages"),
        RetryPolicy::Never,
        Idempotency::NotApplicable,
        EffectClass::Read,
        Observability::Info,
        coverage,
        source(),
        "ghost contracts exist only inside these tests.",
    )
    .expect("valid contract")
}

fn scenario(name: &str) -> Coverage {
    Coverage::Scenarios(vec![CoverageRef::new(name).expect("ref")])
}

/// The expected refusal for every invalid registry fixture:
/// (file, registered rule, bounded detail tag).
const INVALID_REGISTRIES: &[(&str, &str, &str)] = &[
    (
        "wrong-schema-version.registry.json",
        "error.registry-invalid",
        "identity",
    ),
    (
        "unknown-top-field.registry.json",
        "error.registry-invalid",
        "wire-malformed",
    ),
    (
        "duplicate-error-code.registry.json",
        "error.code-reused",
        "duplicate-code",
    ),
    (
        "unknown-category.registry.json",
        "error.contract-invalid",
        "category",
    ),
    (
        "unsorted-errors.registry.json",
        "error.registry-invalid",
        "noncanonical-bytes",
    ),
    (
        "tombstoned-code-reused.registry.json",
        "error.code-reused",
        "tombstone-collision",
    ),
    (
        "empty-union.registry.json",
        "error.contract-invalid",
        "empty-union",
    ),
    (
        "unsafe-retry-on-write.registry.json",
        "error.retry-idempotency-conflict",
        "safe-retry-requires-guaranteed",
    ),
    (
        "private-field-in-public-template.registry.json",
        "error.payload-invalid",
        "template-private-field",
    ),
];

fn detail_of(set: &lekalo_core::diagnostics::DiagnosticSet) -> String {
    set.as_slice()
        .first()
        .and_then(|diagnostic| diagnostic.data().get("detail"))
        .map(|value| match value {
            DataValue::Token(text) => text.clone(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn rule_ids(set: &lekalo_core::diagnostics::DiagnosticSet) -> Vec<String> {
    let mut ids: Vec<String> = set.as_slice().iter().map(|d| d.id().to_owned()).collect();
    ids.sort();
    ids
}

#[test]
fn embedded_registry_round_trips_canonical_bytes() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let registry = registry();
    assert_eq!(registry.errors().len(), 5);
    assert_eq!(registry.bindings().len(), 3);
    assert_eq!(registry.tombstones().len(), 1);
    assert_eq!(
        registry.canonical_bytes().as_bytes(),
        error_contract::REGISTRY_BYTES
    );
    let not_found = registry
        .error(&id("planner.task_not_found"))
        .expect("declared");
    assert_eq!(not_found.code().as_str(), "LEK-ERR-005");
    assert_eq!(not_found.category(), ErrorCategory::NotFound);
}

#[test]
fn planner_project_satisfies_every_binding_and_reachability_rule() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let project = compile_planner();
    let findings = error_contract::validate(&registry(), &project);
    assert!(
        findings.is_empty(),
        "unexpected findings: {:?}",
        rule_ids(&findings)
    );
}

#[test]
fn empty_project_reports_unresolved_operations_types_and_members() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let project = ir::CompiledProject {
        model_version: ModelVersion::V1_0_0,
        project: None,
        modules: Vec::new(),
        definitions: Vec::new(),
    };
    let findings = error_contract::validate(&registry(), &project);
    let ids = rule_ids(&findings);
    // Three unresolved operations, plus the two distinct unresolved
    // payload leaves (planner.text and planner.task_id; identical
    // findings collapse).
    assert_eq!(
        ids,
        vec![
            "error.binding-invalid".to_owned(),
            "error.binding-invalid".to_owned(),
            "error.binding-invalid".to_owned(),
            "error.payload-invalid".to_owned(),
            "error.payload-invalid".to_owned(),
        ]
    );
}

#[test]
fn unreachable_errors_need_union_membership_scenario_or_waiver() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let base = registry();
    let project = compile_planner();

    // Tests-only coverage without any binding: unreachable.
    let with_uncovered_ghost = {
        let mut errors = base.errors().to_vec();
        errors.push(ghost(
            "planner.ghost_uncovered",
            "LEK-ERR-010",
            Coverage::Tests(vec![
                CoverageRef::new("tests/error_contract.rs#x").expect("ref")
            ]),
        ));
        ErrorRegistry::from_parts(errors, base.bindings().to_vec(), base.tombstones().to_vec())
            .expect("variant registry")
    };
    let findings = error_contract::validate(&with_uncovered_ghost, &project);
    assert_eq!(rule_ids(&findings), vec!["error.unreachable".to_owned()]);
    assert_eq!(
        findings.as_slice()[0].data().get("subject"),
        Some(&DataValue::Token("planner.ghost_uncovered".to_owned()))
    );

    // Scenario coverage satisfies reachability without a binding.
    let with_scenario_ghost = {
        let mut errors = base.errors().to_vec();
        errors.push(ghost(
            "planner.ghost_scenariod",
            "LEK-ERR-011",
            scenario("planner.focus_one_task"),
        ));
        ErrorRegistry::from_parts(errors, base.bindings().to_vec(), base.tombstones().to_vec())
            .expect("variant registry")
    };
    let findings = error_contract::validate(&with_scenario_ghost, &project);
    assert!(
        findings.is_empty(),
        "scenario coverage is reachability evidence: {:?}",
        rule_ids(&findings)
    );

    // An explicit waiver satisfies reachability without a binding.
    let with_waived_ghost = {
        let mut errors = base.errors().to_vec();
        errors.push(ghost(
            "planner.ghost_waived",
            "LEK-ERR-012",
            Coverage::Waiver(
                Waiver::new(
                    "waiver/planner/ghost",
                    "planner-owners",
                    "declared for a future operation; unreachable today.",
                    None,
                )
                .expect("waiver"),
            ),
        ));
        ErrorRegistry::from_parts(errors, base.bindings().to_vec(), base.tombstones().to_vec())
            .expect("variant registry")
    };
    let findings = error_contract::validate(&with_waived_ghost, &project);
    assert!(
        findings.is_empty(),
        "waivers satisfy reachability: {:?}",
        rule_ids(&findings)
    );
}

#[test]
fn invalid_registry_fixtures_fail_with_their_declared_rule() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    pin_workspace_root();
    for (name, rule, detail) in INVALID_REGISTRIES {
        let bytes = std::fs::read(Path::new(&workspace_root()).join(INVALID_ROOT).join(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        match ErrorRegistry::from_bytes(&bytes) {
            Ok(_) => panic!("{name}: expected refusal"),
            Err(set) => {
                assert_eq!(
                    rule_ids(&set),
                    vec![(*rule).to_owned()],
                    "{name}: wrong rule"
                );
                assert_eq!(&detail_of(&set), detail, "{name}: wrong detail");
            }
        }
    }
}

#[test]
fn public_payloads_reject_private_unknown_and_mistyped_values() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let project = compile_planner();
    let base = registry();
    let not_found = base.error(&id("planner.task_not_found")).expect("declared");

    // Required public field missing.
    let outcome = OperationOutcome::<()>::declared(not_found, PublicPayload::empty(), &project);
    assert!(outcome.is_err());

    // Mistyped value: task_id is a uuid scalar, not a number.
    let mut values = BTreeMap::new();
    values.insert(
        "task_id".to_owned(),
        lekalo_core::error_contract::FieldValue::Count(7),
    );
    let payload = PublicPayload::validated(values, not_found, &project);
    assert!(payload.is_err());

    // Unknown fields never pass.
    let mut values = BTreeMap::new();
    values.insert(
        "unknown".to_owned(),
        lekalo_core::error_contract::FieldValue::Text("x".to_owned()),
    );
    let payload = PublicPayload::validated(values, not_found, &project);
    assert!(payload.is_err());

    // The valid path carries the canonical identity unchanged.
    let mut values = BTreeMap::new();
    values.insert(
        "task_id".to_owned(),
        lekalo_core::error_contract::FieldValue::Text(
            "0d9f4a59-5a9e-4f39-a75d-ccbf0a8d8e10".to_owned(),
        ),
    );
    let payload = PublicPayload::validated(values, not_found, &project).expect("values");
    let outcome = OperationOutcome::<()>::declared(not_found, payload, &project).expect("declared");
    match outcome {
        OperationOutcome::Declared {
            id: error_id, code, ..
        } => {
            assert_eq!(error_id, id("planner.task_not_found"));
            assert_eq!(code.as_str(), "LEK-ERR-005");
        }
        _ => panic!("expected the declared channel"),
    }
}

#[test]
fn private_payload_fields_never_enter_public_projections() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let base = registry();
    let project = compile_planner();
    let conflict = base.error(&id("planner.focus_conflict")).expect("declared");
    let mut values = BTreeMap::new();
    values.insert(
        "task_id".to_owned(),
        lekalo_core::error_contract::FieldValue::Text("task-9".to_owned()),
    );
    let payload = PublicPayload::validated(values, conflict, &project).expect("public values");
    let rendered = normalize::public_payload_bytes(&payload);
    assert!(rendered.contains("\"task_id\":\"task-9\""));
    assert!(!rendered.contains("focused_by"));
}

#[test]
fn infrastructure_outcomes_stay_outside_the_declared_union() {
    let failure = InfrastructureFailure::new(InfrastructureKind::Provider, "store\x08down");
    assert_eq!(failure.public_summary(), "infrastructure failure");
    assert_eq!(failure.detail(), "store down");
    let outcome: OperationOutcome<()> = OperationOutcome::Infrastructure(failure);
    assert!(outcome.is_infrastructure());
    match &outcome {
        OperationOutcome::Infrastructure(_) => {}
        _ => panic!("infrastructure must keep its own channel"),
    }
}

#[test]
fn diff_classifies_the_owner_approved_vectors() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let base = registry();

    // Identical registries: no changes.
    let same = ErrorRegistry::from_bytes(base.canonical_bytes().as_bytes()).expect("same");
    let result = diff::diff(&base, &same).expect("diff");
    assert!(result.equal());

    // Adding one covered error is non-breaking.
    let with_ghost = {
        let mut errors = base.errors().to_vec();
        errors.push(ghost(
            "planner.ghost_added",
            "LEK-ERR-010",
            scenario("planner.focus_one_task"),
        ));
        ErrorRegistry::from_parts(errors, base.bindings().to_vec(), base.tombstones().to_vec())
            .expect("variant")
    };
    let result = diff::diff(&base, &with_ghost).expect("diff");
    assert_eq!(result.changes().len(), 1);
    assert_eq!(
        result.changes()[0].kind(),
        diff::ErrorChangeKind::ErrorAdded
    );
    assert_eq!(result.changes()[0].class(), diff::DiffClass::NonBreaking);

    // Removing one error is breaking and drops its union members.
    let without_not_found = drop_error(&base, "planner.task_not_found");
    let result = diff::diff(&base, &without_not_found).expect("diff");
    assert!(result.changes().iter().any(|change| {
        change.kind() == diff::ErrorChangeKind::ErrorRemoved
            && change.class() == diff::DiffClass::Breaking
    }));
    assert!(result.changes().iter().any(|change| {
        change.kind() == diff::ErrorChangeKind::UnionMemberRemoved
            && change.class() == diff::DiffClass::Breaking
    }));

    // Reusing the code of a removed error is invalid, never a guess:
    // each side is internally valid, but the code changes owner.
    let reused = {
        let without = drop_error(&base, "planner.task_not_found");
        let mut errors = without.errors().to_vec();
        errors.push(ghost(
            "planner.ghost_reuse",
            "LEK-ERR-005",
            scenario("planner.focus_one_task"),
        ));
        ErrorRegistry::from_parts(
            errors,
            without.bindings().to_vec(),
            base.tombstones().to_vec(),
        )
        .expect("variant")
    };
    let result = diff::diff(&base, &reused).expect("diff");
    assert!(result.changes().iter().any(|change| {
        change.kind() == diff::ErrorChangeKind::CodeReused
            && change.class() == diff::DiffClass::Invalid
    }));

    // Tightening retry is breaking.
    let tightened = {
        let conflict = base.error(&id("planner.focus_conflict")).expect("declared");
        let replacement = ErrorContract::new(
            conflict.id().as_str(),
            conflict.code().as_str(),
            conflict.category(),
            conflict.payload().clone(),
            conflict.messages().clone(),
            RetryPolicy::Never,
            conflict.idempotency(),
            conflict.effect(),
            conflict.observability(),
            conflict.coverage().clone(),
            conflict.source().clone(),
            conflict.invariant(),
        )
        .expect("replacement");
        let errors: Vec<ErrorContract> = base
            .errors()
            .iter()
            .map(|error| {
                if error.id() == replacement.id() {
                    replacement.clone()
                } else {
                    error.clone()
                }
            })
            .collect();
        ErrorRegistry::from_parts(errors, base.bindings().to_vec(), base.tombstones().to_vec())
            .expect("variant")
    };
    let result = diff::diff(&base, &tightened).expect("diff");
    assert!(result.changes().iter().any(|change| {
        change.kind() == diff::ErrorChangeKind::RetryChanged
            && change.class() == diff::DiffClass::Breaking
    }));

    // Determinism: the change list is identical across two runs.
    let first = diff::diff(&base, &tightened).expect("diff");
    let second = diff::diff(&base, &tightened).expect("diff");
    assert_eq!(first, second);
}

fn drop_error(base: &ErrorRegistry, id_text: &str) -> ErrorRegistry {
    let errors: Vec<ErrorContract> = base
        .errors()
        .iter()
        .filter(|error| error.id().as_str() != id_text)
        .cloned()
        .collect();
    let bindings: Vec<OperationErrorContract> = base
        .bindings()
        .iter()
        .map(|binding| {
            let members: Vec<ErrorRef> = binding
                .errors()
                .members()
                .iter()
                .filter(|member| member.id().as_str() != id_text)
                .cloned()
                .collect();
            OperationErrorContract::new(
                binding.operation().as_str(),
                binding.kind(),
                binding.output().clone(),
                ErrorUnion::new(members).expect("union"),
            )
            .expect("binding")
        })
        .collect();
    ErrorRegistry::from_parts(errors, bindings, base.tombstones().to_vec()).expect("variant")
}

#[test]
fn union_construction_is_order_insensitive_and_duplicate_free() {
    let members = vec![
        ErrorRef::new("planner.zeta").expect("member"),
        ErrorRef::new("planner.alpha").expect("member"),
        ErrorRef::new("planner.zeta").expect("duplicate"),
    ];
    let union = ErrorUnion::new(members).expect("union");
    let spellings: Vec<&str> = union
        .members()
        .iter()
        .map(|member| member.id().as_str())
        .collect();
    assert_eq!(spellings, vec!["planner.alpha", "planner.zeta"]);
    assert!(ErrorUnion::new(Vec::new()).is_err());
}

#[test]
fn mapping_vectors_preserve_the_canonical_identity_and_hide_private_fields() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let base = registry();
    let project = compile_planner();
    let conflict = base.error(&id("planner.focus_conflict")).expect("declared");
    let mut values = BTreeMap::new();
    values.insert(
        "task_id".to_owned(),
        lekalo_core::error_contract::FieldValue::Text("task-9".to_owned()),
    );
    let payload = PublicPayload::validated(values, conflict, &project).expect("public values");

    for vector in [
        mapping::node_error_vector(conflict, &payload),
        mapping::php_error_vector(conflict, &payload),
        mapping::go_error_vector(conflict, &payload),
    ] {
        assert!(vector.contains("\"id\":\"planner.focus_conflict\""));
        assert!(vector.contains("\"code\":\"LEK-ERR-001\""));
        assert!(vector.contains("\"category\":\"conflict\""));
        display_vector(&vector);
        assert!(
            vector.contains("task-9"),
            "payload value must survive: {vector}"
        );
        assert!(!vector.contains("focused_by"));
    }
    // The core contract carries no HTTP status and no exception class.
    let rendered = normalize::error_bytes(conflict);
    assert!(!rendered.contains("http"));
    assert!(!rendered.contains("Exception"));
}

#[test]
fn target_mappings_refuse_catch_alls_status_only_and_missing_members() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let operation = id("planner.focus_task");

    // Strict + catch-all: refused.
    let entry = MappingEntry::preserving(ProjectionForm::ResultTag);
    assert!(TargetMapping::new(
        "node-typescript",
        true,
        vec![("LEK-ERR-001".to_owned(), entry)],
        true,
    )
    .is_err());

    // Status-only identity: refused in any profile.
    let status_only = MappingEntry::preserving(ProjectionForm::HttpStatus);
    assert!(TargetMapping::new(
        "legacy-http",
        false,
        vec![("LEK-ERR-001".to_owned(), status_only)],
        false,
    )
    .is_err());

    // A strict mapping that misses union members: mapping-missing.
    let partial = TargetMapping::new(
        "node-typescript",
        true,
        vec![
            mapped("LEK-ERR-001", ProjectionForm::ResultTag),
            mapped("LEK-ERR-002", ProjectionForm::Exception),
        ],
        false,
    )
    .expect("mapping");
    match partial.check_against(&registry(), &operation) {
        Err(set) => assert_eq!(rule_ids(&set), vec!["error.mapping-missing".to_owned()]),
        Ok(()) => panic!("expected mapping-missing"),
    }

    // The complete strict mapping checks clean and preserves every code.
    let complete = TargetMapping::new(
        "node-typescript",
        true,
        vec![
            mapped("LEK-ERR-001", ProjectionForm::ResultTag),
            mapped("LEK-ERR-002", ProjectionForm::Exception),
            mapped("LEK-ERR-003", ProjectionForm::ResultTag),
            mapped("LEK-ERR-004", ProjectionForm::ResultTag),
            mapped("LEK-ERR-005", ProjectionForm::ResultTag),
        ],
        false,
    )
    .expect("mapping");
    complete
        .check_against(&registry(), &operation)
        .expect("complete strict mapping");
}

fn display_vector(vector: &str) {
    eprintln!("VECTOR: {vector}");
}

fn mapped(code: &str, form: ProjectionForm) -> (String, MappingEntry) {
    (code.to_owned(), MappingEntry::preserving(form))
}
