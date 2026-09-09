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

fn native_vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/requirements/native-parser-vectors.json"
    ))
    .unwrap()
}

fn tree_bytes(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
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
    walk(dir, dir, &mut out);
    out.sort();
    out
}

// Expected catalogs/digests and archive text come from executing the pinned
// native parser, not this reader. Repeated sections deliberately fail closed.
fn check_native_vectors(kind: &str) {
    for vector in native_vectors()["vectors"].as_array().unwrap() {
        if vector["kind"] != kind {
            continue;
        }
        let name = vector["name"].as_str().unwrap();
        let temp = scratch();
        let dir = temp.path();
        let accepted = dir.join("openspec/specs/planner/spec.md");
        let active = dir.join("openspec/changes/2026-09-01-archive-focus");
        fs::write(&accepted, vector["accepted"].as_str().unwrap()).unwrap();
        fs::write(
            active.join("specs/planner/spec.md"),
            vector["delta"].as_str().unwrap(),
        )
        .unwrap();
        let mut value = attachment(dir);
        value["references"] = vector["expectedEntries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                json!({
                    "symbol":"planner.focus_task", "relation":"implements", "source":"openspec",
                    "requirement":entry["id"], "revision":entry["revision"]
                })
            })
            .collect::<Vec<_>>()
            .into();
        save_attachment(dir, &value);
        let before = tree_bytes(dir);
        if vector["unsupported"] == true {
            for op in ["validate", "report", "trace"] {
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
                    1,
                    "{name}/{op}: {}",
                    stderr_text(&output)
                );
                assert!(
                    output.stdout.is_empty(),
                    "{name}/{op} must not emit success data"
                );
                assert!(
                    stderr_text(&output).contains("requirements.provider-invalid"),
                    "{name}"
                );
                assert!(
                    stderr_text(&output).contains("duplicate-operation-section"),
                    "{name}"
                );
            }
            assert_eq!(tree_bytes(dir), before, "{name} no writes");
            continue;
        }
        requirements_json(dir, "validate", 0);
        let report = requirements_json(dir, "report", 0);
        let entries: Vec<Value> = report["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| json!({"id":r["id"], "revision":r["digest"]}))
            .collect();
        assert_eq!(
            Value::from(entries),
            vector["expectedEntries"],
            "{name} native catalog and body digests"
        );
        let trace = requirements_json(dir, "trace", 0);
        assert_eq!(
            trace["trace"]["relations"].as_array().unwrap().len(),
            vector["expectedEntries"].as_array().unwrap().len(),
            "{name}"
        );
        assert_eq!(tree_bytes(dir), before, "{name} no writes");

        for excluded in vector["excluded"].as_array().unwrap() {
            value["references"].as_array_mut().unwrap().push(json!({
                "symbol":"planner.focus_task", "relation":"implements", "source":"openspec",
                "requirement":excluded["id"], "revision":excluded["revision"]
            }));
        }
        if !vector["excluded"].as_array().unwrap().is_empty() {
            save_attachment(dir, &value);
            let before = tree_bytes(dir);
            requirements_json(dir, "validate", 3);
            let report = requirements_json(dir, "report", 0);
            let trace = requirements_json(dir, "trace", 0);
            for excluded in vector["excluded"].as_array().unwrap() {
                assert!(
                    report["report"]["references"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["requirement"] == excluded["id"] && r["status"] == "missing"),
                    "{name}"
                );
                let target = format!("requirement:openspec:{}", excluded["id"].as_str().unwrap());
                assert!(
                    !trace["trace"]["relations"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["toNode"] == target),
                    "{name} example must not be confirmed"
                );
            }
            assert_eq!(tree_bytes(dir), before, "{name} denied no writes");
        }
    }
}

#[test]
fn native_fence_matrix_matches_catalog_revisions_and_cli_gates() {
    check_native_vectors("fence");
}

#[test]
fn native_lexical_catalogs_and_actual_rebuilt_specs_preserve_trace() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/requirements/native-lexical-vectors.json"
    ))
    .unwrap();
    for vector in vectors["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let temp = scratch();
        let dir = temp.path();
        let spec = dir.join("openspec/specs/planner/spec.md");
        let active = dir.join("openspec/changes/2026-09-01-archive-focus");
        fs::write(&spec, vector["accepted"].as_str().unwrap()).unwrap();
        fs::write(
            active.join("specs/planner/spec.md"),
            vector["delta"].as_str().unwrap(),
        )
        .unwrap();
        let mut value = attachment(dir);
        value["references"] =
            if vector["unsupported"] == true {
                json!([])
            } else {
                vector["expectedEntries"].as_array().unwrap().iter().map(|e| json!({
                "symbol":"planner.focus_task", "relation":"implements", "source":"openspec",
                "requirement":e["id"], "revision":e["revision"]
            })).collect::<Vec<_>>().into()
            };
        save_attachment(dir, &value);
        let snapshot = tree_bytes(dir);
        if vector["unsupported"] == true {
            for op in ["validate", "report", "trace"] {
                let result = lekalo_in(
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
                    exit_code(&result),
                    1,
                    "{name}/{op}: {}",
                    stderr_text(&result)
                );
                assert!(result.stdout.is_empty(), "{name}/{op} partial success");
                assert!(
                    stderr_text(&result).contains("requirements.provider-invalid"),
                    "{name}/{op}"
                );
            }
            assert_eq!(snapshot, tree_bytes(dir), "{name} rejected no writes");
            continue;
        }
        let output = lekalo_in(
            dir,
            &[
                "--json",
                "requirements",
                "report",
                "requirements.attachment.json",
                "--project",
                ".",
            ],
        );
        assert_eq!(exit_code(&output), 0, "{name}: {}", stderr_text(&output));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        let entries: Vec<_> = report["report"]["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| json!({"id":r["id"], "revision":r["digest"]}))
            .collect();
        assert_eq!(
            Value::from(entries),
            vector["expectedEntries"],
            "{name} native catalog/revisions"
        );
        requirements_json(dir, "validate", 0);
        let before = requirements_json(dir, "trace", 0);
        assert_eq!(
            before["trace"]["relations"].as_array().unwrap().len(),
            vector["expectedEntries"].as_array().unwrap().len(),
            "{name}"
        );
        assert_eq!(snapshot, tree_bytes(dir), "{name} no writes");
        if let Some(rebuilt) = vector["archiveAccepted"].as_str() {
            // Actual upstream buildUpdatedSpec output, recorded in the fixture.
            // Native refusals have no rebuilt text and are extraction-only cases.
            fs::write(&spec, rebuilt).unwrap();
            fs::create_dir_all(dir.join("openspec/changes/archive")).unwrap();
            fs::rename(active, dir.join("openspec/changes/archive/correction")).unwrap();
            let snapshot = tree_bytes(dir);
            requirements_json(dir, "validate", 0);
            assert_eq!(
                before,
                requirements_json(dir, "trace", 0),
                "{name} actual native rebuild"
            );
            assert_eq!(snapshot, tree_bytes(dir), "{name} archived no writes");
        } else {
            assert!(
                vector["nativeError"].is_string(),
                "{name} must record the native refusal"
            );
        }
    }
}

#[cfg(any(unix, windows))]
fn directory_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        let result = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(target.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "junction: {}",
            stderr_text(&result)
        );
    }
}

#[test]
fn native_fence_eof_refuses_lossy_inputs_and_preserves_closed_archive_traces() {
    // These rebuilt bytes were returned by the pinned original buildUpdatedSpec,
    // including its lossy rebuilds for the four deliberately unsupported inputs.
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/requirements/native-fence-eof-vectors.json"
    ))
    .unwrap();
    for vector in vectors["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let temp = scratch();
        let dir = temp.path();
        let spec = dir.join("openspec/specs/planner/spec.md");
        let active = dir.join("openspec/changes/2026-09-01-archive-focus");
        fs::write(&spec, vector["accepted"].as_str().unwrap()).unwrap();
        fs::write(
            active.join("specs/planner/spec.md"),
            vector["delta"].as_str().unwrap(),
        )
        .unwrap();
        let mut value = attachment(dir);
        value["references"] = vector["expectedEntries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                json!({
                    "symbol":"planner.focus_task", "relation":"implements", "source":"openspec",
                    "requirement":entry["id"], "revision":entry["revision"]
                })
            })
            .collect::<Vec<_>>()
            .into();
        save_attachment(dir, &value);
        let mut active_trace = None;
        for archived in [false, true] {
            if archived {
                fs::write(&spec, vector["archiveAccepted"].as_str().unwrap()).unwrap();
                fs::create_dir_all(dir.join("openspec/changes/archive")).unwrap();
                fs::rename(&active, dir.join("openspec/changes/archive/eof")).unwrap();
            }
            for op in ["report", "validate", "trace"] {
                let before = tree_bytes(dir);
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
                assert_eq!(tree_bytes(dir), before, "{name}/{op}/{archived} no writes");
                if vector["unsupported"] == true {
                    assert_eq!(exit_code(&output), 1, "{name}/{op}/{archived}");
                    assert!(output.stdout.is_empty(), "{name}/{op} partial success");
                    assert!(stderr_text(&output).contains("requirements.provider-invalid"));
                    assert!(stderr_text(&output).contains("unsupported-native-grammar"));
                    continue;
                }
                assert_eq!(
                    exit_code(&output),
                    0,
                    "{name}/{op}: {}",
                    stderr_text(&output)
                );
                let result: Value = serde_json::from_slice(&output.stdout).unwrap();
                if op == "report" {
                    let entries: Vec<_> = result["report"]["requirements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| json!({"id":r["id"], "revision":r["digest"]}))
                        .collect();
                    assert_eq!(Value::from(entries), vector["expectedEntries"], "{name}");
                } else if op == "trace" {
                    lekalo_core::trace::TraceManifest::parse(
                        &serde_json::to_vec(&result["trace"]).unwrap(),
                    )
                    .unwrap();
                    if archived {
                        assert_eq!(active_trace.as_ref(), Some(&output.stdout), "{name}");
                    } else {
                        active_trace = Some(output.stdout);
                    }
                }
            }
        }
    }
}

#[test]
#[cfg(any(unix, windows))]
fn provider_directory_links_fail_closed_even_when_empty_or_missing_children() {
    for kind in ["root", "specs", "changes", "change", "ancestor"] {
        for full in [false, true] {
            for referenced in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let dir = temp.path().join("project");
                copy_fixture(&fixture_path().join("lekalo"), &dir.join("lekalo"));
                let external = temp.path().join("external");
                fs::create_dir(&external).unwrap();
                if full {
                    copy_fixture(&fixture_path().join("openspec"), &external);
                }
                let link = match kind {
                    "root" => dir.join("openspec"),
                    "specs" => dir.join("openspec/specs"),
                    "changes" => dir.join("openspec/changes"),
                    "change" => dir.join("openspec/changes/correction"),
                    "ancestor" => dir.join("nested"),
                    _ => unreachable!(),
                };
                fs::create_dir_all(link.parent().unwrap()).unwrap();
                directory_link(&external, &link);
                let mut value = attachment(&fixture_path());
                if kind == "ancestor" {
                    value["providers"][0]["root"] = "nested/openspec".into();
                }
                if !referenced {
                    value["references"] = json!([]);
                }
                save_attachment(&dir, &value);
                for op in ["validate", "report", "trace"] {
                    let result = lekalo_in(
                        &dir,
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
                        exit_code(&result),
                        1,
                        "{kind}/{full}/{referenced}/{op}: {}",
                        stderr_text(&result)
                    );
                    assert!(result.stdout.is_empty());
                    assert!(stderr_text(&result).contains("requirements.provider-invalid"));
                }
            }
        }
    }
    for kind in ["absent", "empty-root", "empty-specs", "full"] {
        for referenced in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let dir = temp.path();
            copy_fixture(&fixture_path().join("lekalo"), &dir.join("lekalo"));
            match kind {
                "empty-root" => fs::create_dir(dir.join("openspec")).unwrap(),
                "empty-specs" => fs::create_dir_all(dir.join("openspec/specs")).unwrap(),
                "full" => copy_fixture(&fixture_path().join("openspec"), &dir.join("openspec")),
                _ => (),
            }
            let mut value = attachment(&fixture_path());
            if !referenced {
                value["references"] = json!([]);
            }
            save_attachment(dir, &value);
            let result = lekalo_in(
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
            let expected = if !referenced || kind == "full" {
                0
            } else if kind == "absent" {
                4
            } else {
                3
            };
            assert_eq!(
                exit_code(&result),
                expected,
                "{kind}/{referenced}: {}",
                stderr_text(&result)
            );
        }
    }
}

#[test]
fn native_repeated_sections_deny_and_distinct_sections_resolve() {
    check_native_vectors("repeated");
}

#[test]
fn explicit_removal_report_and_query_override_body_similarity() {
    for (operation, survivors, expected) in [
        ("remove", 0, "removed"),
        ("remove", 1, "removed"),
        ("remove", 2, "removed"),
        ("rename", 1, "renamed"),
        ("absent", 1, "rename-candidate"),
        ("absent", 2, "ambiguous-rename"),
    ] {
        let temp = scratch();
        let dir = temp.path();
        let body = "The planner SHALL retain the requested action.\n";
        let mut accepted = "## Requirements\n".to_owned();
        if operation != "absent" {
            accepted.push_str(&format!("### Requirement: Old\n{body}"));
        }
        for i in 0..survivors {
            accepted.push_str(&format!("\n### Requirement: Unrelated {i}\n{body}"));
        }
        fs::write(dir.join("openspec/specs/planner/spec.md"), accepted).unwrap();
        let delta = match operation {
            "remove" => Some("## REMOVED Requirements\n- `### Requirement: Old`\n"),
            "rename" => Some("## RENAMED Requirements\n- FROM: `### Requirement: Old`\n- TO: `### Requirement: Renamed`\n"),
            _ => None,
        };
        if let Some(delta) = delta {
            let path = dir.join("openspec/changes/operation/specs/planner");
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("spec.md"), delta).unwrap();
        }
        let mut value = attachment(dir);
        value["references"] = json!([{
            "symbol": "planner.focus_task", "relation": "implements", "source": "openspec",
            "requirement": "planner.REQ-old", "revision": digest(body.as_bytes())
        }]);
        save_attachment(dir, &value);
        let before = tree_bytes(dir);
        let report = requirements_json(dir, "report", 0);
        let reference = &report["report"]["references"][0];
        assert_eq!(reference["status"], "missing");
        assert_eq!(
            reference["renamedTo"],
            if operation == "rename" {
                json!("planner.REQ-renamed")
            } else {
                Value::Null
            }
        );
        assert_eq!(
            reference["renameCandidates"].as_array().unwrap().len(),
            if operation == "absent" { survivors } else { 0 }
        );
        assert_eq!(report["report"]["impact"][0]["change"], expected);
        let output = lekalo_in(
            dir,
            &[
                "--json",
                "requirements",
                "query",
                "requirements.attachment.json",
                "impact",
                "--project",
                ".",
            ],
        );
        assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
        assert!(output.stderr.is_empty());
        let query: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(query["requirements"]["impact"], report["report"]["impact"]);
        assert_eq!(requirements_json(dir, "validate", 3)["status"], "denied");
        assert_eq!(
            tree_bytes(dir),
            before,
            "{operation}/{survivors}: no writes"
        );
    }
}

#[test]
fn native_plan_archive_preserves_confirmed_trace() {
    for vector in native_vectors()["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == "distinct-sections" || v["name"] == "distinct-sections-reversed")
    {
        let temp = scratch();
        let dir = temp.path();
        let accepted = dir.join("openspec/specs/planner/spec.md");
        let active = dir.join("openspec/changes/2026-09-01-archive-focus");
        fs::write(&accepted, vector["accepted"].as_str().unwrap()).unwrap();
        fs::write(
            active.join("specs/planner/spec.md"),
            vector["delta"].as_str().unwrap(),
        )
        .unwrap();
        let mut value = attachment(dir);
        value["references"] = vector["expectedEntries"].as_array().unwrap().iter()
            .map(|e| json!({"symbol":"planner.focus_task", "relation":"implements", "source":"openspec", "requirement":e["id"], "revision":e["revision"]})).collect::<Vec<_>>().into();
        save_attachment(dir, &value);
        requirements_json(dir, "validate", 0);
        let before = requirements_json(dir, "trace", 0);
        // This disk transition applies the native parser's recorded plan; it
        // does not invoke or claim qualification of the OpenSpec archive CLI.
        fs::write(&accepted, vector["archiveAccepted"].as_str().unwrap()).unwrap();
        let archive = dir.join("openspec/changes/archive");
        fs::create_dir_all(&archive).unwrap();
        fs::rename(active, archive.join("2026-09-01-archive-focus")).unwrap();
        let snapshot = tree_bytes(dir);
        requirements_json(dir, "validate", 0);
        let after = requirements_json(dir, "trace", 0);
        assert_eq!(before, after, "{} native plan archive", vector["name"]);
        assert_eq!(tree_bytes(dir), snapshot);
    }
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
    assert_eq!(stdout_text(&output).trim(), "lekalo 0.2.4");
    let json = lekalo_in(&fixture_path(), &["--json", "--version"]);
    assert_eq!(exit_code(&json), 0);
    assert_eq!(
        stdout_text(&json).trim(),
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.4\"\n}"
    );
}
