//! Issue #93 CLI tests for the `lekalo search` / `lekalo find` /
//! `lekalo search-index` handoff: the hermetic planner fixture over the
//! accepted envelope, exact versus fuzzy semantics, the declared /
//! detected effect trust split, provider-qualified requirement
//! ambiguity, stale-row exclusion, cursor generation binding, cache
//! custody, and the closed-grammar refusals.
//!
//! Cache hygiene: every test runs in its own TempDir-owned disposable
//! copy of the fixture with the guard held for the whole test, so no
//! test ever writes the tracked fixture's cache home and no parallel
//! run can race a sibling (the tracked fixture stays byte-identical).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// Byte-sorted-key compact JSON — byte-identical to the core's
/// canonical JSON, so a planted store sidecar carries the exact bytes
/// the (r6) content-addressed checks hashed.
fn canonical_json_bytes(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            let body: Vec<String> = keys
                .iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key serializes"),
                        canonical_json_bytes(&map[*key])
                    )
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        serde_json::Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical_json_bytes).collect();
            format!("[{}]", body.join(","))
        }
        other => serde_json::to_string(other).expect("scalar serializes"),
    }
}

const FIXTURE: &str = "tests/fixtures/search/planner";

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

/// A disposable copy of the fixture in its own temp dir: every cache-
/// enabled test runs here and never touches the tracked fixture's
/// cache home. The TempDir guard is returned and held by the caller
/// for the whole test (forgetting it would leak the directory).
fn disposable_project() -> (PathBuf, TempDir) {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    copy_dir(&fixture_path(), &project);
    std::fs::remove_dir_all(project.join(".lekalo")).ok();
    (alias_free_path(&project), temp)
}

fn fixture_path() -> PathBuf {
    alias_free_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(FIXTURE),
    )
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

/// Build once in a disposable project with the standard manifest.
fn build_once(project: &Path) -> Output {
    lekalo_in(
        project,
        &[
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    )
}

/// `find --id` answers unique on the valid envelope with the bounded
/// selection card.
#[test]
fn find_by_exact_id_answers_unique_with_a_bounded_card() {
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
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

/// A cursor's fuzzy toggle changes the page semantics mid-pagination:
/// the token binds the mode, so the replay refuses.
#[test]
fn cursor_refuses_a_fuzzy_toggle() {
    let (project, _guard) = disposable_project();
    let first = lekalo_in(
        &project,
        &[
            "--json",
            "search",
            "focus",
            "--limit",
            "2",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&first), 0, "{}", stderr_text(&first));
    let cursor = json(&first)["page"]["next"]
        .as_str()
        .expect("more pages exist")
        .to_owned();
    let toggled = lekalo_in(
        &project,
        &[
            "--json",
            "search",
            "focus",
            "--fuzzy",
            "--limit",
            "2",
            "--cursor",
            &cursor,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&toggled), 1);
    assert_eq!(
        json(&toggled)["diagnostics"][0]["id"],
        "search.cursor-invalid"
    );
}

/// A cursor whose persisted generation was removed is refused: a page
/// never silently recomputes from different bytes.
#[test]
fn cursor_refuses_a_removed_generation() {
    let (project, _guard) = disposable_project();
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
    let cursor = json(&first)["page"]["next"]
        .as_str()
        .expect("more pages exist")
        .to_owned();
    std::fs::remove_dir_all(project.join(".lekalo/cache/search")).expect("remove store");
    let replay = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--limit",
            "2",
            "--cursor",
            &cursor,
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&replay), 1);
    assert_eq!(
        json(&replay)["diagnostics"][0]["id"],
        "search.cursor-invalid"
    );
}

/// Concatenated pages equal the full ranked set: no repeats, no skips.
#[test]
fn pagination_concatenates_without_repeats_or_skips() {
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
    let build = build_once(&project);
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

    // Status under the bypass refuses too: the durable read-only oracle
    // has no honest no-cache answer.
    let status_bypass = lekalo_in(
        &project,
        &[
            "--json",
            "--no-cache",
            "search-index",
            "status",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&status_bypass), 1);
    assert_eq!(json(&status_bypass)["diagnostics"][0]["id"], "cli.usage");
}

/// Producer admission is honest: without `--inputs` the optional
/// producer domains report `unknown` coverage, never fabricated
/// completeness; with the manifest the trace domain reports the exact
/// partial state of the committed aifhub manifest; and a domain the
/// manifest does not admit stays `unknown` even when a manifest exists
/// (absence of admitted evidence is never read as proof).
#[test]
fn coverage_states_are_honest_with_and_without_producers() {
    let (project, _guard) = disposable_project();
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
    // Every producer domain always reports a row: no domain silently
    // disappears because a manifest admitted only some producers. The
    // error-contract and ownership/profile families carry their own
    // rows alongside the original seven.
    assert_eq!(rows.len(), 9, "all nine domains report coverage");

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

    // An effects-only manifest leaves diagnostics `unknown`, and a
    // diagnostic-shaped query says so instead of a silent valid/none.
    let manifest = serde_json::json!({
        "schemaVersion": "lekalo/search-inputs/v0.8.0",
        "identity": "dev.lekalo.search-inputs@0.8.0",
        "projectId": "planner",
        "inputs": [
            {"kind": "effects", "path": "evidence/effects.detected.json",
             "identity": "dev.lekalo.effects@0.2.16"}
        ]
    });
    std::fs::write(
        project.join("evidence/effects-only.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest bytes"),
    )
    .expect("write effects-only manifest");
    let effects_only = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "diagnostic",
            "--project",
            ".",
            "--inputs",
            "evidence/effects-only.json",
        ],
    ));
    assert_eq!(
        exit_code(&lekalo_in(
            &project,
            &[
                "find",
                "--kind",
                "diagnostic",
                "--project",
                ".",
                "--inputs",
                "evidence/effects-only.json"
            ]
        )),
        0
    );
    let rows = effects_only["coverage"].as_array().expect("coverage");
    let diagnostics = rows
        .iter()
        .find(|row| row["domain"] == "diagnostics")
        .expect("diagnostics row");
    assert_eq!(diagnostics["state"], "unknown");
    let codes = effects_only["reasonCodes"].as_array().expect("codes");
    assert!(
        codes.contains(&serde_json::Value::from("search.coverage-incomplete")),
        "the needed domain's incompleteness is disclosed"
    );
}

/// Stale waiver rows are excluded by default and admitted only under
/// `--include-stale`; the active waiver stays beside its subject.
#[test]
fn stale_waivers_are_excluded_until_explicitly_included() {
    let (project, _guard) = disposable_project();
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
    let (project, _guard) = disposable_project();
    let output = lekalo_in(
        &project,
        &["--json", "search", "xyzzynomatch", "--project", "."],
    );
    assert_eq!(exit_code(&output), 0, "{}", stdout_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["status"], "valid");
    assert_eq!(envelope["selection"], "none");
}

/// The status verdict is independent of the caller's working directory
/// and of where the manifest lives: built with a nonstandard `--inputs`
/// path, a drifted pinned producer is `stale` from the parent directory
/// (never a false `ready`), and a clean rebuild is `ready` again.
#[test]
fn status_verdict_is_independent_of_cwd_and_manifest_location() {
    let (project, _guard) = disposable_project();
    // Nonstandard manifest location the CLI explicitly accepts.
    std::fs::copy(
        project.join("evidence/search-inputs.json"),
        project.join("evidence/custom-inputs.json"),
    )
    .expect("copy manifest");
    let build = lekalo_in(
        &project,
        &[
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/custom-inputs.json",
        ],
    );
    assert_eq!(exit_code(&build), 0, "{}", stderr_text(&build));

    // The documented --project selector from OUTSIDE the project root.
    let parent = project.parent().expect("parent dir");
    let status = lekalo_in(
        parent,
        &["--json", "search-index", "status", "--project", "project"],
    );
    assert_eq!(exit_code(&status), 0, "{}", stderr_text(&status));
    assert_eq!(json(&status)["searchIndex"]["state"], "ready");

    // Drift one pinned producer by one byte: stale, from outside, even
    // though the conventional manifest path never existed.
    let mut effects = std::fs::read_to_string(project.join("evidence/effects.detected.json"))
        .expect("read effects");
    effects.push(' ');
    std::fs::write(project.join("evidence/effects.detected.json"), effects).expect("drift effects");
    let drifted = lekalo_in(
        parent,
        &["--json", "search-index", "status", "--project", "project"],
    );
    assert_eq!(exit_code(&drifted), 0, "{}", stderr_text(&drifted));
    let envelope = json(&drifted);
    assert_eq!(envelope["searchIndex"]["state"], "stale");
    let codes = envelope["reasonCodes"].as_array().expect("codes");
    assert!(codes.contains(&serde_json::Value::from("search.index-stale")));

    // Model drift is detected the same way (the recomputed snapshot
    // digest changes even with every pin file byte-identical): a changed
    // module description changes the compiled Model's cards.
    std::fs::write(project.join("evidence/effects.detected.json"), {
        let mut restored =
            std::fs::read_to_string(project.join("evidence/effects.detected.json")).expect("read");
        restored.pop();
        restored
    })
    .expect("restore effects");
    let description = project.join("lekalo/modules/planner/module.yaml");
    let original = std::fs::read_to_string(&description).expect("read module");
    let drifted_module = original.replace(
        "\"Task focus planning module\"",
        "\"Task focus planning module, now with drift\"",
    );
    assert!(drifted_module != original, "fixture shape");
    std::fs::write(&description, drifted_module).expect("drift module");
    let model_drift = lekalo_in(
        parent,
        &["--json", "search-index", "status", "--project", "project"],
    );
    assert_eq!(exit_code(&model_drift), 0);
    assert_eq!(json(&model_drift)["searchIndex"]["state"], "stale");

    // A rebuild under the drifted model republishes honestly.
    let rebuild = lekalo_in(
        &project,
        &[
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/custom-inputs.json",
        ],
    );
    assert_eq!(exit_code(&rebuild), 0, "{}", stderr_text(&rebuild));
    let fresh = lekalo_in(
        parent,
        &["--json", "search-index", "status", "--project", "project"],
    );
    assert_eq!(json(&fresh)["searchIndex"]["state"], "ready");
}

/// A removed manifest is admission drift, never an invisible policy
/// change: the generation's recorded pins AND its recorded admitting
/// configuration both re-verify, so removing the manifest file (or
/// editing its admission set) is `stale` — the oracle re-reads the
/// admission config it recorded, not just the producer bytes.
#[test]
fn removed_manifest_is_stale_not_ready() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    std::fs::remove_file(project.join("evidence/search-inputs.json")).expect("remove manifest");
    let status = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(exit_code(&status), 0, "{}", stderr_text(&status));
    // The admitting configuration was revoked: the recorded pins may
    // still verify byte for byte, but the run that admitted them no
    // longer exists as configured. Removal is an explicit drift.
    assert_eq!(json(&status)["searchIndex"]["state"], "stale");
    // And a drifted producer under the removed manifest is still
    // stale: the oracle reads the pins, not the pointer.
    let effects = project.join("evidence/effects.detected.json");
    let mut body = std::fs::read_to_string(&effects).expect("read effects");
    body.push(' ');
    std::fs::write(&effects, body).expect("drift effects");
    let drifted = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(json(&drifted)["searchIndex"]["state"], "stale");
}

/// Poisoned storage never reports successful reuse: a corrupted
/// generation payload is atomically repaired by the next update, and
/// status tells the truth before and after.
#[test]
fn poisoned_generation_is_repaired_by_rebuild() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let current = std::fs::read_to_string(project.join(".lekalo/cache/search/current.json"))
        .expect("read pointer");
    let digest = serde_json::from_str::<serde_json::Value>(&current).expect("parse pointer")
        ["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let generation = project.join(".lekalo/cache/search").join(&digest);
    std::fs::write(&generation, b"{}").expect("poison payload");
    let status = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(json(&status)["searchIndex"]["state"], "corrupt");
    // Update refuses to reuse the poisoned bytes and republishes.
    let update = lekalo_in(
        &project,
        &[
            "search-index",
            "update",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&update), 0, "{}", stderr_text(&update));
    assert!(
        !stdout_text(&update).contains("\"reused\":true"),
        "poisoned storage is never reused"
    );
    let repaired = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(json(&repaired)["searchIndex"]["state"], "ready");
}

/// A valid generation copied beneath a foreign digest name is an alias,
/// not a generation: status reports corrupt, never ready.
#[test]
fn aliased_generation_is_corrupt() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let current = std::fs::read_to_string(project.join(".lekalo/cache/search/current.json"))
        .expect("read pointer");
    let digest = serde_json::from_str::<serde_json::Value>(&current).expect("parse pointer")
        ["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let payload =
        std::fs::read(project.join(".lekalo/cache/search").join(&digest)).expect("read generation");
    // Re-hash the payload under a doctored internal digest so only the
    // filename disagrees (the pure alias case).
    let doctored = String::from_utf8(payload.clone()).expect("utf8").replace(
        &format!("\"sha256:{digest}\""),
        &format!("\"sha256:{}\"", "a".repeat(64)),
    );
    let alias = project.join(".lekalo/cache/search").join("b".repeat(64));
    std::fs::write(&alias, doctored).expect("write alias");
    let pointer = format!("{{\"digest\":\"{}\"}}", "b".repeat(64));
    std::fs::write(project.join(".lekalo/cache/search/current.json"), pointer)
        .expect("point at alias");
    let status = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(json(&status)["searchIndex"]["state"], "corrupt");
    let _ = payload;
}

/// The `waiver` card kind is a legal exact filter: schema parity holds
/// for every published enum member.
#[test]
fn waiver_kind_filter_admits_waiver_cards() {
    let (project, _guard) = disposable_project();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "waiver",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["status"], "valid");
    let items = envelope["items"].as_array().expect("items");
    assert!(!items.is_empty(), "waiver cards are filterable");
    for item in items {
        assert_eq!(item["card"]["kind"], "waiver");
    }
}

/// An unsupported manifest contract version is typed end-to-end:
/// `search.version-unsupported` with the unsupported-version status and
/// exit 5, never a collapsed registry-invalid/exit 1.
#[test]
fn unsupported_manifest_version_is_typed_exit_5() {
    let (project, _guard) = disposable_project();
    let manifest = r#"{"schemaVersion":"lekalo/search-inputs/v99.0.0","identity":"dev.lekalo.search-inputs@99.0.0","projectId":"planner","inputs":[]}"#;
    std::fs::write(project.join("evidence/future-inputs.json"), manifest)
        .expect("write future manifest");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--project",
            ".",
            "--inputs",
            "evidence/future-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 5, "{}", stdout_text(&output));
    let envelope = json(&output);
    assert_eq!(envelope["status"], "unsupported-version");
    assert_eq!(
        envelope["diagnostics"][0]["id"],
        "search.version-unsupported"
    );
    assert_eq!(envelope["diagnostics"][0]["code"], "LEK-SRCH-002");
}

/// A sensitive native identity is withheld whole even when the record's
/// location is an allowed path: an allowed location never authorizes
/// another field's sensitive path, in cards or qualified identities.
#[test]
fn mixed_field_sensitive_identity_is_withheld() {
    let (project, _guard) = disposable_project();
    let index_path = project.join("evidence/observed.index.json");
    let original = std::fs::read_to_string(&index_path).expect("read index");
    // Text-level splice: the canonical wire bytes stay canonical, so the
    // exercised path is identity admission, not a re-serialization
    // refusal.
    let tampered = original.replacen(
        "\"stable_key\":\"apps/api/src/planner/focus-task.ts#focusTask\"",
        "\"stable_key\":\".env#SECRET_R2_CANARY\"",
        1,
    );
    assert!(tampered != original, "fixture shape");
    std::fs::write(&index_path, tampered).expect("write index");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.focus_task",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let rendered = stdout_text(&output);
    assert!(
        !rendered.contains("SECRET_R2_CANARY"),
        "the sensitive identity never reaches the response"
    );
    let envelope = json(&output);
    assert_eq!(envelope["status"], "valid");
    let card = &envelope["items"][0]["card"];
    // The card discloses the withholding instead of leaking the path.
    assert_eq!(card["completeness"], "withheld");
}

/// Every recorded native candidate spelling answers the exact tier: the
/// second candidate (`path#focusTaskById`) is exact-qualified and
/// `--native-symbol` finds it on the binding card.
#[test]
fn every_candidate_native_spelling_is_exact_lookup_able() {
    let (project, _guard) = disposable_project();
    let spelling = "apps/api/src/planner/focus-task.ts#focusTaskById";
    let exact = json(&lekalo_in(
        &project,
        &[
            "--json",
            "search",
            spelling,
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    assert_eq!(
        exit_code(&lekalo_in(
            &project,
            &[
                "search",
                spelling,
                "--project",
                ".",
                "--inputs",
                "evidence/search-inputs.json"
            ]
        )),
        0
    );
    assert_eq!(exact["items"][0]["card"]["key"], "bind:planner.focus_task");
    assert_eq!(exact["items"][0]["match"]["tier"], "exact-qualified");
    let by_symbol = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "binding",
            "--native-symbol",
            spelling,
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    assert_eq!(
        exit_code(&lekalo_in(
            &project,
            &[
                "find",
                "--kind",
                "binding",
                "--native-symbol",
                spelling,
                "--project",
                ".",
                "--inputs",
                "evidence/search-inputs.json"
            ]
        )),
        0
    );
    let keys: Vec<&str> = by_symbol["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(
        keys.contains(&"bind:planner.focus_task"),
        "the second candidate answers the exact native-symbol filter"
    );
}

/// The recorded currency state stays the card's freshness: a stale
/// observed record is excluded by default and visible only under
/// `--include-stale`, never laundered into an as-recorded fact.
#[test]
fn stale_observed_state_is_excluded_by_default() {
    let (project, _guard) = disposable_project();
    let index_path = project.join("evidence/observed.index.json");
    let original = std::fs::read_to_string(&index_path).expect("read index");
    // Text-level splice keeps the canonical wire bytes canonical.
    let tampered = original.replacen(
        "\"state\":\"current\",\"promoted\":false",
        "\"state\":\"stale\",\"promoted\":false",
        1,
    );
    assert!(tampered != original, "fixture shape");
    std::fs::write(&index_path, tampered).expect("write index");
    let without = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "binding",
            "--native-symbol",
            "apps/api/src/planner/focus-task.ts#focusTask",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    assert_eq!(without["selection"], "none", "stale evidence is excluded");
    let included = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "binding",
            "--native-symbol",
            "apps/api/src/planner/focus-task.ts#focusTask",
            "--include-stale",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    assert_eq!(included["selection"], "ambiguous");
    assert_eq!(included["items"][0]["card"]["freshness"], "stale");
}

/// Trace joins resolve through the nodes' typed fields, not their node
/// IDs: the scenario whose node ID differs from its semantic ID joins
/// the requirement chain, so `--requirement` + `--requirement-source`
/// finds it.
#[test]
fn trace_joins_resolve_typed_scenario_ids() {
    let (project, _guard) = disposable_project();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "scenario",
            "--requirement",
            "PLANNER-REQ-001",
            "--requirement-source",
            "openspec",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    let keys: Vec<&str> = envelope["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(
        keys.contains(&"def:planner.focus_flow"),
        "the requirement-to-scenario chain resolves: {keys:?}"
    );
}

/// A schema-valid producer document is decoded through the closed
/// published grammar: removing a required envelope member, or an
/// unknown provenance type, is a typed refusal — never a silently
/// skipped row with complete coverage.
#[test]
fn closed_effects_decoding_refuses_schema_invalid_documents() {
    let (project, _guard) = disposable_project();
    let original = std::fs::read_to_string(project.join("evidence/effects.detected.json"))
        .expect("read effects");
    // Drop the required `modelVersion` member.
    let no_model: serde_json::Value = {
        let mut value = serde_json::from_str::<serde_json::Value>(&original).expect("parse");
        value
            .as_object_mut()
            .expect("object")
            .remove("modelVersion");
        value
    };
    std::fs::write(
        project.join("evidence/effects.detected.json"),
        serde_json::to_vec(&no_model).expect("serialize"),
    )
    .expect("write effects");
    let refused = lekalo_in(
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
    );
    assert_eq!(exit_code(&refused), 1);
    assert_eq!(
        json(&refused)["diagnostics"][0]["id"],
        "search.input-invalid"
    );

    // A provenance type outside the closed pair refuses (never skips).
    let untrusted: serde_json::Value = {
        let mut value = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(project.join("evidence/effects.detected.json")).expect("read"),
        )
        .expect("parse");
        // restore modelVersion
        value["modelVersion"] = serde_json::Value::String("0.2.16".to_owned());
        value["effects"][0]["provenance"]["type"] =
            serde_json::Value::String("untrusted".to_owned());
        value
    };
    std::fs::write(
        project.join("evidence/effects.detected.json"),
        serde_json::to_vec(&untrusted).expect("serialize"),
    )
    .expect("write effects");
    let skipped = lekalo_in(
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
    );
    assert_eq!(exit_code(&skipped), 1, "unknown provenance type refuses");
}

/// A canonical effect subject the Model does not declare is refused at
/// admission: a producer cannot invent entities or fields beside the
/// genuine facts.
#[test]
fn invented_canonical_subject_refuses() {
    let (project, _guard) = disposable_project();
    let effects_path = project.join("evidence/effects.detected.json");
    let mut effects: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&effects_path).expect("read"))
            .expect("parse");
    effects["effects"][0]["resource"]["id"] =
        serde_json::Value::String("planner.nonexistent".to_owned());
    std::fs::write(
        &effects_path,
        serde_json::to_vec(&effects).expect("serialize"),
    )
    .expect("write effects");
    let output = lekalo_in(
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
    );
    assert_eq!(exit_code(&output), 1);
    assert_eq!(
        json(&output)["diagnostics"][0]["data"]["detail"],
        "subject-unknown"
    );
}

/// A tampered per-record revision is refused: a record rewritten beside
/// a foreign revision never silently merges as same-project evidence.
#[test]
fn tampered_observed_revision_refuses() {
    let (project, _guard) = disposable_project();
    let index_path = project.join("evidence/observed.index.json");
    let original = std::fs::read_to_string(&index_path).expect("read index");
    // Text-level splice: the canonical wire bytes stay canonical, so the
    // refusal is the revision custody check, not a re-serialization
    // artifact.
    let revision = "sha256:0e260666954d4e626a2971a25ace7d87782aa13ec167995a2b0ce450254c6e61";
    let provenance = format!(
        "{{\"origin\":\"observed\",\"confidence\":\"high\",\"adapter\":\"lekalo-target-node-typescript\",\"revision\":\"{revision}\"}}"
    );
    let forged = format!(
        "{{\"origin\":\"observed\",\"confidence\":\"high\",\"adapter\":\"lekalo-target-node-typescript\",\"revision\":\"sha256:{}\"}}",
        "f".repeat(64)
    );
    assert!(original.contains(&provenance), "fixture shape");
    let tampered = original.replacen(&provenance, &forged, 1);
    std::fs::write(&index_path, tampered).expect("write index");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "binding",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 1);
    assert_eq!(
        json(&output)["diagnostics"][0]["data"]["detail"],
        "revision-mismatch"
    );
}

/// A ninth typed input is a filterable fact, not a truncation casualty:
/// exact filters run over the complete typed facts while the wire card
/// stays bounded with a disclosed omitted count.
#[test]
fn ninth_input_type_is_still_filterable() {
    let (project, _guard) = disposable_project();
    // Extend the disposable model: a scalar type plus a command with
    // nine typed inputs, the ninth of a distinct type.
    let entities = project.join("lekalo/modules/planner/entities.yaml");
    let mut body = std::fs::read_to_string(&entities).expect("read entities");
    // Continue the file's existing `definitions:` list (a second root
    // key would be a duplicate-key refusal).
    body.push_str(
        "  - id: planner.priority\n    kind: scalar\n    version: 1\n    description: \"Sort priority\"\n    base: string\n",
    );
    std::fs::write(&entities, body).expect("write entities");
    let commands = project.join("lekalo/modules/planner/commands.yaml");
    let mut inputs = String::new();
    for index in 0..9 {
        let id = if index == 8 {
            "planner.priority"
        } else {
            "planner.task_id"
        };
        inputs.push_str(&format!(
            "      - name: field{index}\n        type: \"{id}\"\n        required: true\n"
        ));
    }
    let command = format!(
        "  - id: planner.wide_task\n    kind: command\n    version: 1\n    description: \"Wide input command\"\n    input:\n{inputs}  - id: planner.wide_effect\n    kind: effect\n    version: 1\n    description: \"Wide effect\"\n    operation: update\n    entity: \"planner.user_task_planning\"\n"
    );
    std::fs::write(&commands, {
        let original = std::fs::read_to_string(&commands).expect("read commands");
        original + &command
    })
    .expect("write commands");
    let ninth = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--input-type",
            "planner.priority",
            "--project",
            ".",
        ],
    ));
    assert_eq!(
        exit_code(&lekalo_in(
            &project,
            &[
                "find",
                "--kind",
                "command",
                "--input-type",
                "planner.priority",
                "--project",
                "."
            ]
        )),
        0
    );
    assert_eq!(ninth["selection"], "unique");
    assert_eq!(ninth["items"][0]["card"]["key"], "def:planner.wide_task");
    // The wire card stays bounded: eight signatures shown, the rest
    // counted in the disclosed omitted total.
    let card = &ninth["items"][0]["card"];
    assert_eq!(
        card["signatures"].as_array().expect("signatures").len(),
        8,
        "the response projection respects the published bound"
    );
    assert!(
        card["omitted"].as_u64().expect("omitted") >= 1,
        "the omission is disclosed"
    );
}

/// A schema-valid 65-effect envelope produces a schema-valid response:
/// the outgoing card is capped at the published 64-effect bound with
/// the omission disclosed, never 66 unbounded effects.
#[test]
fn oversize_effect_card_is_capped_with_disclosed_omission() {
    let (project, _guard) = disposable_project();
    // Give the disposable model an operation with 64+ detectable
    // subjects: one entity with many fields, one command. The evidence
    // envelope then carries 65 schema-valid evidence rows for it.
    let entities = project.join("lekalo/modules/planner/entities.yaml");
    let mut body = std::fs::read_to_string(&entities).expect("read entities");
    let mut fields = String::new();
    for index in 0..70 {
        fields.push_str(&format!(
            "      - name: wide_field_{index}\n        type: \"planner.text\"\n        required: true\n"
        ));
    }
    body.push_str(&format!(
        "  - id: planner.wide_entity\n    kind: entity\n    version: 1\n    description: \"Wide entity\"\n    fields:\n{fields}    identity:\n      - wide_field_0\n"
    ));
    std::fs::write(&entities, body).expect("write entities");
    let commands = project.join("lekalo/modules/planner/commands.yaml");
    let command = "  - id: planner.wide_writer\n    kind: command\n    version: 1\n    description: \"Wide writer\"\n  - id: planner.wide_write_effect\n    kind: effect\n    version: 1\n    description: \"Wide write\"\n    operation: update\n    entity: \"planner.wide_entity\"\n";
    std::fs::write(&commands, {
        let original = std::fs::read_to_string(&commands).expect("read commands");
        original + command
    })
    .expect("write commands");

    // 65 schema-valid evidence rows (declared rows are rebuilt from the
    // IR, so every row here is evidence-provenance).
    let mut rows = Vec::new();
    for index in 0..65 {
        rows.push(serde_json::json!({
            "action": "set",
            "confidence": "extracted",
            "effect": format!("#{index}"),
            "field": format!("wide_field_{index}"),
            "kind": "write-field",
            "occurrence": 0,
            "operation": "operation:planner.wide_writer",
            "provenance": {
                "type": "evidence",
                "adapterId": "vendor.example/scan",
                "targetId": "vendor.example/node-typescript",
                "protocolVersion": "0.2.16",
                "evidenceDigest": format!("sha256:{}", format!("{:064x}", index)),
                "trust": "current"
            },
            "resource": {"id": "planner.wide_entity", "kind": "canonical"},
            "sensitivity": null,
            "transactionGroup": null
        }));
    }
    let envelope = serde_json::json!({
        "schemaVersion": "lekalo/effects/v0.2.16",
        "identity": "dev.lekalo.effects@0.2.16",
        "modelVersion": "0.2.16",
        "project": "planner",
        "metadata": {
            "declaredCount": 0,
            "detectedCount": 65,
            "effectCount": 65,
            "envelopeCount": 1,
            "operationCount": 1
        },
        "effects": rows
    });
    // Point the manifest at the oversized evidence.
    std::fs::write(
        project.join("evidence/effects-wide.json"),
        serde_json::to_vec(&envelope).expect("serialize envelope"),
    )
    .expect("write evidence");
    let manifest = r#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[{"kind":"effects","path":"evidence/effects-wide.json","identity":"dev.lekalo.effects@0.2.16"}]}"#;
    std::fs::write(project.join("evidence/wide-inputs.json"), manifest).expect("write manifest");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--id",
            "planner.wide_writer",
            "--project",
            ".",
            "--inputs",
            "evidence/wide-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    let card = &envelope["items"][0]["card"];
    let effects = card["effects"].as_array().expect("effects");
    assert!(
        effects.len() <= 64,
        "the outgoing card respects the published 64-effect bound"
    );
    assert!(
        card["omitted"].as_u64().expect("omitted") >= 1,
        "the effect omission is disclosed"
    );
}

/// The manifest's own posting budget is honored: a `maxPostings: 1`
/// build refuses (typed, denied) instead of publishing the global-cap
/// index with an unchanged digest.
#[test]
fn manifest_posting_budget_is_honored() {
    let (project, _guard) = disposable_project();
    let manifest = r#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[],"limits":{"maxPostings":1}}"#;
    std::fs::write(project.join("evidence/tiny-inputs.json"), manifest).expect("write manifest");
    let build = lekalo_in(
        &project,
        &[
            "--json",
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/tiny-inputs.json",
        ],
    );
    assert_eq!(exit_code(&build), 1, "{}", stdout_text(&build));
    let envelope = json(&build);
    assert_eq!(envelope["status"], "invalid");
    let codes = envelope["reasonCodes"].as_array().expect("codes");
    assert!(codes.contains(&serde_json::Value::from("search.result-limit")));
}

/// The mandatory producer families (F11) answer with positive probes:
/// error-contract signatures, diagnostic-set finding cards, and the
/// artifact-ownership/profile snapshot joins.
#[test]
fn producer_families_answer_with_typed_facts() {
    let (project, _guard) = disposable_project();
    let run = |args: &[&str]| {
        let output = lekalo_in(&project, args);
        assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
        json(&output)
    };
    const INPUTS: [&str; 4] = ["--project", ".", "--inputs", "evidence/search-inputs.json"];
    fn with_inputs<'a>(extra: &[&'a str]) -> Vec<&'a str> {
        let mut all: Vec<&str> = extra.to_vec();
        all.extend_from_slice(&INPUTS);
        all
    }
    // Error signatures: the validated registry slice binds operation
    // errors; `--error` resolves through the typed signatures.
    let errors = run(&with_inputs(&[
        "--json",
        "find",
        "--error",
        "planner.store_unavailable",
    ]));
    let keys: Vec<&str> = errors["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(
        keys.contains(&"def:planner.focus_task"),
        "error signatures resolve: {keys:?}"
    );
    // Error negative: an undeclared error is a valid none.
    let error_none = run(&with_inputs(&[
        "--json",
        "find",
        "--error",
        "planner.nonexistent_error",
    ]));
    assert_eq!(error_none["status"], "valid");
    assert_eq!(error_none["selection"], "none");
    // Diagnostic set: finding cards with the rule identity and severity
    // subkind, joined to their subjects.
    let diagnostics = run(&with_inputs(&["--json", "find", "--kind", "diagnostic"]));
    let items = diagnostics["items"].as_array().expect("items");
    assert!(items.len() >= 2, "finding cards are projected");
    assert!(items
        .iter()
        .all(|item| item["card"]["kind"] == "diagnostic"));
    let by_rule = run(&with_inputs(&[
        "--json",
        "find",
        "--diagnostic",
        "planner.focus-concurrency",
    ]));
    // The reverse join answers the subject beside its finding card.
    assert_eq!(by_rule["selection"], "multiple");
    let rule_keys: Vec<&str> = by_rule["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(rule_keys.contains(&"diag:focus-concurrency-001"));
    assert!(rule_keys.contains(&"def:planner.focus_task"));
    // Profile membership: a recorded fact, never a conformance claim.
    let profile = run(&with_inputs(&[
        "--json",
        "find",
        "--profile",
        "focus-critical",
    ]));
    let keys: Vec<&str> = profile["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(keys.contains(&"def:planner.focus_task"), "{keys:?}");
    assert!(keys.contains(&"def:planner.refocus_task"), "{keys:?}");
    let profile_none = run(&with_inputs(&[
        "--json",
        "find",
        "--profile",
        "no-such-profile",
    ]));
    assert_eq!(profile_none["status"], "valid");
    assert_eq!(profile_none["selection"], "none");
    // Artifact ownership: the artifact-owned definition answers the
    // ownership filter through producer facts.
    let ownership = run(&with_inputs(&["--json", "find", "--ownership", "project"]));
    let keys: Vec<&str> = ownership["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(keys.contains(&"def:planner.focus_task"), "{keys:?}");
}

/// The diagnostic-set wire decodes closed: an unknown member and a
/// forged envelope count are typed refusals, never skipped findings.
#[test]
fn diagnostic_set_wire_decodes_closed() {
    let (project, _guard) = disposable_project();
    let original = std::fs::read_to_string(project.join("evidence/diagnostics.json"))
        .expect("read diagnostics");
    for tampered in [
        original.replacen(
            "\"severity\": \"warning\"",
            "\"severity\": \"warning\",\n      \"unfamiliar\": true",
            1,
        ),
        original.replacen("\"findingCount\": 2", "\"findingCount\": 99", 1),
    ] {
        std::fs::write(project.join("evidence/diagnostics.json"), &tampered)
            .expect("write hostile diagnostics");
        let output = lekalo_in(
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
        );
        assert_eq!(exit_code(&output), 1);
        assert_eq!(
            json(&output)["diagnostics"][0]["id"],
            "search.input-invalid"
        );
    }
    // Restore and confirm the honest document admits again.
    std::fs::write(project.join("evidence/diagnostics.json"), original)
        .expect("restore diagnostics");
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "diagnostic",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
}

/// The tracked fixture never regains cache residue: this test reads it
/// only under `--no-cache` and asserts the cache home stays absent.
#[test]
fn tracked_fixture_stays_cache_clean() {
    let project = fixture_path();
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "--no-cache",
            "find",
            "--id",
            "planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    assert!(
        !project.join(".lekalo/cache/search").exists(),
        "the tracked fixture must not gain search-cache residue"
    );
}

/// A candidate's own `.path` field is admitted independently under the
/// frozen privacy grammar: an ordinary native spelling never smuggles a
/// sensitive path value beside it, and the withheld card discloses the
/// withholding without ever returning the rejected path (issue #93 r4).
#[test]
fn candidate_sensitive_path_is_withheld_whole() {
    let (project, _guard) = disposable_project();
    // The hostile edit: only the first candidate's own path field,
    // preserving its ordinary `path#symbol` native spelling (the
    // record's location and stable key stay admitted).
    let index = project.join("evidence/observed.index.json");
    let body = std::fs::read_to_string(&index).expect("read observed");
    let marker = "\"candidates\":[{";
    let (head, tail) = body.split_once(marker).expect("candidates");
    let hostile_tail = tail.replacen(
        "\"path\":\"apps/api/src/planner/focus-task.ts\"",
        "\"path\":\".env\"",
        1,
    );
    assert_ne!(hostile_tail, tail, "the hostile edit must land");
    let hostile = format!("{head}{marker}{hostile_tail}");
    std::fs::write(&index, hostile).expect("write observed");
    assert_eq!(exit_code(&build_once(&project)), 0, "{}", {
        let probe = lekalo_in(
            &project,
            &[
                "--json",
                "search-index",
                "build",
                "--project",
                ".",
                "--inputs",
                "evidence/search-inputs.json",
            ],
        );
        stderr_text(&probe)
    });
    // The binding still answers by its ordinary native spelling…
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "binding",
            "--native-symbol",
            "apps/api/src/planner/focus-task.ts#focusTask",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let text = stdout_text(&output);
    assert!(
        !text.contains(".env"),
        "the sensitive candidate path never returns"
    );
    // …but the card discloses the withholding and never carries the
    // rejected path, in the response or the persisted generation.
    let envelope = json(&output);
    assert_eq!(envelope["items"][0]["card"]["completeness"], "withheld");
    let store = project.join(".lekalo/cache/search");
    for entry in std::fs::read_dir(&store).expect("store") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") || name == "current.json" {
            continue;
        }
        let bytes = std::fs::read_to_string(entry.path()).expect("generation");
        assert!(
            !bytes.contains(".env"),
            "the sensitive candidate path never persists"
        );
    }
}

/// A cursor page never serves stale Model bytes: the token binds the
/// generation, and the serving generation must still equal what the
/// CURRENT Model admission would publish (issue #93 r4).
#[test]
fn stale_model_cursor_page_refuses() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let first = json(&lekalo_in(
        &project,
        &[
            "--json",
            "search",
            "task",
            "--limit",
            "3",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    let token = first["page"]["next"].as_str().expect("cursor").to_owned();
    // Model drift: one description change.
    let commands = project.join("lekalo/modules/planner/commands.yaml");
    let body = std::fs::read_to_string(&commands).expect("read commands");
    std::fs::write(
        &commands,
        body.replace(
            "Focus one user task planning record",
            "Focus one user task now",
        ),
    )
    .expect("drift model");
    let continued = lekalo_in(
        &project,
        &[
            "--json",
            "search",
            "task",
            "--limit",
            "3",
            "--cursor",
            &token,
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&continued), 1);
    assert_eq!(
        json(&continued)["diagnostics"][0]["id"],
        "search.cursor-invalid"
    );
    assert_eq!(
        json(&continued)["diagnostics"][0]["data"]["detail"],
        "generation-mismatch"
    );
}

/// A self-consistent forged generation (edited card, rebound digest,
/// renamed file, updated pointer, rebound cursor checksum) still
/// refuses: the serving generation must equal the recomputable current
/// admission, byte for byte (issue #93 r4).
#[test]
fn self_consistent_forged_generation_refuses() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let first = json(&lekalo_in(
        &project,
        &[
            "--json",
            "search",
            "focus",
            "--limit",
            "2",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    let token = first["page"]["next"].as_str().expect("cursor").to_owned();
    // Forge: edit one command card in the stored generation, recompute
    // its canonical digest, store it under the new name, repoint, and
    // rebind the cursor token to the forged generation.
    let store = project.join(".lekalo/cache/search");
    let pointer = std::fs::read_to_string(store.join("current.json")).expect("pointer");
    let old_hex = json_from(&pointer)["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let generation_path = store.join(&old_hex);
    let mut document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&generation_path).expect("generation"))
            .expect("generation json");
    for card in document["cards"].as_array_mut().expect("cards") {
        if card["key"] == "def:planner.focus_task" {
            card["description"] = serde_json::Value::from(".env#FORGED_STORED_CANARY");
        }
    }
    let mut record = document.clone();
    record["digest"] = serde_json::Value::from("");
    let canonical = serde_json::to_string(&record).expect("canonical");
    let forged_hex = sha256_hex_test(canonical.as_bytes());
    document["digest"] = serde_json::Value::from(format!("sha256:{forged_hex}"));
    let payload = serde_json::to_string(&document).expect("payload");
    std::fs::write(store.join(&forged_hex), payload).expect("write forged");
    std::fs::write(
        store.join("current.json"),
        format!("{{\"digest\":\"{forged_hex}\"}}"),
    )
    .expect("repoint");
    // Rebind the cursor token to the forged generation digest.
    let rebound = rebind_cursor_token(&token, &format!("sha256:{forged_hex}"));
    let continued = lekalo_in(
        &project,
        &[
            "--json",
            "search",
            "focus",
            "--limit",
            "2",
            "--cursor",
            &rebound,
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&continued), 1, "{}", stdout_text(&continued));
    let envelope = json(&continued);
    assert_eq!(envelope["diagnostics"][0]["id"], "search.cursor-invalid");
    assert!(
        !stdout_text(&continued).contains("FORGED_STORED_CANARY"),
        "forged stored content never serves"
    );
}

/// Helper: parse json text (used for the pointer document).
fn json_from(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("json")
}

/// Helper: bare lowercase hex SHA-256 for the forged-generation probe.
fn sha256_hex_test(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Helper: rebuild one cursor token against a different index digest
/// (URL-safe base64 without padding, checksum-bound like the real one).
fn rebind_cursor_token(token: &str, index_digest: &str) -> String {
    let (_check, encoded) = token.split_once(':').expect("token");
    let decoded = base64_url_decode_test(encoded);
    let mut text = String::from_utf8(decoded).expect("utf8");
    // The token body carries snake_case members; swap the index digest.
    let start =
        text.find("\"index_digest\":\"").expect("index digest") + "\"index_digest\":\"".len();
    let end = text[start..].find('"').expect("close quote") + start;
    text.replace_range(start..end, index_digest);
    let reencoded = base64_url_encode_test(text.as_bytes());
    let digest = sha256_hex_test(reencoded.as_bytes());
    format!("{}:{}", &digest[..16], reencoded)
}

fn base64_url_encode_test(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 63] as char);
        }
    }
    out
}

fn base64_url_decode_test(text: &str) -> Vec<u8> {
    fn value_of(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => u8::MAX,
        }
    }
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let mut values = [0u8; 4];
        for (index, byte) in chunk.iter().enumerate() {
            values[index] = value_of(*byte);
        }
        let triple = ((values[0] as u32) << 18)
            | ((values[1] as u32) << 12)
            | ((values[2] as u32) << 6)
            | values[3] as u32;
        out.push((triple >> 16) as u8);
        if chunk.len() > 2 {
            out.push((triple >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(triple as u8);
        }
    }
    out
}

/// Ordinary warm queries reuse the saved producer registration: after a
/// manifest build, a manifest-less query serves the admitted
/// generation's producer facts, and the registration survives until an
/// explicit build replaces it (issue #93 r4).
#[test]
fn warm_queries_reuse_the_saved_registration() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let warm = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--requirement",
            "PLANNER-REQ-001",
            "--project",
            ".",
        ],
    ));
    let keys: Vec<&str> = warm["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(
        keys.contains(&"req:openspec:PLANNER-REQ-001"),
        "the saved producer registration serves warm queries: {keys:?}"
    );
    // A drift in a pinned producer falls back to the honest cold
    // canonical-only answer instead of serving stale registration.
    let effects = project.join("evidence/effects.detected.json");
    let mut body = std::fs::read_to_string(&effects).expect("read effects");
    body.push(' ');
    std::fs::write(&effects, body).expect("drift effects");
    let cold = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--requirement",
            "PLANNER-REQ-001",
            "--project",
            ".",
        ],
    ));
    let cold_keys: Vec<&str> = cold["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["card"]["key"].as_str().expect("key"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !cold_keys.iter().any(|key| key.starts_with("req:")),
        "drifted producer registration never serves warm: {cold_keys:?}"
    );
}

/// The persisted generation is schema-valid even when a card carries
/// more facts than the bounded wire previews: the durable snapshot
/// keeps the complete searchable facts, reloads clean, and the response
/// card stays bounded with the omission disclosed (issue #93 r4).
#[test]
fn persisted_generation_keeps_nine_signatures_and_reloads_clean() {
    let (project, _guard) = disposable_project();
    // The same wide-command model extension as the ninth-input probe.
    let entities = project.join("lekalo/modules/planner/entities.yaml");
    let mut body = std::fs::read_to_string(&entities).expect("read entities");
    body.push_str(
        "  - id: planner.priority\n    kind: scalar\n    version: 1\n    description: \"Sort priority\"\n    base: string\n",
    );
    std::fs::write(&entities, body).expect("write entities");
    let commands = project.join("lekalo/modules/planner/commands.yaml");
    let mut inputs = String::new();
    for index in 0..9 {
        let id = if index == 8 {
            "planner.priority"
        } else {
            "planner.task_id"
        };
        inputs.push_str(&format!(
            "      - name: field{index}\n        type: \"{id}\"\n        required: true\n"
        ));
    }
    let command = format!(
        "  - id: planner.wide_task\n    kind: command\n    version: 1\n    description: \"Wide input command\"\n    input:\n{inputs}"
    );
    std::fs::write(&commands, {
        let original = std::fs::read_to_string(&commands).expect("read commands");
        original + &command
    })
    .expect("write commands");
    assert_eq!(exit_code(&build_once(&project)), 0);
    // The stored generation carries all nine input signatures and
    // reloads (a schema-invalid snapshot would be corrupt custody).
    let store = project.join(".lekalo/cache/search");
    let pointer = std::fs::read_to_string(store.join("current.json")).expect("pointer");
    let hex = json_from(&pointer)["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let generation: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(store.join(&hex)).expect("generation"))
            .expect("generation json");
    let wide = generation["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|card| card["key"] == "def:planner.wide_task")
        .expect("wide card");
    assert_eq!(
        wide["signatures"].as_array().expect("signatures").len(),
        9,
        "the complete typed facts persist"
    );
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(
        status["searchIndex"]["state"], "ready",
        "the stored generation reloads clean"
    );
    // The response card stays bounded at the wire previews with the
    // omission disclosed.
    let output = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.wide_task",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&output), 0, "{}", stderr_text(&output));
    let envelope = json(&output);
    let card = &envelope["items"][0]["card"];
    assert!(card["signatures"].as_array().expect("bounded").len() <= 8);
    assert!(
        card["omitted"].as_u64().expect("omitted") >= 1,
        "the wire discloses the omitted ninth fact"
    );
}

// ---------------------------------------------------------------------------
// Issue #93 r5 regressions: the warm/update lifecycle trust boundary,
// publish-side validation, Model-pin and relation-status custody,
// registration preservation, completeness honesty, requirement→binding
// joins, statement-identity conflicts, schema/runtime bound parity, the
// Windows runtime-cache name policy, and receipt honesty.
// ---------------------------------------------------------------------------

/// Write one probe producer document plus its single-input manifest
/// under the disposable project's `evidence/` tree, returning the
/// manifest's project-relative path.
fn write_probe_manifest(
    project: &Path,
    document: &serde_json::Value,
    name: &str,
    kind: &str,
    identity: &str,
) -> String {
    std::fs::write(
        project.join("evidence").join(name),
        serde_json::to_vec_pretty(document).expect("producer bytes"),
    )
    .expect("write producer");
    let producer_path = format!("evidence/{name}");
    let manifest = serde_json::json!({
        "schemaVersion": "lekalo/search-inputs/v0.8.0",
        "identity": "dev.lekalo.search-inputs@0.8.0",
        "projectId": "planner",
        "inputs": [
            {"kind": kind, "path": producer_path, "identity": identity}
        ]
    });
    std::fs::write(
        project.join("evidence/probe-manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest bytes"),
    )
    .expect("write manifest");
    "evidence/probe-manifest.json".to_owned()
}

fn read_evidence(project: &Path, name: &str) -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(project.join("evidence").join(name)).expect("read evidence"),
    )
    .expect("evidence json")
}

/// A self-consistent forged generation (edited card, rebound digest,
/// renamed file, repointed pointer — NO cursor involved) never serves
/// an ordinary warm query: the warm path now carries the same
/// derivation proof as the cursor path and the freshness oracle, and
/// the honest re-admitted answer (which also repairs the pointer) is
/// served instead (issue #93 r5).
#[test]
fn warm_forged_generation_never_serves() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    // Forge: edit one card, recompute the canonical content digest,
    // store under the new name, repoint current.json.
    let store = project.join(".lekalo/cache/search");
    let pointer = std::fs::read_to_string(store.join("current.json")).expect("pointer");
    let old_hex = json_from(&pointer)["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let mut document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(store.join(&old_hex)).expect("generation"))
            .expect("generation json");
    for card in document["cards"].as_array_mut().expect("cards") {
        if card["key"] == "def:planner.focus_task" {
            card["description"] = serde_json::Value::from("FORGED_WARM_CANARY");
        }
    }
    let mut record = document.clone();
    record["digest"] = serde_json::Value::from("");
    let canonical = serde_json::to_string(&record).expect("canonical");
    let forged_hex = sha256_hex_test(canonical.as_bytes());
    document["digest"] = serde_json::Value::from(format!("sha256:{forged_hex}"));
    std::fs::write(
        store.join(&forged_hex),
        serde_json::to_string(&document).expect("payload"),
    )
    .expect("write forged");
    std::fs::write(
        store.join("current.json"),
        format!("{{\"digest\":\"{forged_hex}\"}}"),
    )
    .expect("repoint");
    // The freshness oracle honestly calls the store stale...
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(status["searchIndex"]["state"], "stale");
    // Probe the ordinary cursor-less query against the planted forge
    // BEFORE an update can repair it.
    let query = lekalo_in(
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
    assert_eq!(exit_code(&query), 0, "{}", stderr_text(&query));
    assert!(!stdout_text(&query).contains("FORGED_WARM_CANARY"));
    // Replant the same forge to independently exercise update reuse.
    std::fs::write(
        store.join("current.json"),
        format!("{{\"digest\":\"{forged_hex}\"}}"),
    )
    .expect("repoint for update");
    // Update re-admits the registration and republishes the true
    // generation — never `reused:true` over the forge.
    let update = lekalo_in(
        &project,
        &["--json", "search-index", "update", "--project", "."],
    );
    assert_eq!(exit_code(&update), 0, "{}", stderr_text(&update));
    assert!(
        !stdout_text(&update).contains("\"reused\":true"),
        "no-op reuse over a forged generation: {}",
        stdout_text(&update)
    );
    let repaired = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(repaired["searchIndex"]["state"], "ready");
    // ...and the ordinary cursor-less warm query serves the re-admitted
    // truth over the repaired store, never the forged bytes.
    let query = lekalo_in(
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
    assert_eq!(exit_code(&query), 0, "{}", stderr_text(&query));
    assert!(
        !stdout_text(&query).contains("FORGED_WARM_CANARY"),
        "forged stored content never serves warm"
    );
}

/// Issue #93 r7 (r4 F3 re-opened) — the exact forged-memo repro: the
/// derivation memo's `derived` field is self-asserted, so the review's
/// fresh repro rewrites an honest memo's `"derived":"sha256:<H>"` →
/// `"sha256:<F>"` after planting a self-consistent forged generation,
/// and the r6 match check (content-key → filename, memo.derived ==
/// document.digest — every byte attacker-writable under
/// `.lekalo/cache/search/`) certifies the forgery: `find` serves the
/// forged bytes, `status` reports ready, and `update` reports
/// `reused:true` over the forgery. Because a memo in the same
/// attacker-writable cache can never attest a derivation, the memo is
/// gone entirely: honest runs record no sidecar, the derivation proof
/// is always decisive on serve/status paths, and a planted memo is
/// inert bytes that neither accelerate nor corrupt anything.
#[test]
fn forged_derivation_memo_never_certifies_a_forged_generation() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let store = project.join(".lekalo/cache/search");
    // An honest run records no derivation memo at all: there is no
    // sidecar to forge in the first place (issue #93 r7).
    assert!(!store.join("memo").exists(), "no memo sidecar is written");
    let query = lekalo_in(
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
    assert_eq!(exit_code(&query), 0, "{}", stderr_text(&query));
    let honest_answer = stdout_text(&query);
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(status["searchIndex"]["state"], "ready");

    // Forge: edit one card, recompute the canonical content digest,
    // store under the new name, repoint current.json — the
    // self-consistent forge class the r6 memo was trusted against.
    let pointer = std::fs::read_to_string(store.join("current.json")).expect("pointer");
    let honest_hex = json_from(&pointer)["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let mut document: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(store.join(&honest_hex)).expect("generation"),
    )
    .expect("generation json");
    for card in document["cards"].as_array_mut().expect("cards") {
        if card["key"] == "def:planner.focus_task" {
            card["description"] = serde_json::Value::from("FORGED_MEMO_CANARY");
        }
    }
    let mut record = document.clone();
    record["digest"] = serde_json::Value::from("");
    let canonical = serde_json::to_string(&record).expect("canonical");
    let forged_hex = sha256_hex_test(canonical.as_bytes());
    document["digest"] = serde_json::Value::from(format!("sha256:{forged_hex}"));
    std::fs::write(
        store.join(&forged_hex),
        serde_json::to_string(&document).expect("payload"),
    )
    .expect("write forged");
    std::fs::write(
        store.join("current.json"),
        format!("{{\"digest\":\"{forged_hex}\"}}"),
    )
    .expect("repoint");

    // The review's exact repro: rewrite the honest memo's
    // `"derived":"sha256:<H>"` → `"sha256:<F>"` and plant it under the
    // memo-key filename the r6 checker computes for the (honest) pins
    // the forged document still carries. With the r6 mechanism this
    // certification served the forged bytes end to end. The planted
    // bytes are a fully valid r6-shaped memo (every field, canonical
    // JSON, key-hash filename), so this pins the whole mechanism, not
    // a malformed degeneration of it.
    let memo_dir = store.join("memo");
    std::fs::create_dir_all(&memo_dir).expect("memo dir");
    // The r6 memo omitted the admission member entirely when absent
    // (skip_serializing_if) and refused unknown members, so mirror the
    // honest shape exactly: admission present only when the generation
    // carries one.
    let mut memo = serde_json::Map::new();
    memo.insert(
        "schemaVersion".to_owned(),
        serde_json::Value::from("lekalo/search-derivation-memo/v0.8.0"),
    );
    memo.insert(
        "identity".to_owned(),
        serde_json::Value::from("dev.lekalo.search-derivation-memo@0.8.0"),
    );
    memo.insert(
        "recipe".to_owned(),
        serde_json::Value::from("semantic-search/1"),
    );
    memo.insert(
        "modelDigest".to_owned(),
        serde_json::Value::from(document["modelDigest"].as_str().expect("model digest")),
    );
    if let Some(admission) = document.get("admission") {
        memo.insert("admission".to_owned(), admission.clone());
    }
    memo.insert("inputs".to_owned(), document["inputs"].clone());
    memo.insert(
        "derived".to_owned(),
        serde_json::Value::from(format!("sha256:{forged_hex}")),
    );
    let memo_bytes = canonical_json_bytes(&serde_json::Value::Object(memo.clone()));
    // The r6 memo filename is the SHA-256 of its own KEY bytes — the
    // record with an empty `derived` — so the planted file must carry
    // exactly that name, exactly like the honest memo it impersonates
    // (the review's repro edits the honest memo's derived claim in
    // place, under its unchanged key-hash name).
    let mut key = memo.clone();
    key.insert("derived".to_owned(), serde_json::Value::from(String::new()));
    let memo_name =
        sha256_hex_test(canonical_json_bytes(&serde_json::Value::Object(key)).as_bytes());
    std::fs::write(memo_dir.join(&memo_name), &memo_bytes).expect("plant memo");

    // `status` must call the forged generation stale — a planted memo
    // never certifies freshness — and the Drifted status path is
    // read-only (it never repairs the pointer), so this must run
    // BEFORE any warm query.
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(status["searchIndex"]["state"], "stale");

    // `find` must never serve the forged bytes over the planted memo —
    // the derivation proof is decisive, so the honest re-admitted
    // answer is served instead.
    let served = lekalo_in(
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
    assert_eq!(exit_code(&served), 0, "{}", stderr_text(&served));
    assert!(
        !stdout_text(&served).contains("FORGED_MEMO_CANARY"),
        "forged stored content never serves warm: {}",
        stdout_text(&served)
    );
    assert_eq!(
        stdout_text(&served),
        honest_answer,
        "the honest bytes, not the forged ones"
    );

    // Replant the same forge + memo to independently exercise update:
    // the pointer was repaired by the stale-verdict path's republish
    // only via update below, so repoint it first.
    std::fs::write(
        store.join("current.json"),
        format!("{{\"digest\":\"{forged_hex}\"}}"),
    )
    .expect("repoint for update");
    let update = lekalo_in(
        &project,
        &["--json", "search-index", "update", "--project", "."],
    );
    assert_eq!(exit_code(&update), 0, "{}", stderr_text(&update));
    assert!(
        !stdout_text(&update).contains("\"reused\":true"),
        "no-op reuse over a forged generation via a planted memo: {}",
        stdout_text(&update)
    );
    // The planted memo is inert: no honest path ever reads or rewrites
    // it, and none is ever recorded again.
    assert_eq!(
        std::fs::read_to_string(memo_dir.join(&memo_name)).expect("memo untouched"),
        memo_bytes,
        "the planted memo is never consumed nor repaired into authority"
    );

    // The repaired store serves honestly and a drained update is a
    // genuine no-op reuse of the honest generation — with no memo
    // machinery to accelerate it (the derivation proof re-runs).
    let repaired = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(repaired["searchIndex"]["state"], "ready");
    let drained = lekalo_in(
        &project,
        &["--json", "search-index", "update", "--project", "."],
    );
    assert_eq!(exit_code(&drained), 0, "{}", stderr_text(&drained));
    assert!(
        stdout_text(&drained).contains("\"reused\":true"),
        "honest generation still reuses incrementally: {}",
        stdout_text(&drained)
    );
    // No honest run ever records a memo again — the planted bytes stay
    // garbage and no new authority ever appears beside the generation.
    let memo_entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&memo_dir)
        .expect("memo dir")
        .collect::<Result<_, _>>()
        .expect("entries");
    assert_eq!(
        memo_entries.len(),
        1,
        "only the planted (never-consumed) memo remains"
    );
    assert_eq!(
        memo_entries[0].file_name().to_string_lossy(),
        memo_name,
        "the planted memo is never rewritten, re-keyed, nor consumed"
    );
    let _ = std::fs::remove_dir_all(&memo_dir); // planted bytes are garbage, never state
}

/// `search-index update --inputs <manifest>` honors the requested
/// admission: a replacement manifest republishes under the new
/// admission (never `reused:true` of the old generation), and a missing
/// manifest path is a typed refusal, never a success receipt
/// (issue #93 r5).
#[test]
fn update_honors_an_explicit_replacement_manifest() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let first = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    let original = first["searchIndex"]["generation"]
        .as_str()
        .expect("digest")
        .to_owned();
    // A one-producer effects-only manifest is a real replacement
    // admission: build honors it, so update must too.
    let manifest = serde_json::json!({
        "schemaVersion": "lekalo/search-inputs/v0.8.0",
        "identity": "dev.lekalo.search-inputs@0.8.0",
        "projectId": "planner",
        "inputs": [
            {"kind": "effects", "path": "evidence/effects.detected.json",
             "identity": "dev.lekalo.effects@0.2.16"}
        ]
    });
    std::fs::write(
        project.join("evidence/effects-only.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest bytes"),
    )
    .expect("write manifest");
    let update = lekalo_in(
        &project,
        &[
            "--json",
            "search-index",
            "update",
            "--project",
            ".",
            "--inputs",
            "evidence/effects-only.json",
        ],
    );
    assert_eq!(exit_code(&update), 0, "{}", stderr_text(&update));
    let receipt = json(&update);
    assert_ne!(receipt["reused"], serde_json::Value::from(true));
    assert_ne!(
        receipt["digest"].as_str().expect("digest"),
        original,
        "the explicit manifest controls the published generation"
    );
    // The replacement registration serves: the smaller admission has no
    // binding producers.
    let bindings = json(&lekalo_in(
        &project,
        &["--json", "find", "--kind", "binding", "--project", "."],
    ));
    assert_eq!(
        bindings["items"].as_array().expect("items").len(),
        0,
        "the explicit replacement admission shrank the served set"
    );
    // A nonexistent manifest is a typed refusal, never a silent reuse.
    let missing = lekalo_in(
        &project,
        &[
            "--json",
            "search-index",
            "update",
            "--project",
            ".",
            "--inputs",
            "evidence/does-not-exist.json",
        ],
    );
    assert_eq!(exit_code(&missing), 1, "{}", stdout_text(&missing));
    assert_eq!(
        json(&missing)["diagnostics"][0]["id"],
        "search.input-invalid"
    );
}

/// A published generation is self-validating: an observed record whose
/// symbol the Model no longer defines (a ghost `def:` target) never
/// publishes a generation the store's own loader brands corrupt — the
/// qualified identity attaches only to existing cards and publication
/// validates before it writes (issue #93 r5).
#[test]
fn ghost_observed_symbol_publishes_a_self_validating_generation() {
    let (project, _guard) = disposable_project();
    let observed_path = project.join("evidence/observed.index.json");
    let original = std::fs::read_to_string(&observed_path).expect("read index");
    // Typed round-trip: the observed wire only admits its exact
    // canonical rendering, so the probe mutates the typed index and
    // re-renders through the owners' own canonical serializer.
    let mut index: lekalo_core::observed::ObservedIndex =
        serde_json::from_str(&original).expect("typed parse");
    let mut ghost = index.symbols[0].clone();
    ghost.id = "planner.ghost_probe".to_owned();
    ghost.stable_key = Some("apps/api/src/planner/ghost.ts#ghostProbe".to_owned());
    ghost.location = Some(lekalo_core::observed::types::SourceLocation {
        path: "apps/api/src/planner/ghost.ts".to_owned(),
        line: Some(3),
    });
    ghost.candidates = Vec::new();
    let position = index
        .symbols
        .iter()
        .position(|record| record.id.as_str() > "planner.ghost_probe")
        .unwrap_or(index.symbols.len());
    index.symbols.insert(position, ghost);
    std::fs::write(
        &observed_path,
        lekalo_core::observed::canonical_bytes(&index).expect("canonical"),
    )
    .expect("write observed");
    let build = lekalo_in(
        &project,
        &[
            "--json",
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&build), 0, "{}", stderr_text(&build));
    // Immediately after: the durable store must serve its own bytes.
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(
        status["searchIndex"]["state"], "ready",
        "the published generation is self-validating"
    );
    // The ghost's binding card is searchable; its qualified identity
    // never dangles off a nonexistent definition card.
    let query = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.ghost_probe",
            "--project",
            ".",
        ],
    ));
    let keys: Vec<&str> = query["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(keys.contains(&"bind:planner.ghost_probe"), "{keys:?}");
}

/// A removed Model definition with a surviving observed record does
/// not leave a dangling qualified key in the updated generation.
#[test]
fn removed_definition_with_surviving_observed_record_reloads_cleanly() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let queries_path = project.join("lekalo/modules/planner/queries.yaml");
    let queries = std::fs::read_to_string(&queries_path).expect("queries");
    let removed = queries
        .split("  - id: planner.get_status")
        .next()
        .expect("prefix");
    assert_ne!(queries, removed);
    std::fs::write(&queries_path, removed).expect("remove definition");
    let update = lekalo_in(
        &project,
        &["--json", "search-index", "update", "--project", "."],
    );
    assert_eq!(exit_code(&update), 0, "{}", stdout_text(&update));
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(status["searchIndex"]["state"], "ready");
    let query = json(&lekalo_in(
        &project,
        &["--json", "find", "--kind", "binding", "--project", "."],
    ));
    assert!(query["items"]
        .as_array()
        .expect("bindings")
        .iter()
        .any(|item| item["card"]["key"] == "bind:planner.get_status"));
}

/// A declared producer Model pin that does not bind the active Model is
/// an explicit unverified binding gap: the trace domain serves
/// honestly partial and its implements witnesses are capped at
/// `extracted` — never a verified link built on a foreign revision
/// (issue #93 r5).
#[test]
fn foreign_model_pins_disclose_unverified_binding_gaps() {
    let (project, _guard) = disposable_project();
    let mut trace = read_evidence(&project, "trace.openspec.json");
    trace["modelRef"]["digest"] = serde_json::Value::from(
        "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    );
    let manifest = write_probe_manifest(
        &project,
        &trace,
        "trace.foreign-pin.json",
        "trace",
        "dev.lekalo.trace-manifest@0.2.16",
    );
    let query = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.focus_task",
            "--project",
            ".",
            "--inputs",
            &manifest,
        ],
    );
    assert_eq!(exit_code(&query), 0, "{}", stderr_text(&query));
    let envelope = json(&query);
    let card = &envelope["items"][0]["card"];
    let implements = card["relations"]
        .as_array()
        .expect("relations")
        .iter()
        .find(|witness| witness["relation"] == "implements")
        .expect("the foreign-pin trace still joins");
    assert_ne!(
        implements["confidence"], "verified",
        "a foreign Model pin never yields a verified link"
    );
    assert_eq!(implements["confidence"], "extracted");
    let rows = envelope["coverage"].as_array().expect("coverage");
    for domain in ["trace", "requirements"] {
        let row = rows
            .iter()
            .find(|row| row["domain"] == domain)
            .expect("row");
        assert_eq!(row["state"], "partial", "the binding gap is disclosed");
    }
}

/// Independently pin-bearing requirement attachments never silently
/// verify relationships against a foreign Model either.
#[test]
fn foreign_requirement_model_pin_caps_relationship_confidence() {
    let (project, _guard) = disposable_project();
    let mut attachment = read_evidence(&project, "requirements.attachment.json");
    attachment["modelRef"]["digest"] = serde_json::json!(
        "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    );
    let manifest = write_probe_manifest(
        &project,
        &attachment,
        "requirements.foreign-pin.json",
        "requirements",
        "dev.lekalo.requirements@0.2.16",
    );
    let query = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.focus_task",
            "--project",
            ".",
            "--inputs",
            &manifest,
        ],
    );
    assert_eq!(exit_code(&query), 0, "{}", stdout_text(&query));
    let envelope = json(&query);
    let row = envelope["coverage"]
        .as_array()
        .expect("coverage")
        .iter()
        .find(|row| row["domain"] == "requirements")
        .expect("requirements row");
    assert_eq!(row["state"], "partial");
    let relations = envelope["items"][0]["card"]["relations"]
        .as_array()
        .expect("relations");
    assert!(!relations.is_empty());
    assert!(relations
        .iter()
        .all(|relation| relation["confidence"] != "verified"));
}

/// Trace relations recorded `stale` never bridge default joins: the
/// joined requirement/gate cards and the bridged definition cards are
/// excluded by default and reconstructed under `--include-stale`
/// (issue #93 r5).
#[test]
fn stale_trace_relations_do_not_bridge_default_joins() {
    let (project, _guard) = disposable_project();
    let mut trace = read_evidence(&project, "trace.openspec.json");
    trace["completeness"] = serde_json::Value::from("partial");
    trace["gaps"] = serde_json::json!([
        {"gapKind": "missing-gate", "status": "candidate",
         "anchorNode": "requirement:PLANNER-REQ-001",
         "expected": "openspec.native.gate.focus"}
    ]);
    for relation in trace["relations"].as_array_mut().expect("relations") {
        relation["status"] = serde_json::Value::from("stale");
    }
    let manifest = write_probe_manifest(
        &project,
        &trace,
        "trace.all-stale.json",
        "trace",
        "dev.lekalo.trace-manifest@0.2.16",
    );
    let run = |extra: &[&str]| {
        let mut args: Vec<&str> = vec![
            "--json",
            "find",
            "--requirement",
            "PLANNER-REQ-001",
            "--requirement-source",
            "openspec",
        ];
        args.extend_from_slice(extra);
        args.push("--project");
        args.push(".");
        args.push("--inputs");
        args.push(&manifest);
        lekalo_in(&project, &args)
    };
    let default = run(&[]);
    assert_eq!(exit_code(&default), 0, "{}", stderr_text(&default));
    let default_keys: Vec<String> = json(&default)["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["card"]["key"].as_str().expect("key").to_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        default_keys.is_empty(),
        "stale relations never bridge the default join: {default_keys:?}"
    );
    let included = run(&["--include-stale"]);
    assert_eq!(exit_code(&included), 0, "{}", stderr_text(&included));
    let included_envelope = json(&included);
    let stale_keys: Vec<&str> = included_envelope["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(
        stale_keys.contains(&"req:openspec:PLANNER-REQ-001")
            && stale_keys.contains(&"def:planner.focus_task")
            && stale_keys.contains(&"gate:hlv.gate.focus"),
        "--include-stale reconstructs the stale chain: {stale_keys:?}"
    );
}

/// Page two of a registered query continues without re-passing
/// `--inputs`: the cursor path re-admits the serving generation's own
/// recorded admission, so the identical request paginates the same
/// ranked set (issue #93 r5).
#[test]
fn registered_cursor_continues_without_repassing_inputs() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let first = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--limit",
            "1",
            "--project",
            ".",
        ],
    ));
    assert_eq!(first["status"], "valid");
    let cursor = first["page"]["next"]
        .as_str()
        .expect("a registered query issues a cursor")
        .to_owned();
    let continued = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--limit",
            "1",
            "--cursor",
            &cursor,
            "--project",
            ".",
        ],
    );
    assert_eq!(
        exit_code(&continued),
        0,
        "the identical continuation serves without extra flags: {}",
        stdout_text(&continued)
    );
    let second = json(&continued);
    assert_eq!(second["status"], "valid");
    let keys: Vec<&str> = second["items"]
        .as_array()
        .expect("page two items")
        .iter()
        .map(|item| item["card"]["key"].as_str().expect("key"))
        .collect();
    assert!(!keys.is_empty(), "page two carries the next ranked row");
    let first_key = first["items"][0]["card"]["key"].as_str().expect("key");
    assert!(
        !keys.contains(&first_key),
        "concatenated pages never repeat a row"
    );
}

/// Drift update preserves the saved registration: after a Model edit a
/// plain `search-index update` rebuilds the registration under the new
/// Model partition (republish, never a canonical-only clobber), and
/// the drifted ordinary query re-admits the registration instead of
/// silently discarding the producers (issue #93 r5).
#[test]
fn drift_update_preserves_the_registration() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let before = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    let original = before["searchIndex"]["generation"]
        .as_str()
        .expect("digest")
        .to_owned();
    // Model drift: one description change.
    let commands = project.join("lekalo/modules/planner/commands.yaml");
    let body = std::fs::read_to_string(&commands).expect("read commands");
    std::fs::write(
        &commands,
        body.replace(
            "Focus one user task planning record",
            "Focus one user task now",
        ),
    )
    .expect("drift model");
    // A plain update republishes the registration under the new Model
    // partition FIRST (before any query repairs the store): never a
    // canonical-only clobber that destroys the registration.
    let update = lekalo_in(
        &project,
        &["--json", "search-index", "update", "--project", "."],
    );
    assert_eq!(exit_code(&update), 0, "{}", stderr_text(&update));
    let receipt = json(&update);
    assert_ne!(receipt["reused"], serde_json::Value::from(true));
    assert_ne!(
        receipt["digest"].as_str().expect("digest"),
        original,
        "drift republishes under the new Model partition"
    );
    // The registration survived: bindings and requirement joins serve.
    let bindings = json(&lekalo_in(
        &project,
        &["--json", "find", "--kind", "binding", "--project", "."],
    ));
    assert!(
        !bindings["items"].as_array().expect("items").is_empty(),
        "the producer registration survives the drift update"
    );
    let drifted = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--requirement",
            "PLANNER-REQ-001",
            "--requirement-source",
            "openspec",
            "--project",
            ".",
        ],
    ));
    let drifted_keys: Vec<&str> = drifted["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["card"]["key"].as_str().expect("key"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        drifted_keys.contains(&"req:openspec:PLANNER-REQ-001"),
        "the drifted query re-admits the registration, never discards it: {drifted_keys:?}"
    );
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(status["searchIndex"]["state"], "ready");
}

/// An absent effects producer is `unknown`, never `complete`: a
/// canonical-only field-writer query answers a disclosed empty set
/// with the coverage warning, and the admitted producer raises the
/// domain (issue #93 r5).
#[test]
fn absent_effects_producer_reports_unknown_coverage() {
    let (project, _guard) = disposable_project();
    let cold = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--writes",
            "planner.user_task_planning.focused_at",
            "--project",
            ".",
        ],
    ));
    assert_eq!(cold["status"], "valid");
    assert_eq!(cold["selection"], "none");
    let rows = cold["coverage"].as_array().expect("coverage");
    let effects = rows
        .iter()
        .find(|row| row["domain"] == "effects")
        .expect("effects row");
    assert_eq!(
        effects["state"], "unknown",
        "an absent producer never claims completeness"
    );
    let codes = cold["reasonCodes"].as_array().expect("codes");
    assert!(
        codes.contains(&serde_json::Value::from("search.coverage-incomplete")),
        "the confident empty answer is disclosed: {codes:?}"
    );
    // Control: the admitted producer answers the two field writers.
    let warm = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--writes",
            "planner.user_task_planning.focused_at",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    ));
    let keys: Vec<&str> = warm["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["card"]["key"].as_str().expect("key"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        keys.contains(&"def:planner.focus_task") && keys.contains(&"def:planner.refocus_task"),
        "{keys:?}"
    );
}

/// Requirement joins reach the native binding cards: the definition's
/// joined requirement refs flow onto `bind:` cards, so "which bindings
/// implement this requirement" answers through typed identities
/// (issue #93 r5).
#[test]
fn requirement_joins_reach_native_binding_cards() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let query = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--requirement",
            "PLANNER-REQ-001",
            "--requirement-source",
            "openspec",
            "--kind",
            "binding",
            "--project",
            ".",
        ],
    ));
    assert_eq!(query["status"], "valid");
    let keys: Vec<&str> = query["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["card"]["key"].as_str().expect("key"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        keys.contains(&"bind:planner.focus_task"),
        "the requirement reaches its native binding: {keys:?}"
    );
}

/// Conflicting diagnostic statements on one finding id are a typed
/// identity conflict, never last-write-wins: same id and rule/subject
/// but a different severity or disposition refuses; an exact
/// restatement stays a legal duplicate (issue #93 r5).
#[test]
fn conflicting_diagnostic_statements_refuse() {
    let (project, _guard) = disposable_project();
    let base = read_evidence(&project, "diagnostics.json");
    // Same id, same rule and subject, conflicting lifecycle statements.
    let mut conflicting = base.clone();
    conflicting["findings"]
        .as_array_mut()
        .expect("findings")
        .push(serde_json::json!({
            "id": "focus-concurrency-001",
            "rule": "planner.focus-concurrency",
            "subject": "planner.focus_task",
            "severity": "info",
            "disposition": "resolved"
        }));
    conflicting["metadata"]["findingCount"] =
        serde_json::Value::from(conflicting["findings"].as_array().expect("findings").len());
    let manifest = write_probe_manifest(
        &project,
        &conflicting,
        "diagnostics.conflict.json",
        "diagnostics",
        "dev.lekalo.search-diagnostics@0.8.0",
    );
    let refused = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "diagnostic",
            "--project",
            ".",
            "--inputs",
            &manifest,
        ],
    );
    assert_eq!(exit_code(&refused), 1, "{}", stdout_text(&refused));
    assert_eq!(
        json(&refused)["diagnostics"][0]["data"]["detail"],
        "identity-conflict"
    );
    // An exact restatement is a legal duplicate: the active finding
    // still serves beside its re-stated copy.
    let mut restated = base.clone();
    let original = restated["findings"][0].clone();
    restated["findings"]
        .as_array_mut()
        .expect("findings")
        .push(original);
    restated["metadata"]["findingCount"] =
        serde_json::Value::from(restated["findings"].as_array().expect("findings").len());
    let manifest = write_probe_manifest(
        &project,
        &restated,
        "diagnostics.restated.json",
        "diagnostics",
        "dev.lekalo.search-diagnostics@0.8.0",
    );
    let admitted = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--diagnostic",
            "planner.focus-concurrency",
            "--project",
            ".",
            "--inputs",
            &manifest,
        ],
    );
    assert_eq!(exit_code(&admitted), 0, "{}", stdout_text(&admitted));
    let envelope = json(&admitted);
    let keys: Vec<&str> = envelope["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item["card"]["key"].as_str().expect("key"))
                .collect()
        })
        .unwrap_or_default();
    assert!(keys.contains(&"diag:focus-concurrency-001"), "{keys:?}");
}

/// One finding id has one statement across ALL admitted files (r11),
/// independent of filename order. Identical copies deduplicate; changes
/// to any identity member refuse before a diagnostic card is projected.
#[test]
fn cross_document_diagnostic_identity_is_global() {
    let (project, _guard) = disposable_project();
    let mut base = read_evidence(&project, "diagnostics.json");
    base["findings"] = serde_json::json!([base["findings"][0].clone()]);
    base["metadata"]["findingCount"] = serde_json::json!(1);
    let manifest = serde_json::json!({
        "schemaVersion": "lekalo/search-inputs/v0.8.0",
        "identity": "dev.lekalo.search-inputs@0.8.0",
        "projectId": "planner",
        "inputs": [
            {"kind": "diagnostics", "path": "evidence/a.json", "identity": "dev.lekalo.search-diagnostics@0.8.0"},
            {"kind": "diagnostics", "path": "evidence/z.json", "identity": "dev.lekalo.search-diagnostics@0.8.0"}
        ]
    });
    std::fs::write(
        project.join("evidence/two-files.json"),
        serde_json::to_vec(&manifest).expect("manifest"),
    )
    .expect("write manifest");
    let write_pair = |left: &serde_json::Value, right: &serde_json::Value| {
        for (name, document) in [("a.json", left), ("z.json", right)] {
            std::fs::write(
                project.join("evidence").join(name),
                serde_json::to_vec(document).expect("producer bytes"),
            )
            .expect("write producer");
        }
    };
    let args = [
        "--json",
        "--no-cache",
        "find",
        "--kind",
        "diagnostic",
        "--project",
        ".",
        "--inputs",
        "evidence/two-files.json",
    ];
    write_pair(&base, &base);
    let admitted = lekalo_in(&project, &args);
    assert_eq!(exit_code(&admitted), 0, "{}", stderr_text(&admitted));
    let document = json(&admitted);
    assert_eq!(document["items"].as_array().expect("items").len(), 1);
    assert_eq!(
        document["items"][0]["card"]["key"],
        "diag:focus-concurrency-001"
    );
    assert_eq!(document["items"][0]["card"]["subkind"], "warning");
    assert!(document["coverage"]
        .as_array()
        .expect("coverage")
        .iter()
        .any(|row| row["domain"] == "diagnostics" && row["state"] == "complete"));
    for (member, value) in [
        ("severity", "info"),
        ("disposition", "resolved"),
        ("rule", "planner.different-rule"),
        ("subject", "planner.refocus_task"),
    ] {
        let mut conflicting = base.clone();
        conflicting["findings"][0][member] = serde_json::json!(value);
        for swapped in [false, true] {
            if swapped {
                write_pair(&conflicting, &base);
            } else {
                write_pair(&base, &conflicting);
            }
            let refused = lekalo_in(&project, &args);
            assert_eq!(
                exit_code(&refused),
                1,
                "{member}, swapped={swapped}: {refused:?}"
            );
            let envelope = json(&refused);
            assert_eq!(envelope["diagnostics"][0]["id"], "search.input-invalid");
            assert_eq!(
                envelope["diagnostics"][0]["data"]["detail"],
                "identity-conflict"
            );
        }
    }
}

/// Hostile nesting stays well below the byte cap but must be a typed
/// refusal, not a process stack overflow, at manifest AND producer reads.
#[test]
fn deeply_nested_search_inputs_refuse_without_stack_overflow() {
    let (project, _guard) = disposable_project();
    let shallow = r#"{"schemaVersion":"lekalo/search-inputs/v0.8.0","identity":"dev.lekalo.search-inputs@0.8.0","projectId":"planner","inputs":[]}"#;
    let manifest_path = project.join("evidence/depth.json");
    std::fs::write(&manifest_path, shallow).expect("shallow manifest");
    let args = [
        "--json",
        "--no-cache",
        "find",
        "--id",
        "planner.focus_task",
        "--project",
        ".",
        "--inputs",
        "evidence/depth.json",
    ];
    let accepted = lekalo_in(&project, &args);
    assert_eq!(exit_code(&accepted), 0, "{accepted:?}");
    let nested = format!("{}0{}", "[".repeat(400_000), "]".repeat(400_000));
    // A closed-manifest prefix followed by nested arrays reproduces
    // the lexer stack overflow independently of the typed decoder.
    let hostile = shallow.replace("[]", &nested);
    std::fs::write(&manifest_path, hostile).expect("hostile manifest");
    let refused = lekalo_in(&project, &args);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert_eq!(
        json(&refused)["diagnostics"][0]["id"],
        "search.input-invalid"
    );
    // The producer boundary shares the lexer, and must refuse too.
    std::fs::write(
        &manifest_path,
        shallow.replace("[]", r#"[{"kind":"diagnostics","path":"evidence/deep-producer.json","identity":"dev.lekalo.search-diagnostics@0.8.0"}]"#),
    )
    .expect("producer manifest");
    std::fs::write(project.join("evidence/deep-producer.json"), nested).expect("deep producer");
    let refused = lekalo_in(&project, &args);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert_eq!(
        json(&refused)["diagnostics"][0]["id"],
        "search.input-invalid"
    );
}

#[cfg(any(unix, windows))]
fn file_symlink(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("file symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).expect("file symlink");
}

/// Status cannot disclose the contents of outside pointer/generation
/// links before the later Model/structure pass (issue #93 r11).
#[test]
#[cfg(any(unix, windows))]
fn status_refuses_pointer_and_generation_file_links() {
    let (project, guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let home = project.join(".lekalo/cache/search");
    let pointer = home.join("current.json");
    let honest_pointer = std::fs::read(&pointer).expect("pointer");
    let digest = serde_json::from_slice::<serde_json::Value>(&honest_pointer)
        .expect("pointer json")["digest"]
        .as_str()
        .expect("digest")
        .to_owned();
    let outside_pointer = guard.path().join("outside-pointer.json");
    std::fs::write(&outside_pointer, &honest_pointer).expect("outside pointer");
    std::fs::remove_file(&pointer).expect("remove pointer");
    file_symlink(&outside_pointer, &pointer);
    let assert_denied = || {
        let status = lekalo_in(
            &project,
            &["--json", "search-index", "status", "--project", "."],
        );
        assert_eq!(status.status.code(), Some(0), "{status:?}");
        let document = json(&status);
        assert_eq!(document["searchIndex"]["state"], "denied");
        assert!(document["searchIndex"]["generation"].is_null());
        assert!(document["searchIndex"]["cards"].is_null());
        let find = lekalo_in(
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
        assert_eq!(find.status.code(), Some(3), "{find:?}");
        assert!(json(&find)["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|row| row["id"] == "structure.path-link"));
    };
    for external_digest in ["c".repeat(64), "f".repeat(64)] {
        std::fs::write(
            &outside_pointer,
            format!("{{\"digest\":\"{external_digest}\"}}"),
        )
        .expect("outside edit");
        assert_denied();
    }
    std::fs::remove_file(&pointer).expect("unlink pointer");
    std::fs::write(&pointer, honest_pointer).expect("restore pointer");
    let generation = home.join(&digest);
    let honest_generation = std::fs::read(&generation).expect("generation");
    let outside_generation = guard.path().join("outside-generation");
    std::fs::write(&outside_generation, &honest_generation).expect("outside generation");
    std::fs::remove_file(&generation).expect("remove generation");
    file_symlink(&outside_generation, &generation);
    assert_denied();
    assert_eq!(
        std::fs::read(outside_generation).expect("unchanged outside"),
        honest_generation
    );
    std::fs::remove_file(&generation).expect("unlink generation");
    std::fs::write(generation, honest_generation).expect("restore generation");
    let restored = lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    );
    assert_eq!(json(&restored)["searchIndex"]["state"], "ready");
}

/// The published producer row bounds are enforced at admission: a
/// diagnostics snapshot with 4097 findings and an ownership profile
/// with 4097 members are the same refusal the published schemas give
/// (issue #93 r5).
#[test]
fn producer_row_bounds_match_the_published_schemas() {
    let (project, _guard) = disposable_project();
    let base = read_evidence(&project, "diagnostics.json");
    let mut oversize = base.clone();
    {
        let findings = oversize["findings"].as_array_mut().expect("findings");
        let template = findings[0].clone();
        findings.clear();
        for index in 0..4097 {
            let mut row = template.clone();
            row["id"] = serde_json::Value::from(format!("bound-probe-{index:04}"));
            findings.push(row);
        }
        oversize["metadata"]["findingCount"] = serde_json::Value::from(findings.len());
    }
    let manifest = write_probe_manifest(
        &project,
        &oversize,
        "diagnostics.oversize.json",
        "diagnostics",
        "dev.lekalo.search-diagnostics@0.8.0",
    );
    let refused = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "diagnostic",
            "--project",
            ".",
            "--inputs",
            &manifest,
        ],
    );
    assert_eq!(exit_code(&refused), 1, "{}", stdout_text(&refused));
    assert_eq!(
        json(&refused)["diagnostics"][0]["id"],
        "search.input-invalid"
    );
    // The exact schema maximum remains admissible (even though the
    // default result page is bounded independently).
    oversize["findings"].as_array_mut().expect("findings").pop();
    oversize["metadata"]["findingCount"] = serde_json::json!(4096);
    let maximum_manifest = write_probe_manifest(
        &project,
        &oversize,
        "diagnostics.maximum.json",
        "diagnostics",
        "dev.lekalo.search-diagnostics@0.8.0",
    );
    let admitted = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "diagnostic",
            "--project",
            ".",
            "--inputs",
            &maximum_manifest,
        ],
    );
    assert_eq!(exit_code(&admitted), 0, "{}", stdout_text(&admitted));
    // The ownership-profile member bound.
    let mut ownership = read_evidence(&project, "ownership-profile.json");
    {
        let members = ownership["profiles"][0]["members"]
            .as_array_mut()
            .expect("members");
        let member = members[0].clone();
        members.clear();
        for _ in 0..4097 {
            members.push(member.clone());
        }
    }
    let manifest = write_probe_manifest(
        &project,
        &ownership,
        "ownership.oversize.json",
        "ownership-profile",
        "dev.lekalo.search-ownership-profile@0.8.0",
    );
    let refused = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--project",
            ".",
            "--inputs",
            &manifest,
        ],
    );
    assert_eq!(exit_code(&refused), 1, "{}", stdout_text(&refused));
    assert_eq!(
        json(&refused)["diagnostics"][0]["id"],
        "search.input-invalid"
    );
    ownership["profiles"][0]["members"]
        .as_array_mut()
        .expect("members")
        .pop();
    let maximum_manifest = write_probe_manifest(
        &project,
        &ownership,
        "ownership.maximum.json",
        "ownership-profile",
        "dev.lekalo.search-ownership-profile@0.8.0",
    );
    let admitted = lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--kind",
            "command",
            "--project",
            ".",
            "--inputs",
            &maximum_manifest,
        ],
    );
    assert_eq!(exit_code(&admitted), 0, "{}", stdout_text(&admitted));
}

/// Runtime cache artifacts never brick search: the Windows WAL-teardown
/// spelling under `.lekalo/cache/` is a runtime artifact name, so the
/// governed-tree scan keeps serving while it exists (issue #93 r5).
#[test]
fn runtime_cache_artifacts_never_brick_search() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    std::fs::write(
        project.join(".lekalo/cache/CACHE.SQLITE-SHM.tmp"),
        b"transient wal sidecar",
    )
    .expect("plant sidecar");
    let query = lekalo_in(
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
    assert_eq!(
        exit_code(&query),
        0,
        "a runtime cache artifact never refuses the query: {}",
        stdout_text(&query)
    );
    let status = json(&lekalo_in(
        &project,
        &["--json", "search-index", "status", "--project", "."],
    ));
    assert_eq!(status["searchIndex"]["state"], "ready");
}

/// The definition card mirrors the binding card's withholding
/// disclosure: when candidates of one observed event were withheld, the
/// merged definition card serializes `withheld`, never a complete
/// bindings list over silently truncated evidence (issue #93 r5).
#[test]
fn definition_card_discloses_withheld_bindings() {
    let (project, _guard) = disposable_project();
    let observed_path = project.join("evidence/observed.index.json");
    let original = std::fs::read_to_string(&observed_path).expect("read index");
    // Typed round-trip (the observed wire admits only its exact
    // canonical rendering): the get_status record gains five hostile
    // candidate spellings beside its admitted location.
    let mut index: lekalo_core::observed::ObservedIndex =
        serde_json::from_str(&original).expect("typed parse");
    let record = index
        .symbols
        .iter_mut()
        .find(|record| record.id == "planner.get_status")
        .expect("get_status record");
    let fingerprint = record.fingerprint.clone();
    for spelling in [
        "../outside/secrets/keys.ts#leak",
        ".git/hooks/evil.ts#hook",
        "C:/Windows/system32/evil.ts#x",
        "apps/api/src/planner/Upper.ts#ok",
        "apps/api/src/planner/spaces name.ts#s",
    ] {
        let (path, _symbol) = spelling.rsplit_once('#').expect("path#symbol");
        record
            .candidates
            .push(lekalo_core::observed::types::CandidateRecord {
                native: spelling.to_owned(),
                path: path.to_owned(),
                line: Some(4),
                fingerprint: fingerprint.clone(),
                confidence: lekalo_core::observed::types::Confidence::High,
            });
    }
    std::fs::write(
        &observed_path,
        lekalo_core::observed::canonical_bytes(&index).expect("canonical"),
    )
    .expect("write observed");
    assert_eq!(exit_code(&build_once(&project)), 0);
    let query = json(&lekalo_in(
        &project,
        &[
            "--json",
            "find",
            "--id",
            "planner.get_status",
            "--project",
            ".",
        ],
    ));
    let cards: Vec<&serde_json::Value> = query["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| &item["card"])
        .collect();
    let binding = cards
        .iter()
        .find(|card| card["key"] == "bind:planner.get_status")
        .expect("binding card");
    let definition = cards
        .iter()
        .find(|card| card["key"] == "def:planner.get_status")
        .expect("definition card");
    assert_eq!(binding["completeness"], "withheld");
    assert_eq!(
        definition["completeness"], "withheld",
        "the merged definition mirrors the event's withholding disclosure"
    );
}

/// A no-op `build` receipt names the build command: the operation field
/// never contradicts the invoked command (issue #93 r5).
#[test]
fn no_op_build_receipt_names_the_build_command() {
    let (project, _guard) = disposable_project();
    assert_eq!(exit_code(&build_once(&project)), 0);
    let rebuild = lekalo_in(
        &project,
        &[
            "--json",
            "search-index",
            "build",
            "--project",
            ".",
            "--inputs",
            "evidence/search-inputs.json",
        ],
    );
    assert_eq!(exit_code(&rebuild), 0, "{}", stderr_text(&rebuild));
    let receipt = json(&rebuild);
    assert_eq!(receipt["operation"], "search-index-build");
    assert_eq!(receipt["reused"], serde_json::Value::from(true));
}
