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

/// Join the affected scenarios and gates: direct scenario coverage plus
/// the bounded reverse dependency/effect witnesses over the selected
/// subjects, with every covered subject joined (never only the first),
/// receipts scoped to the assessed destinations, qualified receipts
/// only, and the conservative aggregate failed > executed > declared.
pub(super) fn affected_joins(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    selected: &[&Subject],
    subjects: &[super::wire::SubjectRow],
) -> AffectedJoins {
    let _ = subjects;
    let selected_ids: BTreeSet<&str> = selected.iter().map(|s| s.id.as_str()).collect();
    // The typed dependency/effect witnesses of the selected subjects:
    // a scenario covering a (reverse) dependent is an affected witness
    // of the selection, joined within the bounded extraction closure.
    let mut witness_ids: BTreeSet<&str> = selected_ids.clone();
    for subject in selected {
        for witness in subject.witnesses.iter().chain(subject.dependents.iter()) {
            if let Some((_, semantic)) = witness.split_once(':') {
                witness_ids.insert(semantic);
            }
        }
    }
    let assessed_destinations: BTreeSet<&str> = input
        .destinations
        .iter()
        .map(|d| d.target_id.as_str())
        .collect();
    let mut scenarios: BTreeMap<String, (JoinState, BTreeSet<String>)> = BTreeMap::new();
    for (scenario, covered) in &input.subjects.covered_by {
        // Every selected subject the scenario reaches joins, in stable
        // (sorted) order; a first-match drop would misstate the surface.
        let reached: BTreeSet<&str> = covered
            .iter()
            .filter(|c| selected_ids.contains(c.as_str()))
            .map(|c| c.as_str())
            .collect();
        let witnessed = covered
            .iter()
            .any(|c| witness_ids.contains(c.as_str()) && !selected_ids.contains(c.as_str()));
        if reached.is_empty() && !witnessed {
            continue;
        }
        let entry = scenarios
            .entry(scenario.clone())
            .or_insert((JoinState::Declared, BTreeSet::new()));
        for subject in reached {
            entry.1.insert(subject.to_owned());
        }
    }
    if let Some(evidence) = evidence {
        for record in &evidence.scenarios {
            let Some(entry) = scenarios.get_mut(&record.scenario) else {
                continue;
            };
            // A receipt joins only when it is target-qualified against
            // this assessment's destinations AND its custody was
            // verified: a claimed result without qualified receipt
            // custody never upgrades the join.
            let Some(target) = &record.target_id else {
                continue;
            };
            if !assessed_destinations.contains(target.as_str()) {
                continue;
            }
            if !input
                .receipts_qualified
                .contains(&(record.scenario.clone(), target.clone()))
            {
                continue;
            }
            // Conservative aggregate: a recorded failure dominates, an
            // execution never erases an earlier failure, and an untested
            // record never resets a recorded state.
            entry.0 = match record.state {
                wire::ScenarioReceipt::RecordedFail => JoinState::Failed,
                wire::ScenarioReceipt::RecordedPass if entry.0 != JoinState::Failed => {
                    JoinState::Executed
                }
                _ => entry.0,
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
            // An authored executed state is a claim, never evidence: it
            // joins only through qualified receipt custody, reconciled
            // with the custody's result; an unqualified claim demotes to
            // declared, and a qualified failure stays failed.
            let state = match gate.state {
                JoinState::Executed => match input.gates_qualified.get(&gate.gate) {
                    Some(true) => JoinState::Executed,
                    Some(false) => JoinState::Failed,
                    None => JoinState::Declared,
                },
                other => other,
            };
            gates.push(AffectedGate {
                gate: gate.gate.clone(),
                state,
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
    // One missing implementation per distinct (destination, hook):
    // an effect that joins its trigger's already-counted outcome never
    // doubles the count for one absent hook. Only the primary hook
    // carrier (the command or query) counts; effect rows keep their own
    // planning rows but not their own implementation count.
    let mut missing = 0u64;
    let mut missing_hooks: BTreeSet<(String, String)> = BTreeSet::new();
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
        let scenarios = covering_scenarios(input, subject);
        // Gates join through the subject's covering scenarios: the gate
        // index is keyed by scenario id, never by subject id.
        let gate_refs = covering_gates(&gates_by_scenario, &scenarios);
        for row in &subject_row.rows {
            if row.blocking {
                unresolved += 1;
            }
            match row.classification {
                Classification::Unsupported => {
                    if row.dimension == Dimension::Behavioral {
                        if matches!(subject.kind, "command" | "query")
                            && missing_hooks.insert((row.destination.clone(), subject.id.clone()))
                        {
                            missing += 1;
                        }
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::ImplementHook,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec![],
                        ));
                    } else if row.dimension == Dimension::Storage {
                        // An explicitly unsupported storage policy is a
                        // hard blocker with concrete migration work, not
                        // an obligation that disappears from planning.
                        storage_reviews.insert(subject.typed_id.clone());
                        rows.push(planning_row(
                            subject_row,
                            row,
                            Action::ReviewStoragePolicy,
                            gate_refs.clone(),
                            scenarios.clone(),
                            vec!["storage-offered".into()],
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
    // Only genuinely identical rows merge; distinct obligations always
    // keep distinct stable ids, and any residual base-id collision gets
    // an explicit occurrence suffix instead of silently dropping work.
    rows.dedup_by(|left, right| left == right);
    let mut occurrences: BTreeMap<String, u64> = BTreeMap::new();
    for row in rows.iter_mut() {
        let count = occurrences.entry(row.id.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            row.id = format!("{}.{}", row.id, count);
        }
    }
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

fn covering_scenarios(input: &Assessment, subject: &Subject) -> Vec<String> {
    // Direct coverage plus the bounded typed dependency/effect
    // witnesses: a scenario covering a (reverse) dependent of the
    // subject is an affected witness of the subject.
    let mut joined: BTreeSet<&str> = std::iter::once(subject.id.as_str()).collect();
    for witness in subject.witnesses.iter().chain(subject.dependents.iter()) {
        if let Some((_, semantic)) = witness.split_once(':') {
            joined.insert(semantic);
        }
    }
    let mut found: Vec<String> = input
        .subjects
        .covered_by
        .iter()
        .filter(|(_, covered)| covered.iter().any(|c| joined.contains(c.as_str())))
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
    // The destination stays verbatim in the id: an encoding that drops
    // characters collides distinct targets (go-service vs goservice)
    // and silently merges their obligations.
    let destination_part = row.destination.as_str();
    // The obligation identity joins the id when the row carries one,
    // keeping per-policy and per-requirement rows injective. A storage
    // row's exact required policy joins too, so two same-kind policy
    // obligations (decimal 18/6 vs decimal 16/4) are textually distinct
    // without occurrence suffixes.
    let requirement_part = row
        .evidence_refs
        .first()
        .map(|requirement| format!(".{}", requirement.replace(':', ".")))
        .unwrap_or_default();
    let policy_part = row
        .required
        .as_ref()
        .map(|policy| {
            let mut parts = vec![policy.kind.clone(), policy.value.clone()];
            if let Some(precision) = policy.precision {
                parts.push(precision.to_string());
            }
            if let Some(scale) = policy.scale {
                parts.push(scale.to_string());
            }
            format!(".{}", parts.join("."))
        })
        .unwrap_or_default();
    PlanningRow {
        id: format!(
            "plan.{}.{}.{}.{}{}{}",
            action.as_str(),
            destination_part,
            row.dimension.as_str(),
            subject_row.subject.replace(':', "."),
            requirement_part,
            policy_part
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
        required: row.required.clone(),
        offered: row.offered.clone(),
        prerequisites: subject_row.requirements.clone(),
        scenarios,
        gates,
        evidence_needed,
    }
}

/// Whether the strict `--check` gate must deny this report: any blocking
/// obligation (unsupported, partial/lossy, or unknown) denies, and so do
/// coverage gaps — a truncated extraction or an empty assessment is a
/// false green, never a pass.
pub fn denial(report: &super::wire::Report) -> bool {
    if report.coverage.truncated || !report.coverage.complete {
        return true;
    }
    if report.destinations.is_empty() || report.subjects.is_empty() {
        return true;
    }
    let any_rows = report
        .subjects
        .iter()
        .any(|subject| !subject.rows.is_empty());
    if !any_rows {
        return true;
    }
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
