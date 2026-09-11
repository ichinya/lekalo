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
use crate::artifacts::GenerateService;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::ir::Compilation;
use crate::loader::LoadSelection;
use crate::lockfile::types::Sha256Digest;
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
    components.push(Component::state(
        "scenarios.execution",
        false,
        ComponentState::Unsupported,
        Some("core.capability-unavailable"),
    ));
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
