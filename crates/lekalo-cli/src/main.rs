//! Issue #3/#7/#8/#9/#10/#11 CLI: clap syntax and presentation. The core
//! owns every decision; this binary selects, renders, and maps exits. Since
//! #11 both renderers project the exact same `DomainResult`.

use clap::{error::ErrorKind, Args, ColorChoice, Parser, Subcommand, ValueEnum};
use lekalo_core::artifacts::{ArtifactFailure, CheckReceipt, GenerateService};
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

/// The storage-projection attachment type of the `lekalo storage`
/// commands (issue #117).
type StorageAttachment = lekalo_core::storage_projection::StorageProjectionAttachment;
/// The storage-engine-profile attachment type of the `lekalo
/// storage-profile` commands (issue #117).
type ProfileAttachment = lekalo_core::storage_engine_profile::StorageEngineProfile;

mod doctor_git;
mod git_input;

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
        /// Discover this adapter program through the safe describe
        /// handshake and pin it into the created lock (issue #91
        /// catalog seam); the program vector follows `--`. Refused on
        /// an existing lock as stale.
        #[arg(trailing_var_arg = true)]
        program_args: Vec<String>,
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
    /// Validate, inspect, project or compare HTTP/JSON transport
    /// attachments (issue #70). The core owns every decision; this
    /// binary only selects, renders, and maps exits.
    Transport {
        #[command(subcommand)]
        command: TransportCommands,
    },
    /// Render, check, or inspect the OpenAPI projection of one
    /// transport attachment (issue #46). The core owns every decision;
    /// this binary only reads the documents, selects, renders, and
    /// maps exits.
    Openapi {
        #[command(subcommand)]
        command: OpenapiCommands,
    },
    /// Validate one declarative query-model attachment against the
    /// project, or compare two attachments of the same family.
    QueryModel {
        #[command(subcommand)]
        command: QueryModelCommands,
    },
    /// The storage family: the engine matrix, profile validation, the
    /// deterministic DDL rendering, migration planning, drift, and
    /// capability mapping (issue #69), plus the storage-projection
    /// validate/project/diff/introspect-check/plan handoff (issue
    /// #117). The core owns every decision; this binary only reads
    /// documents, selects, renders, and maps exits.
    Storage {
        #[command(subcommand)]
        command: StorageCommands,
    },
    /// Validate, evaluate, render, or compare typed-expression
    /// attachments (issue #66). The core owns every decision; this
    /// binary only selects, renders, and maps exits.
    Expressions {
        #[command(subcommand)]
        command: ExpressionsCommands,
    },
    /// Validate, project, or compare storage-engine-profile
    /// capability evidence (issue #117).
    StorageProfile {
        #[command(subcommand)]
        command: StorageProfileCommands,
    },
    /// Validate or inspect one data-classification attachment against
    /// the project (issue #87): custody, subject resolution, grants,
    /// and the governing policy.
    Classification {
        #[command(subcommand)]
        command: ClassificationCommands,
    },
    /// Derive or inspect the data-flow report over the classified
    /// project (issue #87): flows, tenant relations, gate decisions,
    /// and the first-class unknown list.
    Dataflow {
        #[command(subcommand)]
        command: DataflowCommands,
    },
    /// The privacy family (issue #119): the deterministic, custody-
    /// verified export-decision evaluator over the frozen #120 policy.
    /// The core owns every decision; this binary only reads the
    /// decision file, renders, and maps exits.
    Privacy {
        #[command(subcommand)]
        command: PrivacyCommands,
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
        /// Demand the full locked inventory before any work (`--check`
        /// and generation modes).
        #[arg(long)]
        locked: bool,
        /// Plan (with --dry-run) or apply (with --confirm) the
        /// deterministic clean of orphaned generated files.
        #[arg(long)]
        clean: bool,
        /// Compute the clean plan or the generation plan without
        /// applying it.
        #[arg(long)]
        dry_run: bool,
        /// Apply the clean plan with this exact identity
        /// (`sha256:<64 lowercase hex>`).
        #[arg(long, value_name = "PLAN_ID")]
        confirm: Option<String>,
        /// Absent with an adapter program selects every declared target.
        #[arg(long, value_name = "TARGET")]
        target: Vec<String>,
        /// Scope the generation attribution to one module.
        #[arg(long, value_name = "MODULE")]
        module: Option<String>,
        /// Issue #89 escalation policy: permit the adapter's described
        /// scopes to exceed its manifest ceiling for this run. Off by
        /// default; the widening stays visible in the confinement
        /// evidence.
        #[arg(long)]
        allow_permission_expansion: bool,
        /// The adapter program and its arguments, spawned directly;
        /// the vector follows `--` and its entry bytes must equal the
        /// locked pins exactly.
        #[arg(trailing_var_arg = true)]
        program_args: Vec<String>,
        /// Per-exchange adapter deadline in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = DEFAULT_SCAN_TIMEOUT_MS * 10)]
        timeout_ms: u64,
    },
    /// Run the read-only verification pipeline over a validated project:
    /// core validation, drift, per-target adapter validation, portable
    Verify {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Verify this target id through its adapter; repeat for several
        /// targets. Absent verifies every locked adapter.
        #[arg(long, value_name = "TARGET")]
        target: Vec<String>,
        /// Scope the reported binding and scenario views to one module.
        #[arg(long, value_name = "MODULE")]
        module: Option<String>,
        /// Resolve the affected scope from the working-tree changes.
        #[arg(long)]
        changed: bool,
        /// Demand the full locked inventory before any work.
        #[arg(long)]
        locked: bool,
        /// Summarize this project-relative trace manifest.
        #[arg(long, value_name = "PATH")]
        trace: Option<String>,
        /// The adapter program and its arguments, spawned directly;
        /// the vector follows `--` and is required for adapter
        /// validation.
        #[arg(trailing_var_arg = true)]
        program_args: Vec<String>,
        /// Per-exchange adapter deadline in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = DEFAULT_SCAN_TIMEOUT_MS * 10)]
        timeout_ms: u64,
    },
    /// Bootstrap a new greenfield Lekalo project in the invocation
    /// directory (issue #97), or adopt an existing repository with
    /// `--adopt` (issue #38).
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
        /// Greenfield: the semantic id of the first module.
        #[arg(long, value_name = "MODULE", default_value = "app")]
        module: String,
        /// Greenfield: the canonical model frontend of the generated
        /// documents.
        #[arg(long, value_enum, default_value_t)]
        frontend: InitFrontend,
        /// Greenfield: write the opt-in editor/schema hints
        /// (`.vscode/settings.json`).
        #[arg(long)]
        editor_hints: bool,
        /// Bootstrap-root / adoption-root selector, relative to the
        /// invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Print the full bootstrap plan without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Create one additional empty module in an initialized project
    /// (issue #97).
    Module {
        #[command(subcommand)]
        command: Box<ModuleCommands>,
    },
    /// Run the target adapter conformance suite (issue #31).
    Adapter {
        #[command(subcommand)]
        command: Box<AdapterCommands>,
    },
    Cache {
        #[command(subcommand)]
        command: Box<CacheCommands>,
    },
    /// Diagnose project, model, adapters, artifacts, and integrations in
    /// one read-only readiness report.
    Doctor {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Preview the closed safe-fix recipes for every finding; nothing
        /// is ever repaired, installed, updated, or written.
        #[arg(long)]
        fix: bool,
        /// Optional trace manifests supplying HLV/OpenSpec/AI Factory
        /// gate evidence (repeatable).
        #[arg(long = "trace", value_name = "PATH")]
        traces: Vec<String>,
    },
    /// Report the freshness panel — lock, cache, bindings, artifacts —
    /// plus the exact git/model/lock revisions.
    Status {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Report phase readiness with required and optional checks.
    Readiness {
        /// The readiness phase: model, implement, generate, verify, or
        /// release (alias: done).
        #[arg(long, value_enum)]
        phase: DoctorPhase,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Optional trace manifests supplying HLV/OpenSpec/AI Factory
        /// gate evidence (repeatable).
        #[arg(long = "trace", value_name = "PATH")]
        traces: Vec<String>,
    },
    /// Record, bind, verify, and promote existing code in observed mode
    /// (issue #39). The core owns every decision; this binary only
    /// selects, renders, and maps exits.
    Observe {
        #[command(subcommand)]
        command: Box<ObserveCommands>,
    },
    /// Scan existing code through one target adapter and record the
    /// bindings (issue #42). Everything after the program path is passed
    /// to the adapter verbatim (no shell), so adapter flags come last.
    Scan {
        /// The target the scanner must declare (`node-typescript`).
        #[arg(long, value_name = "TARGET")]
        target: String,
        /// Explicit adapter profile recorded with the bindings.
        #[arg(long, value_name = "PROFILE")]
        profile: Option<String>,
        /// Per-exchange adapter deadline in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = DEFAULT_SCAN_TIMEOUT_MS)]
        timeout_ms: u64,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The scanner program and its arguments, spawned directly.
        #[arg(trailing_var_arg = true)]
        program_args: Vec<String>,
    },
    /// List, propose, confirm, and audit the binding registry
    /// (issue #42). The core owns every decision; this binary only
    /// selects, renders, and maps exits.
    Bindings {
        #[command(subcommand)]
        command: BindingsCommands,
    },
    /// Record, verify, and govern AI-written implementation in
    /// contracted mode (issue #40). The model is primary, the target
    /// source is maintained code, and the adapter checks conformance
    /// and may generate support artifacts only. The core owns every
    /// decision; this binary only selects, renders, and maps exits.
    Contract {
        #[command(subcommand)]
        command: ContractCommands,
    },
    /// Plan native build/test gates over a Node workspace (issue #48).
    /// Planning is read-only: it produces the immutable proposed plan
    /// with its digest. Execution of trusted synthetic fixtures is
    /// owned by the test-only harness — the production binary never
    /// launches a gate command and answers with a typed refusal.
    Native {
        #[command(subcommand)]
        command: Box<NativeCommands>,
    },
    /// Resolve NFR constraints against their measured evidence
    /// (issue #85): the gate, the derived report, and the closed
    /// queries. The core owns every decision; this binary only
    /// selects, renders, and maps exits.
    Nfr {
        #[command(subcommand)]
        command: NfrCommands,
    },
}

/// The `native` subcommands (issue #48).
#[derive(Debug, Subcommand)]
enum NativeCommands {
    /// Propose the immutable native gate plan over the current
    /// workspace: workspace inventory, affected closure, confirmed
    /// commands, and the plan digest. Read-only; never launches a
    /// command. The plan document is read from stdin.
    Run {
        /// The plan document path; use `-` for stdin. The plan must
        /// carry a current trusted-fixture custody, and execution still
        /// requires the test-only harness — this production command
        /// answers with a typed blocked/unsupported refusal.
        #[arg(value_name = "PLAN")]
        plan: String,
    },
}

/// The per-exchange scan deadline default (issue #42).
const DEFAULT_SCAN_TIMEOUT_MS: u64 = 60_000;

/// The `nfr` subcommands (issue #85): validate is the gate (exit 0
/// pass, 1 invalid, 3 denied, 4 unavailable); report and query are
/// informational and exit 0 whenever the resolution completes.
#[derive(Debug, Subcommand)]
enum NfrCommands {
    /// Validate the attachment and gate every mandatory constraint:
    /// violated, unverified, stale, unsupported, or conflicted
    /// mandatory rows deny the gate; `--strict` escalates advisory
    /// violated/unverified/stale rows into the denied set.
    Validate {
        /// Path to the NFR attachment JSON document.
        path: String,
        /// Path to one evidence document; repeat for several
        /// environments.
        #[arg(long = "evidence", value_name = "FILE")]
        evidence: Vec<String>,
        /// Escalate advisory violated/unverified/stale rows into the
        /// denied set (the impact --profile strict precedent).
        #[arg(long)]
        strict: bool,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`); expiry is deterministic in this date, never
        /// a clock.
        #[arg(long, value_name = "DATE")]
        as_of: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Emit the canonical resolution report: per-constraint statuses,
    /// per-environment rows, foreign evidence, open questions, and the
    /// gate verdict.
    Report {
        /// Path to the NFR attachment JSON document.
        path: String,
        /// Path to one evidence document; repeat for several
        /// environments.
        #[arg(long = "evidence", value_name = "FILE")]
        evidence: Vec<String>,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`).
        #[arg(long, value_name = "DATE")]
        as_of: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Run one closed query over the resolution report.
    Query {
        /// Path to the NFR attachment JSON document.
        path: String,
        /// The closed selector: `report`, `unverified`, `stale`,
        /// `foreign-environment`, `open-questions`, `coverage-gaps`,
        /// `constraint:ID`, or `symbol:SEMANTIC-ID`.
        selector: String,
        /// Path to one evidence document; repeat for several
        /// environments.
        #[arg(long = "evidence", value_name = "FILE")]
        evidence: Vec<String>,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`).
        #[arg(long, value_name = "DATE")]
        as_of: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Emit the neutral #22 trace-manifest projection of the resolved
    /// NFR constraints: constraint, symbol, and gate nodes, the
    /// implements and evidences edges, and the explicit gaps.
    Trace {
        /// Path to the NFR attachment JSON document.
        path: String,
        /// Path to one evidence document; repeat for several
        /// environments.
        #[arg(long = "evidence", value_name = "FILE")]
        evidence: Vec<String>,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`).
        #[arg(long, value_name = "DATE")]
        as_of: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Compare two same-family NFR attachments semantically; the
    /// verdict stays data (breaking, non-breaking, policy-change).
    Diff {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
    },
    /// Project the impact of a constraint change through the accepted
    /// impact engine: the changed constraints' scope symbols enter as
    /// a synthesized typed changed-input set, and the standard impact
    /// payload carries the affected scenarios and gates.
    Impact {
        /// Path to the candidate NFR attachment JSON document.
        path: String,
        /// Path to the base NFR attachment JSON document.
        #[arg(long, value_name = "BASE")]
        base: String,
        /// Cap the traversal depth (1..=256).
        #[arg(long, value_name = "N", default_value_t = 8)]
        depth: u16,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The `module` subcommands: the module-authoring surface (issue #97).
#[derive(Debug, Subcommand)]
enum ModuleCommands {
    /// Create one additional empty module: the module manifest and
    /// nothing else, no-overwrite, gated by the normal load and
    /// validation path.
    New {
        /// The semantic id (and directory name) of the new module.
        id: String,
        /// The canonical model frontend of the generated document.
        #[arg(long, value_enum, default_value_t)]
        frontend: InitFrontend,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Print the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
}

/// The canonical model frontend of greenfield bootstrap documents.
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum InitFrontend {
    /// Human-friendly block YAML in the fixed `.yaml` homes.
    #[default]
    Yaml,
    /// Compact JSON bytes in the same homes (the adoption spelling).
    Json,
}

impl InitFrontend {
    /// The core frontend value.
    const fn core(self) -> lekalo_core::init::bootstrap::Frontend {
        match self {
            Self::Yaml => lekalo_core::init::bootstrap::Frontend::Yaml,
            Self::Json => lekalo_core::init::bootstrap::Frontend::Json,
        }
    }
}

/// The `bindings` subcommands: the binding registry surface (issue #42).
#[derive(Debug, Subcommand)]
enum BindingsCommands {
    /// Project the whole registry: implement/expose/verify rows with
    /// source, confidence, provenance, and freshness.
    List {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Derive the current confirmation proposals with their full
    /// candidate sets; ambiguous mappings list every candidate and pick
    /// none.
    Propose {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Confirm inferred bindings: one by proposal id (with --candidate
    /// for an ambiguous proposal), or the unambiguous set as a batch
    /// with the planned-and-confirmed preview.
    Confirm {
        /// The proposal id (`prop-<64 hex>`).
        proposal: Option<String>,
        /// The native candidate an ambiguous proposal resolves to.
        #[arg(long, value_name = "NATIVE")]
        candidate: Option<String>,
        /// Confirm every unambiguous current proposal as one batch.
        #[arg(long)]
        batch: bool,
        /// Preview the batch plan without confirming anything.
        #[arg(long, requires = "batch")]
        preview: bool,
        /// Apply exactly the previewed batch plan identity.
        #[arg(long, value_name = "PLAN_ID", requires = "batch")]
        confirm: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Re-fingerprint every binding after source changes: a changed
    /// signature or path is stale (gate failure) or correctly
    /// re-resolved, never silent.
    Audit {
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
/// The closed readiness-phase vocabulary for the CLI surface; `done` is
/// an accepted alias of `release`.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum DoctorPhase {
    /// Model loads and validates.
    Model,
    /// Implementation against pinned inputs.
    Implement,
    /// Generation against resolved adapters and profiles.
    Generate,
    /// Verification with available tools and clean artifacts.
    Verify,
    /// Release: the full gate.
    Release,
    /// The `done` alias of `release`.
    Done,
}

impl DoctorPhase {
    /// The canonical core phase.
    fn phase(self) -> lekalo_core::doctor::model::Phase {
        match self {
            Self::Model => lekalo_core::doctor::model::Phase::Model,
            Self::Implement => lekalo_core::doctor::model::Phase::Implement,
            Self::Generate => lekalo_core::doctor::model::Phase::Generate,
            Self::Verify => lekalo_core::doctor::model::Phase::Verify,
            Self::Release | Self::Done => lekalo_core::doctor::model::Phase::Release,
        }
    }
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
    /// Enumerate the adapter package discovery sources without running
    /// anything (issue #32). Auto-discovery never installs or trusts.
    Discover {
        /// The closed discovery source: path:<fs-path>, exec:<name>,
        /// release:<channel>/<id>, or registry:<registry>/<package>.
        #[arg(long, value_name = "SOURCE")]
        source: String,
        /// Refuse sources that are not already local (exact semantics:
        /// release/registry records do not exist in v1).
        #[arg(long)]
        offline: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// List the installed adapter packages from the local store
    /// inventory (issue #32).
    List {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Show one installed package's manifest projection, trust, and
    /// provenance (issue #32).
    Info {
        /// The adapter id.
        id: String,
        /// Optional exact version; defaults to the selected pin.
        #[arg(long, value_name = "VERSION")]
        version: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Install one adapter package from a closed source (issue #32).
    ///
    /// `--dry-run` renders the plan and writes nothing; `--confirm`
    /// applies exactly that previewed plan id.
    Install {
        /// The closed source coordinate (path:<fs-path>, exec:<name>,
        /// release:<channel>/<id>, registry:<registry>/<package>).
        source: String,
        /// Render the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Apply exactly the previewed plan id.
        #[arg(long = "confirm", value_name = "PLAN_ID")]
        confirm: Option<String>,
        /// Accept a permission-widening update diff (required for a
        /// plan flagged escalated).
        #[arg(long)]
        allow_escalation: bool,
        /// Refuse sources that are not already local.
        #[arg(long)]
        offline: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Update one installed package to another immutable version
    /// (issue #32). The plan renders the permission/capability diff;
    /// a widening diff refuses without `--allow-escalation`.
    Update {
        /// The adapter id.
        id: String,
        /// The target version; defaults to the newest installed one.
        #[arg(long, value_name = "VERSION")]
        to: Option<String>,
        /// Render the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Apply exactly the previewed plan id.
        #[arg(long = "confirm", value_name = "PLAN_ID")]
        confirm: Option<String>,
        /// Accept a permission-widening diff.
        #[arg(long)]
        allow_escalation: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Repoint the selected pin to a previously installed immutable
    /// version (issue #32). Bytes are never modified.
    Rollback {
        /// The adapter id.
        id: String,
        /// The exact previously installed version.
        #[arg(long, value_name = "VERSION")]
        to: String,
        /// Render the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Apply exactly the previewed plan id.
        #[arg(long = "confirm", value_name = "PLAN_ID")]
        confirm: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Record an explicit trust transition for an installed package (issue #32).
    Trust {
        /// The adapter id.
        id: String,
        /// The target trust level; never inferred, always explicit.
        #[arg(long = "level", value_enum)]
        level: AdapterTrustLevel,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Record a revocation for an adapter id/version in the local store (issue #32).
    Revoke {
        /// The adapter id.
        id: String,
        /// The revoked version, or * for the whole id.
        #[arg(long, value_name = "VERSION", default_value = "*")]
        version: String,
        /// The closed reason token.
        #[arg(long, value_name = "TOKEN")]
        reason: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Report or purge the quarantine custody (issue #32).
    Quarantine {
        #[command(subcommand)]
        command: QuarantineCommands,
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

/// The `query-model` subcommands: a thin handoff to the core query-model
/// resolver (issue #64). The attachment document is read at the given
/// path and every decision — wire validation, semantic self-check,
/// Model custody, reference resolution, the strict tenant gate, and
/// the plan projection — lives in the core. Nothing is ever written.
/// The `transport` subcommands (issue #70): the thin
/// validate/inspect/project/diff handoff over the core transport-http
/// family. The attachment document is read at the given path and every
/// decision — wire validation, semantic validation, the projection,
/// and the diff classification — lives in the core.
#[derive(Debug, Subcommand)]
enum TransportCommands {
    /// Validate the attachment against the selected project: wire
    /// normalization, custody, the semantic pass, and — under
    /// `--strict` — the mapping-completeness and capability gates.
    Validate {
        /// Path to the transport attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Path to the bound #62 error registry the error map checks
        /// against; without it the embedded seed registry (the planner
        /// seed) is bound, so any non-seed project with declared error
        /// entries must pass --errors or refuses as unbound.
        #[arg(long, value_name = "FILE")]
        errors: Option<String>,
        /// Path to the bound #64 query-model attachment the query
        /// endpoint checks resolve against.
        #[arg(long, value_name = "FILE")]
        query_model: Option<String>,
        /// Enforce the strict profile gates.
        #[arg(long)]
        strict: bool,
    },
    /// Inspect one endpoint binding: the joined Model surface (method,
    /// path, invokes) plus every declared transport member.
    Inspect {
        /// Path to the transport attachment JSON document.
        path: String,
        /// The endpoint symbol to inspect.
        #[arg(long, value_name = "SYMBOL")]
        endpoint: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Project the attachment into the canonical route surface of one
    /// closed namespace (node, laravel, go, or rust).
    Project {
        /// Path to the transport attachment JSON document.
        path: String,
        /// The closed projection namespace.
        #[arg(long, value_enum)]
        namespace: TransportNamespace,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Path to the bound #62 error registry.
        #[arg(long, value_name = "FILE")]
        errors: Option<String>,
        /// Path to the bound #64 query-model attachment.
        #[arg(long, value_name = "FILE")]
        query_model: Option<String>,
    },
    /// Compare two same-family attachments and classify every changed
    /// path; the verdict stays data, never an exit code.
    Diff {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
    },
}

/// The `openapi` subcommands (issue #46): the thin render/check/
/// inspect handoff over the core OpenAPI projection. Every decision —
/// the projection, the fragment merge, the bind/drift check, and the
/// bounds — lives in the core; this binary reads the documents,
/// selects, renders, and maps exits.
#[derive(Debug, Subcommand)]
enum OpenapiCommands {
    /// Render the OpenAPI document of one validated attachment: the
    /// canonical bytes, their digest, and the projection findings.
    Render {
        /// Path to the transport attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Path to the bound #62 error registry.
        #[arg(long, value_name = "FILE")]
        errors: Option<String>,
        /// Path to the bound #64 query-model attachment.
        #[arg(long, value_name = "FILE")]
        query_model: Option<String>,
        /// The declared OpenAPI version (3.1 default; 3.0 declared
        /// alternative).
        #[arg(long, value_enum, default_value_t = OpenapiVersion::V31)]
        version: OpenapiVersion,
        /// The declared document mode (full default).
        #[arg(long, value_enum, default_value_t = OpenapiMode::Full)]
        mode: OpenapiMode,
    },
    /// Check a maintained OpenAPI document against the current
    /// attachment: bind every operation, recompute the fragments, and
    /// report per-pointer drift plus the unbound-manual inventory.
    Check {
        /// Path to the maintained OpenAPI document (YAML or JSON).
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Path to the transport attachment the document binds to.
        #[arg(long, value_name = "FILE")]
        transport: String,
        /// Path to the bound #62 error registry.
        #[arg(long, value_name = "FILE")]
        errors: Option<String>,
        /// Path to the bound #64 query-model attachment.
        #[arg(long, value_name = "FILE")]
        query_model: Option<String>,
        /// Path to the ownership sidecar manifest; the default is the
        /// `<stem>.ownership.json` sibling of the document.
        #[arg(long, value_name = "FILE")]
        ownership: Option<String>,
        /// The declared OpenAPI version of the maintained document
        /// (3.1 default; the recomputation renders at the same
        /// version, so a maintained 3.0 document does not drift on
        /// its nullable spellings).
        #[arg(long, value_enum, default_value_t = OpenapiVersion::V31)]
        version: OpenapiVersion,
        /// The declared document mode of the maintained document
        /// (full default; the recomputation honors it).
        #[arg(long, value_enum, default_value_t = OpenapiMode::Full)]
        mode: OpenapiMode,
    },
    /// Inspect one endpoint's rendered operation: the joined Model
    /// surface plus every projected OpenAPI member.
    Inspect {
        /// Path to the transport attachment JSON document.
        path: String,
        /// The endpoint symbol to inspect.
        #[arg(long, value_name = "SYMBOL")]
        endpoint: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Path to the bound #62 error registry.
        #[arg(long, value_name = "FILE")]
        errors: Option<String>,
        /// Path to the bound #64 query-model attachment.
        #[arg(long, value_name = "FILE")]
        query_model: Option<String>,
    },
    /// Compare two same-family attachments and report the pointer-level
    /// view of the transport compatibility classes; the verdict stays
    /// data, never an exit code.
    Diff {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The declared OpenAPI version for pointer resolution.
        #[arg(long, value_enum, default_value_t = OpenapiVersion::V31)]
        version: OpenapiVersion,
    },
}

/// The declared OpenAPI version token.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum OpenapiVersion {
    /// OpenAPI 3.1 (the default).
    #[value(name = "3.1")]
    V31,
    /// OpenAPI 3.0 (the declared alternative).
    #[value(name = "3.0")]
    V30,
}

impl OpenapiVersion {
    const fn as_str(self) -> &'static str {
        match self {
            Self::V31 => "3.1",
            Self::V30 => "3.0",
        }
    }
}

/// The declared document mode token.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum OpenapiMode {
    /// One generator-owned document.
    Full,
    /// Per-pointer fragments merged through the ownership manifest.
    Fragments,
}

impl OpenapiMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Fragments => "fragments",
        }
    }
}

/// The closed projection namespace vocabulary.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum TransportNamespace {
    /// Node (ECMAScript) route table.
    Node,
    /// Laravel controller surface.
    Laravel,
    /// Go handler surface.
    Go,
    /// Rust (axum-style) route surface.
    Rust,
}

impl TransportNamespace {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Laravel => "laravel",
            Self::Go => "go",
            Self::Rust => "rust",
        }
    }
}

#[derive(Debug, Subcommand)]
enum QueryModelCommands {
    /// Validate the attachment against the selected project and emit
    /// the deterministic plan summary of every declared query.
    Validate {
        /// Path to the query-model attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// Enforce the strict tenant-filter requirement.
        #[arg(long)]
        strict: bool,
    },
    /// Compare two same-family attachments and classify every changed
    /// path; the verdict stays data, never an exit code.
    Diff {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
    },
}

/// The closed storage-engine vocabulary of the CLI.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum StorageEngineArg {
    /// The PostgreSQL engine.
    Postgres,
}

/// The closed capability-mapping profiles of the CLI.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum StorageCapabilityProfileArg {
    /// Block on unsupported, unknown, and unapproved partial.
    Strict,
    /// Degrade explicitly; never pass on unsupported or unknown.
    Permissive,
}

/// The `storage` subcommands (issues #69 and #117): the thin handoff
/// over the storage-engine and storage-projection core families. The
/// core owns every decision; this binary only selects, renders, and
/// maps exits.
#[derive(Debug, Subcommand)]
enum StorageCommands {
    /// Print the owner-published engine version matrix, or one
    /// version's capability answers.
    Profile {
        /// The closed engine vocabulary.
        #[arg(long, value_enum)]
        engine: StorageEngineArg,
        /// The exact engine version pin (major.minor.patch); absent
        /// prints every published major row.
        #[arg(long, value_name = "V")]
        version: Option<String>,
    },
    /// Validate one storage-engine attachment and emit its canonical
    /// bytes.
    ValidateEngine {
        /// Path to the storage-engine attachment JSON document.
        path: String,
    },
    /// Derive the deterministic migration plan from two same-project
    /// storage-projection attachments under one engine profile. A
    /// destructive plan is gated: it prints blocked unless the exact
    /// planId is named with --confirm (the native-gate custody
    /// pattern).
    MigratePlan {
        /// Path to the base storage-projection attachment.
        base: String,
        /// Path to the candidate storage-projection attachment.
        candidate: String,
        /// Path to the storage-engine profile attachment.
        #[arg(long, value_name = "PATH")]
        profile: String,
        /// Apply custody: the exact planId (sha256 digest) of the
        /// gated plan. A wrong digest refuses.
        #[arg(long, value_name = "PLAN_ID")]
        confirm: Option<String>,
    },
    /// Run the storage-component conformance battery over the
    /// committed fixture set: the profile and its bound projection,
    /// the optional checked-mode evidence pair, the engine input
    /// document, and the runtime goldens. A skip is never a pass.
    Conformance {
        /// Path to the storage-engine profile attachment.
        #[arg(long, value_name = "PATH")]
        profile: String,
        /// Path to the bound storage-projection attachment.
        #[arg(long, value_name = "PATH")]
        projection: String,
        /// Path to the zero-drift introspection evidence.
        #[arg(long, value_name = "PATH")]
        scan: Option<String>,
        /// Path to the drifted introspection evidence.
        #[arg(long, value_name = "PATH")]
        drifted: Option<String>,
        /// Path to the engine input document (the runtime goldens
        /// must equal it byte for byte).
        #[arg(long, value_name = "PATH")]
        input: Option<String>,
        /// Paths to the runtime goldens (repeatable).
        #[arg(long = "runtime", value_name = "PATH")]
        runtimes: Vec<String>,
    },
    /// Print the single runtime-neutral engine input document every
    /// runtime consumer (Node.js, Laravel, Go, Rust) receives.
    Input {
        /// Path to the storage-engine attachment JSON document.
        profile: String,
        /// Path to the bound storage-projection attachment JSON
        /// document.
        #[arg(long, value_name = "PATH")]
        projection: String,
    },
    /// Render the deterministic DDL document of one profile over its
    /// bound storage-projection attachment.
    Ddl {
        /// Path to the storage-engine attachment JSON document.
        profile: String,
        /// Path to the bound storage-projection attachment JSON
        /// document; its canonical digest must equal the profile's
        /// projectionRef.
        #[arg(long, value_name = "PATH")]
        projection: String,
    },
    /// Compare one checked-mode storage-observation evidence document
    /// against its bound storage-projection attachment and report the
    /// typed drift findings.
    Drift {
        /// Path to the storage-observation evidence JSON document.
        scan: String,
        /// Path to the bound storage-projection attachment JSON
        /// document.
        #[arg(long, value_name = "ATTACHMENT")]
        projection: String,
        /// Path to the storage-engine profile attachment JSON document
        /// whose projectionRef binds both sides.
        #[arg(long, value_name = "PATH")]
        profile: String,
    },
    /// Project the engine capability snapshot, optionally mapped
    /// against one transaction-concurrency attachment's requirements.
    Capabilities {
        /// Path to the storage-engine attachment JSON document.
        path: String,
        /// Path to the bound storage-projection attachment JSON
        /// document.
        #[arg(long, value_name = "PATH")]
        projection: String,
        /// The closed mapping profile; the default is strict.
        #[arg(long, value_enum, default_value_t = StorageCapabilityProfileArg::Strict)]
        profile: StorageCapabilityProfileArg,
        /// Optional path to a transaction-concurrency attachment whose
        /// capability requirements are mapped against the snapshot.
        #[arg(long, value_name = "PATH")]
        requirements: Option<String>,
    },
    /// Validate one storage-projection attachment against the
    /// selected project and emit the derived-projection summary of
    /// every declared namespace.
    Validate {
        /// Path to the storage-projection attachment JSON document.
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Derive one namespace's storage projection from one attachment
    /// and emit its canonical bytes.
    Project {
        /// Path to the storage-projection attachment JSON document.
        path: String,
        /// The closed target namespace vocabulary.
        #[arg(long, value_enum)]
        namespace: StorageNamespace,
    },
    /// Compare two same-family attachments and classify every changed
    /// path; the verdict stays data, never an exit code.
    Diff {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
    },
    /// Compare one declared projection against one adapter-produced
    /// introspection evidence document; drift is data, never a
    /// guessed repair. No database connection exists anywhere.
    IntrospectCheck {
        /// Path to the declared storage-projection attachment.
        #[arg(long, value_name = "PATH")]
        projection: String,
        /// Path to the storage-introspection evidence document.
        #[arg(long, value_name = "PATH")]
        evidence: String,
        /// The closed target namespace vocabulary.
        #[arg(long, value_enum)]
        namespace: StorageNamespace,
    },
    /// Derive the non-executable migration plan over one comparison;
    /// destructive and backfill steps carry the explicit gate. The
    /// plan-id acknowledgment stays data: with `--confirm` and the
    /// exact plan identity the envelope records the acknowledgment,
    /// and a wrong identity refuses as stale.
    Plan {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
        /// The exact plan identity to acknowledge.
        #[arg(long, value_name = "PLAN_ID")]
        confirm: Option<String>,
    },
}

/// The `expressions` subcommands (issue #66): the thin
/// validate/eval/render/diff handoff over the core family.
#[derive(Debug, Subcommand)]
enum ExpressionsCommands {
    /// Validate one attachment: exhaustive static typing, canonical
    /// bytes, and the required capability set.
    Validate {
        /// Path to the expressions attachment JSON document.
        path: String,
        /// Path to a built-in capability snapshot; a required token
        /// missing from the snapshot blocks managed mode.
        #[arg(long, value_name = "FILE")]
        builtin_support: Option<String>,
    },
    /// Evaluate the shared vectors of one attachment against the
    /// deterministic reference evaluator with the injected clock.
    Eval {
        /// Path to the expressions attachment JSON document.
        path: String,
        /// Path to the evaluation-vector document.
        #[arg(long, value_name = "FILE")]
        vectors: String,
        /// Path to a built-in capability snapshot (managed mode).
        #[arg(long, value_name = "FILE")]
        builtin_support: Option<String>,
    },
    /// Render one complete cross-target program (node, php, or go)
    /// that computes the shared vectors.
    Render {
        /// Path to the expressions attachment JSON document.
        path: String,
        /// The closed target vocabulary.
        #[arg(long, value_enum)]
        target: ExpressionTarget,
        /// Path to a built-in capability snapshot (managed mode).
        #[arg(long, value_name = "FILE")]
        builtin_support: Option<String>,
    },
    /// Compare two same-family attachments and classify every
    /// changed path; the verdict stays data, never an exit code.
    Diff {
        /// Path to the base attachment JSON document.
        base: String,
        /// Path to the candidate attachment JSON document.
        candidate: String,
    },
}

/// The closed storage namespace vocabulary of the CLI.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum StorageNamespace {
    /// The PostgreSQL namespace.
    Postgres,
    /// The Laravel (Eloquent migration) namespace.
    Laravel,
    /// The MySQL namespace.
    Mysql,
    /// The MariaDB namespace.
    Mariadb,
}

impl StorageNamespace {
    /// The core namespace key.
    const fn key(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Laravel => "laravel",
            Self::Mysql => "mysql",
            Self::Mariadb => "mariadb",
        }
    }

    /// The core enum value.
    const fn core(self) -> lekalo_core::storage_projection::Namespace {
        match self {
            Self::Postgres => lekalo_core::storage_projection::Namespace::Postgres,
            Self::Laravel => lekalo_core::storage_projection::Namespace::Laravel,
            Self::Mysql => lekalo_core::storage_projection::Namespace::Mysql,
            Self::Mariadb => lekalo_core::storage_projection::Namespace::Mariadb,
        }
    }
}

/// The `storage-profile` subcommands (issue #117): the thin
/// validate/capabilities/portability/diff handoff over the core
/// storage-engine-profile family.
#[derive(Debug, Subcommand)]
enum StorageProfileCommands {
    /// Validate one engine profile attachment and emit the identity
    /// summary.
    Validate {
        /// Path to the storage-engine-profile attachment JSON document.
        path: String,
    },
    /// Emit the capability snapshot JSON of one profile (the #24
    /// bridge input).
    Capabilities {
        /// Path to the storage-engine-profile attachment JSON document.
        path: String,
    },
    /// Compare two profiles and emit the portability report.
    Portability {
        /// Path to the source engine profile JSON document.
        base: String,
        /// Path to the target engine profile JSON document.
        target: String,
        /// Attach the named PostgreSQL-specific semantics block.
        #[arg(long)]
        postgres_divergences: bool,
    },
    /// Compare two same-family profiles and classify every changed
    /// path; the verdict stays data, never an exit code.
    Diff {
        /// Path to the base profile JSON document.
        base: String,
        /// Path to the candidate profile JSON document.
        candidate: String,
    },
}

/// The closed expression render target vocabulary.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum ExpressionTarget {
    /// Node (ECMAScript).
    Node,
    /// PHP.
    Php,
    /// Go.
    Go,
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

/// The `contract` subcommands: the contracted-mode surface (issue #40).
/// The core owns every decision — declaration validation, conformance
/// classification, attachment custody, and support-artifact ownership;
/// this layer selects, renders, and maps exits.
#[derive(Debug, Subcommand)]
enum ContractCommands {
    /// Merge one adapter declaration document into the conformed
    /// registry; a binding recorded with a fingerprint is conformance
    /// evidence until the source or the contract changes.
    Update {
        /// The adapter declaration document (JSON), relative to the
        /// invocation directory.
        #[arg(long, value_name = "FILE")]
        declaration: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Run the conformance gate: re-fingerprint every binding,
    /// recompute the canonical signatures and declared effects from the
    /// typed IR, and re-digest every fingerprinted support artifact.
    Check {
        /// Restrict the gate to one module.
        #[arg(long, value_name = "MODULE")]
        module: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Attach verbatim native-test or gate ids to one recorded symbol.
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
    /// Register one support artifact in the ownership manifest; the
    /// path must stay inside the generated home, and maintained
    /// implementation paths refuse by construction.
    Support {
        /// The owning semantic id.
        symbol: String,
        /// The closed support-artifact kind.
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// The exact logical path inside `.lekalo/generated/**`.
        #[arg(long, value_name = "PATH")]
        path: String,
        /// The closed lifecycle (generated, scaffolded, checked).
        #[arg(long, value_name = "LIFECYCLE", default_value = "generated")]
        lifecycle: String,
        /// The SHA-256 over the artifact's exact observed bytes.
        #[arg(long, value_name = "SHA256")]
        digest: Option<String>,
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

/// The process entry: the closed command tree's derive surface is
/// large, so the runtime runs on an explicitly bounded thread instead
/// of the platform-default main-thread stack (1 MiB on Windows), which
/// debug builds of the parser can exceed. Join semantics preserve both
/// the exit code and a crash's unwind (exit 101).
fn main() -> ExitCode {
    // The combined subcommand surface overflows the default main-thread
    // stack in debug builds during clap's recursive tree walk; run the
    // CLI on a worker thread with an explicit stack reservation, and
    // preserve a crash's unwind through the join.
    let runtime = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(runtime)
        .expect("runtime thread");
    match runtime.join() {
        Ok(code) => ExitCode::from(code),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn runtime() -> u8 {
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
                program_args,
            } => run_lock(project, check, program_args),
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
            Commands::QueryModel { command } => run_query_model(command),
            Commands::Transport { command } => run_transport(command),
            Commands::Storage { command } => run_storage(command),
            Commands::StorageProfile { command } => run_storage_profile(command),
            Commands::Openapi { command } => run_openapi(command),
            Commands::Expressions { command } => run_expressions(command),
            Commands::Graph { command } => run_graph(command, cli.no_cache),
            Commands::Effects { command } => run_effects(command, cli.no_cache),
            Commands::Trace { command } => run_trace(command),
            Commands::Requirements { command } => run_requirements(command),
            Commands::Generate {
                project,
                check,
                locked,
                clean,
                dry_run,
                confirm,
                target,
                module,
                allow_permission_expansion,
                program_args,
                timeout_ms,
            } => run_generate(
                project,
                check,
                locked,
                clean,
                dry_run,
                confirm,
                target,
                module,
                allow_permission_expansion,
                program_args,
                timeout_ms,
            ),
            Commands::Verify {
                project,
                target,
                module,
                changed,
                locked,
                trace,
                program_args,
                timeout_ms,
            } => run_verify(
                project,
                target,
                module,
                changed,
                locked,
                trace,
                program_args,
                timeout_ms,
            ),
            Commands::Doctor {
                project,
                fix,
                traces,
            } => run_doctor(project, fix, traces),
            Commands::Status { project } => run_status(project),
            Commands::Readiness {
                phase,
                project,
                traces,
            } => run_readiness(phase.phase(), project, traces),
            Commands::Cache { command } => run_cache(*command),
            Commands::Scan {
                target,
                profile,
                timeout_ms,
                project,
                program_args,
            } => run_scan(
                &target,
                profile.as_deref(),
                timeout_ms,
                &project,
                program_args,
            ),
            Commands::Bindings { command } => run_bindings(command),
            Commands::Contract { command } => run_contract(command),
            Commands::Native { command } => match *command {
                NativeCommands::Run { plan } => run_native_run(&plan),
            },
            Commands::Nfr { command } => run_nfr(command),
            Commands::Classification { command } => run_classification(command),
            Commands::Dataflow { command } => run_dataflow(command),
            Commands::Init {
                adopt,
                target,
                profile,
                project_id,
                module,
                frontend,
                editor_hints,
                project,
                dry_run,
            } => run_init(
                adopt,
                target,
                profile,
                project_id,
                module,
                frontend,
                editor_hints,
                project,
                dry_run,
            ),
            Commands::Module { command } => run_module(*command),
            Commands::Observe { command } => run_observe(*command),
            Commands::Privacy { command } => {
                return match command {
                    PrivacyCommands::Evaluate { decision } => run_privacy_evaluate(&decision),
                    PrivacyCommands::Export {
                        artifact,
                        destination,
                        dry_run,
                        consent,
                        project,
                    } => run_privacy_export(
                        &artifact,
                        &destination,
                        dry_run,
                        consent.as_deref(),
                        &project,
                    ),
                    PrivacyCommands::Redact {
                        payload,
                        repository,
                        terms,
                        ..
                    } => run_privacy_redact(&payload, repository.as_deref(), &terms),
                };
            }
            Commands::Adapter { command } => match run_adapter(*command) {
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
                    return if write_ok { exit } else { OUTPUT_FAILURE };
                }
            },
        },
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp => {
                let ok = error.print().is_ok();
                return if ok { 0 } else { OUTPUT_FAILURE };
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
fn emit(result: DomainResult, json: bool) -> u8 {
    let exit_code = result.exit_code();
    #[allow(clippy::let_and_return)]
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
        exit_code
    } else {
        OUTPUT_FAILURE
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
            // Classification review (#87): when the attachment is
            // present, its custody/subject/grant/sink review runs
            // automatically, before every other surface so a
            // present-but-broken attachment is never masked. The strict
            // profile invalidates on any error finding; the default
            // profile records the findings in the report diagnostics.
            let (review, recorded) = classification_validate_review(
                &compilation,
                &lekalo_core::loader::canonical_model_bytes(&model),
                &selection,
                strict,
            );
            if let Some(result) = review {
                return result;
            }
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
            // Transport home (#70): when `lekalo/transport.yaml`
            // exists, the semantic pass includes it — wire
            // normalization plus the Model-bound checks; the
            // cross-family gates (error union, query model,
            // capabilities) stay with `lekalo transport validate`,
            // which binds their explicit contexts.
            let root = match lekalo_core::orchestration::project_root(&selection) {
                Ok(root) => root,
                Err(result) => return result,
            };
            match lekalo_core::transport_http::read_document(&root) {
                Err(diagnostics) => return DomainResult::invalid(diagnostics),
                Ok(Some(attachment)) => {
                    let context =
                        lekalo_core::transport_http::ValidationContext::new(&compilation.project);
                    if let Err(diagnostics) =
                        lekalo_core::transport_http::validate(&attachment, &context)
                    {
                        return DomainResult::invalid(diagnostics);
                    }
                    // OpenAPI projection preflight (#46): the home must
                    // render at the declared defaults; a refusal (a
                    // bound, a version-unsupported construct) fails the
                    // run. The full-document gate stays with `lekalo
                    // openapi render`.
                    if let Err(diagnostics) = lekalo_core::openapi::render(
                        &attachment,
                        &context,
                        &lekalo_core::openapi::RenderConfig::new(),
                    ) {
                        return DomainResult::invalid(diagnostics);
                    }
                }
                Ok(None) => {}
            }
            let (json, human) = render_validate_success(&model, &report);
            let mut diagnostics = report.diagnostics().as_slice().to_vec();
            diagnostics.extend(recorded);
            DomainResult::validation(json, human, diagnostics)
        }
    }
}

/// The classification review inside `lekalo validate` (issue #87): the
/// declared attachment (discovered at the canonical home under the
/// project root) plus its governing policy run the full custody,
/// subject-resolution, policy/grant, and strict sensitive-sink review.
/// Returns the terminal review result, if any, plus the recorded
/// findings (default profile keeps the run valid with the findings
/// visible in the diagnostics; the strict profile invalidates on any
/// error-severity finding). The first tuple member is `None` when no
/// attachment is declared.
fn classification_validate_review(
    compilation: &lekalo_core::ir::Compilation,
    model_json: &str,
    selection: &LoadSelection,
    strict: bool,
) -> (
    Option<DomainResult>,
    Vec<lekalo_core::diagnostics::Diagnostic>,
) {
    let root = match lekalo_core::doctor::project_root(selection) {
        Err(_) => return (None, Vec::new()),
        Ok(root) => root,
    };
    let (attachment, policy) = match lekalo_core::classification::discover(&root) {
        Ok(Some(pair)) => pair,
        Ok(None) => return (None, Vec::new()),
        Err(set) => return (Some(DomainResult::invalid(set)), Vec::new()),
    };
    // Custody and subject resolution: structured violations are
    // terminal invalid sets in every profile.
    if let Err(set) = lekalo_core::classification::validate_custody(
        &attachment,
        &policy,
        &compilation.project,
        model_json,
    ) {
        return (Some(DomainResult::invalid(set)), Vec::new());
    }
    let resolution = match lekalo_core::classification::validate_subjects(&attachment, compilation)
    {
        Err(set) => return (Some(DomainResult::invalid(set)), Vec::new()),
        Ok(resolution) => resolution,
    };
    let outcome = match lekalo_core::classification::validate_policy_and_grants(
        &attachment,
        &policy,
        &resolution,
        &compilation.project,
    ) {
        Err(set) => return (Some(DomainResult::invalid(set)), Vec::new()),
        Ok(outcome) => outcome,
    };
    if strict && outcome.invalid {
        return (
            Some(DomainResult::invalid(
                lekalo_core::classification::findings_set(&outcome),
            )),
            Vec::new(),
        );
    }
    // Recorded, never silently skipped: the findings ride the success
    // diagnostics as warning-class rows.
    let recorded = lekalo_core::classification::findings_set(&outcome)
        .as_slice()
        .to_vec();
    (None, recorded)
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
fn run_lock(project: Option<String>, check: bool, program_args: Vec<String>) -> DomainResult {
    let selection = selection_for(&project);
    // The issue #91 catalog seam: an explicit adapter supply is
    // discovered describe-only, resolved into the created lock through
    // a request that names the discovered adapter id, and never touches
    // an existing lock.
    let supply = match program_args.split_first() {
        Some((program, adapter_args)) => {
            let root = match lekalo_core::orchestration::project_root(&selection) {
                Ok(root) => root,
                Err(result) => return result,
            };
            match lekalo_core::orchestration::AdapterSupply::new(
                &root,
                program,
                adapter_args.to_vec(),
            ) {
                Ok(supply) => Some(supply),
                Err(failure) => return DomainResult::from(&failure),
            }
        }
        None => None,
    };
    let mut candidates;
    let request = match supply.as_ref() {
        Some(supply) => {
            let root =
                lekalo_core::orchestration::project_root(&selection).expect("root resolved above");
            let mut client = lekalo_core::target_protocol::TargetClient::default();
            let discovered = match lekalo_core::orchestration::catalog::discover(
                &mut client,
                supply,
                &root,
                lekalo_core::target_protocol::transport::TransportLimits::default(),
            ) {
                Ok(discovered) => discovered,
                Err(failure) => return DomainResult::from(&failure),
            };
            candidates = match lekalo_core::orchestration::candidate_supply(&discovered, supply) {
                Ok(candidates) => candidates,
                Err(failure) => return DomainResult::from(&failure),
            };
            // Issue #32: when the discovered adapter is the selected
            // installed store pin, emit the installed provenance (source
            // kind installed, package manifest digest, pinned trust) into
            // the lock instead of a plain project pin. A provenance
            // refusal refuses the lock (fix round 2, devin F-8): a pin
            // that cannot carry its custody evidence never commits.
            if let Ok(inventory) = lekalo_core::adapter_package::Inventory::load(&root) {
                if let Some(row) = inventory
                    .selected(&discovered.adapter.id)
                    .filter(|row| row.version == discovered.adapter.version)
                {
                    if let Err(failure) = candidates.with_installed_provenance(
                        &row.version,
                        &row.digest,
                        &row.manifest_digest,
                        row.trust.as_str(),
                        row.install_plan_id.as_deref(),
                    ) {
                        return DomainResult::from(&failure);
                    }
                }
            }
            match LockService::load_request(&selection) {
                Ok(request) => request,
                Err(failure) => return DomainResult::from(&failure),
            }
            .with_adapter(
                lekalo_core::lockfile::types::ComponentId::parse(&discovered.adapter.id)
                    .map_err(|failure| DomainResult::from(&failure))
                    .expect("discovered adapter id is a valid component id"),
            )
        }
        None => {
            candidates = CandidateSet::empty();
            match LockService::load_request(&selection) {
                Ok(request) => request,
                Err(failure) => return DomainResult::from(&failure),
            }
        }
    };
    match LockService::lock_with_request(
        &selection,
        request,
        candidates,
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

/// Run `lekalo init`: the thin handoff to the core adoption service
/// (`--adopt`, issue #38) or the core greenfield bootstrap service
/// (issue #97). The core owns every decision; this layer only checks
/// the closed request grammars and maps them onto the stable
/// `cli.usage` failure.
#[allow(clippy::too_many_arguments)]
fn run_init(
    adopt: bool,
    target: Option<String>,
    profile: Option<String>,
    project_id: Option<String>,
    module: String,
    frontend: InitFrontend,
    editor_hints: bool,
    project: Option<String>,
    dry_run: bool,
) -> DomainResult {
    if let Some(target) = target.as_deref() {
        if !lekalo_core::init::detect::valid_target_id(target) {
            return DomainResult::usage_error();
        }
    }
    // A profile selects within one explicit target: an orphan or
    // malformed profile is the stable usage failure before any plan
    // or write.
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
    if adopt {
        return lekalo_core::init::adopt(&lekalo_core::init::AdoptRequest {
            project,
            target,
            profile,
            project_id,
            dry_run,
        });
    }
    lekalo_core::init::bootstrap::bootstrap(&lekalo_core::init::bootstrap::BootstrapRequest {
        project,
        project_id,
        module,
        frontend: frontend.core(),
        target,
        profile,
        editor_hints,
        dry_run,
    })
}

/// Run `lekalo module new`: the thin handoff to the core module
/// creation service (issue #97).
fn run_module(command: ModuleCommands) -> DomainResult {
    let ModuleCommands::New {
        id,
        frontend,
        project,
        dry_run,
    } = command;
    lekalo_core::init::bootstrap::module_new(&lekalo_core::init::bootstrap::ModuleNewRequest {
        project,
        id,
        frontend: frontend.core(),
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
///
/// Issue #32: the launched program first passes the adapter package
/// resolution gate — the implicit local-development descriptor is
/// synthesized from the entry bytes and the integrity/trust gates run
/// before any child process exists.
fn run_adapter(command: AdapterCommands) -> AdapterRun {
    match command {
        AdapterCommands::Test {
            profile,
            report,
            repeats,
            timeout_ms,
            program_args,
        } => run_adapter_test(profile, report, repeats, timeout_ms, program_args),
        AdapterCommands::Discover {
            source,
            offline,
            project,
        } => run_adapter_discover(&source, offline, &project),
        AdapterCommands::List { project } => run_adapter_list(&project),
        AdapterCommands::Info {
            id,
            version,
            project,
        } => run_adapter_info(&id, version.as_deref(), &project),
        AdapterCommands::Install {
            source,
            dry_run,
            confirm,
            allow_escalation,
            offline,
            project,
        } => run_adapter_install(
            &source,
            dry_run,
            confirm.as_deref(),
            allow_escalation,
            offline,
            &project,
        ),
        AdapterCommands::Update {
            id,
            to,
            dry_run,
            confirm,
            allow_escalation,
            project,
        } => run_adapter_repoint(
            AdapterRepoint::Update,
            &id,
            to.as_deref(),
            dry_run,
            confirm.as_deref(),
            allow_escalation,
            &project,
        ),
        AdapterCommands::Rollback {
            id,
            to,
            dry_run,
            confirm,
            project,
        } => run_adapter_repoint(
            AdapterRepoint::Rollback,
            &id,
            Some(to.as_str()),
            dry_run,
            confirm.as_deref(),
            false,
            &project,
        ),

        AdapterCommands::Trust { id, level, project } => run_adapter_trust(&id, level, &project),
        AdapterCommands::Revoke {
            id,
            version,
            reason,
            project,
        } => run_adapter_revoke(&id, &version, &reason, &project),
        AdapterCommands::Quarantine { command } => match command {
            QuarantineCommands::List { project } => run_adapter_quarantine_list(&project),
            QuarantineCommands::Purge { all, project } => {
                run_adapter_quarantine_purge(all, &project)
            }
            QuarantineCommands::Release { id, project } => {
                run_adapter_quarantine_release(&id, &project)
            }
        },
    }
}

/// Run `lekalo adapter discover`: enumerate one closed discovery source
/// without running anything. The report is a deterministic JSON receipt
/// on stdout; auto-discovery never installs, trusts, or executes.
/// The gate that refused a candidate, in gate order (pure, unit-tested —
/// fix round 4, cline F-NEW-4). The discover receipt renders it so the
/// label and the `adapter.*` reason code always agree.
fn gate_label_of(failure: &lekalo_core::adapter_package::PackageFailure) -> &'static str {
    use lekalo_core::adapter_package::PackageFailure;
    match failure {
        PackageFailure::ManifestInvalid { .. } => "manifest",
        PackageFailure::Incompatible { .. } => "compatibility",
        PackageFailure::ChecksumMismatch { .. } => "integrity",
        PackageFailure::SignatureUnverified { .. } => "signature",
        PackageFailure::Revoked { .. }
        | PackageFailure::Quarantined { .. }
        | PackageFailure::TrustInsufficient { .. } => "trust",
        _ => "integrity",
    }
}

fn run_adapter_discover(source: &str, offline: bool, project: &Option<String>) -> AdapterRun {
    let parsed = match lekalo_core::adapter_package::DiscoverySource::parse(source) {
        Ok(parsed) => parsed,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let context = lekalo_core::adapter_package::ResolveContext {
        root: Some(root.clone()),
        offline,
    };
    let candidates = match lekalo_core::adapter_package::discover(&parsed, Some(&root)) {
        Ok(candidates) => candidates,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let mut rows = Vec::new();
    for candidate in &candidates {
        let manifest = &candidate.manifest;
        let mut row = serde_json::json!({
            "id": manifest.adapter_id(),
            "version": manifest.adapter_version().to_string(),
            "packageDigest": manifest.package_digest().as_str(),
            "manifestDigest": manifest.digest().as_str(),
            "sourceKind": manifest.source_kind().as_str(),
            "synthesized": candidate.synthesized,
            "status": manifest.status().as_str(),
        });
        // The assigned trust level, plus the gate verdict: failures are
        // surfaced honestly per candidate (devin F-11), never swallowed.
        let level = lekalo_core::adapter_package::assign_trust(candidate);
        row["trust"] = serde_json::json!(level.as_str());
        match lekalo_core::adapter_package::resolve_candidate(candidate.clone(), &context) {
            Ok(resolved) => {
                row["gates"] = serde_json::json!({
                    "integrity": true,
                    "signature": true,
                    "trust": resolved.trust.as_str(),
                });
            }
            Err(failure) => {
                // The failed gate is named, not flattened into a generic
                // integrity verdict (fix round 2, devin F-15): the reason
                // code and the gate label now agree.
                let gate = gate_label_of(&failure);
                row["gates"] = serde_json::json!({
                    gate: false,
                    "reason": lekalo_core::adapter_package::diagnostic::reason_of(&failure),
                });
            }
        }
        rows.push(row);
    }
    let document = serde_json::json!({
        "status": "valid",
        "schemaVersion": "lekalo/adapter-discovery/v0.3.2",
        "source": source,
        "offline": offline,
        "candidates": rows,
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("discovery receipt serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("discovery receipt serializes"),
            format!("discover {} : {} candidate(s)", source, rows.len()),
        ),
    }
}

/// Run `lekalo adapter list`: the installed inventory rows.
fn run_adapter_list(project: &Option<String>) -> AdapterRun {
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let rows: Vec<serde_json::Value> = inventory
        .rows()
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "version": row.version,
                "digest": row.digest,
                "trust": row.trust,
                "selected": row.selected,
                "quarantined": row.quarantined,
            })
        })
        .collect();
    let document = serde_json::json!({
        "status": "valid",
        "schemaVersion": lekalo_core::adapter_package::version::INVENTORY_SCHEMA_VERSION,
        "packages": rows,
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("inventory serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("inventory serializes"),
            format!("list : {} package(s)", rows.len()),
        ),
    }
}

/// Run `lekalo adapter info`: one package's manifest projection, trust,
/// and provenance from the store inventory.
fn run_adapter_info(id: &str, version: Option<&str>, project: &Option<String>) -> AdapterRun {
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let row = match version {
        Some(version) => inventory
            .rows()
            .iter()
            .find(|row| row.id == id && row.version == version && !row.quarantined),
        None => inventory.selected(id),
    };
    let Some(row) = row else {
        return AdapterRun::Envelope(DomainResult::from(
            lekalo_core::lockfile::LockFailure::ComponentUnavailable {
                kind: "adapter",
                id: id.to_owned(),
            },
        ));
    };
    let document = serde_json::json!({
        "status": "valid",
        "schemaVersion": lekalo_core::adapter_package::version::INVENTORY_SCHEMA_VERSION,
        "package": {
            "id": row.id,
            "version": row.version,
            "digest": row.digest,
            "manifestDigest": row.manifest_digest,
            "trust": row.trust,
            "source": row.source,
            "installPlanId": row.install_plan_id,
            "selected": row.selected,
            "quarantined": row.quarantined,
        },
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("info serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("info serializes"),
            format!("info {} {} : {}", row.id, row.version, row.trust),
        ),
    }
}

/// Resolve the project root for the adapter package surfaces.
fn project_root_for(project: &Option<String>) -> Result<std::path::PathBuf, DomainResult> {
    let selection = selection_for(project);
    lekalo_core::orchestration::project_root(&selection)
}

/// Render one install plan as the wire document (the dry-run output).
fn render_plan(plan: &lekalo_core::adapter_package::InstallPlan) -> String {
    let mut value = plan.to_json();
    value["planId"] = serde_json::json!(plan.plan_id);
    serde_json::to_string_pretty(&value).expect("plan serializes")
}

/// Run `lekalo adapter install`: plan (writes nothing) or confirm
/// (apply exactly that plan id).
/// Run `lekalo adapter install`: plan (writes nothing) or confirm
/// (apply exactly that plan id).
fn run_adapter_install(
    source: &str,
    dry_run: bool,
    confirm: Option<&str>,
    allow_escalation: bool,
    offline: bool,
    project: &Option<String>,
) -> AdapterRun {
    if !dry_run && confirm.is_none() {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::InstallPlanRequired,
        ));
    }
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let parsed = match lekalo_core::adapter_package::DiscoverySource::parse(source) {
        Ok(parsed) => parsed,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let context = lekalo_core::adapter_package::ResolveContext {
        root: Some(root.clone()),
        offline,
    };
    let resolved = match lekalo_core::adapter_package::resolve(&parsed, &context) {
        Ok(resolved) => resolved,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let current = inventory
        .selected(resolved.adapter_id())
        .and_then(|row| current_manifest(&root, row));
    let quarantined = resolved.trust.quarantined_at_install();
    let plan = lekalo_core::adapter_package::install::plan(
        &resolved.candidate,
        resolved.trust,
        quarantined,
        current.as_ref(),
    );
    if dry_run || confirm.is_none() {
        return AdapterRun::Document {
            document: render_plan(&plan),
            result: DomainResult::receipt(
                serde_json::to_string(&plan.to_json()).expect("plan serializes"),
                format!(
                    "install {} {} : plan {}",
                    plan.id, plan.version, plan.plan_id
                ),
            ),
        };
    }
    let confirmed = confirm.expect("checked above");
    match lekalo_core::adapter_package::install::apply_from_candidate(
        &root,
        &resolved.candidate,
        &plan,
        confirmed,
        allow_escalation,
    ) {
        Ok(()) => {
            let document = serde_json::json!({
                "status": "valid",
                "schemaVersion": lekalo_core::adapter_package::version::INSTALL_PLAN_SCHEMA_VERSION,
                "applied": plan.plan_id,
                "id": plan.id,
                "version": plan.version,
                "trust": plan.trust.as_str(),
                "quarantined": plan.quarantined,
            });
            AdapterRun::Document {
                document: serde_json::to_string_pretty(&document).expect("applies serializes"),
                result: DomainResult::receipt(
                    serde_json::to_string(&document).expect("applies serializes"),
                    format!(
                        "install {} {} : applied {}",
                        plan.id, plan.version, plan.plan_id
                    ),
                ),
            }
        }
        Err(rejection) => AdapterRun::Envelope(install_rejection(rejection)),
    }
}

/// The repoint family: update and rollback share the machinery. Both
/// operate over already-installed immutable versions; update resolves
/// the target version from the store inventory, rollback demands one.
#[allow(clippy::too_many_arguments)]
/// The repoint family: update and rollback share the machinery. Both
/// operate over already-installed immutable versions; update resolves
/// the target version from the store inventory, rollback demands one.
#[derive(Clone, Copy)]
enum AdapterRepoint {
    Update,
    Rollback,
}

#[allow(clippy::too_many_arguments)]
/// The forward-update selector for `adapter update` without `--to`:
/// the newest installed row with a version strictly greater than the
/// selected pin (or the newest row when nothing is selected). Pure and
/// unit-tested — the SemVer precedence decision and the corrupt-row
/// refusal are the exact logic the CLI surface executes (fix round 4,
/// devin N-3: a corrupt inventory row returns the inventory-corruption
/// diagnostic instead of panicking a comparator).
fn select_forward_update<'a>(
    promoted: &'a [lekalo_core::adapter_package::InventoryRow],
    selected: Option<&lekalo_core::adapter_package::InventoryRow>,
) -> Result<
    Option<&'a lekalo_core::adapter_package::InventoryRow>,
    lekalo_core::adapter_package::PackageFailure,
> {
    let corrupt = || lekalo_core::adapter_package::PackageFailure::RecoveryRequired {
        stage: "inventory".to_owned(),
    };
    let parse =
        |text: &str| lekalo_core::lockfile::types::SemVer::parse(text).map_err(|_| corrupt());
    let selected_ver = match selected {
        Some(current) => Some(parse(&current.version)?),
        None => None,
    };
    let mut versions: Vec<(&lekalo_core::adapter_package::InventoryRow, _)> = promoted
        .iter()
        .filter(|row| !row.selected)
        .map(|row| parse(&row.version).map(|parsed| (row, parsed)))
        .collect::<Result<_, _>>()?;
    versions.sort_by(|left, right| right.1.cmp(&left.1));
    Ok(versions
        .into_iter()
        .find(|(_, ver)| selected_ver.as_ref().map_or(true, |current| ver > current))
        .map(|(row, _)| row))
}

fn run_adapter_repoint(
    kind: AdapterRepoint,
    id: &str,
    to: Option<&str>,
    dry_run: bool,
    confirm: Option<&str>,
    allow_escalation: bool,
    project: &Option<String>,
) -> AdapterRun {
    if !dry_run && confirm.is_none() {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::InstallPlanRequired,
        ));
    }
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    // Resolve the target row among the installed (promoted) versions.
    let promoted: Vec<_> = inventory
        .rows()
        .iter()
        .filter(|row| row.id == id && !row.quarantined)
        .cloned()
        .collect();
    let target = match (kind, to) {
        (_, Some(version)) => promoted
            .iter()
            .find(|row| row.version == version)
            .or_else(|| inventory.selected(id).filter(|row| row.version == version)),
        // "update" without --to only ever moves forward: the newest
        // installed row newer than the selected one. With nothing newer
        // the command refuses (component-unavailable) rather than
        // silently downgrading (devin F-10).
        (AdapterRepoint::Update, None) => {
            match select_forward_update(&promoted, inventory.selected(id)) {
                Ok(target) => target,
                Err(failure) => {
                    return AdapterRun::Envelope(
                        lekalo_core::adapter_package::diagnostic::domain_result(&failure),
                    )
                }
            }
        }
        (AdapterRepoint::Rollback, None) => None,
    };
    let target = match target {
        Some(target) => target,
        None => {
            return AdapterRun::Envelope(DomainResult::from(
                lekalo_core::lockfile::LockFailure::ComponentUnavailable {
                    kind: "adapter",
                    id: id.to_owned(),
                },
            ))
        }
    };

    // Issue #32 fix round 2 (devin F-2): a revoked version can never be
    // repointed to — rollback and update both refuse before any plan
    // exists, so a revoked pin can never be resurrected or selected.
    {
        let store = match lekalo_core::adapter_package::trust::RevocationStore::load(&root) {
            Ok(store) => store,
            Err(failure) => {
                return AdapterRun::Envelope(
                    lekalo_core::adapter_package::diagnostic::domain_result(&failure),
                );
            }
        };
        if store.is_revoked(id, &target.version) {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &lekalo_core::adapter_package::PackageFailure::Revoked {
                    id: id.to_owned(),
                    version: target.version.clone(),
                },
            ));
        }
    }
    let current_row = inventory.selected(id);
    let current = current_row.and_then(|row| current_manifest(&root, row));
    // The repoint plan: one repoint action; the bytes already sit in the
    // immutable store, so no stage runs. The diff compares the currently
    // selected manifest with the target's stored manifest.
    let diff =
        current
            .as_ref()
            .zip(current_manifest(&root, target))
            .map(|(current, target_manifest)| {
                lekalo_core::adapter_package::diff::diff_manifests(current, &target_manifest)
            });
    let trust = lekalo_core::adapter_package::TrustLevel::parse(&target.trust)
        .unwrap_or(lekalo_core::adapter_package::TrustLevel::Community);
    let mut plan = lekalo_core::adapter_package::InstallPlan {
        id: id.to_owned(),
        version: target.version.clone(),
        digest: target.digest.clone(),
        manifest_digest: target.manifest_digest.clone(),
        manifest_bytes: Vec::new(),
        source: target.source.clone(),
        trust,
        quarantined: false,
        actions: vec![
            lekalo_core::adapter_package::install::InstallAction::Repoint {
                id: id.to_owned(),
                version: target.version.clone(),
            },
        ],
        diff,
        plan_id: String::new(),
    };
    plan.plan_id = plan.compute_plan_id();
    let verb = if matches!(kind, AdapterRepoint::Update) {
        "update"
    } else {
        "rollback"
    };
    if dry_run || confirm.is_none() {
        return AdapterRun::Document {
            document: render_plan(&plan),
            result: DomainResult::receipt(
                serde_json::to_string(&plan.to_json()).expect("plan serializes"),
                format!(
                    "{} {} {} : plan {}",
                    verb, plan.id, plan.version, plan.plan_id
                ),
            ),
        };
    }
    let confirmed = confirm.expect("checked above");
    if plan.compute_plan_id() != confirmed {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::SourceChanged,
        ));
    }
    if let Some(diff) = &plan.diff {
        if diff.escalated && !allow_escalation {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &lekalo_core::adapter_package::PackageFailure::PermissionEscalated {
                    adapter: id.to_owned(),
                    member: diff
                        .escalated_member
                        .clone()
                        .unwrap_or_else(|| "permissions".to_owned()),
                },
            ));
        }
    }
    let mut next = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(next) => next,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let applied = next
        .select(id, &target.version, &target.digest)
        .map_err(|_| ())
        .and_then(|()| next.store(&root).map_err(|_| ()));
    match applied {
        Ok(()) => {
            let document = serde_json::json!({
                "status": "valid",
                "schemaVersion": lekalo_core::adapter_package::version::INSTALL_PLAN_SCHEMA_VERSION,
                "applied": plan.plan_id,
                "id": plan.id,
                "version": plan.version,
                "selected": true,
            });
            AdapterRun::Document {
                document: serde_json::to_string_pretty(&document).expect("applies serializes"),
                result: DomainResult::receipt(
                    serde_json::to_string(&document).expect("applies serializes"),
                    format!(
                        "{} {} {} : applied {}",
                        verb, plan.id, plan.version, plan.plan_id
                    ),
                ),
            }
        }
        Err(()) => AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::RecoveryRequired {
                stage: "repoint".to_owned(),
            },
        )),
    }
}

/// Re-load one installed package's manifest from the store bytes.
fn current_manifest(
    root: &std::path::Path,
    row: &lekalo_core::adapter_package::inventory::InventoryRow,
) -> Option<lekalo_core::adapter_package::ManifestDocument> {
    let digest8: String = row.digest["sha256:".len()..].chars().take(8).collect();
    let dir = root.join(
        format!(
            ".lekalo/adapters/packages/{}/{}-{}",
            row.id, row.version, digest8
        )
        .replace('/', std::path::MAIN_SEPARATOR_STR),
    );
    let bytes =
        std::fs::read(dir.join(lekalo_core::adapter_package::integrity::MANIFEST_FILE)).ok()?;
    lekalo_core::adapter_package::ManifestDocument::from_bytes(&bytes).ok()
}

/// Map an apply rejection onto its registered envelope.
fn install_rejection(
    rejection: lekalo_core::adapter_package::install::ApplyRejection,
) -> DomainResult {
    use lekalo_core::adapter_package::install::ApplyRejection;
    use lekalo_core::adapter_package::PackageFailure;
    match rejection {
        ApplyRejection::SourceChanged => {
            lekalo_core::adapter_package::diagnostic::domain_result(&PackageFailure::SourceChanged)
        }
        ApplyRejection::PermissionEscalated { member } => {
            lekalo_core::adapter_package::diagnostic::domain_result(
                &PackageFailure::PermissionEscalated {
                    adapter: String::new(),
                    member,
                },
            )
        }
        ApplyRejection::InstallConflict { path } => {
            lekalo_core::adapter_package::diagnostic::domain_result(
                &PackageFailure::InstallConflict { path },
            )
        }
        ApplyRejection::RecoveryRequired { stage } => {
            lekalo_core::adapter_package::diagnostic::domain_result(
                &PackageFailure::RecoveryRequired { stage },
            )
        }
    }
}

/// The quarantine custody subcommands (issue #32).

#[derive(Debug, Subcommand)]

enum QuarantineCommands {
    /// List the quarantined packages.
    List {
        /// Project root selector, relative to the invocation directory.

        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },

    /// Release a quarantined package: re-plan it as a normal (trusted)
    /// install into the live store under explicit confirmation.
    Release {
        /// The adapter id.
        id: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },

    /// Purge quarantined bytes (the only removal path).
    Purge {
        /// Purge every quarantined package.

        #[arg(long)]
        all: bool,

        /// Project root selector, relative to the invocation directory.

        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The closed trust-level vocabulary accepted by `adapter trust`.

#[derive(Debug, Clone, Copy, clap::ValueEnum)]

enum AdapterTrustLevel {
    /// The project's own local development trust.
    LocalDevelopment,

    /// Community trust (quarantined posture).
    Community,
}

/// Run `lekalo adapter trust`: the explicit trust transition. The level
/// is never inferred and never widened by any other command.
fn run_adapter_trust(id: &str, level: AdapterTrustLevel, project: &Option<String>) -> AdapterRun {
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    // verified/builtin denote verified provenance; no shipped verifier
    // exists, so the CLI can never grant them (devin F-8). They are only
    // earned through a reviewed signature verifier or the distribution.
    let level_token = match level {
        AdapterTrustLevel::LocalDevelopment => "local-development",
        AdapterTrustLevel::Community => "community",
    };
    let mut inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let mut changed = 0usize;
    let rows: Vec<_> = inventory
        .rows()
        .iter()
        .filter(|row| row.id == id)
        .cloned()
        .collect();
    for mut row in rows {
        row.trust = level_token.to_owned();
        inventory.upsert(row);
        changed += 1;
    }
    if changed == 0 {
        return AdapterRun::Envelope(DomainResult::from(
            lekalo_core::lockfile::LockFailure::ComponentUnavailable {
                kind: "adapter",
                id: id.to_owned(),
            },
        ));
    }
    if let Err(failure) = inventory.store(&root) {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &failure,
        ));
    }
    let document = serde_json::json!({
        "status": "valid",
        "schemaVersion": lekalo_core::adapter_package::version::INVENTORY_SCHEMA_VERSION,
        "id": id,
        "trust": level_token,
        "packages": changed,
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("trust serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("trust serializes"),
            format!("trust {} : {}", id, level_token),
        ),
    }
}

/// Run `lekalo adapter revoke`: append a revocation record to the local
/// store. Revocation overrides every other trust signal.
fn run_adapter_revoke(
    id: &str,
    version: &str,
    reason: &str,
    project: &Option<String>,
) -> AdapterRun {
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let mut store = match lekalo_core::adapter_package::trust::RevocationStore::load(&root) {
        Ok(store) => store,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    if let Err(failure) = store.append(
        &root,
        lekalo_core::adapter_package::trust::RevocationRecord {
            id: id.to_owned(),
            version: version.to_owned(),
            reason: reason.to_owned(),
        },
    ) {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &failure,
        ));
    }
    let document = serde_json::json!({
        "status": "valid",
        "id": id,
        "version": version,
        "reason": reason,
        "revoked": true,
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("revoke serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("revoke serializes"),
            format!("revoke {} {} : {}", id, version, reason),
        ),
    }
}

/// Run `lekalo adapter quarantine list`: the quarantined inventory rows.
fn run_adapter_quarantine_list(project: &Option<String>) -> AdapterRun {
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let rows: Vec<serde_json::Value> = inventory
        .rows()
        .iter()
        .filter(|row| row.quarantined)
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "version": row.version,
                "digest": row.digest,
                "trust": row.trust,
            })
        })
        .collect();
    let count = rows.len();
    let document = serde_json::json!({
        "status": "valid",
        "schemaVersion": lekalo_core::adapter_package::version::INVENTORY_SCHEMA_VERSION,
        "quarantined": rows,
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("quarantine serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("quarantine serializes"),
            format!("quarantine list : {} package(s)", count),
        ),
    }
}

/// Run `lekalo adapter quarantine purge`: the only removal path for
/// quarantined bytes (never an automatic deletion).
fn run_adapter_quarantine_purge(all: bool, project: &Option<String>) -> AdapterRun {
    if !all {
        return AdapterRun::Envelope(DomainResult::usage_error());
    }
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    match quarantine_purge_all(&root) {
        Ok(purged) => {
            let document = serde_json::json!({
                "status": "valid",
                "purged": purged,
            });
            AdapterRun::Document {
                document: serde_json::to_string_pretty(&document).expect("purge serializes"),
                result: DomainResult::receipt(
                    serde_json::to_string(&document).expect("purge serializes"),
                    format!("quarantine purge : {} package(s)", purged),
                ),
            }
        }
        Err(failure) => AdapterRun::Envelope(
            lekalo_core::adapter_package::diagnostic::domain_result(&failure),
        ),
    }
}

/// The purge core over one project root (pure filesystem + inventory
/// logic, unit-testable without spawning — fix round 4, cline F-NEW-4):
/// removes every quarantined custody tree, cleans emptied per-id
/// parents, and drops the matching inventory rows. Returns the count of
/// purged packages.
fn quarantine_purge_all(
    root: &std::path::Path,
) -> Result<usize, lekalo_core::adapter_package::PackageFailure> {
    let mut inventory = lekalo_core::adapter_package::Inventory::load(root)?;
    let quarantined: Vec<_> = inventory
        .rows()
        .iter()
        .filter(|row| row.quarantined)
        .cloned()
        .collect();
    for row in &quarantined {
        // Remove every location the bytes can occupy: the quarantine
        // custody tree and any pre-fix orphan under packages/**.
        let quarantine_dir = root.join(
            lekalo_core::adapter_package::quarantine::quarantine_path(
                &row.id,
                &row.version,
                &row.digest,
            )
            .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let packages_dir = root.join(
            lekalo_core::adapter_package::quarantine::package_path(
                &row.id,
                &row.version,
                &row.digest,
            )
            .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let _ = std::fs::remove_dir_all(&quarantine_dir);
        let _ = std::fs::remove_dir_all(&packages_dir);
        // The per-id parent directories are custody scaffolding: once
        // empty they are removed too (fix round 4, cline F-NEW-3), so
        // the store carries no residue shells. remove_dir only succeeds
        // when the directory is empty, so other versions are untouched.
        for custody_dir in [&quarantine_dir, &packages_dir] {
            if let Some(parent) = custody_dir.parent() {
                let _ = std::fs::remove_dir(parent);
            }
        }
        inventory
            .rows_mut()
            .retain(|existing| existing.id != row.id || existing.version != row.version);
    }
    inventory.store(root)?;
    Ok(quarantined.len())
}

/// Run `lekalo adapter quarantine release`: move the quarantined bytes
/// into the live packages/** store and select the pin. The explicit
/// transition out of quarantine — never implicit.
fn run_adapter_quarantine_release(id: &str, project: &Option<String>) -> AdapterRun {
    let root = match project_root_for(project) {
        Ok(root) => root,
        Err(result) => return AdapterRun::Envelope(result),
    };
    let mut inventory = match lekalo_core::adapter_package::Inventory::load(&root) {
        Ok(inventory) => inventory,
        Err(failure) => {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ))
        }
    };
    let row = match inventory
        .rows()
        .iter()
        .find(|row| row.id == id && row.quarantined)
    {
        Some(row) => row.clone(),
        None => {
            return AdapterRun::Envelope(DomainResult::from(
                lekalo_core::lockfile::LockFailure::ComponentUnavailable {
                    kind: "adapter",
                    id: id.to_owned(),
                },
            ));
        }
    };
    // Issue #32 fix round 2 (devin F-4 / cline F-2): use the shared
    // custody-path helpers for both sides of the promotion.
    let digest8: String = row.digest["sha256:".len()..].chars().take(8).collect();
    let quarantine_dir = root.join(
        lekalo_core::adapter_package::quarantine::quarantine_path(
            &row.id,
            &row.version,
            &row.digest,
        )
        .replace('/', std::path::MAIN_SEPARATOR_STR),
    );
    let packages_dir = root.join(
        lekalo_core::adapter_package::quarantine::package_path(&row.id, &row.version, &row.digest)
            .replace('/', std::path::MAIN_SEPARATOR_STR),
    );
    if !quarantine_dir.is_dir() {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::Quarantined {
                id: id.to_owned(),
                version: row.version.clone(),
            },
        ));
    }
    if packages_dir.exists() {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::InstallConflict {
                path: format!("packages/{}/{}-{}", id, row.version, digest8),
            },
        ));
    }
    // Re-verify the quarantined bytes against the recorded digests before
    // promoting (fix round 2, devin F-4 residual): custody never promotes
    // bytes the integrity gate has not re-checked in quarantine.
    {
        let manifest_path =
            quarantine_dir.join(lekalo_core::adapter_package::integrity::MANIFEST_FILE);
        let quarantined_failure = |_| lekalo_core::adapter_package::PackageFailure::Quarantined {
            id: id.to_owned(),
            version: row.version.clone(),
        };
        let manifest_bytes = std::fs::read(&manifest_path)
            .map_err(quarantined_failure)
            .map_err(|failure| {
                AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                    &failure,
                ))
            });
        let manifest_bytes = match manifest_bytes {
            Ok(bytes) => bytes,
            Err(envelope) => return envelope,
        };
        let verified = lekalo_core::adapter_package::ManifestDocument::from_bytes(&manifest_bytes)
            .and_then(|manifest| {
                let candidate = lekalo_core::adapter_package::discovery::DiscoveryCandidate {
                    manifest,
                    package_root: Some(quarantine_dir.clone()),
                    synthesized: false,
                    // Custody is not consulted by verify_package (pure
                    // digest-domain re-check); the value only satisfies
                    // the closed candidate grammar.
                    custody: lekalo_core::adapter_package::discovery::Custody::Record,
                };
                lekalo_core::adapter_package::verify_package(&candidate)
            });
        if let Err(failure) = verified {
            return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
                &failure,
            ));
        }
    }
    if let Some(parent) = packages_dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::rename(&quarantine_dir, &packages_dir).is_err() {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &lekalo_core::adapter_package::PackageFailure::RecoveryRequired {
                stage: "release".to_owned(),
            },
        ));
    }
    // The emptied quarantine <id> parent is custody scaffolding; remove
    // it when empty (fix round 4, cline F-NEW-3). remove_dir only
    // succeeds on an empty directory, so sibling versions are safe.
    if let Some(parent) = quarantine_dir.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    inventory.quarantine_release(id);
    if let Err(failure) = inventory.select(id, &row.version, &row.digest) {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &failure,
        ));
    }
    if let Err(failure) = inventory.store(&root) {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &failure,
        ));
    }
    let document = serde_json::json!({
        "status": "valid",
        "id": id,
        "version": row.version,
        "released": true,
        "selected": true,
    });
    AdapterRun::Document {
        document: serde_json::to_string_pretty(&document).expect("release serializes"),
        result: DomainResult::receipt(
            serde_json::to_string(&document).expect("release serializes"),
            format!("quarantine release {} : released {}", id, row.version),
        ),
    }
}

/// Run `lekalo adapter test` (the issue #31 conformance battery behind
/// the issue #32 resolution gate).
fn run_adapter_test(
    profile: AdapterTestProfile,
    report: Option<AdapterTestReport>,
    repeats: u8,
    timeout_ms: u64,
    program_args: Vec<String>,
) -> AdapterRun {
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
    // The issue #32 resolution gate: synthesize the implicit descriptor
    // and run every gate. A gate refusal renders its registered adapter
    // rule and never spawns the adapter.
    if let Err(failure) = gate_adapter_command(&command.program, &command.args) {
        return AdapterRun::Envelope(lekalo_core::adapter_package::diagnostic::domain_result(
            &failure,
        ));
    }
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

/// The issue #32 resolution gate for one invocation-supplied adapter
/// command: prefer a real `adapter.manifest.json` beside the launched
/// entry (the same manifested-preference as the catalog seam), else
/// synthesize the implicit local-development descriptor from the entry
/// (the executable, or its first argument when that names an existing
/// regular file — the same interpreter-script convention as the catalog
/// seam), and run the integrity, signature, and trust gates. The project
/// revocation store scopes the trust gate (fix round 2, devin F-11:
/// `adapter test` no longer evaluates an empty store). A refusal returns
/// the packaged failure; the caller renders its registered `adapter.*`
/// rule and never spawns the adapter.
fn gate_adapter_command(
    program: &std::path::Path,
    args: &[String],
) -> Result<
    lekalo_core::adapter_package::ResolvedAdapter,
    lekalo_core::adapter_package::PackageFailure,
> {
    let entry = args
        .first()
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| program.to_path_buf());
    let entry_dir = entry
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let context = lekalo_core::adapter_package::ResolveContext {
        root: std::env::current_dir()
            .ok()
            .and_then(|cwd| lekalo_core::project_fs::Fs::find_root(&cwd).ok().flatten()),
        offline: true,
    };
    let manifested = if entry_dir.join("adapter.manifest.json").is_file() {
        lekalo_core::adapter_package::discover(
            &lekalo_core::adapter_package::DiscoverySource::Path(entry_dir.clone()),
            context.root.as_ref(),
        )?
        .into_iter()
        .next()
    } else {
        None
    };
    let candidate = match manifested {
        Some(candidate) => candidate,
        None => lekalo_core::adapter_package::implicit_local_development(&entry)?,
    };
    lekalo_core::adapter_package::resolve_candidate(candidate, &context)
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

/// Parse the supplied `--trace` manifests into the typed evidence
/// handoff. The files are read here (the same seam `lekalo trace` owns)
/// and every parse decision stays in the core; paths never cross.
fn doctor_traces(paths: &[String]) -> Vec<lekalo_core::doctor::TraceManifestEvidence> {
    paths
        .iter()
        .map(|path| {
            let evidence = match std::fs::read(path) {
                Err(_) => lekalo_core::doctor::TraceManifestEvidence {
                    reason_ids: vec!["loader.io".to_owned()],
                    gaps: 0,
                    external_refs: Vec::new(),
                },
                Ok(bytes) => match lekalo_core::trace::TraceManifest::parse(&bytes) {
                    Err(diagnostics) => lekalo_core::doctor::TraceManifestEvidence {
                        reason_ids: diagnostics
                            .reason_ids()
                            .into_iter()
                            .map(str::to_owned)
                            .collect(),
                        gaps: 0,
                        external_refs: Vec::new(),
                    },
                    Ok(manifest) => {
                        let report = manifest.report();
                        let mut refs: Vec<String> = manifest
                            .manifest()
                            .nodes
                            .iter()
                            .flat_map(|node| node.external_refs.iter())
                            .map(|reference| reference.system.as_str().to_owned())
                            .collect();
                        refs.sort();
                        refs.dedup();
                        lekalo_core::doctor::TraceManifestEvidence {
                            reason_ids: Vec::new(),
                            gaps: report.gap_count,
                            external_refs: refs,
                        }
                    }
                },
            };
            evidence
        })
        .collect()
}

/// The Git facts of one selection: unavailable when the project root
/// itself cannot be resolved, otherwise the read-only adapter handoff.
fn doctor_git_facts(selection: &LoadSelection) -> lekalo_core::doctor::GitFacts {
    match lekalo_core::doctor::project_root(selection) {
        Err(_) => lekalo_core::doctor::GitFacts {
            state: lekalo_core::doctor::GitState::Unavailable,
            commit: None,
            dirty: None,
        },
        Ok(root) => doctor_git::git_facts(&root),
    }
}

/// `lekalo doctor`: the full read-only readiness report. `--fix` only
/// previews the closed safe-fix recipes; nothing is ever written.
fn run_doctor(project: Option<String>, fix: bool, traces: Vec<String>) -> DomainResult {
    let selection = selection_for(&project);
    let git = doctor_git_facts(&selection);
    let evidence = doctor_traces(&traces);
    let options = lekalo_core::doctor::Options {
        kind: lekalo_core::doctor::model::ReportKind::Doctor,
        phase: None,
        fix,
        traces: evidence,
    };
    lekalo_core::doctor::report(&selection, &git, &options)
}

/// `lekalo status`: the freshness panel plus the exact revisions.
fn run_status(project: Option<String>) -> DomainResult {
    let selection = selection_for(&project);
    let git = doctor_git_facts(&selection);
    let options = lekalo_core::doctor::Options {
        kind: lekalo_core::doctor::model::ReportKind::Status,
        ..lekalo_core::doctor::Options::default()
    };
    lekalo_core::doctor::report(&selection, &git, &options)
}

/// `lekalo readiness --phase PHASE`: the phase-gated readiness report.
fn run_readiness(
    phase: lekalo_core::doctor::model::Phase,
    project: Option<String>,
    traces: Vec<String>,
) -> DomainResult {
    let selection = selection_for(&project);
    let git = doctor_git_facts(&selection);
    let evidence = doctor_traces(&traces);
    let options = lekalo_core::doctor::Options {
        kind: lekalo_core::doctor::model::ReportKind::Readiness,
        phase: Some(phase),
        fix: false,
        traces: evidence,
    };
    lekalo_core::doctor::report(&selection, &git, &options)
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

/// The selected NFR operation, resolved before the attachment is read.
enum NfrStep {
    Validate { strict: bool },
    Report,
    Query(String),
}

/// Run one `lekalo nfr` operation: parse the attachment and every
/// evidence document, resolve against the selected project, and emit
/// the requested view. The core owns every decision; this binary only
/// selects, renders, and maps exits.
fn run_nfr(command: NfrCommands) -> DomainResult {
    // The diff and impact operations never resolve evidence: the
    // diff is a pure two-document comparison, and impact synthesizes
    // its typed changed-input handoff from two documents.
    match &command {
        NfrCommands::Diff { base, candidate } => return nfr_diff(base, candidate),
        NfrCommands::Trace {
            path,
            evidence,
            as_of,
            project,
        } => {
            let as_of = match lekalo_core::nfr::IsoDate::parse(as_of) {
                Ok(as_of) => as_of,
                Err(_) => return DomainResult::usage_error(),
            };
            return nfr_trace(path, evidence, &as_of, project);
        }
        NfrCommands::Impact {
            path,
            base,
            depth,
            project,
        } => return nfr_impact(path, base, *depth, project),
        _ => {}
    }
    let (path, evidence_paths, as_of, project, step) = match command {
        NfrCommands::Validate {
            path,
            evidence,
            strict,
            as_of,
            project,
        } => (path, evidence, as_of, project, NfrStep::Validate { strict }),
        NfrCommands::Report {
            path,
            evidence,
            as_of,
            project,
        } => (path, evidence, as_of, project, NfrStep::Report),
        NfrCommands::Query {
            path,
            selector,
            evidence,
            as_of,
            project,
        } => (path, evidence, as_of, project, NfrStep::Query(selector)),
        NfrCommands::Diff { .. } | NfrCommands::Impact { .. } | NfrCommands::Trace { .. } => {
            unreachable!("diff, impact, and trace are handled before the resolution path")
        }
    };
    let as_of = match lekalo_core::nfr::IsoDate::parse(&as_of) {
        Ok(as_of) => as_of,
        Err(_) => {
            return DomainResult::usage_error();
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return DomainResult::invalid(lekalo_core::nfr::diagnostic::io_failure(detail));
        }
    };
    let attachment = match lekalo_core::nfr::NfrAttachment::parse(&bytes) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let mut evidence: Vec<lekalo_core::nfr::EvidenceSet> = Vec::new();
    for evidence_path in &evidence_paths {
        let bytes = match std::fs::read(evidence_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                let detail = match error.kind() {
                    io::ErrorKind::NotFound => "file-missing",
                    _ => "file-unreadable",
                };
                return DomainResult::unavailable(
                    lekalo_core::nfr::diagnostic::evidence_unavailable(detail, None),
                );
            }
        };
        match lekalo_core::nfr::EvidenceSet::parse(&bytes) {
            Ok(set) => evidence.push(set),
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        }
    }
    let refs: Vec<&lekalo_core::nfr::EvidenceSet> = evidence.iter().collect();
    let capabilities = nfr_capability_snapshot(&project);
    let selection = selection_for(&project);
    let resolution =
        match lekalo_core::nfr::resolve(&attachment, &refs, &capabilities, &as_of, &selection) {
            Ok(resolution) => resolution,
            Err(result) => return result,
        };
    match step {
        NfrStep::Validate { strict } => nfr_validate(
            &resolution,
            if strict {
                lekalo_core::nfr::GateProfile::Strict
            } else {
                lekalo_core::nfr::GateProfile::Default
            },
        ),
        NfrStep::Report => nfr_report(&resolution.report),
        NfrStep::Query(selector) => nfr_query(&resolution, &selector),
    }
}

/// `lekalo nfr diff`: the pure semantic comparison of two same-family
/// attachments; the verdict stays data.
fn nfr_diff(base_path: &str, candidate_path: &str) -> DomainResult {
    let read = |path: &str| -> Result<lekalo_core::nfr::NfrAttachment, DomainResult> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                let detail = match error.kind() {
                    io::ErrorKind::NotFound => "file-missing",
                    _ => "file-unreadable",
                };
                return Err(DomainResult::invalid(
                    lekalo_core::nfr::diagnostic::io_failure(detail),
                ));
            }
        };
        lekalo_core::nfr::NfrAttachment::parse(&bytes).map_err(DomainResult::invalid)
    };
    let base = match read(base_path) {
        Ok(base) => base,
        Err(result) => return result,
    };
    let candidate = match read(candidate_path) {
        Ok(candidate) => candidate,
        Err(result) => return result,
    };
    let diff = match lekalo_core::nfr::diff::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let paths: Vec<String> = diff
        .paths()
        .iter()
        .map(|path| {
            format!(
                "{{\"path\":\"{}\",\"class\":\"{}\"}}",
                path.path(),
                path.class().key()
            )
        })
        .collect();
    let class = if diff
        .paths()
        .iter()
        .any(|path| path.class() == lekalo_core::nfr::diff::DiffClass::Breaking)
    {
        "breaking"
    } else {
        "compatible"
    };
    let json = format!(
        "{{\"status\":\"valid\",\"diff\":{{\"equal\":{},\"verdict\":\"{}\",\"paths\":[{}]}}}}",
        diff.equal(),
        class,
        paths.join(","),
    );
    let mut human = format!("nfr diff equal {}", diff.equal());
    for path in diff.paths() {
        human.push_str(&format!(
            "
#   {} {}",
            path.path(),
            path.class().key()
        ));
    }
    DomainResult::diff(json, human, Vec::new())
}

/// `lekalo nfr trace`: the neutral #22 trace-manifest projection,
/// validated by the accepted trace validator and emitted as canonical
/// bytes with their digest.
fn nfr_trace(
    path: &str,
    evidence_paths: &[String],
    as_of: &lekalo_core::nfr::IsoDate,
    project: &Option<String>,
) -> DomainResult {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return DomainResult::invalid(lekalo_core::nfr::diagnostic::io_failure(detail));
        }
    };
    let attachment = match lekalo_core::nfr::NfrAttachment::parse(&bytes) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let mut evidence: Vec<lekalo_core::nfr::EvidenceSet> = Vec::new();
    for evidence_path in evidence_paths {
        let bytes = match std::fs::read(evidence_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                let detail = match error.kind() {
                    io::ErrorKind::NotFound => "file-missing",
                    _ => "file-unreadable",
                };
                return DomainResult::unavailable(
                    lekalo_core::nfr::diagnostic::evidence_unavailable(detail, None),
                );
            }
        };
        match lekalo_core::nfr::EvidenceSet::parse(&bytes) {
            Ok(set) => evidence.push(set),
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        }
    }
    let refs: Vec<&lekalo_core::nfr::EvidenceSet> = evidence.iter().collect();
    let capabilities = nfr_capability_snapshot(project);
    let selection = selection_for(project);
    let resolution =
        match lekalo_core::nfr::resolve(&attachment, &refs, &capabilities, as_of, &selection) {
            Ok(resolution) => resolution,
            Err(result) => return result,
        };
    let manifest = match resolution.report.trace_manifest() {
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

/// `lekalo nfr impact`: the changed constraints' scope symbols enter
/// the accepted impact engine as a synthesized typed handoff; the
/// standard impact payload carries an NFR provenance section.
fn nfr_impact(path: &str, base_path: &str, depth: u16, project: &Option<String>) -> DomainResult {
    let read = |source: &str| -> Result<lekalo_core::nfr::NfrAttachment, DomainResult> {
        let bytes = match std::fs::read(source) {
            Ok(bytes) => bytes,
            Err(error) => {
                let detail = match error.kind() {
                    io::ErrorKind::NotFound => "file-missing",
                    _ => "file-unreadable",
                };
                return Err(DomainResult::invalid(
                    lekalo_core::nfr::diagnostic::io_failure(detail),
                ));
            }
        };
        lekalo_core::nfr::NfrAttachment::parse(&bytes).map_err(DomainResult::invalid)
    };
    let base = match read(base_path) {
        Ok(base) => base,
        Err(result) => return result,
    };
    let candidate = match read(path) {
        Ok(candidate) => candidate,
        Err(result) => return result,
    };
    let changed = match lekalo_core::nfr::impact::changed_input_set(&base, &candidate, path) {
        Ok(Some(set)) => set,
        Ok(None) => return DomainResult::invalid(lekalo_core::nfr::diagnostic::projection_empty()),
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let request = match lekalo_core::impact::ImpactRequest::for_changed().with_depth(depth) {
        Ok(request) => request,
        Err(set) => return DomainResult::invalid(set),
    };
    let selection = selection_for(project);
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
    match lekalo_core::impact::analyze(
        &compilation.project,
        &graph,
        &effects,
        &request,
        Some(&changed),
        None,
    ) {
        Ok(result) => {
            // The standard impact payload plus the NFR provenance
            // section: which constraints changed and which symbols
            // entered the radius.
            let bytes = match result.to_canonical_json() {
                Ok(bytes) => bytes,
                Err(set) => return DomainResult::invalid(set),
            };
            let changed_constraints: Vec<String> = changed
                .entries()
                .iter()
                .flat_map(|entry| entry.symbol_ids().iter().cloned())
                .collect();
            let json = format!(
                "{{\"status\":\"valid\",\"impact\":{bytes},\"nfr\":{{\"attachment\":\"{}\",\"changedScopeSymbols\":{}}}}}",
                path,
                serde_json::to_string(&changed_constraints)
                    .unwrap_or_else(|_| "[]".to_owned()),
            );
            let mut human = format!(
                "nfr impact: changed scope symbols {}",
                changed_constraints.join(", ")
            );
            human.push_str(&format!(
                "
#   direct {} transitive {} gates {}",
                result.direct().summary.returned,
                result.transitive().summary.returned,
                result.gates().summary.returned,
            ));
            DomainResult::impact(json, human, result.warnings().to_vec())
        }
        Err(lekalo_core::impact::ImpactFailure::Invalid(set)) => DomainResult::invalid(set),
        Err(lekalo_core::impact::ImpactFailure::Denied(set)) => DomainResult::denied(set),
    }
}

/// The resolved capability snapshot: the committed project lock's
/// capability table when a valid lock exists, otherwise the explicit
/// fact that no profile was resolved.
fn nfr_capability_snapshot(project: &Option<String>) -> lekalo_core::nfr::CapabilitySnapshot {
    let selection = selection_for(project);
    let root = match lekalo_core::orchestration::project_root(&selection) {
        Ok(root) => root,
        Err(_) => return lekalo_core::nfr::CapabilitySnapshot::unresolved(),
    };
    match lekalo_core::lockfile::plan::LockService::read_state_at(&root) {
        Ok(lekalo_core::lockfile::LockState::Present(lock)) => {
            let capabilities = lock
                .capabilities()
                .iter()
                .filter_map(|capability| {
                    let support = match capability.support() {
                        lekalo_core::lockfile::types::Support::Full => {
                            lekalo_core::nfr::Support::Full
                        }
                        lekalo_core::lockfile::types::Support::Partial => {
                            lekalo_core::nfr::Support::Partial
                        }
                        // Unsupported and unknown never satisfy a
                        // requirement: they stay absent.
                        _ => return None,
                    };
                    Some((capability.id().as_str().to_owned(), support))
                })
                .collect();
            lekalo_core::nfr::CapabilitySnapshot::resolved(capabilities)
        }
        _ => lekalo_core::nfr::CapabilitySnapshot::unresolved(),
    }
}

/// `lekalo nfr validate`: the gate summary plus the verdict; exit 0
/// pass, 3 denied (the aggregated gate set), 1/4 for the terminal
/// failures mapped before this point.
fn nfr_validate(
    resolution: &lekalo_core::nfr::Resolution,
    profile: lekalo_core::nfr::GateProfile,
) -> DomainResult {
    use lekalo_core::nfr::ResolutionVerdict;
    let report = &resolution.report;
    let rows = report
        .runtime
        .constraints
        .iter()
        .chain(report.ai_budget.constraints.iter());
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.status).or_insert(0usize) += 1;
    }
    let count = |status: &str| counts.get(status).copied().unwrap_or(0);
    let verdict = resolution.verdict_for(profile);
    let json = format!(
        "{{\"status\":\"valid\",\"nfr\":{{\"projectId\":\"{}\",\"constraintCount\":{},\"satisfied\":{},\"violated\":{},\"unverified\":{},\"stale\":{},\"foreignEnvironment\":{},\"openQuestion\":{},\"unsupported\":{},\"conflict\":{},\"capabilitiesResolved\":{},\"profile\":\"{}\"}}}}",
        report.project_id,
        count_all(report),
        count("satisfied"),
        count("violated"),
        count("unverified"),
        count("stale"),
        count("foreign-environment"),
        count("open-question"),
        count("unsupported"),
        count("conflict"),
        report.capabilities_resolved,
        profile.key(),
    );
    let human = format!(
        "nfr {}\n#   constraints {}; satisfied {}; violated {}; unverified {}; stale {}; \
         foreign {}; open questions {}; unsupported {}; conflict {} (profile {}, as-of {})",
        report.project_id,
        count_all(report),
        count("satisfied"),
        count("violated"),
        count("unverified"),
        count("stale"),
        count("foreign-environment"),
        count("open-question"),
        count("unsupported"),
        count("conflict"),
        profile.key(),
        report.as_of,
    );
    let warnings = resolution.warnings().to_vec();
    match verdict {
        ResolutionVerdict::Pass => DomainResult::graph(json, human, warnings),
        ResolutionVerdict::Denied(diagnostics) => DomainResult::denied(diagnostics),
    }
}

/// The total constraint row count across both dimension sections.
fn count_all(report: &lekalo_core::nfr::Report) -> usize {
    report.runtime.constraints.len() + report.ai_budget.constraints.len()
}

/// `lekalo nfr report`: the canonical report bytes are the export;
/// JSON output embeds the same bytes as a value plus the digest.
fn nfr_report(report: &lekalo_core::nfr::Report) -> DomainResult {
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

/// One closed NFR query selector.
enum NfrSelection {
    /// The whole report.
    Report,
    /// Constraints in one status.
    Status(&'static str),
    /// The foreign-evidence rows.
    ForeignEnvironment,
    /// The registered open questions.
    OpenQuestions,
    /// The coverage gaps.
    CoverageGaps,
    /// One constraint by id.
    Constraint(String),
    /// Every constraint whose scope names one symbol.
    Symbol(String),
}

/// Parse one closed NFR query selector.
fn nfr_selection(selector: &str) -> Option<NfrSelection> {
    match selector {
        "report" => return Some(NfrSelection::Report),
        "foreign-environment" => return Some(NfrSelection::ForeignEnvironment),
        "open-questions" => return Some(NfrSelection::OpenQuestions),
        "coverage-gaps" => return Some(NfrSelection::CoverageGaps),
        _ => {}
    }
    if let Some(constraint) = selector.strip_prefix("constraint:") {
        return Some(NfrSelection::Constraint(constraint.to_owned()));
    }
    if let Some(symbol) = selector.strip_prefix("symbol:") {
        return Some(NfrSelection::Symbol(symbol.to_owned()));
    }
    for status in [
        "unverified",
        "stale",
        "satisfied",
        "violated",
        "open-question",
        "unsupported",
        "conflict",
    ] {
        if selector == status {
            return Some(NfrSelection::Status(status));
        }
    }
    None
}

/// `lekalo nfr query`: the closed selectors answered from the resolved
/// report; an unknown selector or subject is the stable usage or
/// unknown failure, never an empty success.
fn nfr_query(resolution: &lekalo_core::nfr::Resolution, selector: &str) -> DomainResult {
    let selection = match nfr_selection(selector) {
        Some(selection) => selection,
        None => return DomainResult::usage_error(),
    };
    let report = &resolution.report;
    let all = || {
        report
            .runtime
            .constraints
            .iter()
            .chain(report.ai_budget.constraints.iter())
    };
    let render = |rows: serde_json::Value, human: String| {
        let json = format!(
            "{{\"status\":\"valid\",\"nfr\":{}}}",
            serde_json::to_string(&rows).unwrap_or_else(|_| "null".to_owned())
        );
        DomainResult::graph(json, human, Vec::new())
    };
    match selection {
        NfrSelection::Report => nfr_report(report),
        NfrSelection::Status(status) => {
            let ids: Vec<String> = all()
                .filter(|row| row.status == status)
                .map(|row| row.constraint_id.clone())
                .collect();
            if ids.is_empty() {
                return DomainResult::invalid(lekalo_core::nfr::diagnostic::projection_empty());
            }
            render(
                serde_json::json!({ "selector": selector, "constraints": ids }),
                format!("nfr {selector}: {} constraints", ids.len()),
            )
        }
        NfrSelection::ForeignEnvironment => {
            if report.foreign_evidence.is_empty() {
                return DomainResult::invalid(lekalo_core::nfr::diagnostic::projection_empty());
            }
            render(
                serde_json::json!({ "selector": "foreign-environment", "rows": report.foreign_evidence }),
                format!(
                    "nfr foreign-environment: {} rows",
                    report.foreign_evidence.len()
                ),
            )
        }
        NfrSelection::OpenQuestions => {
            if report.open_questions.is_empty() {
                return DomainResult::invalid(lekalo_core::nfr::diagnostic::projection_empty());
            }
            render(
                serde_json::json!({ "selector": "open-questions", "rows": report.open_questions }),
                format!("nfr open questions: {}", report.open_questions.len()),
            )
        }
        NfrSelection::CoverageGaps => {
            if report.coverage_gaps.is_empty() {
                return DomainResult::invalid(lekalo_core::nfr::diagnostic::projection_empty());
            }
            render(
                serde_json::json!({ "selector": "coverage-gaps", "rows": report.coverage_gaps }),
                format!("nfr coverage gaps: {}", report.coverage_gaps.len()),
            )
        }
        NfrSelection::Constraint(constraint) => {
            let row = all().find(|row| row.constraint_id == constraint);
            let row = match row {
                Some(row) => row,
                None => return DomainResult::usage_error(),
            };
            render(
                serde_json::json!({ "constraint": row }),
                format!("nfr {}: {} ({})", row.constraint_id, row.status, row.kind),
            )
        }
        NfrSelection::Symbol(symbol) => {
            let ids: Vec<String> = all()
                .filter(|row| row.scope.r#ref == symbol)
                .map(|row| row.constraint_id.clone())
                .collect();
            if ids.is_empty() {
                return DomainResult::invalid(lekalo_core::nfr::diagnostic::projection_empty());
            }
            render(
                serde_json::json!({ "symbol": symbol, "constraints": ids }),
                format!("nfr symbol {symbol}: {} constraints", ids.len()),
            )
        }
    }
}

/// Read one transport attachment document from disk with a classified
/// read-only failure routed through the transport family.
fn read_transport_document(path: &str) -> Result<serde_json::Value, DomainResult> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return Err(DomainResult::invalid(
                lekalo_core::transport_http::io_failure(detail),
            ));
        }
    };
    serde_json::from_slice(&bytes)
        .map_err(|_| DomainResult::invalid(lekalo_core::transport_http::io_failure("invalid-json")))
}

/// Run one `lekalo transport` operation. The core owns every
/// decision; this binary only reads the documents, selects, renders,
/// and maps exits.
fn run_transport(command: TransportCommands) -> DomainResult {
    match command {
        TransportCommands::Validate {
            path,
            project,
            errors,
            query_model,
            strict,
        } => transport_validate(
            &path,
            &project,
            errors.as_deref(),
            query_model.as_deref(),
            strict,
        ),
        TransportCommands::Inspect {
            path,
            endpoint,
            project,
        } => transport_inspect(&path, &endpoint, &project),
        TransportCommands::Project {
            path,
            namespace,
            project,
            errors,
            query_model,
        } => transport_project(
            &path,
            namespace.as_str(),
            &project,
            errors.as_deref(),
            query_model.as_deref(),
        ),
        TransportCommands::Diff { base, candidate } => transport_diff(&base, &candidate),
    }
}

/// Read one optional context document from disk.
fn read_context_document(path: Option<&str>) -> Result<Option<serde_json::Value>, DomainResult> {
    match path {
        None => Ok(None),
        Some(path) => {
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let detail = match error.kind() {
                        io::ErrorKind::NotFound => "file-missing",
                        _ => "file-unreadable",
                    };
                    return Err(DomainResult::invalid(
                        lekalo_core::transport_http::io_failure(detail),
                    ));
                }
            };
            serde_json::from_slice(&bytes).map(Some).map_err(|_| {
                DomainResult::invalid(lekalo_core::transport_http::io_failure("invalid-json"))
            })
        }
    }
}

/// One loaded transport session: the attachment, the compiled
/// project, the bound registry, the optional query model, and the
/// validated context every transport command shares.
struct TransportSession {
    attachment: lekalo_core::transport_http::TransportDocument,
    compilation: lekalo_core::ir::Compilation,
    registry: lekalo_core::error_contract::ErrorRegistry,
    query_model: Option<lekalo_core::query_model::QueryModelAttachment>,
    capabilities: lekalo_core::transport_http::CapabilityMap,
}

/// Load and bind one transport session: the document, the project
/// (load, compile, custody), the error registry, and the optional
/// query model.
fn transport_session(
    path: &str,
    project: &Option<String>,
    errors_path: Option<&str>,
    query_model_path: Option<&str>,
) -> Result<TransportSession, DomainResult> {
    let document = read_transport_document(path)?;
    let attachment = match lekalo_core::transport_http::TransportDocument::from_value(&document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return Err(DomainResult::invalid(diagnostics)),
    };
    let selection = selection_for(project);
    // The bound #62 registry: explicit path, or the embedded seed.
    let registry = match read_context_document(errors_path)? {
        Some(json) => serde_json::to_string(&json)
            .map_err(|_| lekalo_core::transport_http::io_failure("errors-registry-invalid"))
            .and_then(|canonical| {
                lekalo_core::error_contract::ErrorRegistry::from_bytes(canonical.as_bytes())
            })
            .map_err(DomainResult::invalid)?,
        None => lekalo_core::error_contract::ErrorRegistry::embedded()
            .cloned()
            .map_err(DomainResult::invalid)?,
    };
    let query_model = match read_context_document(query_model_path)? {
        Some(json) => Some(
            lekalo_core::query_model::QueryModelAttachment::from_value(&json)
                .map_err(DomainResult::invalid)?,
        ),
        None => None,
    };
    // The project: load, compile, and check custody exactly like the
    // query-model resolver (project id, Model version, Model digest).
    let model = lekalo_core::loader::normalize_model(&selection)?;
    let compilation = match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => return Err(failure.into_result()),
    };
    let model_json = match lekalo_core::loader::run(&selection, false) {
        DomainResult::Valid {
            payload: lekalo_core::result::SuccessPayload::Model { json, .. },
            ..
        } => json,
        other => return Err(other),
    };
    let computed = lekalo_core::digest::sha256_hex(model_json.as_bytes());
    if attachment.model_ref().digest().as_str() != format!("sha256:{computed}") {
        return Err(DomainResult::invalid(
            lekalo_core::transport_http::rule_set(
                "transport.contract-invalid",
                "model-digest",
                None,
            ),
        ));
    }
    Ok(TransportSession {
        attachment,
        compilation,
        registry,
        query_model,
        capabilities: lekalo_core::transport_http::CapabilityMap::http_json(),
    })
}

impl TransportSession {
    /// The validation context of this session.
    fn context(&self) -> lekalo_core::transport_http::ValidationContext<'_> {
        let context =
            lekalo_core::transport_http::ValidationContext::new(&self.compilation.project)
                .with_errors(&self.registry)
                .with_capabilities(&self.capabilities);
        match &self.query_model {
            Some(model) => context.with_query_model(model),
            None => context,
        }
    }
}

/// `lekalo transport validate`: wire normalization, custody, and the
/// semantic pass against the selected project and every bound context.
fn transport_validate(
    path: &str,
    project: &Option<String>,
    errors_path: Option<&str>,
    query_model_path: Option<&str>,
    strict: bool,
) -> DomainResult {
    let session = match transport_session(path, project, errors_path, query_model_path) {
        Ok(session) => session,
        Err(result) => return result,
    };
    let attachment = &session.attachment;
    let context = session.context();
    let context = if strict { context.strict() } else { context };
    if let Err(diagnostics) = lekalo_core::transport_http::validate(attachment, &context) {
        return DomainResult::invalid(diagnostics);
    }
    let digest = attachment
        .digest()
        .map(|digest| digest.as_str().to_owned())
        .unwrap_or_default();
    let json = format!(
        "{{\"status\":\"valid\",\"transport\":{{\"projectId\":\"{}\",\"endpoints\":{},\"schemes\":{},\"canonicalDigest\":\"{}\"}}}}",
        attachment.project_id().as_str(),
        attachment.endpoints().len(),
        attachment.schemes().len(),
        digest,
    );
    let human = format!(
        "transport {}: {} endpoints, {} schemes",
        attachment.project_id().as_str(),
        attachment.endpoints().len(),
        attachment.schemes().len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo transport project`: the canonical route surface of one
/// closed namespace, byte-stable and deterministic.
fn transport_project(
    path: &str,
    namespace: &str,
    project: &Option<String>,
    errors_path: Option<&str>,
    query_model_path: Option<&str>,
) -> DomainResult {
    let session = match transport_session(path, project, errors_path, query_model_path) {
        Ok(session) => session,
        Err(result) => return result,
    };
    let attachment = &session.attachment;
    let context = session.context();
    // The projection requires a validated attachment: an unresolved
    // endpoint refuses rather than guessing a route.
    if let Err(diagnostics) = lekalo_core::transport_http::validate(attachment, &context) {
        return DomainResult::invalid(diagnostics);
    }
    let surface = match lekalo_core::transport_http::project(attachment, &context, namespace) {
        Ok(surface) => surface,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let json = format!(
        "{{\"status\":\"valid\",\"surface\":{}}}",
        surface.canonical_bytes(),
    );
    let human = format!(
        "route surface {}: {} routes",
        namespace,
        attachment.endpoints().len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo transport diff`: the pure semantic comparison of two
/// same-family attachments; the verdict stays data and the strict
/// `wire-consumer` blocking signal rides along.
fn transport_diff(base_path: &str, candidate_path: &str) -> DomainResult {
    let base_document = match read_transport_document(base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match lekalo_core::transport_http::TransportDocument::from_value(&base_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate_document = match read_transport_document(candidate_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate =
        match lekalo_core::transport_http::TransportDocument::from_value(&candidate_document) {
            Ok(attachment) => attachment,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let diff = match lekalo_core::transport_http::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let count = |class| -> usize {
        diff.paths()
            .iter()
            .filter(|path| path.class() == class)
            .count()
    };
    let breaking = count(lekalo_core::transport_http::DiffClass::Breaking);
    let non_breaking = count(lekalo_core::transport_http::DiffClass::NonBreaking);
    let policy_change = count(lekalo_core::transport_http::DiffClass::PolicyChange);
    let paths: Vec<String> = diff
        .paths()
        .iter()
        .map(|path| {
            format!(
                "{{\"path\":\"{}\",\"class\":\"{}\"}}",
                path.path(),
                path.class().key()
            )
        })
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"transportDiff\":{{\"equal\":{},\"breaking\":{},\"nonBreaking\":{},\"policyChange\":{},\"wireConsumerBlocked\":{},\"paths\":[{}]}}}}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
        diff.wire_consumer_blocked(),
        paths.join(","),
    );
    let human = format!(
        "transport diff: {} breaking, {} non-breaking, {} policy-change (wire-consumer blocked: {})",
        breaking,
        non_breaking,
        policy_change,
        diff.wire_consumer_blocked(),
    );
    DomainResult::diff(json, human, Vec::new())
}

/// `lekalo transport inspect`: one endpoint's joined surface.
fn transport_inspect(path: &str, endpoint: &str, project: &Option<String>) -> DomainResult {
    let document = match read_transport_document(path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let attachment = match lekalo_core::transport_http::TransportDocument::from_value(&document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let binding = match attachment.endpoint(endpoint) {
        Some(binding) => binding,
        None => {
            return DomainResult::invalid(lekalo_core::transport_http::rule_set(
                "transport.endpoint-unresolved",
                "endpoint-missing",
                Some(endpoint),
            ));
        }
    };
    // The joined Model surface when a project is selected.
    let joined = project.as_ref().and_then(|_| {
        let selection = selection_for(project);
        let model = lekalo_core::loader::normalize_model(&selection).ok()?;
        let compilation = lekalo_core::ir::compile(&model).ok()?;
        compilation
            .project
            .definitions
            .iter()
            .find_map(|definition| match definition {
                lekalo_core::ir::Definition::Endpoint(def) if def.id.as_str() == endpoint => {
                    Some((
                        def.method.as_str().to_owned(),
                        def.path.as_str().to_owned(),
                        def.invokes.as_str().to_owned(),
                    ))
                }
                _ => None,
            })
    });
    // The full declared binding, serialized by the core in its exact
    // wire spelling: every declared transport member, params as an
    // array keyed by the (name, location) wire identity.
    let mut binding_object = match lekalo_core::transport_http::binding_json(binding) {
        serde_json::Value::Object(map) => map,
        _ => unreachable!("binding_json renders an object"),
    };
    binding_object.insert(
        "operationId".to_owned(),
        serde_json::Value::String(binding.effective_operation_id().as_str().to_owned()),
    );
    if let Some((method, path, invokes)) = joined {
        binding_object.insert("method".to_owned(), serde_json::Value::String(method));
        binding_object.insert("path".to_owned(), serde_json::Value::String(path));
        binding_object.insert("invokes".to_owned(), serde_json::Value::String(invokes));
    }
    let json = format!(
        "{{\"status\":\"valid\",\"endpoint\":{}}}",
        serde_json::Value::Object(binding_object),
    );
    let human = format!(
        "endpoint {}: operationId {}",
        endpoint,
        binding.effective_operation_id().as_str(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// Run one `lekalo openapi` operation. The core owns every decision;
/// this binary reads the documents, selects, renders, and maps exits.
fn run_openapi(command: OpenapiCommands) -> DomainResult {
    match command {
        OpenapiCommands::Render {
            path,
            project,
            errors,
            query_model,
            version,
            mode,
        } => {
            if matches!(mode, OpenapiMode::Fragments) {
                // Fragments emission is ownership-aware: it merges into
                // the maintained document on the adapter apply path,
                // which owns the filesystem views. The stateless render
                // has nothing to merge into — it refuses instead of
                // printing a pretend-full document (r1 F-7/cline F-2).
                return DomainResult::usage_error();
            }
            openapi_render(
                &path,
                &project,
                errors.as_deref(),
                query_model.as_deref(),
                version.as_str(),
                mode.as_str(),
            )
        }
        OpenapiCommands::Check {
            path,
            project,
            transport,
            errors,
            query_model,
            ownership,
            version,
            mode,
        } => openapi_check(
            &path,
            &transport,
            &project,
            errors.as_deref(),
            query_model.as_deref(),
            ownership.as_deref(),
            version.as_str(),
            mode.as_str(),
        ),
        OpenapiCommands::Inspect {
            path,
            endpoint,
            project,
            errors,
            query_model,
        } => openapi_inspect(
            &path,
            &endpoint,
            &project,
            errors.as_deref(),
            query_model.as_deref(),
        ),
        OpenapiCommands::Diff {
            base,
            candidate,
            project,
            version,
        } => openapi_diff(&base, &candidate, &project, version.as_str()),
    }
}

/// The declared render configuration of one CLI invocation; an
/// unknown token is a clap-level usage failure.
fn openapi_config(version: &str, mode: &str) -> Result<lekalo_core::openapi::RenderConfig, ()> {
    let version = lekalo_core::openapi::DocumentVersion::parse(version).ok_or(())?;
    let mode = lekalo_core::openapi::DocumentMode::parse(mode).ok_or(())?;
    Ok(lekalo_core::openapi::RenderConfig::new()
        .with_version(version)
        .with_mode(mode))
}

/// Read one ownership sidecar manifest, or the default when absent.
fn read_ownership(
    path: Option<&str>,
) -> Result<lekalo_core::openapi::OwnershipManifest, DomainResult> {
    let resolved = match path {
        Some(path) => std::path::PathBuf::from(path),
        None => return Ok(lekalo_core::openapi::OwnershipManifest::default()),
    };
    let bytes = match std::fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(lekalo_core::openapi::OwnershipManifest::default());
        }
        Err(_) => {
            return Err(DomainResult::invalid(lekalo_core::openapi::io_failure(
                "ownership-unreadable",
            )))
        }
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| DomainResult::invalid(lekalo_core::openapi::io_failure("ownership-json")))?;
    lekalo_core::openapi::OwnershipManifest::from_value(&value).map_err(DomainResult::invalid)
}

/// `lekalo openapi render`: the canonical document of one validated
/// attachment, byte-stable, with the projection findings as warnings.
fn openapi_render(
    path: &str,
    project: &Option<String>,
    errors_path: Option<&str>,
    query_model_path: Option<&str>,
    version: &str,
    mode: &str,
) -> DomainResult {
    let config = match openapi_config(version, mode) {
        Ok(config) => config,
        Err(()) => return DomainResult::usage_error(),
    };
    let session = match transport_session(path, project, errors_path, query_model_path) {
        Ok(session) => session,
        Err(result) => return result,
    };
    let context = session.context();
    if let Err(diagnostics) = lekalo_core::transport_http::validate(&session.attachment, &context) {
        return DomainResult::invalid(diagnostics);
    }
    let document = match lekalo_core::openapi::render(&session.attachment, &context, &config) {
        Ok(document) => document,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let findings = lekalo_core::openapi::projection_partial(document.findings());
    let json = format!(
        "{{\"status\":\"valid\",\"openapi\":{{\"projectId\":\"{}\",\"openapiVersion\":\"{}\",\"mode\":\"{}\",\"canonicalDigest\":\"{}\",\"endpoints\":{},\"document\":{}}}}}",
        session.attachment.project_id().as_str(),
        config.version.wire_str(),
        config.mode.as_str(),
        document.digest(),
        session.attachment.endpoints().len(),
        document.root(),
    );
    let human = format!(
        "openapi {}: {} operations, digest {}",
        config.version.wire_str(),
        document.operation_pointers().len(),
        document.digest(),
    );
    DomainResult::graph(json, human, findings.as_slice().to_vec())
}

/// `lekalo openapi check`: bind the maintained document, recompute the
/// fragments, and report the per-pointer verdict; drift, conflicts,
/// and unresolved anchors are the invalid set, the unbound-manual
/// inventory rides informationally.
#[allow(clippy::too_many_arguments)]
fn openapi_check(
    document_path: &str,
    transport_path: &str,
    project: &Option<String>,
    errors_path: Option<&str>,
    query_model_path: Option<&str>,
    ownership_path: Option<&str>,
    version: &str,
    mode: &str,
) -> DomainResult {
    let bytes = match std::fs::read(document_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return DomainResult::invalid(lekalo_core::openapi::io_failure(detail));
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            return DomainResult::invalid(lekalo_core::openapi::io_failure("document-encoding"))
        }
    };
    let existing = match lekalo_core::openapi::parse_document_text(&text) {
        Ok(document) => document,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let ownership = match read_ownership(ownership_path) {
        Ok(ownership) => ownership,
        Err(result) => return result,
    };
    let session = match transport_session(transport_path, project, errors_path, query_model_path) {
        Ok(session) => session,
        Err(result) => return result,
    };
    let context = session.context();
    if let Err(diagnostics) = lekalo_core::transport_http::validate(&session.attachment, &context) {
        return DomainResult::invalid(diagnostics);
    }
    let config = match openapi_config(version, mode) {
        Ok(config) => config,
        Err(()) => return DomainResult::usage_error(),
    };
    let report = match lekalo_core::openapi::check(
        &existing,
        &session.attachment,
        &context,
        &config,
        &ownership,
    ) {
        Ok(report) => report,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    if !report.is_conformant() {
        return DomainResult::invalid(report.diagnostics());
    }
    let json = format!(
        "{{\"status\":\"valid\",\"openapiCheck\":{{\"conformant\":true,\"boundClean\":{},\"drifts\":[],\"conflicts\":[],\"unresolved\":[],\"manual\":{}}}}}",
        report.bound_clean,
        serde_json::to_string(&report.manual).unwrap_or_else(|_| "[]".to_owned()),
    );
    let human = format!(
        "openapi check: conformant, {} bound, {} manual",
        report.bound_clean,
        report.manual.len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo openapi inspect`: one endpoint's rendered operation.
fn openapi_inspect(
    path: &str,
    endpoint: &str,
    project: &Option<String>,
    errors_path: Option<&str>,
    query_model_path: Option<&str>,
) -> DomainResult {
    let session = match transport_session(path, project, errors_path, query_model_path) {
        Ok(session) => session,
        Err(result) => return result,
    };
    let context = session.context();
    if let Err(diagnostics) = lekalo_core::transport_http::validate(&session.attachment, &context) {
        return DomainResult::invalid(diagnostics);
    }
    let document = match lekalo_core::openapi::render(
        &session.attachment,
        &context,
        &lekalo_core::openapi::RenderConfig::new(),
    ) {
        Ok(document) => document,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let pointer = match document
        .operation_pointers()
        .iter()
        .find(|(_, candidate)| candidate == endpoint)
        .map(|(pointer, _)| pointer.clone())
    {
        Some(pointer) => pointer,
        None => {
            return DomainResult::invalid(lekalo_core::openapi::rule_set(
                "openapi.binding-unresolved",
                "endpoint-missing",
                Some(endpoint),
            ))
        }
    };
    // Resolve the pointer inside the rendered document.
    let mut node = document.root().clone();
    for token in pointer
        .split('/')
        .skip(1)
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
    {
        node = node.get(&token).cloned().unwrap_or(serde_json::Value::Null);
    }
    let json = format!(
        "{{\"status\":\"valid\",\"endpoint\":{{\"endpoint\":\"{}\",\"pointer\":\"{}\",\"operation\":{}}}}}",
        endpoint,
        pointer,
        node,
    );
    let human = format!("openapi endpoint {}: {}", endpoint, pointer);
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo openapi diff`: the pointer-level view of the transport
/// compatibility classes over two same-family attachments; the
/// verdict stays data and the strict wire-consumer blocking signal
/// rides along.
fn openapi_diff(
    base_path: &str,
    candidate_path: &str,
    project: &Option<String>,
    version: &str,
) -> DomainResult {
    let version = match lekalo_core::openapi::DocumentVersion::parse(version) {
        Some(version) => version,
        None => return DomainResult::usage_error(),
    };
    // Both sides share the one compiled project: compare refuses mixed
    // pins, so a single session's context serves the mapping.
    let base_document = match read_transport_document(base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match lekalo_core::transport_http::TransportDocument::from_value(&base_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate_document = match read_transport_document(candidate_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate =
        match lekalo_core::transport_http::TransportDocument::from_value(&candidate_document) {
            Ok(attachment) => attachment,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let selection = selection_for(project);
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Ok(model) => model,
        Err(result) => return result,
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => return failure.into_result(),
    };

    let result = match lekalo_core::openapi::compare_documents(
        &base,
        &candidate,
        &compilation.project,
        version,
    ) {
        Ok(result) => result,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let json = format!(
        "{{\"status\":\"valid\",\"openapiDiff\":{}}}",
        lekalo_core::openapi::diff_json(&result),
    );
    let human = format!(
        "openapi diff: {} changed paths (wire-consumer blocked: {})",
        result.paths().len(),
        result.wire_consumer_blocked(),
    );
    DomainResult::diff(json, human, Vec::new())
}

/// Run one `lekalo query-model` operation. The core owns every
/// decision; this binary only reads the document, selects, renders,
/// and maps exits.
fn run_query_model(command: QueryModelCommands) -> DomainResult {
    match command {
        QueryModelCommands::Validate {
            path,
            project,
            strict,
        } => query_model_validate(&path, &project, strict),
        QueryModelCommands::Diff { base, candidate } => query_model_diff(&base, &candidate),
    }
}

fn run_storage(command: StorageCommands) -> DomainResult {
    match command {
        StorageCommands::Profile { engine, version } => storage_profile(engine, version),
        StorageCommands::ValidateEngine { path } => storage_engine_validate(&path),
        StorageCommands::Ddl {
            profile,
            projection,
        } => storage_ddl(&profile, &projection),
        StorageCommands::MigratePlan {
            base,
            candidate,
            profile,
            confirm,
        } => storage_migrate_plan(&base, &candidate, &profile, confirm.as_deref()),
        StorageCommands::Conformance {
            profile,
            projection,
            scan,
            drifted,
            input,
            runtimes,
        } => storage_conformance(
            &profile,
            &projection,
            scan.as_deref(),
            drifted.as_deref(),
            input.as_deref(),
            &runtimes,
        ),
        StorageCommands::Input {
            profile,
            projection,
        } => storage_input(&profile, &projection),
        StorageCommands::Capabilities {
            path,
            projection,
            profile,
            requirements,
        } => storage_capabilities(&path, &projection, profile, requirements.as_deref()),
        StorageCommands::Drift {
            scan,
            projection,
            profile,
        } => storage_drift(&scan, &projection, &profile),
        StorageCommands::Validate { path, project: _ } => storage_validate(&path),
        StorageCommands::Project { path, namespace } => storage_project(&path, namespace),
        StorageCommands::Diff { base, candidate } => storage_diff(&base, &candidate),
        StorageCommands::Plan {
            base,
            candidate,
            confirm,
        } => storage_plan(&base, &candidate, confirm.as_deref()),
        StorageCommands::IntrospectCheck {
            projection,
            evidence,
            namespace,
        } => storage_introspect_check(&projection, &evidence, namespace),
    }
}

/// `lekalo storage drift`: the declared-versus-observed comparison.
/// The verdict stays data; exits stay envelope-owned.
fn storage_drift(scan_path: &str, projection_path: &str, profile_path: &str) -> DomainResult {
    let profile = match read_storage_profile(profile_path) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let projection_document = match read_attachment_document(projection_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let projection = match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(
        &projection_document,
    ) {
        Ok(projection) => projection,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let scan_document = match read_attachment_document(scan_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let evidence =
        match lekalo_core::storage_engine::IntrospectionEvidence::from_value(&scan_document) {
            Ok(evidence) => evidence,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let report = match lekalo_core::storage_engine::compare_drift(&profile, &projection, &evidence)
    {
        Ok(report) => report,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let bytes = match report.canonical_bytes() {
        Ok(bytes) => bytes,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let human = if report.ok() {
        "storage drift: the observed schema matches the declaration".to_owned()
    } else {
        format!(
            "storage drift: {} finding(s); the verdict stays data",
            report.findings().len()
        )
    };
    DomainResult::graph(bytes, human, Vec::new())
}

/// `lekalo storage profile`: the owner-published matrix projection.
fn storage_profile(engine: StorageEngineArg, version: Option<String>) -> DomainResult {
    match engine {
        StorageEngineArg::Postgres => {
            use lekalo_core::storage_engine::postgres::version_matrix;
            let capabilities = |major: u32| {
                version_matrix::CAPABILITY_IDS
                    .iter()
                    .filter_map(|id| {
                        version_matrix::answer(major, id).map(|support| {
                            format!("{{\"id\":\"{id}\",\"support\":\"{}\"}}", support.key())
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(",")
            };
            match version {
                Some(pin) => {
                    let parsed = match lekalo_core::storage_engine::VersionPin::parse(&pin) {
                        Ok(parsed) => parsed,
                        Err(_) => {
                            return DomainResult::invalid(lekalo_core::storage_engine::io_failure(
                                "engine-version",
                            ))
                        }
                    };
                    if version_matrix::row_for(parsed.major()).is_none() {
                        return DomainResult::unsupported_version(
                            lekalo_core::storage_engine::unsupported_failure("major-unpublished"),
                        );
                    }
                    let json = format!(
                        "{{\"status\":\"valid\",\"engine\":\"postgres\",\"engineVersion\":\"{pin}\",\"capabilities\":[{}]}}",
                        capabilities(parsed.major())
                    );
                    DomainResult::graph(
                        json,
                        format!("postgres {pin}: published capability matrix row"),
                        Vec::new(),
                    )
                }
                None => {
                    let rows: Vec<String> = version_matrix::ROWS
                        .iter()
                        .map(|row| {
                            format!(
                                "{{\"major\":{},\"capabilities\":[{}]}}",
                                row.major,
                                capabilities(row.major)
                            )
                        })
                        .collect();
                    let json = format!(
                        "{{\"status\":\"valid\",\"engine\":\"postgres\",\"rows\":[{}]}}",
                        rows.join(",")
                    );
                    DomainResult::graph(
                        json,
                        "postgres: every published capability matrix row".to_owned(),
                        Vec::new(),
                    )
                }
            }
        }
    }
}

/// `lekalo storage validate-engine`: normalize one storage-engine
/// attachment and print its canonical bytes.
fn storage_engine_validate(path: &str) -> DomainResult {
    let document = match read_attachment_document(path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let attachment =
        match lekalo_core::storage_engine::StorageEngineAttachment::from_value(&document) {
            Ok(attachment) => attachment,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let bytes = match attachment.canonical_bytes() {
        Ok(bytes) => bytes,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    DomainResult::graph(
        bytes,
        format!(
            "storage engine {}: {} profile",
            attachment.engine().key(),
            attachment.engine_version().as_str()
        ),
        Vec::new(),
    )
}

/// `lekalo storage ddl`: the deterministic DDL document.
fn storage_ddl(profile_path: &str, projection_path: &str) -> DomainResult {
    let profile = match read_storage_profile(profile_path) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let document = match read_attachment_document(projection_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let projection =
        match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(&document) {
            Ok(projection) => projection,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let rendered = match lekalo_core::storage_engine::postgres::ddl::render(&profile, &projection) {
        Ok(rendered) => rendered,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let bytes = match rendered.canonical_bytes() {
        Ok(bytes) => bytes,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    DomainResult::graph(
        bytes,
        format!(
            "postgres DDL for {}: {} statements",
            profile.engine_version().as_str(),
            rendered.statements().len()
        ),
        Vec::new(),
    )
}

/// `lekalo storage capabilities`: the engine snapshot, optionally
/// mapped against declared requirements.
fn storage_capabilities(
    path: &str,
    projection_path: &str,
    profile: StorageCapabilityProfileArg,
    requirements: Option<&str>,
) -> DomainResult {
    let engine_profile = match read_storage_profile(path) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let document = match read_attachment_document(projection_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let projection =
        match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(&document) {
            Ok(projection) => projection,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let snapshot =
        lekalo_core::storage_engine::postgres::snapshot::build(&engine_profile, &projection);
    let answers: Vec<String> = snapshot
        .answers()
        .iter()
        .map(|(capability, support)| {
            format!(
                "{{\"capability\":\"{capability}\",\"support\":\"{}\"}}",
                support.key()
            )
        })
        .collect();
    let (mapped, verdicts_json) = match requirements {
        Some(path) => {
            let document = match read_attachment_document(path) {
                Ok(document) => document,
                Err(result) => return result,
            };
            let attachment = match lekalo_core::transaction_concurrency::
                TransactionConcurrencyAttachment::from_value(&document)
            {
                Ok(attachment) => attachment,
                Err(diagnostics) => return DomainResult::invalid(diagnostics),
            };
            let profile = match profile {
                StorageCapabilityProfileArg::Strict => {
                    lekalo_core::transaction_concurrency::CapabilityProfile::Strict
                }
                StorageCapabilityProfileArg::Permissive => {
                    lekalo_core::transaction_concurrency::CapabilityProfile::Permissive
                }
            };
            let decision = lekalo_core::transaction_concurrency::map_capabilities(
                attachment.capability_requirements(),
                &snapshot,
                profile,
            );
            let verdicts: Vec<String> = decision
                .verdicts()
                .iter()
                .map(|verdict| {
                    format!(
                        "{{\"requirement\":\"{}\",\"capability\":\"{}\",\"support\":\"{}\",\"blocked\":{}}}",
                        verdict.requirement_id(),
                        verdict.capability(),
                        verdict.support().key(),
                        verdict.blocked()
                    )
                })
                .collect();
            (
                format!(
                    ",\"blocked\":{},\"profile\":\"{}\"",
                    decision.blocked(),
                    profile.key()
                ),
                format!(",\"verdicts\":[{}]", verdicts.join(",")),
            )
        }
        None => (String::new(), String::new()),
    };
    let human = format!(
        "engine capabilities: {} answers{}",
        answers.len(),
        if requirements.is_some() {
            " mapped against the declared requirements"
        } else {
            ""
        }
    );
    let json = format!(
        "{{\"status\":\"valid\",\"engine\":\"{}\",\"engineVersion\":\"{}\",\"capabilities\":[{}]{verdicts_json}{mapped}}}",
        engine_profile.engine().key(),
        engine_profile.engine_version().as_str(),
        answers.join(",")
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage migrate-plan`: the gated plan document. A blocked
/// plan is a typed denial whose diagnostic carries the exact `planId`
/// digest the caller must name through `--confirm`; a confirmed or
/// ready plan prints its bytes.
fn storage_migrate_plan(
    base_path: &str,
    candidate_path: &str,
    profile_path: &str,
    confirm: Option<&str>,
) -> DomainResult {
    let profile = match read_storage_profile(profile_path) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let base_document = match read_attachment_document(base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(
        &base_document,
    ) {
        Ok(base) => base,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate_document = match read_attachment_document(candidate_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate = match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(
        &candidate_document,
    ) {
        Ok(candidate) => candidate,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let plan =
        match lekalo_core::storage_engine::plan_migration(&profile, &base, &candidate, confirm) {
            Ok(plan) => plan,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    if plan.status() == lekalo_core::storage_engine::PlanStatus::Blocked {
        return DomainResult::denied(lekalo_core::storage_engine::gated_plan_failure(
            "destructive-steps",
            plan.plan_id(),
        ));
    }
    let bytes = match plan.canonical_bytes() {
        Ok(bytes) => bytes,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let human = format!(
        "migration plan {}: {} step(s), status {}",
        plan.plan_id().chars().skip(7).take(12).collect::<String>(),
        plan.steps().len(),
        plan.status().key()
    );
    DomainResult::graph(bytes, human, Vec::new())
}

/// `lekalo storage input`: the one runtime-neutral document.
fn storage_input(profile_path: &str, projection_path: &str) -> DomainResult {
    let profile = match read_storage_profile(profile_path) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let document = match read_attachment_document(projection_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let projection =
        match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(&document) {
            Ok(projection) => projection,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let input = match lekalo_core::storage_engine::input_document(&profile, &projection) {
        Ok(input) => input,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    DomainResult::graph(
        input,
        format!(
            "engine input for {}: one canonical document",
            profile.engine_version().as_str()
        ),
        Vec::new(),
    )
}

/// `lekalo storage conformance`: the closed battery, fixed order.
fn storage_conformance(
    profile_path: &str,
    projection_path: &str,
    scan: Option<&str>,
    drifted: Option<&str>,
    input: Option<&str>,
    runtimes: &[String],
) -> DomainResult {
    let profile = match read_storage_profile(profile_path) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let projection_document = match read_attachment_document(projection_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let projection = match lekalo_core::storage_projection::StorageProjectionAttachment::from_value(
        &projection_document,
    ) {
        Ok(projection) => projection,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    fn read_evidence(
        path: &str,
    ) -> Result<lekalo_core::storage_engine::IntrospectionEvidence, DomainResult> {
        let document = read_attachment_document(path)?;
        lekalo_core::storage_engine::IntrospectionEvidence::from_value(&document)
            .map_err(DomainResult::invalid)
    }
    let evidence = match scan {
        Some(path) => match read_evidence(path) {
            Ok(evidence) => Some(evidence),
            Err(result) => return result,
        },
        None => None,
    };
    let drifted = match drifted {
        Some(path) => match read_evidence(path) {
            Ok(evidence) => Some(evidence),
            Err(result) => return result,
        },
        None => None,
    };
    let read_text = |path: &str| {
        std::fs::read_to_string(path).map_err(|_| {
            DomainResult::invalid(lekalo_core::storage_engine::io_failure("file-unreadable"))
        })
    };
    let input_text = match input {
        Some(path) => match read_text(path) {
            Ok(text) => Some(text),
            Err(result) => return result,
        },
        None => None,
    };
    let mut goldens: Vec<String> = Vec::with_capacity(runtimes.len());
    for path in runtimes {
        match read_text(path) {
            Ok(text) => goldens.push(text),
            Err(result) => return result,
        }
    }
    let golden_refs: Vec<&str> = goldens.iter().map(|text| text.as_str()).collect();
    let inputs = lekalo_core::storage_engine::conformance::BatteryInputs {
        profile: &profile,
        projection: &projection,
        evidence: evidence.as_ref(),
        drifted: drifted.as_ref(),
        input: input_text.as_deref(),
        runtime_goldens: &golden_refs,
    };
    let battery = lekalo_core::storage_engine::conformance::run(&inputs);
    let bytes = match battery.canonical_bytes() {
        Ok(bytes) => bytes,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    if !battery.ok() {
        return DomainResult::denied(lekalo_core::storage_engine::conformance_failure(
            "battery-failed",
        ));
    }
    let human = format!(
        "storage conformance: {} checks, battery green",
        battery.checks().len()
    );
    DomainResult::graph(bytes, human, Vec::new())
}
/// Read one storage-engine attachment from disk.
fn read_storage_profile(
    path: &str,
) -> Result<lekalo_core::storage_engine::StorageEngineAttachment, DomainResult> {
    let document = read_attachment_document(path)?;
    lekalo_core::storage_engine::StorageEngineAttachment::from_value(&document)
        .map_err(DomainResult::invalid)
}

/// Read one attachment document from disk with a classified read-only
/// failure.
fn read_attachment_document(path: &str) -> Result<serde_json::Value, DomainResult> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return Err(DomainResult::invalid(lekalo_core::query_model::io_failure(
                detail,
            )));
        }
    };
    serde_json::from_slice(&bytes)
        .map_err(|_| DomainResult::invalid(lekalo_core::query_model::io_failure("invalid-json")))
}

/// `lekalo query-model validate`: resolve the attachment against the
/// selected project (custody, references, strict tenant gate) and emit
/// the canonical plan summary.
fn query_model_validate(path: &str, project: &Option<String>, strict: bool) -> DomainResult {
    let document = match read_attachment_document(path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let attachment = match lekalo_core::query_model::QueryModelAttachment::from_value(&document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let selection = selection_for(project);
    let profile = lekalo_core::query_model::Profile::from_strict(strict);
    let resolution = match lekalo_core::query_model::resolve(&attachment, &selection, profile) {
        Ok(resolution) => resolution,
        Err(result) => return result,
    };
    let foreign = attachment
        .queries()
        .iter()
        .filter(|decl| decl.foreign.is_some())
        .count();
    let json = format!(
        "{{\"status\":\"valid\",\"queryModel\":{{\"projectId\":\"{}\",\"queryCount\":{},\"foreignQueries\":{},\"tenancyScopes\":{}}},\"plans\":{}}}",
        attachment.project_id().as_str(),
        attachment.queries().len(),
        foreign,
        attachment.tenancy().len(),
        plans_json(&resolution),
    );
    let human = format!(
        "query model {}: {} queries ({} foreign), {} tenancy scopes, {} plans",
        attachment.project_id().as_str(),
        attachment.queries().len(),
        foreign,
        attachment.tenancy().len(),
        resolution.plans().len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// The canonical plan array of one resolution, as JSON text.
fn plans_json(resolution: &lekalo_core::query_model::Resolution) -> String {
    let plans: Vec<String> = resolution
        .plans()
        .iter()
        .map(|plan| plan.canonical_bytes())
        .collect();
    format!("[{}]", plans.join(","))
}

/// `lekalo query-model diff`: the pure semantic comparison of two
/// same-family attachments; the verdict stays data.
fn query_model_diff(base_path: &str, candidate_path: &str) -> DomainResult {
    let base_document = match read_attachment_document(base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate_document = match read_attachment_document(candidate_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match lekalo_core::query_model::QueryModelAttachment::from_value(&base_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate =
        match lekalo_core::query_model::QueryModelAttachment::from_value(&candidate_document) {
            Ok(attachment) => attachment,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let diff = match lekalo_core::query_model::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let count = |class| -> usize {
        diff.paths()
            .iter()
            .filter(|path| path.class() == class)
            .count()
    };
    let breaking = count(lekalo_core::query_model::DiffClass::Breaking);
    let non_breaking = count(lekalo_core::query_model::DiffClass::NonBreaking);
    let policy_change = count(lekalo_core::query_model::DiffClass::PolicyChange);
    let paths: Vec<String> = diff
        .paths()
        .iter()
        .map(|path| {
            format!(
                "{{\"path\":\"{}\",\"class\":\"{}\"}}",
                path.path(),
                path.class().key()
            )
        })
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"queryModelDiff\":{{\"equal\":{},\"breaking\":{},\"nonBreaking\":{},\"policyChange\":{},\"paths\":[{}]}}}}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
        paths.join(","),
    );
    let human = format!(
        "query model diff: equal {}; breaking {}; non-breaking {}; policy-change {}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
    );
    DomainResult::graph(json, human, Vec::new())
}

/// Read one attachment document from disk with a classified read-only
/// failure. `family` selects the registered rule: `storage` maps onto
/// `storage.input-invalid`, `profile` onto `storage.profile-invalid`,
/// and `evidence` onto `storage.introspection-invalid`.
fn read_storage_document(
    family: StorageDocFamily,
    path: &str,
) -> Result<serde_json::Value, DomainResult> {
    let io = |detail: &str| match family {
        StorageDocFamily::Projection => {
            DomainResult::invalid(lekalo_core::storage_projection::io_failure(detail))
        }
        StorageDocFamily::Profile | StorageDocFamily::Evidence => DomainResult::usage_error(),
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                io::ErrorKind::NotFound => "file-missing",
                _ => "file-unreadable",
            };
            return Err(io(detail));
        }
    };
    serde_json::from_slice(&bytes).map_err(|_| io("invalid-json"))
}

/// The closed document-family selector of the storage CLI readers.
#[derive(Clone, Copy, Debug)]
enum StorageDocFamily {
    /// The storage-projection attachment.
    Projection,
    /// The storage-engine-profile attachment.
    Profile,
    /// The storage-introspection evidence.
    Evidence,
}

/// `lekalo storage validate`: validate the attachment and emit the
/// deterministic summary of every declared projection.
fn storage_validate(path: &str) -> DomainResult {
    let document = match read_storage_document(StorageDocFamily::Projection, path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let attachment = match StorageAttachment::from_value(&document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let namespaces: Vec<String> = attachment
        .projections()
        .iter()
        .map(|projection| projection.namespace().key().to_owned())
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"storage\":{{\"projectId\":\"{}\",\"attachmentRevision\":\"{}\",\"entities\":{},\"relations\":{},\"namespaces\":[{}]}}}}",
        attachment.project_id().as_str(),
        attachment.attachment_revision().as_str(),
        attachment.entities().len(),
        attachment.relations().len(),
        namespaces
            .iter()
            .map(|namespace| format!("\"{namespace}\""))
            .collect::<Vec<_>>()
            .join(","),
    );
    let human = format!(
        "storage attachment {}: {} entities, {} relations, namespaces {}",
        attachment.project_id().as_str(),
        attachment.entities().len(),
        attachment.relations().len(),
        namespaces.join(", "),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage project`: derive one namespace's projection and
/// emit its canonical bytes.
fn storage_project(path: &str, namespace: StorageNamespace) -> DomainResult {
    let document = match read_storage_document(StorageDocFamily::Projection, path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let attachment = match StorageAttachment::from_value(&document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let derived = match lekalo_core::storage_projection::project(&attachment, namespace.core()) {
        Ok(derived) => derived,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let bytes = match lekalo_core::storage_projection::canonical::derived_bytes(&derived) {
        Ok(bytes) => bytes,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let human = format!(
        "derived {} projection: {} tables, {} joins",
        namespace.key(),
        derived.tables().len(),
        derived.joins().len(),
    );
    DomainResult::graph(bytes, human, Vec::new())
}

/// `lekalo storage diff`: the pure semantic comparison of two
/// same-family attachments; the verdict stays data.
fn storage_diff(base_path: &str, candidate_path: &str) -> DomainResult {
    let base_document = match read_storage_document(StorageDocFamily::Projection, base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate_document =
        match read_storage_document(StorageDocFamily::Projection, candidate_path) {
            Ok(document) => document,
            Err(result) => return result,
        };
    let base = match StorageAttachment::from_value(&base_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate = match StorageAttachment::from_value(&candidate_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let diff = match lekalo_core::storage_projection::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let count = |class| -> usize {
        diff.paths()
            .iter()
            .filter(|path| path.class() == class)
            .count()
    };
    let breaking = count(lekalo_core::storage_projection::DiffClass::Breaking);
    let non_breaking = count(lekalo_core::storage_projection::DiffClass::NonBreaking);
    let policy_change = count(lekalo_core::storage_projection::DiffClass::PolicyChange);
    let paths: Vec<String> = diff
        .paths()
        .iter()
        .map(|path| {
            let risk = path
                .risk()
                .map(|risk| format!(",\"risk\":\"{}\"", risk.key()))
                .unwrap_or_default();
            format!(
                "{{\"path\":\"{}\",\"layer\":\"{}\",\"class\":\"{}\"{}}}",
                path.path(),
                path.layer().key(),
                path.class().key(),
                risk
            )
        })
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"storageDiff\":{{\"equal\":{},\"breaking\":{},\"nonBreaking\":{},\"policyChange\":{},\"paths\":[{}]}}}}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
        paths.join(","),
    );
    let human = format!(
        "storage diff: equal {}; breaking {}; non-breaking {}; policy-change {}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage plan`: the non-executable migration plan over one
/// comparison, with the plan-id acknowledgment plumbing.
fn storage_plan(base_path: &str, candidate_path: &str, confirm: Option<&str>) -> DomainResult {
    let base_document = match read_storage_document(StorageDocFamily::Projection, base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate_document =
        match read_storage_document(StorageDocFamily::Projection, candidate_path) {
            Ok(document) => document,
            Err(result) => return result,
        };
    let base = match StorageAttachment::from_value(&base_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate = match StorageAttachment::from_value(&candidate_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let diff = match lekalo_core::storage_projection::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let plan = lekalo_core::storage_projection::migration_plan(&diff);
    // The plan-id acknowledgment: echoing the exact identity records
    // the acknowledgment; a wrong identity refuses as stale instead of
    // acknowledging a plan the caller never saw.
    let acknowledged = match confirm {
        None => false,
        Some(plan_id) if plan_id == plan.plan_id => true,
        Some(_) => {
            return DomainResult::invalid(lekalo_core::storage_projection::io_failure(
                "plan-changed",
            ))
        }
    };
    let json = serde_json::to_string(&plan).unwrap_or_else(|_| "{}".to_owned());
    let json = format!("{{\"status\":\"valid\",\"acknowledged\":{acknowledged},\"plan\":{json}}}");
    let human = format!(
        "storage plan {}: {} steps, {} gated; acknowledged {acknowledged}",
        &plan.plan_id[..19.min(plan.plan_id.len())],
        plan.steps.len(),
        plan.gated,
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage introspect-check`: the closed drift comparison of
/// one declared projection against one evidence document. Read-only;
/// drift is data.
fn storage_introspect_check(
    projection_path: &str,
    evidence_path: &str,
    namespace: StorageNamespace,
) -> DomainResult {
    let projection_document =
        match read_storage_document(StorageDocFamily::Projection, projection_path) {
            Ok(document) => document,
            Err(result) => return result,
        };
    let evidence_document = match read_storage_document(StorageDocFamily::Evidence, evidence_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let attachment = match StorageAttachment::from_value(&projection_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let evidence = match lekalo_core::storage_introspection::StorageIntrospection::from_value(
        &evidence_document,
    ) {
        Ok(evidence) => evidence,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let report = match lekalo_core::storage_introspection::introspect_check(
        &attachment,
        namespace.core(),
        &evidence,
    ) {
        Ok(report) => report,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let drifts: Vec<String> = report
        .drifts
        .iter()
        .map(|drift| {
            format!(
                "{{\"kind\":\"{}\",\"path\":\"{}\"}}",
                drift.kind.key(),
                drift.path
            )
        })
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"introspectCheck\":{{\"equal\":{},\"drifts\":[{}]}}}}",
        report.equal,
        drifts.join(","),
    );
    let human = format!(
        "introspect check: equal {}; drifts {}",
        report.equal,
        report.drifts.len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// The `lekalo storage-profile` subcommands: the thin handoff to the
/// core storage-engine-profile family (issue #117).
fn run_storage_profile(command: StorageProfileCommands) -> DomainResult {
    match command {
        StorageProfileCommands::Validate { path } => storage_profile_validate(&path),
        StorageProfileCommands::Capabilities { path } => storage_profile_capabilities(&path),
        StorageProfileCommands::Portability {
            base,
            target,
            postgres_divergences,
        } => storage_profile_portability(&base, &target, postgres_divergences),
        StorageProfileCommands::Diff { base, candidate } => storage_profile_diff(&base, &candidate),
    }
}

/// Read one profile document from disk with a classified read-only
/// failure.
fn read_profile_document(
    family: StorageDocFamily,
    path: &str,
) -> Result<serde_json::Value, DomainResult> {
    read_storage_document(family, path)
}

/// `lekalo storage-profile validate`.
fn storage_profile_validate(path: &str) -> DomainResult {
    let document = match read_profile_document(StorageDocFamily::Profile, path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let profile = match ProfileAttachment::from_value(&document) {
        Ok(profile) => profile,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let json = format!(
        "{{\"status\":\"valid\",\"storageProfile\":{{\"projectId\":\"{}\",\"engine\":\"{}\",\"engineVersion\":\"{}\",\"capabilities\":{}}}}}",
        profile.project_id().as_str(),
        profile.engine().engine().key(),
        profile.engine().engine_version(),
        profile.capabilities().len(),
    );
    let human = format!(
        "storage profile {} {}: {} declared capabilities",
        profile.engine().engine().key(),
        profile.engine().engine_version(),
        profile.capabilities().len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage-profile capabilities`: the #24 snapshot bridge
/// input as data.
fn storage_profile_capabilities(path: &str) -> DomainResult {
    let document = match read_profile_document(StorageDocFamily::Profile, path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let profile = match ProfileAttachment::from_value(&document) {
        Ok(profile) => profile,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let snapshot = lekalo_core::storage_engine_profile::to_snapshot(&profile);
    // The snapshot has no direct serialization; emit the declared
    // support states byte-sorted instead — the exact bridge input.
    let mut entries: Vec<String> = profile
        .capabilities()
        .iter()
        .map(|(id, capability)| {
            format!(
                "{{\"capability\":\"{}\",\"support\":\"{}\"}}",
                id.key(),
                capability.support().key()
            )
        })
        .collect();
    entries.sort();
    let _ = &snapshot;
    let json = format!(
        "{{\"status\":\"valid\",\"capabilitySnapshot\":{{\"engine\":\"{}\",\"entries\":[{}]}}}}",
        profile.engine().engine().key(),
        entries.join(","),
    );
    let human = format!("capability snapshot: {} entries", entries.len(),);
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage-profile portability`: the engine portability
/// report as data.
fn storage_profile_portability(
    base_path: &str,
    target_path: &str,
    postgres_divergences: bool,
) -> DomainResult {
    let base_document = match read_profile_document(StorageDocFamily::Profile, base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let target_document = match read_profile_document(StorageDocFamily::Profile, target_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match ProfileAttachment::from_value(&base_document) {
        Ok(profile) => profile,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let target = match ProfileAttachment::from_value(&target_document) {
        Ok(profile) => profile,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let report = lekalo_core::storage_engine_profile::portability(&base, &target);
    let report = if postgres_divergences {
        lekalo_core::storage_engine_profile::named_postgres_divergences(report)
    } else {
        report
    };
    let json = serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_owned());
    let human = format!(
        "portability {} -> {}: {} changes, {} losses, {} gains",
        report.source,
        report.target,
        report.changes.len(),
        report.loses.len(),
        report.gains.len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo storage-profile diff`: the pure comparison of two
/// same-family profiles; the verdict stays data.
fn storage_profile_diff(base_path: &str, candidate_path: &str) -> DomainResult {
    let base_document = match read_profile_document(StorageDocFamily::Profile, base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate_document = match read_profile_document(StorageDocFamily::Profile, candidate_path)
    {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match ProfileAttachment::from_value(&base_document) {
        Ok(profile) => profile,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate = match ProfileAttachment::from_value(&candidate_document) {
        Ok(profile) => profile,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let diff = match lekalo_core::storage_engine_profile::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let count = |class| -> usize {
        diff.paths()
            .iter()
            .filter(|path| path.class() == class)
            .count()
    };
    let breaking = count(lekalo_core::storage_engine_profile::DiffClass::Breaking);
    let non_breaking = count(lekalo_core::storage_engine_profile::DiffClass::NonBreaking);
    let policy_change = count(lekalo_core::storage_engine_profile::DiffClass::PolicyChange);
    let paths: Vec<String> = diff
        .paths()
        .iter()
        .map(|path| {
            format!(
                "{{\"path\":\"{}\",\"layer\":\"{}\",\"class\":\"{}\"}}",
                path.path(),
                path.layer().key(),
                path.class().key()
            )
        })
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"storageProfileDiff\":{{\"equal\":{},\"breaking\":{},\"nonBreaking\":{},\"policyChange\":{},\"paths\":[{}]}}}}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
        paths.join(","),
    );
    let human = format!(
        "storage profile diff: equal {}; breaking {}; non-breaking {}; policy-change {}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
    );
    DomainResult::graph(json, human, Vec::new())
}

/// The `lekalo expressions` subcommands: thin selection and
/// rendering over the core family (issue #66).
fn run_expressions(command: ExpressionsCommands) -> DomainResult {
    match command {
        ExpressionsCommands::Validate {
            path,
            builtin_support,
        } => expressions_validate(&path, builtin_support.as_deref()),
        ExpressionsCommands::Eval {
            path,
            vectors,
            builtin_support,
        } => expressions_eval(&path, &vectors, builtin_support.as_deref()),
        ExpressionsCommands::Render {
            path,
            target,
            builtin_support,
        } => expressions_render(&path, target, builtin_support.as_deref()),
        ExpressionsCommands::Diff { base, candidate } => expressions_diff(&base, &candidate),
    }
}

/// Read one JSON document from a path with typed failures.
fn read_expressions_document(path: &str) -> Result<serde_json::Value, DomainResult> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = if error.kind() == std::io::ErrorKind::NotFound {
                "document-missing"
            } else {
                "document-unreadable"
            };
            return Err(DomainResult::invalid(lekalo_core::expressions::io_failure(
                detail,
            )));
        }
    };
    serde_json::from_slice(&bytes)
        .map_err(|_| DomainResult::invalid(lekalo_core::expressions::io_failure("invalid-json")))
}

/// Load and validate the attachment, then gate the declared
/// capability snapshot when one is supplied.
fn expressions_attachment(
    path: &str,
    support_path: Option<&str>,
) -> Result<lekalo_core::expressions::ExpressionsAttachment, DomainResult> {
    let document = read_expressions_document(path)?;
    let attachment = match lekalo_core::expressions::ExpressionsAttachment::from_value(&document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return Err(DomainResult::invalid(diagnostics)),
    };
    if let Some(support_path) = support_path {
        let support_document = read_expressions_document(support_path)?;
        let support = match lekalo_core::expressions::BuiltinSupport::from_value(&support_document)
        {
            Ok(support) => support,
            Err(diagnostics) => return Err(DomainResult::invalid(diagnostics)),
        };
        if let Err(diagnostics) =
            lekalo_core::expressions::check_builtin_support(&attachment, &support)
        {
            return Err(DomainResult::invalid(diagnostics));
        }
    }
    Ok(attachment)
}

/// `lekalo expressions validate`.
fn expressions_validate(path: &str, support: Option<&str>) -> DomainResult {
    let attachment = match expressions_attachment(path, support) {
        Ok(attachment) => attachment,
        Err(result) => return result,
    };
    let conditions = attachment
        .expressions()
        .iter()
        .filter(|record| record.kind().key() == "condition")
        .count();
    let assignments = attachment.expressions().len() - conditions;
    let digest = match attachment.canonical_digest() {
        Ok(digest) => digest,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let capabilities: Vec<String> = attachment
        .required_capabilities()
        .iter()
        .map(|token| format!("\"{token}\""))
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"expressions\":{{\"projectId\":\"{}\",\"expressionCount\":{},\"conditions\":{},\"assignments\":{},\"builtinSemantics\":\"{}\",\"requiredCapabilities\":[{}],\"digest\":\"sha256:{}\"}}}}",
        attachment.project_id().as_str(),
        attachment.expressions().len(),
        conditions,
        assignments,
        attachment.builtin_semantics(),
        capabilities.join(","),
        digest,
    );
    let human = format!(
        "expressions {}: {} records ({} conditions, {} assignments), {} required capabilities",
        attachment.project_id().as_str(),
        attachment.expressions().len(),
        conditions,
        assignments,
        attachment.required_capabilities().len(),
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo expressions eval`: the deterministic reference evaluator
/// over the shared vectors; every expectation must hold.
fn expressions_eval(path: &str, vectors_path: &str, support: Option<&str>) -> DomainResult {
    let attachment = match expressions_attachment(path, support) {
        Ok(attachment) => attachment,
        Err(result) => return result,
    };
    let vectors_document = match read_expressions_document(vectors_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let vectors = match lekalo_core::expressions::VectorsDocument::from_value(&vectors_document) {
        Ok(vectors) => vectors,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    use lekalo_core::expressions::{evaluate, Clock, VectorExpect};
    let mut rows: Vec<String> = Vec::with_capacity(vectors.vectors.len());
    let mut failures = 0usize;
    for vector in &vectors.vectors {
        let Some(record) = attachment.expression(&vector.expression) else {
            return DomainResult::invalid(lekalo_core::expressions::vector_expression_unknown(
                &vector.id,
            ));
        };
        let bindings = match lekalo_core::expressions::Bindings::from_json(record, &vector.bindings)
        {
            Ok(bindings) => bindings,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
        let clock = vector
            .clock
            .map(Clock::from_seconds)
            .unwrap_or_else(|| Clock::from_datetime("1970-01-01T00:00:00Z").expect("epoch"));
        match evaluate(record, &bindings, &clock) {
            Ok(value) => {
                let expected_ok = match &vector.expect {
                    VectorExpect::Value(expected) => expected.to_json() == value.to_json(),
                    VectorExpect::Error(_) => false,
                };
                if !expected_ok {
                    failures += 1;
                }
                rows.push(format!(
                    "{{\"id\":\"{}\",\"value\":{}}}",
                    vector.id,
                    value.to_json()
                ));
            }
            Err(diagnostics) => {
                let token = diagnostics
                    .as_slice()
                    .iter()
                    .find(|d| d.id() == "expression.eval-invalid")
                    .and_then(|d| d.data().get("detail"))
                    .and_then(|value| serde_json::to_string(value).ok())
                    .unwrap_or_else(|| "\"eval-failed\"".to_owned());
                let expected_ok = match &vector.expect {
                    VectorExpect::Error(expected) => token.contains(expected.key()),
                    VectorExpect::Value(_) => false,
                };
                if !expected_ok {
                    failures += 1;
                }
                rows.push(format!("{{\"id\":\"{}\",\"error\":{}}}", vector.id, token));
            }
        }
    }
    let json = format!(
        "{{\"status\":\"valid\",\"expressionEval\":{{\"vectors\":{},\"failures\":{},\"results\":[{}]}}}}",
        vectors.vectors.len(),
        failures,
        rows.join(","),
    );
    let human = format!(
        "expression eval: {} vectors, {} mismatched expectations",
        vectors.vectors.len(),
        failures,
    );
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo expressions render`: the complete cross-target program.
fn expressions_render(path: &str, target: ExpressionTarget, support: Option<&str>) -> DomainResult {
    let attachment = match expressions_attachment(path, support) {
        Ok(attachment) => attachment,
        Err(result) => return result,
    };
    let core_target = match target {
        ExpressionTarget::Node => lekalo_core::expressions::Target::Node,
        ExpressionTarget::Php => lekalo_core::expressions::Target::Php,
        ExpressionTarget::Go => lekalo_core::expressions::Target::Go,
    };
    let program = lekalo_core::expressions::render_program(&attachment, core_target);
    let json = format!(
        "{{\"status\":\"valid\",\"expressionRender\":{{\"target\":\"{}\",\"program\":{}}}}}",
        core_target.key(),
        serde_json::to_string(&program).unwrap_or_default(),
    );
    let human = program;
    DomainResult::graph(json, human, Vec::new())
}

/// `lekalo expressions diff`.
fn expressions_diff(base_path: &str, candidate_path: &str) -> DomainResult {
    let base_document = match read_expressions_document(base_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let candidate_document = match read_expressions_document(candidate_path) {
        Ok(document) => document,
        Err(result) => return result,
    };
    let base = match lekalo_core::expressions::ExpressionsAttachment::from_value(&base_document) {
        Ok(attachment) => attachment,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let candidate =
        match lekalo_core::expressions::ExpressionsAttachment::from_value(&candidate_document) {
            Ok(attachment) => attachment,
            Err(diagnostics) => return DomainResult::invalid(diagnostics),
        };
    let diff = match lekalo_core::expressions::compare(&base, &candidate) {
        Ok(diff) => diff,
        Err(diagnostics) => return DomainResult::invalid(diagnostics),
    };
    let count = |class| -> usize {
        diff.paths()
            .iter()
            .filter(|path| path.class() == class)
            .count()
    };
    let breaking = count(lekalo_core::expressions::DiffClass::Breaking);
    let non_breaking = count(lekalo_core::expressions::DiffClass::NonBreaking);
    let policy_change = count(lekalo_core::expressions::DiffClass::PolicyChange);
    let paths: Vec<String> = diff
        .paths()
        .iter()
        .map(|path| {
            format!(
                "{{\"path\":\"{}\",\"class\":\"{}\"}}",
                path.path(),
                path.class().key()
            )
        })
        .collect();
    let json = format!(
        "{{\"status\":\"valid\",\"expressionsDiff\":{{\"equal\":{},\"breaking\":{},\"nonBreaking\":{},\"policyChange\":{},\"paths\":[{}]}}}}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
        paths.join(","),
    );
    let human = format!(
        "expressions diff: equal {}; breaking {}; non-breaking {}; policy-change {}",
        diff.equal(),
        breaking,
        non_breaking,
        policy_change,
    );
    DomainResult::graph(json, human, Vec::new())
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

/// Run `lekalo generate`: `--check` is the read-only drift gate (with
/// the optional `--locked` inventory preflight), `--clean` previews and
/// applies the deterministic orphan clean, and `--adapter` runs the
/// issue #91 generation pipeline: bind the exact lock and inputs, plan
/// through the target protocol, and — only without `--dry-run` —
/// publish verified writes and replace the ownership manifest.
#[allow(clippy::too_many_arguments)]
fn run_generate(
    project: Option<String>,
    check: bool,
    locked: bool,
    clean: bool,
    dry_run: bool,
    confirm: Option<String>,
    targets: Vec<String>,
    module: Option<String>,
    allow_permission_expansion: bool,
    program_args: Vec<String>,
    timeout_ms: u64,
) -> DomainResult {
    // Exactly one mode; the clean modifiers belong to --clean only; a
    // mutating clean needs a bound preview identity, never a bare run.
    let with_adapter = !program_args.is_empty();
    if check {
        // The drift gate takes no generation or clean modifiers.
        if clean || dry_run || with_adapter || !targets.is_empty() || module.is_some() {
            return DomainResult::usage_error();
        }
    } else if clean {
        if with_adapter || !targets.is_empty() || module.is_some() || dry_run == confirm.is_some() {
            if dry_run {
                return DomainResult::usage_error();
            }
            // A mutating clean without a bound preview never ships by
            // accident.
            return DomainResult::from(&ArtifactFailure::PreviewRequired);
        }
        if let Some(plan_id) = &confirm {
            if well_formed_plan_id(plan_id).is_none() {
                return DomainResult::usage_error();
            }
        }
    } else {
        // The generation pipeline: the adapter program vector is the
        // mandatory generation operand.
        if !with_adapter && targets.is_empty() && module.is_none() && !dry_run {
            return DomainResult::usage_error();
        }
    }
    let selection = selection_for(&project);
    if check {
        if locked {
            // The `--locked` drift gate refuses an inventory that does
            // not carry every locked component before reading bytes.
            if let Err(result) = lekalo_core::orchestration::locked_check(&selection) {
                return result;
            }
        }
        return match GenerateService::check(&selection) {
            Ok(receipt) => DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                check_human(&receipt),
            ),
            Err(failure) => DomainResult::from(&failure),
        };
    }
    if clean {
        if dry_run {
            return match GenerateService::clean_plan(&selection) {
                Ok(receipt) => DomainResult::receipt(
                    serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                    clean_human("preview", &receipt.plan_id, receipt.count),
                ),
                Err(failure) => DomainResult::from(&failure),
            };
        }
        let plan_id = confirm.as_deref().expect("exclusivity checked above");
        return match GenerateService::clean_apply(&selection, plan_id) {
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
        };
    }
    // The generation pipeline: an adapter program is mandatory.
    let Some((program, adapter_args)) = program_args.split_first() else {
        return DomainResult::usage_error();
    };
    let root = match lekalo_core::orchestration::project_root(&selection) {
        Ok(root) => root,
        Err(result) => return result,
    };
    let supply =
        match lekalo_core::orchestration::AdapterSupply::new(&root, program, adapter_args.to_vec())
        {
            Ok(supply) => supply,
            Err(failure) => return DomainResult::from(&failure),
        };
    lekalo_core::orchestration::generate(lekalo_core::orchestration::GenerateRequest {
        selection: &selection,
        targets,
        module,
        dry_run,
        locked,
        supply: Some(supply),
        timeout_ms,
        allow_permission_expansion,
    })
}

/// Run `lekalo verify` (issue #91): the read-only aggregation of the
/// core validation, the drift gate, the per-target adapter validation,
/// and the optional binding, scenario, and trace summaries. Nothing is
/// ever written; the exit class distinguishes fail, degraded, and
/// infrastructure/config errors.
#[allow(clippy::too_many_arguments)]
fn run_verify(
    project: Option<String>,
    targets: Vec<String>,
    module: Option<String>,
    changed: bool,
    locked: bool,
    trace: Option<String>,
    program_args: Vec<String>,
    timeout_ms: u64,
) -> DomainResult {
    let selection = selection_for(&project);
    // The affected scope of a `--changed` run: resolved here through the
    // accepted Git handoff, consumed as plain module ids by the core.
    let mut changed_modules = Vec::new();
    if changed {
        let root = match lekalo_core::orchestration::project_root(&selection) {
            Ok(root) => root,
            Err(result) => return result,
        };
        let compilation = match compile_selection(&selection) {
            Ok(compilation) => compilation,
            Err(result) => return result,
        };
        let set = match git_input::changed_input_set(
            &root,
            None,
            None,
            true,
            &source_paths_of(&compilation),
        ) {
            Ok(set) => set,
            Err(failure) => return DomainResult::invalid(failure.diagnostic_set()),
        };
        for entry in set.entries() {
            for symbol in entry.symbol_ids() {
                if let Some(module) = symbol.split('.').next() {
                    if !changed_modules.iter().any(|known| known == module) {
                        changed_modules.push(module.to_owned());
                    }
                }
            }
        }
        changed_modules.sort();
    }
    let supply = match program_args.split_first() {
        Some((program, adapter_args)) => {
            let root = match lekalo_core::orchestration::project_root(&selection) {
                Ok(root) => root,
                Err(result) => return result,
            };
            match lekalo_core::orchestration::AdapterSupply::new(
                &root,
                program,
                adapter_args.to_vec(),
            ) {
                Ok(supply) => Some(supply),
                Err(failure) => return DomainResult::from(&failure),
            }
        }
        None => None,
    };
    lekalo_core::orchestration::verify(lekalo_core::orchestration::VerifyRequest {
        selection: &selection,
        targets,
        module,
        changed_modules,
        locked,
        trace,
        supply,
        timeout_ms,
    })
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

use std::path::PathBuf;

/// Run `lekalo scan`: discover and select one target adapter through the
/// accepted #28 seam, run the read-only `scan` exchange, and merge the
/// produced inventory into the binding registry through the accepted #39
/// seam. The adapter program is spawned directly (no shell); a missing
/// program is the stable usage failure.
/// Run `lekalo native run` (issue #48): the production execution
/// surface. The core independently validates the plan document and the
/// approval binding, then answers with the typed refusal for this
/// repository trust: private/untrusted is plan-only until #89
/// (confinement-required) and a trusted synthetic fixture plan is
/// answered unsupported (fixture-runner-not-shipped) because the real
/// fixture runner is compiled only into the test harness. No gate
/// command is ever launched from this binary.
fn run_native_run(plan_ref: &str) -> DomainResult {
    let bytes = if plan_ref == "-" {
        let mut buffer = Vec::new();
        use std::io::Read;
        if io::stdin().lock().read_to_end(&mut buffer).is_err() {
            return DomainResult::usage_error();
        }
        buffer
    } else {
        match std::fs::read(plan_ref) {
            Ok(bytes) => bytes,
            Err(_) => return DomainResult::usage_error(),
        }
    };
    match lekalo_core::native_gate::production_run(&bytes) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "native run : {} (plan {})",
                receipt.outcome,
                receipt.plan_digest.get(..19).unwrap_or("")
            ),
        ),
        Err(failure) => DomainResult::from(&failure),
    }
}

fn run_scan(
    target: &str,
    profile: Option<&str>,
    timeout_ms: u64,
    project: &Option<String>,
    program_args: Vec<String>,
) -> DomainResult {
    let Some((program, args)) = program_args.split_first() else {
        return DomainResult::usage_error();
    };
    if program.is_empty() {
        return DomainResult::usage_error();
    }
    let selection = selection_for(project);
    let context = match lekalo_core::observed::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    let limits = lekalo_core::target_protocol::transport::TransportLimits {
        timeout_ms,
        ..Default::default()
    };
    let request = lekalo_core::observed::scan_service::ScanRequest {
        target,
        profile,
        command: lekalo_core::target_protocol::transport::AdapterCommand {
            program: PathBuf::from(program),
            args: args.to_vec(),
        },
        limits,
    };
    match lekalo_core::observed::scan_service::run(&context, &request) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "scan {} target {} : {} bindings ({} explicit, {} confirmed, {} inferred, {} \
                 stale), {} tests",
                receipt.project,
                receipt.target.as_deref().unwrap_or("-"),
                receipt.symbols,
                receipt.explicit,
                receipt.confirmed,
                receipt.inferred,
                receipt.stale,
                receipt.test_bindings
            ),
        ),
        Err(result) => result,
    }
}

/// The human rows of `bindings list`: one line per relation row.
fn bindings_list_human(receipt: &lekalo_core::observed::bindings::ListReceipt) -> String {
    let mut lines = vec![format!(
        "bindings {} target {} : {} bindings, {} endpoints, {} tests",
        receipt.project,
        receipt.target.as_deref().unwrap_or("-"),
        receipt.counts.bindings,
        receipt.counts.endpoints,
        receipt.counts.tests
    )];
    for row in receipt
        .bindings
        .iter()
        .chain(receipt.endpoints.iter())
        .chain(receipt.tests.iter())
    {
        let native = row.native.as_deref().unwrap_or("-");
        let path = row.path.as_deref().unwrap_or("");
        let at = match row.line {
            Some(line) if !path.is_empty() => format!(":{line}"),
            _ => String::new(),
        };
        let place = format!("{path}{at}");
        lines.push(format!(
            "  {} {} {} {} {} ({}, {}, {})",
            row.relation,
            row.semantic,
            row.kind,
            native,
            place,
            row.source,
            row.confidence,
            row.state
        ));
    }
    lines.join("\n")
}

/// Run one `bindings` subcommand (issue #42): load the project through
/// the accepted seam, hand everything to the core binding registry, and
/// project the result. Every registry decision — proposal derivation,
/// ambiguity policy, confirmation, batch plans, freshness — lives in the
/// core; this binary only selects, renders, and maps exits.
fn run_bindings(command: BindingsCommands) -> DomainResult {
    match command {
        BindingsCommands::List { project } => {
            let context = match observed_context(&project) {
                Ok(context) => context,
                Err(result) => return result,
            };
            match lekalo_core::observed::bindings::list(&context) {
                Ok(receipt) => DomainResult::receipt(
                    serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                    bindings_list_human(&receipt),
                ),
                Err(set) => DomainResult::invalid(set),
            }
        }
        BindingsCommands::Propose { project } => {
            let context = match observed_context(&project) {
                Ok(context) => context,
                Err(result) => return result,
            };
            match lekalo_core::observed::bindings::propose(&context) {
                Ok(receipt) => {
                    let mut lines = vec![format!(
                        "bindings propose {} proposals ({} ambiguous)",
                        receipt.proposals.len(),
                        receipt.ambiguous
                    )];
                    for proposal in &receipt.proposals {
                        lines.push(format!(
                            "  {} {} {} ({}{} candidates)",
                            proposal.proposal,
                            proposal.symbol,
                            proposal.confidence,
                            if proposal.ambiguous {
                                "ambiguous, "
                            } else {
                                ""
                            },
                            proposal.candidates.len()
                        ));
                        for candidate in &proposal.candidates {
                            lines.push(format!(
                                "    candidate {} {} (at {}:{}, {})",
                                candidate.native,
                                candidate.confidence,
                                candidate.path,
                                candidate.line.unwrap_or(0),
                                candidate.fingerprint.as_deref().unwrap_or("-")
                            ));
                        }
                    }
                    DomainResult::receipt(
                        serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                        lines.join("\n"),
                    )
                }
                Err(set) => DomainResult::invalid(set),
            }
        }
        BindingsCommands::Confirm {
            proposal,
            candidate,
            batch,
            preview,
            confirm,
            project,
        } => {
            // Exactly one action: a single proposal, a batch preview, or
            // a batch apply; modifiers never mix across the modes.
            if batch == proposal.is_some() {
                return DomainResult::usage_error();
            }
            if (preview || confirm.is_some()) != batch {
                return DomainResult::usage_error();
            }
            if preview && confirm.is_some() {
                return DomainResult::usage_error();
            }
            if let Some(plan_id) = confirm.as_deref() {
                if well_formed_plan_id(plan_id).is_none() {
                    return DomainResult::usage_error();
                }
            }
            let context = match observed_context(&project) {
                Ok(context) => context,
                Err(result) => return result,
            };
            if batch {
                match lekalo_core::observed::bindings::confirm_batch(&context, confirm.as_deref()) {
                    Ok(receipt) if receipt.phase == "plan" => DomainResult::receipt(
                        serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                        format!(
                            "bindings confirm preview plan {} ({} proposals)",
                            receipt.plan.as_deref().unwrap_or("-"),
                            receipt.entries.len()
                        ),
                    ),
                    Ok(receipt) => DomainResult::receipt(
                        serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                        format!(
                            "bindings confirm applied plan {} ({} confirmed)",
                            receipt.plan.as_deref().unwrap_or("-"),
                            receipt.confirmed.len()
                        ),
                    ),
                    Err(set) => DomainResult::invalid(set),
                }
            } else {
                let proposal_id = proposal.as_deref().expect("exclusivity checked above");
                match lekalo_core::observed::bindings::confirm(
                    &context,
                    proposal_id,
                    candidate.as_deref(),
                ) {
                    Ok(receipt) => DomainResult::receipt(
                        serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                        format!(
                            "bindings confirm {} -> {} ({} at {}:{})",
                            receipt.symbol,
                            receipt.native,
                            receipt.binding,
                            receipt.path,
                            receipt.state
                        ),
                    ),
                    Err(set) => DomainResult::invalid(set),
                }
            }
        }
        BindingsCommands::Audit { project } => {
            let context = match observed_context(&project) {
                Ok(context) => context,
                Err(result) => return result,
            };
            match lekalo_core::observed::bindings::audit(&context) {
                Ok(receipt) => DomainResult::receipt(
                    serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                    format!(
                        "bindings audit {} symbols ({} current, {} unknown), {} tests ({} \
                         current, {} unknown)",
                        receipt.symbols,
                        receipt.current,
                        receipt.unknown,
                        receipt.test_bindings,
                        receipt.tests_current,
                        receipt.tests_unknown
                    ),
                ),
                Err(set) => DomainResult::invalid(set),
            }
        }
    }
}

/// The observed context of one selection (`--project` beats
/// `LEKALO_PROJECT`); loader failures pass through untouched.
fn observed_context(
    project: &Option<String>,
) -> Result<lekalo_core::observed::ObservedContext, DomainResult> {
    let selection = selection_for(project);
    lekalo_core::observed::context(&selection)
}
/// Read the declaration document bytes; the path is an
/// invocation-relative input document, never a project file.
fn declaration_bytes(path: &str) -> Result<Vec<u8>, DomainResult> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let detail = match error.kind() {
                std::io::ErrorKind::NotFound => "declaration-missing",
                _ => "declaration-unreadable",
            };
            return Err(DomainResult::invalid(
                lekalo_core::contracted::declaration_invalid_set(detail, None),
            ));
        }
    };
    if bytes.len() > lekalo_core::contracted::MAX_DECLARATION_BYTES {
        return Err(DomainResult::invalid(
            lekalo_core::contracted::declaration_limit_set("declaration-bytes", bytes.len()),
        ));
    }
    Ok(bytes)
}

/// Run one `contract` subcommand (issue #40): load the project through
/// the accepted seam, hand everything to the core contracted engine,
/// and project the result. Every contracted decision — declaration
/// validation, conformance classification, attachment custody, and
/// support-artifact ownership — lives in the core.
fn run_contract(command: ContractCommands) -> DomainResult {
    match command {
        ContractCommands::Update {
            declaration,
            project,
        } => run_contract_update(&declaration, &project),
        ContractCommands::Check { module, project } => {
            run_contract_check(module.as_deref(), &project)
        }
        ContractCommands::Attach {
            symbol,
            native_test,
            gate,
            project,
        } => run_contract_attach(&symbol, native_test.as_deref(), gate.as_deref(), &project),
        ContractCommands::Support {
            symbol,
            kind,
            path,
            lifecycle,
            digest,
            project,
        } => run_contract_support(
            &symbol,
            &kind,
            &path,
            &lifecycle,
            digest.as_deref(),
            &project,
        ),
    }
}

fn run_contract_update(declaration: &str, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let bytes = match declaration_bytes(declaration) {
        Ok(bytes) => bytes,
        Err(result) => return result,
    };
    let context = match lekalo_core::contracted::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::contracted::update_registry(&context, &bytes) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "contract update {} symbols ({} recorded, {} artifacts)",
                receipt.symbols,
                receipt.recorded.len(),
                receipt.artifacts
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_contract_check(module: Option<&str>, project: &Option<String>) -> DomainResult {
    let selection = selection_for(project);
    let context = match lekalo_core::contracted::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::contracted::check(&context, module) {
        Ok(receipt) => {
            let human = format!(
                "contract check {} symbols ({} conformant, {} stale, {} unknown, {} artifacts, {} stale artifacts)",
                receipt.symbols,
                receipt.conformant,
                receipt.stale,
                receipt.unknown,
                receipt.artifacts,
                receipt.stale_artifacts
            );
            DomainResult::receipt(
                serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
                human,
            )
        }
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_contract_attach(
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
    let context = match lekalo_core::contracted::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::contracted::attach(&context, symbol, &tests, &gates) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "contract attach {} ({} native tests, {} gates)",
                receipt.symbol,
                receipt.native_tests.len(),
                receipt.gates.len()
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}

fn run_contract_support(
    symbol: &str,
    kind: &str,
    path: &str,
    lifecycle: &str,
    digest: Option<&str>,
    project: &Option<String>,
) -> DomainResult {
    let Some(kind) = lekalo_core::contracted::SupportKind::parse(kind) else {
        return DomainResult::usage_error();
    };
    let Some(lifecycle) = lekalo_core::contracted::SupportLifecycle::parse(lifecycle) else {
        return DomainResult::usage_error();
    };
    let selection = selection_for(project);
    let context = match lekalo_core::contracted::context(&selection) {
        Ok(context) => context,
        Err(result) => return result,
    };
    match lekalo_core::contracted::support(&context, symbol, kind, path, lifecycle, digest) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
            format!(
                "contract support {} ({} {})",
                receipt.path,
                receipt.kind.key(),
                receipt.lifecycle.key()
            ),
        ),
        Err(set) => DomainResult::invalid(set),
    }
}
/// The `classification` subcommands (issue #87).
#[derive(Debug, Subcommand)]
enum ClassificationCommands {
    /// Validate one classification attachment and its governing policy
    /// against the project: custody pins, subject resolution, grant
    /// coherence, and (under the strict profile) the sensitive-sink rule
    /// over the declared graph. The documents are read from the given
    /// project-relative paths; the core owns every decision.
    Validate {
        /// The classification attachment document path.
        #[arg(long, value_name = "PATH")]
        attachment: String,
        /// The classification-policy document path.
        #[arg(long, value_name = "PATH")]
        policy: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`); expiry is deterministic in this date, never
        /// a clock. Defaults to the fixed classification as-of date,
        /// overridable via `LEKALO_AS_OF`.
        #[arg(long, value_name = "DATE")]
        as_of: Option<String>,
    },
    /// Inspect one classification attachment: the resolved kinds of
    /// every declared subject in canonical order.
    Inspect {
        /// The classification attachment document path.
        #[arg(long, value_name = "PATH")]
        attachment: String,
        /// The classification-policy document path.
        #[arg(long, value_name = "PATH")]
        policy: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`); expiry is deterministic in this date, never
        /// a clock. Defaults to the fixed classification as-of date,
        /// overridable via `LEKALO_AS_OF`.
        #[arg(long, value_name = "DATE")]
        as_of: Option<String>,
    },
}

/// The `privacy` subcommands (issue #119).
#[derive(Debug, Subcommand)]
enum PrivacyCommands {
    /// Evaluate one export-decision input (exact JSON bytes) against
    /// the custody-verified frozen #120 policy. Prints the closed
    /// `ExportDecisionOutput` and exits 0 (allow), 3 (deny or
    /// transform-required), or 1 (malformed input or custody
    /// failure). Startup, parse, and custody failures print the
    /// separate closed CLI error object on stderr.
    Evaluate {
        /// The decision input document path.
        #[arg(long, value_name = "FILE")]
        decision: String,
    },
    /// Export one artifact document under the fail-closed privacy
    /// pipeline (issue #119): class resolution, decision evaluation,
    /// the closed redaction transforms, the leak-scanner verification
    /// pass, and the writes under `.lekalo/privacy/`. `--dry-run`
    /// prints the exact candidate payload and the redaction diff and
    /// writes nothing.
    Export {
        /// The artifact envelope document path.
        artifact: String,
        /// The closed destination spec: `workspace`,
        /// `repository-store`, `transfer-tenant`, `transfer-external`,
        /// `transfer-cross-tenant`, or `publish`.
        #[arg(long, value_name = "SPEC")]
        destination: String,
        /// Plan the export and print the candidate payload plus the
        /// redaction diff; writes nothing.
        #[arg(long)]
        dry_run: bool,
        /// The authorizing export-transfer consent evidence document
        /// (bound to the exact subject digest by the runtime).
        #[arg(long, value_name = "FILE")]
        consent: Option<String>,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Show the redaction diff contract of one payload document
    /// (issue #119): the closed transforms, the leak findings, and
    /// the redacted payload on stdout. Read-only: writes nothing.
    Redact {
        /// The payload document path.
        #[arg(long, value_name = "FILE")]
        payload: String,
        /// Accepted for symmetry with `export`; redaction is
        /// read-only by definition.
        #[arg(long)]
        dry_run: bool,
        /// The declared repository identity to scan for.
        #[arg(long, value_name = "NAME")]
        repository: Option<String>,
        /// A declared protected term (person name) to scan for;
        /// repeatable.
        #[arg(long = "term", value_name = "NAME")]
        terms: Vec<String>,
    },
}

/// The `dataflow` subcommands (issue #87).
#[derive(Debug, Subcommand)]
enum DataflowCommands {
    /// Derive the data-flow report over the classified project: flows,
    /// findings, unknowns, and the aggregated gate verdict, pinned to
    /// the exact input digests. Read-only; the report is never written
    /// by this command.
    Report {
        /// The classification attachment document path.
        #[arg(long, value_name = "PATH")]
        attachment: String,
        /// The classification-policy document path.
        #[arg(long, value_name = "PATH")]
        policy: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The reference date for expiry and validity evaluation
        /// (`YYYY-MM-DD`); expiry is deterministic in this date, never
        /// a clock. Defaults to the fixed classification as-of date,
        /// overridable via `LEKALO_AS_OF`.
        #[arg(long, value_name = "DATE")]
        as_of: Option<String>,
        /// One transport endpoint-actor binding (issue #70 seam, plan
        /// §5.1): `SYMBOL:ACTOR` where ACTOR is `public` or
        /// `authenticated`; the binding resolves the endpoint's invoked
        /// operation result. Repeatable.
        #[arg(long = "endpoint", value_name = "SYMBOL:ACTOR")]
        endpoints: Vec<String>,
    },
}

/// Parse one `--endpoint SYMBOL:ACTOR` binding into a typed exposure
/// (issue #70 seam): the endpoint symbol must resolve to a declared
/// endpoint definition and its actor must be the closed vocabulary.
fn parse_endpoint_exposures(
    endpoints: &[String],
    compilation: &lekalo_core::ir::Compilation,
) -> Result<Vec<lekalo_core::dataflow::EndpointExposure>, DomainResult> {
    let mut exposures = Vec::new();
    for endpoint in endpoints {
        let Some((symbol, actor)) = endpoint.rsplit_once(':') else {
            return Err(DomainResult::usage_error());
        };
        let actor = match actor {
            "public" => lekalo_core::dataflow::EndpointActor::Public,
            "authenticated" => lekalo_core::dataflow::EndpointActor::Authenticated,
            _ => return Err(DomainResult::usage_error()),
        };
        let Some(lekalo_core::ir::Definition::Endpoint(definition)) = compilation
            .project
            .definitions
            .iter()
            .find(|definition| definition.id().as_str() == symbol)
        else {
            return Err(DomainResult::invalid(
                lekalo_core::classification::diagnostic::unknown_subject(
                    "endpoint-unknown",
                    symbol,
                ),
            ));
        };
        exposures.push(lekalo_core::dataflow::EndpointExposure {
            endpoint: symbol.to_owned(),
            actor,
            result_subject: lekalo_core::classification::SubjectPath::parse(
                definition.invokes.as_str(),
            )
            .map_err(|_| DomainResult::usage_error())?,
        });
    }
    Ok(exposures)
}

/// Validate the classification as-of input at the CLI boundary (r4
/// F-1): expiry is a lexicographic compare against the wire's
/// fixed-width UTC shape, so raw unchecked text fails open (`--as-of
/// '!'` makes an expired grant live). Accepted spellings: the wire
/// shape `YYYY-MM-DDTHH:MM:SSZ` (validated date and time), or a bare
/// `YYYY-MM-DD` normalized to midnight UTC. Anything else refuses
/// before any evaluation — malformed input denies, never passes.
fn normalize_as_of(raw: &str) -> Option<String> {
    // ASCII only (r5 F-1): the fixed-width checks below slice at byte
    // offsets, and a multi-byte char spanning a slice boundary would
    // panic instead of refusing. Non-ASCII input is never a valid
    // spelling — refuse it before any slicing happens.
    if !raw.is_ascii() {
        return None;
    }
    let bytes = raw.as_bytes();
    if bytes.len() == 20 && bytes[10] == b'T' && bytes[19] == b'Z' {
        lekalo_core::nfr::IsoDate::parse(&raw[..10]).ok()?;
        let (hour, minute, second) = (&raw[11..13], &raw[14..16], &raw[17..19]);
        if &raw[13..14] != ":" || &raw[16..17] != ":" {
            return None;
        }
        let digits = [hour, minute, second]
            .iter()
            .all(|part| part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_digit()));
        if !digits {
            return None;
        }
        let (hour, minute, second) = (
            hour.parse::<u8>().ok()?,
            minute.parse::<u8>().ok()?,
            second.parse::<u8>().ok()?,
        );
        (hour < 24 && minute < 60 && second < 60).then(|| raw.to_owned())
    } else if bytes.len() == 10 {
        let date = lekalo_core::nfr::IsoDate::parse(raw).ok()?;
        Some(format!("{}T00:00:00Z", date.as_str()))
    } else {
        None
    }
}

/// Resolve the effective as-of date for one classification surface:
/// the `--as-of` flag, else `LEKALO_AS_OF`, else the fixed
/// deterministic default. Malformed input is a usage refusal.
fn resolve_as_of(flag: Option<String>) -> Result<String, DomainResult> {
    match flag.or_else(|| std::env::var("LEKALO_AS_OF").ok()) {
        None => Ok(lekalo_core::classification::DEFAULT_AS_OF.to_owned()),
        Some(text) => normalize_as_of(&text).ok_or_else(DomainResult::usage_error),
    }
}

/// Read one attachment document; IO failure is a typed invalid set.
fn read_document(path: &str) -> Result<Vec<u8>, DomainResult> {
    std::fs::read(path)
        .map_err(|_| DomainResult::invalid(lekalo_core::classification::io_failure_set()))
}

/// Load the compiled project for the classification/dataflow surfaces:
/// the shared loader seam with the same cache semantics as validate.
fn load_compiled_for(
    project: &Option<String>,
) -> Result<(String, lekalo_core::ir::Compilation), DomainResult> {
    let selection = LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    #[allow(clippy::question_mark)] // DomainResult is not an error type
    let (model, compilation) = match lekalo_core::cache::load_compiled(&selection, false) {
        Err(result) => return Err(result),
        Ok(pair) => pair,
    };
    let model_json = lekalo_core::loader::canonical_model_bytes(&model);
    Ok((model_json, compilation))
}

/// Parse the classification attachment and its governing policy, and
/// resolve both against the pinned compilation. Custody mismatches
/// (project, modelRef, irRef) are typed invalid sets from the core.
fn parse_classification_pair(
    attachment_path: &str,
    policy_path: &str,
    compilation: &lekalo_core::ir::Compilation,
    model_json: &str,
) -> Result<
    (
        lekalo_core::classification::Attachment,
        lekalo_core::classification::PolicyAttachment,
        lekalo_core::classification::Resolution,
    ),
    DomainResult,
> {
    let attachment_bytes = read_document(attachment_path)?;
    let policy_bytes = read_document(policy_path)?;
    let attachment = lekalo_core::classification::Attachment::parse(&attachment_bytes)
        .map_err(DomainResult::invalid)?;
    let policy = lekalo_core::classification::PolicyAttachment::parse(&policy_bytes)
        .map_err(DomainResult::invalid)?;
    if let Err(set) = lekalo_core::classification::validate_custody(
        &attachment,
        &policy,
        &compilation.project,
        model_json,
    ) {
        return Err(DomainResult::invalid(set));
    }
    let resolution = lekalo_core::classification::validate_subjects(&attachment, compilation)
        .map_err(DomainResult::invalid)?;
    Ok((attachment, policy, resolution))
}

/// Run one `classification` subcommand (issue #87): validate or
/// inspect. The core owns every decision; this binary reads the two
/// documents, renders, and maps exits.
fn run_classification(command: ClassificationCommands) -> DomainResult {
    match command {
        ClassificationCommands::Validate {
            attachment,
            policy,
            project,
            as_of,
        } => {
            let as_of = match resolve_as_of(as_of) {
                Err(result) => return result,
                Ok(resolved) => resolved,
            };
            let (model_json, compilation) = match load_compiled_for(&project) {
                Err(result) => return result,
                Ok(pair) => pair,
            };
            let (attachment, policy, resolution) =
                match parse_classification_pair(&attachment, &policy, &compilation, &model_json) {
                    Err(result) => return result,
                    Ok(parts) => parts,
                };
            match lekalo_core::classification::validate_policy_and_grants_as_of(
                &attachment,
                &policy,
                &resolution,
                &compilation.project,
                &as_of,
            ) {
                Err(set) => DomainResult::invalid(set),
                Ok(outcome) => {
                    let (json, human) = classification_validate_payload(&attachment, &outcome);
                    if outcome.invalid {
                        DomainResult::invalid(lekalo_core::classification::findings_set(&outcome))
                    } else {
                        DomainResult::graph(json, human, Vec::new())
                    }
                }
            }
        }
        ClassificationCommands::Inspect {
            attachment,
            policy,
            project,
            as_of,
        } => {
            let as_of = match resolve_as_of(as_of) {
                Err(result) => return result,
                Ok(resolved) => resolved,
            };
            let (model_json, compilation) = match load_compiled_for(&project) {
                Err(result) => return result,
                Ok(pair) => pair,
            };
            let (attachment, policy, resolution) =
                match parse_classification_pair(&attachment, &policy, &compilation, &model_json) {
                    Err(result) => return result,
                    Ok(parts) => parts,
                };
            let (json, human) =
                classification_inspect_payload(&attachment, &policy, &resolution, &as_of);
            DomainResult::graph(json, human, Vec::new())
        }
    }
}

/// Render the `classification validate` payload.
fn classification_validate_payload(
    attachment: &lekalo_core::classification::Attachment,
    outcome: &lekalo_core::classification::ValidationOutcome,
) -> (String, String) {
    let mut json = String::from("{\"status\":");
    json.push_str(if outcome.invalid {
        "\"invalid\","
    } else {
        "\"valid\","
    });
    json.push_str("\"identity\":");
    json.push_str(&serde_json::to_string(lekalo_core::classification::IDENTITY).expect("identity"));
    json.push_str(",\"subjects\":");
    json.push_str(&attachment.classifications().len().to_string());
    json.push_str(&outcome.wire_findings());
    json.push('}');
    let human = if outcome.invalid {
        format!(
            "classification invalid: {} finding(s)",
            outcome.finding_count()
        )
    } else {
        format!(
            "classification valid: {} subject(s), 0 findings",
            attachment.classifications().len()
        )
    };
    (json, human)
}

/// Render the `classification inspect` payload.
fn classification_inspect_payload(
    attachment: &lekalo_core::classification::Attachment,
    policy: &lekalo_core::classification::PolicyAttachment,
    resolution: &lekalo_core::classification::Resolution,
    as_of: &str,
) -> (String, String) {
    let mut json = String::from("{\"status\":\"valid\",\"subjects\":[");
    let mut human = Vec::new();
    for (index, entry) in attachment.classifications().iter().enumerate() {
        // Grants participate in the inspect view: a *valid* reviewed
        // lowering shows the lowered kind — the same shared predicate
        // every grant consumer uses, so an expired or otherwise dead
        // grant never lowers the displayed kind (review r3, F-2).
        let mark = resolution.resolve_with_grants(entry.subject(), |grant| {
            lekalo_core::classification::grant_is_valid(grant, policy, as_of)
        });
        let resolved = lekalo_core::classification::ResolvedKind::Classified(mark.kind);
        if index > 0 {
            json.push(',');
        }
        let kind = resolved
            .kind()
            .map(|kind| kind.as_str().to_owned())
            .unwrap_or_else(|| "unclassified".to_owned());
        json.push_str(
            &serde_json::to_string(&serde_json::json!({
                "subject": entry.subject().as_str(),
                "kind": kind,
            }))
            .expect("subject row"),
        );
        human.push(format!("{} : {}", entry.subject().as_str(), kind));
    }
    json.push_str("]}");
    (json, human.join("\n"))
}

/// Run one `dataflow` subcommand (issue #87): derive the read-only
/// report. The core owns every decision; this binary reads the two
/// documents, renders, and maps exits.
/// The privacy evaluator protocol (issue #119, plan S3): the exact
/// exit contract of the reference evaluator - 0 allow, 3 deny or
/// transform-required, 1 malformed input or custody failure - with
/// the closed `ExportDecisionOutput` on stdout and the closed
/// `{status:"invalid",reasonCodes:[...]}` startup object on stderr.
/// Never a secret, value, or payload crosses this surface: the input
/// is metadata-only and the output is the closed decision shape.
fn run_privacy_evaluate(path: &str) -> u8 {
    let context = match lekalo_core::privacy::TrustedContext::embedded() {
        Err(error) => {
            let _ = write_stderr(&cli_invalid_json(error.code()));
            return OUTPUT_FAILURE;
        }
        Ok(context) => context,
    };
    let bytes = match std::fs::read(path) {
        Err(_) => {
            let _ = write_stderr(&cli_invalid_json("custody.required-file-missing"));
            return OUTPUT_FAILURE;
        }
        Ok(bytes) => bytes,
    };
    let input: serde_json::Value = match serde_json::from_slice(&bytes) {
        Err(error) => {
            let _ = write_stderr(&cli_invalid_json(&format!(
                "custody.decision.json: {error}"
            )));
            return OUTPUT_FAILURE;
        }
        Ok(input) => input,
    };
    let evaluated = lekalo_core::privacy::evaluate::evaluate_decision(&input, context);
    let rendered = serde_json::to_string_pretty(&evaluated.output).unwrap_or_default();
    let write_ok = write_stdout(&rendered);
    let exit = if evaluated.malformed {
        OUTPUT_FAILURE
    } else {
        evaluated.output.exit_code()
    };
    if write_ok {
        exit
    } else {
        OUTPUT_FAILURE
    }
}

/// The closed CLI startup-error object (never an `ExportDecisionOutput`).
fn cli_invalid_json(reason: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "status": "invalid",
        "reasonCodes": [reason],
    }))
    .unwrap_or_default()
}

/// `lekalo privacy export`: the fail-closed export pipeline. The exit
/// contract mirrors the evaluator: 0 ready, 3 denied or leak-refused,
/// 1 malformed. The summary is metadata-only; the candidate payload
/// appears only under the explicit `payload` member.
fn run_privacy_export(
    artifact: &str,
    destination: &str,
    dry_run: bool,
    consent: Option<&str>,
    project: &Option<String>,
) -> u8 {
    let project_dir = match project_root_for(project) {
        Err(result) => return emit(result, false),
        Ok(dir) => dir,
    };
    let Some(spec) = lekalo_core::privacy::export::DestinationSpec::parse(destination) else {
        let _ = write_stderr(&cli_invalid_json("privacy.destination-unknown"));
        return OUTPUT_FAILURE;
    };
    let consent_json = match consent {
        Some(path) => match std::fs::read_to_string(path) {
            Err(_) => {
                let _ = write_stderr(&cli_invalid_json("privacy.consent-unreadable"));
                return OUTPUT_FAILURE;
            }
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Err(_) => {
                    let _ = write_stderr(&cli_invalid_json("privacy.consent-invalid"));
                    return OUTPUT_FAILURE;
                }
                Ok(record) => Some(record),
            },
        },
        None => None,
    };
    let artifact_path = std::path::Path::new(artifact);
    match lekalo_core::privacy::export::run_export(
        &project_dir,
        artifact_path,
        spec,
        consent_json.as_ref(),
        dry_run,
    ) {
        Ok(outcome) => {
            let summary = serde_json::json!({
                "status": "ready",
                "decision": serde_json::to_value(outcome.decision()).unwrap_or_default(),
                "artifactKind": outcome.artifact_kind(),
                "artifactRef": outcome.artifact_ref(),
                "destination": outcome.destination().as_str(),
                "payload": outcome.payload(),
                "payloadDigest": outcome.payload_digest(),
                "appliedTransforms": outcome.applied_transforms().iter().map(|t| t.as_str()).collect::<Vec<_>>(),
                "findings": outcome.findings(),
                "residuals": outcome.residuals(),
                "exportPath": outcome.export_path(),
                "decisionPath": outcome.decision_path(),
                "written": outcome.written(),
                "dryRun": dry_run,
            });
            let _ = write_stdout(&serde_json::to_string_pretty(&summary).unwrap_or_default());
            0
        }
        Err(lekalo_core::privacy::export::ExportFailure::Denied(output)) => {
            let _ = write_stdout(&serde_json::to_string_pretty(&output).unwrap_or_default());
            3
        }
        Err(lekalo_core::privacy::export::ExportFailure::ResidualLeaks { output, leaks }) => {
            let refusal = serde_json::json!({
                "status": "refused-leaks",
                "decision": serde_json::to_value(&output).unwrap_or_default(),
                "reasonCodes": leaks.iter().map(|leak| format!("leak.{}", leak.kind().as_str())).collect::<Vec<String>>(),
                "residualLeaks": leaks,
            });
            let _ = write_stdout(&serde_json::to_string_pretty(&refusal).unwrap_or_default());
            3
        }
        Err(lekalo_core::privacy::export::ExportFailure::Malformed(code)) => {
            let _ = write_stderr(&cli_invalid_json(code));
            OUTPUT_FAILURE
        }
    }
}

/// `lekalo privacy redact`: the read-only redaction diff contract.
fn run_privacy_redact(payload_path: &str, repository: Option<&str>, terms: &[String]) -> u8 {
    let text = match std::fs::read_to_string(payload_path) {
        Err(_) => {
            let _ = write_stderr(&cli_invalid_json("privacy.payload-unreadable"));
            return OUTPUT_FAILURE;
        }
        Ok(text) => text,
    };
    use lekalo_core::privacy::vocab::TransformId;
    let transforms = [TransformId::RedactSecrets, TransformId::RedactPii];
    let owned_terms: Vec<String> = terms.to_vec();
    let subject = lekalo_core::privacy::redact::RedactionSubject {
        repository: repository.map(|name| {
            (
                name,
                lekalo_core::privacy::vocab::RepositoryRole::ConsumerRepository,
            )
        }),
        protected_terms: &owned_terms,
    };
    let (redacted, findings, residuals, applied) =
        lekalo_core::privacy::export::redact_preview(&text, &transforms, &[], subject);
    let report = serde_json::json!({
        "status": "ready",
        "appliedTransforms": applied.iter().map(|transform| transform.as_str()).collect::<Vec<_>>(),
        "diff": findings,
        "residuals": residuals,
        "redacted": redacted,
    });
    let _ = write_stdout(&serde_json::to_string_pretty(&report).unwrap_or_default());
    0
}

fn run_dataflow(command: DataflowCommands) -> DomainResult {
    match command {
        DataflowCommands::Report {
            attachment,
            policy,
            project,
            as_of,
            endpoints,
        } => {
            let as_of = match resolve_as_of(as_of) {
                Err(result) => return result,
                Ok(resolved) => resolved,
            };
            let (model_json, compilation) = match load_compiled_for(&project) {
                Err(result) => return result,
                Ok(pair) => pair,
            };
            let (attachment, policy, resolution) =
                match parse_classification_pair(&attachment, &policy, &compilation, &model_json) {
                    Err(result) => return result,
                    Ok(parts) => parts,
                };
            let exposures = match parse_endpoint_exposures(&endpoints, &compilation) {
                Err(result) => return result,
                Ok(exposures) => exposures,
            };
            match lekalo_core::dataflow::run_report(
                &compilation,
                &model_json,
                &attachment,
                &policy,
                &resolution,
                &exposures,
                &as_of,
            ) {
                Err(set) => DomainResult::invalid(set),
                Ok((report, diagnostics)) => {
                    let bytes = match lekalo_core::dataflow::report_canonical_bytes(&report) {
                        Err(set) => return DomainResult::invalid(set),
                        Ok(bytes) => bytes,
                    };
                    let denied = report.verdict().as_str() == "denied";
                    // The embedded envelope states the real verdict: a
                    // denial never prints "valid" at the top level.
                    let json = format!(
                        "{{\"status\":\"{}\",\"report\":{bytes}}}",
                        report.verdict().as_str()
                    );
                    let human = format!(
                        "dataflow {}: {} flow(s), {} finding(s), {} unknown(s)",
                        report.verdict().as_str(),
                        report.flows().len(),
                        report.findings().len(),
                        report.unknowns().len()
                    );
                    if denied {
                        // Denied, but never evidence-free: the denied
                        // envelope carries the report JSON alongside the
                        // mirrored findings so the user sees the rows
                        // that produced the verdict (F-11).
                        DomainResult::denied_json(json, human, diagnostics)
                    } else {
                        DomainResult::graph(json, human, Vec::new())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod adapter_update_tests {
    use super::select_forward_update;
    use lekalo_core::adapter_package::{InventoryRow, PackageFailure};

    fn row(id: &str, version: &str, selected: bool) -> InventoryRow {
        InventoryRow {
            id: id.to_owned(),
            version: version.to_owned(),
            digest: format!("sha256:{}", "11".repeat(32)),
            manifest_digest: format!("sha256:{}", "22".repeat(32)),
            trust: "local-development".to_owned(),
            source: "path:x".to_owned(),
            install_plan_id: None,
            selected,
            quarantined: false,
        }
    }

    /// Regression (fix round 2, cline F-4 / devin F-7, pinned here at
    /// unit level per fix round 4, cline F-NEW-4): two-digit components
    /// order by SemVer precedence, never string comparison.
    #[test]
    fn forward_selection_orders_two_digit_components_by_semver() {
        // Selected 0.3.9, installed {0.3.10, 0.3.2}: the honest forward
        // step is 0.3.10 even though "0.3.10" < "0.3.2" lexically.
        let rows = vec![row("a", "0.3.10", false), row("a", "0.3.2", false)];
        let selected = row("a", "0.3.9", true);
        let target = select_forward_update(&rows, Some(&selected))
            .expect("rows parse")
            .expect("a forward update exists");
        assert_eq!(target.version, "0.3.10");

        // The no-downgrade guard: from 0.3.10, neither older row applies.
        let selected = row("a", "0.3.10", true);
        assert!(
            select_forward_update(&rows, Some(&selected))
                .expect("rows parse")
                .is_none(),
            "no older version may be selected as a forward update"
        );

        // Nothing selected: the newest row wins.
        let target = select_forward_update(&rows, None)
            .expect("rows parse")
            .expect("the newest row applies");
        assert_eq!(target.version, "0.3.10");
    }

    /// Regression (fix round 4, devin N-3): a corrupt inventory row
    /// returns the inventory-corruption diagnostic instead of panicking
    /// a SemVer comparator.
    #[test]
    fn a_corrupt_row_refuses_with_a_diagnostic() {
        let rows = vec![row("a", "not-a-version", false)];
        let error = select_forward_update(&rows, None).expect_err("corrupt row refuses");
        assert_eq!(
            error,
            PackageFailure::RecoveryRequired {
                stage: "inventory".to_owned()
            }
        );
        // A corrupt selected pin refuses too.
        let rows = vec![row("a", "1.0.0", false)];
        let selected = row("a", "0.3", true);
        assert!(select_forward_update(&rows, Some(&selected)).is_err());
    }
}

#[cfg(test)]
mod quarantine_custody_tests {
    use super::*;

    /// Regression (fix round 4, cline F-NEW-3): purge removes the
    /// emptied per-id parent directories along with the custody
    /// directories — no residue shells under quarantine/** or
    /// packages/**, and sibling versions survive.
    #[test]
    fn purge_cleans_the_emptied_parent_directories() {
        let root = std::env::temp_dir().join(format!("lekalo-cli-purge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lekalo")).expect("lekalo dir");
        std::fs::write(root.join("lekalo/project.yaml"), "project: purge-test\n")
            .expect("project marker");
        // Two quarantined versions of one id, plus a promoted version of
        // another id that must survive.
        for dir in [
            ".lekalo/adapters/quarantine/a/1.0.0-11111111",
            ".lekalo/adapters/quarantine/a/2.0.0-22222222",
            ".lekalo/adapters/packages/b/1.0.0-33333333",
        ] {
            std::fs::create_dir_all(root.join(dir.replace('/', std::path::MAIN_SEPARATOR_STR)))
                .expect("custody dir");
        }
        std::fs::write(
            root.join(".lekalo/adapters/quarantine/a/1.0.0-11111111/adapter.mjs"),
            b"bytes",
        )
        .expect("bytes");
        let inventory = serde_json::json!({
            "schemaVersion": lekalo_core::adapter_package::version::INVENTORY_SCHEMA_VERSION,
            "identity": lekalo_core::adapter_package::version::INVENTORY_IDENTITY,
            "packages": [
                { "id": "a", "version": "1.0.0",
                  "digest": format!("sha256:{}", "11".repeat(32)),
                  "manifestDigest": format!("sha256:{}", "11".repeat(32)),
                  "trust": "community", "source": "release:ch/a",
                  "selected": false, "quarantined": true },
                { "id": "a", "version": "2.0.0",
                  "digest": format!("sha256:{}", "22".repeat(32)),
                  "manifestDigest": format!("sha256:{}", "22".repeat(32)),
                  "trust": "community", "source": "release:ch/a",
                  "selected": false, "quarantined": true },
                { "id": "b", "version": "1.0.0",
                  "digest": format!("sha256:{}", "33".repeat(32)),
                  "manifestDigest": format!("sha256:{}", "33".repeat(32)),
                  "trust": "local-development", "source": "path:x",
                  "selected": true, "quarantined": false }
            ]
        });
        std::fs::write(
            root.join(".lekalo/adapters/inventory.json"),
            serde_json::to_vec_pretty(&inventory).expect("inventory serializes"),
        )
        .expect("inventory written");

        let purged = quarantine_purge_all(&root).expect("the fixture purges");
        assert_eq!(purged, 2, "both quarantined rows purge");
        // Both custody trees are gone — including the emptied a/ shell;
        // the custody roots themselves legally remain.
        assert!(!root.join(".lekalo/adapters/quarantine/a").exists());
        assert!(!root
            .join(".lekalo/adapters/quarantine/a/1.0.0-11111111")
            .exists());
        // The untouched promoted package survives.
        assert!(root
            .join(".lekalo/adapters/packages/b/1.0.0-33333333")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod gate_label_tests {
    use super::gate_label_of;
    use lekalo_core::adapter_package::PackageFailure;

    /// Regression (fix round 4, cline F-NEW-4): the discover receipt's
    /// gate label follows the resolution gate order — a refusal names the
    /// gate that refused, never a generic integrity verdict.
    #[test]
    fn the_gate_label_matches_the_refusing_gate() {
        assert_eq!(
            gate_label_of(&PackageFailure::ManifestInvalid {
                reason: "grammar".to_owned()
            }),
            "manifest"
        );
        assert_eq!(
            gate_label_of(&PackageFailure::Incompatible {
                adapter: "a".to_owned()
            }),
            "compatibility"
        );
        assert_eq!(
            gate_label_of(&PackageFailure::ChecksumMismatch {
                domain: "package".to_owned(),
                identity: "a".to_owned()
            }),
            "integrity"
        );
        assert_eq!(
            gate_label_of(&PackageFailure::SignatureUnverified {
                scheme: "minisign".to_owned()
            }),
            "signature"
        );
        for failure in [
            PackageFailure::Revoked {
                id: "a".to_owned(),
                version: "1.0.0".to_owned(),
            },
            PackageFailure::Quarantined {
                id: "a".to_owned(),
                version: "1.0.0".to_owned(),
            },
            PackageFailure::TrustInsufficient {
                id: "a".to_owned(),
                level: "community".to_owned(),
            },
        ] {
            assert_eq!(gate_label_of(&failure), "trust");
        }
        // Unknown-failure fallback stays fail-closed on the integrity gate.
        assert_eq!(
            gate_label_of(&PackageFailure::InstallPlanRequired),
            "integrity"
        );
    }
}
