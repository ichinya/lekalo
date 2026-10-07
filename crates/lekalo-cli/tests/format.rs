//! Issue #94 CLI tests for `lekalo format`: the real binary drives the
//! documented dispositions — check 0/3/1/5, apply 0, stdin editor flow,
//! scope selectors, empty semantic diff after formatting, and the
//! partial-write refusal.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURES: &str = "tests/fixtures/format";

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

fn workspace_root() -> PathBuf {
    alias_free_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../"))
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .env_remove("LEKALO_PROJECT")
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

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dest");
    for entry in fs::read_dir(from).expect("read source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Materialize one fixture seed into a fresh named sandbox.
fn sandbox(name: &str, seed: &str) -> PathBuf {
    let destination =
        std::env::temp_dir().join(format!("lekalo-format-cli-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&destination);
    copy_dir(&workspace_root().join(FIXTURES).join(seed), &destination);
    destination
}

#[test]
fn check_gate_is_ci_usable_across_dispositions() {
    // 3: valid source needs formatting; stdout; reason code; JSON report.
    let noncanonical = sandbox("gate-check", "seeds/noncanonical");
    let output = lekalo_in(&noncanonical, &["--json", "format", "--check"]);
    assert_eq!(exit_code(&output), 3, "{}", stdout_text(&output));
    let stdout = stdout_text(&output);
    assert!(
        stdout.contains("\"status\": \"denied\"")
            && stdout.contains("format.diff")
            && stdout.contains("lekalo/modules/planner/entities.yaml"),
        "the denied envelope carries the report and rows"
    );
    assert!(stderr_text(&output).is_empty(), "denied writes stdout");
    // The check is read-only: no runtime tree, no source change.
    assert!(
        !noncanonical.join(".lekalo").exists(),
        "check creates nothing"
    );
    // 0: an already canonical tree is valid.
    let canonical = sandbox("gate-canonical", "planner-canonical");
    let clean = lekalo_in(&canonical, &["--json", "format", "--check"]);
    assert_eq!(exit_code(&clean), 0, "{}", stdout_text(&clean));
    assert!(
        stdout_text(&clean).contains("\"changed\":0"),
        "zero changed rows"
    );
    // 1: a parse error is invalid on stderr with a loader code.
    let broken = sandbox("gate-parse", "seeds/invalid-parse");
    let parse = lekalo_in(&broken, &["format", "--check"]);
    assert_eq!(exit_code(&parse), 1);
    assert!(
        stderr_text(&parse).contains("loader.yaml-unsupported"),
        "parse errors stay parse errors: {}",
        stderr_text(&parse)
    );
    // 5: an unknown Model version is never claimed format-different.
    let old = sandbox("gate-version", "seeds/unsupported-version");
    let version = lekalo_in(&old, &["format", "--check"]);
    assert_eq!(exit_code(&version), 5);
    assert!(
        stderr_text(&version).contains("versioning.unsupported-version"),
        "{}",
        stderr_text(&version)
    );
    let _ = fs::remove_dir_all(&noncanonical);
    let _ = fs::remove_dir_all(&canonical);
    let _ = fs::remove_dir_all(&broken);
    let _ = fs::remove_dir_all(&old);
}

#[test]
fn apply_is_idempotent_and_semantically_invisible() {
    let before = sandbox("idem-before", "seeds/noncanonical");
    let after = sandbox("idem-after", "seeds/noncanonical");
    // Apply once in `after`; `before` stays the noncanonical original.
    let apply = lekalo_in(&after, &["format"]);
    assert_eq!(exit_code(&apply), 0, "{}", stdout_text(&apply));
    // Idempotence: a second apply changes nothing and a check is valid.
    let first = fs::read(after.join("lekalo/modules/planner/entities.yaml")).expect("bytes");
    let mtime = fs::metadata(after.join("lekalo/modules/planner/entities.yaml"))
        .expect("meta")
        .modified()
        .expect("mtime");
    let again = lekalo_in(&after, &["format"]);
    assert_eq!(exit_code(&again), 0);
    assert_eq!(
        fs::read(after.join("lekalo/modules/planner/entities.yaml")).expect("bytes"),
        first,
        "F(F(x)) == F(x)"
    );
    assert_eq!(
        fs::metadata(after.join("lekalo/modules/planner/entities.yaml"))
            .expect("meta")
            .modified()
            .expect("mtime"),
        mtime,
        "a no-op apply does not touch the file"
    );
    let check = lekalo_in(&after, &["format", "--check"]);
    assert_eq!(exit_code(&check), 0);
    // The semantic diff of a formatting-only change is empty.
    let before_name = before.file_name().unwrap().to_string_lossy().into_owned();
    let after_name = after.file_name().unwrap().to_string_lossy().into_owned();
    let parent = after.parent().expect("shared temp parent");
    let diff = lekalo_in(
        parent,
        &[
            "diff",
            &before_name,
            &after_name,
            "--format",
            "json",
            "--json",
        ],
    );
    let stdout = stdout_text(&diff);
    assert_eq!(exit_code(&diff), 0, "{stdout}");
    assert!(
        stdout.contains("\"changes\":[]"),
        "formatting produces an empty semantic diff: {stdout}"
    );
    let _ = fs::remove_dir_all(&before);
    let _ = fs::remove_dir_all(&after);
}

#[test]
fn an_invalid_file_is_never_partially_rewritten() {
    let root = sandbox("partial", "seeds/noncanonical");
    // A late malformed file sorts after earlier candidates: planning must
    // refuse before the first replacement.
    let scenarios = root.join("lekalo/modules/planner/scenarios.yaml");
    let scenarios_before = fs::read(&scenarios).expect("original bytes");
    fs::write(&scenarios, b"definitions: [\n  broken\n").expect("malformed tail");
    let entities = root.join("lekalo/modules/planner/entities.yaml");
    let entities_before = fs::read(&entities).expect("original bytes");
    let output = lekalo_in(&root, &["format"]);
    assert_eq!(exit_code(&output), 1, "{}", stderr_text(&output));
    assert_eq!(
        fs::read(&entities).expect("after"),
        entities_before,
        "no earlier file was rewritten when a later source is malformed"
    );
    let _ = fs::remove_dir_all(&root);
    let _ = scenarios_before;
}

#[test]
fn scope_selectors_limit_the_plan_and_refuse_unknowns() {
    let root = sandbox("scope", "seeds/noncanonical");
    let project_before = fs::read(root.join("lekalo/project.yaml")).expect("before");
    let module = lekalo_in(
        &root,
        &["format", "--check", "lekalo/modules/scheduling", "--json"],
    );
    assert_eq!(exit_code(&module), 3);
    let stdout = stdout_text(&module);
    assert!(
        stdout.contains("lekalo/modules/scheduling/module.yaml")
            && !stdout.contains("lekalo/project.yaml"),
        "the module scope covers only its own documents: {stdout}"
    );
    let document = lekalo_in(
        &root,
        &["format", "--check", "lekalo/modules/scheduling/module.yaml"],
    );
    assert_eq!(exit_code(&document), 3);
    let unknown = lekalo_in(&root, &["format", "--check", "lekalo/absent.yaml"]);
    assert_eq!(exit_code(&unknown), 1);
    assert!(stderr_text(&unknown).contains("format.selection-invalid"));
    let _ = fs::remove_dir_all(&root);
    let _ = project_before;
}

#[test]
fn comment_loss_blocks_then_applies_under_explicit_opt_in() {
    let root = sandbox("cli-loss", "seeds/comment-loss");
    let blocked = lekalo_in(&root, &["--json", "format"]);
    assert_eq!(exit_code(&blocked), 3, "{}", stdout_text(&blocked));
    assert!(
        stdout_text(&blocked).contains("format.comments-loss"),
        "the loss is enumerated before any write"
    );
    let lossy = lekalo_in(&root, &["--json", "format", "--allow-lossy-comments"]);
    assert_eq!(exit_code(&lossy), 0, "{}", stdout_text(&lossy));
    let formatted =
        fs::read_to_string(root.join("lekalo/modules/scheduling/module.yaml")).expect("formatted");
    assert!(!formatted.contains("# this trailing comment"));
    let stable = lekalo_in(&root, &["format", "--check"]);
    assert_eq!(exit_code(&stable), 0, "lossy output stabilizes");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn editor_stdin_flow_returns_replacement_and_never_writes() {
    let root = sandbox("cli-stdin", "planner-canonical");
    let path = root.join("lekalo/modules/scheduling/module.yaml");
    let before = fs::read(&path).expect("disk bytes");
    // Reject the exclusive-mode conflicts at the usage layer.
    for args in [
        vec!["format", "--stdin", "--stdin-filepath", "x", "--check"],
        vec![
            "format",
            "--stdin",
            "--stdin-filepath",
            "x",
            "lekalo/project.yaml",
        ],
        vec!["format", "--stdin"],
    ] {
        let conflict = lekalo_in(&root, &args);
        assert_ne!(exit_code(&conflict), 0, "usage conflicts refuse: {args:?}");
    }
    let buffer = "schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling module\n";
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "--json",
            "format",
            "--stdin",
            "--stdin-filepath",
            "lekalo/modules/scheduling/module.yaml",
        ])
        .current_dir(alias_free_path(&root))
        .env_remove("LEKALO_PROJECT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn lekalo format --stdin");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(buffer.as_bytes())
        .expect("write buffer");
    let output = child.wait_with_output().expect("editor run");
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("envelope json");
    let rows = envelope
        .pointer("/format/files")
        .and_then(|rows| rows.as_array())
        .expect("report rows")
        .clone();
    assert_eq!(rows.len(), 1, "exactly one editor row");
    let replacement = rows[0]
        .get("replacement")
        .and_then(|text| text.as_str())
        .expect("the editor row carries the replacement text");
    assert!(replacement.starts_with('"'), "canonical document text");
    assert_eq!(fs::read(&path).expect("after"), before, "no disk write");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn planner_fixture_has_a_stable_formatted_representation() {
    // The committed golden formats to itself on this platform: check is
    // valid and apply is a byte-identical no-op.
    let root = sandbox("planner-golden", "planner-canonical");
    let check = lekalo_in(&root, &["format", "--check", "--json"]);
    assert_eq!(exit_code(&check), 0, "{}", stdout_text(&check));
    let golden = workspace_root().join(FIXTURES).join("planner-canonical");
    for relative in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/module.yaml",
        "lekalo/modules/planner/entities.yaml",
        "lekalo/modules/planner/scenarios.yaml",
        "lekalo/modules/scheduling/module.yaml",
    ] {
        let before = fs::read(root.join(relative)).expect("before");
        let golden_bytes = fs::read(golden.join(relative)).expect("golden");
        assert_eq!(before, golden_bytes, "{relative} matches the golden");
    }
    let apply = lekalo_in(&root, &["format"]);
    assert_eq!(exit_code(&apply), 0);
    for relative in [
        "lekalo/project.yaml",
        "lekalo/modules/planner/entities.yaml",
    ] {
        assert_eq!(
            fs::read(root.join(relative)).expect("after"),
            fs::read(golden.join(relative)).expect("golden"),
            "{relative} is stable under apply"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn interrupted_state_blocks_load_and_is_reported_by_check() {
    let root = sandbox("cli-recovery", "planner-canonical");
    let journal = root.join(".lekalo/cache/format/abc123");
    fs::create_dir_all(&journal).expect("home");
    fs::write(
        journal.join("journal.json"),
        "{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"planId\":\"abc123\",\"manifestSha256\":\"x\"}\n",
    )
    .expect("journal");
    let load = lekalo_in(&root, &["load"]);
    assert_eq!(exit_code(&load), 1);
    assert!(
        stderr_text(&load).contains("format.recovery-required"),
        "{}",
        stderr_text(&load)
    );
    let check = lekalo_in(&root, &["format", "--check"]);
    assert_eq!(exit_code(&check), 1);
    assert!(
        stderr_text(&check).contains("format.recovery-required"),
        "check reads state and refuses; it never recovers"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn editor_stdin_refuses_comment_losing_buffers() {
    let root = sandbox("cli-stdin-loss", "planner-canonical");
    let path = root.join("lekalo/modules/scheduling/module.yaml");
    let before = fs::read(&path).expect("disk bytes");
    // A lossy buffer refuses: the replacement text is never returned as
    // success (exit 0 with a dropped comment).
    let buffer = "schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling\n    module # this trailing comment has no owner line\n";
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "--json",
            "format",
            "--stdin",
            "--stdin-filepath",
            "lekalo/modules/scheduling/module.yaml",
        ])
        .current_dir(alias_free_path(&root))
        .env_remove("LEKALO_PROJECT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn editor run");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(buffer.as_bytes())
        .expect("write buffer");
    let output = child.wait_with_output().expect("editor run");
    assert_eq!(exit_code(&output), 3, "{}", stdout_text(&output));
    let stdout = stdout_text(&output);
    assert!(
        stdout.contains("format.stdin-loss"),
        "the registered editor-mode loss diagnostic gates the editor return: {stdout}"
    );
    assert!(
        !stdout.contains("format.comments-loss"),
        "the disk-mode loss identity is never used for a buffer refusal: {stdout}"
    );
    // The span is exact (finding r2 8): line 7, byte offset beyond 0.
    let envelope: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    let range = envelope
        .pointer("/diagnostics/0/source/range")
        .cloned()
        .expect("the loss diagnostic carries its exact span");
    assert_eq!(range["start"]["line"], 7, "the loss line: {range}");
    assert!(
        range["start"]["byte"].as_u64().expect("byte") > 0,
        "the real byte offset, never hardcoded 0: {range}"
    );
    assert_eq!(range["end"]["line"], 7, "a same-line span: {range}");
    assert!(
        range["end"]["byte"].as_u64().expect("byte")
            > range["start"]["byte"].as_u64().expect("byte"),
        "the span covers the lost comment line: {range}"
    );
    assert!(
        !stdout.contains("\"replacement\""),
        "no replacement returned: {stdout}"
    );
    assert_eq!(fs::read(&path).expect("after"), before, "disk untouched");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn editor_stdin_judges_edits_against_the_unsaved_buffer() {
    let root = sandbox("cli-stdin-oracle", "planner-canonical");
    let path = root.join("lekalo/modules/scheduling/module.yaml");
    let before = fs::read(&path).expect("disk bytes");
    // The buffer semantically diverges from the saved file: the oracle
    // compares against the buffer, so the edit survives into the
    // replacement instead of being refused as format.semantic-change.
    let buffer = "schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: The unsaved semantic edit\n";
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "--json",
            "format",
            "--stdin",
            "--stdin-filepath",
            "lekalo/modules/scheduling/module.yaml",
        ])
        .current_dir(alias_free_path(&root))
        .env_remove("LEKALO_PROJECT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn editor run");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(buffer.as_bytes())
        .expect("write buffer");
    let output = child.wait_with_output().expect("editor run");
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("envelope json");
    let replacement = envelope
        .pointer("/format/files/0/replacement")
        .and_then(|text| text.as_str())
        .expect("the replacement text");
    assert!(
        replacement.contains("The unsaved semantic edit"),
        "the replacement carries the buffer's edit: {replacement}"
    );
    assert_eq!(fs::read(&path).expect("after"), before, "disk untouched");
    let _ = fs::remove_dir_all(&root);
}
