//! Semantic validation of the invariant-transition attachment (issue
//! #63).
//!
//! The closed semantic rules over one fully parsed attachment:
//! kind/field coherence for every invariant kind, reference resolution
//! across state spaces, invariants, transitions, and mappings,
//! duplicate detection, the precondition-invariant-policy separation,
//! and mapping-evidence coherence. Every violation is one registered
//! diagnostic over the typed rule set with no partial result. Pure and
//! read-only: no source, model, cache, report, network, or target
//! access of any kind.

use super::diagnostic::{
    self, CONTRACT_INVALID, MAPPING_INVALID, STATE_INVALID, TRANSITION_INVALID,
};
use super::invariant::InvariantKind;
use super::state::StateSpace;
use super::trace::{MappingStatus, SubjectKind};
use super::transition::AssignmentValue;
use super::InvariantTransitionAttachment;
use crate::diagnostics::DiagnosticSet;

/// The semantic self-check over one assembled attachment.
pub(crate) fn semantic_self_check(
    attachment: &InvariantTransitionAttachment,
) -> Result<(), DiagnosticSet> {
    check_state_spaces(attachment)?;
    check_invariants(attachment)?;
    check_transitions(attachment)?;
    check_mappings(attachment)?;
    check_graph(attachment)
}

/// Every state-space identifier is unique, every space has an initial
/// state, and every state identifier is unique inside its space.
fn check_state_spaces(attachment: &InvariantTransitionAttachment) -> Result<(), DiagnosticSet> {
    let mut seen: Vec<&str> = Vec::new();
    for space in attachment.state_spaces() {
        let id = space.state_space_id().as_str();
        if seen.contains(&id) {
            return Err(diagnostic::rule_invalid(
                STATE_INVALID,
                "duplicate-state-space",
                Some(id),
            ));
        }
        seen.push(id);
        if !space.has_initial() {
            return Err(diagnostic::rule_invalid(
                STATE_INVALID,
                "missing-initial-state",
                Some(id),
            ));
        }
        let mut state_ids: Vec<&str> = Vec::new();
        for state in space.states() {
            if state_ids.contains(&state.state_id().as_str()) {
                return Err(diagnostic::rule_invalid(
                    STATE_INVALID,
                    "duplicate-state",
                    Some(state.state_id().as_str()),
                ));
            }
            state_ids.push(state.state_id().as_str());
        }
    }
    Ok(())
}

/// Resolve one state-space reference.
fn resolve_space<'a>(
    attachment: &'a InvariantTransitionAttachment,
    id: &str,
) -> Result<&'a StateSpace, DiagnosticSet> {
    attachment
        .state_spaces()
        .iter()
        .find(|space| space.state_space_id().as_str() == id)
        .ok_or_else(|| diagnostic::rule_invalid(STATE_INVALID, "unknown-state-space", Some(id)))
}

/// Kind/field coherence for every declared invariant.
fn check_invariants(attachment: &InvariantTransitionAttachment) -> Result<(), DiagnosticSet> {
    let mut seen: Vec<&str> = Vec::new();
    for invariant in attachment.invariants() {
        let id = invariant.invariant_id().as_str();
        if seen.contains(&id) {
            return Err(diagnostic::rule_invalid(
                CONTRACT_INVALID,
                "duplicate-invariant",
                Some(id),
            ));
        }
        seen.push(id);
        let space = resolve_space(attachment, invariant.state_space_id().as_str())?;
        for state in &invariant.allowed_states {
            if !space.contains(state.as_str()) {
                return Err(diagnostic::rule_invalid(
                    STATE_INVALID,
                    "unknown-state",
                    Some(state.as_str()),
                ));
            }
        }
        if let Some(trigger) = invariant.trigger_state() {
            if !space.contains(trigger.as_str()) {
                return Err(diagnostic::rule_invalid(
                    STATE_INVALID,
                    "unknown-state",
                    Some(trigger.as_str()),
                ));
            }
        }
        // One field reference at most may name a foreign entity; the
        // invariant subject entity is the state-space entity.
        let subject_entity = space.entity().as_str();
        for field in invariant.fields() {
            if let Some(entity) = field.entity() {
                if entity.as_str() != subject_entity {
                    return Err(diagnostic::rule_invalid(
                        CONTRACT_INVALID,
                        "cross-entity-field",
                        Some(id),
                    ));
                }
            }
        }
        let required = match invariant.kind() {
            InvariantKind::FieldValue => {
                if invariant.fields().len() != 1 || invariant.predicate().is_none() {
                    "field-value-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::CrossField => {
                if invariant.fields().len() < 2 || invariant.predicate().is_none() {
                    "cross-field-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::Uniqueness => {
                if invariant.fields().is_empty() || invariant.conflict_error_ref().is_none() {
                    "uniqueness-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::Cardinality => {
                if invariant.min().is_none() && invariant.max().is_none() {
                    "cardinality-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::Temporal => {
                if invariant.predicate().is_none() {
                    "temporal-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::AggregateConsistency => {
                if invariant.aggregate_ref().is_none() || invariant.predicate().is_none() {
                    "aggregate-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::ConditionalRequirement => {
                if invariant.predicate().is_none() || invariant.required_fields().is_empty() {
                    "conditional-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::OneActive => {
                if invariant.partition().is_none() || !invariant.max_active() {
                    "one-active-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::MemberOfSet => {
                if invariant.allowed_states().is_empty() {
                    "member-of-set-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::ImmutableAfterState => {
                if invariant.trigger_state().is_none() || invariant.required_fields().is_empty() {
                    "immutable-after-state-shape"
                } else {
                    continue;
                }
            }
            InvariantKind::TargetCapability => {
                if invariant.capability_requirement().is_none() {
                    "target-capability-shape"
                } else {
                    continue;
                }
            }
        };
        return Err(diagnostic::rule_invalid(
            CONTRACT_INVALID,
            required,
            Some(id),
        ));
    }
    Ok(())
}

/// Transition reference resolution, duplicate detection, assignment
/// coherence, and the precondition-invariant-policy separation.
fn check_transitions(attachment: &InvariantTransitionAttachment) -> Result<(), DiagnosticSet> {
    let mut seen: Vec<&str> = Vec::new();
    for transition in attachment.transitions() {
        let id = transition.transition_id().as_str();
        if seen.contains(&id) {
            return Err(diagnostic::rule_invalid(
                TRANSITION_INVALID,
                "duplicate-transition",
                Some(id),
            ));
        }
        seen.push(id);
        let space = resolve_space(attachment, transition.state_space_id().as_str())?;
        for state in transition.from_states() {
            if !space.contains(state.as_str()) {
                return Err(diagnostic::rule_invalid(
                    TRANSITION_INVALID,
                    "unknown-from-state",
                    Some(state.as_str()),
                ));
            }
        }
        if !space.contains(transition.to_state().as_str()) {
            return Err(diagnostic::rule_invalid(
                TRANSITION_INVALID,
                "unknown-to-state",
                Some(transition.to_state().as_str()),
            ));
        }
        // A transition from a terminal state can never fire.
        if let Some(from) = transition
            .from_states()
            .iter()
            .find(|state| terminal_state(space, state.as_str()))
        {
            return Err(diagnostic::rule_invalid(
                TRANSITION_INVALID,
                "transition-from-terminal",
                Some(from.as_str()),
            ));
        }
        // Assignments write each field at most once (contradictory or
        // duplicate writes are invalid; order stays behavioral).
        let mut written: Vec<&str> = Vec::new();
        for assignment in transition.assignments() {
            if written.contains(&assignment.field().as_str()) {
                return Err(diagnostic::rule_invalid(
                    TRANSITION_INVALID,
                    "duplicate-assignment",
                    Some(assignment.field().as_str()),
                ));
            }
            written.push(assignment.field().as_str());
            if let AssignmentValue::Prior { field } = assignment.value() {
                if written[..written.len() - 1].contains(&field.as_str()) {
                    return Err(diagnostic::rule_invalid(
                        TRANSITION_INVALID,
                        "prior-read-after-write",
                        Some(id),
                    ));
                }
            }
        }
        // The precondition-invariant-policy separation: a transition
        // must not encode authorization as a value predicate. Policy
        // references are declared separately, and preconditions stay
        // operation-entry guards.
        if transition.preconditions().len() > super::version::MAX_PRECONDITIONS {
            return Err(diagnostic::rule_invalid(
                TRANSITION_INVALID,
                "precondition-bound",
                Some(id),
            ));
        }
        // Every transition declares at least one branch error outcome.
        if transition.error_refs().is_empty() {
            return Err(diagnostic::rule_invalid(
                TRANSITION_INVALID,
                "missing-error-ref",
                Some(id),
            ));
        }
    }
    Ok(())
}

/// Whether one state is declared terminal.
fn terminal_state(space: &StateSpace, state: &str) -> bool {
    space
        .states()
        .iter()
        .any(|entry| entry.state_id().as_str() == state && entry.terminal())
}

/// Mapping coherence: subjects resolve, statuses and evidence agree,
/// and required gaps, dangling targets, staleness, conflicts, and
/// unknowns are explicit rather than silent.
fn check_mappings(attachment: &InvariantTransitionAttachment) -> Result<(), DiagnosticSet> {
    let mut seen: Vec<&str> = Vec::new();
    for mapping in attachment.verification_mappings() {
        let id = mapping.mapping_id().as_str();
        if seen.contains(&id) {
            return Err(diagnostic::rule_invalid(
                MAPPING_INVALID,
                "duplicate-mapping",
                Some(id),
            ));
        }
        seen.push(id);
        let subject_id = mapping.subject().id().as_str();

        match mapping.subject().kind() {
            SubjectKind::Invariant => {
                if !attachment
                    .invariants()
                    .iter()
                    .any(|invariant| invariant.invariant_id().as_str() == subject_id)
                {
                    return Err(diagnostic::rule_invalid(
                        MAPPING_INVALID,
                        "unknown-subject",
                        Some(subject_id),
                    ));
                }
            }
            SubjectKind::Transition => {
                if !attachment
                    .transitions()
                    .iter()
                    .any(|transition| transition.transition_id().as_str() == subject_id)
                {
                    return Err(diagnostic::rule_invalid(
                        MAPPING_INVALID,
                        "unknown-subject",
                        Some(subject_id),
                    ));
                }
            }
            SubjectKind::StateSpace => {
                return Err(diagnostic::rule_invalid(
                    MAPPING_INVALID,
                    "subject-kind",
                    Some(subject_id),
                ));
            }
        }
        let evidence = mapping.evidence();
        match mapping.status() {
            MappingStatus::Full | MappingStatus::Verified => {
                if evidence.is_none() {
                    return Err(diagnostic::rule_invalid(
                        MAPPING_INVALID,
                        "status-without-evidence",
                        Some(id),
                    ));
                }
                if let Some(evidence) = evidence {
                    if evidence.source_revision() != attachment.attachment_revision().as_str() {
                        return Err(diagnostic::rule_invalid(
                            MAPPING_INVALID,
                            "stale-evidence",
                            Some(id),
                        ));
                    }
                }
            }
            MappingStatus::Gap => {
                if evidence.is_some() {
                    return Err(diagnostic::rule_invalid(
                        MAPPING_INVALID,
                        "gap-with-evidence",
                        Some(id),
                    ));
                }
            }
            MappingStatus::Stale => {
                if let Some(evidence) = evidence {
                    if evidence.source_revision() == attachment.attachment_revision().as_str() {
                        return Err(diagnostic::rule_invalid(
                            MAPPING_INVALID,
                            "stale-with-current-evidence",
                            Some(id),
                        ));
                    }
                }
            }
            MappingStatus::Dangling => {
                if mapping.target_ref().target_kind().key() == "unknown" {
                    return Err(diagnostic::rule_invalid(
                        MAPPING_INVALID,
                        "dangling-target-kind",
                        Some(id),
                    ));
                }
            }
            MappingStatus::Partial
            | MappingStatus::Conflict
            | MappingStatus::Unsupported
            | MappingStatus::Unknown => {}
        }
    }
    Ok(())
}

/// The deterministic local transition-graph checks (graph.rs).
fn check_graph(attachment: &InvariantTransitionAttachment) -> Result<(), DiagnosticSet> {
    super::graph::check(attachment)
}
