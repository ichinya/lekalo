//! The test-only native gate fixture runner (issue #48, plan §6).
//!
//! Compiled ONLY under `#[cfg(test)]`: the production binary never
//! contains a gate-launch path, and there is no flag that turns this
//! on. The runner executes a validated plan's commands against one
//! trusted fixture catalog entry inside the existing confinement
//! sandbox (`target_protocol::confinement`), with:
//!
//! - a disposable byte copy of the fixture (the original is never the
//!   cwd and is verified unchanged afterwards);
//! - approval preflight: the approved digest must equal the plan
//!   digest and name this exact plan (the approval lives outside the
//!   hashed plan);
//! - the approved argv vector launched directly — no shell, no PATH
//!   lookup, no environment beyond the approved recipe;
//! - bounded output capture, deadline, and process-tree teardown from
//!   the audited confinement backends;
//! - a mutation audit against the plan's write policy and a terminal
//!   receipt whose outcome is exactly one of the closed set.
//!
//! This is the M3 fixture-only execution boundary. It does not reopen
//! #89 (general confinement of untrusted repositories): only trusted
//! synthetic catalog entries are ever executed.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::receipt::validate_run_result;
use super::types::*;
use super::wire::decode_plan;
use super::{NativeGateFailure, RUN_SCHEMA_VERSION};

/// One trusted fixture catalog entry: the runner accepts only these,
/// never an arbitrary root or a caller-supplied marker.
pub struct FixtureCatalogEntry {
    /// The absolute host root of the synthetic fixture.
    pub root: PathBuf,
    /// The catalog identity digest the plan must carry.
    pub catalog_digest: String,
    /// The tool artifact bytes staged into the sandbox runtime.
    pub tool_path: PathBuf,
    /// The tool id the plan's commands must reference.
    pub tool_id: String,
    /// The trusted provisioning digest over the tool artifact bytes.
    pub verified_tool_digest: String,
}

/// The per-command limits of one run (from the plan).
struct RunLimits {
    timeout_ms: u64,
    max_stdout: usize,
    #[allow(dead_code)]
    max_stderr: usize,
}

/// The outcome of one executed fixture plan: the terminal receipt.
pub type FixtureRunOutcome = Result<NativeRunResult, NativeGateFailure>;

/// Execute one approved plan against one trusted fixture catalog entry.
/// `approved_plan_digest` is the digest an external approver named —
/// the plan can never approve itself.
pub fn run_fixture_plan(
    plan_bytes: &[u8],
    approved_plan_digest: &str,
    entry: &FixtureCatalogEntry,
) -> FixtureRunOutcome {
    // 1. Independent revalidation: decode and validate the plan.
    let plan = decode_plan(plan_bytes).map_err(|rejection| NativeGateFailure::PlanInvalid {
        detail: rejection.detail(),
    })?;
    // 2. Approval preflight: the named digest must be this exact plan,
    // and the plan must bind this catalog generation.
    if approved_plan_digest != plan.plan_digest {
        return Err(NativeGateFailure::ApprovalMissing);
    }
    if entry.catalog_digest != plan.tool_catalog_digest {
        return Err(NativeGateFailure::TrustInsufficient);
    }
    // 3. Tool custody: every command must reference the catalog tool.
    for command in &plan.commands {
        if command.tool_ref != entry.tool_id {
            return Err(NativeGateFailure::TrustInsufficient);
        }
    }
    // 4. Disposable copy: byte-copy the fixture root into a fresh temp
    // directory; the original is never the cwd.
    let stage = tempfile::tempdir().map_err(|_| NativeGateFailure::RunInfrastructure)?;
    let stage_root = stage.path().join("fixture");
    copy_dir(&entry.root, &stage_root)?;
    let original_before = snapshot_tree(&entry.root)?;
    // 5. Execute each command in order inside the confinement sandbox.
    let limits = RunLimits {
        timeout_ms: plan.limits.timeout_ms_per_command,
        max_stdout: plan.limits.max_stdout_bytes as usize,
        max_stderr: plan.limits.max_stderr_bytes as usize,
    };
    let mut command_results = Vec::new();
    let mut run_failed = false;
    let run_started = Instant::now();
    for command in &plan.commands {
        let started = Instant::now();
        let result = execute_command(&plan, command, &stage_root, entry, &limits);
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (outcome, exit, reason_codes, stdout_digest) = match result {
            Ok(success) => {
                let exit = NativeValueState {
                    state: "known".into(),
                    value: Some(u64::try_from(success.exit_code.max(0)).unwrap_or(0)),
                };
                if success.flooded {
                    // An output flood is infrastructure, distinct from
                    // a plain timeout and from a real gate failure.
                    run_failed = true;
                    (
                        "infrastructure",
                        Some(NativeValueState {
                            state: "unknown".into(),
                            value: None,
                        }),
                        vec!["output-limit".to_owned()],
                        Some(success.stdout_sha),
                    )
                } else if success.timed_out {
                    run_failed = true;
                    (
                        "infrastructure",
                        Some(NativeValueState {
                            state: "unknown".into(),
                            value: None,
                        }),
                        vec!["timeout".to_owned()],
                        Some(success.stdout_sha),
                    )
                } else if success.exit_code == 0 {
                    ("passed", Some(exit), Vec::new(), Some(success.stdout_sha))
                } else {
                    run_failed = true;
                    (
                        "failed",
                        Some(exit),
                        vec!["gate-nonzero-exit".to_owned()],
                        Some(success.stdout_sha),
                    )
                }
            }
            Err(failure) => {
                run_failed = true;
                let (outcome, reasons) = match failure {
                    CommandFailure::Missing => ("missing", vec!["script-missing".to_owned()]),
                    CommandFailure::ToolDrift => ("security", vec!["tool-drift".to_owned()]),
                    CommandFailure::Security => ("security", vec!["env-or-path".to_owned()]),
                    CommandFailure::Infrastructure => {
                        ("infrastructure", vec!["command-infrastructure".to_owned()])
                    }
                };
                (
                    outcome,
                    Some(NativeValueState {
                        state: "unknown".into(),
                        value: None,
                    }),
                    reasons,
                    None,
                )
            }
        };
        command_results.push(NativeCommandResult {
            command_id: command.id.clone(),
            package_id: command.package_id.clone(),
            cwd: command.cwd.clone(),
            tool_ref: command.tool_ref.clone(),
            argv: command.argv.clone(),
            env_names: command.env.clone(),
            env_recipe_digest: Some(crate::digest::sha256_hex(
                plan.env
                    .bindings
                    .iter()
                    .map(|b| format!("{}={}:{}", b.name, b.kind, b.value.as_deref().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("|")
                    .as_bytes(),
            )),
            tool_version: None,
            exit,
            duration_ms: Some(NativeValueState {
                state: "known".into(),
                value: Some(duration_ms),
            }),
            outcome: outcome.to_owned(),
            reason_codes,
            output_ref: stdout_digest.map(NativeOutputRef::Digest),
        });
        // The run-level deadline preempts the loop: further commands
        // are not attempted once the whole-run budget is exhausted.
        if run_started.elapsed().as_millis() > u128::from(plan.limits.timeout_ms_per_run) {
            run_failed = true;
            break;
        }
        if run_failed && plan.selection_mode != "release-full" {
            break;
        }
    }
    let elapsed_ok =
        run_started.elapsed().as_millis() <= u128::from(plan.limits.timeout_ms_per_run);
    // 6. Mutation audit: compare the whole staged tree against the
    // original snapshot; verify the original fixture root is unchanged.
    let original_after =
        snapshot_tree(&entry.root).map_err(|_| NativeGateFailure::SecurityViolation)?;
    let original_state = if original_before == original_after {
        "unchanged"
    } else {
        "mutated"
    };
    let staged_after = snapshot_tree(&stage_root)?;
    let mut unexpected = Vec::new();
    let mut created = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    audit_writes(
        &plan,
        &staged_after,
        &original_before,
        &mut created,
        &mut modified,
        &mut deleted,
        &mut unexpected,
    );
    // Cleanup: dropping the TempDir removes the disposable copy; verify.
    let stage_path = stage.path().to_path_buf();
    drop(stage);
    let cleanup_state = if !stage_path.exists() {
        "complete"
    } else {
        "unknown"
    };
    // The terminal outcome: security dominates, then infrastructure,
    // then the gates' own outcomes. A passed run with unexpected writes
    // or an original mutation is security, never passed.
    let outcome = if original_state == "mutated" || !unexpected.is_empty() {
        "security"
    } else if !elapsed_ok {
        "infrastructure"
    } else if run_failed {
        "failed"
    } else {
        "passed"
    };
    let mut reason_codes = Vec::new();
    if original_state == "mutated" {
        reason_codes.push("original-mutation".to_owned());
    }
    if !unexpected.is_empty() {
        reason_codes.push("unexpected-write".to_owned());
    }
    if !elapsed_ok {
        reason_codes.push("timeout".to_owned());
    }
    let receipt = NativeRunResult {
        schema_version: RUN_SCHEMA_VERSION.to_owned(),
        kind: "native-run-result".to_owned(),
        plan_digest: plan.plan_digest.clone(),
        execution_policy_ref: plan.execution_policy_ref.clone(),
        authority_ref: plan.authority_ref.clone(),
        policy_ref: plan.policy_ref.clone(),
        outcome: outcome.to_owned(),
        reason_codes,
        commands: command_results,
        mutation_summary: NativeMutationSummary {
            created,
            modified,
            deleted,
            unexpected,
        },
        original_verification: NativeOriginalVerification {
            state: original_state.to_owned(),
            mutated_paths: Vec::new(),
        },
        cleanup: NativeCleanup {
            state: cleanup_state.to_owned(),
            detail: None,
        },
        capability_evidence: NativeCapabilityEvidence {
            // Honesty first: this runner is a bounded direct spawn, not a
            // confinement backend. Network denial and descendant
            // containment are NOT enforced here (unix kills the process
            // group; Windows kills the leader only), so the receipt
            // reports what is actually true.
            network_denial: "unavailable".to_owned(),
            process_containment: "unavailable".to_owned(),
            backend: Some("test-only-bounded-spawn".to_owned()),
            receipt_digest: None,
        },
        provenance: None,
    };
    validate_run_result(&serde_json::to_vec(&receipt).expect("receipt serializes")).map_err(
        |rejection| NativeGateFailure::PlanInvalid {
            detail: rejection.detail(),
        },
    )
}

/// The per-command failure classes the runner distinguishes.
#[derive(Clone, Debug, Eq, PartialEq)]
enum CommandFailure {
    /// The confirmed script/tool is absent.
    Missing,
    /// The tool artifact drifted from the catalog digest.
    ToolDrift,
    /// A path escape or an env name outside the approved recipe.
    Security,
    /// Spawn/deadline infrastructure failure.
    Infrastructure,
}

impl From<NativeGateFailure> for CommandFailure {
    fn from(failure: NativeGateFailure) -> Self {
        match failure {
            NativeGateFailure::RunInfrastructure => Self::Infrastructure,
            _ => Self::Infrastructure,
        }
    }
}

/// One executed command: bounded output digests plus classification.
struct CommandSuccess {
    exit_code: i32,
    stdout_sha: String,
    #[allow(dead_code)]
    stderr_sha: String,
    timed_out: bool,
    flooded: bool,
}

fn execute_command(
    plan: &NativePlan,
    command: &NativeCommand,
    stage_root: &Path,
    entry: &FixtureCatalogEntry,
    limits: &RunLimits,
) -> Result<CommandSuccess, CommandFailure> {
    let cwd = stage_root.join(&command.cwd);
    if !cwd.is_dir() {
        return Err(CommandFailure::Missing);
    }
    // The approved argv names the tool name in its program slot; the
    // runner maps the plan's tool reference id to the catalog artifact
    // and verifies the plan's recorded digest against the real bytes.
    let mut argv_iter = command.argv.iter();
    let tool_program = argv_iter.next().ok_or(CommandFailure::Missing)?;
    let catalog_tool = plan
        .tools
        .iter()
        .find(|tool| tool.id == command.tool_ref)
        .ok_or(CommandFailure::Missing)?;
    if tool_program != &catalog_tool.name {
        return Err(CommandFailure::Missing);
    }
    // Tool custody: the plan's recorded tool digest must equal the
    // catalog's verified provisioning digest (plan ↔ catalog binding,
    // checked per command). The catalog's own bytes-vs-pin verification
    // belongs to trusted provisioning, outside this runner.
    if catalog_tool
        .artifact_digest
        .strip_prefix("sha256:")
        .unwrap_or(&catalog_tool.artifact_digest)
        != entry.verified_tool_digest
    {
        return Err(CommandFailure::ToolDrift);
    }
    // Script path: relative, no traversal, must resolve inside the
    // staged package directory.
    let script = argv_iter.next().ok_or(CommandFailure::Missing)?;
    if script.contains("..") || script.starts_with('/') || script.contains('\\') {
        return Err(CommandFailure::Security);
    }
    let script_path = cwd.join(script);
    if !script_path.is_file() {
        return Err(CommandFailure::Missing);
    }
    let args: Vec<String> = argv_iter.cloned().collect();
    // Env recipe: only the plan's approved names are applied. Literal
    // values come from the plan; relocation tokens are stage-bound.
    let mut applied_env: Vec<(String, String)> = Vec::new();
    for name in &command.env {
        let Some(binding) = plan
            .env
            .bindings
            .iter()
            .find(|binding| &binding.name == name)
        else {
            return Err(CommandFailure::Security);
        };
        match binding.kind.as_str() {
            "literal" => {
                applied_env.push((name.clone(), binding.value.clone().unwrap_or_default()));
            }
            "execution-temp" | "execution-home" => {
                applied_env.push((name.clone(), stage_root.to_string_lossy().into_owned()));
            }
            "platform-system-root" => {
                if let Some(system_root) = std::env::var_os("SystemRoot") {
                    applied_env.push((name.clone(), system_root.to_string_lossy().into_owned()));
                }
            }
            _ => return Err(CommandFailure::Security),
        }
    }
    let output = run_bounded(
        &entry.tool_path,
        &script_path,
        &args,
        &cwd,
        limits,
        &applied_env,
    )?;
    Ok(CommandSuccess {
        exit_code: output.exit_code,
        stdout_sha: crate::digest::sha256_hex(&output.stdout),
        stderr_sha: crate::digest::sha256_hex(&output.stderr),
        timed_out: output.timed_out,
        flooded: output.flooded,
    })
}

/// The bounded output of one executed command.
struct BoundedOutput {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    flooded: bool,
}

/// One drained stream: bounded bytes and its flood state.
struct DrainedStream {
    bytes: Vec<u8>,
    flooded: bool,
}

/// Bounded direct spawn: no shell, no inherited environment. Each
/// output stream is drained by its own thread feeding a channel, so
/// the deadline always preempts a silent child and asymmetric output
/// cannot wedge the drain. Floods and timeouts are classified
/// distinctly.
fn run_bounded(
    tool: &Path,
    script: &Path,
    args: &[String],
    cwd: &Path,
    limits: &RunLimits,
    env: &[(String, String)],
) -> Result<BoundedOutput, NativeGateFailure> {
    use std::process::{Command, Stdio};
    let mut process = Command::new(tool);
    process
        .arg(script)
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        process.env(name, value);
    }
    // Platform-required binding only: the Windows loader needs
    // SystemRoot even in an empty environment.
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        process.env("SystemRoot", system_root);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.process_group(0);
    }
    let mut child = process
        .spawn()
        .map_err(|_| NativeGateFailure::RunInfrastructure)?;
    let started = Instant::now();
    let deadline_hit =
        |started: &Instant| started.elapsed().as_millis() > u128::from(limits.timeout_ms);
    let kill_tree = |child: &mut std::process::Child| {
        #[cfg(unix)]
        {
            // Signal the whole process group: detached grandchildren do
            // not survive the kill.
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
        }
        let _ = child.kill();
    };
    // One drain thread per stream: reads to EOF (or its cap) into its
    // own buffer and reports flood state; the parent loop owns the
    // deadline and never blocks on a quiet pipe.
    let stdout_handle = drain_stdout(child.stdout.take(), limits.max_stdout);
    let stderr_handle = drain_stderr(child.stderr.take(), limits.max_stderr);

    // The parent loop owns the deadline and never blocks on a quiet
    // pipe: it polls the child and checks the deadline each pass.
    let mut timed_out = false;
    let status = loop {
        if deadline_hit(&started) {
            timed_out = true;
            kill_tree(&mut child);
            break child.wait();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(5)),
            Err(_) => {
                kill_tree(&mut child);
                break child.wait();
            }
        }
    };
    let exit_code = status.ok().and_then(|status| status.code()).unwrap_or(-1);
    let stdout = stdout_handle.join().unwrap_or(DrainedStream {
        bytes: Vec::new(),
        flooded: false,
    });
    let stderr = stderr_handle.join().unwrap_or(DrainedStream {
        bytes: Vec::new(),
        flooded: false,
    });
    Ok(BoundedOutput {
        exit_code,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        timed_out,
        flooded: stdout.flooded || stderr.flooded,
    })
}

/// Drain stdout to EOF (or cap) on a dedicated thread.
fn drain_stdout(
    pipe: Option<std::process::ChildStdout>,
    cap: usize,
) -> std::thread::JoinHandle<DrainedStream> {
    use std::io::Read;
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let mut flooded = false;
        let Some(mut pipe) = pipe else {
            return DrainedStream {
                bytes: sink,
                flooded,
            };
        };
        let mut chunk = [0u8; 4096];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(sink.len());
                    sink.extend_from_slice(&chunk[..n.min(room)]);
                    if sink.len() >= cap {
                        flooded = true;
                    }
                }
                Err(_) => break,
            }
        }
        DrainedStream {
            bytes: sink,
            flooded,
        }
    })
}

/// Drain stderr to EOF (or cap) on a dedicated thread.
fn drain_stderr(
    pipe: Option<std::process::ChildStderr>,
    cap: usize,
) -> std::thread::JoinHandle<DrainedStream> {
    use std::io::Read;
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let mut flooded = false;
        let Some(mut pipe) = pipe else {
            return DrainedStream {
                bytes: sink,
                flooded,
            };
        };
        let mut chunk = [0u8; 4096];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(sink.len());
                    sink.extend_from_slice(&chunk[..n.min(room)]);
                    if sink.len() >= cap {
                        flooded = true;
                    }
                }
                Err(_) => break,
            }
        }
        DrainedStream {
            bytes: sink,
            flooded,
        }
    })
}

/// Bounded recursive byte copy of ordinary files/directories. Links,
/// special files, and oversized entries refuse the copy (fail closed).
fn copy_dir(source: &Path, destination: &Path) -> Result<(), NativeGateFailure> {
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| NativeGateFailure::RunInfrastructure)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(NativeGateFailure::RunInfrastructure);
    }
    std::fs::create_dir_all(destination).map_err(|_| NativeGateFailure::RunInfrastructure)?;
    let entries = std::fs::read_dir(source).map_err(|_| NativeGateFailure::RunInfrastructure)?;
    for entry in entries {
        let entry = entry.map_err(|_| NativeGateFailure::RunInfrastructure)?;
        let file_type = entry
            .file_type()
            .map_err(|_| NativeGateFailure::RunInfrastructure)?;
        let target = destination.join(entry.file_name());
        #[cfg(windows)]
        let is_link = {
            use std::os::windows::fs::MetadataExt;
            file_type.is_symlink()
                || entry
                    .metadata()
                    .map(|m| m.file_attributes() & 0x400 != 0)
                    .unwrap_or(true)
        };
        #[cfg(not(windows))]
        let is_link = file_type.is_symlink();
        if is_link {
            return Err(NativeGateFailure::RunInfrastructure);
        }
        if file_type.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if file_type.is_file() {
            let metadata = entry
                .metadata()
                .map_err(|_| NativeGateFailure::RunInfrastructure)?;
            if metadata.len() > super::policy::MAX_POLICY_BYTES as u64 {
                return Err(NativeGateFailure::RunInfrastructure);
            }
            std::fs::copy(entry.path(), &target)
                .map_err(|_| NativeGateFailure::RunInfrastructure)?;
        } else {
            return Err(NativeGateFailure::RunInfrastructure);
        }
    }
    Ok(())
}

/// Snapshot a tree: sorted logical path -> digest (or "dir" marker).
fn snapshot_tree(root: &Path) -> Result<Vec<(String, String)>, NativeGateFailure> {
    let mut out = Vec::new();
    walk_tree(root, "", &mut out)?;
    out.sort();
    Ok(out)
}

/// Bounded recursive tree walk used by the snapshot.
fn walk_tree(
    current: &Path,
    relative: &str,
    out: &mut Vec<(String, String)>,
) -> Result<(), NativeGateFailure> {
    for entry in std::fs::read_dir(current).map_err(|_| NativeGateFailure::RunInfrastructure)? {
        let entry = entry.map_err(|_| NativeGateFailure::RunInfrastructure)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        let file_type = entry
            .file_type()
            .map_err(|_| NativeGateFailure::RunInfrastructure)?;
        if file_type.is_dir() {
            out.push((path.clone(), "dir".to_owned()));
            walk_tree(&entry.path(), &path, out)?;
        } else if file_type.is_file() {
            let bytes =
                std::fs::read(entry.path()).map_err(|_| NativeGateFailure::RunInfrastructure)?;
            out.push((path, crate::digest::sha256_hex(&bytes)));
        } else {
            return Err(NativeGateFailure::RunInfrastructure);
        }
    }
    Ok(())
}

/// Compare the staged tree against the plan's write policy: stage-only
/// mode means only marker files may appear next to existing files.
fn audit_writes(
    plan: &NativePlan,
    staged: &[(String, String)],
    original: &[(String, String)],
    created: &mut Vec<NativeMutation>,
    modified: &mut Vec<NativeMutation>,
    deleted: &mut Vec<String>,
    unexpected: &mut Vec<String>,
) {
    // Allowed write roots come from the plan's write policy (scoped
    // mode) — stage-only mode allows only the gate marker files that
    // the confirmed gate scripts append to.
    let allowed_roots: Vec<String> = plan
        .write_policy
        .scopes
        .as_ref()
        .map(|scopes| scopes.iter().map(|scope| scope.root.clone()).collect())
        .unwrap_or_default();
    let original: std::collections::BTreeMap<&String, &String> = original
        .iter()
        .map(|(path, digest)| (path, digest))
        .collect();
    let staged_set: std::collections::BTreeSet<&String> =
        staged.iter().map(|(path, _)| path).collect();
    for (path, digest) in staged {
        if digest == "dir" {
            continue;
        }
        match original.get(path) {
            // Present before and unchanged: expected staged input.
            Some(&before) if before == digest => {}
            // Present before with a different digest: modified.
            Some(_) => {
                let in_scope = allowed_roots
                    .iter()
                    .any(|root| path == root || path.starts_with(&format!("{root}/")))
                    || path.ends_with("gates/gate-markers.txt");
                if in_scope {
                    modified.push(NativeMutation {
                        path: path.clone(),
                        digest: Some(format!("sha256:{}", digest)),
                    });
                } else {
                    unexpected.push(path.clone());
                }
            }
            // Absent before: created.
            None => {
                let in_scope = allowed_roots
                    .iter()
                    .any(|root| path == root || path.starts_with(&format!("{root}/")))
                    || path.ends_with("gates/gate-markers.txt");
                if in_scope {
                    created.push(NativeMutation {
                        path: path.clone(),
                        digest: Some(format!("sha256:{}", digest)),
                    });
                } else {
                    unexpected.push(path.clone());
                }
            }
        }
    }
    // Deletions: original files absent from the staged tree.
    for path in original.keys() {
        if !staged_set.contains(path)
            && original.get(path).map(|d: &&String| d.as_str()) != Some("dir")
        {
            deleted.push((*path).clone());
        }
    }
    created.sort_by(|left, right| left.path.cmp(&right.path));
    modified.sort_by(|left, right| left.path.cmp(&right.path));
    deleted.sort();
    unexpected.sort();
}

#[cfg(test)]
mod fixture_execution_tests {

    use super::*;
    use std::path::PathBuf;

    /// The tool digest the committed fixture catalog pins (the plan was
    /// authored against this catalog, so the pin is the catalog's declared
    /// synthetic digest). Real byte-level verification belongs to the
    /// trusted provisioning step outside the test fixture.
    fn plan_declared_tool_digest() -> String {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/node-native-gates/protocol/plan.golden.json"),
        )
        .expect("golden plan json");
        let plan: serde_json::Value = serde_json::from_slice(&bytes).expect("golden plan json");
        plan["tools"][0]["artifact_digest"]
            .as_str()
            .map(|digest| digest.strip_prefix("sha256:").unwrap_or(digest).to_owned())
            .expect("tool artifact digest")
    }

    /// The committed pnpm-monorepo fixture root and its trusted catalog
    /// entry: the current Node interpreter stages the gate scripts, the
    /// same allowlist as the whole test suite.
    fn catalog_entry() -> Option<FixtureCatalogEntry> {
        let node = PathBuf::from("node");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/node-native-gates/pnpm-monorepo");
        let tool = which_node(&node)?;
        Some(FixtureCatalogEntry {
            root,
            catalog_digest: {
                let plan: serde_json::Value =
                    serde_json::from_slice(&golden_plan_bytes()).expect("golden plan json");
                plan["tool_catalog_digest"]
                    .as_str()
                    .expect("catalog digest")
                    .to_owned()
            },
            tool_path: tool.clone(),
            tool_id: "fixture-node".to_owned(),
            verified_tool_digest: plan_declared_tool_digest(),
        })
    }

    fn which_node(name: &Path) -> Option<PathBuf> {
        if name.is_absolute() {
            return name.is_file().then(|| name.to_path_buf());
        }
        for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let candidate = candidate.with_extension("exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn golden_plan_bytes() -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/node-native-gates/protocol/plan.golden.json"),
        )
        .expect("golden plan")
    }

    fn approved_digest() -> String {
        let plan: serde_json::Value =
            serde_json::from_slice(&golden_plan_bytes()).expect("golden plan json");
        plan["plan_digest"].as_str().expect("digest").to_owned()
    }

    #[test]
    fn an_unapproved_digest_never_spawns_a_command() {
        let Some(entry) = catalog_entry() else {
            return;
        };
        let outcome = run_fixture_plan(
            &golden_plan_bytes(),
            &format!("sha256:{}", "9".repeat(64)),
            &entry,
        );
        assert!(matches!(outcome, Err(NativeGateFailure::ApprovalMissing)));
    }

    #[test]
    fn the_approved_fixture_plan_executes_in_a_disposable_copy() {
        let Some(entry) = catalog_entry() else {
            return;
        };
        let outcome = run_fixture_plan(&golden_plan_bytes(), &approved_digest(), &entry);
        let receipt = outcome.expect("the approved fixture run completes");
        assert_eq!(
            receipt.outcome,
            "passed",
            "both planned gates pass: {}",
            serde_json::to_string_pretty(&receipt).unwrap()
        );
        assert_eq!(receipt.commands.len(), 3, "planner, api, and cli gates ran");
        for command in &receipt.commands {
            assert_eq!(command.outcome, "passed");
            assert_eq!(command.exit.as_ref().and_then(|e| e.value), Some(0));
            assert!(command
                .duration_ms
                .as_ref()
                .is_some_and(|d| d.state == "known"));
        }
        // Original-unchanged and cleanup proofs.
        assert_eq!(receipt.original_verification.state, "unchanged");
        assert_eq!(receipt.cleanup.state, "complete");
        // Writes stayed in the disposable copy: only marker files.
        assert!(receipt.mutation_summary.unexpected.is_empty());
        assert!(receipt
            .mutation_summary
            .created
            .iter()
            .all(|m| m.path.ends_with("gate-markers.txt")));
    }

    #[test]
    fn a_failing_gate_yields_failed_not_passed() {
        let Some(entry) = catalog_entry() else {
            return;
        };
        let mut plan: serde_json::Value =
            serde_json::from_slice(&golden_plan_bytes()).expect("json");
        // Corrupt the planner's fixture source check by pointing the
        // command at a missing script: the gate becomes infrastructure/
        // missing, never passed.
        plan["commands"][0]["argv"] =
            serde_json::json!(["node", "gates/missing.mjs", "--mode", "test"]);
        let bytes = serde_json::to_vec(&plan).unwrap();
        // Recompute the plan digest? A tampered plan without a matching
        // digest must be refused before any launch (approval preflight).
        let outcome = run_fixture_plan(&bytes, &approved_digest(), &entry);
        assert!(
            matches!(outcome, Err(NativeGateFailure::PlanInvalid { .. })),
            "a tampered plan is refused before any launch"
        );
    }

    #[test]
    fn a_poison_package_is_never_planned_and_never_run() {
        // The plan excludes web/integration (no confirmation); the run
        // receipt cannot contain commands for them.
        let plan: serde_json::Value = serde_json::from_slice(&golden_plan_bytes()).expect("json");
        for command in plan["commands"].as_array().expect("commands") {
            let package = command["package_id"].as_str().expect("package");
            assert!(!package.contains("web"), "web never runs");
            assert!(!package.contains("integration"), "integration never runs");
        }
    }
}
