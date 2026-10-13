//! Qualified selective verification (issue #136, increment I4): the
//! pinned specification is really executed — baseline in a pristine
//! disposable copy, candidate in a patched disposable copy — with
//! bounded, continuously drained output, bounded runtime with
//! process-tree teardown, a scrubbed child environment, per-check
//! independent expectations, and an explicit behavior oracle. A
//! specification or a test id never decodes as an execution receipt:
//! only this module's execution variant, produced by actual runs, carries
//! outcomes.
//!
//! Where the qualified tool is missing or a command cannot run, the
//! conclusion is honestly `unavailable` and no test is reported as
//! passed. Never a marker, never an environment backdoor.
//!
//! Pure core (X2/R3): this module never spawns a process, reads the
//! host environment, or resolves PATH. Execution happens exclusively
//! through an injected [`VerificationHost`] capability — an admitted
//! host boundary the core only defines and the trusted binary
//! supplies — so lekalo-core stays offline and spawn-free. The
//! original-project write boundary is enforced at the seams that
//! matter: the whole-tree state digest is pinned at plan time,
//! re-checked before any run, and re-checked after the runs, so a
//! verification child that writes the original project turns every
//! downstream boundary (verify's own honesty and apply's admission)
//! into a typed refusal. A specification that requests a confinement
//! dimension the executor cannot actually enforce (`no-network`) is
//! refused with a typed scope refusal, never silently run while the
//! claim is dropped.
//!
//! Trusted apply (F11/X1/R1): an execution document is a derived
//! record of one run, never install authority. `adoption apply`
//! re-executes the pinned specification itself through the injected
//! host capability and admits only its own fresh run's measured
//! outcome — binding the exact plan, patch, specification,
//! interpreter, and whole consumed input revision — so a hand-forged
//! execution document, however internally consistent, cannot
//! authorize a write.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::diagnostic;
use super::version;
use super::wire::{
    self, CheckOutcome, CheckResults, OracleVerdict, PatchDocument, RefactorPlan,
    VerificationDocument,
};
use super::Outcome;
use crate::result::DomainResult;

/// The qualified tool allow-list. A specification naming any other
/// program is refused before any process exists; the program must be a
/// bare name resolved through PATH, never a path with separators.
const QUALIFIED_PROGRAMS: [&str; 7] = ["node", "npm", "npx", "deno", "bun", "python", "python3"];

/// Directories never pinned by the whole-tree state digest and never
/// copied into a disposable candidate copy: VCS, build, and lekalo
/// runtime state.
const TREE_EXCLUDED: [&str; 3] = [".git", "target", ".lekalo"];

/// The closed execution-custody marker member: assembled only by this
/// module's real runs and required by the wire document's closed shape.
/// A document that decodes without it never carries a measured
/// conclusion.
pub(crate) const CUSTODY_MEMBER: &str = "custody";

/// The admitted host execution capability (X2/R3): the trusted binary
/// supplies the process boundary — PATH resolution, spawn, bounded
/// drained pipes, the scrubbed child environment, the deadline, and
/// process-tree teardown — while lekalo-core stays pure and offline.
/// The core defines the contract and enforces every policy decision
/// (the qualified allow-list, digest pins, bounds, the tree barrier)
/// around what the host reports; the host reports measurements, never
/// conclusions.
pub trait VerificationHost {
    /// Resolve one qualified bare program name to its executable path
    /// through the host's PATH. `None` means the tool is honestly
    /// unavailable; the core records it, never invents a run.
    fn resolve_program(&self, program: &str) -> Option<PathBuf>;
    /// Run one command line in the given working directory and return
    /// `(exit code, bounded stdout)`. An `Err` is a measured refusal
    /// detail (spawn/timeout class), never a synthesized outcome.
    fn run(
        &self,
        program: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<(Option<i32>, String), String>;
}

/// The deterministic whole-tree state digest (F3/R2): SHA-256 over the
/// sorted `path US sha256(content)` lines of every file under the
/// project root except VCS, build, and lekalo runtime homes, under the
/// same byte/file ceilings as the disposable copy. This pins the
/// complete consumed input closure — including files an argv-pinned
/// runner only reaches through its own loader (imports, configs) — so
/// drift anywhere in the project between plan and verify/apply, or an
/// original-tree write by a verification child, is a typed refusal.
pub(crate) fn tree_state_digest(root: &Path) -> Outcome<String> {
    let mut paths: Vec<String> = Vec::new();
    collect_tree(root, root, &mut paths, &mut 0usize, &mut 0usize)?;
    paths.sort();
    let mut payload = String::new();
    for path in &paths {
        let bytes = std::fs::read(root.join(path)).map_err(|_| {
            DomainResult::invalid(diagnostic::evidence_incomplete_set("tree", "unreadable"))
        })?;
        payload.push_str(path);
        payload.push('\u{1f}');
        payload.push_str(&crate::digest::sha256_hex(&bytes));
        payload.push('\n');
    }
    Ok(format!(
        "sha256:{}",
        crate::digest::sha256_hex(payload.as_bytes())
    ))
}

/// Recursively collect the project-relative file set under the shared
/// exclusion and ceiling rules.
fn collect_tree(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<String>,
    total_bytes: &mut usize,
    total_files: &mut usize,
) -> Outcome<()> {
    let entries = std::fs::read_dir(directory).map_err(|_| {
        DomainResult::invalid(diagnostic::evidence_incomplete_set("tree", "unreadable"))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if TREE_EXCLUDED.contains(&name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|_| {
            DomainResult::invalid(diagnostic::evidence_incomplete_set("tree", "unreadable"))
        })?;
        if metadata.is_dir() {
            collect_tree(root, &entry.path(), paths, total_bytes, total_files)?;
        } else if metadata.is_file() {
            *total_bytes += metadata.len() as usize;
            *total_files += 1;
            if *total_bytes > version::COPY_TOTAL_BYTES || *total_files > version::COPY_TOTAL_FILES
            {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "tree",
                    "over-limit",
                )));
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if version::is_rel_path(&relative) {
                paths.push(relative);
            }
        }
        // Reparse points, symlinks, and junctions are skipped on sight.
    }
    Ok(())
}

/// The private custody envelope of one real execution. It serializes
/// into the closed wire document under [`CUSTODY_MEMBER`]; it is never
/// accepted from an outside caller.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCustody {
    /// The domain-separated digest over the exact measured document
    /// without its custody member (the run's content address).
    pub run: String,
    /// The measured wall-clock duration of the whole verification, in
    /// milliseconds (bound 1..=60_000_000); a forged document has no
    /// measured duration to present.
    pub measured_ms: u64,
}

/// The consumed-input manifest of one run: the exact bytes of the
/// consumed tool argv script, keyed by path, plus the exact typed
/// absence of every consumed-but-absent path. Both sides travel in the
/// execution document's `baseline_source` tree line, so apply can
/// re-derive and compare them byte-for-byte (F3/X3).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumedInputs {
    /// The consumed script path and its exact content digest; empty
    /// means no script existed at argv[0].
    pub scripts: Vec<ScriptPin>,
    /// The consumed-but-absent paths, sorted, bounded.
    pub absent: Vec<String>,
}

/// One consumed script byte pin.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptPin {
    pub path: String,
    pub sha256: String,
}

impl ConsumedInputs {
    fn manifest(&self) -> String {
        let scripts = self
            .scripts
            .iter()
            .map(|script| format!("{}\u{1f}{}\n", script.path, script.sha256))
            .collect::<Vec<_>>()
            .join("");
        let absent = self
            .absent
            .iter()
            .map(|path| format!("!{path}\n"))
            .collect::<Vec<_>>()
            .join("");
        format!("{scripts}{absent}")
    }

    /// The baseline-source tree line over the consumed manifest only.
    fn tree_digest(&self) -> String {
        format!(
            "sha256:{}",
            crate::digest::sha256_hex(self.manifest().as_bytes())
        )
    }

    /// The manifest line count, bounded by the source-file ceiling.
    fn bounded(&self) -> bool {
        self.scripts.len() <= version::MAX_SOURCE_FILES
            && self.absent.len() <= version::MAX_SOURCE_FILES
            && self.scripts.iter().all(|script| {
                version::is_rel_path(&script.path) && version::is_sha256(&script.sha256)
            })
            && self.absent.iter().all(|path| version::is_rel_path(path))
    }
}

/// The consumed-input manifest of one disposable-copy run: every
/// relative-path entry of the tool argv — the runner script and every
/// test, oracle, or config path the specification passes it — is pinned
/// with its exact bytes (or typed absence) into the execution and
/// revalidated before install (F3/X3). The specification's argv is the
/// closed declaration of what the tool consumes; an import reached only
/// through the runner's own loader is the runner's business and is read
/// inside the disposable copy, never trusted as admission evidence from
/// outside. Anything a consumer needs bound must travel in argv.
fn consumed_manifest(root: &Path, tool: &super::wire::ToolRef) -> ConsumedInputs {
    let mut scripts = Vec::new();
    let mut absent = Vec::new();
    for arg in &tool.args {
        if version::is_rel_path(arg) {
            match std::fs::read(root.join(arg)) {
                Ok(bytes) => scripts.push(ScriptPin {
                    path: arg.clone(),
                    sha256: format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
                }),
                Err(_) => absent.push(arg.clone()),
            }
        }
    }
    scripts.sort_by(|left, right| left.path.cmp(&right.path));
    absent.sort();
    ConsumedInputs { scripts, absent }
}

/// Execute one specification against the current tree: baseline and
/// candidate runs happen in disposable copies — the copy is the child's
/// working directory — and no report or cache home is written. Runs
/// happen only through the injected [`VerificationHost`] capability
/// (X2/R3); a requested `no-network` confinement is refused as
/// unenforceable before any process exists. The whole-tree state digest
/// is compared against the plan before any run and re-checked after
/// the runs, so a drifted plan never executes and a child that writes
/// the original project is a typed refusal, never a laundered run.
pub fn execute(
    ctx: &super::analysis::AdoptionContext,
    host: &dyn VerificationHost,
    index: Option<&crate::observed::types::ObservedIndex>,
    plan: &RefactorPlan,
    spec_bytes: &[u8],
    patch: Option<&PatchDocument>,
) -> Outcome<VerificationDocument> {
    let spec = wire::verification_of_bytes(spec_bytes).map_err(|error| error.domain_result())?;
    if spec.variant != "specification" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "not-a-specification",
        )));
    }
    let spec_digest = {
        let value = serde_json::to_value(&spec).expect("spec serializes");
        wire::digest("adoption-verification", &value)
    };
    // Confinement honesty (X2): the executor enforces exactly what it
    // can — the disposable-copy working directory, bounded captured
    // output, the qualified allow-list, the scrubbed environment, and
    // the deadline. A requested dimension it cannot enforce at the
    // process level is a typed refusal, never a silently dropped claim.
    if spec.confinement.iter().any(|token| token == "no-network") {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            "confinement",
            "no-network-unenforceable",
        )));
    }
    if plan.verification.as_deref() != Some(spec_digest.as_str()) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "plan-mismatch",
        )));
    }
    // Pre-execution barrier (F3/X3/R2): the plan's complete pin set —
    // model, lock, observed index, adapter, every pinned source
    // (evidence ∪ patch paths ∪ the specification's argv files) — is
    // re-fingerprinted against the live tree before any process
    // exists, and the whole-tree state digest must still match the
    // plan's. A drifted plan never executes; the same staleness class
    // the apply path enforces, moved in front of the run.
    let tree_before = tree_state_digest(&ctx.root)?;
    {
        let current = super::analysis::compute_pins(ctx, index, &plan_input_paths(plan, &spec))?;
        let mut changed = super::transaction::changed_pins(&plan.pins, &current);
        if plan.tree.as_deref() != Some(tree_before.as_str()) {
            changed.push("project-tree".to_owned());
        }
        if !changed.is_empty() {
            return Err(DomainResult::denied(diagnostic::plan_stale_set(
                &plan.plan, &changed,
            )));
        }
    }
    let Some(tool) = &spec.tool else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "tool-missing",
        )));
    };
    if !QUALIFIED_PROGRAMS.contains(&tool.program.as_str())
        || tool.program.contains('/')
        || tool.program.contains('\\')
    {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            &tool.program,
            "tool-not-qualified",
        )));
    }
    // The pinned tool digest is enforced when the specification carries
    // one: the resolved program's bytes must hash to the pin, so a
    // swapped interpreter can never silently serve a pinned run.
    let Some(program) = host.resolve_program(&tool.program) else {
        // An unresolvable tool is honestly unavailable; the consumed
        // manifest of the (absent) argv script still travels so the
        // document records what the run would have measured.
        let consumed = consumed_manifest(&ctx.root, tool);
        let mut execution = shell(
            plan,
            &spec,
            spec_digest,
            Some(consumed.tree_digest()),
            None,
            None,
            None,
        );
        execution.conclusion = Some("unavailable".to_owned());
        return Ok(execution);
    };
    verify_tool_digest(&program, tool)?;
    // The consumed-input manifest of the tool argv script, taken from
    // the live tree before the runs (F3/X3): the exact runner bytes are
    // pinned into the execution record, so a trivialized runner can
    // never serve a run it did not measure.
    let consumed = consumed_manifest(&ctx.root, tool);
    if !consumed.bounded() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "consumed",
            "over-limit",
        )));
    }

    let started = Instant::now();
    // Baseline: a pristine disposable copy.
    let (baseline_copy, baseline_guard) = copy_tree(&ctx.root)?;
    let baseline = run_checks(host, &program, &tool.args, &spec.checks, &baseline_copy);

    // Candidate: a second disposable copy with the exact patch applied.
    let candidate_result = match patch {
        Some(document) => {
            let (candidate_copy, candidate_guard) = copy_tree(&ctx.root)?;
            let checks = match apply_patch(&candidate_copy, document) {
                Ok(()) => run_checks(host, &program, &tool.args, &spec.checks, &candidate_copy),
                Err(_) => CheckResults {
                    checks: spec
                        .checks
                        .iter()
                        .map(|check| CheckOutcome {
                            id: check.id.clone(),
                            status: "unavailable".to_owned(),
                            exit: None,
                            detail: "candidate-prepare-failed".to_owned(),
                        })
                        .collect(),
                },
            };
            // The disposable candidate copy never survives the run: the
            // guard removes the whole temp home, inner copy included.
            drop(candidate_guard);
            checks
        }
        None => CheckResults {
            checks: spec
                .checks
                .iter()
                .map(|check| CheckOutcome {
                    id: check.id.clone(),
                    status: "unavailable".to_owned(),
                    exit: None,
                    detail: "no-candidate".to_owned(),
                })
                .collect(),
        },
    };
    // Both disposable temp homes are removed on every exit path: the
    // baseline guard drops here, the candidate guard at its scope end.
    drop(baseline_guard);
    // The original-tree write boundary (X2/R3): the runs happened in
    // disposable copies, so the original project must be byte-identical
    // to the barrier image. A child that escaped its copy — an absolute
    // path write, a host-state mutation — refuses here; the run is
    // never laundered into a conclusion over a tree it changed.
    if tree_state_digest(&ctx.root)? != tree_before {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            "verification",
            "original-tree-write",
        )));
    }

    // The behavior oracle: the kind=behavior-oracle checks must pass on
    // both sides. A shallow happy-path test passing beside a broken
    // oracle is a failed verification, never a completion.
    let oracle_ids: Vec<String> = spec
        .checks
        .iter()
        .filter(|check| check.kind == "behavior-oracle")
        .map(|check| check.id.clone())
        .collect();
    let oracle_failed: Vec<String> = oracle_ids
        .iter()
        .filter(|id| {
            let baseline_check = baseline.checks.iter().find(|c| &c.id == *id);
            let candidate_check = candidate_result.checks.iter().find(|c| &c.id == *id);
            !matches!(
                (
                    baseline_check.map(|c| c.status.as_str()),
                    candidate_check.map(|c| c.status.as_str())
                ),
                (Some("passed"), Some("passed"))
            )
        })
        .cloned()
        .collect();
    let oracle = if oracle_ids.is_empty() {
        OracleVerdict {
            status: "unavailable".to_owned(),
            detail: "no-oracle-declared".to_owned(),
        }
    } else if oracle_failed.is_empty() {
        OracleVerdict {
            status: "passed".to_owned(),
            detail: "declared-obligations-held".to_owned(),
        }
    } else {
        OracleVerdict {
            status: "failed".to_owned(),
            detail: oracle_failed.join(","),
        }
    };
    let any_unavailable = baseline
        .checks
        .iter()
        .chain(candidate_result.checks.iter())
        .any(|check| check.status == "unavailable");
    let any_failed = baseline
        .checks
        .iter()
        .chain(candidate_result.checks.iter())
        .any(|check| check.status == "failed");
    let conclusion = if any_unavailable && !any_failed {
        "unavailable"
    } else if any_failed || oracle.status == "failed" {
        "failed"
    } else if oracle.status == "unavailable" {
        "unavailable"
    } else {
        "preserved"
    };
    let mut execution = shell(
        plan,
        &spec,
        spec_digest,
        Some(consumed.tree_digest()),
        Some(baseline),
        Some(candidate_result),
        patch.map(|document| {
            let value = serde_json::to_value(document).expect("patch serializes");
            wire::digest("adoption-patch", &wire::without_member(&value, "patch"))
        }),
    );
    execution.oracle = Some(oracle);
    execution.conclusion = Some(conclusion.to_owned());
    // Execution custody: the run binds the exact plan, candidate
    // revision, and consumed-input tree line into the envelope only
    // this module assembles for real runs.
    let measured_ms = (started.elapsed().as_millis() as u64).clamp(1, 60_000_000);
    execution.custody = Some(custody_of(&execution, measured_ms));
    Ok(execution)
}

/// The plan-time consumed set (the same set apply re-fingerprints):
/// read scope ∪ mutation scope ∪ the specification's rel-path argv
/// entries, so the pre-execution barrier and the apply barrier cannot
/// disagree structurally (F3/R2).
fn plan_input_paths(
    plan: &RefactorPlan,
    spec: &VerificationDocument,
) -> std::collections::BTreeSet<String> {
    let mut paths: std::collections::BTreeSet<String> = plan
        .read_scope
        .iter()
        .chain(plan.mutation_scope.iter())
        .cloned()
        .collect();
    if let Some(tool) = &spec.tool {
        for arg in &tool.args {
            if version::is_rel_path(arg) {
                paths.insert(arg.clone());
            }
        }
    }
    paths
}

/// The private custody envelope of one measured execution: the
/// domain-separated digest over the exact document without its custody
/// member, plus the measured whole-run duration.
fn custody_of(execution: &VerificationDocument, measured_ms: u64) -> ExecutionCustody {
    let value = serde_json::to_value(execution).expect("execution serializes");
    let run = wire::digest(
        "adoption-verification-run",
        &wire::without_member(&value, CUSTODY_MEMBER),
    );
    ExecutionCustody { run, measured_ms }
}

/// Verify the resolved program's bytes against the pinned tool digest.
/// An unpinned specification skips this check; a pinned specification
/// refuses a swapped interpreter as a denied scope refusal.
fn verify_tool_digest(program: &Path, tool: &super::wire::ToolRef) -> Outcome<()> {
    let Some(expected) = &tool.digest else {
        return Ok(());
    };
    let bytes = std::fs::read(program)
        .map_err(|_| DomainResult::invalid(diagnostic::input_invalid_set("tool", "unreadable")))?;
    let actual = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
    if &actual != expected {
        return Err(DomainResult::denied(diagnostic::scope_refused_set(
            &tool.program,
            "tool-digest-mismatch",
        )));
    }
    Ok(())
}

/// Assemble the execution envelope around the measured outcomes. The
/// `disposable-copy` confinement token binds the exact candidate
/// revision digest the run covered (admission checks it), or stays
/// bare when no candidate ran.
#[allow(clippy::too_many_arguments)]
fn shell(
    plan: &RefactorPlan,
    spec: &VerificationDocument,
    spec_digest: String,
    baseline_source: Option<String>,
    baseline: Option<CheckResults>,
    candidate: Option<CheckResults>,
    candidate_digest: Option<String>,
) -> VerificationDocument {
    let confinement = match candidate_digest {
        Some(digest) => vec![
            format!("disposable-copy:{digest}"),
            "bounded-output".to_owned(),
        ],
        None => vec!["disposable-copy".to_owned(), "bounded-output".to_owned()],
    };
    VerificationDocument {
        schema_version: version::VERIFICATION_SCHEMA_VERSION.to_owned(),
        identity: version::VERIFICATION_IDENTITY.to_owned(),
        variant: "execution".to_owned(),
        verification: spec.verification.clone(),
        spec: Some(spec_digest),
        plan: Some(plan.plan.clone()),
        tool: spec.tool.clone(),
        // The exact consumed-input tree line of this run's argv script;
        // apply re-derives it from the live tree at admission (F3/X3).
        baseline_source,
        checks: spec.checks.clone(),
        confinement,
        baseline,
        candidate,
        oracle: None,
        conclusion: None,
        custody: None,
    }
}

/// Run every check once against one disposable copy, through the
/// injected host capability: the core measures and classifies what the
/// host reports, never spawning anything itself (X2/R3).
fn run_checks(
    host: &dyn VerificationHost,
    program: &Path,
    args: &[String],
    checks: &[super::wire::CheckSpec],
    copy: &Path,
) -> CheckResults {
    let mut outcomes = Vec::new();
    let run = host.run(program, args, copy);
    for check in checks {
        let outcome = match &run {
            Err(detail) => CheckOutcome {
                id: check.id.clone(),
                status: "unavailable".to_owned(),
                exit: None,
                detail: detail.clone(),
            },
            Ok(measured) => {
                let exit_ok = measured.0 == Some(check.expect_exit as i32);
                let stdout_ok = check
                    .expect_stdout
                    .iter()
                    .all(|expected| measured.1.contains(expected.as_str()));
                let status = if exit_ok && stdout_ok {
                    "passed"
                } else if measured.0.is_none() {
                    "unavailable"
                } else {
                    "failed"
                };
                CheckOutcome {
                    id: check.id.clone(),
                    status: status.to_owned(),
                    exit: measured.0.map(|exit| exit.max(0) as u32),
                    detail: if !exit_ok {
                        "exit-mismatch".to_owned()
                    } else if !stdout_ok {
                        "stdout-missing".to_owned()
                    } else {
                        "ok".to_owned()
                    },
                }
            }
        };
        outcomes.push(outcome);
    }
    CheckResults { checks: outcomes }
}

/// Copy the project into a fresh disposable directory, skipping VCS,
/// build, and runtime-state homes, under strict byte and file ceilings.
/// The temp home is held by RAII for the whole construction: every
/// early return — over-limit, unreadable, or copy failure — removes
/// the whole directory, so partially copied project material never
/// leaks into the host temp area (F7). Returns the detached inner
/// `project` path plus the guard for the outer home; the caller
/// removes the inner directory on its own schedule.
fn copy_tree(root: &Path) -> Outcome<(PathBuf, tempdir::Guard)> {
    let mut guard = tempdir::Guard::new()?;
    let destination = guard.path().join("project");
    std::fs::create_dir_all(&destination).map_err(|_| {
        DomainResult::invalid(diagnostic::evidence_incomplete_set(
            "copy",
            "tempdir-unavailable",
        ))
    })?;
    let mut total_bytes = 0usize;
    let mut total_files = 0usize;
    if let Err(error) = copy_recursive(root, &destination, &mut total_bytes, &mut total_files) {
        // Nothing of this tree survives a failed preparation.
        guard.remove_tree();
        return Err(error);
    }
    Ok((destination, guard))
}

/// The bounded disposable temp home: a fresh host-temp directory held
/// for the whole run and removed on drop. Unlike `TempDir.keep()`, the
/// directory can never leak on an early return, and the success path
/// removes the whole home (inner copy included) instead of leaving
/// empty parent litter.
mod tempdir {
    use super::diagnostic;
    use crate::result::DomainResult;
    use std::path::{Path, PathBuf};

    pub(super) struct Guard {
        path: Option<PathBuf>,
    }

    impl Guard {
        pub(super) fn new() -> super::super::Outcome<Self> {
            // tempfile is the accepted bounded-temp seam; the handle is
            // kept alive through the guard so only an explicit success
            // path can shorten its lifetime.
            match tempfile::Builder::new()
                .prefix("lekalo-adoption-")
                .tempdir()
            {
                Ok(dir) => Ok(Self {
                    path: Some(dir.keep()),
                }),
                Err(_) => Err(DomainResult::invalid(diagnostic::evidence_incomplete_set(
                    "copy",
                    "tempdir-unavailable",
                ))),
            }
        }

        pub(super) fn path(&self) -> &Path {
            self.path
                .as_deref()
                .expect("guard holds the temp home until taken")
        }

        /// Best-effort whole-home removal on a failed preparation.
        pub(super) fn remove_tree(&mut self) {
            if let Some(path) = self.path.take() {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            self.remove_tree();
        }
    }
}

fn copy_recursive(
    source: &Path,
    destination: &Path,
    total_bytes: &mut usize,
    total_files: &mut usize,
) -> Outcome<()> {
    let entries = std::fs::read_dir(source).map_err(|_| {
        DomainResult::invalid(diagnostic::evidence_incomplete_set("copy", "unreadable"))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if TREE_EXCLUDED.contains(&name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|_| {
            DomainResult::invalid(diagnostic::evidence_incomplete_set("copy", "unreadable"))
        })?;
        let target = destination.join(name);
        if metadata.is_dir() {
            std::fs::create_dir_all(&target).map_err(|_| {
                DomainResult::invalid(diagnostic::evidence_incomplete_set(
                    "copy",
                    "tempdir-unavailable",
                ))
            })?;
            copy_recursive(&entry.path(), &target, total_bytes, total_files)?;
        } else if metadata.is_file() {
            *total_bytes += metadata.len() as usize;
            *total_files += 1;
            if *total_bytes > version::COPY_TOTAL_BYTES || *total_files > version::COPY_TOTAL_FILES
            {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "copy",
                    "over-limit",
                )));
            }
            std::fs::copy(entry.path(), &target).map_err(|_| {
                DomainResult::invalid(diagnostic::evidence_incomplete_set("copy", "unreadable"))
            })?;
        }
        // Reparse points, symlinks, and junctions are skipped on sight.
    }
    Ok(())
}

/// Apply the exact patch set to a disposable copy. Only the exact
/// bounded relative paths are touched; nothing else moves — never a
/// marker into the original project tree.
fn apply_patch(copy: &Path, patch: &PatchDocument) -> Outcome<()> {
    for file in &patch.files {
        let target = copy.join(&file.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|_| {
                DomainResult::invalid(diagnostic::evidence_incomplete_set("patch", "create"))
            })?;
        }
        match file.action.as_str() {
            "delete" => {
                let _ = std::fs::remove_file(&target);
            }
            _ => {
                let content = file.content.as_deref().unwrap_or_default();
                std::fs::write(&target, content).map_err(|_| {
                    DomainResult::invalid(diagnostic::evidence_incomplete_set("patch", "write"))
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(id: &str, kind: &str) -> wire::CheckSpec {
        wire::CheckSpec {
            id: id.to_owned(),
            kind: kind.to_owned(),
            expect_exit: 0,
            expect_stdout: Vec::new(),
        }
    }

    /// A stub host: it resolves nothing and never runs — the pure-core
    /// contract is that no code path can reach a process through it.
    struct OfflineHost;

    impl VerificationHost for OfflineHost {
        fn resolve_program(&self, _program: &str) -> Option<PathBuf> {
            None
        }
        fn run(
            &self,
            _program: &Path,
            _args: &[String],
            _cwd: &Path,
        ) -> Result<(Option<i32>, String), String> {
            Err("offline-host".to_owned())
        }
    }

    #[test]
    fn unresolvable_tools_conclude_unavailable_never_passed() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("src")).expect("dir");
        std::fs::write(dir.path().join("src/tasks.mjs"), b"task").expect("write");
        let ctx = super::super::analysis::AdoptionContext {
            model: crate::loader::NormalizedModel {
                model_version: crate::loader::project_docs::ModelVersion::Current,
                project: None,
                modules: Vec::new(),
                definitions: Vec::new(),
            },
            root: dir.path().to_path_buf(),
        };
        let spec = specification(&["tasks.native-test"], "node", vec!["tools/verify.mjs"]);
        let bytes = serde_json::to_vec(&spec).expect("spec serializes");
        let spec_digest = {
            let value = serde_json::to_value(&spec).expect("spec serializes");
            wire::digest("adoption-verification", &value)
        };
        let tree = tree_state_digest(dir.path()).expect("tree digest");
        let mut plan = plan_over("src/tasks.mjs", Some(spec_digest), Some(tree));
        let mut paths = std::collections::BTreeSet::new();
        paths.insert("src/tasks.mjs".to_owned());
        plan.pins = super::super::analysis::compute_pins(&ctx, None, &paths).expect("pins");
        let execution =
            execute(&ctx, &OfflineHost, None, &plan, &bytes, None).expect("honest unavailable");
        assert_eq!(execution.conclusion.as_deref(), Some("unavailable"));
        assert!(
            execution
                .baseline
                .iter()
                .chain(execution.candidate.iter())
                .flat_map(|side| side.checks.iter())
                .all(|outcome| outcome.status == "unavailable"),
            "no test is ever reported passed without a run"
        );
    }

    #[test]
    fn tree_state_digest_pins_every_consumed_file_and_refuses_oversized_trees() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("src.mjs"), b"one").expect("write");
        std::fs::create_dir_all(dir.path().join("src")).expect("dir");
        std::fs::create_dir_all(dir.path().join("tools")).expect("dir");
        std::fs::create_dir(dir.path().join(".lekalo")).expect("dir");
        std::fs::write(dir.path().join(".lekalo/runtime.json"), b"excluded").expect("write");
        let first = tree_state_digest(dir.path()).expect("digest");
        let again = tree_state_digest(dir.path()).expect("digest");
        assert_eq!(first, again, "deterministic");
        // Runtime state never moves the digest.
        std::fs::write(dir.path().join(".lekalo/runtime.json"), b"changed").expect("write");
        assert_eq!(tree_state_digest(dir.path()).expect("digest"), first);
        // An unlisted import moved after the run does.
        std::fs::write(dir.path().join("src/limits.mjs"), b"imported").expect("write");
        assert_ne!(tree_state_digest(dir.path()).expect("digest"), first);
        // A second new file is a new state too (typed absence pinned).
        std::fs::write(dir.path().join("tools/oracle.mjs"), b"oracle").expect("write");
        let second = tree_state_digest(dir.path()).expect("digest");
        assert_ne!(second, first);
        // The shared copy ceilings refuse oversized trees with the
        // typed over-limit class, exactly like copy preparation.
        std::fs::write(
            dir.path().join("big.mjs"),
            vec![0u8; version::COPY_TOTAL_BYTES + 1],
        )
        .expect("write");
        let refused = tree_state_digest(dir.path()).expect_err("over-limit");
        assert_eq!(refused.status(), crate::result::Status::Invalid);
    }

    #[test]
    fn consumed_manifest_covers_every_argv_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("runner.mjs"), b"runner").expect("write");
        std::fs::write(dir.path().join("oracle.mjs"), b"oracle").expect("write");
        let tool = wire::ToolRef {
            program: "node".to_owned(),
            args: vec![
                "runner.mjs".to_owned(),
                "--flag".to_owned(),
                "oracle.mjs".to_owned(),
                "missing.mjs".to_owned(),
            ],
            digest: None,
        };
        let manifest = consumed_manifest(dir.path(), &tool);
        assert_eq!(manifest.scripts.len(), 2, "rel-path argv entries only");
        assert_eq!(manifest.scripts[0].path, "oracle.mjs");
        assert_eq!(manifest.scripts[1].path, "runner.mjs");
        assert_eq!(manifest.absent, vec!["missing.mjs".to_owned()]);
        assert!(manifest.bounded());
        // The tree line is stable and content-addressed.
        let again = consumed_manifest(dir.path(), &tool);
        assert_eq!(manifest.tree_digest(), again.tree_digest());
        std::fs::write(dir.path().join("oracle.mjs"), b"drifted").expect("write");
        let drifted = consumed_manifest(dir.path(), &tool);
        assert_ne!(manifest.tree_digest(), drifted.tree_digest());
    }

    /// A minimal plan bound to one pinned source path.
    fn plan_over(path: &str, verification: Option<String>, tree: Option<String>) -> RefactorPlan {
        RefactorPlan {
            schema_version: version::REFACTOR_PLAN_SCHEMA_VERSION.to_owned(),
            identity: version::REFACTOR_PLAN_IDENTITY.to_owned(),
            plan: "sha256:".to_owned() + &"a".repeat(64),
            project: "taskboard".to_owned(),
            analysis: "sha256:".to_owned() + &"b".repeat(64),
            selection: wire::Scope {
                kind: "symbol".to_owned(),
                id: "taskboard.complete_task".to_owned(),
            },
            patch: None,
            verification,
            recommendations: Vec::new(),
            unsupported: Vec::new(),
            read_scope: vec![path.to_owned()],
            mutation_scope: Vec::new(),
            preservation: Vec::new(),
            pins: super::wire::Pins {
                model: "sha256:".to_owned() + &"c".repeat(64),
                lock: None,
                observed_index: None,
                adapter: None,
                sources: Vec::new(),
            },
            rollback: wire::RollbackHome {
                journal: String::new(),
                snapshots: String::new(),
            },
            tree,
        }
    }

    fn specification(checks: &[&str], program: &str, args: Vec<&str>) -> VerificationDocument {
        VerificationDocument {
            schema_version: version::VERIFICATION_SCHEMA_VERSION.to_owned(),
            identity: version::VERIFICATION_IDENTITY.to_owned(),
            variant: "specification".to_owned(),
            verification: "probe".to_owned(),
            spec: None,
            plan: None,
            tool: Some(wire::ToolRef {
                program: program.to_owned(),
                args: args.into_iter().map(str::to_owned).collect(),
                digest: None,
            }),
            baseline_source: None,
            checks: checks.iter().map(|id| check(id, "native-test")).collect(),
            confinement: vec!["disposable-copy".to_owned()],
            baseline: None,
            candidate: None,
            oracle: None,
            conclusion: None,
            custody: None,
        }
    }
}
