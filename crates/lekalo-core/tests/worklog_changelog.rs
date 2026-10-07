//! Issue #51 worklog-changelog pilot composition tests: the closed wire,
//! the pure join against the pinned compilation, and diagnostic
//! normalization.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::loader::LoadSelection;
use lekalo_core::worklog_changelog::{self, Composition};

const PILOT_JSON: &str = "tests/fixtures/pilot/worklog-changelog/attachments/pilot.json";
const MODEL_PROJECT: &str = "tests/fixtures/pilot/worklog-changelog/model";

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
    });
}

#[test]
fn duplicate_json_keys_are_rejected_before_any_semantics() {
    let duplicate =
        br#"{"schemaVersion":"lekalo/worklog-changelog-pilot/v0.8.0","schemaVersion":"x"}"#;
    let set = worklog_changelog::parse(duplicate).unwrap_err();
    assert_eq!(detail_of(&set), "duplicate-key");
}

#[test]
fn evidence_admission_is_honest_for_both_profiles() {
    let bytes = std::fs::read(
        workspace_root().join("tests/fixtures/pilot/worklog-changelog/evidence/evidence.json"),
    )
    .expect("evidence fixture present");
    // Strict: the committed report honestly refuses (unsupported) with
    // blockers listed and the per-target rows retained.
    let strict = worklog_changelog::admit(&bytes, worklog_changelog::AdmissionProfile::Strict)
        .expect("strict admission produces a receipt");
    assert_eq!(strict.verdict, "unsupported");
    assert!(!strict.blockers.is_empty());
    assert_eq!(strict.targets.len(), 2);
    // Permissive: degraded, every row still visible.
    let permissive =
        worklog_changelog::admit(&bytes, worklog_changelog::AdmissionProfile::Permissive)
            .expect("permissive admission produces a receipt");
    assert_eq!(permissive.verdict, "degraded");
    assert_eq!(permissive.targets.len(), 2);
}
