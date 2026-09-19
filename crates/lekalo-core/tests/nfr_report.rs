//! Issue #85 NFR report-engine tests: the per-status truth table
//! (satisfied, violated, unverified, stale, foreign-environment,
//! open-question, unsupported, conflict), the verdict composition
//! (mandatory versus advisory, default versus strict), the as-of
//! expiry determinism, and the report wire identity.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::loader::LoadSelection;
use lekalo_core::nfr::{
    CapabilitySnapshot, EvidenceSet, GateProfile, IsoDate, NfrAttachment, Resolution,
    ResolutionVerdict,
};

/// The committed planner fixture (shared with `nfr.rs`).
const FIXTURE: &str = "tests/fixtures/nfr/planner";
const ATTACHMENT: &str = "nfr.attachment.json";
const EVIDENCE_EU: &str = "nfr-evidence.staging-eu.json";
const EVIDENCE_US: &str = "nfr-evidence.staging-us.json";

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

fn fixture_attachment() -> NfrAttachment {
    let bytes = fs::read(fixture_path(ATTACHMENT)).expect("fixture attachment reads");
    NfrAttachment::parse(&bytes).expect("fixture attachment parses")
}

fn fixture_evidence(name: &str) -> EvidenceSet {
    let bytes = fs::read(fixture_path(name)).expect("fixture evidence reads");
    EvidenceSet::parse(&bytes).expect("fixture evidence parses")
}

fn fixture_evidence_value(name: &str) -> serde_json::Value {
    serde_json::from_slice(&fs::read(fixture_path(name)).expect("fixture reads")).unwrap()
}

fn parse_evidence(value: &serde_json::Value) -> EvidenceSet {
    EvidenceSet::parse(&serde_json::to_vec(value).unwrap()).expect("parses")
}

fn attachment_json() -> serde_json::Value {
    serde_json::from_slice(&fs::read(fixture_path(ATTACHMENT)).expect("fixture reads")).unwrap()
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

const AS_OF: &str = "2026-09-30";

fn resolve_with(
    attachment: &NfrAttachment,
    evidence: &[&EvidenceSet],
    capabilities: &CapabilitySnapshot,
) -> Resolution {
    with_cwd(&workspace_root(), || {
        lekalo_core::nfr::resolve(
            attachment,
            evidence,
            capabilities,
            &IsoDate::parse(AS_OF).unwrap(),
            &selection(),
        )
        .expect("fixture resolution completes")
    })
}

fn status_of_constraint<'a>(resolution: &'a Resolution, constraint_id: &str) -> &'a str {
    let report = &resolution.report;
    report
        .runtime
        .constraints
        .iter()
        .chain(report.ai_budget.constraints.iter())
        .find(|row| row.constraint_id == constraint_id)
        .unwrap_or_else(|| panic!("constraint {constraint_id} missing"))
        .status
}

#[test]
fn fixture_statuses_are_the_satisfied_truth_table() {
    let attachment = fixture_attachment();
    let eu = fixture_evidence(EVIDENCE_EU);
    let us = fixture_evidence(EVIDENCE_US);
    let resolution = resolve_with(&attachment, &[&eu, &us], &CapabilitySnapshot::unresolved());
    // All three measured constraints carry current compatible-env pass
    // evidence inside their bounds.
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
        "satisfied"
    );
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.focus-memory"),
        "satisfied"
    );
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.ai-monthly-cost"),
        "satisfied"
    );
    // The privacy reference carries no evidence at all: unverified,
    // never spelled satisfied (AC#2).
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.privacy-review"),
        "unverified"
    );
    // The foreign environment stays visible under its own key and
    // never satisfies anything (AC#3).
    assert_eq!(resolution.report.foreign_evidence.len(), 1);
    assert_eq!(
        resolution.report.foreign_evidence[0].constraint_id,
        "planner.nfr.api-focus-p95"
    );
    assert!(resolution.report.foreign_evidence[0]
        .env_key
        .starts_with("staging-us:"));
    assert_eq!(
        resolution.report.foreign_evidence[0].reason,
        "labels-mismatch"
    );
    // Dimensions stay disjoint: three runtime rows, one ai-budget row
    // (AC#7).
    assert_eq!(resolution.report.runtime.constraints.len(), 3);
    assert_eq!(resolution.report.ai_budget.constraints.len(), 1);
    // The default gate passes: every measured constraint is satisfied
    // and the unverified one is advisory.
    assert!(matches!(resolution.verdict, ResolutionVerdict::Pass));
    assert_eq!(resolution.report.verdict, "pass");
}

#[test]
fn a_violated_mandatory_constraint_denies_the_gate() {
    let attachment = fixture_attachment();
    let mut eu = fixture_evidence_value(EVIDENCE_EU);
    eu["results"][0]["measurements"][0]["value"]["value"] = serde_json::json!("300");
    let eu = parse_evidence(&eu);
    let resolution = resolve_with(&attachment, &[&eu], &CapabilitySnapshot::unresolved());
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
        "violated"
    );
    match &resolution.verdict {
        ResolutionVerdict::Denied(_) => { /* exit class 3 is the CLI mapping */ }
        ResolutionVerdict::Pass => panic!("violated mandatory must deny"),
    }
}

#[test]
fn stale_revisions_and_expiry_are_the_same_status() {
    let attachment = fixture_attachment();

    // A revision mismatch is stale.
    let mut old = fixture_evidence_value(EVIDENCE_EU);
    old["results"][0]["constraintRevision"] = serde_json::json!("1.1.0");
    let old = parse_evidence(&old);
    let resolution = resolve_with(&attachment, &[&old], &CapabilitySnapshot::unresolved());
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
        "stale"
    );

    // So is expiry before the as-of date.
    let mut expired = fixture_evidence_value(EVIDENCE_EU);
    expired["results"][0]["expiresOn"] = serde_json::json!("2026-09-01");
    let expired = parse_evidence(&expired);
    let resolution = resolve_with(&attachment, &[&expired], &CapabilitySnapshot::unresolved());
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
        "stale"
    );

    // The same evidence is current at an earlier as-of date: the
    // evaluation is deterministic in the injected date, never a clock.
    with_cwd(&workspace_root(), || {
        let resolution = lekalo_core::nfr::resolve(
            &attachment,
            &[&expired],
            &CapabilitySnapshot::unresolved(),
            &IsoDate::parse("2026-08-15").unwrap(),
            &selection(),
        )
        .expect("resolution completes");
        assert_eq!(
            status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
            "satisfied"
        );
    });
}

#[test]
fn contradictory_receipts_at_one_environment_are_a_conflict() {
    let attachment = fixture_attachment();
    let mut eu = fixture_evidence_value(EVIDENCE_EU);
    let mut contradicting = eu["results"][0].clone();
    contradicting["resultStatus"] = serde_json::json!("fail");
    contradicting["evidenceDigest"] = serde_json::json!(
        "sha256:8888888888888888888888888888888888888888888888888888888888888888"
    );
    contradicting["measurements"][0]["value"]["value"] = serde_json::json!("402");
    eu["results"].as_array_mut().unwrap().push(contradicting);
    let eu = parse_evidence(&eu);
    let resolution = resolve_with(&attachment, &[&eu], &CapabilitySnapshot::unresolved());
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
        "conflict"
    );
    assert!(matches!(resolution.verdict, ResolutionVerdict::Denied(_)));
}

#[test]
fn unmeasured_value_states_stay_unverified() {
    let attachment = fixture_attachment();
    for state in ["unknown", "unsupported", "withheld"] {
        let mut eu = fixture_evidence_value(EVIDENCE_EU);
        let value = eu["results"][0]["measurements"][0]["value"]
            .as_object_mut()
            .unwrap();
        value.insert("state".to_owned(), serde_json::json!(state));
        value.remove("value");
        let eu = parse_evidence(&eu);
        let resolution = resolve_with(&attachment, &[&eu], &CapabilitySnapshot::unresolved());
        assert_eq!(
            status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
            "unverified",
            "state {state} is unverified"
        );
        assert!(matches!(resolution.verdict, ResolutionVerdict::Denied(_)));
    }
}

#[test]
fn an_unsatisfied_capability_is_unsupported() {
    // Mutate the attachment so the latency constraint requires a
    // capability the (empty) resolved profile does not provide.
    let mut json = attachment_json();
    json["constraints"][1]["capabilities"] = serde_json::json!([
        {"id": "testing.load", "minimum": "full"}
    ]);
    let attachment = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let eu = fixture_evidence(EVIDENCE_EU);
    let resolution = resolve_with(
        &attachment,
        &[&eu],
        &CapabilitySnapshot::resolved(Vec::new()),
    );
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.api-focus-p95"),
        "unsupported"
    );
    assert!(matches!(resolution.verdict, ResolutionVerdict::Denied(_)));
    assert!(
        resolution
            .warnings()
            .iter()
            .any(|warning| warning.id() == "nfr.capability-unsatisfied"),
        "the capability warning fires"
    );
}

#[test]
fn declaration_constraints_are_open_questions_and_never_proof() {
    let mut json = attachment_json();
    // The advisory privacy reference becomes a declaration row, kept
    // alone so no mandatory row dilutes the verdict.
    json["constraints"] = serde_json::json!([json["constraints"][3]]);
    json["constraints"][0]["measurement"]["method"] = serde_json::json!("declaration");
    let attachment = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let resolution = resolve_with(&attachment, &[], &CapabilitySnapshot::unresolved());
    assert_eq!(
        status_of_constraint(&resolution, "planner.nfr.privacy-review"),
        "open-question"
    );
    // Default and strict: an open question is by design, never a
    // failure, so it never blocks.
    assert!(matches!(resolution.verdict, ResolutionVerdict::Pass));
    assert!(matches!(
        resolution.verdict_for(GateProfile::Strict),
        ResolutionVerdict::Pass
    ));
}

#[test]
fn strict_escalates_advisory_failures() {
    // Keep only the advisory privacy reference (audit-document method,
    // no evidence): advisory unverified.
    let mut json = attachment_json();
    json["constraints"] = serde_json::json!([json["constraints"][3]]);
    let attachment = NfrAttachment::parse(&serde_json::to_vec(&json).unwrap()).expect("parses");
    let resolution = resolve_with(&attachment, &[], &CapabilitySnapshot::unresolved());
    // Default: the unverified advisory row is visible and never blocks.
    assert!(matches!(resolution.verdict, ResolutionVerdict::Pass));
    // Strict: advisory unverified joins the denied set (AC#5).
    match resolution.verdict_for(GateProfile::Strict) {
        ResolutionVerdict::Denied(_) => {}
        ResolutionVerdict::Pass => panic!("strict must escalate advisory unverified"),
    }
}

#[test]
fn the_report_canonical_bytes_carry_the_wire_identity() {
    let attachment = fixture_attachment();
    let eu = fixture_evidence(EVIDENCE_EU);
    let resolution = resolve_with(&attachment, &[&eu], &CapabilitySnapshot::unresolved());
    let bytes = resolution
        .report
        .canonical_bytes()
        .expect("canonical bytes");
    let json: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(json["schemaVersion"], "lekalo/nfr-report/v0.4.0");
    assert_eq!(json["identity"], "dev.lekalo.nfr-report@0.4.0");
    assert_eq!(json["asOf"], AS_OF);
    assert_eq!(json["capabilitiesResolved"], false);
    // Deterministic: two resolutions agree byte for byte.
    let again = resolve_with(&attachment, &[&eu], &CapabilitySnapshot::unresolved());
    assert_eq!(bytes, again.report.canonical_bytes().unwrap());
}
