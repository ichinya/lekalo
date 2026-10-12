//! The `lekalo managed` surface (issue #41): explicit ownership admission,
//! deterministic plan previews, confirmed applies, upgrade diffs, exact
//! rollback/resolve, and equivalence adjudication. Every decision lives in
//! the core; this module selects, renders, and maps exits.

use clap::{Args, Subcommand};
use lekalo_core::managed as core;
use lekalo_core::managed::receipts::{
    AssignmentView, AuthorityReceipt, CheckReceipt, PlanReceipt, UpgradeChange, UpgradeReceipt,
};
use lekalo_core::DomainResult;
use std::io::Read;

/// The project root selector shared by every subcommand.
#[derive(Debug, Args)]
pub struct ProjectArgs {
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// The caller-owned ownership configuration (never recovered from
    /// target source).
    #[arg(long, value_name = "FILE")]
    pub ownership: String,
    /// One pinned adapter coverage document to admit (repeatable scope via
    /// `--module`/`--target`).
    #[arg(long, value_name = "FILE")]
    pub coverage: Option<String>,
    /// Restrict the inspection to one exact module id.
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,
    /// Restrict the inspection to one exact target id.
    #[arg(long, value_name = "TARGET")]
    pub target: Option<String>,
    #[command(flatten)]
    pub project: ProjectArgs,
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    /// The managed lane selector; only `managed` is negotiable here.
    #[arg(long, value_name = "MODE")]
    pub to: String,
    #[arg(long, value_name = "FILE")]
    pub ownership: String,
    /// The pinned adapter coverage evidence for the selected subject.
    #[arg(long, value_name = "FILE")]
    pub coverage: String,
    /// The exact module id (attribution and scope, not isolation).
    #[arg(long, value_name = "MODULE")]
    pub module: String,
    /// The exact target id.
    #[arg(long, value_name = "TARGET")]
    pub target: String,
    /// Additional pinned generation inputs (repeatable).
    #[arg(long, value_name = "FILE")]
    pub input: Vec<String>,
    #[command(flatten)]
    pub project: ProjectArgs,
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// The caller-saved closed plan (immutable reviewed JSON).
    #[arg(long, value_name = "FILE")]
    pub plan: String,
    /// The coverage evidence pinned by the plan's `coverageDigest`.
    #[arg(long, value_name = "FILE")]
    pub coverage: String,
    /// The ownership configuration rechecked before publication.
    #[arg(long, value_name = "FILE")]
    pub ownership: String,
    /// The exact `sha256:<plan-digest>` confirmation.
    #[arg(long, value_name = "DIGEST")]
    pub confirm: Option<String>,
    /// Preview the transaction without writing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// The execution evidence of a bound generation (required to commit a
    /// plan with validated extensions): the pinned generated-test digests
    /// with their observed outcomes.
    #[arg(long, value_name = "FILE")]
    pub execution: Option<String>,
    #[command(flatten)]
    pub project: ProjectArgs,
}

#[derive(Debug, Args)]
pub struct UpgradePlanArgs {
    /// The exact module id.
    #[arg(long, value_name = "MODULE")]
    pub module: String,
    /// The exact target id.
    #[arg(long, value_name = "TARGET")]
    pub target: String,
    /// The caller-owned ownership configuration both documents admit
    /// against.
    #[arg(long, value_name = "FILE")]
    pub ownership: String,
    /// The current adapter coverage evidence.
    #[arg(long, value_name = "FILE")]
    pub base: String,
    /// The candidate adapter coverage evidence.
    #[arg(long, value_name = "FILE")]
    pub candidate: String,
    #[command(flatten)]
    pub project: ProjectArgs,
}

#[derive(Debug, Args)]
pub struct RollbackArgs {
    /// The retained destination generation digest.
    #[arg(long, value_name = "DIGEST")]
    pub to: String,
    /// Preview the restoration without writing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// The exact `sha256:<rollback-plan-digest>` confirmation.
    #[arg(long, value_name = "DIGEST")]
    pub confirm: Option<String>,
    #[command(flatten)]
    pub project: ProjectArgs,
}

#[derive(Debug, Args)]
pub struct ResolveArgs {
    /// The drifted generated path to restore.
    #[arg(long, value_name = "PATH")]
    pub path: String,
    /// Restore the exact recorded bytes (the only resolution).
    #[arg(long)]
    pub restore: bool,
    /// Preview the restoration without writing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// The exact `sha256:<resolve-plan-digest>` confirmation.
    #[arg(long, value_name = "DIGEST")]
    pub confirm: Option<String>,
    #[command(flatten)]
    pub project: ProjectArgs,
}

#[derive(Debug, Subcommand)]
pub enum ManagedCommands {
    /// Inspect ownership and semantic coverage without any write.
    Check(CheckArgs),
    /// Preview a reviewed generation plan; the result rides stdout only.
    Plan(PlanArgs),
    /// Consume a caller-saved plan with its exact digest confirmation.
    Apply(ApplyArgs),
    /// Diff the exact adapter/coverage pins of a candidate upgrade.
    UpgradePlan(UpgradePlanArgs),
    /// Preview or confirm restoration of a previous generation.
    Rollback(RollbackArgs),
    /// Restore one drifted generated path from recorded bytes.
    Resolve(ResolveArgs),
    /// Reconcile interrupted publications: complete or undo every
    /// unresolved journal and clear a stale mutation lock.
    Recover {
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// Print the exact generation authority of the project: the canonical
    /// Model/IR/lock pins and the module's required ledger. Read-only.
    Authority {
        /// The exact module id whose ledger is reported.
        #[arg(long, value_name = "MODULE")]
        module: String,
        #[command(flatten)]
        project: ProjectArgs,
    },
    /// Adjudicate one neutral behavioral-equivalence evidence document.
    Equivalence {
        /// The closed equivalence evidence document.
        #[arg(long, value_name = "FILE")]
        evidence: String,
        #[command(flatten)]
        project: ProjectArgs,
    },
}

type Outcome = std::result::Result<DomainResult, DomainResult>;

fn invalid(reason: &str) -> DomainResult {
    core::failure(core::INPUT_INVALID, &core::detail(reason))
}

fn read(path: &str) -> std::result::Result<Vec<u8>, DomainResult> {
    let file = std::fs::File::open(path).map_err(|_| invalid("input-read"))?;
    if !file
        .metadata()
        .map_err(|_| invalid("input-metadata"))?
        .is_file()
    {
        return Err(invalid("input-file"));
    }
    let mut bytes = vec![];
    file.take(core::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("input-read"))?;
    if bytes.len() > core::MAX_BYTES {
        return Err(invalid("input-bound"));
    }
    Ok(bytes)
}

/// The project root: `--project DIR` relative to the invocation directory.
fn root(project: &Option<String>) -> std::path::PathBuf {
    match project {
        Some(directory) => std::path::PathBuf::from(directory),
        None => std::path::PathBuf::from("."),
    }
}

/// The loader selection of one invocation: the same `--project` selector
/// the accepted pipeline uses for root discovery. The selection grammar
/// admits only relative selectors, so an absolute or backslash spelling
/// normalizes to its cwd-relative form first.
fn selection(project: &Option<String>) -> lekalo_core::loader::LoadSelection {
    let Some(raw) = project else {
        return lekalo_core::loader::LoadSelection { project: None };
    };
    let path = std::path::PathBuf::from(raw);
    let candidate = if path.is_absolute() {
        let cwd = std::env::current_dir().unwrap_or_default();
        let (Some(cwd), Ok(absolute)) = (cwd.canonicalize().ok(), path.canonicalize()) else {
            return lekalo_core::loader::LoadSelection { project: None };
        };
        absolute
            .strip_prefix(&cwd)
            .map(|p| p.to_path_buf())
            .unwrap_or(absolute)
    } else {
        path
    };
    let mut selector = candidate.to_string_lossy().replace('\\', "/");
    if selector.is_empty() {
        selector = ".".to_owned();
    }
    lekalo_core::loader::LoadSelection {
        project: Some(selector),
    }
}

fn run_check(args: CheckArgs) -> Outcome {
    let ownership = core::parse_ownership(&read(&args.ownership)?)?;
    let mut assignments = Vec::new();
    for module in &ownership.modules {
        if let Some(selected) = &args.module {
            if &module.module != selected {
                continue;
            }
        }
        for target in &module.targets {
            if let Some(selected) = &args.target {
                if &target.target != selected {
                    continue;
                }
            }
            assignments.push(AssignmentView {
                module: module.module.clone(),
                target: target.target.clone(),
                mode: match module.mode {
                    core::Mode::Observed => "observed".to_owned(),
                    core::Mode::Contracted => "contracted".to_owned(),
                    core::Mode::Managed => "managed".to_owned(),
                },
                generated_root: target.generated_root.clone(),
                extensions: target.extensions.len(),
            });
        }
    }
    let mut admitted = Vec::new();
    let mut coverage_digest = None;
    if let Some(coverage_path) = &args.coverage {
        let coverage_bytes = read(coverage_path)?;
        let coverage = core::parse_coverage(&coverage_bytes)?;
        let assignment = ownership
            .modules
            .iter()
            .find(|module| module.module == coverage.module)
            .ok_or_else(|| invalid("module"))?;
        let target = assignment
            .targets
            .iter()
            .find(|target| target.target == coverage.target)
            .ok_or_else(|| invalid("target"))?;
        // The generation authority is derived from the exact project: the
        // coverage evidence must pin it and discharge every required
        // obligation before it admits (finding D).
        let authority =
            core::authority::derive(&selection(&args.project.project), &coverage.module)?;
        core::authority::qualify(&root(&args.project.project), &coverage, &authority)?;
        core::authority::ledger_complete(&coverage, &authority)?;
        core::coverage::admit(assignment, target, &coverage)?;
        // Ownership provenance resolves against the locked components too
        // (r4 finding 6): forged `profileRef`/`implementationRef` pins are
        // refused at inspection time, not only at apply.
        core::authority::resolve_target_provenance(target, &authority)?;
        // Every validated custom/foreign binding resolves to a real pinned
        // implementation file (finding K).
        core::bindings::resolve(&root(&args.project.project), target, &coverage)?;
        admitted.push(format!("{}/{}", coverage.module, coverage.target));
        coverage_digest = Some(core::hash(&coverage));
    }
    // Custody drift observation (finding R): with a committed managed
    // generation, every recorded output must still carry its exact
    // recorded bytes; any drift is reported read-only.
    let mut drifted: Vec<String> = Vec::new();
    if core::history::exists(&root(&args.project.project)) {
        let current = match core::history::current(&root(&args.project.project)) {
            Ok(Some(current)) => current,
            Ok(None) => String::new(),
            Err(error) => return Err(error),
        };
        if !current.is_empty() {
            let record = core::history::record(&root(&args.project.project), &current)?;
            for output in &record.outputs {
                match core::fs::read_file(&root(&args.project.project), &output.path) {
                    Ok(Some(bytes)) => {
                        if core::digest(&bytes) != output.digest {
                            drifted.push(output.path.clone());
                        }
                    }
                    Ok(None) => drifted.push(output.path.clone()),
                    Err(_) => drifted.push(output.path.clone()),
                }
            }
        }
    }
    if !drifted.is_empty() {
        return Err(core::diagnostic::failure(
            core::GENERATED_DRIFT,
            &[("path", drifted[0].as_str()), ("reason", "custody-drift")],
        ));
    }
    Ok(core::render(&CheckReceipt {
        project: ownership.project,
        assignments,
        admitted,
        coverage_digest,
    }))
}

fn run_plan(args: PlanArgs) -> Outcome {
    if args.to != "managed" {
        return Err(invalid("to"));
    }
    let ownership = core::parse_ownership(&read(&args.ownership)?)?;
    let coverage_bytes = read(&args.coverage)?;
    let coverage = core::parse_coverage(&coverage_bytes)?;
    let assignment = ownership
        .modules
        .iter()
        .find(|module| module.module == args.module)
        .ok_or_else(|| invalid("module"))?;
    let target = assignment
        .targets
        .iter()
        .find(|target| target.target == args.target)
        .ok_or_else(|| invalid("target"))?;
    let admission = core::coverage::admit(assignment, target, &coverage)?;
    // The generation authority: the exact canonical Model/IR/lock the plan
    // is derived under, qualified against the coverage evidence; every
    // required obligation must be discharged before any plan exists
    // (finding D).
    let authority = core::authority::derive(&selection(&args.project.project), &args.module)?;
    core::authority::qualify(&root(&args.project.project), &coverage, &authority)?;
    core::authority::ledger_complete(&coverage, &authority)?;
    // Ownership provenance resolves against the locked components too
    // (r4 finding 6): forged `profileRef`/`implementationRef` pins never
    // produce a plan.
    core::authority::resolve_target_provenance(target, &authority)?;
    // Every validated custom/foreign binding resolves to a real pinned
    // implementation file before any plan exists (finding K).
    core::bindings::resolve(&root(&args.project.project), target, &coverage)?;
    // The complete pinned input closure: every explicit input plus the
    // ownership configuration itself.
    let project_root = root(&args.project.project);
    let mut closure = Vec::new();
    let mut inputs: Vec<(String, String)> = Vec::new();
    inputs.push((
        args.ownership.replace('\\', "/"),
        core::digest(&read(&args.ownership)?),
    ));
    for input in &args.input {
        inputs.push((input.replace('\\', "/"), core::digest(&read(input)?)));
    }
    inputs.sort();
    inputs.dedup();
    for (path, digest) in inputs {
        closure.push(core::InputPin { path, digest });
    }
    // The before-state comes from recorded custody, never from guesses.
    let recorded: std::collections::BTreeMap<String, core::RecordOutput> =
        match core::history::current(&project_root) {
            Ok(Some(current)) => core::history::record(&project_root, &current)?
                .outputs
                .into_iter()
                .map(|output| (output.path.clone(), output))
                .collect(),
            Ok(None) => std::collections::BTreeMap::new(),
            Err(error) => return Err(error),
        };
    let mut before = core::plan::BeforeState {
        recorded: std::collections::BTreeMap::new(),
        actual: std::collections::BTreeMap::new(),
    };
    for (path, output) in &recorded {
        before.recorded.insert(
            path.clone(),
            core::plan::Recorded {
                digest: output.digest.clone(),
                role: output.role,
                owner_module: output.owner_module.clone(),
                owner_target: output.owner_target.clone(),
                object_ref: output.after_ref.value().cloned(),
            },
        );
        match core::fs::read_file(&project_root, path) {
            Ok(Some(bytes)) => {
                before.actual.insert(path.clone(), core::digest(&bytes));
            }
            // An unreadable recorded path (a link or reparse alias in the
            // ancestor chain) refuses the plan instead of looking absent.
            Ok(None) => {}
            Err(_) => {
                return Err(core::diagnostic::failure(
                    core::GENERATED_DRIFT,
                    &[("path", path.as_str()), ("reason", "unreadable")],
                ))
            }
        }
    }
    // Unclaimed files at a would-be create path are never invisible at
    // planning time either. The pre-scan renders with the Model-derived
    // semantics (r6, codex F9): the interface types are Model-owned.
    if let Ok(inventory) =
        core::canonical::render_inventory(&coverage, &target.generated_root, &authority.semantics)
    {
        for artifact in &inventory {
            if before.actual.contains_key(&artifact.path) {
                continue;
            }
            match core::fs::read_file(&project_root, &artifact.path) {
                Ok(Some(bytes)) => {
                    before
                        .actual
                        .insert(artifact.path.clone(), core::digest(&bytes));
                }
                // An unclaimed coordinate behind a link or reparse alias
                // refuses the plan as a create conflict.
                Ok(None) => {}
                Err(_) => {
                    return Err(core::diagnostic::failure(
                        core::OWNERSHIP_DENIED,
                        &[
                            ("path", artifact.path.as_str()),
                            ("reason", "create-conflict"),
                        ],
                    ))
                }
            }
        }
    }
    let previous = core::history::current(&project_root)?;
    let action = if previous.is_some() {
        core::PlanAction::Regeneration
    } else {
        core::PlanAction::Promotion
    };
    let plan = core::plan::build(
        action,
        &admission,
        &coverage,
        closure,
        &before,
        previous,
        None,
        core::hash(&coverage),
        &authority.semantics,
    )?;
    let action = match plan.action {
        core::PlanAction::Promotion => "promotion",
        core::PlanAction::Regeneration => "regeneration",
        core::PlanAction::Upgrade => "upgrade",
    };
    Ok(core::render(&PlanReceipt {
        action: action.to_owned(),
        module: args.module,
        target: args.target,
        plan,
    }))
}

fn run_apply(args: ApplyArgs) -> Outcome {
    let plan = read(&args.plan)?;
    let coverage = read(&args.coverage)?;
    let ownership = core::parse_ownership(&read(&args.ownership)?)?;
    // The generation authority is re-derived and requalified at apply
    // time: a plan whose Model/IR/lock moved since the review is stale
    // (finding D).
    let parsed_plan = core::verify_plan(&plan)?;
    let coverage_doc = core::parse_coverage(&coverage)?;
    let authority =
        core::authority::derive(&selection(&args.project.project), &parsed_plan.modules[0])?;
    core::authority::qualify(&root(&args.project.project), &coverage_doc, &authority)?;
    core::authority::ledger_complete(&coverage_doc, &authority)?;
    // Every validated custom/foreign binding re-resolves at apply time.
    if let (Some(_module), Some(target)) = (
        ownership
            .modules
            .iter()
            .find(|m| m.module == parsed_plan.modules[0]),
        ownership
            .modules
            .iter()
            .find(|m| m.module == parsed_plan.modules[0])
            .and_then(|m| {
                m.targets
                    .iter()
                    .find(|t| t.target == parsed_plan.targets[0])
            }),
    ) {
        core::bindings::resolve(&root(&args.project.project), target, &coverage_doc)?;
    } else {
        return Err(invalid("module"));
    }
    // The execution evidence of a bound generation (r6 codex F4): the
    // behavioral gate a binding makes structurally incomplete offline —
    // supplied, verified, and retained by the core before the commit.
    let execution = match &args.execution {
        Some(path) => Some(read(path)?),
        None => None,
    };
    Err(core::apply::apply(
        &root(&args.project.project),
        &plan,
        &coverage,
        &ownership,
        args.confirm.as_deref(),
        args.dry_run,
        Some(&authority),
        execution.as_deref(),
    ))
}

fn run_upgrade_plan(args: UpgradePlanArgs) -> Outcome {
    let base = core::parse_coverage(&read(&args.base)?)?;
    let candidate = core::parse_coverage(&read(&args.candidate)?)?;
    if base.module != args.module || candidate.module != args.module {
        return Err(invalid("module"));
    }
    if base.target != args.target || candidate.target != args.target {
        return Err(invalid("target"));
    }
    // Both documents must admit against the exact ownership assignment:
    // an incomparable or unqualifiable candidate is reported, never an
    // empty safe-looking diff (finding N).
    let ownership = core::parse_ownership(&read(&args.ownership)?)?;
    let assignment = ownership
        .modules
        .iter()
        .find(|module| module.module == args.module)
        .ok_or_else(|| invalid("module"))?;
    let target = assignment
        .targets
        .iter()
        .find(|target| target.target == args.target)
        .ok_or_else(|| invalid("target"))?;
    let authority = core::authority::derive(&selection(&args.project.project), &args.module)?;
    let mut comparable = true;
    let mut changes = Vec::new();
    // The shared full admission pipeline qualifies both documents
    // (finding 13): authority pins, ledger completeness (both
    // directions), locked-adapter binding, binding resolution, and
    // semantic admission. A candidate the plan lane would refuse is
    // incomparable — comparable never means admissible.
    let live_root = root(&args.project.project);
    if let Err(error) = core::authority::qualify(&live_root, &base, &authority) {
        let _ = error;
        comparable = false;
    }
    if let Err(error) = core::authority::qualify(&live_root, &candidate, &authority) {
        let _ = error;
        comparable = false;
    }
    if core::authority::ledger_complete(&base, &authority).is_err()
        || core::authority::ledger_complete(&candidate, &authority).is_err()
    {
        comparable = false;
    }
    if core::bindings::resolve(&live_root, target, &base).is_err()
        || core::bindings::resolve(&live_root, target, &candidate).is_err()
    {
        comparable = false;
    }
    let _ = core::coverage::admit(assignment, target, &base).map_err(|_| comparable = false);
    let candidate_admission =
        core::coverage::admit(assignment, target, &candidate).map_err(|_| comparable = false);
    // The complete pinned change set: recipe, adapter, and every
    // requirement row (support, witness, projection, sample) plus the
    // extension rows.
    if base.recipe.id != candidate.recipe.id {
        changes.push(UpgradeChange {
            field: "recipe.id".to_owned(),
            before: base.recipe.id.clone(),
            after: candidate.recipe.id.clone(),
        });
    }
    if base.recipe.version != candidate.recipe.version {
        changes.push(UpgradeChange {
            field: "recipe.version".to_owned(),
            before: base.recipe.version.clone(),
            after: candidate.recipe.version.clone(),
        });
    }
    if base.recipe.digest != candidate.recipe.digest {
        changes.push(UpgradeChange {
            field: "recipe.digest".to_owned(),
            before: base.recipe.digest.clone(),
            after: candidate.recipe.digest.clone(),
        });
    }
    if base.adapter.entry != candidate.adapter.entry {
        changes.push(UpgradeChange {
            field: "adapter.entry".to_owned(),
            before: base.adapter.entry.clone(),
            after: candidate.adapter.entry.clone(),
        });
    }
    if base.adapter.package_digest != candidate.adapter.package_digest {
        changes.push(UpgradeChange {
            field: "adapter.packageDigest".to_owned(),
            before: base.adapter.package_digest.clone(),
            after: candidate.adapter.package_digest.clone(),
        });
    }
    if base.adapter.protocol != candidate.adapter.protocol {
        changes.push(UpgradeChange {
            field: "adapter.protocol".to_owned(),
            before: base.adapter.protocol.clone(),
            after: candidate.adapter.protocol.clone(),
        });
    }
    // In-place requirement diffs: every pinned row is compared by its
    // canonical serialization, so an in-place support/witness/projection/
    // sample change is a named change, never a silent keep.
    let mut candidate_rows: std::collections::BTreeMap<&str, &core::Requirement> = candidate
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect();
    for base_requirement in &base.requirements {
        let Some(candidate_requirement) = candidate_rows.remove(base_requirement.id.as_str())
        else {
            continue;
        };
        if core::canonical_bytes(base_requirement) != core::canonical_bytes(candidate_requirement) {
            changes.push(UpgradeChange {
                field: format!("requirement.{}", base_requirement.id),
                before: core::canonical_bytes(base_requirement),
                after: core::canonical_bytes(candidate_requirement),
            });
        }
    }
    let base_extensions: std::collections::BTreeMap<&str, &core::ExtensionCheck> = base
        .extensions
        .iter()
        .map(|check| (check.symbol.as_str(), check))
        .collect();
    for check in &candidate.extensions {
        let Some(base_check) = base_extensions.get(check.symbol.as_str()) else {
            continue;
        };
        if core::canonical_bytes(*base_check) != core::canonical_bytes(check) {
            changes.push(UpgradeChange {
                field: format!("extension.{}", check.symbol),
                before: core::canonical_bytes(*base_check),
                after: core::canonical_bytes(check),
            });
        }
    }
    let base_ids: Vec<&str> = base.requirements.iter().map(|r| r.id.as_str()).collect();
    let candidate_ids: Vec<&str> = candidate
        .requirements
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    let mut requirements_added: Vec<String> = candidate_ids
        .iter()
        .filter(|id| !base_ids.contains(id))
        .map(|id| (*id).to_owned().to_owned())
        .collect();
    requirements_added.sort();
    let requirements_removed: Vec<String> = base_ids
        .iter()
        .filter(|id| !candidate_ids.contains(id))
        .map(|id| (*id).to_owned().to_owned())
        .collect();
    // The candidate upgrade is only comparable when the candidate is a
    // real qualified successor of the admitted base.
    if candidate_admission.is_err() {
        comparable = false;
    }
    Ok(core::render(&UpgradeReceipt {
        module: args.module,
        target: args.target,
        comparable,
        changes,
        requirements_added,
        requirements_removed,
    }))
}

fn run_rollback(args: RollbackArgs) -> Outcome {
    let project_root = root(&args.project.project);
    if args.dry_run {
        let plan = core::rollback::build(&project_root, &args.to)?;
        return Ok(core::render(&plan));
    }
    // The confirmed apply consumes the exact saved plan bytes.
    let plan = core::rollback::build(&project_root, &args.to)?;
    // The live generation authority (finding 8): the destination is
    // compatible only while the project still derives the exact canonical
    // Model/IR/lock the destination generation was admitted under. The
    // derivation goes through the accepted loader pipeline and fails
    // closed on a broken or missing authority; the core then refuses a
    // destination whose derived custody pins diverge from it.
    let record = core::history::record(&project_root, plan.destination_generation_digest.as_str())?;
    let module = record
        .outputs
        .first()
        .map(|output| output.owner_module.clone())
        .ok_or_else(|| invalid("generation-record"))?;
    let authority = core::authority::derive(&selection(&args.project.project), &module)?;
    let bytes = serde_json::to_vec(&plan).expect("typed rollback plan");
    Err(core::rollback::apply(
        &project_root,
        &bytes,
        args.confirm.as_deref(),
        false,
        Some(&authority),
    ))
}

fn run_resolve(args: ResolveArgs) -> Outcome {
    if !args.restore {
        return Err(DomainResult::usage_error());
    }
    let project_root = root(&args.project.project);
    // The previewed plan identity is exactly what the caller confirms; the
    // confirmation digest covers the drifted before-state, so a file that
    // moved after the reviewed preview invalidates it (finding A).
    Err(core::rollback::resolve(
        &project_root,
        &args.path,
        args.confirm.as_deref(),
        args.dry_run,
    ))
}

/// Run one managed subcommand.
pub fn run(command: ManagedCommands) -> DomainResult {
    match execute(command) {
        Ok(result) | Err(result) => result,
    }
}

fn execute(command: ManagedCommands) -> Outcome {
    match command {
        ManagedCommands::Check(args) => run_check(args),
        ManagedCommands::Plan(args) => run_plan(args),
        ManagedCommands::Apply(args) => run_apply(args),
        ManagedCommands::UpgradePlan(args) => run_upgrade_plan(args),
        ManagedCommands::Rollback(args) => run_rollback(args),
        ManagedCommands::Resolve(args) => run_resolve(args),
        ManagedCommands::Recover { project } => {
            let selection = selection(&project.project);
            let project_root = root(&project.project);
            // Apply journals anchor only against a live derivation of the
            // exact generation authority (r6-r7, codex F1): the module id
            // comes from each journal's retained plan, and the derivation
            // runs the accepted loader pipeline against the live project.
            let derive = |module: &str| core::authority::derive(&selection, module);
            Err(core::recovery::run(&project_root, &derive))
        }
        ManagedCommands::Authority { module, project } => {
            let authority = core::authority::derive(&selection(&project.project), &module)?;
            Ok(core::render(&AuthorityReceipt {
                module,
                model: authority.model_digest,
                ir: authority.ir_digest,
                lock: authority.lock_digest,
                ledger: authority
                    .ledger
                    .iter()
                    .map(|(id, kind)| format!("{id}:{}", kind.as_str()))
                    .collect(),
            }))
        }
        ManagedCommands::Equivalence { evidence, project } => Err(
            core::equivalence::adjudicate_for(&root(&project.project), &read(&evidence)?),
        ),
    }
}
