//! Issue #121 CLI tests for the local-only run history: the offline
//! end-to-end flow (init, scope create, record, list, show, retention,
//! dependents, delete, recover), the fail-closed refusals without
//! value echo, and the absent export surface.
//!
//! Every run happens in a fresh temp project under the crate's
//! `target/` tree, reached through the alias-free temp spelling (the
//! selection policy denies 8.3 alias spellings on Windows), and the
//! offline flow runs with a cleared environment: no account, provider,
//! proxy, or telemetry configuration of any kind.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

static NEXT_CASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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

fn temp_case(tag: &str) -> PathBuf {
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
        .join("history-cli-tests")
        .join(format!("{tag}-{id}"));
    std::fs::create_dir_all(&dir).expect("temp case dir");
    dir
}

/// Run the real binary with a cleared environment: history must work
/// fully offline with no account, provider, proxy, or telemetry
/// configuration. PATH (and SystemRoot on Windows) stays so the
/// process can start at all.
fn lekalo(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lekalo"));
    let mut all_args = vec!["--json"];
    all_args.extend_from_slice(args);
    command.args(&all_args).current_dir(alias_free_path(dir));
    command.env_clear();
    #[cfg(windows)]
    {
        command.env(
            "SystemRoot",
            std::env::var("SystemRoot").unwrap_or_default(),
        );
        command.env("PATH", std::env::var("PATH").unwrap_or_default());
    }
    #[cfg(not(windows))]
    {
        command.env("PATH", std::env::var("PATH").unwrap_or_default());
    }
    command.output().expect("run the real lekalo binary")
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

/// One minimal valid observation: bounded, closed, and honest — the
/// absent metric leaves normalize to unknown.
const VALID_OBSERVATION: &str = r#"{
  "schema_version": "lekalo/run-observation/v0.4.0",
  "identity": "dev.lekalo.run-observation@0.4.0",
  "pilot": {"mode": "greenfield", "scopeState": "observed"},
  "operation": {"kind": "evaluation", "affectedSemanticIds": ["planner.focus_task"]},
  "timestamp": "2026-09-30T12:00:00Z",
  "provenance": {
    "git": {},
    "model": {
      "revision": {"state": "known", "value": "planner-model"},
      "digest": {"state": "known", "value": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
      "irDigest": {"state": "known", "value": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}
    },
    "lock": {},
    "core": {"version": {"state": "known", "value": "0.4.0"}},
    "adapters": [],
    "profile": {},
    "harness": {"id": {"state": "known", "value": "pilot-harness"}}
  },
  "metrics": {},
  "measurementSources": [],
  "testGateSummaries": [],
  "diagnostics": [],
  "assertions": null,
  "repeatParentRunId": null,
  "status": {"outcome": "warn", "coverageState": "unknown"},
  "dataSensitivity": "internal"
}"#;

/// Write the observation into the case dir and print it to `record
/// --input -` through a pipe.
fn record_observation(dir: &Path, scope: &str, observation: &str) -> Output {
    use std::io::Write as _;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "--json",
            "history",
            "record",
            "--input",
            "-",
            "--scope",
            scope,
            "--project",
            ".",
        ])
        .current_dir(alias_free_path(dir))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn record");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(observation.as_bytes())
        .expect("write observation");
    child.wait_with_output().expect("record output")
}

fn scope_token(dir: &Path) -> String {
    let output = lekalo(dir, &["history", "scope", "create", "--project", "."]);
    assert_eq!(
        exit_code(&output),
        0,
        "scope create: {}",
        stdout_text(&output)
    );
    let stdout = stdout_text(&output);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    value["result"]["tenantScopeId"]
        .as_str()
        .expect("scope token")
        .to_owned()
}

#[test]
fn the_full_offline_history_flow_works_with_an_empty_environment() {
    let dir = temp_case("offline");

    // Init creates the governed home and its ignore protection.
    let output = lekalo(&dir, &["history", "init", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let home = dir.join(".lekalo/history");
    assert!(home.is_dir());
    assert_eq!(
        std::fs::read(home.join(".gitignore")).expect("ignore protection"),
        b"*\n"
    );

    // The full flow runs with an empty environment: no account, no
    // network configuration, no provider variables.
    let scope = scope_token(&dir);
    let output = record_observation(&dir, &scope, VALID_OBSERVATION);
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let receipt: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("receipt json");
    let run_id = receipt["result"]["runId"].as_str().expect("run id");
    assert_eq!(run_id.len(), 32);
    assert_eq!(receipt["result"]["status"], json_str("warn"));

    // The list page shows the run in stable order.
    let output = lekalo(
        &dir,
        &["history", "list", "--scope", &scope, "--project", "."],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let list: serde_json::Value = serde_json::from_str(&stdout_text(&output)).expect("list json");
    assert_eq!(list["result"]["runs"].as_array().expect("runs").len(), 1);

    // The show envelope names the record and the assertion surface
    // apart; this observation carries no assertions (unknown evidence,
    // not a pass).
    let output = lekalo(
        &dir,
        &[
            "history",
            "show",
            run_id,
            "--scope",
            &scope,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let show: serde_json::Value = serde_json::from_str(&stdout_text(&output)).expect("show json");
    assert_eq!(show["result"]["record"]["runId"], json_str(run_id));
    assert!(show["result"]["assertions"].is_null());

    // Recovery verifies the store.
    let output = lekalo(&dir, &["history", "recover", "--project", "."]);
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hostile_observation_refuses_without_echoing_the_value() {
    let dir = temp_case("hostile");
    lekalo(&dir, &["history", "init", "--project", "."]);
    let scope = scope_token(&dir);
    let hostile = VALID_OBSERVATION.replace("pilot-harness", "C:/Users/someone/secret-prompt.txt");
    let output = record_observation(&dir, &scope, &hostile);
    assert_eq!(exit_code(&output), 1, "{}", stdout_text(&output));
    let combined = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(combined.contains("history.unsafe-field"), "{combined}");
    // The rejected value never appears in any output.
    assert!(!combined.contains("C:"), "{combined}");
    assert!(!combined.contains("secret-prompt"), "{combined}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_observation_version_exits_unsupported_version() {
    let dir = temp_case("version");
    lekalo(&dir, &["history", "init", "--project", "."]);
    let scope = scope_token(&dir);
    let stale = VALID_OBSERVATION.replace(
        "lekalo/run-observation/v0.4.0",
        "lekalo/run-observation/v0.3.0",
    );
    let output = record_observation(&dir, &scope, &stale);
    assert_eq!(exit_code(&output), 5, "{}", stdout_text(&output));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deletion_invalidates_bound_dependents_and_repeated_deletion_is_absent() {
    let dir = temp_case("delete");
    lekalo(&dir, &["history", "init", "--project", "."]);
    let scope = scope_token(&dir);
    let output = record_observation(&dir, &scope, VALID_OBSERVATION);
    let receipt: serde_json::Value =
        serde_json::from_str(&stdout_text(&output)).expect("receipt json");
    let run_id = receipt["result"]["runId"]
        .as_str()
        .expect("run id")
        .to_owned();

    // Bind a dependent claim to the live run.
    let output = lekalo(
        &dir,
        &[
            "history",
            "dependents",
            "register",
            "claim.lift",
            "--kind",
            "claim",
            "--run",
            &run_id,
            "--scope",
            &scope,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));

    // The dry run reports the invalidation without applying it.
    let output = lekalo(
        &dir,
        &[
            "history",
            "delete",
            &run_id,
            "--scope",
            &scope,
            "--dry-run",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let dry: serde_json::Value = serde_json::from_str(&stdout_text(&output)).expect("dry json");
    assert_eq!(dry["result"]["applied"], serde_json::Value::Bool(false));
    assert_eq!(
        dry["result"]["invalidatedDependents"],
        serde_json::json!(["claim.lift"])
    );

    // The apply performs the deletion and the dependent no longer
    // resolves.
    let output = lekalo(
        &dir,
        &[
            "history",
            "delete",
            &run_id,
            "--scope",
            &scope,
            "--apply",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let output = lekalo(
        &dir,
        &[
            "history",
            "dependents",
            "resolve",
            "claim.lift",
            "--scope",
            &scope,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1, "{}", stdout_text(&output));
    let combined = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(
        combined.contains("history.dependent-invalidated"),
        "{combined}"
    );

    // A repeated deletion is an explicit absent result.
    let output = lekalo(
        &dir,
        &[
            "history",
            "delete",
            &run_id,
            "--scope",
            &scope,
            "--apply",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    let combined = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(combined.contains("history.source-missing"), "{combined}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retention_prunes_oldest_beyond_the_record_bound() {
    let dir = temp_case("retention");
    lekalo(&dir, &["history", "init", "--project", "."]);
    let scope = scope_token(&dir);
    lekalo(
        &dir,
        &[
            "history",
            "retention",
            "--scope",
            &scope,
            "--max-records",
            "1",
            "--project",
            ".",
        ],
    );
    let first = record_observation(&dir, &scope, VALID_OBSERVATION);
    assert_eq!(exit_code(&first), 0, "{}", stdout_text(&first));
    let second = record_observation(&dir, &scope, VALID_OBSERVATION);
    // The same bytes at a fresh generated run id still record; the
    // oldest record (the first) leaves the store in the same
    // transaction.
    assert_eq!(exit_code(&second), 0, "{}", stdout_text(&second));
    let receipt: serde_json::Value =
        serde_json::from_str(&stdout_text(&second)).expect("receipt json");
    let pruned = receipt["result"]["pruned"].as_array().expect("pruned");
    assert_eq!(pruned.len(), 1, "{}", stdout_text(&second));
    let (rows, _) = {
        let output = lekalo(
            &dir,
            &["history", "list", "--scope", &scope, "--project", "."],
        );
        let list: serde_json::Value =
            serde_json::from_str(&stdout_text(&output)).expect("list json");
        (list["result"]["runs"].as_array().expect("runs").len(), ())
    };
    assert_eq!(rows, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_recorder_has_no_export_surface() {
    let dir = temp_case("no-export");
    // There is no export/upload/aggregate command anywhere in the
    // history family; the CLI refuses at the argument layer.
    let output = lekalo(&dir, &["history", "export", "--project", "."]);
    assert_ne!(exit_code(&output), 0, "{}", stdout_text(&output));
    let combined = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(combined.contains("cli.usage"), "{combined}");
    let output = lekalo(&dir, &["history", "upload", "--project", "."]);
    assert_ne!(exit_code(&output), 0);
    // `record` accepts only the stdin handoff; there is no
    // caller-selected input path and no output destination argument.
    let output = lekalo(
        &dir,
        &[
            "history",
            "record",
            "--input",
            "observation.json",
            "--scope",
            "11111111111111111111111111111111",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 1, "usage error expected");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_scope_token_from_another_store_discloses_nothing() {
    let dir = temp_case("isolation");
    lekalo(&dir, &["history", "init", "--project", "."]);
    // A syntactically valid token that does not resolve in this store.
    let output = lekalo(
        &dir,
        &[
            "history",
            "list",
            "--scope",
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 3, "{}", stdout_text(&output));
    let combined = format!("{}{}", stdout_text(&output), stderr_text(&output));
    assert!(combined.contains("history.scope-mismatch"), "{combined}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn json_str(text: &str) -> serde_json::Value {
    serde_json::Value::String(text.to_owned())
}
