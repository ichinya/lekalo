//! The `lekalo generate` orchestration pipeline (issue #91).
//!
//! One invocation validates the Model, binds the exact lock and inputs,
//! discovers the invocation-supplied adapter, resolves the target set,
//! plans every write through the `lekalo.target/v1` protocol, and — only
//! for an explicit apply — publishes protocol-verified writes and
//! replaces the ownership manifest atomically. Every target decision of
//! the adapter stays inside the adapter; this layer orchestrates,
//! validates bindings, and aggregates. Target failures stay isolated:
//! the remaining targets complete, and every failure diagnostic is
//! preserved in the aggregate envelope.

use std::path::Path;

use crate::adapter_package::budget::ExpansionPolicy;
use crate::artifacts::check::{inputs_revision, Prepared};
use crate::artifacts::types::{
    AdapterRef, ArtifactEntry, ArtifactKey, ArtifactKind, ArtifactManifest, ArtifactPath,
    Lifecycle, ProjectRef, SemanticOwnerId, SourceMapBinding, SourceRange, MANIFEST_DIR,
    MANIFEST_NAME, MAX_ARTIFACT_BYTES,
};
use crate::artifacts::ArtifactFailure;
use crate::diagnostics::types::token_value;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::digest::sha256_hex;
use crate::ir::Compilation;
use crate::loader::{self, LoadSelection, ModelVersion};
use crate::lockfile::types::Sha256Digest;
use crate::lockfile::{LockRequirement, LockVerifier, Lockfile, ResolvedAdapter, RuntimeInventory};
use crate::project_fs::Fs;
use crate::result::{DomainResult, Status};
use crate::target_protocol::transport::TransportLimits;
use crate::target_protocol::wire::{Operation, WriteAction, WriteEntry};
use crate::target_protocol::{CallRequest, TargetClient, TargetFailure};

use super::catalog::{binding_failure, discover_with_policy, locked_adapter, AdapterSupply};
use super::receipt::{
    AdapterReceipt, GenerateReceipt, InputsReceipt, IrEvidenceReceipt, ScopeReceipt, TargetCounts,
    TargetReceipt, TargetState, Verdict, WriteReceipt, IDENTITY, SCHEMA_VERSION,
};
use super::version::{
    CLIENT_SDK_EVIDENCE_DIR, DEFAULT_TIMEOUT_MS, IR_EVIDENCE_DIR, MAX_TARGETS,
    OPENAPI_EVIDENCE_DIR, TRANSPORT_EVIDENCE_DIR,
};
use super::Failure;

/// The request of one generate invocation.
pub struct GenerateRequest<'a> {
    pub selection: &'a LoadSelection,
    /// The explicit target selection; empty selects every target the
    /// discovered adapter declares.
    pub targets: Vec<String>,
    /// The optional module scope of the attribution.
    pub module: Option<String>,
    pub dry_run: bool,
    pub locked: bool,
    pub supply: Option<AdapterSupply>,
    pub timeout_ms: u64,
    /// Issue #89: when set, the adapter's described scopes may exceed
    /// its manifest ceiling for this run (`--allow-permission-expansion`).
    /// The widening is refused by default and stays visible in the
    /// confinement evidence when permitted.
    pub allow_permission_expansion: bool,
}

/// One target's terminal outcome.
enum TargetOutcome {
    Done(Box<TargetReceipt>),
    Failed(Failure),
}

/// Run the generate pipeline and project the receipt or the aggregate
/// failure envelope.
pub fn generate(request: GenerateRequest<'_>) -> DomainResult {
    match run(request) {
        Ok(receipt) => DomainResult::receipt(
            serde_json::to_string_pretty(&receipt).expect("generate receipt serializes"),
            generate_human(&receipt),
        ),
        Err(result) => result,
    }
}

/// The stable human summary of a generate receipt.
fn generate_human(receipt: &GenerateReceipt) -> String {
    let writes: usize = receipt
        .targets
        .iter()
        .map(|target| target.writes.len())
        .sum();
    format!(
        "generate {} {} : {} targets ({} applied, {} planned, {} failed), {} writes, verdict {}",
        receipt.project,
        receipt.mode,
        receipt.counts.targets,
        receipt.counts.applied,
        receipt.counts.planned,
        receipt.counts.failed,
        writes,
        receipt.verdict.as_str()
    )
}

fn run(request: GenerateRequest<'_>) -> Result<GenerateReceipt, DomainResult> {
    if request.targets.len() > MAX_TARGETS {
        return Err(Failure::NoTargetSelected.into());
    }
    let limits = effective_limits(request.timeout_ms);
    // Pipeline steps 1-3: the project root, the validated inputs, the
    // exact lock, and the parsed manifest.
    let prepared = Prepared::prepare(request.selection)
        .map_err(|failure| DomainResult::from(&Failure::Artifact(failure)))?;
    let Some(supply) = request.supply.as_ref() else {
        return Err(DomainResult::from(&Failure::AdapterSupplyRequired));
    };
    let compilation = compile(request.selection)?;
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())
        .ok_or_else(|| DomainResult::from(&Failure::ProjectRefUnresolved))?;
    let ir_bytes = compilation.project.to_canonical_json();
    let ir_digest = Sha256Digest::from_hex(&sha256_hex(ir_bytes.as_bytes()));
    // The canonical IR evidence: runtime-derived, refreshed on every
    // generate run, and the only bytes an adapter may read as input.
    let evidence_path = format!("{IR_EVIDENCE_DIR}/{project_id}.json");
    write_evidence(prepared.root(), &evidence_path, ir_bytes.as_bytes())?;
    // Issue #59: the PHP operations join is the acceptance authority.
    // When the project declares an operations input, it joins against
    // the compiled IR, the embedded #62 error registry, the staged
    // evidence digests, and the closed recipes HERE — before any
    // adapter exchange — and a refused join is a typed invalid result.
    let operations_input_path = prepared
        .root()
        .join("lekalo")
        .join("operations")
        .join(format!("{project_id}.operations.json"));
    if operations_input_path.is_file() {
        run_operations_join(
            prepared.root(),
            &operations_input_path,
            &compilation.project,
            &project_id,
        )?;
    }
    // Issue #60: the PHP routes join is the acceptance authority for
    // the route wrappers. When the project declares a routes input, it
    // joins against the compiled IR endpoints, the transport-http
    // attachment, and the bound types/operations inputs HERE — before
    // any adapter exchange — and a refused join is a typed invalid
    // result.
    let routes_input_path = prepared
        .root()
        .join("lekalo")
        .join("routes")
        .join(format!("{project_id}.routes.json"));
    if routes_input_path.is_file() {
        run_routes_join(
            prepared.root(),
            &routes_input_path,
            &compilation.project,
            &project_id,
        )?;
    }
    // Transport preflight (#70): when the canonical transport home
    // exists, it validates against the compiled project and its
    // canonical bytes land under the `lekalo.cache` evidence home —
    // the only transport input an adapter may read, covered by its
    // declared read scopes. An invalid home refuses the run before
    // any adapter is discovered.
    match crate::transport_http::read_document(prepared.root()) {
        Err(diagnostics) => return Err(DomainResult::invalid(diagnostics)),
        Ok(Some(attachment)) => {
            let context = crate::transport_http::ValidationContext::new(&compilation.project);
            crate::transport_http::validate(&attachment, &context)
                .map_err(DomainResult::invalid)?;
            let transport_path = format!("{TRANSPORT_EVIDENCE_DIR}/{project_id}.json");
            let transport_bytes = attachment
                .canonical_bytes()
                .map_err(DomainResult::invalid)?;
            write_evidence(prepared.root(), &transport_path, transport_bytes.as_bytes())?;
            // OpenAPI evidence (#46): the canonical projection of the
            // validated transport home lands beside the other evidence
            // homes — the only OpenAPI input an adapter may read. The
            // embedded #62 registry is bound so the identity variants
            // render; the declared defaults (3.1, full) apply.
            let registry =
                crate::error_contract::ErrorRegistry::embedded().map_err(DomainResult::invalid)?;
            let context = crate::transport_http::ValidationContext::new(&compilation.project)
                .with_errors(registry);
            let rendered =
                crate::openapi::render(&attachment, &context, &crate::openapi::RenderConfig::new())
                    .map_err(DomainResult::invalid)?;
            let openapi_path = format!("{OPENAPI_EVIDENCE_DIR}/{project_id}.json");
            write_evidence(
                prepared.root(),
                &openapi_path,
                rendered.canonical_bytes().as_bytes(),
            )?;
            // Client-SDK evidence (#72): the typed client contract the
            // language backends render from, derived from the same
            // validated join (transport + IR + the embedded #62
            // registry + the bound #64 query-model home). The SDK
            // projection requires the full context: a project without
            // the query-model home skips the derivation honestly — the
            // transport home stays the gate — but a PRESENT home that
            // fails to read or validate propagates its registered
            // refusal, never a silent skip.
            let capabilities = crate::transport_http::CapabilityMap::http_json();
            match crate::client_sdk::source::read_query_model(prepared.root()) {
                Err(diagnostics) => return Err(DomainResult::invalid(diagnostics)),
                Ok(None) => {}
                Ok(Some(query_model)) => {
                    let context =
                        crate::transport_http::ValidationContext::new(&compilation.project)
                            .with_errors(registry)
                            .with_query_model(&query_model)
                            .with_capabilities(&capabilities);
                    let sdk = crate::client_sdk::project(
                        &attachment,
                        &context,
                        &crate::client_sdk::ClientConfig::generated(),
                    )
                    .map_err(DomainResult::invalid)?;
                    let sdk_path = format!("{CLIENT_SDK_EVIDENCE_DIR}/{project_id}.json");
                    write_evidence(
                        prepared.root(),
                        &sdk_path,
                        sdk.canonical_bytes()
                            .map_err(DomainResult::invalid)?
                            .as_bytes(),
                    )?;
                }
            }
        }
        Ok(None) => {}
    }

    let mut client = TargetClient::new(limits);
    let policy = if request.allow_permission_expansion {
        ExpansionPolicy::AllowEscalated
    } else {
        ExpansionPolicy::Refuse
    };
    let discovered = discover_with_policy(&mut client, supply, prepared.root(), limits, policy)?;
    let Some(locked) = locked_adapter(prepared.lock(), &discovered) else {
        return Err(DomainResult::from(&Failure::AdapterNotLocked {
            adapter: discovered.adapter.id.clone(),
        }));
    };
    let executable = binding_failure(locked, &discovered)?;
    if request.locked {
        locked_preflight_with(&prepared, &locked_inventory(locked, &executable))?;
    }
    let mut selected = if request.targets.is_empty() {
        discovered.targets.clone()
    } else {
        request.targets.clone()
    };
    selected.sort();
    selected.dedup();
    if selected.is_empty() {
        return Err(DomainResult::from(&Failure::NoTargetSelected));
    }
    let scope = scope_of(&compilation, request.module.as_deref())?;
    let scope_refs = scope_inputs(&compilation, &scope);
    let revision = inputs_revision(prepared.inputs().model(), prepared.inputs().ir());
    let inputs = InputsReceipt {
        model_version: prepared.inputs().model().version().as_str().to_owned(),
        model_digest: prepared.inputs().model().digest().as_str().to_owned(),
        ir_version: prepared.inputs().ir().version().as_str().to_owned(),
        ir_digest: prepared.inputs().ir().digest().as_str().to_owned(),
        revision: revision.as_str().to_owned(),
    };

    // Pipeline steps 4-9 per target, isolated.
    let mut targets: Vec<TargetReceipt> = Vec::new();
    let mut failures: Vec<DomainResult> = Vec::new();
    for target in &selected {
        match run_target(
            &prepared,
            &compilation,
            &mut client,
            supply,
            &discovered.adapter.id,
            target,
            &discovered.selected_profile(None).unwrap_or_default(),
            &evidence_path,
            &scope_refs,
            request.dry_run,
            locked,
        ) {
            TargetOutcome::Done(receipt) => targets.push(*receipt),
            TargetOutcome::Failed(failure) => failures.push(DomainResult::from(&failure)),
        }
    }
    targets.sort_by(|left, right| left.target.cmp(&right.target));
    let counts = TargetCounts {
        targets: targets.len(),
        planned: targets
            .iter()
            .filter(|target| target.state == TargetState::Planned)
            .count(),
        applied: targets
            .iter()
            .filter(|target| target.state == TargetState::Applied)
            .count(),
        failed: failures.len(),
    };
    if !failures.is_empty() {
        // Isolated and preserved: every failed target keeps its own
        // registered diagnostic in the aggregate envelope.
        return Err(aggregate_envelopes(failures));
    }
    let propagated = crate::privacy::export::propagated_class(prepared.root());
    let receipt = GenerateReceipt {
        schema_version: SCHEMA_VERSION,
        operation: "generate",
        mode: if request.dry_run { "dry-run" } else { "apply" },
        identity: IDENTITY,
        project: project_id,
        lock_digest: prepared.lock().digest().as_str().to_owned(),
        locked: request.locked,
        inputs,
        ir_evidence: IrEvidenceReceipt {
            path: evidence_path,
            digest: ir_digest.as_str().to_owned(),
        },
        scope,
        targets,
        counts,
        verdict: Verdict::Ready,
        class: propagated.as_ref().map(|(labels, _)| labels.clone()),
        policy_ref: propagated.map(|(_, policy)| policy),
    };
    Ok(receipt)
}

/// The effective transport limits of one run.
pub(crate) fn effective_limits(timeout_ms: u64) -> TransportLimits {
    TransportLimits {
        timeout_ms: if timeout_ms == 0 {
            DEFAULT_TIMEOUT_MS
        } else {
            timeout_ms
        },
        ..Default::default()
    }
}

/// The explicit `--locked` preflight: every locked component and
/// platform artifact must exist in the local inventory before any
/// runner or output.
pub(crate) fn locked_preflight(prepared: &Prepared) -> Result<(), DomainResult> {
    locked_preflight_with(prepared, &RuntimeInventory::empty())
}

/// The `--locked` preflight with an explicit local inventory.
pub(crate) fn locked_preflight_with(
    prepared: &Prepared,
    inventory: &RuntimeInventory,
) -> Result<(), DomainResult> {
    let registry = crate::lockfile::plan::embedded_registry()?;
    let base = crate::lockfile::plan::LockService::request_at(prepared.root())?;
    let request =
        crate::lockfile::plan::LockService::merge_locked_identities(&base, prepared.lock());
    if let crate::lockfile::LockVerdict::Refused(failure) = LockVerifier::verify(
        prepared.lock(),
        &request,
        inventory,
        registry,
        LockRequirement::Required,
    ) {
        return Err(DomainResult::from(&Failure::Lock(failure)));
    }
    Ok(())
}

/// The `--locked` inventory of one bound supply: the launched adapter
/// entry bytes are the locally available component and platform
/// artifact, proven equal to the locked pins by the binding check.
pub(crate) fn locked_inventory(
    locked: &ResolvedAdapter,
    executable: &Sha256Digest,
) -> RuntimeInventory {
    RuntimeInventory::empty()
        .with_adapter(crate::lockfile::verify::InventoryComponent::new(
            locked.id().clone(),
            locked.version().clone(),
            locked.digest().clone(),
        ))
        .with_artifact(crate::lockfile::verify::InventoryArtifact::new(
            locked.id().clone(),
            crate::lockfile::types::Platform::parse("any").expect("any is a legal platform"),
            executable.clone(),
        ))
}

/// The deterministic aggregate of isolated failures: the worst accepted
/// exit class wins, and every diagnostic is preserved in the merged
/// envelope. Shared with the verify pipeline.
pub(crate) fn aggregate_envelopes(results: Vec<DomainResult>) -> DomainResult {
    for status in [
        Status::UnsupportedVersion,
        Status::Invalid,
        Status::Denied,
        Status::Unsupported,
        Status::Unavailable,
    ] {
        let matched: Vec<&DomainResult> = results
            .iter()
            .filter(|result| result.status() == status)
            .collect();
        if !matched.is_empty() {
            let mut diagnostics = Vec::new();
            for result in &matched {
                diagnostics.extend(result.diagnostics().iter().cloned());
            }
            let set = wire_set(status, diagnostics);
            return match status {
                Status::UnsupportedVersion => DomainResult::UnsupportedVersion { diagnostics: set },
                Status::Invalid => DomainResult::Invalid { diagnostics: set },
                Status::Denied => DomainResult::Denied { diagnostics: set },
                Status::Unsupported => DomainResult::UnsupportedOperation { diagnostics: set },
                _ => DomainResult::Unavailable { diagnostics: set },
            };
        }
    }
    let mut diagnostics = Vec::new();
    for result in &results {
        diagnostics.extend(result.diagnostics().iter().cloned());
    }
    DomainResult::Invalid {
        diagnostics: wire_set(Status::Invalid, diagnostics),
    }
}

/// Assemble one validated wire set; a rejected set is a developer fault
/// collapsed to the registry invariant rule.
pub(crate) fn wire_set(
    status: Status,
    diagnostics: Vec<Diagnostic>,
) -> crate::diagnostics::DiagnosticSet {
    crate::diagnostics::DiagnosticSet::try_from_unsorted(diagnostics, status)
        .unwrap_or_else(|_| crate::diagnostics::DiagnosticSet::empty())
}

/// The stable wire token of one write action.
fn action_str(action: WriteAction) -> String {
    match action {
        WriteAction::Create => "create".to_owned(),
        WriteAction::Replace => "replace".to_owned(),
        WriteAction::Delete => "delete".to_owned(),
    }
}

/// One isolated target: plan, and — only on an explicit apply — publish
/// and record.
#[allow(clippy::too_many_arguments)]
fn run_target(
    prepared: &Prepared,
    compilation: &Compilation,
    client: &mut TargetClient,
    supply: &AdapterSupply,
    adapter_id: &str,
    target: &str,
    profile: &str,
    evidence_path: &str,
    scope_refs: &[String],
    dry_run: bool,
    locked: &ResolvedAdapter,
) -> TargetOutcome {
    let Some(outcome) = client.describe_outcome() else {
        return TargetOutcome::Failed(Failure::Target(TargetFailure::HandshakeRequired {
            operation: Operation::Generate,
        }));
    };
    let capabilities = outcome.capabilities.clone();
    if !capabilities
        .targets
        .iter()
        .any(|declared| declared == target)
    {
        return TargetOutcome::Failed(Failure::TargetNotDeclared {
            target: target.to_owned(),
            adapter: adapter_id.to_owned(),
        });
    }
    if !crate::target_protocol::plan::covered_by(evidence_path, &capabilities.read_scopes) {
        return TargetOutcome::Failed(Failure::Target(TargetFailure::CapabilityUnsupported {
            detail: "ir-read-scope",
        }));
    }
    let profile_token = (!profile.is_empty()).then(|| profile.to_owned());
    let planned = client.call(
        &supply.command,
        CallRequest {
            operation: Operation::Generate,
            target: Some(target),
            profile: profile_token.as_deref(),
            profile_resolution: None,
            ir_path: Some(evidence_path),
            dry_run: Some(true),
            plan_id: None,
            native_request: None,
        },
        prepared.root(),
        prepared.fs(),
        None,
    );
    let planned = match planned {
        Ok(outcome) => outcome,
        Err(failure) => return TargetOutcome::Failed(Failure::Target(failure)),
    };
    let writes = planned.response.writes.clone().unwrap_or_default();
    let Some(plan_id) = planned.plan_id.clone() else {
        return TargetOutcome::Failed(Failure::Target(TargetFailure::plan_mismatch(
            None,
            "missing-plan-id",
        )));
    };
    if dry_run {
        return TargetOutcome::Done(Box::new(TargetReceipt {
            target: target.to_owned(),
            profile: profile_token,
            adapter: adapter_receipt(locked),
            state: TargetState::Planned,
            reason_code: None,
            plan_id: Some(plan_id),
            writes: write_rows(&writes),
            manifest_digest: None,
        }));
    }
    // Pipeline step 6: ownership and policy validation before any write.
    if let Err(failure) = validate_plan_ownership(prepared, &writes) {
        return TargetOutcome::Failed(failure);
    }
    // Pipeline step 7: the explicit apply consumes the bound plan.
    let applied = client.call(
        &supply.command,
        CallRequest {
            operation: Operation::Generate,
            target: Some(target),
            profile: profile_token.as_deref(),
            profile_resolution: None,
            ir_path: Some(evidence_path),
            dry_run: Some(false),
            plan_id: Some(&plan_id),
            native_request: None,
        },
        prepared.root(),
        prepared.fs(),
        None,
    );
    let applied = match applied {
        Ok(outcome) => outcome,
        Err(failure) => return TargetOutcome::Failed(Failure::Target(failure)),
    };
    let writes = applied.response.writes.clone().unwrap_or_default();
    // Pipeline step 8: verify the actual bytes against the plan.
    if let Err(failure) = verify_applied(prepared, &writes) {
        return TargetOutcome::Failed(failure);
    }
    // Pipeline step 9: replace the ownership manifest atomically.
    let manifest = match update_manifest(prepared, compilation, &writes, locked, scope_refs) {
        Ok(digest) => digest,
        Err(failure) => return TargetOutcome::Failed(Failure::Artifact(failure)),
    };
    TargetOutcome::Done(Box::new(TargetReceipt {
        target: target.to_owned(),
        profile: profile_token,
        adapter: adapter_receipt(locked),
        state: TargetState::Applied,
        reason_code: None,
        plan_id: Some(plan_id),
        writes: write_rows(&writes),
        manifest_digest: Some(manifest.as_str().to_owned()),
    }))
}

fn adapter_receipt(locked: &ResolvedAdapter) -> AdapterReceipt {
    AdapterReceipt {
        id: locked.id().as_str().to_owned(),
        version: locked.version().to_string(),
        digest: locked.digest().as_str().to_owned(),
    }
}

fn write_rows(writes: &[WriteEntry]) -> Vec<WriteReceipt> {
    let mut rows: Vec<WriteReceipt> = writes
        .iter()
        .map(|entry| WriteReceipt {
            path: entry.path.clone(),
            action: action_str(entry.action),
            digest: entry.sha256.clone(),
        })
        .collect();
    rows.sort_by(|left, right| {
        (&left.path, left.action.as_str()).cmp(&(&right.path, right.action.as_str()))
    });
    rows
}

/// Compile the accepted selection into typed IR.
pub(crate) fn compile(selection: &LoadSelection) -> Result<Compilation, DomainResult> {
    let model = loader::normalize_model(selection)?;
    crate::ir::compile(&model).map_err(|failure| failure.into_result())
}

/// The unknown-scope refusal carries the accepted validation rule.
pub(crate) fn unknown_module(module: &str) -> DomainResult {
    let mut data = DataObject::new();
    data.insert("module".to_owned(), token_value(module));
    let diagnostic =
        crate::diagnostics::normalize::build("validate.module-unresolved", None, None, data)
            .expect("validate.module-unresolved is registered and active");
    DomainResult::Invalid {
        diagnostics: wire_set(Status::Invalid, vec![diagnostic]),
    }
}

/// The operations join (issue #59): parse the authored operations input
/// and join it against the compiled IR, the embedded #62 error
/// registry, the staged evidence digests, and the closed recipes. Any
/// finding refuses the run as the registered
/// `php-operations.join-invalid` invalid result — before any adapter
/// exchange.
pub(crate) fn run_operations_join(
    root: &Path,
    input_path: &Path,
    compilation: &crate::ir::CompiledProject,
    project_id: &str,
) -> Result<(), DomainResult> {
    let join_findings = |findings: &[crate::php_operations::Finding]| -> DomainResult {
        let codes: Vec<String> = findings
            .iter()
            .take(16)
            .map(|finding| finding.code.clone())
            .collect();
        let first = findings.first();
        let detail = first
            .map(|finding| {
                let pointer = finding
                    .pointer
                    .as_deref()
                    .map(|pointer| format!(" at {pointer}"))
                    .unwrap_or_default();
                format!(
                    "{}{}: {}",
                    finding.semantic_id.as_deref().unwrap_or("operations"),
                    pointer,
                    finding.detail
                )
            })
            .unwrap_or_else(|| "the operations input join refused".to_owned());
        let mut data = DataObject::new();
        data.insert("projectId".to_owned(), token_value(project_id));
        data.insert(
            "codes".to_owned(),
            crate::diagnostics::DataValue::List(
                codes
                    .iter()
                    .map(|code| {
                        crate::diagnostics::Scalar::Token(crate::diagnostics::types::bound_token(
                            code,
                        ))
                    })
                    .collect(),
            ),
        );
        data.insert("detail".to_owned(), token_value(&detail));
        let diagnostic =
            crate::diagnostics::normalize::build("php-operations.join-invalid", None, None, data)
                .expect("php-operations.join-invalid is registered and active");
        DomainResult::Invalid {
            diagnostics: wire_set(Status::Invalid, vec![diagnostic]),
        }
    };
    let bytes = std::fs::read(input_path).map_err(|_| {
        join_findings(&[crate::php_operations::Finding {
            code: "operations.input-unreadable".to_owned(),
            semantic_id: None,
            pointer: None,
            detail: "the operations input document is unreadable".to_owned(),
        }])
    })?;
    let input =
        crate::php_operations::parse_input(&bytes).map_err(|findings| join_findings(&findings))?;
    let findings = crate::php_operations::check_join(root, &input, compilation);
    if findings.is_empty() {
        return Ok(());
    }
    Err(join_findings(&findings))
}

/// The routes join (issue #60): parse the authored routes input and
/// join it against the compiled IR endpoints, the transport-http
/// attachment, and the staged evidence digests. Any finding refuses the
/// run as the registered `php-routes.join-invalid` invalid result —
/// before any adapter exchange.
pub(crate) fn run_routes_join(
    root: &Path,
    input_path: &Path,
    compilation: &crate::ir::CompiledProject,
    project_id: &str,
) -> Result<(), DomainResult> {
    let join_findings = |findings: &[crate::php_routes::Finding]| -> DomainResult {
        let codes: Vec<String> = findings
            .iter()
            .take(16)
            .map(|finding| finding.code.clone())
            .collect();
        let first = findings.first();
        let detail = first
            .map(|finding| {
                let pointer = finding
                    .pointer
                    .as_deref()
                    .map(|pointer| format!(" at {pointer}"))
                    .unwrap_or_default();
                format!(
                    "{}{}: {}",
                    finding.semantic_id.as_deref().unwrap_or("routes"),
                    pointer,
                    finding.detail
                )
            })
            .unwrap_or_else(|| "the routes input join refused".to_owned());
        let mut data = DataObject::new();
        data.insert("projectId".to_owned(), token_value(project_id));
        data.insert(
            "codes".to_owned(),
            crate::diagnostics::DataValue::List(
                codes
                    .iter()
                    .map(|code| {
                        crate::diagnostics::Scalar::Token(crate::diagnostics::types::bound_token(
                            code,
                        ))
                    })
                    .collect(),
            ),
        );
        data.insert("detail".to_owned(), token_value(&detail));
        let diagnostic =
            crate::diagnostics::normalize::build("php-routes.join-invalid", None, None, data)
                .expect("php-routes.join-invalid is registered and active");
        DomainResult::Invalid {
            diagnostics: wire_set(Status::Invalid, vec![diagnostic]),
        }
    };
    let bytes = std::fs::read(input_path).map_err(|_| {
        join_findings(&[crate::php_routes::Finding {
            code: "routes.input-unreadable".to_owned(),
            semantic_id: None,
            pointer: None,
            detail: "the routes input document is unreadable".to_owned(),
        }])
    })?;
    let input =
        crate::php_routes::parse_input(&bytes).map_err(|findings| join_findings(&findings))?;
    let findings = crate::php_routes::check_join(root, &input, compilation);
    if findings.is_empty() {
        return Ok(());
    }
    Err(join_findings(&findings))
}

/// The attribution scope: every module, or one known module.
fn scope_of(compilation: &Compilation, module: Option<&str>) -> Result<ScopeReceipt, DomainResult> {
    if let Some(module) = module {
        let known = compilation
            .project
            .modules
            .iter()
            .any(|candidate| candidate.id.as_str() == module);
        if !known {
            return Err(unknown_module(module));
        }
        return Ok(ScopeReceipt {
            modules: vec![module.to_owned()],
            all_modules: false,
        });
    }
    Ok(ScopeReceipt {
        modules: Vec::new(),
        all_modules: true,
    })
}

/// The sorted definition ids of the attribution scope.
fn scope_inputs(compilation: &Compilation, scope: &ScopeReceipt) -> Vec<String> {
    let mut ids: Vec<String> = compilation
        .project
        .definitions
        .iter()
        .filter(|definition| {
            scope.all_modules
                || scope
                    .modules
                    .iter()
                    .any(|module| definition.id().as_str().starts_with(module.as_str()))
        })
        .map(|definition| definition.id().as_str().to_owned())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Write the canonical IR evidence under the reserved runtime home:
/// same-directory staging, an atomic rename, and a read-back check.
fn write_evidence(root: &Path, logical: &str, bytes: &[u8]) -> Result<(), DomainResult> {
    let physical = root.join(logical);
    if let Ok(existing) = std::fs::read(&physical) {
        if existing == bytes {
            return Ok(());
        }
    }
    if let Some(parent) = physical.parent() {
        std::fs::create_dir_all(parent).map_err(|_| evidence_io())?;
    }
    let stage = physical.with_extension("json.tmp");
    std::fs::write(&stage, bytes).map_err(|_| evidence_io())?;
    std::fs::rename(&stage, &physical).map_err(|_| evidence_io())?;
    let read_back = std::fs::read(&physical).map_err(|_| evidence_io())?;
    if read_back != bytes {
        return Err(evidence_io());
    }
    Ok(())
}

fn evidence_io() -> DomainResult {
    DomainResult::from(&Failure::Artifact(ArtifactFailure::Io("ir-evidence")))
}

/// Pipeline step 6: a plan may create or replace only artifacts it may
/// own; a write over a never-overwritten lifecycle is an ownership
/// policy denial, and a plan touching the manifest bookkeeping itself is
/// refused outright.
fn validate_plan_ownership(prepared: &Prepared, writes: &[WriteEntry]) -> Result<(), Failure> {
    for entry in writes {
        if entry.path.starts_with(MANIFEST_DIR) {
            return Err(Failure::Target(TargetFailure::ProtectedPath {
                path: entry.path.clone(),
                home: "ownership-manifest",
            }));
        }
        // Issue #56: the lifecycle denial covers every action — a delete
        // plan over a recorded scaffolded or checked artifact is the
        // cleanup-deletes-user-files hazard, refused exactly like a
        // create or replace over it.
        if let Some(manifest) = prepared.manifest() {
            for recorded in manifest.artifacts() {
                if recorded.key().path().as_str() == entry.path
                    && recorded.lifecycle() != Lifecycle::Generated
                {
                    return Err(Failure::Target(TargetFailure::ProtectedPath {
                        path: entry.path.clone(),
                        home: "lifecycle",
                    }));
                }
            }
        }
        if entry.action == WriteAction::Delete {
            continue;
        }
    }
    Ok(())
}

/// Pipeline step 8: the published bytes must match the plan exactly.
fn verify_applied(prepared: &Prepared, writes: &[WriteEntry]) -> Result<(), Failure> {
    for entry in writes {
        match entry.action {
            WriteAction::Delete => {
                if crate::artifacts::check::observe(prepared.fs(), &entry.path)
                    .map_err(Failure::Artifact)?
                    .is_some()
                {
                    return Err(Failure::Target(TargetFailure::plan_mismatch(
                        Some(entry.path.clone()),
                        "delete-not-applied",
                    )));
                }
            }
            WriteAction::Create | WriteAction::Replace => {
                let observed = crate::artifacts::check::observe(prepared.fs(), &entry.path)
                    .map_err(Failure::Artifact)?;
                let expected = entry
                    .sha256
                    .as_deref()
                    .map(Sha256Digest::parse)
                    .transpose()
                    .map_err(|_| Failure::Lock(crate::lockfile::LockFailure::SchemaInvalid))?;
                match (observed, expected) {
                    (Some(observed), Some(expected)) if observed.digest == expected => {}
                    _ => {
                        return Err(Failure::Target(TargetFailure::plan_mismatch(
                            Some(entry.path.clone()),
                            "bytes",
                        )))
                    }
                }
            }
        }
    }
    Ok(())
}

/// Pipeline step 9: build the next ownership manifest from the current
/// entries plus this run's generated artifacts and replace the document
/// atomically. A stale manifest may be replaced only by a full-scope run
/// whose plan covers every recorded generated entry — a scoped run
/// refuses instead of dropping ownership it did not regenerate.
fn update_manifest(
    prepared: &Prepared,
    compilation: &Compilation,
    writes: &[WriteEntry],
    locked: &ResolvedAdapter,
    scope_refs: &[String],
) -> Result<Sha256Digest, ArtifactFailure> {
    let lock: &Lockfile = prepared.lock();
    let model_version: ModelVersion = compilation.project.model_version;
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())
        .ok_or(ArtifactFailure::ReferenceInvalid)?;
    let project =
        ProjectRef::parse(&project_id, model_version).ok_or(ArtifactFailure::ReferenceInvalid)?;
    let protocol = lock.target_protocol().map(|pin| pin.version().clone());
    let adapter = AdapterRef::new(
        locked.id().clone(),
        locked.version().clone(),
        locked.digest().clone(),
        locked.artifacts().to_vec(),
        protocol,
    )
    .ok_or(ArtifactFailure::ReferenceInvalid)?;
    let owner = SemanticOwnerId::parse(&format!("{project_id}.generated"), model_version)
        .ok_or(ArtifactFailure::ReferenceInvalid)?;
    let input_refs: Vec<SemanticOwnerId> = scope_refs
        .iter()
        .map(|reference| SemanticOwnerId::parse(reference, model_version))
        .collect::<Option<Vec<_>>>()
        .ok_or(ArtifactFailure::ReferenceInvalid)?;
    let stale = prepared.manifest().is_some_and(|manifest| {
        manifest.lock_ref().digest().as_str() != lock.digest().as_str()
            || manifest.model() != prepared.inputs().model()
            || manifest.ir() != prepared.inputs().ir()
    });
    let mut keep: Vec<ArtifactEntry> = Vec::new();
    if let Some(manifest) = prepared.manifest() {
        for recorded in manifest.artifacts() {
            let path = recorded.key().path().as_str();
            if writes.iter().any(|entry| entry.path == path) {
                continue;
            }
            // A scoped run over a stale manifest would silently drop
            // ownership it did not regenerate: refuse instead. The
            // refusal applies to generated entries only — a scaffolded
            // or checked artifact can never re-enter a write plan by
            // construction (it is user-owned after its one emission),
            // so it is kept, never counted as droppable ownership.
            if stale && recorded.lifecycle() == Lifecycle::Generated {
                return Err(ArtifactFailure::StaleManifest);
            }
            if recorded.lifecycle() != Lifecycle::Generated {
                keep.push(recorded.clone());
            }
        }
    }
    for entry in writes {
        if entry.action == WriteAction::Delete {
            continue;
        }
        let digest = entry
            .sha256
            .as_deref()
            .map(Sha256Digest::parse)
            .transpose()
            .map_err(|_| ArtifactFailure::ReferenceInvalid)?
            .ok_or(ArtifactFailure::ReferenceInvalid)?;
        let key_path = ArtifactPath::parse(&entry.path).ok_or(ArtifactFailure::ReferenceInvalid)?;
        // Issue #45 attribution: the written path determines the closed
        // kind by convention — generated Zod schema modules are `schema`,
        // their `.map.json` sidecars are `data`, everything else stays
        // `source`. No wire change: the convention is core-side only.
        let key = ArtifactKey::new(owner.clone(), key_path, artifact_kind_for(&entry.path));
        keep.retain(|recorded| recorded.key() != &key);
        keep.push(ArtifactEntry::new(
            key,
            lifecycle_for(&entry.path),
            Some(adapter.clone()),
            None,
            digest,
            input_refs.clone(),
        ));
    }
    keep.sort_by(|left, right| left.key().cmp(right.key()));
    // Source maps: ingest the emitted `.map.json` sidecars of this run's
    // writes into the manifest's `source_maps` bindings (issue #45). Each
    // sidecar declaration range becomes one half-open byte range bound to
    // the exact generation inputs; a malformed sidecar fails the whole
    // apply — deterministic, never a half-recorded map.
    let mut source_maps = Vec::new();
    for entry in writes {
        if entry.action == WriteAction::Delete || !entry.path.ends_with(".map.json") {
            continue;
        }
        let binding = source_map_binding_for(
            prepared,
            &entry.path,
            &owner,
            model_version,
            inputs_revision(prepared.inputs().model(), prepared.inputs().ir()),
        )?;
        if let Some(binding) = binding {
            source_maps.push(binding);
        }
    }
    let build = |digest: Sha256Digest| {
        ArtifactManifest::new(
            project.clone(),
            crate::artifacts::types::LockRevision::new(lock.digest().clone()),
            prepared.inputs().model().clone(),
            prepared.inputs().ir().clone(),
            keep.clone(),
            source_maps.clone(),
            digest,
        )
    };
    // The self-digest covers the canonical payload without the
    // manifest_digest property: build with a placeholder, compute over
    // the non-self-referential byte domain, rebuild, and parse the
    // document back so every closed-format invariant is proven.
    let draft = build(Sha256Digest::from_hex(&sha256_hex(&[0u8])));
    let computed = Sha256Digest::from_hex(&sha256_hex(
        &crate::artifacts::canonical::digest_input_bytes(&draft),
    ));
    let manifest = build(computed);
    let bytes = manifest.canonical_bytes();
    let parsed = ArtifactManifest::parse_canonical(&bytes)?;
    write_manifest_atomic(prepared.root(), &bytes)?;
    Ok(parsed.manifest_digest().clone())
}

/// The closed artifact kind of one generated write, by path convention
/// (issue #45): `.map.json` sidecars are `data`, `.ts` modules under a
/// `zod/` segment are `schema`, everything else stays `source`.
fn artifact_kind_for(path: &str) -> ArtifactKind {
    if path.ends_with(".map.json") {
        return ArtifactKind::Data;
    }
    if path.ends_with(".ts") && path.split('/').any(|segment| segment == "zod") {
        return ArtifactKind::Schema;
    }
    // Issue #47: the generated scenario-test compiler owns the
    // scenario-tests home — test files and the shared testkit are `test`
    // artifacts (review F-6: the emitted spellings are `<id>.test.ts`
    // and `testkit.ts`); the port shim and reporter stay support
    // `source`. Issue #56: the Laratesto backend emits the same roles
    // under PHP spellings — `<id>.test.php` and `scenario-test-kit.php`.
    if path.split('/').any(|segment| segment == "scenario-tests")
        && (path.ends_with(".test.ts")
            || path.ends_with(".test.php")
            || path.ends_with("/testkit.ts")
            || path.ends_with("/scenario-test-kit.php"))
    {
        return ArtifactKind::Test;
    }
    ArtifactKind::Source
}

/// The ownership lifecycle of one generated write, by path convention
/// (issue #56, plan S3; issue #58 for the type scaffold home): the
/// `tests/lekalo/` scenario scaffold home and the `app/lekalo-types/`
/// type scaffold home are the user-owned conventions — anything emitted
/// there is scaffolded once and never overwritten, so it records
/// `scaffolded`. Their `.map.json` sidecars stay `generated`: the
/// sidecars are the managed markers the scaffold-once rules key on, and
/// their bytes must stay exactly what the emitters produced. The type
/// scaffold root is deliberately a closed constant shared with the
/// adapter policy (`app/lekalo-types`), so a policy cannot silently
/// move a scaffold under a root the core would misclassify.
fn lifecycle_for(path: &str) -> Lifecycle {
    if path.starts_with("tests/lekalo/") && !path.ends_with(".map.json") {
        return Lifecycle::Scaffolded;
    }
    if path.starts_with("app/lekalo-types/") && !path.ends_with(".map.json") {
        return Lifecycle::Scaffolded;
    }
    // Issue #59: the operations scaffold home is the second user-owned
    // convention. The closed constant is shared with the adapter policy,
    // and the sidecar stays generated (the managed marker the
    // scaffold-once rules key on).
    if path.starts_with("app/lekalo-operations/") && !path.ends_with(".map.json") {
        return Lifecycle::Scaffolded;
    }
    Lifecycle::Generated
}

/// One ingested source-map binding from an emitted `.map.json` sidecar,
/// or `None` when the sidecar is absent from the staged view (a `create`
/// plan's map may legitimately not exist yet at planning time — the
/// digest binding stays with the write receipt).
fn source_map_binding_for(
    prepared: &Prepared,
    sidecar_path: &str,
    owner: &SemanticOwnerId,
    model_version: ModelVersion,
    input_revision: Sha256Digest,
) -> Result<Option<SourceMapBinding>, ArtifactFailure> {
    let (dir, name) = match sidecar_path.rfind('/') {
        Some(at) => (&sidecar_path[..at], &sidecar_path[at + 1..]),
        None => (".", sidecar_path),
    };
    let bytes = match prepared.fs().read_file_opt(dir, name, MAX_ARTIFACT_BYTES) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Ok(None),
        Err(_) => return Err(ArtifactFailure::Io("artifact-read")),
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| ArtifactFailure::SourceMapInvalid)?;
    // Identity first: the `.map.json` suffix is a sidecar convention, not
    // every sidecar's contract (issue #58). A document that does not
    // carry a `declarations` member at all is not a source map — the PHP
    // types mapping sidecar rides the same suffix under the generated
    // and scaffold homes — so it binds nothing and never fails the
    // apply. A document that DOES claim declarations — as any JSON value,
    // array or not — stays under source-map validation: a corrupt map
    // (e.g. `{"declarations":"corrupt"}`) is a hard failure, never a
    // silent skip, exactly like the pre-#58 behavior for every
    // unparseable sidecar.
    let declarations = match value.get("declarations") {
        None => return Ok(None),
        Some(serde_json::Value::Array(declarations)) => declarations,
        Some(_) => return Err(ArtifactFailure::SourceMapInvalid),
    };
    if declarations.len() > 4096 {
        return Err(ArtifactFailure::SourceMapInvalid);
    }
    let mut entries = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        let semantic_id = declaration
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or(ArtifactFailure::SourceMapInvalid)?;
        let start = declaration
            .get("start")
            .and_then(serde_json::Value::as_u64)
            .filter(|start| *start <= u32::MAX as u64)
            .ok_or(ArtifactFailure::SourceMapInvalid)? as u32;
        let end = declaration
            .get("end")
            .and_then(serde_json::Value::as_u64)
            .filter(|end| *end <= u32::MAX as u64)
            .ok_or(ArtifactFailure::SourceMapInvalid)? as u32;
        entries.push(SourceRange::new(
            SemanticOwnerId::parse(semantic_id, model_version)
                .ok_or(ArtifactFailure::ReferenceInvalid)?,
            start,
            end,
        ));
    }
    let module_path = module_path_of(prepared.fs(), sidecar_path);
    let key = ArtifactKey::new(
        owner.clone(),
        // The binding targets the generated module the ranges index (the
        // `.ts` sibling of the sidecar), never the sidecar itself — the
        // ranges are declaration offsets inside the module's bytes.
        ArtifactPath::parse(&module_path).ok_or(ArtifactFailure::ReferenceInvalid)?,
        artifact_kind_for(&module_path),
    );
    Ok(Some(SourceMapBinding::new(key, input_revision, entries)))
}

/// The generated module path of one `.map.json` sidecar path. The
/// TypeScript default maps `X.map.json` → `X.ts`; alternate-language
/// emitters ship a different sibling beside the sidecar (issue #56: the
/// Laratesto backend pairs `X.test.php` with `X.test.map.json`), so a
/// sibling staged on disk wins — the `.ts` spelling stays the
/// absent-file default of create plans whose modules are not written
/// yet. Alternate spellings are probed by bounded extension, never by
/// directory listing, so the mapping stays deterministic.
fn module_path_of(fs: &Fs, sidecar_path: &str) -> String {
    /// Alternate-language module spellings probed before the TypeScript
    /// default: each emitted by a shipped backend emitter.
    const ALTERNATE_MODULE_EXTENSIONS: &[&str] = &["php"];
    let (dir, name) = match sidecar_path.rfind('/') {
        Some(at) => (&sidecar_path[..at], &sidecar_path[at + 1..]),
        None => (".", sidecar_path),
    };
    let base = name.strip_suffix(".map.json").unwrap_or(name);
    for extension in ALTERNATE_MODULE_EXTENSIONS {
        let candidate = format!("{base}.{extension}");
        if matches!(
            fs.read_file_opt(dir, &candidate, MAX_ARTIFACT_BYTES),
            Ok(Some(_))
        ) {
            return if dir == "." {
                candidate
            } else {
                format!("{dir}/{candidate}")
            };
        }
    }
    sidecar_path
        .strip_suffix(".map.json")
        .map(|base| format!("{base}.ts"))
        .unwrap_or_else(|| sidecar_path.to_owned())
}

fn write_manifest_atomic(root: &Path, bytes: &[u8]) -> Result<(), ArtifactFailure> {
    let dir = root.join(MANIFEST_DIR);
    std::fs::create_dir_all(&dir).map_err(|_| ArtifactFailure::Io("manifest-write"))?;
    let physical = dir.join(MANIFEST_NAME);
    let stage = dir.join("ownership.json.tmp");
    std::fs::write(&stage, bytes).map_err(|_| ArtifactFailure::Io("manifest-write"))?;
    std::fs::rename(&stage, &physical).map_err(|_| ArtifactFailure::Io("manifest-write"))?;
    let read_back = std::fs::read(&physical).map_err(|_| ArtifactFailure::Io("manifest-write"))?;
    if read_back != bytes {
        return Err(ArtifactFailure::Io("manifest-write"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deterministic worst-class precedence of the aggregate
    /// envelope: unsupported-version first, invalid second, denied
    /// third, then the two exit-4 classes, and every diagnostic
    /// preserved.
    #[test]
    fn aggregate_envelopes_precedence() {
        use crate::diagnostics::normalize::build;
        use crate::diagnostics::DiagnosticSet;
        use crate::result::Status;
        let one = |id: &str, status: Status| {
            let diagnostic = build(id, None, None, DataObject::new()).expect("rule registered");
            let set = DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
                .expect("single diagnostic");
            match status {
                Status::Invalid => DomainResult::Invalid { diagnostics: set },
                Status::Denied => DomainResult::Denied { diagnostics: set },
                Status::Unavailable => DomainResult::Unavailable { diagnostics: set },
                Status::UnsupportedVersion => DomainResult::UnsupportedVersion { diagnostics: set },
                _ => DomainResult::UnsupportedOperation { diagnostics: set },
            }
        };
        let version = one("versioning.unsupported-version", Status::UnsupportedVersion);
        let invalid = one("lock.missing", Status::Invalid);
        let denied = one("lock.digest-mismatch", Status::Denied);
        let unavailable = one("lock.component-unavailable", Status::Unavailable);

        // A lone class keeps its own identity.
        assert_eq!(
            aggregate_envelopes(vec![denied.clone()]).status(),
            Status::Denied
        );
        // Invalid dominates denied.
        assert_eq!(
            aggregate_envelopes(vec![denied.clone(), invalid.clone()]).status(),
            Status::Invalid
        );
        // Version dominates everything.
        assert_eq!(
            aggregate_envelopes(vec![invalid, denied, version]).status(),
            Status::UnsupportedVersion
        );
        // Exit-4 classes stay available.
        assert_eq!(
            aggregate_envelopes(vec![unavailable]).status(),
            Status::Unavailable
        );
    }

    /// The receipt wire serializes with the closed discriminator, the
    /// contract identity, and the canonical verdict tokens.
    #[test]
    fn receipt_wire_spelling() {
        assert_eq!(SCHEMA_VERSION, "lekalo/orchestration/v0.2.16");
        assert_eq!(IDENTITY, "dev.lekalo.orchestration-report@0.2.16");
        assert_eq!(Verdict::Blocked.as_str(), "blocked");
        assert_eq!(TargetState::Applied.as_str(), "applied");
        assert_eq!(
            super::super::receipt::ComponentState::Unsupported.as_str(),
            "unsupported"
        );
    }

    /// Issue #47 (plan S6, aligned by review F-6): the ownership manifest
    /// classifies the EMITTED scenario-test spellings — test files and
    /// the shared testkit are test artifacts, .test.map.json sidecars are
    /// data, the port.ts shim and reporter stay support source, and the
    /// zod home keeps schema.
    #[test]
    fn artifact_kinds_classify_by_path_convention() {
        use super::artifact_kind_for;
        assert_eq!(
            artifact_kind_for(
                "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.ts"
            ),
            ArtifactKind::Test
        );
        // The emitted shared testkit is a test artifact (review F-6: the
        // classifier pins the EMITTED spelling testkit.ts).
        assert_eq!(
            artifact_kind_for("src/generated/node-typescript/scenario-tests/testkit.ts"),
            ArtifactKind::Test
        );
        assert_eq!(
            artifact_kind_for(
                "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.map.json"
            ),
            ArtifactKind::Data
        );
        // The port shim and the reporter are support code, never tests.
        assert_eq!(
            artifact_kind_for("src/generated/node-typescript/scenario-tests/port.ts"),
            ArtifactKind::Source
        );
        assert_eq!(
            artifact_kind_for("src/generated/node-typescript/scenario-tests/reporter.mjs"),
            ArtifactKind::Source
        );
        // A test-looking file outside the scenario-tests home stays source.
        assert_eq!(
            artifact_kind_for("src/generated/other/minimal.test.ts"),
            ArtifactKind::Source
        );

        // The zod home keeps its issue #45 classification.
        assert_eq!(
            artifact_kind_for(".lekalo/generated/node-typescript/zod/planner.ts"),
            ArtifactKind::Schema
        );

        // Issue #56: the Laratesto spellings classify identically — the
        // generated test and the shared kit are test artifacts, the
        // sidecar is data, and the port shim plus reporter stay source.
        assert_eq!(
            artifact_kind_for(
                "src/generated/php-laravel/scenario-tests/planner/planner.scenario.focus_happy.test.php"
            ),
            ArtifactKind::Test
        );
        assert_eq!(
            artifact_kind_for("src/generated/php-laravel/scenario-tests/scenario-test-kit.php"),
            ArtifactKind::Test
        );
        assert_eq!(
            artifact_kind_for(
                "src/generated/php-laravel/scenario-tests/planner/planner.scenario.focus_happy.test.map.json"
            ),
            ArtifactKind::Data
        );
        assert_eq!(
            artifact_kind_for("src/generated/php-laravel/scenario-tests/port.php"),
            ArtifactKind::Source
        );
        assert_eq!(
            artifact_kind_for("src/generated/php-laravel/scenario-tests/scenario-reporter.php"),
            ArtifactKind::Source
        );
        // A PHP test-looking file outside the scenario-tests home stays source.
        assert_eq!(
            artifact_kind_for("src/generated/other/minimal.test.php"),
            ArtifactKind::Source
        );

        // Review F-6: the emitted sidecar spelling `.test.map.json` pairs
        // through module_path_of to the emitted `.test.ts` artifact —
        // the absent-sibling default.
        let empty = temp_fs("module-default");
        assert_eq!(
            super::module_path_of(
                &empty,
                "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.map.json",
            ),
            "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.ts",
        );
    }

    /// One read capability over a fresh temporary root for the
    /// sidecar-pairing probes.
    fn temp_fs(tag: &str) -> Fs {
        let dir =
            std::env::temp_dir().join(format!("lekalo-generate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        Fs::open(&dir).expect("temp fs")
    }

    /// Issue #56: a `.test.map.json` sidecar pairs with the emitted
    /// sibling that is actually staged — `X.test.php` for the Laratesto
    /// backend, `X.test.ts` for the TypeScript default.
    #[test]
    fn module_path_of_pairs_the_staged_sibling() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("lekalo-generate-module-php-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let module_dir = root.join("src/generated/php-laravel/scenario-tests/planner");
        std::fs::create_dir_all(&module_dir).expect("module dir");
        let mut module =
            std::fs::File::create(module_dir.join("planner.scenario.focus_happy.test.php"))
                .expect("staged php module");
        module.write_all(b"<?php\n").expect("module bytes");
        drop(module);
        let fs = Fs::open(&root).expect("fs");
        assert_eq!(
            super::module_path_of(
                &fs,
                "src/generated/php-laravel/scenario-tests/planner/planner.scenario.focus_happy.test.map.json",
            ),
            "src/generated/php-laravel/scenario-tests/planner/planner.scenario.focus_happy.test.php",
        );
        // No staged sibling keeps the TypeScript default spelling.
        let absent = temp_fs("module-absent");
        assert_eq!(
            super::module_path_of(
                &absent,
                "src/generated/php-laravel/scenario-tests/planner/planner.scenario.focus_happy.test.map.json",
            ),
            "src/generated/php-laravel/scenario-tests/planner/planner.scenario.focus_happy.test.ts",
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review cline F-1: the ownership manifest ingests every emitted
    /// sidecar declaration id through `SemanticOwnerId::parse` (the Model
    /// symbol grammar), and the pairing binds the sidecar to the emitted
    /// `<id>.test.ts` artifact. The emitted declaration ids are
    /// grammar-valid Model symbols; the kind/step spelling rides in
    /// metadata.
    #[test]
    fn emitted_sidecar_declaration_ids_ingest_through_the_owner_grammar() {
        use crate::loader::ModelVersion;
        let version = ModelVersion::Current;
        let scenario_id = "planner.scenario.minimal";
        // The exact declaration-id spellings the emitter produces:
        // the scenario id itself, and the scenario leaf scoped under
        // each then-step id.
        let emitted_ids = [
            scenario_id.to_owned(),
            "minimal.output".to_owned(),
            "minimal.state".to_owned(),
        ];
        for id in &emitted_ids {
            assert!(
                crate::ir::grammar::is_symbol_id(version, id),
                "grammar-valid declaration id: {id}"
            );
            assert!(
                SemanticOwnerId::parse(id, version).is_some(),
                "manifest-ingestible declaration id: {id}"
            );
        }
        // The refused shapes stay refused: the old kind-prefixed ids the
        // emitter used before the fix are exactly what the grammar
        // rejects, so the manifest apply hard-failed on them.
        for id in ["scenario:planner.scenario.minimal", "then:output"] {
            assert!(SemanticOwnerId::parse(id, version).is_none());
        }
        // The sidecar path pairs to the emitted module and classifies
        // as `test`, so the binding passes the map-without-artifact
        // refusal for the emitted write set.
        let sidecar =
            "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.map.json";
        let module = super::module_path_of(&temp_fs("module-sidecar"), sidecar);
        assert!(module.ends_with(".test.ts"));
        assert_eq!(super::artifact_kind_for(&module), ArtifactKind::Test);
    }

    /// Issue #56 (plan S3): the lifecycle convention the manifest
    /// retention and delete-refusal rules key on — everything under the
    /// user-owned `tests/lekalo/` scaffold home is scaffolded except its
    /// generated `.map.json` sidecars, and every managed path stays
    /// generated.
    #[test]
    fn lifecycle_for_marks_only_the_scaffold_home() {
        use super::lifecycle_for;
        assert_eq!(
            lifecycle_for("tests/lekalo/scenario-tests/planner/planner.scenario.happy.test.php"),
            Lifecycle::Scaffolded
        );
        // The emitted sidecar inside the scaffold home stays generated:
        // it is machine-owned even there.
        assert_eq!(
            lifecycle_for(
                "tests/lekalo/scenario-tests/planner/planner.scenario.happy.test.map.json"
            ),
            Lifecycle::Generated
        );
        assert_eq!(
            lifecycle_for(
                "src/generated/php-laravel/scenario-tests/planner/planner.scenario.happy.test.php"
            ),
            Lifecycle::Generated
        );
        assert_eq!(
            lifecycle_for("src/generated/node-typescript/planner.ts"),
            Lifecycle::Generated
        );
        assert_eq!(
            lifecycle_for("tests/planner/happy_test.php"),
            Lifecycle::Generated
        );
        // Issue #58: the type scaffold home is user-owned by the same
        // convention, and its bundle marker sidecar stays generated.
        assert_eq!(
            lifecycle_for("app/lekalo-types/planner/task_dto.php"),
            Lifecycle::Scaffolded
        );
        assert_eq!(
            lifecycle_for("app/lekalo-types/planner/optional/optional_due_date.php"),
            Lifecycle::Scaffolded
        );
        assert_eq!(
            lifecycle_for("app/lekalo-types/types.map.json"),
            Lifecycle::Generated
        );
        // A lookalike root outside the closed constant stays generated.
        assert_eq!(
            lifecycle_for("app/lekalo-types-extra/planner/task_dto.php"),
            Lifecycle::Generated
        );
    }
}

/// The loader resolves the fixture through the process working
/// directory: serialize every cwd mutation across the join suites that
/// share this test binary.
#[cfg(test)]
static GENERATE_CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod operations_join_tests {
    use super::*;
    use crate::loader::{normalize_model, LoadSelection};

    /// The join runs before any adapter exchange: a bogus policy
    /// binding refuses the run as the registered
    /// `php-operations.join-invalid` invalid result.
    #[test]
    fn a_bogus_policy_binding_refuses_the_generate_run() {
        let _guard = GENERATE_CWD_LOCK.lock().expect("cwd lock");
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_dir
            .parent()
            .and_then(Path::parent)
            .expect("workspace");
        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(workspace).expect("enter workspace");
        let selection = LoadSelection {
            project: Some("tests/fixtures/php-laravel/operations/model".to_owned()),
        };
        let model = normalize_model(&selection).expect("fixture loads");
        let compilation = crate::ir::compile(&model).expect("fixture compiles");
        let sandbox =
            std::env::temp_dir().join(format!("lekalo-join-wiring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sandbox);
        std::fs::create_dir_all(sandbox.join(".lekalo/cache/ir")).expect("evidence home");
        std::fs::create_dir_all(sandbox.join("lekalo/types")).expect("types home");
        std::fs::create_dir_all(sandbox.join("lekalo/operations")).expect("operations home");
        let ir_bytes = compilation.project.to_canonical_json();
        std::fs::write(sandbox.join(".lekalo/cache/ir/planner.json"), &ir_bytes).expect("evidence");
        let ir_digest = format!("sha256:{}", sha256_hex(ir_bytes.as_bytes()));
        let types_input = format!(
            "{{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",\"irDigest\":\"{ir_digest}\",\"projectId\":\"planner\",\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}}\n"
        );
        std::fs::write(
            sandbox.join("lekalo/types/planner.types.json"),
            &types_input,
        )
        .expect("types input");
        let types_digest = format!("sha256:{}", sha256_hex(types_input.as_bytes()));
        let zeros = format!("sha256:{}", "0".repeat(64));
        let template = std::fs::read_to_string(
            workspace.join("tests/fixtures/php-laravel/operations/inputs/planner.operations.json"),
        )
        .expect("template");
        let mut input: serde_json::Value = serde_json::from_str(
            &template
                .replace(
                    &format!("\"irDigest\": \"{zeros}\""),
                    &format!("\"irDigest\": \"{ir_digest}\""),
                )
                .replace(
                    &format!("\"typesInputDigest\": \"{zeros}\""),
                    &format!("\"typesInputDigest\": \"{types_digest}\""),
                ),
        )
        .expect("template decodes");
        input["operations"][1]["policy"] = serde_json::json!({"id": "planner.bogus_policy"});
        std::fs::write(
            sandbox.join("lekalo/operations/planner.operations.json"),
            serde_json::to_string_pretty(&input).expect("input serializes") + "\n",
        )
        .expect("operations input");
        let outcome = run_operations_join(
            &sandbox,
            &sandbox.join("lekalo/operations/planner.operations.json"),
            &compilation.project,
            "planner",
        )
        .expect_err("a bogus policy binding refuses the run");
        assert_eq!(outcome.status(), crate::result::Status::Invalid);
        let rendered = outcome.to_json_string();
        assert!(
            rendered.contains("php-operations.join-invalid"),
            "the registered rule rides the refusal: {rendered}"
        );
        assert!(
            rendered.contains("operations.policy-unresolved"),
            "the typed finding code rides the data: {rendered}"
        );
        let _ = std::fs::remove_dir_all(&sandbox);
        std::env::set_current_dir(original).expect("restore cwd");
    }
}

#[cfg(test)]
mod routes_join_tests {
    use super::*;
    use crate::loader::{normalize_model, LoadSelection};

    /// The join runs before any adapter exchange: a route record bound
    /// to an unresolvable endpoint refuses the run as the registered
    /// `php-routes.join-invalid` invalid result.
    #[test]
    fn an_unresolvable_endpoint_refuses_the_generate_run() {
        // The loader path policy only accepts workspace-relative project
        // selections resolved through the process working directory, so
        // this suite enters the workspace root under the shared cwd lock
        // (the operations join suite mutates cwd in this same binary).
        let _guard = GENERATE_CWD_LOCK.lock().expect("cwd lock");
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_dir
            .parent()
            .and_then(Path::parent)
            .expect("workspace");
        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(workspace).expect("enter workspace");
        let selection = LoadSelection {
            project: Some("tests/fixtures/php-laravel/routes/model".to_owned()),
        };
        let model = normalize_model(&selection).expect("fixture loads");
        let compilation = crate::ir::compile(&model).expect("fixture compiles");
        let sandbox =
            std::env::temp_dir().join(format!("lekalo-routes-join-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sandbox);
        for home in [
            ".lekalo/cache/ir",
            ".lekalo/cache/transport",
            ".lekalo/cache/openapi",
            "lekalo/types",
            "lekalo/operations",
            "lekalo/routes",
        ] {
            std::fs::create_dir_all(sandbox.join(home)).expect("evidence home");
        }
        let ir_bytes = compilation.project.to_canonical_json();
        std::fs::write(sandbox.join(".lekalo/cache/ir/planner.json"), &ir_bytes).expect("evidence");
        let ir_digest = format!("sha256:{}", sha256_hex(ir_bytes.as_bytes()));
        let transport = std::fs::read(
            workspace.join("tests/fixtures/php-laravel/routes/inputs/transport.json"),
        )
        .expect("transport fixture");
        let canonical_transport = &transport[..transport.len() - 1];
        std::fs::write(
            sandbox.join(".lekalo/cache/transport/planner.json"),
            canonical_transport,
        )
        .expect("transport evidence");
        std::fs::write(sandbox.join("lekalo/transport.yaml"), canonical_transport)
            .expect("transport home");
        let transport_digest = format!("sha256:{}", sha256_hex(canonical_transport));
        let types_input = format!(
            "{{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",\"irDigest\":\"{ir_digest}\",\"projectId\":\"planner\",\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}}\n"
        );
        std::fs::write(
            sandbox.join("lekalo/types/planner.types.json"),
            &types_input,
        )
        .expect("types input");
        let types_digest = format!("sha256:{}", sha256_hex(types_input.as_bytes()));
        let zeros = format!("sha256:{}", "0".repeat(64));
        let operations_template = std::fs::read_to_string(
            workspace.join("tests/fixtures/php-laravel/routes/inputs/planner.operations.json"),
        )
        .expect("operations template");
        let operations_bytes = operations_template
            .replace(
                &format!("\"irDigest\": \"{zeros}\""),
                &format!("\"irDigest\": \"{ir_digest}\""),
            )
            .replace(
                &format!("\"typesInputDigest\": \"{zeros}\""),
                &format!("\"typesInputDigest\": \"{types_digest}\""),
            );
        std::fs::write(
            sandbox.join("lekalo/operations/planner.operations.json"),
            &operations_bytes,
        )
        .expect("operations input");
        let operations_digest = format!("sha256:{}", sha256_hex(operations_bytes.as_bytes()));
        let routes_template = std::fs::read_to_string(
            workspace.join("tests/fixtures/php-laravel/routes/inputs/planner.routes.json"),
        )
        .expect("routes template");
        let mut input: serde_json::Value = serde_json::from_str(
            &routes_template
                .replace(
                    &format!("\"irDigest\": \"{zeros}\""),
                    &format!("\"irDigest\": \"{ir_digest}\""),
                )
                .replace(
                    &format!("\"transportDigest\": \"{zeros}\""),
                    &format!("\"transportDigest\": \"{transport_digest}\""),
                )
                .replace(
                    &format!("\"typesInputDigest\": \"{zeros}\""),
                    &format!("\"typesInputDigest\": \"{types_digest}\""),
                )
                .replace(
                    &format!("\"operationsInputDigest\": \"{zeros}\""),
                    &format!("\"operationsInputDigest\": \"{operations_digest}\""),
                ),
        )
        .expect("template decodes");
        input["routes"][0]["id"] = serde_json::json!("planner.endpoint_aaa");
        std::fs::write(
            sandbox.join("lekalo/routes/planner.routes.json"),
            serde_json::to_string_pretty(&input).expect("input serializes") + "\n",
        )
        .expect("routes input");
        let outcome = run_routes_join(
            &sandbox,
            &sandbox.join("lekalo/routes/planner.routes.json"),
            &compilation.project,
            "planner",
        )
        .expect_err("an unresolvable endpoint refuses the run");
        assert_eq!(outcome.status(), crate::result::Status::Invalid);
        let rendered = outcome.to_json_string();
        assert!(
            rendered.contains("php-routes.join-invalid"),
            "the registered rule rides the refusal: {rendered}"
        );
        assert!(
            rendered.contains("routes.endpoint-unresolved"),
            "the typed finding code rides the data: {rendered}"
        );
        let _ = std::fs::remove_dir_all(&sandbox);
        std::env::set_current_dir(original).expect("restore cwd");
    }
}
