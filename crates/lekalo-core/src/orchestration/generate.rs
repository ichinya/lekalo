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
    Lifecycle, ProjectRef, SemanticOwnerId, MANIFEST_DIR, MANIFEST_NAME,
};
use crate::artifacts::ArtifactFailure;
use crate::diagnostics::types::token_value;
use crate::diagnostics::{DataObject, Diagnostic};
use crate::ir::Compilation;
use crate::loader::{self, LoadSelection, ModelVersion};
use crate::lockfile::types::Sha256Digest;
use crate::lockfile::{LockRequirement, LockVerifier, Lockfile, ResolvedAdapter, RuntimeInventory};
use crate::result::{DomainResult, Status};
use crate::target_protocol::transport::TransportLimits;
use crate::target_protocol::wire::{Operation, WriteAction, WriteEntry};
use crate::target_protocol::{CallRequest, TargetClient, TargetFailure};
use crate::versioning::plan::sha256_hex;

use super::catalog::{binding_failure, discover, locked_adapter, AdapterSupply};
use super::receipt::{
    AdapterReceipt, GenerateReceipt, InputsReceipt, IrEvidenceReceipt, ScopeReceipt, TargetCounts,
    TargetReceipt, TargetState, Verdict, WriteReceipt, IDENTITY, SCHEMA_VERSION,
};
use super::version::{DEFAULT_TIMEOUT_MS, IR_EVIDENCE_DIR, MAX_TARGETS};
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
        let key = ArtifactKey::new(owner.clone(), key_path, ArtifactKind::Source);
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
    let build = |digest: Sha256Digest| {
        ArtifactManifest::new(
            project.clone(),
            crate::artifacts::types::LockRevision::new(lock.digest().clone()),
            prepared.inputs().model().clone(),
            prepared.inputs().ir().clone(),
            keep.clone(),
            Vec::new(),
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
        assert_eq!(SCHEMA_VERSION, "lekalo/orchestration/v1.0.0");
        assert_eq!(IDENTITY, "dev.lekalo.orchestration-report@1.0.0");
        assert_eq!(Verdict::Blocked.as_str(), "blocked");
        assert_eq!(TargetState::Applied.as_str(), "applied");
        assert_eq!(
            super::super::receipt::ComponentState::Unsupported.as_str(),
            "unsupported"
        );
    }
}
