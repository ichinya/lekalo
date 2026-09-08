//! Issue #36 requirements integration tests: the closed attachment wire,
//! the read-only OpenSpec provider resolution (active changes, accepted
//! specs, archive symmetry, conflicts), the derived report, the neutral
//! trace-manifest projection, the gate verdicts, and the no-write
//! boundary.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::loader::LoadSelection;
use lekalo_core::requirements::{RequirementsAttachment, ResolutionVerdict};

/// The committed planner fixture: one minimal Lekalo project plus one
/// OpenSpec tree (two accepted requirements, one active change adding a
/// third) and one attachment referencing all three.
const FIXTURE: &str = "tests/fixtures/requirements/planner";
const ATTACHMENT: &str = "requirements.attachment.json";

/// Resolution touches the loader, which resolves selections against the
/// process working directory; serialize every test that changes it.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

/// Run `step` with the process working directory moved to `root`.
fn with_cwd<T>(root: &Path, step: impl FnOnce() -> T) -> T {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(root).expect("enter root");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(step));
    std::env::set_current_dir(original).expect("restore cwd");
    outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Parse the committed fixture attachment.
fn fixture_attachment() -> RequirementsAttachment {
    let bytes = fs::read(workspace_root().join(FIXTURE).join(ATTACHMENT))
        .expect("fixture attachment reads");
    RequirementsAttachment::parse(&bytes).expect("fixture attachment parses")
}

/// Resolve the fixture attachment from the committed fixture tree.
fn resolve_fixture() -> lekalo_core::requirements::Resolution {
    let attachment = fixture_attachment();
    with_cwd(&workspace_root(), || {
        let selection = LoadSelection {
            project: Some(FIXTURE.to_owned()),
        };
        attachment
            .resolve(&selection)
            .expect("fixture resolution succeeds")
    })
}

/// Copy the fixture into a fresh temporary directory and return the
/// temporary project root (the directory that holds `lekalo/` and
/// `openspec/`).
struct TempProject {
    root: PathBuf,
    _handle: tempfile::TempDir,
}

/// Recursive copy for the fixture tree (a small, known set of files).
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create target dir");
    for entry in fs::read_dir(from).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

fn temp_project() -> TempProject {
    let handle = tempfile::tempdir().expect("tempdir");
    let root = handle.path().join("planner");
    copy_dir(&workspace_root().join(FIXTURE), &root);
    TempProject {
        root,
        _handle: handle,
    }
}

/// Snapshot every file under `root` (relative path, exact bytes) for
/// the no-write assertion.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("contained path")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, fs::read(&path).expect("read file")));
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

/// A rewritten attachment wire value for the wire-rule tests.
fn attachment_json(mutation: impl FnOnce(&mut serde_json::Value)) -> serde_json::Value {
    let bytes = fs::read(workspace_root().join(FIXTURE).join(ATTACHMENT))
        .expect("fixture attachment reads");
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).expect("fixture json");
    mutation(&mut json);
    json
}

#[test]
fn golden_fixture_resolves_fresh() {
    let resolution = resolve_fixture();
    let report = &resolution.report;
    assert!(
        matches!(resolution.verdict, ResolutionVerdict::Pass),
        "fixture gate passes"
    );
    assert_eq!(report.project_id, "planner");
    // Two accepted requirements plus the one added by the active change.
    assert_eq!(report.requirements.len(), 3);
    let changed = report
        .requirements
        .iter()
        .find(|row| row.id == "planner.REQ-archive-notification")
        .expect("change-added requirement resolves");
    assert_eq!(changed.origin, "change");
    assert_eq!(
        changed.origin_id.as_deref(),
        Some("2026-09-01-archive-focus")
    );
    assert_eq!(report.references.len(), 3);
    assert!(
        report.references.iter().all(|row| row.status == "fresh"),
        "every pinned reference is fresh"
    );
    assert!(report.coverage_gaps.is_empty());
    assert!(report.conflicts.is_empty());
    assert!(report.impact.is_empty());
    // The catalog revision is the digest over the sorted catalog rows.
    assert_eq!(report.source_revision.len(), 64);
}

#[test]
fn resolution_is_deterministic_and_canonical() {
    let first = resolve_fixture();
    let second = resolve_fixture();
    let first_bytes = first.report.canonical_bytes().expect("canonical bytes");
    let second_bytes = second.report.canonical_bytes().expect("canonical bytes");
    assert_eq!(first_bytes, second_bytes, "report bytes are stable");
    // Canonical form: byte-sorted object keys, compact separators.
    let dynamic: serde_json::Value = serde_json::from_str(&first_bytes).expect("report parses");
    let resorted = serde_json::to_string(&dynamic).expect("re-serialize");
    assert_eq!(first_bytes, resorted, "keys are byte-sorted");
}

#[test]
fn provider_order_does_not_change_the_report() {
    let mut permuted = attachment_json(|json| {
        let providers = json["providers"].as_array().expect("providers").clone();
        json["providers"] = serde_json::Value::Array(providers.into_iter().rev().collect());
        let references = json["references"].as_array().expect("references").clone();
        json["references"] = serde_json::Value::Array(references.into_iter().rev().collect());
    });
    // The single-provider fixture needs a second namespace to make the
    // permutation meaningful; duplicate the tree under a new source.
    permuted["providers"] = serde_json::json!([
        { "source": "openspec", "kind": "openspec", "root": "openspec" },
        { "source": "vendor", "kind": "openspec", "root": "vendor/openspec" }
    ]);
    let attachment =
        RequirementsAttachment::from_value(&permuted).expect("permuted attachment parses");
    assert_eq!(
        attachment.providers()[0].source,
        "openspec",
        "providers canonicalize to source order"
    );
}

#[test]
fn model_pin_mismatch_denies_resolution() {
    let mut json = attachment_json(|json| {
        json["modelRef"]["digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    });
    json["modelRef"]["digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let attachment = RequirementsAttachment::from_value(&json).expect("parses");
    let outcome = with_cwd(&workspace_root(), || {
        attachment.resolve(&LoadSelection {
            project: Some(FIXTURE.to_owned()),
        })
    });
    let failure = match outcome {
        Ok(_) => panic!("pin mismatch must refuse"),
        Err(failure) => failure,
    };
    assert_eq!(failure.exit_code(), 3, "custody mismatch denies");
    assert_eq!(
        failure.diagnostics()[0].id(),
        "requirements.model-ref-mismatch"
    );
}

#[test]
fn unknown_reference_targets_refuse() {
    let mut json = attachment_json(|json| {
        json["references"][0]["symbol"] = serde_json::json!("planner.missing_symbol");
    });
    let attachment = RequirementsAttachment::from_value(&json).expect("parses");
    let outcome = with_cwd(&workspace_root(), || {
        attachment.resolve(&LoadSelection {
            project: Some(FIXTURE.to_owned()),
        })
    });
    let failure = match outcome {
        Ok(_) => panic!("unknown symbol must refuse"),
        Err(failure) => failure,
    };
    assert_eq!(failure.exit_code(), 1);
    assert_eq!(failure.diagnostics()[0].id(), "requirements.ref-unknown");

    json["references"][0]["symbol"] = serde_json::json!("planner.focus_task");
    json["references"][0]["source"] = serde_json::json!("unknown");
    let attachment = RequirementsAttachment::from_value(&json).expect("parses");
    let outcome = with_cwd(&workspace_root(), || {
        attachment.resolve(&LoadSelection {
            project: Some(FIXTURE.to_owned()),
        })
    });
    let failure = match outcome {
        Ok(_) => panic!("unknown source must refuse"),
        Err(failure) => failure,
    };
    assert_eq!(failure.diagnostics()[0].id(), "requirements.ref-unknown");
}

#[test]
fn wire_rejects_closed_violations() {
    // Unknown top-level field.
    let error = RequirementsAttachment::from_value(&attachment_json(|json| {
        json["extra"] = serde_json::json!(true);
    }))
    .expect_err("unknown field refuses");
    assert_eq!(error.as_slice()[0].id(), "requirements.document-invalid");

    // Duplicate reference tuple.
    let _unused = RequirementsAttachment::from_value(&attachment_json(|json| {
        let first = json["references"][0].clone();
        json["references"]
            .as_array_mut()
            .expect("array")
            .push(first);
    }))
    .expect_err("duplicate reference refuses");

    // Nested provider roots.
    let _unused = RequirementsAttachment::from_value(&attachment_json(|json| {
        json["providers"] = serde_json::json!([
            { "source": "a", "kind": "openspec", "root": "openspec" },
            { "source": "b", "kind": "openspec", "root": "openspec/changes" }
        ]);
    }))
    .expect_err("nested roots refuse");

    // Malformed revision digest.
    let _unused = RequirementsAttachment::from_value(&attachment_json(|json| {
        json["references"][0]["revision"] = serde_json::json!("sha256:short");
    }))
    .expect_err("bad digest refuses");
}

#[test]
fn stale_body_denies_and_reports_impact() {
    let project = temp_project();
    let path = project.root.join("openspec/specs/planner/spec.md");
    let edited = fs::read_to_string(&path).expect("spec reads").replace(
        "report the previously focused task",
        "report the previously focused TASK",
    );
    fs::write(&path, edited).expect("spec writes");

    let attachment = fixture_attachment();
    let resolution = with_cwd(project.root.parent().expect("parent"), || {
        attachment
            .resolve(&LoadSelection {
                project: Some("planner".to_owned()),
            })
            .expect("resolution succeeds")
    });
    let report = &resolution.report;
    let stale = report
        .references
        .iter()
        .find(|row| row.requirement == "planner.REQ-focus-task")
        .expect("focus reference");
    assert_eq!(stale.status, "stale");
    assert_ne!(
        stale.current_revision.as_deref(),
        Some(stale.revision.as_str())
    );
    assert!(
        report
            .impact
            .iter()
            .any(|row| row.requirement == "planner.REQ-focus-task"
                && row.change == "changed"
                && row.symbols == vec!["planner.focus_task".to_owned()]),
        "changed requirement impact lists the affected symbol"
    );
    assert!(matches!(resolution.verdict, ResolutionVerdict::Denied(_)));
}

#[test]
fn rename_is_detected_as_an_id_move() {
    let project = temp_project();
    let path = project.root.join("openspec/specs/planner/spec.md");
    let edited = fs::read_to_string(&path).expect("spec reads").replace(
        "### Requirement: Restore focus",
        "### Requirement: Restore last focus",
    );
    fs::write(&path, edited).expect("spec writes");

    let attachment = fixture_attachment();
    let resolution = with_cwd(project.root.parent().expect("parent"), || {
        attachment
            .resolve(&LoadSelection {
                project: Some("planner".to_owned()),
            })
            .expect("resolution succeeds")
    });
    let stale = resolution
        .report
        .references
        .iter()
        .find(|row| row.requirement == "planner.REQ-restore-focus")
        .expect("renamed reference");
    assert_eq!(stale.status, "missing");
    assert_eq!(
        stale.renamed_to.as_deref(),
        Some("planner.REQ-restore-last-focus"),
        "the pinned body moved to the new id"
    );
    assert!(
        resolution
            .report
            .impact
            .iter()
            .any(|row| row.change == "renamed"
                && row.renamed_to.as_deref() == Some("planner.REQ-restore-last-focus")),
        "rename impact carries the new id"
    );
}

#[test]
fn removal_is_reported_as_removed() {
    let project = temp_project();
    let path = project.root.join("openspec/specs/planner/spec.md");
    let edited = fs::read_to_string(&path)
        .expect("spec reads")
        .replace(
            "### Requirement: Restore focus\n\nThe planner SHALL restore the most recent focus when the focused task\nis archived without an explicit successor.\n",
            "",
        );
    fs::write(&path, edited).expect("spec writes");

    let attachment = fixture_attachment();
    let resolution = with_cwd(project.root.parent().expect("parent"), || {
        attachment
            .resolve(&LoadSelection {
                project: Some("planner".to_owned()),
            })
            .expect("resolution succeeds")
    });
    let removed = resolution
        .report
        .references
        .iter()
        .find(|row| row.requirement == "planner.REQ-restore-focus")
        .expect("removed reference");
    assert_eq!(removed.status, "missing");
    assert_eq!(removed.renamed_to, None);
    assert!(
        resolution
            .report
            .impact
            .iter()
            .any(|row| row.change == "removed"),
        "removal impact is explicit"
    );
}

#[test]
fn conflicting_active_changes_block_resolution() {
    let project = temp_project();
    for (change, version) in [
        ("2026-09-02-edit-a", "Version A of the restore rule."),
        ("2026-09-03-edit-b", "Version B of the restore rule."),
    ] {
        let dir = project
            .root
            .join("openspec/changes")
            .join(change)
            .join("specs/planner");
        fs::create_dir_all(&dir).expect("change dir");
        fs::write(
            dir.join("spec.md"),
            format!("## MODIFIED Requirements\n\n### Requirement: Restore focus\n\n{version}\n"),
        )
        .expect("delta writes");
    }
    let attachment = fixture_attachment();
    let resolution = with_cwd(project.root.parent().expect("parent"), || {
        attachment
            .resolve(&LoadSelection {
                project: Some("planner".to_owned()),
            })
            .expect("resolution succeeds")
    });
    assert_eq!(resolution.report.conflicts.len(), 1);
    assert_eq!(resolution.report.conflicts[0].detail, "multiple-changes");
    assert_eq!(resolution.report.conflicts[0].title, "Restore focus");
    let disputed = resolution
        .report
        .references
        .iter()
        .find(|row| row.requirement == "planner.REQ-restore-focus")
        .expect("disputed reference");
    assert_eq!(disputed.status, "conflict");
    assert!(matches!(resolution.verdict, ResolutionVerdict::Denied(_)));
}

#[test]
fn archive_preserves_accepted_traceability() {
    let project = temp_project();
    let attachment = fixture_attachment();
    let selection = || LoadSelection {
        project: Some("planner".to_owned()),
    };
    let before = with_cwd(project.root.parent().expect("parent"), || {
        attachment.resolve(&selection()).expect("before resolution")
    });
    let before_row = before
        .report
        .requirements
        .iter()
        .find(|row| row.id == "planner.REQ-archive-notification")
        .expect("change-added requirement");
    let spec_path = project.root.join("openspec/specs/planner/spec.md");
    let delta_path = project
        .root
        .join("openspec/changes/2026-09-01-archive-focus/specs/planner/spec.md");
    let spec = fs::read_to_string(&spec_path).expect("spec reads");
    let delta = fs::read_to_string(&delta_path).expect("delta reads");
    let block = delta
        .split("### Requirement:")
        .nth(1)
        .map(|rest| format!("### Requirement:{rest}"))
        .expect("delta block");
    fs::write(
        &spec_path,
        format!("{}\n\n{}\n", spec.trim_end(), block.trim_end()),
    )
    .expect("spec writes");
    fs::create_dir_all(project.root.join("openspec/changes/archive")).expect("archive dir");
    fs::rename(
        project
            .root
            .join("openspec/changes/2026-09-01-archive-focus"),
        project
            .root
            .join("openspec/changes/archive/2026-09-01-archive-focus"),
    )
    .expect("archive move");

    let after = with_cwd(project.root.parent().expect("parent"), || {
        attachment.resolve(&selection()).expect("after resolution")
    });
    let after_row = after
        .report
        .requirements
        .iter()
        .find(|row| row.id == "planner.REQ-archive-notification")
        .expect("archived requirement still resolves");
    assert_eq!(after_row.origin, "accepted");
    assert_eq!(after_row.digest, before_row.digest, "body digest survives");
    assert_eq!(after_row.id, before_row.id, "id survives");
    assert!(
        matches!(after.verdict, ResolutionVerdict::Pass),
        "traceability is fresh across the archive"
    );
}

#[test]
fn absent_provider_tree_is_unavailable_when_referenced() {
    let project = temp_project();
    fs::rename(
        project.root.join("openspec"),
        project.root.join("openspec-absent"),
    )
    .expect("hide tree");
    let attachment = fixture_attachment();
    let outcome = with_cwd(project.root.parent().expect("parent"), || {
        attachment.resolve(&LoadSelection {
            project: Some("planner".to_owned()),
        })
    });
    let failure = match outcome {
        Ok(_) => panic!("absent tree must refuse"),
        Err(failure) => failure,
    };
    assert_eq!(failure.exit_code(), 4, "unavailability is exit 4");
    assert_eq!(
        failure.diagnostics()[0].id(),
        "requirements.provider-unavailable"
    );
}

#[test]
fn standalone_attachment_without_providers_is_valid() {
    let json = attachment_json(|json| {
        json["providers"] = serde_json::json!([]);
        json["references"] = serde_json::json!([]);
    });
    let attachment = RequirementsAttachment::from_value(&json).expect("parses");
    let resolution = with_cwd(&workspace_root(), || {
        attachment
            .resolve(&LoadSelection {
                project: Some(FIXTURE.to_owned()),
            })
            .expect("standalone resolution succeeds")
    });
    assert!(resolution.report.requirements.is_empty());
    assert!(matches!(resolution.verdict, ResolutionVerdict::Pass));
}

#[test]
fn resolution_never_writes_anything() {
    let project = temp_project();
    let before = snapshot(&project.root);
    let attachment = fixture_attachment();
    with_cwd(project.root.parent().expect("parent"), || {
        let selection = LoadSelection {
            project: Some("planner".to_owned()),
        };
        let resolution = attachment.resolve(&selection).expect("resolves");
        let _ = resolution.report.canonical_bytes();
        let _ = resolution.report.trace_manifest();
    });
    let after = snapshot(&project.root);
    assert_eq!(before, after, "the integration is read-only");
}

#[test]
fn trace_projection_validates_against_the_accepted_validator() {
    let resolution = resolve_fixture();
    let manifest = resolution
        .report
        .trace_manifest()
        .expect("projection validates");
    let report = manifest.report();
    // Requirement nodes (3) plus the two referenced symbols.
    assert_eq!(report.node_count, 5);
    assert_eq!(report.relation_count, 3);
    assert_eq!(report.manifest_id, "requirements-trace");
    assert_eq!(report.completeness.as_str(), "partial");
    assert_eq!(report.export_profile.as_str(), "requirement-to-gate");
    // No gate/test chain exists in this projection, so every requirement
    // sink is uncovered — and the manifest says so explicitly.
    assert_eq!(report.uncovered_sinks.len(), 3);
    let bytes = manifest.canonical_bytes().expect("canonical bytes");
    let again = resolve_fixture()
        .report
        .trace_manifest()
        .expect("projection validates")
        .canonical_bytes()
        .expect("canonical bytes");
    assert_eq!(bytes, again, "projection is deterministic");
}

#[test]
fn committed_goldens_match_the_canonical_writer() {
    let resolution = resolve_fixture();
    let report_bytes = resolution
        .report
        .canonical_bytes()
        .expect("canonical report bytes");
    let golden_report = fs::read_to_string(
        workspace_root().join("tests/fixtures/requirements/golden/planner.report.json"),
    )
    .expect("report golden reads");
    assert_eq!(
        report_bytes,
        golden_report.trim_end(),
        "report golden is byte-identical"
    );

    let manifest = resolution
        .report
        .trace_manifest()
        .expect("projection validates");
    let trace_bytes = manifest.canonical_bytes().expect("canonical trace bytes");
    let golden_trace = fs::read_to_string(
        workspace_root().join("tests/fixtures/requirements/golden/planner.trace.json"),
    )
    .expect("trace golden reads");
    assert_eq!(
        trace_bytes,
        golden_trace.trim_end(),
        "trace golden is byte-identical"
    );
    let sidecar = fs::read_to_string(
        workspace_root().join("tests/fixtures/requirements/golden/planner.trace.json.sha256"),
    )
    .expect("trace sidecar reads");
    assert_eq!(sidecar.trim(), manifest.digest().expect("digest"));
}

#[test]
fn empty_projection_refuses() {
    let json = attachment_json(|json| {
        json["providers"] =
            serde_json::json!([{ "source": "openspec", "kind": "openspec", "root": "openspec" }]);
        json["references"] = serde_json::json!([]);
    });
    let attachment = RequirementsAttachment::from_value(&json).expect("parses");
    let resolution = with_cwd(&workspace_root(), || {
        attachment
            .resolve(&LoadSelection {
                project: Some(FIXTURE.to_owned()),
            })
            .expect("resolves")
    });
    // A catalog exists here, so the trace projection is fine; the empty
    // refusal applies to a tree with no requirements at all.
    assert!(resolution.report.trace_manifest().is_ok());
}
