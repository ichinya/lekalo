//! The `module api` CLI surface (issue #113). The core owns every
//! decision; this binary only loads exact files, dispatches, renders, and
//! maps exits. Read-only: no canonical Model, lock, report, baseline, or
//! cache writes, no adapter or network activity, and no Git state change.
//!
//! Status mapping per `result.rs`: valid/invalid/denied/unavailable/
//! unsupported-version project to the stable exit classes of the single
//! `DomainResult` envelope.

use clap::Subcommand;
use lekalo_core::loader::LoadSelection;
use lekalo_core::module_api::diagnostic::invalid;
use lekalo_core::module_api::diff::compare_public;
use lekalo_core::module_api::lock::{check_lock, parse_lock, resolve_lock};
use lekalo_core::module_api::wire;
use lekalo_core::module_api::{
    self, check, consumer_snapshot, plan_consumer_context, resolve, FindingClass,
};
use lekalo_core::{DomainResult, Status};
use std::collections::BTreeSet;
use std::io::Read;

/// The `module api` subcommands.
#[derive(Debug, Subcommand)]
pub enum ApiCommands {
    /// Validate the whole bound project against the attachment: imports,
    /// export closures, applicable obligations, effects, and consumers.
    Check {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The module API attachment document (project-relative path).
        #[arg(long, value_name = "PATH")]
        api: String,
        /// Filter owned diagnostics to one exact module id; mandatory
        /// cross-module errors are retained.
        #[arg(long, value_name = "MODULE")]
        module: Option<String>,
        /// Reject exact semantic boundary violations instead of recording
        /// them advisory.
        #[arg(long)]
        strict: bool,
        /// Check the supplied consumer lock against the current
        /// resolution (requires --lock).
        #[arg(long, requires = "lock")]
        check_lock: bool,
        /// The consumer lock document to check (with --check-lock).
        #[arg(long, value_name = "PATH")]
        lock: Option<String>,
    },
    /// Emit the complete public bundle for one exact module id. An
    /// incomplete or illegal closure never produces a certified bundle.
    Export {
        /// The exact producer module id.
        module: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The module API attachment document.
        #[arg(long, value_name = "PATH")]
        api: String,
    },
    /// Emit the immutable declared and resolved consumer snapshot of one
    /// producer revision.
    Consumers {
        /// The exact producer module id.
        module: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The module API attachment document.
        #[arg(long, value_name = "PATH")]
        api: String,
    },
    /// Print a candidate consumer lock payload (never a file), or check
    /// an existing lock against the current resolution.
    Lock {
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The module API attachment document.
        #[arg(long, value_name = "PATH")]
        api: String,
        /// The exact producer module id.
        #[arg(long, value_name = "MODULE")]
        module: String,
        /// The exact consumer module id.
        #[arg(long, value_name = "MODULE")]
        consumer: String,
        /// The consumer lock document to check (requires --check).
        #[arg(long, value_name = "PATH", requires = "check")]
        lock: Option<String>,
        /// Headless check mode: a stale or foreign lock denies.
        #[arg(long)]
        check: bool,
    },
    /// Compare two exact public bundles of the same semantic module with
    /// the optional consumer snapshot pair.
    Diff {
        /// The base public bundle document.
        base: String,
        /// The candidate public bundle document.
        candidate: String,
        /// The base consumer snapshot (the pair is required together).
        #[arg(long, value_name = "PATH", requires = "consumers_after")]
        consumers_before: Option<String>,
        /// The candidate consumer snapshot (the pair is required
        /// together).
        #[arg(long, value_name = "PATH", requires = "consumers_before")]
        consumers_after: Option<String>,
        /// Evaluate each consumer's declared policy separately and deny
        /// incompatible resolutions.
        #[arg(long)]
        strict: bool,
    },
    /// Plan a bounded consumer capsule from one standalone public bundle
    /// only. No project, attachment, provider checkout, cache, or network.
    Context {
        /// The standalone public bundle document.
        public: String,
        /// The exact consumer module id.
        #[arg(long, value_name = "MODULE")]
        consumer: String,
        /// Exact comma-separated export ids; nonempty and unique.
        #[arg(long, value_name = "IDS")]
        uses: String,
        /// The explicit token budget (1..=1,000,000).
        #[arg(long, value_name = "TOKENS")]
        budget: u64,
    },
}

/// Read one bounded input file; the core never touches the filesystem.
fn read(path: &str) -> Result<Vec<u8>, Failure> {
    let file = std::fs::File::open(path).map_err(|_| Failure::Set(invalid("input-read")))?;
    if !file
        .metadata()
        .map_err(|_| invalid("input-metadata"))?
        .is_file()
    {
        return Err(Failure::Set(invalid("input-file")));
    }
    let mut bytes = vec![];
    file.take(wire::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Set(invalid("input-read")))?;
    if bytes.len() > wire::MAX_BYTES {
        return Err(Failure::Set(invalid("input-size")));
    }
    Ok(bytes)
}

/// The certified-bundle gate: the boundary check must report no exact
/// semantic violation for the module *and for every transitive public
/// dependency of its certified closure* before a bundle or lock may be
/// emitted (finding 20: a dirty dependency's own closure can never hide
/// behind --module scoping). Illegal closures refuse regardless of
/// advisory mode.
fn certified(
    attachment: &[u8],
    compilation: &lekalo_core::ir::Compilation,
    module: &str,
) -> Result<(), Failure> {
    let resolved = resolve(attachment, compilation)?;
    let mut session = lekalo_core::module_api::public::ExportSession::new(&resolved, compilation);
    if let Err(failure) = session.bundle_checked(module) {
        return match failure {
            lekalo_core::module_api::public::ExportFailure::Invalid(set) => Err(Failure::Set(set)),
            lekalo_core::module_api::public::ExportFailure::Cycle(_) => {
                Err(Failure::Set(invalid("public-dependency-cycle")))
            }
        };
    }
    // The certified closure: every transitive public dependency pin.
    let mut scope: BTreeSet<String> = BTreeSet::new();
    scope.insert(module.to_owned());
    let mut work: Vec<String> = vec![module.to_owned()];
    let mut seen: BTreeSet<String> = work.iter().cloned().collect();
    while let Some(current) = work.pop() {
        let Ok(bundle) = session.bundle(&current) else {
            continue;
        };
        for dependency in &bundle.public_dependencies {
            scope.insert(dependency.module_id.clone());
            if seen.insert(dependency.module_id.clone()) {
                work.push(dependency.module_id.clone());
            }
        }
    }
    // The unfiltered boundary check owns the whole project; the certified
    // set refuses when any scoped member owns a boundary violation.
    let verdict = check(attachment, compilation, false, None).map_err(Failure::from)?;
    let scoped: Vec<_> = verdict
        .evidence
        .iter()
        .filter(|finding| {
            is_boundary_rule(finding.rule)
                && finding
                    .consumer
                    .as_deref()
                    .or(finding.producer.as_deref())
                    .is_some_and(|owner| scope.contains(owner))
        })
        .cloned()
        .collect();
    if !scoped.is_empty() {
        let set = lekalo_core::module_api::diagnostic::set(&scoped, Status::Invalid)
            .map_err(Failure::from)?;
        return Err(Failure::Set(set));
    }
    Ok(())
}

/// Whether one rule is an exact semantic boundary violation.
fn is_boundary_rule(rule: &str) -> bool {
    matches!(
        rule,
        "module-api.symbol-not-exported"
            | "module-api.import-undeclared"
            | "module-api.public-private-exposure"
            | "module-api.public-target-exposure"
            | "module-api.cross-module-write-forbidden"
            | "module-api.dependency-cycle"
    )
}

/// Map one core failure set to the typed result: unsupported family
/// versions exit 5 on stderr; everything else is invalid.
fn failure(set: lekalo_core::diagnostics::DiagnosticSet) -> DomainResult {
    if set
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "module-api.version-unsupported")
    {
        DomainResult::unsupported_version(set)
    } else {
        DomainResult::invalid(set)
    }
}

/// The shared load/compile preamble of the project-bound commands. A
/// loader/compile refusal carries its finished `DomainResult` verbatim.
fn compilation(project: &Option<String>) -> Result<lekalo_core::ir::Compilation, Failure> {
    let selection = LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Ok(model) => model,
        Err(result) => return Err(Failure::Finished(result)),
    };
    match lekalo_core::ir::compile(&model) {
        Ok(compilation) => Ok(compilation),
        Err(failure) => Err(Failure::Finished(failure.into_result())),
    }
}

/// The envelope projection of one typed payload document.
fn envelope(name: &str, payload: serde_json::Value, human: String) -> DomainResult {
    let mut value = serde_json::json!({ "status": "valid" });
    value[name] = payload;
    DomainResult::validation(value.to_string(), human, Vec::new())
}

/// The early failure a command can finish with: either a typed diagnostic
/// set for the shared invalid/unsupported-version seam, or a loader/compile
/// `DomainResult` that already owns its status and stream.
enum Failure {
    Set(lekalo_core::diagnostics::DiagnosticSet),
    Finished(DomainResult),
}

impl From<lekalo_core::diagnostics::DiagnosticSet> for Failure {
    fn from(set: lekalo_core::diagnostics::DiagnosticSet) -> Self {
        Self::Set(set)
    }
}

/// The `validate --module-api` integration seam (issue #113): existing
/// validation without these flags preserves the accepted behavior; with
/// the seam, the accepted base validation runs first and the boundary
/// check gates the combined verdict. Strict boundary violations are
/// invalid; a stale checked lock denies with full evidence.
pub struct ValidateSeam {
    pub project: Option<String>,
    pub module: Option<String>,
    pub strict: bool,
    pub attachment: String,
    pub lock: Option<String>,
}

/// Run the explicit validator handoff. The boundary check runs regardless
/// of the base verdict: exact strict boundary violations invalidate the
/// run and take precedence over a base policy denial, a stale checked
/// lock denies with full evidence, and otherwise the base result
/// (including its own strict-profile denial) is returned unchanged.
pub fn run_validate_seam(seam: ValidateSeam) -> DomainResult {
    let base = crate::run_validate(seam.project.clone(), seam.module.clone(), seam.strict, true);
    let execution = || -> Result<DomainResult, Failure> {
        let compilation = compilation(&seam.project)?;
        let attachment = read(&seam.attachment)?;
        let verdict = check(
            &attachment,
            &compilation,
            seam.strict,
            seam.module.as_deref(),
        )?;
        let mut evidence = verdict.evidence.clone();
        let mut lock_stale = false;
        if let Some(lock_path) = &seam.lock {
            // One freshness comparison everywhere (finding 15): the full
            // check_lock comparison, never a digest-only shortcut.
            let supplied = parse_lock(&read(lock_path)?)?;
            let resolved = resolve(&attachment, &compilation)?;
            let producer = supplied.producer.module.clone();
            let current = resolve_lock(&resolved, &compilation, &producer, &supplied.consumer)?;
            let findings = check_lock(&current, &supplied);
            lock_stale = !findings.is_empty();
            evidence.extend(findings.iter().cloned());
        }
        // Strict boundary violations invalidate (finding 8: the gate is
        // seam.strict, never unconditional); the advisory default keeps
        // the base verdict and records the findings as valid evidence.
        if seam.strict
            && evidence
                .iter()
                .any(|finding| is_boundary_rule(finding.rule))
        {
            let set = lekalo_core::module_api::diagnostic::set(&evidence, Status::Invalid)?;
            return Ok(DomainResult::invalid(set));
        }
        if lock_stale {
            let all = lekalo_core::module_api::diagnostic::denied_set(
                &evidence,
                "validate",
                "lock-policy-denied",
            )?;
            return Ok(DomainResult::denied(all));
        }
        if base.status() != Status::Valid {
            return Ok(base);
        }
        // The receipt keeps its accepted shape: the success envelope rides
        // the base validation payload with advisory boundary evidence.
        let set = lekalo_core::module_api::diagnostic::set(&evidence, Status::Valid)?;
        Ok(base_with_diagnostics(base, set.as_slice().to_vec()))
    };
    match execution() {
        Ok(result) => result,
        Err(Failure::Set(set)) => failure(set),
        Err(Failure::Finished(result)) => result,
    }
}

/// Re-emit a valid base result with additional advisory diagnostics.
fn base_with_diagnostics(
    base: DomainResult,
    diagnostics: Vec<lekalo_core::diagnostics::Diagnostic>,
) -> DomainResult {
    match base {
        DomainResult::Valid {
            payload: lekalo_core::result::SuccessPayload::Validation { json, human },
            diagnostics: existing,
        } => {
            let mut merged = existing;
            merged.extend(diagnostics);
            DomainResult::validation(json, human, merged)
        }
        other => other,
    }
}

/// Dispatch one `module api` invocation.
pub fn run(command: ApiCommands) -> DomainResult {
    match execute(command) {
        Ok(result) => result,
        Err(Failure::Set(set)) => failure(set),
        Err(Failure::Finished(result)) => result,
    }
}
fn execute(command: ApiCommands) -> Result<DomainResult, Failure> {
    match command {
        ApiCommands::Check {
            project,
            api,
            module,
            strict,
            check_lock: check_flag,
            lock,
        } => {
            let compilation = compilation(&project)?;
            let attachment = read(&api)?;
            let verdict = check(&attachment, &compilation, strict, module.as_deref())?;
            let mut evidence = verdict.evidence.clone();
            // The optional lock seam: a stale lock under --check is a
            // policy denial (exit 3), never a silent pass.
            let mut lock_name = String::new();
            if check_flag {
                let Some(lock_path) = &lock else {
                    return Err(Failure::Set(invalid("lock-required")));
                };
                // One freshness comparison everywhere (finding 15): the
                // full identity/digest/policy/selection/dependency
                // comparison against the current resolution.
                let supplied = parse_lock(&read(lock_path)?)?;
                let resolved = resolve(&attachment, &compilation)?;
                let producer = supplied.producer.module.clone();
                let current = resolve_lock(&resolved, &compilation, &producer, &supplied.consumer)?;
                let findings = check_lock(&current, &supplied);
                evidence.extend(findings.iter().cloned());
                lock_name = format!("{}::{}", supplied.producer.module, supplied.consumer);
            }
            report(verdict, &evidence, strict, check_flag, &lock_name).map_err(Failure::from)
        }
        ApiCommands::Export {
            module,
            project,
            api,
        } => {
            let compilation = compilation(&project)?;
            let attachment = read(&api)?;
            let resolved = resolve(&attachment, &compilation)?;
            // Certified-bundle rule: an incomplete or illegal public
            // closure refuses even without --strict.
            certified(&attachment, &compilation, &module)?;
            let bundle = module_api::export_public(&resolved, &compilation, &module)?;
            let human = format!(
                "Module API: {} exports for {} (apiVersion {}).",
                bundle.exports.len(),
                module,
                bundle.api_version.as_deref().unwrap_or("digest-only")
            );
            Ok(envelope(
                "modulePublicContract",
                serde_json::to_value(&bundle).expect("typed payload serializes"),
                human,
            ))
        }
        ApiCommands::Consumers {
            module,
            project,
            api,
        } => {
            let compilation = compilation(&project)?;
            let resolved = resolve(&read(&api)?, &compilation)?;
            let snapshot = consumer_snapshot(&resolved, &compilation, &module)?;
            let human = format!(
                "Module API consumers: {} declared/resolved consumers for {}.",
                snapshot.consumers.len(),
                module
            );
            Ok(envelope(
                "moduleApiConsumers",
                serde_json::to_value(&snapshot).expect("typed payload serializes"),
                human,
            ))
        }
        ApiCommands::Lock {
            project,
            api,
            module,
            consumer,
            lock,
            check: check_flag,
        } => {
            let compilation = compilation(&project)?;
            let attachment = read(&api)?;
            let resolved = resolve(&attachment, &compilation)?;
            certified(&attachment, &compilation, &module)?;
            let current = resolve_lock(&resolved, &compilation, &module, &consumer)?;
            if check_flag {
                let Some(lock_path) = &lock else {
                    return Err(Failure::Set(invalid("lock-required")));
                };
                let supplied = parse_lock(&read(lock_path)?)?;
                let findings = check_lock(&current, &supplied);
                if findings.is_empty() {
                    let human = format!("Module API lock: fresh for {consumer}.");
                    return Ok(envelope(
                        "moduleApiLock",
                        serde_json::to_value(&current).expect("typed payload serializes"),
                        human,
                    ));
                }
                // Stale/foreign lock: policy denial with full evidence.
                let detail = findings
                    .iter()
                    .map(|finding| finding.detail.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                let final_set = lekalo_core::module_api::diagnostic::denied_set(
                    &findings,
                    &format!("{module}::{consumer}"),
                    if detail.is_empty() {
                        "lock-policy"
                    } else {
                        &detail
                    },
                )?;
                let payload = serde_json::json!({
                    "status": "denied",
                    "moduleApiLock": current,
                });
                return Ok(DomainResult::denied_json(
                    payload.to_string(),
                    format!("Module API lock: denied ({detail})."),
                    final_set,
                ));
            }
            let human = format!("Module API lock candidate for {consumer} (stdout only).");
            Ok(envelope(
                "moduleApiLock",
                serde_json::to_value(&current).expect("typed payload serializes"),
                human,
            ))
        }
        ApiCommands::Diff {
            base,
            candidate,
            consumers_before,
            consumers_after,
            strict,
        } => {
            let before_bytes = read(&base)?;
            let after_bytes = read(&candidate)?;
            let before_snapshot = consumers_before.map(|path| read(&path)).transpose()?;
            let after_snapshot = consumers_after.map(|path| read(&path)).transpose()?;
            let (report, evidence) = compare_public(
                &before_bytes,
                &after_bytes,
                before_snapshot.as_deref(),
                after_snapshot.as_deref(),
            )?;
            let human = format!(
                "Module API diff: {} changes, verdict {}.",
                report.changes.len(),
                report.verdict
            );
            // Strict denies both an incompatible consumer resolution and a
            // bare version-policy violation (finding 10: the gate is
            // toothless without consumer snapshots otherwise).
            if strict
                && (report.verdict == "policy-denied"
                    || report.version_policy.decision == "violation")
            {
                let detail = evidence
                    .iter()
                    .map(|finding| finding.detail.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                let denied = lekalo_core::module_api::diagnostic::denied_set(
                    &evidence,
                    &report.after.module,
                    if detail.is_empty() {
                        "diff-policy"
                    } else {
                        &detail
                    },
                )?;
                let mut payload =
                    serde_json::json!({ "status": "denied", "moduleApiDiff": report });
                payload["status"] = "denied".into();
                return Ok(DomainResult::denied_json(
                    payload.to_string(),
                    human,
                    denied,
                ));
            }
            Ok(envelope(
                "moduleApiDiff",
                serde_json::to_value(&report).expect("typed payload serializes"),
                human,
            ))
        }
        ApiCommands::Context {
            public,
            consumer,
            uses,
            budget,
        } => {
            let selected = uses
                .split(',')
                .map(|use_id| use_id.trim().to_owned())
                .filter(|use_id| !use_id.is_empty())
                .collect::<Vec<_>>();
            if selected.is_empty() {
                return Err(Failure::Set(invalid("uses-required")));
            }
            let capsule = plan_consumer_context(&read(&public)?, &consumer, &selected, budget)?;
            let human = format!(
                "Module API context: {} selected exports, fits {} (required {} of {}).",
                capsule.exports.len(),
                capsule.fits,
                capsule.required_cost,
                budget
            );
            Ok(envelope(
                "moduleConsumerContext",
                serde_json::to_value(&capsule).expect("typed payload serializes"),
                human,
            ))
        }
    }
}

/// Project the check verdict into the typed result: strict violations are
/// invalid, required lock-check policy failures deny, advisory reports
/// stay valid with the full evidence retained.
fn report(
    verdict: module_api::Verdict,
    evidence: &[lekalo_core::module_api::diagnostic::Evidence],
    strict: bool,
    lock_check: bool,
    lock_name: &str,
) -> Result<DomainResult, lekalo_core::diagnostics::DiagnosticSet> {
    let human = format!(
        "Module API check: {} findings, assessment {}.",
        verdict.report.findings.len(),
        verdict.report.assessment
    );
    // The denied payload carries its denied status: the embedded report
    // never claims valid on a denied run (finding 15).
    let mut payload = serde_json::json!({ "status": "valid", "moduleApiReport": verdict.report });
    match verdict.class {
        FindingClass::Invalid => {
            let set = lekalo_core::module_api::diagnostic::set(evidence, Status::Invalid)?;
            Ok(DomainResult::invalid(set))
        }
        FindingClass::Evidence if !verdict.report.complete && strict => {
            let set = lekalo_core::module_api::diagnostic::set(evidence, Status::Unavailable)?;
            Ok(DomainResult::unavailable(set))
        }
        _ if lock_check
            && evidence
                .iter()
                .any(|finding| finding.rule == "module-api.lock-stale") =>
        {
            let all = lekalo_core::module_api::diagnostic::denied_set(
                evidence,
                lock_name,
                "lock-policy-denied",
            )?;
            payload["status"] = "denied".into();
            Ok(DomainResult::denied_json(payload.to_string(), human, all))
        }
        _ => {
            let set = lekalo_core::module_api::diagnostic::set(evidence, Status::Valid)?;
            Ok(DomainResult::validation(
                payload.to_string(),
                human,
                set.as_slice().to_vec(),
            ))
        }
    }
}
