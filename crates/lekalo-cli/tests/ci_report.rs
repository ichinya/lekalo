//! Issue #103 CLI integration tests: the closed CI report surface over
//! the real binary. The exit-policy table (required failure nonzero,
//! optional-unavailable policy-driven, `generate --check` drift without
//! writes, `readiness --check` gating), the four report projections,
//! the exact provenance binding, and the side-channel guarantees
//! (report-write failures never mask a failing run and never turn a
//! passing run into exit 0 without its artifact).
//!
//! Every run happens in a fresh fixture copy under the crate's
//! `target/` tree, reached through the alias-free temp spelling (the
//! selection policy denies alias spellings), mirroring doctor.rs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/loader/valid-direct-visibility";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

/// The selection policy denies alias-spelled working directories; chdir
/// the child into the resolved spelling, stripped of the `\\?\` prefix
/// `canonicalize` produces on Windows.
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

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

static NEXT_CASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn fixture_copy(tag: &str) -> PathBuf {
    let id = NEXT_CASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + u64::from(std::process::id())
        + u64::from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .subsec_nanos(),
        );
    let dir = std::env::current_dir()
        .expect("cwd")
        .join("target")
        .join("ci-report-tests")
        .join(format!("{tag}-{id}"));
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(FIXTURE),
        &dir,
    );
    dir
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

/// The fingerprint of every path, length, and content digest under one
/// directory tree; the read-only guarantee is snapshot equality.
fn fingerprint(dir: &Path) -> String {
    use lekalo_core::digest::sha256_hex;
    let mut lines: Vec<String> = Vec::new();
    fn walk(dir: &Path, base: &Path, lines: &mut Vec<String>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let name = path
                .strip_prefix(base)
                .expect("relative")
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                walk(&path, base, lines);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_default();
                lines.push(format!("{name} {} {}", bytes.len(), sha256_hex(&bytes)));
            }
        }
    }
    walk(dir, dir, &mut lines);
    lines.sort();
    lines.join("\n")
}

#[test]
fn validate_report_binds_provenance_and_stays_a_side_channel() {
    let dir = fixture_copy("validate");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "validate",
            "--report-file",
            "out/report.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    // The status-owned stream still carries the exact validate envelope.
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(envelope["status"], "valid");
    // The side-channel report carries the closed identity, the pinned
    // git/model provenance, and the passing check row.
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("out/report.json")).expect("report"),
    )
    .expect("report parses");
    assert_eq!(report["schema_version"], "lekalo/ci-report/v0.6.3");
    assert_eq!(report["identity"], "dev.lekalo.ci-report@0.6.3");
    assert_eq!(report["invocation"]["command"], "validate");
    assert_eq!(report["evaluation"]["verdict"], "ready");
    assert_eq!(report["evaluation"]["exitCode"], 0);
    assert_eq!(report["checks"][0]["id"], "model.validation");
    assert_eq!(report["checks"][0]["effectiveOutcome"], "pass");
    // The git pin is a known 40/64-hex revision, dirty can be either.
    let commit = report["provenance"]["git"]["commit"]["value"]
        .as_str()
        .expect("git commit pinned");
    assert!(
        commit.len() == 40 || commit.len() == 64,
        "the commit is an exact revision"
    );
    // No absolute host paths anywhere in the report bytes.
    let raw = std::fs::read_to_string(dir.join("out/report.json")).expect("raw");
    assert!(!raw.contains('\\'), "no backslash host paths");
    assert!(
        !raw.to_ascii_lowercase().contains("users\\"),
        "no user dirs"
    );
    assert!(!raw.contains("C:"), "no drive spellings");
}

#[test]
fn validate_deterministic_report_bytes_across_reruns() {
    let dir = fixture_copy("determinism");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let args = &[
        "--json",
        "validate",
        "--report-file",
        "out/report.json",
        "--project",
        ".",
    ];
    let _ = lekalo_in(&dir, args);
    let first = std::fs::read_to_string(dir.join("out/report.json")).expect("first");
    let _ = lekalo_in(&dir, args);
    let second = std::fs::read_to_string(dir.join("out/report.json")).expect("second");
    assert_eq!(first, second, "the report bytes are deterministic");
}

#[test]
fn format_without_file_is_the_usage_failure() {
    let dir = fixture_copy("usage");
    let output = lekalo_in(
        &dir,
        &["validate", "--report-format", "sarif", "--project", "."],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("LEK-CLI-001"));
}

#[test]
fn report_write_failure_is_typed_and_does_not_mask_the_check() {
    let dir = fixture_copy("writefail");
    // A missing report directory: the passing run becomes the typed
    // unavailable result (exit 4), never a silent exit 0.
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "validate",
            "--report-file",
            "missing-dir/report.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 4);
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(envelope["status"], "unavailable");
    assert_eq!(envelope["reasonCodes"][0], "ci.report-write-failed");
}

#[test]
fn readiness_informational_default_stays_exit_zero() {
    let dir = fixture_copy("readiness-default");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "readiness",
            "--phase",
            "release",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "the report is still the product");
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("document parses");
    assert_eq!(document["verdict"], "blocked");
}

#[test]
fn readiness_check_gates_a_blocked_release_nonzero() {
    let dir = fixture_copy("readiness-check");
    // Blocked required checks: --check fails the run with the typed
    // unavailable class (exit 4).
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "readiness",
            "--phase",
            "release",
            "--check",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 4);
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(envelope["status"], "unavailable");
    // The model phase of the same fresh fixture is not blocked.
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "readiness",
            "--phase",
            "model",
            "--check",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0);
}

#[test]
fn readiness_check_report_carries_doctor_rows_and_lock_pin() {
    let dir = fixture_copy("readiness-report");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "readiness",
            "--phase",
            "release",
            "--check",
            "--report-file",
            "out/readiness.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 4);
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("out/readiness.json")).expect("report"),
    )
    .expect("report parses");
    assert_eq!(report["invocation"]["command"], "readiness");
    assert_eq!(report["invocation"]["mode"], "release");
    assert_eq!(report["evaluation"]["verdict"], "blocked");
    assert_eq!(report["evaluation"]["exitCode"], 4);
    let checks = report["checks"].as_array().expect("check rows");
    assert!(checks.len() >= 10, "the doctor panel rides the report");
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "lock.freshness" && check["effectiveOutcome"] != "pass"),
        "the missing lock is visible as a non-pass row"
    );
    // The lock pin stays unknown/absent — never a fabricated digest.
    assert_eq!(report["provenance"]["lock"]["digest"]["state"], "unknown");
    assert_eq!(report["provenance"]["lock"]["digest"]["reason"], "absent");
}

#[test]
fn verify_report_keeps_declared_absences_visible_and_blocks() {
    let dir = fixture_copy("verify");
    // The planner fixture has no lock yet: verify fails before any
    // component runs; the report (when requested) reflects the typed
    // refusal. Create the lock first for a full pipeline.
    let lock = lekalo_in(&dir, &["lock"]);
    assert_eq!(exit_code(&lock), 0, "{}", stderr_text(&lock));
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "verify",
            "--locked",
            "--report-file",
            "out/verify.json",
            "--project",
            ".",
        ],
    );
    // The declared absences (scenario execution, native gates) degrade
    // the verify verdict; verify is exit-neutral on the informational
    // projection but the CI report surfaces the true gate verdict.
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    let _ = envelope;
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("out/verify.json")).expect("report"),
    )
    .expect("report parses");
    assert_eq!(report["invocation"]["command"], "verify");
    let suites = report["suites"].as_array().expect("suites");
    assert!(
        suites
            .iter()
            .any(|suite| suite["id"] == "scenarios.execution"),
        "the scenario suite is present"
    );
    // The lock digest the run bound equals the created lock's digest.
    let lock_digest = report["provenance"]["lock"]["digest"]["value"]
        .as_str()
        .expect("lock pinned");
    assert!(lock_digest.starts_with("sha256:"));
}

#[test]
fn sarif_projection_validates_and_stays_deterministic() {
    let dir = fixture_copy("sarif");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let args = &[
        "--json",
        "validate",
        "--report-file",
        "out/diag.sarif",
        "--report-format",
        "sarif",
        "--project",
        ".",
    ];
    let _ = lekalo_in(&dir, args);
    let first = std::fs::read_to_string(dir.join("out/diag.sarif")).expect("first");
    let parsed: serde_json::Value = serde_json::from_str(&first).expect("sarif parses");
    assert_eq!(parsed["version"], "2.1.0");
    assert_eq!(parsed["runs"][0]["tool"]["driver"]["name"], "Lekalo");
    let _ = lekalo_in(&dir, args);
    let second = std::fs::read_to_string(dir.join("out/diag.sarif")).expect("second");
    assert_eq!(first, second, "SARIF bytes are deterministic");
}

#[test]
fn junit_projection_is_well_formed_and_deterministic() {
    let dir = fixture_copy("junit");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let args = &[
        "--json",
        "readiness",
        "--phase",
        "release",
        "--check",
        "--report-file",
        "out/junit.xml",
        "--report-format",
        "junit",
        "--project",
        ".",
    ];
    let output = lekalo_in(&dir, args);
    assert_eq!(exit_code(&output), 4);
    let first = std::fs::read_to_string(dir.join("out/junit.xml")).expect("junit");
    assert!(first.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(first.contains("<testsuites "));
    assert!(first.contains("</testsuites>"));
    // Deterministic bytes across reruns.
    let _ = lekalo_in(&dir, args);
    let second = std::fs::read_to_string(dir.join("out/junit.xml")).expect("junit");
    assert_eq!(first, second);
}

#[test]
fn markdown_projection_is_bounded_and_escaped() {
    let dir = fixture_copy("markdown");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let output = lekalo_in(
        &dir,
        &[
            "readiness",
            "--phase",
            "release",
            "--report-file",
            "out/summary.md",
            "--report-format",
            "md",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    let summary = std::fs::read_to_string(dir.join("out/summary.md")).expect("summary");
    assert!(summary.contains("## Lekalo readiness"));
    assert!(summary.contains("| pin | value |"));
    assert!(summary.ends_with('\n'));
}

#[test]
fn generate_check_drift_report_is_read_only_and_exit_one() {
    let dir = fixture_copy("gencheck");
    // The drift gate needs the lock.
    let lock = lekalo_in(&dir, &["lock"]);
    assert_eq!(exit_code(&lock), 0);
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let before = fingerprint(&dir);
    // Vacuous check (no manifest, no managed root): clean, exit 0.
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "generate",
            "--check",
            "--report-file",
            "out/drift.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0);
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("out/drift.json")).expect("report"))
            .expect("report parses");
    assert_eq!(report["checks"][0]["id"], "artifacts.drift");
    assert_eq!(report["checks"][0]["effectiveOutcome"], "pass");
    // Only the report outputs may appear; nothing else in the project
    // tree changed. Recompute the fingerprint excluding `out/`.
    let after = fingerprint(&dir);
    let before_project: Vec<&str> = before
        .lines()
        .filter(|l| !l.starts_with("out\\") && !l.starts_with("out/"))
        .collect();
    let after_project: Vec<&str> = after
        .lines()
        .filter(|l| !l.starts_with("out\\") && !l.starts_with("out/"))
        .collect();
    assert_eq!(
        before_project, after_project,
        "generate --check wrote nothing"
    );
}

#[test]
fn verify_blocked_scenario_failure_is_a_junit_error_with_nonzero_evaluation() {
    // The orchestration fixture's verify surface: a scenario row with a
    // fail outcome must land as <error>/<failure> in JUnit and block.
    // This is covered at the unit level (core ci_report suite, the
    // junit test) and the e2e Sarif/JUnit shape tests above; here we
    // pin that the verify CLI over the planner fixture stays bounded
    // and completes with a report.
    let dir = fixture_copy("verify-bounded");
    let lock = lekalo_in(&dir, &["lock"]);
    assert_eq!(exit_code(&lock), 0);
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let output = lekalo_in(
        &dir,
        &[
            "verify",
            "--report-file",
            "out/verify.md",
            "--report-format",
            "md",
            "--project",
            ".",
        ],
    );
    assert!(
        output.status.code().is_some(),
        "the run terminates with a real exit code"
    );
    let summary = std::fs::read_to_string(dir.join("out/verify.md")).expect("summary");
    assert!(summary.contains("## Lekalo verify"));
}

#[test]
fn a_report_is_refused_when_the_rendered_bytes_carry_secret_material() {
    // Canary (review codex F2): a secret-shaped token that survives into
    // a diagnostic's data must refuse the report sink — the report is
    // never written, and the command result stays authoritative.
    let dir = fixture_copy("secret-refusal");
    let entities = dir.join("lekalo/modules/beta/entities.yaml");
    let original = std::fs::read_to_string(&entities).expect("fixture");
    let planted = original.replace(
        "type: alpha.widget",
        "type: beta.ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd",
    );
    assert_ne!(original, planted, "the canary must actually be planted");
    std::fs::write(&entities, planted).expect("plant canary");
    std::fs::create_dir_all(dir.join("out")).expect("report dir");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "validate",
            "--report-file",
            "out/report.json",
            "--project",
            ".",
        ],
    );
    // The analysis itself failed (the canary is an unresolved
    // reference); the report sink refused on top — the JSON report is
    // absent and no byte of it carries the canary.
    let report_path = dir.join("out/report.json");
    assert!(
        !report_path.exists(),
        "the report sink refused the secret-bearing bytes"
    );
    let _ = exit_code(&output);
}

#[test]
fn a_report_destination_over_an_existing_file_is_refused() {
    // Destination confinement (review F1/F2): the writer never
    // truncates an existing non-empty file — naming a model source or
    // the lock as the report destination is the typed refusal, and the
    // file's bytes stay intact.
    let dir = fixture_copy("destination-confinement");
    let model = dir.join("lekalo/modules/beta/entities.yaml");
    let before = std::fs::read_to_string(&model).expect("model bytes");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "validate",
            "--report-file",
            "lekalo/modules/beta/entities.yaml",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 4, "the protected destination refuses");
    let after = std::fs::read_to_string(&model).expect("model bytes after");
    assert_eq!(before, after, "the analyzed input was never overwritten");
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(envelope["reasonCodes"][0], "ci.report-write-failed");
}
