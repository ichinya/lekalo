//! Issue #92: `lekalo doctor`, `lekalo status`, and `lekalo readiness`.
//!
//! One command diagnoses the readiness of a Lekalo project — root and
//! layout, Model/schema/IR version compatibility, imports and references,
//! lockfile freshness, installed adapter versions and digests, capability
//! and profile resolution, cache health, binding and index freshness,
//! generated artifact drift, native tool availability, optional
//! OpenSpec/HLV/AI Factory integration evidence, filesystem permissions
//! and path confinement, and platform limitations — before planning,
//! implementation, generation, verification, or release.
//!
//! Doctor is read-only by default and forever: every internal load runs
//! with the cache bypassed, nothing spawns adapters, nothing repairs,
//! installs, or updates. `--fix` only renders the closed safe-fix recipes
//! as a preview. The result is one versioned document
//! (`lekalo/doctor/v1.0.0`); the envelope stays `valid` whenever the
//! report was produced — findings live in the report, the stable exit
//! policy is documented in `docs/doctor.md`. Underlying provider, loader,
//! lock, and adapter refusals are preserved as registry rule ids; no
//! secret, environment value, host identity, or absolute path ever
//! appears.

pub mod checks;
pub mod model;
pub mod version;

use crate::loader::LoadSelection;
use crate::result::DomainResult;
use model::{DoctorReport, Revisions};

pub use model::{GitFacts, GitState, Phase, ReportKind, TraceManifestEvidence};

/// The report-selection options of one doctor run.
#[derive(Clone, Debug)]
pub struct Options {
    pub kind: ReportKind,
    pub phase: Option<Phase>,
    pub fix: bool,
    pub traces: Vec<TraceManifestEvidence>,
}

/// Resolve the project root for one selection. The CLI edge uses the
/// resolved path only to drive its read-only Git adapter; the root is
/// never echoed.
pub fn project_root(selection: &LoadSelection) -> Result<std::path::PathBuf, DomainResult> {
    crate::loader::root_for_selection(selection)
}

impl Default for Options {
    fn default() -> Self {
        Self {
            kind: ReportKind::Doctor,
            phase: None,
            fix: false,
            traces: Vec::new(),
        }
    }
}

/// The one entry point behind `doctor`, `status`, and `readiness`.
///
/// The report is the product: it is emitted `valid` whenever it was
/// produced, whatever verdict it records. Only a malformed invocation
/// (usage) fails the envelope; path-confinement refusals surface inside
/// the report as blocked checks with their preserved rule ids.
pub fn report(selection: &LoadSelection, git: &GitFacts, options: &Options) -> DomainResult {
    let doc = build(selection, git, options);
    let json = serde_json::to_string_pretty(&doc).expect("the doctor document serializes");
    DomainResult::receipt(json, human_summary(options.kind, options.phase, &doc))
}

/// Assemble the typed document behind [`report`]; the entry point for the
/// typed assertions of the test suite.
pub(crate) fn build(selection: &LoadSelection, git: &GitFacts, options: &Options) -> DoctorReport {
    let (project_check, root) = checks::project_root(selection);
    let structure = match &root {
        Some(root) => crate::project_fs::Fs::validate_project(root),
        None => crate::project_fs::StructureOutcome::Invalid(Vec::new()),
    };
    let targets_declared = match &structure {
        crate::project_fs::StructureOutcome::Valid(report) => !report.targets.is_empty(),
        _ => false,
    };
    let model = checks::model_checks(selection);
    let facts = match &root {
        Some(root) => checks::lock_facts(selection, root),
        None => checks::LockFacts::none(),
    };
    let panel = checks::panel(&checks::PanelContext {
        selection,
        project_check,
        root: root.as_deref(),
        structure: &structure,
        model: &model,
        facts: &facts,
        git,
        traces: &options.traces,
        targets_declared,
        kind: options.kind,
        phase: options.phase,
    });
    let revisions = Revisions {
        git: model::GitRevision {
            state: git_state_spelling(git.state),
            commit: git.commit.clone(),
            dirty: git.dirty,
        },
        model: model::ModelRevision {
            state: match &model.model_version {
                Some(_) => "ok",
                None => "unknown",
            },
            version: model.model_version.clone(),
        },
        lock: facts.revision.clone(),
    };
    let mut doc = DoctorReport::finalize(
        options.kind,
        options.phase,
        crate::lockfile::PRODUCT_VERSION,
        revisions,
        panel,
    );
    if options.fix {
        let mut recipes = Vec::new();
        for check in &doc.checks {
            let Some(id) = check.next_action else {
                continue;
            };
            if recipes.iter().any(|recipe: &model::Recipe| recipe.id == id) {
                continue;
            }
            if let Some((id, mutating, steps)) = checks::recipe(id) {
                recipes.push(model::Recipe {
                    id,
                    mutating,
                    steps,
                });
            }
        }
        doc.recipes = Some(recipes);
    }
    doc
}

fn git_state_spelling(state: model::GitState) -> &'static str {
    match state {
        model::GitState::Available => "ok",
        model::GitState::Unavailable => "unavailable",
        model::GitState::NotARepository => "not-a-repository",
    }
}

/// The stable single-line human summary of one report.
fn human_summary(kind: ReportKind, phase: Option<Phase>, report: &DoctorReport) -> String {
    let count = |state: model::CheckState| {
        report
            .checks
            .iter()
            .filter(|check| check.state == state)
            .count()
    };
    match (kind, phase) {
        (ReportKind::Status, _) => format!(
            "status {} : lock {}, cache {}, bindings {}, artifacts {}",
            verdict_spelling(report.verdict),
            report.revisions.lock.state,
            check_word(&report.checks, "cache.health"),
            check_word(&report.checks, "bindings.freshness"),
            check_word(&report.checks, "artifacts.drift"),
        ),
        (ReportKind::Readiness, Some(phase)) => format!(
            "readiness {} {} : {} checks ({} ok, {} degraded, {} blocked)",
            phase.as_str(),
            verdict_spelling(report.verdict),
            report.checks.len(),
            count(model::CheckState::Ok),
            count(model::CheckState::Degraded),
            count(model::CheckState::Blocked) + count(model::CheckState::Unknown),
        ),
        _ => format!(
            "doctor {} : {} checks ({} ok, {} degraded, {} blocked)",
            verdict_spelling(report.verdict),
            report.checks.len(),
            count(model::CheckState::Ok),
            count(model::CheckState::Degraded),
            count(model::CheckState::Blocked) + count(model::CheckState::Unknown),
        ),
    }
}

fn verdict_spelling(verdict: model::Verdict) -> &'static str {
    match verdict {
        model::Verdict::Ready => "ready",
        model::Verdict::Degraded => "degraded",
        model::Verdict::Blocked => "blocked",
    }
}

fn check_word(checks: &[model::Check], id: &str) -> &'static str {
    checks
        .iter()
        .find(|check| check.id == id)
        .and_then(|check| {
            check.reason.or(match check.state {
                model::CheckState::Ok => Some("ok"),
                model::CheckState::Degraded => Some("degraded"),
                model::CheckState::Blocked => Some("blocked"),
                model::CheckState::Unknown => Some("unknown"),
            })
        })
        .unwrap_or("unknown")
}

#[cfg(test)]
mod tests;
