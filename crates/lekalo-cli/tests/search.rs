//! Issue #93 CLI tests for the `lekalo search` / `lekalo find` /
//! `lekalo search-index` handoff: the hermetic planner fixture over the
//! accepted envelope, exact versus fuzzy semantics, the declared /
//! detected effect trust split, provider-qualified requirement
//! ambiguity, stale-row exclusion, cursor generation binding, cache
//! custody, and the closed-grammar refusals.
//!
//! Every child chdir runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/search/planner";

/// Serializes tests that mutate the shared fixture's cache home. Tests
/// that only read (every command without `--no-cache` touches the
/// shared cache home's lock) hold this lock for their whole run, so a
/// concurrent `cache clear` from a sibling test can never race a
/// reader; the two tests that build/clear copy the fixture to a
/// disposable temp project instead and run lock-free.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
            .join(FIXTURE),
    )
}

/// A disposable copy of the fixture in its own temp dir: cache-mutating
/// tests (build/clear) run here lock-free and never touch the shared
/// fixture's cache home. The alias-free spelling keeps the selection
/// policy happy on Windows runners.
fn disposable_project() -> PathBuf {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    copy_dir(&fixture_path(), &project);
    std::fs::remove_dir_all(project.join(".lekalo")).ok();
    // Keep the temp dir alive for the run by forgetting the guard: the
    // test process is short-lived and the OS reclaims the path.
    let path = alias_free_path(&project);
    std::mem::forget(temp);
    path
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

/// A disposable copy of the fixture in its own temp dir: cache-mutating
/// tests (build/clear) run here lock-free and never touch the shared
/// fixture's cache home. The alias-free spelling keeps the selection
/// policy happy on Windows runners.
fn json(output: &Output) -> serde_json::Value {
    let text = stdout_text(output);
    let text = if text.trim().is_empty() {
        stderr_text(output)
    } else {
        text
    };
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("envelope json: {error}; raw=<{}>", text))
}

/// `find --id` answers unique on the valid envelope with the bounded
/// selection card.
#[test]
fn find_by_exact_id_answers_unique_with_a_bounded_card() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["selection"], "unique");
    let card = &envelope["items"][0]["card"];
    assert_eq!(card["key"], "def:planner.focus_task");
    assert_eq!(card["kind"], "command");
    assert_eq!(card["module"], "planner");
    assert_eq!(card["provenance"], "ir");
    // The typed signature and the requirement link ride the card.
    assert_eq!(card["signatures"][0]["id"], "planner.task_id");
    assert_eq!(card["requirements"][0], "PLANNER-REQ-001");
}

/// The exact full ID ranks first over every lexical and fuzzy result.
#[test]
fn search_exact_id_outranks_the_lexical_tail() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let output = lekalo_in(
        &project,
        &["--json", "search", "planner.focus_task", "--project", "."],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    assert_eq!(
        envelope["items"][0]["card"]["key"],
        "def:planner.focus_task"
    );
    assert_eq!(envelope["items"][0]["match"]["tier"], "exact-id");
    assert_eq!(envelope["items"][0]["match"]["edits"], 0);
}

/// Fuzzy is opt-in: without `--fuzzy` an edited query is empty; with it,
/// hits carry the lexical-fuzzy label and equal candidates are ambiguous.
#[test]
fn fuzzy_is_opt_in_labeled_and_ambiguity_honest() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let plain = json(&lekalo_in(
        &project,
        &["--json", "search", "fokus", "--project", "."],
    ));
    assert_eq!(
        exit_code(&lekalo_in(
            &project,
            &["--json", "search", "fokus", "--project", "."]
        )),
        0
    );
    assert_eq!(plain["selection"], "none");
    assert_eq!(plain["items"].as_array().map(Vec::len), Some(0));

    let fuzzy = json(&lekalo_in(
        &project,
        &["--json", "search", "fokus", "--fuzzy", "--project", "."],
    ));
    assert_eq!(fuzzy["selection"], "ambiguous");
    let items = fuzzy["items"].as_array().expect("items");
    assert!(items.len() >= 2);
    for item in items {
        assert_eq!(item["match"]["tier"], "fuzzy");
        assert_eq!(item["match"]["label"], "lexical-fuzzy");
    }
    let codes = fuzzy["reasonCodes"].as_array().expect("reason codes");
    assert!(codes.contains(&serde_json::Value::from("search.ambiguous")));
}

/// Same-name symbols across modules are ambiguous: both candidates stay
/// visible over the whole visible set, none is silently selected.
#[test]
fn same_name_symbols_are_ambiguous_not_silently_resolved() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let output = lekalo_in(
        &project,
        &["--json", "search", "get_status", "--project", "."],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["selection"], "ambiguous");
    let keys: Vec<&str> = envelope["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(keys.contains(&"def:notify.get_status"));
    assert!(keys.contains(&"def:planner.get_status"));
}

/// `find` requires at least one filter; `search` requires a nonempty
/// query — both refuse with `search.query-invalid` at exit 1.
#[test]
fn bare_find_refuses_with_the_registered_query_invalid_rule() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let output = lekalo_in(&project, &["--json", "find", "--project", "."]);
    assert_eq!(exit_code(&output), 1, "{}", stdout_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["status"], "invalid");
    assert_eq!(envelope["diagnostics"][0]["id"], "search.query-invalid");
    assert_eq!(envelope["diagnostics"][0]["registry_version"], "0.8.0");
    assert_eq!(envelope["diagnostics"][0]["code"], "LEK-SRCH-001");
}

/// A cursor bound to a different query/generation is refused with
/// `search.cursor-invalid`, never paged into another answer.
#[test]
fn cursor_refuses_a_foreign_query() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let first = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--limit",
            "2",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let envelope = json(&first);
    let cursor = envelope["page"]["next"]
        .as_str()
        .expect("more pages exist")
        .to_owned();
    let mismatch = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "query",
            "--limit",
            "2",
            "--cursor",
            &cursor,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&mismatch), 1);
    assert_eq!(
        json(&mismatch)["diagnostics"][0]["id"],
        "search.cursor-invalid"
    );
    let garbage = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--limit",
            "2",
            "--cursor",
            "AAAA:BBBB",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&garbage), 1);
    assert_eq!(
        json(&garbage)["diagnostics"][0]["data"]["detail"],
        "malformed"
    );
}

/// Concatenated pages equal the full ranked set: no repeats, no skips.
#[test]
fn pagination_concatenates_without_repeats_or_skips() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        // A cursor requires the immutable persisted generation, so the
        // warm (cache-backed) path pages here; the no-cache bypass is
        // refused with --cursor by contract.
        let mut args = vec![
            "--json".to_owned(),
            "search".to_owned(),
            "task".to_owned(),
            "--limit".to_owned(),
            "3".to_owned(),
            "--project".to_owned(),
            ".".to_owned(),
        ];
        if let Some(token) = &cursor {
            args.push("--cursor".to_owned());
            args.push(token.clone());
        }
        let output = lekalo_in(
            &project,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
        let envelope = json(&output);
        for item in envelope["items"].as_array().expect("items") {
            let key = item["card"]["key"].as_str().expect("key").to_owned();
            assert!(!seen.contains(&key), "repeat: {key}");
            seen.push(key);
        }
        match envelope["page"]["next"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
    }
    assert!(seen.len() >= 5, "the fixture pages through several rows");
}

/// The full lifecycle: build publishes a generation, status reports it
/// ready, and `cache clear --yes` removes it while migration custody
/// survives untouched.
#[test]
fn index_lifecycle_and_cache_custody() {
    let project = disposable_project();
    let build = lekalo_in(
        &project,
        &[
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&build), 0, "{}", stderr_text(&build));
    let receipts = stdout_text(&build);
    assert!(receipts.contains("search index built"));

    let status = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(exit_code(&status), 0, "{}", stderr_text(&status));
    let envelope = json(&status);
    assert_eq!(envelope["searchIndex"]["state"], "ready");
    assert!(envelope["searchIndex"]["cards"].as_u64().expect("cards") >= 20);

    // Cache custody: search generations live under the governed cache
    // home and are cleared by the explicit confirmation, migrations
    // survive.
    let migrations = project.join(".lekalo/cache/migrations");
    std::fs::create_dir_all(&migrations).expect("migrations dir");
    std::fs::write(migrations.join("keep.bin"), b"custody").expect("custody file");
    let clear = lekalo_in(&project, &["cache", "clear", "--yes", "--project", "."]);
    assert_eq!(exit_code(&clear), 0, "{}", stderr_text(&clear));
    assert!(migrations.join("keep.bin").exists(), "migration custody");
    assert!(
        !project.join(".lekalo/cache/search/current.json").exists(),
        "search generation cleared"
    );

    // A build under the bypass is a usage error: --no-cache never writes.
    let bypass = lekalo_in(
        &project,
        &[
            "--json",
            "--no-cache",
            "search-index",
            "build",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&bypass), 1);
    assert_eq!(json(&bypass)["diagnostics"][0]["id"], "cli.usage");
}

/// Producer admission is honest: without `--inputs` the optional
/// producer domains report `unknown` coverage, never fabricated
/// completeness; with the manifest the trace domain reports the exact
/// partial state of the committed aifhub manifest.
#[test]
fn coverage_states_are_honest_with_and_without_producers() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let without = json(&lekalo_in(
        &project,
        &["--json", "find", "--kind", "command", "--project", "."],
    ));
    let rows = without["coverage"].as_array().expect("coverage");
    let trace = rows
        .iter()
        .find(|row| row["domain"] == "trace")
        .expect("trace");
    assert_eq!(trace["state"], "unknown");

    let with = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    let rows = with["coverage"].as_array().expect("coverage");
    let trace = rows
        .iter()
        .find(|row| row["domain"] == "trace")
        .expect("trace");
    assert_eq!(trace["state"], "partial", "the aifhub manifest is partial");
}

/// Stale waiver rows are excluded by default and admitted only under
/// `--include-stale`; the active waiver stays beside its subject.
#[test]
fn stale_waivers_are_excluded_until_explicitly_included() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let run = |args: &[&str]| {
        let output = lekalo_in(&project, args);
        assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
        json(&output)
    };
    let revoked = run(&[
        "--json",
        "find",
        "--waiver",
        "lint-focus-debt-revoked",
        "--project",
        ".",
        "--inputs",
        "evidence/search-inputs.json",
    ]);
    assert_eq!(revoked["selection"], "none");
    let included = run(&[
        "--json",
        "find",
        "--waiver",
        "lint-focus-debt-revoked",
        "--include-stale",
        "--project",
        ".",
        "--inputs",
        "evidence/search-inputs.json",
    ]);
    assert_eq!(included["selection"], "unique");
    assert_eq!(
        included["items"][0]["card"]["key"],
        "waiv:lint-focus-debt-revoked"
    );
    assert_eq!(included["items"][0]["card"]["freshness"], "stale");
    let active = run(&[
        "--json",
        "find",
        "--waiver",
        "lint-focus-debt",
        "--project",
        ".",
        "--inputs",
        "evidence/search-inputs.json",
    ]);
    let keys: Vec<&str> = active["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(keys.contains(&"waiv:lint-focus-debt"));
    assert!(keys.contains(&"def:planner.focus_task"));
}

/// Zero matches is a valid answer, never an error.
#[test]
fn zero_matches_is_valid() {
    let project = fixture_path();
    let _guard = CWD_LOCK.lock().expect("lock");
    let output = lekalo_in(
        &project,
        &["--json", "search", "xyzzynomatch", "--project", "."],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["selection"], "none");
}
