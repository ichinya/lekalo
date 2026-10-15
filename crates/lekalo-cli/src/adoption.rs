//! The `lekalo adoption` command surface (issue #136): the thin
//! handoff to the core adoption service. Every decision — scope
//! resolution, pin admission, plan digests, verification execution,
//! transactional installation, and rollback conflict detection — lives
//! in the core; this layer only selects inputs, renders documents, and
//! maps exits per the result contract.

use clap::Subcommand;
use lekalo_core::adoption::version::{CHECK_OUTPUT_BYTES, CHECK_TIMEOUT_MS};
use lekalo_core::adoption::VerificationHost;
use lekalo_core::result::DomainResult;

use crate::selection_for;

/// The admitted host execution capability (X2/R3): lekalo-core stays
/// pure and offline — it defines the verification contract and never
/// spawns — while this trusted binary supplies the process boundary:
/// PATH resolution, the bounded drained pipes, the scrubbed child
/// environment, the deadline with process-tree teardown, and the
/// qualified allow-list's runtime half. Every policy decision (which
/// program is qualified, which digest is pinned, which bounds hold)
/// stays in the core; this layer only reports measurements.
struct ProcessHost;

impl VerificationHost for ProcessHost {
    fn resolve_program(&self, program: &str) -> Option<std::path::PathBuf> {
        let path = std::env::var_os("PATH")?;
        let executable = if cfg!(windows) {
            format!("{program}.exe")
        } else {
            program.to_owned()
        };
        std::env::split_paths(&path)
            .map(|directory| directory.join(&executable))
            .find(|candidate| candidate.is_file())
    }

    fn run(
        &self,
        program: &std::path::Path,
        args: &[String],
        cwd: &std::path::Path,
    ) -> Result<(Option<i32>, String), String> {
        execute_command(program, args, cwd)
    }
}

/// The bounded child environment: an allow-listed subset of the host
/// names a runtime genuinely needs, everything else cleared. Secrets
/// and unrelated host state never reach a verification child.
const CHILD_ENV_ALLOWED: [&str; 6] = [
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "COMSPEC",
    "PROGRAMFILES",
];

/// One bounded run: `(exit code, bounded stdout)`. Both pipes drain on
/// dedicated threads for the whole child lifetime — a tool emitting
/// more than the pipe buffer can never deadlock the deadline — the
/// child gets its own process group on unix, teardown kills the whole
/// process tree, and the child environment is cleared down to the
/// bounded runtime allow-list. A timeout, a spawn failure, or a signal
/// exit is measured, never invented.
fn execute_command(
    program: &std::path::Path,
    args: &[String],
    cwd: &std::path::Path,
) -> Result<(Option<i32>, String), String> {
    use std::process::{Command, Stdio};
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
    // reaps descendants.
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
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() > std::time::Duration::from_millis(CHECK_TIMEOUT_MS) {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
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
                let remaining = CHECK_OUTPUT_BYTES.saturating_sub(bytes.len());
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

/// Process-tree teardown: the unix child sits in its own process group;
/// Windows walks the tree with the platform tool.
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

/// The `adoption` subcommands.
#[derive(Debug, Subcommand)]
pub enum AdoptionCommands {
    /// Read-only scoped analysis: a minimal starter plan for exactly one
    /// use case, module, or symbol, with per-fact provenance and
    /// confidence, explicit unknowns and ambiguities, and no writes.
    Analyze {
        /// One stable observed or canonical symbol id.
        #[arg(long, value_name = "ID", conflicts_with_all = ["module", "use_case"])]
        symbol: Option<String>,
        /// One exact module id (no wildcards).
        #[arg(long, value_name = "ID", conflicts_with_all = ["symbol", "use_case"])]
        module: Option<String>,
        /// One reviewed use-case scope record id.
        #[arg(long, value_name = "ID", conflicts_with_all = ["symbol", "module"])]
        use_case: Option<String>,
        /// The declared adapter target the analysis pins.
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        /// The declared adapter profile the analysis pins.
        #[arg(long, value_name = "PROFILE")]
        profile: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Selective confirmation: preview the canonical write plan from an
    /// analysis plus explicit selections, or apply exactly a reviewed
    /// plan digest.
    Confirm {
        /// The saved analysis document (preview input).
        #[arg(long, value_name = "FILE", requires = "definition")]
        analysis: Option<String>,
        /// One reviewed definition id (repeatable).
        #[arg(long = "definition", value_name = "ID")]
        definition: Vec<String>,
        /// The binding proposal resolved for the selected definition.
        #[arg(long, value_name = "PROPOSAL_ID", requires = "candidate")]
        binding: Option<String>,
        /// The native candidate the proposal resolves to.
        #[arg(long, value_name = "NATIVE_ID", requires = "binding")]
        candidate: Option<String>,
        /// Preview only; nothing is written.
        #[arg(long, requires = "analysis")]
        dry_run: bool,
        /// The saved confirmation plan and its reviewed digest (apply
        /// input; exclusive with the preview inputs).
        #[arg(long, value_name = "FILE", conflicts_with_all = ["analysis", "dry_run"], requires = "confirm")]
        plan: Option<String>,
        /// The reviewed confirmation plan digest.
        #[arg(long, value_name = "DIGEST", conflicts_with = "dry_run")]
        confirm: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Selective refactor preview: recommendations with reason, files,
    /// risk, and verification for one scope; a supplied reviewed
    /// candidate patch makes the plan applicable.
    Refactor {
        /// One stable symbol id.
        #[arg(long, value_name = "ID", conflicts_with_all = ["module", "use_case"])]
        symbol: Option<String>,
        /// One exact module id.
        #[arg(long, value_name = "ID", conflicts_with_all = ["symbol", "use_case"])]
        module: Option<String>,
        /// One reviewed use-case scope record id.
        #[arg(long, value_name = "ID", conflicts_with_all = ["symbol", "module"])]
        use_case: Option<String>,
        /// The declared adapter target.
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        /// The declared adapter profile.
        #[arg(long, value_name = "PROFILE")]
        profile: Option<String>,
        /// The closed verification specification (required).
        #[arg(long, value_name = "FILE")]
        verification: String,
        /// The supplied reviewed candidate patch (advice only when
        /// absent).
        #[arg(long, value_name = "FILE")]
        candidate: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Really execute the pinned verification: baseline and candidate
    /// runs in disposable copies with the behavior oracle. Read-only
    /// for the project tree.
    Verify {
        /// The saved refactor plan.
        #[arg(long, value_name = "FILE")]
        plan: String,
        /// The closed verification specification.
        #[arg(long, value_name = "FILE")]
        verification: String,
        /// The supplied reviewed candidate patch (required when the
        /// plan carries one).
        #[arg(long, value_name = "FILE")]
        candidate: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Apply one confirmed refactor plan: snapshots, a durable journal,
    /// per-path compare-and-swap writes, and the phase receipt.
    Apply {
        /// The saved refactor plan.
        #[arg(long, value_name = "FILE")]
        plan: String,
        /// The pinned verification specification. Apply executes it
        /// itself through the admitted host capability — an execution
        /// document is a derived record and never install authority.
        #[arg(long, value_name = "FILE")]
        verification: String,
        /// The reviewed candidate patch bytes (required when the plan
        /// carries a patch digest).
        #[arg(long, value_name = "FILE")]
        candidate: Option<String>,
        /// Required explicit confirmation of the exact plan digest.
        #[arg(long, value_name = "DIGEST")]
        confirm: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Roll one completed apply or confirmation back to its exact prior
    /// state; later edits refuse as conflicts.
    Rollback {
        /// The saved apply or confirmation receipt.
        #[arg(long, value_name = "FILE")]
        receipt: String,
        /// Preview the exact restoration without touching bytes.
        #[arg(long)]
        dry_run: bool,
        /// Required explicit confirmation of the exact receipt digest.
        #[arg(long, value_name = "DIGEST", conflicts_with = "dry_run")]
        confirm: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// The durable recovery surface of one plan: the interrupted apply
    /// or confirmation phase's journal with every snapshot's integrity
    /// classified, so a partial mutation is discoverable instead of
    /// invisible. With `--restore`, a stranded installation is actually
    /// restored from the journal's snapshots and pre-states — the
    /// executable path for a receipt failure after committed writes
    /// (no completed receipt required).
    Recovery {
        /// The plan digest (or a saved plan file) to inspect.
        #[arg(long, value_name = "FILE", conflicts_with = "digest")]
        plan: Option<String>,
        /// The exact plan digest to inspect.
        #[arg(long = "digest", value_name = "DIGEST", conflicts_with = "plan")]
        digest: Option<String>,
        /// Restore every journaled write to its exact pre-state.
        #[arg(long, requires = "digest")]
        restore: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// The hybrid coverage join: effective mode, freshness, and gaps
    /// for exactly one selected scope.
    Coverage {
        /// One stable symbol id.
        #[arg(long, value_name = "ID", conflicts_with = "module")]
        symbol: Option<String>,
        /// One exact module id.
        #[arg(long, value_name = "ID", conflicts_with = "symbol")]
        module: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

fn read_input(name: &str, path: &str) -> Result<Vec<u8>, DomainResult> {
    std::fs::read(path).map_err(|error| {
        let detail = match error.kind() {
            std::io::ErrorKind::NotFound => "file-missing",
            _ => "file-unreadable",
        };
        DomainResult::invalid(lekalo_core::adoption::diagnostic_input_invalid(
            name, detail,
        ))
    })
}

fn emit(json: String, human: String) -> DomainResult {
    DomainResult::receipt(json, human)
}

fn selector(
    symbol: Option<String>,
    module: Option<String>,
    use_case: Option<String>,
) -> Result<lekalo_core::adoption::AnalysisScope, DomainResult> {
    match (symbol, module, use_case) {
        (Some(symbol), None, None) => Ok(lekalo_core::adoption::AnalysisScope::Symbol(symbol)),
        (None, Some(module), None) => Ok(lekalo_core::adoption::AnalysisScope::Module(module)),
        (None, None, Some(use_case)) => Ok(lekalo_core::adoption::AnalysisScope::UseCase(use_case)),
        _ => Err(DomainResult::usage_error()),
    }
}

/// Dispatch one `adoption` subcommand.
pub fn run(command: AdoptionCommands) -> DomainResult {
    match command {
        AdoptionCommands::Analyze {
            symbol,
            module,
            use_case,
            target,
            profile,
            project,
        } => {
            let Ok(scope) = selector(symbol, module, use_case) else {
                return DomainResult::usage_error();
            };
            run_analyze(scope, target, profile, &project)
        }
        AdoptionCommands::Confirm {
            analysis,
            definition,
            binding,
            candidate,
            dry_run,
            plan,
            confirm,
            project,
        } => match (plan, confirm) {
            (Some(plan), Some(confirm)) if analysis.is_none() && !dry_run => {
                run_confirm_apply(&plan, &confirm, &project)
            }
            (None, None) if analysis.is_some() && !definition.is_empty() => run_confirm_preview(
                analysis.as_deref().expect("checked"),
                definition,
                binding,
                candidate,
                dry_run,
                &project,
            ),
            _ => DomainResult::usage_error(),
        },
        AdoptionCommands::Refactor {
            symbol,
            module,
            use_case,
            target,
            profile,
            verification,
            candidate,
            project,
        } => {
            let Ok(scope) = selector(symbol, module, use_case) else {
                return DomainResult::usage_error();
            };
            run_refactor(
                scope,
                target,
                profile,
                &verification,
                candidate.as_deref(),
                &project,
            )
        }
        AdoptionCommands::Verify {
            plan,
            verification,
            candidate,
            project,
        } => run_verify(&plan, &verification, candidate.as_deref(), &project),
        AdoptionCommands::Apply {
            plan,
            verification,
            candidate,
            confirm,
            project,
        } => run_apply(
            &plan,
            &verification,
            candidate.as_deref(),
            &confirm,
            &project,
        ),
        AdoptionCommands::Rollback {
            receipt,
            dry_run,
            confirm,
            project,
        } => {
            if dry_run == confirm.is_some() {
                return DomainResult::usage_error();
            }
            run_rollback(&receipt, dry_run, confirm.as_deref(), &project)
        }
        AdoptionCommands::Recovery {
            plan,
            digest,
            restore,
            project,
        } => {
            let resolved = match (plan, digest) {
                (Some(path), _) => {
                    let bytes = match read_input("plan", &path) {
                        Ok(bytes) => bytes,
                        Err(result) => return result,
                    };
                    match lekalo_core::adoption::wire::refactor_plan_of_bytes(&bytes) {
                        Ok(document) => document.plan,
                        // The wire error keeps its own class: a
                        // wrong-version plan stays the exit-5
                        // unsupported-version refusal (X9), never an
                        // invalid-input downgrade.
                        Err(first) => {
                            match lekalo_core::adoption::wire::confirmation_plan_of_bytes(&bytes) {
                                Ok(document) => document.plan,
                                Err(error) => {
                                    return match (first, error) {
                                        (_, second) if second.is_version() => {
                                            second.domain_result()
                                        }
                                        (first, _) if first.is_version() => first.domain_result(),
                                        (_, error) => error.domain_result(),
                                    }
                                }
                            }
                        }
                    }
                }
                (_, Some(digest)) => {
                    if !lekalo_core::adoption::version::is_sha256(&digest) {
                        return DomainResult::invalid(
                            lekalo_core::adoption::diagnostic_input_invalid(
                                "digest",
                                "invalid-digest",
                            ),
                        );
                    }
                    digest
                }
                _ => return DomainResult::usage_error(),
            };
            if restore {
                run_recovery_restore(&resolved, &project)
            } else {
                run_recovery(&resolved, &project)
            }
        }
        AdoptionCommands::Coverage {
            symbol,
            module,
            project,
        } => {
            let scope = match (symbol, module) {
                (Some(symbol), None) => lekalo_core::adoption::AnalysisScope::Symbol(symbol),
                (None, Some(module)) => lekalo_core::adoption::AnalysisScope::Module(module),
                _ => return DomainResult::usage_error(),
            };
            run_coverage(scope, &project)
        }
    }
}

fn run_analyze(
    scope: lekalo_core::adoption::AnalysisScope,
    target: Option<String>,
    profile: Option<String>,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let index = match lekalo_core::observed::context(&selection) {
        Ok(observed) => lekalo_core::observed::index::load_index(&observed)
            .ok()
            .flatten(),
        Err(_) => None,
    };
    match lekalo_core::adoption::analyze(
        &context,
        index.as_ref(),
        &scope,
        target.as_deref(),
        profile.as_deref(),
    ) {
        Ok(document) => emit(
            serde_json::to_string_pretty(&document).expect("adoption document serializes"),
            format!(
                "adoption analyze {} {} : {} definitions ({} confirmed), {} unknowns, {} \
                 ambiguities; no writes",
                scope.wire().kind,
                scope.wire().id,
                document.definitions.len(),
                document
                    .definitions
                    .iter()
                    .filter(|definition| definition.status == "confirmed")
                    .count(),
                document.unknowns.len(),
                document.ambiguities.len()
            ),
        ),
        Err(result) => result,
    }
}

fn run_confirm_preview(
    analysis: &str,
    definitions: Vec<String>,
    binding: Option<String>,
    candidate: Option<String>,
    dry_run: bool,
    project: &Option<String>,
) -> DomainResult {
    let _ = dry_run;
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let observed = match lekalo_core::observed::context(&selection) {
        Ok(observed) => observed,
        Err(result) => return result,
    };
    let index = match lekalo_core::observed::index::load_index(&observed) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    let analysis_bytes = match read_input("analysis", analysis) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let request = lekalo_core::adoption::ConfirmRequest {
        analysis: analysis_bytes,
        definitions,
        binding: binding.zip(candidate),
    };
    match lekalo_core::adoption::confirm_plan(&context, index.as_ref(), &request) {
        Ok(document) => emit(
            serde_json::to_string_pretty(&document).expect("adoption document serializes"),
            format!(
                "adoption confirm : plan {} writes {} definitions ({} unresolved); preview \
                 only, apply with --confirm",
                document.plan,
                document.writes.len(),
                document.unresolved.len()
            ),
        ),
        Err(result) => result,
    }
}

fn run_confirm_apply(plan_path: &str, confirm: &str, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let observed = match lekalo_core::observed::context(&selection) {
        Ok(observed) => observed,
        Err(result) => return result,
    };
    let index = match lekalo_core::observed::index::load_index(&observed) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    let plan_bytes = match read_input("plan", plan_path) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    match lekalo_core::adoption::confirm_apply(&context, index.as_ref(), &plan_bytes, confirm) {
        Ok(receipt) => emit(
            serde_json::to_string_pretty(&receipt).expect("adoption receipt serializes"),
            format!(
                "adoption confirm apply : {} ({} writes); receipt {}",
                receipt.outcome,
                receipt.writes.len(),
                receipt.receipt
            ),
        ),
        Err(result) => result,
    }
}

fn run_refactor(
    scope: lekalo_core::adoption::AnalysisScope,
    target: Option<String>,
    profile: Option<String>,
    verification: &str,
    candidate: Option<&str>,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let observed = match lekalo_core::observed::context(&selection) {
        Ok(observed) => observed,
        Err(result) => return result,
    };
    let index = match lekalo_core::observed::index::load_index(&observed) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    let verification_bytes = match read_input("verification", verification) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let candidate_bytes = match candidate {
        Some(path) => match read_input("candidate", path) {
            Ok(bytes) => Some(bytes),
            Err(result) => return result,
        },
        None => None,
    };
    let request = lekalo_core::adoption::RefactorRequest {
        scope,
        target,
        profile,
        verification: verification_bytes,
        candidate: candidate_bytes,
    };
    match lekalo_core::adoption::refactor_plan(&context, index.as_ref(), &request) {
        Ok(document) => emit(
            serde_json::to_string_pretty(&document).expect("adoption document serializes"),
            format!(
                "adoption refactor {} : plan {} ({} recommendations, {} unsupported, patch: \
                 {}); preview only, decline writes nothing",
                document.selection.id,
                document.plan,
                document.recommendations.len(),
                document.unsupported.len(),
                document.patch.as_deref().unwrap_or("none")
            ),
        ),
        Err(result) => result,
    }
}

fn run_verify(
    plan_path: &str,
    verification: &str,
    candidate: Option<&str>,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    // The observed index rides along for the full pre-execution pin
    // barrier: model/lock/index/adapter members are compared against
    // the plan exactly like apply does (R2).
    let index = match lekalo_core::observed::context(&selection) {
        Ok(observed) => lekalo_core::observed::index::load_index(&observed)
            .ok()
            .flatten(),
        Err(_) => None,
    };
    let plan_bytes = match read_input("plan", plan_path) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let plan = match lekalo_core::adoption::wire::refactor_plan_of_bytes(&plan_bytes) {
        Ok(plan) => plan,
        // A wrong-version plan keeps its exit-5 class through every
        // consumer (X9).
        Err(error) => return error.domain_result(),
    };
    let patch = match (&plan.patch, candidate) {
        (Some(_), Some(path)) => match read_input("candidate", path) {
            Ok(bytes) => {
                let patch = lekalo_core::adoption::wire::patch_of_bytes(&bytes);
                match patch {
                    Ok(patch) => Some(patch),
                    Err(error) => return error.domain_result(),
                }
            }
            Err(result) => return result,
        },
        (Some(_), None) => {
            return DomainResult::invalid(lekalo_core::adoption::diagnostic_input_invalid(
                "candidate",
                "required-by-plan",
            ));
        }
        (None, _) => None,
    };
    let verification_bytes = match read_input("verification", verification) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let host = ProcessHost;
    match lekalo_core::adoption::verify(
        &context,
        &host,
        index.as_ref(),
        &plan,
        &verification_bytes,
        patch.as_ref(),
    ) {
        Ok(execution) => emit(
            serde_json::to_string_pretty(&execution).expect("verification serializes"),
            format!(
                "adoption verify : conclusion {} (baseline {}, candidate {}, oracle {})",
                execution.conclusion.as_deref().unwrap_or("-"),
                execution
                    .baseline
                    .as_ref()
                    .map(|side| side.checks.iter().filter(|c| c.status == "passed").count())
                    .unwrap_or(0),
                execution
                    .candidate
                    .as_ref()
                    .map(|side| side.checks.iter().filter(|c| c.status == "passed").count())
                    .unwrap_or(0),
                execution
                    .oracle
                    .as_ref()
                    .map(|oracle| oracle.status.as_str())
                    .unwrap_or("-")
            ),
        ),
        Err(result) => result,
    }
}

fn run_apply(
    plan_path: &str,
    verification: &str,
    candidate: Option<&str>,
    confirm: &str,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let observed = match lekalo_core::observed::context(&selection) {
        Ok(observed) => observed,
        Err(result) => return result,
    };
    let index = match lekalo_core::observed::index::load_index(&observed) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    let plan_bytes = match read_input("plan", plan_path) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    // The pinned verification specification — not an execution record:
    // apply executes the specification itself through the admitted
    // host capability, so caller-authored evidence can never authorize
    // a write (F11/X1/R1).
    let spec_bytes = match read_input("verification", verification) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let plan = match lekalo_core::adoption::wire::refactor_plan_of_bytes(&plan_bytes) {
        Ok(plan) => plan,
        // A wrong-version plan keeps its exit-5 class through every
        // consumer (X9).
        Err(error) => return error.domain_result(),
    };
    let patch_bytes = match (&plan.patch, candidate) {
        (Some(_), Some(path)) => match read_input("candidate", path) {
            Ok(bytes) => Some(bytes),
            Err(result) => return result,
        },
        (Some(_), None) => {
            return DomainResult::invalid(lekalo_core::adoption::diagnostic_input_invalid(
                "candidate",
                "required-by-plan",
            ));
        }
        (None, _) => None,
    };
    let empty: Vec<u8> = Vec::new();
    let host = ProcessHost;
    match lekalo_core::adoption::apply_refactor(
        &context,
        &host,
        index.as_ref(),
        &plan_bytes,
        patch_bytes.as_deref().unwrap_or(&empty),
        &spec_bytes,
        confirm,
    ) {
        Ok(receipt) => emit(
            serde_json::to_string_pretty(&receipt).expect("adoption receipt serializes"),
            format!(
                "adoption apply : {} ({} writes); receipt {}",
                receipt.outcome,
                receipt.writes.len(),
                receipt.receipt
            ),
        ),
        Err(result) => result,
    }
}

fn run_rollback(
    receipt_path: &str,
    dry_run: bool,
    confirm: Option<&str>,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let receipt_bytes = match read_input("receipt", receipt_path) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    if dry_run {
        match lekalo_core::adoption::rollback_preview(&context, &receipt_bytes) {
            Ok(receipt) => emit(
                serde_json::to_string_pretty(&receipt).expect("adoption receipt serializes"),
                format!(
                    "adoption rollback : would restore {} paths of plan {}; nothing written",
                    receipt.writes.len(),
                    receipt.plan
                ),
            ),
            Err(result) => result,
        }
    } else {
        let Some(confirm) = confirm else {
            return DomainResult::usage_error();
        };
        match lekalo_core::adoption::rollback(&context, &receipt_bytes, confirm) {
            Ok(receipt) => emit(
                serde_json::to_string_pretty(&receipt).expect("adoption receipt serializes"),
                format!(
                    "adoption rollback : {} ({} paths restored)",
                    receipt.outcome,
                    receipt.writes.len()
                ),
            ),
            Err(result) => result,
        }
    }
}

fn run_recovery(plan_digest: &str, project: &Option<String>) -> DomainResult {
    // Root-only resolution (r8, devin F1/omp F3): the view is the
    // diagnostic surface for a stranded journal, and the reader barrier
    // now fails Model loads closed while one exists — so the view must
    // not depend on a full load.
    let selection = selection_for(project);
    let root = match lekalo_core::adoption::recovery_root(&selection) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match lekalo_core::adoption::recovery_view(&root, plan_digest) {
        Ok(view) => emit(
            serde_json::to_string_pretty(&view).expect("recovery view serializes"),
            format!(
                "adoption recovery : plan {} phase {} ({} steps, intact: {})",
                view.plan,
                view.phase.as_deref().unwrap_or("none"),
                view.steps.len(),
                view.intact
            ),
        ),
        Err(result) => result,
    }
}

/// The executable recovery path (X5): restore every journaled write of
/// a stranded apply/confirmation to its exact pre-state, independent of
/// any completed receipt.
fn run_recovery_restore(plan_digest: &str, project: &Option<String>) -> DomainResult {
    // Root-only resolution (r8): the restore resolves the very journal
    // the reader barrier blocks loads on; a full Model load here would
    // deadlock recovery behind its own barrier.
    let selection = selection_for(project);
    let root = match lekalo_core::adoption::recovery_root(&selection) {
        Ok(root) => root,
        Err(result) => return result,
    };
    match lekalo_core::adoption::recover(&root, plan_digest) {
        Ok(outcome) => emit(
            serde_json::to_string_pretty(&outcome).expect("recovery outcome serializes"),
            format!(
                "adoption recovery --restore : plan {} {} ({} paths restored)",
                outcome.plan,
                outcome.outcome,
                outcome.restored.len()
            ),
        ),
        Err(result) => result,
    }
}

fn run_coverage(
    scope: lekalo_core::adoption::AnalysisScope,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::adoption::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let observed = match lekalo_core::observed::context(&selection) {
        Ok(observed) => observed,
        Err(result) => return result,
    };
    let index = match lekalo_core::observed::index::load_index(&observed) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    match lekalo_core::adoption::coverage(&context, index.as_ref(), &scope) {
        Ok(document) => emit(
            serde_json::to_string_pretty(&document).expect("adoption document serializes"),
            format!(
                "adoption coverage {} {} : observed {}, contracted-legacy {}, contracted-new \
                 {}, unbound {}, conflicting {}; {} gaps",
                scope.wire().kind,
                scope.wire().id,
                document.counts.observed,
                document.counts.contracted_legacy,
                document.counts.contracted_new,
                document.counts.unbound,
                document.counts.conflicting,
                document.gaps.len()
            ),
        ),
        Err(result) => result,
    }
}
