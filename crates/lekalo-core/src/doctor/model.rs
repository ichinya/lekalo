//! The closed doctor/readiness wire model (issue #92).
//!
//! One document serves `doctor`, `status`, and `readiness`: the report
//! kind selects the check panel, the readiness phase selects the
//! required/optional split. Field order below is normative wire order;
//! checks are sorted by id; the verdict is derived, never hand-set.
//! No absolute paths, host identity, environment values, secret material,
//! or free-form text ever appears: every reason and next action is a
//! closed token, and diagnostics carry registry rule ids only.

use serde::Serialize;

use super::version::{IDENTITY, SCHEMA_VERSION};

/// The closed report-kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportKind {
    /// `lekalo doctor`: every check.
    Doctor,
    /// `lekalo status`: the freshness panel plus revisions.
    Status,
    /// `lekalo readiness --phase PHASE`: the phase-gated panel.
    Readiness,
}

impl ReportKind {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::Status => "status",
            Self::Readiness => "readiness",
        }
    }
}

/// The closed readiness-phase vocabulary (`done` is an accepted CLI alias
/// of `release` and never appears on the wire).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Phase {
    /// Model loads and validates.
    Model,
    /// Implementation against pinned inputs.
    Implement,
    /// Generation against resolved adapters and profiles.
    Generate,
    /// Verification with available tools and clean artifacts.
    Verify,
    /// Release/done: the full gate.
    Release,
}

impl Phase {
    /// Parse the closed wire spelling; `done` normalizes to `release`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "model" => Some(Self::Model),
            "implement" => Some(Self::Implement),
            "generate" => Some(Self::Generate),
            "verify" => Some(Self::Verify),
            "release" | "done" => Some(Self::Release),
            _ => None,
        }
    }

    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Implement => "implement",
            Self::Generate => "generate",
            Self::Verify => "verify",
            Self::Release => "release",
        }
    }

    /// The checks this phase requires to be free of blockers.
    pub(crate) fn required_checks(self) -> &'static [&'static str] {
        match self {
            Self::Model => &[
                "fs.confinement",
                "model.references",
                "model.version",
                "project.root",
            ],
            Self::Implement => &[
                "fs.confinement",
                "lock.freshness",
                "model.references",
                "model.version",
                "project.root",
            ],
            Self::Generate => &[
                "adapters.inventory",
                "artifacts.drift",
                "bindings.freshness",
                "capabilities.profiles",
                "fs.confinement",
                "lock.freshness",
                "model.references",
                "model.version",
                "project.root",
            ],
            Self::Verify | Self::Release => &[
                "adapters.inventory",
                "artifacts.drift",
                "bindings.freshness",
                "capabilities.profiles",
                "fs.confinement",
                "lock.freshness",
                "model.references",
                "model.version",
                "project.root",
                "tools.gates",
            ],
        }
    }
}

/// The derived verdict of one report.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Every check is `ok`.
    Ready,
    /// No required check is blocked or unknown; at least one check is not
    /// `ok`. Missing optional evidence (for example HLV) degrades here and
    /// is never a core failure.
    Degraded,
    /// A required check is blocked or unknown, or an optional check is
    /// blocked.
    Blocked,
}

/// The closed check-state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    /// The check found nothing to report.
    Ok,
    /// The check found an actionable, non-blocking condition.
    Degraded,
    /// The check found a blocking condition.
    Blocked,
    /// The check could not run because an upstream input was unavailable.
    Unknown,
}

/// The closed Git revision state handed over by the read-only CLI adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitState {
    /// Git is available and the project root is a working tree.
    Available,
    /// Git is not installed or could not be executed.
    Unavailable,
    /// Git runs but the project root is not a repository.
    NotARepository,
}

/// The exact revision facts of one report (issue #92 requirement: the
/// exact git/model/lock revision; nothing more about the host).
#[derive(Clone, Debug, Serialize)]
pub struct Revisions {
    pub git: GitRevision,
    #[serde(rename = "model")]
    pub model: ModelRevision,
    #[serde(rename = "lock")]
    pub lock: LockRevision,
}

/// The Git revision panel: the exact commit identity when available.
#[derive(Clone, Debug, Serialize)]
pub struct GitRevision {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
}

/// The Model revision panel.
#[derive(Clone, Debug, Serialize)]
pub struct ModelRevision {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The lock revision panel.
#[derive(Clone, Debug, Serialize)]
pub struct LockRevision {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// One closed check result.
#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub state: CheckState,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    /// The closed safe-fix recipe id; present exactly when the state is
    /// not `ok`. Advice only: doctor never executes a recipe.
    #[serde(rename = "nextAction", skip_serializing_if = "Option::is_none")]
    pub next_action: Option<&'static str>,
    /// Preserved registry rule ids of the underlying refusal, normalized,
    /// deduplicated, and bounded.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Closed informational tokens (platform facts, external-ref
    /// coverage); never free-form text, never a finding.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<&'static str>,
}

/// One safe-fix recipe of the `--fix` preview. Recipes are advice: the
/// mutating ones name the exact explicit CLI confirmation the caller must
/// run itself; doctor executes nothing.
#[derive(Clone, Debug, Serialize)]
pub struct Recipe {
    pub id: &'static str,
    pub mutating: bool,
    pub steps: Vec<&'static str>,
}

/// The typed Git facts handed over by the read-only CLI adapter: the
/// exact commit identity when available, never repository details.
#[derive(Clone, Debug)]
pub struct GitFacts {
    pub state: GitState,
    pub commit: Option<String>,
    pub dirty: Option<bool>,
}

/// One supplied trace manifest's typed evidence, built at the CLI edge
/// through the core's own manifest parser; paths never cross this seam.
#[derive(Clone, Debug, Default)]
pub struct TraceManifestEvidence {
    /// Empty when the manifest parsed and validated.
    pub reason_ids: Vec<String>,
    /// The manifest's recorded gaps (degraded evidence, not failures).
    pub gaps: usize,
    /// The sorted unique externalRef source systems it cites.
    pub external_refs: Vec<String>,
}

/// The top-level doctor/readiness document. `status` is part of the
/// document (receipt house style): the report is the product, emitted
/// `valid` whenever it was produced.
#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub status: &'static str,
    #[serde(rename = "schemaVersion")]
    pub schema_version: &'static str,
    pub identity: &'static str,
    pub report: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<&'static str>,
    #[serde(rename = "productVersion")]
    pub product_version: String,
    pub verdict: Verdict,
    pub revisions: Revisions,
    pub checks: Vec<Check>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipes: Option<Vec<Recipe>>,
}

impl DoctorReport {
    /// Assemble the document: checks must already be sorted by id; the
    /// verdict is derived from the required split of the selected panel.
    pub(crate) fn finalize(
        kind: ReportKind,
        phase: Option<Phase>,
        product_version: &str,
        revisions: Revisions,
        mut checks: Vec<Check>,
    ) -> Self {
        checks.sort_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));
        let verdict = derive_verdict(kind, phase, &checks);
        Self {
            status: "valid",
            schema_version: SCHEMA_VERSION,
            identity: IDENTITY,
            report: kind.as_str(),
            phase: phase.map(Phase::as_str),
            product_version: product_version.to_owned(),
            verdict,
            revisions,
            checks,
            recipes: None,
        }
    }
}

/// The verdict rule (ADR-0032): `doctor` and `readiness` block on any
/// required check that is blocked or unknown; `status` is informational
/// and blocks only on blocked checks. Any non-`ok` check degrades.
fn derive_verdict(kind: ReportKind, phase: Option<Phase>, checks: &[Check]) -> Verdict {
    let mut degraded = false;
    for check in checks {
        let relevant = match (kind, phase) {
            (ReportKind::Status, _) => check.state == CheckState::Blocked,
            (_, Some(phase)) => {
                phase.required_checks().contains(&check.id)
                    && matches!(check.state, CheckState::Blocked | CheckState::Unknown)
            }
            (_, None) => matches!(check.state, CheckState::Blocked | CheckState::Unknown),
        };
        if relevant {
            return Verdict::Blocked;
        }
        if check.state != CheckState::Ok {
            degraded = true;
        }
    }
    if degraded {
        Verdict::Degraded
    } else {
        Verdict::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(id: &'static str, state: CheckState, required: bool) -> Check {
        Check {
            id,
            state,
            required,
            reason: None,
            next_action: None,
            diagnostics: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[test]
    fn phase_aliases_and_spellings_are_closed() {
        assert_eq!(Phase::parse("done"), Some(Phase::Release));
        assert_eq!(Phase::parse("release").map(Phase::as_str), Some("release"));
        assert_eq!(Phase::parse("verify").map(Phase::as_str), Some("verify"));
        assert_eq!(Phase::parse("ship"), None);
    }

    #[test]
    fn verdict_blocked_on_required_only() {
        // An optional degraded check never blocks; a required one does.
        let optional = vec![check("cache.health", CheckState::Degraded, false)];
        assert_eq!(
            derive_verdict(ReportKind::Readiness, Some(Phase::Model), &optional),
            Verdict::Degraded
        );
        let mut required = optional.clone();
        required.push(check("model.version", CheckState::Blocked, true));
        assert_eq!(
            derive_verdict(ReportKind::Readiness, Some(Phase::Model), &required),
            Verdict::Blocked
        );
    }

    #[test]
    fn status_blocks_only_on_blocked_checks() {
        let checks = vec![check("lock.freshness", CheckState::Degraded, false)];
        assert_eq!(
            derive_verdict(ReportKind::Status, None, &checks),
            Verdict::Degraded
        );
    }

    #[test]
    fn doctor_blocks_on_any_blocked_or_unknown_check() {
        let checks = vec![check("cache.health", CheckState::Unknown, false)];
        assert_eq!(
            derive_verdict(ReportKind::Doctor, None, &checks),
            Verdict::Blocked
        );
    }

    #[test]
    fn finalize_sorts_checks_by_id() {
        let checks = vec![
            check("project.root", CheckState::Ok, true),
            check("artifacts.drift", CheckState::Ok, false),
        ];
        let report = DoctorReport::finalize(
            ReportKind::Doctor,
            None,
            "0.0.0",
            Revisions {
                git: GitRevision {
                    state: "unavailable",
                    commit: None,
                    dirty: None,
                },
                model: ModelRevision {
                    state: "unknown",
                    version: None,
                },
                lock: LockRevision {
                    state: "absent",
                    digest: None,
                },
            },
            checks,
        );
        assert_eq!(
            report.checks.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec!["artifacts.drift", "project.root"]
        );
    }
}
