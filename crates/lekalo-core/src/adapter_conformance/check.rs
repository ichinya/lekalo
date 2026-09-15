//! The closed conformance check catalog (issue #31).
//!
//! Every check the suite can run is one typed entry with a stable id, one
//! failure class, and a fixed catalog position. The catalog — not a
//! filename list — is the contract: reports may only carry these ids, in
//! this order, and the verdict aggregation is a pure fold over outcomes.

/// The stable check identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CheckId {
    /// The mandatory describe handshake succeeds and returns a
    /// well-formed, grammar-valid capability map.
    DescribeHandshake,
    /// Version negotiation pins the session to the highest protocol
    /// version both sides support, proven by a describe at that version.
    DescribeNegotiation,
    /// Declared capability ids are registered, declared transports,
    /// scopes, and identity tokens are grammatical, and the evidence
    /// digest matches the canonical capability bytes.
    CapabilityDeclaration,
    /// On a 1.1.0+ session, every IR-carrying operation the adapter
    /// declares is backed by a declared accepted IR contract version.
    CapabilityIrDeclaration,
    /// Strict only: the adapter declares the complete v1 operation
    /// surface.
    CapabilitySurface,
    /// Canonical Lekalo and OpenSpec homes are byte-identical before and
    /// after every exchange of the whole run.
    ConfinementCanonical,
    /// Every declared write plan path stays inside the adapter's
    /// declared write scopes.
    ConfinementPlanScopes,
    /// The invalid-references fixture produces a well-formed classified
    /// response (findings or an in-envelope error), never a crash or
    /// garbage.
    InputInvalidIr,
    /// Every induced in-envelope operation error carries the closed
    /// error shape with bounded code and message lengths.
    DiagnosticsStructured,
    /// Repeated identical exchanges produce byte-identical canonical
    /// responses; nondeterminism is detected across the repeats.
    DeterminismRepeats,
    /// The generate dry run produces a verified write plan whose paths,
    /// actions, and digests match the observed project state.
    GenerateDryRunPlan,
    /// The apply publishes exactly the declared writes and nothing else.
    GenerateApplyPlan,
    /// A failed or refused apply consumes its authority; the retry path
    /// replans instead of replaying.
    ApplyRetryDiscipline,
    /// The clean cycle plans only deletions and removes exactly the
    /// planned files.
    PlanCleanCycle,
    /// The scenario fixture's verify result is normalized: sorted,
    /// closed, and byte-identical across repeated runs.
    ScenarioNormalization,
    /// The applied artifact bytes hash-match the declared plan digests,
    /// recorded as manifest evidence.
    ArtifactManifestEvidence,
    /// Durable evidence stays redacted: no absolute paths, host roots,
    /// or raw provider text in scan entries or the report.
    RedactionEvidence,
    /// A cancelled read-only exchange is classified, the child is
    /// reaped, and a fresh handshake recovers.
    ProcessCancellation,
}

impl CheckId {
    /// The stable dotted wire id.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DescribeHandshake => "describe.handshake",
            Self::DescribeNegotiation => "describe.negotiation",
            Self::CapabilityDeclaration => "capability.declaration",
            Self::CapabilityIrDeclaration => "capability.ir-declaration",
            Self::CapabilitySurface => "capability.surface",
            Self::ConfinementCanonical => "confinement.canonical",
            Self::ConfinementPlanScopes => "confinement.plan-scopes",
            Self::InputInvalidIr => "input.invalid-ir",
            Self::DiagnosticsStructured => "diagnostics.structured",
            Self::DeterminismRepeats => "determinism.repeats",
            Self::GenerateDryRunPlan => "generate.dry-run-plan",
            Self::GenerateApplyPlan => "generate.apply-plan",
            Self::ApplyRetryDiscipline => "apply.retry-discipline",
            Self::PlanCleanCycle => "plan.clean-cycle",
            Self::ScenarioNormalization => "scenario.normalization",
            Self::ArtifactManifestEvidence => "artifact.manifest-evidence",
            Self::RedactionEvidence => "redaction.evidence",
            Self::ProcessCancellation => "process.cancellation",
        }
    }

    /// The JUnit class name of the check.
    pub const fn classname(self) -> &'static str {
        match self {
            Self::DescribeHandshake | Self::DescribeNegotiation => "lekalo.adapter.describe",
            Self::CapabilityDeclaration
            | Self::CapabilityIrDeclaration
            | Self::CapabilitySurface => "lekalo.adapter.capability",
            Self::ConfinementCanonical | Self::ConfinementPlanScopes => {
                "lekalo.adapter.confinement"
            }
            Self::InputInvalidIr => "lekalo.adapter.input",
            Self::DiagnosticsStructured => "lekalo.adapter.diagnostics",
            Self::DeterminismRepeats => "lekalo.adapter.determinism",
            Self::GenerateDryRunPlan | Self::GenerateApplyPlan | Self::ApplyRetryDiscipline => {
                "lekalo.adapter.generate"
            }
            Self::PlanCleanCycle => "lekalo.adapter.plan",
            Self::ScenarioNormalization => "lekalo.adapter.scenario",
            Self::ArtifactManifestEvidence => "lekalo.adapter.artifact",
            Self::RedactionEvidence => "lekalo.adapter.redaction",
            Self::ProcessCancellation => "lekalo.adapter.process",
        }
    }

    /// The failure class a failed outcome of this check carries.
    pub const fn class(self) -> CheckClass {
        match self {
            Self::ConfinementCanonical | Self::ConfinementPlanScopes | Self::RedactionEvidence => {
                CheckClass::Security
            }
            Self::DescribeHandshake
            | Self::DescribeNegotiation
            | Self::CapabilityDeclaration
            | Self::CapabilityIrDeclaration => CheckClass::Protocol,
            Self::ProcessCancellation => CheckClass::Process,
            Self::DeterminismRepeats => CheckClass::Determinism,
            _ => CheckClass::Feature,
        }
    }

    /// The fixed catalog position of the check.
    pub const fn position(self) -> usize {
        self as usize
    }
}

/// The closed failure classes. The class decides the verdict before any
/// count is taken: security and protocol failures are never compensated
/// by passing feature tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CheckClass {
    /// An optional capability misbehaved; compensable by nothing but a
    /// fix, yet ranked below the hard classes.
    Feature,
    /// Repeated runs disagree; the deterministic-output contract broke.
    Determinism,
    /// The adapter process could not complete an exchange.
    Process,
    /// The adapter cannot prove it speaks the published protocol.
    Protocol,
    /// A confinement or redaction guarantee was violated.
    Security,
}

impl CheckClass {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Feature => "feature",
            Self::Determinism => "determinism",
            Self::Process => "process",
            Self::Protocol => "protocol",
            Self::Security => "security",
        }
    }
}

/// The closed outcome states.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CheckState {
    /// The check ran and held.
    Pass,
    /// The check ran and broke; the class records how hard.
    Fail,
    /// The check could not apply to this adapter; a skipped check never
    /// counts as a pass and withholds the badge.
    Skipped,
}

impl CheckState {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skipped => "skipped",
        }
    }
}
/// One recorded check outcome. The class is the class of the recorded
/// result, not merely the check's inherent class: a crash during a
/// feature check is a process-class failure, and a refusal during any
/// exchange is a security-class one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckOutcome {
    /// The stable check id.
    pub id: CheckId,
    /// The class of the recorded result.
    pub class: CheckClass,
    /// The outcome state.
    pub state: CheckState,
    /// A bounded, redacted detail token for failures and skips.
    pub detail: Option<&'static str>,
}

impl CheckOutcome {
    /// A passing outcome of the check's inherent class.
    pub const fn pass(id: CheckId) -> Self {
        Self {
            id,
            class: id.class(),
            state: CheckState::Pass,
            detail: None,
        }
    }

    /// A failed outcome with its actual class and bounded detail token.
    pub const fn fail(id: CheckId, class: CheckClass, detail: &'static str) -> Self {
        Self {
            id,
            class,
            state: CheckState::Fail,
            detail: Some(detail),
        }
    }

    /// A skipped outcome with its bounded reason token.
    pub const fn skipped(id: CheckId, reason: &'static str) -> Self {
        Self {
            id,
            class: id.class(),
            state: CheckState::Skipped,
            detail: Some(reason),
        }
    }
}

/// The full catalog in its fixed order.
pub const CATALOG: [CheckId; 18] = [
    CheckId::DescribeHandshake,
    CheckId::DescribeNegotiation,
    CheckId::CapabilityDeclaration,
    CheckId::CapabilityIrDeclaration,
    CheckId::CapabilitySurface,
    CheckId::ConfinementCanonical,
    CheckId::ConfinementPlanScopes,
    CheckId::InputInvalidIr,
    CheckId::DiagnosticsStructured,
    CheckId::DeterminismRepeats,
    CheckId::GenerateDryRunPlan,
    CheckId::GenerateApplyPlan,
    CheckId::ApplyRetryDiscipline,
    CheckId::PlanCleanCycle,
    CheckId::ScenarioNormalization,
    CheckId::ArtifactManifestEvidence,
    CheckId::RedactionEvidence,
    CheckId::ProcessCancellation,
];

/// The aggregate verdict of one run, derived from the check outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Verdict {
    /// Every run check held; skips exist only where the design allows.
    Pass,
    /// A feature or determinism check failed.
    Feature,
    /// A process-level exchange could not complete.
    Process,
    /// The adapter cannot prove it speaks the published protocol.
    Protocol,
    /// A confinement or redaction guarantee was violated.
    Security,
}

impl Verdict {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Feature => "feature",
            Self::Process => "process",
            Self::Protocol => "protocol",
            Self::Security => "security",
        }
    }

    /// The domain status the verdict projects. Security cannot be
    /// compensated, protocol cannot be compensated, and a broken process
    /// is an availability classification; only plain feature failures
    /// are the invalid class.
    pub const fn status(self) -> crate::result::Status {
        match self {
            Self::Pass => crate::result::Status::Valid,
            Self::Feature => crate::result::Status::Invalid,
            Self::Process => crate::result::Status::Unavailable,
            Self::Protocol => crate::result::Status::Unsupported,
            Self::Security => crate::result::Status::Denied,
        }
    }

    /// Derive the verdict from outcomes. Hard classes dominate in the
    /// order security, protocol, process; then any failure; a run with
    /// neither failures nor hard classes passes.
    pub fn derive(outcomes: &[CheckOutcome]) -> Self {
        let hard = |want: CheckClass| {
            outcomes
                .iter()
                .any(|o| o.state == CheckState::Fail && o.class == want)
        };
        if hard(CheckClass::Security) {
            Self::Security
        } else if hard(CheckClass::Protocol) {
            Self::Protocol
        } else if hard(CheckClass::Process) {
            Self::Process
        } else if hard(CheckClass::Feature) || hard(CheckClass::Determinism) {
            Self::Feature
        } else {
            Self::Pass
        }
    }
}
