//! Direct core-API boundary regressions for `lekalo_core::init::adopt`
//! (issue #38 correction 2).
//!
//! The CLI always validated `--target`/`--profile` before calling the
//! core; these tests prove the public library entry itself now fails
//! closed on the same inputs — before any root discovery, detection
//! walk, read probe, or write — in debug and release builds alike. The
//! refusal is the registered `cli.usage` failure and the wire never
//! echoes the rejected value or any absolute probe path. Every hostile
//! value addresses a unique externally owned temp tree; nothing outside
//! that owned tree is read or written.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::init::{adopt, AdoptRequest};
use lekalo_core::{DomainResult, Status};

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// One unique externally owned probe. The adopted root is `probe1/`
/// inside the temp tree, so the pre-fix `../../../` escape would have
/// written one level above the adopted root — still inside this owned
/// tree, where the test can prove it never happens.
struct Probe {
    _outer: tempfile::TempDir,
    root: PathBuf,
}

impl Probe {
    fn new() -> Self {
        let outer = tempfile::tempdir().expect("unique external temp root");
        let root = outer.path().join("probe1");
        std::fs::create_dir(&root).expect("adopted probe root");
        Self {
            _outer: outer,
            root,
        }
    }

    fn outer(&self) -> &Path {
        self._outer.path()
    }
}

/// Run one adoption request with the process working directory moved
/// into the adopted probe root (the lock serializes every test that
/// changes the directory).
fn adopt_at(root: &Path, request: AdoptRequest) -> DomainResult {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(root).expect("enter the probe root");
    let outcome = adopt(&request);
    std::env::set_current_dir(original).expect("restore the working directory");
    outcome
}

/// The derived reason-code spellings of one result.
fn reasons(result: &DomainResult) -> Vec<String> {
    result
        .reason_codes()
        .iter()
        .map(|code| code.as_str().to_owned())
        .collect()
}

/// `(relative path, is_dir)` for every entry under `root`, in
/// deterministic order.
fn snapshot(root: &Path) -> Vec<(String, bool)> {
    fn walk(current: &Path, prefix: &str, out: &mut Vec<(String, bool)>) {
        let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(current)
            .expect("probe entries readable")
            .filter_map(|entry| entry.ok())
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let is_dir = entry.file_type().expect("entry type").is_dir();
            out.push((relative.clone(), is_dir));
            if is_dir {
                walk(&entry.path(), &relative, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

/// The probe tree is untouched: no write, no read-probe artifact, and in
/// particular no canonical `lekalo` home was created.
fn assert_untouched(probe: &Probe) {
    assert_eq!(
        snapshot(probe.outer()),
        vec![("probe1".to_owned(), true)],
        "the probe tree must stay byte-identical"
    );
    assert!(
        !probe.root.join("lekalo").exists(),
        "no canonical project dir may appear"
    );
}

/// The exact registered usage refusal: invalid status, exit 1, stderr,
/// the `cli.usage` reason alone, and no echo of the rejected values or
/// of any absolute probe path on the wire or in the human text.
fn assert_usage_refusal(result: &DomainResult, probe: &Probe, rejected: &[&str]) {
    assert_eq!(
        result.status(),
        Status::Invalid,
        "{}",
        result.to_json_string()
    );
    assert_eq!(result.exit_code(), 1);
    assert!(result.writes_stderr());
    assert_eq!(reasons(result), vec!["cli.usage"]);
    let wire = result.to_json_string();
    let human = result.to_human_string("lekalo");
    let mut forbidden: Vec<&str> = rejected
        .iter()
        .filter(|value| !value.is_empty())
        .copied()
        .collect();
    let root_spelling = probe.root.to_string_lossy().into_owned();
    let outer_spelling = probe.outer().to_string_lossy().into_owned();
    forbidden.push(root_spelling.as_str());
    forbidden.push(outer_spelling.as_str());
    for value in forbidden {
        assert!(!wire.contains(value), "wire echo of {value:?}: {wire}");
        assert!(!human.contains(value), "human echo of {value:?}: {human}");
    }
    assert_untouched(probe);
}

#[test]
fn traversal_target_fails_closed_before_any_probe_or_write() {
    let probe = Probe::new();
    let result = adopt_at(
        &probe.root,
        AdoptRequest {
            target: Some("../../../escaped-outside".to_owned()),
            ..AdoptRequest::default()
        },
    );
    assert_usage_refusal(&result, &probe, &["escaped-outside"]);
    // The pre-fix escape wrote one level above the adopted root and
    // reported success; the boundary now refuses before any resolution.
    assert!(!probe.outer().join("escaped-outside.yaml").exists());
}

#[test]
fn absolute_backslash_and_escape_targets_fail_closed() {
    for target in [
        "/abs-escape",
        "C:\\escape-target",
        "back\\slash-target",
        "..",
        "a/../b",
    ] {
        let probe = Probe::new();
        let result = adopt_at(
            &probe.root,
            AdoptRequest {
                target: Some(target.to_owned()),
                ..AdoptRequest::default()
            },
        );
        assert_usage_refusal(&result, &probe, &[target]);
    }
}

#[test]
fn invalid_target_grammars_fail_closed() {
    for target in [
        "Bad_Target",
        "Node",
        "node typescript",
        "",
        "-leading",
        "has.dot",
        "tödlich",
    ] {
        let probe = Probe::new();
        let result = adopt_at(
            &probe.root,
            AdoptRequest {
                target: Some(target.to_owned()),
                ..AdoptRequest::default()
            },
        );
        assert_usage_refusal(&result, &probe, &[target]);
    }
}

#[test]
fn malformed_and_injection_profiles_fail_closed() {
    for profile in [
        "Default_Profile",
        "UPPER",
        "with space",
        "with_underscore",
        "x\", \"injected\": true}",
        "quote\"inside",
        "tab\tinside",
    ] {
        let probe = Probe::new();
        let result = adopt_at(
            &probe.root,
            AdoptRequest {
                target: Some("node-typescript".to_owned()),
                profile: Some(profile.to_owned()),
                ..AdoptRequest::default()
            },
        );
        assert_usage_refusal(&result, &probe, &[profile]);
    }
}

#[test]
fn orphan_profile_fails_closed() {
    let probe = Probe::new();
    let result = adopt_at(
        &probe.root,
        AdoptRequest {
            profile: Some("default".to_owned()),
            ..AdoptRequest::default()
        },
    );
    assert_usage_refusal(&result, &probe, &[]);
}

#[test]
fn dry_run_is_refused_like_apply_for_invalid_requests() {
    for (target, profile) in [
        (Some("../../../escaped-outside"), None),
        (Some("Bad_Target"), None),
        (Some("node-typescript"), Some("Default_Profile")),
        (None, Some("default")),
    ] {
        let probe = Probe::new();
        let result = adopt_at(
            &probe.root,
            AdoptRequest {
                target: target.map(str::to_owned),
                profile: profile.map(str::to_owned),
                dry_run: true,
                ..AdoptRequest::default()
            },
        );
        // A dry run plans nothing and writes nothing; an invalid request
        // must not even produce a plan receipt or a read-probe.
        assert_usage_refusal(&result, &probe, &[]);
    }
}

#[test]
fn explicit_project_id_grammar_fails_closed() {
    let probe = Probe::new();
    let result = adopt_at(
        &probe.root,
        AdoptRequest {
            project: Some(".".to_owned()),
            project_id: Some("Bad_ID".to_owned()),
            target: Some("node-typescript".to_owned()),
            profile: Some("strict".to_owned()),
            ..AdoptRequest::default()
        },
    );
    assert_eq!(
        result.status(),
        Status::Denied,
        "{}",
        result.to_json_string()
    );
    assert_eq!(result.exit_code(), 3);
    assert!(!result.writes_stderr());
    assert_eq!(reasons(&result), vec!["init.adopt-id-required"]);
    let wire = result.to_json_string();
    assert!(
        !wire.contains("Bad_ID"),
        "no echo of the rejected id: {wire}"
    );
    assert_untouched(&probe);
}

#[test]
fn no_profile_control_writes_only_the_legacy_project_document() {
    let probe = Probe::new();
    let request = || AdoptRequest {
        project: Some(".".to_owned()),
        project_id: Some("probe".to_owned()),
        ..AdoptRequest::default()
    };
    let result = adopt_at(&probe.root, request());
    assert_eq!(
        result.status(),
        Status::Valid,
        "{}",
        result.to_json_string()
    );
    assert_eq!(result.exit_code(), 0);
    let receipt: serde_json::Value =
        serde_json::from_str(&result.to_json_string()).expect("receipt parses");
    assert_eq!(receipt["mode"], "apply");
    assert_eq!(receipt["changed"], true);
    assert_eq!(receipt["created"], 1);
    assert_eq!(receipt["projectId"], "probe");
    assert!(receipt["target"].is_null());
    assert!(receipt["adapterProfile"].is_null());
    assert_eq!(receipt["gate"]["status"], "valid");
    assert_eq!(receipt["gate"]["modelVersion"], "1.0.0");
    let project_bytes =
        std::fs::read(probe.root.join("lekalo/project.yaml")).expect("project document");
    assert_eq!(
        project_bytes,
        b"{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"probe\",\"kind\":\"project\",\"version\":1,\"description\":\"Adopted existing project.\"}]}\n"
    );
    assert!(
        !probe.root.join("lekalo/targets").exists(),
        "no target home without an explicit target"
    );

    // Idempotence: the identical second run writes nothing.
    let again = adopt_at(&probe.root, request());
    assert_eq!(again.status(), Status::Valid, "{}", again.to_json_string());
    let receipt: serde_json::Value =
        serde_json::from_str(&again.to_json_string()).expect("receipt parses");
    assert_eq!(receipt["changed"], false);
    assert_eq!(receipt["alreadyPresent"], 1);
    assert_eq!(receipt["created"], 0);
    assert!(receipt["gate"].is_null());
    assert_eq!(
        std::fs::read(probe.root.join("lekalo/project.yaml")).expect("project document"),
        project_bytes
    );
}

#[test]
fn explicit_profile_control_records_exact_bytes_idempotence_and_conflict() {
    let probe = Probe::new();
    let request = || AdoptRequest {
        project: Some(".".to_owned()),
        project_id: Some("probe".to_owned()),
        target: Some("node-typescript".to_owned()),
        profile: Some("strict".to_owned()),
        ..AdoptRequest::default()
    };
    let result = adopt_at(&probe.root, request());
    assert_eq!(
        result.status(),
        Status::Valid,
        "{}",
        result.to_json_string()
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&result.to_json_string()).expect("receipt parses");
    assert_eq!(receipt["created"], 2);
    assert_eq!(receipt["target"], "node-typescript");
    assert_eq!(receipt["adapterProfile"], "strict");
    let target_bytes = std::fs::read(probe.root.join("lekalo/targets/node-typescript.yaml"))
        .expect("target document");
    assert_eq!(
        target_bytes,
        b"{\"target\":\"node-typescript\",\"profile\":\"strict\",\"note\":\"Adopted target selection.\"}\n"
    );

    // Idempotent re-run with the identical explicit selection.
    let again = adopt_at(&probe.root, request());
    assert_eq!(again.status(), Status::Valid, "{}", again.to_json_string());
    let receipt: serde_json::Value =
        serde_json::from_str(&again.to_json_string()).expect("receipt parses");
    assert_eq!(receipt["changed"], false);
    assert_eq!(receipt["alreadyPresent"], 2);

    // The same target with a different profile plans different bytes and
    // denies as a no-overwrite conflict instead of overwriting; the
    // explicit-profile document survives byte-exact.
    let conflict = adopt_at(
        &probe.root,
        AdoptRequest {
            profile: Some("default".to_owned()),
            ..request()
        },
    );
    assert_eq!(
        conflict.status(),
        Status::Denied,
        "{}",
        conflict.to_json_string()
    );
    assert_eq!(conflict.exit_code(), 3);
    assert_eq!(reasons(&conflict), vec!["init.adopt-conflict"]);
    assert_eq!(
        std::fs::read(probe.root.join("lekalo/targets/node-typescript.yaml"))
            .expect("target document"),
        target_bytes
    );
}
