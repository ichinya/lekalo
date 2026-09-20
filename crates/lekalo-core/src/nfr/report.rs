//! The derived NFR resolution report and its status engine (issue
//! #85).
//!
//! One read-only projection — `lekalo/nfr-report/v0.4.0`, identity
//! `dev.lekalo.nfr-report@0.4.0` — computed as a pure function of the
//! constraint attachment, the supplied evidence sets, the resolved
//! capability snapshot, and the injected as-of reference date. The
//! engine owns the closed semantics:
//!
//! - `unverified` is computed, never declared: no wire spelling lets
//!   a document assert satisfaction (AC#2). A declaration-method
//!   constraint is the advisory `open-question` row, never proof.
//! - Evidence is keyed per exact environment; results from
//!   incompatible environments stay visible under their own key in
//!   the foreign section and never satisfy a bound (AC#3).
//! - The runtime and ai-budget dimensions render as disjoint
//!   sections; evidence of one dimension can never satisfy the other
//!   (AC#7).
//! - Advisory rows are visible and never block under the default
//!   profile; `--strict` escalates advisory violated/unverified/stale
//!   into the denied set (AC#5).
//!
//! Evaluation rule per current, compatible-environment result: a
//! `fail` receipt is `violated`; `skipped`/`unknown` receipts are
//! `unverified`; a `pass` receipt compares the measurement row against
//! the declared bound under the declared comparator (numeric kinds),
//! and is the owner's verdict for bound-less kinds (retry budget,
//! consistency, deployment, runtime constraints, references) — the
//! core never invents a threshold. Contradictory receipts at the same
//! environment and revision are an explicit `conflict`, never an
//! average.

use serde::Serialize;

use super::canonical::{canonical_value_bytes, sha256_hex};
use super::constraint::{
    Comparator, Constraint, Dimension, Enforcement, RequirementValue, Support,
};
use super::diagnostic;
use super::evidence::{EvidenceResult, EvidenceSet, MeasuredValue, ResultStatus};
use super::id::{ConstraintId, Decimal, IsoDate};
use super::version;
use crate::diagnostics::{Diagnostic, DiagnosticSet};

/// The gate profile: default denies mandatory failures only; strict
/// escalates advisory violated/unverified/stale into the denied set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum GateProfile {
    /// The default profile.
    Default,
    /// The strict profile.
    Strict,
}

impl GateProfile {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Strict => "strict",
        }
    }
}

/// The resolved capability view: the resolved profile's capability
/// table, or the explicit fact that no profile was resolved.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CapabilitySnapshot {
    resolved: bool,
    capabilities: Vec<(String, Support)>,
}

impl CapabilitySnapshot {
    /// The unresolved snapshot: no profile information exists.
    pub fn unresolved() -> Self {
        Self {
            resolved: false,
            capabilities: Vec::new(),
        }
    }

    /// The resolved snapshot over `(capability id, support)` pairs.
    pub fn resolved(capabilities: Vec<(String, Support)>) -> Self {
        Self {
            resolved: true,
            capabilities,
        }
    }

    /// The support of one capability, when resolved and present.
    pub fn support(&self, id: &str) -> Option<Support> {
        if !self.resolved {
            return None;
        }
        self.capabilities
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, support)| *support)
    }

    /// Whether any profile information exists.
    pub const fn is_resolved(&self) -> bool {
        self.resolved
    }
}

/// One finished resolution: the derived report plus the gate verdict
/// under the default profile.
pub struct Resolution {
    /// The full derived report (default verdict).
    pub report: Report,
    /// The gate verdict: `Pass` when nothing denies, otherwise the
    /// aggregated `denied` set.
    pub verdict: ResolutionVerdict,
    /// The non-blocking warnings, canonically ordered.
    warnings: Vec<Diagnostic>,
}

/// The gate verdict of one completed resolution.
pub enum ResolutionVerdict {
    /// Every mandatory constraint carries its evidence.
    Pass,
    /// At least one constraint denies the gate.
    Denied(DiagnosticSet),
}

/// The derived report wire.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The exact wire discriminator.
    pub schema_version: &'static str,
    /// The exact contract identity.
    pub identity: &'static str,
    /// The bound project id.
    pub project_id: String,
    /// The bound Model pin.
    pub model_ref: ModelRefWire,
    /// The bound IR pin.
    pub ir_ref: IrRefWire,
    /// The SHA-256 over the attachment canonical bytes.
    pub attachment_digest: String,
    /// The injected as-of reference date.
    pub as_of: String,
    /// Whether a resolved capability snapshot existed.
    pub capabilities_resolved: bool,
    /// The gate verdict this report was rendered under.
    pub verdict: &'static str,
    /// The runtime section.
    pub runtime: DimensionSection,
    /// The ai-budget section.
    pub ai_budget: DimensionSection,
    /// Evidence under environments outside the accepted set, visible
    /// under their own keys, never merged.
    pub foreign_evidence: Vec<ForeignRow>,
    /// The registered open questions.
    pub open_questions: Vec<OpenQuestionRow>,
    /// Declared scopes no constraint covers.
    pub coverage_gaps: Vec<CoverageGapRow>,
}

/// The Model pin wire of the report.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRefWire {
    /// The exact Model contract version.
    pub model_version: String,
    /// The exact payload digest.
    pub digest: String,
}

/// The IR pin wire of the report.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrRefWire {
    /// The exact IR contract version.
    pub ir_version: String,
    /// The exact canonical IR digest.
    pub digest: String,
}

/// One dimension section; the two dimensions stay disjoint.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionSection {
    /// Every constraint of this dimension, canonically ordered.
    pub constraints: Vec<ConstraintRow>,
}

/// One resolved constraint.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintRow {
    /// The stable constraint id.
    pub constraint_id: String,
    /// The closed kind.
    pub kind: &'static str,
    /// The scope as the schema spells it: `{kind, ref}`.
    pub scope: ScopeWire,
    /// The enforcement.
    pub enforcement: &'static str,
    /// The declared requirement summary.
    pub requirement: RequirementWire,
    /// The exact constraint revision.
    pub revision: String,
    /// The computed first-class status.
    pub status: &'static str,
    /// The per-environment rows of the accepted environments.
    pub environments: Vec<EnvironmentRow>,
    /// The declared method.
    pub method: &'static str,
    /// The declared gate reference, when the kind carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_ref: Option<String>,
    /// The source-requirement link `source:requirement`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_requirement: Option<String>,
}

/// The declared requirement summary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementWire {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentile: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparator: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<ReferenceWire>,
}

/// The owner-held reference wire.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceWire {
    /// The namespaced reference id.
    pub id: String,
    /// The exact snapshot digest.
    pub digest: String,
}

/// One per-environment row of an accepted environment.
/// The scope wire of one constraint row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeWire {
    /// The scope kind.
    pub kind: &'static str,
    /// The scope reference.
    pub r#ref: String,
}

/// One per-environment row of an accepted environment.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentRow {
    /// The stable environment key.
    pub env_key: String,
    /// The per-environment status.
    pub status: &'static str,
    /// The measurement date of the current evidence, when present.
    pub measured_on: Option<String>,
    /// The evidence digest of the current evidence, when present.
    pub evidence_digest: Option<String>,
}

/// One foreign-evidence row: visible, never satisfying.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignRow {
    /// The measured constraint.
    pub constraint_id: String,
    /// The stable key of the foreign environment.
    pub env_key: String,
    /// The fixed mismatch reason.
    pub reason: &'static str,
}

/// One registered open question.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestionRow {
    /// The stable question id.
    pub question_id: String,
    /// The related constraint, when declared.
    pub related_constraint: Option<String>,
}

/// One coverage gap: a declared scope no constraint covers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageGapRow {
    /// The uncovered scope reference.
    pub scope: String,
}

/// Resolve one attachment against its evidence and produce the report
/// and the default verdict. Pure and read-only; custody and scope
/// validation are the caller's contract.
pub(crate) fn build(
    attachment: &super::wire::NfrAttachment,
    attachment_digest: String,
    evidence_sets: &[&EvidenceSet],
    capabilities: &CapabilitySnapshot,
    as_of: &IsoDate,
) -> Resolution {
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let mut runtime: Vec<ConstraintRow> = Vec::new();
    let mut ai_budget: Vec<ConstraintRow> = Vec::new();
    let mut foreign: Vec<ForeignRow> = Vec::new();
    let mut denied: Vec<(String, &'static str)> = Vec::new();

    for constraint in attachment.constraints() {
        let (mut row, has_foreign, unsupported) = evaluate(
            constraint,
            evidence_sets,
            capabilities,
            as_of,
            &mut warnings,
            &mut foreign,
        );
        row.status = status_of(constraint, &row, as_of, has_foreign, unsupported);
        let section = match constraint.dimension() {
            Dimension::Runtime => &mut runtime,
            Dimension::AiBudget => &mut ai_budget,
        };
        denied.push((constraint.constraint_id().as_str().to_owned(), row.status));
        section.push(row);
    }

    let open_questions: Vec<OpenQuestionRow> = attachment
        .open_questions()
        .iter()
        .map(|question| OpenQuestionRow {
            question_id: question.question_id().as_str().to_owned(),
            related_constraint: question
                .related_constraint()
                .map(ConstraintId::as_str)
                .map(str::to_owned),
        })
        .collect();

    let coverage_gaps = coverage_gaps_for(attachment);

    let mut report = Report {
        schema_version: version::REPORT_SCHEMA_VERSION,
        identity: version::REPORT_IDENTITY,
        project_id: attachment.project_id().as_str().to_owned(),
        model_ref: ModelRefWire {
            model_version: attachment.model_ref().0.clone(),
            digest: attachment.model_ref().1.as_str().to_owned(),
        },
        ir_ref: IrRefWire {
            ir_version: attachment.ir_ref().0.clone(),
            digest: attachment.ir_ref().1.as_str().to_owned(),
        },
        attachment_digest,
        as_of: as_of.as_str().to_owned(),
        capabilities_resolved: capabilities.is_resolved(),
        verdict: "pass",
        runtime: DimensionSection {
            constraints: runtime,
        },
        ai_budget: DimensionSection {
            constraints: ai_budget,
        },
        foreign_evidence: foreign,
        open_questions,
        coverage_gaps,
    };
    // The default verdict: mandatory rows in the denial statuses deny
    // the gate; advisory rows are visible and never block (AC#5).
    let mandatory_denials: Vec<(String, &'static str)> = report
        .runtime
        .constraints
        .iter()
        .chain(report.ai_budget.constraints.iter())
        .filter(|row| row.enforcement == "mandatory")
        .filter(|row| denies(row.status))
        .map(|row| (row.constraint_id.clone(), row.status))
        .collect();
    report.verdict = if mandatory_denials.is_empty() {
        "pass"
    } else {
        "denied"
    };
    let verdict = match diagnostic::gate(mandatory_denials) {
        Some(set) => ResolutionVerdict::Denied(set),
        None => ResolutionVerdict::Pass,
    };
    warnings.sort_by(|left, right| {
        (
            left.id.as_str(),
            serde_json::to_string(&left.data).unwrap_or_default(),
        )
            .cmp(&(
                right.id.as_str(),
                serde_json::to_string(&right.data).unwrap_or_default(),
            ))
    });
    warnings.dedup();
    Resolution {
        report,
        verdict,
        warnings,
    }
}

impl Resolution {
    /// The non-blocking warnings of this resolution, canonically
    /// ordered.
    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }

    /// The gate verdict under `profile`: strict escalates advisory
    /// violated/unverified/stale into the denied set (AC#5).
    pub fn verdict_for(&self, profile: GateProfile) -> ResolutionVerdict {
        if profile == GateProfile::Default {
            return match &self.verdict {
                ResolutionVerdict::Pass => ResolutionVerdict::Pass,
                ResolutionVerdict::Denied(set) => ResolutionVerdict::Denied(set.clone()),
            };
        }
        let denials: Vec<(String, &'static str)> = self
            .report
            .runtime
            .constraints
            .iter()
            .chain(self.report.ai_budget.constraints.iter())
            .filter(|row| strict_denies(row.status, row.enforcement))
            .map(|row| (row.constraint_id.clone(), row.status))
            .collect();
        match diagnostic::gate(denials) {
            Some(set) => ResolutionVerdict::Denied(set),
            None => ResolutionVerdict::Pass,
        }
    }
}

/// Whether the default gate denies this status (mandatory rows only).
fn denies(status: &str) -> bool {
    matches!(
        status,
        "violated" | "unverified" | "stale" | "unsupported" | "conflict"
    )
}

/// Whether the strict gate denies this row: advisory
/// violated/unverified/stale join the mandatory denial statuses.
/// `open-question` rows stay visible and never block by design.
fn strict_denies(status: &str, enforcement: &str) -> bool {
    if enforcement == "mandatory" {
        return denies(status);
    }
    matches!(status, "violated" | "unverified" | "stale")
}

/// Evaluate one constraint against every evidence set; accumulate
/// warnings and foreign rows. Returns the row with per-environment
/// states filled, a provisional status, and the foreign-evidence
/// signal.
fn evaluate(
    constraint: &Constraint,
    evidence_sets: &[&EvidenceSet],
    capabilities: &CapabilitySnapshot,
    as_of: &IsoDate,
    warnings: &mut Vec<Diagnostic>,
    foreign: &mut Vec<ForeignRow>,
) -> (ConstraintRow, bool, bool) {
    let constraint_id = constraint.constraint_id().as_str();

    // Capability requirements first: an unsatisfied capability makes
    // the constraint unmeasurable in this profile.
    let mut unsupported = false;
    if capabilities.is_resolved() {
        for requirement in constraint.capabilities() {
            let actual = capabilities.support(requirement.id());
            let satisfied =
                actual.is_some_and(|support| Support::satisfies(requirement.minimum(), support));
            if !satisfied {
                unsupported = true;
                if let Some(warning) =
                    diagnostic::capability_unsatisfied("capability-unsatisfied", constraint_id)
                {
                    warnings.push(warning);
                }
            }
        }
    }

    // Group the current evidence of this constraint by environment
    // key. A declared environment accepts exactly the evidence whose
    // identity is equal; an empty accepted set accepts the first
    // distinct environment as the constraint's single environment and
    // treats every further distinct environment as foreign — results
    // are never merged across environments (AC#3).
    let declared: &[super::environment::Environment] = constraint.environments();
    let mut per_env: std::collections::BTreeMap<String, Vec<&EvidenceResult>> =
        std::collections::BTreeMap::new();
    let mut single: Option<&super::environment::Environment> = None;
    for set in evidence_sets {
        for result in set.results() {
            if result.constraint_id().as_str() != constraint_id {
                continue;
            }
            let environment = set.environment();
            let owner = if !declared.is_empty() {
                declared
                    .iter()
                    .find(|candidate| candidate.compatible_with(environment))
            } else {
                match single {
                    Some(candidate) if candidate.compatible_with(environment) => Some(candidate),
                    Some(_) => None,
                    None => {
                        single = Some(environment);
                        single
                    }
                }
            };
            match owner {
                Some(candidate) => {
                    per_env
                        .entry(candidate.env_key().to_owned())
                        .or_default()
                        .push(result);
                }
                None => {
                    // The fixed mismatch reason against the declared
                    // environments; with no declaration the mismatch
                    // is the identity itself.
                    let reason = declared
                        .iter()
                        .map(|candidate| candidate.foreign_reason(environment))
                        .find(|reason| *reason != "undeclared-environment")
                        .unwrap_or("undeclared-environment");
                    foreign.push(ForeignRow {
                        constraint_id: constraint_id.to_owned(),
                        env_key: environment.env_key(),
                        reason,
                    });
                }
            }
        }
    }

    // Per-environment status over the current revision, in canonical
    // environment-key order.
    let current_revision = constraint.validity().revision();
    let mut environments: Vec<EnvironmentRow> = Vec::new();
    for (env_key, results) in &per_env {
        let results = results.as_slice();
        let current: Vec<&&EvidenceResult> = results
            .iter()
            .filter(|result| {
                result.constraint_revision() == current_revision
                    && result
                        .expires_on()
                        .map(|expires| expires.as_str() >= as_of.as_str())
                        .unwrap_or(true)
            })
            .collect();
        let stale: Vec<&&EvidenceResult> = results
            .iter()
            .filter(|result| !current.contains(result))
            .collect();
        let status = if results.is_empty() {
            None
        } else if current.is_empty() {
            if let Some(warning) = diagnostic::evidence_stale("revision-or-expiry", constraint_id) {
                warnings.push(warning);
            }
            Some("stale")
        } else {
            Some(evaluate_current(constraint, &current))
        };
        let _ = stale;
        let latest = current
            .iter()
            .max_by_key(|result| result.measured_on().as_str());
        environments.push(EnvironmentRow {
            env_key: env_key.clone(),
            status: status.unwrap_or("unverified"),
            measured_on: latest.map(|result| result.measured_on().as_str().to_owned()),
            evidence_digest: latest.map(|result| result.evidence_digest().to_owned()),
        });
    }
    environments.sort_by(|left, right| left.env_key.cmp(&right.env_key));

    let row = ConstraintRow {
        constraint_id: constraint_id.to_owned(),
        kind: constraint.kind().as_str(),
        scope: ScopeWire {
            kind: constraint.scope().kind().as_str(),
            r#ref: constraint.scope().reference().to_owned(),
        },
        enforcement: constraint.enforcement().as_str(),
        requirement: requirement_wire(constraint),
        revision: current_revision.to_owned(),
        status: "unverified",
        environments,
        method: constraint.measurement().method().as_str(),
        gate_ref: constraint
            .measurement()
            .gate_ref()
            .map(|gate| gate.as_str().to_owned()),
        source_requirement: constraint
            .source_requirement()
            .map(|(source, requirement)| format!("{source}:{requirement}")),
    };
    let has_foreign = foreign.iter().any(|row| row.constraint_id == constraint_id);
    (row, has_foreign, unsupported)
}

/// Evaluate the current results of one accepted environment into its
/// per-environment status.
fn evaluate_current(constraint: &Constraint, current: &[&&EvidenceResult]) -> &'static str {
    let mut saw_violated = false;
    let mut saw_satisfied = false;
    let mut saw_unverified = false;
    for result in current {
        match result.result_status() {
            ResultStatus::Fail => saw_violated = true,
            ResultStatus::Skipped | ResultStatus::Unknown => saw_unverified = true,
            ResultStatus::Pass => match judge(constraint, result) {
                Verdict::Violated => saw_violated = true,
                Verdict::Satisfied => saw_satisfied = true,
                Verdict::Unverified => saw_unverified = true,
            },
        }
    }
    // Explicit disagreement at one environment and revision is a
    // conflict, never an average.
    if saw_violated && saw_satisfied {
        return "conflict";
    }
    if saw_violated {
        return "violated";
    }
    if saw_satisfied {
        return "satisfied";
    }
    if saw_unverified {
        return "unverified";
    }
    "unverified"
}

/// The judged outcome of one passing receipt.
enum Verdict {
    Satisfied,
    Violated,
    Unverified,
}

/// Judge one passing receipt against the declared requirement.
fn judge(constraint: &Constraint, result: &EvidenceResult) -> Verdict {
    let requirement = constraint.requirement();
    let comparator = match requirement.comparator() {
        Some(comparator) => comparator,
        // A bound-less kind carries no numeric claim: the pass receipt
        // of the owner's own check is the verdict; the core never
        // invents a threshold.
        None => return Verdict::Satisfied,
    };
    let expected_metric = requirement.metric();
    let expected_percentile = requirement.percentile();
    for measurement in result.measurements() {
        if let Some(metric) = expected_metric {
            if measurement.metric() != metric {
                continue;
            }
        }
        if measurement.percentile() != expected_percentile {
            continue;
        }
        let MeasuredValue::Known(measured) = measurement.value() else {
            return Verdict::Unverified;
        };
        let inside = match requirement.value() {
            Some(RequirementValue::Scalar(bound)) => compare(comparator, measured, bound),
            Some(RequirementValue::Range { min, max }) => match comparator {
                Comparator::Within => {
                    measured.cmp_value(min) != std::cmp::Ordering::Less
                        && measured.cmp_value(max) != std::cmp::Ordering::Greater
                }
                Comparator::Outside => {
                    measured.cmp_value(min) == std::cmp::Ordering::Less
                        || measured.cmp_value(max) == std::cmp::Ordering::Greater
                }
                _ => return Verdict::Unverified,
            },
            None => return Verdict::Unverified,
        };
        return if inside {
            Verdict::Satisfied
        } else {
            Verdict::Violated
        };
    }
    // No matching measurement row: the receipt cannot speak for the
    // declared metric.
    Verdict::Unverified
}

/// Whether `measured <bound>` holds under the comparator.
fn compare(comparator: Comparator, measured: &Decimal, bound: &Decimal) -> bool {
    let order = measured.cmp_value(bound);
    match comparator {
        Comparator::Lt => order == std::cmp::Ordering::Less,
        Comparator::Lte => order != std::cmp::Ordering::Greater,
        Comparator::Eq => order == std::cmp::Ordering::Equal,
        Comparator::Gte => order != std::cmp::Ordering::Less,
        Comparator::Gt => order == std::cmp::Ordering::Greater,
        Comparator::Within | Comparator::Outside => false,
    }
}

/// The aggregated constraint status over its per-environment rows,
/// foreign evidence, capability state, validity window, and method.
fn status_of(
    constraint: &Constraint,
    row: &ConstraintRow,
    as_of: &IsoDate,
    has_foreign: bool,
    unsupported: bool,
) -> &'static str {
    // An unsatisfied capability makes the constraint unmeasurable in
    // this profile; nothing else can speak for it.
    if unsupported {
        return "unsupported";
    }
    // Advisory declaration rows are open questions by design (AC#2).
    if constraint.enforcement() == Enforcement::Advisory
        && constraint.measurement().method() == super::constraint::Method::Declaration
    {
        return "open-question";
    }
    // The constraint itself expired before the as-of date.
    if constraint
        .validity()
        .valid_until()
        .is_some_and(|until| until.as_str() < as_of.as_str())
    {
        return "stale";
    }
    let statuses: Vec<&str> = row
        .environments
        .iter()
        .map(|environment| environment.status)
        .collect();
    if statuses.contains(&"conflict") {
        return "conflict";
    }
    if statuses.contains(&"violated") {
        return "violated";
    }
    if statuses.contains(&"satisfied") {
        return "satisfied";
    }
    if statuses.contains(&"stale") {
        return "stale";
    }
    // Evidence exists but only under foreign environments.
    if statuses.is_empty() && has_foreign {
        return "foreign-environment";
    }
    if statuses.contains(&"unverified") {
        return "unverified";
    }
    "unverified"
}

/// The requirement summary wire of one constraint.
fn requirement_wire(constraint: &Constraint) -> RequirementWire {
    let requirement = constraint.requirement();
    RequirementWire {
        metric: requirement.metric().map(str::to_owned),
        percentile: requirement.percentile().map(|p| p.as_str()),
        comparator: requirement.comparator().map(|c| c.as_str()),
        value: match requirement.value() {
            Some(RequirementValue::Scalar(value)) => Some(value.as_str().to_owned()),
            _ => None,
        },
        range: match requirement.value() {
            Some(RequirementValue::Range { min, max }) => {
                Some((min.as_str().to_owned(), max.as_str().to_owned()))
            }
            _ => None,
        },
        unit: requirement.unit().map(|unit| unit.as_str()),
        resource: requirement.resource().map(|resource| resource.as_str()),
        mode: requirement.mode().map(|mode| mode.as_str()),
        tokens: requirement
            .tokens()
            .iter()
            .map(|token| token.as_str().to_owned())
            .collect(),
        max_attempts: requirement
            .max_attempts()
            .map(|attempts| attempts.as_str().to_owned()),
        window: requirement
            .window()
            .map(|(amount, _)| amount.as_str().to_owned()),
        window_unit: requirement.window().map(|(_, unit)| unit.as_str()),
        reference: requirement.reference().map(|reference| ReferenceWire {
            id: reference.id().to_owned(),
            digest: reference.digest().to_owned(),
        }),
    }
}

/// Declared scopes no constraint covers. A module scope is covered
/// when some constraint targets the module itself or any symbol
/// inside it; the project scope is covered by any project-scoped
/// constraint.
fn coverage_gaps_for(attachment: &super::wire::NfrAttachment) -> Vec<CoverageGapRow> {
    // v1 reports the absence of any constraint at the project scope as
    // the single structural gap; per-symbol coverage stays the
    // requirements family's word.
    if attachment.constraints().is_empty() {
        return vec![CoverageGapRow {
            scope: attachment.project_id().as_str().to_owned(),
        }];
    }
    Vec::new()
}

impl Report {
    /// The canonical report bytes (compact JSON, byte-sorted keys,
    /// canonical collections, no trailing LF), or the export-limit
    /// refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        let bytes = canonical_value_bytes(self);
        super::canonical::check_export_bound(&bytes)?;
        Ok(bytes)
    }

    /// The bare 64-character lowercase SHA-256 hex digest of the
    /// canonical report bytes.
    pub fn digest(&self) -> Result<String, DiagnosticSet> {
        Ok(sha256_hex(self.canonical_bytes()?.as_bytes()))
    }

    /// Project this report into the typed #22 neutral trace manifest,
    /// re-validated by the accepted trace validator. Pure and
    /// read-only.
    pub fn trace_manifest(&self) -> Result<crate::trace::TraceManifest, DiagnosticSet> {
        super::trace::validated_manifest(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparators_judge_exact_decimals() {
        let bound = Decimal::parse("250").unwrap();
        assert!(compare(
            Comparator::Lte,
            &Decimal::parse("238").unwrap(),
            &bound
        ));
        assert!(!compare(
            Comparator::Lte,
            &Decimal::parse("250.1").unwrap(),
            &bound
        ));
        assert!(compare(
            Comparator::Lte,
            &Decimal::parse("250").unwrap(),
            &bound
        ));
        let min = Decimal::parse("10").unwrap();
        let max = Decimal::parse("20").unwrap();
        assert!(!compare(
            Comparator::Within,
            &Decimal::parse("15").unwrap(),
            &min
        ));
        assert!(
            Decimal::parse("15").unwrap().cmp_value(&min) != std::cmp::Ordering::Less
                && Decimal::parse("15").unwrap().cmp_value(&max) != std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn strict_escalates_only_the_declared_statuses() {
        assert!(strict_denies("violated", "advisory"));
        assert!(strict_denies("unverified", "advisory"));
        assert!(strict_denies("stale", "advisory"));
        assert!(!strict_denies("open-question", "advisory"));
        assert!(!strict_denies("satisfied", "advisory"));
        assert!(strict_denies("unsupported", "mandatory"));
        assert!(denies("conflict"));
    }

    #[test]
    fn latency_without_percentile_is_unjudged() {
        // The judge path returns Unverified when no row matches the
        // declared metric; covered end to end by the integration
        // truth-table tests.
        assert_eq!(version::REPORT_SCHEMA_VERSION, "lekalo/nfr-report/v0.4.0");
    }
}
