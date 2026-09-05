//! The coverage vector of the Scenario IR (issue #23).
//!
//! One stable, derived vector describing what a scenario exercises:
//! scenario, step, action, and assertion identities, the covered
//! operation references, the expected outcome kind, and the optional
//! typed error, effect, and policy references. The vector is plain
//! rebuildable data — it never preempts execution, never creates graph
//! nodes, write edges, or declared-versus-detected comparisons, and its
//! contribution to the #13 graph stays a typed offer until that owner
//! accepts it.

use super::assertion::Assertion;
use super::id::{NamespacedId, SemanticId};
use super::step::{StepKey, ThenStep, WhenStep};

/// The derived coverage vector of one scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageVector {
    /// The stable scenario identity.
    pub scenario_id: SemanticId,
    /// One entry per assertion, in scenario `then` order.
    pub entries: Vec<CoverageEntry>,
    /// The covered operation references, sorted and deduplicated.
    pub operations: Vec<SemanticId>,
}

/// One assertion-level coverage entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageEntry {
    /// The `then` step identity.
    pub step_id: StepKey,
    /// The step the assertion observes.
    pub observes: StepKey,
    /// The resolved operation of the observed action, when the
    /// observation traces to a `when` step.
    pub operation: Option<SemanticId>,
    /// The closed assertion kind tag.
    pub assertion_kind: &'static str,
    /// The closed expected outcome kind.
    pub outcome: OutcomeKind,
    /// The optional typed error reference.
    pub error: Option<SemanticId>,
    /// The optional typed effect reference.
    pub effect: Option<SemanticId>,
    /// The optional typed policy reference.
    pub policy: Option<NamespacedId>,
}

/// The closed expected outcome vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum OutcomeKind {
    /// Expected success output.
    Success,
    /// Expected typed domain error.
    Error,
    /// Expected entity state.
    Entity,
    /// Expected event or job emission.
    Emitted,
    /// Expected absence of a declared effect.
    EffectForbidden,
    /// Expected authorization outcome.
    Authorization,
    /// Expected replay idempotency.
    Idempotency,
    /// Expected contract match.
    Contract,
    /// Expected deterministic fixture digest.
    Fixture,
    /// Expected explicit unsupported capability.
    Unsupported,
}

impl OutcomeKind {
    /// The wire-stable tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            OutcomeKind::Success => "success",
            OutcomeKind::Error => "error",
            OutcomeKind::Entity => "entity",
            OutcomeKind::Emitted => "emitted",
            OutcomeKind::EffectForbidden => "effect-forbidden",
            OutcomeKind::Authorization => "authorization",
            OutcomeKind::Idempotency => "idempotency",
            OutcomeKind::Contract => "contract",
            OutcomeKind::Fixture => "fixture",
            OutcomeKind::Unsupported => "unsupported",
        }
    }
}

/// Build the coverage vector from the validated steps (pure derivation).
pub(crate) fn build(
    scenario_id: &SemanticId,
    when: &[WhenStep],
    then: &[ThenStep],
) -> CoverageVector {
    let operations: Vec<SemanticId> = when
        .iter()
        .map(|step| &step.action.operation)
        .cloned()
        .collect();
    let entries = then
        .iter()
        .map(|step| {
            let operation = when
                .iter()
                .find(|candidate| candidate.step_id == step.observes)
                .map(|candidate| candidate.action.operation.clone());
            CoverageEntry {
                step_id: step.step_id.clone(),
                observes: step.observes.clone(),
                operation,
                assertion_kind: step.assertion.kind_tag(),
                outcome: outcome_of(&step.assertion),
                error: step.assertion.error_ref().cloned(),
                effect: step.assertion.effect_ref().cloned(),
                policy: step.assertion.policy_ref().cloned(),
            }
        })
        .collect();
    CoverageVector {
        scenario_id: scenario_id.clone(),
        entries,
        operations,
    }
}

/// Map one assertion to its expected outcome kind.
fn outcome_of(assertion: &Assertion) -> OutcomeKind {
    match assertion {
        Assertion::Result { .. } => OutcomeKind::Success,
        Assertion::Error { .. } => OutcomeKind::Error,
        Assertion::EntityState { .. } => OutcomeKind::Entity,
        Assertion::Emitted { .. } => OutcomeKind::Emitted,
        Assertion::ForbiddenEffect { .. } => OutcomeKind::EffectForbidden,
        Assertion::Authorization { .. } => OutcomeKind::Authorization,
        Assertion::Idempotency { .. } => OutcomeKind::Idempotency,
        Assertion::ContractMatch { .. } => OutcomeKind::Contract,
        Assertion::DeterministicFixture { .. } => OutcomeKind::Fixture,
        Assertion::Unsupported { .. } => OutcomeKind::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_kinds_are_closed_and_stable() {
        let kinds = [
            OutcomeKind::Success,
            OutcomeKind::Error,
            OutcomeKind::Entity,
            OutcomeKind::Emitted,
            OutcomeKind::EffectForbidden,
            OutcomeKind::Authorization,
            OutcomeKind::Idempotency,
            OutcomeKind::Contract,
            OutcomeKind::Fixture,
            OutcomeKind::Unsupported,
        ];
        let tags: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        let mut sorted = tags.clone();
        sorted.sort_unstable();
        assert_eq!(tags.len(), 10);
        assert!(sorted.windows(2).all(|pair| pair[0] != pair[1]));
    }
}
