//! Issue #20 CLI tests for the cache handoff: the read-only health
//! projection, the explicit-confirmation clear, and the byte-identity of
//! cached, warm, and `--no-cache` runs on the published command surfaces.
//!
//! Every run happens in a fresh copy of the hermetic planner fixture
//! under the crate's `target/` tree, reached through the alias-free temp
//! spelling: GitHub's Windows runners export `%TEMP%` with the 8.3
//! profile alias and the selection policy denies alias spellings before
//! any command logic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/loader/valid-direct-visibility";

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
        .join("cache-cli-tests")
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

#[test]
fn cache_status_reports_missing_then_ok() {
    let dir = fixture_copy("status");
    let missing = lekalo_in(&dir, &["--json", "cache", "status", "--project", "."]);
    assert_eq!(exit_code(&missing), 0, "{}", stdout_text(&missing));
    assert!(stdout_text(&missing).starts_with("{\"status\":\"valid\",\"cache\":{"));
    assert!(stdout_text(&missing).contains("\"state\":\"missing\""));

    let load = lekalo_in(&dir, &["load", "--project", "."]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));

    let ok = lekalo_in(&dir, &["--json", "cache", "status", "--project", "."]);
    assert_eq!(exit_code(&ok), 0);
    let json = stdout_text(&ok);
    assert!(json.contains("\"state\":\"ok\""), "{json}");
    assert!(
        json.contains("\"identity\":\"dev.lekalo.cache@1.0.0\""),
        "{json}"
    );
    assert!(json.contains("\"backend\":\"sqlite\""), "{json}");
    // No native path fragments, no host data, no volatile telemetry.
    assert!(!json.contains('\\'), "no native paths in the wire: {json}");

    let human = lekalo_in(&dir, &["cache", "status", "--project", "."]);
    assert_eq!(exit_code(&human), 0);
    assert!(
        stdout_text(&human).starts_with("cache ok : "),
        "{}",
        stdout_text(&human)
    );
}

#[test]
fn cache_clear_requires_the_explicit_yes_flag() {
    let dir = fixture_copy("clear-flag");
    let load = lekalo_in(&dir, &["load", "--project", "."]);
    assert_eq!(exit_code(&load), 0);

    let refused = lekalo_in(&dir, &["cache", "clear", "--project", "."]);
    assert_eq!(exit_code(&refused), 1, "{}", stdout_text(&refused));
    assert!(
        stderr_text(&refused).contains("cli.usage"),
        "{}",
        stderr_text(&refused)
    );
    assert!(dir.join(".lekalo/cache").exists(), "nothing was cleared");
}

#[test]
fn cache_clear_yes_removes_the_home_but_not_migrations() {
    let dir = fixture_copy("clear-yes");
    let load = lekalo_in(&dir, &["load", "--project", "."]);
    assert_eq!(exit_code(&load), 0);
    // Migration custody (#9) lives under the same runtime tree and stays.
    std::fs::create_dir_all(dir.join(".lekalo/cache/migrations")).expect("migration home");
    std::fs::write(dir.join(".lekalo/cache/migrations/journal"), b"x").expect("journal");

    let cleared = lekalo_in(&dir, &["cache", "clear", "--yes", "--project", "."]);
    assert_eq!(exit_code(&cleared), 0, "{}", stdout_text(&cleared));
    assert!(
        stdout_text(&cleared).starts_with("cache cleared : "),
        "{}",
        stdout_text(&cleared)
    );
    assert!(!dir.join(".lekalo/cache/cache.sqlite").exists());
    assert!(dir.join(".lekalo/cache/migrations/journal").exists());

    let status = lekalo_in(&dir, &["--json", "cache", "status", "--project", "."]);
    assert!(stdout_text(&status).contains("\"state\":\"missing\""));
    // The canonical project tree is untouched by the clear.
    let reload = lekalo_in(&dir, &["load", "--project", "."]);
    assert_eq!(exit_code(&reload), 0, "{}", stderr_text(&reload));
}

#[test]
fn cached_warm_and_no_cache_runs_are_byte_identical() {
    let dir = fixture_copy("equivalence");
    let cached_load = lekalo_in(&dir, &["--json", "load", "--project", "."]);
    assert_eq!(exit_code(&cached_load), 0, "{}", stderr_text(&cached_load));
    let warm_load = lekalo_in(&dir, &["--json", "load", "--project", "."]);
    let no_cache_load = lekalo_in(&dir, &["--json", "--no-cache", "load", "--project", "."]);
    assert_eq!(stdout_text(&cached_load), stdout_text(&warm_load));
    assert_eq!(stdout_text(&cached_load), stdout_text(&no_cache_load));

    let cached_ir = lekalo_in(&dir, &["--json", "load", "--ir", "--project", "."]);
    let no_cache_ir = lekalo_in(
        &dir,
        &["--json", "--no-cache", "load", "--ir", "--project", "."],
    );
    assert_eq!(exit_code(&cached_ir), 0, "{}", stderr_text(&cached_ir));
    assert_eq!(stdout_text(&cached_ir), stdout_text(&no_cache_ir));
    let cached_graph = lekalo_in(&dir, &["--json", "graph", "export", "--project", "."]);
    let no_cache_graph = lekalo_in(
        &dir,
        &["--json", "--no-cache", "graph", "export", "--project", "."],
    );
    assert_eq!(
        exit_code(&cached_graph),
        0,
        "{}",
        stderr_text(&cached_graph)
    );
    assert_eq!(stdout_text(&cached_graph), stdout_text(&no_cache_graph));

    let cached_validate = lekalo_in(&dir, &["--json", "validate", "--project", "."]);
    let no_cache_validate = lekalo_in(
        &dir,
        &["--json", "--no-cache", "validate", "--project", "."],
    );
    assert_eq!(
        exit_code(&cached_validate),
        0,
        "{}",
        stderr_text(&cached_validate)
    );
    assert_eq!(
        stdout_text(&cached_validate),
        stdout_text(&no_cache_validate)
    );
}

/// The effects projection over the planner fixture: cached and
/// `--no-cache` runs are byte-identical, and the run records its fragment.
#[test]
fn effects_runs_are_byte_identical_with_and_without_cache() {
    let dir = fixture_copy("effects");
    // Swap in the effects planner fixture, which declares operations.
    std::fs::remove_dir_all(&dir).expect("reset");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join("tests/fixtures/effects/planner"),
        &dir,
    );
    let cached = lekalo_in(
        &dir,
        &[
            "--json",
            "effects",
            "show",
            "planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&cached), 0, "{}", stderr_text(&cached));
    let no_cache = lekalo_in(
        &dir,
        &[
            "--json",
            "--no-cache",
            "effects",
            "show",
            "planner.focus_task",
            "--project",
            ".",
        ],
    );
    assert_eq!(exit_code(&no_cache), 0, "{}", stderr_text(&no_cache));
    assert_eq!(stdout_text(&cached), stdout_text(&no_cache));

    let status = lekalo_in(&dir, &["--json", "cache", "status", "--project", "."]);
    assert!(stdout_text(&status).contains("\"effect-fragment\""));
}

#[test]
fn no_cache_creates_no_runtime_files() {
    let dir = fixture_copy("no-cache");
    let load = lekalo_in(&dir, &["--no-cache", "load", "--project", "."]);
    assert_eq!(exit_code(&load), 0, "{}", stderr_text(&load));
    assert!(!dir.join(".lekalo").exists(), "--no-cache writes nothing");
}

#[test]
fn a_linked_cache_home_is_a_fail_closed_denial() {
    let dir = fixture_copy("deny");
    std::fs::create_dir_all(dir.join(".lekalo")).expect("runtime dir");
    let outside = fixture_copy("deny-target");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, dir.join(".lekalo/cache")).expect("symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside, dir.join(".lekalo/cache")).expect("symlink");
    let denied = lekalo_in(&dir, &["--json", "cache", "status", "--project", "."]);
    assert_eq!(exit_code(&denied), 3, "{}", stdout_text(&denied));
    assert!(
        stdout_text(&denied).contains("structure.path-link"),
        "{}",
        stdout_text(&denied)
    );
    // The pipeline refuses the hostile home the same way.
    let load = lekalo_in(&dir, &["load", "--project", "."]);
    assert_eq!(exit_code(&load), 3, "{}", stdout_text(&load));
}
