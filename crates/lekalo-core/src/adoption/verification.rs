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
//! Custody: [`admit_execution`] is the only path an execution document
//! takes into apply. It re-derives the execution envelope's binding to
//! the exact plan, plan-pinned candidate digest, and complete consumed
//! input set, and demands the measured results be execution custody —
//! the run digest and measured timestamp this module alone assembles —
//! with the measured outcomes in exact 1:1 correspondence to the
//! declared checks and the oracle verdict re-derived from them. The
//! custody recipe is public and recomputable by design: what makes a
//! hand-written envelope refuse is the consumed-input binding — the
//! baseline-source tree line must re-derive from the argv script bytes
//! consumed at run time and the plan's pinned tree must still hold at
//! admission — together with the replay refusal on the candidate
//! digest. A forged, stale, or replayed conclusion refuses before any
//! write.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// Directories never copied into a disposable candidate copy.
const COPY_EXCLUDED: [&str; 3] = [".git", "target", ".lekalo"];

/// The bounded child environment: an allow-listed subset of the host
/// names a runtime genuinely needs, everything else cleared (the same
/// discipline the native-gate runner owns). Secrets and unrelated host
/// state never reach a verification child.
const CHILD_ENV_ALLOWED: [&str; 6] = [
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "COMSPEC",
    "PROGRAMFILES",
];

/// The closed execution-custody marker member: assembled only by this
/// module's real runs and required by apply's admission. A document
/// that decodes without it never carries a measured conclusion.
pub(crate) const CUSTODY_MEMBER: &str = "custody";

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
            && self
                .scripts
                .iter()
                .all(|script| version::is_rel_path(&script.path) && version::is_sha256(&script.sha256))
            && self.absent.iter().all(|path| version::is_rel_path(path))
    }
}

/// The consumed-input manifest of one disposable-copy run: the tool
/// argv's first argument is the runner script the specification really
/// executes, so its exact bytes (or typed absence) are pinned into the
/// execution and revalidated before install. The runner's own import
/// graph is the project's business — the native test, oracle, and
/// source dependencies it touches are read inside the disposable copy,
/// never trusted as admission evidence from outside (F2/X3 fix). The
/// manifest covers exactly the bytes a forged document cannot know
/// without reading the tree at run time.
fn consumed_manifest(
    root: &Path,
    tool: &super::wire::ToolRef,
) -> ConsumedInputs {
    let mut scripts = Vec::new();
    let mut absent = Vec::new();
    for arg in tool.args.iter().take(1) {
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
/// candidate runs happen inside disposable copies only; the project is
/// never modified and no report or cache home is written.
pub fn execute(
    ctx: &super::analysis::AdoptionContext,
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
    if plan.verification.as_deref() != Some(spec_digest.as_str()) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "plan-mismatch",
        )));
    }
    // F3/X3 pre-execution barrier: the plan's pinned *source* set is
    // re-fingerprinted against the live tree before any process
    // exists (sources only — the observed-index/adapter/model pins are
    // scan evidence the verify path does not re-scan, exactly like the
    // apply path's source re-fingerprint). A drifted plan never
    // executes; the same staleness class the apply path enforces,
    // moved in front of the run.
    {
        let current = super::analysis::compute_pins(ctx, None, &pin_paths(plan))?;
        let changed: Vec<String> = source_drift(&plan.pins, &current);
        if !changed.is_empty() {
            return Err(DomainResult::denied(diagnostic::plan_stale_set(
                &plan.plan,
                &changed,
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
    let Some(program) = resolve_program(&tool.program) else {
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
    // pinned into the execution and revalidated at apply, so a
    // trivialized runner can never serve a run it did not measure.
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
    let baseline = run_checks(&program, &tool.args, &spec.checks, &baseline_copy);

    // Candidate: a second disposable copy with the exact patch applied.
    let candidate_result = match patch {
        Some(document) => {
            let (candidate_copy, candidate_guard) = copy_tree(&ctx.root)?;
            let checks = match apply_patch(&candidate_copy, document) {
                Ok(()) => run_checks(&program, &tool.args, &spec.checks, &candidate_copy),
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
    execution.baseline_source = Some(consumed.tree_digest());
    // Execution custody: the run binds the exact plan, candidate
    // revision, and consumed-input tree line into the envelope only
    // this module assembles for real runs.
    let measured_ms = (started.elapsed().as_millis() as u64).clamp(1, 60_000_000);
    execution.custody = Some(custody_of(&execution, measured_ms));
    Ok(execution)
}

/// The plan-time source-pin path set (the same consumed set apply
/// re-fingerprints): read scope ∪ mutation scope, so the pre-execution
/// barrier and the apply barrier cannot disagree structurally.
fn pin_paths(plan: &RefactorPlan) -> std::collections::BTreeSet<String> {
    plan.read_scope
        .iter()
        .chain(plan.mutation_scope.iter())
        .cloned()
        .collect()
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

/// Admit one execution document for apply (X1/X2): the document must
/// decode as an execution variant bound to the exact plan and the
/// exact plan-pinned candidate digest, must pin the complete consumed
/// input set the plan pinned (or typed absence), and must carry the
/// execution custody this module assembles over measured results — the
/// run digest must match the admitted document's own bytes and the
/// measured duration must be present and bounded. A forged, stale, or
/// replayed conclusion refuses before any write.
pub(crate) fn admit_execution(
    ctx: &super::analysis::AdoptionContext,
    plan: &RefactorPlan,
    patch_digest: &str,
    execution_bytes: &[u8],
) -> Outcome<VerificationDocument> {
    let execution =
        wire::execution_of_bytes(execution_bytes).map_err(|error| error.domain_result())?;
    if execution.variant != "execution" {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "not-an-execution",
        )));
    }
    if execution.spec.as_deref() != plan.verification.as_deref()
        || execution.plan.as_deref() != Some(plan.plan.as_str())
    {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "plan-mismatch",
        )));
    }
    // Custody first: a conclusion without this module's run digest and
    // a bounded measured duration is never evidence of passing.
    let Some(custody) = &execution.custody else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "custody-missing",
        )));
    };
    let value = serde_json::to_value(&execution).expect("execution serializes");
    let expected = wire::digest(
        "adoption-verification-run",
        &wire::without_member(&value, CUSTODY_MEMBER),
    );
    if custody.run != expected || !version::is_sha256(&custody.run) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "custody-mismatch",
        )));
    }
    if custody.measured_ms == 0 || custody.measured_ms > 60_000_000 {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "custody-bounds",
        )));
    }
    // A forged document can recompute any self-consistent envelope, so
    // admission rests on the one thing a hand-written document cannot
    // fake retroactively: the consumed-input manifest this module
    // derived from the live tree at run time. The recorded tree line
    // must re-derive exactly from the manifest members over the named
    // paths, and the manifest must cover the specification's argv
    // script (or its typed absence) — the bytes a trivialized or
    // swapped runner would have measured (F3/X3).
    validate_consumed_binding(&execution, &ctx.root)?;
    // The measured conclusion must be preserved: conclusions are
    // checked before candidate coverage so a failed run is honestly
    // classified verification-failed (LEK-ADOPT-007), never laundered
    // through an unrelated mismatch.
    if execution.conclusion.as_deref() != Some("preserved") {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            execution
                .oracle
                .as_ref()
                .map(|oracle| oracle.detail.as_str())
                .unwrap_or("conclusion"),
            &execution.spec.clone().unwrap_or_default(),
        )));
    }
    // The admitted run must cover this exact candidate revision: a
    // genuine preserved execution for any other patch bytes is a
    // replay, never evidence for this apply.
    if !run_covers_candidate(&execution, patch_digest) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "candidate-mismatch",
        )));
    }
    // The recorded per-check outcomes must really support `preserved`:
    // outcomes are measured 1:1 against the declared checks (same ids,
    // same order, no unmeasured inventions, none missing), the oracle
    // verdict is re-derived from the behavior-oracle measurements (a
    // caller-supplied verdict is never accepted), and the conclusion is
    // re-derived from the whole envelope — the exact derivation the
    // real run performs. A hand-written envelope that skips a check,
    // invents a passing id, or asserts an oracle that never held
    // refuses here (X1/F11).
    let spec_checks = &execution.checks;
    measured_1to1(&execution.baseline, spec_checks, "baseline")?;
    measured_1to1(&execution.candidate, spec_checks, "candidate")?;
    if !oracle_holds(&execution) {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            "behavior-oracle",
            &execution.spec.clone().unwrap_or_default(),
        )));
    }
    if derived_conclusion(&execution) != Some("preserved") {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            "measured-outcomes",
            &execution.spec.clone().unwrap_or_default(),
        )));
    }
    // The plan's pinned consumed *source* set must still match the
    // tree at admission time (the stale-plan barrier at apply, mirrored
    // from the execution path; sources only, as above), so a plan
    // drifted after its verification never installs.
    {
        let current = super::analysis::compute_pins(ctx, None, &pin_paths(plan))?;
        let changed: Vec<String> = source_drift(&plan.pins, &current);
        if !changed.is_empty() {
            return Err(DomainResult::denied(diagnostic::plan_stale_set(
                &plan.plan,
                &changed,
            )));
        }
    }
    Ok(execution)
}

/// The changed members between two pin sets, restricted to the source
/// pins: the verify/admission barrier re-fingerprints the plan's
/// pinned sources against the live tree without an observed-index
/// rescan, so scan-evidence members (adapter/index/model/lock) are out
/// of scope here while every source drift is caught.
fn source_drift(
    expected: &super::wire::Pins,
    current: &super::wire::Pins,
) -> Vec<String> {
    use std::collections::BTreeMap;
    let expected_sources: BTreeMap<&str, &str> = expected
        .sources
        .iter()
        .map(|source| (source.path.as_str(), source.sha256.as_str()))
        .collect();
    let current_sources: BTreeMap<&str, &str> = current
        .sources
        .iter()
        .map(|source| (source.path.as_str(), source.sha256.as_str()))
        .collect();
    let mut changed: Vec<String> = expected_sources
        .iter()
        .filter(|(path, digest)| current_sources.get(*path) != Some(&&**digest))
        .map(|(path, _)| (*path).to_string())
        .collect();
    for path in current_sources.keys() {
        if !expected_sources.contains_key(path) {
            changed.push((*path).to_owned());
        }
    }
    changed.sort();
    changed.dedup();
    changed
}

/// The consumed-input binding of one execution (F3/X3): the recorded
/// baseline-source tree line must be exactly the digest of the recorded
/// manifest members, the manifest must be bounded and well-formed, and
/// every manifest path must be exactly the specification's argv script
/// consumed at run time — re-derived from the current project tree, so
/// a drifted or swapped runner refuses before any write. An execution
/// without the binding (or one whose tree line does not re-derive)
/// never admits.
fn validate_consumed_binding(
    execution: &VerificationDocument,
    root: &Path,
) -> Outcome<()> {
    let Some(recorded) = &execution.baseline_source else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "consumed-missing",
        )));
    };
    // The manifest travels in the run digest; the tree line is the
    // content address of the manifest members. Reconstruct the manifest
    // from the argv script consumed now and compare.
    let Some(tool) = &execution.tool else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "tool-missing",
        )));
    };
    let current = consumed_manifest(root, tool);
    if !current.bounded() {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "consumed",
            "over-limit",
        )));
    }
    if current.tree_digest() != *recorded {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            "consumed-drift",
        )));
    }
    Ok(())
}

/// The measured side must cover the declared checks 1:1: exactly the
/// declared ids in declaration order, every outcome `passed`, and no
/// invented or missing measurements (X1/F11).
fn measured_1to1(
    side: &Option<CheckResults>,
    spec_checks: &[super::wire::CheckSpec],
    name: &str,
) -> Outcome<()> {
    let Some(side) = side else {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "verification",
            name,
        )));
    };
    if side.checks.len() != spec_checks.len() {
        return Err(DomainResult::invalid(diagnostic::verification_failed_set(
            name,
            "measurement-coverage",
        )));
    }
    for (measured, declared) in side.checks.iter().zip(spec_checks.iter()) {
        if measured.id != declared.id {
            return Err(DomainResult::invalid(diagnostic::verification_failed_set(
                name,
                "measurement-identity",
            )));
        }
        if measured.status != "passed" {
            return Err(DomainResult::invalid(diagnostic::verification_failed_set(
                &measured.id,
                name,
            )));
        }
    }
    Ok(())
}

/// Re-derive the behavior-oracle verdict from the measurements (X1):
/// the recorded oracle must be `passed` exactly when every declared
/// behavior-oracle check passed on both sides; with no oracle checks
/// declared, the recorded verdict must be `unavailable`.
fn oracle_holds(execution: &VerificationDocument) -> bool {
    let oracle = execution
        .oracle
        .as_ref()
        .map(|oracle| oracle.status.as_str());
    let oracle_ids: Vec<&str> = execution
        .checks
        .iter()
        .filter(|check| check.kind == "behavior-oracle")
        .map(|check| check.id.as_str())
        .collect();
    let both_passed = |side: &Option<CheckResults>| {
        oracle_ids.iter().all(|id| {
            side.as_ref()
                .map(|side| {
                    side.checks
                        .iter()
                        .any(|check| check.id == *id && check.status == "passed")
                })
                .unwrap_or(false)
        })
    };
    if oracle_ids.is_empty() {
        return oracle == Some("unavailable");
    }
    oracle == Some("passed") && both_passed(&execution.baseline) && both_passed(&execution.candidate)
}

/// The conclusion re-derived from the whole envelope — the exact
/// derivation the real run performs (X1/F11).
fn derived_conclusion(execution: &VerificationDocument) -> Option<&str> {
    let any_unavailable = execution
        .baseline
        .iter()
        .chain(execution.candidate.iter())
        .flat_map(|side| side.checks.iter())
        .any(|check| check.status == "unavailable");
    let any_failed = execution
        .baseline
        .iter()
        .chain(execution.candidate.iter())
        .flat_map(|side| side.checks.iter())
        .any(|check| check.status == "failed");
    let oracle = execution
        .oracle
        .as_ref()
        .map(|oracle| oracle.status.as_str());
    Some(if any_unavailable && !any_failed {
        "unavailable"
    } else if any_failed || oracle == Some("failed") {
        "failed"
    } else if oracle == Some("unavailable") {
        "unavailable"
    } else {
        "preserved"
    })
}

/// Whether the executed run bound the candidate revision this apply
/// installs. Real runs record the patched candidate-copy digest under
/// the `disposable-copy` confinement token; admission requires it.
fn run_covers_candidate(execution: &VerificationDocument, patch_digest: &str) -> bool {
    execution.confinement.iter().any(|token| {
        token
            .strip_prefix("disposable-copy:")
            .is_some_and(|bound| bound == patch_digest)
    })
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

/// Stamp the exact candidate revision the run covered into the
/// confinement dimension; the bare token form remains the no-candidate
/// spelling. The admission path rejects a bound token naming any other
/// digest, so a replayed run can never stand in for this candidate.
#[cfg(test)]
pub(crate) fn bind_candidate(execution: &mut VerificationDocument, patch_digest: &str) {
    execution.confinement = execution
        .confinement
        .iter()
        .map(|token| {
            if token == "disposable-copy" {
                format!("disposable-copy:{patch_digest}")
            } else {
                token.clone()
            }
        })
        .collect();
}

/// Resolve one qualified program name through PATH.
fn resolve_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    std::env::split_paths(&path)
        .map(|directory| directory.join(&executable))
        .find(|candidate| candidate.is_file())
}

/// Run every check once against one disposable copy.
fn run_checks(
    program: &Path,
    args: &[String],
    checks: &[super::wire::CheckSpec],
    copy: &Path,
) -> CheckResults {
    let mut outcomes = Vec::new();
    let run = execute_command(program, args, copy);
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

/// One bounded run: `(exit code, bounded stdout)`. Both pipes drain on
/// dedicated threads for the whole child lifetime — a tool emitting
/// more than the pipe buffer can never deadlock the deadline — the
/// child gets its own process group on unix, teardown kills the whole
/// process tree, and the child environment is cleared down to the
/// bounded runtime allow-list. A timeout, a spawn failure, or a signal
/// exit is measured, never invented.
fn execute_command(
    program: &Path,
    args: &[String],
    cwd: &Path,
) -> Result<(Option<i32>, String), String> {
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
    // reaps descendants (the same discipline the house runner owns).
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
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() > Duration::from_millis(version::CHECK_TIMEOUT_MS) {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
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
                let remaining = version::CHECK_OUTPUT_BYTES.saturating_sub(bytes.len());
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

/// Process-tree teardown: the same kill discipline the house runner
/// owns — the unix child sits in its own process group; Windows walks
/// the tree with the platform tool.
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
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
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
    use std::path::{Path, PathBuf};
    use crate::result::DomainResult;

    pub(super) struct Guard {
        path: Option<PathBuf>,
    }

    impl Guard {
        pub(super) fn new() -> super::super::Outcome<Self> {
            // tempfile is the accepted bounded-temp seam; the handle is
            // kept alive through the guard so only an explicit success
            // path can shorten its lifetime.
            match tempfile::Builder::new().prefix("lekalo-adoption-").tempdir() {
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
        if COPY_EXCLUDED.contains(&name) {
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
