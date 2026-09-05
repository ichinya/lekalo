//! Issue #7 loader conformance: fixture-driven CLI runs, deterministic
//! canonical output, root discovery, env precedence, and platform link
//! rejections.
//!
//! Selectors are always invocation-relative (the #4 grammar rejects
//! absolute selectors), so every run sets an explicit working directory and
//! passes `--project` as a relative path or `.`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE_ROOT: &str = "tests/fixtures/loader";

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

/// Resolve a fixture path to its canonical on-disk spelling.
///
/// GitHub's Windows runners export `%TEMP%` spelled with the 8.3 alias of
/// the profile directory (`C:\Users\RUNNER~1\AppData\Local\Temp`), and the
/// selection policy denies alias spellings (`structure.selection-alias`)
/// before any document classification is reached. Temp-backed fixtures
/// must therefore chdir into the resolved spelling; `canonicalize` returns
/// it under a `\\?\` verbatim prefix that is stripped back to the plain
/// drive form so the child sees ordinary path components.
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

/// Run `lekalo load` with a relative selector from `cwd` (default: the
/// workspace root). Absolute selectors are grammar violations and are never
/// exercised through this helper.
fn run_load(selector: &str, extra_args: &[&str], cwd: Option<&Path>) -> Output {
    let mut command = Command::new(binary());
    command
        .arg("--no-cache")
        .arg("load")
        .arg("--project")
        .arg(selector);
    for argument in extra_args {
        command.arg(argument);
    }
    let cwd = cwd.map(alias_free_path);
    command
        .current_dir(cwd.as_deref().unwrap_or(&workspace_root()))
        .env_remove("LEKALO_PROJECT");
    command.output().expect("run lekalo load")
}

/// Run `lekalo load` with no selector: discovery from `cwd`.
fn run_discovery(cwd: &Path, extra_args: &[&str]) -> Output {
    let mut command = Command::new(binary());
    command.arg("--no-cache").arg("load");
    for argument in extra_args {
        command.arg(argument);
    }
    command.current_dir(cwd).env_remove("LEKALO_PROJECT");
    command.output().expect("run lekalo load")
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}

/// Run one fixture directory and assert its `expect.json` contract.
fn check_fixture(name: &str) {
    let fixture_dir = workspace_root().join(FIXTURE_ROOT).join(name);
    let selector = format!("{FIXTURE_ROOT}/{name}");
    let expect: Value = serde_json::from_str(
        &std::fs::read_to_string(
            workspace_root()
                .join(FIXTURE_ROOT)
                .join(name)
                .join("expect.json"),
        )
        .unwrap_or_else(|error| panic!("{name}: read expect.json: {error}")),
    )
    .expect("expect.json parses");

    let output = run_load(&selector, &["--json"], None);
    eprintln!("checking fixture: {name}");
    let expected_status = expect["status"].as_str().expect("status string");
    let expected_exit = expect["exit"].as_i64().unwrap_or(match expected_status {
        "valid" => 0,
        "denied" => 3,
        "unsupported-version" => 5,
        _ => 1,
    });
    let expected_stream = expect["stream"].as_str().unwrap_or("stdout");
    assert_exit(&output, expected_exit as i32);

    let envelope_text = if expected_stream == "stderr" {
        assert!(
            output.stdout.is_empty(),
            "{name}: stdout not empty on failure"
        );
        stderr_text(&output)
    } else {
        assert!(
            output.stderr.is_empty(),
            "{name}: stderr not empty on {expected_status}"
        );
        stdout_text(&output)
    };
    let envelope: Value = serde_json::from_str(envelope_text.trim())
        .unwrap_or_else(|error| panic!("{name}: envelope parses: {error}\n{envelope_text}"));
    assert_eq!(
        envelope["status"].as_str(),
        Some(expected_status),
        "{name}: status"
    );
    if let Some(code) = expect["code"].as_str() {
        let first = envelope["diagnostics"][0]["id"]
            .as_str()
            .unwrap_or_default();
        assert_eq!(first, code, "{name}: first reason code");
    }
    if let Some(span) = expect["span"].as_object() {
        let actual = &envelope["diagnostics"][0]["source"]["range"];
        assert_eq!(
            actual["start"]["byte"], span["start"]["byte"],
            "{name}: span byte"
        );
        assert_eq!(
            actual["start"]["line"], span["start"]["line"],
            "{name}: span line"
        );
        assert_eq!(
            actual["start"]["column"], span["start"]["column"],
            "{name}: span column"
        );
    }
    if expected_status == "valid" {
        // Success output is exactly one compact line.
        assert_eq!(
            envelope_text.lines().count(),
            1,
            "{name}: success is one line\n{envelope_text}"
        );
    }
    if let Some(golden) = expect["modelGolden"].as_str() {
        let golden_bytes = std::fs::read(fixture_dir.join(golden)).expect("golden bytes");
        assert_eq!(
            output.stdout.as_slice(),
            golden_bytes.as_slice(),
            "{name}: canonical model bytes match the golden"
        );
    }
}

#[test]
fn every_loader_fixture_matches_its_expectation() {
    let root = workspace_root().join(FIXTURE_ROOT);
    let mut names: Vec<String> = std::fs::read_dir(&root)
        .expect("fixture root")
        .map(|entry| {
            entry
                .expect("fixture entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.starts_with("valid-") || name.starts_with("invalid-"))
        .collect();
    names.sort();
    assert!(
        names.len() >= 40,
        "fixture suite unexpectedly small: {}",
        names.len()
    );
    for name in names {
        check_fixture(&name);
    }
}

#[test]
fn twin_projects_produce_identical_canonical_bytes() {
    let json_twin = run_load(
        &format!("{FIXTURE_ROOT}/valid-twin-json"),
        &["--json"],
        None,
    );
    let yaml_twin = run_load(
        &format!("{FIXTURE_ROOT}/valid-twin-yaml"),
        &["--json"],
        None,
    );
    assert_eq!(json_twin.stdout, yaml_twin.stdout);
    // Byte-stable across repeated runs.
    let repeat = run_load(
        &format!("{FIXTURE_ROOT}/valid-twin-json"),
        &["--json"],
        None,
    );
    assert_eq!(json_twin.stdout, repeat.stdout);
}

#[test]
fn spans_add_sorted_source_map_sibling_without_touching_model() {
    let plain = run_load(
        &format!("{FIXTURE_ROOT}/valid-yaml-1-0-0"),
        &["--json"],
        None,
    );
    let with_spans = run_load(
        &format!("{FIXTURE_ROOT}/valid-yaml-1-0-0"),
        &["--json", "--spans"],
        None,
    );
    let with_spans: Value = serde_json::from_str(stdout_text(&with_spans).trim()).unwrap();
    let plain: Value = serde_json::from_str(stdout_text(&plain).trim()).unwrap();
    assert!(plain.get("sourceMap").is_none());
    let source_map = with_spans["sourceMap"].as_array().expect("sourceMap array");
    assert!(!source_map.is_empty());
    // Sorted by (path, pointer, startByte).
    let keys: Vec<(String, String, u64)> = source_map
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().unwrap().to_owned(),
                entry["pointer"].as_str().unwrap().to_owned(),
                entry["start"]["byte"].as_u64().unwrap(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    // The model itself is untouched by --spans.
    assert_eq!(with_spans["model"], plain["model"]);
    assert_eq!(with_spans["modelVersion"], plain["modelVersion"]);
}

#[test]
fn human_output_is_one_stable_line_per_status() {
    let valid = run_load(&format!("{FIXTURE_ROOT}/valid-zero-modules"), &[], None);
    assert_exit(&valid, 0);
    assert_eq!(
        stdout_text(&valid),
        "loaded model 1.0.0: 0 modules, 0 definitions\n"
    );

    let invalid = run_load(&format!("{FIXTURE_ROOT}/invalid-json-comment"), &[], None);
    assert_exit(&invalid, 1);
    let rendered = stderr_text(&invalid);
    let line = rendered.trim_end();
    assert!(
        line.starts_with("invalid error [LEK-LOAD-") && line.contains("] loader.json-parse"),
        "{line}"
    );
    assert_eq!(line.lines().count(), 1);

    let denied = run_load(
        &format!("{FIXTURE_ROOT}/invalid-import-path-escape"),
        &[],
        None,
    );
    assert_exit(&denied, 3);
    assert!(
        stdout_text(&denied).starts_with("denied error [LEK-LOAD-"),
        "denied human names the rule: {}",
        stdout_text(&denied)
    );

    let unsupported = run_load(
        &format!("{FIXTURE_ROOT}/invalid-version-unsupported"),
        &[],
        None,
    );
    assert_exit(&unsupported, 5);
    assert!(
        stderr_text(&unsupported).starts_with("unsupported-version error [LEK-VER-"),
        "unsupported-version human names the rule: {}",
        stderr_text(&unsupported)
    );
}

#[test]
fn selection_precedence_project_flag_beats_environment() {
    let good = format!("{FIXTURE_ROOT}/valid-zero-modules");
    let bad = format!("{FIXTURE_ROOT}/invalid-json-comment");
    let mut command = Command::new(binary());
    command
        .args(["--no-cache", "load", "--json", "--project"])
        .arg(&good)
        .current_dir(workspace_root());
    command.env("LEKALO_PROJECT", &bad);
    let output = command.output().expect("run");
    assert_exit(&output, 0);
}

#[test]
fn environment_selector_loads_and_clean_output_leaks_no_paths() {
    let selector = format!("{FIXTURE_ROOT}/valid-zero-modules");
    let absolute = workspace_root().join(&selector);
    let mut command = Command::new(binary());
    command
        .args(["--no-cache", "load", "--json"])
        .current_dir(workspace_root());
    command.env("LEKALO_PROJECT", &selector);
    let output = command.output().expect("run");
    assert_exit(&output, 0);
    let text = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(!text.contains("LEKALO"), "env name leaked: {text}");
    assert!(
        !text.contains(absolute.to_string_lossy().as_ref()),
        "absolute path leaked: {text}"
    );
}

#[test]
fn discovery_walks_upward_from_a_nested_directory() {
    let nested = workspace_root()
        .join(FIXTURE_ROOT)
        .join("valid-yaml-1-0-0")
        .join("lekalo")
        .join("modules");
    let output = run_discovery(&nested, &["--json"]);
    assert_exit(&output, 0);
    let envelope: Value = serde_json::from_str(&stdout_text(&output)).unwrap();
    assert_eq!(envelope["status"], "valid");
    // The success envelope never echoes the absolute root.
    assert!(
        !stdout_text(&output).contains("Temp"),
        "absolute path leaked"
    );
}

#[test]
fn selection_violations_never_touch_the_filesystem() {
    for selector in ["..", "a\\b", "a:b", "/abs", "a%20b", "~x"] {
        let output = run_load(selector, &["--json"], None);
        assert_eq!(output.status.code(), Some(1), "selector {selector:?}");
        let text = stderr_text(&output);
        assert!(
            text.contains("structure.selection-"),
            "selector {selector:?}: {text}"
        );
    }
}

fn write_minimal_project(project: &Path) {
    let module = project.join("lekalo").join("modules").join("m");
    std::fs::create_dir_all(&module).expect("layout");
    std::fs::write(
        project.join("lekalo").join("project.yaml"),
        "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"j\",\"kind\":\"project\",\"version\":1}]}\n",
    )
    .expect("project doc");
    std::fs::write(
        module.join("module.yaml"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: j\n    kind: module\n    version: 1\n",
    )
    .expect("module doc");
}

#[cfg(windows)]
#[test]
fn junction_inside_module_tree_is_a_policy_denial() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    let modules = project.join("lekalo").join("modules");
    std::fs::create_dir_all(&modules).expect("layout");
    std::fs::write(
        project.join("lekalo").join("project.yaml"),
        "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"j\",\"kind\":\"project\",\"version\":1}]}\n",
    )
    .expect("project doc");
    let real = temp.path().join("outside-target");
    std::fs::create_dir_all(&real).expect("target dir");
    let junction = modules.join("linked");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&real)
        .status()
        .expect("spawn mklink");
    if !status.success() {
        // Junction creation can be restricted; skip without failing.
        return;
    }
    let output = run_load("proj", &["--json"], Some(temp.path()));
    assert_exit(&output, 3);
    let text = stdout_text(&output);
    assert!(text.contains("structure.path-link"), "{text}");
}

#[cfg(unix)]
#[test]
fn symlink_inside_module_tree_is_a_policy_denial() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    let modules = project.join("lekalo").join("modules");
    std::fs::create_dir_all(&modules).expect("layout");
    std::fs::write(
        project.join("lekalo").join("project.yaml"),
        "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"s\",\"kind\":\"project\",\"version\":1}]}\n",
    )
    .expect("project doc");
    if symlink("elsewhere", modules.join("linked")).is_err() {
        return;
    }
    let output = run_load("proj", &["--json"], Some(temp.path()));
    assert_eq!(output.status.code(), Some(3));
    let text = stdout_text(&output);
    assert!(text.contains("structure.path-link"), "{text}");
}

#[test]
fn missing_canonical_document_fails_closed_after_structure_changes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    write_minimal_project(&project);
    let output = run_load("proj", &["--json"], Some(temp.path()));
    assert_exit(&output, 0);
    // Simulate the race: the canonical module document disappears before a
    // re-read; the loader must fail closed instead of treating it as an
    // optional absence.
    std::fs::remove_file(
        project
            .join("lekalo")
            .join("modules")
            .join("m")
            .join("module.yaml"),
    )
    .expect("remove");
    let output = run_load("proj", &["--json"], Some(temp.path()));
    assert_ne!(output.status.code(), Some(0));
}

#[test]
fn lone_cr_documents_are_rejected_as_encoding_defects() {
    // Committed fixtures are LF-normalized by .gitattributes, so the
    // lone-CR input is materialized at runtime to keep exact bytes.
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    std::fs::create_dir_all(project.join("lekalo")).expect("layout");
    std::fs::write(
        project.join("lekalo").join("project.yaml"),
        "schema_version: \"1.0.0\"\rdefinitions:\r  - id: core\r    kind: project\r    version: 1\r",
    )
    .expect("lone-cr doc");
    let output = run_load("proj", &["--json"], Some(temp.path()));
    assert_exit(&output, 1);
    let text = stderr_text(&output);
    assert!(text.contains("loader.encoding"), "{text}");
    assert!(text.contains("lone-cr"), "{text}");
}

#[test]
fn hostile_megabyte_import_token_yields_bounded_deterministic_stderr() {
    // One million `z` bytes is a legal document under the size gates but an
    // illegal module ID; classification must stay exit 1 stderr while the
    // echoed token stops scaling the envelope.
    let token = "z".repeat(1_000_000);
    let module_doc = format!(
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: core\n    kind: module\n    version: 1\n    imports:\n      - \"{token}\"\n"
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    std::fs::create_dir_all(project.join("lekalo").join("modules").join("m")).expect("layout");
    std::fs::write(
        project.join("lekalo").join("project.yaml"),
        "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"j\",\"kind\":\"project\",\"version\":1}]}\n",
    )
    .expect("project doc");
    std::fs::write(
        project
            .join("lekalo")
            .join("modules")
            .join("m")
            .join("module.yaml"),
        module_doc,
    )
    .expect("module doc");

    let output = run_load("proj", &["--json"], Some(temp.path()));
    assert_exit(&output, 1);
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty on invalid"
    );
    let envelope: Value =
        serde_json::from_str(stderr_text(&output).trim()).expect("envelope parses");
    assert_eq!(envelope["status"].as_str(), Some("invalid"));
    assert_eq!(
        envelope["diagnostics"][0]["id"].as_str(),
        Some("loader.import-invalid")
    );
    // The echo is exactly the 64-scalar prefix plus the elision marker.
    let echoed = envelope["diagnostics"][0]["data"]["import"]
        .as_str()
        .expect("echoed import");
    let mut expected = "z".repeat(64);
    expected.push('…');
    assert_eq!(echoed, expected);
    // Small hard cap on the whole stderr envelope, hostile input or not.
    assert!(
        output.stderr.len() <= 1024,
        "stderr envelope is {} bytes; hard cap is 1024",
        output.stderr.len()
    );
    // Deterministic bytes across repeated runs.
    let again = run_load("proj", &["--json"], Some(temp.path()));
    assert_exit(&again, 1);
    assert_eq!(output.stderr, again.stderr);
    // Human mode keeps the one stable status line.
    let human = run_load("proj", &[], Some(temp.path()));
    assert_exit(&human, 1);
    assert!(
        stderr_text(&human).starts_with("invalid error [LEK-LOAD-"),
        "human names the loader rule: {}",
        stderr_text(&human)
    );
}

#[test]
fn short_import_tokens_stay_verbatim_and_useful_in_error_envelopes() {
    // Tokens within the echo bound must reach `data.import` byte-for-byte:
    // bounding hostile echoes must never degrade legitimate diagnostics.
    let cases: &[(&str, i32, bool, &str)] = &[
        // (fixture, exit, stderr?, expected `data.import` token)
        ("invalid-import-path-escape", 3, false, "../core"),
        ("invalid-import-grammar", 1, true, "Other"),
        ("invalid-import-duplicate", 1, true, "other"),
    ];
    for (name, exit, on_stderr, token) in cases {
        let output = run_load(&format!("{FIXTURE_ROOT}/{name}"), &["--json"], None);
        assert_exit(&output, *exit);
        let envelope_text = if *on_stderr {
            assert!(output.stdout.is_empty(), "{name}: stdout not empty");
            stderr_text(&output)
        } else {
            assert!(output.stderr.is_empty(), "{name}: stderr not empty");
            stdout_text(&output)
        };
        let envelope: Value = serde_json::from_str(envelope_text.trim()).expect("envelope parses");
        let echoed = envelope["diagnostics"][0]["data"]["import"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: data.import missing"));
        assert_eq!(echoed, *token, "{name}: token must stay verbatim");
        assert!(!echoed.contains('…'), "{name}: no elision for short tokens");
        assert!(
            envelope_text.len() <= 1024,
            "{name}: envelope is {} bytes",
            envelope_text.len()
        );
    }
}

#[test]
fn root_not_found_when_no_marker_exists() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = run_discovery(temp.path(), &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr_text(&output);
    assert!(text.contains("structure.root-not-found"), "{text}");
}
