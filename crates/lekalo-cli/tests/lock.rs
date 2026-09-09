//! Issue #10 CLI tests for `lekalo lock`: create/check behavior, the
//! committed golden bytes, refusal classification on the accepted
//! 0/1/3/4/5 envelope, and byte-identical determinism across two projects.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GOLDEN: &str = include_str!("../../../tests/fixtures/lockfile/valid/contract-only.lock.json");
const GOLDEN_DIGEST: &str =
    "sha256:f2f9366f102ca7996f4976a8f19d5052b2e68ce5b85304fbce9686981faef33e";
const REFERENCE_PROJECT: &str = "tests/fixtures/lockfile/project";

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path(dir))
        .output()
        .expect("run the real lekalo binary")
}

/// GitHub's Windows runners export `%TEMP%` spelled with the 8.3 alias of
/// the profile directory (`C:\Users\RUNNER~1\AppData\Local\Temp`), and the
/// selection policy denies alias spellings (`structure.selection-alias`)
/// before any command logic runs. Chdir the child into the resolved,
/// alias-free spelling; `canonicalize` returns it under a `\\?\` verbatim
/// prefix that is stripped back to the plain drive form.
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn assert_lf_only(text: &str) {
    assert!(text.ends_with('\n'), "one final LF");
    assert!(!text.contains("\r\n"), "no CRLF");
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

fn copy_reference_project(root: &Path) {
    copy_dir(&workspace_path(REFERENCE_PROJECT), root);
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let target_path = target.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            std::fs::copy(entry.path(), target_path).expect("copy file");
        }
    }
}

fn project_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lekalo-lock-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp project dir");
    copy_reference_project(&dir);
    dir
}

#[test]
fn lock_create_then_check_produces_the_committed_golden_bytes() {
    let dir = project_dir("golden");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let bytes = std::fs::read(dir.join("lekalo.lock")).expect("created lock");
    assert_eq!(bytes, GOLDEN.as_bytes(), "created bytes match the golden");

    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(0));
    assert_lf_only(&stdout(&output));
    let output = lekalo_in(&dir, &["--json", "lock"]);
    assert_eq!(output.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("json envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["operation"], "lock");
    assert_eq!(document["mode"], "check");
    assert_eq!(document["lockDigest"], GOLDEN_DIGEST);
    assert_eq!(document["resolverVersion"], "1.0.0");
    assert_eq!(document["counts"]["adapters"], 0);
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn lock_create_human_line_is_stable_and_deterministic_across_projects() {
    let dir_a = project_dir("det-a");
    let dir_b = project_dir("det-b");
    let output_a = lekalo_in(&dir_a, &["lock"]);
    let output_b = lekalo_in(&dir_b, &["lock"]);
    assert_eq!(output_a.status.code(), Some(0));
    assert_eq!(output_a.status.code(), output_b.status.code());
    assert_eq!(stdout(&output_a), stdout(&output_b), "byte-identical runs");
    assert!(
        stdout(&output_a).starts_with("lock created sha256:"),
        "stable human projection: {}",
        stdout(&output_a)
    );
    std::fs::remove_dir_all(&dir_a).expect("cleanup");
    std::fs::remove_dir_all(&dir_b).expect("cleanup");
}

#[test]
fn lock_check_refuses_a_missing_lock_without_creating_it() {
    let dir = project_dir("missing");
    let output = lekalo_in(&dir, &["lock", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        stderr(&output).contains("lock.missing"),
        "typed refusal: {}",
        stderr(&output)
    );
    assert!(!dir.join("lekalo.lock").exists(), "zero writes on refusal");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn digest_mismatch_is_a_stdout_denial_distinct_from_validation_failure() {
    let dir = project_dir("tamper");
    std::fs::write(
        dir.join("lekalo.lock"),
        GOLDEN.replace("sha256:2cba65b0", "sha256:3cba65b0"),
    )
    .expect("tampered lock");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(stdout(&output).contains("lock.digest-mismatch"));
    assert!(stderr(&output).is_empty(), "denials stay on stdout");

    // Validation failure stays on stderr with exit 1.
    let broken = "{ not json }";
    std::fs::write(dir.join("lekalo.lock"), broken).expect("broken lock");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.schema-invalid"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn noncanonical_locks_are_refused_and_never_rewritten() {
    let dir = project_dir("noncanonical");
    let payload = GOLDEN.trim_end_matches('\n');
    std::fs::write(dir.join("lekalo.lock"), format!("{payload}\n\n")).expect("noncanonical");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.noncanonical"));
    // --locked never silently rewrites: the bytes are untouched.
    let bytes = std::fs::read(dir.join("lekalo.lock")).expect("bytes");
    assert_eq!(bytes, format!("{payload}\n\n").as_bytes());
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn a_multi_adapter_lock_gets_past_the_protocol_publication_gate() {
    // Issue #27 publishes the protocol: a lock that carries adapters no
    // longer dies in the publication gate (exit 5). The same fixture now
    // travels further and is refused on request currency instead — a
    // different diagnosis at a later checkpoint, never a silent pass.
    let dir = project_dir("protocol");
    let multi = std::fs::read_to_string(workspace_path(
        "tests/fixtures/lockfile/valid/multi-adapter.lock.json",
    ))
    .expect("multi-adapter fixture");
    let value: serde_json::Value =
        serde_json::from_str(multi.trim_end_matches('\n')).expect("fixture json");
    std::fs::write(
        dir.join("lekalo.lock"),
        serde_json::to_string(&value).expect("wire"),
    )
    .expect("write lock");
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("lock.stale"));
    assert!(!stderr(&output).contains("versioning.protocol-unpublished"));
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn flag_exclusivity_and_unknown_flags_map_to_cli_usage() {
    let dir = project_dir("usage");
    // `--offline` composes with create and check.
    let output = lekalo_in(&dir, &["lock", "--offline"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let output = lekalo_in(&dir, &["lock", "--offline", "--check"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    // An unknown flag maps to the stable usage failure (JSON with --json).
    let output = lekalo_in(&dir, &["--json", "lock", "--bogus"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        std::fs::read_to_string(workspace_path(
            "tests/fixtures/diagnostics/usage-envelope.json"
        ))
        .expect("usage envelope fixture")
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// One hostile `schema_version` discriminator: a ~400-character token
/// carrying a JSON-escaped control character — the exact shape a tampered
/// lock can carry, previously bounded only by the 1 MiB file cap.
fn hostile_lock_bytes() -> Vec<u8> {
    let token = format!("lekalo/lock/v9.0.0\\u0001{}", "A".repeat(375));
    format!("{{\"schema_version\":\"{token}\"}}\n").into_bytes()
}

/// B2 regression: the attacker-controlled `data.found` echo of
/// `lock.unsupported-schema-version` passes the bounded-token invariant in
/// both projections — capped at 256 bytes, control-clean, and unable to
/// scale the exit-5 envelope however hostile the lock discriminator is.
#[test]
fn hostile_lock_schema_version_echo_is_bounded_and_control_clean() {
    let dir = project_dir("hostile-schema-version");
    std::fs::write(dir.join("lekalo.lock"), hostile_lock_bytes()).expect("hostile lock");

    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(output.stdout.is_empty(), "human failure rides stderr");
    assert!(stderr(&output).contains("lock.unsupported-schema-version"));

    let output = lekalo_in(&dir, &["--json", "lock"]);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(output.stdout.is_empty(), "json failure rides stderr");
    let envelope = stderr(&output);
    assert!(
        envelope.len() <= 2048,
        "envelope is bounded, got {} bytes",
        envelope.len()
    );
    let document: serde_json::Value = serde_json::from_str(&envelope).expect("envelope parses");
    assert_eq!(document["status"], "unsupported-version");
    assert_eq!(
        document["diagnostics"][0]["id"],
        "lock.unsupported-schema-version"
    );
    let found = document["diagnostics"][0]["data"]["found"]
        .as_str()
        .expect("found echoed");
    assert!(
        found.len() <= 256,
        "found token is bounded, got {} bytes",
        found.len()
    );
    assert!(
        !found.chars().any(char::is_control),
        "control character reached the wire: {found:?}"
    );
    assert_no_control_strings(&document);
    assert_eq!(
        std::fs::read(dir.join("lekalo.lock")).expect("lock still present"),
        hostile_lock_bytes(),
        "the hostile lock is never rewritten"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// Every string in a parsed envelope is free of control characters: the
/// bounded-token invariant collapses them before any wire item is built.
fn assert_no_control_strings(value: &serde_json::Value) {
    match value {
        serde_json::Value::String(text) => assert!(
            !text.chars().any(char::is_control),
            "control character reached the wire: {text:?}"
        ),
        serde_json::Value::Array(items) => items.iter().for_each(assert_no_control_strings),
        serde_json::Value::Object(map) => map.values().for_each(assert_no_control_strings),
        _ => {}
    }
}
