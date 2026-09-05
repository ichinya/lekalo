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
    /// Project the deterministic dependency graph of semantic symbols.
    Graph {
        #[command(subcommand)]
        command: GraphCommands,
    },
    /// Project the declared and detected effect graph of operations.
    Effects {
        #[command(subcommand)]
        command: EffectsCommands,
    },
    /// Validate, canonically export, or query one neutral trace manifest.
    Trace {
        #[command(subcommand)]
        command: TraceCommands,
    },
}

/// The `trace` subcommands: a thin handoff to the core trace validator.
/// The manifest document is read at the given path and every decision —
/// wire validation, completeness policy, canonical bytes, queries —
/// lives in the core.
#[derive(Debug, Subcommand)]
enum TraceCommands {
    /// Validate one neutral trace manifest document.
    Validate {
        /// Path to the trace manifest JSON document.
        path: String,
    },
    /// Emit the canonical bytes and digest of one validated manifest.
    Export {
        /// Path to the trace manifest JSON document.
        path: String,
    },
    /// Run one closed forward/reverse query over a validated manifest.
    Query {
        /// Path to the trace manifest JSON document.
        path: String,
        /// The closed selector: `requirements-for:ID`, `symbols-for:ID`,
        /// `artifacts-for:ID`, `tests-for:ID`, `gates-for:ID`,
        /// `diagnostics-for:ID`, or `gaps`.
        selector: String,
    },
}

/// The `effects` subcommands: a thin handoff to the core effect engine.
#[derive(Debug, Subcommand)]
enum EffectsCommands {
    /// Show one operation's declared and detected effects.
    Show {
        /// The semantic id of the operation (or `operation:id`).
        operation: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Report which operations write (or read with `--readers`) one
    /// resource or exact field.
    Writers {
        /// The resource: an entity semantic id, an exact `entity.field`,
        /// or a typed `kind:id` resource reference.
        resource: String,
        /// Answer the reverse-readers view instead of the writers view.
        #[arg(long)]
        readers: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Classify parallel-change conflicts for one explicit change set.
    ///
    /// The changed operations are supplied here as a typed handoff; this
    /// command never parses Git or infers changed symbols (#16 owns that).
    Conflicts {
        /// Comma-separated changed operation ids.
        #[arg(long, value_name = "OPERATIONS")]
        changed: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The `graph` subcommands: a thin handoff to the core graph engine.
#[derive(Debug, Subcommand)]
enum GraphCommands {
    /// Show one symbol with its direct dependencies and dependents.
    Show {
        /// The semantic id of the symbol (or `kind:id`).
        symbol: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Report what depends on one symbol (the reverse view).
    Callers {
        /// The semantic id of the symbol (or `kind:id`).
        symbol: String,
        /// Walk the full reverse closure, not just direct callers.
        #[arg(long)]
        transitive: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Report the shortest dependency path between two symbols.
    Path {
        /// The start semantic id.
        from: String,
        /// The end semantic id.
        to: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Export the whole canonical graph as compact deterministic JSON.
    Export {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The export format; only the closed canonical JSON exists.
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: GraphFormat,
        /// Attach the declaration spans sidecar from the #8 source map.
        #[arg(long)]
        spans: bool,
    },
}

/// The closed graph export format vocabulary.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum GraphFormat {
    /// Canonical compact JSON (the only v1 format).
    Json,
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
            Commands::Graph { command } => run_graph(command),
            Commands::Effects { command } => run_effects(command),
            Commands::Trace { command } => run_trace(command),
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

/// Run one `effects` subcommand: load and compile the project, hand the
/// IR to the core effect engine, and project the result. Every effect
/// decision — projection, evidence attachment, comparison, conflicts,
/// limits, diagnostics — lives in the core; this binary only selects,
/// renders, and maps exits.
fn run_effects(command: EffectsCommands) -> DomainResult {
    match command {
        EffectsCommands::Show { operation, project } => with_effects(&project, |project, graph| {
            effects_show(project, graph, &operation)
        }),
        EffectsCommands::Writers {
            resource,
            readers,
            project,
        } => with_effects(&project, |project, graph| {
            effects_writers(project, graph, &resource, readers)
        }),
        EffectsCommands::Conflicts { changed, project } => {
            with_effects(&project, |project, graph| {
                effects_conflicts(project, graph, &changed)
            })
        }
    }
}

/// Load and compile the selected project, build the effect graph, and run
/// `step`. Load, IR, and effect failures pass through untouched in that
/// order.
fn with_effects(
    project: &Option<String>,
    step: impl FnOnce(
        &lekalo_core::ir::CompiledProject,
        &lekalo_core::effects::EffectGraph,
    ) -> DomainResult,
) -> DomainResult {
    let selection = LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Err(result) => return result,
        Ok(model) => model,
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Err(failure) => return failure.into_result(),
        Ok(compilation) => compilation,
    };
    let graph = match lekalo_core::effects::build(&compilation.project) {
        Err(set) => return DomainResult::invalid(set),
        Ok(graph) => graph,
    };
    step(&compilation.project, &graph)
}

/// Resolve the `effects writers` selector: a typed `kind:id` resource,
/// an entity semantic id, or an exact `entity.field` scope. A selector
/// that names no known definition is an explicit `graph.unknown-node`
/// failure; a known resource with no matching effects is an empty
/// success.
fn effects_selector(
    project: &lekalo_core::ir::CompiledProject,
    selector: &str,
) -> Result<lekalo_core::effects::SubjectSelector, DomainResult> {
    use lekalo_core::effects::{
        FieldName as EffectField, ResourceId, ResourceKind, SubjectSelector,
    };
    let unknown = || DomainResult::invalid(lekalo_core::effects::unknown_subject_set(selector));
    if let Some((kind_key, id)) = selector.split_once(':') {
        let Some(kind) = ResourceKind::from_key(kind_key) else {
            return Err(unknown());
        };
        let Some(resource) = ResourceId::new(kind, id) else {
            return Err(unknown());
        };
        return Ok(SubjectSelector::entity(resource));
    }
    let known_entity = |semantic: &str| {
        project.definitions.iter().any(|definition| {
            definition.kind() == lekalo_core::ir::DefinitionKind::Entity
                && definition.id().as_str() == semantic
        })
    };
    if known_entity(selector) {
        let resource = ResourceId::new(ResourceKind::Canonical, selector).expect("checked id");
        return Ok(SubjectSelector::entity(resource));
    }
    if let Some((entity, field)) = selector.rsplit_once('.') {
        if known_entity(entity) {
            if let (Some(resource), Some(field)) = (
                ResourceId::new(ResourceKind::Canonical, entity),
                EffectField::new(field),
            ) {
                return Ok(SubjectSelector::exact_field(resource, field));
            }
        }
    }
    Err(unknown())
}

/// `lekalo effects show OPERATION`: the operation's declared and
/// detected edges; an unknown operation is an explicit
/// `graph.unknown-node` failure, never an empty success.
fn effects_show(
    project: &lekalo_core::ir::CompiledProject,
    graph: &lekalo_core::effects::EffectGraph,
    operation: &str,
) -> DomainResult {
    let Some(operation) = operation_id(operation) else {
        return DomainResult::invalid(lekalo_core::effects::unknown_subject_set(operation));
    };
    if !graph.knows_operation(&operation)
        && !project_defines_operation(project, operation.semantic_id())
    {
        return DomainResult::invalid(lekalo_core::effects::unknown_subject_set(
            operation.as_str(),
        ));
    }
    let payload = match lekalo_core::effects::canonical::show_payload_bytes(graph, &operation) {
        Ok(payload) => payload,
        Err(set) => return DomainResult::invalid(set),
    };
    let edges = graph.operation_edges(&operation);
    let mut human = vec![format!(
        "effects {} : {} edges",
        operation.as_str(),
        edges.len()
    )];
    for edge in &edges {
        human.push(effect_line("effect", edge));
    }
    effects_envelope(payload, human.join("\n"))
}

/// `lekalo effects writers RESOURCE [--readers]`: the reverse view over
/// one subject selector.
fn effects_writers(
    project: &lekalo_core::ir::CompiledProject,
    graph: &lekalo_core::effects::EffectGraph,
    resource: &str,
    readers: bool,
) -> DomainResult {
    let selector = match effects_selector(project, resource) {
        Ok(selector) => selector,
        Err(result) => return result,
    };
    let payload =
        match lekalo_core::effects::canonical::writers_payload_bytes(graph, &selector, readers) {
            Ok(payload) => payload,
            Err(set) => return DomainResult::invalid(set),
        };
    let edges = if readers {
        graph.readers(&selector)
    } else {
        graph.writers(&selector)
    };
    let edges = match edges {
        Ok(edges) => edges,
        Err(set) => return DomainResult::invalid(set),
    };
    let view = if readers { "readers" } else { "writers" };
    let mut human = vec![format!(
        "{} of {} : {} operations",
        view,
        selector.subject().resource(),
        edges.len()
    )];
    for edge in &edges {
        human.push(effect_line(view, edge));
    }
    effects_envelope(payload, human.join("\n"))
}

/// `lekalo effects conflicts --changed OPS`: classify the explicit
/// change set against the whole graph; never infers changed symbols.
fn effects_conflicts(
    _project: &lekalo_core::ir::CompiledProject,
    graph: &lekalo_core::effects::EffectGraph,
    changed: &str,
) -> DomainResult {
    let mut operations = Vec::new();
    for name in changed.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return DomainResult::usage_error();
        }
        match operation_id(name) {
            Some(operation) => operations.push(operation),
            None => return DomainResult::invalid(lekalo_core::effects::unknown_subject_set(name)),
        }
    }
    let change_set = match lekalo_core::effects::ChangeSet::new(operations) {
        Ok(change_set) => change_set,
        Err(set) => return DomainResult::invalid(set),
    };
    let report = match graph.conflicts(&change_set) {
        Ok(report) => report,
        Err(set) => return DomainResult::invalid(set),
    };
    let payload = match lekalo_core::effects::canonical::conflicts_payload_bytes(&report) {
        Ok(payload) => payload,
        Err(set) => return DomainResult::invalid(set),
    };
    let mut human = vec![format!(
        "conflicts for {} changed operations : {} conflicts{}",
        change_set.changed().len(),
        report.items().len(),
        if report.complete() { "" } else { " (bounded)" }
    )];
    for item in report.items() {
        human.push(format!(
            "  {} {} x {} on {}",
            item.classification().key(),
            item.left(),
            item.right(),
            item.subject()
        ));
    }
    effects_envelope(payload, human.join("\n"))
}

/// Parse one bare semantic id or `operation:id` wire form.
fn operation_id(text: &str) -> Option<lekalo_core::effects::OperationId> {
    if text.contains(':') {
        lekalo_core::effects::OperationId::from_qualified(text)
    } else {
        lekalo_core::effects::OperationId::from_semantic(text)
    }
}

/// Whether the project defines one operation symbol.
fn project_defines_operation(project: &lekalo_core::ir::CompiledProject, semantic: &str) -> bool {
    project.definitions.iter().any(|definition| {
        matches!(
            definition,
            lekalo_core::ir::Definition::Command(_) | lekalo_core::ir::Definition::Query(_)
        ) && definition.id().as_str() == semantic
    })
}

/// The accepted effects success envelope: the fixed key order `status`,
/// `effects`. Human and JSON are projections of the same result.
fn effects_envelope(payload: String, human: String) -> DomainResult {
    DomainResult::graph(
        format!("{{\"status\":\"valid\",\"effects\":{}}}", payload),
        human,
        Vec::new(),
    )
}

/// One stable human effect line: `kind operation -> resource[.field] (confidence)`.
fn effect_line(label: &str, edge: &lekalo_core::effects::EffectEdge) -> String {
    let key = edge.key();
    let subject = match key.subject().field() {
        Some(field) => format!("{}#{}", key.subject().resource(), field),
        None => key.subject().resource().to_string(),
    };
    format!(
        "  {label} {} {} -> {} ({})",
        key.kind(),
        key.operation(),
        subject,
        edge.confidence().as_str()
    )
}

/// Run one `graph` subcommand: load and compile the project, hand the IR
/// to the core graph engine, and project the result. Every graph decision
/// — construction, traversal, cycle policy, limits, diagnostics — lives in
/// the core; this binary only selects, renders, and maps exits.
fn run_graph(command: GraphCommands) -> DomainResult {
    match command {
        GraphCommands::Show { symbol, project } => {
            with_graph(&project, &[], |graph, _| graph_show(graph, &symbol))
        }
        GraphCommands::Callers {
            symbol,
            transitive,
            project,
        } => with_graph(&project, &[], |graph, _| {
            graph_callers(graph, &symbol, transitive)
        }),
        GraphCommands::Path { from, to, project } => {
            with_graph(&project, &[], |graph, _| graph_path(graph, &from, &to))
        }
        GraphCommands::Export {
            project,
            format: GraphFormat::Json,
            spans,
        } => with_graph(&project, &["graph export"], |graph, compilation| {
            graph_export(graph, compilation, spans)
        }),
    }
}

/// Load and compile the selected project, build the graph, and run `step`.
/// Load, IR, and graph failures pass through untouched in that order.
fn with_graph(
    project: &Option<String>,
    steps: &[&str],
    render: impl FnOnce(
        &lekalo_core::graph::DependencyGraph,
        &lekalo_core::ir::Compilation,
    ) -> DomainResult,
) -> DomainResult {
    let _ = steps;
    let selection = LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Err(result) => return result,
        Ok(model) => model,
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Err(failure) => return failure.into_result(),
        Ok(compilation) => compilation,
    };
    let graph = match lekalo_core::graph::build(&compilation.project) {
        Err(set) => return DomainResult::invalid(set),
        Ok(graph) => graph,
    };
    render(&graph, &compilation)
}

/// `lekalo graph show SYMBOL`: the node card plus direct dependencies and
/// dependents; an unknown symbol is an explicit `graph.unknown-node`
/// failure, never an empty success.
fn graph_show(graph: &lekalo_core::graph::DependencyGraph, symbol: &str) -> DomainResult {
    let Some(node) = graph.resolve(symbol) else {
        return DomainResult::invalid(lekalo_core::graph::diagnostic::unknown_node_set(symbol));
    };
    let filter = lekalo_core::graph::EdgeFilter::new();
    let dependencies = graph.direct_dependencies(node.id(), &filter);
    let dependents = graph.reverse_dependencies(node.id(), &filter);
    let json = format!(
        "{{\"status\":\"valid\",\"graph\":{}}}",
        lekalo_core::graph::canonical::show_payload_bytes(node, &dependencies, &dependents)
    );
    let mut human = vec![format!("{} {}", node.kind(), node.id().semantic_id())];
    for edge in &dependencies {
        human.push(edge_line("dependency", edge));
    }
    for edge in &dependents {
        human.push(edge_line("dependent", edge));
    }
    DomainResult::graph(json, human.join("\n"), Vec::new())
}

/// `lekalo graph callers SYMBOL`: the reverse view — direct by default,
/// the full bounded reverse closure with `--transitive`.
fn graph_callers(
    graph: &lekalo_core::graph::DependencyGraph,
    symbol: &str,
    transitive: bool,
) -> DomainResult {
    let Some(node) = graph.resolve(symbol) else {
        return DomainResult::invalid(lekalo_core::graph::diagnostic::unknown_node_set(symbol));
    };
    let filter = lekalo_core::graph::EdgeFilter::new();
    if transitive {
        let spec = lekalo_core::graph::TraversalSpec::new(lekalo_core::graph::Direction::Reverse)
            .with_filter(filter);
        let traversal = match graph.transitive(node.id(), &spec) {
            Err(set) => return DomainResult::invalid(set),
            Ok(traversal) => traversal,
        };
        let json = format!(
            "{{\"status\":\"valid\",\"graph\":{}}}",
            lekalo_core::graph::canonical::callers_payload_bytes(
                node,
                traversal.edges(),
                traversal.complete()
            )
        );
        let mut human = vec![format!(
            "{} {} (transitive, depth {})",
            node.kind(),
            node.id().semantic_id(),
            traversal.max_depth_seen()
        )];
        for edge in traversal.edges() {
            human.push(edge_line("caller", edge));
        }
        DomainResult::graph(json, human.join("\n"), Vec::new())
    } else {
        let callers: Vec<lekalo_core::graph::GraphEdge> = graph
            .reverse_dependencies(node.id(), &filter)
            .into_iter()
            .cloned()
            .collect();
        let json = format!(
            "{{\"status\":\"valid\",\"graph\":{}}}",
            lekalo_core::graph::canonical::callers_payload_bytes(node, callers.as_slice(), true)
        );
        let mut human = vec![format!("{} {}", node.kind(), node.id().semantic_id())];
        for edge in &callers {
            human.push(edge_line("caller", edge));
        }
        DomainResult::graph(json, human.join("\n"), Vec::new())
    }
}

/// `lekalo graph path FROM TO`: the deterministic shortest path; no path
/// is an explicit `graph.path-not-found` failure.
fn graph_path(graph: &lekalo_core::graph::DependencyGraph, from: &str, to: &str) -> DomainResult {
    let Some(from_node) = graph.resolve(from) else {
        return DomainResult::invalid(lekalo_core::graph::diagnostic::unknown_node_set(from));
    };
    let Some(to_node) = graph.resolve(to) else {
        return DomainResult::invalid(lekalo_core::graph::diagnostic::unknown_node_set(to));
    };
    let spec = lekalo_core::graph::PathSpec::new();
    let path = match graph.shortest_path(from_node.id(), to_node.id(), &spec) {
        Err(set) => return DomainResult::invalid(set),
        Ok(path) => path,
    };
    let json = format!(
        "{{\"status\":\"valid\",\"graph\":{}}}",
        lekalo_core::graph::canonical::path_payload_bytes(&path)
    );
    let mut human = vec![format!(
        "path {} -> {} ({} hops, {})",
        from_node.id(),
        to_node.id(),
        path.length(),
        path.confidence().as_str()
    )];
    for (position, edge) in path.edges().iter().enumerate() {
        let _ = position;
        human.push(edge_line("hop", edge));
    }
    DomainResult::graph(json, human.join("\n"), Vec::new())
}

/// `lekalo graph export`: the canonical graph bytes in the success
/// envelope, with the optional declaration-span sidecar.
fn graph_export(
    graph: &lekalo_core::graph::DependencyGraph,
    compilation: &lekalo_core::ir::Compilation,
    spans: bool,
) -> DomainResult {
    let bytes = match graph.to_canonical_json() {
        Err(set) => return DomainResult::invalid(set),
        Ok(bytes) => bytes,
    };
    let mut json = String::from("{\"status\":\"valid\",\"graph\":");
    json.push_str(&bytes);
    if spans {
        json.push_str(",\"spans\":");
        json.push_str(&lekalo_core::graph::canonical::spans_sidecar_bytes(
            graph,
            compilation.source_map.entries(),
        ));
    }
    json.push('}');
    let human = format!(
        "built graph {}: {} nodes, {} edges",
        graph.identity(),
        graph.nodes().len(),
        graph.edges().len()
    );
    DomainResult::graph(json, human, Vec::new())
}

/// One stable human edge line: `relation from -> to (confidence)`.
fn edge_line(label: &str, edge: &lekalo_core::graph::GraphEdge) -> String {
    let key = edge.key();
    format!(
        "  {label} {} {} -> {} ({})",
        key.relation(),
        key.from(),
        key.to(),
        edge.confidence().as_str()
    )
}

/// Run one `trace` subcommand: read the manifest document bytes and hand
/// them to the core trace validator. Every trace decision — wire
/// validation, completeness policy, canonical export, queries — lives in
/// the core; this binary only reads the file, selects, renders, and maps
/// exits onto the accepted 0/1 envelope.
fn run_trace(command: TraceCommands) -> DomainResult {
    let (path, step) = match command {
        TraceCommands::Validate { path } => (path, TraceStep::Validate),
        TraceCommands::Export { path } => (path, TraceStep::Export),
        TraceCommands::Query { path, selector } => (path, TraceStep::Query(selector)),
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                io::ErrorKind::PermissionDenied => "file-unreadable",
                _ => "file-unreadable",
            };
            return DomainResult::invalid(lekalo_core::trace::io_failure(detail));
        }
    };
    let manifest = match lekalo_core::trace::TraceManifest::parse(&bytes) {
        Ok(manifest) => manifest,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    match step {
        TraceStep::Validate => trace_validate(&manifest),
        TraceStep::Export => trace_export(&manifest),
        TraceStep::Query(selector) => trace_query(&manifest, &selector),
    }
}

/// The selected trace operation, resolved before the file is read.
enum TraceStep {
    Validate,
    Export,
    Query(String),
}

/// `lekalo trace validate PATH`: the accepted success envelope carries the
/// manifest summary; partial manifests report their uncovered sinks
/// explicitly, never silently.
fn trace_validate(manifest: &lekalo_core::trace::TraceManifest) -> DomainResult {
    let report = manifest.report();
    let mut uncovered = String::new();
    if !report.uncovered_sinks.is_empty() {
        uncovered = format!(
            ",\"uncoveredSinks\":[{}]",
            report
                .uncovered_sinks
                .iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    let json = format!(
        "{{\"status\":\"valid\",\"trace\":{{\"manifestId\":\"{}\",\"projectRef\":\"{}\",\"completeness\":\"{}\",\"exportProfile\":\"{}\",\"sourceRevision\":\"{}\",\"nodeCount\":{},\"relationCount\":{},\"gapCount\":{},\"uncoveredSinkCount\":{}{}}}}}",
        report.manifest_id,
        report.project_ref,
        report.completeness.as_str(),
        report.export_profile.as_str(),
        report.source_revision,
        report.node_count,
        report.relation_count,
        report.gap_count,
        report.uncovered_sinks.len(),
        uncovered
    );
    let human = format!(
        "trace manifest {}\n#   completeness {} ({})\n#   nodes {}; relations {}; gaps {}; uncovered sinks {}",
        report.manifest_id,
        report.completeness.as_str(),
        report.export_profile.as_str(),
        report.node_count,
        report.relation_count,
        report.gap_count,
        report.uncovered_sinks.len()
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo trace export PATH`: the canonical bytes are the export. Human
/// output is exactly the canonical bytes (the renderer adds the final
/// LF); JSON output embeds the same bytes as a value plus the digest.
fn trace_export(manifest: &lekalo_core::trace::TraceManifest) -> DomainResult {
    let canonical = match manifest.canonical_bytes() {
        Ok(canonical) => canonical,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let digest = match manifest.digest() {
        Ok(digest) => digest,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let json =
        format!("{{\"status\":\"valid\",\"trace\":{canonical},\"manifestDigest\":\"{digest}\"}}");
    DomainResult::graph(json, canonical, Vec::new())
}

/// `lekalo trace query PATH SELECTOR`: the closed forward/reverse
/// selectors answered from the normalized relation index; `gaps` projects
/// the explicit gap list.
fn trace_query(manifest: &lekalo_core::trace::TraceManifest, selector: &str) -> DomainResult {
    let selection = match lekalo_core::trace::QuerySelection::parse(selector) {
        Ok(selection) => selection,
        Err(_) => {
            return DomainResult::invalid(lekalo_core::trace::io_failure("selector-invalid"));
        }
    };
    if selection == lekalo_core::trace::QuerySelection::Gaps {
        return trace_gaps(manifest);
    }
    let rows = match manifest.query(&selection) {
        Ok(rows) => rows,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let mut rows_json = String::new();
    let mut human = String::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            rows_json.push(',');
            human.push('\n');
        }
        rows_json.push_str(&format!(
            "{{\"id\":\"{}\",\"nodeId\":\"{}\",\"relation\":\"{}\",\"occurrence\":\"{}\",\"status\":\"{}\",\"confidence\":\"{}\"}}",
            row.id,
            row.node_id,
            row.relation.as_str(),
            row.occurrence,
            row.status.as_str(),
            row.confidence.as_str()
        ));
        human.push_str(&format!(
            "{} {} {} {} {}/{}",
            matched_kind_label(&selection),
            row.id,
            row.relation.as_str(),
            row.occurrence,
            row.status.as_str(),
            row.confidence.as_str()
        ));
    }
    let (name, target) = (selection.name(), selection.target().unwrap_or_default());
    let json = format!(
        "{{\"status\":\"valid\",\"trace\":{{\"selector\":\"{name}\",\"target\":\"{target}\",\"rows\":[{rows_json}]}}}}"
    );
    let human = if rows.is_empty() {
        format!("{name} {target}: no matches")
    } else {
        human
    };
    DomainResult::graph(json, human, Vec::new())
}

/// The explicit gap projection: every gap with its kind, status, anchor,
/// and expectation, in canonical order — a missing link is never silent.
fn trace_gaps(manifest: &lekalo_core::trace::TraceManifest) -> DomainResult {
    let gaps = &manifest.manifest().gaps;
    let mut rows_json = String::new();
    let mut human = String::new();
    for (index, gap) in gaps.iter().enumerate() {
        if index > 0 {
            rows_json.push(',');
            human.push('\n');
        }
        let anchor = gap.anchor_node.as_deref().unwrap_or_default();
        let expected = gap.expected.as_deref().unwrap_or_default();
        rows_json.push_str(&format!(
            "{{\"gapKind\":\"{}\",\"status\":\"{}\",\"anchorNode\":{},\"expected\":{}}}",
            gap.gap_kind.as_str(),
            gap.status.as_str(),
            quoted_or_null(gap.anchor_node.as_deref()),
            quoted_or_null(gap.expected.as_deref())
        ));
        let _ = anchor;
        let _ = expected;
        human.push_str(&format!(
            "gap {} {}",
            gap.gap_kind.as_str(),
            gap.status.as_str()
        ));
        if let Some(anchor) = gap.anchor_node.as_deref() {
            human.push_str(&format!(" anchor={anchor}"));
        }
        if let Some(expected) = gap.expected.as_deref() {
            human.push_str(&format!(" expected={expected}"));
        }
    }
    let json = format!(
        "{{\"status\":\"valid\",\"trace\":{{\"selector\":\"gaps\",\"rows\":[{rows_json}]}}}}"
    );
    let human = if gaps.is_empty() {
        "gaps: none".to_owned()
    } else {
        human
    };
    DomainResult::graph(json, human, Vec::new())
}

fn quoted_or_null(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{value}\""),
        None => "null".to_owned(),
    }
}

/// The stable human label for one matched query row.
fn matched_kind_label(selection: &lekalo_core::trace::QuerySelection) -> &'static str {
    match selection {
        lekalo_core::trace::QuerySelection::RequirementsFor(_) => "requirement",
        lekalo_core::trace::QuerySelection::SymbolsFor(_) => "symbol",
        lekalo_core::trace::QuerySelection::ArtifactsFor(_) => "artifact",
        lekalo_core::trace::QuerySelection::TestsFor(_) => "native_test",
        lekalo_core::trace::QuerySelection::GatesFor(_) => "gate",
        lekalo_core::trace::QuerySelection::DiagnosticsFor(_) => "diagnostic",
        lekalo_core::trace::QuerySelection::Gaps => "gap",
    }
}
