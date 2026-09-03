//! Issue #8 IR conformance: fixture-driven `lekalo load --ir` runs, golden
//! canonical bytes, JSON/YAML twin equality, sorted occurrence-safe source
//! maps, typed failure envelopes, and byte-identical reruns.
//!
//! Selectors are invocation-relative, so every run sets an explicit working
//! directory (the workspace root) and passes `--project` as a relative path.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE_ROOT: &str = "tests/fixtures/ir";
const FULL_KINDS: &str = "valid-full-kinds";
const JSON_TWIN: &str = "valid-json-twin";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate lives under workspace/crates")
        .to_path_buf()
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_lekalo")
}

fn run_load(selector: &str, extra_args: &[&str]) -> Output {
    let mut command = Command::new(binary());
    command.arg("load").arg("--project").arg(selector);
    for argument in extra_args {
        command.arg(argument);
    }
    command
        .current_dir(workspace_root())
        .env_remove("LEKALO_PROJECT");
    command.output().expect("run lekalo load")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn fixture_names() -> Vec<String> {
    let root = workspace_root().join(FIXTURE_ROOT);
    let mut names: Vec<String> = std::fs::read_dir(&root)
        .expect("fixture root exists")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| root.join(name).join("expect.json").exists())
        .collect();
    names.sort();
    names
}

#[test]
fn every_ir_fixture_matches_its_expectation() {
    assert!(
        fixture_names().len() >= 10,
        "fixture corpus is present: {:?}",
        fixture_names()
    );
    for name in fixture_names() {
        let expect: Value = serde_json::from_str(
            &std::fs::read_to_string(
                workspace_root()
                    .join(FIXTURE_ROOT)
                    .join(&name)
                    .join("expect.json"),
            )
            .unwrap_or_else(|error| panic!("{name}: read expect.json: {error}")),
        )
        .unwrap_or_else(|error| panic!("{name}: expect.json parses: {error}"));

        let output = run_load(&format!("{FIXTURE_ROOT}/{name}"), &["--ir", "--json"]);
        let expected_status = expect["status"].as_str().expect("status string");
        let expected_exit = match expected_status {
            "valid" => 0,
            "invalid" => 1,
            other => panic!("{name}: unknown expected status {other}"),
        };
        assert_eq!(output.status.code(), Some(expected_exit), "{name}");

        let rendered = match expected_status {
            "valid" => stdout_text(&output),
            _ => stderr_text(&output),
        };
        let parsed: Value = serde_json::from_str(rendered.trim())
            .unwrap_or_else(|error| panic!("{name}: envelope parses: {error}"));
        assert_eq!(
            parsed["status"].as_str(),
            Some(expected_status),
            "{name}: status"
        );

        if let Some(codes) = expect["reasonCodes"].as_array() {
            let actual: Vec<String> = parsed["reasonCodes"]
                .as_array()
                .expect("reasonCodes array")
                .iter()
                .map(|code| code["code"].as_str().expect("code string").to_owned())
                .collect();
            let mut expected_codes: Vec<String> = codes
                .iter()
                .map(|code| code.as_str().expect("code string").to_owned())
                .collect();
            expected_codes.sort();
            let mut actual_sorted = actual.clone();
            actual_sorted.sort();
            assert_eq!(actual_sorted, expected_codes, "{name}: reason codes");
            assert!(!actual.is_empty(), "{name}: codes are present");
        }

        eprintln!("checked fixture: {name}");
    }
}

#[test]
fn canonical_ir_bytes_match_the_frozen_golden() {
    let golden = std::fs::read_to_string(
        workspace_root()
            .join(FIXTURE_ROOT)
            .join(FULL_KINDS)
            .join("ir.golden.json"),
    )
    .expect("read ir.golden.json");
    let output = run_load(&format!("{FIXTURE_ROOT}/{FULL_KINDS}"), &["--ir", "--json"]);
    assert_exit(&output, 0);
    let envelope: Value = serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    assert!(envelope.get("model").is_none(), "--ir replaces the model");
    assert_eq!(
        envelope["ir"]["contract"].as_str(),
        Some("dev.lekalo.ir@0.1.0"),
        "contract identity is bound"
    );
    assert_eq!(
        envelope["ir"]["modelVersion"].as_str(),
        Some("1.0.0"),
        "source model version is preserved"
    );
    // The `ir` value is the canonical IR object serialized with no
    // whitespace; re-serializing the parsed value must reproduce the golden
    // bytes exactly.
    let ir_text = serde_json::to_string(&envelope["ir"]).expect("ir serializes");
    assert_eq!(ir_text, golden.trim(), "canonical IR bytes are golden");
}

#[test]
fn json_twin_produces_identical_canonical_ir_bytes() {
    let golden = std::fs::read_to_string(
        workspace_root()
            .join(FIXTURE_ROOT)
            .join(FULL_KINDS)
            .join("ir.golden.json"),
    )
    .expect("read ir.golden.json");
    let output = run_load(&format!("{FIXTURE_ROOT}/{JSON_TWIN}"), &["--ir", "--json"]);
    assert_exit(&output, 0);
    let envelope: Value = serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    let ir_text = serde_json::to_string(&envelope["ir"]).expect("ir serializes");
    assert_eq!(ir_text, golden.trim(), "JSON twin IR bytes are identical");
}

#[test]
fn repeated_ir_runs_are_byte_identical() {
    let first = run_load(&format!("{FIXTURE_ROOT}/{FULL_KINDS}"), &["--ir", "--json"]);
    let second = run_load(&format!("{FIXTURE_ROOT}/{FULL_KINDS}"), &["--ir", "--json"]);
    assert_eq!(first.stdout, second.stdout, "canonical IR is stable");
    assert_eq!(first.stderr, second.stderr);
}

#[test]
fn spans_add_a_sorted_occurrence_safe_source_map_sibling() {
    let golden = std::fs::read_to_string(
        workspace_root()
            .join(FIXTURE_ROOT)
            .join(FULL_KINDS)
            .join("ir.spans.golden.json"),
    )
    .expect("read ir.spans.golden.json");
    let plain = run_load(&format!("{FIXTURE_ROOT}/{FULL_KINDS}"), &["--ir", "--json"]);
    let with_spans = run_load(
        &format!("{FIXTURE_ROOT}/{FULL_KINDS}"),
        &["--ir", "--spans", "--json"],
    );
    assert_exit(&plain, 0);
    assert_exit(&with_spans, 0);

    let plain_value: Value =
        serde_json::from_str(stdout_text(&plain).trim()).expect("plain envelope");
    let spans_value: Value =
        serde_json::from_str(stdout_text(&with_spans).trim()).expect("spans envelope");
    assert!(
        plain_value.get("sourceMap").is_none(),
        "no map without --spans"
    );
    assert_eq!(plain_value["ir"], spans_value["ir"], "spans never alter IR");

    let source_map = spans_value["sourceMap"].as_array().expect("map array");
    assert!(
        source_map.len() >= source_map_window_minimum(),
        "every definition, field, type, and reference position has an entry"
    );
    // Sorted by (path, pointer, startByte) and occurrence-safe.
    let mut sorted: Vec<(String, String, u64)> = source_map
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().expect("path").to_owned(),
                entry["pointer"].as_str().expect("pointer").to_owned(),
                entry["start"]["byte"].as_u64().expect("byte"),
            )
        })
        .collect();
    let unsorted = sorted.clone();
    sorted.sort();
    assert_eq!(unsorted, sorted, "source map is sorted");
    let mut pointers: Vec<(String, String)> = sorted
        .iter()
        .map(|(path, pointer, _)| (path.clone(), pointer.clone()))
        .collect();
    pointers.sort();
    pointers.dedup();
    // Definition-level pointers are unique; type wrapper suffixes may repeat
    // a base pointer with distinct spans, so dedup on (path, pointer, byte).
    let mut unique_entries: Vec<(String, String, u64)> = sorted.clone();
    unique_entries.dedup();

    // The emitted array equals the frozen golden byte for byte; compare the
    // raw slice because Value re-serialization would reorder object keys.
    let raw = stdout_text(&with_spans);
    let marker = ",\"sourceMap\":";
    let start = raw.find(marker).expect("sourceMap sibling") + marker.len();
    let array = &raw[start..raw.len() - 2];
    assert_eq!(array, golden.trim(), "source map bytes are golden");
}

fn source_map_window_minimum() -> usize {
    // 17 definitions + project + 2 modules + every field/type/reference
    // crumb of the full-kinds fixture; the frozen golden is authoritative,
    // this bound only guards against an empty or truncated map.
    150
}

#[test]
fn ir_failures_bind_to_the_invalid_exit_and_stderr_stream() {
    for name in [
        "invalid-unknown-field",
        "invalid-kind-placement",
        "invalid-missing-field",
        "invalid-duplicate-set-member",
        "invalid-decision-unknown",
        "invalid-id-syntax",
        "invalid-empty-identity",
        "invalid-operation-unknown",
    ] {
        let output = run_load(&format!("{FIXTURE_ROOT}/{name}"), &["--ir"]);
        assert_eq!(output.status.code(), Some(1), "{name}: exit 1");
        assert!(
            !stderr_text(&output).is_empty(),
            "{name}: stderr carries the envelope"
        );
        assert!(
            stdout_text(&output).is_empty(),
            "{name}: stdout stays empty"
        );
        let human = stderr_text(&output).trim().to_owned();
        assert!(
            human.starts_with("invalid: ir."),
            "{name}: human line names ir codes: {human}"
        );
    }
}

#[test]
fn without_ir_flag_the_loader_emits_the_preserved_model() {
    let plain = run_load(&format!("{FIXTURE_ROOT}/{FULL_KINDS}"), &["--json"]);
    assert_exit(&plain, 0);
    let envelope: Value = serde_json::from_str(stdout_text(&plain).trim()).expect("envelope");
    assert!(envelope.get("ir").is_none(), "no IR without --ir");
    assert!(envelope.get("model").is_some(), "model is preserved");
}

#[test]
fn human_success_line_names_the_ir_contract() {
    let output = run_load(&format!("{FIXTURE_ROOT}/{FULL_KINDS}"), &["--ir"]);
    assert_exit(&output, 0);
    assert_eq!(
        stdout_text(&output).trim(),
        "compiled ir dev.lekalo.ir@0.1.0: 2 modules, 19 definitions"
    );
}

#[test]
fn human_failures_list_sorted_ir_codes_on_one_line() {
    let output = run_load(&format!("{FIXTURE_ROOT}/invalid-unknown-field"), &["--ir"]);
    assert_eq!(output.status.code(), Some(1));
    let human = stderr_text(&output).trim().to_owned();
    assert_eq!(human, "invalid: ir.unknown-field");
}
