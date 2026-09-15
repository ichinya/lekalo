//! Issue #38 CLI tests for `lekalo init --adopt`: the dry-run no-write
//! plan, the minimal canonical skeleton, the unchanged monorepo sources,
//! the no-overwrite conflict, the idempotent re-run, ambiguity resolution,
//! and the Laravel/Go/empty-repository detection fixtures.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// The accepted adoption fixtures (issue #38).
const NODE_MONOREPO: &str = "tests/fixtures/adopt/node-monorepo";
const LARAVEL_APP: &str = "tests/fixtures/adopt/laravel-app";
const GO_MODULE: &str = "tests/fixtures/adopt/go-module";
const EMPTY_REPO: &str = "tests/fixtures/adopt/empty-repo";

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

fn stdout_json(output: &Output) -> Value {
    serde_json::from_str(stdout_text(output).trim()).expect("stdout json envelope")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(name)
}

/// Copy a fixture into a fresh alias-free temp directory; adopt runs on
/// the copy so the committed fixtures stay pristine evidence.
fn materialize(name: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    copy_tree(&fixture(name), &root);
    (temp, root)
}

/// Recursively copy `from` into `to` (regular files and directories only;
/// the fixtures carry no links).
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create copy root");
    for entry in fs::read_dir(from).expect("read fixture") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy fixture file");
        }
    }
}

/// The sorted `(relative path, bytes)` of every regular file below `root`,
/// skipping the canonical `lekalo/` tree when `skip_lekalo` is set.
fn snapshot(root: &Path, skip_lekalo: bool) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(root, root, skip_lekalo, &mut out);
    out.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    out
}

fn walk(root: &Path, dir: &Path, skip_lekalo: bool, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("walk directory") {
        let entry = entry.expect("walk entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if skip_lekalo && (name == "lekalo" || name == ".lekalo") {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .expect("walk below root")
            .to_string_lossy()
            .replace('\\', "/");
        if entry.file_type().expect("walk type").is_dir() {
            walk(root, &entry.path(), skip_lekalo, out);
        } else {
            out.push((relative, fs::read(entry.path()).expect("walk read")));
        }
    }
}

/// The Node.js monorepo dry-run plans every write, detects the workspace
/// with provenance, and changes nothing on disk.
#[test]
fn adopt_dry_run_plans_every_write_without_touching_the_tree() {
    let (_temp, root) = materialize(NODE_MONOREPO);
    let before = snapshot(&root, false);
    let output = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--adopt",
            "--target",
            "node-typescript",
            "--dry-run",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["operation"], "init");
    assert_eq!(envelope["mode"], "dry-run");
    assert_eq!(envelope["changed"], false);
    assert_eq!(envelope["projectId"], "acme_node_monorepo");
    assert_eq!(envelope["projectIdSource"]["source"], "package.json#name");
    assert_eq!(
        envelope["projectIdSource"]["original"],
        "acme-node-monorepo"
    );
    assert_eq!(envelope["target"], "node-typescript");
    let writes = envelope["writes"].as_array().expect("writes array");
    assert_eq!(writes.len(), 2);
    let paths: Vec<&str> = writes
        .iter()
        .map(|write| write["path"].as_str().expect("write path"))
        .collect();
    assert_eq!(
        paths,
        vec!["lekalo/project.yaml", "lekalo/targets/node-typescript.yaml"]
    );
    for write in writes {
        assert_eq!(write["action"], "create");
        assert_eq!(write["disposition"], "create");
    }
    let detection = &envelope["detection"];
    assert_eq!(detection["basis"], "workspace-root");
    let workspace_roots = detection["workspaceRoots"]
        .as_array()
        .expect("workspace roots");
    assert_eq!(workspace_roots.len(), 1);
    assert_eq!(workspace_roots[0]["path"], ".");
    assert_eq!(workspace_roots[0]["ecosystem"], "node");
    assert_eq!(workspace_roots[0]["members"][0], "apps/*");
    let modules = detection["modules"].as_array().expect("observed modules");
    let module_paths: Vec<&str> = modules
        .iter()
        .map(|module| module["path"].as_str().expect("module path"))
        .collect();
    assert_eq!(module_paths, vec!["apps/web", "packages/ui"]);
    for module in modules {
        assert_eq!(module["mode"], "observed");
        assert_eq!(module["confidence"], "high");
    }
    let layouts = detection["layouts"].as_array().expect("layouts");
    assert!(layouts
        .iter()
        .any(|layout| layout["kind"] == "openspec" && layout["confidence"] == "high"));
    let gates = detection["gates"].as_array().expect("gates");
    assert!(gates.iter().any(|gate| gate["gate"] == "build"
        && gate["source"] == "workflow"
        && gate["command"] == "npm run build"));
    assert!(detection["packageManagers"]
        .as_array()
        .expect("package managers")
        .iter()
        .any(|item| item["detail"] == "npm"));
    // Dry-run wrote nothing at all.
    assert_eq!(snapshot(&root, false), before);
}

/// The applied adoption writes only the minimal canonical skeleton, keeps
/// every `apps/**` and `packages/**` byte identical, and the skeleton
/// passes the normal loader and validator.
#[test]
fn adopt_creates_minimal_skeleton_and_preserves_the_monorepo() {
    let (_temp, root) = materialize(NODE_MONOREPO);
    let before = snapshot(&root, true);
    let output = lekalo_in(&root, &["--json", "init", "--adopt"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["mode"], "apply");
    assert_eq!(envelope["changed"], true);
    assert_eq!(envelope["created"], 1);
    assert_eq!(envelope["gate"]["status"], "valid");
    assert_eq!(envelope["gate"]["modelVersion"], "1.0.0");
    // The canonical skeleton is exactly the project document.
    let project_yaml = fs::read(root.join("lekalo").join("project.yaml")).expect("skeleton bytes");
    assert_eq!(
        project_yaml,
        b"{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"acme_node_monorepo\",\"kind\":\"project\",\"version\":1,\"description\":\"Adopted existing project.\"}]}\n"
    );
    // No `apps/**` or `packages/**` byte changed; only `lekalo/**` appeared.
    assert_eq!(snapshot(&root, true), before);
    // The written skeleton loads and validates through the normal path.
    let load = lekalo_in(&root, &["--json", "load"]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));
    let validate = lekalo_in(&root, &["--json", "validate"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
    assert_eq!(stdout_json(&validate)["status"], "valid");
}

/// A repeated init is idempotent: identical bytes are reported, no gate
/// runs, and the skeleton bytes stay identical.
#[test]
fn adopt_is_idempotent_on_a_second_run() {
    let (_temp, root) = materialize(NODE_MONOREPO);
    let first = lekalo_in(&root, &["init", "--adopt", "--target", "node-typescript"]);
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let skeleton = fs::read(root.join("lekalo").join("project.yaml")).expect("skeleton");
    let second = lekalo_in(
        &root,
        &["--json", "init", "--adopt", "--target", "node-typescript"],
    );
    assert_eq!(exit_code(&second), 0, "{}", stderr_text(&second));
    let envelope = stdout_json(&second);
    assert_eq!(envelope["mode"], "apply");
    assert_eq!(envelope["changed"], false);
    assert_eq!(envelope["created"], 0);
    assert_eq!(envelope["alreadyPresent"], 2);
    assert_eq!(envelope["gate"], Value::Null);
    assert_eq!(
        fs::read(root.join("lekalo").join("project.yaml")).expect("skeleton"),
        skeleton
    );
}

/// The explicit adapter profile (issue #38 target/profile selection):
/// the recorded selection persists in the opaque target document and the
/// receipt, an orphan or malformed profile is the stable usage failure
/// before any write, an identical re-run is idempotent, and a changed
/// selection never overwrites — it denies with `init.adopt-conflict`.
#[test]
fn adopt_records_the_explicit_adapter_profile() {
    let (_temp, root) = materialize(NODE_MONOREPO);
    // An orphan profile is refused before any plan or write.
    let orphan = lekalo_in(&root, &["init", "--adopt", "--profile", "default"]);
    assert_eq!(exit_code(&orphan), 1);
    assert!(stdout_text(&orphan).is_empty());
    assert!(stderr_text(&orphan).contains("cli.usage"));
    assert!(!root.join("lekalo").exists(), "nothing is written");
    // A malformed profile token never reaches the core.
    let malformed = lekalo_in(
        &root,
        &[
            "init",
            "--adopt",
            "--target",
            "node-typescript",
            "--profile",
            "Default_Profile",
        ],
    );
    assert_eq!(exit_code(&malformed), 1);
    assert!(stderr_text(&malformed).contains("cli.usage"));
    assert!(!root.join("lekalo").exists(), "nothing is written");
    // The valid selection is recorded and persists.
    let document = "{\"target\":\"node-typescript\",\"profile\":\"default\",\"note\":\"Adopted target selection.\"}\n";
    let applied = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--adopt",
            "--target",
            "node-typescript",
            "--profile",
            "default",
        ],
    );
    assert_eq!(exit_code(&applied), 0, "{}", stderr_text(&applied));
    let envelope = stdout_json(&applied);
    assert_eq!(envelope["target"], "node-typescript");
    assert_eq!(envelope["adapterProfile"], "default");
    assert_eq!(
        envelope["gate"]["profile"], "default",
        "the gate profile stays the validator profile"
    );
    assert_eq!(
        fs::read_to_string(
            root.join("lekalo")
                .join("targets")
                .join("node-typescript.yaml")
        )
        .expect("target document"),
        document
    );
    // Dry-run over the adopted tree plans nothing and writes nothing.
    let dry = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--adopt",
            "--target",
            "node-typescript",
            "--profile",
            "default",
            "--dry-run",
        ],
    );
    assert_eq!(exit_code(&dry), 0, "{}", stderr_text(&dry));
    let dry_envelope = stdout_json(&dry);
    assert_eq!(dry_envelope["mode"], "dry-run");
    assert_eq!(dry_envelope["adapterProfile"], "default");
    assert_eq!(dry_envelope["writes"][0]["disposition"], "already-present");
    assert_eq!(
        fs::read_to_string(
            root.join("lekalo")
                .join("targets")
                .join("node-typescript.yaml")
        )
        .expect("target document"),
        document
    );
    // An identical re-run is idempotent.
    let repeat = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--adopt",
            "--target",
            "node-typescript",
            "--profile",
            "default",
        ],
    );
    assert_eq!(exit_code(&repeat), 0, "{}", stderr_text(&repeat));
    assert_eq!(stdout_json(&repeat)["changed"], false);
    assert_eq!(
        fs::read_to_string(
            root.join("lekalo")
                .join("targets")
                .join("node-typescript.yaml")
        )
        .expect("target document"),
        document
    );
    // A changed selection is different bytes: denied, never overwritten.
    let changed = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--adopt",
            "--target",
            "node-typescript",
            "--profile",
            "strict",
        ],
    );
    assert_eq!(exit_code(&changed), 3);
    assert_eq!(
        stdout_json(&changed)["reasonCodes"][0],
        "init.adopt-conflict"
    );
    assert_eq!(
        fs::read_to_string(
            root.join("lekalo")
                .join("targets")
                .join("node-typescript.yaml")
        )
        .expect("target document survives"),
        document
    );
}

/// An existing file with different content denies the whole adoption with
/// `init.adopt-conflict` and the conflicting bytes survive untouched.
#[test]
fn adopt_refuses_to_overwrite_existing_files() {
    let (_temp, root) = materialize(NODE_MONOREPO);
    let first = lekalo_in(&root, &["init", "--adopt"]);
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    fs::write(
        root.join("lekalo").join("project.yaml"),
        "# user-edited project\n",
    )
    .expect("tamper skeleton");
    let second = lekalo_in(&root, &["--json", "init", "--adopt"]);
    assert_eq!(exit_code(&second), 3);
    let envelope = stdout_json(&second);
    assert_eq!(envelope["status"], "denied");
    assert_eq!(envelope["reasonCodes"][0], "init.adopt-conflict");
    let denied = envelope["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .any(|diagnostic| diagnostic["data"]["path"] == "lekalo/project.yaml");
    assert!(denied, "the conflicting path is reported");
    assert_eq!(
        fs::read(root.join("lekalo").join("project.yaml")).expect("tampered bytes"),
        b"# user-edited project\n"
    );
}

/// Ambiguous monorepo roots demand explicit resolution via `--project`.
#[test]
fn adopt_ambiguous_roots_require_explicit_resolution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    fs::create_dir_all(root.join("apps").join("inner")).expect("nested dirs");
    fs::write(
        root.join("package.json"),
        "{\"name\":\"root\",\"workspaces\":[\"apps/*\"]}\n",
    )
    .expect("root workspace");
    fs::write(
        root.join("apps").join("inner").join("Cargo.toml"),
        "[workspace]\nmembers = [\"c\"]\n",
    )
    .expect("nested workspace");
    // Two workspace roots on the ancestor chain refuse without a choice.
    let nested = root.join("apps").join("inner");
    let refused = lekalo_in(&nested, &["--json", "init", "--adopt", "--dry-run"]);
    assert_eq!(exit_code(&refused), 3);
    let envelope = stdout_json(&refused);
    assert_eq!(envelope["status"], "denied");
    assert_eq!(envelope["reasonCodes"][0], "init.adopt-ambiguous-root");
    // An explicit root selector resolves the ambiguity.
    let explicit = lekalo_in(
        &nested,
        &["--json", "init", "--adopt", "--project", ".", "--dry-run"],
    );
    assert_eq!(exit_code(&explicit), 0, "{}", stderr_text(&explicit));
    assert_eq!(stdout_json(&explicit)["detection"]["basis"], "explicit");
}
/// A malformed target id never reaches the core: the stable usage
/// failure before any plan or write (greenfield `init` without
/// `--adopt` is issue #97's bootstrap surface, covered in
/// `bootstrap.rs`).
#[test]
fn init_usage_failures_stay_stable() {
    let (_temp, root) = materialize(NODE_MONOREPO);
    let bad_target = lekalo_in(&root, &["init", "--adopt", "--target", "Bad_Target"]);
    assert!(!root.join("lekalo").exists(), "nothing is written");
    assert_eq!(exit_code(&bad_target), 1);
    assert!(stdout_text(&bad_target).is_empty());
    assert!(stderr_text(&bad_target).contains("cli.usage"));
}

/// The Laravel fixture: composer framework hints, the artisan gate
/// proposal, and the composer script workflow gate.
#[test]
fn adopt_detects_the_laravel_layout() {
    let (_temp, root) = materialize(LARAVEL_APP);
    let output = lekalo_in(&root, &["--json", "init", "--adopt", "--dry-run"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["projectId"], "acme_shop");
    assert_eq!(envelope["projectIdSource"]["source"], "composer.json#name");
    assert_eq!(envelope["projectIdSource"]["original"], "acme/shop");
    let detection = &envelope["detection"];
    let languages = detection["languages"].as_array().expect("languages");
    assert!(languages.iter().any(|hint| hint["label"] == "laravel"
        && hint["kind"] == "framework"
        && hint["confidence"] == "high"));
    let gates = detection["gates"].as_array().expect("gates");
    assert!(gates
        .iter()
        .any(|gate| gate["gate"] == "test" && gate["command"] == "composer test"));
    assert!(gates
        .iter()
        .any(|gate| gate["command"] == "php artisan test" && gate["source"] == "derived"));
    // The write plan is the single project document.
    assert_eq!(envelope["writes"].as_array().expect("writes").len(), 1);
}

/// The Go fixture: module-path id derivation and the derived go gates.
#[test]
fn adopt_detects_the_go_module() {
    let (_temp, root) = materialize(GO_MODULE);
    let output = lekalo_in(&root, &["--json", "init", "--adopt", "--dry-run"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["projectId"], "ledger");
    assert_eq!(envelope["projectIdSource"]["source"], "go.mod#module");
    let detection = &envelope["detection"];
    assert!(detection["languages"]
        .as_array()
        .expect("languages")
        .iter()
        .any(|hint| hint["label"] == "go" && hint["confidence"] == "high"));
    let gates = detection["gates"].as_array().expect("gates");
    assert!(gates
        .iter()
        .any(|gate| gate["command"] == "go test ./..." && gate["source"] == "derived"));
}

/// The empty repository: adoption falls back to the invocation directory,
/// derives the id from the directory name, and the skeleton validates.
#[test]
fn adopt_supports_an_empty_repository() {
    let (temp, root) = materialize(EMPTY_REPO);
    let output = lekalo_in(&root, &["--json", "init", "--adopt"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["detection"]["basis"], "invocation-directory");
    assert_eq!(envelope["projectId"], "repo");
    assert_eq!(envelope["projectIdSource"]["source"], "directory-name");
    assert_eq!(envelope["gate"]["modelVersion"], "1.0.0");
    let validate = lekalo_in(temp.path(), &["--json", "validate", "--project", "repo"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
    // The source directory name is never derived from a README-only repo.
    assert!(envelope["detection"]["manifests"]
        .as_array()
        .expect("manifests")
        .is_empty());
}
