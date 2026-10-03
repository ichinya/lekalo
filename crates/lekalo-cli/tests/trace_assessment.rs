//! Real process-boundary and custody tests for the neutral bridge consumer.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/trace-assessment")
        .join(name)
}

fn run(dir: &Path, input: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(["--json", "trace", "assess"])
        .arg(fixture("trace.json"))
        .arg("--evidence")
        .arg(fixture(input))
        .current_dir(dir)
        // A caller's current runtime cannot satisfy or falsify an injected
        // receipt. No HLV or native executable is available through PATH.
        .env("PATH", "")
        .output()
        .expect("real Lekalo process")
}

fn receipt(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("assessment receipt")
}

#[test]
fn required_hlv_failure_keeps_native_and_openspec_receipts_on_stdout() {
    let output = run(&fixture("greenfield"), "input/hlv-fail.json");
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
    let result = receipt(&output);
    assert_eq!(result["status"], "denied");
    let assessment = &result["payload"]["assessment"];
    let evidence = assessment["evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 3);
    assert!(evidence
        .iter()
        .any(|e| e["provider"] == "openspec" && e["outcome"] == "pass"));
    assert!(evidence
        .iter()
        .any(|e| e["provider"] == "source-native" && e["outcome"] == "pass"));
    let hlv = evidence.iter().find(|e| e["provider"] == "hlv").unwrap();
    assert_eq!(hlv["diagnostics"][0]["code"], "CTR-030");
    assert_eq!(hlv["diagnostics"][1]["code"], "GATE-005");
    assert_eq!(assessment["chains"][0]["execution"], "passed");
}

#[test]
fn optional_unavailable_is_degraded_and_uses_registered_diagnostics() {
    let output = run(&fixture("adopt"), "input/optional-unavailable.json");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let result = receipt(&output);
    assert_eq!(result["assessment"]["verdict"], "degraded");
    let evidence = result["assessment"]["evidence"].as_array().unwrap();
    assert!(evidence
        .iter()
        .any(|e| e["provider"] == "hlv" && e["outcome"] == "unavailable"));
    assert_eq!(
        result["assessment"]["diagnostics"][0]["code"],
        "LEK-TRACE-005"
    );
}

#[test]
fn lekalo_assessment_works_without_any_hlv_executable_or_receipt() {
    let output = run(&fixture("greenfield"), "input/without-hlv.json");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let result = receipt(&output);
    assert_eq!(result["assessment"]["verdict"], "ready");
    assert!(result["assessment"]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["provider"] != "hlv"));
}

#[test]
fn failure_then_success_in_the_same_layout_does_not_sync_or_cache_state() {
    for (layout, sentinel) in [
        ("greenfield", "contracts/sentinel.yaml"),
        ("adopt", ".hlv/contracts/sentinel.yaml"),
    ] {
        let dir = fixture(layout);
        let sentinel = dir.join(sentinel);
        let before = std::fs::read(&sentinel).unwrap();
        let evidence = std::fs::read(dir.join("evidence.json")).unwrap();
        let failing = run(&dir, "input/missing-mapping.json");
        assert_eq!(failing.status.code(), Some(3));
        let ready = run(&dir, "input/ready.json");
        assert_eq!(ready.status.code(), Some(0));
        assert_eq!(receipt(&ready)["assessment"]["verdict"], "ready");
        assert_eq!(std::fs::read(&sentinel).unwrap(), before);
        assert_eq!(std::fs::read(dir.join("evidence.json")).unwrap(), evidence);
        assert!(
            !dir.join(".lekalo").exists(),
            "read-only command created runtime custody"
        );
    }
}

#[test]
fn unavailable_input_is_a_bounded_io_refusal_on_stderr() {
    let output = run(&fixture("greenfield"), "input/absent-secret-file.json");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    let result: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(result["reasonCodes"][0], "loader.io");
    assert!(!text.contains("absent-secret-file"));
    assert!(!text.contains(&fixture("").to_string_lossy().to_string()));
}
