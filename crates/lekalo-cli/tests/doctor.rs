//! Issue #92 CLI tests for the doctor/status/readiness family: the
//! read-only reports over the hermetic planner fixture, the stable exit
//! policy, state classification (fresh vs degraded vs blocked), the
//! `--fix` recipe preview, the `--trace` HLV evidence handoff, and the
//! read-only guarantee.
//!
//! Every run happens in a fresh copy of the hermetic planner fixture
//! under the crate's `target/` tree, reached through the alias-free
//! temp spelling (the selection policy denies alias spellings).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/loader/valid-direct-visibility";
const TRACE: &str = "tests/fixtures/trace/full.trace.json";

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

static NEXT_CASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A fresh copy of the fixture with a created lock; the reported verdict
/// is then deterministic modulo the optional HLV evidence.
fn locked_fixture_copy(tag: &str) -> PathBuf {
    let dir = fixture_copy(tag);
    let output = lekalo_in(&dir, &["lock"]);
    assert_eq!(exit_code(&output), 0, "lock creation succeeds");
    dir
}

fn fixture_copy(tag: &str) -> PathBuf {
    let id = NEXT_CASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + u64::from(std::process::id())
        + u64::from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .subsec_nanos(),
        );
    let dir = std::env::current_dir()
        .expect("cwd")
        .join("target")
        .join("doctor-cli-tests")
        .join(format!("{tag}-{id}"));
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(FIXTURE),
        &dir,
    );
    dir
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

/// The fingerprint of every path and length under one directory.
fn fingerprint(dir: &Path) -> Vec<(String, u64)> {
    let mut entries = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<(String, u64)>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let name = path
                .strip_prefix(dir)
                .expect("relative")
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                out.push((name.clone(), 0));
                walk(&path, out);
            } else {
                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((name, len));
            }
        }
    }
    walk(dir, &mut entries);
    entries.sort();
    entries
}

#[test]
fn doctor_reports_the_fresh_fixture_without_mutating_it() {
    let dir = locked_fixture_copy("ready");
    let trace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(TRACE);
    let trace = trace.to_string_lossy().replace('\\', "/");
    let before = fingerprint(&dir);
    let output = lekalo_in(
        &dir,
        &["--json", "doctor", "--trace", &trace, "--project", "."],
    );
    // The stable exit policy: a produced report is valid, exit 0, stdout.
    assert_eq!(exit_code(&output), 0);
    assert!(stderr_text(&output).is_empty());
    let json = stdout_text(&output);
    let document: serde_json::Value = serde_json::from_str(&json).expect("wire document parses");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["schemaVersion"], "lekalo/doctor/v1.0.0");
    assert_eq!(document["identity"], "dev.lekalo.doctor@1.0.0");
    assert_eq!(document["report"], "doctor");
    assert_eq!(document["productVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(document["verdict"], "ready");
    assert_eq!(document["revisions"]["lock"]["state"], "fresh");
    assert_eq!(document["revisions"]["model"]["version"], "1.0.0");
    // The exact model/lock revisions are pinned by the fixture bytes.
    let lock_digest = document["revisions"]["lock"]["digest"].as_str().unwrap();
    assert!(lock_digest.starts_with("sha256:"));
    assert_eq!(lock_digest.len(), "sha256:".len() + 64);
    // Every check of the closed panel is present, exactly once, sorted.
    let ids: Vec<&str> = document["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            "adapters.inventory",
            "artifacts.drift",
            "bindings.freshness",
            "cache.health",
            "capabilities.profiles",
            "fs.confinement",
            "integrations.hlv",
            "lock.freshness",
            "model.references",
            "model.version",
            "platform.limits",
            "project.root",
            "tools.gates",
        ]
    );
    let after = fingerprint(&dir);
    assert_eq!(before, after, "doctor never mutates the project");
}

#[test]
fn missing_optional_hlv_degrades_but_is_not_a_core_failure() {
    let dir = locked_fixture_copy("hlv");
    let output = lekalo_in(&dir, &["doctor", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    assert!(stdout_text(&output).starts_with("doctor degraded"));
    // With the trace evidence supplied the same project reports ready.
    let trace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(TRACE);
    let trace = trace.to_string_lossy().replace('\\', "/");
    let output = lekalo_in(&dir, &["doctor", "--trace", &trace, "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    assert!(stdout_text(&output).starts_with("doctor ready"));
}

#[test]
fn stale_states_are_distinguished_per_check() {
    // No lock: the lock check is degraded-absent, the artifact check is
    // unknown (it cannot run without a lock) and names its next action.
    let dir = fixture_copy("absent");
    let output = lekalo_in(&dir, &["--json", "doctor", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("document parses");
    let checks = document["checks"].as_array().unwrap();
    let lock = checks.iter().find(|c| c["id"] == "lock.freshness").unwrap();
    assert_eq!(lock["state"], "degraded");
    assert_eq!(lock["reason"], "absent");
    assert_eq!(lock["nextAction"], "create-lock");
    assert_eq!(lock["diagnostics"][0], "lock.missing");
    let artifacts = checks
        .iter()
        .find(|c| c["id"] == "artifacts.drift")
        .unwrap();
    assert_eq!(artifacts["state"], "unknown");
    assert_eq!(artifacts["reason"], "no-lock");
    // A fresh lock distinguishes: fresh, not absent.
    let dir = locked_fixture_copy("fresh");
    let output = lekalo_in(&dir, &["--json", "status", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("document parses");
    assert_eq!(document["report"], "status");
    assert_eq!(document["verdict"], "ready");
    assert_eq!(document["revisions"]["lock"]["state"], "fresh");
    let cache = document["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "cache.health")
        .unwrap();
    assert_eq!(cache["state"], "ok");
    assert_eq!(cache["reason"], "missing", "an absent cache is healthy");
}

#[test]
fn fix_previews_recipes_and_writes_nothing() {
    let dir = fixture_copy("fix");
    let before = fingerprint(&dir);
    let output = lekalo_in(&dir, &["--json", "doctor", "--fix", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("document parses");
    let recipes = document["recipes"].as_array().expect("--fix lists recipes");
    assert!(
        recipes
            .iter()
            .any(|recipe| recipe["id"] == "create-lock" && recipe["mutating"] == true),
        "the absent lock names its mutating recipe as advice"
    );
    assert!(fingerprint(&dir) == before, "--fix writes nothing");
}

#[test]
fn readiness_phases_gate_the_required_checks() {
    let dir = fixture_copy("phases");
    // Model phase: only optional checks fail here (no lock, no HLV).
    let output = lekalo_in(&dir, &["readiness", "--phase", "model", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    assert!(stdout_text(&output).starts_with("readiness model degraded"));
    // The `done` alias is accepted and normalizes to release.
    let output = lekalo_in(&dir, &["readiness", "--phase", "done", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    assert!(stdout_text(&output).starts_with("readiness release blocked"));
    // An unknown phase is the stable usage failure, not a panic.
    let output = lekalo_in(&dir, &["readiness", "--phase", "ship", "--project", "."]);
    assert_eq!(exit_code(&output), 1);
    assert!(stderr_text(&output).contains("LEK-CLI-001"));
}

#[test]
fn a_project_with_targets_but_no_adapters_is_a_blocker_with_next_action() {
    let dir = locked_fixture_copy("targets");
    std::fs::create_dir_all(dir.join("lekalo/targets")).expect("targets home");
    std::fs::write(
        dir.join("lekalo/targets/render.yaml"),
        "kind: target\nid: render\n",
    )
    .expect("target file");
    let output = lekalo_in(
        &dir,
        &[
            "--json",
            "readiness",
            "--phase",
            "generate",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "the report is still the product");
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("document parses");
    assert_eq!(document["phase"], "generate");
    assert_eq!(document["verdict"], "blocked");
    let checks = document["checks"].as_array().unwrap();
    let adapters = checks
        .iter()
        .find(|c| c["id"] == "adapters.inventory")
        .unwrap();
    assert_eq!(adapters["state"], "blocked");
    assert_eq!(adapters["reason"], "targets-without-adapters");
    assert_eq!(adapters["nextAction"], "resolve-adapters");
    assert_eq!(adapters["required"], true);
    let capabilities = checks
        .iter()
        .find(|c| c["id"] == "capabilities.profiles")
        .unwrap();
    assert_eq!(capabilities["reason"], "targets-without-profile");
}

#[test]
fn a_broken_project_reports_blocked_structure_without_panicking() {
    let dir = fixture_copy("broken");
    std::fs::remove_dir_all(dir.join("lekalo")).expect("remove the model home");
    let output = lekalo_in(&dir, &["--json", "doctor", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "a produced report is valid");
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("document parses");
    assert_eq!(document["verdict"], "blocked");
    let checks = document["checks"].as_array().unwrap();
    // The selection itself still resolves; the missing model home fails
    // the model checks with the preserved structure refusal.
    let version = checks.iter().find(|c| c["id"] == "model.version").unwrap();
    assert_eq!(version["state"], "unknown");
    let diagnostics = version["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|id| id.as_str().unwrap().starts_with("structure.")),
        "the structure refusal is preserved"
    );
    let references = checks
        .iter()
        .find(|c| c["id"] == "model.references")
        .unwrap();
    assert_eq!(references["state"], "blocked");
    assert!(references["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id.as_str().unwrap().starts_with("structure.")));
    assert!(!document["revisions"]["git"]["commit"].is_null());
    assert_eq!(document["revisions"]["model"]["state"], "unknown");
    assert_eq!(document["revisions"]["lock"]["state"], "absent");
}

#[test]
fn platform_notes_cover_the_running_platform() {
    let dir = fixture_copy("platform");
    let output = lekalo_in(&dir, &["--json", "doctor", "--project", "."]);
    assert_eq!(exit_code(&output), 0);
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("document parses");
    let checks = document["checks"].as_array().unwrap();
    let platform = checks
        .iter()
        .find(|c| c["id"] == "platform.limits")
        .unwrap();
    let notes: Vec<&str> = platform["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note.as_str().unwrap())
        .collect();
    let grammar_ok = notes.contains(&"posix-path-syntax") || notes.contains(&"windows-path-syntax");
    let case_ok = notes.contains(&"case-sensitive-filesystem")
        || notes.contains(&"case-insensitive-filesystem");
    assert!(
        grammar_ok,
        "path-grammar diagnostics are covered: {notes:?}"
    );
    assert!(
        case_ok,
        "case-sensitivity diagnostics are covered: {notes:?}"
    );
    #[cfg(target_os = "windows")]
    assert!(notes.contains(&"symlink-privilege-required"));
}
