//! The deterministic four-dimension assessment (issue #19).
//!
//! Pure classification over typed inputs: the caller supplies the
//! validated compilation, the extracted subjects, exact verified target
//! pins, and qualified evidence. No filesystem, process, or network
//! operation happens here, and no wall-clock read influences any verdict.
//! Precedence follows the reviewed recipe: explicit incompatibility is
//! unsupported, known restricted coverage is partial, target-specific
//! facts keep their risk tag, qualified full declared coverage is
//! portable or supported-with-mapping, and everything absent or unmapped
//! stays unknown.

use super::requirements::Subjects;
use super::wire::{
    self, AdapterSnapshot, Binding, Classification, Coverage, Destination, Dimension,
    DimensionSummary, EvidenceDocument, EvidenceInput, EvidencePresence, Inputs, Limits, LockInput,
    LockPresence, ObligationRow, PinState, Policy, ProfilePin, Ratio, Report, ReportDiagnostic,
    Scope, ScopeKind, SourceSelection, SubjectRow, Summary, TargetsInput, Verification,
};
use super::{diagnostic, planning, Request};
use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;
use std::collections::{BTreeMap, BTreeSet};

/// One exact destination (or source) input resolved by the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetInput {
    pub target_id: String,
    pub state: PinState,
    pub profile: Option<ProfilePin>,
    pub adapter: Option<AdapterSnapshot>,
    pub storage_profile_refs: Vec<String>,
}

/// The complete typed input of one assessment.
pub struct Assessment<'a> {
    pub compilation: &'a Compilation,
    pub recipe: &'a wire::Recipe,
    pub subjects: Subjects,
    pub request: &'a Request,
    pub source: TargetInput,
    pub destinations: Vec<TargetInput>,
    pub evidence: Option<&'a EvidenceDocument>,
    /// The qualified, producer-validated implementation projection, or
    /// `None` when no complete implementation family was joined.
    pub implementation: Option<&'a crate::implementation::PortabilityReport>,
    pub targets_input: TargetsInput,
    pub lock_digest: Option<String>,
}

/// The reason tokens of one assessment. The closed vocabulary keeps the
/// report's `reason` member bounded and greppable.
pub mod reasons {
    pub const DECLARED_PORTABLE: &str = "declared-portable";
    pub const DECLARED_TARGET_SPECIFIC: &str = "declared-target-specific";
    pub const DEPENDENCY_TARGET_SPECIFIC: &str = "dependency-target-specific";
    pub const NO_TARGET_SPECIFIC_WITNESS: &str = "no-target-specific-witness";
    pub const HOOK_GENERATED: &str = "hook-generated";
    pub const HOOK_CUSTOM: &str = "hook-custom";
    pub const HOOK_FOREIGN_MAPPED: &str = "hook-foreign-mapped";
    pub const HOOK_FOREIGN_PARTIAL: &str = "hook-foreign-partial";
    pub const HOOK_FOREIGN_UNMAPPED: &str = "hook-foreign-unmapped";
    pub const HOOK_EXTERNAL_MAPPED: &str = "hook-external-mapped";
    pub const HOOK_EXTERNAL_PARTIAL: &str = "hook-external-partial";
    pub const HOOK_EXTERNAL_UNMAPPED: &str = "hook-external-unmapped";
    pub const HOOK_UNSUPPORTED: &str = "hook-unsupported";
    pub const HOOK_MISSING: &str = "hook-missing";
    pub const IMPLEMENTATION_INVENTORY_ABSENT: &str = "implementation-inventory-absent";
    pub const IMPLEMENTATION_INVENTORY_INCOMPLETE: &str = "implementation-inventory-incomplete";
    pub const BINDING_EVIDENCE_ABSENT: &str = "binding-evidence-absent";
    pub const NO_TRIGGERING_COMMAND: &str = "no-triggering-command";
    pub const NO_RESTRICTED_POLICY: &str = "no-restricted-policy";
    pub const POLICY_COVERED: &str = "policy-covered";
    pub const STORAGE_POLICY_LOSS: &str = "storage-policy-loss";
    pub const STORAGE_POLICY_UNKNOWN: &str = "storage-policy-unknown";
    pub const STORAGE_INVENTORY_ABSENT: &str = "storage-inventory-absent";
    pub const STORAGE_INVENTORY_INCOMPLETE: &str = "storage-inventory-incomplete";
    pub const CAPABILITY_DECLARED: &str = "capability-declared";
    pub const CAPABILITY_PARTIAL: &str = "capability-partial";
    pub const CAPABILITY_ABSENT: &str = "capability-absent";
    pub const CAPABILITY_EXPLICIT_UNSUPPORTED: &str = "capability-explicit-unsupported";
    pub const IMPLICIT_HOOK_UNSUPPORTED: &str = "implicit-hook-unsupported";
    pub const NO_OPERATIONAL_OBLIGATION: &str = "no-operational-obligation";
    pub const DIMENSION_NOT_ASSIGNED: &str = "dimension-not-assigned";
    pub const DESTINATION_EVIDENCE_GAP: &str = "destination-evidence-gap";
}

/// The fixed report-reason vocabulary of the registered portability
/// diagnostic codes.
pub mod codes {
    pub const EVIDENCE_INCOMPLETE: &str = "portability.evidence-incomplete";
    pub const MAPPING_PARTIAL: &str = "portability.mapping-partial";
    pub const CAPABILITY_UNSUPPORTED: &str = "portability.capability-unsupported";
    pub const IMPLEMENTATION_REQUIRED: &str = "portability.implementation-required";
    pub const BEHAVIOR_LOSS: &str = "portability.behavior-loss";
    pub const ANALYSIS_INCOMPLETE: &str = "portability.analysis-incomplete";
    pub const CHECK_DENIED: &str = "portability.check-denied";
}

struct Sink {
    diagnostics: Vec<ReportDiagnostic>,
    seen: BTreeSet<(String, Option<String>, Option<Dimension>, &'static str)>,
}

impl Sink {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            seen: BTreeSet::new(),
        }
    }
    fn push(
        &mut self,
        code: &str,
        subject: Option<String>,
        dimension: Option<Dimension>,
        detail: &'static str,
    ) {
        let key = (code.to_owned(), subject.clone(), dimension, detail);
        if self.seen.insert(key) {
            self.diagnostics.push(ReportDiagnostic {
                code: code.to_owned(),
                subject,
                dimension,
                detail: detail.to_owned(),
            });
        }
    }
}

/// The measured work bounds of one assessment run.
struct Bounds {
    limits: Limits,
    obligation_rows: u64,
    non_applicable_rows: u64,
    exhausted: bool,
}

impl Bounds {
    fn charge_row(&mut self) -> bool {
        self.obligation_rows += 1;
        if self.obligation_rows > self.limits.max_obligation_rows {
            self.exhausted = true;
            return false;
        }
        true
    }
    fn charge_non_applicable(&mut self) {
        self.non_applicable_rows += 1;
    }
}

/// Run the full assessment and produce the closed report.
pub fn assess(input: &Assessment) -> Result<Report, DiagnosticSet> {
    let project = &input.compilation.project;
    let project_id = project
        .project
        .as_ref()
        .map(|p| p.id.as_str().to_owned())
        .unwrap_or_default();
    let recipe = input.recipe;

    // Scope selection: the selected subjects carry the rows; support
    // dependencies stay visible as witnesses without entering the
    // selected denominator.
    let selected = select_subjects(&input.subjects, input.request)?;

    let mut bounds = Bounds {
        limits: recipe.document.limits,
        obligation_rows: 0,
        non_applicable_rows: 0,
        exhausted: false,
    };
    if input.subjects.rows.len() as u64 > recipe.document.limits.max_subjects {
        bounds.exhausted = true;
    }
    let mut sink = Sink::new();
    if bounds.exhausted {
        sink.push(
            codes::ANALYSIS_INCOMPLETE,
            None,
            None,
            "subject-bound-exhausted",
        );
    }
    if input.subjects.truncated {
        bounds.exhausted = true;
    }

    let evidence = input.evidence;
    let destinations = &input.destinations;

    // Per-subject, per-destination, per-dimension obligation rows.
    let mut subject_rows: Vec<SubjectRow> = Vec::new();
    for subject in &selected {
        let mut rows: Vec<ObligationRow> = Vec::new();
        let dimensions = recipe.dimensions_for_kind(subject.kind);
        for destination in destinations {
            for dimension in Dimension::ALL {
                if !dimensions.contains(&dimension) {
                    if bounds.charge_row() {
                        bounds.charge_non_applicable();
                        rows.push(non_applicable_row(&destination.target_id, dimension));
                    }
                    continue;
                }
                if !bounds.charge_row() {
                    continue;
                }
                rows.push(classify(
                    input,
                    evidence,
                    &mut sink,
                    subject,
                    &destination.target_id,
                    dimension,
                ));
            }
        }
        subject_rows.push(SubjectRow {
            subject: subject.typed_id.clone(),
            member: None,
            requirements: subject.deps.clone(),
            rows,
        });
    }
    if bounds.exhausted {
        sink.push(
            codes::ANALYSIS_INCOMPLETE,
            None,
            None,
            "obligation-bound-exhausted",
        );
    }
    // With no destination configured the inventory stays explicit and no
    // supported destination is fabricated.
    if destinations.is_empty() {
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            None,
            None,
            reasons::DESTINATION_EVIDENCE_GAP,
        );
    }
    if bounds.exhausted {
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            None,
            None,
            "incomplete-extraction-lower-bound",
        );
    }

    let summaries = summarize(destinations, &subject_rows);
    let affected = planning::affected_joins(input, evidence, &selected, &subject_rows);
    let migration = planning::migration_surface(
        input,
        evidence,
        &selected,
        &subject_rows,
        &affected,
        bounds.exhausted,
    );

    let eligible = selected.len() as u64;
    Ok(Report {
        schema_version: super::version::REPORT_SCHEMA_VERSION.to_owned(),
        identity: super::version::REPORT_IDENTITY.to_owned(),
        recipe_ref: wire::RecipeRef {
            id: "dev.lekalo.portability-recipe".to_owned(),
            version: recipe.document_version().to_owned(),
            digest: recipe.digest.clone(),
        },
        project_ref: wire::ProjectRef {
            project_id,
            model_version: project.model_version.as_str().to_owned(),
            ir_digest: format!("sha256:{}", crate::implementation::ir_digest(project)),
        },
        scope: Scope {
            kind: scope_kind(input.request),
            selector: scope_selector(input.request),
        },
        source: SourceSelection {
            target_id: input.source.target_id.clone(),
            state: input.source.state,
            profile: input.source.profile.clone(),
        },
        destinations: destinations
            .iter()
            .map(|d| Destination {
                target_id: d.target_id.clone(),
                state: d.state,
                profile: d.profile.clone(),
                adapter: d.adapter.clone(),
                storage_profile_refs: d.storage_profile_refs.clone(),
            })
            .collect(),
        inputs: Inputs {
            targets_config: input.targets_input.clone(),
            evidence: EvidenceInput {
                state: if evidence.is_some() {
                    EvidencePresence::Supplied
                } else {
                    EvidencePresence::Absent
                },
            },
            lock_digest: match &input.lock_digest {
                Some(value) => LockInput {
                    state: LockPresence::Known,
                    value: Some(value.clone()),
                },
                None => LockInput {
                    state: LockPresence::Absent,
                    value: None,
                },
            },
        },
        coverage: Coverage {
            eligible_subjects: eligible,
            obligation_rows: bounds.obligation_rows,
            non_applicable_rows: bounds.non_applicable_rows,
            complete: !bounds.exhausted,
            truncated: bounds.exhausted,
            limits: recipe.document.limits,
        },
        subjects: subject_rows,
        summaries,
        migration_surface: migration,
        affected_scenarios: affected.0,
        affected_gates: affected.1,
        diagnostics: sink.diagnostics,
    })
}

fn scope_kind(request: &Request) -> ScopeKind {
    match request.scope {
        super::request::Scope::Project => ScopeKind::Project,
        super::request::Scope::Symbol(_) => ScopeKind::Symbol,
        super::request::Scope::Module(_) => ScopeKind::Module,
    }
}

fn scope_selector(request: &Request) -> Option<String> {
    match &request.scope {
        super::request::Scope::Project => None,
        super::request::Scope::Symbol(id) | super::request::Scope::Module(id) => Some(id.clone()),
    }
}

/// Resolve the selected subjects for the request scope.
fn select_subjects<'a>(
    subjects: &'a Subjects,
    request: &Request,
) -> Result<Vec<&'a super::requirements::Subject>, DiagnosticSet> {
    let all = subjects.rows.iter().collect::<Vec<_>>();
    match &request.scope {
        super::request::Scope::Project => Ok(all),
        super::request::Scope::Symbol(id) => {
            let found: Vec<&super::requirements::Subject> = all
                .iter()
                .copied()
                .filter(|s| s.id == *id || s.typed_id == *id)
                .collect();
            if found.is_empty() {
                return Err(diagnostic::invalid("symbol-unresolved"));
            }
            Ok(found)
        }
        super::request::Scope::Module(id) => {
            let found: Vec<&super::requirements::Subject> = all
                .iter()
                .copied()
                .filter(|s| s.module == *id || s.id == *id)
                .collect();
            if found.is_empty() {
                return Err(diagnostic::invalid("module-unresolved"));
            }
            Ok(found)
        }
    }
}

fn non_applicable_row(destination: &str, dimension: Dimension) -> ObligationRow {
    ObligationRow {
        destination: destination.to_owned(),
        dimension,
        classification: Classification::NonApplicable,
        verification: Verification::NotApplicable,
        binding: None,
        blocking: false,
        reason: reasons::DIMENSION_NOT_ASSIGNED.to_owned(),
        required: None,
        offered: None,
        evidence_refs: vec![],
        witnesses: vec![],
    }
}

/// The orthogonal verification state of one subject/destination join:
/// the recorded receipts of the scenarios that cover the subject.
fn verification_for(
    subjects: &Subjects,
    evidence: Option<&EvidenceDocument>,
    subject: &super::requirements::Subject,
    destination: &str,
) -> Verification {
    if subject.kind == "scenario" {
        return receipt_state(evidence, &subject.id, destination);
    }
    // The covering scenarios of this subject carry the recorded receipts.
    let covering = subjects
        .covered_by
        .iter()
        .filter(|(_, covered)| covered.iter().any(|c| c == &subject.id))
        .map(|(scenario, _)| scenario.as_str())
        .collect::<Vec<_>>();
    let mut has_fail = false;
    let mut has_pass = false;
    for scenario in covering {
        match receipt_state(evidence, scenario, destination) {
            Verification::RecordedFail => has_fail = true,
            Verification::RecordedPass => has_pass = true,
            _ => {}
        }
    }
    if has_fail {
        Verification::RecordedFail
    } else if has_pass {
        Verification::RecordedPass
    } else {
        Verification::Untested
    }
}

fn receipt_state(
    evidence: Option<&EvidenceDocument>,
    scenario: &str,
    destination: &str,
) -> Verification {
    let Some(evidence) = evidence else {
        return Verification::Untested;
    };
    match evidence
        .scenarios
        .iter()
        .find(|s| s.scenario == scenario && s.target_id.as_deref() == Some(destination))
    {
        Some(record) => match record.state {
            wire::ScenarioReceipt::RecordedPass => Verification::RecordedPass,
            wire::ScenarioReceipt::RecordedFail => Verification::RecordedFail,
            wire::ScenarioReceipt::Untested => Verification::Untested,
        },
        None => Verification::Untested,
    }
}

/// Classify one subject/destination/dimension obligation.
#[allow(clippy::too_many_lines)]
fn classify(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    sink: &mut Sink,
    subject: &super::requirements::Subject,
    destination: &str,
    dimension: Dimension,
) -> ObligationRow {
    let row = match dimension {
        Dimension::Structural => structural(subject, destination),
        Dimension::Behavioral => behavioral(input, evidence, sink, subject, destination),
        Dimension::Storage => storage(evidence, sink, subject, destination),
        Dimension::Operational => operational(input, evidence, sink, subject, destination),
    };
    let mut row = row;
    row.verification = if row.classification == Classification::NonApplicable {
        Verification::NotApplicable
    } else {
        verification_for(&input.subjects, evidence, subject, destination)
    };
    row.witnesses = subject.witnesses.clone();
    row
}

fn structural(subject: &super::requirements::Subject, destination: &str) -> ObligationRow {
    let base = |classification, reason: &'static str| ObligationRow {
        destination: destination.to_owned(),
        dimension: Dimension::Structural,
        classification,
        verification: Verification::Untested,
        binding: None,
        blocking: false,
        reason: reason.to_owned(),
        required: None,
        offered: None,
        evidence_refs: vec![],
        witnesses: vec![],
    };
    if subject.declared_portability == Some("target-specific") {
        return base(
            Classification::TargetSpecific,
            reasons::DECLARED_TARGET_SPECIFIC,
        );
    }
    // The bounded dependency closure carries the target-specific risk:
    // any witness declared target-specific makes the dependent one a
    // target-specific risk with an explicit replacement obligation.
    if subject
        .witnesses
        .iter()
        .any(|w| w.starts_with("target-binding:"))
    {
        return base(
            Classification::TargetSpecific,
            reasons::DEPENDENCY_TARGET_SPECIFIC,
        );
    }
    if subject.declared_portability == Some("portable") {
        return base(Classification::Portable, reasons::DECLARED_PORTABLE);
    }
    base(
        Classification::Portable,
        reasons::NO_TARGET_SPECIFIC_WITNESS,
    )
}

fn implementation_status(
    input: &Assessment,
    symbol: &str,
    destination: &str,
) -> Option<crate::implementation::PortStatus> {
    let projection = input.implementation?;
    let found = projection.symbols.iter().find(|s| s.symbol == symbol)?;
    found
        .targets
        .iter()
        .find(|t| t.target == destination)
        .map(|t| t.status)
}

fn behavioral(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    sink: &mut Sink,
    subject: &super::requirements::Subject,
    destination: &str,
) -> ObligationRow {
    let base = |classification,
                binding: Option<Binding>,
                blocking,
                reason: &'static str,
                evidence_refs: Vec<String>| ObligationRow {
        destination: destination.to_owned(),
        dimension: Dimension::Behavioral,
        classification,
        verification: Verification::Untested,
        binding,
        blocking,
        reason: reason.to_owned(),
        required: None,
        offered: None,
        evidence_refs,
        witnesses: vec![],
    };
    // The implementation family must be declared complete for a
    // definitive absence; otherwise the row stays unknown.
    let family = evidence.and_then(|e| {
        e.families
            .iter()
            .find(|f| f.family == wire::EvidenceFamily::Implementation)
    });
    let inventory_qualified =
        matches!(family, Some(f) if f.complete) && input.implementation.is_some();
    if !inventory_qualified {
        let reason = if family.is_some() {
            reasons::IMPLEMENTATION_INVENTORY_INCOMPLETE
        } else {
            reasons::IMPLEMENTATION_INVENTORY_ABSENT
        };
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(subject.typed_id.clone()),
            Some(Dimension::Behavioral),
            reason,
        );
        return base(Classification::Unknown, None, true, reason, vec![]);
    }
    // Effects join the hook coverage of their triggering commands.
    if subject.kind == "effect" {
        if subject.triggers.is_empty() {
            return base(
                Classification::NonApplicable,
                None,
                false,
                reasons::NO_TRIGGERING_COMMAND,
                vec![],
            );
        }
        let mut worst = Classification::Portable;
        let mut binding = None;
        let mut reason = reasons::HOOK_GENERATED;
        let mut blocking = false;
        for trigger in &subject.triggers {
            let row = hook_row(input, evidence, sink, trigger, destination);
            if row.classification.rank() < worst.rank() {
                worst = row.classification;
                binding = row.binding;
                reason = row.reason;
                blocking = row.blocking;
            }
        }
        return base(worst, binding, blocking, reason, vec![]);
    }
    let row = hook_row(input, evidence, sink, &subject.id, destination);
    base(
        row.classification,
        row.binding,
        row.blocking,
        row.reason,
        row.evidence_refs,
    )
}

struct HookOutcome {
    classification: Classification,
    binding: Option<Binding>,
    blocking: bool,
    reason: &'static str,
    evidence_refs: Vec<String>,
}

fn hook_row(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    sink: &mut Sink,
    symbol: &str,
    destination: &str,
) -> HookOutcome {
    match implementation_status(input, symbol, destination) {
        None => {
            sink.push(
                codes::EVIDENCE_INCOMPLETE,
                Some(symbol.to_owned()),
                Some(Dimension::Behavioral),
                reasons::BINDING_EVIDENCE_ABSENT,
            );
            HookOutcome {
                classification: Classification::Unknown,
                binding: None,
                blocking: true,
                reason: reasons::BINDING_EVIDENCE_ABSENT,
                evidence_refs: vec![],
            }
        }
        Some(status) => match status {
            crate::implementation::PortStatus::Generated => HookOutcome {
                classification: Classification::Portable,
                binding: Some(Binding::Generated),
                blocking: false,
                reason: reasons::HOOK_GENERATED,
                evidence_refs: vec![],
            },
            crate::implementation::PortStatus::Custom => HookOutcome {
                classification: Classification::SupportedWithMapping,
                binding: Some(Binding::Custom),
                blocking: false,
                reason: reasons::HOOK_CUSTOM,
                evidence_refs: vec![],
            },
            crate::implementation::PortStatus::Foreign => {
                mapping_outcome(evidence, sink, symbol, destination, Binding::Foreign)
            }
            crate::implementation::PortStatus::External => {
                mapping_outcome(evidence, sink, symbol, destination, Binding::External)
            }
            crate::implementation::PortStatus::Unsupported => {
                sink.push(
                    codes::IMPLEMENTATION_REQUIRED,
                    Some(symbol.to_owned()),
                    Some(Dimension::Behavioral),
                    reasons::HOOK_UNSUPPORTED,
                );
                HookOutcome {
                    classification: Classification::Unsupported,
                    binding: Some(Binding::Unsupported),
                    blocking: true,
                    reason: reasons::HOOK_UNSUPPORTED,
                    evidence_refs: vec![],
                }
            }
            crate::implementation::PortStatus::Missing => {
                sink.push(
                    codes::IMPLEMENTATION_REQUIRED,
                    Some(symbol.to_owned()),
                    Some(Dimension::Behavioral),
                    reasons::HOOK_MISSING,
                );
                HookOutcome {
                    classification: Classification::Unsupported,
                    binding: Some(Binding::Missing),
                    blocking: true,
                    reason: reasons::HOOK_MISSING,
                    evidence_refs: vec![],
                }
            }
        },
    }
}

fn mapping_outcome(
    evidence: Option<&EvidenceDocument>,
    sink: &mut Sink,
    symbol: &str,
    _destination: &str,
    binding: Binding,
) -> HookOutcome {
    let mappings: Vec<&wire::MappingRecord> = evidence
        .map(|e| {
            e.mappings
                .iter()
                .filter(|m| m.subject.as_deref() == Some(symbol))
                .collect()
        })
        .unwrap_or_default();
    let full = mappings
        .iter()
        .any(|m| m.coverage == wire::MappingCoverage::Full);
    let partial = mappings
        .iter()
        .any(|m| m.coverage == wire::MappingCoverage::Partial);
    let refs: Vec<String> = mappings.iter().map(|m| m.id.clone()).collect();
    if full {
        HookOutcome {
            classification: Classification::SupportedWithMapping,
            binding: Some(binding),
            blocking: false,
            reason: if binding == Binding::Foreign {
                reasons::HOOK_FOREIGN_MAPPED
            } else {
                reasons::HOOK_EXTERNAL_MAPPED
            },
            evidence_refs: refs,
        }
    } else if partial {
        sink.push(
            codes::MAPPING_PARTIAL,
            Some(symbol.to_owned()),
            Some(Dimension::Behavioral),
            if binding == Binding::Foreign {
                reasons::HOOK_FOREIGN_PARTIAL
            } else {
                reasons::HOOK_EXTERNAL_PARTIAL
            },
        );
        HookOutcome {
            classification: Classification::Partial,
            binding: Some(binding),
            blocking: true,
            reason: if binding == Binding::Foreign {
                reasons::HOOK_FOREIGN_PARTIAL
            } else {
                reasons::HOOK_EXTERNAL_PARTIAL
            },
            evidence_refs: refs,
        }
    } else {
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(symbol.to_owned()),
            Some(Dimension::Behavioral),
            reasons::HOOK_FOREIGN_UNMAPPED,
        );
        HookOutcome {
            classification: Classification::Unknown,
            binding: Some(binding),
            blocking: true,
            reason: reasons::HOOK_FOREIGN_UNMAPPED,
            evidence_refs: refs,
        }
    }
}

fn storage(
    evidence: Option<&EvidenceDocument>,
    sink: &mut Sink,
    subject: &super::requirements::Subject,
    destination: &str,
) -> ObligationRow {
    let base = |classification,
                blocking,
                reason: &'static str,
                required: Option<Policy>,
                offered: Option<Policy>| ObligationRow {
        destination: destination.to_owned(),
        dimension: Dimension::Storage,
        classification,
        verification: Verification::Untested,
        binding: None,
        blocking,
        reason: reason.to_owned(),
        required,
        offered,
        evidence_refs: vec![],
        witnesses: vec![],
    };
    let family = evidence.and_then(|e| {
        e.families
            .iter()
            .find(|f| f.family == wire::EvidenceFamily::Storage)
    });
    let Some(evidence) = evidence else {
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(subject.typed_id.clone()),
            Some(Dimension::Storage),
            reasons::STORAGE_INVENTORY_ABSENT,
        );
        return base(
            Classification::Unknown,
            true,
            reasons::STORAGE_INVENTORY_ABSENT,
            None,
            None,
        );
    };
    let required: Vec<&wire::StorageRequirement> = evidence
        .storage_required
        .iter()
        .filter(|r| r.subject == subject.typed_id)
        .collect();
    if required.is_empty() {
        if matches!(family, Some(f) if f.complete) {
            return base(
                Classification::Portable,
                false,
                reasons::NO_RESTRICTED_POLICY,
                None,
                None,
            );
        }
        let reason = if family.is_some() {
            reasons::STORAGE_INVENTORY_INCOMPLETE
        } else {
            reasons::STORAGE_INVENTORY_ABSENT
        };
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(subject.typed_id.clone()),
            Some(Dimension::Storage),
            reason,
        );
        return base(Classification::Unknown, true, reason, None, None);
    }
    let mut worst = Classification::Portable;
    let mut worst_reason = reasons::POLICY_COVERED;
    let mut blocking = false;
    let mut required_policy = None;
    let mut offered_policy = None;
    for requirement in &required {
        let offered = evidence
            .storage_offered
            .iter()
            .find(|o| o.target_id == destination && o.policy.kind == requirement.policy.kind);
        required_policy = Some(requirement.policy.clone());
        let Some(offered) = offered else {
            sink.push(
                codes::EVIDENCE_INCOMPLETE,
                Some(subject.typed_id.clone()),
                Some(Dimension::Storage),
                reasons::STORAGE_POLICY_UNKNOWN,
            );
            if Classification::Unknown.rank() < worst.rank() {
                worst = Classification::Unknown;
                worst_reason = reasons::STORAGE_POLICY_UNKNOWN;
                blocking = true;
                offered_policy = None;
            }
            continue;
        };
        offered_policy = Some(offered.policy.clone());
        if offered.policy.value == "unsupported" {
            sink.push(
                codes::CAPABILITY_UNSUPPORTED,
                Some(subject.typed_id.clone()),
                Some(Dimension::Storage),
                reasons::CAPABILITY_EXPLICIT_UNSUPPORTED,
            );
            if Classification::Unsupported.rank() < worst.rank() {
                worst = Classification::Unsupported;
                worst_reason = reasons::CAPABILITY_EXPLICIT_UNSUPPORTED;
                blocking = true;
            }
        } else if !policies_equal(&requirement.policy, &offered.policy) {
            sink.push(
                codes::BEHAVIOR_LOSS,
                Some(subject.typed_id.clone()),
                Some(Dimension::Storage),
                reasons::STORAGE_POLICY_LOSS,
            );
            if Classification::Partial.rank() < worst.rank() {
                worst = Classification::Partial;
                worst_reason = reasons::STORAGE_POLICY_LOSS;
                blocking = true;
            }
        } else if Classification::Portable.rank() < worst.rank() {
            worst = Classification::Portable;
            worst_reason = reasons::POLICY_COVERED;
        }
    }
    base(
        worst,
        blocking,
        worst_reason,
        required_policy,
        offered_policy,
    )
}

fn policies_equal(required: &Policy, offered: &Policy) -> bool {
    required.kind == offered.kind
        && required.value == offered.value
        && required.precision == offered.precision
        && required.scale == offered.scale
}

fn operational(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    sink: &mut Sink,
    subject: &super::requirements::Subject,
    destination: &str,
) -> ObligationRow {
    let base = |classification, blocking, reason: &'static str, evidence_refs: Vec<String>| {
        ObligationRow {
            destination: destination.to_owned(),
            dimension: Dimension::Operational,
            classification,
            verification: Verification::Untested,
            binding: None,
            blocking,
            reason: reason.to_owned(),
            required: None,
            offered: None,
            evidence_refs,
            witnesses: vec![],
        }
    };
    // Observed implicit hooks keep their inferred status and proof gap:
    // they stay qualified observations, never canonical Model members.
    if let Some(evidence) = evidence {
        if let Some(observation) = evidence.hooks.iter().find(|h| {
            h.subject == subject.typed_id
                && h.target_id == destination
                && h.state == wire::HookObservationState::ObservedUnsupported
        }) {
            sink.push(
                codes::BEHAVIOR_LOSS,
                Some(subject.typed_id.clone()),
                Some(Dimension::Operational),
                reasons::IMPLICIT_HOOK_UNSUPPORTED,
            );
            let _ = observation;
            return base(
                Classification::Partial,
                true,
                reasons::IMPLICIT_HOOK_UNSUPPORTED,
                vec![],
            );
        }
    }
    // Publication obligations join the declared profile capabilities.
    let has_publication = subject.kind == "effect" && !subject.emits.is_empty();
    let requirements: Vec<&wire::OperationalRequirement> = evidence
        .map(|e| {
            e.operational_requirements
                .iter()
                .filter(|r| r.subject == subject.typed_id)
                .collect()
        })
        .unwrap_or_default();
    if !has_publication && requirements.is_empty() {
        return base(
            Classification::NonApplicable,
            false,
            reasons::NO_OPERATIONAL_OBLIGATION,
            vec![],
        );
    }
    let profile = input
        .destinations
        .iter()
        .find(|d| d.target_id == destination)
        .and_then(|d| d.profile.as_ref());
    let Some(profile) = profile else {
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(subject.typed_id.clone()),
            Some(Dimension::Operational),
            reasons::CAPABILITY_ABSENT,
        );
        return base(
            Classification::Unknown,
            true,
            reasons::CAPABILITY_ABSENT,
            vec![],
        );
    };
    let required_capability = if has_publication {
        "events.publication".to_owned()
    } else {
        // The reviewed recipe bridges the required semantic capability to
        // the profile-owned capability namespace; an unmapped requirement
        // stays unknown.
        let recipe = input.recipe;
        requirements
            .iter()
            .filter_map(|r| {
                recipe
                    .document
                    .semantic_mappings
                    .iter()
                    .find(|m| m.requirement == r.requirement)
                    .map(|m| m.capability.clone())
            })
            .next()
            .unwrap_or_default()
    };
    if required_capability.is_empty() {
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(subject.typed_id.clone()),
            Some(Dimension::Operational),
            reasons::CAPABILITY_ABSENT,
        );
        return base(
            Classification::Unknown,
            true,
            reasons::CAPABILITY_ABSENT,
            vec![],
        );
    }
    let Some(capability) = profile
        .capabilities
        .iter()
        .find(|c| c.id == required_capability)
    else {
        // An unsatisfied declaration is not proof that the native
        // runtime cannot implement the capability.
        sink.push(
            codes::EVIDENCE_INCOMPLETE,
            Some(subject.typed_id.clone()),
            Some(Dimension::Operational),
            reasons::CAPABILITY_ABSENT,
        );
        return base(
            Classification::Unknown,
            true,
            reasons::CAPABILITY_ABSENT,
            vec![],
        );
    };
    match capability.support.as_str() {
        "full" => base(
            Classification::Portable,
            false,
            reasons::CAPABILITY_DECLARED,
            vec![required_capability],
        ),
        "partial" => {
            sink.push(
                codes::MAPPING_PARTIAL,
                Some(subject.typed_id.clone()),
                Some(Dimension::Operational),
                reasons::CAPABILITY_PARTIAL,
            );
            base(
                Classification::Partial,
                true,
                reasons::CAPABILITY_PARTIAL,
                vec![required_capability],
            )
        }
        _ => {
            sink.push(
                codes::EVIDENCE_INCOMPLETE,
                Some(subject.typed_id.clone()),
                Some(Dimension::Operational),
                reasons::CAPABILITY_ABSENT,
            );
            base(
                Classification::Unknown,
                true,
                reasons::CAPABILITY_ABSENT,
                vec![required_capability],
            )
        }
    }
}

/// The per-destination dimension summaries with the published
/// structural ratio and the verification partitions.
fn summarize(destinations: &[TargetInput], subjects: &[SubjectRow]) -> Vec<Summary> {
    destinations
        .iter()
        .map(|destination| {
            let mut dimensions: BTreeMap<Dimension, DimensionSummary> = BTreeMap::new();
            for dimension in Dimension::ALL {
                dimensions.insert(dimension, DimensionSummary::empty());
            }
            let mut blockers = 0;
            let mut unknowns = 0;
            let mut custom = 0;
            for subject in subjects {
                for row in &subject.rows {
                    if row.destination != destination.target_id {
                        continue;
                    }
                    let Some(entry) = dimensions.get_mut(&row.dimension) else {
                        continue;
                    };
                    match row.classification {
                        Classification::Portable => entry.portable += 1,
                        Classification::SupportedWithMapping => {
                            entry.supported_with_mapping += 1;
                            if matches!(row.binding, Some(Binding::Custom)) {
                                custom += 1;
                            }
                        }
                        Classification::Partial => entry.partial += 1,
                        Classification::TargetSpecific => entry.target_specific += 1,
                        Classification::Unsupported => entry.unsupported += 1,
                        Classification::Unknown => entry.unknown += 1,
                        Classification::NonApplicable => entry.non_applicable += 1,
                    }
                    if row.classification != Classification::NonApplicable {
                        entry.eligible_applicable += 1;
                    }
                    match row.verification {
                        Verification::Untested => entry.verification.untested += 1,
                        Verification::RecordedPass => entry.verification.recorded_pass += 1,
                        Verification::RecordedFail => entry.verification.recorded_fail += 1,
                        Verification::NotApplicable => entry.verification.not_applicable += 1,
                    }
                    if row.blocking {
                        blockers += 1;
                    }
                    if row.classification == Classification::Unknown {
                        unknowns += 1;
                    }
                }
            }
            for (dimension, entry) in dimensions.iter_mut() {
                if *dimension == Dimension::Structural && entry.eligible_applicable > 0 {
                    entry.structural_portable = Some(Ratio {
                        numerator: entry.portable,
                        denominator: entry.eligible_applicable,
                    });
                }
            }
            Summary {
                destination: destination.target_id.clone(),
                dimensions,
                blockers,
                unknowns,
                custom_obligations: custom,
            }
        })
        .collect()
}
