//! The target adapter conformance suite (issue #31).
//!
//! Any target adapter must pass one shared battery of contract checks
//! before it is considered compatible with an exact protocol/IR version
//! pair. The suite drives the adapter-under-test through the production
//! [`TargetClient`] against a hermetic embedded fixture project, records
//! one outcome per closed catalog check, and aggregates the verdict so
//! that hard security and protocol failures can never be compensated by
//! passing feature tests:
//!
//! - `describe`/handshake and protocol/IR version negotiation;
//! - capability declaration, IR backing, and the strict full surface;
//! - deterministic output across repeated runs;
//! - the dry-run write plan, its apply, and the replan retry discipline;
//! - path confinement and canonical-home immutability;
//! - deadline/cancellation behavior and crash-free recovery;
//! - invalid-input handling and structured in-envelope diagnostics;
//! - scenario result normalization;
//! - artifact manifest evidence;
//! - redaction of durable evidence.
//!
//! The suite runs locally and in CI through `lekalo adapter test`; the
//! report projects to the deterministic JSON envelope and JUnit XML.

mod check;
mod diagnostic;
mod fixture;
mod report;
mod version;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::project_fs::Fs;
use crate::result::{DomainResult, Status};
use crate::target_protocol::capability;
use crate::target_protocol::scopes;
use crate::target_protocol::transport::{AdapterCommand, TransportLimits};
use crate::target_protocol::wire::{self, Capabilities, Operation, ResponseEnvelope};
use crate::target_protocol::{CallRequest, TargetClient, TargetFailure};

pub use check::{CheckClass, CheckId, CheckOutcome, CheckState, Verdict, CATALOG};
pub use fixture::{
    IR_INVALID_PATH, IR_INVALID_REFS, IR_MINIMAL, IR_PATH, SCENARIO_PATH, SCENARIO_TXN,
};
pub use report::{
    AdapterSection, Badge, ConformanceReport, Counts, ReportCheck, SessionSection, SuiteSection,
};
pub use version::{
    Profile, DEFAULT_REPEATS, DEFAULT_TIMEOUT_MS, IDENTITY, MAX_REPEATS, MAX_TIMEOUT_MS,
    MIN_REPEATS, MIN_TIMEOUT_MS, SCHEMA_VERSION,
};

/// The closed v1 operation set in catalog order.
const ALL_OPERATIONS: [Operation; 8] = [
    Operation::Describe,
    Operation::Scan,
    Operation::Bind,
    Operation::Validate,
    Operation::Generate,
    Operation::Verify,
    Operation::Clean,
    Operation::PlanClean,
];

/// The suite options, clamped into the closed bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuiteOptions {
    /// The battery profile.
    pub profile: Profile,
    /// Repetition count of every determinism probe.
    pub repeats: u8,
    /// Per-exchange adapter deadline in milliseconds.
    pub timeout_ms: u64,
}

impl Default for SuiteOptions {
    fn default() -> Self {
        Self {
            profile: Profile::Default,
            repeats: DEFAULT_REPEATS,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

impl SuiteOptions {
    /// Clamp the caller-supplied options into the closed bounds.
    pub fn clamped(mut self) -> Self {
        self.repeats = self.repeats.clamp(MIN_REPEATS, MAX_REPEATS);
        self.timeout_ms = self.timeout_ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
        self
    }
}

/// The terminal outcome of one suite run.
#[derive(Clone, Debug, PartialEq)]
pub struct SuiteOutcome {
    /// The full report.
    pub report: ConformanceReport,
    /// The verdict's domain status.
    pub status: Status,
    /// The failed-check diagnostics in normalized order.
    pub diagnostics: crate::diagnostics::DiagnosticSet,
}

impl SuiteOutcome {
    /// The exact JSON envelope bytes (without trailing newline).
    pub fn envelope_json(&self) -> String {
        self.report
            .envelope_json(self.status, self.diagnostics.as_slice())
    }

    /// The exact JUnit XML bytes (with trailing newline).
    pub fn junit(&self) -> String {
        self.report.junit()
    }

    /// The human summary lines.
    pub fn human(&self) -> String {
        self.report.human()
    }

    /// The standard domain result: the envelope payload on the valid
    /// path, the closed diagnostics on every failure path.
    pub fn domain_result(&self) -> DomainResult {
        match self.status {
            Status::Valid => DomainResult::Valid {
                payload: crate::result::SuccessPayload::Receipt {
                    json: self.envelope_json(),
                    human: self.human(),
                },
                diagnostics: Vec::new(),
            },
            Status::Invalid => DomainResult::Invalid {
                diagnostics: self.diagnostics.clone(),
            },
            Status::Denied => DomainResult::Denied {
                diagnostics: self.diagnostics.clone(),
            },
            Status::Unavailable => DomainResult::Unavailable {
                diagnostics: self.diagnostics.clone(),
            },
            Status::Unsupported => DomainResult::UnsupportedOperation {
                diagnostics: self.diagnostics.clone(),
            },
            Status::UnsupportedVersion => DomainResult::UnsupportedVersion {
                diagnostics: self.diagnostics.clone(),
            },
        }
    }
}

/// A suite infrastructure failure: the run could not produce a verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuiteError {
    /// The bounded detail token.
    pub detail: &'static str,
}

/// Project a suite infrastructure failure onto its domain result: the
/// registered `adapter.process-failure` rule at the unavailable class.
pub fn infrastructure_result(error: SuiteError) -> DomainResult {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert(
        "check".to_owned(),
        crate::diagnostics::types::token_value("run"),
    );
    data.insert(
        "class".to_owned(),
        crate::diagnostics::types::token_value("process"),
    );
    data.insert(
        "detail".to_owned(),
        crate::diagnostics::types::token_value(error.detail),
    );
    let diagnostic =
        crate::diagnostics::normalize::build("adapter.process-failure", None, None, data)
            .expect("adapter.process-failure is registered");
    let diagnostics =
        crate::diagnostics::DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Unavailable)
            .unwrap_or_else(|_| crate::diagnostics::DiagnosticSet::empty());
    DomainResult::Unavailable { diagnostics }
}

/// Run the full battery against one adapter.
pub fn run(command: &AdapterCommand, options: &SuiteOptions) -> Result<SuiteOutcome, SuiteError> {
    let options = options.clamped();
    if fixture::scenario_custody().is_err() {
        return Err(SuiteError {
            detail: "fixture-custody",
        });
    }
    let temp = tempfile::tempdir().map_err(|_| SuiteError {
        detail: "fixture-root",
    })?;
    let root = temp.path().canonicalize().map_err(|_| SuiteError {
        detail: "fixture-root",
    })?;
    fixture::materialize(&root).map_err(|_| SuiteError {
        detail: "fixture-write",
    })?;
    let mut runner = Runner::new(command.clone(), root, options);
    runner.run_battery();
    Ok(runner.finish())
}

/// The per-run execution state.
struct Runner {
    command: AdapterCommand,
    root: PathBuf,
    options: SuiteOptions,
    client: TargetClient,
    outcomes: BTreeMap<CheckId, CheckOutcome>,
    /// Every completed exchange response in canonical bytes, in run
    /// order. The redaction check reads this record.
    exchanges: Vec<String>,
    /// Determinism probes: token to first canonical bytes and the number
    /// of samples observed.
    probes: BTreeMap<&'static str, (String, u32)>,
    capabilities: Option<Capabilities>,
    negotiated: Option<&'static str>,
    capability_digest: Option<String>,
    baseline: Option<fixture::Observation>,
}

impl Runner {
    fn new(command: AdapterCommand, root: PathBuf, options: SuiteOptions) -> Self {
        let limits = TransportLimits {
            timeout_ms: options.timeout_ms,
            ..TransportLimits::default()
        };
        Self {
            command,
            root,
            options,
            client: TargetClient::new(limits),
            outcomes: BTreeMap::new(),
            exchanges: Vec::new(),
            probes: BTreeMap::new(),
            capabilities: None,
            negotiated: None,
            capability_digest: None,
            baseline: None,
        }
    }

    /// Record one outcome; the last write wins, so later evidence can
    /// downgrade an optimistic pass but never upgrade a failure.
    fn record(&mut self, outcome: CheckOutcome) {
        self.outcomes.insert(outcome.id, outcome);
    }

    fn declared(&self, operation: Operation) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.operations.contains(&operation))
    }

    /// Run the whole battery in its fixed order.
    fn run_battery(&mut self) {
        if self.describe_phase().is_err() {
            return;
        }
        self.capability_phase();
        self.confinement_baseline();
        self.input_phase();
        self.read_determinism_phase();
        self.generate_phase();
        self.clean_phase();
        self.scenario_phase();
        self.process_phase();
        self.finish_confinement();
        self.redaction_phase();
    }

    /// The describe handshake and negotiation checks.
    fn describe_phase(&mut self) -> Result<(), ()> {
        let described = match self.client.describe(&self.command, &self.root) {
            Ok(outcome) => Some((
                outcome.capabilities.clone(),
                outcome.capability_digest.clone(),
                outcome.negotiated_version,
            )),
            Err(failure) => {
                let (class, detail) = classify(&failure);
                self.record(CheckOutcome {
                    id: CheckId::DescribeHandshake,
                    class,
                    state: CheckState::Fail,
                    detail: Some(detail),
                });
                self.record(match class {
                    CheckClass::Protocol => CheckOutcome {
                        id: CheckId::DescribeNegotiation,
                        class,
                        state: CheckState::Fail,
                        detail: Some(detail),
                    },
                    _ => CheckOutcome::skipped(CheckId::DescribeNegotiation, "not-run"),
                });
                None
            }
        };
        match described {
            Some((capabilities, digest, negotiated)) => {
                self.negotiated = Some(negotiated);
                self.capability_digest = Some(digest);
                self.record(CheckOutcome::pass(CheckId::DescribeHandshake));
                self.record(CheckOutcome::pass(CheckId::DescribeNegotiation));
                self.capabilities = Some(capabilities);
                Ok(())
            }
            None => Err(()),
        }
    }

    /// The capability declaration checks; no exchanges.
    fn capability_phase(&mut self) {
        let Some(caps) = self.capabilities.clone() else {
            return;
        };
        // Registered capability ids and a bound evidence digest.
        let ids_registered = caps
            .capabilities
            .keys()
            .all(|id| capability::definition(id).is_some());
        let digest_bound = self
            .capability_digest
            .as_ref()
            .is_some_and(|digest| wire::is_sha256_digest(digest));
        self.record(if ids_registered && digest_bound {
            CheckOutcome::pass(CheckId::CapabilityDeclaration)
        } else {
            CheckOutcome::fail(
                CheckId::CapabilityDeclaration,
                CheckClass::Protocol,
                "declaration",
            )
        });
        // IR backing for declared IR-carrying operations.
        let ir_ops = caps.operations.iter().any(|op| op.requires_ir());
        self.record(
            if self.negotiated == Some(crate::target_protocol::version::BASE_VERSION) {
                CheckOutcome::skipped(CheckId::CapabilityIrDeclaration, "legacy-session")
            } else if !ir_ops {
                CheckOutcome::skipped(CheckId::CapabilityIrDeclaration, "no-ir-operations")
            } else if caps
                .ir_versions
                .iter()
                .any(|declared| declared == crate::ir::version::VERSION)
            {
                CheckOutcome::pass(CheckId::CapabilityIrDeclaration)
            } else {
                CheckOutcome::fail(
                    CheckId::CapabilityIrDeclaration,
                    CheckClass::Protocol,
                    "ir-undeclared",
                )
            },
        );
        // The strict full-surface requirement.
        let complete = ALL_OPERATIONS.iter().all(|op| caps.operations.contains(op));
        self.record(match self.options.profile {
            Profile::Default => {
                CheckOutcome::skipped(CheckId::CapabilitySurface, "default-profile")
            }
            Profile::Strict if complete => CheckOutcome::pass(CheckId::CapabilitySurface),
            Profile::Strict => {
                CheckOutcome::fail(CheckId::CapabilitySurface, CheckClass::Protocol, "surface")
            }
        });
    }

    /// Observe the fixture root before any feature exchange.
    fn confinement_baseline(&mut self) {
        match fixture::observe(&self.root) {
            Ok(observed) => self.baseline = Some(observed),
            Err(_) => self.record(CheckOutcome::fail(
                CheckId::ConfinementCanonical,
                CheckId::ConfinementCanonical.class(),
                "observe",
            )),
        }
    }

    /// The invalid-references input and structured-diagnostics checks.
    fn input_phase(&mut self) {
        if !self.declared(Operation::Validate) {
            self.record(CheckOutcome::skipped(
                CheckId::InputInvalidIr,
                "operation-undeclared",
            ));
            self.record(CheckOutcome::skipped(
                CheckId::DiagnosticsStructured,
                "operation-undeclared",
            ));
            return;
        }
        let shape = CallShape {
            operation: Operation::Validate,
            ir_path: Some(IR_INVALID_PATH.to_owned()),
            ..CallShape::default()
        };
        match self.exchange(&shape) {
            Exchange::Ok { response, .. } => {
                // The wire contract governs this check: any well-formed
                // outcome (findings, an honest ok verdict, or an
                // in-envelope error) is graceful invalid-input handling;
                // only a dead exchange is a failure.
                let _ = &response;
                self.record(CheckOutcome::pass(CheckId::InputInvalidIr));
            }
            Exchange::Failed(failure) => {
                let terminal = failure_is_terminal(&failure);
                self.record_failure(CheckId::InputInvalidIr, &failure);
                if terminal {
                    self.skip_remaining();
                }
            }
        }
        self.structured_phase();
    }

    /// Validate the shape of every observed in-envelope error.
    fn structured_phase(&mut self) {
        let errors: Vec<wire::OperationError> = self
            .exchanges
            .iter()
            .filter_map(|bytes| serde_json::from_str::<ResponseEnvelope>(bytes).ok())
            .filter_map(|response| response.error)
            .collect();
        if errors.is_empty() {
            self.record(CheckOutcome::skipped(
                CheckId::DiagnosticsStructured,
                "no-error-observed",
            ));
            return;
        }
        let shaped = errors.iter().all(|error| {
            (1..=128).contains(&error.code.chars().count())
                && (1..=512).contains(&error.message.chars().count())
        });
        self.record(if shaped {
            CheckOutcome::pass(CheckId::DiagnosticsStructured)
        } else {
            CheckOutcome::fail(
                CheckId::DiagnosticsStructured,
                CheckId::DiagnosticsStructured.class(),
                "shape",
            )
        });
    }

    /// Repeated read-only exchanges: validate and verify results must be
    /// byte-identical.
    fn read_determinism_phase(&mut self) {
        for (operation, ir_path, probe) in [
            (Operation::Validate, Some(IR_PATH), "validate"),
            (Operation::Verify, Some(IR_PATH), "verify"),
        ] {
            if !self.declared(operation) {
                continue;
            }
            for _ in 0..self.options.repeats {
                let shape = CallShape {
                    operation,
                    ir_path: ir_path.map(str::to_owned),
                    ..CallShape::default()
                };
                match self.exchange(&shape) {
                    Exchange::Ok { response, .. } => {
                        self.probe(probe, &response);
                    }
                    Exchange::Failed(failure) => {
                        let terminal = failure_is_terminal(&failure);
                        self.record_failure(CheckId::DeterminismRepeats, &failure);
                        if terminal {
                            self.skip_remaining();
                        }
                        break;
                    }
                }
            }
        }
        self.settle_determinism();
    }

    /// The generate dry-run repeats, the apply, and the retry
    /// discipline. The last dry run leaves the binding the apply uses.
    fn generate_phase(&mut self) {
        let pending = [
            CheckId::GenerateDryRunPlan,
            CheckId::GenerateApplyPlan,
            CheckId::ApplyRetryDiscipline,
            CheckId::ArtifactManifestEvidence,
        ];
        if !self.declared(Operation::Generate) {
            for id in pending {
                self.record(CheckOutcome::skipped(id, "operation-undeclared"));
            }
            return;
        }
        let Some(target) = self.first_target() else {
            for id in pending {
                self.record(CheckOutcome::fail(id, id.class(), "target-undeclared"));
            }
            return;
        };
        let mut planned = false;
        for _ in 0..self.options.repeats {
            let shape = CallShape {
                operation: Operation::Generate,
                target: Some(target.clone()),
                ir_path: Some(IR_PATH.to_owned()),
                dry_run: Some(true),
                ..CallShape::default()
            };
            match self.exchange(&shape) {
                Exchange::Ok { response, .. } => {
                    self.probe("generate-plan", &response);
                    if let Some(writes) = &response.writes {
                        if !self.writes_in_scopes(writes) {
                            self.record(CheckOutcome::fail(
                                CheckId::ConfinementPlanScopes,
                                CheckClass::Security,
                                "plan-escape",
                            ));
                        }
                        planned = true;
                    }
                }
                Exchange::Failed(failure) => {
                    let terminal = failure_is_terminal(&failure);
                    self.record_failure(CheckId::GenerateDryRunPlan, &failure);
                    for id in [CheckId::GenerateApplyPlan, CheckId::ApplyRetryDiscipline] {
                        self.record(CheckOutcome::skipped(id, "not-run"));
                    }
                    self.record(CheckOutcome::skipped(
                        CheckId::ArtifactManifestEvidence,
                        "not-run",
                    ));
                    if terminal {
                        self.skip_remaining();
                    }
                    self.settle_determinism();
                    return;
                }
            }
        }
        self.settle_determinism();
        if !planned {
            self.record(CheckOutcome::fail(
                CheckId::GenerateDryRunPlan,
                CheckId::GenerateDryRunPlan.class(),
                "no-plan",
            ));
            for id in [CheckId::GenerateApplyPlan, CheckId::ApplyRetryDiscipline] {
                self.record(CheckOutcome::skipped(id, "no-plan"));
            }
            self.record(CheckOutcome::skipped(
                CheckId::ArtifactManifestEvidence,
                "no-plan",
            ));
            return;
        }
        self.record(CheckOutcome::pass(CheckId::GenerateDryRunPlan));
        self.retry_phase(&target);
    }

    /// Refuse a foreign plan id, replan, then apply the exact plan.
    fn retry_phase(&mut self, target: &str) {
        // A foreign plan id is refused and consumes the pending
        // authority: the retry path replans, never replays.
        let forged = CallShape {
            operation: Operation::Generate,
            target: Some(target.to_owned()),
            ir_path: Some(IR_PATH.to_owned()),
            dry_run: Some(false),
            plan_id: Some(format!("plan-{}", "0".repeat(64))),
            ..CallShape::default()
        };
        match self.exchange(&forged) {
            Exchange::Ok { .. } => {
                self.record(CheckOutcome::fail(
                    CheckId::ApplyRetryDiscipline,
                    CheckId::ApplyRetryDiscipline.class(),
                    "forged-plan",
                ));
                for id in [
                    CheckId::GenerateApplyPlan,
                    CheckId::ArtifactManifestEvidence,
                ] {
                    self.record(CheckOutcome::skipped(id, "authority-lost"));
                }
                return;
            }
            Exchange::Failed(failure) => {
                if !matches!(
                    failure,
                    TargetFailure::PlanMismatch { .. } | TargetFailure::RequestInvalid { .. }
                ) {
                    let terminal = failure_is_terminal(&failure);
                    self.record_failure(CheckId::ApplyRetryDiscipline, &failure);
                    for id in [
                        CheckId::GenerateApplyPlan,
                        CheckId::ArtifactManifestEvidence,
                    ] {
                        self.record(CheckOutcome::skipped(id, "not-run"));
                    }
                    if terminal {
                        self.skip_remaining();
                    }
                    return;
                }
            }
        }
        // Replan: a fresh dry run restores the authority.
        let replan = CallShape {
            operation: Operation::Generate,
            target: Some(target.to_owned()),
            ir_path: Some(IR_PATH.to_owned()),
            dry_run: Some(true),
            ..CallShape::default()
        };
        let replanned = match self.exchange(&replan) {
            Exchange::Ok { plan_id, .. } => plan_id,
            Exchange::Failed(failure) => {
                let terminal = failure_is_terminal(&failure);
                self.record_failure(CheckId::ApplyRetryDiscipline, &failure);
                for id in [
                    CheckId::GenerateApplyPlan,
                    CheckId::ArtifactManifestEvidence,
                ] {
                    self.record(CheckOutcome::skipped(id, "not-run"));
                }
                if terminal {
                    self.skip_remaining();
                }
                return;
            }
        };
        let Some(replan_id) = replanned else {
            self.record(CheckOutcome::fail(
                CheckId::ApplyRetryDiscipline,
                CheckId::ApplyRetryDiscipline.class(),
                "no-plan",
            ));
            for id in [
                CheckId::GenerateApplyPlan,
                CheckId::ArtifactManifestEvidence,
            ] {
                self.record(CheckOutcome::skipped(id, "no-plan"));
            }
            return;
        };
        // Apply the exact replanned plan.
        let apply = CallShape {
            operation: Operation::Generate,
            target: Some(target.to_owned()),
            ir_path: Some(IR_PATH.to_owned()),
            dry_run: Some(false),
            plan_id: Some(replan_id),
            ..CallShape::default()
        };
        match self.exchange(&apply) {
            Exchange::Ok { response, .. } => {
                self.record(CheckOutcome::pass(CheckId::ApplyRetryDiscipline));
                self.record(CheckOutcome::pass(CheckId::GenerateApplyPlan));
                let declared = response.writes.clone().unwrap_or_default();
                self.manifest_phase(&declared);
            }
            Exchange::Failed(failure) => {
                let terminal = failure_is_terminal(&failure);
                self.record_failure(CheckId::GenerateApplyPlan, &failure);
                for id in [
                    CheckId::ApplyRetryDiscipline,
                    CheckId::ArtifactManifestEvidence,
                ] {
                    self.record(CheckOutcome::skipped(id, "not-run"));
                }
                if terminal {
                    self.skip_remaining();
                }
            }
        }
    }

    /// The applied bytes must match the declared plan digests.
    fn manifest_phase(&mut self, declared: &[wire::WriteEntry]) {
        let Ok(after) = fixture::observe(&self.root) else {
            self.record(CheckOutcome::fail(
                CheckId::ArtifactManifestEvidence,
                CheckId::ArtifactManifestEvidence.class(),
                "observe",
            ));
            return;
        };
        let held = declared
            .iter()
            .all(|entry| match (&entry.action, &entry.sha256) {
                (wire::WriteAction::Delete, _) => !after.contains_key(&entry.path),
                (_, Some(digest)) => after
                    .get(&entry.path)
                    .map(|observed_hex| format!("sha256:{}", observed_hex) == *digest)
                    .unwrap_or(false),
                (_, None) => false,
            });
        self.record(if held {
            CheckOutcome::pass(CheckId::ArtifactManifestEvidence)
        } else {
            CheckOutcome::fail(
                CheckId::ArtifactManifestEvidence,
                CheckId::ArtifactManifestEvidence.class(),
                "digest",
            )
        });
    }

    /// The plan-clean/clean cycle.
    fn clean_phase(&mut self) {
        if !self.declared(Operation::PlanClean) {
            self.record(CheckOutcome::skipped(
                CheckId::PlanCleanCycle,
                "operation-undeclared",
            ));
            return;
        }
        if !self.declared(Operation::Clean) {
            self.record(CheckOutcome::skipped(
                CheckId::PlanCleanCycle,
                "clean-undeclared",
            ));
            return;
        }
        // A clean cycle only applies when the adapter's write scopes hold
        // something deletable: after a successful generate apply. A
        // generate-less adapter plans an empty deletion cycle instead.
        let generated = self.outcomes.get(&CheckId::GenerateApplyPlan);
        let cleanable = self.declared(Operation::Generate)
            && generated.is_some_and(|outcome| outcome.state == CheckState::Pass);
        if self.declared(Operation::Generate) && !cleanable {
            self.record(CheckOutcome::skipped(
                CheckId::PlanCleanCycle,
                "nothing-to-clean",
            ));
            return;
        }
        let plan = CallShape {
            operation: Operation::PlanClean,
            ..CallShape::default()
        };
        let plan_id = match self.exchange(&plan) {
            Exchange::Ok { response, plan_id } => {
                let deletions_only = response.writes.as_ref().is_some_and(|writes| {
                    writes
                        .iter()
                        .all(|entry| entry.action == wire::WriteAction::Delete)
                });
                if deletions_only {
                    plan_id
                } else {
                    None
                }
            }
            Exchange::Failed(failure) => {
                let terminal = failure_is_terminal(&failure);
                self.record_failure(CheckId::PlanCleanCycle, &failure);
                if terminal {
                    self.skip_remaining();
                }
                return;
            }
        };
        let Some(plan_id) = plan_id else {
            self.record(CheckOutcome::fail(
                CheckId::PlanCleanCycle,
                CheckId::PlanCleanCycle.class(),
                "plan",
            ));
            return;
        };
        let clean = CallShape {
            operation: Operation::Clean,
            plan_id: Some(plan_id),
            ..CallShape::default()
        };
        match self.exchange(&clean) {
            Exchange::Ok { .. } => self.record(CheckOutcome::pass(CheckId::PlanCleanCycle)),
            Exchange::Failed(failure) => {
                let terminal = failure_is_terminal(&failure);
                self.record_failure(CheckId::PlanCleanCycle, &failure);
                if terminal {
                    self.skip_remaining();
                }
            }
        }
    }

    /// The scenario normalization check over repeated verifies.
    fn scenario_phase(&mut self) {
        if !self.declared(Operation::Verify) {
            self.record(CheckOutcome::skipped(
                CheckId::ScenarioNormalization,
                "operation-undeclared",
            ));
            return;
        }
        if !self.read_scopes_cover(SCENARIO_PATH) {
            self.record(CheckOutcome::skipped(
                CheckId::ScenarioNormalization,
                "fixture-not-in-read-scopes",
            ));
            return;
        }
        for _ in 0..self.options.repeats {
            let shape = CallShape {
                operation: Operation::Verify,
                ir_path: Some(IR_PATH.to_owned()),
                ..CallShape::default()
            };
            match self.exchange(&shape) {
                Exchange::Ok { response, .. } => self.probe("verify-scenario", &response),
                Exchange::Failed(failure) => {
                    let terminal = failure_is_terminal(&failure);
                    self.record_failure(CheckId::ScenarioNormalization, &failure);
                    if terminal {
                        self.skip_remaining();
                    }
                    break;
                }
            }
        }
        if !self.outcomes.contains_key(&CheckId::ScenarioNormalization) {
            self.record(match self.probes.get("verify-scenario") {
                Some((_, count)) if *count >= 1 => {
                    CheckOutcome::pass(CheckId::ScenarioNormalization)
                }
                _ => CheckOutcome::fail(
                    CheckId::ScenarioNormalization,
                    CheckId::ScenarioNormalization.class(),
                    "no-result",
                ),
            });
        }
    }

    /// Cancellation handling, recovery, and the describe-digest
    /// determinism probe.
    fn process_phase(&mut self) {
        let candidate = [
            Operation::Scan,
            Operation::Bind,
            Operation::Validate,
            Operation::Verify,
            Operation::PlanClean,
        ]
        .into_iter()
        .find(|operation| self.declared(*operation));
        let Some(operation) = candidate else {
            self.record(CheckOutcome::skipped(
                CheckId::ProcessCancellation,
                "operation-undeclared",
            ));
            return;
        };
        let shape = CallShape {
            operation,
            ir_path: operation.requires_ir().then(|| IR_PATH.to_owned()),
            target: (operation == Operation::Bind).then(|| {
                self.first_target()
                    .unwrap_or_else(|| "conformance".to_owned())
            }),
            profile: (operation == Operation::Bind).then(|| "default".to_owned()),
            ..CallShape::default()
        };
        let cancel = AtomicBool::new(true);
        let Ok(fs) = Fs::open(&self.root) else {
            self.record(CheckOutcome::fail(
                CheckId::ProcessCancellation,
                CheckId::ProcessCancellation.class(),
                "observe",
            ));
            return;
        };
        let outcome = self.client.call(
            &self.command,
            shape.to_call_request(),
            &self.root,
            &fs,
            Some(&cancel),
        );
        match outcome {
            Ok(_) | Err(TargetFailure::Cancelled) => {}
            Err(failure) => {
                let terminal = failure_is_terminal(&failure);
                self.record_failure(CheckId::ProcessCancellation, &failure);
                if terminal {
                    self.skip_remaining();
                    return;
                }
            }
        }
        // A fresh handshake must recover; its digest feeds determinism.
        let recovered = match self.client.describe(&self.command, &self.root) {
            Ok(described) => Some((
                described.capabilities.clone(),
                described.capability_digest.clone(),
                described.negotiated_version,
            )),
            Err(_) => None,
        };
        match recovered {
            Some((capabilities, digest, negotiated)) => {
                self.record(CheckOutcome::pass(CheckId::ProcessCancellation));
                self.negotiated = Some(negotiated);
                self.capability_digest = Some(digest.clone());
                self.capabilities = Some(capabilities);
                self.probe_text("describe-digest", &digest);
            }
            None => {
                self.record(CheckOutcome::fail(
                    CheckId::ProcessCancellation,
                    CheckId::ProcessCancellation.class(),
                    "recovery",
                ));
                self.skip_remaining();
                return;
            }
        }
        self.settle_determinism();
    }

    /// Canonical homes unchanged; every change inside declared scopes.
    fn finish_confinement(&mut self) {
        if self
            .outcomes
            .get(&CheckId::ConfinementCanonical)
            .is_some_and(|outcome| outcome.state == CheckState::Fail)
        {
            return;
        }
        let Some(baseline) = self.baseline.clone() else {
            self.record(CheckOutcome::fail(
                CheckId::ConfinementCanonical,
                CheckId::ConfinementCanonical.class(),
                "observe",
            ));
            return;
        };
        let Ok(after) = fixture::observe(&self.root) else {
            self.record(CheckOutcome::fail(
                CheckId::ConfinementCanonical,
                CheckId::ConfinementCanonical.class(),
                "observe",
            ));
            return;
        };
        let changed = fixture::changed(&baseline, &after);
        let canonical_touched = changed
            .iter()
            .any(|path| path.starts_with("lekalo/") || path.starts_with("openspec/"));
        self.record(if canonical_touched {
            CheckOutcome::fail(
                CheckId::ConfinementCanonical,
                CheckId::ConfinementCanonical.class(),
                "canonical-mutation",
            )
        } else {
            CheckOutcome::pass(CheckId::ConfinementCanonical)
        });
        if !self.outcomes.contains_key(&CheckId::ConfinementPlanScopes) {
            let scopes = self.write_scopes();
            let escaped = changed
                .iter()
                .any(|path| !scopes.iter().any(|scope| scopes::scope_covers(scope, path)));
            self.record(if escaped {
                CheckOutcome::fail(
                    CheckId::ConfinementPlanScopes,
                    CheckId::ConfinementPlanScopes.class(),
                    "scope-escape",
                )
            } else {
                CheckOutcome::pass(CheckId::ConfinementPlanScopes)
            });
        }
    }

    /// Durable evidence stays redacted: no invocation-local host path or
    /// adapter argv text may appear in any recorded response.
    fn redaction_phase(&mut self) {
        // Only invocation-local HOST paths count as leak needles: the
        // fixture root and any absolute command member. Bare program
        // names ("node") and relative script names are public argv, not
        // host state; a short generic token would false-positive on
        // adapter ids such as `node-typescript`.
        let host_local = |text: &str| {
            text.len() > 2
                && (Path::new(text).is_absolute() || text.contains('/') || text.contains('\\'))
        };
        let mut needles: Vec<String> = vec![self.root.to_string_lossy().into_owned()];
        let program = self.command.program.to_string_lossy().into_owned();
        if host_local(&program) {
            needles.push(program);
        }
        for arg in &self.command.args {
            let text = arg.as_str();
            if host_local(text) {
                needles.push(text.to_owned());
            }
        }
        let leaked = self.exchanges.iter().any(|bytes| {
            needles
                .iter()
                .filter(|needle| !needle.is_empty())
                .any(|needle| bytes.contains(needle.as_str()))
        });
        let scan_paths_logical = self
            .exchanges
            .iter()
            .filter_map(|bytes| serde_json::from_str::<ResponseEnvelope>(bytes).ok())
            .flat_map(|response| response.result)
            .flat_map(|result| result.entries.unwrap_or_default())
            .all(|entry| scopes::is_logical_path(&entry.path));
        self.record(if leaked || !scan_paths_logical {
            CheckOutcome::fail(
                CheckId::RedactionEvidence,
                CheckId::RedactionEvidence.class(),
                "leak",
            )
        } else {
            CheckOutcome::pass(CheckId::RedactionEvidence)
        });
    }

    /// Record one canonical response under its determinism probe.
    fn probe(&mut self, token: &'static str, response: &ResponseEnvelope) {
        let bytes = serde_json::to_string(response).unwrap_or_default();
        self.exchanges.push(bytes.clone());
        self.probe_text(token, &bytes);
    }

    fn probe_text(&mut self, token: &'static str, bytes: &str) {
        if let Some(entry) = self.probes.get_mut(token) {
            let mismatch = entry.0 != bytes;
            entry.1 += 1;
            if mismatch {
                self.record(CheckOutcome::fail(
                    CheckId::DeterminismRepeats,
                    CheckClass::Determinism,
                    token,
                ));
            }
            return;
        }
        self.probes.insert(token, (bytes.to_owned(), 1));
    }

    /// Close the determinism check: a pass only when at least one probe
    /// repeated and none failed.
    fn settle_determinism(&mut self) {
        if self
            .outcomes
            .get(&CheckId::DeterminismRepeats)
            .is_some_and(|outcome| outcome.state == CheckState::Fail)
        {
            return;
        }
        let repeated = self.probes.values().any(|(_, count)| *count >= 2);
        self.record(if repeated {
            CheckOutcome::pass(CheckId::DeterminismRepeats)
        } else {
            CheckOutcome::skipped(CheckId::DeterminismRepeats, "no-repeatable-probe")
        });
    }

    fn writes_in_scopes(&self, writes: &[wire::WriteEntry]) -> bool {
        let scopes = self.write_scopes();
        writes.iter().all(|entry| {
            scopes
                .iter()
                .any(|scope| scopes::scope_covers(scope, &entry.path))
        })
    }

    /// Record a failed check from a classified protocol failure; the
    /// caller handles terminal short-circuits.
    fn record_failure(&mut self, id: CheckId, failure: &TargetFailure) {
        let (class, detail) = classify(failure);
        self.record(CheckOutcome {
            id,
            class,
            state: CheckState::Fail,
            detail: Some(detail),
        });
    }

    /// Mark every still-unrecorded check as not-run.
    fn skip_remaining(&mut self) {
        for id in check::CATALOG {
            self.outcomes
                .entry(id)
                .or_insert_with(|| CheckOutcome::skipped(id, "not-run"));
        }
    }

    fn exchange(&mut self, shape: &CallShape) -> Exchange {
        let Ok(fs) = Fs::open(&self.root) else {
            return Exchange::Failed(TargetFailure::RequestInvalid { detail: "root" });
        };
        match self.client.call(
            &self.command,
            shape.to_call_request(),
            &self.root,
            &fs,
            None,
        ) {
            Ok(outcome) => {
                let bytes = serde_json::to_string(&outcome.response).unwrap_or_default();
                self.exchanges.push(bytes);
                Exchange::Ok {
                    response: Box::new(outcome.response),
                    plan_id: outcome.plan_id,
                }
            }
            Err(failure) => Exchange::Failed(failure),
        }
    }

    fn read_scopes_cover(&self, path: &str) -> bool {
        let scopes = self
            .capabilities
            .as_ref()
            .map(|caps| caps.read_scopes.clone())
            .unwrap_or_default();
        scopes.iter().any(|scope| scopes::scope_covers(scope, path))
    }

    fn write_scopes(&self) -> Vec<String> {
        self.capabilities
            .as_ref()
            .map(|caps| caps.write_scopes.clone())
            .unwrap_or_default()
    }

    fn first_target(&self) -> Option<String> {
        self.capabilities
            .as_ref()
            .and_then(|caps| caps.targets.first().cloned())
    }

    /// Assemble the terminal outcome.
    fn finish(mut self) -> SuiteOutcome {
        self.skip_remaining();
        let ordered: Vec<CheckOutcome> = check::CATALOG
            .iter()
            .map(|id| {
                self.outcomes
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| CheckOutcome::skipped(*id, "not-run"))
            })
            .collect();
        let adapter = self
            .capabilities
            .as_ref()
            .map(|caps| AdapterSection {
                id: caps.adapter.id.clone(),
                version: caps.adapter.version.clone(),
                digest: caps.adapter.digest.clone(),
            })
            .unwrap_or(AdapterSection {
                id: "unknown".to_owned(),
                version: "0.0.0".to_owned(),
                digest: format!("sha256:{}", "0".repeat(64)),
            });
        let session = SessionSection {
            protocol: self
                .negotiated
                .unwrap_or(crate::target_protocol::version::BASE_VERSION),
            ir: crate::ir::version::VERSION,
            capability_digest: self
                .capability_digest
                .clone()
                .unwrap_or_else(|| format!("sha256:{}", "0".repeat(64))),
            timeout_ms: self.options.timeout_ms,
            repeats: self.options.repeats,
        };
        let verdict = Verdict::derive(&ordered);
        let report = ConformanceReport::build(
            env!("CARGO_PKG_VERSION"),
            self.options.profile,
            adapter,
            session,
            &ordered,
        );
        SuiteOutcome {
            status: verdict.status(),
            diagnostics: diagnostic::diagnostic_set(verdict, &ordered),
            report,
        }
    }
}

/// One pending exchange request in suite shape.
#[derive(Clone, Debug)]
struct CallShape {
    operation: Operation,
    target: Option<String>,
    profile: Option<String>,
    ir_path: Option<String>,
    dry_run: Option<bool>,
    plan_id: Option<String>,
}

impl Default for CallShape {
    fn default() -> Self {
        Self {
            operation: Operation::Describe,
            target: None,
            profile: None,
            ir_path: None,
            dry_run: None,
            plan_id: None,
        }
    }
}

impl CallShape {
    fn to_call_request(&self) -> CallRequest<'_> {
        CallRequest {
            operation: self.operation,
            target: self.target.as_deref(),
            profile: self.profile.as_deref(),
            profile_resolution: None,
            ir_path: self.ir_path.as_deref(),
            dry_run: self.dry_run,
            plan_id: self.plan_id.as_deref(),
        }
    }
}

/// The result of one exchange attempt: the validated response plus the
/// client-computed plan identity of a planning exchange.
enum Exchange {
    Ok {
        response: Box<ResponseEnvelope>,
        plan_id: Option<String>,
    },
    Failed(TargetFailure),
}

/// Map a closed protocol failure onto its check class and bounded
/// detail token. Registry preflight refusals surface as process-class
/// failures; the version registry is a precondition the suite cannot
/// compensate.
fn classify(failure: &TargetFailure) -> (CheckClass, &'static str) {
    match failure {
        TargetFailure::ProtocolMismatch { detail } => (CheckClass::Protocol, detail.detail()),
        TargetFailure::ResponseInvalid { detail } => {
            (CheckClass::Protocol, response_invalidity_token(*detail))
        }
        TargetFailure::IrUnsupported => (CheckClass::Protocol, "ir-unsupported"),
        TargetFailure::ScopeViolation { detail, .. } => (CheckClass::Security, detail),
        TargetFailure::ProtectedPath { .. } => (CheckClass::Security, "protected-path"),
        TargetFailure::DryRunMutation { .. } => (CheckClass::Security, "dry-run-mutation"),
        TargetFailure::Timeout => (CheckClass::Process, "deadline"),
        TargetFailure::Crash { .. } => (CheckClass::Process, "crash"),
        TargetFailure::OutputLimit { .. } => (CheckClass::Process, "output-limit"),
        TargetFailure::Cancelled => (CheckClass::Process, "cancelled"),
        TargetFailure::TransportFailed { detail } => (CheckClass::Process, detail),
        TargetFailure::OperationFailed { .. } => (CheckClass::Feature, "operation-failed"),
        TargetFailure::PlanMismatch { detail, .. } => (CheckClass::Feature, detail),
        TargetFailure::RequestInvalid { detail } => (CheckClass::Feature, detail),
        TargetFailure::HandshakeRequired { .. } => (CheckClass::Feature, "handshake-required"),
        TargetFailure::CapabilityUnsupported { detail } => (CheckClass::Feature, detail),
        TargetFailure::ProtocolUnpublished | TargetFailure::RegistryInvalid => {
            (CheckClass::Process, "registry")
        }
    }
}

/// The bounded token of a response invalidity.
fn response_invalidity_token(detail: wire::ResponseInvalidity) -> &'static str {
    match detail {
        wire::ResponseInvalidity::NotJson => "not-json",
        wire::ResponseInvalidity::Shape => "shape",
        wire::ResponseInvalidity::RequestId => "request-id",
        wire::ResponseInvalidity::Operation => "operation",
        wire::ResponseInvalidity::Evidence => "evidence",
        wire::ResponseInvalidity::UnexpectedMember => "unexpected-member",
        wire::ResponseInvalidity::WritesMissing => "writes-missing",
        wire::ResponseInvalidity::WriteDigest => "write-digest",
        wire::ResponseInvalidity::ErrorPairing => "error-pairing",
    }
}

/// Whether a failure means the adapter process can no longer serve the
/// battery, so remaining feature checks would only repeat it.
fn failure_is_terminal(failure: &TargetFailure) -> bool {
    matches!(
        failure,
        TargetFailure::Timeout
            | TargetFailure::Crash { .. }
            | TargetFailure::TransportFailed { .. }
            | TargetFailure::OutputLimit { .. }
    )
}
