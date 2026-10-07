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
                from: "subject".to_owned(),
                to: "title".to_owned(),
            },
        ],
        sample: State::known(Sample {
            input: BTreeMap::from([
                ("id".to_owned(), Scalar::Text("t-1".to_owned())),
                ("subject".to_owned(), Scalar::Text("Focus".to_owned())),
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
    root
}

#[test]
fn canonical_renderer_is_byte_stable_and_ordered() {
    let coverage = coverage_doc();
    let first = render_inventory(&coverage, "apps/planner/gen").expect("inventory");
    let second = render_inventory(&coverage, "apps/planner/gen").expect("inventory");
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
            "apps/planner/gen/src/completeTask.ts",
            "apps/planner/gen/test/completeTask.test.ts",
            "apps/planner/gen/src/focusTask.ts",
            "apps/planner/gen/test/focusTask.test.ts"
        ]
    );
    let implementation = String::from_utf8(first[2].bytes.clone()).expect("utf8");
    assert!(!implementation.contains('\r'), "fixed LF endings");
    assert!(!implementation.contains("20"), "no wall-clock data");
    assert!(implementation
        .contains("export function focusTask(input: FocusTaskInput): FocusTaskOutput"));
    assert!(implementation.contains("title: input.subject"));
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
        "apps/planner/gen/src/focusTask.ts".to_owned(),
        Recorded {
            digest: PIN_B.to_owned(),
            role: ArtifactRole::Implementation,
            owner_module: "planner".to_owned(),
            owner_target: "node-typescript".to_owned(),
            object_ref: Some(PIN_B.to_owned()),
        },
    );
    before.actual.insert(
        "apps/planner/gen/src/focusTask.ts".to_owned(),
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
    )
    .expect("plan");
    let plan_bytes = serde_json::to_vec(&built).expect("wire");
    // Missing confirmation never mutates.
    let refused = apply::apply(&root, &plan_bytes, &coverage_bytes, &ownership, None, false);
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
    );
    assert_eq!(applied.status(), Status::Valid, "{applied:?}");
    let generation = history::current(&root)
        .expect("custody")
        .expect("generation");
    for path in [
        "apps/planner/gen/src/focusTask.ts",
        "apps/planner/gen/test/focusTask.test.ts",
    ] {
        assert!(root.join(path).is_file(), "{path} published");
    }
    // Re-applying the identical plan is stale, never a silent rewrite.
    let stale = apply::apply(
        &root,
        &plan_bytes,
        &coverage_bytes,
        &ownership,
        Some(built.plan_digest.as_str()),
        false,
    );
    assert_eq!(stale.status(), Status::Invalid);
    assert_eq!(
        stale.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
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
        record.input_closure.clone(),
        &before,
        Some(generation.clone()),
        None,
        hash(&coverage),
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
    let handler = root.join("apps/planner/gen/src/focusTask.ts");
    std::fs::write(&handler, b"// manually edited\n").expect("edit");
    let drifted = apply::apply(
        &root,
        &serde_json::to_vec(&rebuilt).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(rebuilt.plan_digest.as_str()),
        false,
    );
    assert_eq!(drifted.status(), Status::Invalid);
    assert_eq!(
        drifted.reason_codes().first().map(|code| code.as_str()),
        Some("managed.generated-drift")
    );
    // Resolve restores the exact recorded bytes.
    let resolution = rollback::resolve(&root, "apps/planner/gen/src/focusTask.ts", None, false);
    assert_eq!(
        resolution.status(),
        Status::Invalid,
        "resolve requires its confirmation"
    );
    let preview = rollback::resolve(&root, "apps/planner/gen/src/focusTask.ts", None, true);
    assert_eq!(preview.status(), Status::Valid, "{preview:?}");
    let resolve_digest = resolve_plan_digest(&preview);
    let restored = rollback::resolve(
        &root,
        "apps/planner/gen/src/focusTask.ts",
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
            .find(|o| o.path == "apps/planner/gen/src/focusTask.ts")
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
    )
    .expect("plan");
    let applied = apply::apply(
        &root,
        &serde_json::to_vec(&first).expect("wire"),
        &coverage_bytes,
        &ownership,
        Some(first.plan_digest.as_str()),
        false,
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
    let refused = rollback::apply(&root, &bytes, None, false);
    assert_eq!(refused.status(), Status::Invalid);
    // The destination input closure is checked against the live inputs:
    // the model moved between generations, so the destination is refused
    // until the pinned input bytes are back (finding G).
    let stale = rollback::apply(&root, &bytes, Some(preview.plan_digest.as_str()), false);
    assert_eq!(stale.status(), Status::Invalid, "{stale:?}");
    assert_eq!(
        stale.reason_codes().first().map(|code| code.as_str()),
        Some("managed.plan-stale")
    );
    // With the destination's pinned inputs restored, the rollback runs.
    write_input(&root, "model/planner.model.json", model);
    let restored = rollback::apply(&root, &bytes, Some(preview.plan_digest.as_str()), false);
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
