//! Issue #38 CLI tests for `lekalo init --adopt`: the dry-run no-write
//! plan, the minimal canonical skeleton, the unchanged monorepo sources,
//! the no-overwrite conflict, the idempotent re-run, ambiguity resolution,
//! and adoption of an empty repository.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// The accepted adoption fixtures (issue #38).
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

/// A repeated init is idempotent: identical bytes are reported, no gate
/// runs, and the skeleton bytes stay identical.
#[test]
fn adopt_is_idempotent_on_a_second_run() {
    let (_temp, root) = materialize(EMPTY_REPO);
    let first = lekalo_in(&root, &["init", "--adopt", "--target", "fixture-adapter"]);
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let skeleton = fs::read(root.join("lekalo").join("project.yaml")).expect("skeleton");
    let second = lekalo_in(
        &root,
        &["--json", "init", "--adopt", "--target", "fixture-adapter"],
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
    let (_temp, root) = materialize(EMPTY_REPO);
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
            "fixture-adapter",
            "--profile",
            "Default_Profile",
        ],
    );
    assert_eq!(exit_code(&malformed), 1);
    assert!(stderr_text(&malformed).contains("cli.usage"));
    assert!(!root.join("lekalo").exists(), "nothing is written");
    // The valid selection is recorded and persists.
    let document = "{\"target\":\"fixture-adapter\",\"profile\":\"default\",\"note\":\"Adopted target selection.\"}\n";
    let applied = lekalo_in(
        &root,
        &[
            "--json",
            "init",
            "--adopt",
            "--target",
            "fixture-adapter",
            "--profile",
            "default",
        ],
    );
    assert_eq!(exit_code(&applied), 0, "{}", stderr_text(&applied));
    let envelope = stdout_json(&applied);
    assert_eq!(envelope["target"], "fixture-adapter");
    assert_eq!(envelope["adapterProfile"], "default");
    assert_eq!(
        envelope["gate"]["profile"], "default",
        "the gate profile stays the validator profile"
    );
    assert_eq!(
        fs::read_to_string(
            root.join("lekalo")
                .join("targets")
                .join("fixture-adapter.yaml")
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
            "fixture-adapter",
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
                .join("fixture-adapter.yaml")
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
            "fixture-adapter",
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
                .join("fixture-adapter.yaml")
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
            "fixture-adapter",
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
                .join("fixture-adapter.yaml")
        )
        .expect("target document survives"),
        document
    );
}

/// An existing file with different content denies the whole adoption with
/// `init.adopt-conflict` and the conflicting bytes survive untouched.
#[test]
fn adopt_refuses_to_overwrite_existing_files() {
    let (_temp, root) = materialize(EMPTY_REPO);
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
    let (_temp, root) = materialize(EMPTY_REPO);
    let bad_target = lekalo_in(&root, &["init", "--adopt", "--target", "Bad_Target"]);
    assert!(!root.join("lekalo").exists(), "nothing is written");
    assert_eq!(exit_code(&bad_target), 1);
    assert!(stdout_text(&bad_target).is_empty());
    assert!(stderr_text(&bad_target).contains("cli.usage"));
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
    assert_eq!(envelope["gate"]["modelVersion"], "0.2.16");
    let validate = lekalo_in(temp.path(), &["--json", "validate", "--project", "repo"]);
    assert_eq!(exit_code(&validate), 0, "{}", stderr_text(&validate));
    // The source directory name is never derived from a README-only repo.
    assert!(envelope["detection"]["manifests"]
        .as_array()
        .expect("manifests")
        .is_empty());
}
