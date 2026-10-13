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
) -> AffectedJoins {
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
            // An authored executed state is a claim, never evidence:
            // it starts demoted to declared and joins as executed or
            // failed only through qualified receipt custody for a
            // destination this assessment actually assesses, reconciled
            // with that custody's result. Custody naming another
            // destination never upgrades or fails this gate, and the
            // multi-destination aggregate is conservative: any
            // qualified failure dominates and an execution never erases
            // an earlier failure.
            let mut state = match gate.state.as_join() {
                JoinState::Executed => JoinState::Declared,
                other => other,
            };
            for destination in &assessed_destinations {
                if let Some(result) = input
                    .gates_qualified
                    .get(&(gate.gate.clone(), (*destination).to_owned()))
                {
                    state = if *result {
                        JoinState::Executed
                    } else {
                        JoinState::Failed
                    };
                    if state == JoinState::Failed {
                        break;
                    }
                }
            }
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
/// Every clipped list is reported through the assessment sink so the
/// report names its own clipping. `affected` is the **untruncated
/// domain join** of this scope: the coverage-gap verdict, its
/// collect-evidence rows, and the affected counts are derived from the
/// full join, never from the emission-clipped lists — a gap beyond the
/// emitted join prefix still marks the surface incomplete and denies
/// the strict gate.
#[allow(clippy::too_many_arguments)]
pub(super) fn migration_surface(
    input: &Assessment,
    evidence: Option<&EvidenceDocument>,
    selected: &[&Subject],
    subjects: &[super::wire::SubjectRow],
    affected: &AffectedJoins,
    truncated: bool,
    sink: &mut super::assess::Sink,
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
    // The gate-coverage gaps of this exact scope: every affected
    // scenario no gate record covers becomes explicit collect-evidence
    // planning work — the missing coverage never disappears from the
    // surface, and it marks the surface incomplete. The join is the
    // untruncated domain set, so a gap the emission clip would drop
    // still carries its row and flips the typed verdict.
    let covered: BTreeSet<&str> = evidence
        .map(|e| {
            e.gates
                .iter()
                .flat_map(|gate| gate.scenarios.iter().map(String::as_str))
                .collect()
        })
        .unwrap_or_default();
    let mut coverage_gaps = false;
    for scenario in &affected.0 {
        if covered.contains(scenario.scenario.as_str()) {
            continue;
        }
        coverage_gaps = true;
        rows.push(PlanningRow {
            id: format!(
                "plan.collect-evidence.gate-coverage.{}",
                scenario.scenario.replace(':', ".")
            ),
            subject: format!("scenario:{}", scenario.scenario),
            member: None,
            dimension: Dimension::Operational,
            destination: None,
            action: Action::CollectEvidence,
            reason: reasons::GATE_COVERAGE_UNKNOWN.to_owned(),
            requirement: None,
            required: None,
            offered: None,
            prerequisites: Vec::new(),
            scenarios: vec![scenario.scenario.clone()],
            gates: Vec::new(),
            evidence_needed: vec!["gate-record".into()],
        });
    }
    // The published planning-row bounds: every list the row carries is
    // capped at its schema maximum with a named clipping diagnostic,
    // and every id is bounded to the published semanticId length — an
    // overlong obligation identity keeps a readable prefix plus a
    // collision-free digest of its full tuple, so legal input maxima
    // can never produce an id a conforming consumer must reject.
    let mut clip_prerequisites = false;
    let mut clip_join_lists = false;
    for row in rows.iter_mut() {
        if row.prerequisites.len() > super::assess::EMIT_BOUNDS.prerequisites {
            row.prerequisites
                .truncate(super::assess::EMIT_BOUNDS.prerequisites);
            clip_prerequisites = true;
        }
        if row.scenarios.len() > super::assess::EMIT_BOUNDS.join_list {
            row.scenarios.truncate(super::assess::EMIT_BOUNDS.join_list);
            clip_join_lists = true;
        }
        if row.gates.len() > super::assess::EMIT_BOUNDS.join_list {
            row.gates.truncate(super::assess::EMIT_BOUNDS.join_list);
            clip_join_lists = true;
        }
        if row.id.len() > MAX_SEMANTIC_ID {
            row.id = bound_planning_id(row);
        }
    }
    if clip_prerequisites {
        sink.push(
            codes::ANALYSIS_INCOMPLETE,
            None,
            None,
            "prerequisites-clipped",
        );
    }
    if clip_join_lists {
        sink.push(codes::ANALYSIS_INCOMPLETE, None, None, "join-list-clipped");
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
        complete: !truncated && !coverage_gaps,
        rows,
    }
}

/// The published `semanticId` length bound of the report contract.
const MAX_SEMANTIC_ID: usize = 192;

/// The bounded spelling of one planning-row id. Short identities keep
/// their readable spelling verbatim; an overlong one keeps a bounded
/// readable prefix plus a 16-hex digest of the full obligation tuple
/// (action, destination, dimension, subject, requirement, required
/// policy), so the id is stable, injective in practice, and always
/// inside the published bound — never a length-dependent truncation
/// collision.
fn bound_planning_id(row: &PlanningRow) -> String {
    let requirement_part = row
        .requirement
        .as_ref()
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
    let destination = row.destination.as_deref().unwrap_or("-");
    let core = format!(
        "{}.{}.{}.{}{}{}",
        row.action.as_str(),
        destination,
        row.dimension.as_str(),
        row.subject.replace(':', "."),
        requirement_part,
        policy_part
    );
    let tuple = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        row.action.as_str(),
        destination,
        row.dimension.as_str(),
        row.subject,
        row.requirement.as_deref().unwrap_or(""),
        policy_part
    );
    let digest = crate::digest::sha256_hex(tuple.as_bytes());
    // "plan." + head + "." + 16 digest hex, within 192.
    let budget = MAX_SEMANTIC_ID - 5 - 17;
    let mut head: String = core.chars().take(budget).collect();
    while head.ends_with('.') {
        head.pop();
    }
    format!("plan.{head}.{}", &digest[..16])
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
                    .push((gate.gate.clone(), gate.state.as_join()));
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
/// coverage gaps — a truncated extraction, an empty assessment, a clipped
/// planning surface, or an affected scenario with no gate coverage is a
/// false green, never a pass.
///
/// Every verdict input here is a typed member of the report, never the
/// bounded diagnostics presentation channel: a schema-legal evidence
/// manifest can push thousands of ordinary findings into the
/// diagnostics list, and a verdict that lived there would be crowdable
/// by exactly the untrusted input this feature exists to distrust.
pub fn denial(report: &super::wire::Report) -> bool {
    if report.coverage.truncated || !report.coverage.complete {
        return true;
    }
    // The typed migration-surface verdict: an incomplete surface (a
    // gate-coverage gap or a clipped planning list) denies even when
    // the presentation channel has been crowded.
    if !report.migration_surface.complete {
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
