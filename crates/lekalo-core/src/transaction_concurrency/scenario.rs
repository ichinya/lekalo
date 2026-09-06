//! Deterministic concurrency race cases keyed to Scenario IR documents
//! (issue #24).
//!
//! A case is pure typed data: participants and invocations point at
//! existing #23 `when` steps, the schedule is an explicit deterministic
//! partial order with optional barriers — never a promise about OS thread
//! order — and expected outcomes are closed typed enums, never raw
//! values. Execution belongs to the owner harnesses (#47/#56/#107/#31);
//! a serial execution of two invokes never satisfies a race fixture.

use crate::scenario::id::{NamespacedId, SemanticId, StepId};

use super::identity::LocalId;

/// The typed expected outcome of one participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ExpectedOutcome {
    /// The invocation succeeded.
    Success,
    /// The invocation lost a race and returned the typed conflict.
    Conflict,
    /// The invocation failed with a declared error.
    Error,
    /// Infrastructure failed; neither pass nor conformance.
    Infrastructure,
    /// The backend capability is missing; reported, never passed.
    Unsupported,
    /// An explicitly degraded result.
    Degraded,
    /// Compensation ran and recovery is required.
    RecoveryRequired,
}

impl ExpectedOutcome {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Conflict => "conflict",
            Self::Error => "error",
            Self::Infrastructure => "infrastructure",
            Self::Unsupported => "unsupported",
            Self::Degraded => "degraded",
            Self::RecoveryRequired => "recovery_required",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "success" => Some(Self::Success),
            "conflict" => Some(Self::Conflict),
            "error" => Some(Self::Error),
            "infrastructure" => Some(Self::Infrastructure),
            "unsupported" => Some(Self::Unsupported),
            "degraded" => Some(Self::Degraded),
            "recovery_required" => Some(Self::RecoveryRequired),
            _ => None,
        }
    }
}

/// The exact Scenario IR binding of one case: the stable scenario id,
/// its explicit contract version, and the digest of the exact canonical
/// IR payload the steps were compiled from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioRef {
    pub(crate) scenario_id: SemanticId,
    pub(crate) scenario_version: crate::lockfile::types::SemVer,
    pub(crate) ir_digest: crate::lockfile::types::Sha256Digest,
}

impl ScenarioRef {
    /// The stable scenario identity.
    pub fn scenario_id(&self) -> &SemanticId {
        &self.scenario_id
    }

    /// The scenario's explicit contract version.
    pub fn scenario_version(&self) -> &crate::lockfile::types::SemVer {
        &self.scenario_version
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &crate::lockfile::types::Sha256Digest {
        &self.ir_digest
    }
}

/// One racing participant, bound to an existing `when` step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Participant {
    pub(crate) participant_id: LocalId,
    pub(crate) step_id: StepId,
}

impl Participant {
    /// The participant identity.
    pub fn participant_id(&self) -> &LocalId {
        &self.participant_id
    }

    /// The referenced `when` step.
    pub fn step_id(&self) -> &StepId {
        &self.step_id
    }
}

/// One concrete invocation of a participant's step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invocation {
    pub(crate) invocation_id: LocalId,
    pub(crate) participant_id: LocalId,
    pub(crate) step_id: StepId,
}

impl Invocation {
    /// The invocation identity.
    pub fn invocation_id(&self) -> &LocalId {
        &self.invocation_id
    }

    /// The acting participant.
    pub fn participant_id(&self) -> &LocalId {
        &self.participant_id
    }

    /// The invoked `when` step.
    pub fn step_id(&self) -> &StepId {
        &self.step_id
    }
}

/// One schedule node: an invocation or a barrier join point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleNode {
    pub(crate) node_id: LocalId,
    pub(crate) invocation: Option<LocalId>,
    pub(crate) joins: Vec<LocalId>,
}

impl ScheduleNode {
    /// The node identity.
    pub fn node_id(&self) -> &LocalId {
        &self.node_id
    }

    /// The invoked invocation, for `invoke` nodes.
    pub fn invocation(&self) -> Option<&LocalId> {
        self.invocation.as_ref()
    }

    /// The predecessor nodes this node waits for (unordered set).
    pub fn joins(&self) -> &[LocalId] {
        &self.joins
    }
}

/// One named barrier over two or more schedule nodes: all must complete
/// before any proceeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Barrier {
    pub(crate) barrier_id: LocalId,
    pub(crate) waits_for: Vec<LocalId>,
}

impl Barrier {
    /// The barrier identity.
    pub fn barrier_id(&self) -> &LocalId {
        &self.barrier_id
    }

    /// The joined nodes (unordered set).
    pub fn waits_for(&self) -> &[LocalId] {
        &self.waits_for
    }
}

/// One expected participant outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutcomeExpectation {
    pub(crate) participant_id: LocalId,
    pub(crate) outcome: ExpectedOutcome,
}

impl OutcomeExpectation {
    /// The participant.
    pub fn participant_id(&self) -> &LocalId {
        &self.participant_id
    }

    /// The expected typed outcome.
    pub const fn outcome(&self) -> ExpectedOutcome {
        self.outcome
    }
}

/// One deterministic concurrency case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcurrencyCase {
    pub(crate) case_id: NamespacedId,
    pub(crate) scenario_ref: ScenarioRef,
    pub(crate) participants: Vec<Participant>,
    pub(crate) invocations: Vec<Invocation>,
    pub(crate) schedule: Vec<ScheduleNode>,
    pub(crate) barriers: Vec<Barrier>,
    pub(crate) expected_invariants: Vec<NamespacedId>,
    pub(crate) expected_outcomes: Vec<OutcomeExpectation>,
    pub(crate) capability_refs: Vec<NamespacedId>,
}

impl ConcurrencyCase {
    /// The validated case identifier.
    pub fn case_id(&self) -> &NamespacedId {
        &self.case_id
    }

    /// The exact Scenario IR binding.
    pub const fn scenario_ref(&self) -> &ScenarioRef {
        &self.scenario_ref
    }

    /// The racing participants.
    pub fn participants(&self) -> &[Participant] {
        &self.participants
    }

    /// The concrete invocations.
    pub fn invocations(&self) -> &[Invocation] {
        &self.invocations
    }

    /// The deterministic partial order.
    pub fn schedule(&self) -> &[ScheduleNode] {
        &self.schedule
    }

    /// The named barriers.
    pub fn barriers(&self) -> &[Barrier] {
        &self.barriers
    }

    /// The invariants the committed state must satisfy, in declared
    /// order.
    pub fn expected_invariants(&self) -> &[NamespacedId] {
        &self.expected_invariants
    }

    /// The expected typed outcomes, keyed by participant.
    pub fn expected_outcomes(&self) -> &[OutcomeExpectation] {
        &self.expected_outcomes
    }

    /// The referenced capability-requirement ids.
    pub fn capability_refs(&self) -> &[NamespacedId] {
        &self.capability_refs
    }
}
