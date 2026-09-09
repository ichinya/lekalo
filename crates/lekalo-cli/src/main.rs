//! Issue #3/#7/#8/#9/#10/#11 CLI: clap syntax and presentation. The core
//! owns every decision; this binary selects, renders, and maps exits. Since
//! #11 both renderers project the exact same `DomainResult`.

use clap::{error::ErrorKind, Args, ColorChoice, Parser, Subcommand};
use lekalo_core::artifacts::{ArtifactFailure, CheckReceipt, GenerateService};

mod git_input;
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

/// The `impact` argument surface: one symbol root or the typed
/// `--changed` selector family, plus the bounded filters.
#[derive(Debug, Args)]
struct ImpactArgs {
    /// The semantic id of the symbol (exclusive with `--changed`).
    symbol: Option<String>,
    /// Analyze the changed inputs instead of one symbol.
    #[arg(long, conflicts_with = "symbol")]
    changed: bool,
    /// The committed read-only base revision.
    #[arg(long, value_name = "REF", requires = "changed")]
    base: Option<String>,
    /// The committed read-only candidate revision (exclusive with
    /// `--worktree`).
    #[arg(
        long,
        value_name = "REF",
        requires = "changed",
        conflicts_with = "worktree"
    )]
    head: Option<String>,
    /// Select the current index/worktree candidate (exclusive with
    /// `--head`).
    #[arg(long, requires = "changed")]
    worktree: bool,
    /// The traversal depth.
    #[arg(long, value_name = "DEPTH", default_value_t = 3)]
    depth: u16,
    /// Narrow the traversal to one module semantic id.
    #[arg(long, value_name = "MODULE")]
    module: Option<String>,
    /// Narrow the target projections to one target id.
    #[arg(long, value_name = "TARGET")]
    target: Option<String>,
    /// Admit only these relations (repeatable).
    #[arg(long, value_name = "KIND")]
    relation: Vec<String>,
    /// The closed impact profile.
    #[arg(long, value_name = "PROFILE", default_value = "default")]
    profile: String,
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
}

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

    /// Bypass every cache read, write, and lock; outputs match a clean
    /// full rebuild and no runtime files are created or touched.
    #[arg(long, global = true)]
    no_cache: bool,

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
    /// Inspect one semantic symbol: identity, contract, effects,
    /// relations, and bounded projections in one deterministic view.
    Inspect {
        /// A full semantic id (`planner.focus_task`) or a safe short
        /// name (`focus_task`).
        symbol: String,
        /// Comma-separated optional projections: `bindings`, `scenarios`.
        #[arg(long, value_name = "SECTIONS")]
        include: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Report the impact and change radius of one semantic symbol, or of
    /// the changed inputs of a Git revision range or the working tree.
    Impact {
        #[command(flatten)]
        args: ImpactArgs,
    },
    /// Build a bounded context capsule for one semantic symbol, or for the
    /// explicitly supplied changed symbols. The human projection of the
    /// valid result is the agent-facing Markdown; `--json` emits the
    /// structured capsule (`lekalo/context/v1.0.0`).
    Context {
        /// The semantic id of the symbol (or `kind:id`); exactly one of
        /// this and `--changed` is required.
        symbol: Option<String>,
        /// Comma-separated changed symbol ids (a typed handoff; this
        /// command never parses Git or infers changed symbols).
        #[arg(long, value_name = "SYMBOLS")]
        changed: Option<String>,
        /// The token budget of the capsule; an over-budget capsule is
        /// emitted with explicit truncation metadata instead of an error.
        #[arg(long, value_name = "TOKENS")]
        budget: u64,
        /// Attach the declaration-span sidecar (logical project-relative
        /// paths only) as the opt-in source-evidence path.
        #[arg(long)]
        spans: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
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
    /// Compare two accepted project selections semantically.
    Diff {
        /// The base selector, or the base when `--base` names the base.
        #[arg(value_name = "OLD")]
        first: String,
        /// The candidate selector when `--base` names the base.
        #[arg(value_name = "NEW")]
        second: Option<String>,
        /// The base project selector (`--base OLD NEW`).
        #[arg(long, value_name = "DIR")]
        base: Option<String>,
        /// Comma-separated built-in compatibility profiles.
        #[arg(long, value_name = "PROFILES")]
        profiles: Option<String>,
        /// The output format; only the closed canonical JSON exists.
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: DiffFormat,
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
    /// Resolve one requirements attachment against its project: the
    /// read-only OpenSpec requirement traceability integration.
    Requirements {
        #[command(subcommand)]
        command: RequirementsCommands,
    },
    /// Check generated-artifact ownership and drift, or plan and apply a
    /// confirmed clean of orphaned generated files.
    Generate {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Read-only drift check: verify the ownership manifest against
        /// the exact lock, inputs, adapters, and bytes; writes nothing.
        #[arg(long)]
        check: bool,
        /// Plan (with --dry-run) or apply (with --confirm) the
        /// deterministic clean of orphaned generated files.
        #[arg(long)]
        clean: bool,
        /// Compute the clean plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Apply the clean plan with this exact identity
        /// (`sha256:<64 lowercase hex>`).
        #[arg(long, value_name = "PLAN_ID")]
        confirm: Option<String>,
    },
    /// Adopt an existing repository: detection, a minimal canonical
    /// skeleton, and the no-overwrite adoption plan (issue #38).
    Init {
        /// Adopt the existing repository at the adoption root.
        #[arg(long)]
        adopt: bool,
        /// Explicit target selection; written to `lekalo/targets/`.
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        /// Explicit adapter profile selection; recorded with the target
        /// document and receipt. Requires `--target`.
        #[arg(long, value_name = "PROFILE")]
        profile: Option<String>,
        /// Explicit canonical project id when derivation is ambiguous.
        #[arg(long, value_name = "ID")]
        project_id: Option<String>,
        /// Adoption root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Print the full adoption plan without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the target adapter conformance suite (issue #31).
    Adapter {
        #[command(subcommand)]
        command: AdapterCommands,
    },
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
    /// Record, bind, verify, and promote existing code in observed mode
    /// (issue #39). The core owns every decision; this binary only
    /// selects, renders, and maps exits.
    Observe {
        #[command(subcommand)]
        command: ObserveCommands,
    },
}

/// The `adapter` subcommands: the issue #31 conformance suite handoff.
/// The core owns every decision; this layer selects, renders, and maps
/// exits.
#[derive(Debug, Subcommand)]
enum AdapterCommands {
    /// Run the conformance battery against one adapter executable.
    ///
    /// Everything after the program path is passed to the adapter
    /// verbatim (no shell), so adapter flags come last.
    Test {
        /// The closed battery profile.
        #[arg(long, value_name = "PROFILE", default_value = "default")]
        profile: AdapterTestProfile,
        /// Print this report document on stdout for every completed
        /// run, regardless of the verdict.
        #[arg(long, value_name = "FORMAT")]
        report: Option<AdapterTestReport>,
        /// Repetition count of every determinism probe.
        #[arg(
            long,
            value_name = "N",
            default_value_t = lekalo_core::adapter_conformance::DEFAULT_REPEATS
        )]
        repeats: u8,
        /// Per-exchange adapter deadline in milliseconds.
        #[arg(
            long,
            value_name = "MS",
            default_value_t = lekalo_core::adapter_conformance::DEFAULT_TIMEOUT_MS
        )]
        timeout_ms: u64,
        /// The adapter program and its arguments, spawned directly.
        #[arg(trailing_var_arg = true)]
        program_args: Vec<String>,
    },
}

/// The closed conformance battery profile vocabulary.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum AdapterTestProfile {
    /// The default battery.
    Default,
    /// The strict battery: the complete operation surface is required.
    Strict,
}

impl From<AdapterTestProfile> for lekalo_core::adapter_conformance::Profile {
    fn from(profile: AdapterTestProfile) -> Self {
        match profile {
            AdapterTestProfile::Default => Self::Default,
            AdapterTestProfile::Strict => Self::Strict,
        }
    }
}

/// The closed report format vocabulary.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum AdapterTestReport {
    /// The deterministic JSON envelope with the embedded report.
    Json,
    /// The deterministic JUnit XML document.
    Junit,
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

/// The `requirements` subcommands: a thin handoff to the core
/// requirements resolver. The attachment document is read at the given
/// path and every decision — wire validation, Model pin custody,
/// provider resolution, coverage, impact, trace projection — lives in
/// the core. Nothing is ever written.
#[derive(Debug, Subcommand)]
enum RequirementsCommands {
    /// Validate the attachment and gate every requirement reference:
    /// stale, missing, or conflicted references deny the gate.
    Validate {
        /// Path to the requirements attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Emit the canonical resolution report: catalog, coverage gaps,
    /// conflicts, and changed-requirement impact.
    Report {
        /// Path to the requirements attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Run one closed query over the resolution report.
    Query {
        /// Path to the requirements attachment JSON document.
        path: String,
        /// The closed selector: `report`, `coverage-gaps`, `impact`,
        /// `symbol:ID`, or `requirement:SOURCE:ID`.
        selector: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Emit the neutral #22 trace-manifest projection of the resolved
    /// requirements.
    Trace {
        /// Path to the requirements attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The `cache` subcommands: the thin status/clear handoff (issue #20).
#[derive(Debug, Subcommand)]
enum CacheCommands {
    /// Print the read-only, bounded cache health projection.
    Status {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Clear the cache home (`.lekalo/cache/**` except migration state).
    Clear {
        /// Required explicit confirmation; the CI-safe invocation form.
        #[arg(long)]
        yes: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The `observe` subcommands: the observed-mode surface (issue #39).
#[derive(Debug, Subcommand)]
enum ObserveCommands {
    /// Merge one adapter scan document into the observed index; a
    /// binding recorded under a stable key survives a source move.
    Update {
        /// The adapter scan document (JSON), relative to the invocation
        /// directory.
        #[arg(long, value_name = "FILE")]
        scan: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Bind one recorded symbol to a source location explicitly.
    Bind {
        /// The recorded semantic id.
        symbol: String,
        /// The adapter stable key that survives file moves.
        #[arg(long, value_name = "KEY")]
        key: Option<String>,
        /// The logical project-relative source path.
        #[arg(long, value_name = "PATH")]
        path: String,
        /// The 1-based source line.
        #[arg(long, value_name = "LINE")]
        line: Option<u64>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Confirm one inferred binding; confirmed facts are user-owned.
    Confirm {
        /// The recorded semantic id.
        symbol: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Run the staleness gate over every recorded binding; any stale
    /// binding fails with registered diagnostics.
    Check {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Attach native tests and gates to one recorded symbol.
    Attach {
        /// The recorded semantic id.
        symbol: String,
        /// Comma-separated native test ids (verbatim external ids).
        #[arg(long, value_name = "IDS")]
        native_test: Option<String>,
        /// Comma-separated gate ids (verbatim external ids).
        #[arg(long, value_name = "IDS")]
        gate: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Project the observed card of one recorded symbol.
    Inspect {
        /// The recorded semantic id.
        symbol: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Project the observed impact of one recorded symbol; the recorded
    /// graph always reports its own incompleteness.
    Impact {
        /// The recorded semantic id.
        symbol: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Plan (with --dry-run) or apply (with --confirm) the explicit
    /// promotion of observed symbols into the canonical model.
    Promote {
        /// One semantic id to promote.
        #[arg(long, value_name = "SYMBOL")]
        symbol: Option<String>,
        /// Every eligible symbol of one module.
        #[arg(long, value_name = "MODULE")]
        module: Option<String>,
        /// Compute the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Apply the plan with this exact identity
        /// (`sha256:<64 lowercase hex>`).
        #[arg(long, value_name = "PLAN_ID")]
        confirm: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The closed diff output format vocabulary.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum DiffFormat {
    /// Canonical compact JSON (the only v1 format).
    Json,
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
            Commands::Load { project, spans, ir } => run_load(project, spans, ir, cli.no_cache),
            Commands::Migrate { migrate } => run_migrate(migrate),
            Commands::Validate {
                project,
                module,
                strict,
            } => run_validate(project, module, strict, cli.no_cache),
            Commands::Compatibility => run_compatibility(),
            Commands::Inspect {
                symbol,
                include,
                project,
            } => run_inspect(&symbol, include.as_deref(), &project),
            Commands::Impact { args } => run_impact(args),
            Commands::Context {
                symbol,
                changed,
                budget,
                spans,
                project,
            } => run_context(symbol, changed, budget, spans, &project),
            Commands::Diff {
                first,
                second,
                base,
                profiles,
                format: DiffFormat::Json,
            } => run_diff(first, second, base, profiles),
            Commands::Graph { command } => run_graph(command, cli.no_cache),
            Commands::Effects { command } => run_effects(command, cli.no_cache),
            Commands::Trace { command } => run_trace(command),
            Commands::Requirements { command } => run_requirements(command),
            Commands::Generate {
                project,
                check,
                clean,
                dry_run,
                confirm,
            } => run_generate(project, check, clean, dry_run, confirm),
            Commands::Cache { command } => run_cache(command),
            Commands::Observe { command } => run_observe(command),
            Commands::Init {
                adopt,
                target,
                profile,
                project_id,
                project,
                dry_run,
            } => run_init(adopt, target, profile, project_id, project, dry_run),
            Commands::Adapter { command } => match run_adapter(command) {
                AdapterRun::Envelope(result) => result,
                AdapterRun::Document { document, result } => {
                    // The requested report document owns stdout for
                    // every completed run; a failing verdict still
                    // renders its envelope on the status-owned stream.
                    let exit = result.exit_code();
                    let write_ok = write_stdout(&document);
                    if result.writes_stderr() {
                        let _ = write_stderr(&result.to_json_string());
                    }
                    return if write_ok {
                        ExitCode::from(exit)
                    } else {
                        ExitCode::from(OUTPUT_FAILURE)
                    };
                }
            },
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
fn run_load(project: Option<String>, spans: bool, ir: bool, no_cache: bool) -> DomainResult {
    let selection = LoadSelection {
        project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    match ir {
        false => lekalo_core::cache::run_load(&selection, spans, no_cache),
        true => match lekalo_core::cache::load_compiled(&selection, no_cache) {
            Err(result) => result,
            Ok((model, compilation)) => {
                let (json, human) = render_ir_success(&model, &compilation, spans);
                DomainResult::ir(json, human)
            }
        },
    }
}

/// Run `lekalo diff`: load and compile two accepted project selections,
/// hand both typed IR values to the core semantic diff, and project the
/// result. Load and IR failures pass through untouched in selector
/// order; the diff itself never reads Git, YAML text, adapters, or the
/// filesystem beyond the accepted #7 selection.
fn run_diff(
    first: String,
    second: Option<String>,
    base: Option<String>,
    profiles: Option<String>,
) -> DomainResult {
    // Exactly one base and one candidate: either two positionals or
    // `--base OLD` plus one positional. Over- or under-specification is
    // the stable usage failure, never a guessed selector.
    let (base_selector, candidate_selector) = match (&second, &base) {
        (Some(second), None) => (first.clone(), second.clone()),
        (None, Some(base)) => (base.clone(), first.clone()),
        _ => return DomainResult::usage_error(),
    };
    let request = match profiles {
        None => lekalo_core::diff::DiffRequest::new(),
        Some(terms) => {
            let parsed = match lekalo_core::diff::parse_profile_terms(&terms) {
                Ok(parsed) => parsed,
                Err(set) => return DomainResult::invalid(set),
            };
            let mut request = lekalo_core::diff::DiffRequest::new();
            for profile in parsed {
                request = request.with_profile(profile);
            }
            request
        }
    };
    let base_selection = LoadSelection {
        project: Some(base_selector),
    };
    let base_compilation = match compile_selection(&base_selection) {
        Ok(compilation) => compilation,
        Err(result) => return result,
    };
    let candidate_selection = LoadSelection {
        project: Some(candidate_selector),
    };
    let candidate_compilation = match compile_selection(&candidate_selection) {
        Ok(compilation) => compilation,
        Err(result) => return result,
    };
    let outcome = lekalo_core::diff::compare(
        &base_compilation.project,
        &candidate_compilation.project,
        &request,
    );
    let result = match outcome {
        Err(set) => return DomainResult::invalid(set),
        Ok(result) => result,
    };
    let payload = match result.to_canonical_json() {
        Ok(bytes) => bytes,
        Err(set) => return DomainResult::invalid(set),
    };
    let mut human = vec![result.to_human()];
    for profile in result.profiles() {
        human.push(format!(
            "  profile {} : {}",
            profile.profile_id().key(),
            profile.verdict().key()
        ));
    }
    for change in result.changes() {
        human.push(format!(
            "  {} {} ({})",
            change.kind().key(),
            change.subject(),
            change.reasons().join(", ")
        ));
    }
    DomainResult::diff(
        format!("{{\"status\":\"valid\",\"diff\":{payload}}}"),
        human.join("\n"),
        Vec::new(),
    )
}

/// Load and compile one selection; a loader or IR failure becomes the
/// terminal domain result.
fn compile_selection(
    selection: &LoadSelection,
) -> Result<lekalo_core::ir::Compilation, DomainResult> {
    let model = lekalo_core::loader::normalize_model(selection)?;
    let compilation = match lekalo_core::ir::compile(&model) {
        Err(failure) => return Err(failure.into_result()),
        Ok(compilation) => compilation,
    };
    Ok(compilation)
}

/// Run `lekalo validate`: load, compile to the typed IR, and run the
/// semantic rules under the selected built-in profile. Loader, IR, and
/// versioning failures pass through untouched; semantic invalidity maps to
/// the invalid envelope (exit 1) and valid outcomes carry only
/// warning/info diagnostics (exit 0).
fn run_validate(
    project: Option<String>,
    module: Option<String>,
    strict: bool,
    no_cache: bool,
) -> DomainResult {
    let selection = LoadSelection {
        project: project.or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let (model, compilation) = match lekalo_core::cache::load_compiled(&selection, no_cache) {
        Err(result) => return result,
        Ok(pair) => pair,
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
            // Authorization review (#25): reference integrity is
            // invalid in every profile; the strict profile blocks
            // uncovered protected effects, stale model pins, and
            // non-full adapter mapping states (denied, exit 3).
            match lekalo_core::authorization::review_selection(&selection, &compilation, strict) {
                Err(result) => return result,
                Ok(lekalo_core::authorization::Review::Invalid(set)) => {
                    return DomainResult::invalid(set);
                }
                Ok(lekalo_core::authorization::Review::Denied(set)) => {
                    return DomainResult::denied(set);
                }
                Ok(lekalo_core::authorization::Review::Ok) => {}
            }
            let (json, human) = render_validate_success(&model, &report);
            let diagnostics = report.diagnostics().as_slice().to_vec();
            DomainResult::validation(json, human, diagnostics)
        }
    }
}

/// Run `lekalo inspect`: load and compile the project, build the graph
/// and effect projections, and hand everything to the core inspect
/// engine. Every inspect decision — selector grammar, resolution,
/// ambiguity, bounds, section states — lives in the core; this binary
/// only selects, renders, and maps exits. A malformed `--include`
/// value is a stable usage failure before any project is loaded.
fn run_inspect(symbol: &str, include: Option<&str>, project: &Option<String>) -> DomainResult {
    let include = match include {
        None => lekalo_core::inspect::Include::default(),
        Some(text) => match lekalo_core::inspect::Include::parse(text) {
            Some(include) => include,
            None => return DomainResult::usage_error(),
        },
    };
    // Grammar-validate the selector before any project discovery: a
    // malformed selector is a pure usage failure with no echo.
    if let Err(set) = lekalo_core::inspect::validate_selector(symbol) {
        return DomainResult::invalid(set);
    }
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
    let effects = match lekalo_core::effects::build(&compilation.project) {
        Err(set) => return DomainResult::invalid(set),
        Ok(effects) => effects,
    };
    let request = lekalo_core::inspect::InspectRequest {
        selector: symbol.to_owned(),
        include,
    };
    match lekalo_core::inspect::run(&compilation, &graph, &effects, &request) {
        Err(set) => DomainResult::invalid(set),
        Ok(outcome) => DomainResult::graph(
            format!("{{\"status\":\"valid\",\"inspect\":{}}}", outcome.json),
            outcome.human,
            Vec::new(),
        ),
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

/// Run `lekalo init --adopt`: the thin handoff to the core adoption
/// service. Greenfield `init` is not part of issue #38 and stays the
/// stable usage failure until its own issue lands.
fn run_init(
    adopt: bool,
    target: Option<String>,
    profile: Option<String>,
    project_id: Option<String>,
    project: Option<String>,
    dry_run: bool,
) -> DomainResult {
    if !adopt {
        return DomainResult::usage_error();
    }
    if let Some(target) = target.as_deref() {
        if !lekalo_core::init::detect::valid_target_id(target) {
            return DomainResult::usage_error();
        }
    }
    // A profile selects within one explicit target: an orphan or malformed
    // profile is the stable usage failure before any plan or write.
    if let Some(profile) = profile.as_deref() {
        if target.is_none() || !lekalo_core::target_protocol::scopes::is_token(profile) {
            return DomainResult::usage_error();
        }
    }
    if let Some(project_id) = project_id.as_deref() {
        if !lekalo_core::init::valid_project_id(project_id) {
            return DomainResult::usage_error();
        }
    }
    lekalo_core::init::adopt(&lekalo_core::init::AdoptRequest {
        project,
        target,
        profile,
        project_id,
        dry_run,
    })
}

/// The terminal output of `lekalo adapter test`: either the standard
/// envelope, or an explicitly requested report document that owns
/// stdout for every completed run.
enum AdapterRun {
    Envelope(DomainResult),
    Document {
        /// The exact report document bytes.
        document: String,
        /// The verdict result carrying the exit class.
        result: DomainResult,
    },
}

/// Run `lekalo adapter test`: the thin handoff to the issue #31
/// conformance engine. The core owns every decision; this layer only
/// selects the battery, renders the report, and maps exits.
fn run_adapter(command: AdapterCommands) -> AdapterRun {
    let AdapterCommands::Test {
        profile,
        report,
        repeats,
        timeout_ms,
        program_args,
    } = command;
    let Some((program, args)) = program_args.split_first() else {
        return AdapterRun::Envelope(DomainResult::usage_error());
    };
    if program.is_empty() {
        return AdapterRun::Envelope(DomainResult::usage_error());
    }
    let command = lekalo_core::target_protocol::transport::AdapterCommand {
        program: std::path::PathBuf::from(program),
        args: args.to_vec(),
    };
    let options = lekalo_core::adapter_conformance::SuiteOptions {
        profile: profile.into(),
        repeats,
        timeout_ms,
    };
    match lekalo_core::adapter_conformance::run(&command, &options) {
        Ok(outcome) => match report {
            None => AdapterRun::Envelope(outcome.domain_result()),
            Some(AdapterTestReport::Json) => AdapterRun::Document {
                document: outcome.envelope_json(),
                result: outcome.domain_result(),
            },
            Some(AdapterTestReport::Junit) => AdapterRun::Document {
                document: outcome.junit(),
                result: outcome.domain_result(),
            },
        },
        Err(error) => AdapterRun::Envelope(
            lekalo_core::adapter_conformance::infrastructure_result(error),
        ),
    }
}

/// Write one document to stdout with the trailing newline protocol.
fn write_stdout(document: &str) -> bool {
    use std::io::Write;
    let mut handle = io::stdout().lock();
    handle
        .write_all(document.as_bytes())
        .and_then(|()| {
            if document.ends_with('\n') {
                handle.flush()
            } else {
                handle.write_all(b"\n").and_then(|()| handle.flush())
            }
        })
        .is_ok()
}

/// Write one envelope to stderr with the trailing newline protocol.
fn write_stderr(envelope: &str) -> bool {
    use std::io::Write;
    let mut handle = io::stderr().lock();
    handle
        .write_all(envelope.as_bytes())
        .and_then(|()| handle.write_all(b"\n"))
        .and_then(|()| handle.flush())
        .is_ok()
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
fn run_effects(command: EffectsCommands, no_cache: bool) -> DomainResult {
    match command {
        EffectsCommands::Show { operation, project } => {
            with_effects(&project, no_cache, |project, graph| {
                effects_show(project, graph, &operation)
            })
        }
        EffectsCommands::Writers {
            resource,
            readers,
            project,
        } => with_effects(&project, no_cache, |project, graph| {
            effects_writers(project, graph, &resource, readers)
        }),
        EffectsCommands::Conflicts { changed, project } => {
            with_effects(&project, no_cache, |project, graph| {
                effects_conflicts(project, graph, &changed)
            })
        }
    }
}

/// Run one `cache` subcommand: the read-only health projection, or the
/// explicit-confirmation clear of the governed cache home.
fn run_cache(command: CacheCommands) -> DomainResult {
    match command {
        CacheCommands::Status { project } => lekalo_core::cache::status(&selection_for(&project)),
        CacheCommands::Clear { yes, project } => {
            if !yes {
                // The explicit confirmation is part of the accepted
                // surface; there is no implicit clear.
                return DomainResult::usage_error();
            }
            lekalo_core::cache::clear(&selection_for(&project))
        }
    }
}

/// Load and compile the selected project, build the effect graph, and run
/// `step`. Load, IR, and effect failures pass through untouched in that
/// order.
fn with_effects(
    project: &Option<String>,
    no_cache: bool,
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
    let session = match lekalo_core::cache::Session::open(&selection, no_cache) {
        Err(result) => return result,
        Ok(session) => session,
    };
    let (_model, compilation) = match session.load_compiled(&selection) {
        Err(result) => return result,
        Ok(pair) => pair,
    };
    let graph = match lekalo_core::effects::build(&compilation.project) {
        Err(set) => return DomainResult::invalid(set),
        Ok(graph) => graph,
    };
    session.record_effects(&graph);
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

/// Run `lekalo context`: load and compile the project, hand the IR to the
/// core context engine, and project the one normalized capsule to both
/// renderers (Markdown on the human stream, the structured capsule inside
/// the JSON envelope). Load and IR failures pass through untouched;
/// capsule input failures are the stable `invalid` envelope.
fn run_context(
    symbol: Option<String>,
    changed: Option<String>,
    budget: u64,
    spans: bool,
    project: &Option<String>,
) -> DomainResult {
    let scope = match (symbol, changed) {
        (Some(symbol), None) => lekalo_core::context::CapsuleScope::Symbol(symbol),
        (None, Some(changed)) => {
            let mut roots = Vec::new();
            for name in changed.split(',') {
                let name = name.trim();
                if name.is_empty() {
                    return DomainResult::usage_error();
                }
                roots.push(name.to_owned());
            }
            lekalo_core::context::CapsuleScope::Changed(roots)
        }
        (Some(_), Some(_)) | (None, None) => return DomainResult::usage_error(),
    };
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
    match lekalo_core::context::plan(&scope, budget, spans, &compilation) {
        Err(set) => DomainResult::invalid(set),
        Ok(capsule) => DomainResult::graph(
            format!(
                "{{\"status\":\"valid\",\"context\":{}}}",
                capsule.to_canonical_json()
            ),
            capsule.to_markdown(),
            Vec::new(),
        ),
    }
}

/// Run one `graph` subcommand: load and compile the project, hand the IR
/// to the core graph engine, and project the result. Every graph decision
/// — construction, traversal, cycle policy, limits, diagnostics — lives in
/// the core; this binary only selects, renders, and maps exits.
fn run_graph(command: GraphCommands, no_cache: bool) -> DomainResult {
    match command {
        GraphCommands::Show { symbol, project } => {
            with_graph(&project, no_cache, |graph, _| graph_show(graph, &symbol))
        }
        GraphCommands::Callers {
            symbol,
            transitive,
            project,
        } => with_graph(&project, no_cache, |graph, _| {
            graph_callers(graph, &symbol, transitive)
        }),
        GraphCommands::Path { from, to, project } => {
            with_graph(&project, no_cache, |graph, _| graph_path(graph, &from, &to))
        }
        GraphCommands::Export {
            project,
            format: GraphFormat::Json,
            spans,
        } => with_graph(&project, no_cache, |graph, compilation| {
            graph_export(graph, compilation, spans)
        }),
    }
}

/// Load and compile the selected project, build the graph, record the
/// graph fragment, and run `render`.
fn with_graph(
    project: &Option<String>,
    no_cache: bool,
    render: impl FnOnce(
        &lekalo_core::graph::DependencyGraph,
        &lekalo_core::ir::Compilation,
    ) -> DomainResult,
) -> DomainResult {
    let selection = LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let session = match lekalo_core::cache::Session::open(&selection, no_cache) {
        Err(result) => return result,
        Ok(session) => session,
    };
    let (_model, compilation) = match session.load_compiled(&selection) {
        Err(result) => return result,
        Ok(pair) => pair,
    };
    let graph = match lekalo_core::graph::build(&compilation.project) {
        Err(set) => return DomainResult::invalid(set),
        Ok(graph) => graph,
    };
    session.record_graph(&compilation, &graph);
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

/// The selected requirements operation, resolved before the attachment
/// is read.
enum RequirementsStep {
    Validate,
    Report,
    Query(String),
    Trace,
}

/// Run one `lekalo requirements` operation: parse the attachment,
/// resolve it against the selected project (loader and IR failures pass
/// through unchanged), and project the requested view. The core owns
/// every decision; this binary only selects, renders, and maps exits.
fn run_requirements(command: RequirementsCommands) -> DomainResult {
    let (path, project, step) = match command {
        RequirementsCommands::Validate { path, project } => {
            (path, project, RequirementsStep::Validate)
        }
        RequirementsCommands::Report { path, project } => (path, project, RequirementsStep::Report),
        RequirementsCommands::Query {
            path,
            selector,
            project,
        } => (path, project, RequirementsStep::Query(selector)),
        RequirementsCommands::Trace { path, project } => (path, project, RequirementsStep::Trace),
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return DomainResult::invalid(lekalo_core::requirements::io_failure(detail));
        }
    };
    let attachment = match lekalo_core::requirements::RequirementsAttachment::parse(&bytes) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let selection = selection_for(&project);
    let resolution = match attachment.resolve(&selection) {
        Ok(resolution) => resolution,
        Err(result) => return result,
    };
    match step {
        RequirementsStep::Validate => requirements_validate(&resolution),
        RequirementsStep::Report => requirements_report(&resolution.report),
        RequirementsStep::Query(selector) => requirements_query(&resolution.report, &selector),
        RequirementsStep::Trace => requirements_trace(&resolution.report),
    }
}

/// `lekalo requirements validate`: the accepted summary envelope carries
/// the resolution counts; the gate denies on any stale, missing, or
/// conflicted reference.
fn requirements_validate(resolution: &lekalo_core::requirements::Resolution) -> DomainResult {
    use lekalo_core::requirements::ResolutionVerdict;
    let report = &resolution.report;
    let fresh = report
        .references
        .iter()
        .filter(|row| row.status == "fresh")
        .count();
    let stale = report
        .references
        .iter()
        .filter(|row| row.status == "stale")
        .count();
    let missing = report
        .references
        .iter()
        .filter(|row| row.status == "missing")
        .count();
    let conflicted = report
        .references
        .iter()
        .filter(|row| row.status == "conflict")
        .count();
    let json = format!(
        "{{\"status\":\"valid\",\"requirements\":{{\"projectId\":\"{}\",\"sourceRevision\":\"{}\",\"requirementCount\":{},\"referenceCount\":{},\"fresh\":{},\"stale\":{},\"missing\":{},\"conflict\":{},\"coverageGaps\":{},\"conflicts\":{}}}}}",
        report.project_id,
        report.source_revision,
        report.requirements.len(),
        report.references.len(),
        fresh,
        stale,
        missing,
        conflicted,
        report.coverage_gaps.len(),
        report.conflicts.len(),
    );
    let human = format!(
        "requirements {}\n#   requirements {}; references {}; fresh {}; stale {}; \
         missing {}; conflict {}; coverage gaps {}; conflicts {}",
        report.project_id,
        report.requirements.len(),
        report.references.len(),
        fresh,
        stale,
        missing,
        conflicted,
        report.coverage_gaps.len(),
        report.conflicts.len(),
    );
    match &resolution.verdict {
        ResolutionVerdict::Pass => DomainResult::graph(json, human, Vec::new()),
        ResolutionVerdict::Denied(diagnostics) => DomainResult::denied(diagnostics.clone()),
    }
}

/// `lekalo requirements report`: the canonical report bytes are the
/// export; JSON output embeds the same bytes as a value plus the digest.
fn requirements_report(report: &lekalo_core::requirements::Report) -> DomainResult {
    let canonical = match report.canonical_bytes() {
        Ok(canonical) => canonical,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let digest = match report.digest() {
        Ok(digest) => digest,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let json =
        format!("{{\"status\":\"valid\",\"report\":{canonical},\"reportDigest\":\"{digest}\"}}");
    DomainResult::graph(json, canonical, Vec::new())
}

/// `lekalo requirements trace`: the neutral #22 trace-manifest
/// projection, validated by the accepted trace validator and emitted as
/// canonical bytes with their digest.
fn requirements_trace(report: &lekalo_core::requirements::Report) -> DomainResult {
    let manifest = match report.trace_manifest() {
        Ok(manifest) => manifest,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
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

/// One closed requirements query selector.
enum RequirementsSelection {
    /// The whole report.
    Report,
    /// Requirements no symbol links.
    CoverageGaps,
    /// The changed-requirement impact rows.
    Impact,
    /// Every reference of one symbol (the reverse lookup).
    Symbol(String),
    /// One catalog requirement and its references.
    Requirement(String, String),
}

/// Parse one closed requirements query selector.
fn requirements_selection(selector: &str) -> Option<RequirementsSelection> {
    if selector == "report" {
        return Some(RequirementsSelection::Report);
    }
    if selector == "coverage-gaps" {
        return Some(RequirementsSelection::CoverageGaps);
    }
    if selector == "impact" {
        return Some(RequirementsSelection::Impact);
    }
    if let Some(symbol) = selector.strip_prefix("symbol:") {
        return Some(RequirementsSelection::Symbol(symbol.to_owned()));
    }
    if let Some(rest) = selector.strip_prefix("requirement:") {
        let (source, requirement) = rest.split_once(':')?;
        return Some(RequirementsSelection::Requirement(
            source.to_owned(),
            requirement.to_owned(),
        ));
    }
    None
}

/// `lekalo requirements query`: the closed selectors answered from the
/// resolved report; an unknown selector or subject is the stable usage
/// or unknown failure, never an empty success.
fn requirements_query(report: &lekalo_core::requirements::Report, selector: &str) -> DomainResult {
    let selection = match requirements_selection(selector) {
        Some(selection) => selection,
        None => return DomainResult::usage_error(),
    };
    let render = |rows: serde_json::Value, human: String| {
        let json = format!(
            "{{\"status\":\"valid\",\"requirements\":{}}}",
            serde_json::to_string(&rows).unwrap_or_else(|_| "null".to_owned())
        );
        DomainResult::graph(json, human, Vec::new())
    };
    match selection {
        RequirementsSelection::Report => requirements_report(report),
        RequirementsSelection::CoverageGaps => {
            let human = report
                .coverage_gaps
                .iter()
                .map(|row| format!("gap {}:{} {}", row.source, row.id, row.digest))
                .collect::<Vec<_>>()
                .join("\n");
            render(
                serde_json::json!({ "coverageGaps": report.coverage_gaps }),
                human,
            )
        }
        RequirementsSelection::Impact => {
            let human = report
                .impact
                .iter()
                .map(|row| {
                    let renamed = row
                        .renamed_to
                        .as_deref()
                        .map(|id| format!(" -> {id}"))
                        .unwrap_or_default();
                    format!(
                        "impact {}:{} {}{} ({} symbol(s))",
                        row.source,
                        row.requirement,
                        row.change,
                        renamed,
                        row.symbols.len()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            render(serde_json::json!({ "impact": report.impact }), human)
        }
        RequirementsSelection::Symbol(symbol) => {
            let rows: Vec<&lekalo_core::requirements::ReferenceRow> = report
                .references
                .iter()
                .filter(|row| row.symbol == symbol)
                .collect();
            if rows.is_empty() {
                return DomainResult::invalid(lekalo_core::requirements::io_failure(
                    "unknown-subject",
                ));
            }
            let human = rows
                .iter()
                .map(|row| {
                    format!(
                        "requirement {}:{} {} {}",
                        row.source, row.requirement, row.relation, row.status
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            render(serde_json::json!({ "references": rows }), human)
        }
        RequirementsSelection::Requirement(source, requirement) => {
            let Some(row) = report
                .requirements
                .iter()
                .find(|row| row.source == source && row.id == requirement)
            else {
                return DomainResult::invalid(lekalo_core::requirements::io_failure(
                    "unknown-subject",
                ));
            };
            let human = format!(
                "requirement {}:{} {} symbols {}",
                row.source,
                row.id,
                row.digest,
                row.symbols.len()
            );
            render(serde_json::json!({ "requirement": row }), human)
        }
    }
}

/// Run `lekalo generate`: `--check` is the read-only drift gate,
/// `--clean --dry-run` previews the deterministic clean plan, and
/// `--clean --confirm sha256:<planId>` applies exactly that plan.
fn run_generate(
    project: Option<String>,
    check: bool,
    clean: bool,
    dry_run: bool,
    confirm: Option<String>,
) -> DomainResult {
    // Exactly one mode; the clean modifiers belong to --clean only; a
    // mutating clean needs a bound preview identity, never a bare run.
    if check == clean || (!clean && (dry_run || confirm.is_some())) {
        return DomainResult::usage_error();
    }
    if clean {
        if dry_run == confirm.is_some() {
            if dry_run {
                return DomainResult::usage_error();
            }
            // A mutating clean without a bound preview never ships by accident.
            return DomainResult::from(&ArtifactFailure::PreviewRequired);
        }
        if let Some(plan_id) = &confirm {
            if well_formed_plan_id(plan_id).is_none() {
                return DomainResult::usage_error();
            }
        }
    }
    let selection = selection_for(&project);
    if check {
        match GenerateService::check(&selection) {
            Ok(receipt) => DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                check_human(&receipt),
            ),
            Err(failure) => DomainResult::from(&failure),
        }
    } else if dry_run {
        match GenerateService::clean_plan(&selection) {
            Ok(receipt) => DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                clean_human("preview", &receipt.plan_id, receipt.count),
            ),
            Err(failure) => DomainResult::from(&failure),
        }
    } else {
        let plan_id = confirm.as_deref().expect("exclusivity checked above");
        match GenerateService::clean_apply(&selection, plan_id) {
            Ok(receipt) => {
                let verb = if receipt.changed {
                    "applied"
                } else {
                    "unchanged"
                };
                DomainResult::receipt(
                    serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                    clean_human(verb, &receipt.plan_id, receipt.deleted),
                )
            }
            Err(failure) => DomainResult::from(&failure),
        }
    }
}

/// The stable human summary of a drift check.
fn check_human(receipt: &CheckReceipt) -> String {
    format!(
        "generate check {} manifest {} lock {} (artifacts {}, clean {}, \
         stale {}, manual-drift {}, missing {}, orphan {}, reported {})",
        receipt.verdict,
        receipt.manifest_digest.as_deref().unwrap_or("none"),
        receipt.lock_digest,
        receipt.counts.artifacts,
        receipt.counts.clean,
        receipt.counts.stale,
        receipt.counts.manual_drift,
        receipt.counts.missing,
        receipt.counts.orphan,
        receipt.counts.reported,
    )
}

/// The stable human summary of a clean preview or apply.
fn clean_human(verb: &str, plan_id: &str, count: usize) -> String {
    format!("generate {verb} plan {} (-{count})", plan_id)
}
/// The project root path the impact Git adapter runs in: the explicit
/// selector, `LEKALO_PROJECT`, or the invocation directory.
fn impact_project_root(project: &Option<String>) -> std::path::PathBuf {
    std::path::PathBuf::from(
        project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok())
            .unwrap_or_else(|| ".".to_owned()),
    )
}

/// The source-path index of one compilation: logical path to its sorted
/// unique definition ids (the changed-input resolution surface).
fn source_paths_of(
    compilation: &lekalo_core::ir::Compilation,
) -> std::collections::HashMap<&str, Vec<&str>> {
    let mut index: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for entry in compilation.source_map.entries() {
        let Some(symbol) = &entry.semantic_id else {
            continue;
        };
        let symbols = index.entry(entry.path.as_str()).or_default();
        if !symbols.contains(&symbol.as_str()) {
            symbols.push(symbol.as_str());
        }
    }
    for symbols in index.values_mut() {
        symbols.sort();
    }
    index
}

/// Run `lekalo impact`: load and compile the project, build the graph and
/// effect projections, resolve the changed inputs when `--changed` was
/// selected, and hand everything to the core analyzer. Every impact
/// decision — traversal, risks, gates, limits, diagnostics — lives in the
/// core; this binary only selects, renders, and maps exits.
fn run_impact(args: ImpactArgs) -> DomainResult {
    let ImpactArgs {
        symbol,
        changed,
        base,
        head,
        worktree,
        depth,
        module,
        target,
        relation,
        profile,
        project,
    } = args;
    // Selector semantics the flags alone cannot express.
    if !changed && (base.is_some() || head.is_some() || worktree) {
        return DomainResult::invalid(lekalo_core::impact::diagnostic::selector_invalid_set(
            "selector-requires-changed",
        ));
    }
    let profile = match lekalo_core::impact::ImpactProfile::parse(&profile) {
        Some(profile) => profile,
        None => {
            return DomainResult::invalid(lekalo_core::impact::diagnostic::selector_invalid_set(
                "unknown-profile",
            ))
        }
    };
    let request = if changed {
        lekalo_core::impact::ImpactRequest::for_changed()
    } else {
        match &symbol {
            Some(symbol) => match lekalo_core::impact::ImpactRequest::for_symbol(symbol) {
                Ok(request) => request,
                Err(set) => return DomainResult::invalid(set),
            },
            None => return DomainResult::usage_error(),
        }
    };
    let request = match request
        .with_depth(depth)
        .and_then(|request| request.with_relations(&relation))
    {
        Ok(request) => request,
        Err(set) => return DomainResult::invalid(set),
    };
    let request = match if let Some(module) = &module {
        request.with_module(module)
    } else {
        Ok(request)
    } {
        Ok(request) => request,
        Err(set) => return DomainResult::invalid(set),
    };
    let request = match if let Some(target) = &target {
        request.with_target(target)
    } else {
        Ok(request)
    } {
        Ok(request) => request,
        Err(set) => return DomainResult::invalid(set),
    };
    let request = request.with_profile(profile);

    // Load and compile; load, IR, and projection failures pass through.
    let selection = selection_for(&project);
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
    let effects = match lekalo_core::effects::build(&compilation.project) {
        Err(set) => return DomainResult::invalid(set),
        Ok(effects) => effects,
    };

    // The typed Git handoff lives entirely at this edge.
    let changed_set = if changed {
        match git_input::changed_input_set(
            &impact_project_root(&project),
            base.as_deref(),
            head.as_deref(),
            worktree,
            &source_paths_of(&compilation),
        ) {
            Ok(set) => Some(set),
            Err(failure) => return DomainResult::invalid(failure.diagnostic_set()),
        }
    } else {
        None
    };
    // The observed index, when the project records one; a present but
    // unusable index fails closed instead of silently ignoring recorded
    // code.
    let observed = match observed_view(&project) {
        Err(result) => return result,
        Ok(observed) => observed,
    };
    match lekalo_core::impact::analyze(
        &compilation.project,
        &graph,
        &effects,
        &request,
        changed_set.as_ref(),
        observed.as_ref(),
    ) {
        Ok(result) => render_impact(&result),
        Err(lekalo_core::impact::ImpactFailure::Invalid(set)) => DomainResult::invalid(set),
        Err(lekalo_core::impact::ImpactFailure::Denied(set)) => DomainResult::denied(set),
    }
}

/// The observed view of one selection: `Ok(None)` when the project
/// records no index, the analyzer view when it does.
fn observed_view(
    project: &Option<String>,
) -> Result<Option<lekalo_core::observed::view::ObservedView>, DomainResult> {
    let selection = selection_for(project);
    let context = lekalo_core::observed::context(&selection)?;
    match lekalo_core::observed::load_index(&context) {
        Ok(Some(index)) => Ok(Some(lekalo_core::observed::view::ObservedView::of(&index))),
        Ok(None) => Ok(None),
        Err(set) => Err(DomainResult::invalid(set)),
    }
}

/// The accepted impact success envelope: the fixed key order `status`,
/// `impact`. Human and JSON are projections of the same result.
fn render_impact(result: &lekalo_core::impact::ImpactResult) -> DomainResult {
    let bytes = match result.to_canonical_json() {
        Ok(bytes) => bytes,
        Err(set) => return DomainResult::invalid(set),
    };
    let json = format!("{{\"status\":\"valid\",\"impact\":{}}}", bytes);
    let mut human = vec![format!(
        "impact {} ({})",
        result.input_mode().key(),
        result
            .roots()
            .iter()
            .map(|root| root.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )];
    human.push(format!(
        "  direct {} transitive {} mandatory-public {}",
        result.direct().summary.returned,
        result.transitive().summary.returned,
        result.mandatory_public().summary.returned
    ));
    human.push(format!("  risks {}", result.risks().summary.returned));
    human.push(format!("  gates {}", result.gates().summary.returned));
    human.push(format!(
        "  evidence {} completeness {}",
        result.evidence().summary.state.key(),
        result.completeness().state.key()
    ));
    DomainResult::impact(json, human.join("\n"), result.warnings().to_vec())
}

/// Run one `observe` subcommand (issue #39): load the project through the
/// accepted seam, hand everything to the core observed engine, and
/// project the result. Every observed decision — scan normalization,
/// merge, binding resolution, staleness, promotion — lives in the core;
/// this binary only selects, renders, and maps exits.
fn run_observe(command: ObserveCommands) -> DomainResult {
    match command {
        ObserveCommands::Update { scan, project } => run_observe_update(&scan, &project),
        ObserveCommands::Bind {
            symbol,
            key,
            path,
            line,
            project,
        } => run_observe_bind(&symbol, key.as_deref(), &path, line, &project),
        ObserveCommands::Confirm { symbol, project } => run_observe_confirm(&symbol, &project),
        ObserveCommands::Check { project } => run_observe_check(&project),
        ObserveCommands::Attach {
            symbol,
            native_test,
            gate,
            project,
        } => run_observe_attach(&symbol, native_test.as_deref(), gate.as_deref(), &project),
        ObserveCommands::Inspect { symbol, project } => run_observe_inspect(&symbol, &project),
        ObserveCommands::Impact { symbol, project } => run_observe_impact(&symbol, &project),
        ObserveCommands::Promote {
            symbol,
            module,
            dry_run,
            confirm,
            project,
        } => run_observe_promote(symbol, module, dry_run, confirm.as_deref(), &project),
    }
}

/// Read the scan document bytes; the path is an invocation-relative
/// input document, never a project file.
fn scan_bytes(path: &str) -> Result<Vec<u8>, DomainResult> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                std::io::ErrorKind::NotFound => "scan-missing",
                _ => "scan-unreadable",
            };
            return Err(DomainResult::invalid(
                lekalo_core::observed::scan_io_failure(detail),
            ));
        }
    };
    if bytes.len() > lekalo_core::observed::MAX_SCAN_BYTES {
        return Err(DomainResult::invalid(
            lekalo_core::observed::scan_limit_set("scan-bytes", bytes.len()),
        ));
    }
    Ok(bytes)
}

fn run_observe_update(scan: &str, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let bytes = match scan_bytes(scan) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::observed::update_index(&context, &bytes) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "observe update {} symbols ({} explicit, {} confirmed, {} inferred, {} stale)",
                receipt.symbols,
                receipt.explicit,
                receipt.confirmed,
                receipt.inferred,
                receipt.stale
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_observe_bind(
    symbol: &str,
    key: Option<&str>,
    path: &str,
    line: Option<u64>,
    project: &Option<String>,
) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::observed::bind_explicit(&context, symbol, key, path, line) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "observe bind {} ({} {})",
                receipt.symbol,
                receipt.binding.key(),
                receipt.state.key()
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_observe_confirm(symbol: &str, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::observed::confirm_binding(&context, symbol) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!("observe confirm {} (confirmed)", receipt.symbol),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_observe_check(project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::observed::staleness(&context) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "observe check {} symbols ({} current, {} unknown)",
                receipt.symbols, receipt.current, receipt.unknown
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

/// Split one comma-separated external id list; empty members refuse.
fn external_ids(value: Option<&str>) -> Result<Vec<String>, DomainResult> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let ids: Vec<String> = value.split(',').map(str::to_owned).collect();
    if ids.iter().any(|id| id.is_empty()) {
        return Err(DomainResult::usage_error());
    }
    Ok(ids)
}

fn run_observe_attach(
    symbol: &str,
    native_test: Option<&str>,
    gate: Option<&str>,
    project: &Option<String>,
) -> DomainResult {
    let tests = match external_ids(native_test) {
        Ok(tests) => tests,
        Err(result) => return result,
    };
    let gates = match external_ids(gate) {
        Ok(gates) => gates,
        Err(result) => return result,
    };
    if tests.is_empty() && gates.is_empty() {
        return DomainResult::usage_error();
    }
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::observed::attach(&context, symbol, &tests, &gates) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "observe attach {} ({} tests, {} gates)",
                receipt.symbol,
                receipt.native_tests.len(),
                receipt.gates.len()
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_observe_inspect(symbol: &str, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let index = match observed_index(&context) {
        Ok(index) => index,
        Err(result) => return result,
    };
    match lekalo_core::observed::view::inspect_card(&index, symbol) {
        Ok(card) => DomainResult::receipt(card.to_json(), card.to_human()),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_observe_impact(symbol: &str, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let index = match observed_index(&context) {
        Ok(index) => index,
        Err(result) => return result,
    };
    match lekalo_core::observed::view::impact_card(&index, symbol) {
        Ok(impact) => DomainResult::receipt(impact.to_json(), impact.to_human()),
        Err(set) => DomainResult::invalid(set),
    }
}

/// The recorded index of a loaded context; absence is a registered
/// failure for every operation except `update`.
fn observed_index(
    context: &lekalo_core::observed::ObservedContext,
) -> Result<lekalo_core::observed::ObservedIndex, DomainResult> {
    match lekalo_core::observed::load_index(context) {
        Ok(Some(index)) => Ok(index),
        Ok(None) => Err(DomainResult::invalid(
            lekalo_core::observed::missing_index_set(),
        )),
        Err(set) => Err(DomainResult::invalid(set)),
    }
}

fn run_observe_promote(
    symbol: Option<String>,
    module: Option<String>,
    dry_run: bool,
    confirm: Option<&str>,
    project: &Option<String>,
) -> DomainResult {
    // Exactly one of --symbol and --module; exactly one action.
    if symbol.is_some() == module.is_some() {
        return DomainResult::usage_error();
    }
    if dry_run == confirm.is_some() {
        return DomainResult::usage_error();
    }
    if let Some(plan_id) = confirm {
        if well_formed_plan_id(plan_id).is_none() {
            return DomainResult::usage_error();
        }
    }
    let selection = lekalo_core::loader::LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let target = match (symbol, module) {
        (Some(symbol), _) => lekalo_core::observed::PromotionSelection::Symbol(symbol),
        (_, Some(module)) => lekalo_core::observed::PromotionSelection::Module(module),
        _ => return DomainResult::usage_error(),
    };
    if dry_run {
        match lekalo_core::observed::promote::plan(&context, &target) {
            Ok(receipt) => DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                format!(
                    "observe promote preview plan {} (-{} symbols, {} ineligible)",
                    receipt.plan,
                    receipt.symbols.len(),
                    receipt.ineligible.len()
                ),
            ),
            Err(set) => DomainResult::invalid(set),
        }
    } else {
        let plan_id = confirm.expect("exclusivity checked above");
        match lekalo_core::observed::promote::apply(&context, &target, plan_id) {
            Ok(receipt) => DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                format!(
                    "observe promote applied plan {} ({} symbols)",
                    receipt.plan,
                    receipt.symbols.len()
                ),
            ),
            Err(set) => DomainResult::invalid(set),
        }
    }
}
