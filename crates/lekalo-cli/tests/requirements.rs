//! Issue #36 CLI tests for the `lekalo requirements` handoff: the gate
//! envelope on the hermetic planner fixture, the canonical report and
//! trace exports, the closed query selectors, the denied/unavailable
//! exit protocol, and the version custody probe.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn copy_fixture(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lekalo" {
            continue;
        }
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

fn attachment(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("requirements.attachment.json")).unwrap()).unwrap()
}

fn save_attachment(dir: &Path, value: &Value) {
    fs::write(
        dir.join("requirements.attachment.json"),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
}

fn digest(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        lekalo_core::versioning::plan::sha256_hex(bytes)
    )
}

fn repin_model(dir: &Path, value: &mut Value) {
    let load = lekalo_in(dir, &["--json", "load", "--project", "."]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));
    // The production pin binds the entire canonical load envelope, without LF.
    value["modelRef"]["digest"] = digest(load.stdout.strip_suffix(b"\n").unwrap()).into();
}

fn requirements_json(dir: &Path, op: &str, expected: u8) -> Value {
    let output = lekalo_in(
        dir,
        &[
            "--json",
            "requirements",
            op,
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(
        exit_code(&output),
        expected,
        "{op}: {}",
        stderr_text(&output)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    if op == "trace" {
        lekalo_core::trace::TraceManifest::parse(&serde_json::to_vec(&value["trace"]).unwrap())
            .expect("accepted trace validator");
    }
    value
}

#[test]
fn accepted_requirements_are_confined_to_the_first_native_section() {
    let body = "This example is documentation only and is not an accepted requirement.\n";
    let example = format!("### Requirement: Example only\n\n{body}");
    for placement in ["before", "appendix", "none", "second-section", "inside"] {
        let temp = scratch();
        let dir = temp.path();
        let path = dir.join("openspec/specs/planner/spec.md");
        let original = fs::read_to_string(&path).unwrap();
        let text = match placement {
            "before" => format!("{example}\n{original}"),
            "appendix" => format!("{original}\n## Appendix\n\n{example}"),
            "none" => original.replace("## Requirements", "## Examples"),
            "second-section" => format!("{original}\n## Appendix\n\n## Requirements\n{example}"),
            "inside" => format!(
                "{}\n{example}",
                original.replace("## Requirements", "##\tReQuIrEmEnTs  ")
            ),
            _ => unreachable!(),
        };
        fs::write(&path, text).unwrap();
        let mut value = attachment(dir);
        if placement != "none" {
            value["references"].as_array_mut().unwrap().push(json!({
                "symbol":"planner.focus_task", "relation":"implements", "source":"openspec",
                "requirement":"planner.REQ-example-only", "revision":digest(body.as_bytes())
            }));
        }
        save_attachment(dir, &value);
        let expected = if placement == "inside" { 0 } else { 3 };
        requirements_json(dir, "validate", expected);
        let report = requirements_json(dir, "report", 0);
        let trace = requirements_json(dir, "trace", 0);
        let excluded: Vec<&str> = if placement == "none" {
            vec!["planner.REQ-focus-task", "planner.REQ-restore-focus"]
        } else if placement == "inside" {
            vec![]
        } else {
            vec!["planner.REQ-example-only"]
        };
        for id in excluded {
            assert!(
                !report["report"]["requirements"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id),
                "{placement}"
            );
            assert!(report["report"]["references"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["requirement"] == id && r["status"] == "missing"));
            assert!(!trace["trace"]["relations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["toNode"] == format!("requirement:openspec:{id}")));
        }
        // Active delta remains authoritative even when the accepted section is absent.
        assert!(report["report"]["references"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["requirement"] == "planner.REQ-archive-notification"
                && r["status"] == "fresh"));
    }
}

#[test]
fn project_id_vectors_work_through_cli_with_exact_model_pins() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/requirements/project-id-vectors.json"
    ))
    .unwrap();
    for vector in vectors.as_array().unwrap() {
        let temp = scratch();
        let dir = temp.path();
        let mut value = attachment(dir);
        value["projectId"] = vector["id"].clone();
        if vector["valid"] == true {
            let file = dir.join("lekalo/project.yaml");
            fs::write(
                &file,
                fs::read_to_string(&file).unwrap().replace(
                    "id: planner",
                    &format!("id: {}", vector["id"].as_str().unwrap()),
                ),
            )
            .unwrap();
            repin_model(dir, &mut value);
            save_attachment(dir, &value);
            for op in ["validate", "report", "trace"] {
                requirements_json(dir, op, 0);
            }
        } else {
            save_attachment(dir, &value);
            let output = lekalo_in(
                dir,
                &[
                    "--json",
                    "requirements",
                    "validate",
                    "requirements.attachment.json",
                    "--project",
                    ".",
                ],
            );
            assert_eq!(exit_code(&output), 1, "{vector}: {}", stderr_text(&output));
            assert!(stderr_text(&output).contains("requirements.document-invalid"));
        }
    }
}

#[test]
fn maximum_semantic_ids_project_without_collisions_and_keep_gap_anchors() {
    let temp = scratch();
    let dir = temp.path();
    let module = "m".repeat(63);
    let prefix = format!("{module}.{}.{}", "n".repeat(63), "s".repeat(62));
    let symbols = [format!("{prefix}s"), format!("{prefix}t")];
    let module_dir = dir.join("lekalo/modules").join(&module);
    fs::create_dir_all(&module_dir).unwrap();
    fs::write(module_dir.join("module.yaml"), format!("schema_version: \"1.0.0\"\ndefinitions:\n  - id: {module}\n    kind: module\n    version: 1\n")).unwrap();
    let definitions: Vec<String> = symbols
        .iter()
        .map(|id| format!("  - id: {id}\n    kind: command\n    version: 1\n"))
        .collect();
    let commands = module_dir.join("commands.yaml");
    fs::write(
        &commands,
        format!(
            "schema_version: \"1.0.0\"\ndefinitions:\n{}",
            definitions.join("")
        ),
    )
    .unwrap();
    let mut value = attachment(dir);
    value["references"][0]["symbol"] = symbols[0].clone().into();
    value["references"][1]["symbol"] = symbols[1].clone().into();
    repin_model(dir, &mut value);
    save_attachment(dir, &value);
    requirements_json(dir, "validate", 0);
    requirements_json(dir, "report", 0);
    let trace = requirements_json(dir, "trace", 0);
    let canonical =
        lekalo_core::trace::TraceManifest::parse(&serde_json::to_vec(&trace["trace"]).unwrap())
            .unwrap()
            .canonical_bytes()
            .unwrap();
    assert_eq!(
        canonical,
        include_str!("../../../tests/fixtures/requirements/golden/maximal.trace.json").trim_end()
    );
    assert_eq!(
        digest(canonical.as_bytes()),
        include_str!("../../../tests/fixtures/requirements/golden/maximal.trace.json.sha256")
            .trim()
    );
    let nodes = trace["trace"]["nodes"].as_array().unwrap();
    let node_ids: Vec<String> = symbols
        .iter()
        .map(|symbol| {
            assert_eq!(symbol.len(), 191);
            let node = nodes.iter().find(|n| n["semanticId"] == *symbol).unwrap();
            let id = node["nodeId"].as_str().unwrap();
            assert!(id.len() <= 192);
            assert!(trace["trace"]["relations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["fromNode"] == id && r["status"] == "confirmed"));
            id.to_owned()
        })
        .collect();
    assert_ne!(
        node_ids[0], node_ids[1],
        "distinct final characters must not truncate to one node"
    );
    value["references"].as_array_mut().unwrap().reverse();
    value["providers"].as_array_mut().unwrap().reverse();
    fs::write(
        &commands,
        format!(
            "schema_version: \"1.0.0\"\ndefinitions:\n{}{}",
            definitions[1], definitions[0]
        ),
    )
    .unwrap();
    repin_model(dir, &mut value);
    save_attachment(dir, &value);
    assert_eq!(
        requirements_json(dir, "trace", 0),
        trace,
        "input permutations preserve canonical export and digest"
    );
    for conflict in [false, true] {
        let mut changed = value.clone();
        if conflict {
            let delta = dir.join("openspec/changes/duplicate/specs/planner");
            fs::create_dir_all(&delta).unwrap();
            fs::write(
                delta.join("spec.md"),
                "## ADDED Requirements\n### Requirement: Focus task\nContradiction\n",
            )
            .unwrap();
        } else {
            for row in changed["references"].as_array_mut().unwrap() {
                if row["symbol"] == symbols[1] {
                    row["requirement"] = "planner.REQ-missing".into();
                }
            }
        }
        save_attachment(dir, &changed);
        requirements_json(dir, "validate", 3);
        let projected = requirements_json(dir, "trace", 0);
        assert!(projected["trace"]["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["anchorNode"] == node_ids[1]
                && g["gapKind"]
                    == if conflict {
                        "conflict"
                    } else {
                        "missing-requirement"
                    }));
        assert!(!projected["trace"]["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["fromNode"] == node_ids[1]));
    }
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

/// The selection policy denies alias-spelled working directories
/// (`structure.selection-alias`) before any command logic runs; chdir
/// the child into the resolved spelling, stripped of the `\\?\` verbatim
/// prefix `canonicalize` produces on Windows.
fn alias_free_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        // `\\?\C:\...` -> `C:\...`; UNC (`\\?\UNC\...`) stays verbatim.
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

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/requirements/planner"),
    )
}

#[test]
fn validate_gate_accepts_the_fresh_fixture() {
    let fixture = fixture_path();
    let human = lekalo_in(
        &fixture,
        &[
            "requirements",
            "validate",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&human), 0, "{}", stderr_text(&human));
    assert!(
        human
            .stdout
            .windows(b"references 3".len())
            .any(|window| window == b"references 3"),
        "{}",
        stdout_text(&human)
    );
    let json = lekalo_in(
        &fixture,
        &[
            "--json",
            "requirements",
            "validate",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&json), 0);
    let envelope: serde_json::Value =
        serde_json::from_str(stdout_text(&json).trim()).expect("envelope json");
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["requirements"]["fresh"], 3);
    assert_eq!(envelope["requirements"]["coverageGaps"], 0);
}

#[test]
fn report_and_trace_emit_schema_valid_canonical_bytes() {
    let fixture = fixture_path();
    let report = lekalo_in(
        &fixture,
        &[
            "requirements",
            "report",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&report), 0, "{}", stderr_text(&report));
    let parsed: serde_json::Value =
        serde_json::from_str(stdout_text(&report).trim()).expect("report json");
    assert_eq!(parsed["identity"], "dev.lekalo.requirements-report@1.0.0");

    let trace = lekalo_in(
        &fixture,
        &[
            "requirements",
            "trace",
            "requirements.attachment.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&trace), 0, "{}", stderr_text(&trace));
    let manifest: serde_json::Value =
        serde_json::from_str(stdout_text(&trace).trim()).expect("trace json");
    assert_eq!(manifest["schemaVersion"], "lekalo/trace-manifest/v1.0.0");
    assert_eq!(manifest["completeness"], "partial");
    assert_eq!(manifest["manifestId"], "requirements-trace");
}

#[test]
fn query_answers_the_closed_selectors() {
    let fixture = fixture_path();
    let symbol = lekalo_in(
        &fixture,
        &[
            "requirements",
            "query",
            "requirements.attachment.json",
            "symbol:planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&symbol), 0, "{}", stderr_text(&symbol));
    assert!(
        stdout_text(&symbol).contains("openspec:planner.REQ-focus-task derived_from fresh"),
        "{}",
        stdout_text(&symbol)
    );

    let unknown = lekalo_in(
        &fixture,
        &[
            "requirements",
            "query",
            "requirements.attachment.json",
            "symbol:planner.not_a_symbol",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&unknown), 1, "unknown subject is invalid");

    let selector = lekalo_in(
        &fixture,
        &[
            "requirements",
            "query",
            "requirements.attachment.json",
            "requirements-for",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&selector), 1, "unknown selector is usage");
}

#[test]
fn missing_attachment_is_invalid_not_a_crash() {
    let fixture = fixture_path();
    let output = lekalo_in(
        &fixture,
        &[
            "requirements",
            "validate",
            "does-not-exist.json",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert!(
        stderr_text(&output).contains("requirements.document-invalid"),
        "{}",
        stderr_text(&output)
    );
}

#[test]
fn version_custody_probe() {
    let output = lekalo_in(&fixture_path(), &["--version"]);
    assert_eq!(exit_code(&output), 0);
    assert_eq!(stdout_text(&output).trim(), "lekalo 0.2.1");
    let json = lekalo_in(&fixture_path(), &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.1\"\n}"
    );
}
