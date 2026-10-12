//! Managed unit tests: closed parsing, deterministic rendering, plan
//! identity, preflight refusals, staged apply, and exact restoration.

use super::canonical::{camel, render_inventory, requirement_name};
use super::coverage;
use super::history;
use super::plan::{self, BeforeState, Recorded};
use super::rollback;
use super::wire::*;
use super::*;
use crate::result::Status;
use std::collections::BTreeMap;
use std::path::PathBuf;

const PIN_A: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const PIN_B: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const PIN_C: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";

fn reference() -> Reference {
    Reference {
        id: "planner-basic".to_owned(),
        version: "1".to_owned(),
        digest: PIN_A.to_owned(),
    }
}

fn requirement(id: &str, kind: RequirementKind, support: Support) -> Requirement {
    Requirement {
        id: id.to_owned(),
        kind,
        support,
        witness: State::known(PIN_A.to_owned()),
        detail: None,
        projection: vec![
            Projection {
                from: "id".to_owned(),
                to: "id".to_owned(),
            },
            Projection {
                from: "title".to_owned(),
                to: "title".to_owned(),
            },
        ],
        sample: State::known(Sample {
            input: BTreeMap::from([
                ("id".to_owned(), Scalar::Text("t-1".to_owned())),
                ("title".to_owned(), Scalar::Text("Focus".to_owned())),
            ]),
            output: BTreeMap::from([
                ("id".to_owned(), Scalar::Text("t-1".to_owned())),
                ("title".to_owned(), Scalar::Text("Focus".to_owned())),
            ]),
        }),
    }
}

fn coverage_doc() -> Coverage {
    Coverage {
        schema_version: header("managed-coverage").0,
        identity: header("managed-coverage").1,
        module: "planner".to_owned(),
        target: "node-typescript".to_owned(),
        recipe: reference(),
        adapter: Adapter {
            entry: "adapters/managed.mjs".to_owned(),
            package_digest: PIN_B.to_owned(),
            protocol: PROTOCOL.to_owned(),
        },
        authority: State::known(AuthorityPin {
            model: PIN_A.to_owned(),
            ir: PIN_B.to_owned(),
            lock: PIN_A.to_owned(),
        }),
        requirements: vec![
            requirement(
                "planner.complete_task",
                RequirementKind::Command,
                Support::Full,
            ),
            requirement("planner.focus_task", RequirementKind::Query, Support::Full),
        ],
        extensions: Vec::new(),
        complete: true,
    }
}

fn target_assignment() -> TargetAssignment {
    TargetAssignment {
        target: "node-typescript".to_owned(),
        profile_ref: reference(),
        implementation_ref: reference(),
        generated_root: "apps/planner/gen".to_owned(),
        extensions: Vec::new(),
    }
}

fn ownership_doc() -> Ownership {
    Ownership {
        schema_version: header("managed-ownership").0,
        identity: header("managed-ownership").1,
        project: "demo".to_owned(),
        modules: vec![ModuleAssignment {
            module: "planner".to_owned(),
            mode: Mode::Managed,
            targets: vec![target_assignment()],
        }],
    }
}

fn project_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("lekalo-managed-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp project");
    write_test_lock(&root);
    root
}

/// The fixture Model semantics of the fixture coverage: exactly the
/// obligations `coverage_doc()`'s requirements render under (r6, codex
/// F9 — the renderer's interface types are Model-derived, so unit-test
/// plans carry the same obligation semantics the real authority derives).
fn test_semantics() -> Vec<super::authority::Obligation> {
    vec![
        super::authority::Obligation {
            id: "planner.complete_task".to_owned(),
            kind: crate::ir::DefinitionKind::Command,
            input: vec![
                ("id".to_owned(), "string".to_owned()),
                ("title".to_owned(), "string".to_owned()),
            ],
            reads: Vec::new(),
            returns: None,
            return_fields: Vec::new(),
        },
        super::authority::Obligation {
            id: "planner.focus_task".to_owned(),
            kind: crate::ir::DefinitionKind::Query,
            input: Vec::new(),
            reads: Vec::new(),
            returns: None,
            return_fields: vec![
                ("id".to_owned(), "string".to_owned()),
                ("title".to_owned(), "string".to_owned()),
            ],
        },
    ]
}

/// A test generation authority: the lock-revision pin every committed
/// record must carry (findings 3 and 8). The digest value is a fixture
/// constant; the real CLI path always derives the live authority.
fn test_authority() -> super::authority::Authority {
    super::authority::Authority {
        model_digest: PIN_A.to_owned(),
        ir_digest: PIN_B.to_owned(),
        // The canonical digest of the committed fixture lock (its exact
        // payload bytes): unit tests that write a project lock carry the
        // same bytes, so the destination-lock compatibility check holds.
        lock_digest: "sha256:7ef11bfe5df256991ea482c2831f4c99ec45db5bce63d76bffe5e47ff1113062"
            .to_owned(),
        lock_protocol: Some(PROTOCOL.to_owned()),
        model_version: "0.2.16".to_owned(),
        project_id: "demo".to_owned(),
        ledger: Vec::new(),
        semantics: test_semantics(),
        adapter: None,
        // The locked provenance inventory (r4 finding 6): the fixture
        // coverage's recipe and the fixture ownership's profile/
        // implementation references resolve against exactly these.
        generators: vec![super::authority::LockedGenerator {
            id: "planner-basic".to_owned(),
            version: "1.0.0".to_owned(),
            digest: PIN_A.to_owned(),
            adapter_id: "node-typescript".to_owned(),
            adapter_version: "1.0.0".to_owned(),
        }],
        components: vec![super::authority::LockedComponent {
            id: "planner-basic".to_owned(),
            version: "1.0.0".to_owned(),
            digest: PIN_A.to_owned(),
        }],
        sources: Vec::new(),
    }
}

/// The recovery-time authority derivation of unit tests: the same fixture
/// authority every test apply stamped (the real CLI derives the live
/// project's authority through the accepted loader pipeline).
fn test_authority_of(
    _module: &str,
) -> Result<super::authority::Authority, crate::result::DomainResult> {
    Ok(test_authority())
}

/// Create a directory junction (Windows test helper).
#[cfg(windows)]
fn junction(target: &std::path::Path, link: &std::path::Path) {
    std::os::windows::fs::symlink_dir(target, link).expect("junction");
}

/// Recursively copy a fixture tree (test helper).
fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).expect("mkdir");
    for entry in std::fs::read_dir(source).expect("read dir") {
        let entry = entry.expect("entry");
        let to = destination.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy");
        }
    }
}

/// The exact committed fixture lock bytes, for unit-test project roots.
/// Relative to the crate directory (cargo test runs there).
fn write_test_lock(root: &std::path::Path) {
    let bytes = match std::fs::read("tests/fixtures/managed/planner/lekalo.lock") {
        Ok(bytes) => bytes,
        // The cwd may be the workspace root; try both spellings.
        Err(_) => std::fs::read("../../tests/fixtures/managed/planner/lekalo.lock")
            .or_else(|_| {
                std::fs::read("C:/Users/User/orca/workspaces/lekalo/m8-issue-41/tests/fixtures/managed/planner/lekalo.lock")
            })
            .expect("fixture lock"),
    };
    write_input(root, "lekalo.lock", &bytes);
}

#[test]
fn canonical_renderer_is_byte_stable_and_ordered() {
    let coverage = coverage_doc();
    let first =
        render_inventory(&coverage, "apps/planner/gen", &test_semantics()).expect("inventory");
    let second =
        render_inventory(&coverage, "apps/planner/gen", &test_semantics()).expect("inventory");
    assert_eq!(first, second);
    assert_eq!(
        first.len(),
        4,
        "two requirements render implementation+test"
    );
    let paths: Vec<&str> = first.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "apps/planner/gen/src/complete-task.ts",
            "apps/planner/gen/test/complete-task.test.ts",
            "apps/planner/gen/src/focus-task.ts",
            "apps/planner/gen/test/focus-task.test.ts"
        ]
    );
    let implementation = String::from_utf8(first[2].bytes.clone()).expect("utf8");
    assert!(!implementation.contains('\r'), "fixed LF endings");
    assert!(!implementation.contains("20"), "no wall-clock data");
    assert!(implementation
        .contains("export function focusTask(input: FocusTaskInput): FocusTaskOutput"));
    assert!(implementation.contains("title: input.title"));
}

#[test]
fn name_helpers_follow_the_ledger_spelling() {
    assert_eq!(
        requirement_name("planner", "planner.focus_task"),
        Some("focus_task")
    );
    assert_eq!(requirement_name("planner", "other.focus_task"), None);
    assert_eq!(camel("focus_task"), "focusTask");
    assert_eq!(super::canonical::pascal("focus_task"), "FocusTask");
}

#[test]
fn ownership_parsing_rejects_closed_wire_violations() {
    let document = ownership_doc();
    let bytes = serde_json::to_vec(&document).expect("wire");
    assert!(parse_ownership(&bytes).is_ok());
    // Duplicate keys are refused before serde can erase them.
    let duplicated = String::from_utf8(bytes).expect("utf8").replacen(
        "\"project\":\"demo\"",
        "\"project\":\"demo\",\"project\":\"demo\"",
        1,
    );
    assert!(parse_ownership(duplicated.as_bytes()).is_err());
    // Wrong family identity is unsupported-version, not invalid.
    let mut wrong = document.clone();
    wrong.identity = "dev.lekalo.managed-ownership@0.2.16".to_owned();
    let error = parse_ownership(&serde_json::to_vec(&wrong).expect("wire")).expect_err("identity");
    assert_eq!(error.status(), Status::UnsupportedVersion);
    // Unsorted modules are invalid.
    let mut unsorted = ownership_doc();
    unsorted.modules.push(ModuleAssignment {
        module: "alpha".to_owned(),
        mode: Mode::Managed,
        targets: vec![target_assignment()],
    });
    let error = parse_ownership(&serde_json::to_vec(&unsorted).expect("wire")).expect_err("order");
    assert_eq!(error.status(), Status::Invalid);
    // Overlapping generated roots give one file two owners.
    let mut overlap = ownership_doc();
    overlap.modules.push(ModuleAssignment {
        module: "alpha".to_owned(),
        mode: Mode::Managed,
        targets: vec![TargetAssignment {
            generated_root: "apps/planner/gen/inner".to_owned(),
            ..target_assignment()
        }],
    });
    let error = parse_ownership(&serde_json::to_vec(&overlap).expect("wire")).expect_err("overlap");
    assert_eq!(error.status(), Status::Invalid);
    // Absolute or traversal coordinates never parse.
    assert!(!rel_path("/abs/path"));
    assert!(!rel_path("apps/../secrets"));
    assert!(!rel_path("apps\\windows"));
}

#[test]
fn admission_blocks_capability_and_extension_gaps() {
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage_doc());
    assert!(admitted.is_ok());
    // A partial required semantic blocks managed selection with a witness.
    let mut gap = coverage_doc();
    gap.requirements[0].support = Support::Partial;
    let error = coverage::admit(assignment, target, &gap).expect_err("gap");
    assert_eq!(error.status(), Status::Unsupported);
    assert_eq!(
        error.reason_codes().first().map(|code| code.as_str()),
        Some("managed.capability-gap")
    );
    // Missing required protocol successor is unsupported-version.
    let mut protocol = coverage_doc();
    protocol.adapter.protocol = "0.6.4".to_owned();
    let error = coverage::admit(assignment, target, &protocol).expect_err("protocol");
    assert_eq!(error.status(), Status::UnsupportedVersion);
    // An unvalidated extension selection never resolves.
    let mut unresolved = coverage_doc();
    unresolved.extensions.push(ExtensionCheck {
        symbol: "legacy_hook".to_owned(),
        binding: "custom".to_owned(),
        contract_digest: PIN_B.to_owned(),
        validated: false,
    });
    let error = coverage::admit(assignment, target, &unresolved).expect_err("extension");
    assert_eq!(error.status(), Status::Invalid);
    assert_eq!(
        error.reason_codes().first().map(|code| code.as_str()),
        Some("managed.extension-unresolved")
    );
}

fn write_input(root: &std::path::Path, path: &str, bytes: &[u8]) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
    std::fs::write(full, bytes).expect("write");
}

fn empty_before() -> BeforeState {
    BeforeState {
        recorded: BTreeMap::new(),
        actual: BTreeMap::new(),
    }
}

#[test]
fn plan_identity_is_deterministic_and_order_insensitive() {
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage_doc()).expect("admission");
    let closure_a = vec![
        InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: PIN_A.to_owned(),
        },
        InputPin {
            path: "attachments/semantics.json".to_owned(),
            digest: PIN_B.to_owned(),
        },
    ];
    let mut closure_b = closure_a.clone();
    closure_b.reverse();
    let plan_a = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage_doc(),
        closure_a.clone(),
        &empty_before(),
        None,
        None,
        hash(&coverage_doc()),
        &test_semantics(),
    )
    .expect("plan");
    let plan_b = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage_doc(),
        closure_b,
        &empty_before(),
        None,
        None,
        hash(&coverage_doc()),
        &test_semantics(),
    )
    .expect("plan");
    assert_eq!(
        plan_a.plan_digest, plan_b.plan_digest,
        "same closure, same plan identity"
    );
    assert_eq!(plan_a.outputs.len(), 4);
    assert!(plan_a
        .outputs
        .iter()
        .all(|output| output.action == OutputAction::Create));
    // A changed coverage pin changes the reviewed identity.
    let plan_c = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage_doc(),
        closure_a.clone(),
        &empty_before(),
        None,
        None,
        PIN_B.to_owned(),
        &test_semantics(),
    )
    .expect("plan");
    assert_ne!(plan_a.plan_digest, plan_c.plan_digest);
}

#[test]
fn preflight_names_the_moved_pin() {
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage_doc()).expect("admission");
    let closure = vec![InputPin {
        path: "model/planner.model.json".to_owned(),
        digest: PIN_A.to_owned(),
    }];
    let mut before = empty_before();
    before.recorded.insert(
        "apps/planner/gen/src/focus-task.ts".to_owned(),
        Recorded {
            digest: PIN_B.to_owned(),
            role: ArtifactRole::Implementation,
            owner_module: "planner".to_owned(),
            owner_target: "node-typescript".to_owned(),
            object_ref: Some(PIN_B.to_owned()),
        },
    );
    before.actual.insert(
        "apps/planner/gen/src/focus-task.ts".to_owned(),
        "sha256:3333333333333333333333333333333333333333333333333333333333333333".to_owned(),
    );
    let error = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage_doc(),
        closure.clone(),
        &before,
        None,
        None,
        hash(&coverage_doc()),
        &test_semantics(),
    )
    .expect_err("drift");
    assert_eq!(
        error.reason_codes().first().map(|code| code.as_str()),
        Some("managed.generated-drift")
    );
    // With a clean before-state the plan builds; a moved input pin is stale.
    let built = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage_doc(),
        closure,
        &empty_before(),
        None,
        None,
        hash(&coverage_doc()),
        &test_semantics(),
    )
    .expect("plan");
    let mut inputs = BTreeMap::new();
    inputs.insert("model/planner.model.json".to_owned(), PIN_B.to_owned());
    let roots = BTreeMap::from([(
        ("planner".to_owned(), "node-typescript".to_owned()),
        "apps/planner/gen".to_owned(),
    )]);
    let error = plan::preflight(&built, &roots, &inputs, &empty_before()).expect_err("stale");
    assert_eq!(
        error.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
}

#[test]
fn apply_transaction_is_recoverable_and_exact() {
    let root = project_root("apply");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let closure = vec![InputPin {
        path: "model/planner.model.json".to_owned(),
        digest: digest(model),
    }];
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        closure,
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let plan_bytes = serde_json::to_vec(&built).expect("wire");
    // Missing confirmation never mutates.
    let refused = apply::apply(
        &root,
        &plan_bytes,
        &coverage_bytes,
        &ownership,
        None,
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid);
    assert!(!history::exists(&root));
    // Wrong confirmation never mutates.
    let refused = apply::apply(
        &root,
        &plan_bytes,
        &coverage_bytes,
        &ownership,
        Some(PIN_A),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid);
    // Dry run writes nothing but previews the inventory.
    let preview = apply::apply(
        &root,
        &plan_bytes,
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        true,
        Some(&test_authority()),
        None,
    );
    assert_eq!(preview.status(), Status::Valid, "{preview:?}");
    assert!(!history::exists(&root));
    // The confirmed apply publishes and records custody.
    let applied = apply::apply(
        &root,
        &plan_bytes,
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let generation = history::current(&root)
        .expect("custody")
        .expect("generation");
    for path in [
        "apps/planner/gen/src/focus-task.ts",
        "apps/planner/gen/test/focus-task.test.ts",
    ] {
        assert!(root.join(path).is_file(), "{path} published");
    }
    // Re-applying the identical plan is stale, never a silent rewrite.
    // The create coordinates are now claimed, so the refusal is the
    // ownership denial of the create-conflict preflight (finding 2).
    let stale = apply::apply(
        &root,
        &plan_bytes,
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(stale.status(), Status::Denied);
    assert_eq!(
        stale.reason_codes().first().map(|code| code.as_str()),
        Some("managed.ownership-denied")
    );
    // A regeneration plan over the committed generation is Unchanged.
    let record = history::record(&root, &generation).expect("record");
    let mut before = empty_before();
    for output in &record.outputs {
        let bytes = std::fs::read(root.join(&output.path)).expect("generated");
        before.recorded.insert(
            output.path.clone(),
            Recorded {
                digest: output.digest.clone(),
                role: output.role,
                owner_module: output.owner_module.clone(),
                owner_target: output.owner_target.clone(),
                object_ref: output.after_ref.value().cloned(),
            },
        );
        before.actual.insert(output.path.clone(), digest(&bytes));
    }
    let rebuilt = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage,
        record
            .input_closure
            .iter()
            .filter(|pin| !pin.path.starts_with(".lekalo/managed-authority/"))
            .cloned()
            .collect(),
        &before,
        Some(generation.clone()),
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    assert!(
        rebuilt
            .outputs
            .iter()
            .all(|output| output.action == OutputAction::Unchanged),
        "byte-stable regeneration"
    );
    assert_eq!(
        generation_identity_of(&rebuilt),
        generation_identity_of(&built),
        "stable generation digest"
    );
    // Manual drift of a generated file blocks regeneration until resolve.
    let handler = root.join("apps/planner/gen/src/focus-task.ts");
    std::fs::write(&handler, b"// manually edited\n").expect("edit");
    let drifted = apply::apply(
        &root,
        &serde_json::to_vec(&rebuilt).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(rebuilt.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(drifted.status(), Status::Invalid);
    assert_eq!(
        drifted.reason_codes().first().map(|code| code.as_str()),
        Some("managed.generated-drift")
    );
    // Resolve restores the exact recorded bytes.
    let resolution = rollback::resolve(&root, "apps/planner/gen/src/focus-task.ts", None, false);
    assert_eq!(
        resolution.status(),
        Status::Invalid,
        "resolve requires its confirmation"
    );
    let preview = rollback::resolve(&root, "apps/planner/gen/src/focus-task.ts", None, true);
    assert_eq!(preview.status(), Status::Valid, "{preview:?}");
    let resolve_digest = resolve_plan_digest(&preview);
    let restored = rollback::resolve(
        &root,
        "apps/planner/gen/src/focus-task.ts",
        Some(&format!("sha256:{resolve_digest}")),
        false,
    );
    assert_eq!(restored.status(), Status::Valid, "{restored:?}");
    let record = history::record(&root, &generation).expect("record");
    let expected = history::object(
        &root,
        record
            .outputs
            .iter()
            .find(|o| o.path == "apps/planner/gen/src/focus-task.ts")
            .expect("output")
            .after_ref
            .value()
            .expect("ref"),
    )
    .expect("object");
    assert_eq!(
        std::fs::read(&handler).expect("bytes"),
        expected,
        "exact recorded bytes"
    );
    // An unresolved journal blocks every further mutation.
    let blocker = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "blocked-0001".to_owned(),
        state: TransactionState::Prepared,
        action: TransactionAction::Apply,
        plan_digest: State::known(PIN_A.to_owned()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: Vec::new(),
        affected_files: Vec::new(),
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &blocker).expect("journal");
    let blocked = apply::apply(
        &root,
        &serde_json::to_vec(&rebuilt).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(rebuilt.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(
        blocked.reason_codes().first().map(|code| code.as_str()),
        Some("managed.recovery-required")
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn generation_identity_of(plan: &GenerationPlan) -> String {
    super::inventory_digest(
        plan.input_closure.iter(),
        plan.outputs.iter().map(|output| {
            (
                output.path.as_str(),
                &output.role,
                output.owner_module.as_str(),
                output.owner_target.as_str(),
                output
                    .after_digest
                    .value()
                    .map(|digest| digest.as_str())
                    .unwrap_or_default(),
            )
        }),
    )
}

fn resolve_plan_digest(receipt: &DomainResult) -> String {
    let json = receipt.to_json_string();
    let value: serde_json::Value = serde_json::from_str(&json).expect("envelope");
    value["managed"]["planDigest"]
        .as_str()
        .expect("digest")
        .trim_start_matches("sha256:")
        .to_owned()
}

#[test]
fn rollback_restores_the_exact_previous_generation() {
    let root = project_root("rollback");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let first = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&first).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(first.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let first_generation = history::current(&root)
        .expect("custody")
        .expect("generation");
    // A second generation over changed inputs replaces the inventory.
    let model_v2 = b"{ \"planner\": true, \"v\": 2 }\n";
    write_input(&root, "model/planner.model.json", model_v2);
    let second = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model_v2),
        }],
        &{
            let record = history::record(&root, &first_generation).expect("record");
            let mut before = empty_before();
            for output in &record.outputs {
                let bytes = std::fs::read(root.join(&output.path)).expect("generated");
                before.recorded.insert(
                    output.path.clone(),
                    Recorded {
                        digest: output.digest.clone(),
                        role: output.role,
                        owner_module: output.owner_module.clone(),
                        owner_target: output.owner_target.clone(),
                        object_ref: output.after_ref.value().cloned(),
                    },
                );
                before.actual.insert(output.path.clone(), digest(&bytes));
            }
            before
        },
        Some(first_generation.clone()),
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    // Change one requirement so the bytes differ; rebuild over the new plan.
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&second).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(second.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let second_generation = history::current(&root)
        .expect("custody")
        .expect("generation");
    assert_ne!(first_generation, second_generation);
    // Rollback preview builds the exact restore set with no writes.
    let preview = rollback::build(&root, &first_generation).expect("rollback plan");
    assert!(!preview.restore.is_empty());
    let bytes = serde_json::to_vec(&preview).expect("wire");
    let refused = rollback::apply(&root, &bytes, None, false, Some(&test_authority()));
    assert_eq!(refused.status(), Status::Invalid);
    // The destination input closure is checked against the live inputs:
    // the model moved between generations, so the destination is refused
    // until the pinned input bytes are back (finding G).
    let stale = rollback::apply(
        &root,
        &bytes,
        Some(preview.plan_digest.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(stale.status(), Status::Invalid, "{stale:?}");
    assert_eq!(
        stale.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    // With the destination's pinned inputs restored, the rollback runs.
    write_input(&root, "model/planner.model.json", model);
    let restored = rollback::apply(
        &root,
        &bytes,
        Some(preview.plan_digest.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(restored.status(), Status::Valid, "{restored:?}");
    assert_eq!(
        history::current(&root)
            .expect("custody")
            .expect("generation"),
        first_generation
    );
    // The restored record and bytes match the first generation exactly.
    let record = history::record(&root, &first_generation).expect("record");
    for output in &record.outputs {
        let bytes = history::object(&root, output.after_ref.value().expect("ref")).expect("object");
        assert_eq!(std::fs::read(root.join(&output.path)).expect("file"), bytes);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn equivalence_adjudication_never_lowers_the_denominator() {
    let evidence =
        |verdict: ScenarioOutcome, gaps: Vec<String>, declared: EquivalenceVerdict| Equivalence {
            schema_version: header("managed-equivalence").0,
            identity: header("managed-equivalence").1,
            module: "planner".to_owned(),
            target: "node-typescript".to_owned(),
            implementation_digest: PIN_A.to_owned(),
            reference_digest: State::known(PIN_B.to_owned()),
            scenarios: vec![ScenarioVerdict {
                scenario: "planner.focus".to_owned(),
                digest: PIN_B.to_owned(),
                verdict,
            }],
            execution: vec![ScenarioExecution {
                scenario: "planner.focus".to_owned(),
                observed: PIN_B.to_owned(),
            }],
            gaps,
            verdict: declared,
        };
    let matching = serde_json::to_vec(&evidence(
        ScenarioOutcome::Match,
        Vec::new(),
        EquivalenceVerdict::Match,
    ))
    .expect("wire");
    let result = super::equivalence::adjudicate(&matching);
    assert_eq!(result.status(), Status::Valid);
    let mismatch = serde_json::to_vec(&evidence(
        ScenarioOutcome::Mismatch,
        Vec::new(),
        EquivalenceVerdict::Match,
    ))
    .expect("wire");
    let result = super::equivalence::adjudicate(&mismatch);
    assert_eq!(result.status(), Status::Invalid);
    assert_eq!(
        result.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-failed")
    );
    let unsupported = serde_json::to_vec(&evidence(
        ScenarioOutcome::Unsupported,
        Vec::new(),
        EquivalenceVerdict::Match,
    ))
    .expect("wire");
    let result = super::equivalence::adjudicate(&unsupported);
    assert_eq!(result.status(), Status::Unsupported);
    assert_eq!(
        result.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-incomplete")
    );
    let gap = serde_json::to_vec(&evidence(
        ScenarioOutcome::Match,
        vec!["expression.regex".to_owned()],
        EquivalenceVerdict::Match,
    ))
    .expect("wire");
    let result = super::equivalence::adjudicate(&gap);
    assert_eq!(result.status(), Status::Unsupported);
}

#[test]
fn generated_roots_never_nest_between_sort_neighbors() {
    // `gen` < `gen-x` < `gen/inner` in byte order: the adjacent-window
    // spelling of the nesting check accepted the descendant because the
    // sibling sorted between (finding B).
    let mut ownership = ownership_doc();
    ownership.modules.push(ModuleAssignment {
        module: "alpha".to_owned(),
        mode: Mode::Managed,
        targets: vec![TargetAssignment {
            generated_root: "apps/planner/gen-x".to_owned(),
            ..target_assignment()
        }],
    });
    ownership.modules.push(ModuleAssignment {
        module: "beta".to_owned(),
        mode: Mode::Managed,
        targets: vec![TargetAssignment {
            generated_root: "apps/planner/gen/inner".to_owned(),
            ..target_assignment()
        }],
    });
    let error = parse_ownership(&serde_json::to_vec(&ownership).expect("wire"))
        .expect_err("nested root across sort neighbors");
    assert_eq!(error.status(), Status::Invalid);
}

#[test]
fn nested_constraints_match_the_published_schema() {
    // A hostile nested reference is refused by the runtime admission path,
    // exactly like the published schema (finding M).
    let mut ownership = ownership_doc();
    ownership.modules[0].targets[0].profile_ref.digest = "not-a-digest".to_owned();
    let error =
        parse_ownership(&serde_json::to_vec(&ownership).expect("wire")).expect_err("digest");
    assert_eq!(error.status(), Status::Invalid);
    let mut coverage = coverage_doc();
    coverage.recipe.digest = "sha256:nothex".to_owned();
    let error =
        parse_coverage(&serde_json::to_vec(&coverage).expect("wire")).expect_err("coverage pin");
    assert_eq!(error.status(), Status::Invalid);
    let mut adapter = coverage_doc();
    adapter.adapter.package_digest = "not-a-digest".to_owned();
    let error = parse_coverage(&serde_json::to_vec(&adapter).expect("wire")).expect_err("adapter");
    assert_eq!(error.status(), Status::Invalid);
    // A requirement without projection/sample is closed-wire refused —
    // the same minimum edit the tightened schema refuses (finding C).
    let bare = coverage_doc();
    let requirement = serde_json::to_value(&bare.requirements[0]).expect("wire");
    let mut removed = serde_json::Map::new();
    for (key, value) in requirement.as_object().expect("object") {
        if key != "projection" && key != "sample" {
            removed.insert(key.clone(), value.clone());
        }
    }
    let mut document = serde_json::to_value(&bare).expect("wire");
    document["requirements"][0] = serde_json::Value::Object(removed);
    let error =
        parse_coverage(&serde_json::to_vec(&document).expect("wire")).expect_err("closed wire");
    assert_eq!(error.status(), Status::Invalid);
}

#[test]
fn tampered_records_are_refused_without_touching_files() {
    // Any edit to a stored generation record breaks its content-bound
    // identity; restore authority never flows from a forged record (I).
    let root = project_root("tamper");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let generation = history::current(&root)
        .expect("custody")
        .expect("generation");
    // Retarget one recorded output at an unrelated maintained file.
    let record = history::record(&root, &generation).expect("record");
    let mut forged = record.clone();
    let target_path = "apps/planner/gen/src/focus-task.ts";
    let output = forged
        .outputs
        .iter_mut()
        .find(|output| output.path == target_path)
        .expect("output");
    output.path = "maintained/handler.ts".to_owned();
    forged
        .outputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    let hex = generation.strip_prefix("sha256:").expect("digest");
    write_input(
        &root,
        &format!(".lekalo/generated/manifests/managed/objects/{hex}"),
        &serde_json::to_vec(&forged).expect("wire"),
    );
    // The resolve refuses: lookup identity, inventory identity, and
    // custody objects are re-derived before any byte moves.
    write_input(&root, "apps/planner/gen/src/focus-task.ts", b"// drifted\n");
    let resolution = rollback::resolve(&root, "maintained/handler.ts", None, true);
    assert_eq!(resolution.status(), Status::Invalid);
    assert_eq!(
        std::fs::read(root.join("maintained/handler.ts"))
            .map(|_| "exists")
            .unwrap_or("absent"),
        "absent",
        "the forged target was never created"
    );
    let forged_resolution = rollback::resolve(&root, target_path, None, true);
    assert_ne!(
        forged_resolution.status(),
        Status::Valid,
        "forged custody never resolves"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn interrupted_publication_recovers_to_a_defined_state() {
    // The defined recovery path: an interrupted apply journal either
    // completes (disk already matches the recorded generation) or is
    // abandoned by converging the affected paths to the committed bytes;
    // the barrier clears only through recovery (finding J).
    let root = project_root("recovery");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    // A prepared journal with no disk effect abandons cleanly (the exact
    // shape `staged_apply` writes first: `prepared`, empty pending set,
    // no generation pin — never-emitted state/pending combinations are
    // refused outright since r6-r7, devin F1c).
    let generation = history::current(&root)
        .expect("custody")
        .expect("generation");
    let blocker = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "interrupted-0001".to_owned(),
        state: TransactionState::Prepared,
        action: TransactionAction::Apply,
        plan_digest: State::known(built.plan_digest.clone()),
        generation_digest: State::Absent,
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: Vec::new(),
        affected_files: Vec::new(),
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &blocker).expect("journal");
    // Every mutator is blocked while the journal is unresolved.
    let blocked = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(blocked.status(), Status::Invalid);
    assert_eq!(
        blocked.reason_codes().first().map(|code| code.as_str()),
        Some("managed.recovery-required")
    );
    // The defined recovery path reconciles the journal.
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    assert!(history::unresolved(&root).expect("custody").is_empty());
    // The committed generation is untouched by the abandoned journal.
    assert_eq!(history::current(&root).expect("custody"), Some(generation));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolve_confirmation_binds_the_drifted_before_state() {
    // The confirmation digest covers the drifted bytes: editing the file
    // after the reviewed preview invalidates the confirmation (finding A).
    let root = project_root("resolve-bind");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let path = "apps/planner/gen/src/focus-task.ts";
    write_input(&root, path, b"// drift one\n");
    let preview = rollback::resolve(&root, path, None, true);
    assert_eq!(preview.status(), Status::Valid, "{preview:?}");
    let digest = resolve_plan_digest(&preview);
    // The file moves again after the reviewed preview: the same
    // confirmation must now refuse and preserve the unseen bytes.
    write_input(&root, path, b"// drift two\n");
    let stale = rollback::resolve(&root, path, Some(&format!("sha256:{digest}")), false);
    assert_eq!(stale.status(), Status::Invalid, "{stale:?}");
    assert_eq!(
        std::fs::read(root.join(path)).expect("bytes"),
        b"// drift two\n".to_vec(),
        "stale confirmation never moves the file"
    );
    // Without a confirmation the resolve never moves anything either.
    let unconfirmed = rollback::resolve(&root, path, None, false);
    assert_eq!(unconfirmed.status(), Status::Invalid);
    assert_eq!(
        std::fs::read(root.join(path)).expect("bytes"),
        b"// drift two\n".to_vec()
    );
    // The exact confirmation of the fresh preview restores the recorded
    // bytes.
    let fresh = rollback::resolve(&root, path, None, true);
    let fresh_digest = resolve_plan_digest(&fresh);
    let restored = rollback::resolve(&root, path, Some(&format!("sha256:{fresh_digest}")), false);
    assert_eq!(restored.status(), Status::Valid, "{restored:?}");
    let record = history::record(
        &root,
        &history::current(&root).expect("current").expect("gen"),
    )
    .expect("record");
    let expected = history::object(
        &root,
        record
            .outputs
            .iter()
            .find(|output| output.path == path)
            .expect("output")
            .after_ref
            .value()
            .expect("ref"),
    )
    .expect("object");
    assert_eq!(std::fs::read(root.join(path)).expect("bytes"), expected);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn forged_recovery_journal_never_deletes_maintained_files() {
    // A schema-shaped journal naming a maintained file with no plan
    // binding is refused; the maintained file survives (finding 1).
    let root = project_root("forged-recovery");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    write_input(&root, "maintained/keep.ts", b"export const keep = 1;\n");
    let blocker = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "forged-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::Absent,
        generation_digest: State::Absent,
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: "maintained/keep.ts".to_owned(),
            op: PendingOp::Publish,
        }],
        affected_files: vec!["maintained/keep.ts".to_owned()],
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &blocker).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join("maintained/keep.ts")).expect("maintained bytes"),
        b"export const keep = 1;\n".to_vec(),
        "the forged journal never deletes maintained files",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn forged_plan_edits_never_overwrite_unclaimed_files() {
    // A create coordinate occupied by an unclaimed maintained file is an
    // ownership denial; the maintained bytes survive (finding 2 / Q).
    let root = project_root("forged-plan");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    write_input(
        &root,
        "apps/planner/gen/src/focus-task.ts",
        b"// maintained handler\n",
    );
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let mut before = empty_before();
    before.actual.insert(
        "apps/planner/gen/src/focus-task.ts".to_owned(),
        digest(b"// maintained handler\n"),
    );
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &before,
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    );
    assert_eq!(
        built.expect_err("unclaimed create").status(),
        Status::Denied
    );
    assert_eq!(
        std::fs::read(root.join("apps/planner/gen/src/focus-task.ts")).expect("bytes"),
        b"// maintained handler\n".to_vec(),
        "the unclaimed file was never overwritten",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rehashed_records_never_authorize_restore_outside_ownership() {
    // A caller-authored record retargeted at a maintained path and
    // restored under a fresh self-consistent digest never authenticates:
    // resolve refuses before any byte moves (finding 3 / I).
    let root = project_root("rehashed-record");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let generation = history::current(&root).expect("custody").expect("gen");
    let hex = generation.strip_prefix("sha256:").expect("digest");
    let record_path = root.join(format!(".lekalo/generated/manifests/managed/objects/{hex}"));
    let original = std::fs::read(&record_path).expect("record");
    let mut forged: GenerationRecord = serde_json::from_slice(&original).expect("record wire");
    let output = forged
        .outputs
        .iter_mut()
        .find(|output| output.path == "apps/planner/gen/src/focus-task.ts")
        .expect("output");
    output.path = "maintained/victim.ts".to_owned();
    forged
        .outputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    forged.generation_digest = super::generation_digest(&forged);
    forged.record_digest = super::record_identity(&forged);
    let forged_hex = forged
        .record_digest
        .strip_prefix("sha256:")
        .expect("digest")
        .to_owned();
    write_input(
        &root,
        &format!(".lekalo/generated/manifests/managed/objects/{forged_hex}"),
        &serde_json::to_vec(&forged).expect("wire"),
    );
    write_input(
        &root,
        ".lekalo/generated/manifests/managed/current",
        format!("{forged_hex}\n").as_bytes(),
    );
    write_input(&root, "maintained/victim.ts", b"// maintained source\n");
    let resolution = rollback::resolve(&root, "maintained/victim.ts", None, true);
    assert_eq!(resolution.status(), Status::Invalid, "{resolution:?}");
    assert_eq!(
        std::fs::read(root.join("maintained/victim.ts")).expect("bytes"),
        b"// maintained source\n".to_vec(),
        "the rehashed record never overwrites the maintained file",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn zero_hash_equivalence_scenarios_are_missing_evidence_never_match() {
    // Scenario or reference pins that resolve to no custody object are
    // incomplete evidence: managed.equivalence-incomplete (finding 5).
    let root = project_root("equivalence-evidence");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    // The implementation pin is the digest of the **live** generated
    // bytes; the scenario and reference pins are zero hashes that neither
    // name the retained executable scenario nor resolve to any custody
    // object: the verdict is incomplete evidence, never a match.
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    let implementation = super::equivalence::live_implementation_digest_for_test(
        &root,
        &record,
        "planner",
        "node-typescript",
    )
    .expect("live inventory");
    let zero = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let evidence = Equivalence {
        schema_version: header("managed-equivalence").0,
        identity: header("managed-equivalence").1,
        module: "planner".to_owned(),
        target: "node-typescript".to_owned(),
        implementation_digest: implementation,
        reference_digest: State::known(zero.to_owned()),
        scenarios: vec![ScenarioVerdict {
            scenario: "planner.focus_task".to_owned(),
            digest: zero.to_owned(),
            verdict: ScenarioOutcome::Match,
        }],
        execution: vec![ScenarioExecution {
            scenario: "planner.focus_task".to_owned(),
            observed: zero.to_owned(),
        }],
        gaps: Vec::new(),
        verdict: EquivalenceVerdict::Match,
    };
    let bytes = serde_json::to_vec(&evidence).expect("wire");
    let result = super::equivalence::adjudicate_for(&root, &bytes);
    assert_eq!(result.status(), Status::Unsupported, "{result:?}");
    assert_eq!(
        result.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-incomplete")
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn hash_correct_binding_without_the_export_is_unresolved() {
    // A validated row whose file carries the exact pinned digest but no
    // export of the declared symbol is managed.extension-unresolved
    // (finding 6 / K).
    let root = project_root("binding-export");
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let mut coverage = coverage_doc();
    let bytes = b"export {};\n".to_vec();
    coverage.extensions.push(ExtensionCheck {
        symbol: "missing_hook".to_owned(),
        binding: "custom/empty.ts".to_owned(),
        contract_digest: digest(&bytes),
        validated: true,
    });
    write_input(&root, "custom/empty.ts", &bytes);
    let error = super::bindings::resolve(&root, target, &coverage).expect_err("export");
    assert_eq!(error.status(), Status::Invalid);
    assert_eq!(
        error.reason_codes().first().map(|code| code.as_str()),
        Some("managed.extension-unresolved")
    );
    let bytes = b"export function missingHook(input) { return input; }\n".to_vec();
    write_input(&root, "custom/empty.ts", &bytes);
    coverage.extensions[0].contract_digest = digest(&bytes);
    assert!(super::bindings::resolve(&root, target, &coverage).is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rendered_implementations_invoke_resolved_bindings() {
    // The generated implementation imports and calls the resolved binding
    // (finding 6): the hook is live in the handler, not test-only.
    let mut coverage = coverage_doc();
    let bytes = b"export function legacyHook(input) { return input; }\n".to_vec();
    coverage.extensions.push(ExtensionCheck {
        symbol: "legacy_hook".to_owned(),
        binding: "src/extensions/legacy_hook.ts".to_owned(),
        contract_digest: digest(&bytes),
        validated: true,
    });
    let inventory =
        render_inventory(&coverage, "apps/planner/gen", &test_semantics()).expect("inventory");
    let handler = inventory
        .iter()
        .find(|artifact| artifact.path == "apps/planner/gen/src/focus-task.ts")
        .expect("handler");
    let text = String::from_utf8(handler.bytes.clone()).expect("utf8");
    assert!(
        text.contains("import { legacyHook } from"),
        "the handler imports the binding: {text}"
    );
    assert!(
        text.contains("legacyHook(structuredClone(input));"),
        "the handler invokes the binding: {text}"
    );
}

#[test]
fn broken_or_missing_live_authority_refuses_rollback() {
    // Destination compatibility fails closed (finding 8): a live lock
    // that no longer parses, or is missing, refuses the rollback, and so
    // does a live canonical Model that moved — even under unchanged lock
    // bytes — because the live derived authority no longer matches the
    // destination's derived custody pins.
    let root = project_root("rollback-lock");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let destination = history::current(&root).expect("custody").expect("gen");
    // A second generation so the destination differs from the current one.
    let model_v2 = b"{ \"planner\": true, \"v\": 2 }\n";
    write_input(&root, "model/planner.model.json", model_v2);
    let second = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model_v2),
        }],
        &{
            let record = history::record(&root, &destination).expect("record");
            let mut before = empty_before();
            for output in &record.outputs {
                let bytes = std::fs::read(root.join(&output.path)).expect("generated");
                before.recorded.insert(
                    output.path.clone(),
                    Recorded {
                        digest: output.digest.clone(),
                        role: output.role,
                        owner_module: output.owner_module.clone(),
                        owner_target: output.owner_target.clone(),
                        object_ref: output.after_ref.value().cloned(),
                    },
                );
                before.actual.insert(output.path.clone(), digest(&bytes));
            }
            before
        },
        Some(destination.clone()),
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&second).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(second.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let destination_inputs_restored = || {
        let preview = rollback::build(&root, &destination).expect("rollback plan");
        let bytes = serde_json::to_vec(&preview).expect("wire");
        // The destination's explicit inputs are the first model revision;
        // put them back so only the authority under test can be the
        // reason for a refusal.
        write_input(&root, "model/planner.model.json", model);
        (preview.plan_digest, bytes)
    };
    // A live lock that no longer parses: the destination lock revision
    // cannot be confirmed, so the rollback is refused before any byte
    // moves.
    write_input(&root, "lekalo.lock", b"broken lock");
    let (digest_value, bytes) = destination_inputs_restored();
    let refused = rollback::apply(
        &root,
        &bytes,
        Some(digest_value.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    // A missing lock fails closed the same way.
    let _ = std::fs::remove_file(root.join("lekalo.lock"));
    let (digest_value, bytes) = destination_inputs_restored();
    let refused = rollback::apply(
        &root,
        &bytes,
        Some(digest_value.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    // A moved canonical Model under unchanged lock bytes: the live derived
    // authority (exactly what the CLI presents) no longer matches the
    // destination's derived custody pins.
    write_test_lock(&root);
    let mut moved = test_authority();
    moved.model_digest = PIN_C.to_owned();
    let (digest_value, bytes) = destination_inputs_restored();
    let refused = rollback::apply(
        &root,
        &bytes,
        Some(digest_value.as_str()),
        false,
        Some(&moved),
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    // Without any live authority the refusal is unconditional.
    let (digest_value, bytes) = destination_inputs_restored();
    let refused = rollback::apply(&root, &bytes, Some(digest_value.as_str()), false, None);
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    // With the exact destination authority back, the rollback runs and
    // restores the recorded bytes.
    let (digest_value, bytes) = destination_inputs_restored();
    let restored = rollback::apply(
        &root,
        &bytes,
        Some(digest_value.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(restored.status(), Status::Valid, "{restored:?}");
    assert_eq!(
        history::current(&root).expect("custody").expect("gen"),
        destination
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn recovery_keeps_the_committed_public_manifest() {
    // An abandoned journal leaves the committed public manifest exactly
    // the committed bytes (finding 9, recovery side).
    let root = project_root("recovery-manifest");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let committed_manifest =
        std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)).expect("manifest");
    let blocker = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "blocked-0002".to_owned(),
        state: TransactionState::Prepared,
        action: TransactionAction::Apply,
        plan_digest: State::known(built.plan_digest.clone()),
        generation_digest: State::Absent,
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: Vec::new(),
        affected_files: Vec::new(),
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &blocker).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)).expect("manifest"),
        committed_manifest,
        "the committed manifest is untouched by an abandoned no-op journal",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stale_recovery_lock_clear_never_escapes_the_project() {
    // The mutation lock is cleared only through the confined capability:
    // a junctioned custody home refuses before any outside byte moves
    // (finding 11). Windows-only: junctions are an NTFS reparse concept.
    if !cfg!(windows) {
        return;
    }
    let root = project_root("recovery-lock");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let _ = std::fs::remove_dir_all(root.join(".lekalo"));
    let outside = root.join("outside-custody");
    std::fs::create_dir_all(outside.join("generated/manifests/managed")).expect("outside");
    std::fs::write(
        outside.join("generated/manifests/managed/mutation.lock"),
        b"stale",
    )
    .expect("sentinel");
    junction(&outside, &root.join(".lekalo"));
    let receipt = recovery::run(&root, &test_authority_of);
    assert_ne!(receipt.status(), Status::Valid, "{receipt:?}");
    assert!(
        outside
            .join("generated/manifests/managed/mutation.lock")
            .is_file(),
        "the outside lock was never removed through the junction",
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

// ---- Round-3 regressions (issue #41 r3 findings) ---------------------------

fn apply_first_generation(root: &std::path::Path) -> super::wire::GenerationPlan {
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model = b"{ \"planner\": true }\n";
    write_input(root, "model/planner.model.json", model);
    // The ownership document is a pinned closure input of every CLI plan;
    // recovery's ownership binding (r6-r7 devin F2) requires exactly this
    // pin, so the fixture apply carries it too.
    let ownership_bytes = serde_json::to_vec(&ownership).expect("wire");
    write_input(root, "ownership.json", &ownership_bytes);
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![
            InputPin {
                path: "model/planner.model.json".to_owned(),
                digest: digest(model),
            },
            InputPin {
                path: "ownership.json".to_owned(),
                digest: digest(&ownership_bytes),
            },
        ],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    built
}

#[test]
fn write_file_refuses_a_shared_inode_and_preserves_the_alias() {
    // A hardlink at a generated path is a regular file: the mutation must
    // refuse instead of truncating the outside bytes (finding 9).
    let root = project_root("hardlink-leaf");
    let outside = root.join("outside-sentinel.ts");
    std::fs::write(&outside, b"OUTSIDE\n").expect("sentinel");
    let alias = root.join("apps/planner/gen/src/focus-task.ts");
    std::fs::create_dir_all(alias.parent().expect("parent")).expect("mkdir");
    std::fs::hard_link(&outside, &alias).expect("hardlink");
    let written = fs::write_file(&root, "apps/planner/gen/src/focus-task.ts", b"MANAGED\n");
    assert!(written.is_err(), "a shared inode must refuse");
    assert_eq!(std::fs::read(&outside).expect("outside"), b"OUTSIDE\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn staged_write_refuses_a_shared_stage_leaf() {
    // A hardlink planted at the stage name is never truncated (finding 9).
    let root = project_root("hardlink-stage");
    let outside = root.join("outside-sentinel.json");
    std::fs::write(&outside, b"OUTSIDE\n").expect("sentinel");
    write_input(&root, "custody/keep.txt", b"keep\n");
    std::fs::create_dir_all(root.join("custody")).expect("mkdir");
    let stage = root.join("custody/leaf.stage");
    std::fs::hard_link(&outside, &stage).expect("hardlink");
    let staged = fs::write_staged(&root, "custody/leaf.json", "custody/leaf.stage", b"NEW\n");
    assert!(staged.is_err(), "a shared stage leaf must refuse");
    assert_eq!(std::fs::read(&outside).expect("outside"), b"OUTSIDE\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn recovery_clears_the_lock_only_after_proving_it_stale() {
    // A live holder (this guard) blocks recovery; a dropped guard is
    // provably abandoned and is cleared (finding 3).
    let root = project_root("lock-live");
    let guard = history::lock(&root).expect("lock");
    write_input(
        &root,
        &history::custody_logical(&format!("transactions/{}.json", "stale-0001")),
        br#"{
  "schemaVersion": "lekalo/managed-transaction/v0.8.0",
  "identity": "dev.lekalo.managed-transaction@0.8.0",
  "transactionId": "stale-0001",
  "state": "prepared",
  "action": "apply",
  "planDigest": { "state": "absent" },
  "generationDigest": { "state": "absent" },
  "beforeManifestDigest": { "state": "absent" },
  "afterManifestDigest": { "state": "absent" },
  "pending": [],
  "affectedFiles": [],
  "conflict": { "state": "absent" },
  "beforeDigest": { "state": "absent" }
}"#,
    );
    let blocked = recovery::run(&root, &test_authority_of);
    assert_eq!(blocked.status(), Status::Invalid, "{blocked:?}");
    assert!(
        root.join(history::custody_logical(history::MUTATION_LOCK))
            .is_file(),
        "the live lock survives",
    );
    drop(guard);
    let cleared = recovery::run(&root, &test_authority_of);
    assert_eq!(cleared.status(), Status::Valid, "{cleared:?}");
    assert!(
        !root
            .join(history::custody_logical(history::MUTATION_LOCK))
            .exists(),
        "the stale lock is cleared",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn forged_journal_cannot_delete_unclaimed_paths() {
    // A journal naming an unclaimed path (even with a borrowed real plan
    // digest) converges nothing: exact reviewed outputs only (finding 1).
    let root = project_root("forged-delete");
    apply_first_generation(&root);
    let victim = root.join("apps/planner/gen/maintained.ts");
    std::fs::write(&victim, b"// maintained\n").expect("victim");
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "forged-delete-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(record.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: "apps/planner/gen/maintained.ts".to_owned(),
            op: PendingOp::Delete,
        }],
        affected_files: vec!["apps/planner/gen/maintained.ts".to_owned()],
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_ne!(receipt.status(), Status::Valid, "{receipt:?}");
    assert_eq!(
        std::fs::read(&victim).expect("victim"),
        b"// maintained\n",
        "the unclaimed victim is never deletable",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn first_publication_journal_converges_to_absence_and_retries() {
    // An interrupted first publication (no committed generation pointer)
    // converges its published bytes to absence and a fresh apply succeeds
    // (finding 11). Since r4 finding 1 the journal always carries its
    // generation pin before naming pending moves — the crash shapes are a
    // journal with an empty pending set (nothing moved) or one anchored to
    // the retained record; both converge, and a pending set without its
    // anchored generation pin is exactly the forged shape recovery now
    // refuses (see `forged_apply_journal_without_anchor_never_moves`).
    let root = project_root("first-pub-undo");
    let built = apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    // Simulate a crash before any application byte moved: rewind the
    // pointer to absence and leave the journal unresolved in its
    // publishing state, anchored to the retained record.
    let _ = std::fs::remove_file(root.join(history::custody_logical("current")));
    for path in record
        .outputs
        .iter()
        .map(|output| output.path.as_str())
        .chain([super::custody::MANIFEST_LOGICAL])
    {
        let full = root.join(path);
        if full.is_file() {
            std::fs::remove_file(full).expect("unpublish");
        }
    }
    let mut pending: Vec<Pending> = record
        .outputs
        .iter()
        .map(|output| Pending {
            path: output.path.clone(),
            op: PendingOp::Publish,
        })
        .collect();
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    let mut affected: Vec<String> = record
        .outputs
        .iter()
        .map(|output| output.path.clone())
        .collect();
    affected.sort();
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "firstpub00010000".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(record.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending,
        affected_files: affected,
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    assert!(
        !root.join("apps/planner/gen/src/focus-task.ts").exists(),
        "published bytes converge to absence",
    );
    assert!(history::unresolved(&root).expect("custody").is_empty());
    // The fresh apply succeeds without any custody reset.
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let ownership = ownership_doc();
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rollback_recreates_outputs_deleted_by_a_later_generation() {
    // The canonical rollback-to-previous-manifest path: outputs an
    // intermediate generation deleted are recreated from the destination's
    // retained bytes (findings 3/14).
    let root = project_root("rollback-recreate");
    apply_first_generation(&root);
    let first = history::current(&root).expect("custody").expect("gen");
    // A second generation that deletes the focus-task outputs: drop the
    // requirement from the coverage and rebuild over the recorded state.
    let ownership = ownership_doc();
    let mut coverage = coverage_doc();
    coverage
        .requirements
        .retain(|requirement| requirement.id != "planner.focus_task");
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    let mut before = empty_before();
    for output in &record.outputs {
        before.recorded.insert(
            output.path.clone(),
            super::plan::Recorded {
                digest: output.digest.clone(),
                role: output.role,
                owner_module: output.owner_module.clone(),
                owner_target: output.owner_target.clone(),
                object_ref: output.after_ref.value().cloned(),
            },
        );
        let bytes = fs::read_file(&root, &output.path)
            .expect("read")
            .expect("bytes");
        before.actual.insert(output.path.clone(), digest(&bytes));
    }
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let second = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(b"{ \"planner\": true }\n"),
        }],
        &before,
        Some(generation),
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("second plan");
    assert!(second
        .outputs
        .iter()
        .any(|output| output.action == OutputAction::Delete));
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&second).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(second.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    assert!(
        !root.join("apps/planner/gen/src/focus-task.ts").exists(),
        "the second generation deleted the outputs",
    );
    // Rollback to the first generation must recreate the deleted outputs.
    let preview = rollback::build(&root, &first).expect("rollback plan");
    let bytes = serde_json::to_vec(&preview).expect("wire");
    let confirmed = rollback::apply(
        &root,
        &bytes,
        Some(preview.plan_digest.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(confirmed.status(), Status::Valid, "{confirmed:?}");
    assert!(
        root.join("apps/planner/gen/src/focus-task.ts").is_file(),
        "the deleted outputs are recreated from retained bytes",
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn binding_qualification_requires_real_callable_exports() {
    // A commented-out export and a non-callable named export never qualify
    // (finding 6 / codex 6).
    let callable = b"export function hook(input) { return input; }\n";
    assert!(super::bindings::exports_symbol(callable, "hook"));
    let arrow = b"export const hook = (input) => input;\nexport {};\n";
    assert!(super::bindings::exports_symbol(arrow, "hook"));
    let commented = b"/* export function hook() {} */\n// export function hook() {}\nexport {};\n";
    assert!(
        !super::bindings::exports_symbol(commented, "hook"),
        "a commented-out export is invisible",
    );
    let noncallable = b"export const hook = 7;\nexport {};\n";
    assert!(
        !super::bindings::exports_symbol(noncallable, "hook"),
        "a non-callable named export is refused",
    );
    let renamed = b"const internal = () => 1;\nexport { internal as hook };\n";
    assert!(super::bindings::exports_symbol(renamed, "hook"));
    let unterminated = b"/* export function hook() {}\n";
    assert!(
        !super::bindings::exports_symbol(unterminated, "hook"),
        "an unterminated comment fails closed",
    );
}

#[test]
fn equivalence_fails_on_live_drift_even_with_correct_evidence() {
    // The implementation pin is recomputed from the live bytes at
    // adjudication: manual drift fails the gate (finding 5).
    let root = project_root("equivalence-drift");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    let implementation = super::equivalence::live_implementation_digest_for_test(
        &root,
        &record,
        "planner",
        "node-typescript",
    )
    .expect("live inventory");
    let reference = record
        .outputs
        .iter()
        .find(|output| output.path == "apps/planner/gen/src/focus-task.ts")
        .expect("reference")
        .digest
        .clone();
    let scenario_digest = |file: &str| {
        record
            .outputs
            .iter()
            .find(|output| output.path == file)
            .expect("scenario test")
            .digest
            .clone()
    };
    let evidence = Equivalence {
        schema_version: header("managed-equivalence").0,
        identity: header("managed-equivalence").1,
        module: "planner".to_owned(),
        target: "node-typescript".to_owned(),
        implementation_digest: implementation,
        reference_digest: State::known(reference),
        scenarios: vec![
            ScenarioVerdict {
                scenario: "planner.complete_task".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/complete-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            },
            ScenarioVerdict {
                scenario: "planner.focus_task".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/focus-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            },
        ],
        // Executed-outcome receipts (r4 finding 2): the digests of the
        // recomputed executed outputs — the pinned projection applied to
        // the pinned sample inputs of the retained coverage.
        execution: vec![
            ScenarioExecution {
                scenario: "planner.complete_task".to_owned(),
                observed: "sha256:4ec9852ef939bad75ea72af4e8ed09668f78d38a01a814262048448f3fb83374"
                    .to_owned(),
            },
            ScenarioExecution {
                scenario: "planner.focus_task".to_owned(),
                observed: "sha256:4ec9852ef939bad75ea72af4e8ed09668f78d38a01a814262048448f3fb83374"
                    .to_owned(),
            },
        ],
        gaps: Vec::new(),
        verdict: EquivalenceVerdict::Match,
    };
    let bytes = serde_json::to_vec(&evidence).expect("wire");
    let matched = super::equivalence::adjudicate_for(&root, &bytes);
    assert_eq!(matched.status(), Status::Valid, "{matched:?}");
    // Manual drift of one handler fails the gate.
    write_input(&root, "apps/planner/gen/src/focus-task.ts", b"// drifted\n");
    let drifted = super::equivalence::adjudicate_for(&root, &bytes);
    assert_eq!(drifted.status(), Status::Invalid, "{drifted:?}");
    assert_eq!(
        drifted.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-failed")
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ---- Round-5 regressions (issue #41 r4 findings) ---------------------------

/// The r4 finding 1a exploit, reproduced: a minted plan object in the
/// store plus a forged journal pinning it never deletes an unclaimed
/// maintained file — the journal's plan pin must equal the anchored
/// record's authenticated plan digest.
#[test]
fn minted_plan_object_never_deletes_unclaimed_files() {
    let root = project_root("r5-minted-plan");
    let built = apply_first_generation(&root);
    let victim = "apps/planner/gen/maintained.ts";
    write_input(
        &root,
        victim,
        b"// maintained by hand\nexport const keep = 1;\n",
    );
    let live = std::fs::read(root.join(victim)).expect("victim");
    // Mint a plan that claims the victim as a reviewed create output with
    // the live bytes as its after state, self-digest consistent.
    let mut minted = built.clone();
    minted.outputs.push(PlanOutput {
        path: victim.to_owned(),
        action: OutputAction::Create,
        role: ArtifactRole::Implementation,
        owner_module: "planner".to_owned(),
        owner_target: "node-typescript".to_owned(),
        before_digest: State::Absent,
        after_digest: State::known(digest(&live)),
    });
    minted
        .outputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    minted.plan_digest = plan_digest(&minted);
    let bytes = serde_json::to_vec_pretty(&minted).expect("wire");
    let hex = digest(&bytes).trim_start_matches("sha256:").to_owned();
    write_input(
        &root,
        &history::custody_logical(&format!("objects/{hex}")),
        &bytes,
    );
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "minted-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(minted.plan_digest.clone()),
        generation_digest: State::Absent,
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: victim.to_owned(),
            op: PendingOp::Publish,
        }],
        affected_files: vec![victim.to_owned()],
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(victim)).expect("victim"),
        b"// maintained by hand\nexport const keep = 1;\n".to_vec(),
        "the minted plan never deletes the unclaimed file",
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The r4 finding 1b exploit, reproduced: a forged journal pinning the
/// drifted bytes as its `beforeDigest` never silently overwrites manual
/// drift — with or without a plan pin, the caller-authored before pin is
/// not an accepted live state for apply journals.
#[test]
fn forged_before_digest_never_overwrites_manual_drift() {
    let root = project_root("r5-forged-before");
    apply_first_generation(&root);
    let path = "apps/planner/gen/src/focus-task.ts";
    let drift_bytes = b"// manual maintenance edit\nexport const drift = true;\n";
    write_input(&root, path, drift_bytes);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    for plan_pin in [
        State::<String>::Absent,
        State::known(record.plan_digest.clone()),
    ] {
        // Remove any journal left by the previous iteration.
        let _ = std::fs::remove_file(
            root.join(history::custody_logical("transactions/forged-0002.json")),
        );
        let journal = Transaction {
            schema_version: header("managed-transaction").0,
            identity: header("managed-transaction").1,
            transaction_id: "forged-0002".to_owned(),
            state: TransactionState::Publishing,
            action: TransactionAction::Apply,
            plan_digest: plan_pin,
            generation_digest: State::known(GenerationDigest {
                generation_digest: generation.clone(),
            }),
            before_manifest_digest: State::Absent,
            after_manifest_digest: State::Absent,
            pending: vec![Pending {
                path: path.to_owned(),
                op: PendingOp::Restore,
            }],
            affected_files: vec![path.to_owned()],
            conflict: State::Absent,
            before_digest: State::known(digest(drift_bytes)),
        };
        history::write_transaction(&root, &journal).expect("journal");
        let receipt = recovery::run(&root, &test_authority_of);
        assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
        assert_eq!(
            std::fs::read(root.join(path)).expect("drift"),
            drift_bytes.to_vec(),
            "the forged before-digest never overwrites the drift",
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The r4 finding 1c exploit, reproduced: a forged journal pinning a
/// caller-minted manifest object as `beforeManifestDigest` never swaps
/// the public manifest — the pin resolves only against the committed
/// generation's retained manifest or the live bytes' own digest.
#[test]
fn forged_manifest_pin_never_swaps_the_public_manifest() {
    let root = project_root("r5-forged-manifest");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let published = std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)).expect("manifest");
    // Mint a manifest object claiming a maintained file.
    let mut forged: serde_json::Value = serde_json::from_slice(&published).expect("manifest wire");
    let mut artifact = forged["artifacts"][0].clone();
    artifact["path"] = serde_json::json!("apps/planner/gen/maintained.ts");
    forged["artifacts"]
        .as_array_mut()
        .expect("artifacts")
        .push(artifact);
    let bytes = serde_json::to_vec(&forged).expect("wire");
    let pin = history::write_object(&root, &bytes).expect("object");
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "forged-0003".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::Absent,
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::known(pin),
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: super::custody::MANIFEST_LOGICAL.to_owned(),
            op: PendingOp::Publish,
        }],
        affected_files: Vec::new(),
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)).expect("manifest"),
        published,
        "the forged manifest pin never swaps the public manifest",
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A minted rollback plan object never bounds a rollback journal's moves
/// (r4 finding 1, rollback shape): the authoritative intent is rebuilt
/// from the anchored records and the journal pin must name exactly it.
#[test]
fn minted_rollback_plan_never_bounds_recovery() {
    let root = project_root("r5-minted-rollback");
    apply_first_generation(&root);
    let first = history::current(&root).expect("custody").expect("gen");
    // A second generation so a rollback has somewhere to go.
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let model_v2 = b"{ \"planner\": true, \"v\": 2 }\n";
    write_input(&root, "model/planner.model.json", model_v2);
    let record = history::record(&root, &first).expect("record");
    let mut before = empty_before();
    for output in &record.outputs {
        let bytes = std::fs::read(root.join(&output.path)).expect("generated");
        before.recorded.insert(
            output.path.clone(),
            super::plan::Recorded {
                digest: output.digest.clone(),
                role: output.role,
                owner_module: output.owner_module.clone(),
                owner_target: output.owner_target.clone(),
                object_ref: output.after_ref.value().cloned(),
            },
        );
        before.actual.insert(output.path.clone(), digest(&bytes));
    }
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let second = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(model_v2),
        }],
        &before,
        Some(first.clone()),
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&second).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(second.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let victim = "apps/planner/gen/maintained.ts";
    write_input(&root, victim, b"// maintained\n");
    let live = std::fs::read(root.join(victim)).expect("victim");
    // Mint a rollback plan whose restore set includes the victim with the
    // live bytes; every lineage field is borrowed from the real records.
    let mut minted = rollback::build(&root, &first).expect("rollback plan");
    minted.restore.push(RestoreOp {
        path: victim.to_owned(),
        digest: digest(&live),
        object_ref: record.outputs[0].after_ref.value().expect("ref").clone(),
    });
    minted
        .restore
        .sort_by(|left, right| left.path.cmp(&right.path));
    minted.plan_digest = plan_digest(&minted);
    let bytes = serde_json::to_vec_pretty(&minted).expect("wire");
    history::write_object(&root, &bytes).expect("object");
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "minted-rb-0001".to_owned(),
        state: TransactionState::Restoring,
        action: TransactionAction::Rollback,
        plan_digest: State::known(minted.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: first.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: victim.to_owned(),
            op: PendingOp::Restore,
        }],
        affected_files: vec![victim.to_owned()],
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(victim)).expect("victim"),
        b"// maintained\n".to_vec(),
        "the minted rollback plan never deletes the unclaimed file",
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// An interrupted resolve journal converges *behaviorally* (r6-r7, devin
/// F1 / codex F3): recovery never restores a byte on the journal's own
/// word — the retained-intent object of r6 was a deterministic function
/// of the journal itself and anchored nothing. When the confirmed
/// restore never landed (live bytes are still the pinned drift), the
/// journal closes `abandoned` with the drift preserved and the caller
/// re-runs `managed resolve --confirm` for the exact recorded bytes.
#[test]
fn interrupted_resolve_journal_preserves_the_confirmed_drift() {
    let root = project_root("r5-resolve-recovery");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    let path = "apps/planner/gen/src/focus-task.ts";
    let drift_bytes = b"// confirmed drift\n";
    write_input(&root, path, drift_bytes);
    let mut transaction = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "resolve-0001".to_owned(),
        state: TransactionState::Restoring,
        action: TransactionAction::Resolve,
        plan_digest: State::Absent,
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: path.to_owned(),
            op: PendingOp::Restore,
        }],
        affected_files: vec![path.to_owned()],
        conflict: State::Absent,
        before_digest: State::known(digest(drift_bytes)),
    };
    transaction.plan_digest = State::known(plan_digest(&transaction));
    // The retained confirmed intent, exactly as `staged_resolve` writes
    // it: the same document in its `prepared` shape. Since r6-r7 this is
    // an audit object only — the anchor below must hold even when a
    // forger plants or omits it.
    let mut intent = transaction.clone();
    intent.state = TransactionState::Prepared;
    history::write_object(
        &root,
        &serde_json::to_vec_pretty(&intent).expect("typed intent"),
    )
    .expect("retained intent");
    history::write_transaction(&root, &transaction).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    // The resolve never landed: recovery closes the journal abandoned and
    // preserves the drift — the caller re-runs the confirmed resolve.
    assert_eq!(
        std::fs::read(root.join(path)).expect("drift"),
        drift_bytes.to_vec(),
        "recovery never restores on a resolve journal's own word",
    );
    assert!(history::unresolved(&root).expect("custody").is_empty());
    // The caller's re-run is the only path to the recorded bytes: the
    // exact confirmation of a fresh preview restores them.
    let preview = rollback::resolve(&root, path, None, true);
    assert_eq!(preview.status(), Status::Valid, "{preview:?}");
    let fresh = resolve_plan_digest(&preview);
    let restored = rollback::resolve(&root, path, Some(&format!("sha256:{fresh}")), false);
    assert_eq!(restored.status(), Status::Valid, "{restored:?}");
    let expected = history::object(
        &root,
        record
            .outputs
            .iter()
            .find(|output| output.path == path)
            .expect("output")
            .after_ref
            .value()
            .expect("ref"),
    )
    .expect("object");
    assert_eq!(
        std::fs::read(root.join(path)).expect("restored"),
        expected,
        "the re-run confirmed resolve converges to the committed bytes",
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A manifest destination blocked by non-file data refuses the journal
/// before any byte moves and leaves it unresolved (r4 finding 10):
/// recovery never finalizes divergent custody as `abandoned`.
#[test]
fn blocked_manifest_destination_refuses_without_finalizing() {
    let root = project_root("r5-manifest-dir");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    let manifest_path = root.join(super::custody::MANIFEST_LOGICAL);
    std::fs::remove_file(&manifest_path).expect("remove manifest");
    std::fs::create_dir(&manifest_path).expect("alien directory");
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "blocked-0004".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(record.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: super::custody::MANIFEST_LOGICAL.to_owned(),
            op: PendingOp::Publish,
        }],
        affected_files: vec!["apps/planner/gen/src/focus-task.ts".to_owned()],
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert!(
        manifest_path.is_dir(),
        "the alien directory is left in place (never deleted)",
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "the journal stays unresolved: recovery remains owed",
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The journal id grammar is the emitted single-component spelling, and a
/// directory inside the journal home fails the scan closed (r4 finding
/// 11): smuggled sub-directory journals are impossible to represent and
/// visible when planted.
#[test]
fn journal_ids_are_single_component_and_directories_fail_closed() {
    let root = project_root("r5-journal-id");
    apply_first_generation(&root);
    let document = serde_json::to_vec(&Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "nested/deep".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::Absent,
        generation_digest: State::Absent,
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: Vec::new(),
        affected_files: Vec::new(),
        conflict: State::Absent,
        before_digest: State::Absent,
    })
    .expect("wire");
    assert!(parse_transaction(&document).is_err(), "separators refuse");
    std::fs::create_dir_all(root.join(history::custody_logical("transactions/nested")))
        .expect("smuggled directory");
    assert!(
        history::unresolved(&root).is_err(),
        "a directory in the journal home fails the scan closed",
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Adapter entry conformance is proven over the comment-free executable
/// surface (r4 finding 4): tokens inside a comment are invisible, and a
/// comment-only stub never proves locked capability.
#[test]
fn comment_only_adapter_tokens_never_prove_capability() {
    let stub = b"// describe validate verify generate protocol_version\nexport {};\n";
    let refused =
        super::authority::adapter_entry_conformance(stub).expect_err("comment-only stub refuses");
    assert_eq!(refused.status(), Status::Invalid);
    let real = std::fs::read("tests/fixtures/managed/planner/adapters/managed.mjs")
        .or_else(|_| std::fs::read("../../tests/fixtures/managed/planner/adapters/managed.mjs"))
        .expect("fixture adapter");
    assert!(super::authority::adapter_entry_conformance(&real).is_ok());
}

/// Binding qualification resolves named-export clauses and parenthesized
/// initializers to real callable declarations (r4 finding 3): both
/// confirmed non-callable shapes refuse.
#[test]
fn export_clauses_and_paren_initializers_require_callables() {
    // `const hook = 7; export { hook }` — the clause resolves back to a
    // non-callable declaration.
    let list = b"const hook = 7;\nexport { hook };\n";
    assert!(
        !super::bindings::exports_symbol(list, "hook"),
        "a named export of a non-callable const refuses",
    );
    // `export const hook = (7)` — the parenthesized initializer is an
    // arbitrary expression, not an arrow or function expression.
    let paren = b"export const hook = (7);\n";
    assert!(
        !super::bindings::exports_symbol(paren, "hook"),
        "an arbitrary parenthesized initializer refuses",
    );
    // Real callable shapes still qualify.
    assert!(super::bindings::exports_symbol(
        b"export const hook = (input) => input;\n",
        "hook"
    ));
    assert!(super::bindings::exports_symbol(
        b"export const hook = (a: string): string => a;\n",
        "hook"
    ));
    assert!(super::bindings::exports_symbol(
        b"const internal = () => 1;\nexport { internal as hook };\n",
        "hook"
    ));
    assert!(super::bindings::exports_symbol(
        b"export const hook = async (input) => input;\n",
        "hook"
    ));
    // A re-export from another module cannot be verified here and never
    // qualifies.
    assert!(
        !super::bindings::exports_symbol(b"export { hook } from \"./other.ts\";\n", "hook"),
        "an unverifiable re-export refuses",
    );
}

/// A projection `to` rename changes emitted bytes under unchanged
/// Model/IR/lock and is refused (r4 finding 5): the emitted naming is
/// Model-derived, so `to` must equal the Model-side `from` name.
#[test]
fn projection_to_renames_are_model_refused() {
    let authority = super::authority::Authority {
        model_digest: PIN_A.to_owned(),
        ir_digest: PIN_B.to_owned(),
        lock_digest: PIN_A.to_owned(),
        lock_protocol: Some(PROTOCOL.to_owned()),
        model_version: "0.2.16".to_owned(),
        project_id: "demo".to_owned(),
        ledger: vec![
            (
                "planner.complete_task".to_owned(),
                crate::ir::DefinitionKind::Command,
            ),
            (
                "planner.focus_task".to_owned(),
                crate::ir::DefinitionKind::Query,
            ),
        ],
        semantics: vec![
            super::authority::Obligation {
                id: "planner.complete_task".to_owned(),
                kind: crate::ir::DefinitionKind::Command,
                input: vec![
                    ("id".to_owned(), "string".to_owned()),
                    ("title".to_owned(), "string".to_owned()),
                ],
                reads: Vec::new(),
                returns: None,
                return_fields: Vec::new(),
            },
            super::authority::Obligation {
                id: "planner.focus_task".to_owned(),
                kind: crate::ir::DefinitionKind::Query,
                input: Vec::new(),
                reads: Vec::new(),
                returns: None,
                return_fields: vec![
                    ("id".to_owned(), "string".to_owned()),
                    ("title".to_owned(), "string".to_owned()),
                ],
            },
        ],
        adapter: None,
        generators: Vec::new(),
        components: Vec::new(),
        sources: Vec::new(),
    };
    assert!(super::authority::semantics_match(&coverage_doc(), &authority).is_ok());
    let mut renamed = coverage_doc();
    renamed.requirements[0].projection[1].to = "heading".to_owned();
    let error =
        super::authority::semantics_match(&renamed, &authority).expect_err("rename refused");
    assert_eq!(
        error.reason_codes().first().map(|code| code.as_str()),
        Some("managed.capability-gap")
    );
    // The emitted bytes really are projection-named: a rename would move
    // them under identical authority pins.
    let inventory =
        render_inventory(&renamed, "apps/planner/gen", &test_semantics()).expect("inventory");
    let handler = String::from_utf8(
        inventory
            .iter()
            .find(|artifact| artifact.path == "apps/planner/gen/src/complete-task.ts")
            .expect("handler")
            .bytes
            .clone(),
    )
    .expect("utf8");
    assert!(handler.contains("heading: input.title"));
}

/// Forged recipe and ownership provenance pins never commit (r4 finding
/// 6): both must resolve against the locked generator/profile inventory.
#[test]
fn forged_provenance_pins_never_commit() {
    let root = project_root("r5-provenance");
    write_input(
        &root,
        "model/planner.model.json",
        b"{ \"planner\": true }\n",
    );
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let build_plan = |coverage: &Coverage| {
        let admitted = coverage::admit(assignment, target, coverage).expect("admission");
        plan::build(
            PlanAction::Promotion,
            &admitted,
            coverage,
            vec![InputPin {
                path: "model/planner.model.json".to_owned(),
                digest: digest(b"{ \"planner\": true }\n"),
            }],
            &empty_before(),
            None,
            None,
            hash(coverage),
            &test_semantics(),
        )
        .expect("plan")
    };
    // A forged recipe digest: the plan is internally consistent, but the
    // recipe pin names no locked generator.
    let mut recipe = coverage_doc();
    recipe.recipe.digest = PIN_C.to_owned();
    let recipe_plan = build_plan(&recipe);
    let refused = apply::apply(
        &root,
        &serde_json::to_vec(&recipe_plan).expect("wire"),
        &serde_json::to_vec(&recipe).expect("wire"),
        &ownership,
        Some(recipe_plan.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    // Forged ownership references refuse the same way.
    let mut ownership_forged = ownership_doc();
    ownership_forged.modules[0].targets[0].profile_ref.digest = PIN_C.to_owned();
    let plan = build_plan(&coverage_doc());
    let refused = apply::apply(
        &root,
        &serde_json::to_vec(&plan).expect("wire"),
        &serde_json::to_vec(&coverage_doc()).expect("wire"),
        &ownership_forged,
        Some(plan.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    assert!(!history::exists(&root), "nothing committed");
    let _ = std::fs::remove_dir_all(&root);
}

/// Behavioral evidence is honest end to end (r4 finding 2, hardened in
/// r6 by devin F5 / codex F6): a self-contradictory coverage sample — the
/// emitted test would assert an expectation the deterministic handler
/// never produces — never *admits*, so no plan, no apply, and no published
/// self-failing artifact can exist; and over an honestly admitted
/// generation, an executed-outcome receipt that disagrees with the
/// recomputed executed behavior fails the equivalence gate.
#[test]
fn equivalence_lying_verdicts_over_failing_samples_refuse() {
    let root = project_root("r6-lying-verdicts");
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    // A coverage whose focus_task sample output the projection contradicts:
    // the generated native test would fail on arrival (handler returns
    // title "Focus" while the pinned expectation says "Different").
    let mut coverage = coverage_doc();
    if let State::Known { value } = &mut coverage.requirements[1].sample {
        value
            .output
            .insert("title".to_owned(), Scalar::Text("Different".to_owned()));
    }
    // r6 (devin F5 / codex F6): the self-failing sample never admits —
    // at planning time, before any plan, byte, or custody object exists.
    let refused = coverage::admit(assignment, target, &coverage).expect_err("admission");
    assert_eq!(refused.status(), Status::Unsupported);
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.capability-gap")
    );
    let reason = refused.diagnostics()[0].data.get("reason").cloned();
    let Some(crate::diagnostics::types::DataValue::Token(reason)) = reason else {
        panic!("expected a token reason");
    };
    assert_eq!(reason, "scenario-assertion");
    write_input(
        &root,
        "model/planner.model.json",
        b"{ \"planner\": true }\n",
    );
    // The apply over such a coverage never publishes either (codex F6:
    // the publication barrier is the admission, not a post-hoc verb).
    // The plan is assembled directly over the mutated coverage (its
    // admission struct is hand-built) so the refusal provably comes from
    // admission, not from a coverage-digest mismatch.
    let admission = coverage::Admission {
        module: "planner".to_owned(),
        target: "node-typescript".to_owned(),
        recipe: coverage.recipe.clone(),
        generated_root: "apps/planner/gen".to_owned(),
        rendered_requirements: 2,
        ledger_requirements: coverage.requirements.len(),
        extensions: Vec::new(),
    };
    let built = plan::build(
        PlanAction::Promotion,
        &admission,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(b"{ \"planner\": true }\n"),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    let failed_apply = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &serde_json::to_vec(&coverage).expect("wire"),
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(
        failed_apply.status(),
        Status::Unsupported,
        "{failed_apply:?}"
    );
    assert!(
        !history::exists(&root),
        "the self-failing sample never publishes"
    );
    let _ = std::fs::remove_dir_all(&root);
    // Over an honestly admitted generation, a lying executed-outcome
    // receipt — one that disagrees with the recomputed executed behavior —
    // still fails the gate (r4 finding 2, retained).
    let honest_root = project_root("r6-lying-verdicts-honest");
    apply_first_generation(&honest_root);
    let generation = history::current(&honest_root)
        .expect("custody")
        .expect("gen");
    let record = history::record(&honest_root, &generation).expect("record");
    let implementation = super::equivalence::live_implementation_digest_for_test(
        &honest_root,
        &record,
        "planner",
        "node-typescript",
    )
    .expect("live inventory");
    let scenario_digest = |file: &str| {
        record
            .outputs
            .iter()
            .find(|output| output.path == file)
            .expect("scenario test")
            .digest
            .clone()
    };
    let reference = record
        .outputs
        .iter()
        .find(|output| output.path == "apps/planner/gen/src/focus-task.ts")
        .expect("reference")
        .after_ref
        .value()
        .expect("ref")
        .clone();
    // The recomputed executed output of both scenarios: the projection
    // applied to the pinned sample inputs.
    let recomputed = "sha256:4ec9852ef939bad75ea72af4e8ed09668f78d38a01a814262048448f3fb83374";
    let mut lying = Equivalence {
        schema_version: header("managed-equivalence").0,
        identity: header("managed-equivalence").1,
        module: "planner".to_owned(),
        target: "node-typescript".to_owned(),
        implementation_digest: implementation,
        reference_digest: State::known(reference),
        scenarios: vec![
            ScenarioVerdict {
                scenario: "planner.complete_task".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/complete-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            },
            ScenarioVerdict {
                scenario: "planner.focus_task".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/focus-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            },
        ],
        execution: vec![
            ScenarioExecution {
                scenario: "planner.complete_task".to_owned(),
                observed: recomputed.to_owned(),
            },
            ScenarioExecution {
                scenario: "planner.focus_task".to_owned(),
                observed: recomputed.to_owned(),
            },
        ],
        gaps: Vec::new(),
        verdict: EquivalenceVerdict::Match,
    };
    // The receipt that matches the lie instead: it disagrees with the
    // recomputed executed behavior and fails the gate.
    lying.execution[1].observed =
        "sha256:a419b8cd65c2c3fc70b435c16455e2a235d88da22d88d4fc381d8b22736918fc".to_owned();
    let refused = super::equivalence::adjudicate_for(
        &honest_root,
        &serde_json::to_vec(&lying).expect("wire"),
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-failed")
    );
    let _ = std::fs::remove_dir_all(&honest_root);
}

/// The staging coordinate is validated like the destination (r4 finding
/// 8): an unconfinable stage spelling refuses before anything is created.
#[test]
fn write_staged_refuses_an_unconfinable_stage_coordinate() {
    let root = project_root("r5-stage");
    write_input(&root, "custody/keep.txt", b"keep\n");
    for stage in [
        "custody/../escape.stage",
        "custody\\win.stage",
        "/abs.stage",
    ] {
        let refused = fs::write_staged(&root, "custody/leaf.json", stage, b"NEW\n");
        assert!(refused.is_err(), "stage {stage} must refuse");
    }
    assert!(!root.join("escape.stage").exists());
    assert!(!root.join("custody/leaf.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

/// Under-lock revalidation covers every mutable authority input (r4
/// finding 9): a Model source that no longer matches the authority's
/// pinned derivation refuses the apply before any byte moves.
#[test]
fn under_lock_model_source_drift_refuses_the_apply() {
    let root = project_root("r5-source-race");
    write_input(
        &root,
        "model/planner.model.json",
        b"{ \"planner\": true }\n",
    );
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(b"{ \"planner\": true }\n"),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    // The authority pins a Model source state the live project no longer
    // carries — exactly the racer's swap between derivation and lock.
    let mut authority = test_authority();
    authority.sources = vec![(
        "lekalo/modules/planner/module.yaml".to_owned(),
        PIN_C.to_owned(),
    )];
    let refused = apply::apply(
        &root,
        &serde_json::to_vec(&built).expect("wire"),
        &serde_json::to_vec(&coverage).expect("wire"),
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&authority),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    assert!(!history::exists(&root), "nothing committed");
    let _ = std::fs::remove_dir_all(&root);
}

// ---- Round-6 regressions (issue #41 r5 findings, devin + codex) ----------

/// Apply one further generation whose rendered bytes differ from the
/// committed ones (a moved sample expectation), returning the new
/// generation digest.
fn apply_changed_generation(root: &std::path::Path, title: &str) -> String {
    let ownership = ownership_doc();
    let mut coverage = coverage_doc();
    for requirement in &mut coverage.requirements {
        if let State::Known { value } = &mut requirement.sample {
            value
                .output
                .insert("title".to_owned(), Scalar::Text(title.to_owned()));
            value
                .input
                .insert("title".to_owned(), Scalar::Text(title.to_owned()));
        }
    }
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    let generation = history::current(root).expect("custody").expect("gen");
    let record = history::record(root, &generation).expect("record");
    let mut before = empty_before();
    for output in &record.outputs {
        let bytes = std::fs::read(root.join(&output.path)).expect("generated");
        before.recorded.insert(
            output.path.clone(),
            plan::Recorded {
                digest: output.digest.clone(),
                role: output.role,
                owner_module: output.owner_module.clone(),
                owner_target: output.owner_target.clone(),
                object_ref: output.after_ref.value().cloned(),
            },
        );
        before.actual.insert(output.path.clone(), digest(&bytes));
    }
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let built = plan::build(
        PlanAction::Regeneration,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(b"{ \"planner\": true }\n"),
        }],
        &before,
        Some(generation),
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    write_input(root, "model/planner.model.json", b"{ \"planner\": true }\n");
    let applied = apply::apply(
        root,
        &serde_json::to_vec(&built).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    history::current(root).expect("custody").expect("gen")
}

/// An interrupted rollback to a mid-chain destination anchors and converges
/// (r6, devin F1 [P1 blocking] / codex F10): a genuine three-generation
/// chain A→B→C whose rollback to B was interrupted — both before any byte
/// moved and after every move but the pointer commit — recovers to a
/// defined state instead of wedging the project forever with
/// `recovery-unanchored`.
#[test]
fn interrupted_mid_chain_rollback_recovers() {
    let root = project_root("r6-mid-chain");
    apply_first_generation(&root);
    let second = apply_changed_generation(&root, "Focus v2");
    let third = apply_changed_generation(&root, "Focus v3");
    let second_record = history::record(&root, &second).expect("record");
    let third_record = history::record(&root, &third).expect("record");
    // The destination is a genuine mid-chain ancestor: its own predecessor
    // is the first generation, not the committed one.
    assert!(second_record.previous_generation.value().is_some());
    assert_ne!(
        second_record
            .previous_generation
            .value()
            .map(|previous| previous.generation_digest.clone()),
        Some(third.clone())
    );
    let preview = rollback::build(&root, &second).expect("rollback plan");
    let before_manifest =
        std::fs::read(root.join(super::custody::MANIFEST_LOGICAL)).expect("committed manifest");
    let before_pin = history::write_object(&root, &before_manifest).expect("object");
    let journal_of = |state: TransactionState| Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "midchain00001".to_owned(),
        state,
        action: TransactionAction::Rollback,
        plan_digest: State::known(preview.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: second.clone(),
        }),
        before_manifest_digest: State::known(before_pin.clone()),
        after_manifest_digest: State::known(preview.manifest_digest.clone()),
        pending: {
            let mut pending: Vec<Pending> = second_record
                .outputs
                .iter()
                .map(|output| Pending {
                    path: output.path.clone(),
                    op: PendingOp::Restore,
                })
                .collect();
            pending.push(Pending {
                path: super::custody::MANIFEST_LOGICAL.to_owned(),
                op: PendingOp::Publish,
            });
            pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
            pending
        },
        affected_files: {
            let mut affected: Vec<String> = second_record
                .outputs
                .iter()
                .map(|output| output.path.clone())
                .collect();
            affected.sort();
            affected
        },
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    // Phase A: the rollback was killed before any byte moved — the live
    // tree still carries the committed generation's bytes.
    history::write_transaction(&root, &journal_of(TransactionState::Restoring)).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    assert_eq!(
        history::current(&root).expect("custody").expect("gen"),
        third,
        "an unstarted interrupted rollback converges back to the committed generation"
    );
    assert!(history::unresolved(&root).expect("custody").is_empty());
    // Phase B: the rollback moved every byte and the manifest but was
    // killed before the pointer commit — the exact on-disk state a killed
    // mid-rollback leaves. Re-arm the same journal over that state.
    let confirmed = rollback::apply(
        &root,
        &serde_json::to_vec(&preview).expect("wire"),
        Some(preview.plan_digest.as_str()),
        false,
        Some(&test_authority()),
    );
    assert_eq!(confirmed.status(), Status::Valid, "{confirmed:?}");
    // Rewind the pointer to the pre-rollback state: the tree now carries
    // the destination's exact bytes, manifest included.
    history::write_current(&root, &third).expect("pointer");
    history::write_transaction(&root, &journal_of(TransactionState::Restoring)).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    assert_eq!(
        history::current(&root).expect("custody").expect("gen"),
        second,
        "a fully-moved interrupted rollback completes to its destination"
    );
    // The destination's exact bytes are live again.
    let live =
        std::fs::read(root.join("apps/planner/gen/test/focus-task.test.ts")).expect("test file");
    let expected = history::object(
        &root,
        second_record
            .outputs
            .iter()
            .find(|output| output.path == "apps/planner/gen/test/focus-task.test.ts")
            .expect("output")
            .after_ref
            .value()
            .expect("ref"),
    )
    .expect("object");
    assert_eq!(live, expected);
    let _ = third_record;
    let _ = std::fs::remove_dir_all(&root);
}

/// A fabricated direct successor never authorizes recovery to delete an
/// unclaimed maintained file (r6, codex F1): the self-consistent
/// record+plan pair naming a maintained path as a create is refused at the
/// anchor — its "creates" reach outside every generated root the committed
/// custody can derive — and the victim survives untouched.
#[test]
fn forged_successor_record_never_deletes_unclaimed_files() {
    let root = project_root("r6-forged-successor");
    apply_first_generation(&root);
    let victim = "maintained/victim.ts";
    write_input(&root, victim, b"// maintained by hand\n");
    let committed = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &committed).expect("record");
    let plan = history::plan(&root, &record.plan_digest).expect("retained plan");
    // The forged successor: the real plan plus a create output for the
    // victim, and a record carrying it, every digest self-consistent.
    let mut minted = plan.clone();
    minted.outputs.push(PlanOutput {
        path: victim.to_owned(),
        action: OutputAction::Create,
        role: ArtifactRole::Implementation,
        owner_module: "planner".to_owned(),
        owner_target: "node-typescript".to_owned(),
        before_digest: State::Absent,
        after_digest: State::known(digest(b"// maintained by hand\n")),
    });
    minted
        .outputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    minted.plan_digest = plan_digest(&minted);
    history::write_object(&root, &serde_json::to_vec_pretty(&minted).expect("wire"))
        .expect("minted plan object");
    history::write_object(&root, b"// maintained by hand\n").expect("victim object");
    let mut outputs = record.outputs.clone();
    outputs.push(RecordOutput {
        path: victim.to_owned(),
        role: ArtifactRole::Implementation,
        owner_module: "planner".to_owned(),
        owner_target: "node-typescript".to_owned(),
        digest: digest(b"// maintained by hand\n"),
        before_ref: State::Absent,
        after_ref: State::known(digest(b"// maintained by hand\n")),
    });
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    let mut forged = record.clone();
    forged.previous_generation = State::known(GenerationDigest {
        generation_digest: committed.clone(),
    });
    forged.plan_digest = minted.plan_digest.clone();
    forged.outputs = outputs;
    forged.generation_digest = super::generation_digest(&forged);
    forged.record_digest = super::record_identity(&forged);
    let forged_hex = forged
        .generation_digest
        .strip_prefix("sha256:")
        .expect("digest")
        .to_owned();
    write_input(
        &root,
        &history::custody_logical(&format!("objects/{forged_hex}")),
        &serde_json::to_vec(&forged).expect("wire"),
    );
    let mut pending: Vec<Pending> = minted
        .outputs
        .iter()
        .map(|output| Pending {
            path: output.path.clone(),
            op: PendingOp::Publish,
        })
        .collect();
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    let mut affected: Vec<String> = minted
        .outputs
        .iter()
        .map(|output| output.path.clone())
        .collect();
    affected.sort();
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "forged-succ-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(minted.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: forged.generation_digest.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending,
        affected_files: affected,
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(victim)).expect("victim"),
        b"// maintained by hand\n".to_vec(),
        "the fabricated successor never deletes the unclaimed file",
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "the journal stays unresolved: recovery remains owed"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A resolve journal without its retained confirmed intent never overwrites
/// a manual edit (r6, codex F2 → r6-r7 devin F1 / codex F3): the r6 check
/// anchored to an object whose name a journal forger could always mint,
/// so recovery now converges resolve journals *behaviorally* — the drift
/// is preserved whatever objects sit in the store, the journal closes
/// `abandoned`, and the caller re-runs the confirmed resolve.
#[test]
fn forged_resolve_journal_never_overwrites_unreviewed_drift() {
    let root = project_root("r6-forged-resolve");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let path = "apps/planner/gen/src/focus-task.ts";
    let drift = b"// MANUAL MAINTAINED BY USER\n";
    write_input(&root, path, drift);
    let mut transaction = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "forged-res-0001".to_owned(),
        state: TransactionState::Restoring,
        action: TransactionAction::Resolve,
        plan_digest: State::Absent,
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: path.to_owned(),
            op: PendingOp::Restore,
        }],
        affected_files: vec![path.to_owned()],
        conflict: State::Absent,
        before_digest: State::known(digest(drift)),
    };
    transaction.plan_digest = State::known(plan_digest(&transaction));
    // The caller-mintable "retained intent" object: byte-identical to the
    // genuine flow's audit object, planted exactly as the r6 exploit did
    // (its content address is a deterministic function of this journal).
    let mut intent = transaction.clone();
    intent.state = TransactionState::Prepared;
    history::write_object(
        &root,
        &serde_json::to_vec_pretty(&intent).expect("typed intent"),
    )
    .expect("planted intent");
    history::write_transaction(&root, &transaction).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Valid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(path)).expect("drift"),
        drift.to_vec(),
        "the unreviewed drift is never overwritten — planted intent or not",
    );
    assert!(
        history::unresolved(&root).expect("custody").is_empty(),
        "the journal converged to a defined (abandoned) state"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// An invalid manifest pin refuses before any application byte moves (r6,
/// codex F3 / devin F6): a journal whose `beforeManifestDigest` resolves to
/// neither the live digest nor the committed retained manifest leaves the
/// tree exactly as it was — the missing recorded output is NOT restored
/// before the refusal.
#[test]
fn invalid_manifest_pin_refuses_before_any_byte_moves() {
    let root = project_root("r6-manifest-preflight");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    // The interrupted shape: one recorded output is missing and pending
    // restoration; the manifest pin is caller-forged.
    let missing = "apps/planner/gen/src/complete-task.ts";
    std::fs::remove_file(root.join(missing)).expect("remove output");
    let mut pending: Vec<Pending> = record
        .outputs
        .iter()
        .map(|output| Pending {
            path: output.path.clone(),
            op: PendingOp::Publish,
        })
        .collect();
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    let mut affected: Vec<String> = record
        .outputs
        .iter()
        .map(|output| output.path.clone())
        .collect();
    affected.sort();
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "badpin-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(record.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::known(PIN_C.to_owned()),
        after_manifest_digest: State::Absent,
        pending,
        affected_files: affected,
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert!(
        !root.join(missing).exists(),
        "the missing output is NOT restored before the refusal"
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "the journal stays unresolved"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A journal whose pending set covers only a subset of the reviewed
/// transaction's affected inventory never finalizes a partial destination
/// as completed (r6, codex F4): recovery stays owed while a recorded
/// generated output is missing.
#[test]
fn partial_pending_journal_never_completes_with_missing_output() {
    let root = project_root("r6-partial-pending");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    // One recorded generated output is deleted from the tree; the journal
    // names only one other existing output.
    std::fs::remove_file(root.join("apps/planner/gen/test/complete-task.test.ts"))
        .expect("remove test");
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "partial-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(record.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: "apps/planner/gen/src/focus-task.ts".to_owned(),
            op: PendingOp::Publish,
        }],
        affected_files: vec!["apps/planner/gen/src/focus-task.ts".to_owned()],
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert!(
        !root
            .join("apps/planner/gen/test/complete-task.test.ts")
            .exists(),
        "the missing recorded output stays missing — never papered over"
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "recovery stays owed while a required output diverges"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Equivalence is honest about what ran (r6, codex F5 / devin F7): live
/// bytes drifted from the committed generation fail the gate even when the
/// evidence was re-pinned to the drifted bytes; phantom scenario rows that
/// name no rendered requirement are refused instead of padding the
/// receipt; and a bound coverage — whose handlers invoke a maintained hook
/// the offline gate cannot execute — is incomplete evidence, never a
/// projection-recomputed `match`.
#[test]
fn equivalence_refuses_repinned_drift_phantom_rows_and_bound_coverage() {
    let root = project_root("r6-equivalence-honesty");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &generation).expect("record");
    // Drift the live handler and re-pin the evidence to the drifted bytes.
    let drifted = b"// drifted throwing implementation\n";
    write_input(&root, "apps/planner/gen/src/focus-task.ts", drifted);
    let repinned = super::equivalence::live_implementation_digest_for_test(
        &root,
        &record,
        "planner",
        "node-typescript",
    )
    .expect("drifted inventory");
    let scenario_digest = |file: &str| {
        record
            .outputs
            .iter()
            .find(|output| output.path == file)
            .expect("scenario test")
            .digest
            .clone()
    };
    let reference = record
        .outputs
        .iter()
        .find(|output| output.path == "apps/planner/gen/src/focus-task.ts")
        .expect("reference")
        .after_ref
        .value()
        .expect("ref")
        .clone();
    let recomputed = "sha256:4ec9852ef939bad75ea72af4e8ed09668f78d38a01a814262048448f3fb83374";
    let evidence_of = |implementation: String, extra: bool| {
        let mut scenarios = vec![
            ScenarioVerdict {
                scenario: "planner.complete_task".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/complete-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            },
            ScenarioVerdict {
                scenario: "planner.focus_task".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/focus-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            },
        ];
        let mut execution = vec![
            ScenarioExecution {
                scenario: "planner.complete_task".to_owned(),
                observed: recomputed.to_owned(),
            },
            ScenarioExecution {
                scenario: "planner.focus_task".to_owned(),
                observed: recomputed.to_owned(),
            },
        ];
        if extra {
            // The phantom row: a scenario naming no rendered requirement,
            // with made-up pins.
            scenarios.push(ScenarioVerdict {
                scenario: "planner.ghost".to_owned(),
                digest: scenario_digest("apps/planner/gen/test/focus-task.test.ts"),
                verdict: ScenarioOutcome::Match,
            });
            execution.push(ScenarioExecution {
                scenario: "planner.ghost".to_owned(),
                observed: recomputed.to_owned(),
            });
        }
        Equivalence {
            schema_version: header("managed-equivalence").0,
            identity: header("managed-equivalence").1,
            module: "planner".to_owned(),
            target: "node-typescript".to_owned(),
            implementation_digest: implementation,
            reference_digest: State::known(reference.clone()),
            scenarios,
            execution,
            gaps: Vec::new(),
            verdict: EquivalenceVerdict::Match,
        }
    };
    // (a) Repinned drift fails on the committed-record byte comparison.
    let refused = super::equivalence::adjudicate_for(
        &root,
        &serde_json::to_vec(&evidence_of(repinned, false)).expect("wire"),
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-failed")
    );
    // Restore the committed bytes; phantom rows are corpus corruption.
    let committed_handler = history::object(
        &root,
        record
            .outputs
            .iter()
            .find(|output| output.path == "apps/planner/gen/src/focus-task.ts")
            .expect("output")
            .after_ref
            .value()
            .expect("ref"),
    )
    .expect("object");
    write_input(
        &root,
        "apps/planner/gen/src/focus-task.ts",
        &committed_handler,
    );
    let honest = super::equivalence::live_implementation_digest_for_test(
        &root,
        &record,
        "planner",
        "node-typescript",
    )
    .expect("honest inventory");
    let refused = super::equivalence::adjudicate_for(
        &root,
        &serde_json::to_vec(&evidence_of(honest.clone(), true)).expect("wire"),
    );
    assert_eq!(refused.status(), Status::Unsupported, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-incomplete")
    );
    let _ = std::fs::remove_dir_all(&root);
    // (b) A bound coverage is incomplete evidence: the gate cannot certify
    // executed outcomes for handlers that invoke a maintained hook.
    let bound_root = project_root("r6-equivalence-bound");
    let binding = b"export function legacyHook(input) { return input; }\n";
    write_input(&bound_root, "src/extensions/legacy_hook.ts", binding);
    let mut coverage = coverage_doc();
    coverage.extensions.push(ExtensionCheck {
        symbol: "legacy_hook".to_owned(),
        binding: "src/extensions/legacy_hook.ts".to_owned(),
        contract_digest: digest(binding),
        validated: true,
    });
    let mut ownership = ownership_doc();
    ownership.modules[0].targets[0].extensions.push(Extension {
        symbol: "legacy_hook".to_owned(),
        binding: "src/extensions/legacy_hook.ts".to_owned(),
        contract_digest: digest(binding),
    });
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    write_input(
        &bound_root,
        "model/planner.model.json",
        b"{ \"planner\": true }\n",
    );
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![InputPin {
            path: "model/planner.model.json".to_owned(),
            digest: digest(b"{ \"planner\": true }\n"),
        }],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    // The bound apply is gated on execution evidence (r6-r7, codex F4):
    // no evidence, no commit — the behavioral gate a binding makes
    // structurally incomplete offline is never skipped.
    let bound_evidence_of =
        |test_digest_of: &dyn Fn(&str) -> String, implementation: String, reference: String| {
            Equivalence {
                schema_version: header("managed-equivalence").0,
                identity: header("managed-equivalence").1,
                module: "planner".to_owned(),
                target: "node-typescript".to_owned(),
                implementation_digest: implementation,
                reference_digest: State::known(reference),
                scenarios: vec![
                    ScenarioVerdict {
                        scenario: "planner.complete_task".to_owned(),
                        digest: test_digest_of("planner.complete_task"),
                        verdict: ScenarioOutcome::Match,
                    },
                    ScenarioVerdict {
                        scenario: "planner.focus_task".to_owned(),
                        digest: test_digest_of("planner.focus_task"),
                        verdict: ScenarioOutcome::Match,
                    },
                ],
                execution: vec![
                    ScenarioExecution {
                        scenario: "planner.complete_task".to_owned(),
                        observed: recomputed.to_owned(),
                    },
                    ScenarioExecution {
                        scenario: "planner.focus_task".to_owned(),
                        observed: recomputed.to_owned(),
                    },
                ],
                gaps: Vec::new(),
                verdict: EquivalenceVerdict::Match,
            }
        };
    let plan_test_digest = |scenario: &str| {
        built
            .outputs
            .iter()
            .find(|output| {
                output.path
                    == format!(
                        "apps/planner/gen/test/{}.test.ts",
                        scenario.replace("planner.", "").replace('_', "-")
                    )
            })
            .and_then(|output| output.after_digest.value().cloned())
            .expect("planned test digest")
    };
    let refused_apply = apply::apply(
        &bound_root,
        &serde_json::to_vec(&built).expect("wire"),
        &serde_json::to_vec(&coverage).expect("wire"),
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused_apply.status(), Status::Invalid, "{refused_apply:?}");
    assert_eq!(
        refused_apply
            .reason_codes()
            .first()
            .map(|code| code.as_str()),
        Some("managed.input-invalid"),
        "a bound generation never commits without execution evidence"
    );
    let execution_bytes = serde_json::to_vec(&bound_evidence_of(
        &plan_test_digest,
        PIN_A.to_owned(),
        PIN_A.to_owned(),
    ))
    .expect("wire");
    let applied = apply::apply(
        &bound_root,
        &serde_json::to_vec(&built).expect("wire"),
        &serde_json::to_vec(&coverage).expect("wire"),
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        Some(&execution_bytes),
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let bound_generation = history::current(&bound_root)
        .expect("custody")
        .expect("gen");
    let bound_record = history::record(&bound_root, &bound_generation).expect("record");
    let bound_inventory = super::equivalence::live_implementation_digest_for_test(
        &bound_root,
        &bound_record,
        "planner",
        "node-typescript",
    )
    .expect("inventory");
    let record_test_digest = |scenario: &str| {
        bound_record
            .outputs
            .iter()
            .find(|output| {
                output.path
                    == format!(
                        "apps/planner/gen/test/{}.test.ts",
                        scenario.replace("planner.", "").replace('_', "-")
                    )
            })
            .map(|output| output.digest.clone())
            .expect("recorded test digest")
    };
    // The consumed verdict of a bound generation (r6-r7, codex F4): with
    // the apply-retained execution evidence and receipts that equal it,
    // the gate adjudicates `match` instead of refusing `bound-execution`
    // forever; receipts no committed transaction vouched for stay
    // incomplete evidence.
    let matched = bound_evidence_of(
        &record_test_digest,
        bound_inventory.clone(),
        bound_record.outputs[0]
            .after_ref
            .value()
            .expect("ref")
            .clone(),
    );
    let accepted = super::equivalence::adjudicate_for(
        &bound_root,
        &serde_json::to_vec(&matched).expect("wire"),
    );
    assert_eq!(accepted.status(), Status::Valid, "{accepted:?}");
    let mut lying = matched.clone();
    lying.execution[0].observed = PIN_B.to_owned();
    let refused =
        super::equivalence::adjudicate_for(&bound_root, &serde_json::to_vec(&lying).expect("wire"));
    assert_eq!(refused.status(), Status::Unsupported, "{refused:?}");
    assert_eq!(
        refused.reason_codes().first().map(|code| code.as_str()),
        Some("managed.equivalence-incomplete")
    );
    let _ = std::fs::remove_dir_all(&bound_root);
}

/// Binding qualification scans only top-level executable positions (r6,
/// devin F2 / codex F7): nested declarations, string-literal decoys, and
/// class-shaped bindings never qualify — the emitted test invokes plain
/// calls, so each shape would fail natively.
#[test]
fn binding_qualification_scans_only_top_level_callables() {
    // A declaration nested inside another scope never satisfies an export
    // clause (the generated module could not even load).
    let nested = b"function outer() { function hook(x) { return x; } }\nexport { hook };\n";
    assert!(
        !super::bindings::exports_symbol(nested, "hook"),
        "a nested declaration never satisfies an export clause"
    );
    // A string-literal decoy never qualifies: literal contents are blank.
    let decoy = b"const strhook = \"i am not a function\";\nconst decoy = \"function strhook(x){}\";\nexport { strhook };\n";
    assert!(
        !super::bindings::exports_symbol(decoy, "strhook"),
        "a string-literal decoy never qualifies"
    );
    let marker = b"export const marker = \"function hook() {}\";\nexport const hook = 7;\n";
    assert!(
        !super::bindings::exports_symbol(marker, "hook"),
        "a marker string never rescues a non-callable export"
    );
    let wrapper = b"export function wrapper() { function hook() {} }\nexport const hook = 7;\n";
    assert!(
        !super::bindings::exports_symbol(wrapper, "hook"),
        "a nested declaration never rescues a non-callable export"
    );
    // A class binding never qualifies: the emitted invocation is a plain
    // call and would throw `Class constructor cannot be invoked without
    // 'new'`.
    let class_direct = b"export class hook {}\n";
    assert!(
        !super::bindings::exports_symbol(class_direct, "hook"),
        "a class-shaped binding is refused (plain-call invocation)"
    );
    let class_clause = b"class hook {}\nexport { hook };\n";
    assert!(
        !super::bindings::exports_symbol(class_clause, "hook"),
        "a class-shaped named export is refused"
    );
    // Genuine top-level callables still qualify.
    assert!(super::bindings::exports_symbol(
        b"function hook(x) { return x; }\nexport { hook };\n",
        "hook"
    ));
    assert!(super::bindings::exports_symbol(
        b"export const hook = (x) => x;\n",
        "hook"
    ));
}

/// Adapter entry conformance is structural, not textual (r6, devin F3 /
/// codex F8): a token soup inside one string literal never proves locked
/// capability; a protocol-document entry whose operations are standalone
/// quoted keys does.
#[test]
fn adapter_string_tokens_never_prove_capability() {
    let soup =
        b"const tokens = \"describe validate verify generate protocol_version\";\nexport {};\n";
    let refused =
        super::authority::adapter_entry_conformance(soup).expect_err("token soup refuses");
    assert_eq!(refused.status(), Status::Invalid);
    // A protocol-document spelling: the operations as standalone quoted
    // keys and the negotiation parameter as a structural property.
    let quoted = b"const capabilities = { operations: [\"describe\", \"validate\", \"verify\", \"generate\"], protocol_version: \"0.3.2\" };\nexport {};\n";
    assert!(super::authority::adapter_entry_conformance(quoted).is_ok());
    // A structural implementation qualifies too.
    let structural = b"export function describe() {}\nexport function validate() {}\nexport function verify() {}\nexport function generate() {}\nconst handshake = { protocol_version: \"0.3.2\" };\n";
    assert!(
        super::authority::adapter_entry_conformance(structural).is_ok(),
        "a prefix word (protocol_versions) never satisfies protocol_version"
    );
    // A prefix word never satisfies the negotiation parameter.
    let prefix = b"const capabilities = { operations: [\"describe\", \"validate\", \"verify\", \"generate\"], protocol_versions: [\"0.3.2\"] };\nexport {};\n";
    assert!(super::authority::adapter_entry_conformance(prefix).is_err());
}

/// The rendered interface is Model-typed, never sample-typed (r6, codex
/// F9): under byte-identical Model/IR/lock pins, a coverage sample whose
/// scalar class disagrees with the Model's declared field type refuses —
/// coverage samples are evidence inputs, not authority over the generated
/// interface.
#[test]
fn model_types_pin_the_rendered_interface() {
    let mut authority = test_authority();
    // The Model declares focus_task's returned title as a string.
    let string_semantics = test_semantics();
    assert!(super::authority::semantics_match(&coverage_doc(), &authority).is_ok());
    // The same authority pins, with the sample's title flipped to a
    // number: the sample disagrees with the Model's declared type.
    let mut flipped = coverage_doc();
    for requirement in &mut flipped.requirements {
        if requirement.id == "planner.focus_task" {
            if let State::Known { value } = &mut requirement.sample {
                value.input.insert("title".to_owned(), Scalar::Count(7.0));
                value.output.insert("title".to_owned(), Scalar::Count(7.0));
            }
        }
    }
    let refused = super::authority::semantics_match(&flipped, &authority);
    assert_eq!(
        refused.expect_err("sample type flip refuses").status(),
        Status::Unsupported
    );
    // The rendered bytes really are Model-typed: rendering under a Model
    // that declares the title a number emits `number` even though the
    // sample value is text, and rendering is refused when the Model types
    // a field the renderer cannot spell.
    for obligation in &mut authority.semantics {
        if obligation.id == "planner.focus_task" {
            for (field, spelling) in &mut obligation.return_fields {
                if field == "title" {
                    *spelling = "number".to_owned();
                }
            }
        }
    }
    assert!(super::authority::semantics_match(&flipped, &authority).is_ok());
    let inventory =
        render_inventory(&flipped, "apps/planner/gen", &authority.semantics).expect("inventory");
    let handler = String::from_utf8(
        inventory
            .iter()
            .find(|artifact| artifact.path == "apps/planner/gen/src/focus-task.ts")
            .expect("handler")
            .bytes
            .clone(),
    )
    .expect("utf8");
    assert!(
        handler.contains("title: number;"),
        "the rendered type follows the Model, not the sample: {handler}"
    );
    // A Model type the renderer cannot spell as a scalar surface refuses.
    let mut untypeable = authority.clone();
    for obligation in &mut untypeable.semantics {
        if obligation.id == "planner.focus_task" {
            for (field, spelling) in &mut obligation.return_fields {
                if field == "title" {
                    *spelling = String::new();
                }
            }
        }
    }
    let refused = super::authority::semantics_match(&flipped, &untypeable);
    assert_eq!(
        refused.expect_err("untypeable field refuses").status(),
        Status::Unsupported
    );
    assert!(
        render_inventory(&flipped, "apps/planner/gen", &untypeable.semantics).is_err(),
        "the renderer refuses a field the Model cannot type"
    );
    let _ = string_semantics;
}

/// The custody object store is never first-writer-wins (r6, devin F4): a
/// pre-planted self-consistent record under a predicted generation name,
/// and a pre-planted retained manifest, are conflicts — the transaction's
/// own minted bytes are the only new custody a mutation may create.
#[test]
fn planted_custody_records_and_manifests_are_conflicts() {
    // Derive the exact first-generation custody in a sibling root, then
    // replant altered versions under the same names before the apply.
    let source = project_root("r6-plant-source");
    apply_first_generation(&source);
    let generation = history::current(&source).expect("custody").expect("gen");
    let hex = generation.strip_prefix("sha256:").expect("digest");
    let record_bytes =
        std::fs::read(source.join(history::custody_logical(&format!("objects/{hex}"))))
            .expect("record");
    let manifest_bytes =
        std::fs::read(source.join(super::custody::MANIFEST_LOGICAL)).expect("manifest");

    // A planted record with a rewritten lineage pin and recomputed
    // recordDigest under the same generation name.
    let target = project_root("r6-plant-record");
    write_input(
        &target,
        "model/planner.model.json",
        b"{ \"planner\": true }\n",
    );
    write_test_lock(&target);
    let mut planted: GenerationRecord = serde_json::from_slice(&record_bytes).expect("record wire");
    planted.previous_generation = State::known(GenerationDigest {
        generation_digest: PIN_A.to_owned(),
    });
    planted.record_digest = super::record_identity(&planted);
    write_input(
        &target,
        &history::custody_logical(&format!("objects/{hex}")),
        &serde_json::to_vec(&planted).expect("wire"),
    );
    let ownership = ownership_doc();
    let assignment = &ownership.modules[0];
    let target_assignment = &assignment.targets[0];
    let admitted =
        coverage::admit(assignment, target_assignment, &coverage_doc()).expect("admission");
    // The predicted generation identity reproduces the source apply's own
    // closure (the ownership document is a pinned closure input of every
    // CLI plan — see `apply_first_generation`).
    let ownership_bytes = serde_json::to_vec(&ownership).expect("wire");
    write_input(&target, "ownership.json", &ownership_bytes);
    let built = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage_doc(),
        vec![
            InputPin {
                path: "model/planner.model.json".to_owned(),
                digest: digest(b"{ \"planner\": true }\n"),
            },
            InputPin {
                path: "ownership.json".to_owned(),
                digest: digest(&ownership_bytes),
            },
        ],
        &empty_before(),
        None,
        None,
        hash(&coverage_doc()),
        &test_semantics(),
    )
    .expect("plan");
    let refused = apply::apply(
        &target,
        &serde_json::to_vec(&built).expect("wire"),
        &serde_json::to_vec(&coverage_doc()).expect("wire"),
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert!(
        !history::exists(&target),
        "the planted record never becomes committed lineage"
    );
    let _ = std::fs::remove_dir_all(&target);

    // A planted retained manifest under the predicted generation name.
    let manifest_target = project_root("r6-plant-manifest");
    write_input(
        &manifest_target,
        "model/planner.model.json",
        b"{ \"planner\": true }\n",
    );
    write_input(&manifest_target, "ownership.json", &ownership_bytes);
    write_test_lock(&manifest_target);
    let mut forged_manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).expect("manifest wire");
    forged_manifest["artifacts"]
        .as_array_mut()
        .expect("artifacts")
        .push(serde_json::json!({
            "path": "maintained/victim.ts",
            "content": PIN_A,
        }));
    write_input(
        &manifest_target,
        &history::custody_logical(&format!("manifests/{hex}")),
        &serde_json::to_vec(&forged_manifest).expect("wire"),
    );
    let refused = apply::apply(
        &manifest_target,
        &serde_json::to_vec(&built).expect("wire"),
        &serde_json::to_vec(&coverage_doc()).expect("wire"),
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
        Some(&test_authority()),
        None,
    );
    assert_eq!(refused.status(), Status::Invalid, "{refused:?}");
    assert!(
        !history::exists(&manifest_target),
        "the planted manifest never becomes the authoritative snapshot"
    );
    let _ = std::fs::remove_dir_all(&manifest_target);
    let _ = std::fs::remove_dir_all(&source);
}

/// The derived authority comes from one filesystem snapshot (r6, codex
/// F11 / devin F8): the single-read derivation reproduces the exact
/// historical Model/IR/lock pins the shipped fixture coverage carries —
/// any divergence between the envelope payload the authority hashes and
/// the load pipeline's own bytes would move the Model pin here.
#[test]
fn single_snapshot_authority_pins_the_exact_model_digest() {
    // The fixture is copied into a selection-legal project-relative
    // directory (the selection grammar refuses `..` traversal).
    let fixture_source = [
        "tests/fixtures/managed/planner",
        "../../tests/fixtures/managed/planner",
    ]
    .into_iter()
    .find(|path| std::path::Path::new(path).join("lekalo").is_dir())
    .expect("planner fixture");
    let project = "target/lekalo-snapshot-fixture/planner";
    let _ = std::fs::remove_dir_all(project);
    copy_tree(
        std::path::Path::new(fixture_source),
        std::path::Path::new(project),
    );
    let selection = crate::loader::LoadSelection {
        project: Some(project.to_owned()),
    };
    let authority = super::authority::derive(&selection, "planner").expect("authority");
    let coverage_bytes = std::fs::read(std::path::Path::new(fixture_source).join("coverage.json"))
        .expect("coverage");
    let coverage: Coverage = serde_json::from_slice(&coverage_bytes).expect("wire");
    let State::Known { value: pins } = &coverage.authority else {
        panic!("fixture coverage pins its authority");
    };
    assert_eq!(authority.model_digest, pins.model, "model pin");
    assert_eq!(authority.ir_digest, pins.ir, "ir pin");
    assert_eq!(authority.lock_digest, pins.lock, "lock pin");
    // The resolved semantics type the fixture's fields from the Model:
    // complete_task's done is the named boolean scalar, focus_task's
    // entity fields resolve through task_id/text.
    let complete = authority
        .semantics
        .iter()
        .find(|obligation| obligation.id == "planner.complete_task")
        .expect("command obligation");
    assert_eq!(
        complete
            .input
            .iter()
            .find(|(name, _)| name == "done")
            .map(|(_, spelling)| spelling.clone()),
        Some("boolean".to_owned()),
        "named scalar refs resolve to their declared base"
    );
    let focus = authority
        .semantics
        .iter()
        .find(|obligation| obligation.id == "planner.focus_task")
        .expect("query obligation");
    assert_eq!(
        focus.return_fields,
        vec![
            ("id".to_owned(), "uuid".to_owned()),
            ("title".to_owned(), "string".to_owned()),
        ],
        "entity field types resolve through the compiled project"
    );
}

// ---------------------------------------------------------------------------
// r6→r7 regression tests: devin F1–F4 / codex F1–F7 (round-6 review union).
// ---------------------------------------------------------------------------

/// A resolve journal in a state the resolve writer never emits (`publishing`)
/// never anchors (devin F1c): recovery stays owed and no byte moves.
#[test]
fn resolve_journal_in_never_emitted_state_never_anchors() {
    let root = project_root("r7-resolve-state");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let path = "apps/planner/gen/src/focus-task.ts";
    let drift = b"// drift\n";
    write_input(&root, path, drift);
    let mut transaction = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "r7-res-state-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Resolve,
        plan_digest: State::Absent,
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: path.to_owned(),
            op: PendingOp::Restore,
        }],
        affected_files: vec![path.to_owned()],
        conflict: State::Absent,
        before_digest: State::known(digest(drift)),
    };
    transaction.plan_digest = State::known(plan_digest(&transaction));
    history::write_transaction(&root, &transaction).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(path)).expect("drift"),
        drift.to_vec(),
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "recovery stays owed on a never-emitted journal shape"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A resolve journal whose plan digest is not its own self digest never
/// anchors (devin F1a): `staged_resolve` pins `planDigest =
/// plan_digest(transaction)`, so a forged `sha256:99…` value is a shape the
/// confirmed flow could never write.
#[test]
fn forged_resolve_plan_digest_never_anchors() {
    let root = project_root("r7-resolve-digest");
    apply_first_generation(&root);
    let generation = history::current(&root).expect("custody").expect("gen");
    let path = "apps/planner/gen/src/focus-task.ts";
    let drift = b"// drift\n";
    write_input(&root, path, drift);
    let transaction = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "r7-res-digest-0001".to_owned(),
        state: TransactionState::Restoring,
        action: TransactionAction::Resolve,
        plan_digest: State::known(
            "sha256:9999999999999999999999999999999999999999999999999999999999999999".to_owned(),
        ),
        generation_digest: State::known(GenerationDigest {
            generation_digest: generation.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending: vec![Pending {
            path: path.to_owned(),
            op: PendingOp::Restore,
        }],
        affected_files: vec![path.to_owned()],
        conflict: State::Absent,
        before_digest: State::known(digest(drift)),
    };
    history::write_transaction(&root, &transaction).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(path)).expect("drift"),
        drift.to_vec(),
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "recovery stays owed while the confirmation digest is forged"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A self-consistent forged successor with replace rows over unchanged
/// bytes never hijacks the committed pointer through the completion path
/// (codex F1): the plan's action/before-state classification must be the
/// apply machinery's own — `Replace` where recorded == after is a
/// classification `classify` never produces — and the closure pin the
/// record mints must resolve against live bytes.
#[test]
fn forged_completion_never_hijacks_the_pointer() {
    let root = project_root("r7-forged-completion");
    apply_first_generation(&root);
    let committed = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &committed).expect("record");
    let plan = history::plan(&root, &record.plan_digest).expect("retained plan");
    // The forged successor: the genuine outputs as Replace rows with
    // before = after = the live (committed) digests — the r6 codex F1
    // reproduction — plus one extra closure pin riding the record.
    let mut minted = plan.clone();
    for output in &mut minted.outputs {
        output.action = OutputAction::Replace;
        output.before_digest = output.after_digest.clone();
    }
    minted.plan_digest = plan_digest(&minted);
    history::write_object(&root, &serde_json::to_vec_pretty(&minted).expect("wire"))
        .expect("minted plan object");
    let mut forged = record.clone();
    forged.previous_generation = State::known(GenerationDigest {
        generation_digest: committed.clone(),
    });
    forged.plan_digest = minted.plan_digest.clone();
    forged.input_closure.push(InputPin {
        path: "zz-extra.yaml".to_owned(),
        digest: digest(b"attacker-chosen"),
    });
    forged.generation_digest = super::generation_digest(&forged);
    forged.record_digest = super::record_identity(&forged);
    let forged_hex = forged
        .generation_digest
        .strip_prefix("sha256:")
        .expect("digest")
        .to_owned();
    write_input(
        &root,
        &history::custody_logical(&format!("objects/{forged_hex}")),
        &serde_json::to_vec(&forged).expect("wire"),
    );
    let mut pending: Vec<Pending> = minted
        .outputs
        .iter()
        .map(|output| Pending {
            path: output.path.clone(),
            op: PendingOp::Publish,
        })
        .collect();
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    let mut affected: Vec<String> = minted
        .outputs
        .iter()
        .map(|output| output.path.clone())
        .collect();
    affected.push("zz-extra.yaml".to_owned());
    affected.sort();
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "r7-forged-comp-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(minted.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: forged.generation_digest.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending,
        affected_files: affected,
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        history::current(&root).expect("custody"),
        Some(committed),
        "the committed pointer never advances to caller-minted lineage"
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "recovery stays owed"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A forged first-publication journal never deletes an unclaimed file
/// outside the caller-declared generated roots (devin F2): with nothing
/// committed, every r6 binding was vacuous — the confinement now derives
/// from the closure-pinned ownership document (hash-bound to the record's
/// `ownershipDigest`) and the deterministic render, so even a forged
/// record/plan pair carrying the *real* ownership pin and a retained
/// coverage object cannot name a maintained file as an output.
#[test]
fn forged_first_publication_never_deletes_outside_declared_roots() {
    let root = project_root("r7-forged-first-publication");
    let ownership = ownership_doc();
    let coverage = coverage_doc();
    let model = b"{ \"planner\": true }\n";
    write_input(&root, "model/planner.model.json", model);
    let ownership_bytes = serde_json::to_vec(&ownership).expect("wire");
    write_input(&root, "ownership.json", &ownership_bytes);
    // Retain the genuine coverage and a victim snapshot as objects.
    let coverage_bytes = serde_json::to_vec(&coverage).expect("wire");
    history::write_object(&root, &coverage_bytes).expect("coverage object");
    let victim = "maintained/victim.ts";
    let victim_bytes = b"// HAND MAINTAINED\n";
    write_input(&root, victim, victim_bytes);
    history::write_object(&root, victim_bytes).expect("victim object");
    // The forged plan: the genuine closure pins (which resolve live) plus
    // a "create" for the victim with after = the victim's live digest.
    let assignment = &ownership.modules[0];
    let target = &assignment.targets[0];
    let admitted = coverage::admit(assignment, target, &coverage).expect("admission");
    let mut minted = plan::build(
        PlanAction::Promotion,
        &admitted,
        &coverage,
        vec![
            InputPin {
                path: "model/planner.model.json".to_owned(),
                digest: digest(model),
            },
            InputPin {
                path: "ownership.json".to_owned(),
                digest: digest(&ownership_bytes),
            },
        ],
        &empty_before(),
        None,
        None,
        hash(&coverage),
        &test_semantics(),
    )
    .expect("plan");
    minted.outputs.push(PlanOutput {
        path: victim.to_owned(),
        action: OutputAction::Create,
        role: ArtifactRole::Implementation,
        owner_module: "planner".to_owned(),
        owner_target: "node-typescript".to_owned(),
        before_digest: State::Absent,
        after_digest: State::known(digest(victim_bytes)),
    });
    minted
        .outputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    minted.plan_digest = plan_digest(&minted);
    history::write_object(&root, &serde_json::to_vec_pretty(&minted).expect("wire"))
        .expect("minted plan object");
    // The forged record: no previous link, the real ownership hash, and
    // every minted output with its content-addressed snapshot.
    let outputs: Vec<RecordOutput> = minted
        .outputs
        .iter()
        .map(|output| RecordOutput {
            path: output.path.clone(),
            role: output.role,
            owner_module: output.owner_module.clone(),
            owner_target: output.owner_target.clone(),
            digest: output.after_digest.value().expect("after").clone(),
            before_ref: State::Absent,
            after_ref: State::known(output.after_digest.value().expect("after").clone()),
        })
        .collect();
    let mut forged = GenerationRecord {
        schema_version: header("managed-generation-record").0,
        identity: header("managed-generation-record").1,
        plan_digest: minted.plan_digest.clone(),
        generation_digest: String::new(),
        previous_generation: State::Absent,
        input_closure: minted.input_closure.clone(),
        outputs,
        ownership_digest: hash(&ownership),
        lock_pin: State::known(test_authority().lock_digest),
        record_digest: String::new(),
    };
    forged.generation_digest = super::generation_digest(&forged);
    forged.record_digest = super::record_identity(&forged);
    let forged_hex = forged
        .generation_digest
        .strip_prefix("sha256:")
        .expect("digest")
        .to_owned();
    write_input(
        &root,
        &history::custody_logical(&format!("objects/{forged_hex}")),
        &serde_json::to_vec(&forged).expect("wire"),
    );
    let mut pending: Vec<Pending> = minted
        .outputs
        .iter()
        .map(|output| Pending {
            path: output.path.clone(),
            op: PendingOp::Publish,
        })
        .collect();
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    let mut affected: Vec<String> = minted
        .outputs
        .iter()
        .map(|output| output.path.clone())
        .collect();
    affected.sort();
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "r7-forged-first-0001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(minted.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: forged.generation_digest.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending,
        affected_files: affected,
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert!(
        root.join(victim).is_file(),
        "the unclaimed file outside every declared generated root survives"
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "recovery stays owed"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// An unclaimed file *inside* the generated root survives a forged
/// successor whose single "create" names it (codex F2): the deterministic
/// render never produced the path, so deletion authority never derives
/// from a caller-authored plan row — the apply machinery itself would
/// have refused the create on live bytes.
#[test]
fn unclaimed_file_inside_generated_root_survives_forged_create() {
    let root = project_root("r7-forged-inside-root");
    apply_first_generation(&root);
    let committed = history::current(&root).expect("custody").expect("gen");
    let record = history::record(&root, &committed).expect("record");
    let plan = history::plan(&root, &record.plan_digest).expect("retained plan");
    let victim = "apps/planner/gen/NOTES.md";
    let victim_bytes = b"dropped by a user inside the generated root\n";
    write_input(&root, victim, victim_bytes);
    history::write_object(&root, victim_bytes).expect("victim object");
    let mut minted = plan.clone();
    minted.outputs.push(PlanOutput {
        path: victim.to_owned(),
        action: OutputAction::Create,
        role: ArtifactRole::Implementation,
        owner_module: "planner".to_owned(),
        owner_target: "node-typescript".to_owned(),
        before_digest: State::Absent,
        after_digest: State::known(digest(victim_bytes)),
    });
    minted
        .outputs
        .sort_by(|left, right| left.path.cmp(&right.path));
    minted.plan_digest = plan_digest(&minted);
    history::write_object(&root, &serde_json::to_vec_pretty(&minted).expect("wire"))
        .expect("minted plan object");
    let mut outputs = record.outputs.clone();
    outputs.push(RecordOutput {
        path: victim.to_owned(),
        role: ArtifactRole::Implementation,
        owner_module: "planner".to_owned(),
        owner_target: "node-typescript".to_owned(),
        digest: digest(victim_bytes),
        before_ref: State::Absent,
        after_ref: State::known(digest(victim_bytes)),
    });
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    let mut forged = record.clone();
    forged.previous_generation = State::known(GenerationDigest {
        generation_digest: committed.clone(),
    });
    forged.plan_digest = minted.plan_digest.clone();
    forged.outputs = outputs;
    forged.generation_digest = super::generation_digest(&forged);
    forged.record_digest = super::record_identity(&forged);
    let forged_hex = forged
        .generation_digest
        .strip_prefix("sha256:")
        .expect("digest")
        .to_owned();
    write_input(
        &root,
        &history::custody_logical(&format!("objects/{forged_hex}")),
        &serde_json::to_vec(&forged).expect("wire"),
    );
    let mut pending: Vec<Pending> = minted
        .outputs
        .iter()
        .map(|output| Pending {
            path: output.path.clone(),
            op: PendingOp::Publish,
        })
        .collect();
    pending.push(Pending {
        path: super::custody::MANIFEST_LOGICAL.to_owned(),
        op: PendingOp::Publish,
    });
    pending.sort_by(|left, right| (&left.path, &left.op).cmp(&(&right.path, &right.op)));
    let mut affected: Vec<String> = minted
        .outputs
        .iter()
        .map(|output| output.path.clone())
        .collect();
    affected.sort();
    let journal = Transaction {
        schema_version: header("managed-transaction").0,
        identity: header("managed-transaction").1,
        transaction_id: "r7-forged-inside-001".to_owned(),
        state: TransactionState::Publishing,
        action: TransactionAction::Apply,
        plan_digest: State::known(minted.plan_digest.clone()),
        generation_digest: State::known(GenerationDigest {
            generation_digest: forged.generation_digest.clone(),
        }),
        before_manifest_digest: State::Absent,
        after_manifest_digest: State::Absent,
        pending,
        affected_files: affected,
        conflict: State::Absent,
        before_digest: State::Absent,
    };
    history::write_transaction(&root, &journal).expect("journal");
    let receipt = recovery::run(&root, &test_authority_of);
    assert_eq!(receipt.status(), Status::Invalid, "{receipt:?}");
    assert_eq!(
        std::fs::read(root.join(victim)).expect("victim"),
        victim_bytes.to_vec(),
        "an unclaimed file inside the generated root is never deleted on a forged word",
    );
    assert!(
        !history::unresolved(&root).is_ok_and(|unresolved| unresolved.is_empty()),
        "recovery stays owed"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Named function expressions and regex bodies never qualify as callable
/// declarations (devin F3): the name of `(function phook(){})` binds
/// inside the expression only, and a regex literal's body is pattern
/// text — `export { phook }` resolves to nothing the generated test
/// could import.
#[test]
fn named_function_expressions_and_regexes_never_declare() {
    // A parenthesized named function expression (call argument).
    let paren = b"const g = (function phook(x){ return x; });\nexport { phook };\n";
    assert!(
        !super::bindings::exports_symbol(paren, "phook"),
        "a named function expression inside parentheses never declares module scope"
    );
    // A plain assignment-position named function expression.
    let assigned = b"const g = function nfehook(x){ return x; };\nexport { nfehook };\n";
    assert!(
        !super::bindings::exports_symbol(assigned, "nfehook"),
        "an assignment-position named function expression never declares module scope"
    );
    // A regex literal carrying the declaration shape in its body.
    let regex = b"const r = /function reghook(x)/g;\nexport { reghook };\n";
    assert!(
        !super::bindings::exports_symbol(regex, "reghook"),
        "regex-literal bodies are blank, never declarations"
    );
    // A regex in return position (division-lookalike) is backstopped by
    // statement position: `return` never precedes a declaration.
    let returned = b"function f(){ return /function bshook(x)/; }\nexport { bshook };\n";
    assert!(
        !super::bindings::exports_symbol(returned, "bshook"),
        "`return` is expression context, never statement position"
    );
    // Controls: genuine statement-position declarations still qualify.
    assert!(super::bindings::exports_symbol(
        b"function realhook(x) { return x; }\nexport { realhook };\n",
        "realhook"
    ));
    assert!(super::bindings::exports_symbol(
        b"export async function asyncHook(x) { return x; }\n",
        "asyncHook"
    ));
    assert!(super::bindings::exports_symbol(
        b"export const arrowhook = (input) => input ?? null;\n",
        "arrowhook"
    ));
}

/// Inert adapter structures never prove capability (devin F4 / codex F5):
/// object keys with inert values, a literal array bound to any other
/// name, and literals inside call arguments are exactly as non-behavioral
/// as the string soup.
#[test]
fn inert_adapter_structures_never_prove_capability() {
    let keys = b"const ops = { describe: 1, validate: 1, verify: 1, generate: 1, protocol_version: 1 };\nexport {};\n";
    assert!(
        super::authority::adapter_entry_conformance(keys).is_err(),
        "inert object keys never qualify the operations"
    );
    let array = b"export const ops = [\"describe\", \"validate\", \"verify\", \"generate\"];\nconst handshake = { protocol_version: \"0.8.0\" };\nexport {};\n";
    assert!(
        super::authority::adapter_entry_conformance(array).is_err(),
        "a literal array bound to a non-operations name never qualifies"
    );
    let logged = b"console.log(\"describe\");\nconst capabilities = { operations: [\"validate\", \"verify\", \"generate\"], protocol_version: \"0.8.0\" };\n";
    assert!(
        super::authority::adapter_entry_conformance(logged).is_err(),
        "a literal inside a call argument never qualifies"
    );
    // Controls: the protocol-document row and invocable identifiers pass.
    let quoted = b"const capabilities = { operations: [\"describe\", \"validate\", \"verify\", \"generate\"], protocol_version: \"0.3.2\" };\nexport {};\n";
    assert!(super::authority::adapter_entry_conformance(quoted).is_ok());
    let structural = b"export function describe() {}\nexport function validate() {}\nexport function verify() {}\nexport function generate() {}\nconst handshake = { protocol_version: \"0.3.2\" };\n";
    assert!(super::authority::adapter_entry_conformance(structural).is_ok());
}

/// The confined read path refuses shared-inode leaves (codex F6): an
/// in-root hardlink to a file outside the project never sources bytes
/// through `read_file` — the same exclusivity the write path always
/// required.
#[test]
fn read_path_refuses_a_shared_inode_leaf() {
    let root = project_root("r7-hardlink-read");
    let outside = std::env::temp_dir().join(format!(
        "lekalo-managed-outside-{}-hardlink.ts",
        std::process::id()
    ));
    std::fs::write(&outside, b"// bytes outside the project\n").expect("outside");
    let directory = root.join("src/extensions");
    std::fs::create_dir_all(&directory).expect("mkdir");
    let inside = directory.join("hardlinked-binding.ts");
    std::fs::hard_link(&outside, &inside).expect("hardlink");
    let read = super::fs::read_file(&root, "src/extensions/hardlinked-binding.ts");
    assert!(
        read.is_err(),
        "an in-root hardlink (shared inode) never reads through the confined capability"
    );
    assert_eq!(
        std::fs::read(&outside).expect("outside"),
        b"// bytes outside the project\n".to_vec(),
        "the outside alias is untouched"
    );
    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&root);
}

/// Credential-shaped caller values never enter the receipt stream (codex
/// F7): the screening is spelling-canonical — any case spelling of the
/// `gh[pousr]_` prefix — and refuses the ownership project id and the
/// coverage subject ids at parse time.
#[test]
fn credential_shaped_caller_values_never_enter_receipts() {
    assert!(!super::credential_free("ghp_ABC123def456GHI789jkl"));
    assert!(!super::credential_free("GHP_ABC123def456GHI789jkl"));
    assert!(!super::credential_free("project GhS_abcdef0123456789abcd"));
    assert!(super::credential_free("planner-demo"));
    assert!(super::credential_free("ghp_short"));
    let mut ownership = ownership_doc();
    ownership.project = "GHP_ABC123def456GHI789jkl".to_owned();
    let refused = super::parse_ownership(&serde_json::to_vec(&ownership).expect("wire"));
    assert!(refused.is_err(), "a credential-shaped project id refuses");
    let mut coverage = coverage_doc();
    coverage.module = "ghu_ABC123def456GHI789jkl".to_owned();
    let refused = super::parse_coverage(&serde_json::to_vec(&coverage).expect("wire"));
    assert!(refused.is_err(), "a credential-shaped module id refuses");
}
