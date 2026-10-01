//! The `lekalo verify` orchestration pipeline (issue #91).
//!
//! One invocation runs the read-only verification phases over the
//! validated project: the core structural/semantic validation, the
//! ownership drift gate, the per-target adapter validation through the
//! `lekalo.target/v1` protocol, the portable scenario summary, and the
//! trace summary — then aggregates every component into one receipt
//! where required and optional components stay distinguished, target
//! results stay separate, and nothing is ever written. Scenario and
//! native-test execution and the selected native gates are declared
//! component absences (`core.capability-unavailable`) until their
//! backends land; they are never silently skipped and never counted as
//! execution.

use crate::artifacts::check::Prepared;
use crate::artifacts::types::ArtifactKind;
use crate::artifacts::GenerateService;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::ir::Compilation;
use crate::loader::LoadSelection;
use crate::lockfile::types::Sha256Digest;
use crate::project_fs::{EntryType, Fs};
use crate::result::DomainResult;
use crate::target_protocol::transport::TransportLimits;
use crate::target_protocol::wire::Operation;
use crate::target_protocol::wire::OperationResult;
use crate::target_protocol::{CallRequest, TargetClient};
use crate::validator::{self, ValidationProfile};

use super::catalog::{binding_failure, discover, locked_adapter, AdapterSupply};
use super::generate::compile;
use super::receipt::{
    ComponentReceipt, ComponentState, InputsReceipt, ScenarioCoverage, ScopeReceipt,
    SeverityCounts, TraceSummary, Verdict, VerdictCountsReceipt, VerifyReceipt, IDENTITY,
    SCHEMA_VERSION,
};
use super::version::IR_EVIDENCE_DIR;
use super::Failure;

/// The one semantic identity of the scenario-execution gate (issue #56,
/// plan S5): the exported `gate:` node id names this exact receipt
/// component, never an arbitrary string — `gate evidences` edges are
/// traceable back to the validated component that published them.
const SCENARIOS_EXECUTION: &str = "scenarios.execution";

// F-7 (issue #47 review): the exported scenario relations flow through
// the trace manifest mechanism.
use crate::scenario_evidence::{trace_manifest_document, RunRecord, TraceContext};

/// The request of one verify invocation.
pub struct VerifyRequest<'a> {
    pub selection: &'a LoadSelection,
    /// The explicit target selection; empty skips adapter components
    /// unless the lock pins adapters, which are then all verified.
    pub targets: Vec<String>,
    /// The optional module scope.
    pub module: Option<String>,
    /// The affected modules of a `--changed` run (resolved by the CLI
    /// from the Git handoff); non-empty selects changed mode.
    pub changed_modules: Vec<String>,
    pub locked: bool,
    /// The project-relative logical path of an optional trace manifest.
    pub trace: Option<String>,
    pub supply: Option<AdapterSupply>,
    pub timeout_ms: u64,
}

/// One component's receipt plus its failure envelope, when the
/// component failed.
struct Component {
    receipt: ComponentReceipt,
    failure: Option<DomainResult>,
}

impl Component {
    fn pass(id: &str, required: bool) -> Self {
        Self::state(id, required, ComponentState::Pass, None)
    }

    fn state(id: &str, required: bool, state: ComponentState, reason_code: Option<&str>) -> Self {
        Self {
            receipt: ComponentReceipt {
                id: id.to_owned(),
                required,
                state,
                reason_code: reason_code.map(str::to_owned),
                severity_counts: None,
                verdict_counts: None,
                findings: None,
                scenarios: None,
                trace: None,
            },
            failure: None,
        }
    }

    fn failed(id: &str, required: bool, reason: &str, failure: DomainResult) -> Self {
        let mut component = Self::state(id, required, ComponentState::Fail, Some(reason));
        component.failure = Some(failure);
        component
    }
}

/// Run the verify pipeline and project the receipt or the aggregate
/// envelope.
pub fn verify(request: VerifyRequest<'_>) -> DomainResult {
    match run(request) {
        Ok(outcome) => outcome,
        Err(result) => result,
    }
}

/// The CI evidence seam (issue #103): run the pipeline and hand back
/// both the terminal domain result and every assembled component row.
/// The blocked verdict no longer discards the receipt: the components
/// ride alongside the aggregate envelope, so a CI report can project
/// the failure classes without re-running anything. The terminal
/// result is identical to [`verify`].
pub struct Verified {
    /// The terminal domain result (the exact `verify` projection).
    pub result: DomainResult,
    /// One component row per executed or declared-absent component, in
    /// fixed id order; empty when the pipeline failed before assembly.
    pub components: Vec<ComponentReceipt>,
    /// The verdict the receipt derived, when assembled.
    pub verdict: Option<Verdict>,
}

/// Run the verify pipeline retaining the typed evidence (issue #103).
pub fn verify_with_components(request: VerifyRequest<'_>) -> Verified {
    match run(request) {
        Ok(outcome) => {
            // A receipt-bearing success carries its components; the
            // degraded `UnsupportedOperation` path loses them today,
            // and the CI report records the declared absences instead.
            if let DomainResult::Valid { .. } = outcome {
                // The receipt bytes are re-derived by the report layer
                // through the declared component ids; the success
                // projection carries only the envelope.
            }
            Verified {
                result: outcome,
                components: components_of_last_run().unwrap_or_default(),
                verdict: verdict_of_last_run(),
            }
        }
        Err(result) => Verified {
            result,
            components: components_of_last_run().unwrap_or_default(),
            verdict: verdict_of_last_run(),
        },
    }
}

// The assembled components of the most recent `run` in this thread.
// The pipeline is single-threaded per invocation; the handoff is the
// bounded evidence seam between the runner and the CI layer.
thread_local! {
    static LAST_COMPONENTS: std::cell::RefCell<Option<(Vec<ComponentReceipt>, Option<Verdict>)>> =
        const { std::cell::RefCell::new(None) };
}

fn record_components(components: Vec<ComponentReceipt>, verdict: Option<Verdict>) {
    LAST_COMPONENTS.with(|slot| {
        *slot.borrow_mut() = Some((components, verdict));
    });
}

fn components_of_last_run() -> Option<Vec<ComponentReceipt>> {
    LAST_COMPONENTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|(components, _)| components.clone())
    })
}

fn verdict_of_last_run() -> Option<Verdict> {
    LAST_COMPONENTS.with(|slot| slot.borrow().as_ref().and_then(|(_, verdict)| *verdict))
}

/// `lekalo trace collect` (issue #56, plan S5): the one write command of
/// the trace surface. The scenario → test → gate manifest is rebuilt
/// from the adjudicated ingest home through the production rollup and
/// its canonical bytes persisted under `.lekalo/import/trace/` — the
/// durable document `trace validate`/`trace query` then inspect. An
/// empty or absent ingest home exports nothing and reports so honestly;
/// a record set that cannot survive the trace mechanism is a typed
/// refusal, never a partial document.
pub fn collect_scenario_trace(selection: &LoadSelection) -> DomainResult {
    match collect_scenario_trace_run(selection) {
        Ok(result) | Err(result) => result,
    }
}

fn collect_scenario_trace_run(selection: &LoadSelection) -> Result<DomainResult, DomainResult> {
    let prepared = Prepared::prepare(selection)
        .map_err(|failure| DomainResult::from(&Failure::Artifact(failure)))?;
    let compilation = compile(selection)?;
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())
        .ok_or(Failure::ProjectRefUnresolved)
        .map_err(DomainResult::from)?;
    let rollup = scenarios_execution_rollup(
        prepared.fs(),
        &scenario_trace_context(&prepared, &project_id),
        prepared.inputs().ir().digest(),
        &expected_scenario_tests(&prepared),
    );
    if !rollup.present {
        // No ingest home (or unreadable): there is nothing to export and
        // no stale export may linger — remove a previous document so the
        // durable surface never outlives its evidence.
        let stale = prepared
            .root()
            .join(crate::scenario_evidence::TRACE_EXPORT_DIR)
            .join(crate::scenario_evidence::TRACE_EXPORT_NAME);
        let _ = std::fs::remove_file(&stale);
        let json = format!(
            "{{\"status\":\"ok\",\"exported\":false,\"path\":\"{}/{}\",\"records\":0}}",
            crate::scenario_evidence::TRACE_EXPORT_DIR,
            crate::scenario_evidence::TRACE_EXPORT_NAME,
        );
        return Ok(DomainResult::receipt(
            json,
            "trace collect: no scenario run records to export".to_owned(),
        ));
    }
    let Some(manifest) = rollup.manifest_bytes else {
        // Records existed but could not survive the trace mechanism: a
        // typed refusal, never a partial or absent document.
        return Ok(DomainResult::invalid(
            crate::scenario_evidence::run_invalid("trace-export"),
        ));
    };
    let parsed = crate::trace::TraceManifest::parse(&manifest).map_err(DomainResult::invalid)?;
    let digest = parsed.digest().map_err(DomainResult::invalid)?;
    crate::observed::store::write_confined(
        prepared.root(),
        crate::scenario_evidence::TRACE_EXPORT_DIR,
        crate::scenario_evidence::TRACE_EXPORT_NAME,
        &manifest,
    )
    .map_err(DomainResult::invalid)?;
    let trace = rollup.trace.unwrap_or(TraceSummary {
        relations: 0,
        gaps: 0,
        uncovered_sinks: 0,
    });
    let json = format!(
        "{{\"status\":\"ok\",\"exported\":true,\"path\":\"{}/{}\",\"records\":{},\"relations\":{},\"gaps\":{},\"uncoveredSinks\":{},\"manifestDigest\":\"{}\"}}",
        crate::scenario_evidence::TRACE_EXPORT_DIR,
        crate::scenario_evidence::TRACE_EXPORT_NAME,
        rollup.records,
        trace.relations,
        trace.gaps,
        trace.uncovered_sinks,
        digest,
    );
    let human = format!(
        "trace collect: {}/{} ({} records; {} relations; {} gaps)",
        crate::scenario_evidence::TRACE_EXPORT_DIR,
        crate::scenario_evidence::TRACE_EXPORT_NAME,
        rollup.records,
        trace.relations,
        trace.gaps,
    );
    Ok(DomainResult::receipt(json, human))
}

fn run(request: VerifyRequest<'_>) -> Result<DomainResult, DomainResult> {
    let limits = super::generate::effective_limits(request.timeout_ms);
    let prepared = Prepared::prepare(request.selection)
        .map_err(|failure| DomainResult::from(&Failure::Artifact(failure)))?;
    let compilation = compile(request.selection)?;
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())
        .ok_or(Failure::ProjectRefUnresolved)
        .map_err(DomainResult::from)?;
    let mode = if request.changed_modules.is_empty() {
        "full"
    } else {
        "changed"
    };
    let scope = if let Some(module) = request.module.as_deref() {
        let known = compilation
            .project
            .modules
            .iter()
            .any(|candidate| candidate.id.as_str() == module);
        if !known {
            return Err(Failure::Loader(super::generate::unknown_module(module)).into());
        }
        ScopeReceipt {
            modules: vec![module.to_owned()],
            all_modules: false,
        }
    } else if request.changed_modules.is_empty() {
        ScopeReceipt {
            modules: Vec::new(),
            all_modules: true,
        }
    } else {
        ScopeReceipt {
            modules: request.changed_modules.clone(),
            all_modules: false,
        }
    };
    let inputs = InputsReceipt {
        model_version: prepared.inputs().model().version().as_str().to_owned(),
        model_digest: prepared.inputs().model().digest().as_str().to_owned(),
        ir_version: prepared.inputs().ir().version().as_str().to_owned(),
        ir_digest: prepared.inputs().ir().digest().as_str().to_owned(),
        revision: crate::artifacts::check::inputs_revision(
            prepared.inputs().model(),
            prepared.inputs().ir(),
        )
        .as_str()
        .to_owned(),
    };
    let ir_digest = prepared.inputs().ir().digest();

    let mut components: Vec<Component> = Vec::new();
    // Required: the core structural/semantic validation.
    components.push(model_component(&compilation));
    // Required: the ownership drift gate.
    components.push(drift_component(request.selection));

    // Per-target adapter validation, isolated; without an explicit
    // selection every locked adapter is verified, and a lock that pins
    // no adapters records the declared absence.
    let mut targets = request.targets.clone();
    if targets.is_empty() {
        targets = prepared
            .lock()
            .adapters()
            .iter()
            .map(|adapter| adapter.id().as_str().to_owned())
            .collect();
        targets.sort();
        targets.dedup();
    }
    if let Some(supply) = request.supply.as_ref() {
        for target in &targets {
            components.push(adapter_component(
                &prepared,
                &compilation,
                supply,
                target,
                ir_digest,
                limits,
                request.locked,
            ));
        }
    } else if !targets.is_empty() {
        for target in &targets {
            components.push(Component::state(
                &format!("adapter.{target}"),
                true,
                ComponentState::Unsupported,
                Some("lock.component-unavailable"),
            ));
        }
    }

    // Optional components.
    components.push(bindings_component(request.selection));
    components.push(scenarios_component(&compilation, &scope));
    components.push(scenarios_execution_component(&prepared, &project_id));
    components.push(Component::state(
        "native.gates",
        false,
        ComponentState::Unsupported,
        Some("core.capability-unavailable"),
    ));
    if let Some(trace) = request.trace.as_deref() {
        components.push(trace_component(&prepared, trace));
    } else {
        components.push(Component::state(
            "trace.summary",
            false,
            ComponentState::Absent,
            None,
        ));
    }
    components.sort_by(|left, right| left.receipt.id.cmp(&right.receipt.id));

    // The verdict: any failure blocks; a required component that is
    // degraded, unsupported, or absent degrades; an optional component
    // with findings degrades. The two declared permanent absences
    // (scenario execution and native gates, both carrying the fixed
    // capability-unavailable reason) stay exit-neutral until their
    // backends land: they are reported in the receipt, never skipped.
    let verdict = if components
        .iter()
        .any(|component| component.receipt.state == ComponentState::Fail)
    {
        Verdict::Blocked
    } else if components.iter().any(|component| {
        component.receipt.state == ComponentState::Degraded
            || (component.receipt.required
                && matches!(
                    component.receipt.state,
                    ComponentState::Unsupported | ComponentState::Absent,
                ))
    }) {
        Verdict::Degraded
    } else {
        Verdict::Ready
    };
    let receipt = VerifyReceipt {
        schema_version: SCHEMA_VERSION,
        operation: "verify",
        mode,
        identity: IDENTITY,
        project: project_id,
        lock_digest: prepared.lock().digest().as_str().to_owned(),
        locked: request.locked,
        inputs,
        scope,
        components: components.iter().map(|c| c.receipt.clone()).collect(),
        verdict,
    };
    // Issue #103: the CI evidence seam captures the assembled rows
    // before any envelope aggregation discards them.
    record_components(receipt.components.clone(), Some(receipt.verdict));
    if verdict == Verdict::Blocked {
        // The aggregate failure envelope preserves every component
        // failure; the exit class follows the deterministic precedence.
        let failures: Vec<DomainResult> = components
            .iter()
            .filter_map(|component| component.failure.clone())
            .collect();
        return Err(super::generate::aggregate_envelopes(failures));
    }
    if verdict == Verdict::Degraded {
        // Degraded verification is never a silent success: the envelope
        // is the unsupported class with one registered diagnostic per
        // non-pass component, exit 4.
        // One registered capability diagnostic per non-pass component;
        // the component id rides in the symbol slot and the receipt
        // carries the specific reason codes.
        let diagnostics: Vec<Diagnostic> = components
            .iter()
            .filter(|component| {
                matches!(
                    component.receipt.state,
                    ComponentState::Degraded | ComponentState::Unsupported
                )
            })
            .map(|component| {
                crate::diagnostics::normalize::build(
                    "core.capability-unavailable",
                    Some(component.receipt.id.clone()),
                    None,
                    DataObject::new(),
                )
                .expect("core.capability-unavailable is registered and active")
            })
            .collect();
        let set = super::generate::wire_set(crate::result::Status::Unsupported, diagnostics);
        return Ok(DomainResult::UnsupportedOperation { diagnostics: set });
    }
    Ok(DomainResult::receipt(
        serde_json::to_string_pretty(&receipt).expect("verify receipt serializes"),
        verify_human(&receipt),
    ))
}

/// The stable human summary of a verify receipt.
fn verify_human(receipt: &VerifyReceipt) -> String {
    let pass = receipt
        .components
        .iter()
        .filter(|component| component.receipt_state_pass())
        .count();
    format!(
        "verify {} {} : {} components ({} pass), verdict {}",
        receipt.project,
        receipt.mode,
        receipt.components.len(),
        pass,
        receipt.verdict.as_str()
    )
}

impl ComponentReceipt {
    fn receipt_state_pass(&self) -> bool {
        self.state == ComponentState::Pass
    }
}

/// The required core structural/semantic validation component.
fn model_component(compilation: &Compilation) -> Component {
    let profile = match ValidationProfile::embedded_default() {
        Ok(profile) => profile,
        Err(_) => {
            return Component::failed(
                "model.validation",
                true,
                "diagnostics.registry-invalid",
                DomainResult::Invalid {
                    diagnostics: validator::registry_invariant_failure(),
                },
            )
        }
    };
    match validator::validate(compilation, profile) {
        Ok(report) => {
            let counts = report.counts();
            let mut component = Component::pass("model.validation", true);
            component.receipt.severity_counts = Some(SeverityCounts {
                errors: counts.error as usize,
                warnings: counts.warning as usize,
                infos: counts.info as usize,
            });
            component
        }
        Err(set) => Component::failed(
            "model.validation",
            true,
            "semantic.invalid",
            DomainResult::Invalid { diagnostics: set },
        ),
    }
}

/// The required ownership drift component.
fn drift_component(selection: &LoadSelection) -> Component {
    match GenerateService::check(selection) {
        Ok(receipt) => {
            let counts = &receipt.counts;
            let blocking = counts.stale + counts.manual_drift + counts.missing + counts.orphan;
            let state = if blocking > 0 {
                ComponentState::Fail
            } else if counts.reported > 0 {
                ComponentState::Degraded
            } else {
                ComponentState::Pass
            };
            let mut component = Component::state(
                "artifact.drift",
                true,
                state,
                (blocking > 0).then_some("lock.source-changed"),
            );
            component.receipt.verdict_counts = Some(VerdictCountsReceipt {
                artifacts: counts.artifacts,
                clean: counts.clean,
                stale: counts.stale,
                manual_drift: counts.manual_drift,
                missing: counts.missing,
                orphan: counts.orphan,
                blocking,
            });
            component
        }
        Err(failure) => {
            let mapped = DomainResult::from(&Failure::Artifact(failure));
            match mapped.status() {
                crate::result::Status::Unavailable | crate::result::Status::Unsupported => {
                    Component::state(
                        "artifact.drift",
                        true,
                        ComponentState::Unsupported,
                        Some("lock.missing"),
                    )
                }
                _ => Component::failed("artifact.drift", true, "lock.source-changed", mapped),
            }
        }
    }
}

/// One target's adapter validation component, fully isolated.
#[allow(clippy::too_many_arguments)]
fn adapter_component(
    prepared: &Prepared,
    compilation: &Compilation,
    supply: &AdapterSupply,
    target: &str,
    ir_digest: &Sha256Digest,
    limits: TransportLimits,
    locked_required: bool,
) -> Component {
    let id = format!("adapter.{target}");
    let mut client = TargetClient::default();
    let discovered = match discover(&mut client, supply, prepared.root(), limits) {
        Ok(discovered) => discovered,
        Err(failure) => return target_component(&id, true, failure),
    };
    let Some(locked) = locked_adapter(prepared.lock(), &discovered) else {
        return target_component(
            &id,
            true,
            Failure::AdapterNotLocked {
                adapter: discovered.adapter.id.clone(),
            },
        );
    };
    let executable = match binding_failure(locked, &discovered) {
        Ok(executable) => executable,
        Err(failure) => return target_component(&id, true, failure),
    };
    if locked_required {
        if let Err(_result) = super::generate::locked_preflight_with(
            prepared,
            &super::generate::locked_inventory(locked, &executable),
        ) {
            return Component::state(
                &id,
                true,
                ComponentState::Unsupported,
                Some("lock.component-unavailable"),
            );
        }
    }
    // Verify is read-only: the canonical IR evidence must already exist
    // and match the current inputs revision exactly.
    let Some(project) = compilation.project.project.as_ref() else {
        return Component::state(
            &id,
            true,
            ComponentState::Unsupported,
            Some("core.capability-unavailable"),
        );
    };
    let project = project.id.as_str();
    let evidence_path = format!("{IR_EVIDENCE_DIR}/{project}.json");
    let evidence = crate::artifacts::check::observe(prepared.fs(), &evidence_path);
    match evidence {
        Ok(Some(observed)) if &observed.digest == ir_digest => {}
        Ok(_) => {
            return Component::state(&id, true, ComponentState::Unsupported, Some("lock.stale"));
        }
        Err(failure) => return target_component(&id, true, Failure::Artifact(failure)),
    }
    let profile = discovered.selected_profile(None).unwrap_or_default();
    let profile_token = (!profile.is_empty()).then_some(profile.as_str());
    let outcome = client.call(
        &supply.command,
        CallRequest {
            operation: Operation::Verify,
            target: Some(target),
            profile: profile_token,
            profile_resolution: None,
            ir_path: Some(&evidence_path),
            dry_run: None,
            plan_id: None,
            native_request: None,
        },
        prepared.root(),
        prepared.fs(),
        None,
    );
    match outcome {
        Ok(outcome) => {
            let result: Option<&OperationResult> = outcome.response.result.as_ref();
            let ok = result.and_then(|result| result.ok).unwrap_or(false);
            let findings = result
                .and_then(|result| result.findings.as_ref())
                .map(|findings| findings.len())
                .unwrap_or(0);
            let state = if ok {
                ComponentState::Pass
            } else {
                ComponentState::Fail
            };
            let mut component =
                Component::state(&id, true, state, (!ok).then_some("adapter.check-failed"));
            component.receipt.findings = Some(findings);
            component
        }
        Err(failure) => target_component(&id, true, Failure::Target(failure)),
    }
}

/// Project one target failure onto its isolated component row: the
/// mapped status decides fail versus declared absence.
fn target_component(id: &str, required: bool, failure: Failure) -> Component {
    let mapped = DomainResult::from(&failure);
    match mapped.status() {
        crate::result::Status::Unavailable | crate::result::Status::Unsupported => {
            let reason = mapped
                .diagnostics()
                .first()
                .map(|diagnostic| diagnostic.id().to_owned())
                .unwrap_or_else(|| "core.capability-unavailable".to_owned());
            Component::state(id, required, ComponentState::Unsupported, Some(&reason))
        }
        _ => {
            let reason = mapped
                .diagnostics()
                .first()
                .map(|diagnostic| diagnostic.id().to_owned())
                .unwrap_or_else(|| "adapter.check-failed".to_owned());
            Component::failed(id, required, &reason, mapped)
        }
    }
}

/// The optional binding-registry freshness component.
fn bindings_component(selection: &LoadSelection) -> Component {
    let context = match crate::observed::context(selection) {
        Ok(context) => context,
        Err(result) => {
            let missing = result.status() == crate::result::Status::Invalid
                && result
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.id() == "observed.index-missing");
            if missing {
                return Component::state(
                    "bindings.registry",
                    false,
                    ComponentState::Absent,
                    Some("observed.index-missing"),
                );
            }
            return Component::failed("bindings.registry", false, "observed.index-io", result);
        }
    };
    match crate::observed::bindings::list(&context) {
        Ok(receipt) => {
            let stale = receipt.counts.stale;
            let mut component = Component::state(
                "bindings.registry",
                false,
                if stale > 0 {
                    ComponentState::Degraded
                } else {
                    ComponentState::Pass
                },
                (stale > 0).then_some("observed.stale-binding"),
            );
            component.receipt.findings = Some(stale);
            component
        }
        Err(set) => {
            // A project that never recorded an observed index has no
            // binding registry: legal absence, never a failure.
            let missing = set
                .as_slice()
                .iter()
                .any(|diagnostic| diagnostic.id() == "observed.index-missing");
            if missing {
                return Component::state(
                    "bindings.registry",
                    false,
                    ComponentState::Absent,
                    Some("observed.index-missing"),
                );
            }
            Component::failed(
                "bindings.registry",
                false,
                "observed.index-io",
                DomainResult::Invalid { diagnostics: set },
            )
        }
    }
}

/// The optional scenario-execution evidence component (issue #47, plan
/// S9): the durable run records in the adjudicated ingest home roll up
/// deterministically. A run with any assertion or infrastructure
/// failure fails the component; unsupported rows degrade it (never a
/// pass); an empty ingest home stays the declared absence it was before
/// the backend landed — reported, exit-neutral, never silently skipped.
fn scenarios_execution_component(prepared: &Prepared, project_id: &str) -> Component {
    let rollup = scenarios_execution_rollup(
        prepared.fs(),
        &scenario_trace_context(prepared, project_id),
        prepared.inputs().ir().digest(),
        &expected_scenario_tests(prepared),
    );
    if !rollup.present {
        // No ingest home (or unreadable): the backend has not run.
        return Component::state(
            SCENARIOS_EXECUTION,
            false,
            ComponentState::Unsupported,
            Some("core.capability-unavailable"),
        );
    }
    let state = if rollup.blocking > 0 || rollup.missing > 0 {
        ComponentState::Fail
    } else if rollup.unsupported > 0 || rollup.degraded > 0 || rollup.stale > 0 {
        ComponentState::Degraded
    } else {
        ComponentState::Pass
    };
    let mut component = Component::state(SCENARIOS_EXECUTION, false, state, rollup_reason(&rollup));
    // The blocked verdict discards the receipt: the classed envelope is
    // what carries the assertion/infrastructure distinction to the user.
    component.failure = rollup.failure;
    component.receipt.findings = Some(
        rollup.blocking + rollup.missing + rollup.unsupported + rollup.degraded + rollup.stale,
    );
    // F-7: the exported manifest rows ride in the receipt's trace slot,
    // so the verify receipt carries the emitted relations.
    component.receipt.trace = rollup.trace;
    component
}

/// The semantic reason of one execution rollup (issue #56, plan S5):
/// evidence integrity outranks the conformance classes, and every
/// reason is a registered diagnostic id — the same id the failure
/// envelope carries, so the classification is observable on a blocked
/// verdict, not only inside the discarded receipt. Assertion failures
/// dominate boot/infrastructure findings: an `invalid` envelope always
/// outranks `unavailable` under the established aggregation precedence.
fn rollup_reason(rollup: &ExecutionRollup) -> Option<&'static str> {
    if rollup.invalid > 0 {
        return Some("scenario.run-record-invalid");
    }
    if rollup.assertion > 0 {
        return Some("scenario.assertion-failed");
    }
    if rollup.infrastructure > 0 || rollup.missing > 0 {
        // A boot failure and a declared test that never reported are the
        // same class: the suite could not produce its evidence.
        return Some("scenario.infrastructure");
    }
    // Review F-10: evidence compiled under a previous IR revision is
    // stale — it degrades the component with its own reason, never
    // indistinguishable from current evidence.
    if rollup.stale > 0 {
        return Some("scenario.stale-evidence");
    }
    if rollup.unsupported > 0 || rollup.degraded > 0 {
        return Some("scenario.unsupported-capability");
    }
    None
}

/// The failure envelope of a blocked scenario-execution gate: one
/// registered diagnostic per present failure class, with the first
/// offending scenario named — a blocked verify surfaces the class
/// distinction, never a bare component id. A diagnostic set holds one
/// status class, so the `invalid` family (malformed records, failed
/// assertions) wins the envelope when both families are present; the
/// unavailable family's counts stay in the receipt's findings.
fn execution_failure(
    rollup: &ExecutionRollup,
    first_assertion: Option<&(String, Option<String>)>,
    first_infrastructure: Option<&String>,
    first_missing: Option<&String>,
) -> DomainResult {
    use crate::diagnostics::DataObject;
    use crate::scenario::diagnostic::{one, token};
    let mut invalid: Vec<Diagnostic> = Vec::new();
    if rollup.invalid > 0 {
        let mut data = DataObject::new();
        data.insert("detail".to_owned(), token("evidence-invalid"));
        if let Ok(diagnostic) = one("scenario.run-record-invalid", None, data) {
            invalid.push(diagnostic);
        }
    }
    if rollup.assertion > 0 {
        let mut data = DataObject::new();
        let (scenario, step) = first_assertion
            .map(|(scenario, step)| (scenario.as_str(), step.as_deref().unwrap_or("run")))
            .unwrap_or(("suite", "run"));
        data.insert("scenario".to_owned(), token(scenario));
        data.insert("step".to_owned(), token(step));
        if let Ok(diagnostic) = one("scenario.assertion-failed", Some(scenario.to_owned()), data) {
            invalid.push(diagnostic);
        }
    }
    if !invalid.is_empty() {
        return DomainResult::Invalid {
            diagnostics: super::generate::wire_set(crate::result::Status::Invalid, invalid),
        };
    }
    let mut unavailable: Vec<Diagnostic> = Vec::new();
    if rollup.infrastructure > 0 {
        let scenario = first_infrastructure.map_or("suite", String::as_str);
        let mut data = DataObject::new();
        data.insert("scenario".to_owned(), token(scenario));
        data.insert("detail".to_owned(), token("boot-failure"));
        if let Ok(diagnostic) = one("scenario.infrastructure", Some(scenario.to_owned()), data) {
            unavailable.push(diagnostic);
        }
    }
    if rollup.missing > 0 {
        let scenario = first_missing.map_or("suite", String::as_str);
        let mut data = DataObject::new();
        data.insert("scenario".to_owned(), token(scenario));
        data.insert("detail".to_owned(), token("missing-record"));
        if let Ok(diagnostic) = one("scenario.infrastructure", Some(scenario.to_owned()), data) {
            unavailable.push(diagnostic);
        }
    }
    DomainResult::Unavailable {
        diagnostics: super::generate::wire_set(crate::result::Status::Unavailable, unavailable),
    }
}

/// The trace export context of one prepared project (review F-7): the
/// manifest revision is the exact lock revision, the manifest header
/// pins the current Model digest, and the gate identity names the
/// scenario-execution component itself — the receipt-validated gate,
/// never an arbitrary string (issue #56, plan S5).
fn scenario_trace_context(prepared: &Prepared, project_id: &str) -> TraceContext {
    // The manifest revision grammar is bare lowercase hex — the lock
    // digest's `sha256:` wire spelling would never validate.
    let mut context = TraceContext::new(
        prepared
            .lock()
            .digest()
            .as_str()
            .strip_prefix("sha256:")
            .expect("the lock digest spelling is fixed")
            .to_owned(),
    );
    context.model_digest = prepared.inputs().model().digest().as_str().to_owned();
    context.project = project_id.to_owned();
    context.gate = Some(SCENARIOS_EXECUTION.to_owned());
    context
}

/// The expected scenario-test inventory of one prepared project (issue
/// #56, plan S5): every recorded `test` artifact under a scenario-tests
/// home — generated or scaffolded alike — owes one run record, so a
/// suite that silently drops a test can never roll up as a successful
/// partial run. Without an ownership manifest there is no declared
/// inventory to hold anyone to.
fn expected_scenario_tests(prepared: &Prepared) -> std::collections::BTreeSet<String> {
    prepared
        .manifest()
        .map(|manifest| {
            manifest
                .artifacts()
                .iter()
                .filter(|entry| {
                    let path = entry.key().path().as_str();
                    entry.key().kind() == ArtifactKind::Test
                        && path.split('/').any(|segment| segment == "scenario-tests")
                        && (path.ends_with(".test.ts") || path.ends_with(".test.php"))
                })
                .map(|entry| entry.key().path().as_str().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The deterministic rollup of one ingest home (review F-7): every
/// durable run record is aggregated exactly like the component does, and
/// the surviving records are exported through the trace manifest
/// mechanism — [`trace_manifest_document`] assembles the closed wire
/// document (the production [`crate::scenario_evidence::trace_relations`]
/// caller) and [`crate::trace::TraceManifest::parse`] re-validates it
/// before the rows are published. A document that does not survive the
/// mechanism is a blocking failure, never a silent skip.
#[derive(Default)]
pub(crate) struct ExecutionRollup {
    /// The ingest home held at least one parsable record.
    pub present: bool,
    pub blocking: usize,
    /// Records carrying at least one failed assertion row (issue #56).
    pub assertion: usize,
    /// Records carrying at least one infrastructure row (issue #56): a
    /// boot, spawn, or teardown failure is never confused with an
    /// evaluated assertion that failed.
    pub infrastructure: usize,
    /// Records that never parsed — unreadable, malformed, or off-contract
    /// bytes (issue #56): evidence integrity, not a conformance verdict.
    pub invalid: usize,
    /// Declared scenario tests that produced no run record at all
    /// (issue #56): a silently dropped test is never a partial pass.
    pub missing: usize,
    pub unsupported: usize,
    pub degraded: usize,
    /// Records compiled under a previous IR revision (review F-10).
    pub stale: usize,
    /// The count of parsed records the manifest covers.
    pub records: usize,
    pub trace: Option<TraceSummary>,
    /// The canonical bytes of the validated exported manifest — the
    /// durable document `lekalo trace collect` persists.
    pub manifest_bytes: Option<Vec<u8>>,
    /// The classed failure envelope a blocked gate aggregates — the
    /// classification is observable in the emitted diagnostics, never
    /// only inside the receipt.
    pub failure: Option<DomainResult>,
}

/// Roll one ingest home up over the validated root capability.
pub(crate) fn scenarios_execution_rollup(
    fs: &Fs,
    context: &TraceContext,
    expected_ir: &Sha256Digest,
    expected: &std::collections::BTreeSet<String>,
) -> ExecutionRollup {
    let dir = crate::scenario_evidence::INGEST_DIR;
    let mut rollup = ExecutionRollup::default();
    let entries = match fs.entries(dir) {
        Ok(entries) => entries,
        Err(_) => {
            // No ingest home (or unreadable): the backend has not run.
            return rollup;
        }
    };
    let mut names: Vec<String> = entries
        .iter()
        .filter(|(name, kind)| *kind == EntryType::File && name.ends_with(".json"))
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    if names.is_empty() {
        return rollup;
    }
    rollup.present = true;
    let mut all_records = Vec::new();
    let mut observed = std::collections::BTreeSet::new();
    let mut first_assertion: Option<(String, Option<String>)> = None;
    let mut first_infrastructure: Option<String> = None;
    for name in &names {
        let bytes = match fs.read_file_opt(dir, name, 1 << 20) {
            Ok(Some(bytes)) => bytes,
            _ => {
                rollup.blocking += 1;
                rollup.invalid += 1;
                continue;
            }
        };
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                rollup.blocking += 1;
                rollup.invalid += 1;
                continue;
            }
        };
        let record = match RunRecord::from_value(&value) {
            Ok(record) => record,
            Err(_) => {
                rollup.blocking += 1;
                rollup.invalid += 1;
                continue;
            }
        };
        let summary = record.summary();
        // Review F-10: a record produced under a previous IR revision is
        // stale evidence — it counts, but the component degrades so the
        // staleness is never indistinguishable from current evidence.
        if record.ir_digest != expected_ir.as_str() {
            rollup.stale += 1;
        }
        // Issue #56: the failure classes stay distinct in the rollup —
        // an evaluated assertion that failed and a boot/infrastructure
        // failure are different findings with different reasons. The
        // first offender of each class is kept so the blocked envelope
        // names a real scenario, never a vague suite token.
        if summary.failed > 0 {
            rollup.assertion += 1;
            if first_assertion.is_none() {
                let step = record
                    .assertions
                    .iter()
                    .find(|row| row.outcome == "fail")
                    .and_then(|row| row.step_id.clone());
                first_assertion = Some((record.scenario_id.clone(), step));
            }
        }
        if summary.infrastructure > 0 {
            rollup.infrastructure += 1;
            if first_infrastructure.is_none() {
                first_infrastructure = Some(record.scenario_id.clone());
            }
        }
        if summary.has_blocking_failure() {
            rollup.blocking += 1;
        } else if summary.unsupported > 0 || summary.degraded > 0 {
            rollup.unsupported += 1;
        } else if summary.all_passed() {
            // counted as executed; nothing else to aggregate
        } else {
            rollup.degraded += 1;
        }
        observed.insert(record.test.1.clone());
        all_records.push(record);
    }
    // Issue #56: every declared scenario-test artifact owes one record;
    // a suite that dropped a test never rolls up as a partial pass. The
    // first missing path is reduced to its scenario stem so the blocked
    // envelope names the scenario, not a file.
    let first_missing = expected
        .iter()
        .find(|path| !observed.contains(*path))
        .map(|path| {
            let file = path.rsplit('/').next().unwrap_or(path.as_str());
            file.strip_suffix(".test.ts")
                .or_else(|| file.strip_suffix(".test.php"))
                .unwrap_or(file)
                .to_owned()
        });
    rollup.missing = expected
        .iter()
        .filter(|path| !observed.contains(*path))
        .count();
    rollup.records = all_records.len();
    if !all_records.is_empty() {
        // F-7: export the aggregated relations through the trace manifest
        // mechanism; the emitted rows are observable in the rollup, and
        // the canonical bytes are the durable document `lekalo trace
        // collect` persists. A document that does not survive the
        // mechanism is a blocking failure, never a silent skip.
        match trace_manifest_document(&all_records, context) {
            Ok(document) => {
                let bytes = serde_json::to_vec_pretty(&document).expect("document serializes");
                match crate::trace::TraceManifest::parse(&bytes) {
                    Ok(parsed) => {
                        let report = parsed.report();
                        rollup.trace = Some(TraceSummary {
                            relations: report.relation_count,
                            gaps: report.gap_count,
                            uncovered_sinks: parsed.uncovered_sinks().len(),
                        });
                        rollup.manifest_bytes =
                            parsed.canonical_bytes().ok().map(String::into_bytes);
                    }
                    Err(_) => {
                        rollup.blocking += 1;
                        rollup.invalid += 1;
                    }
                }
            }
            Err(_) => {
                rollup.blocking += 1;
                rollup.invalid += 1;
            }
        }
    }
    // Issue #56: attach the classed failure envelope once every failure
    // source — records, inventory, the manifest mechanism — has counted.
    if rollup.blocking > 0 || rollup.missing > 0 {
        rollup.failure = Some(execution_failure(
            &rollup,
            first_assertion.as_ref(),
            first_infrastructure.as_ref(),
            first_missing.as_ref(),
        ));
    }
    rollup
}

/// The optional portable-scenario coverage component.
fn scenarios_component(compilation: &Compilation, scope: &ScopeReceipt) -> Component {
    let scenarios: Vec<&crate::ir::ScenarioDef> = compilation
        .project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            crate::ir::Definition::Scenario(scenario) => Some(scenario),
            _ => None,
        })
        .filter(|scenario| {
            scope.all_modules
                || scope
                    .modules
                    .iter()
                    .any(|module| scenario.id.as_str().starts_with(module.as_str()))
        })
        .collect();
    if scenarios.is_empty() {
        return Component::state("scenarios.portable", false, ComponentState::Absent, None);
    }
    let mut covered: Vec<&str> = Vec::new();
    let mut uncovered = 0usize;
    for scenario in &scenarios {
        if scenario.covers.is_empty() {
            uncovered += 1;
        }
        for reference in &scenario.covers {
            covered.push(reference.as_str());
        }
    }
    covered.sort_unstable();
    covered.dedup();
    let mut component = Component::pass("scenarios.portable", false);
    component.receipt.scenarios = Some(ScenarioCoverage {
        scenarios: scenarios.len(),
        covered_operations: covered.len(),
        uncovered_scenarios: uncovered,
    });
    component
}

/// The optional trace summary component.
fn trace_component(prepared: &Prepared, logical: &str) -> Component {
    let (dir, name) = crate::artifacts::check::split(logical);
    let bytes = match prepared.fs().read_file_opt(dir, name, 1 << 20) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Component::state(
                "trace.summary",
                false,
                ComponentState::Absent,
                Some("structure.document-missing"),
            )
        }
        Err(_) => {
            return Component::state(
                "trace.summary",
                false,
                ComponentState::Absent,
                Some("loader.io"),
            )
        }
    };
    match crate::trace::TraceManifest::parse(&bytes) {
        Ok(manifest) => {
            let report = manifest.report();
            let gaps = report.gap_count;
            let uncovered = manifest.uncovered_sinks().len();
            let mut component = Component::pass("trace.summary", false);
            component.receipt.trace = Some(TraceSummary {
                relations: report.relation_count,
                gaps,
                uncovered_sinks: uncovered,
            });
            component
        }
        Err(set) => Component::failed(
            "trace.summary",
            false,
            "graph.input-invalid",
            DomainResult::Invalid { diagnostics: set },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{rollup_reason, scenarios_execution_rollup, ExecutionRollup};
    use crate::lockfile::types::Sha256Digest;
    use crate::project_fs::Fs;
    use crate::result::DomainResult;
    use crate::scenario_evidence::TraceContext;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::fs;
    use tempfile::TempDir;

    /// The canonical run-record fixture, byte-identical in shape to the
    /// reporter's canonical writer output (scenario_evidence tests).
    fn valid_run_record() -> serde_json::Value {
        json!({
            "assertions": [
                { "kind": "result", "observes": "run", "outcome": "pass", "step_id": "output" },
                { "kind": "entity_state", "observes": "run", "outcome": "pass", "step_id": "state" }
            ],
            "binding_mode": "generated",
            "identity": "dev.lekalo.scenario-run@0.4.0",
            "profile": null,
            "runner": { "id": "node:test", "version": "24.13.0" },
            "schema_version": "lekalo/scenario-run/v0.4.0",
            "scenario": {
                "id": "planner.scenario.focus_happy",
                "ir_digest": format!("sha256:{}", "1".repeat(64)),
                "operations": ["planner.focus_task"],
                "symbols": [],
                "version": "0.2.16"
            },
            "started_by": "lekalo-scenario-harness",
            "test": {
                "fingerprint": format!("sha256:{}", "2".repeat(64)),
                "id": "planner.scenario.focus_happy",
                "path": "src/generated/node-typescript/scenario-tests/planner/planner.scenario.focus_happy.test.ts"
            }
        })
    }

    fn context() -> TraceContext {
        let mut context = TraceContext::new("3".repeat(64));
        context.model_digest = format!("sha256:{}", "4".repeat(64));
        context
    }

    /// The gated production context: verify exports under the
    /// scenario-execution gate identity the receipt validates.
    fn gated_context() -> TraceContext {
        let mut context = context();
        context.gate = Some(super::SCENARIOS_EXECUTION.to_owned());
        context
    }

    /// No declared scenario-test inventory.
    fn no_expected() -> BTreeSet<String> {
        BTreeSet::new()
    }

    /// The prepared IR digest the fixture's record was compiled under.
    fn fixture_ir() -> Sha256Digest {
        Sha256Digest::parse(&format!("sha256:{}", "1".repeat(64))).expect("digest")
    }

    /// Review F-7: `trace_relations` has a production caller — verify
    /// ingests the durable run records and exports their relations
    /// through the trace manifest mechanism; the test observes the
    /// emitted manifest rows in the rollup.
    #[test]
    fn execution_rollup_emits_trace_manifest_rows_from_ingested_records() {
        let temp = TempDir::new().expect("temp dir");
        let ingest = temp.path().join(".lekalo/import/scenario-runs");
        fs::create_dir_all(&ingest).expect("ingest home");
        fs::write(
            ingest.join("run.json"),
            serde_json::to_vec_pretty(&valid_run_record()).expect("record serializes"),
        )
        .expect("record written");
        let fs = Fs::open(temp.path()).expect("validated root");

        let rollup = scenarios_execution_rollup(&fs, &context(), &fixture_ir(), &no_expected());

        assert!(rollup.present, "ingest home with one record is present");
        assert_eq!(rollup.blocking, 0, "a passing record never blocks");
        assert_eq!(rollup.unsupported, 0);
        assert_eq!(rollup.degraded, 0);
        assert_eq!(rollup.stale, 0, "a record under the current IR is fresh");
        let trace = rollup.trace.expect("the manifest rows are emitted");
        // One record, one operation, no gate: exactly the two verifies
        // rows (test→scenario, test→symbol) survive the mechanism.
        assert_eq!(trace.relations, 2, "verifies rows are exported");
        // The partial manifest declares the explicit missing-requirement
        // gap the scenario segment cannot close.
        assert_eq!(trace.gaps, 1);
    }

    /// Issue #56: under the production gate context the manifest exports
    /// the evidences edges — gate → native_test and gate → scenario —
    /// on top of the verifies rows, and the canonical manifest bytes are
    /// the durable document `lekalo trace collect` persists.
    #[test]
    fn execution_rollup_exports_gate_evidences_and_the_durable_document() {
        let temp = TempDir::new().expect("temp dir");
        let ingest = temp.path().join(".lekalo/import/scenario-runs");
        fs::create_dir_all(&ingest).expect("ingest home");
        fs::write(
            ingest.join("run.json"),
            serde_json::to_vec_pretty(&valid_run_record()).expect("record serializes"),
        )
        .expect("record written");
        let fs = Fs::open(temp.path()).expect("validated root");

        let rollup =
            scenarios_execution_rollup(&fs, &gated_context(), &fixture_ir(), &no_expected());

        let trace = rollup.trace.expect("the gated manifest is emitted");
        // verifies test→scenario + test→symbol, then evidences
        // gate→test + gate→scenario.
        assert_eq!(trace.relations, 4, "the gate evidences rows ride along");
        assert_eq!(trace.gaps, 1, "the declared missing-requirement gap stays");
        let bytes = rollup
            .manifest_bytes
            .expect("the canonical document is exported");
        let parsed = crate::trace::TraceManifest::parse(&bytes).expect("re-parse");
        let rows = parsed
            .query(&crate::trace::QuerySelection::GatesFor(
                "planner.scenario.focus_happy".to_owned(),
            ))
            .expect("the test id resolves");
        assert!(
            rows.iter().any(|row| {
                row.relation == crate::trace::relation::RelationKind::Evidences
                    && row.id == super::SCENARIOS_EXECUTION
            }),
            "the validated gate evidences the native test"
        );
    }

    /// Review F-10: a record compiled under a previous IR revision is
    /// stale evidence — the rollup counts it and the component degrades
    /// with the stale-evidence reason, never a silent pass.
    #[test]
    fn execution_rollup_marks_records_under_a_previous_ir_revision_stale() {
        let temp = TempDir::new().expect("temp dir");
        let ingest = temp.path().join(".lekalo/import/scenario-runs");
        fs::create_dir_all(&ingest).expect("ingest home");
        fs::write(
            ingest.join("run.json"),
            serde_json::to_vec_pretty(&valid_run_record()).expect("record serializes"),
        )
        .expect("record written");
        let fs = Fs::open(temp.path()).expect("validated root");
        let current = Sha256Digest::parse(&format!("sha256:{}", "9".repeat(64))).expect("digest");

        let rollup = scenarios_execution_rollup(&fs, &context(), &current, &no_expected());

        assert!(rollup.present);
        assert_eq!(rollup.stale, 1, "the previous-revision record is stale");
        assert_eq!(rollup.blocking, 0, "staleness is degradation, not failure");
    }

    /// The declared absence is preserved: without an ingest home the
    /// rollup reports nothing, and the component stays the unsupported
    /// absence it was before the backend landed.
    #[test]
    fn execution_rollup_stays_absent_without_an_ingest_home() {
        let temp = TempDir::new().expect("temp dir");
        let fs = Fs::open(temp.path()).expect("validated root");

        let rollup = scenarios_execution_rollup(&fs, &context(), &fixture_ir(), &no_expected());

        assert!(!rollup.present);
        assert!(rollup.trace.is_none());
        assert_eq!(
            (rollup.blocking, rollup.unsupported, rollup.degraded),
            (0, 0, 0)
        );
    }

    /// Issue #56: assertion failures and boot/infrastructure failures
    /// keep separate counters and separate reasons — an evaluated
    /// assertion that failed is never reported as a boot failure, and a
    /// run carrying both reports the mixed class honestly.
    #[test]
    fn execution_rollup_distinguishes_assertion_and_infrastructure() {
        let temp = TempDir::new().expect("temp dir");
        let ingest = temp.path().join(".lekalo/import/scenario-runs");
        fs::create_dir_all(&ingest).expect("ingest home");
        let mut failing = valid_run_record();
        failing["assertions"][0]["outcome"] = json!("fail");
        failing["test"]["id"] = json!("node:failing");
        failing["test"]["path"] =
            json!("src/generated/node-typescript/scenario-tests/planner/failing.test.ts");
        let mut broken = valid_run_record();
        broken["assertions"][0]["outcome"] = json!("infrastructure");
        broken["assertions"][1]["outcome"] = json!("infrastructure");
        broken["test"]["id"] = json!("node:broken");
        broken["test"]["path"] =
            json!("src/generated/node-typescript/scenario-tests/planner/broken.test.ts");
        fs::write(
            ingest.join("a-fail.json"),
            serde_json::to_vec_pretty(&failing).expect("serializes"),
        )
        .expect("written");
        fs::write(
            ingest.join("b-infra.json"),
            serde_json::to_vec_pretty(&broken).expect("serializes"),
        )
        .expect("written");
        let fs = Fs::open(temp.path()).expect("validated root");

        let rollup = scenarios_execution_rollup(&fs, &context(), &fixture_ir(), &no_expected());

        assert_eq!(rollup.blocking, 2, "both records block");
        assert_eq!(rollup.assertion, 1, "one record carries failed assertions");
        assert_eq!(rollup.infrastructure, 1, "one record carries boot rows");
        assert_eq!(
            rollup_reason(&rollup),
            Some("scenario.assertion-failed"),
            "the invalid family dominates a mixed run"
        );
        // The envelope is the observable surface: the assertion class
        // carries the invalid-status diagnostic for a mixed run.
        match rollup.failure {
            Some(DomainResult::Invalid { .. }) => {}
            _ => panic!("a mixed run is an invalid-status envelope"),
        }

        let only_assertion = ExecutionRollup {
            blocking: 1,
            assertion: 1,
            ..ExecutionRollup::default()
        };
        assert_eq!(
            rollup_reason(&only_assertion),
            Some("scenario.assertion-failed")
        );
        let only_infra = ExecutionRollup {
            blocking: 1,
            infrastructure: 1,
            ..ExecutionRollup::default()
        };
        assert_eq!(rollup_reason(&only_infra), Some("scenario.infrastructure"));
    }

    /// Issue #56: a declared scenario-test artifact that produced no run
    /// record is missing evidence — the suite is never a successful
    /// partial run — while a record matching its declared path clears it.
    #[test]
    fn execution_rollup_marks_missing_expected_inventory() {
        let temp = TempDir::new().expect("temp dir");
        let ingest = temp.path().join(".lekalo/import/scenario-runs");
        fs::create_dir_all(&ingest).expect("ingest home");
        fs::write(
            ingest.join("run.json"),
            serde_json::to_vec_pretty(&valid_run_record()).expect("record serializes"),
        )
        .expect("record written");
        let fs = Fs::open(temp.path()).expect("validated root");

        let mut expected = BTreeSet::new();
        expected.insert(
            "src/generated/node-typescript/scenario-tests/planner/planner.scenario.focus_happy.test.ts"
                .to_owned(),
        );
        let covered = scenarios_execution_rollup(&fs, &context(), &fixture_ir(), &expected);
        assert_eq!(covered.missing, 0, "the declared test reported");
        assert_eq!(rollup_reason(&covered), None, "no finding");

        expected.insert(
            "src/generated/node-typescript/scenario-tests/planner/dropped.test.ts".to_owned(),
        );
        let rollup = scenarios_execution_rollup(&fs, &context(), &fixture_ir(), &expected);
        assert_eq!(rollup.missing, 1, "the dropped test produced no record");
        assert_eq!(rollup.blocking, 0, "no record failed — evidence is absent");
        assert_eq!(
            rollup_reason(&rollup),
            Some("scenario.infrastructure"),
            "missing inventory is the report-failure class"
        );
        match rollup.failure {
            Some(DomainResult::Unavailable { .. }) => {}
            _ => panic!("a missing record is an unavailable-status envelope"),
        }
    }

    /// Issue #56: bytes that never parse as a run record are malformed
    /// evidence — they block with the integrity reason, not a
    /// conformance verdict.
    #[test]
    fn execution_rollup_marks_unparsable_bytes_invalid() {
        let temp = TempDir::new().expect("temp dir");
        let ingest = temp.path().join(".lekalo/import/scenario-runs");
        fs::create_dir_all(&ingest).expect("ingest home");
        fs::write(ingest.join("garbage.json"), b"not a run record").expect("written");
        let fs = Fs::open(temp.path()).expect("validated root");

        let rollup = scenarios_execution_rollup(&fs, &context(), &fixture_ir(), &no_expected());

        assert!(rollup.present);
        assert_eq!(rollup.invalid, 1, "the malformed document is counted");
        assert_eq!(rollup.blocking, 1);
        assert_eq!(rollup.records, 0, "nothing parsed");
        assert_eq!(rollup_reason(&rollup), Some("scenario.run-record-invalid"));
    }
}
