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
use crate::result::{DomainResult, Status};
use crate::target_protocol::transport::TransportLimits;
use crate::target_protocol::wire::{Operation, WriteAction, WriteEntry};
use crate::target_protocol::{CallRequest, TargetClient, TargetFailure};

use super::catalog::{binding_failure, discover, locked_adapter, AdapterSupply};
use super::receipt::{
    AdapterReceipt, GenerateReceipt, InputsReceipt, IrEvidenceReceipt, ScopeReceipt, TargetCounts,
    TargetReceipt, TargetState, Verdict, WriteReceipt, IDENTITY, SCHEMA_VERSION,
};
use super::version::{
    DEFAULT_TIMEOUT_MS, IR_EVIDENCE_DIR, MAX_TARGETS, OPENAPI_EVIDENCE_DIR, TRANSPORT_EVIDENCE_DIR,
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
        }
        Ok(None) => {}
    }

    let mut client = TargetClient::new(limits);
    let discovered = discover(&mut client, supply, prepared.root(), limits)?;
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
        if entry.action == WriteAction::Delete {
            continue;
        }
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
            // ownership it did not regenerate: refuse instead.
            if stale {
                return Err(ArtifactFailure::StaleManifest);
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
            Lifecycle::Generated,
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
    // `source`.
    if path.split('/').any(|segment| segment == "scenario-tests")
        && (path.ends_with(".test.ts") || path.ends_with("/testkit.ts"))
    {
        return ArtifactKind::Test;
    }
    ArtifactKind::Source
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
    let declarations = value
        .get("declarations")
        .and_then(serde_json::Value::as_array)
        .ok_or(ArtifactFailure::SourceMapInvalid)?;
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
    let module_path = module_path_of(sidecar_path);
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

/// The `.ts` module path of one `.map.json` sidecar path.
fn module_path_of(sidecar_path: &str) -> String {
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
        // Review F-6: the emitted sidecar spelling `.test.map.json` pairs
        // through module_path_of to the emitted `.test.ts` artifact.
        assert_eq!(
            super::module_path_of(
                "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.map.json",
            ),
            "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.ts",
        );
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
        let module = super::module_path_of(sidecar);
        assert!(module.ends_with(".test.ts"));
        assert_eq!(super::artifact_kind_for(&module), ArtifactKind::Test);
    }
}
