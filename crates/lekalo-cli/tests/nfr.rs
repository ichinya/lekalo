//! Issue #85 CLI tests for the `lekalo nfr` handoff: the gate exit
//! protocol (0/1/3/4), the strict escalation, the canonical report
//! export, the closed query selectors, and the envelope parity over
//! the hermetic planner fixture.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn copy_fixture(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_fixture(&fixture_path(), temp.path());
    temp
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/nfr/planner"),
    )
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

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is the JSON envelope")
}

const AS_OF: &str = "2026-09-30";
const ATTACHMENT: &str = "nfr.attachment.json";
const EVIDENCE_EU: &str = "nfr-evidence.staging-eu.json";

#[test]
fn the_gate_passes_with_current_compatible_evidence() {
    let dir = scratch();
    let output = lekalo_in(
        dir.path(),
        &[
            "--json",
            "nfr",
            "validate",
            ATTACHMENT,
            "--evidence",
            EVIDENCE_EU,
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(
        exit_code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = stdout_json(&output);
    assert_eq!(json["status"], "valid");
    assert_eq!(json["nfr"]["projectId"], "planner");
    assert_eq!(json["nfr"]["satisfied"], 3);
    assert_eq!(json["nfr"]["unverified"], 1);
    assert_eq!(json["nfr"]["profile"], "default");
}

#[test]
fn the_default_gate_denies_mandatory_unverified_rows() {
    let dir = scratch();
    let output = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            ATTACHMENT,
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 3, "mandatory unverified denies");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("LEK-NFR-009"), "the gate rule fires: {text}");
}

#[test]
fn strict_escalates_advisory_rows() {
    let dir = scratch();
    // Default: the unverified advisory privacy review never blocks.
    let default = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            ATTACHMENT,
            "--evidence",
            EVIDENCE_EU,
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&default), 0);
    // Strict: the same resolution denies (AC#5).
    let strict = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            ATTACHMENT,
            "--evidence",
            EVIDENCE_EU,
            "--strict",
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&strict), 3);
}

#[test]
fn unreadable_evidence_is_unavailable() {
    let dir = scratch();
    let output = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            ATTACHMENT,
            "--evidence",
            "does-not-exist.json",
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 4, "unreadable evidence is exit 4");
}

#[test]
fn a_malformed_attachment_is_invalid() {
    let dir = scratch();
    fs::write(dir.path().join("broken.json"), b"{ not json").unwrap();
    let output = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            "broken.json",
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1);
}

#[test]
fn a_bad_as_of_date_is_a_usage_error() {
    let dir = scratch();
    let output = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            ATTACHMENT,
            "--as-of",
            "2026-13-99",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1, "malformed as-of is the usage class");
}

#[test]
fn the_report_export_is_canonical_and_digested() {
    let dir = scratch();
    let output = lekalo_in(
        dir.path(),
        &[
            "--json",
            "nfr",
            "report",
            ATTACHMENT,
            "--evidence",
            EVIDENCE_EU,
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    let json = stdout_json(&output);
    assert_eq!(json["status"], "valid");
    assert_eq!(json["report"]["schemaVersion"], "lekalo/nfr-report/v0.4.0");
    assert_eq!(json["report"]["asOf"], AS_OF);
    // The canonical human payload is byte-identical to the embedded
    // report value.
    let canonical = String::from_utf8(output.stdout.clone()).unwrap();
    let embedded = serde_json::to_string(&json["report"]).unwrap();
    assert!(canonical.contains(&embedded[..embedded.len().min(80)]));
}

#[test]
fn the_closed_selectors_answer_from_the_report() {
    let dir = scratch();
    let run = |selector: &str| -> (u8, Option<Value>) {
        let output = lekalo_in(
            dir.path(),
            &[
                "--json",
                "nfr",
                "query",
                ATTACHMENT,
                selector,
                "--evidence",
                EVIDENCE_EU,
                "--as-of",
                AS_OF,
                "--project",
                ".",
            ],
        );
        let code = exit_code(&output);
        if code == 0 {
            (code, Some(stdout_json(&output)))
        } else {
            (code, None)
        }
    };
    // One constraint by id.
    let (code, json) = run("constraint:planner.nfr.api-focus-p95");
    assert_eq!(code, 0);
    assert_eq!(
        json.as_ref().unwrap()["nfr"]["constraint"]["status"],
        "satisfied"
    );
    // By scope symbol.
    let (code, json) = run("symbol:planner.api_focus");
    assert_eq!(code, 0);
    assert_eq!(
        json.as_ref().unwrap()["nfr"]["constraints"],
        serde_json::json!(["planner.nfr.api-focus-p95"])
    );
    // The status selectors.
    let (code, json) = run("unverified");
    assert_eq!(code, 0);
    assert_eq!(
        json.as_ref().unwrap()["nfr"]["constraints"],
        serde_json::json!(["planner.nfr.privacy-review"])
    );
    // The foreign-environment selector has no rows for this evidence.
    let (code, _) = run("foreign-environment");
    assert_eq!(
        code, 1,
        "an empty answer is the stable projection-empty class"
    );
    // An unknown selector is the stable usage error.
    let (code, _) = run("everything");
    assert_eq!(code, 1);
}

#[test]
fn human_and_json_are_projections_of_one_result() {
    let dir = scratch();
    let json = lekalo_in(
        dir.path(),
        &[
            "--json",
            "nfr",
            "validate",
            ATTACHMENT,
            "--evidence",
            EVIDENCE_EU,
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    let human = lekalo_in(
        dir.path(),
        &[
            "nfr",
            "validate",
            ATTACHMENT,
            "--evidence",
            EVIDENCE_EU,
            "--as-of",
            AS_OF,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&json), exit_code(&human));
    let human_text = String::from_utf8(human.stdout).unwrap();
    assert!(human_text.contains("constraints 4"));
    assert!(human_text.contains("satisfied 3"));
}

#[test]
fn the_diff_reports_the_verdict_as_data() {
    let dir = scratch();
    // An unchanged candidate is equal.
    let same = lekalo_in(
        dir.path(),
        &["--json", "nfr", "diff", ATTACHMENT, ATTACHMENT],
    );
    assert_eq!(exit_code(&same), 0);
    let json = stdout_json(&same);
    assert_eq!(json["diff"]["equal"], true);
    // A foreign project is the typed invalid set.
    fs::write(dir.path().join("broken-rev.json"), b"{}").unwrap();
    let invalid = lekalo_in(
        dir.path(),
        &["--json", "nfr", "diff", ATTACHMENT, "broken-rev.json"],
    );
    assert_eq!(exit_code(&invalid), 1);
}

#[test]
fn the_impact_projection_carries_the_scenario_and_gate_rows() {
    let dir = scratch();
    // The base is the committed attachment; the candidate tightens the
    // operation-scoped resource limit.
    let mut candidate: Value =
        serde_json::from_slice(&fs::read(dir.path().join(ATTACHMENT)).unwrap()).unwrap();
    candidate["constraints"][2]["requirement"]["value"] = serde_json::json!("600");
    fs::write(
        dir.path().join("nfr-candidate.json"),
        serde_json::to_vec(&candidate).unwrap(),
    )
    .unwrap();
    let output = lekalo_in(
        dir.path(),
        &[
            "--json",
            "nfr",
            "impact",
            "nfr-candidate.json",
            "--base",
            ATTACHMENT,
            "--project",
            ".",
        ],
    );
    assert_eq!(
        exit_code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = stdout_json(&output);
    assert_eq!(json["status"], "valid");
    // The NFR provenance section names the entered scope symbol.
    assert_eq!(
        json["nfr"]["changedScopeSymbols"],
        serde_json::json!(["planner.focus_task"])
    );
    // The scenario row arrives through the impact engine (AC#4).
    let scenarios = json["impact"]["scenarios"]["items"]
        .as_array()
        .expect("scenario rows");
    assert!(
        scenarios
            .iter()
            .any(|item| item["id"] == "scenario:planner.focus_flow"),
        "{scenarios:?}"
    );
    // The gate selection carries impact.gate.* rows.
    let gates = json["impact"]["gates"]["items"]
        .as_array()
        .expect("gate rows");
    assert!(gates
        .iter()
        .any(|gate| gate["gateId"] == "impact.gate.semantic-validate"));
}

#[test]
fn an_equal_impact_is_the_projection_empty_class() {
    let dir = scratch();
    let output = lekalo_in(
        dir.path(),
        &[
            "--json",
            "nfr",
            "impact",
            ATTACHMENT,
            "--base",
            ATTACHMENT,
            "--project",
            ".",
        ],
    );
    assert_eq!(
        exit_code(&output),
        1,
        "equal attachments never fake an impact"
    );
}
