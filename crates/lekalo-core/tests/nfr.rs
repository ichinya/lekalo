//! Issue #85 NFR integration tests: the closed attachment and evidence
//! wire, the IR custody and scope resolution, the derived report with
//! its per-environment rows and first-class statuses, the gate
//! verdicts (mandatory versus advisory, default versus strict), the
//! semantic diff, the impact synthesis, the neutral trace projection,
//! the canonical bytes, and the no-write boundary.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::loader::LoadSelection;
use lekalo_core::nfr::{EvidenceSet, NfrAttachment};

/// The committed planner fixture: one minimal Lekalo project (an
/// endpoint, a command it invokes, a covering scenario), one NFR
/// attachment, and two evidence sets under different environments.
const FIXTURE: &str = "tests/fixtures/nfr/planner";
const ATTACHMENT: &str = "nfr.attachment.json";
const EVIDENCE_EU: &str = "nfr-evidence.staging-eu.json";
const EVIDENCE_US: &str = "nfr-evidence.staging-us.json";

/// Resolution touches the loader, which resolves selections against the
/// process working directory; serialize every test that changes it.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn fixture_path(name: &str) -> PathBuf {
    workspace_root().join(FIXTURE).join(name)
}

/// Parse the committed fixture attachment.
fn fixture_attachment() -> NfrAttachment {
    let bytes = fs::read(fixture_path(ATTACHMENT)).expect("fixture attachment reads");
    NfrAttachment::parse(&bytes).expect("fixture attachment parses")
}

/// Parse one committed fixture evidence set.
fn fixture_evidence(name: &str) -> EvidenceSet {
    let bytes = fs::read(fixture_path(name)).expect("fixture evidence reads");
    EvidenceSet::parse(&bytes).expect("fixture evidence parses")
}

fn selection() -> LoadSelection {
    LoadSelection {
        project: Some(FIXTURE.to_owned()),
    }
}

fn with_cwd<T>(root: &Path, step: impl FnOnce() -> T) -> T {
    let guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(alias_free_path(root)).expect("enter root");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(step));
    std::env::set_current_dir(original).expect("restore cwd");
    drop(guard);
    outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn validate_fixture() {
    let attachment = fixture_attachment();
    with_cwd(&workspace_root(), || {
        lekalo_core::nfr::validate::validate(&attachment, &selection())
            .expect("fixture attachment validates against its project");
    });
}

#[test]
fn fixture_validates_against_the_bound_ir() {
    validate_fixture();
}

#[test]
fn custody_refuses_foreign_projects_models_and_ir() {
    let attachment = fixture_attachment();
    with_cwd(&workspace_root(), || {
        // A foreign project id denies before any work.
        let foreign = {
            let mut json = attachment_json();
            json["projectId"] = serde_json::json!("planner2");
            NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses")
        };
        let error = lekalo_core::nfr::validate::validate(&foreign, &selection())
            .expect_err("foreign project is denied");
        assert_eq!(error.exit_code(), 3, "custody mismatch is a denial");
    });
    drop(attachment);
}

/// The fixture attachment as raw JSON for mutation vectors.
fn attachment_json() -> serde_json::Value {
    serde_json::from_slice(&fs::read(fixture_path(ATTACHMENT)).expect("fixture reads")).unwrap()
}

#[test]
fn evidence_coherence_rejects_unknown_constraints_and_unit_drift() {
    let attachment = fixture_attachment();
    let evidence = fixture_evidence(EVIDENCE_EU);
    lekalo_core::nfr::validate::validate_evidence(&attachment, &evidence)
        .expect("fixture evidence is coherent");

    // An evidence result naming an undeclared constraint is invalid.
    let mut json =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fixture_path(EVIDENCE_EU)).unwrap())
            .unwrap();
    json["results"][0]["constraintId"] = serde_json::json!("planner.nfr.undeclared");
    let unknown = EvidenceSet::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    assert!(
        lekalo_core::nfr::validate::validate_evidence(&attachment, &unknown).is_err(),
        "unknown constraint is invalid"
    );

    // A measurement unit drifting from the declared unit is invalid.
    let mut json =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fixture_path(EVIDENCE_EU)).unwrap())
            .unwrap();
    json["results"][0]["measurements"][0]["unit"] = serde_json::json!("seconds");
    let drifted = EvidenceSet::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    assert!(
        lekalo_core::nfr::validate::validate_evidence(&attachment, &drifted).is_err(),
        "unit drift is rejected"
    );
}

#[test]
fn wire_rejects_the_closed_negative_matrix() {
    // Dimension/kind partition: ai-budget kinds never live under
    // dimension runtime.
    let mut json = attachment_json();
    json["constraints"][1]["dimension"] = serde_json::json!("ai-budget");
    assert!(NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).is_err());

    // Declaration under mandatory enforcement is illegal.
    let mut json = attachment_json();
    json["constraints"][2]["measurement"]["method"] = serde_json::json!("declaration");
    assert!(NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).is_err());

    // A measurable kind without a gate is illegal.
    let mut json = attachment_json();
    json["constraints"][1]["measurement"]
        .as_object_mut()
        .unwrap()
        .remove("gateRef");
    assert!(NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).is_err());

    // Duplicate constraint ids are one declaration said twice.
    let mut json = attachment_json();
    let clone = json["constraints"][1].clone();
    json["constraints"].as_array_mut().unwrap().push(clone);
    assert!(NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).is_err());

    // Unknown fields are rejected.
    let mut json = attachment_json();
    json["constraints"][1]["unknown"] = serde_json::json!(true);
    assert!(NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn evidence_wire_rejects_value_state_violations() {
    // A known value must carry its number.
    let mut json =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fixture_path(EVIDENCE_EU)).unwrap())
            .unwrap();
    json["results"][0]["measurements"][0]["value"]
        .as_object_mut()
        .unwrap()
        .remove("value");
    assert!(EvidenceSet::parse(&serde_json::to_vec(&json).unwrap()).is_err());

    // An absent value must never carry one.
    let mut json =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fixture_path(EVIDENCE_EU)).unwrap())
            .unwrap();
    let value = json["results"][0]["measurements"][0]["value"]
        .as_object_mut()
        .unwrap();
    value.insert("state".to_owned(), serde_json::json!("unknown"));
    value.insert("value".to_owned(), serde_json::json!("238"));
    assert!(EvidenceSet::parse(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn environments_with_one_label_difference_are_foreign() {
    let attachment = fixture_attachment();
    let eu = fixture_evidence(EVIDENCE_EU);
    let us = fixture_evidence(EVIDENCE_US);
    let accepted = &attachment.constraints()[1].environments()[0];
    assert!(accepted.compatible_with(eu.environment()));
    assert!(!accepted.compatible_with(us.environment()));
    assert_eq!(accepted.foreign_reason(us.environment()), "labels-mismatch");
}

#[test]
fn canonical_bytes_are_deterministic() {
    let attachment = fixture_attachment();
    let first = lekalo_core::nfr::attachment_canonical_bytes(&attachment).unwrap();
    let second = lekalo_core::nfr::attachment_canonical_bytes(&attachment).unwrap();
    assert_eq!(first, second);
    // Byte-sorted compact form: the raw fixture pretty bytes differ
    // from the canonical export, and the canonical export re-parses.
    let raw = fs::read_to_string(fixture_path(ATTACHMENT)).unwrap();
    assert_ne!(first, raw.replace(['\n', ' '], ""));
    NfrAttachment::parse(first.as_bytes()).expect("canonical bytes re-parse");
}

#[test]
fn resolution_never_writes_inside_the_project() {
    let probe = fixture_path("lekalo");
    let before = tree_bytes(&probe);
    validate_fixture();
    let after = tree_bytes(&probe);
    assert_eq!(before, after, "validation never writes");
}

fn tree_bytes(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// S7: the semantic diff - the closed classification over the fixture.
// ---------------------------------------------------------------------------

#[test]
fn the_diff_classifies_the_closed_paths() {
    use lekalo_core::nfr::diff::{self, DiffClass};

    let base_json = attachment_json();
    let base = NfrAttachment::parse(&serde_json::to_vec(&base_json).unwrap()).expect("parses");

    // Equal inputs: no paths.
    let same = diff::compare(&base, &base).expect("compares");
    assert!(same.equal());

    // A tightened bound is breaking.
    let mut json = base_json.clone();
    json["constraints"][1]["requirement"]["value"] = serde_json::json!("400");
    let candidate = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let result = diff::compare(&base, &candidate).expect("compares");
    assert!(!result.equal());
    let path = result
        .paths()
        .iter()
        .find(|path| path.path() == "constraints/planner.nfr.api-focus-p95/requirement")
        .expect("requirement path");
    assert_eq!(path.class(), DiffClass::Breaking);

    // mandatory -> advisory weakens; advisory -> mandatory strengthens.
    let mut json = base_json.clone();
    json["constraints"][1]["enforcement"] = serde_json::json!("advisory");
    let candidate = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let result = diff::compare(&base, &candidate).expect("compares");
    assert!(result.paths().iter().any(|path| path.path()
        == "constraints/planner.nfr.api-focus-p95/enforcement"
        && path.class() == DiffClass::Breaking));

    // A removed constraint is breaking, an added one non-breaking.
    let mut json = base_json.clone();
    json["constraints"].as_array_mut().unwrap().remove(2);
    let candidate = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let result = diff::compare(&base, &candidate).expect("compares");
    assert!(result
        .paths()
        .iter()
        .any(|path| path.path() == "constraints/planner.nfr.focus-memory"
            && path.class() == DiffClass::Breaking));
    let mut json = base_json.clone();
    json["constraints"].as_array_mut().unwrap().push(serde_json::json!({
        "constraintId": "planner.nfr.new-bound",
        "dimension": "runtime",
        "kind": "timeout",
        "scope": {"kind": "operation", "ref": "planner.focus_task"},
        "requirement": {"metric": "timeout", "comparator": "lte", "value": "5", "unit": "seconds"},
        "enforcement": "advisory",
        "measurement": {"method": "benchmark", "gateRef": "perf.gates/timeout"},
        "validity": {"revision": "1.0.0"}
    }));
    let candidate = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let result = diff::compare(&base, &candidate).expect("compares");
    assert!(result
        .paths()
        .iter()
        .any(|path| path.path() == "constraints/planner.nfr.new-bound"
            && path.class() == DiffClass::NonBreaking));

    // Foreign projects and mixed revisions are the typed error set.
    let mut json = base_json.clone();
    json["projectId"] = serde_json::json!("other");
    let foreign = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    assert!(diff::compare(&base, &foreign).is_err());
    let mut json = base_json;
    json["attachmentRevision"] = serde_json::json!("2.0.0");
    let mixed = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let error = diff::compare(&base, &mixed).expect_err("mixed revision is invalid");
    assert!(
        error.reason_ids().contains(&"nfr.diff-invalid"),
        "the diff rule fires"
    );
}

// ---------------------------------------------------------------------------
// S8: the impact synthesis - a changed constraint reaches the accepted
// impact engine and surfaces the affected scenarios and gates.
// ---------------------------------------------------------------------------

#[test]
fn a_constraint_change_surfaces_its_scenarios_and_gates() {
    let base_json = attachment_json();
    let base = NfrAttachment::parse(&serde_json::to_vec(&base_json).unwrap()).expect("parses");
    // Tighten the operation-scoped resource limit: the constraint's
    // scope symbol is planner.focus_task, covered by the scenario.
    let mut json = base_json.clone();
    json["constraints"][2]["requirement"]["value"] = serde_json::json!("600");
    let candidate = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let changed = lekalo_core::nfr::impact::changed_input_set(
        &base,
        &candidate,
        "lekalo/nfr.attachment.json",
    )
    .expect("synthesizes")
    .expect("the change produces a handoff");
    assert_eq!(changed.entries().len(), 1);
    assert_eq!(changed.entries()[0].symbol_ids(), ["planner.focus_task"]);
    assert_eq!(
        changed.entries()[0].logical_path().0,
        "lekalo/nfr.attachment.json"
    );

    // Equal attachments synthesize no handoff: "nothing changed" is a
    // caller decision.
    assert!(lekalo_core::nfr::impact::changed_input_set(
        &base,
        &base,
        "lekalo/nfr.attachment.json"
    )
    .expect("synthesizes")
    .is_none());

    // Through the accepted impact engine: the radius reaches the
    // covering scenario and the gate rows fire.
    with_cwd(&workspace_root(), || {
        let model = lekalo_core::loader::normalize_model(&selection()).expect("fixture loads");
        let compilation = lekalo_core::ir::compile(&model).expect("fixture compiles");
        let graph = lekalo_core::graph::build(&compilation.project).expect("graph builds");
        let effects = lekalo_core::effects::build(&compilation.project).expect("effects build");
        let request = lekalo_core::impact::ImpactRequest::for_changed();
        let result = lekalo_core::impact::analyze(
            &compilation.project,
            &graph,
            &effects,
            &request,
            Some(&changed),
            None,
        )
        .expect("the analysis completes");
        assert!(result
            .scenarios()
            .items
            .iter()
            .any(|item| item.id.as_str() == "scenario:planner.focus_flow"));
        assert!(
            result
                .gates()
                .items
                .iter()
                .any(|gate| gate.gate_id == "impact.gate.semantic-validate"),
            "the semantic-validate gate fires"
        );
    });
}
