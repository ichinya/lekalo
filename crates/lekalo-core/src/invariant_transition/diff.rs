//! Pure semantic comparison of two same-family attachments (issue
//! #63).
//!
//! The comparison answers one question per changed path with one
//! closed class: **breaking** (a declared guarantee was removed or
//! weakened — a state space, invariant, or transition removed, or a
//! bound declaration changed), **non-breaking** (an addition,
//! description, scenario-reference, precondition, or policy change
//! under the evolution policy), and **policy-change** (verification
//! mapping or property-hint changes). Invalid inputs — foreign
//! identities or mixed Model/IR/attachment revisions — are the typed
//! error set, never a guessed classification. Paths are deterministic
//! and byte-sorted.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::InvariantTransitionAttachment;

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or weakened.
    Breaking,
    /// An addition or descriptive change under the evolution policy.
    NonBreaking,
    /// A verification or hint change with unchanged guarantees.
    PolicyChange,
}

impl DiffClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
        }
    }
}

/// One changed path with its class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    class: DiffClass,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// The finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two same-family attachments. Pure and read-only.
pub fn compare(
    base: &InvariantTransitionAttachment,
    candidate: &InvariantTransitionAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::input_invalid("diff-project-mismatch"));
    }
    if base.attachment_revision().as_str() != candidate.attachment_revision().as_str()
        || base.model_ref().digest.as_str() != candidate.model_ref().digest.as_str()
        || base.ir_digest().as_str() != candidate.ir_digest().as_str()
    {
        return Err(diagnostic::input_invalid("diff-mixed-revision"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();

    // State spaces: removal is breaking, addition is non-breaking, and
    // any member change of a common space is breaking.
    let base_space_ids: Vec<&str> = base
        .state_spaces()
        .iter()
        .map(|space| space.state_space_id().as_str())
        .collect();
    let candidate_space_ids: Vec<&str> = candidate
        .state_spaces()
        .iter()
        .map(|space| space.state_space_id().as_str())
        .collect();
    for id in &base_space_ids {
        if !candidate_space_ids.contains(id) {
            push_path(
                &format!("stateSpaces/{id}"),
                DiffClass::Breaking,
                &mut paths,
            );
        }
    }
    for space in base.state_spaces() {
        if let Some(other) = candidate
            .state_spaces()
            .iter()
            .find(|other| other.state_space_id() == space.state_space_id())
        {
            let state_set = |space: &super::state::StateSpace| {
                let mut states: Vec<String> = space
                    .states()
                    .iter()
                    .map(|state| {
                        format!(
                            "{}/{}/{}",
                            state.state_id().as_str(),
                            u8::from(state.initial()),
                            u8::from(state.terminal())
                        )
                    })
                    .collect();
                states.sort();
                states
            };
            if space.entity() != other.entity()
                || space.cycle_policy() != other.cycle_policy()
                || space.dead_policy() != other.dead_policy()
                || state_set(space) != state_set(other)
            {
                push_path(
                    &format!("stateSpaces/{}", space.state_space_id().as_str()),
                    DiffClass::Breaking,
                    &mut paths,
                );
            }
        }
    }

    // Invariants: kind, subject, fields, and predicate are the
    // guarantee; description and scenario references are descriptive.
    let base_invariant_ids: Vec<&str> = base
        .invariants()
        .iter()
        .map(|item| item.invariant_id().as_str())
        .collect();
    let candidate_invariant_ids: Vec<&str> = candidate
        .invariants()
        .iter()
        .map(|item| item.invariant_id().as_str())
        .collect();
    for id in &base_invariant_ids {
        if !candidate_invariant_ids.contains(id) {
            push_path(&format!("invariants/{id}"), DiffClass::Breaking, &mut paths);
        }
    }
    for id in &candidate_invariant_ids {
        if !base_invariant_ids.contains(id) {
            push_path(
                &format!("invariants/{id}"),
                DiffClass::NonBreaking,
                &mut paths,
            );
        }
    }
    for invariant in base.invariants() {
        if let Some(other) = candidate
            .invariants()
            .iter()
            .find(|other| other.invariant_id() == invariant.invariant_id())
        {
            if invariant.kind() != other.kind()
                || invariant.state_space_id() != other.state_space_id()
                || invariant.fields() != other.fields()
                || invariant.predicate() != other.predicate()
                || invariant.min() != other.min()
                || invariant.max() != other.max()
                || invariant.trigger_state() != other.trigger_state()
                || invariant.allowed_states() != other.allowed_states()
                || invariant.partition() != other.partition()
                || invariant.max_active() != other.max_active()
                || invariant.aggregate_ref() != other.aggregate_ref()
                || invariant.required_fields() != other.required_fields()
                || invariant.capability_requirement() != other.capability_requirement()
                || invariant.conflict_error_ref() != other.conflict_error_ref()
                || invariant.error_refs() != other.error_refs()
            {
                push_path(
                    &format!("invariants/{}", invariant.invariant_id().as_str()),
                    DiffClass::Breaking,
                    &mut paths,
                );
            } else if invariant.description() != other.description()
                || invariant.requirement_refs() != other.requirement_refs()
                || invariant.scenario_refs() != other.scenario_refs()
                || invariant.capability_refs() != other.capability_refs()
            {
                push_path(
                    &format!("invariants/{}", invariant.invariant_id().as_str()),
                    DiffClass::NonBreaking,
                    &mut paths,
                );
            }
        }
    }

    // Transitions: subject, from-set, target, command, and the
    // assignment set are the guarantee; preconditions, policy, and
    // scenario references are guards that may tighten without
    // weakening the declared machine.
    let base_transition_ids: Vec<&str> = base
        .transitions()
        .iter()
        .map(|item| item.transition_id().as_str())
        .collect();
    let candidate_transition_ids: Vec<&str> = candidate
        .transitions()
        .iter()
        .map(|item| item.transition_id().as_str())
        .collect();
    for id in &base_transition_ids {
        if !candidate_transition_ids.contains(id) {
            push_path(
                &format!("transitions/{id}"),
                DiffClass::Breaking,
                &mut paths,
            );
        }
    }
    for id in &candidate_transition_ids {
        if !base_transition_ids.contains(id) {
            push_path(
                &format!("transitions/{id}"),
                DiffClass::NonBreaking,
                &mut paths,
            );
        }
    }
    for transition in base.transitions() {
        if let Some(other) = candidate
            .transitions()
            .iter()
            .find(|other| other.transition_id() == transition.transition_id())
        {
            let narrowed = transition.state_space_id() != other.state_space_id()
                || transition.to_state() != other.to_state()
                || transition.command() != other.command()
                || transition
                    .from_states()
                    .iter()
                    .any(|state| !other.from_states().contains(state))
                || transition
                    .assignments()
                    .iter()
                    .any(|assignment| !other.assignments().contains(assignment))
                || transition.error_refs() != other.error_refs();
            let widened = other
                .from_states()
                .iter()
                .any(|state| !transition.from_states().contains(state))
                || other
                    .assignments()
                    .iter()
                    .any(|assignment| !transition.assignments().contains(assignment));
            if narrowed {
                push_path(
                    &format!("transitions/{}", transition.transition_id().as_str()),
                    DiffClass::Breaking,
                    &mut paths,
                );
            } else if widened
                || transition.parallel() != other.parallel()
                || transition.preconditions() != other.preconditions()
                || transition.policy_ref() != other.policy_ref()
                || transition.requirement_refs() != other.requirement_refs()
                || transition.scenario_refs() != other.scenario_refs()
                || transition.capability_refs() != other.capability_refs()
            {
                push_path(
                    &format!("transitions/{}", transition.transition_id().as_str()),
                    DiffClass::NonBreaking,
                    &mut paths,
                );
            }
        }
    }

    // Verification mappings and property hints are pure policy data.
    let base_mapping_ids: Vec<&str> = base
        .verification_mappings()
        .iter()
        .map(|item| item.mapping_id().as_str())
        .collect();
    let candidate_mapping_ids: Vec<&str> = candidate
        .verification_mappings()
        .iter()
        .map(|item| item.mapping_id().as_str())
        .collect();
    for id in &base_mapping_ids {
        if !candidate_mapping_ids.contains(id) {
            push_path(
                &format!("verificationMappings/{id}"),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }
    for id in &candidate_mapping_ids {
        if !base_mapping_ids.contains(id) {
            push_path(
                &format!("verificationMappings/{id}"),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }
    for mapping in base.verification_mappings() {
        if let Some(other) = candidate
            .verification_mappings()
            .iter()
            .find(|other| other.mapping_id() == mapping.mapping_id())
        {
            if mapping != other {
                push_path(
                    &format!("verificationMappings/{}", mapping.mapping_id().as_str()),
                    DiffClass::PolicyChange,
                    &mut paths,
                );
            }
        }
    }
    let base_hint_ids: Vec<&str> = base
        .property_hints()
        .iter()
        .map(|item| item.hint_id().as_str())
        .collect();
    let candidate_hint_ids: Vec<&str> = candidate
        .property_hints()
        .iter()
        .map(|item| item.hint_id().as_str())
        .collect();
    for id in &base_hint_ids {
        if !candidate_hint_ids.contains(id) {
            push_path(
                &format!("propertyHints/{id}"),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }
    for id in &candidate_hint_ids {
        if !base_hint_ids.contains(id) {
            push_path(
                &format!("propertyHints/{id}"),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }
    for hint in base.property_hints() {
        if let Some(other) = candidate
            .property_hints()
            .iter()
            .find(|other| other.hint_id() == hint.hint_id())
        {
            if hint != other {
                push_path(
                    &format!("propertyHints/{}", hint.hint_id().as_str()),
                    DiffClass::PolicyChange,
                    &mut paths,
                );
            }
        }
    }

    paths.sort();
    let equal = paths.is_empty();
    Ok(DiffResult { equal, paths })
}

/// Record one changed path once.
fn push_path(path: &str, class: DiffClass, paths: &mut Vec<DiffPath>) {
    let path = DiffPath {
        path: path.to_owned(),
        class,
    };
    if !paths.contains(&path) {
        paths.push(path);
    }
}
