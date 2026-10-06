//! Advisory migration-planning rows and the affected scenario/gate
//! joins (issue #19).
//!
//! Rows are stable, deduplicated, and advisory: an action never carries
//! a script, a command, a time quote, a code edit, or a data migration.
//! Counts are planning estimates under the reviewed recipe, never proof
//! of minimal implementation work.

use super::assess::{codes, reasons, Assessment};
use super::requirements::Subject;
use super::wire::{
    self, Action, AffectedGate, AffectedScenario, Binding, Classification, Dimension,
    EvidenceDocument, JoinState, MigrationSurface, ObligationRow, PlanningRow,
};
use std::collections::{BTreeMap, BTreeSet};

/// The affected scenario and gate joins of one report.
pub(super) type AffectedJoins = (Vec<AffectedScenario>, Vec<AffectedGate>);

/// Join the affected scenarios and gates: direct scenario coverage over
/// the selected subjects, with declared/planned/executed states kept
/// separate and unknown gate coverage left explicit.
pub(super) fn affected_joins(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    selected: &[&Subject],
    subjects: &[super::wire::SubjectRow],
) -> AffectedJoins {
    let _ = subjects;
    let selected_ids: BTreeSet<&str> = selected.iter().map(|s| s.id.as_str()).collect();
    let mut scenarios: BTreeMap<String, (JoinState, BTreeSet<String>)> = BTreeMap::new();
    for (scenario, covered) in &input.subjects.covered_by {
        if let Some(reached) = covered
            .iter()
            .find(|c| selected_ids.contains(c.as_str()))
            .cloned()
        {
            let entry = scenarios
                .entry(scenario.clone())
                .or_insert((JoinState::Declared, BTreeSet::new()));
            entry.1.insert(reached);
        }
    }
    if let Some(evidence) = evidence {
        for record in &evidence.scenarios {
            let Some(entry) = scenarios.get_mut(&record.scenario) else {
                continue;
            };
            entry.0 = match record.state {
                wire::ScenarioReceipt::RecordedPass => JoinState::Executed,
                wire::ScenarioReceipt::RecordedFail => JoinState::Planned,
                wire::ScenarioReceipt::Untested => JoinState::Declared,
            };
        }
    }
    let affected_scenarios: Vec<AffectedScenario> = scenarios
        .iter()
        .map(|(scenario, (state, subjects_))| AffectedScenario {
            scenario: scenario.clone(),
            state: *state,
            subjects: subjects_.iter().map(typed_id_for(selected)).collect(),
        })
        .collect();
    let affected_names: BTreeSet<&str> = scenarios.keys().map(String::as_str).collect();
    let mut gates: Vec<AffectedGate> = Vec::new();
    if let Some(evidence) = evidence {
        for gate in &evidence.gates {
            let joined: Vec<String> = gate
                .scenarios
                .iter()
                .filter(|s| affected_names.contains(s.as_str()))
                .cloned()
                .collect();
            if joined.is_empty() {
                continue;
            }
            gates.push(AffectedGate {
                gate: gate.gate.clone(),
                state: gate.state,
                scenarios: joined,
            });
        }
    }
    (affected_scenarios, gates)
}

fn typed_id_for<'a>(selected: &'a [&'a Subject]) -> impl Fn(&String) -> String + 'a {
    move |id: &String| {
        selected
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.typed_id.clone())
            .unwrap_or_else(|| format!("symbol:{id}"))
    }
}

/// Build the reconciled migration surface with the closed planning rows.
#[allow(clippy::too_many_arguments)]
pub(super) fn migration_surface(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    selected: &[&Subject],
    subjects: &[super::wire::SubjectRow],
    affected: &AffectedJoins,
    truncated: bool,
) -> MigrationSurface {
    let mut missing = 0u64;
    let mut mapped: BTreeSet<String> = BTreeSet::new();
    let mut storage_reviews: BTreeSet<String> = BTreeSet::new();
    let mut unresolved = 0u64;
    let mut rows: Vec<PlanningRow> = Vec::new();
    let gates_by_scenario = gate_index(evidence);
    for subject_row in subjects {
        let subject = match selected.iter().find(|s| s.typed_id == subject_row.subject) {
            Some(s) => *s,
            None => continue,
        };
        for row in &subject_row.rows {
            if row.blocking {
                unresolved += 1;
            }
            let gate_refs = covering_gates(&gates_by_scenario, &subject_ids_of(subject));
            let scenarios = covering_scenarios(input, subject);
            match row.classification {
                Classification::Unsupported => {
                    if row.dimension == Dimension::Behavioral {
                        missing += 1;
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::ImplementHook,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec![],
                        ));
                    }
                }
                Classification::Partial => {
                    if row.dimension == Dimension::Storage {
                        storage_reviews.insert(subject.typed_id.clone());
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::ReviewStoragePolicy,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec!["storage-offered".into()],
                        ));
                    } else if row.reason == reasons::IMPLICIT_HOOK_UNSUPPORTED {
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::ReplaceMapping,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec!["explicit-hook".into()],
                        ));
                    } else {
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::ReplaceMapping,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec!["full-mapping".into()],
                        ));
                    }
                }
                Classification::Unknown => {
                    rows.push(planning_row(
                        subject_row,
                        row,
                        Action::CollectEvidence,
                        gate_refs.clone(),
                        scenarios.clone(),
                        evidence_needed_for(row),
                    ));
                }
                Classification::TargetSpecific => {
                    rows.push(planning_row(
                        subject_row,
                        row,
                        Action::BindTarget,
                        gate_refs.clone(),
                        scenarios.clone(),
                        vec![],
                    ));
                }
                Classification::SupportedWithMapping => {
                    mapped.insert(subject.typed_id.clone());
                    if row.verification == wire::Verification::Untested {
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::VerifyScenario,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec!["scenario-receipt".into()],
                        ));
                    }
                }
                Classification::Portable | Classification::NonApplicable => {}
            }
        }
    }
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    rows.dedup_by(|left, right| left.id == right.id);
    MigrationSurface {
        missing_implementations: missing,
        mapped_symbols: mapped.len() as u64,
        storage_reviews: storage_reviews.len() as u64,
        affected_scenarios: affected.0.len() as u64,
        affected_gates: affected.1.len() as u64,
        unresolved_obligations: unresolved,
        complete: !truncated,
        rows,
    }
}

fn subject_ids_of(subject: &Subject) -> Vec<String> {
    vec![subject.id.clone()]
}

fn covering_scenarios(input: &Assessment, subject: &Subject) -> Vec<String> {
    let mut found: Vec<String> = input
        .subjects
        .covered_by
        .iter()
        .filter(|(_, covered)| covered.iter().any(|c| c == &subject.id))
        .map(|(scenario, _)| scenario.clone())
        .collect();
    found.sort();
    found
}

fn gate_index(evidence: Option<&EvidenceDocument>) -> BTreeMap<String, Vec<(String, JoinState)>> {
    let mut index: BTreeMap<String, Vec<(String, JoinState)>> = BTreeMap::new();
    if let Some(evidence) = evidence {
        for gate in &evidence.gates {
            for scenario in &gate.scenarios {
                index
                    .entry(scenario.clone())
                    .or_default()
                    .push((gate.gate.clone(), gate.state));
            }
        }
    }
    index
}

fn covering_gates(
    gates_by_scenario: &BTreeMap<String, Vec<(String, JoinState)>>,
    subject_ids: &[String],
) -> Vec<String> {
    let mut found: BTreeSet<String> = BTreeSet::new();
    for id in subject_ids {
        if let Some(gates) = gates_by_scenario.get(id) {
            found.extend(gates.iter().map(|(gate, _)| gate.clone()));
        }
    }
    found.into_iter().collect()
}

fn evidence_needed_for(row: &ObligationRow) -> Vec<String> {
    match row.reason.as_str() {
        reasons::IMPLEMENTATION_INVENTORY_ABSENT => vec!["implementation".into()],
        reasons::IMPLEMENTATION_INVENTORY_INCOMPLETE => vec!["implementation".into()],
        reasons::BINDING_EVIDENCE_ABSENT => vec!["implementation".into()],
        reasons::STORAGE_INVENTORY_ABSENT => vec!["storage".into()],
        reasons::STORAGE_INVENTORY_INCOMPLETE => vec!["storage".into()],
        reasons::STORAGE_POLICY_UNKNOWN => vec!["storage-offered".into()],
        reasons::CAPABILITY_ABSENT => vec!["adapter-snapshot".into()],
        reasons::HOOK_FOREIGN_UNMAPPED => vec!["semantic-mapping".into()],
        reasons::HOOK_EXTERNAL_UNMAPPED => vec!["semantic-mapping".into()],
        _ => vec![],
    }
}

fn planning_row(
    subject_row: &super::wire::SubjectRow,
    row: &ObligationRow,
    action: Action,
    gates: Vec<String>,
    scenarios: Vec<String>,
    evidence_needed: Vec<String>,
) -> PlanningRow {
    let destination_part = row.destination.replace('-', "");
    PlanningRow {
        id: format!(
            "plan.{}.{}.{}.{}",
            action.as_str(),
            destination_part,
            row.dimension.as_str(),
            subject_row.subject.replace(':', ".")
        ),
        subject: subject_row.subject.clone(),
        member: subject_row.member.clone(),
        dimension: row.dimension,
        destination: Some(row.destination.clone()),
        action,
        reason: row.reason.clone(),
        requirement: row
            .evidence_refs
            .first()
            .cloned()
            .or_else(|| subject_row.requirements.first().cloned()),
        prerequisites: subject_row.requirements.clone(),
        scenarios,
        gates,
        evidence_needed,
    }
}

/// Whether the strict `--check` gate must deny this report: any blocking
/// obligation (unsupported, partial/lossy, or unknown) denies.
pub fn denial(report: &super::wire::Report) -> bool {
    report
        .subjects
        .iter()
        .any(|subject| subject.rows.iter().any(|row| row.blocking))
}

/// The distinct registered diagnostic codes triggered by one report, in
/// stable order for the envelope diagnostic set.
pub fn triggered_codes(report: &super::wire::Report) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    let mut push = |code: &'static str| {
        if !found.contains(&code) {
            found.push(code);
        }
    };
    for diagnostic in &report.diagnostics {
        match diagnostic.code.as_str() {
            codes::EVIDENCE_INCOMPLETE => push("portability.evidence-incomplete"),
            codes::MAPPING_PARTIAL => push("portability.mapping-partial"),
            codes::CAPABILITY_UNSUPPORTED => push("portability.capability-unsupported"),
            codes::IMPLEMENTATION_REQUIRED => push("portability.implementation-required"),
            codes::BEHAVIOR_LOSS => push("portability.behavior-loss"),
            codes::ANALYSIS_INCOMPLETE => push("portability.analysis-incomplete"),
            _ => {}
        }
    }
    found
}

/// The binding-kind label of one destination binding, for the human
/// projection of custom/foreign/external obligations.
pub fn binding_label(binding: Option<Binding>) -> &'static str {
    match binding {
        Some(binding) => binding.as_str(),
        None => "none",
    }
}
