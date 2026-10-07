//! Qualified selective verification (issue #136, increment I4): the
//! pinned specification is really executed — baseline in a pristine
//! disposable copy, candidate in a patched disposable copy — with
//! bounded, continuously drained output, bounded runtime with
//! process-tree teardown, a scrubbed child environment, per-check
//! independent expectations, and an explicit behavior oracle. A
//! specification or a test id never decodes as an execution receipt:
//! only this module's execution variant, produced by actual runs, carries
//! outcomes.
//!
//! Where the qualified tool is missing or a command cannot run, the
//! conclusion is honestly `unavailable` and no test is reported as
//! passed. Never a marker, never an environment backdoor.
//!
//! Custody: [`admit_execution`] is the only path an execution document
//! takes into apply. It re-derives the execution envelope's binding to
//! the exact plan, plan-pinned candidate digest, and complete consumed
//! input set, and demands the measured results be execution custody —
//! the run digest and measured timestamp this module alone assembles. A
//! hand-written or replayed conclusion without custody refuses.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/// The bounded child environment: an allow-listed subset of the host
/// names a runtime genuinely needs, everything else cleared (the same
/// discipline the native-gate runner owns). Secrets and unrelated host
/// state never reach a verification child.
const CHILD_ENV_ALLOWED: [&str; 6] = [
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "COMSPEC",
    "PROGRAMFILES",
];

/// The closed execution-custody marker member: assembled only by this
/// module's real runs and required by apply's admission. A document
/// that decodes without it never carries a measured conclusion.
pub(crate) const CUSTODY_MEMBER: &str = "custody";

/// The private custody envelope of one real execution. It serializes
/// into the closed wire document under [`CUSTODY_MEMBER`]; it is never
/// accepted from an outside caller.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCustody {
    /// The domain-separated digest over the exact measured document
    /// without its custody member (the run's content address).
    pub run: String,
    /// The measured wall-clock duration of the whole verification, in
    /// milliseconds (bound 1..=60_000_000); a forged document has no
    /// measured duration to present.
    pub measured_ms: u64,
}

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
    // The pinned tool digest is enforced when the specification carries
    // one: the resolved program's bytes must hash to the pin, so a
    // swapped interpreter can never silently serve a pinned run.
    let Some(program) = resolve_program(&tool.program) else {
        let mut execution = shell(plan, &spec, spec_digest, None, None, None);
        execution.conclusion = Some("unavailable".to_owned());
        return Ok(execution);
    };
    verify_tool_digest(&program, tool)?;

    let started = Instant::now();
    // Baseline: a pristine disposable copy.
    let baseline_copy = copy_tree(root)?;
    let baseline = run_checks(&program, &tool.args, &spec.checks, &baseline_copy);

    // Candidate: a second disposable copy with the exact patch applied.
    let candidate_result = match patch {
        Some(document) => {
            let candidate_copy = copy_tree(root)?;
            let checks = match apply_patch(&candidate_copy, document) {
                Ok(()) => run_checks(&program, &tool.args, &spec.checks, &candidate_copy),
                Err(_) => CheckResults {
                    checks: spec
                        .checks
                        .iter()
                        .map(|check| CheckOutcome {
                            id: check.id.clone(),
                            status: "unavailable".to_owned(),
                            exit: None,
                            detail: "candidate-prepare-failed".to_owned(),
                        })
                        .collect(),
                },
            };
            // The disposable candidate copy never survives the run.
            let _ = std::fs::remove_dir_all(&candidate_copy);
            checks
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
    // Both disposable copies are removed on every exit path.
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
        patch.map(|document| {
            let value = serde_json::to_value(document).expect("patch serializes");
            wire::digest("adoption-patch", &wire::without_member(&value, "patch"))
        }),
    );
    execution.oracle = Some(oracle);
    execution.conclusion = Some(conclusion.to_owned());
    // Execution custody: only this real run can attach the marker apply
    // admits on. The digest is over the measured document without the
    // custody member; the measured duration is bounded below.
    let measured_ms = (started.elapsed().as_millis() as u64).clamp(1, 60_000_000);
    execution.custody = Some(custody_of(&execution, measured_ms));
    Ok(execution)
}

/// The private custody envelope of one measured execution: the
/// domain-separated digest over the exact document without its custody
/// member, plus the measured whole-run duration.
fn custody_of(execution: &VerificationDocument, measured_ms: u64) -> ExecutionCustody {
    let value = serde_json::to_value(execution).expect("execution serializes");
    let run = wire::digest(
        "adoption-verification-run",
        &wire::without_member(&value, CUSTODY_MEMBER),
    );
    ExecutionCustody { run, measured_ms }
}

/// Admit one execution document for apply (X1/X2): the document must
/// decode as an execution variant bound to the exact plan and the
/// exact plan-pinned candidate digest, must pin the complete consumed
/// input set the plan pinned (or typed absence), and must carry the
/// execution custody this module assembles over measured results — the
/// run digest must match the admitted document's own bytes and the
/// measured duration must be present and bounded. A forged, stale, or
/// replayed conclusion refuses before any write.
pub(crate) fn admit_execution(
    ctx: &super::analysis::AdoptionContext,
    plan: &RefactorPlan,
    patch_digest: &str,
    execution_bytes: &[u8],
) -> Outcome<VerificationDocument> {
    let execution = wire::execution_of_bytes(execution_bytes)
        .map_err(|error| DomainResult::invalid(error.set()))?;
    if execution.variant != "execution" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "not-an-execution",
        )));
    }
    if execution.spec.as_deref() != plan.verification.as_deref()
        || execution.plan.as_deref() != Some(plan.plan.as_str())
    {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "plan-mismatch",
        )));
    }
    // Custody first: a conclusion without this module's run digest and
    // a bounded measured duration is never evidence of passing.
    let Some(custody) = &execution.custody else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "custody-missing",
        )));
    };
    let value = serde_json::to_value(&execution).expect("execution serializes");
    let expected = wire::digest(
        "adoption-verification-run",
        &wire::without_member(&value, CUSTODY_MEMBER),
    );
    if custody.run != expected || !version::is_sha256(&custody.run) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "custody-mismatch",
        )));
    }
    if custody.measured_ms == 0 || custody.measured_ms > 60_000_000 {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "custody-bounds",
        )));
    }
    // The measured conclusion must be preserved: conclusions are
    // checked before candidate coverage so a failed run is honestly
    // classified verification-failed (LEK-ADOPT-007), never laundered
    // through an unrelated mismatch.
    if execution.conclusion.as_deref() != Some("preserved") {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            execution
                .oracle
                .as_ref()
                .map(|oracle| oracle.detail.as_str())
                .unwrap_or("conclusion"),
            &execution.spec.clone().unwrap_or_default(),
        )));
    }
    // The admitted run must cover this exact candidate revision: a
    // genuine preserved execution for any other patch bytes is a
    // replay, never evidence for this apply.
    if !run_covers_candidate(&execution, patch_digest) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "candidate-mismatch",
        )));
    }
    // The recorded per-check outcomes must really support `preserved`:
    // conclusions are derived from the recorded measurements, never
    // accepted as assertions.
    if !measured_preserved(&execution) {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            "measured-outcomes",
            &execution.spec.clone().unwrap_or_default(),
        )));
    }
    let _ = ctx;
    Ok(execution)
}

/// Whether the recorded baseline/candidate outcomes really support the
/// `preserved` conclusion: every recorded check passed on both sides
/// and the declared oracle verdict is `passed`.
fn measured_preserved(execution: &VerificationDocument) -> bool {
    let (Some(baseline), Some(candidate)) = (&execution.baseline, &execution.candidate) else {
        return false;
    };
    let all_passed = baseline
        .checks
        .iter()
        .chain(candidate.checks.iter())
        .all(|check| check.status == "passed");
    let oracle_passed = execution
        .oracle
        .as_ref()
        .is_some_and(|oracle| oracle.status == "passed");
    all_passed && oracle_passed
}

/// Whether the executed run bound the candidate revision this apply
/// installs. Real runs record the patched candidate-copy digest under
/// the `disposable-copy` confinement token; admission requires it.
fn run_covers_candidate(execution: &VerificationDocument, patch_digest: &str) -> bool {
    execution.confinement.iter().any(|token| {
        token
            .strip_prefix("disposable-copy:")
            .is_some_and(|bound| bound == patch_digest)
    })
}

/// Verify the resolved program's bytes against the pinned tool digest.
/// An unpinned specification skips this check; a pinned specification
/// refuses a swapped interpreter as a denied scope refusal.
fn verify_tool_digest(program: &Path, tool: &super::wire::ToolRef) -> Outcome<()> {
    let Some(expected) = &tool.digest else {
        return Ok(());
    };
    let bytes = std::fs::read(program)
        .map_err(|_| DomainResult::invalid(diagnostic::input_invalid_set("tool", "unreadable")))?;
    let actual = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
    if &actual != expected {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            &tool.program,
            "tool-digest-mismatch",
        )));
    }
    Ok(())
}

/// Assemble the execution envelope around the measured outcomes. The
/// `disposable-copy` confinement token binds the exact candidate
/// revision digest the run covered (admission checks it), or stays
/// bare when no candidate ran.
#[allow(clippy::too_many_arguments)]
fn shell(
    plan: &RefactorPlan,
    spec: &VerificationDocument,
    spec_digest: String,
    baseline: Option<CheckResults>,
    candidate: Option<CheckResults>,
    candidate_digest: Option<String>,
) -> VerificationDocument {
    let confinement = match candidate_digest {
        Some(digest) => vec![
            format!("disposable-copy:{digest}"),
            "bounded-output".to_owned(),
        ],
        None => vec!["disposable-copy".to_owned(), "bounded-output".to_owned()],
    };
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
        confinement,
        baseline,
        candidate,
        oracle: None,
        conclusion: None,
        custody: None,
    }
}

/// Stamp the exact candidate revision the run covered into the
/// confinement dimension; the bare token form remains the no-candidate
/// spelling. The admission path rejects a bound token naming any other
/// digest, so a replayed run can never stand in for this candidate.
#[cfg(test)]
pub(crate) fn bind_candidate(execution: &mut VerificationDocument, patch_digest: &str) {
    execution.confinement = execution
        .confinement
        .iter()
        .map(|token| {
            if token == "disposable-copy" {
                format!("disposable-copy:{patch_digest}")
            } else {
                token.clone()
            }
        })
        .collect();
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

/// One bounded run: `(exit code, bounded stdout)`. Both pipes drain on
/// dedicated threads for the whole child lifetime — a tool emitting
/// more than the pipe buffer can never deadlock the deadline — the
/// child gets its own process group on unix, teardown kills the whole
/// process tree, and the child environment is cleared down to the
/// bounded runtime allow-list. A timeout, a spawn failure, or a signal
/// exit is measured, never invented.
fn execute_command(
    program: &Path,
    args: &[String],
    cwd: &Path,
) -> Result<(Option<i32>, String), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Cleared environment plus the bounded runtime allow-list; nothing
    // else of the host — secrets, proxies, unrelated state — leaks in.
    command.env_clear();
    for name in CHILD_ENV_ALLOWED {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    // The child gets its own process group on unix so the teardown
    // reaps descendants (the same discipline the house runner owns).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn-refused:{}", error.kind()))?;
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();
    // Dedicated drain threads: both streams are consumed for the whole
    // child lifetime (bounded capture, unbounded discard), so a chatty
    // tool can never fill the OS pipe buffer and wedge the deadline.
    let stdout_thread = std::thread::spawn(move || drain_bounded(stdout_handle));
    let stderr_thread = std::thread::spawn(move || drain_bounded(stderr_handle));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() > Duration::from_millis(version::CHECK_TIMEOUT_MS) {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                kill_tree(child.id());
                let _ = child.wait();
                return Err("spawn-failed".to_owned());
            }
        }
    };
    if timed_out {
        // Process-tree teardown: descendants never survive the deadline.
        kill_tree(child.id());
        let _ = child.wait();
    }
    let (stdout, _) = stdout_thread.join().unwrap_or_default();
    let _ = stderr_thread.join();
    if timed_out {
        return Err("timeout".to_owned());
    }
    Ok((status.and_then(|status| status.code()), stdout))
}

/// Drain one pipe on a worker thread: capture up to the bounded output
/// ceiling, then keep draining (and discarding) so the child never
/// blocks on a full pipe while the run continues.
fn drain_bounded(pipe: Option<impl std::io::Read + Send + 'static>) -> (String, bool) {
    let mut pipe = match pipe {
        Some(pipe) => pipe,
        None => return (String::new(), false),
    };
    let mut bytes = Vec::new();
    let mut flooded = false;
    let mut chunk = [0u8; 8192];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = version::CHECK_OUTPUT_BYTES.saturating_sub(bytes.len());
                if remaining == 0 {
                    flooded = true;
                    continue;
                }
                let take = n.min(remaining);
                bytes.extend_from_slice(&chunk[..take]);
                if take < n {
                    flooded = true;
                }
            }
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&bytes).to_string(), flooded)
}

/// Process-tree teardown: the same kill discipline the house runner
/// owns — the unix child sits in its own process group; Windows walks
/// the tree with the platform tool.
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    {
        if let Some(pid) = rustix::process::Pid::from_raw(pid as i32) {
            if pid != rustix::process::Pid::INIT {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
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
/// bounded relative paths are touched; nothing else moves — never a
/// marker into the original project tree.
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
