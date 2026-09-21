//! The typed NFR measured-evidence model (issue #85).
//!
//! One evidence set is the measured truth of exactly one environment:
//! results pin the exact constraint id and revision they were produced
//! against, carry valueState-typed measurements (the #120 vocabulary:
//! a value is known or it is one of the explicit absences — unknown,
//! unsupported, withheld; a never-executed run reports unknown, never
//! a fabricated zero), the result status (the #63 vocabulary), and an
//! owner-held artifact digest. Bytes never enter.

use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::NamespacedId;

use super::constraint::Method;
use super::environment::Environment;
use super::id::{ConstraintId, Decimal, IsoDate};

/// The closed measured-value state vocabulary (#120).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MeasuredValue {
    /// The measurement produced a value.
    Known(Decimal),
    /// The measurement did not produce a value.
    Unknown,
    /// The runner cannot measure this metric.
    Unsupported,
    /// The value exists but is withheld from the wire.
    Withheld,
}

impl MeasuredValue {
    /// The exact state key.
    pub const fn state(&self) -> &'static str {
        match self {
            Self::Known(_) => "known",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
            Self::Withheld => "withheld",
        }
    }

    /// The known value, when the state is known.
    pub const fn known(&self) -> Option<&Decimal> {
        match self {
            Self::Known(value) => Some(value),
            _ => None,
        }
    }
}

/// The closed result-status vocabulary (#63): pass, fail, skipped,
/// unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ResultStatus {
    /// The run passed.
    Pass,
    /// The run failed.
    Fail,
    /// The run was skipped.
    Skipped,
    /// The run outcome is unknown.
    Unknown,
}

impl ResultStatus {
    /// The closed vocabulary in canonical order.
    pub const KEYS: [Self; 4] = [Self::Pass, Self::Fail, Self::Skipped, Self::Unknown];

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skipped => "skipped",
            Self::Unknown => "unknown",
        }
    }

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// One typed measurement row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Measurement {
    metric: String,
    percentile: Option<super::constraint::Percentile>,
    value: MeasuredValue,
    unit: String,
}

impl Measurement {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(
        metric: String,
        percentile: Option<super::constraint::Percentile>,
        value: MeasuredValue,
        unit: String,
    ) -> Self {
        Self {
            metric,
            percentile,
            value,
            unit,
        }
    }

    /// The measured metric name.
    pub fn metric(&self) -> &str {
        &self.metric
    }

    /// The measured percentile, when the metric carries one.
    pub const fn percentile(&self) -> Option<super::constraint::Percentile> {
        self.percentile
    }

    /// The measured value with its state.
    pub const fn value(&self) -> &MeasuredValue {
        &self.value
    }

    /// The measurement unit, exactly as declared.
    pub fn unit(&self) -> &str {
        &self.unit
    }
}

/// The typed scenario provenance of one result: provenance only — a
/// measurement can never satisfy a scenario assertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioRef {
    scenario_id: String,
    scenario_version: String,
    ir_digest: Sha256Digest,
}

impl ScenarioRef {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(
        scenario_id: String,
        scenario_version: String,
        ir_digest: Sha256Digest,
    ) -> Self {
        Self {
            scenario_id,
            scenario_version,
            ir_digest,
        }
    }

    /// The scenario semantic id.
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    /// The scenario version.
    pub fn scenario_version(&self) -> &str {
        &self.scenario_version
    }

    /// The pinned IR digest.
    pub fn ir_digest(&self) -> &str {
        self.ir_digest.as_str()
    }
}

/// One measured result pinned to one constraint revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceResult {
    constraint_id: ConstraintId,
    constraint_revision: String,
    method: Method,
    measured_on: IsoDate,
    source_revision: String,
    run_ref: Option<NamespacedId>,
    gate_ref: Option<NamespacedId>,
    scenario_ref: Option<ScenarioRef>,
    measurements: Vec<Measurement>,
    result_status: ResultStatus,
    evidence_digest: Sha256Digest,
    expires_on: Option<IsoDate>,
}

impl EvidenceResult {
    /// Assemble from validated parts (wire internal).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        constraint_id: ConstraintId,
        constraint_revision: String,
        method: Method,
        measured_on: IsoDate,
        source_revision: String,
        run_ref: Option<NamespacedId>,
        gate_ref: Option<NamespacedId>,
        scenario_ref: Option<ScenarioRef>,
        measurements: Vec<Measurement>,
        result_status: ResultStatus,
        evidence_digest: Sha256Digest,
        expires_on: Option<IsoDate>,
    ) -> Self {
        Self {
            constraint_id,
            constraint_revision,
            method,
            measured_on,
            source_revision,
            run_ref,
            gate_ref,
            scenario_ref,
            measurements,
            result_status,
            evidence_digest,
            expires_on,
        }
    }

    /// The measured constraint.
    pub const fn constraint_id(&self) -> &ConstraintId {
        &self.constraint_id
    }

    /// The exact constraint revision this result pins.
    pub fn constraint_revision(&self) -> &str {
        &self.constraint_revision
    }

    /// The measurement method.
    pub const fn method(&self) -> Method {
        self.method
    }

    /// The measurement date.
    pub const fn measured_on(&self) -> &IsoDate {
        &self.measured_on
    }

    /// The owner revision token of the measured artifact.
    pub fn source_revision(&self) -> &str {
        &self.source_revision
    }

    /// The optional namespaced run reference.
    pub const fn run_ref(&self) -> Option<&NamespacedId> {
        self.run_ref.as_ref()
    }

    /// The optional gate reference.
    pub const fn gate_ref(&self) -> Option<&NamespacedId> {
        self.gate_ref.as_ref()
    }

    /// The optional scenario provenance.
    pub const fn scenario_ref(&self) -> Option<&ScenarioRef> {
        self.scenario_ref.as_ref()
    }

    /// The measured rows.
    pub fn measurements(&self) -> &[Measurement] {
        &self.measurements
    }

    /// The run outcome.
    pub const fn result_status(&self) -> ResultStatus {
        self.result_status
    }

    /// The owner-held artifact digest.
    pub fn evidence_digest(&self) -> &str {
        self.evidence_digest.as_str()
    }

    /// The expiry date, when the owner pinned one.
    pub const fn expires_on(&self) -> Option<&IsoDate> {
        self.expires_on.as_ref()
    }
}

/// One finished evidence set: the measured truth of exactly one
/// environment, immutable and canonically ordered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceSet {
    project_id: crate::scenario::id::SemanticId,
    environment: Environment,
    results: Vec<EvidenceResult>,
}

impl EvidenceSet {
    /// Assemble from validated parts (wire internal); results are
    /// stored in the caller's canonical order.
    pub(crate) fn assemble(
        project_id: crate::scenario::id::SemanticId,
        environment: Environment,
        results: Vec<EvidenceResult>,
    ) -> Self {
        Self {
            project_id,
            environment,
            results,
        }
    }

    /// The measured project.
    pub fn project_id(&self) -> &crate::scenario::id::SemanticId {
        &self.project_id
    }

    /// The exact environment of every result in this set.
    pub const fn environment(&self) -> &Environment {
        &self.environment
    }

    /// The results, canonically ordered.
    pub fn results(&self) -> &[EvidenceResult] {
        &self.results
    }
}
