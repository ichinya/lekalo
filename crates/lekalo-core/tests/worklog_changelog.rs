//! Issue #51 worklog-changelog pilot composition tests: the closed wire,
//! the pure join against the pinned compilation, and diagnostic
//! normalization.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::loader::LoadSelection;
use lekalo_core::worklog_changelog::{self, BackendProvenance, Composition};

const PILOT_JSON: &str = "tests/fixtures/pilot/worklog-changelog/attachments/pilot.json";
const MODEL_PROJECT: &str = "tests/fixtures/pilot/worklog-changelog/model";
const EVIDENCE_JSON: &str = "tests/fixtures/pilot/worklog-changelog/evidence/evidence.json";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// The process cwd is shared state; composition joins run one at a
/// time with the workspace root as the project-relative base.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn with_workspace_cwd<T>(step: impl FnOnce() -> T) -> T {
    let guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(step));
    std::env::set_current_dir(original).expect("restore cwd");
    drop(guard);
    outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn compilation() -> lekalo_core::ir::Compilation {
    let selection = LoadSelection {
        project: Some(MODEL_PROJECT.to_owned()),
    };
    lekalo_core::cache::load_compiled(&selection, true)
        .map(|(_, compilation)| compilation)
        .expect("fixture compiles")
}

fn composition_bytes() -> Vec<u8> {
    std::fs::read(PILOT_JSON).expect("pilot composition fixture present")
}

#[test]
fn pilot_document_joins_the_pinned_compilation() {
    with_workspace_cwd(|| {
        let compilation = compilation();
        let current = format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(compilation.project.to_canonical_json().as_bytes())
        );
        let composition: Composition =
            worklog_changelog::parse(&composition_bytes()).expect("pilot document parses");
        println!("pinned ir  : {}", composition.ir_digest());
        println!("current ir : {current}");
        println!(
            "project    : {} vs {}",
            composition.project_id(),
            compilation
                .project
                .project
                .as_ref()
                .map(|project| project.id.as_str())
                .unwrap_or_default()
        );
        composition
            .validate(&compilation.project)
            .expect("composition joins the pinned compilation");
    });
}

/// The committed pilot scenario corpus: every document decodes through
/// the production Scenario IR module and its committed bytes are the
/// canonical form.
#[test]
fn pilot_scenario_corpus_decodes_and_canonicalizes() {
    use lekalo_core::scenario::ScenarioIr;
    let dir = workspace_root().join("tests/fixtures/pilot/worklog-changelog/scenarios");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("scenario corpus present") {
        let path = entry.expect("scenario entry").path();
        let bytes = std::fs::read(&path).expect("scenario bytes");
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("scenario JSON parses");
        let ir = ScenarioIr::from_value(&value)
            .unwrap_or_else(|set| panic!("{}: rejected: {set:?}", path.display()));
        let text = std::str::from_utf8(&bytes).expect("utf8");
        let canonical = ir
            .canonical_bytes()
            .unwrap_or_else(|set| panic!("{}: export refused: {set:?}", path.display()));
        assert_eq!(
            canonical,
            text,
            "{}: canonical bytes diverge",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 12, "the corpus carries the full scenario set");
}

// ---------------------------------------------------------------------------
// Negative vectors: one mutation per registered rule, each refused with
// exactly the typed detail token.
// ---------------------------------------------------------------------------

fn valid_document() -> serde_json::Value {
    serde_json::from_slice(&composition_bytes()).expect("valid fixture parses")
}

fn detail_of(set: &lekalo_core::diagnostics::DiagnosticSet) -> String {
    let rendered = serde_json::to_string(set.as_slice().first().expect("one diagnostic"))
        .expect("diagnostic serializes");
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("diagnostic JSON");
    value["data"]["detail"]
        .as_str()
        .expect("closed detail token")
        .to_owned()
}

#[test]
fn negative_vectors_refuse_with_the_registered_rule_and_detail() {
    with_workspace_cwd(|| {
        let compilation = compilation();
        let validate = |document: &serde_json::Value| {
            let bytes = serde_json::to_vec(document).expect("vector serializes");
            worklog_changelog::parse(&bytes)
                .and_then(|composition| {
                    composition
                        .validate(&compilation.project)
                        .map(|_| composition)
                })
                .map_err(|set| {
                    assert_eq!(set.as_slice().len(), 1, "one diagnostic only: {set:?}");
                    (set.as_slice()[0].id().to_owned(), detail_of(&set))
                })
        };

        // The committed fixture validates.
        assert!(validate(&valid_document()).is_ok());

        // WC-001: unknown field.
        let mut document = valid_document();
        document
            .as_object_mut()
            .unwrap()
            .insert("mystery".to_owned(), serde_json::json!(true));
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "unknown-field");

        // WC-001: wrong schema version.
        let mut document = valid_document();
        document["schemaVersion"] = serde_json::json!("lekalo/worklog-changelog-pilot/v9.9.9");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "schema-version");

        // WC-001: stale IR custody.
        let mut document = valid_document();
        document["irRef"]["digest"] = serde_json::json!(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        );
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "ir-custody");

        // WC-001: duplicate operation role.
        let mut document = valid_document();
        let clone = document["operations"][0].clone();
        document["operations"].as_array_mut().unwrap().push(clone);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "duplicate-role");

        // WC-002: the completion role declares a provider effect.
        let mut document = valid_document();
        document["operations"][0]["effects"] = serde_json::json!([
            { "kind": "update", "effectId": "worklog.effect_task_state_write" },
            { "kind": "enqueue-job", "effectId": "worklog.effect_provider_job_enqueue" }
        ]);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.effects-coupled");
        assert_eq!(detail, "effect-count-mismatch");

        // WC-002: an independence rule whose subject shares one
        // declared effect id with its forbidden target.
        let mut document = valid_document();
        document["independentEffects"][0]["mustNotEnable"] =
            serde_json::json!(["worklog.write_worklog", "worklog.write_worklog"]);
        // (dedup keeps it one target; instead couple via the write role
        // declaring the completion effect)
        document["operations"][1]["effects"] = serde_json::json!([
            { "kind": "update", "effectId": "worklog.effect_task_state_write" }
        ]);
        let (rule, _) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.effects-coupled");

        // WC-003: the approval binding loses its preview digest field.
        let mut document = valid_document();
        document["approvals"][0]["previewDigestField"] = serde_json::json!("not_a_field");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.approval-unbound");
        assert_eq!(detail, "field-not-declared");

        // WC-004: the snapshot loses its append-only immutability.
        let mut document = valid_document();
        document["snapshots"][0]["immutability"] = serde_json::json!("mutable");
        let (rule, _) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");

        // WC-005: the public allowlist serves a field the DTO does not
        // declare.
        let mut document = valid_document();
        document["publicAllowlist"]["fields"][0]["name"] = serde_json::json!("not_in_dto");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.public-data-unsafe");
        assert_eq!(detail, "field-not-in-dto");

        // WC-006: two operations share one replay scope identity.
        let mut document = valid_document();
        document["replayScopes"][1]["scope"] = document["replayScopes"][0]["scope"].clone();
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "replay-scope-collision");

        // WC-007: the publication capability row disappears.
        let mut document = valid_document();
        document["capabilityRequirements"]
            .as_array_mut()
            .unwrap()
            .retain(|requirement| requirement["capability"] != "publish.approval");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.evidence-incomplete");
        assert_eq!(detail, "publish.approval");

        // C7: a required operation member disappears — the wire refuses
        // with the schema's required-member token, never a default.
        let mut document = valid_document();
        document["operations"][0]
            .as_object_mut()
            .unwrap()
            .remove("defaultSelection");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "missing-member");

        // C7: an inline `replay` member is decoded and validated, never
        // ignored — an unknown member inside it refuses.
        let mut document = valid_document();
        document["operations"][0]["replay"] = serde_json::json!({"unexpected": "ignored"});
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "unknown-field");

        // C7: an inline `replay` naming another operation refuses.
        let mut document = valid_document();
        document["operations"][0]["replay"] = serde_json::json!({
            "operation": "worklog.write_worklog",
            "scope": "taskhub.scope.inline",
            "keyRef": "correlation_key",
            "duplicate": "replay_result",
            "sameKeyDifferentRequest": "conflict",
            "payloadBinding": "canonical_digest"
        });
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "replay-operation");

        // C4: a writing operation cannot claim the read-only boundary;
        // eligibility is derived from the compiled operation kind.
        let mut document = valid_document();
        let edit = document["operations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|role| role["operation"] == "worklog.edit_worklog")
            .unwrap();
        edit["commitBoundary"] = serde_json::json!("read_only");
        edit["effects"] = serde_json::json!([
            {"kind": "read", "effectId": "worklog.fabricated_read_0"}
        ]);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.effects-coupled");
        assert_eq!(detail, "read-only-disguise");

        // C4: erasing the public query's read projection refuses.
        let mut document = valid_document();
        document["operations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|role| role["operation"] == "changelog.public_entry")
            .unwrap()["effects"] = serde_json::json!([]);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.effects-coupled");
        assert_eq!(detail, "read-count-mismatch");

        // C4: every claimed read resolves to one compiled read
        // subject; a fabricated read refuses.
        let mut document = valid_document();
        document["operations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|role| role["operation"] == "changelog.public_entry")
            .unwrap()["effects"] = serde_json::json!([
            {"kind": "read", "effectId": "changelog.audit"}
        ]);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.effects-coupled");
        assert_eq!(detail, "read-set-mismatch");

        // C8: a replay key must be a declared input of its own
        // operation.
        let mut document = valid_document();
        document["replayScopes"][0]["keyRef"] = serde_json::json!("nonexistent_field");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.replay-scope-invalid");
        assert_eq!(detail, "unresolved-replay-key");

        // C8: a correlation key must resolve on every member.
        let mut document = valid_document();
        document["correlations"][0]["keyRef"] = serde_json::json!("body");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.replay-scope-invalid");
        assert_eq!(detail, "unresolved-correlation-key");

        // C9: weakening a mandatory guarantee below its full minimum
        // refuses.
        let mut document = valid_document();
        document["capabilityRequirements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|requirement| requirement["capability"] == "publish.approval")
            .unwrap()["minimum"] = serde_json::json!("partial");
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.evidence-incomplete");
        assert_eq!(detail, "publish.approval");

        // C9: binding a mandatory guarantee to the wrong operation
        // refuses.
        let mut document = valid_document();
        document["capabilityRequirements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|requirement| requirement["capability"] == "publish.approval")
            .unwrap()["operationRefs"] = serde_json::json!(["worklog.complete_task"]);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.evidence-incomplete");
        assert_eq!(detail, "publish.approval");

        // C9: dropping one replayed operation from the durable-key
        // guarantee refuses.
        let mut document = valid_document();
        document["capabilityRequirements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|requirement| requirement["capability"] == "idempotency.durable_key")
            .unwrap()["operationRefs"] =
            serde_json::json!(["worklog.complete_task", "worklog.request_provider_comment"]);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.evidence-incomplete");
        assert_eq!(detail, "idempotency.durable_key");

        // C3: two pins for one consumed family conflict instead of
        // silently deduplicating.
        let mut document = valid_document();
        let clone = document["consumed"][0].clone();
        document["consumed"].as_array_mut().unwrap().push(clone);
        let (rule, detail) = validate(&document).unwrap_err();
        assert_eq!(rule, "worklog-changelog.input-invalid");
        assert_eq!(detail, "duplicate-consumed-family");
    });
}

#[test]
fn duplicate_json_keys_are_rejected_before_any_semantics() {
    let duplicate =
        br#"{"schemaVersion":"lekalo/worklog-changelog-pilot/v0.8.0","schemaVersion":"x"}"#;
    let set = worklog_changelog::parse(duplicate).unwrap_err();
    assert_eq!(detail_of(&set), "duplicate-key");
}

fn evidence_admission_target(
    mutation: impl FnOnce(&mut serde_json::Value),
) -> Result<lekalo_core::worklog_changelog::AdmissionReceipt, lekalo_core::result::DomainResult> {
    with_workspace_cwd(|| {
        let compilation = compilation();
        let composition_bytes = composition_bytes();
        let mut evidence: serde_json::Value = serde_json::from_slice(
            &std::fs::read(workspace_root().join(EVIDENCE_JSON)).expect("evidence fixture present"),
        )
        .expect("evidence parses");
        mutation(&mut evidence);
        let report_bytes = serde_json::to_vec(&evidence).expect("report serializes");
        lekalo_core::worklog_changelog::admit(
            &report_bytes,
            &composition_bytes,
            &compilation.project,
            &admitted_backends(),
            lekalo_core::worklog_changelog::AdmissionProfile::Strict,
        )
    })
}

/// The engine provenance receipt the shell resolves for the fixture:
/// the compiled target binding names the php lane, so its bound
/// backend must equal this receipt.
fn admitted_backends() -> std::collections::BTreeMap<String, BackendProvenance> {
    let mut provenance = std::collections::BTreeMap::new();
    provenance.insert(
        "php".to_owned(),
        BackendProvenance {
            identity: "taskhub.runtime/fake-transport".to_owned(),
            digest: "sha256:895ff99de892811b60bbe95eeb6484d960e0ff5e39de4db932b762a12e73b2bc"
                .to_owned(),
        },
    );
    provenance
}

#[test]
fn evidence_admission_is_honest_for_both_profiles() {
    let bytes = std::fs::read(
        workspace_root().join("tests/fixtures/pilot/worklog-changelog/evidence/evidence.json"),
    )
    .expect("evidence fixture present");
    with_workspace_cwd(|| {
        let compilation = compilation();
        let composition_bytes = composition_bytes();
        // Strict: the committed report honestly refuses (unsupported) with
        // blockers listed and the per-target rows retained.
        let strict = lekalo_core::worklog_changelog::admit(
            &bytes,
            &composition_bytes,
            &compilation.project,
            &admitted_backends(),
            lekalo_core::worklog_changelog::AdmissionProfile::Strict,
        )
        .expect("strict admission produces a receipt");
        assert_eq!(strict.verdict, "unsupported");
        assert!(!strict.blockers.is_empty());
        assert_eq!(strict.targets.len(), 2);
        // Permissive: degraded, every row still visible.
        let permissive = lekalo_core::worklog_changelog::admit(
            &bytes,
            &composition_bytes,
            &compilation.project,
            &admitted_backends(),
            lekalo_core::worklog_changelog::AdmissionProfile::Permissive,
        )
        .expect("permissive admission produces a receipt");
        assert_eq!(permissive.verdict, "degraded");
        assert_eq!(permissive.targets.len(), 2);
    });
}

/// C2: a report author cannot remove a required guarantee by erasing
/// rows or clearing flags — the required inventory comes from the
/// composition and the pinned compilation, not from the report.
#[test]
fn forged_or_erased_evidence_cannot_pass_strict_admission() {
    // Erase every capability and assertion row but claim execution:
    // the required inventory is re-derived from the composition.
    let receipt = evidence_admission_target(|report| {
        for target in report["targets"].as_array_mut().unwrap() {
            if target["target"] == "php" {
                target["execution"] = serde_json::json!("executed");
                target["capabilities"] = serde_json::json!([]);
                target["assertions"] = serde_json::json!([]);
                target["blocking"] = serde_json::json!([]);
            }
        }
    })
    .expect("receipt");
    assert_eq!(receipt.verdict, "unsupported");
    assert!(receipt
        .blockers
        .iter()
        .any(|blocker| blocker.starts_with("php:capability-missing:")));

    // An explicit target blocker list stays a blocker even when every
    // row claims success.
    let receipt = evidence_admission_target(|report| {
        for target in report["targets"].as_array_mut().unwrap() {
            if target["target"] == "php" {
                target["blocking"] = serde_json::json!(["publish.approval"]);
            }
        }
    })
    .expect("receipt");
    assert_eq!(receipt.verdict, "unsupported");
    assert!(receipt.blockers.iter().any(|b| b == "publish.approval"));

    // A failed aggregate scenario stays a failure even when a target
    // assertion claims pass; the contradiction itself blocks.
    let receipt = evidence_admission_target(|report| {
        report["scenarios"][0]["status"] = serde_json::json!("fail");
    })
    .expect("receipt");
    assert_eq!(receipt.verdict, "unsupported");
    assert!(receipt
        .blockers
        .iter()
        .any(|b| b.contains("scenario") && b.ends_with(":fail")));

    // An executed claim on an absent backend is refused.
    let receipt = evidence_admission_target(|report| {
        for target in report["targets"].as_array_mut().unwrap() {
            if target["target"] == "php" {
                target["execution"] = serde_json::json!("executed");
                target["backend"] =
                    serde_json::json!({"state": "absent", "identity": null, "digest": null});
            }
        }
    })
    .expect("receipt");
    assert_eq!(receipt.verdict, "unsupported");
    assert!(receipt.blockers.iter().any(|b| b == "php:backend:unbound"));

    // An incomplete consumer closure degrades permissive admission
    // instead of passing it.
    let receipt = evidence_admission_target(|report| {
        report["completeness"]["consumerClosure"] = serde_json::json!("incomplete");
        report["verdict"]["strict"] = serde_json::json!("unsupported");
        report["verdict"]["permissive"] = serde_json::json!("pass");
    })
    .expect("receipt");
    assert!(receipt
        .blockers
        .iter()
        .any(|b| b == "consumers:closure:incomplete"));
    assert!(receipt
        .blockers
        .iter()
        .any(|b| b == "verdict:permissive-claim"));

    // A report pinning different composition bytes is refused outright.
    let result = evidence_admission_target(|report| {
        report["compositionRef"]["digest"] = serde_json::json!(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        );
    });
    assert!(result.is_err());

    // Structurally invalid reports the closed schema rejects are
    // refused by admission too (C6 schema parity).
    for mutation in [
        |report: &mut serde_json::Value| {
            report["reportVersion"] = serde_json::json!("9.9.9");
        },
        |report: &mut serde_json::Value| {
            report["compositionRef"] = serde_json::json!(null);
        },
        |report: &mut serde_json::Value| {
            report["toolchain"] = serde_json::json!({"rogue": true});
        },
        |report: &mut serde_json::Value| {
            report["verdict"] = serde_json::json!(false);
        },
        |report: &mut serde_json::Value| {
            report["targets"][0]["assertions"][0]["rogue"] = serde_json::json!(true);
        },
    ] {
        let result = evidence_admission_target(mutation);
        assert!(result.is_err(), "mutation must refuse");
    }
}
