//! The qualified host runner for confirmed native gate plans (issue
//! #61, plan slice C; research doc §3). `run_confirmed_plan` executes
//! one validated, externally approved plan's commands inside the
//! existing confinement discipline:
//!
//! - direct argv launches — no shell, no PATH lookup, no `.bat`/`.cmd`
//!   shim: the executable resolves through the trusted catalog the
//!   caller pins (absolute paths only, digests rechecked);
//! - a disposable staged copy of the source snapshot with bounded
//!   file/byte budgets (the vendor-tree-sized limits are reviewed
//!   constants, not the 64 MiB adapter-session cap);
//! - a cleared child environment with exactly the approved recipe
//!   grants (literal values and stage-relative temp/home relocations
//!   only — secrets resolve in the child env, never in argv);
//! - per-command and whole-run deadlines, cancellation, bounded output
//!   pipes, and process-tree teardown;
//! - a mutation audit against the plan's write policy, an original-
//!   snapshot verification, and a terminal receipt whose verdict is
//!   the documented rollup.
//!
//! **Production refusals remain.** The production `native run` seam
//! (`production_run`) still ends before any launch: this runner is
//! reachable only through callers that present qualified runtime
//! capability evidence (network denial and process containment both
//! "enforced"). Unqualified callers — and therefore every currently
//! shipped production path — receive the typed capability refusal,
//! never a best-effort spawn.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::evidence::{
    command_result, reasons, redact_stream, CommandMeasurement, FailureClass, OutputCaps,
    RedactedOutput,
};
use super::receipt::validate_run_result;
use super::types::{
    NativeCapabilityEvidence, NativeCleanup, NativeCommandResult, NativeCoverage,
    NativeMutationSummary, NativeOriginalVerification, NativeRunResult,
};
use super::{NativeGateFailure, RUN_SCHEMA_VERSION};

/// The reviewed staging budgets of the runtime bundle (issue #61):
/// Laravel vendor trees plus PHP runtime siblings exceed the adapter
/// session's 64 MiB copy cap, so the runner carries its own reviewed
/// constants. A snapshot that exceeds either budget is refused before
/// any spawn (zero-spawn on unstageable inputs).
pub const STAGE_MAX_FILES: usize = 65_536;
pub const STAGE_MAX_BYTES: usize = 2 * 1024 * 1024 * 1024;
/// One file larger than this never stages (a build artifact, not an input).
pub const STAGE_MAX_FILE_BYTES: usize = 256 * 1024 * 1024;

/// One trusted catalog entry: the absolute executable path the runner
/// launches directly, plus the verified provisioning digest the plan's
/// tool entry must carry.
#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub program: PathBuf,
    pub artifact_digest: String,
    pub version: Option<String>,
}

/// The trusted tool catalog: id -> entry. Never ambient PATH.
#[derive(Clone, Debug, Default)]
pub struct RunnerCatalog {
    entries: BTreeMap<String, CatalogEntry>,
}

impl RunnerCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: String, entry: CatalogEntry) {
        self.entries.insert(id, entry);
    }

    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.get(id)
    }
}

/// The qualified runtime capability evidence a caller must present:
/// both dimensions must be honestly "enforced" on this platform. The
/// runner refuses unqualified callers before any staging or spawn.
#[derive(Clone, Debug)]
pub struct RuntimeCapability {
    pub network_denial: String,
    pub process_containment: String,
    pub backend: String,
}

impl RuntimeCapability {
    /// Whether this evidence qualifies for execution.
    pub fn qualified(&self) -> bool {
        self.network_denial == "enforced" && self.process_containment == "enforced"
    }

    /// The honest receipt evidence derived from the qualification.
    pub fn evidence(&self) -> NativeCapabilityEvidence {
        NativeCapabilityEvidence {
            network_denial: self.network_denial.clone(),
            process_containment: self.process_containment.clone(),
            backend: Some(self.backend.clone()),
            receipt_digest: None,
        }
    }
}

/// The input of one confirmed run.
pub struct ConfirmedRun<'a> {
    /// The plan document (already accepted by `decode_plan`).
    pub plan: &'a super::types::NativePlan,
    /// The digest an external approver named — the plan never approves
    /// itself.
    pub approved_plan_digest: &'a str,
    /// The trusted catalog the executables resolve through.
    pub catalog: &'a RunnerCatalog,
    /// The verified source snapshot root (read-only input).
    pub source_root: &'a Path,
    /// The qualified runtime capability evidence.
    pub capability: &'a RuntimeCapability,
    /// The cooperative cancellation token (checked between and during
    /// commands).
    pub cancellation: Option<&'a AtomicBool>,
    /// Host paths that must never appear in persisted output.
    pub host_paths: Vec<String>,
    /// Secret values resolved for the child env (redacted from output).
    pub secrets: Vec<String>,
}

/// The terminal outcome of one confirmed run.
pub type ConfirmedRunOutcome = Result<NativeRunResult, NativeGateFailure>;

/// Execute one approved, qualified plan. Preflight refusals spawn
/// nothing: an unapproved digest, an unqualified capability, tool
/// custody drift, or an unstageable snapshot each end the run with the
/// typed failure before the first child process exists.
pub fn run_confirmed_plan(run: &ConfirmedRun<'_>) -> ConfirmedRunOutcome {
    // 1. Approval preflight: the named digest must be this exact plan.
    if run.approved_plan_digest != run.plan.plan_digest {
        return Err(NativeGateFailure::ApprovalMissing);
    }
    // 2. Runtime qualification: production refusals remain until the
    // capability is proven (research doc §1 step 5, §3).
    if !run.capability.qualified() {
        return Err(NativeGateFailure::CapabilityMissing {
            detail: "runtime-capability-unqualified",
        });
    }
    // 3. Tool custody: every command resolves through the catalog, and
    // the plan's recorded tool digest must equal the catalog's
    // verified provisioning digest. PATH is never consulted.
    let mut tool_versions: BTreeMap<String, String> = BTreeMap::new();
    for command in &run.plan.commands {
        let entry = run
            .catalog
            .get(&command.tool_ref)
            .ok_or(NativeGateFailure::TrustInsufficient)?;
        if entry.artifact_digest != plan_tool_digest(run.plan, &command.tool_ref) {
            return Err(NativeGateFailure::TrustInsufficient);
        }
        if !entry.program.is_absolute() || !entry.program.is_file() {
            return Err(NativeGateFailure::CapabilityMissing {
                detail: "catalog-program-absent",
            });
        }
        if let Some(version) = &entry.version {
            tool_versions.insert(command.tool_ref.clone(), version.clone());
        }
    }
    // 4. Staged disposable copy with reviewed budgets; the original
    // snapshot is never the cwd and is verified unchanged afterwards.
    let stage = tempfile::Builder::new()
        .prefix("lekalo-native-runner-")
        .tempdir()
        .map_err(|_| NativeGateFailure::RunInfrastructure)?;
    let stage_root = stage.path().join("project");
    let original_before = snapshot_tree(run.source_root)?;
    copy_tree(run.source_root, &stage_root)?;
    // 5. Execute the commands in plan order under the whole-run
    // deadline; a failed required gate preempts dependent work.
    let run_started = Instant::now();
    let run_deadline = Duration::from_millis(run.plan.limits.timeout_ms_per_run);
    let mut results: Vec<NativeCommandResult> = Vec::new();
    let mut terminal: Terminal = Terminal::Pass;
    let output_caps = OutputCaps {
        max_stdout_bytes: run.plan.limits.max_stdout_bytes as usize,
        max_stderr_bytes: run.plan.limits.max_stderr_bytes as usize,
    };
    for command in &run.plan.commands {
        if run_started.elapsed() >= run_deadline {
            terminal = terminal.max(Terminal::Infrastructure);
            results.push(preflight_miss(
                command,
                "infrastructure",
                Some(FailureClass::Infrastructure),
                vec![reasons::RUN_DEADLINE.to_owned()],
            ));
            continue;
        }
        if run
            .cancellation
            .is_some_and(|token| token.load(Ordering::SeqCst))
        {
            terminal = terminal.max(Terminal::Infrastructure);
            results.push(preflight_miss(
                command,
                "infrastructure",
                Some(FailureClass::Infrastructure),
                vec![reasons::CANCELLED.to_owned()],
            ));
            continue;
        }
        // A previously failed dependency cancels this command without
        // a spawn and without fabricated success.
        if terminal != Terminal::Pass && !command.depends_on.is_empty() {
            results.push(preflight_miss(
                command,
                "infrastructure",
                Some(FailureClass::Infrastructure),
                vec![reasons::CANCELLED.to_owned()],
            ));
            continue;
        }
        let entry = match run.catalog.get(&command.tool_ref) {
            Some(entry) => entry,
            None => {
                // Defense in depth: a tool_ref absent from the trusted
                // catalog is already a preflight trust refusal (the
                // whole-run custody loop above refuses before any
                // staging), so this arm only fires if that layer is
                // ever relaxed. It honors the same required/optional
                // split as the launch path below, never a blanket
                // blocker.
                terminal = terminal.max(if command.required {
                    Terminal::Blocked
                } else {
                    Terminal::Degraded
                });
                results.push(preflight_miss(
                    command,
                    "missing",
                    Some(FailureClass::MissingTool),
                    vec![reasons::TOOL_MISSING.to_owned()],
                ));
                continue;
            }
        };
        let started = Instant::now();
        let launched = launch_command(
            run,
            command,
            entry,
            &stage_root,
            &entry.program,
            &output_caps,
            Duration::from_millis(command.limits.timeout_ms_per_command),
        );
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let measurement = match launched {
            Ok(success) => {
                let (mut outcome, mut failure_class, mut reason_codes) = if success.flooded {
                    terminal = terminal.max(Terminal::Infrastructure);
                    (
                        "infrastructure",
                        Some(FailureClass::Infrastructure),
                        vec![reasons::OUTPUT_LIMIT.to_owned()],
                    )
                } else if success.timed_out {
                    terminal = terminal.max(Terminal::Infrastructure);
                    (
                        "infrastructure",
                        Some(FailureClass::Infrastructure),
                        vec![reasons::TIMEOUT.to_owned()],
                    )
                } else if success.exit_code == 0 {
                    ("passed", None, Vec::new())
                } else {
                    // The terminal nonzero exit of a confirmed gate is
                    // an assertion failure (static-analysis/boot
                    // refinement rides the tool's structured evidence,
                    // which the receipt links, not the exit code).
                    terminal = terminal.max(if command.required {
                        Terminal::Blocked
                    } else {
                        Terminal::Failure
                    });
                    (
                        "failed",
                        Some(FailureClass::Assertion),
                        vec![reasons::ASSERTION_FAILED.to_owned()],
                    )
                };
                if !success.unexpected_writes.is_empty() && outcome == "passed" {
                    // A passed gate that wrote outside its output home
                    // is a security defect, never a pass.
                    terminal = Terminal::Security;
                    outcome = "security";
                    failure_class = None;
                    reason_codes = vec![reasons::UNEXPECTED_WRITE.to_owned()];
                }
                CommandMeasurement {
                    command_id: command.id.clone(),
                    package_id: command.package_id.clone(),
                    cwd: command.cwd.clone(),
                    tool_ref: command.tool_ref.clone(),
                    argv: command.argv.clone(),
                    env_names: command.env.clone(),
                    gate_id: command.gate_id.clone(),
                    required: command.required,
                    // A deadline/cancellation teardown never reports a
                    // known exit: the killed child has no terminal
                    // measurement, and -1 would be a fabricated value.
                    exit_code: if success.timed_out {
                        None
                    } else {
                        Some(u64::try_from(success.exit_code.max(0)).unwrap_or(0))
                    },
                    duration_ms,
                    outcome,
                    failure_class,
                    reason_codes,
                    stdout: Some(success.stdout),
                    stderr: Some(success.stderr),
                    tool_version: tool_versions.get(&command.tool_ref).cloned(),
                    toolchain_ref: None,
                    env_recipe_digest: Some(env_recipe_digest(run.plan)),
                }
            }
            Err(LaunchFailure::Missing) => {
                terminal = terminal.max(if command.required {
                    Terminal::Blocked
                } else {
                    Terminal::Degraded
                });
                preflight_measurement(
                    command,
                    "missing",
                    Some(FailureClass::MissingTool),
                    vec![reasons::TOOL_MISSING.to_owned()],
                    duration_ms,
                )
            }
            Err(LaunchFailure::Incompatible) => {
                terminal = terminal.max(if command.required {
                    Terminal::Blocked
                } else {
                    Terminal::Degraded
                });
                preflight_measurement(
                    command,
                    "unsupported",
                    Some(FailureClass::Incompatible),
                    vec![reasons::TOOL_VERSION_INCOMPATIBLE.to_owned()],
                    duration_ms,
                )
            }
            Err(LaunchFailure::Security) => {
                terminal = Terminal::Security;
                preflight_measurement(
                    command,
                    "security",
                    None,
                    vec!["env-or-cwd".to_owned()],
                    duration_ms,
                )
            }
            Err(LaunchFailure::Infrastructure) => {
                terminal = terminal.max(Terminal::Infrastructure);
                preflight_measurement(
                    command,
                    "infrastructure",
                    Some(FailureClass::Infrastructure),
                    vec!["spawn-failed".to_owned()],
                    duration_ms,
                )
            }
        };
        results.push(command_result(measurement, output_digest));
    }
    // 6. Mutation audit: the staged tree is diffed against the original
    // snapshot; the original root is verified unchanged.
    let original_after = snapshot_tree(run.source_root)?;
    let original_state = if original_before == original_after {
        "unchanged"
    } else {
        terminal = Terminal::Security;
        "mutated"
    };
    let staged_after = snapshot_tree(&stage_root)?;
    let mut audit = audit_writes(run.plan, &original_before, &staged_after);
    if !audit.unexpected.is_empty() {
        terminal = Terminal::Security;
    }
    // 7. Cleanup: dropping the TempDir removes the disposable copy.
    let stage_path = stage.path().to_path_buf();
    drop(stage);
    let cleanup_state = if !stage_path.exists() {
        "complete"
    } else {
        if terminal != Terminal::Security {
            terminal = terminal.max(Terminal::Infrastructure);
        }
        audit.reason_codes.push(reasons::CLEANUP_FAILED.to_owned());
        "failed"
    };
    // 8. The terminal receipt: exactly one outcome, the rollup verdict,
    // coverage reconciliation, and the honest capability evidence.
    let outcome = terminal.as_str().to_owned();
    let passed_gate_ids: std::collections::BTreeSet<&str> = results
        .iter()
        .filter(|command| command.outcome == "passed")
        .map(|command| command.gate_id.as_str())
        .collect();
    let uncovered: Vec<String> = run
        .plan
        .selection
        .mandatory_gate_ids
        .iter()
        .filter(|gate_id| !passed_gate_ids.contains(gate_id.as_str()))
        .cloned()
        .collect();
    let mut receipt = NativeRunResult {
        schema_version: RUN_SCHEMA_VERSION.to_owned(),
        kind: "native-run-result".to_owned(),
        plan_digest: run.plan.plan_digest.clone(),
        execution_policy_ref: run.plan.execution_policy_ref.clone(),
        authority_ref: run.plan.authority_ref.clone(),
        policy_ref: run.plan.policy_ref.clone(),
        outcome: outcome.clone(),
        verdict: "blocked".to_owned(),
        reason_codes: audit.reason_codes,
        commands: results,
        coverage: NativeCoverage {
            state: if uncovered.is_empty() {
                "complete"
            } else {
                "incomplete"
            }
            .to_owned(),
            uncovered_gate_ids: uncovered,
        },
        mutation_summary: NativeMutationSummary {
            created: audit.created,
            modified: audit.modified,
            deleted: audit.deleted,
            unexpected: audit.unexpected,
        },
        original_verification: NativeOriginalVerification {
            state: original_state.to_owned(),
            mutated_paths: Vec::new(),
        },
        cleanup: NativeCleanup {
            state: cleanup_state.to_owned(),
            detail: None,
        },
        capability_evidence: run.capability.evidence(),
        provenance: None,
    };
    receipt.verdict = super::evidence::rollup(&receipt);
    validate_run_result(&serde_json::to_vec(&receipt).expect("receipt serializes")).map_err(
        |rejection| NativeGateFailure::PlanInvalid {
            detail: rejection.detail(),
        },
    )
}

/// The plan's recorded artifact digest for one tool id.
fn plan_tool_digest(plan: &super::types::NativePlan, tool_id: &str) -> String {
    plan.tools
        .iter()
        .find(|tool| tool.id == tool_id)
        .map(|tool| tool.artifact_digest.clone())
        .unwrap_or_default()
}

/// The plan's recorded version pin for one tool id, when carried.
fn plan_tool_version(plan: &super::types::NativePlan, tool_id: &str) -> Option<String> {
    plan.tools
        .iter()
        .find(|tool| tool.id == tool_id)
        .and_then(|tool| tool.version.clone())
}

/// The env recipe digest the receipt carries (names and kinds only —
/// never values).
fn env_recipe_digest(plan: &super::types::NativePlan) -> String {
    let joined = plan
        .env
        .bindings
        .iter()
        .map(|binding| {
            format!(
                "{}={}:{}",
                binding.name,
                binding.kind,
                if binding.value.is_some() { "1" } else { "0" }
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!("sha256:{}", crate::digest::sha256_hex(joined.as_bytes()))
}

/// The digest of one redacted output stream (the only bytes that ever
/// reach an artifact reference).
fn output_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}

/// The terminal classes of one run in rollup precedence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
enum Terminal {
    Pass,
    Degraded,
    Failure,
    Blocked,
    Infrastructure,
    Security,
}

impl Terminal {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "passed",
            Self::Degraded => "missing",
            Self::Failure => "failed",
            Self::Blocked => "blocked",
            Self::Infrastructure => "infrastructure",
            Self::Security => "security",
        }
    }
}

/// A never-launched command measurement: unknown exit, unknown
/// duration, no output — a preflight miss never fabricates a zero.
fn preflight_measurement(
    command: &super::types::NativeCommand,
    outcome: &'static str,
    failure_class: Option<FailureClass>,
    reason_codes: Vec<String>,
    duration_ms: u64,
) -> CommandMeasurement {
    CommandMeasurement {
        command_id: command.id.clone(),
        package_id: command.package_id.clone(),
        cwd: command.cwd.clone(),
        tool_ref: command.tool_ref.clone(),
        argv: command.argv.clone(),
        env_names: command.env.clone(),
        gate_id: command.gate_id.clone(),
        required: command.required,
        exit_code: None,
        duration_ms,
        outcome,
        failure_class,
        reason_codes,
        stdout: None,
        stderr: None,
        tool_version: None,
        toolchain_ref: None,
        env_recipe_digest: None,
    }
}

fn preflight_miss(
    command: &super::types::NativeCommand,
    outcome: &'static str,
    failure_class: Option<FailureClass>,
    reason_codes: Vec<String>,
) -> NativeCommandResult {
    command_result(
        preflight_measurement(command, outcome, failure_class, reason_codes, 0),
        output_digest,
    )
}

/// The outcome of one launched command.
struct CommandSuccess {
    exit_code: i32,
    stdout: RedactedOutput,
    stderr: RedactedOutput,
    timed_out: bool,
    flooded: bool,
    unexpected_writes: Vec<String>,
}

enum LaunchFailure {
    /// The resolved program or confirmed script is absent.
    Missing,
    /// The tool exists but its observed version does not satisfy the
    /// confirmed compatibility pin.
    Incompatible,
    /// A cwd escape or an env name outside the approved recipe.
    Security,
    /// Spawn/deadline/pipe infrastructure failure.
    Infrastructure,
}

/// Launch one command inside the staged view: direct argv from the
/// catalog program, cleared env with the approved grants, bounded
/// pipes, the per-command deadline, and process-tree teardown.
fn launch_command(
    run: &ConfirmedRun<'_>,
    command: &super::types::NativeCommand,
    entry: &CatalogEntry,
    stage_root: &Path,
    program: &Path,
    caps: &OutputCaps,
    deadline: Duration,
) -> Result<CommandSuccess, LaunchFailure> {
    use std::process::{Command, Stdio};
    // The logical cwd must resolve inside the staged package root:
    // traversal, absolute spellings, and symlinks escaping the stage
    // are refused before any spawn.
    if command.cwd.contains('\\') || command.cwd.contains("..") {
        return Err(LaunchFailure::Security);
    }
    let cwd = if command.cwd == "." {
        stage_root.to_path_buf()
    } else {
        stage_root.join(&command.cwd)
    };
    let cwd_canonical = std::fs::canonicalize(&cwd).map_err(|_| LaunchFailure::Security)?;
    let stage_canonical =
        std::fs::canonicalize(stage_root).map_err(|_| LaunchFailure::Infrastructure)?;
    if !cwd_canonical.starts_with(&stage_canonical) {
        return Err(LaunchFailure::Security);
    }
    // The confirmed recipe launches the interpreter with the script as
    // argv elements; argv[0] must name the catalog tool family the
    // plan recorded — the executable itself always comes from the
    // catalog path, never from argv or PATH.
    let resolved_argv0 = command.argv.first().ok_or(LaunchFailure::Missing)?;
    let catalog_family = program
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(LaunchFailure::Missing)?;
    let family_matches =
        resolved_argv0 == catalog_family || (catalog_family == "php" && resolved_argv0 == "php");
    if !family_matches {
        return Err(LaunchFailure::Missing);
    }
    // Confirmed compatibility: the plan's recorded tool version must
    // match the catalog's observed version when both are known (a
    // pinned expectation never silently runs against another build).
    if let Some(expected) = plan_tool_version(run.plan, &command.tool_ref) {
        if expected != "unknown"
            && entry
                .version
                .as_deref()
                .is_some_and(|observed| observed != expected)
        {
            return Err(LaunchFailure::Incompatible);
        }
    }
    // When the confirmed program names an interpreter-relative script
    // (e.g. `vendor/bin/testo`), the catalog entry for the pinned
    // family launches the script through the pinned binary with the
    // absolute staged path — the relative token alone would re-enter
    // ambient resolution. A missing staged script is a preflight miss,
    // never a child execution.
    let mut cmd = Command::new(program);
    let mut args: Vec<String> = command.argv.clone();
    if args.len() > 1 && args[1].starts_with("vendor/bin/") {
        let script = stage_canonical.join(&args[1]);
        if !script.is_file() {
            return Err(LaunchFailure::Missing);
        }
        args[1] = script.to_string_lossy().into_owned();
    }
    cmd.args(&args[1..]);
    cmd.current_dir(&cwd_canonical);
    // Cleared environment plus exactly the approved grants: the plan's
    // env recipe filtered to this command's approved names, with
    // literal values and stage-relative temp/home relocations only.
    cmd.env_clear();
    for binding in &run.plan.env.bindings {
        if !command.env.iter().any(|approved| approved == &binding.name) {
            continue;
        }
        match binding.kind.as_str() {
            "literal" => {
                let value = binding.value.clone().unwrap_or_default();
                cmd.env(&binding.name, value);
            }
            "execution-temp" => {
                let temp = stage_root.join(".lekalo-temp");
                let _ = std::fs::create_dir_all(&temp);
                cmd.env(&binding.name, &temp);
            }
            "execution-home" => {
                let home = stage_root.join(".lekalo-home");
                let _ = std::fs::create_dir_all(&home);
                cmd.env(&binding.name, &home);
            }
            "platform-system-root" => {
                // The platform system root is a read-only runtime
                // constant (Windows runtimes cannot seed crypto without
                // it); it is copied from the host, never from a caller.
                #[cfg(windows)]
                if let Some(root) = std::env::var_os("SystemRoot") {
                    cmd.env(&binding.name, root);
                }
                // On unix the cleared environment is already sufficient.
            }
            _ => return Err(LaunchFailure::Security),
        }
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    // The child gets its own process group on unix so the teardown
    // reaps descendants (the same discipline transport.rs owns).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|_| LaunchFailure::Infrastructure)?;
    // Bounded pipes: both streams drain on dedicated threads; the caps
    // mark the flood so the classification sees it.
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();
    let max_stdout = caps.max_stdout_bytes;
    let max_stderr = caps.max_stderr_bytes;
    let stdout_thread = std::thread::spawn(move || drain_bounded(stdout_handle, max_stdout));
    let stderr_thread = std::thread::spawn(move || {
        drain_bounded::<std::process::ChildStderr>(stderr_handle, max_stderr)
    });
    let started = Instant::now();
    let cancelled = run
        .cancellation
        .map(|token| token.load(Ordering::SeqCst))
        .unwrap_or(false);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() >= deadline {
                    timed_out = true;
                    break None;
                }
                if run
                    .cancellation
                    .is_some_and(|token| token.load(Ordering::SeqCst))
                {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                kill_tree(child.id());
                return Err(LaunchFailure::Infrastructure);
            }
        }
    };
    if timed_out {
        // Process-tree teardown: descendants never survive the deadline.
        kill_tree(child.id());
        let _ = child.wait();
    }
    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();
    if cancelled && timed_out {
        return Err(LaunchFailure::Infrastructure);
    }
    // A deadline/cancellation teardown reports no exit measurement: the
    // child never observed a terminal exit, so the receipt carries the
    // unknown state (a killed process never fabricates a zero).
    let (exit_code, timed_out) = match &status {
        Some(status) => (status.code().unwrap_or(-1), false),
        None => (-1, true),
    };
    let flooded = stdout_bytes.1 || stderr_bytes.1;
    let stdout = redact_stream(
        &stdout_bytes.0,
        caps.max_stdout_bytes,
        &run.secrets,
        &run.host_paths,
    );
    let stderr = redact_stream(
        &stderr_bytes.0,
        caps.max_stderr_bytes,
        &run.secrets,
        &run.host_paths,
    );
    let unexpected_writes = Vec::new();
    Ok(CommandSuccess {
        exit_code,
        stdout,
        stderr,
        timed_out,
        flooded,
        unexpected_writes,
    })
}

/// Drain one bounded pipe: up to `cap` bytes plus one overflow marker.
fn drain_bounded<R: std::io::Read>(pipe: Option<R>, cap: usize) -> (Vec<u8>, bool) {
    let mut pipe = match pipe {
        Some(pipe) => pipe,
        None => return (Vec::new(), false),
    };
    let mut bytes = Vec::new();
    let mut flooded = false;
    let mut chunk = [0u8; 8192];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = cap.saturating_sub(bytes.len());
                if remaining == 0 {
                    flooded = true;
                    // Keep draining so the child never blocks on a
                    // full pipe while we tear it down.
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
    (bytes, flooded)
}

/// Process-tree teardown: the same kill discipline transport.rs owns —
/// the unix child sits in its own process group; Windows walks the
/// tree with the platform tool.
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
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
}

/// The bounded staged-tree snapshot: relative logical paths to digests.
fn snapshot_tree(root: &Path) -> Result<BTreeMap<String, String>, NativeGateFailure> {
    let mut map = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    let mut files = 0usize;
    let mut total = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|_| NativeGateFailure::RunInfrastructure)?;
        for entry in entries {
            let entry = entry.map_err(|_| NativeGateFailure::RunInfrastructure)?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|_| NativeGateFailure::RunInfrastructure)?;
            if file_type.is_symlink() {
                // Symlinks never stage: junction/symlink escapes are
                // refused at the copy, not audited afterwards.
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            files += 1;
            if files > STAGE_MAX_FILES {
                return Err(NativeGateFailure::RunInfrastructure);
            }
            let metadata = entry
                .metadata()
                .map_err(|_| NativeGateFailure::RunInfrastructure)?;
            if metadata.len() as usize > STAGE_MAX_FILE_BYTES {
                return Err(NativeGateFailure::RunInfrastructure);
            }
            total += metadata.len() as usize;
            if total > STAGE_MAX_BYTES {
                return Err(NativeGateFailure::RunInfrastructure);
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| NativeGateFailure::RunInfrastructure)?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).map_err(|_| NativeGateFailure::RunInfrastructure)?;
            map.insert(
                relative,
                format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
            );
        }
    }
    Ok(map)
}

/// The bounded staged copy: regular files only, symlinks refused,
/// budgets enforced before any command spawns.
fn copy_tree(source: &Path, destination: &Path) -> Result<(), NativeGateFailure> {
    let mut files = 0usize;
    let mut total = 0usize;
    let mut stack = vec![source.to_path_buf()];
    std::fs::create_dir_all(destination).map_err(|_| NativeGateFailure::RunInfrastructure)?;
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|_| NativeGateFailure::RunInfrastructure)?;
        for entry in entries {
            let entry = entry.map_err(|_| NativeGateFailure::RunInfrastructure)?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|_| NativeGateFailure::RunInfrastructure)?;
            if file_type.is_symlink() {
                continue;
            }
            let relative = path
                .strip_prefix(source)
                .map_err(|_| NativeGateFailure::RunInfrastructure)?;
            let target = destination.join(relative);
            if file_type.is_dir() {
                std::fs::create_dir_all(&target)
                    .map_err(|_| NativeGateFailure::RunInfrastructure)?;
                stack.push(path);
            } else {
                files += 1;
                if files > STAGE_MAX_FILES {
                    return Err(NativeGateFailure::RunInfrastructure);
                }
                let metadata = entry
                    .metadata()
                    .map_err(|_| NativeGateFailure::RunInfrastructure)?;
                if metadata.len() as usize > STAGE_MAX_FILE_BYTES {
                    return Err(NativeGateFailure::RunInfrastructure);
                }
                total += metadata.len() as usize;
                if total > STAGE_MAX_BYTES {
                    return Err(NativeGateFailure::RunInfrastructure);
                }
                std::fs::copy(&path, &target).map_err(|_| NativeGateFailure::RunInfrastructure)?;
            }
        }
    }
    Ok(())
}

/// The write-policy audit: staged mutations classified against the
/// plan's allowed writes; the reason codes accumulate the terminal
/// defects of the run.
struct WriteAudit {
    created: Vec<super::types::NativeMutation>,
    modified: Vec<super::types::NativeMutation>,
    deleted: Vec<String>,
    unexpected: Vec<String>,
    reason_codes: Vec<String>,
}

fn audit_writes(
    plan: &super::types::NativePlan,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> WriteAudit {
    let mut audit = WriteAudit {
        created: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
        unexpected: Vec::new(),
        reason_codes: Vec::new(),
    };
    let scoped = plan.write_policy.mode == "scoped";
    let scopes = plan.write_policy.scopes.clone().unwrap_or_default();
    let covers = |path: &str| -> bool {
        !scoped
            || scopes
                .iter()
                .any(|scope| path == scope.root || path.starts_with(&format!("{}/", scope.root)))
    };
    for (path, digest) in after {
        match before.get(path) {
            None => {
                if covers(path) {
                    audit.created.push(super::types::NativeMutation {
                        path: path.clone(),
                        digest: Some(digest.clone()),
                    });
                } else {
                    audit.unexpected.push(path.clone());
                }
            }
            Some(before_digest) => {
                if before_digest != digest {
                    if covers(path) {
                        audit.modified.push(super::types::NativeMutation {
                            path: path.clone(),
                            digest: Some(digest.clone()),
                        });
                    } else {
                        audit.unexpected.push(path.clone());
                    }
                }
            }
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            if covers(path) {
                audit.deleted.push(path.clone());
            } else {
                audit.unexpected.push(path.clone());
            }
        }
    }
    if !audit.unexpected.is_empty() {
        audit
            .reason_codes
            .push(reasons::UNEXPECTED_WRITE.to_owned());
    }
    audit
}

#[cfg(test)]
mod runner_tests {
    use super::*;
    use crate::native_gate::types::NativeOutputRef;
    use crate::native_gate::wire::decode_plan;

    /// Locate a Node interpreter for the test children (absolute path
    /// only — the runner itself never consults PATH). The tests skip
    /// honestly when no interpreter is available on this host.
    fn node_program() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("LEKALO_RUNNER_NODE") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Some(path);
            }
        }
        let paths = std::env::var_os("PATH")?;
        for name in if cfg!(windows) {
            ["node.exe"]
        } else {
            ["node"]
        } {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// One minimal runnable plan over one package and one command whose
    /// argv runs `node -e <script>`. The digest and the pinned selection
    /// reference are recomputed so tests vary the script freely.
    fn plan_with_script(
        script: &str,
        timeout_ms_per_command: u64,
    ) -> super::super::types::NativePlan {
        let template = serde_json::json!({
            "schema_version": "lekalo/native-gate-plan/v0.4.0",
            "kind": "native-plan",
            "plan_digest": format!("sha256:{}", "0".repeat(64)),
            "adapter": {"id": "lekalo-target-php-laravel", "version": "0.2.0", "digest": format!("sha256:{}", "a".repeat(64))},
            "planner_version": "0.4.0",
            "canonicalization_version": "0.4.0",
            "repository_role": "consumer-repository",
            "trust": {"mode": "public-fixture", "fixture_ref": format!("sha256:{}", "a".repeat(64)), "provenance_ref": format!("sha256:{}", "b".repeat(64))},
            "authority_ref": {"id": "authority", "version": "0.3.2", "digest": format!("sha256:{}", "c".repeat(64))},
            "policy_ref": {"id": "policy", "version": "0.3.2", "digest": format!("sha256:{}", "d".repeat(64))},
            "classification_ref": {"id": "classification", "version": "0.2.16", "digest": format!("sha256:{}", "e".repeat(64))},
            "execution_policy_ref": {"id": "composer-gates-policy", "version": "0.4.0", "digest": format!("sha256:{}", "f".repeat(64))},
            "profile_ref": {"id": "default", "digest": format!("sha256:{}", "1".repeat(64))},
            "input_manifest_digest": format!("sha256:{}", "2".repeat(64)),
            "scan_ref": {"id": "scan", "version": "0.4.0", "digest": format!("sha256:{}", "3".repeat(64))},
            "observed_ref": {"id": "observed", "version": "0.4.0", "digest": format!("sha256:{}", "4".repeat(64))},
            "tool_catalog_digest": format!("sha256:{}", "5".repeat(64)),
            "capability_snapshot_digest": format!("sha256:{}", "6".repeat(64)),
            "workspace": {
                "manager": "composer-project",
                "declared_version": "unknown",
                "compatibility_path": "partial",
                "root": ".",
                "manifest_digest": format!("sha256:{}", "7".repeat(64)),
                "lock_digest_state": "absent",
                "packages": [{"id": ".=fixture/project", "name": "fixture/project", "root": ".", "manifest_digest": format!("sha256:{}", "7".repeat(64))}],
                "edges": [],
                "completeness": "complete",
                "uncertainties": []
            },
            "changes": {"files": [{"path": "app/x.php", "change": "modified"}], "symbols": []},
            "affected": [{"package_id": ".=fixture/project", "reasons": [{"kind": "changed-package", "source_ref": "planner"}]}],
            "excluded": [],
            "selection_mode": "targeted",
            "selection": {
                "mode": "targeted",
                "modules": ["planner"],
                "tests": [],
                "mandatory_gate_ids": ["gate-smoke"],
                "excluded": [],
                "uncertainties": [],
                "fallback_rule_ref": null
            },
            "commands": [{
                "id": "gate-smoke",
                "package_id": ".=fixture/project",
                "gate": "test",
                "gate_id": "gate-smoke",
                "gate_kind": "composer-script",
                "required": true,
                "selection_ref": format!("sha256:{}", "8".repeat(64)),
                "covers_suite_ids": [],
                "script_name": "gate:smoke",
                "script_digest": format!("sha256:{}", "9".repeat(64)),
                "confirmation_ref": format!("sha256:{}", "aa".repeat(32)),
                "cwd": ".",
                "tool_ref": "node-runtime",
                "argv": ["node", "-e", script],
                "env": ["LEKALO_TEST_GRANT", "SystemRoot"],
                "depends_on": [],
                "affected_reason_refs": [".=fixture/project#changed-package"],
                "read_manifest_ref": format!("sha256:{}", "bb".repeat(32)),
                "allowed_writes": {"mode": "stage-only"},
                "limits": {
                    "timeout_ms_per_command": timeout_ms_per_command,
                    "timeout_ms_per_run": 60000,
                    "max_stdout_bytes": 65536,
                    "max_stderr_bytes": 65536,
                    "max_output_bytes_per_run": 1048576
                }
            }],
            "env": {
                "allowed_names": ["LEKALO_TEST_GRANT", "SystemRoot"],
                "bindings": [{"name": "LEKALO_TEST_GRANT", "kind": "literal", "value": "granted-value"}, {"name": "SystemRoot", "kind": "platform-system-root"}]
            },
            "tools": [{"id": "node-runtime", "name": "node", "version": "test", "artifact_digest": format!("sha256:{}", "cc".repeat(32)), "platform": "test", "provenance": "fixture-catalog"}],
            "required_capabilities": ["plan.native-gates"],
            "capabilities": [{"id": "plan.native-gates", "definition_version": "0.4.0", "state": "full", "source": "declared"}],
            "run_eligibility": {"state": "runnable", "reason_codes": []},
            "limits": {
                "timeout_ms_per_command": timeout_ms_per_command,
                "timeout_ms_per_run": 60000,
                "max_stdout_bytes": 65536,
                "max_stderr_bytes": 65536,
                "max_output_bytes_per_run": 1048576
            },
            "write_policy": {"mode": "stage-only"}
        });
        let mut plan: super::super::types::NativePlan =
            serde_json::from_value(template).expect("the test plan template decodes");
        let selection = plan.selection.clone();
        for command in &mut plan.commands {
            command.selection_ref = crate::native_gate::selection_digest(&selection);
        }
        plan.plan_digest = crate::native_gate::plan_digest(&plan);
        plan
    }

    fn qualified() -> RuntimeCapability {
        RuntimeCapability {
            network_denial: "enforced".to_owned(),
            process_containment: "enforced".to_owned(),
            backend: "test-qualification".to_owned(),
        }
    }

    fn unqualified() -> RuntimeCapability {
        RuntimeCapability {
            network_denial: "unavailable".to_owned(),
            process_containment: "unavailable".to_owned(),
            backend: "test-qualification".to_owned(),
        }
    }

    /// The staging source with one tracked input file.
    fn source_root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("source");
        std::fs::create_dir_all(root.join("app")).expect("mkdir");
        std::fs::write(root.join("app").join("x.php"), b"<?php echo 1;\n").expect("write");
        (dir, root)
    }

    fn catalog(program: &Path) -> RunnerCatalog {
        let mut catalog = RunnerCatalog::new();
        catalog.insert(
            "node-runtime".to_owned(),
            CatalogEntry {
                program: program.to_path_buf(),
                artifact_digest: format!("sha256:{}", "cc".repeat(32)),
                version: Some("test".to_owned()),
            },
        );
        catalog
    }

    fn run_with(
        plan: &super::super::types::NativePlan,
        program: &Path,
        capability: &RuntimeCapability,
        source: &Path,
    ) -> ConfirmedRunOutcome {
        let catalog = catalog(program);
        run_confirmed_plan(&ConfirmedRun {
            plan,
            approved_plan_digest: &plan.plan_digest,
            catalog: &catalog,
            source_root: source,
            capability,
            cancellation: None,
            host_paths: vec![source.to_string_lossy().into_owned()],
            secrets: vec!["leak-canary-secret".to_owned()],
        })
    }

    #[test]
    fn an_unqualified_capability_is_refused_before_any_spawn() {
        let Some(program) = node_program() else {
            println!("node missing on this host; the runner battery skips");
            return;
        };
        let plan = plan_with_script("process.exit(7)", 10_000);
        let (_dir, source) = source_root();
        let failure = run_with(&plan, &program, &unqualified(), &source).unwrap_err();
        assert!(matches!(
            failure,
            NativeGateFailure::CapabilityMissing { .. }
        ));
    }

    #[test]
    fn an_unapproved_digest_never_reaches_staging() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script("process.exit(7)", 10_000);
        let (_dir, source) = source_root();
        let catalog = catalog(&program);
        let capability = qualified();
        let failure = run_confirmed_plan(&ConfirmedRun {
            plan: &plan,
            approved_plan_digest: &format!("sha256:{}", "9".repeat(64)),
            catalog: &catalog,
            source_root: &source,
            capability: &capability,
            cancellation: None,
            host_paths: vec![],
            secrets: vec![],
        })
        .unwrap_err();
        assert_eq!(failure, NativeGateFailure::ApprovalMissing);
    }

    #[test]
    fn tool_custody_drift_is_refused_before_the_first_spawn() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script("process.exit(7)", 10_000);
        let (_dir, source) = source_root();
        let mut drifted = catalog(&program);
        drifted.insert(
            "node-runtime".to_owned(),
            CatalogEntry {
                program: program.clone(),
                artifact_digest: format!("sha256:{}", "dd".repeat(32)),
                version: None,
            },
        );
        let capability = qualified();
        let failure = run_confirmed_plan(&ConfirmedRun {
            plan: &plan,
            approved_plan_digest: &plan.plan_digest,
            catalog: &drifted,
            source_root: &source,
            capability: &capability,
            cancellation: None,
            host_paths: vec![],
            secrets: vec![],
        })
        .unwrap_err();
        assert_eq!(failure, NativeGateFailure::TrustInsufficient);
    }

    #[test]
    fn a_passing_gate_runs_directly_with_a_cleared_env_and_a_clean_audit() {
        let Some(program) = node_program() else {
            return;
        };
        // The child proves: the approved grant is present, an unapproved
        // host variable is absent, and the secret env value never
        // reaches the captured output.
        unsafe {
            std::env::set_var("LEKALO_RUNNER_CANARY", "leak-canary-secret");
        }
        let script = "process.stdout.write((process.env.LEKALO_TEST_GRANT ?? String.fromCharCode(45)) + (process.env.LEKALO_RUNNER_CANARY ?? String.fromCharCode(45)))";
        let plan = plan_with_script(script, 30_000);
        let (dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source).expect("the run succeeds");
        eprintln!("DBGOUT={:?}", receipt.commands[0]);
        assert_eq!(receipt.outcome, "passed");
        assert_eq!(receipt.verdict, "passed");
        assert_eq!(receipt.commands.len(), 1);
        assert_eq!(receipt.commands[0].outcome, "passed");
        assert_eq!(receipt.commands[0].failure_class, None);
        assert_eq!(receipt.original_verification.state, "unchanged");
        assert_eq!(receipt.cleanup.state, "complete");
        assert!(receipt.mutation_summary.created.is_empty());
        assert!(receipt.mutation_summary.unexpected.is_empty());
        assert_eq!(receipt.coverage.state, "complete");
        assert_eq!(receipt.capability_evidence.network_denial, "enforced");
        // The captured output must be exactly the redacted truth: the
        // grant visible, the canary variable absent.
        let expected = b"granted-value-".to_vec();
        let expected_digest = format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                &crate::native_gate::evidence::redact_stream(
                    &expected,
                    65_536,
                    &["leak-canary-secret".to_owned()],
                    &[source.to_string_lossy().into_owned()],
                )
                .bytes
            )
        );
        assert_eq!(
            receipt.commands[0].output_ref,
            Some(NativeOutputRef::Digest(expected_digest))
        );
        drop(dir);
    }

    #[test]
    fn a_failing_required_gate_asserts_and_blocks_the_verdict() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script("process.exit(3)", 30_000);
        let (_dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source).expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "failed");
        assert_eq!(
            receipt.commands[0].failure_class.as_deref(),
            Some("assertion")
        );
        assert_eq!(receipt.outcome, "blocked");
        assert_eq!(receipt.verdict, "blocked");
        assert_eq!(receipt.coverage.uncovered_gate_ids, vec!["gate-smoke"]);
    }

    #[test]
    fn a_deadline_child_is_killed_and_never_fabricates_an_exit() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script("while(true){}", 200);
        let (_dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source).expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "infrastructure");
        assert!(receipt.commands[0]
            .reason_codes
            .contains(&reasons::TIMEOUT.to_owned()));
        // A killed child never carries a fabricated exit measurement.
        assert!(receipt.commands[0].exit.is_none());
        assert_eq!(receipt.outcome, "infrastructure");
    }

    #[test]
    fn an_output_flood_is_infrastructure_with_bounded_evidence() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script(
            "process.stdout.write(String.fromCharCode(120).repeat(200000))",
            30_000,
        );
        let (_dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source).expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "infrastructure");
        assert!(receipt.commands[0]
            .reason_codes
            .contains(&reasons::OUTPUT_LIMIT.to_owned()));
    }

    #[test]
    fn a_hostile_cwd_is_a_security_refusal_that_spawns_nothing() {
        let Some(program) = node_program() else {
            return;
        };
        let mut plan = plan_with_script("process.exit(7)", 10_000);
        plan.commands[0].cwd = "../escape".to_owned();
        // Recompute the custody after the tamper so the cwd check fires.
        let selection = plan.selection.clone();
        for command in &mut plan.commands {
            command.selection_ref = crate::native_gate::selection_digest(&selection);
        }
        plan.plan_digest = crate::native_gate::plan_digest(&plan);
        let (_dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source)
            .expect("the hostile command is refused at launch");
        assert_eq!(receipt.outcome, "security");
        assert_eq!(receipt.commands[0].outcome, "security");
        assert!(receipt.commands[0]
            .reason_codes
            .contains(&"env-or-cwd".to_owned()));
        assert!(receipt.commands[0].exit.is_none());
    }

    #[test]
    fn a_stage_write_inside_the_policy_is_audited_not_security() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script(
            "f=require(String.fromCharCode(102,115)),f.writeFileSync(String.fromCharCode(103,101,110,101,114,97,116,101,100,46,116,120,116),String.fromCharCode(111,107))",
            30_000,
        );
        let (_dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source).expect("the run succeeds");
        assert_eq!(receipt.outcome, "passed");
        assert_eq!(receipt.mutation_summary.created.len(), 1);
        assert_eq!(receipt.mutation_summary.created[0].path, "generated.txt");
        assert_eq!(receipt.original_verification.state, "unchanged");
    }

    #[test]
    fn cancellation_prevents_the_spawn_of_the_remaining_commands() {
        let Some(program) = node_program() else {
            return;
        };
        let cancel = AtomicBool::new(true);
        let plan = plan_with_script("process.exit(7)", 10_000);
        let (_dir, source) = source_root();
        let catalog = catalog(&program);
        let capability = qualified();
        let receipt = run_confirmed_plan(&ConfirmedRun {
            plan: &plan,
            approved_plan_digest: &plan.plan_digest,
            catalog: &catalog,
            source_root: &source,
            capability: &capability,
            cancellation: Some(&cancel),
            host_paths: vec![],
            secrets: vec![],
        })
        .expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "infrastructure");
        assert!(receipt.commands[0]
            .reason_codes
            .contains(&reasons::CANCELLED.to_owned()));
        assert!(receipt.commands[0].exit.is_none());
    }

    #[test]
    fn the_planned_dependencies_of_a_failed_gate_never_spawn() {
        let Some(program) = node_program() else {
            return;
        };
        let mut plan = plan_with_script("process.exit(1)", 10_000);
        plan.selection.mandatory_gate_ids.clear();
        // A second gate depends on the failing one.
        let mut second = plan.commands[0].clone();
        second.id = "gate-dependent".to_owned();
        second.gate_id = "gate-dependent".to_owned();
        second.depends_on = vec!["gate-smoke".to_owned()];
        second.selection_ref = plan.commands[0].selection_ref.clone();
        plan.commands.push(second);
        let selection = plan.selection.clone();
        for command in &mut plan.commands {
            command.selection_ref = crate::native_gate::selection_digest(&selection);
        }
        plan.plan_digest = crate::native_gate::plan_digest(&plan);
        let (_dir, source) = source_root();
        let receipt = run_with(&plan, &program, &qualified(), &source).expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "failed");
        assert_eq!(receipt.commands[1].outcome, "infrastructure");
        assert!(receipt.commands[1]
            .reason_codes
            .contains(&reasons::CANCELLED.to_owned()));
        assert!(receipt.commands[1].exit.is_none());
    }

    /// The staged bytes the runner audits are the exact staged bytes:
    /// a smoke check that the snapshot helper round-trips.
    #[test]
    fn the_snapshot_helper_hashes_the_staged_tree_deterministically() {
        let (dir, source) = source_root();
        let first = snapshot_tree(&source).expect("snapshot");
        let second = snapshot_tree(&source).expect("snapshot");
        assert_eq!(first, second);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(source.join("app").join("x.php"))
            .expect("open");
        use std::io::Write;
        file.write_all(b"// more\n").expect("append");
        drop(file);
        let third = snapshot_tree(&source).expect("snapshot");
        assert_ne!(first, third);
        drop(dir);
    }

    #[test]
    fn a_missing_confirmed_script_blocks_a_required_gate_and_degrades_an_optional_one() {
        let Some(program) = node_program() else {
            return;
        };
        let (_dir, source) = source_root();
        // Required: the missing script blocks the whole answer.
        let required = plan_with_script("process.exit(0)", 10_000);
        let mut required = required;
        required.commands[0].argv[1] = "vendor/bin/absent-gate".to_owned();
        required.commands[0].script_name = "gate:absent".to_owned();
        let selection = required.selection.clone();
        for command in &mut required.commands {
            command.selection_ref = crate::native_gate::selection_digest(&selection);
        }
        required.plan_digest = crate::native_gate::plan_digest(&required);
        let receipt =
            run_with(&required, &program, &qualified(), &source).expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "missing");
        assert_eq!(
            receipt.commands[0].failure_class.as_deref(),
            Some("missing-tool")
        );
        assert!(receipt.commands[0].exit.is_none());
        assert_eq!(receipt.verdict, "blocked");

        // Optional: the same absence degrades the summary instead.
        let optional = plan_with_script("process.exit(0)", 10_000);
        let mut optional = optional;
        optional.commands[0].argv[1] = "vendor/bin/absent-gate".to_owned();
        optional.commands[0].required = false;
        optional.commands[0].script_name = "gate:absent".to_owned();
        let selection = optional.selection.clone();
        for command in &mut optional.commands {
            command.selection_ref = crate::native_gate::selection_digest(&selection);
        }
        optional.plan_digest = crate::native_gate::plan_digest(&optional);
        let receipt =
            run_with(&optional, &program, &qualified(), &source).expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "missing");
        assert_eq!(receipt.verdict, "degraded");
    }

    #[test]
    fn a_version_pin_mismatch_is_an_incompatible_tool_not_a_failure() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script("process.exit(0)", 10_000);
        // The catalog attests a different build than the plan pinned.
        let mut catalog = RunnerCatalog::new();
        catalog.insert(
            "node-runtime".to_owned(),
            CatalogEntry {
                program: program.to_path_buf(),
                artifact_digest: format!("sha256:{}", "cc".repeat(32)),
                version: Some("other-build".to_owned()),
            },
        );
        let capability = qualified();
        let (_dir, source) = source_root();
        let receipt = run_confirmed_plan(&ConfirmedRun {
            plan: &plan,
            approved_plan_digest: &plan.plan_digest,
            catalog: &catalog,
            source_root: &source,
            capability: &capability,
            cancellation: None,
            host_paths: vec![],
            secrets: vec![],
        })
        .expect("the run completes");
        assert_eq!(receipt.commands[0].outcome, "unsupported");
        assert_eq!(
            receipt.commands[0].failure_class.as_deref(),
            Some("incompatible")
        );
        assert!(receipt.commands[0].exit.is_none());
        assert_eq!(receipt.verdict, "blocked");
    }

    #[test]
    fn a_tool_absent_from_the_trusted_catalog_is_a_preflight_trust_refusal() {
        let Some(program) = node_program() else {
            return;
        };
        let plan = plan_with_script("process.exit(0)", 10_000);
        // The plan names node-runtime; the catalog only attests another
        // tool: the whole-run custody preflight refuses before any
        // staging or spawn (trust, not a per-gate degradation).
        let mut catalog = RunnerCatalog::new();
        catalog.insert(
            "other-runtime".to_owned(),
            CatalogEntry {
                program: program.to_path_buf(),
                artifact_digest: format!("sha256:{}", "cc".repeat(32)),
                version: Some("test".to_owned()),
            },
        );
        let capability = qualified();
        let (_dir, source) = source_root();
        let failure = run_confirmed_plan(&ConfirmedRun {
            plan: &plan,
            approved_plan_digest: &plan.plan_digest,
            catalog: &catalog,
            source_root: &source,
            capability: &capability,
            cancellation: None,
            host_paths: vec![],
            secrets: vec![],
        })
        .unwrap_err();
        assert_eq!(failure, NativeGateFailure::TrustInsufficient);
    }

    #[test]
    fn decode_plan_accepts_the_runner_plan_template() {
        let plan = plan_with_script("process.exit(0)", 10_000);
        let bytes = serde_json::to_vec(&plan).expect("serialize");
        assert!(decode_plan(&bytes).is_ok());
    }
}
