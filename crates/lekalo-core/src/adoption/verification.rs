//! Qualified selective verification (issue #136, increment I4): the
//! pinned specification is really executed — baseline in a pristine
//! disposable copy, candidate in a patched disposable copy — with
//! bounded output, bounded runtime, per-check independent expectations,
//! and an explicit behavior oracle. A specification or a test id never
//! decodes as an execution receipt: only this module's execution
//! variant, produced by actual runs, carries outcomes.
//!
//! Where the qualified tool is missing or a command cannot run, the
//! conclusion is honestly `unavailable` and no test is reported as
//! passed. Never a marker, never an environment backdoor.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::diagnostic;
use super::version;
use super::wire::{
    self, CheckOutcome, CheckResults, OracleVerdict, PatchDocument, RefactorPlan,
    VerificationDocument,
};
use super::Outcome;
use crate::result::DomainResult;

/// The qualified tool allow-list. A specification naming any other
/// program is refused before any process exists; the program must be a
/// bare name resolved through PATH, never a path with separators.
const QUALIFIED_PROGRAMS: [&str; 7] = ["node", "npm", "npx", "deno", "bun", "python", "python3"];

/// Directories never copied into a disposable candidate copy.
const COPY_EXCLUDED: [&str; 3] = [".git", "target", ".lekalo"];

/// Execute one specification against the current tree: baseline and
/// candidate runs happen inside disposable copies only; the project is
/// never modified and no report or cache home is written.
pub fn execute(
    root: &Path,
    plan: &RefactorPlan,
    spec_bytes: &[u8],
    patch: Option<&PatchDocument>,
) -> Outcome<VerificationDocument> {
    let spec = wire::verification_of_bytes(spec_bytes)
        .map_err(|error| DomainResult::invalid(error.set()))?;
    if spec.variant != "specification" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "not-a-specification",
        )));
    }
    let spec_digest = {
        let value = serde_json::to_value(&spec).expect("spec serializes");
        wire::digest("adoption-verification", &value)
    };
    if plan.verification.as_deref() != Some(spec_digest.as_str()) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "plan-mismatch",
        )));
    }
    let Some(tool) = &spec.tool else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "tool-missing",
        )));
    };
    if !QUALIFIED_PROGRAMS.contains(&tool.program.as_str())
        || tool.program.contains('/')
        || tool.program.contains('\\')
    {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            &tool.program,
            "tool-not-qualified",
        )));
    }
    let Some(program) = resolve_program(&tool.program) else {
        let mut execution = shell(plan, &spec, spec_digest, None, None);
        execution.conclusion = Some("unavailable".to_owned());
        return Ok(execution);
    };

    // Baseline: a pristine disposable copy.
    let baseline_copy = copy_tree(root)?;
    let baseline = run_checks(&program, &tool.args, &spec.checks, &baseline_copy);

    // Candidate: a second disposable copy with the exact patch applied.
    let candidate_result = match patch {
        Some(document) => {
            let candidate_copy = copy_tree(root)?;
            apply_patch(&candidate_copy, document)?;
            run_checks(&program, &tool.args, &spec.checks, &candidate_copy)
        }
        None => CheckResults {
            checks: spec
                .checks
                .iter()
                .map(|check| CheckOutcome {
                    id: check.id.clone(),
                    status: "unavailable".to_owned(),
                    exit: None,
                    detail: "no-candidate".to_owned(),
                })
                .collect(),
        },
    };
    let _ = std::fs::remove_dir_all(&baseline_copy);

    // The behavior oracle: the kind=behavior-oracle checks must pass on
    // both sides. A shallow happy-path test passing beside a broken
    // oracle is a failed verification, never a completion.
    let oracle_ids: Vec<String> = spec
        .checks
        .iter()
        .filter(|check| check.kind == "behavior-oracle")
        .map(|check| check.id.clone())
        .collect();
    let oracle_failed: Vec<String> = oracle_ids
        .iter()
        .filter(|id| {
            let baseline_check = baseline.checks.iter().find(|c| &c.id == *id);
            let candidate_check = candidate_result.checks.iter().find(|c| &c.id == *id);
            !matches!(
                (
                    baseline_check.map(|c| c.status.as_str()),
                    candidate_check.map(|c| c.status.as_str())
                ),
                (Some("passed"), Some("passed"))
            )
        })
        .cloned()
        .collect();
    let oracle = if oracle_ids.is_empty() {
        OracleVerdict {
            status: "unavailable".to_owned(),
            detail: "no-oracle-declared".to_owned(),
        }
    } else if oracle_failed.is_empty() {
        OracleVerdict {
            status: "passed".to_owned(),
            detail: "declared-obligations-held".to_owned(),
        }
    } else {
        OracleVerdict {
            status: "failed".to_owned(),
            detail: oracle_failed.join(","),
        }
    };
    let any_unavailable = baseline
        .checks
        .iter()
        .chain(candidate_result.checks.iter())
        .any(|check| check.status == "unavailable");
    let any_failed = baseline
        .checks
        .iter()
        .chain(candidate_result.checks.iter())
        .any(|check| check.status == "failed");
    let conclusion = if any_unavailable && !any_failed {
        "unavailable"
    } else if any_failed || oracle.status == "failed" {
        "failed"
    } else if oracle.status == "unavailable" {
        "unavailable"
    } else {
        "preserved"
    };
    let mut execution = shell(
        plan,
        &spec,
        spec_digest,
        Some(baseline),
        Some(candidate_result),
    );
    execution.oracle = Some(oracle);
    execution.conclusion = Some(conclusion.to_owned());
    Ok(execution)
}

/// Assemble the execution envelope around the measured outcomes.
fn shell(
    plan: &RefactorPlan,
    spec: &VerificationDocument,
    spec_digest: String,
    baseline: Option<CheckResults>,
    candidate: Option<CheckResults>,
) -> VerificationDocument {
    VerificationDocument {
        schema_version: version::VERIFICATION_SCHEMA_VERSION.to_owned(),
        identity: version::VERIFICATION_IDENTITY.to_owned(),
        variant: "execution".to_owned(),
        verification: spec.verification.clone(),
        spec: Some(spec_digest),
        plan: Some(plan.plan.clone()),
        tool: spec.tool.clone(),
        baseline_source: spec.baseline_source.clone(),
        checks: spec.checks.clone(),
        confinement: vec!["disposable-copy".to_owned(), "bounded-output".to_owned()],
        baseline,
        candidate,
        oracle: None,
        conclusion: None,
    }
}

/// Resolve one qualified program name through PATH.
fn resolve_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    std::env::split_paths(&path)
        .map(|directory| directory.join(&executable))
        .find(|candidate| candidate.is_file())
}

/// Run every check once against one disposable copy.
fn run_checks(
    program: &Path,
    args: &[String],
    checks: &[super::wire::CheckSpec],
    copy: &Path,
) -> CheckResults {
    let mut outcomes = Vec::new();
    let run = execute_command(program, args, copy);
    for check in checks {
        let outcome = match &run {
            Err(detail) => CheckOutcome {
                id: check.id.clone(),
                status: "unavailable".to_owned(),
                exit: None,
                detail: detail.clone(),
            },
            Ok(measured) => {
                let exit_ok = measured.0 == Some(check.expect_exit as i32);
                let stdout_ok = check
                    .expect_stdout
                    .iter()
                    .all(|expected| measured.1.contains(expected.as_str()));
                let status = if exit_ok && stdout_ok {
                    "passed"
                } else if measured.0.is_none() {
                    "unavailable"
                } else {
                    "failed"
                };
                CheckOutcome {
                    id: check.id.clone(),
                    status: status.to_owned(),
                    exit: measured.0.map(|exit| exit.max(0) as u32),
                    detail: if !exit_ok {
                        "exit-mismatch".to_owned()
                    } else if !stdout_ok {
                        "stdout-missing".to_owned()
                    } else {
                        "ok".to_owned()
                    },
                }
            }
        };
        outcomes.push(outcome);
    }
    CheckResults { checks: outcomes }
}

/// One bounded run: `(exit code, bounded stdout)`. A timeout, a spawn
/// failure, or a signal exit is measured, never invented.
fn execute_command(
    program: &Path,
    args: &[String],
    cwd: &Path,
) -> Result<(Option<i32>, String), String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn-refused:{}", error.kind()))?;
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let exit = status.code();
            let stdout = child
                .stdout
                .take()
                .map(|mut stream| read_capped(&mut stream))
                .unwrap_or_default();
            return Ok((exit, stdout));
        }
        if started.elapsed() > Duration::from_millis(version::CHECK_TIMEOUT_MS) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("timeout".to_owned());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Read a stream up to the bounded output ceiling; the remainder is
/// discarded (bounded output is an enforced confinement dimension).
fn read_capped(stream: &mut impl std::io::Read) -> String {
    let mut buffer = vec![0u8; version::CHECK_OUTPUT_BYTES];
    let mut read = 0;
    loop {
        let filled = stream.read(&mut buffer[read..]).unwrap_or(0);
        if filled == 0 {
            break;
        }
        read += filled;
        if read >= version::CHECK_OUTPUT_BYTES {
            break;
        }
    }
    String::from_utf8_lossy(&buffer[..read]).to_string()
}

/// Copy the project into a fresh disposable directory, skipping VCS,
/// build, and runtime-state homes, under strict byte and file ceilings.
fn copy_tree(root: &Path) -> Outcome<PathBuf> {
    let copy = tempfile::tempdir()
        .map_err(|_| {
            DomainResult::invalid(diagnostic::evidence_incomplete_set(
                "copy",
                "tempdir-unavailable",
            ))
        })?
        .keep();
    let destination = copy.join("project");
    std::fs::create_dir_all(&destination).map_err(|_| {
        DomainResult::invalid(diagnostic::evidence_incomplete_set(
            "copy",
            "tempdir-unavailable",
        ))
    })?;
    let mut total_bytes = 0usize;
    let mut total_files = 0usize;
    copy_recursive(root, &destination, &mut total_bytes, &mut total_files)?;
    Ok(destination)
}

fn copy_recursive(
    source: &Path,
    destination: &Path,
    total_bytes: &mut usize,
    total_files: &mut usize,
) -> Outcome<()> {
    let entries = std::fs::read_dir(source).map_err(|_| {
        DomainResult::invalid(diagnostic::evidence_incomplete_set("copy", "unreadable"))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if COPY_EXCLUDED.contains(&name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|_| {
            DomainResult::invalid(diagnostic::evidence_incomplete_set("copy", "unreadable"))
        })?;
        let target = destination.join(name);
        if metadata.is_dir() {
            std::fs::create_dir_all(&target).map_err(|_| {
                DomainResult::invalid(diagnostic::evidence_incomplete_set(
                    "copy",
                    "tempdir-unavailable",
                ))
            })?;
            copy_recursive(&entry.path(), &target, total_bytes, total_files)?;
        } else if metadata.is_file() {
            *total_bytes += metadata.len() as usize;
            *total_files += 1;
            if *total_bytes > version::COPY_TOTAL_BYTES || *total_files > version::COPY_TOTAL_FILES
            {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "copy",
                    "over-limit",
                )));
            }
            std::fs::copy(entry.path(), &target).map_err(|_| {
                DomainResult::invalid(diagnostic::evidence_incomplete_set("copy", "unreadable"))
            })?;
        }
        // Reparse points, symlinks, and junctions are skipped on sight.
    }
    Ok(())
}

/// Apply the exact patch set to a disposable copy. Only the exact
/// bounded relative paths are touched; nothing else moves.
fn apply_patch(copy: &Path, patch: &PatchDocument) -> Outcome<()> {
    for file in &patch.files {
        let target = copy.join(&file.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|_| {
                DomainResult::invalid(diagnostic::evidence_incomplete_set("patch", "create"))
            })?;
        }
        match file.action.as_str() {
            "delete" => {
                let _ = std::fs::remove_file(&target);
            }
            _ => {
                let content = file.content.as_deref().unwrap_or_default();
                std::fs::write(&target, content).map_err(|_| {
                    DomainResult::invalid(diagnostic::evidence_incomplete_set("patch", "write"))
                })?;
            }
        }
    }
    Ok(())
}
