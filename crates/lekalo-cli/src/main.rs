//! Issue #3/#7/#8/#9/#10/#11 CLI: clap syntax and presentation. The core
//! owns every decision; this binary selects, renders, and maps exits. Since
//! #11 both renderers project the exact same `DomainResult`.

use clap::{error::ErrorKind, Args, ColorChoice, Parser, Subcommand};
use lekalo_core::loader::LoadSelection;
use lekalo_core::lockfile::plan::LockService;
use lekalo_core::lockfile::resolution::CandidateSet;
use lekalo_core::lockfile::{LockFailure, LockReceipt, LockRequirement, UpdateReceipt};
use lekalo_core::versioning::compatibility::CompatibilityReport;
use lekalo_core::versioning::migration::{MigrationReceipt, MigrationService, VersioningFailure};
use lekalo_core::versioning::{ModelTarget, TargetMalformation, VersionRegistry};
use lekalo_core::DomainResult;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_NAME: &str = "lekalo";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const OUTPUT_FAILURE: u8 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "lekalo",
    version,
    about = "Compiler and validator foundation for Lekalo",
    disable_help_subcommand = true,
    color = ColorChoice::Never
)]
struct Cli {
    /// Emit the stable JSON envelope.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate the semantic layer of a Lekalo project over the typed IR.
    Validate {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Report module-owned diagnostics plus mandatory cross-module
        /// errors; the whole project is still validated.
        #[arg(long, value_name = "MODULE")]
        module: Option<String>,
        /// Select the strict built-in validation profile.
        #[arg(long)]
        strict: bool,
    },
    /// Load YAML/JSON sources, resolve imports, and emit the canonical model.
    Load {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Emit the sourceMap alongside the canonical model or IR (JSON only).
        #[arg(long)]
        spans: bool,
        /// Compile the loaded model into the typed, deterministic Lekalo IR
        /// and emit its canonical bytes instead of the preserved model.
        #[arg(long)]
        ir: bool,
    },
    /// Migrate the Model contract version to a registered target, or roll
    /// back one recorded migration.
    Migrate {
        #[command(flatten)]
        migrate: MigrateArgs,
    },
    /// Print the embedded contract version registry.
    Compatibility,
    /// Inspect one semantic symbol (recognized; implementation follows in a later issue).
    Inspect { symbol: String },
    /// Report the impact of one semantic symbol (recognized; implementation follows later).
    Impact { symbol: String },
    /// Build a bounded context for one semantic symbol (recognized; implementation follows later).
    Context {
        symbol: String,
        #[arg(long, value_name = "TOKENS")]
        budget: u64,
    },
    /// Create the committed project lock, or check an existing one.
    Lock {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Headless CI gate: refuse a missing lock instead of creating it.
        #[arg(long)]
        check: bool,
        /// Forbid any non-local candidate supply at the provider seam.
        #[arg(long)]
        offline: bool,
    },
    /// Preview a deterministic lock update, or apply one exact plan.
    Update {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Compute and print the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Apply the plan with this exact identity (`sha256:<64 hex>`).
        #[arg(long, value_name = "PLAN_ID")]
        apply: Option<String>,
        /// Forbid any non-local candidate supply at the provider seam.
        #[arg(long)]
        offline: bool,
    },
}

/// The `migrate` arguments.
///
/// `--to` and `--rollback` are mutually exclusive and one is required;
/// `--dry-run` is valid only with `--to`. The exclusivity is enforced in
/// [`run_migrate`] so every violation maps to the stable usage failure
/// instead of clap's own exit code.
#[derive(Debug, Args)]
struct MigrateArgs {
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
    /// The target selector: `model/<canonical-version>` or a declared
    /// alias such as `model/v1`.
    #[arg(long, value_name = "TARGET")]
    to: Option<String>,
    /// Compute the plan and semantic diff without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Roll back the recorded migration with this plan identity.
    #[arg(long, value_name = "PLAN_ID")]
    rollback: Option<String>,
}

fn main() -> ExitCode {
    let json_requested = std::env::args_os()
        .skip(1)
        .take_while(|argument| argument != OsStr::new("--"))
        .any(|argument| argument == OsStr::new("--json"));

    let result = match Cli::try_parse() {
        Ok(cli) => match cli.command {
            Commands::Lock {
                project,
                check,
                offline: _,
            } => run_lock(project, check),
            Commands::Update {
                project,
                dry_run,
                apply,
                offline: _,
            } => run_update(project, dry_run, apply),
            Commands::Load { project, spans, ir } => run_load(project, spans, ir),
            Commands::Migrate { migrate } => run_migrate(migrate),
            Commands::Validate {
                project,
                module,
                strict,
            } => run_validate(project, module, strict),
            Commands::Compatibility => run_compatibility(),
            Commands::Inspect { symbol } => lekalo_core::Request::Inspect { symbol }.dispatch(),
            Commands::Impact { symbol } => lekalo_core::Request::Impact { symbol }.dispatch(),
            Commands::Context { symbol, budget } => {
                lekalo_core::Request::Context { symbol, budget }.dispatch()
            }
        },
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp => {
                let ok = error.print().is_ok();
                return if ok {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(OUTPUT_FAILURE)
                };
            }
            ErrorKind::DisplayVersion => DomainResult::version(VERSION),
            _ => DomainResult::usage_error(),
        },
    };
    emit(result, json_requested)
}

/// Emit one domain result on its protocol stream: failures of the invalid
/// and unsupported-version classes render on stderr, everything else on
/// stdout. Human and JSON are projections of the same object.
fn emit(result: DomainResult, json: bool) -> ExitCode {
    let exit_code = result.exit_code();
    let rendered = if json {
        result.to_json_string()
    } else {
        result.to_human_string(PROGRAM_NAME)
    };
    let write_result = if result.writes_stderr() {
        let mut handle = io::stderr().lock();
        handle
            .write_all(rendered.as_bytes())
            .and_then(|()| handle.write_all(b"\n"))
            .and_then(|()| handle.flush())
    } else {
        let mut handle = io::stdout().lock();
        handle
            .write_all(rendered.as_bytes())
            .and_then(|()| handle.write_all(b"\n"))
            .and_then(|()| handle.flush())
    };
    if write_result.is_ok() {
        ExitCode::from(exit_code)
    } else {
        ExitCode::from(OUTPUT_FAILURE)
    }
}

/// Resolve the selection (explicit `--project` beats `LEKALO_PROJECT`) and
/// run the loader; the loader returns the terminal domain result.
fn run_load(project: Option<String>, spans: bool, ir: bool) -> DomainResult {
    let selection = LoadSelection {
        project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    match ir {
        false => lekalo_core::loader::run(&selection, spans),
        true => match lekalo_core::loader::normalize_model(&selection) {
            Err(result) => result,
            Ok(model) => match lekalo_core::ir::compile(&model) {
                Err(failure) => failure.into_result(),
                Ok(compilation) => {
                    let (json, human) = render_ir_success(&model, &compilation, spans);
                    DomainResult::ir(json, human)
                }
            },
        },
    }
}

/// Run `lekalo validate`: load, compile to the typed IR, and run the
/// semantic rules under the selected built-in profile. Loader, IR, and
/// versioning failures pass through untouched; semantic invalidity maps to
/// the invalid envelope (exit 1) and valid outcomes carry only
/// warning/info diagnostics (exit 0).
fn run_validate(project: Option<String>, module: Option<String>, strict: bool) -> DomainResult {
    let selection = LoadSelection {
        project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Err(result) => return result,
        Ok(model) => model,
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Err(failure) => return failure.into_result(),
        Ok(compilation) => compilation,
    };
    let profile = match if strict {
        lekalo_core::validator::ValidationProfile::embedded_strict()
    } else {
        lekalo_core::validator::ValidationProfile::embedded_default()
    } {
        Ok(profile) => profile,
        // The embedded profile is a developer-owned contract; a parse
        // failure fails closed like the registry invariant.
        Err(_) => return DomainResult::invalid(registry_invariant_failure()),
    };
    let outcome = match &module {
        Some(scope) => lekalo_core::validator::validate_scoped(&compilation, profile, scope),
        None => lekalo_core::validator::validate(&compilation, profile),
    };
    match outcome {
        Err(set) => DomainResult::invalid(set),
        Ok(report) => {
            let (json, human) = render_validate_success(&model, &report);
            let diagnostics = report.diagnostics().as_slice().to_vec();
            DomainResult::validation(json, human, diagnostics)
        }
    }
}

/// The fail-closed set for an unusable embedded contract (developer fault).
fn registry_invariant_failure() -> lekalo_core::diagnostics::DiagnosticSet {
    lekalo_core::validator::registry_invariant_failure()
}

/// Render the validation success payload: the fixed key order `status`,
/// `modelVersion`, `validation`. The validation object is the report wire.
fn render_validate_success(
    model: &lekalo_core::loader::NormalizedModel,
    report: &lekalo_core::validator::ValidationReport,
) -> (String, String) {
    let mut json = String::from("{\"status\":\"valid\",\"modelVersion\":");
    json.push_str(
        &serde_json::to_string(model.model_version.as_str()).expect("version serializes"),
    );
    json.push_str(",\"validation\":");
    json.push_str(&report.to_json());
    json.push('}');
    (json, report.to_human())
}

/// Parse the `--to` selector: exactly `model/<version-or-alias>`.
fn parse_model_selector(selector: &str) -> Result<ModelTarget, VersioningFailure> {
    let rest = selector
        .strip_prefix("model/")
        .ok_or(VersioningFailure::InvalidVersion(
            TargetMalformation::Malformed,
        ))?;
    ModelTarget::parse(rest).map_err(VersioningFailure::from_target_error)
}

/// The selection every project-rooted command resolves.
fn selection_for(project: &Option<String>) -> LoadSelection {
    LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    }
}

/// Run `lekalo lock`: create a missing lock, or check an existing one and
/// never update it. `--check` is the headless CI gate.
fn run_lock(project: Option<String>, check: bool) -> DomainResult {
    let selection = selection_for(&project);
    match LockService::lock(
        &selection,
        CandidateSet::empty(),
        LockRequirement::Optional,
        !check,
    ) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            lock_human(&receipt),
        ),
        Err(failure) => DomainResult::from(&failure),
    }
}

/// Run `lekalo update`: `--dry-run` previews the plan, `--apply PLAN_ID`
/// applies that exact plan; a mutating update without a bound preview is
/// refused.
fn run_update(project: Option<String>, dry_run: bool, apply: Option<String>) -> DomainResult {
    if dry_run && apply.is_some() {
        return DomainResult::usage_error();
    }
    if !dry_run && apply.is_none() {
        // A mutating update without a bound preview never ships by accident.
        return DomainResult::from(&LockFailure::PreviewRequired);
    }
    if let Some(plan_id) = &apply {
        if well_formed_plan_id(plan_id).is_none() {
            return DomainResult::usage_error();
        }
    }
    let selection = selection_for(&project);
    match LockService::plan(&selection, CandidateSet::empty()) {
        Err(failure) => DomainResult::from(&failure),
        Ok(prepared) => {
            if dry_run {
                let receipt = LockService::preview(&prepared);
                DomainResult::receipt(
                    serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                    diff_human("preview", &receipt),
                )
            } else {
                let plan_id = apply.as_deref().expect("exclusivity checked above");
                match LockService::apply(prepared, plan_id) {
                    Ok(receipt) => {
                        let verb = if receipt.changed {
                            "applied"
                        } else {
                            "unchanged"
                        };
                        DomainResult::receipt(
                            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                            diff_human(verb, &receipt),
                        )
                    }
                    Err(failure) => DomainResult::from(&failure),
                }
            }
        }
    }
}

/// Accept only the exact `sha256:<64 lowercase hex>` plan spelling.
fn well_formed_plan_id(text: &str) -> Option<()> {
    let hex = text.strip_prefix("sha256:")?;
    let valid = hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
    valid.then_some(())
}

/// The stable component-count projection.
fn counts_suffix(counts: lekalo_core::lockfile::ComponentCounts) -> String {
    format!(
        "adapters {}, generators {}, profiles {}, capabilities {}",
        counts.adapters, counts.generators, counts.profiles, counts.capabilities
    )
}

/// The stable human summary of a lock create/check.
fn lock_human(receipt: &LockReceipt) -> String {
    let verb = if receipt.mode == "create" {
        "created"
    } else {
        "checked"
    };
    format!(
        "lock {} {} resolver {} ({})",
        verb,
        receipt.lock_digest,
        receipt.resolver_version,
        counts_suffix(receipt.counts)
    )
}

/// The stable human summary of an update preview or apply.
fn diff_human(verb: &str, receipt: &UpdateReceipt) -> String {
    let added = receipt.changes.iter().filter(|c| c.from.is_none()).count();
    let removed = receipt.changes.iter().filter(|c| c.to.is_none()).count();
    let changed = receipt.changes.len() - added - removed;
    format!(
        "update {} {} plan {} (+{} ~{} -{})",
        verb, receipt.after_digest, receipt.plan_id, added, changed, removed
    )
}

/// Run one migrate operation (dry-run, apply, or rollback).
fn run_migrate(args: MigrateArgs) -> DomainResult {
    // Argument exclusivity: exactly one of --to / --rollback; --dry-run
    // only alongside --to.
    if (args.to.is_some() && args.rollback.is_some())
        || (args.to.is_none() && args.rollback.is_none())
        || (args.dry_run && args.to.is_none())
        || (args.rollback.is_some() && args.dry_run)
    {
        return VersioningFailure::InvalidVersion(TargetMalformation::Malformed).into();
    }
    let selection = selection_for(&args.project);
    if let Some(plan_id) = args.rollback {
        match MigrationService::rollback(&selection, &plan_id) {
            Ok(receipt) => DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                rollback_human(&receipt),
            ),
            Err(failure) => DomainResult::from(&failure),
        }
    } else {
        let Some(selector) = args.to else {
            unreachable!("exclusivity checked above");
        };
        match parse_model_selector(&selector) {
            Err(failure) => DomainResult::from(&failure),
            Ok(target) => match MigrationService::plan(&selection, target) {
                Err(plan_failure) => DomainResult::from(&plan_failure),
                Ok(prepared) => {
                    if args.dry_run {
                        let receipt = MigrationService::dry_run_receipt(&prepared);
                        DomainResult::receipt(
                            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                            dry_run_human(&receipt),
                        )
                    } else {
                        match MigrationService::apply(prepared) {
                            Ok(receipt) => DomainResult::receipt(
                                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                                apply_human(&receipt),
                            ),
                            Err(failure) => DomainResult::from(&failure),
                        }
                    }
                }
            },
        }
    }
}

/// The stable human summary of a dry-run receipt.
fn dry_run_human(receipt: &MigrationReceipt) -> String {
    format!(
        "migration plan model {} -> {}: {} documents (plan {})",
        receipt.from,
        receipt.to,
        receipt.files.len(),
        receipt.plan_id
    )
}

/// The stable human summary of an applied receipt.
fn apply_human(receipt: &MigrationReceipt) -> String {
    if receipt.changed {
        format!(
            "migrated model {} -> {} (plan {})",
            receipt.from, receipt.to, receipt.plan_id
        )
    } else {
        format!("model already {} (plan {})", receipt.to, receipt.plan_id)
    }
}

/// The stable human summary of a rollback receipt.
fn rollback_human(receipt: &MigrationReceipt) -> String {
    format!(
        "rolled back to model {} (plan {})",
        receipt.to, receipt.plan_id
    )
}

/// Print the embedded registry projection.
fn run_compatibility() -> DomainResult {
    match VersionRegistry::embedded() {
        Ok(registry) => {
            let report = CompatibilityReport::from_registry(registry);
            DomainResult::receipt(
                serde_json::to_string_pretty(&report).expect("receipt serializes"),
                compatibility_human(&report),
            )
        }
        Err(_) => DomainResult::from(&VersioningFailure::RegistryInvalid),
    }
}

/// The stable one-line compatibility summary.
fn compatibility_human(report: &CompatibilityReport) -> String {
    let model = &report.families[0];
    let ir = &report.families[1];
    let protocol = &report.families[2];
    let protocol_summary = match &protocol.current {
        Some(current) => format!("protocol current {current}"),
        None => "protocol unpublished".to_owned(),
    };
    format!(
        "compatibility: model current {} ({}..{}), ir current {}, {}",
        model.current.as_deref().unwrap_or("none"),
        model.min.as_deref().unwrap_or("none"),
        model.max.as_deref().unwrap_or("none"),
        ir.current.as_deref().unwrap_or("none"),
        protocol_summary
    )
}

/// Render the typed IR success payload: the fixed key order `status`,
/// `modelVersion`, `ir`, and the sorted IR sourceMap when `--spans` was
/// requested. The IR object itself is the canonical IR bytes.
fn render_ir_success(
    model: &lekalo_core::loader::NormalizedModel,
    compilation: &lekalo_core::ir::Compilation,
    spans: bool,
) -> (String, String) {
    let mut json = String::from("{\"status\":\"valid\",\"modelVersion\":");
    json.push_str(
        &serde_json::to_string(model.model_version.as_str()).expect("version serializes"),
    );
    json.push_str(",\"ir\":");
    json.push_str(&compilation.project.to_canonical_json());
    if spans {
        json.push_str(",\"sourceMap\":");
        json.push_str(&compilation.source_map.to_json());
    }
    json.push('}');
    let human = format!(
        "compiled ir {}: {} modules, {} definitions",
        lekalo_core::ir::IDENTITY,
        model.modules.len(),
        model.definitions.len()
    );
    (json, human)
}
