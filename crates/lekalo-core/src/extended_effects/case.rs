//! Deterministic partial-failure cases keyed to Scenario IR documents
//! (issue #26).
//!
//! A case proves, as portable data, what one declared effect sequence
//! promises when a named fault class fires at a named step: an explicit
//! deterministic partial-order schedule (join sets, acyclicity
//! validated), one closed expected outcome per scheduled contract, and
//! the capability references the proof depends on. Execution and
//! evidence belong to the owner harnesses; the case never executes.

use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId, StepId};

/// The typed pin of one Scenario IR document: stable id, explicit
/// contract version, and the digest of the exact canonical IR payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioRef {
    pub(crate) scenario_id: SemanticId,
    pub(crate) scenario_version: SemVer,
    pub(crate) ir_digest: Sha256Digest,
}

impl ScenarioRef {
    /// The stable scenario id.
    pub fn scenario_id(&self) -> &str {
        self.scenario_id.as_str()
    }

    /// The declared scenario contract version.
    pub fn scenario_version(&self) -> &str {
        self.scenario_version.as_str()
    }

    /// The digest of the exact canonical IR payload.
    pub fn ir_digest(&self) -> &str {
        self.ir_digest.as_str()
    }
}

/// The closed fault-injection vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Fault {
    /// The step executes unchanged.
    None,
    /// The effect exceeds its declared timeout.
    Timeout,
    /// The effect fails with its typed error contract.
    Error,
    /// The infrastructure beneath the effect fails.
    Infrastructure,
    /// The step is cancelled before completion.
    Cancel,
}

impl Fault {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Timeout => "timeout",
            Self::Error => "error",
            Self::Infrastructure => "infrastructure",
            Self::Cancel => "cancel",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "timeout" => Some(Self::Timeout),
            "error" => Some(Self::Error),
            "infrastructure" => Some(Self::Infrastructure),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    /// Whether this fault disturbs the step.
    pub(crate) const fn disturbs(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// One declared step: the contract it exercises and the fault injected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseStep {
    pub(crate) step_id: StepId,
    pub(crate) contract_ref: NamespacedId,
    pub(crate) fault: Fault,
}

impl CaseStep {
    /// The scenario-local step id.
    pub fn step_id(&self) -> &str {
        self.step_id.as_str()
    }

    /// The exercised contract reference.
    pub fn contract_ref(&self) -> &str {
        self.contract_ref.as_str()
    }

    /// The injected fault class.
    pub const fn fault(&self) -> Fault {
        self.fault
    }
}

/// The closed schedule node vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NodeKind {
    /// Execute one declared step.
    Invoke,
    /// Observe the fault outcome of one declared step.
    Fault,
    /// Wait for two or more predecessor nodes.
    Barrier,
}

impl NodeKind {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Invoke => "invoke",
            Self::Fault => "fault",
            Self::Barrier => "barrier",
        }
    }

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
            "invoke" => Some(Self::Invoke),
            "fault" => Some(Self::Fault),
            "barrier" => Some(Self::Barrier),
            _ => None,
        }
    }
}

/// One schedule node with its explicit predecessor set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseNode {
    pub(crate) node_id: StepId,
    pub(crate) kind: NodeKind,
    pub(crate) step_id: Option<StepId>,
    pub(crate) joins: Vec<StepId>,
}

impl CaseNode {
    /// The scenario-local node id.
    pub fn node_id(&self) -> &str {
        self.node_id.as_str()
    }

    /// The node kind.
    pub const fn kind(&self) -> NodeKind {
        self.kind
    }

    /// The executed or observed step, when the kind carries one.
    pub fn step_id(&self) -> Option<&str> {
        self.step_id.as_ref().map(|step| step.as_str())
    }

    /// The sorted predecessor set.
    pub fn joins(&self) -> impl Iterator<Item = &str> {
        self.joins.iter().map(|join| join.as_str())
    }
}

/// The closed expected-outcome vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Outcome {
    /// The effect completed and its result is committed.
    Success,
    /// The effect lost a deterministic conflict.
    Conflict,
    /// The effect failed with its typed error contract.
    Error,
    /// The infrastructure beneath the effect failed.
    Infrastructure,
    /// The target cannot honor the declared guarantee.
    Unsupported,
    /// The effect completed in a declared degraded mode.
    Degraded,
    /// The declared compensation ran and recovery is required.
    RecoveryRequired,
}

impl Outcome {
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

    /// Parse the closed wire key.
    pub(crate) fn from_key(text: &str) -> Option<Self> {
        match text {
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

/// One expected outcome for one scheduled contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedOutcome {
    pub(crate) contract_ref: NamespacedId,
    pub(crate) outcome: Outcome,
}

impl ExpectedOutcome {
    /// The contracted effect the outcome speaks about.
    pub fn contract_ref(&self) -> &str {
        self.contract_ref.as_str()
    }

    /// The closed expected outcome.
    pub const fn outcome(&self) -> Outcome {
        self.outcome
    }
}

/// One partial-failure case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartialFailureCase {
    pub(crate) case_id: NamespacedId,
    pub(crate) scenario: ScenarioRef,
    pub(crate) steps: Vec<CaseStep>,
    pub(crate) schedule: Vec<CaseNode>,
    pub(crate) outcomes: Vec<ExpectedOutcome>,
    pub(crate) capability_refs: Vec<NamespacedId>,
}

impl PartialFailureCase {
    /// The case id.
    pub fn case_id(&self) -> &str {
        self.case_id.as_str()
    }

    /// The pinned Scenario IR document.
    pub const fn scenario(&self) -> &ScenarioRef {
        &self.scenario
    }

    /// The declared steps, canonically ordered by step id.
    pub fn steps(&self) -> &[CaseStep] {
        &self.steps
    }

    /// The deterministic schedule, canonically ordered by node id.
    pub fn schedule(&self) -> &[CaseNode] {
        &self.schedule
    }

    /// The expected outcomes, canonically ordered by contract.
    pub fn outcomes(&self) -> &[ExpectedOutcome] {
        &self.outcomes
    }

    /// The capability-requirement references, canonically sorted.
    pub fn capability_refs(&self) -> impl Iterator<Item = &str> {
        self.capability_refs
            .iter()
            .map(|reference| reference.as_str())
    }
}
