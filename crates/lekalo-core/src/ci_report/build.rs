//! The CI report builders (issue #103).
//!
//! One typed [`CommandOutcome`] crosses the CLI seam: the original
//! `DomainResult`, the check/suite rows captured before failure
//! aggregation, and the provenance snapshot. [`build`] derives the
//! closed report from it — the underlying command result is preserved
//! verbatim while the policy evaluation owns the effective exit.

use crate::diagnostics::Diagnostic;
use crate::result::DomainResult;

use super::model::{
    CaseRow, CheckRow, CiReport, CommandName, Coverage, EffectiveOutcome, Evaluation, FailureClass,
    Invocation, Outcome, Producer, Provenance, Publication, PublicationDecision,
    SerializableDiagnostic, SourceOutcome, Suite, Verdict,
};

/// The closed CI policy vocabulary (the built-in default; the research
/// policy file is a later, separately versioned contract). `default`
/// fails every required check and warns optional absences; `strict`
/// promotes optional absences to errors; `lenient` skips them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CiPolicy {
    /// Required failures fail; optional unavailable rows warn (exit 0
    /// with incomplete coverage).
    Default,
    /// Optional unavailable rows are promoted to errors (exit 4).
    Strict,
    /// Optional unavailable rows are skipped silently (exit 0).
    Lenient,
}

impl CiPolicy {
    /// Parse the closed policy token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "default" => Some(Self::Default),
            "strict" => Some(Self::Strict),
            "lenient" => Some(Self::Lenient),
            _ => None,
        }
    }

    /// The stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Strict => "strict",
            Self::Lenient => "lenient",
        }
    }
}

/// One typed command outcome: everything the report needs, captured at
/// the command boundary before any envelope aggregation discards it.
pub struct CommandOutcome {
    /// The closed command name.
    pub command: CommandName,
    /// The closed mode token.
    pub mode: String,
    /// The selected targets (sorted, deduplicated).
    pub targets: Vec<String>,
    /// The selected modules (sorted, deduplicated).
    pub modules: Vec<String>,
    /// Whether the run demanded the full locked inventory.
    pub locked: bool,
    /// The provenance snapshot.
    pub provenance: Provenance,
    /// The check drafts in any order; the builder sorts and applies the
    /// policy.
    pub checks: Vec<CheckDraft>,
    /// The suite drafts in any order; the builder sorts and applies the
    /// policy.
    pub suites: Vec<SuiteDraft>,
    /// The terminal domain result of the command itself.
    pub result: DomainResult,
    /// The explicit evaluated policy date (`YYYY-MM-DD`), when the run
    /// evaluated one.
    pub as_of: Option<String>,
}

/// A draft check row under construction; the policy pass fills the
/// effective outcome.
pub struct CheckDraft {
    /// The stable check id.
    pub id: String,
    /// Whether this check is required.
    pub required: bool,
    /// What the source observed.
    pub source_outcome: SourceOutcome,
    /// The failure class.
    pub failure_class: FailureClass,
    /// Sorted diagnostic indexes.
    pub diagnostic_indexes: Vec<usize>,
    /// The closed reason token.
    pub detail: String,
}

impl CheckDraft {
    /// One passing check row.
    pub fn pass(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            required: true,
            source_outcome: SourceOutcome::Pass,
            failure_class: FailureClass::None,
            diagnostic_indexes: Vec::new(),
            detail: String::new(),
        }
    }
}

/// A draft case row under construction.
pub struct CaseDraft {
    /// The stable case id.
    pub id: String,
    /// Whether this case is required.
    pub required: bool,
    /// What the source observed.
    pub source_outcome: SourceOutcome,
    /// The failure class.
    pub failure_class: FailureClass,
    /// Sorted diagnostic indexes.
    pub diagnostic_indexes: Vec<usize>,
    /// The closed detail token.
    pub detail: String,
}

impl CaseDraft {
    /// One passing case row.
    pub fn pass(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            required: true,
            source_outcome: SourceOutcome::Pass,
            failure_class: FailureClass::None,
            diagnostic_indexes: Vec::new(),
            detail: String::new(),
        }
    }
}

/// A draft suite under construction.
pub struct SuiteDraft {
    /// The stable suite id.
    pub id: String,
    /// The closed suite kind.
    pub kind: super::model::SuiteKind,
    /// The scoped target.
    pub target: Option<String>,
    /// The case rows in execution order.
    pub cases: Vec<CaseDraft>,
}

/// Apply the shared policy table to one check draft.
pub fn apply_check_policy(draft: CheckDraft, policy: CiPolicy) -> CheckRow {
    let table = super::build_policy::CiPolicyTable::of(policy);
    CheckRow {
        effective_outcome: super::model::evaluate_outcome(
            draft.required,
            draft.source_outcome,
            draft.failure_class,
            table,
        ),
        id: draft.id,
        required: draft.required,
        source_outcome: draft.source_outcome,
        failure_class: draft.failure_class,
        diagnostic_indexes: draft.diagnostic_indexes,
        detail: draft.detail,
    }
}

/// Apply the same shared policy table to one case draft: the evaluated
/// outcome is computed once here and serialized on the row, so the
/// JUnit projection and the evaluation verdict can never disagree
/// (review F7).
pub fn apply_case_policy(draft: CaseDraft, policy: CiPolicy) -> CaseRow {
    let table = super::build_policy::CiPolicyTable::of(policy);
    CaseRow {
        effective_outcome: super::model::evaluate_outcome(
            draft.required,
            draft.source_outcome,
            draft.failure_class,
            table,
        ),
        id: draft.id,
        required: draft.required,
        source_outcome: draft.source_outcome,
        failure_class: draft.failure_class,
        diagnostic_indexes: draft.diagnostic_indexes,
        detail: draft.detail,
    }
}

/// Derive the evaluation from the underlying result plus the applied
/// rows. The producing CLI exits with the evaluation; the underlying
/// command result is preserved alongside it.
fn evaluate(
    command_result: &DomainResult,
    checks: &[CheckRow],
    suites: &[Suite],
    _policy: CiPolicy,
) -> Evaluation {
    // The command's own failure is terminal evidence even when the rows
    // are empty (review F8): a preflight refusal (missing lock, loader
    // refusal, usage) is a blocking, incomplete evaluation — never a
    // false-ready verdict.
    let command_failed = command_result.exit_code() != 0;
    let blocking = command_failed
        || checks
            .iter()
            .map(|row| row.effective_outcome)
            .chain(suites.iter().flat_map(|suite| {
                suite
                    .cases
                    .iter()
                    .map(|case: &CaseRow| case.effective_outcome)
            }))
            .any(|outcome| !outcome.is_exit_neutral());
    let degraded_only = checks
        .iter()
        .map(|row| row.effective_outcome)
        .chain(suites.iter().flat_map(|suite| {
            suite
                .cases
                .iter()
                .map(|case: &CaseRow| case.effective_outcome)
        }))
        .any(|outcome| matches!(outcome, EffectiveOutcome::Warn | EffectiveOutcome::Skip));
    let complete = !command_failed
        && !checks.iter().any(|row| {
            matches!(
                (row.source_outcome, row.effective_outcome),
                (SourceOutcome::NotRun, _)
                    | (SourceOutcome::Unavailable, EffectiveOutcome::Skip)
                    | (SourceOutcome::Unavailable, EffectiveOutcome::Warn)
                    | (SourceOutcome::Unsupported, EffectiveOutcome::Skip)
                    | (SourceOutcome::Unsupported, EffectiveOutcome::Warn)
                    | (SourceOutcome::Cancelled, _)
            )
        })
        && !suites
            .iter()
            .flat_map(|suite| suite.cases.iter())
            .any(|case| {
                matches!(
                    (case.source_outcome, case.effective_outcome),
                    (SourceOutcome::NotRun, _)
                        | (SourceOutcome::Unavailable, EffectiveOutcome::Skip)
                        | (SourceOutcome::Unavailable, EffectiveOutcome::Warn)
                        | (SourceOutcome::Unsupported, EffectiveOutcome::Skip)
                        | (SourceOutcome::Unsupported, EffectiveOutcome::Warn)
                        | (SourceOutcome::Cancelled, _)
                )
            });
    let coverage = if complete {
        Coverage::Complete
    } else if !checks.is_empty() || !suites.is_empty() {
        Coverage::Incomplete
    } else {
        Coverage::Unknown
    };
    let verdict = if blocking {
        Verdict::Blocked
    } else if degraded_only {
        Verdict::Degraded
    } else {
        Verdict::Ready
    };
    // The effective status: a blocking evaluation keeps the underlying
    // failure class (never downgraded); a non-blocking evaluation is the
    // command's own result (valid 0 for ready/degraded-success runs,
    // unless the command itself failed for reasons outside the checks).
    let (status, exit_code) = if blocking {
        // The worst class across the failing rows and the command
        // result: a security/denied row keeps its denied class even
        // when the command result was invalid; policy promotion of an
        // optional row uses the unavailable class; everything else
        // invalid-classed.
        let command_status = command_result.status();
        let any_denied = checks
            .iter()
            .any(|row| row.source_outcome == super::model::SourceOutcome::Denied)
            || suites
                .iter()
                .flat_map(|suite| suite.cases.iter())
                .any(|case| case.source_outcome == super::model::SourceOutcome::Denied);
        let promoted = checks
            .iter()
            .any(|row| row.effective_outcome == super::model::EffectiveOutcome::Error)
            || suites
                .iter()
                .flat_map(|suite| suite.cases.iter())
                .any(|case| case.effective_outcome == super::model::EffectiveOutcome::Error);
        if any_denied {
            (crate::result::Status::Denied, 3u8)
        } else if command_status == crate::result::Status::Valid {
            if promoted {
                (crate::result::Status::Unavailable, 4u8)
            } else {
                (crate::result::Status::Invalid, 1)
            }
        } else {
            (command_status, command_status.exit_code())
        }
    } else {
        let status = command_result.status();
        (status, status.exit_code())
    };
    Evaluation {
        status: status.as_str(),
        exit_code,
        verdict,
        coverage,
        complete,
    }
}

/// Build the closed report from one typed outcome. Checks and suites
/// are sorted into their canonical id order here; diagnostics are
/// deduplicated by (index) reference and bounded to the set limit.
pub fn build(outcome: CommandOutcome, policy: CiPolicy) -> CiReport {
    let mut checks: Vec<CheckRow> = outcome
        .checks
        .into_iter()
        .map(|draft| apply_check_policy(draft, policy))
        .collect();
    checks.sort_by(|left, right| left.id.cmp(&right.id));
    let mut suites: Vec<Suite> = outcome
        .suites
        .into_iter()
        .map(|draft| Suite {
            id: draft.id,
            kind: draft.kind,
            target: draft.target,
            cases: draft
                .cases
                .into_iter()
                .map(|case| apply_case_policy(case, policy))
                .collect(),
        })
        .collect();
    suites.sort_by(|left, right| left.id.cmp(&right.id));
    let evaluation = evaluate(&outcome.result, &checks, &suites, policy);
    let diagnostic_indexes: Vec<usize> = checks
        .iter()
        .flat_map(|check| check.diagnostic_indexes.iter().copied())
        .chain(
            suites
                .iter()
                .flat_map(|suite| suite.cases.iter())
                .flat_map(|case| case.diagnostic_indexes.iter().copied()),
        )
        .collect::<std::collections::BTreeSet<usize>>()
        .into_iter()
        .collect();
    CiReport {
        schema_version: super::version::SCHEMA_VERSION,
        identity: super::version::IDENTITY,
        producer: Producer {
            id: "lekalo",
            version: super::version::REPORT_VERSION,
        },
        invocation: Invocation {
            command: outcome.command,
            mode: outcome.mode,
            targets: outcome.targets,
            modules: outcome.modules,
            locked: outcome.locked,
            as_of: outcome.as_of,
        },
        provenance: outcome.provenance,
        command_result: Outcome::of(outcome.result.status()),
        evaluation,
        checks,
        suites,
        diagnostic_indexes,
        diagnostics: Vec::new(),
        publication: Publication {
            classification: "ci-derived",
            decision: PublicationDecision::Allowed,
        },
    }
}

/// Attach the diagnostics the rows reference. The indexes were assigned
/// by the caller against this same normalized order.
pub fn with_diagnostics(mut report: CiReport, diagnostics: Vec<Diagnostic>) -> CiReport {
    report.diagnostics = diagnostics
        .into_iter()
        .take(super::version::MAX_DIAGNOSTICS)
        .map(SerializableDiagnostic::new)
        .collect();
    report
}

/// The registered CI diagnostic constructors, for the CLI edge: the
/// `ci.*` rules with bounded safe detail tokens (never a rejected
/// value, path, or raw IO text). A registry failure collapses to the
/// registry-invariant singleton, like every other producer seam.
pub mod diagnostics {
    /// One bounded, control-cleaned token data value.
    fn token(text: &str) -> crate::diagnostics::DataValue {
        crate::diagnostics::types::token_value(text)
    }

    /// Build one registered diagnostic or collapse to the invariant
    /// singleton set under its status.
    fn set(
        status: crate::result::Status,
        built: Result<crate::diagnostics::Diagnostic, crate::diagnostics::normalize::BuildError>,
    ) -> crate::diagnostics::DiagnosticSet {
        match built {
            Ok(diagnostic) => {
                crate::diagnostics::DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
                    .unwrap_or_else(|_| {
                        crate::diagnostics::DiagnosticSet::try_from_unsorted(Vec::new(), status)
                            .unwrap_or_else(|_| crate::diagnostics::DiagnosticSet::empty())
                    })
            }
            Err(_) => {
                // The registry-invariant collapse: an empty set of the
                // requested status (double developer fault never
                // panics the CLI edge).
                crate::diagnostics::DiagnosticSet::try_from_unsorted(Vec::new(), status)
                    .unwrap_or_else(|_| crate::diagnostics::DiagnosticSet::empty())
            }
        }
    }

    /// `ci.report-write-failed`: the granted report destination refused
    /// the write. `detail` is one closed token (`directory-missing`,
    /// `path-invalid`, `write-denied`), never a host path.
    pub fn report_write_failed(detail: &str) -> crate::diagnostics::DiagnosticSet {
        let mut data = crate::diagnostics::DataObject::new();
        data.insert("detail".to_owned(), token(detail));
        set(
            crate::result::Status::Unavailable,
            crate::diagnostics::normalize::build("ci.report-write-failed", None, None, data),
        )
    }

    /// `ci.required-check-missing`: a check the policy requires never
    /// reached a terminal evaluation. `check` is the closed check id.
    pub fn required_check_missing(check: &str, detail: &str) -> crate::diagnostics::DiagnosticSet {
        let mut data = crate::diagnostics::DataObject::new();
        data.insert("check".to_owned(), token(check));
        data.insert("detail".to_owned(), token(detail));
        set(
            crate::result::Status::Unavailable,
            crate::diagnostics::normalize::build("ci.required-check-missing", None, None, data),
        )
    }
}

/// The closed secret-refusal vocabulary: what the pre-publication scan
/// found in the rendered report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretRefusal {
    /// A secret-shaped token survived into the rendered bytes.
    SecretToken,
}

impl SecretRefusal {
    /// The stable detail spelling (the closed refusal reason).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SecretToken => "secret-token",
        }
    }
}

/// Scan one rendered report projection for secret material before it
/// may be written or published (issue #103 privacy rule: redaction and
/// admission run before every sink). The accepted #119 leak scanner is
/// the authority; a `secret-token` finding refuses the sink. Returns
/// the class of the first blocking finding, or `None` when the bytes
/// carry no secret material.
pub fn scan_rendered(rendered: &str) -> Option<SecretRefusal> {
    let findings = crate::privacy::redact::scan(rendered);
    if findings
        .iter()
        .any(|finding| finding.kind() == crate::privacy::redact::LeakKind::SecretToken)
    {
        return Some(SecretRefusal::SecretToken);
    }
    None
}
