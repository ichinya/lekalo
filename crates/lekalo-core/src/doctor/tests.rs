//! Issue #92 doctor core tests: the closed check panel over the hermetic
//! fixture, state classification (fresh vs degraded vs blocked), the
//! never-mutating contract, and the recipe preview.

use super::model::{Check, CheckState, DoctorReport, GitFacts, GitState, Phase, ReportKind};
use super::{build, report, Options};
use crate::loader::LoadSelection;

/// A fresh, numbered copy of the hermetic planner fixture under the
/// package's target/ tree, addressed by a traversal-free relative
/// selection (the accepted #4 selection policy denies traversal and
/// alias spellings).
fn fixture_selection() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id =
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + u64::from(std::process::id());
    let relative = std::path::PathBuf::from("target")
        .join("doctor-tests")
        .join(format!("case-{id}"));
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/loader/valid-direct-visibility");
    copy_dir(&source, &relative);
    relative.to_string_lossy().replace('\\', "/")
}

fn copy_dir(source: &std::path::Path, target: &std::path::Path) {
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

fn git_available() -> GitFacts {
    GitFacts {
        state: GitState::Available,
        commit: Some("a".repeat(40)),
        dirty: Some(false),
    }
}

fn doctor_options() -> Options {
    Options {
        kind: ReportKind::Doctor,
        ..Options::default()
    }
}

/// The typed document of one doctor run plus the emitted receipt JSON.
fn run(selection: &LoadSelection, git: &GitFacts, options: &Options) -> (DoctorReport, String) {
    let doc = build(selection, git, options);
    let result = report(selection, git, options);
    assert_eq!(result.exit_code(), 0, "a produced report is valid");
    let (json, human) = match &result {
        crate::result::DomainResult::Valid { payload, .. } => match payload {
            crate::result::SuccessPayload::Receipt { json, human } => (json.clone(), human.clone()),
            other => panic!("unexpected payload {other:?}"),
        },
        other => panic!("unexpected result {other:?}"),
    };
    // The human projection is one stable line; the JSON carries the
    // document twice (typed build and receipt bytes stay consistent).
    assert!(json.contains(&format!("\"report\": \"{}\"", doc.report)));
    (doc, human)
}

fn check<'a>(report: &'a DoctorReport, id: &str) -> &'a Check {
    report
        .checks
        .iter()
        .find(|check| check.id == id)
        .unwrap_or_else(|| panic!("check {id} present"))
}

#[test]
fn fresh_fixture_is_ready_with_the_full_panel() {
    let selection = LoadSelection {
        project: Some(fixture_selection()),
    };
    let (report, human) = run(&selection, &git_available(), &doctor_options());
    assert_eq!(report.schema_version, "lekalo/doctor/v1.0.0");
    assert_eq!(report.identity, "dev.lekalo.doctor@1.0.0");
    assert_eq!(report.report, "doctor");
    assert_eq!(report.product_version, crate::lockfile::PRODUCT_VERSION);
    let ids: Vec<&str> = report.checks.iter().map(|c| c.id).collect();
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
    assert_eq!(check(&report, "project.root").state, CheckState::Ok);
    assert_eq!(check(&report, "model.version").state, CheckState::Ok);
    assert_eq!(
        report.revisions.model.version.as_deref(),
        Some("1.0.0"),
        "the fixture compiles under Model 1.0.0"
    );
    // No lock exists in the bare loader fixture: degraded, not blocked.
    assert_eq!(check(&report, "lock.freshness").state, CheckState::Degraded);
    assert_eq!(
        check(&report, "lock.freshness").reason,
        Some("absent"),
        "a missing lock is distinguishable from a stale one"
    );
    // Missing optional HLV evidence degrades; it never fails the core.
    assert_eq!(
        check(&report, "integrations.hlv").state,
        CheckState::Degraded
    );
    assert_eq!(
        check(&report, "integrations.hlv").reason,
        Some("not-supplied")
    );
    assert_eq!(report.verdict, super::model::Verdict::Blocked);
    assert!(human.starts_with("doctor "));
}

#[test]
fn status_panel_is_the_freshness_quartet() {
    let selection = LoadSelection {
        project: Some(fixture_selection()),
    };
    let options = Options {
        kind: ReportKind::Status,
        ..Options::default()
    };
    let (report, human) = run(&selection, &git_available(), &options);
    assert_eq!(report.report, "status");
    assert_eq!(report.phase, None);
    let ids: Vec<&str> = report.checks.iter().map(|c| c.id).collect();
    assert_eq!(
        ids,
        vec![
            "artifacts.drift",
            "bindings.freshness",
            "cache.health",
            "lock.freshness",
        ]
    );
    // Status is informational: degraded freshness never blocks the verdict.
    assert_eq!(report.verdict, super::model::Verdict::Degraded);
    assert!(human.starts_with("status "));
}

#[test]
fn readiness_phase_marks_required_and_degrades_on_optional() {
    let selection = LoadSelection {
        project: Some(fixture_selection()),
    };
    let options = Options {
        kind: ReportKind::Readiness,
        phase: Some(Phase::Model),
        ..Options::default()
    };
    let (report, human) = run(&selection, &git_available(), &options);
    assert_eq!(report.phase, Some("model"));
    let root = check(&report, "project.root");
    assert!(root.required, "model requires the project root");
    let lock = check(&report, "lock.freshness");
    assert!(!lock.required, "model phase does not require the lock");
    // Only optional checks fail here (no lock, no HLV): degraded verdict.
    assert_eq!(report.verdict, super::model::Verdict::Degraded);
    assert!(human.starts_with("readiness model "));
}

#[test]
fn fix_preview_lists_recipes_without_mutating() {
    let selection = LoadSelection {
        project: Some(fixture_selection()),
    };
    let options = Options {
        fix: true,
        ..Options::default()
    };
    let (report, _) = run(&selection, &git_available(), &options);
    let recipes = report.recipes.expect("--fix lists the recipe preview");
    assert!(
        recipes.iter().any(|recipe| recipe.id == "create-lock"),
        "the absent-lock check names its safe-fix recipe"
    );
    assert!(
        recipes.iter().any(|recipe| recipe.id == "trace-validate"),
        "the not-supplied integration check names its recipe"
    );
    for recipe in &recipes {
        assert!(!recipe.steps.is_empty());
        assert!(recipe.steps.len() <= super::version::MAX_RECIPE_STEPS);
    }
}

#[test]
fn doctor_never_writes_into_the_project() {
    let selection = fixture_selection();
    let dir = std::env::current_dir()
        .expect("cwd")
        .join(selection.replace('/', "\\"));
    let fingerprint = |dir: &std::path::Path| -> Vec<(String, u64)> {
        let mut entries = Vec::new();
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, u64)>) {
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
    };
    let before = fingerprint(&dir);
    let sel = LoadSelection {
        project: Some(selection),
    };
    let _ = report(&sel, &git_available(), &doctor_options());
    let after = fingerprint(&dir);
    assert_eq!(before, after, "doctor never mutates the project");
}

#[test]
fn git_unavailable_degrades_tools_without_blocking() {
    let selection = LoadSelection {
        project: Some(fixture_selection()),
    };
    let git = GitFacts {
        state: GitState::Unavailable,
        commit: None,
        dirty: None,
    };
    let (report, _) = run(&selection, &git, &doctor_options());
    let gates = check(&report, "tools.gates");
    assert_eq!(gates.state, CheckState::Degraded);
    assert_eq!(gates.reason, Some("git-unavailable"));
    assert!(report.revisions.git.commit.is_none());
}

#[test]
fn missing_project_reports_blocked_root_with_preserved_ids() {
    let selection = LoadSelection {
        project: Some("./definitely/not/here".to_owned()),
    };
    let (report, _) = run(&selection, &git_available(), &doctor_options());
    let root = check(&report, "project.root");
    assert_eq!(root.state, CheckState::Blocked);
    assert!(
        root.diagnostics
            .iter()
            .any(|id| id.starts_with("structure.")),
        "the structure refusal is preserved: {:?}",
        root.diagnostics
    );
    assert_eq!(report.verdict, super::model::Verdict::Blocked);
    // Downstream project checks report unknown, platform stays informational.
    assert_eq!(check(&report, "lock.freshness").state, CheckState::Unknown);
    assert_eq!(check(&report, "platform.limits").state, CheckState::Ok);
}

#[test]
fn platform_notes_stay_closed_and_sorted() {
    let selection = LoadSelection {
        project: Some(fixture_selection()),
    };
    let (report, _) = run(&selection, &git_available(), &doctor_options());
    let limits = check(&report, "platform.limits");
    assert_eq!(limits.state, CheckState::Ok);
    assert!(!limits.notes.is_empty());
    let mut sorted = limits.notes.clone();
    sorted.sort_unstable();
    assert_eq!(limits.notes, sorted.as_slice());
    for note in &limits.notes {
        assert!(
            matches!(
                *note,
                "case-sensitive-filesystem"
                    | "case-insensitive-filesystem"
                    | "posix-path-syntax"
                    | "windows-path-syntax"
                    | "symlink-privilege-required"
                    | "unicode-normalization-sensitive"
            ),
            "closed platform vocabulary, got {note}"
        );
    }
}

#[test]
fn recipe_catalog_is_closed() {
    for id in [
        "fix-structure",
        "create-lock",
        "preview-lock-update",
        "validate",
        "migrate-model",
        "resolve-adapters",
        "regenerate",
        "clear-cache",
        "recover-migration",
        "install-git",
        "init-git",
        "grant-write",
        "trace-validate",
    ] {
        assert!(super::checks::recipe(id).is_some(), "{id} is a recipe");
    }
    assert!(super::checks::recipe("delete-project").is_none());
}
