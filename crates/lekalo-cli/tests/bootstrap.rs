//! Issue #97 CLI tests for greenfield `lekalo init` and
//! `lekalo module new`: the dry-run machine-readable plan, the minimal
//! valid project, the frontend spelling, the recorded-only target
//! selection, the `.gitignore` merge, the opt-in editor hints, the
//! idempotent re-run with unchanged/added/conflicting classes, the
//! no-overwrite denial, and the module-creation surface.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// One alias-free empty directory.
fn empty_greenfield() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = alias_free_path(temp.path());
    (temp, root)
}

/// A directory whose name sanitizes to nothing: id derivation fails
/// without an explicit `--project-id`.
fn unsanitizable_greenfield() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let marked = temp.path().join("!!!");
    fs::create_dir(&marked).expect("marker dir");
    let root = alias_free_path(&marked);
    (temp, root)
}

/// The selection policy denies alias-spelled working directories
/// (`structure.selection-alias`) before any command logic runs; chdir
/// the child into the resolved spelling, stripped of the `\\?\`
/// verbatim prefix `canonicalize` produces on Windows.
fn alias_free_path(path: &Path) -> PathBuf {
    let resolved = path.canonicalize().expect("canonicalize the temp spelling");
    let text = resolved.to_string_lossy().into_owned();
    let stripped = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
    PathBuf::from(stripped)
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .output()
        .expect("run the real lekalo binary")
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

fn stdout_json(output: &Output) -> Value {
    serde_json::from_str(stdout_text(output).trim()).expect("stdout json envelope")
}

/// Every error envelope carries the stable reason codes the tests
/// assert; JSON output keeps them on stdout for the `denied` class and
/// stderr for the `invalid` class.
fn reason_codes(output: &Output, stream: &str) -> Vec<String> {
    let text = match stream {
        "stdout" => stdout_text(output),
        _ => stderr_text(output),
    };
    let value: Value = serde_json::from_str(text.trim()).expect("json envelope");
    value["reasonCodes"]
        .as_array()
        .expect("reason codes array")
        .iter()
        .map(|code| code.as_str().expect("code text").to_owned())
        .collect()
}
/// The dry-run plans every artifact, records the template versions,
/// and writes nothing at all.
#[test]
fn greenfield_dry_run_plans_every_artifact_without_writing() {
    let (_temp, root) = empty_greenfield();
    let output = lekalo_in(&root, &["--json", "init", "--dry-run"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["operation"], "init");
    assert_eq!(envelope["mode"], "dry-run");
    assert_eq!(envelope["changed"], false);
    assert_eq!(envelope["rootBasis"], "invocation-directory");
    assert_eq!(envelope["module"], "app");
    assert_eq!(envelope["frontend"], "yaml");
    let writes = envelope["writes"].as_array().expect("writes array");
    let paths: Vec<&str> = writes
        .iter()
        .map(|write| write["path"].as_str().expect("write path"))
        .collect();
    assert_eq!(
        paths,
        vec![
            "lekalo/project.yaml",
            "lekalo/modules/app/module.yaml",
            ".gitignore"
        ]
    );
    for write in writes {
        assert_eq!(write["action"], "create");
        assert_eq!(write["disposition"], "create");
    }
    let templates = envelope["templates"].as_array().expect("templates");
    assert_eq!(templates.len(), 3);
    assert!(templates
        .iter()
        .all(|record| record["version"] == env!("CARGO_PKG_VERSION")));
    // Nothing was written.
    assert!(!root.join("lekalo").exists());
    assert!(!root.join(".gitignore").exists());
}

/// The applied bootstrap turns an empty repository into a valid minimal
/// Lekalo project: the canonical skeleton bytes, the managed ignore
/// line, and a tree that loads, validates, locks, and doctors.
#[test]
fn greenfield_bootstrap_creates_a_valid_minimal_project() {
    let (_temp, root) = empty_greenfield();
    let output = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["mode"], "apply");
    assert_eq!(envelope["changed"], true);
    assert_eq!(envelope["added"], 3);
    assert_eq!(envelope["unchanged"], 0);
    assert_eq!(envelope["conflicting"], 0);
    assert_eq!(envelope["gate"]["status"], "valid");
    assert_eq!(envelope["gate"]["modelVersion"], "1.0.0");
    assert_eq!(envelope["projectIdSource"]["source"], "directory-name");
    let project_id = envelope["projectId"]
        .as_str()
        .expect("project id")
        .to_owned();
    assert_eq!(
        fs::read_to_string(root.join("lekalo").join("project.yaml")).expect("project bytes"),
        format!(
            "schema_version: \"1.0.0\"\ndefinitions:\n  - id: {project_id}\n    kind: project\n    version: 1\n    description: \"New Lekalo project.\"\n"
        )
    );
    assert_eq!(
        fs::read_to_string(root.join("lekalo").join("modules").join("app").join("module.yaml"))
            .expect("module bytes"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: app\n    kind: module\n    version: 1\n    description: \"Initial module.\"\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect("ignore bytes"),
        "/.lekalo/\n"
    );
    // No generated application code and no lock: bootstrap writes only
    // the skeleton.
    assert!(!root.join("lekalo.lock").exists());
    let files: Vec<String> = walk(&root);
    assert_eq!(
        files,
        vec![
            ".gitignore".to_owned(),
            "lekalo/modules/app/module.yaml".to_owned(),
            "lekalo/project.yaml".to_owned(),
        ]
    );
    // The bootstrapped tree passes the normal surfaces.
    let load = lekalo_in(&root, &["--json", "load"]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
    assert_eq!(stdout_json(&validate)["status"], "valid");
    let doctor = lekalo_in(&root, &["--json", "doctor"]);
    assert_eq!(exit_code(&doctor), 0, "{}", stderr_text(&doctor));
    let report = stdout_json(&doctor);
    assert_eq!(report["status"], "valid");
    assert!(report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["id"] == "project.root" && check["state"] == "ok"));
    // Explicit resolution creates the committed lock on the bootstrapped
    // tree (the lock is never a bootstrap artifact).
    let lock = lekalo_in(&root, &["--json", "lock"]);
    assert_eq!(exit_code(&lock), 0, "{}", stderr_text(&lock));
    assert!(root.join("lekalo.lock").is_file());
}

/// A repeated bootstrap is idempotent: every artifact is reported
/// unchanged, nothing is written, and no gate runs.
#[test]
fn greenfield_re_run_reports_every_artifact_unchanged() {
    let (_temp, root) = empty_greenfield();
    let first = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let before = walk_with_bytes(&root);
    let second = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&second), 0, "{}", stderr_text(&second));
    let envelope = stdout_json(&second);
    assert_eq!(envelope["mode"], "apply");
    assert_eq!(envelope["changed"], false);
    assert_eq!(envelope["added"], 0);
    assert_eq!(envelope["unchanged"], 3);
    assert_eq!(envelope["conflicting"], 0);
    assert!(envelope["gate"].is_null());
    for write in envelope["writes"].as_array().expect("writes") {
        assert_eq!(write["disposition"], "unchanged");
    }
    assert_eq!(walk_with_bytes(&root), before);
}

/// A user-modified artifact is a conflicting file: the re-run is denied
/// with `init.bootstrap-conflict` and the conflicting bytes survive.
#[test]
fn greenfield_conflicting_files_are_denied_never_overwritten() {
    let (_temp, root) = empty_greenfield();
    let first = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let user_bytes = b"schema_version: \"1.0.0\"\ndefinitions:\n  - id: probe\n    kind: project\n    version: 2\n    description: \"User-owned.\"\n".to_vec();
    fs::write(root.join("lekalo").join("project.yaml"), &user_bytes).expect("user edit");
    let rerun = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&rerun), 3);
    assert_eq!(
        reason_codes(&rerun, "stdout"),
        vec!["init.bootstrap-conflict".to_owned()]
    );
    assert_eq!(
        fs::read(root.join("lekalo").join("project.yaml")).expect("survives"),
        user_bytes
    );
}

/// The `.gitignore` addition merges: user bytes stay a prefix, both
/// managed spellings count as present, and the no-newline case gets
/// exactly one separator.
#[test]
fn greenfield_gitignore_merges_user_bytes() {
    let (_temp, root) = empty_greenfield();
    fs::write(root.join(".gitignore"), "/target\n").expect("seed");
    let output = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect("merged"),
        "/target\n/.lekalo/\n"
    );
    let envelope = stdout_json(&output);
    let entry = envelope["writes"]
        .as_array()
        .expect("writes")
        .iter()
        .find(|write| write["path"] == ".gitignore")
        .expect("ignore entry");
    assert_eq!(entry["action"], "append");
    assert_eq!(entry["disposition"], "append");
    // Re-run: unchanged, no duplicate line.
    let rerun = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&rerun), 0, "{}", stderr_text(&rerun));
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect("stable"),
        "/target\n/.lekalo/\n"
    );
    // A user-authored unrooted spelling counts as present too.
    let (_temp, other) = empty_greenfield();
    fs::write(other.join(".gitignore"), "node_modules\n.lekalo/\n").expect("seed");
    let unrooted = lekalo_in(&other, &["--json", "init"]);
    assert_eq!(exit_code(&unrooted), 0, "{}", stderr_text(&unrooted));
    assert_eq!(
        fs::read_to_string(other.join(".gitignore")).expect("untouched"),
        "node_modules\n.lekalo/\n"
    );
    // The missing trailing newline gets exactly one separator.
    let (_temp, tight) = empty_greenfield();
    fs::write(tight.join(".gitignore"), "/dist").expect("seed");
    let separated = lekalo_in(&tight, &["--json", "init"]);
    assert_eq!(exit_code(&separated), 0, "{}", stderr_text(&separated));
    assert_eq!(
        fs::read_to_string(tight.join(".gitignore")).expect("separated"),
        "/dist\n/.lekalo/\n"
    );
}

/// An explicit target selection is recorded in the target document and
/// the receipt and resolves nothing: no adapter is probed, executed,
/// or installed, and the project still validates without one.
#[test]
fn greenfield_target_selection_is_recorded_only() {
    let (_temp, root) = empty_greenfield();
    let output = lekalo_in(&root, &["--json", "init", "--target", "node-typescript"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["target"], "node-typescript");
    assert!(envelope["adapterProfile"].is_null());
    assert_eq!(
        fs::read_to_string(
            root.join("lekalo")
                .join("targets")
                .join("node-typescript.yaml")
        )
        .expect("target bytes"),
        "{\"target\":\"node-typescript\",\"note\":\"Adopted target selection.\"}\n"
    );
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
    assert_eq!(stdout_json(&validate)["status"], "valid");
    // The profile rides along in the same opaque document.
    let (_temp, other) = empty_greenfield();
    let profiled = lekalo_in(
        &other,
        &[
            "--json",
            "init",
            "--target",
            "node-typescript",
            "--profile",
            "default",
        ],
    );
    assert_eq!(exit_code(&profiled), 0, "{}", stderr_text(&profiled));
    assert_eq!(stdout_json(&profiled)["adapterProfile"], "default");
    assert_eq!(
        fs::read_to_string(other.join("lekalo").join("targets").join("node-typescript.yaml"))
            .expect("target bytes"),
        "{\"target\":\"node-typescript\",\"profile\":\"default\",\"note\":\"Adopted target selection.\"}\n"
    );
}

/// The JSON frontend writes the compact adoption spelling into the same
/// homes and still loads and validates.
#[test]
fn greenfield_json_frontend_uses_the_adoption_spelling() {
    let (_temp, root) = empty_greenfield();
    let output = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--frontend",
            "json",
            "--project-id",
            "probe",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert_eq!(stdout_json(&output)["frontend"], "json");
    assert_eq!(
        fs::read_to_string(root.join("lekalo").join("project.yaml")).expect("project bytes"),
        "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"probe\",\"kind\":\"project\",\"version\":1,\"description\":\"New Lekalo project.\"}]}\n"
    );
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
}

/// The editor hints are opt-in, byte-stable, and no-overwrite: a
/// differing pre-existing settings file denies the whole bootstrap
/// before any write.
#[test]
fn greenfield_editor_hints_are_opt_in() {
    let (_temp, root) = empty_greenfield();
    let plain = lekalo_in(&root, &["--json", "init", "--dry-run"]);
    assert!(!stdout_json(&plain)["writes"]
        .as_array()
        .expect("writes")
        .iter()
        .any(|write| write["path"] == ".vscode/settings.json"));
    let hinted = lekalo_in(&root, &["--json", "init", "--editor-hints"]);
    assert_eq!(exit_code(&hinted), 0, "{}", stderr_text(&hinted));
    assert_eq!(stdout_json(&hinted)["editorHints"], true);
    assert_eq!(
        fs::read_to_string(root.join(".vscode").join("settings.json")).expect("hints"),
        concat!(
            "{\n",
            "  \"yaml.schemas\": {\n",
            "    \"https://lekalo.dev/schemas/model/1.0.0/schema.json\": [\n",
            "      \"lekalo/project.yaml\",\n",
            "      \"lekalo/modules/**/*.yaml\"\n",
            "    ]\n",
            "  }\n",
            "}\n"
        )
    );
    let (_temp, other) = empty_greenfield();
    fs::create_dir_all(other.join(".vscode")).expect("vscode dir");
    fs::write(other.join(".vscode").join("settings.json"), b"{\n}\n").expect("user settings");
    let conflicting = lekalo_in(&other, &["--json", "init", "--editor-hints"]);
    assert_eq!(exit_code(&conflicting), 3);
    assert_eq!(
        reason_codes(&conflicting, "stdout"),
        vec!["init.bootstrap-conflict".to_owned()]
    );
    assert_eq!(
        fs::read(other.join(".vscode").join("settings.json")).expect("survives"),
        b"{\n}\n".to_vec()
    );
    assert!(!other.join("lekalo").exists(), "denied before any write");
}

/// The explicit project id wins over the directory name, an
/// underivable id without one is the stable denial, and malformed
/// flags never reach the core.
#[test]
fn greenfield_project_id_rules_and_usage_failures() {
    let (_temp, root) = empty_greenfield();
    let explicit = lekalo_in(&root, &["--json", "init", "--project-id", "probe"]);
    assert_eq!(exit_code(&explicit), 0, "{}", stderr_text(&explicit));
    let envelope = stdout_json(&explicit);
    assert_eq!(envelope["projectId"], "probe");
    assert_eq!(envelope["projectIdSource"]["source"], "--project-id");
    let (_temp, unnamed) = unsanitizable_greenfield();
    let denied = lekalo_in(&unnamed, &["--json", "init"]);
    assert_eq!(exit_code(&denied), 3);
    assert_eq!(
        reason_codes(&denied, "stdout"),
        vec!["init.bootstrap-id-required".to_owned()]
    );
    assert!(!unnamed.join("lekalo").exists(), "nothing is written");
    for args in [
        vec!["--json", "init", "--module", "Bad_Id"],
        vec!["--json", "init", "--project-id", "Bad_Id"],
        vec!["--json", "init", "--target", "Bad_Target"],
        vec!["--json", "init", "--profile", "default"],
    ] {
        let usage = lekalo_in(&root, &args);
        assert_eq!(exit_code(&usage), 1, "{args:?}");
        assert!(stderr_text(&usage).contains("cli.usage"), "{args:?}");
    }
}

/// Unrelated user files survive a bootstrap byte-identical.
#[test]
fn greenfield_preserves_unrelated_files() {
    let (_temp, root) = empty_greenfield();
    fs::write(root.join("README.md"), b"# user repo\n").expect("readme");
    fs::create_dir_all(root.join("src")).expect("src");
    fs::write(root.join("src").join("main.ts"), b"console.log(1);\n").expect("source");
    let output = lekalo_in(&root, &["--json", "init"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert_eq!(
        fs::read(root.join("README.md")).expect("readme"),
        b"# user repo\n".to_vec()
    );
    assert_eq!(
        fs::read(root.join("src").join("main.ts")).expect("source"),
        b"console.log(1);\n".to_vec()
    );
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
}

/// `lekalo module new` creates one additional empty module behind the
/// same no-overwrite, dry-run, gate, and idempotence semantics.
#[test]
fn module_new_creates_gates_and_revalidates() {
    let (_temp, root) = empty_greenfield();
    let init = lekalo_in(&root, &["--json", "init", "--project-id", "probe"]);
    assert_eq!(exit_code(&init), 0, "{}", stderr_text(&init));
    let dry = lekalo_in(&root, &["--json", "module", "new", "planner", "--dry-run"]);
    assert_eq!(exit_code(&dry), 0, "{}", stderr_text(&dry));
    let dry_envelope = stdout_json(&dry);
    assert_eq!(dry_envelope["operation"], "module");
    assert_eq!(dry_envelope["mode"], "dry-run");
    assert_eq!(dry_envelope["moduleId"], "planner");
    assert!(!root.join("lekalo").join("modules").join("planner").exists());
    let applied = lekalo_in(&root, &["--json", "module", "new", "planner"]);
    assert_eq!(exit_code(&applied), 0, "{}", stderr_text(&applied));
    let envelope = stdout_json(&applied);
    assert_eq!(envelope["changed"], true);
    assert_eq!(envelope["added"], 1);
    assert_eq!(envelope["gate"]["status"], "valid");
    assert_eq!(
        fs::read_to_string(root.join("lekalo").join("modules").join("planner").join("module.yaml"))
            .expect("module bytes"),
        "schema_version: \"1.0.0\"\ndefinitions:\n  - id: planner\n    kind: module\n    version: 1\n    description: \"Initial module.\"\n"
    );
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
    // Idempotent re-run.
    let rerun = lekalo_in(&root, &["--json", "module", "new", "planner"]);
    assert_eq!(exit_code(&rerun), 0, "{}", stderr_text(&rerun));
    let rerun_envelope = stdout_json(&rerun);
    assert_eq!(rerun_envelope["changed"], false);
    assert_eq!(rerun_envelope["unchanged"], 1);
    // A user-modified manifest conflicts and survives.
    fs::write(
        root.join("lekalo")
            .join("modules")
            .join("planner")
            .join("module.yaml"),
        b"schema_version: \"1.0.0\"\ndefinitions:\n  - id: planner\n  kind: module\n  version: 3\n",
    )
    .expect("user edit");
    let conflict = lekalo_in(&root, &["--json", "module", "new", "planner"]);
    assert_eq!(exit_code(&conflict), 3);
    assert_eq!(
        reason_codes(&conflict, "stdout"),
        vec!["init.bootstrap-conflict".to_owned()]
    );
    assert!(root
        .join("lekalo")
        .join("modules")
        .join("planner")
        .join("module.yaml")
        .is_file());
}

/// `module new` outside a project is the normal root-not-found
/// failure, a malformed id is the stable usage failure, and the
/// JSON frontend writes the adoption spelling.
#[test]
fn module_new_failures_and_frontend() {
    let (_temp, root) = empty_greenfield();
    let outside = lekalo_in(&root, &["--json", "module", "new", "planner"]);
    assert_eq!(exit_code(&outside), 1);
    assert!(stderr_text(&outside).contains("structure.root-not-found"));
    let usage = lekalo_in(&root, &["--json", "module", "new", "Bad_Id"]);
    assert_eq!(exit_code(&usage), 1);
    assert!(stderr_text(&usage).contains("cli.usage"));
    let init = lekalo_in(&root, &["--json", "init", "--project-id", "probe"]);
    assert_eq!(exit_code(&init), 0, "{}", stderr_text(&init));
    let json = lekalo_in(
        &root,
        &["--json", "module", "new", "extra", "--frontend", "json"],
    );
    assert_eq!(exit_code(&json), 0, "{}", stderr_text(&json));
    assert_eq!(
        fs::read_to_string(root.join("lekalo").join("modules").join("extra").join("module.yaml"))
            .expect("module bytes"),
        "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"extra\",\"kind\":\"module\",\"version\":1,\"description\":\"Initial module.\"}]}\n"
    );
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
}

/// The sorted logical file list below `root`, excluding nothing: the
/// bootstrap writes a small closed set.
fn walk(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk_inner(root, root, &mut files);
    files.sort();
    files
}

/// The sorted `(logical path, bytes)` inventory of the whole tree.
fn walk_with_bytes(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    walk_bytes_inner(root, root, &mut files);
    files.sort();
    files
}

fn walk_inner(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            walk_inner(root, &path, out);
        } else {
            out.push(
                path.strip_prefix(root)
                    .expect("below root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn walk_bytes_inner(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            walk_bytes_inner(root, &path, out);
        } else {
            out.push((
                path.strip_prefix(root)
                    .expect("below root")
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(&path).expect("bytes"),
            ));
        }
    }
}
