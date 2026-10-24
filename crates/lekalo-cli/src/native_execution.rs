//! Production native planning/execution owns its normalized receipt and exit.
use clap::{Args, Subcommand};
use lekalo_core::native_gate::{execution, NativeGateFailure};
use lekalo_core::DomainResult;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Args)]
pub(crate) struct Inputs {
    /// Existing project root. Source is captured without command execution.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Local integrity-manifested, single-file Node adapter package.
    #[arg(long, value_name = "PACKAGE")]
    adapter: PathBuf,
    /// Explicit versioned native execution policy.
    #[arg(long, value_name = "FILE")]
    policy: PathBuf,
    /// Declared bounded Node project launch profile.
    #[arg(long, value_name = "FILE")]
    profile: PathBuf,
    /// Preprovisioned direct-program catalog; no PATH or package manager.
    #[arg(long, value_name = "FILE")]
    catalog: PathBuf,
}

impl Inputs {
    fn core(self) -> execution::ExecutionInputs {
        execution::ExecutionInputs {
            root: self.project,
            adapter: self.adapter,
            policy: self.policy,
            profile: self.profile,
            catalog: self.catalog,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum NativeCommands {
    /// Create a source/control-bound execution plan. Never runs a project command.
    Plan {
        #[command(flatten)]
        inputs: Inputs,
        /// Explicit changed-input document; absent uses the policy's full fallback.
        #[arg(long, value_name = "FILE")]
        changes: Option<PathBuf>,
    },
    /// Run a confirmed plan in a disposable OS-confined view.
    Run {
        #[arg(value_name = "PLAN")]
        plan: String,
        /// Exact digest from native plan. Historical raw plans remain plan-only.
        #[arg(long, requires_all = ["adapter", "policy", "profile", "catalog"])]
        confirm: Option<String>,
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long, requires = "confirm")]
        adapter: Option<PathBuf>,
        #[arg(long, requires = "confirm")]
        policy: Option<PathBuf>,
        #[arg(long, requires = "confirm")]
        profile: Option<PathBuf>,
        #[arg(long, requires = "confirm")]
        catalog: Option<PathBuf>,
        /// Cancel when this file appears. Supports console-free automation.
        #[arg(long, requires = "confirm", value_name = "FILE")]
        cancel_file: Option<PathBuf>,
    },
}

pub(crate) fn read_plan(path: &str) -> Result<Vec<u8>, NativeGateFailure> {
    if path != "-" {
        return execution::read_execution_document(&PathBuf::from(path));
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| NativeGateFailure::PlanInvalid {
            detail: "execution-plan-read",
        })?;
    if bytes.len() > 1024 * 1024 {
        return Err(NativeGateFailure::PlanInvalid {
            detail: "execution-document-limit",
        });
    }
    Ok(bytes)
}

pub(crate) fn run(command: NativeCommands, json: bool) -> u8 {
    match command {
        NativeCommands::Plan { inputs, changes } => {
            let changes = match changes.map(|path| execution::read_execution_document(&path)) {
                Some(Ok(bytes)) => {
                    let parsed = std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(|text| lekalo_core::classification::parse_checked(text).ok())
                        .and_then(|value| serde_json::from_value(value).ok());
                    let Some(changes) = parsed else {
                        return crate::emit(DomainResult::usage_error(), json);
                    };
                    changes
                }
                Some(Err(failure)) => return crate::emit(DomainResult::from(&failure), json),
                None => Default::default(),
            };
            match execution::create_execution_plan(&inputs.core(), changes) {
                Ok(plan) => crate::emit(
                    DomainResult::receipt(
                        serde_json::to_string_pretty(&plan).expect("plan serializes"),
                        format!(
                            "native plan {} ({} commands; read-only)",
                            plan.plan_digest,
                            plan.native_plan.commands.len()
                        ),
                    ),
                    json,
                ),
                Err(failure) => crate::emit(DomainResult::from(&failure), json),
            }
        }
        NativeCommands::Run {
            plan,
            confirm,
            project,
            adapter,
            policy,
            profile,
            catalog,
            cancel_file,
        } => {
            let bytes = match read_plan(&plan) {
                Ok(bytes) => bytes,
                Err(failure) => return crate::emit(DomainResult::from(&failure), json),
            };
            let Some(confirm) = confirm else {
                let outer = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .is_some_and(|value| {
                        value
                            .get("schema_version")
                            .and_then(|v| v.as_str())
                            .is_some_and(|version| {
                                [
                                    "lekalo/native-execution-plan/v0.9.1",
                                    "lekalo/native-execution-plan/v0.9.2",
                                ]
                                .contains(&version)
                            })
                    });
                if outer {
                    let failure = execution::validate_execution_plan(&bytes)
                        .err()
                        .unwrap_or(NativeGateFailure::ApprovalMissing);
                    return crate::emit(DomainResult::from(&failure), json);
                }
                return crate::emit(crate::run_native_run_bytes(&bytes), json);
            };
            let (Some(adapter), Some(policy), Some(profile), Some(catalog)) =
                (adapter, policy, profile, catalog)
            else {
                return crate::emit(DomainResult::usage_error(), json);
            };
            let input = execution::ExecutionInputs {
                root: project,
                adapter,
                policy,
                profile,
                catalog,
            };
            let result =
                execute_report(input, &bytes, &confirm, cancel_file).map(|report| report.run);
            match result {
                Err(failure) => crate::emit(failure, json),
                Ok(receipt) => {
                    let exit = match receipt.outcome.as_str() {
                        "passed" if receipt.verdict == "passed" => 0,
                        "security" => 3,
                        "missing" | "unsupported" | "infrastructure" => 4,
                        _ => 1,
                    };
                    let rendered = if json {
                        serde_json::to_string_pretty(&receipt).expect("receipt serializes")
                    } else {
                        format!(
                            "native run : {} ({} commands; cleanup {}; original {})",
                            receipt.outcome,
                            receipt.commands.len(),
                            receipt.cleanup.state,
                            receipt.original_verification.state
                        )
                    };
                    let output = if exit == 1 {
                        writeln!(std::io::stderr().lock(), "{rendered}")
                    } else {
                        writeln!(std::io::stdout().lock(), "{rendered}")
                    };
                    if output.is_err() {
                        1
                    } else {
                        exit
                    }
                }
            }
        }
    }
}

pub(crate) fn execute_report(
    input: execution::ExecutionInputs,
    bytes: &[u8],
    confirm: &str,
    cancel_file: Option<PathBuf>,
) -> Result<execution::ExecutionReport, DomainResult> {
    let installed = lekalo_core::watch::interrupt::install();
    if !installed && cancel_file.is_none() {
        return Err(DomainResult::from(&NativeGateFailure::CapabilityMissing {
            detail: "native-cancellation-unavailable",
        }));
    }
    let cancellation = lekalo_core::watch::interrupt::cancellation_flag();
    let stop = Arc::new(AtomicBool::new(false));
    let monitor = cancel_file.map(|path| {
        let stopped = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match std::fs::symlink_metadata(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    _ => {
                        cancellation.store(true, Ordering::SeqCst);
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
    });
    let result = execution::run_execution_report(&input, bytes, confirm, Some(cancellation));
    stop.store(true, Ordering::SeqCst);
    if let Some(monitor) = monitor {
        let _ = monitor.join();
    }
    result.map_err(|failure| DomainResult::from(&failure))
}
