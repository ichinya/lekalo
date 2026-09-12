//! The reference-evaluation trace records (issue #107).
//!
//! One finished trace is immutable, deterministic, and tied to
//! semantic identity everywhere: the scenario id and digest, every
//! step id, every operation, transition, entity, effect, and error
//! reference is the exact stable identifier of the consumed contracts.
//! Verdicts and outcomes are closed typed enums — a scenario whose
//! expectations do not hold is a `fail` verdict, never a diagnostic,
//! and a semantics the reference does not implement is `unsupported`
//! with one fixed reason token, never a guess.

use crate::scenario::value::TypedValue;

/// The overall status of one evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// Every assertion passed and no step was unsupported.
    Pass,
    /// At least one assertion failed.
    Fail,
    /// At least one step or assertion hit explicitly unsupported
    /// reference semantics (and none failed).
    Unsupported,
}

impl Status {
    /// The exact wire key.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unsupported => "unsupported",
        }
    }
}
/// The closed typed-error tokens the executor can produce. They are
/// runtime outcome classes of the reference semantics; mapping them
/// onto declared #62 error identities stays with the error-contract
/// owner — the trace records the declared branch references instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorToken {
    /// No row matches the command's identity input.
    RowNotFound,
    /// An operation-entry precondition evaluated false.
    PreconditionFailed,
    /// A post-write invariant failed and the transaction rolled back.
    InvariantViolated,
    /// A single-row query matched no row.
    QueryEmpty,
    /// A single-row query matched more than one row.
    QueryAmbiguous,
}

impl ErrorToken {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RowNotFound => "row-not-found",
            Self::PreconditionFailed => "precondition-failed",
            Self::InvariantViolated => "invariant-violated",
            Self::QueryEmpty => "query-empty",
            Self::QueryAmbiguous => "query-ambiguous",
        }
    }
}

/// The outcome of one `when` step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// The operation succeeded; commands carry the unit output.
    Ok {
        /// The output value: queries carry their result, commands
        /// carry the null unit value.
        output: TypedValue,
    },
    /// The operation failed with one closed typed token.
    Error {
        /// The closed token of the failure class.
        token: ErrorToken,
        /// The declared error references of the branch, byte-sorted
        /// and deduplicated.
        declared: Vec<String>,
        /// The violated invariants, byte-sorted, for invariant
        /// failures; empty otherwise.
        violations: Vec<String>,
    },
    /// The reference semantics do not cover this step; the fixed
    /// reason token says why.
    Unsupported {
        /// The fixed reason token.
        reason: &'static str,
    },
}

impl Outcome {
    /// The exact wire key.
    pub(crate) const fn key(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "ok",
            Self::Error { .. } => "error",
            Self::Unsupported { .. } => "unsupported",
        }
    }
}

/// One `given` step record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GivenRecord {
    /// The stable scenario-local step identity.
    pub step_id: String,
    /// The closed precondition kind tag.
    pub kind: &'static str,
    /// Whether the step established its data, or the fixed reason it
    /// cannot.
    pub status: Result<(), &'static str>,
    /// The entity of a state step, when established.
    pub entity: Option<String>,
    /// The row key of a state step, when established.
    pub row: Option<String>,
}

/// One `when` step record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhenRecord {
    /// The stable scenario-local step identity.
    pub step_id: String,
    /// The invoked qualified operation.
    pub operation: String,
    /// The execution outcome.
    pub outcome: Outcome,
    /// The transition that executed, when one did.
    pub transition: Option<String>,
    /// The state space of the transition, when one executed.
    pub state_space: Option<String>,
    /// The evaluation clock the step used.
    pub clock: String,
    /// The recorded step this step replayed, when it did.
    pub replay_of: Option<String>,
    /// The effect-log indexes attributed to this step.
    pub effects: Vec<usize>,
    /// The IDs derived from referenced ID sources, in derivation
    /// order.
    pub derived_ids: Vec<TypedValue>,
}

/// The verdict of one `then` assertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// The expectation holds.
    Pass,
    /// The expectation does not hold; the fixed reason token says
    /// which check failed.
    Fail(&'static str),
    /// The reference semantics cannot decide this assertion; the
    /// fixed reason token says why.
    Unsupported(&'static str),
}

/// One `then` assertion record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssertionRecord {
    /// The stable scenario-local step identity.
    pub step_id: String,
    /// The observed `when` step.
    pub observes: String,
    /// The closed assertion kind tag.
    pub kind: &'static str,
    /// The verdict with its fixed reason token.
    pub verdict: Verdict,
}

/// The closed effect-log entry kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectKind {
    /// A write of one entity row.
    EntityWrite,
    /// The declared intent to emit one event.
    EventIntent,
    /// The declared intent to dispatch one job.
    JobIntent,
}

impl EffectKind {
    /// The exact wire key.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EntityWrite => "entity_write",
            Self::EventIntent => "event_intent",
            Self::JobIntent => "job_intent",
        }
    }
}

/// One effect-log entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectRecord {
    /// The global effect index, zero-based and gap-free.
    pub index: usize,
    /// The `when` step that produced the entry.
    pub step: String,
    /// The closed entry kind.
    pub kind: EffectKind,
    /// The written entity (entity writes).
    pub entity: Option<String>,
    /// The closed effect operation of the write (entity writes).
    pub operation: Option<&'static str>,
    /// The row key (entity writes).
    pub row: Option<String>,
    /// The written field names, byte-sorted (entity writes).
    pub fields: Vec<String>,
    /// The emitted event or job identity (intents).
    pub target: Option<String>,
}

/// One final-state row snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RowSnapshot {
    /// The entity identity.
    pub entity: String,
    /// The canonical row key.
    pub key: String,
    /// The field values, byte-sorted by field name.
    pub fields: Vec<(String, TypedValue)>,
}

/// One finished reference evaluation: immutable, deterministically
/// ordered, safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceTrace {
    /// The stable scenario identity.
    pub(crate) scenario_id: String,
    /// The scenario contract version.
    pub(crate) scenario_version: String,
    /// The digest of the exact canonical scenario payload.
    pub(crate) scenario_digest: String,
    /// The pinned Model version of the scenario.
    pub(crate) model_version: String,
    /// The pinned IR digest of the scenario.
    pub(crate) ir_digest: String,
    /// The consumed invariant-transition attachment revision.
    pub(crate) attachment_revision: String,
    /// The refusal token, when the evaluation refused before any
    /// step because the contract pins disagree.
    pub(crate) refusal: Option<&'static str>,
    /// The overall status.
    pub(crate) status: Status,
    /// The `given` step records, in scenario order.
    pub(crate) given: Vec<GivenRecord>,
    /// The `when` step records, in scenario order.
    pub(crate) when: Vec<WhenRecord>,
    /// The effect log, in execution order.
    pub(crate) effects: Vec<EffectRecord>,
    /// The final state, entity-major and key-sorted.
    pub(crate) state: Vec<RowSnapshot>,
    /// The assertion records, in scenario order.
    pub(crate) assertions: Vec<AssertionRecord>,
    /// The digest of the canonical final state.
    pub(crate) state_digest: String,
    /// The digest of the canonical effect log.
    pub(crate) effect_digest: String,
}

impl ReferenceTrace {
    /// The overall status.
    pub const fn status(&self) -> Status {
        self.status
    }

    /// The refusal token, when the evaluation refused the pins.
    pub const fn refusal(&self) -> Option<&'static str> {
        self.refusal
    }

    /// The stable scenario identity of the evaluated document.
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    /// The digest of the exact canonical scenario payload.
    pub fn scenario_digest(&self) -> &str {
        &self.scenario_digest
    }

    /// The `when` step records, in scenario order.
    pub fn when(&self) -> &[WhenRecord] {
        &self.when
    }

    /// The effect log, in execution order.
    pub fn effects(&self) -> &[EffectRecord] {
        &self.effects
    }

    /// The assertion records, in scenario order.
    pub fn assertions(&self) -> &[AssertionRecord] {
        &self.assertions
    }

    /// The final-state row snapshots.
    pub fn state(&self) -> &[RowSnapshot] {
        &self.state
    }

    /// The digest of the canonical final state.
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }

    /// The digest of the canonical effect log.
    pub fn effect_digest(&self) -> &str {
        &self.effect_digest
    }

    /// The `given` step records, in scenario order.
    pub fn given(&self) -> &[GivenRecord] {
        &self.given
    }
}
