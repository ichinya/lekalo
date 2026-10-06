//! The `lekalo managed` surface (issue #41): explicit ownership admission,
//! deterministic plan previews, confirmed applies, upgrade diffs, exact
//! rollback/resolve, and equivalence adjudication. Every decision lives in
//! the core; this module selects, renders, and maps exits.

use clap::{Args, Subcommand};
use lekalo_core::managed as core;
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
    /// Adjudicate one neutral behavioral-equivalence evidence document.
    Equivalence {
        /// The closed equivalence evidence document.
        #[arg(long, value_name = "FILE")]
        evidence: String,
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

/// The admission receipt of one managed module/target assignment.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentView {
    pub module: String,
    pub target: String,
    pub mode: String,
    pub generated_root: String,
    pub extensions: usize,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReceipt {
    pub project: String,
    pub assignments: Vec<AssignmentView>,
    pub admitted: Vec<String>,
    pub coverage_digest: Option<String>,
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
        core::coverage::admit(assignment, target, &coverage)?;
        admitted.push(format!("{}/{}", coverage.module, coverage.target));
        coverage_digest = Some(core::hash(&coverage));
    }
    Ok(core::render(&CheckReceipt {
        project: ownership.project,
        assignments,
        admitted,
        coverage_digest,
    }))
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReceipt {
    pub action: String,
    pub module: String,
    pub target: String,
    pub plan: core::GenerationPlan,
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
        if let Ok(bytes) = std::fs::read(project_root.join(path)) {
            before.actual.insert(path.clone(), core::digest(&bytes));
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
    Err(core::apply::apply(
        &root(&args.project.project),
        &plan,
        &coverage,
        &ownership,
        args.confirm.as_deref(),
        args.dry_run,
    ))
}

/// One exact pin difference between two coverage documents.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeChange {
    pub field: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeReceipt {
    pub module: String,
    pub target: String,
    pub comparable: bool,
    pub changes: Vec<UpgradeChange>,
    pub requirements_added: Vec<String>,
    pub requirements_removed: Vec<String>,
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
    let mut changes = Vec::new();
    if base.recipe.digest != candidate.recipe.digest {
        changes.push(UpgradeChange {
            field: "recipe.digest".to_owned(),
            before: base.recipe.digest.clone(),
            after: candidate.recipe.digest.clone(),
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
    let base_ids: Vec<&str> = base.requirements.iter().map(|r| r.id.as_str()).collect();
    let candidate_ids: Vec<&str> = candidate
        .requirements
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    let requirements_added: Vec<String> = candidate_ids
        .iter()
        .filter(|id| !base_ids.contains(id))
        .map(|id| (*id).to_owned())
        .collect();
    let requirements_removed: Vec<String> = base_ids
        .iter()
        .filter(|id| !candidate_ids.contains(id))
        .map(|id| (*id).to_owned())
        .collect();
    let comparable = true;
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
    let bytes = serde_json::to_vec(&plan).expect("typed rollback plan");
    Err(core::rollback::apply(
        &project_root,
        &bytes,
        args.confirm.as_deref(),
        false,
    ))
}

fn run_resolve(args: ResolveArgs) -> Outcome {
    if !args.restore {
        return Err(DomainResult::usage_error());
    }
    let project_root = root(&args.project.project);
    if args.dry_run {
        return Err(core::rollback::resolve(
            &project_root,
            &args.path,
            None,
            true,
        ));
    }
    // The confirmed resolve consumes the exact previewed plan identity.
    let preview = core::rollback::resolve(&project_root, &args.path, None, true);
    let json = preview.to_json_string();
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|_| invalid("receipt"))?;
    let digest = value["managed"]["planDigest"]
        .as_str()
        .ok_or_else(|| invalid("receipt"))?
        .to_owned();
    Err(core::rollback::resolve(
        &project_root,
        &args.path,
        Some(digest.as_str()),
        false,
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
        ManagedCommands::Equivalence { evidence } => {
            Err(core::equivalence::adjudicate(&read(&evidence)?))
        }
    }
}
