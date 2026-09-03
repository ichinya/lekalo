//! Issue #9 CLI regression: the real `lekalo migrate` binary against
//! hermetic fixture copies. Exit codes, streams, and envelope bytes are
//! exact; source trees are byte-compared against the accepted #6 golden
//! pair. Sandboxes live in invocation-relative scratch directories because
//! the accepted #3 selection grammar denies absolute selectors.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// The versioning fixture root.
const FIXTURE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/versioning"
);

static NEXT_SANDBOX: AtomicU32 = AtomicU32::new(0);

struct Sandbox {
    /// Invocation-relative selector (never absolute).
    selector: String,
    root: PathBuf,
}

impl Sandbox {
    /// Copy a fixture tree into a fresh relative scratch directory so
    /// tests never mutate repository bytes.
    fn from_fixture(name: &str) -> Self {
        let id = NEXT_SANDBOX.fetch_add(1, Ordering::SeqCst);
        let selector = format!("test-scratch/m9-{}-{}/project", std::process::id(), id);
        let root = PathBuf::from(&selector);
        let source = Path::new(FIXTURE_ROOT).join("migration").join(name);
        copy_tree(&source, &root);
        Self { selector, root }
    }

    fn project(&self) -> (&str, &str) {
        ("--project", &self.selector)
    }

    /// The byte content of one document relative to the project root.
    fn read(&self, relative: &str) -> Vec<u8> {
        fs::read(self.root.join(relative)).expect("document exists")
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        fs::write(self.root.join(relative), bytes).expect("document writable");
    }

    /// Whether any runtime migration state exists.
    fn has_runtime_state(&self) -> bool {
        self.root.join(".lekalo/cache/migrations").exists()
    }

    /// Plan home directory names (excluding the lock).
    fn plan_ids(&self) -> Vec<String> {
        let home = self.root.join(".lekalo/cache/migrations");
        let mut names: Vec<String> = fs::read_dir(&home)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name != "active.lock")
            .collect();
        names.sort();
        names
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Remove only this sandbox's tree and its (now likely empty)
        // uniquely-named parent; tests run in parallel, so the shared
        // scratch root must stay.
        let _ = fs::remove_dir_all(&self.root);
        if let Some(parent) = self.root.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("target dir");
    for entry in fs::read_dir(source).expect("source dir") {
        let entry = entry.expect("entry");
        let kind = entry.file_type().expect("entry kind");
        let target_path = target.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target_path);
        } else {
            fs::copy(entry.path(), &target_path).expect("copy file");
        }
    }
}
fn lekalo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .output()
        .expect("run the real lekalo binary")
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Assert one invalid failure in both renderings: plain mode carries the
/// human line on stderr; `--json` (either position) carries the exact
/// pretty envelope.
fn assert_invalid_everywhere(args: &[&str], code: &str, exit: i32) {
    let plain = lekalo(args);
    assert_exit(&plain, exit);
    assert!(
        plain.stdout.is_empty(),
        "plain stdout: {:?}",
        stdout_text(&plain)
    );
    assert_eq!(stderr_text(&plain), format!("invalid: {code}\n"));

    let mut before = vec!["--json"];
    before.extend_from_slice(args);
    let json_before = lekalo(&before);
    assert_exit(&json_before, exit);
    assert!(json_before.stdout.is_empty());
    assert_eq!(stderr_text(&json_before), invalid_envelope(code));

    let mut after = args.to_vec();
    after.push("--json");
    let json_after = lekalo(&after);
    assert_exit(&json_after, exit);
    assert!(json_after.stdout.is_empty());
    assert_eq!(stderr_text(&json_after), invalid_envelope(code));
}

/// The exact unsupported-version failure in both renderings.
fn assert_unsupported_everywhere(args: &[&str]) {
    let plain = lekalo(args);
    assert_exit(&plain, 5);
    assert!(plain.stdout.is_empty());
    assert_eq!(
        stderr_text(&plain),
        "unsupported-version: versioning.unsupported-version\n"
    );

    let mut before = vec!["--json"];
    before.extend_from_slice(args);
    let json_before = lekalo(&before);
    assert_exit(&json_before, 5);
    assert!(json_before.stdout.is_empty());
    assert_eq!(
        stderr_text(&json_before),
        unsupported_envelope("versioning.unsupported-version")
    );
}

fn invalid_envelope(code: &str) -> String {
    format!("{{\n  \"status\": \"invalid\",\n  \"reasonCodes\": [\n    \"{code}\"\n  ]\n}}\n")
}

fn unsupported_envelope(code: &str) -> String {
    format!(
        "{{\n  \"status\": \"unsupported-version\",\n  \"reasonCodes\": [\n    \"{code}\"\n  ]\n}}\n"
    )
}

#[test]
fn unsupported_target_is_exit_five_on_stderr_with_zero_writes() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");
    for selector in ["model/2.0.0", "model/0.2.0", "model/2.0.0-rc.1"] {
        assert_unsupported_everywhere(&[
            "migrate",
            sandbox.project().0,
            sandbox.project().1,
            "--to",
            selector,
        ]);
    }
    assert!(!sandbox.has_runtime_state(), "no runtime state is created");
}

#[test]
fn malformed_selectors_are_exit_one_usage_class() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");
    for selector in [
        "model/1",
        "model/v1.0.0",
        "model/vv1",
        "model/01.0.0",
        "1.0.0",
        "model/",
        "model/1.0.0+meta",
    ] {
        assert_invalid_everywhere(
            &["migrate", "--project", &sandbox.selector, "--to", selector],
            "versioning.invalid-version",
            1,
        );
    }
    assert!(!sandbox.has_runtime_state());
}

#[test]
fn migrate_requires_exactly_one_action_and_dry_run_only_with_to() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");

    // Neither action.
    assert_invalid_everywhere(
        &["migrate", "--project", &sandbox.selector],
        "versioning.invalid-version",
        1,
    );

    // Both actions.
    assert_invalid_everywhere(
        &[
            "migrate",
            "--project",
            &sandbox.selector,
            "--to",
            "model/1.0.0",
            "--rollback",
            &"a".repeat(64),
        ],
        "versioning.invalid-version",
        1,
    );

    // Dry-run without --to.
    assert_invalid_everywhere(
        &["migrate", "--project", &sandbox.selector, "--dry-run"],
        "versioning.invalid-version",
        1,
    );
    assert!(!sandbox.has_runtime_state());
}

#[test]
fn dry_run_matches_the_golden_receipt_and_writes_nothing() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");

    let output = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/v1",
        "--dry-run",
        "--json",
    ]);
    assert_exit(&output, 0);
    assert!(output.stderr.is_empty());

    let golden = fs::read(format!("{FIXTURE_ROOT}/migration/dry-run.golden.json")).expect("golden");
    assert_eq!(
        stdout_text(&output).as_bytes(),
        golden.as_slice(),
        "receipt bytes are golden"
    );

    // The dry-run human line is one stable summary.
    let human = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/v1",
        "--dry-run",
    ]);
    let receipt: Value = serde_json::from_slice(&golden).expect("golden parses");
    let plan_id = receipt["planId"].as_str().expect("planId");
    assert_eq!(
        stdout_text(&human),
        format!("migration plan model 0.1.0 -> 1.0.0: 4 documents (plan {plan_id})\n")
    );

    // Zero writes: the tree is byte-identical and no runtime state exists.
    assert_tree_unchanged(&sandbox);
    assert!(!sandbox.has_runtime_state());
}

fn assert_tree_unchanged(sandbox: &Sandbox) {
    for relative in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/planner/queries.yaml",
    ] {
        let original = fs::read(format!("{FIXTURE_ROOT}/migration/golden-0.1.0/{relative}"))
            .expect("fixture document");
        assert_eq!(sandbox.read(relative), original, "{relative} untouched");
    }
}

#[test]
fn apply_matches_the_1_0_0_golden_and_is_idempotent() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");

    let output = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/1.0.0",
        "--json",
    ]);
    assert_exit(&output, 0);
    assert!(output.stderr.is_empty());

    let receipt: Value = serde_json::from_str(stdout_text(&output).trim()).expect("receipt");
    assert_eq!(receipt["status"], "valid");
    assert_eq!(receipt["operation"], "migrate");
    assert_eq!(receipt["mode"], "apply");
    assert_eq!(receipt["changed"], true);
    assert_eq!(receipt["transaction"], "committed");
    assert_eq!(receipt["from"], "0.1.0");
    assert_eq!(receipt["to"], "1.0.0");
    assert_eq!(receipt["semanticDiff"]["added"], Value::Array(Vec::new()));
    assert_eq!(receipt["semanticDiff"]["removed"], Value::Array(Vec::new()));
    assert_eq!(receipt["semanticDiff"]["changed"], Value::Array(Vec::new()));
    assert_eq!(
        receipt["semanticDiff"]["contractVersionChange"]["from"],
        "0.1.0"
    );
    assert_eq!(receipt["loss"], Value::Array(Vec::new()));
    let backup = &receipt["backup"];
    assert_eq!(backup["id"], receipt["planId"]);
    assert!(backup["manifestSha256"].is_string());

    // Every document now equals the accepted 1.0.0 twin byte for byte.
    for relative in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/planner/queries.yaml",
    ] {
        let twin = fs::read(format!(
            "{}/../../tests/fixtures/model-compat/1.0.0/{relative}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("twin document");
        assert_eq!(sandbox.read(relative), twin, "{relative}");
    }

    // The plan home keeps the immutable backups and manifest: no journal,
    // no lock.
    let plan_ids = sandbox.plan_ids();
    assert_eq!(plan_ids.len(), 1);
    let plan_id = receipt["planId"].as_str().expect("planId");
    assert_eq!(plan_ids[0], plan_id);
    assert!(!sandbox
        .root
        .join(".lekalo/cache/migrations/active.lock")
        .exists());
    assert!(!sandbox
        .root
        .join(format!(".lekalo/cache/migrations/{plan_id}/journal.json"))
        .exists());
    assert!(sandbox
        .root
        .join(format!(".lekalo/cache/migrations/{plan_id}/manifest.json"))
        .exists());
    assert!(sandbox
        .root
        .join(format!(
            ".lekalo/cache/migrations/{plan_id}/before/project.yaml"
        ))
        .exists());

    // A second migration to the same target is a deterministic no-op.
    let again = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/1.0.0",
        "--json",
    ]);
    assert_exit(&again, 0);
    let receipt: Value = serde_json::from_str(stdout_text(&again).trim()).expect("receipt");
    assert_eq!(receipt["changed"], false);
    assert_eq!(receipt["transaction"], "unchanged");
    assert_eq!(receipt["files"], Value::Array(Vec::new()));
    let unchanged_plan = receipt["planId"].as_str().expect("planId").to_owned();
    assert_eq!(
        stdout_text(&lekalo(&[
            "migrate",
            "--project",
            &sandbox.selector,
            "--to",
            "model/1.0.0"
        ])),
        format!("model already 1.0.0 (plan {unchanged_plan})\n")
    );
    let _ = plan_id;
}

#[test]
fn unsupported_version_through_load_ir_and_migrate_is_identical() {
    // The load boundary regression: unsupported 2.0.0 exits 5 with the
    // shared reason through every command, and no canonicalized model
    // appears.
    let sandbox = Sandbox::from_fixture("golden-0.1.0");
    for document in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/planner/queries.yaml",
    ] {
        let original = sandbox.read(document);
        let text = String::from_utf8(original).expect("utf8");
        let rewritten = text.replace("0.1.0", "2.0.0");
        sandbox.write(document, rewritten.as_bytes());
    }

    // `load` keeps the accepted #7 rich diagnostic envelope; `migrate`
    // keeps the #3 string-array convention. Both share the exit class
    // and stream: 5, stderr, stdout empty.
    let load = lekalo(&["load", "--project", &sandbox.selector]);
    assert_exit(&load, 5);
    assert!(load.stdout.is_empty());
    assert!(stderr_text(&load).contains("versioning.unsupported-version"));
    let load_json = lekalo(&["--json", "load", "--project", &sandbox.selector]);
    assert_exit(&load_json, 5);
    assert!(load_json.stdout.is_empty());
    assert!(stderr_text(&load_json).contains("versioning.unsupported-version"));

    let load_ir = lekalo(&["load", "--project", &sandbox.selector, "--ir"]);
    assert_exit(&load_ir, 5);
    assert!(load_ir.stdout.is_empty());

    // The unsupported SOURCE version fails inside the accepted loader
    // phase, so the envelope is the rich diagnostic one; the exit class
    // and stream are the shared exit-5 contract.
    let migrate = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/1.0.0",
    ]);
    assert_exit(&migrate, 5);
    assert!(migrate.stdout.is_empty());
    assert!(stderr_text(&migrate).contains("versioning.unsupported-version"));
    let migrate_json = lekalo(&[
        "--json",
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/1.0.0",
    ]);
    assert_exit(&migrate_json, 5);
    assert!(migrate_json.stdout.is_empty());
    assert!(stderr_text(&migrate_json).contains("versioning.unsupported-version"));

    // No partial write happened: the documents still say 2.0.0.
    assert!(String::from_utf8(sandbox.read("lekalo/project.yaml"))
        .expect("utf8")
        .contains("\"schema_version\": \"2.0.0\""));
}

#[test]
fn manual_precondition_refuses_with_exit_one_and_zero_writes() {
    let sandbox = Sandbox::from_fixture("manual-precondition");

    // The 0.1.0 hyphenated module ID cannot survive the 1.0.0 grammar.
    assert_invalid_everywhere(
        &[
            "migrate",
            "--project",
            &sandbox.selector,
            "--to",
            "model/1.0.0",
        ],
        "versioning.migration-precondition",
        1,
    );

    // Zero writes: every document still says 0.1.0.
    for relative in [
        "lekalo/project.yaml",
        "lekalo/modules/work-items/module.yaml",
    ] {
        assert!(sandbox
            .read(relative)
            .starts_with(b"schema_version: \"0.1.0\""));
    }
    assert!(!sandbox.has_runtime_state());
}

#[test]
fn rollback_restores_the_recorded_before_bytes() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");

    assert_exit(
        &lekalo(&[
            "migrate",
            "--project",
            &sandbox.selector,
            "--to",
            "model/1.0.0",
        ]),
        0,
    );
    let plan_id = sandbox.plan_ids()[0].clone();

    let human = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--rollback",
        &plan_id,
    ]);
    assert_exit(&human, 0);
    assert!(human.stderr.is_empty());
    assert_eq!(
        stdout_text(&human),
        format!("rolled back to model 0.1.0 (plan {plan_id})\n")
    );

    // The tree is byte-identical to the 0.1.0 fixture again.
    assert_tree_unchanged(&sandbox);

    // The project loads again at 0.1.0 and the runtime state is clean.
    let load = lekalo(&["load", "--project", &sandbox.selector]);
    assert_exit(&load, 0);
    assert_eq!(sandbox.plan_ids(), vec![plan_id]);
    assert!(!sandbox
        .root
        .join(".lekalo/cache/migrations/active.lock")
        .exists());
}

#[test]
fn rollback_after_user_edits_conflicts_with_zero_writes() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");

    assert_exit(
        &lekalo(&[
            "migrate",
            "--project",
            &sandbox.selector,
            "--to",
            "model/1.0.0",
        ]),
        0,
    );
    let plan_id = sandbox.plan_ids()[0].clone();

    // The owner edits a document after migrating.
    let document = "lekalo/modules/planner/entities.yaml";
    let mut bytes = sandbox.read(document);
    bytes.extend_from_slice(b"\n"); // whitespace-only edit: still valid, digest differs
    sandbox.write(document, &bytes);

    assert_invalid_everywhere(
        &[
            "migrate",
            "--project",
            &sandbox.selector,
            "--rollback",
            &plan_id,
        ],
        "versioning.rollback-conflict",
        1,
    );

    // The edited bytes are untouched: rollback refuses with zero writes.
    assert_eq!(sandbox.read(document), bytes);
}

#[test]
fn malformed_rollback_plan_id_is_exit_one() {
    let sandbox = Sandbox::from_fixture("golden-0.1.0");
    assert_invalid_everywhere(
        &[
            "migrate",
            "--project",
            &sandbox.selector,
            "--rollback",
            "not-a-plan",
        ],
        "versioning.invalid-version",
        1,
    );
    // A well-formed hex identity with no recorded plan is a rollback
    // failure, not a conflict: nothing exists to restore.
    assert_invalid_everywhere(
        &[
            "migrate",
            "--project",
            &sandbox.selector,
            "--rollback",
            &"0".repeat(64),
        ],
        "versioning.rollback-failed",
        1,
    );
}

#[test]
fn crashed_migration_fails_closed_then_recovers_on_next_migrate() {
    use lekalo_core::versioning::plan::sha256_hex;

    let sandbox = Sandbox::from_fixture("golden-0.1.0");
    assert_exit(
        &lekalo(&[
            "migrate",
            "--project",
            &sandbox.selector,
            "--to",
            "model/1.0.0",
        ]),
        0,
    );
    let plan_id = sandbox.plan_ids()[0].clone();

    // Simulate a crash between the first replacement and the commit: the
    // journal and lock are present and the sources are migrated.
    let home = format!(".lekalo/cache/migrations/{plan_id}");
    let manifest = sandbox.read(&format!("{home}/manifest.json"));
    let journal = format!(
        "{{\"planId\":\"{plan_id}\",\"manifestSha256\":\"{}\"}}",
        sha256_hex(&manifest)
    );
    fs::write(sandbox.root.join(&home).join("journal.json"), journal).expect("journal");
    fs::write(
        sandbox.root.join(".lekalo/cache/migrations/active.lock"),
        format!("{plan_id}\n"),
    )
    .expect("lock");

    // Every reader fails closed while the journal exists.
    let load = lekalo(&["load", "--project", &sandbox.selector]);
    assert_exit(&load, 1);
    assert!(stderr_text(&load).contains("versioning.recovery-required"));

    // The next explicit migrate operation recovers first (restoring the
    // verified before bytes), then migrates again to the target.
    let output = lekalo(&[
        "migrate",
        "--project",
        &sandbox.selector,
        "--to",
        "model/1.0.0",
        "--json",
    ]);
    assert_exit(&output, 0);
    let receipt: Value = serde_json::from_str(stdout_text(&output).trim()).expect("receipt");
    assert_eq!(receipt["transaction"], "committed");

    // The tree is migrated, the journal and lock are cleared, and the
    // project loads again.
    for relative in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/planner/queries.yaml",
    ] {
        let twin = fs::read(format!(
            "{}/../../tests/fixtures/model-compat/1.0.0/{relative}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("twin document");
        assert_eq!(sandbox.read(relative), twin, "{relative}");
    }
    assert!(!sandbox.root.join(&home).join("journal.json").exists());
    assert!(!sandbox
        .root
        .join(".lekalo/cache/migrations/active.lock")
        .exists());
    let load = lekalo(&["load", "--project", &sandbox.selector]);
    assert_exit(&load, 0);
}
