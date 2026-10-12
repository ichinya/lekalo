//! Bounded read-only CLI adapter for issue #19; every decision belongs
//! to the core.
//!
//! This module owns the confined reads (byte-bounded, duplicate-key
//! rejecting, safe project-relative selectors routed through the
//! reparse-checked project filesystem, so no junction or symlink can
//! escape the resolved project root), the exact profile-pin
//! verification against the resolved profile documents, the qualified
//! implementation join through the existing producer, and the human/JSON
//! rendering. The core [`lekalo_core::portability`] performs the whole
//! assessment over typed inputs and launches nothing.

use clap::Args as ClapArgs;
use lekalo_core::diagnostics::DiagnosticSet;
use lekalo_core::portability::{
    self as core, assess, embedded_recipe, extract, parse_evidence, parse_targets, profile_pin,
    resolve_targets, verify_profile_pin, Request, Scope,
};
use lekalo_core::project_fs::{Fs as ProjectFs, FsErrorKind};
use lekalo_core::{DomainResult, Status};
use std::collections::BTreeMap;

/// The `lekalo portability` flag surface (issue #19).
#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    pub project: Option<String>,
    /// The source target identifier.
    #[arg(long, value_name = "TARGET")]
    pub from: Option<String>,
    /// One destination target identifier; repeatable.
    #[arg(long = "to", value_name = "TARGET")]
    pub to: Vec<String>,
    /// The closed target-binding configuration (default
    /// `config/portability-targets.json`).
    #[arg(long, value_name = "PATH")]
    pub targets: Option<String>,
    /// Qualified recorded evidence manifest.
    #[arg(long, value_name = "PATH")]
    pub evidence: Option<String>,
    /// Scope the assessment to one symbol.
    #[arg(long, value_name = "ID", conflicts_with = "module")]
    pub symbol: Option<String>,
    /// Scope the assessment to one module.
    #[arg(long, value_name = "ID")]
    pub module: Option<String>,
    /// Strict gate: unresolved required obligations deny.
    #[arg(long)]
    pub check: bool,
}

/// The default project-relative configuration home, outside the
/// reserved `lekalo/**` trees.
const DEFAULT_TARGETS: &str = "config/portability-targets.json";
/// The published bound of `inputs.targetsConfig.path` in the report
/// contract: the pinned selector is echoed verbatim, so an over-long
/// selector is an admission refusal, never a schema-invalid emission.
/// The same check covers the default constant for defense in depth.
const MAX_TARGETS_PATH: usize = 256;
/// The canonical lock custody file of the project (`lekalo.lock` at the
/// resolved project root), when present.
const LOCK_PATH: &str = "lekalo.lock";

fn invalid(detail: &'static str) -> DiagnosticSet {
    lekalo_core::portability::diagnostic::invalid(detail)
}

fn fs_error_class(kind: FsErrorKind) -> DiagnosticSet {
    match kind {
        FsErrorKind::NotFound => invalid("input-absent"),
        FsErrorKind::Limit { .. } => invalid("input-size"),
        FsErrorKind::Io => invalid("input-read"),
    }
}

/// One bounded, classified read of a project-relative file through the
/// confined project filesystem: safe-path grammar, per-component
/// reparse/symlink-checked directories, regular-file classification
/// before and after the open, byte limit. Every project-relative
/// artifact read routes through this capability — `std::fs::File::open`
/// would follow Windows junctions and Unix symlinks out of the root.
fn read_bounded(fs: &ProjectFs, relative: &str) -> Result<Vec<u8>, DiagnosticSet> {
    if lekalo_core::project_fs::path_violation(relative).is_some() {
        return Err(invalid("artifact-ref"));
    }
    let (directory, name) = match relative.rsplit_once('/') {
        Some((dir, file)) => (dir, file),
        None => ("", relative),
    };
    match fs.read_file_opt(
        directory,
        name,
        lekalo_core::portability::version::MAX_INPUT_BYTES,
    ) {
        Ok(Some(bytes)) => Ok(bytes),
        // The confined capability reports an absent file through the
        // shared error class; the portability classification keeps its
        // own absent token.
        Ok(None) => Err(invalid("input-absent")),
        Err(kind) => Err(fs_error_class(kind)),
    }
}

/// The optional artifact read: an absent file is a `None`, every other
/// failure is terminal.
fn read_bounded_opt(fs: &ProjectFs, relative: &str) -> Result<Option<Vec<u8>>, DiagnosticSet> {
    match read_bounded(fs, relative) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(set) if set
            .as_slice()
            .iter()
            .any(|d| {
                d.id() == "portability.input-invalid"
                    && d.data()
                        .get("detail")
                        .map(|value| {
                            matches!(value, lekalo_core::diagnostics::DataValue::Token(token) if token == "input-absent")
                        })
                        .unwrap_or(false)
            }) =>
        {
            Ok(None)
        }
        Err(set) => Err(set),
    }
}

fn failure(set: DiagnosticSet) -> DomainResult {
    if set
        .as_slice()
        .iter()
        .any(|d| d.id() == "portability.version-unsupported")
    {
        DomainResult::unsupported_version(set)
    } else {
        DomainResult::invalid(set)
    }
}

pub fn run(args: Args) -> DomainResult {
    match execute(args) {
        Ok(result) => result,
        Err(set) => failure(set),
    }
}

/// Decode one profile document and verify the configured pin exactly.
fn load_profile(
    fs: &ProjectFs,
    binding: &lekalo_core::portability::wire::TargetBinding,
) -> Result<Option<lekalo_core::portability::ProfilePin>, DiagnosticSet> {
    let bytes = read_bounded(fs, &binding.profile_document_ref)?;
    let document = lekalo_core::target_profile::document::decode(&bytes)
        .map_err(|_| lekalo_core::portability::diagnostic::invalid("profile-document"))?;
    let resolved = lekalo_core::target_profile::resolution::resolve(&document)
        .map_err(|_| lekalo_core::portability::diagnostic::invalid("profile-resolution"))?;
    let found = resolved
        .iter()
        .find(|profile| profile.id == binding.profile_ref.id)
        .ok_or_else(|| invalid("profile-pin"))?;
    verify_profile_pin(&binding.profile_ref, found)?;
    Ok(Some(profile_pin(found)))
}

/// The verified family custody: the artifact bytes per admitted family
/// plus the decoded storage inventory when the storage family verified.
type VerifiedFamilies = (
    BTreeMap<lekalo_core::portability::wire::EvidenceFamily, Vec<u8>>,
    Option<Vec<lekalo_core::portability::wire::StorageRequirement>>,
);

/// Read, hash, and decode every admitted complete evidence family
/// artifact. Declared completeness without readable, digest-matching,
/// decodable custody stays unverified: the assessment reports the gap
/// instead of granting portability from an authored enum. A digest
/// mismatch is fabricated custody and refuses outright. Every non-
/// implementation family decodes through its closed owner envelope: the
/// exact project pin is verified against this compilation and the
/// storage envelope's complete inventory is reconciled against the
/// manifest, so a content-free or contradicting object never certifies
/// a complete inventory.
fn verify_families(
    fs: &ProjectFs,
    evidence: &lekalo_core::portability::EvidenceDocument,
    compilation: &lekalo_core::ir::Compilation,
) -> Result<VerifiedFamilies, DiagnosticSet> {
    let mut verified = BTreeMap::new();
    let mut storage_inventory = None;
    let project = &compilation.project;
    let expected_project_id = project
        .project
        .as_ref()
        .map(|p| p.id.as_str().to_owned())
        .unwrap_or_default();
    for family in &evidence.families {
        if !family.complete {
            continue;
        }
        let (Some(reference), Some(digest)) = (&family.artifact_ref, &family.digest) else {
            continue;
        };
        let Some(bytes) = read_bounded_opt(fs, reference)? else {
            // An absent claimed-complete artifact is an evidence gap;
            // the assessment demotes the family instead of trusting it.
            continue;
        };
        if format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)) != *digest {
            return Err(invalid("family-digest"));
        }
        if family.family == lekalo_core::portability::wire::EvidenceFamily::Implementation {
            // The implementation family's owner contract is the
            // implementation producer, which validates the artifact
            // against this exact compilation in the join below.
            let value = lekalo_core::portability::wire::decode_dup_safe(&bytes)
                .map_err(|_| invalid("family-shape"))?;
            if !value.is_object() {
                return Err(invalid("family-shape"));
            }
            verified.insert(family.family, bytes);
            continue;
        }
        let artifact =
            lekalo_core::portability::wire::decode_family_artifact(&bytes, family.family)?;
        // The envelope's project pin must be exactly this compilation's:
        // an artifact minted for another project never certifies this
        // one's inventory, whatever its digest pins.
        if artifact.project_ref.project_id != expected_project_id
            || artifact.project_ref.model_version != project.model_version.as_str()
            || artifact.project_ref.ir_digest
                != lekalo_core::implementation::ir_digest(project).as_str()
        {
            return Err(invalid("family-project-pin"));
        }
        if family.family == lekalo_core::portability::wire::EvidenceFamily::Storage {
            // The envelope and the manifest must describe the same
            // complete inventory; a contradiction is fabricated custody.
            let manifest_inventory: std::collections::BTreeSet<(
                &str,
                &lekalo_core::portability::wire::Policy,
            )> = evidence
                .storage_required
                .iter()
                .map(|requirement| (requirement.subject.as_str(), &requirement.policy))
                .collect();
            let artifact_inventory: std::collections::BTreeSet<(
                &str,
                &lekalo_core::portability::wire::Policy,
            )> = artifact
                .storage_required
                .iter()
                .map(|requirement| (requirement.subject.as_str(), &requirement.policy))
                .collect();
            if artifact_inventory != manifest_inventory {
                return Err(invalid("family-inventory"));
            }
            storage_inventory = Some(artifact.storage_required.clone());
        }
        verified.insert(family.family, bytes);
    }
    Ok((verified, storage_inventory))
}

/// Join the qualified implementation projection: the evidence manifest
/// must declare the family complete with verified artifact custody, and
/// the producer must validate the document against this exact
/// compilation. The artifact bytes decode through the
/// duplicate-key-refusing parser before the typed bridge.
fn load_implementation(
    artifact: &[u8],
    compilation: &lekalo_core::ir::Compilation,
) -> Result<Option<lekalo_core::implementation::PortabilityReport>, DiagnosticSet> {
    let value: serde_json::Value = lekalo_core::portability::wire::decode_dup_safe(artifact)
        .map_err(|_| invalid("implementation-shape"))?;
    let document = lekalo_core::implementation::ImplementationDocument::from_value(&value)
        .map_err(|_| invalid("implementation-shape"))?;
    match lekalo_core::implementation::validate(&document, compilation) {
        Ok(report) => Ok(Some(report.portability().clone())),
        Err(set) => Err(set),
    }
}

/// Whether one decoded receipt's provenance pins match this exact run:
/// the compilation (project, model, IR) and the named destination's
/// verified profile pin. A receipt minted for another project, another
/// revision, or another profile never qualifies a claim here.
fn custody_provenance_matches(
    receipt: &lekalo_core::portability::wire::ReceiptArtifact,
    compilation: &lekalo_core::ir::Compilation,
    pins: &BTreeMap<String, Option<lekalo_core::portability::ProfilePin>>,
    target: &str,
) -> bool {
    let project = &compilation.project;
    let project_id = project
        .project
        .as_ref()
        .map(|p| p.id.as_str().to_owned())
        .unwrap_or_default();
    if receipt.project_ref.project_id != project_id
        || receipt.project_ref.model_version != project.model_version.as_str()
        || receipt.project_ref.ir_digest != lekalo_core::implementation::ir_digest(project).as_str()
    {
        return false;
    }
    match pins.get(target) {
        Some(Some(pin)) => {
            receipt.profile_ref.id == pin.id
                && receipt.profile_ref.version == pin.version
                && receipt.profile_ref.resolved_digest == pin.resolved_digest
        }
        _ => false,
    }
}

/// The qualified execution custody: the (scenario, destination) joins
/// with verified receipt custody, and the (gate, destination) joins
/// qualified through receipt custody with their reconciled results.
type QualifiedCustody = (
    std::collections::BTreeSet<(String, String)>,
    BTreeMap<(String, String), bool>,
);

/// Qualify the claimed execution custody: every claimed scenario or gate
/// result must present its referenced receipt artifact, digest-bound to
/// the exact pinned bytes, decoded through the closed receipt contract,
/// naming exactly the claimed subject and destination, pinned to this
/// compilation and to the destination's verified profile, and stating a
/// result that agrees with the claim. Anything else stays unqualified
/// and never upgrades a join; a contradicting result is fabricated
/// custody and refuses outright — a fail receipt can never evidence a
/// pass.
fn qualify_custody(
    fs: &ProjectFs,
    evidence: &lekalo_core::portability::EvidenceDocument,
    compilation: &lekalo_core::ir::Compilation,
    pins: &BTreeMap<String, Option<lekalo_core::portability::ProfilePin>>,
) -> Result<QualifiedCustody, DiagnosticSet> {
    let mut scenarios = std::collections::BTreeSet::new();
    let mut gates = BTreeMap::new();
    for record in &evidence.scenarios {
        let (Some(reference), Some(digest)) = (&record.receipt_ref, &record.receipt_digest) else {
            continue;
        };
        let Some(target) = &record.target_id else {
            continue;
        };
        let Some(bytes) = read_bounded_opt(fs, reference)? else {
            continue;
        };
        if format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)) != *digest {
            return Err(invalid("receipt-digest"));
        }
        let receipt = lekalo_core::portability::wire::decode_receipt(&bytes)?;
        // Scope qualification: the receipt must name exactly the claimed
        // subject and destination; a receipt for another join never
        // proves this one executed.
        if receipt.kind != lekalo_core::portability::wire::ReceiptKind::Scenario
            || receipt.subject != record.scenario
            || receipt.target_id != *target
        {
            continue;
        }
        if !custody_provenance_matches(&receipt, compilation, pins, target) {
            continue;
        }
        // The manifest's claim must agree with the custody's result.
        let claims_pass =
            record.state == lekalo_core::portability::wire::ScenarioReceipt::RecordedPass;
        let claims_fail =
            record.state == lekalo_core::portability::wire::ScenarioReceipt::RecordedFail;
        let result_pass = receipt.result == lekalo_core::portability::wire::ReceiptResult::Pass;
        if (claims_pass && !result_pass) || (claims_fail && result_pass) {
            return Err(invalid("receipt-result"));
        }
        scenarios.insert((record.scenario.clone(), target.clone()));
    }
    for gate in &evidence.gates {
        if gate.state != lekalo_core::portability::wire::GateAuthorState::Executed {
            continue;
        }
        let (Some(reference), Some(digest)) = (&gate.receipt_ref, &gate.receipt_digest) else {
            continue;
        };
        let Some(target) = &gate.target_id else {
            continue;
        };
        let Some(bytes) = read_bounded_opt(fs, reference)? else {
            continue;
        };
        if format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)) != *digest {
            return Err(invalid("receipt-digest"));
        }
        let receipt = lekalo_core::portability::wire::decode_receipt(&bytes)?;
        if receipt.kind != lekalo_core::portability::wire::ReceiptKind::Gate
            || receipt.subject != gate.gate
            || receipt.target_id != *target
        {
            continue;
        }
        if !custody_provenance_matches(&receipt, compilation, pins, target) {
            continue;
        }
        // Gate custody is keyed by the exact (gate, destination) the
        // receipt names: a receipt for one destination never qualifies
        // another destination's gate.
        gates.insert(
            (gate.gate.clone(), target.clone()),
            receipt.result == lekalo_core::portability::wire::ReceiptResult::Pass,
        );
    }
    Ok((scenarios, gates))
}

fn execute(args: Args) -> Result<DomainResult, DiagnosticSet> {
    let selection = lekalo_core::loader::LoadSelection {
        project: args
            .project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    // The loader-resolved project root: every subsequent artifact read
    // binds to this exact tree, never to the raw selector spelled at
    // the invocation directory.
    let root = match lekalo_core::loader::resolve_selection_root(&selection) {
        Ok(root) => root,
        Err(result) => return Ok(result),
    };
    let fs = ProjectFs::open(&root).map_err(fs_error_class)?;
    let model = match lekalo_core::loader::normalize_model(&selection) {
        Ok(model) => model,
        Err(result) => return Ok(result),
    };
    let compilation = match lekalo_core::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => return Ok(failure.into_result()),
    };
    let recipe = embedded_recipe()?;

    // Targets configuration: the explicit selector or the default home;
    // an absent default derives the targets from the Model declarations.
    let targets_path = args
        .targets
        .clone()
        .unwrap_or_else(|| DEFAULT_TARGETS.to_owned());
    // The report echoes the pinned selector as `inputs.targetsConfig.path`
    // (published at most 256 characters): a grammar-legal but over-long
    // selector is refused here, so legal input can never emit a member
    // a conforming consumer must reject.
    if targets_path.len() > MAX_TARGETS_PATH {
        return Err(invalid("targets-path-bound"));
    }
    let targets_bytes = read_bounded_opt(&fs, &targets_path)?;
    let (targets, targets_input) = match targets_bytes {
        Some(bytes) => {
            let document = parse_targets(&bytes)?;
            let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes));
            (
                Some(document),
                lekalo_core::portability::wire::TargetsInput {
                    state: lekalo_core::portability::wire::PinState::Pinned,
                    path: Some(targets_path.clone()),
                    digest: Some(digest),
                },
            )
        }
        None => {
            if args.targets.is_some() {
                return Err(invalid("input-absent"));
            }
            (
                None,
                lekalo_core::portability::wire::TargetsInput {
                    state: lekalo_core::portability::wire::PinState::Derived,
                    path: None,
                    digest: None,
                },
            )
        }
    };

    // Evidence: optional, pinned, and recipe/project verified.
    let mut evidence_digest: Option<String> = None;
    let evidence = match &args.evidence {
        Some(path) => {
            let bytes = read_bounded(&fs, path)?;
            evidence_digest = Some(format!(
                "sha256:{}",
                lekalo_core::digest::sha256_hex(&bytes)
            ));
            let document = parse_evidence(&bytes, &compilation, &recipe)?;
            Some(document)
        }
        None => None,
    };

    // The request scope and selection.
    let request = Request {
        scope: match (&args.symbol, &args.module) {
            (Some(symbol), _) => Scope::Symbol(symbol.clone()),
            (None, Some(module)) => Scope::Module(module.clone()),
            (None, None) => Scope::Project,
        },
        source: args.from.clone(),
        destinations: args.to.clone(),
    };
    request.validate()?;

    // Profile pins, adapter snapshots, and verified storage-profile
    // artifact reads per configured target. The declared storage and
    // adapter references are read, decoded, and digest-pinned here: the
    // report never echoes a reference it has not verified through the
    // confined filesystem, and unparseable artifacts refuse outright.
    let subjects = extract(&compilation.project, &recipe);
    let mut pins: BTreeMap<String, Option<lekalo_core::portability::ProfilePin>> = BTreeMap::new();
    let mut storage_refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut adapter_artifacts: BTreeMap<
        String,
        lekalo_core::portability::wire::AdapterSnapshotArtifact,
    > = BTreeMap::new();
    let mut adapter_evidence_digests: BTreeMap<String, String> = BTreeMap::new();
    if let Some(document) = &targets {
        for binding in &document.targets {
            pins.insert(binding.target_id.clone(), load_profile(&fs, binding)?);
            let mut verified_refs = Vec::new();
            for reference in &binding.storage_profile_refs {
                let bytes = read_bounded(&fs, reference)?;
                // Admission: a storage profile artifact must decode as a
                // closed storage-engine-profile document; arbitrary JSON
                // or bytes never pass as a storage profile.
                let value = lekalo_core::portability::wire::decode_dup_safe(&bytes)
                    .map_err(|_| invalid("storage-profile-shape"))?;
                lekalo_core::storage_engine_profile::StorageEngineProfile::from_value(&value)
                    .map_err(|_| invalid("storage-profile-shape"))?;
                verified_refs.push(format!(
                    "{}#sha256:{}",
                    reference,
                    lekalo_core::digest::sha256_hex(&bytes)
                ));
            }
            storage_refs.insert(binding.target_id.clone(), verified_refs);
            if let Some(adapter_ref) = &binding.adapter_evidence_ref {
                // The adapter evidence reference is opened, decoded
                // through its closed snapshot contract, and digest-
                // pinned: its verified custody reconciles the recorded
                // adapter record below, so manifest fields alone never
                // fabricate recorded provenance.
                let bytes = read_bounded(&fs, adapter_ref)?;
                let artifact = lekalo_core::portability::wire::decode_adapter_snapshot(&bytes)?;
                if artifact.target_id != binding.target_id {
                    return Err(invalid("adapter-provenance"));
                }
                adapter_artifacts.insert(binding.target_id.clone(), artifact);
                adapter_evidence_digests.insert(
                    binding.target_id.clone(),
                    format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
                );
            }
        }
    }
    // The adapter snapshots: a recorded record qualifies only when its
    // target binding carried a digest-bound snapshot artifact that
    // reconciles every field exactly and pins this destination's
    // verified profile — recorded provenance is constructed from the
    // admitted artifact, never echoed from the manifest.
    let mut adapters: BTreeMap<String, Option<lekalo_core::portability::wire::AdapterSnapshot>> =
        BTreeMap::new();
    if let Some(evidence) = &evidence {
        for adapter in &evidence.adapters {
            let snapshot = match adapter.state {
                lekalo_core::portability::wire::AdapterState::Recorded => {
                    let artifact = adapter_artifacts
                        .get(&adapter.target_id)
                        .ok_or_else(|| invalid("adapter-provenance"))?;
                    let record_matches = adapter.identity.as_deref()
                        == Some(artifact.adapter.identity.as_str())
                        && adapter.version.as_deref() == Some(artifact.adapter.version.as_str())
                        && adapter.protocol.as_deref() == Some(artifact.adapter.protocol.as_str())
                        && adapter.capability_digest.as_deref()
                            == Some(artifact.adapter.capability_digest.as_str());
                    let profile_matches = match pins.get(&adapter.target_id) {
                        Some(Some(pin)) => {
                            artifact.profile_ref.id == pin.id
                                && artifact.profile_ref.version == pin.version
                                && artifact.profile_ref.resolved_digest == pin.resolved_digest
                        }
                        _ => false,
                    };
                    if !record_matches || !profile_matches {
                        return Err(invalid("adapter-provenance"));
                    }
                    Some(lekalo_core::portability::wire::AdapterSnapshot {
                        state: adapter.state,
                        identity: adapter.identity.clone(),
                        version: adapter.version.clone(),
                        protocol: adapter.protocol.clone(),
                        capability_digest: adapter.capability_digest.clone(),
                        evidence_digest: adapter_evidence_digests.get(&adapter.target_id).cloned(),
                    })
                }
                lekalo_core::portability::wire::AdapterState::Absent => {
                    Some(lekalo_core::portability::wire::AdapterSnapshot {
                        state: adapter.state,
                        identity: None,
                        version: None,
                        protocol: None,
                        capability_digest: None,
                        evidence_digest: adapter_evidence_digests.get(&adapter.target_id).cloned(),
                    })
                }
            };
            adapters.insert(adapter.target_id.clone(), snapshot);
        }
    }
    let resolved = resolve_targets(
        &request,
        targets.as_ref(),
        &subjects,
        &pins,
        &adapters,
        &storage_refs,
    )?;

    // The lock custody pin: the canonical `lekalo.lock` at the resolved
    // project root, read through the confined filesystem, decoded
    // through the strict lock parser, and reconciled against the exact
    // profile pins of the selected targets. Garbage bytes or a lock
    // that contradicts a selected pin refuse; a lock that corroborates
    // nothing selectable reports unreconciled custody, never known.
    let selected_for_lock: Vec<&lekalo_core::portability::TargetInput> =
        std::iter::once(&resolved.source)
            .chain(resolved.destinations.iter())
            .collect();
    let mut lock_state = lekalo_core::portability::wire::LockPresence::Absent;
    let mut lock_digest: Option<String> = None;
    if let Some(bytes) = read_bounded_opt(&fs, LOCK_PATH)? {
        let lock = lekalo_core::lockfile::Lockfile::parse_canonical(&bytes)
            .map_err(|_| invalid("lock-shape"))?;
        let mut corroborated = 0usize;
        let mut complete = true;
        for target in &selected_for_lock {
            let Some(pin) = &target.profile else {
                continue;
            };
            match lock
                .profiles()
                .iter()
                .find(|profile| profile.id().as_str() == pin.id)
            {
                Some(entry) => {
                    let matches = entry.version().as_str() == pin.version
                        && entry.source_digest().as_str() == pin.source_digest
                        && entry.digest().as_str() == pin.resolved_digest;
                    if !matches {
                        return Err(invalid("lock-profile"));
                    }
                    corroborated += 1;
                }
                None => complete = false,
            }
        }
        lock_state = if complete && corroborated > 0 {
            lekalo_core::portability::wire::LockPresence::Known
        } else {
            lekalo_core::portability::wire::LockPresence::Unreconciled
        };
        lock_digest = Some(format!(
            "sha256:{}",
            lekalo_core::digest::sha256_hex(&bytes)
        ));
    }

    // The qualified evidence families: every admitted complete family
    // artifact read, hash-verified, and decoded through its owner
    // contract, with the storage inventory reconciled.
    let (verified_family_artifacts, verified_storage_inventory) = match &evidence {
        Some(document) => verify_families(&fs, document, &compilation)?,
        None => (BTreeMap::new(), None),
    };
    let implementation = match &evidence {
        Some(document) => {
            let family = document.families.iter().find(|f| {
                f.family == lekalo_core::portability::wire::EvidenceFamily::Implementation
            });
            match family {
                Some(family)
                    if family.complete
                        && verified_family_artifacts.contains_key(&family.family) =>
                {
                    load_implementation(&verified_family_artifacts[&family.family], &compilation)?
                }
                _ => None,
            }
        }
        None => None,
    };
    // The qualified execution custody: scenario receipts and executed
    // gates, each digest-bound, scope-checked, provenance-pinned, and
    // result-reconciled.
    let (receipts_qualified, gates_qualified) = match &evidence {
        Some(document) => qualify_custody(&fs, document, &compilation, &pins)?,
        None => (std::collections::BTreeSet::new(), BTreeMap::new()),
    };

    let assessment = core::Assessment {
        compilation: &compilation,
        recipe: &recipe,
        subjects,
        request: &request,
        source: resolved.source.clone(),
        destinations: resolved.destinations.clone(),
        evidence: evidence.as_ref(),
        implementation: implementation.as_ref(),
        verified_families: verified_family_artifacts.into_keys().collect(),
        verified_storage_inventory,
        receipts_qualified,
        gates_qualified,
        targets_input,
        lock_digest,
        lock_state,
        evidence_digest,
    };
    let report = assess(&assessment)?;

    // The envelope diagnostics: one per distinct triggered rule, in the
    // registry chain, carrying bounded tokens only.
    let mut diagnostics: Vec<lekalo_core::diagnostics::Diagnostic> = Vec::new();
    let mut codes: Vec<&str> = Vec::new();
    for finding in &report.diagnostics {
        if !codes.contains(&finding.code.as_str()) {
            codes.push(match finding.code.as_str() {
                "portability.evidence-incomplete" => "portability.evidence-incomplete",
                "portability.mapping-partial" => "portability.mapping-partial",
                "portability.capability-unsupported" => "portability.capability-unsupported",
                "portability.implementation-required" => "portability.implementation-required",
                "portability.behavior-loss" => "portability.behavior-loss",
                "portability.analysis-incomplete" => "portability.analysis-incomplete",
                _ => continue,
            });
        }
    }
    for code in codes {
        diagnostics.push(lekalo_core::portability::diagnostic::diagnostic(
            code,
            "portability",
            "finding",
        )?);
    }

    let human = human_summary(&report);
    let canonical = lekalo_core::portability::canonical::canonical_bytes(&report);
    // The recipe's declared report byte bound is enforced against the
    // canonical bytes; an oversized report is an explicit analysis
    // failure, never a silently truncated emission.
    if canonical.len() as u64 > recipe.document.limits.max_report_bytes {
        return Err(invalid("report-size"));
    }
    let json = format!("{{\"portability\":{}}}", canonical);

    // The strict gate: unresolved required obligations deny while the
    // report stays visible in the denied envelope.
    if args.check && core::planning::denial(&report) {
        let mut denied = diagnostics;
        denied.push(lekalo_core::portability::diagnostic::diagnostic(
            "portability.check-denied",
            "portability",
            "unresolved-required-obligations",
        )?);
        let set = DiagnosticSet::try_from_unsorted(denied, Status::Denied)
            .map_err(|_| invalid("diagnostic-bound"))?;
        return Ok(DomainResult::denied_json(json, human, set));
    }

    let set = DiagnosticSet::try_from_unsorted(diagnostics, Status::Valid)
        .map_err(|_| invalid("diagnostic-bound"))?;
    // The valid envelope states its own status beside the payload; the
    // denied envelope carries the payload without re-asserting a
    // contradicting inner status.
    let json = format!("{{\"status\":\"valid\",{}", &json[1..]);
    Ok(DomainResult::validation(
        json,
        human,
        set.as_slice().to_vec(),
    ))
}

/// The bounded list projection: every listed class is fully enumerable
/// in the machine output, so the human form names its truncation and
/// the concrete complete-output route instead of silently clipping.
const HUMAN_LIST_LIMIT: usize = 16;

fn push_bounded_list(lines: &mut Vec<String>, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    if items.len() <= HUMAN_LIST_LIMIT {
        lines.push(format!("{title} ({}):", items.len()));
    } else {
        lines.push(format!(
            "{title} ({} shown of {}; complete rows in the JSON output):",
            HUMAN_LIST_LIMIT,
            items.len()
        ));
    }
    lines.extend(
        items
            .iter()
            .take(HUMAN_LIST_LIMIT)
            .map(|l| format!("  {l}")),
    );
}

/// The human projection: every destination's four risk dimension
/// summaries before anything else, then the structural ratio, aggregate
/// counters, and the bounded blocker/partial/obligation listings.
fn human_summary(report: &lekalo_core::portability::Report) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Portability: {} subjects, {} destinations, source {}.",
        report.coverage.eligible_subjects,
        report.destinations.len(),
        report.source.target_id
    ));
    for summary in &report.summaries {
        lines.push(format!("{} risk dimensions:", summary.destination));
        for (dimension, entry) in &summary.dimensions {
            lines.push(format!(
                "  {}: portable {}, supported-with-mapping {}, partial {}, target-specific {}, unsupported {}, unknown {}, non-applicable {}, eligible-applicable {}",
                dimension.as_str(),
                entry.portable,
                entry.supported_with_mapping,
                entry.partial,
                entry.target_specific,
                entry.unsupported,
                entry.unknown,
                entry.non_applicable,
                entry.eligible_applicable,
            ));
            let verification = &entry.verification;
            lines.push(format!(
                "    verification: untested {}, recorded-pass {}, recorded-fail {}, not-applicable {}",
                verification.untested,
                verification.recorded_pass,
                verification.recorded_fail,
                verification.not_applicable,
            ));
        }
        let structural = summary
            .dimensions
            .get(&lekalo_core::portability::Dimension::Structural)
            .expect("structural summary");
        let ratio = match structural.structural_portable {
            Some(ratio) => format!("{}/{}", ratio.numerator, ratio.denominator),
            None => "unknown".to_owned(),
        };
        lines.push(format!(
            "{}: structural portable ratio {}, blockers {}, unknowns {}, custom obligations {}",
            summary.destination,
            ratio,
            summary.blockers,
            summary.unknowns,
            summary.custom_obligations
        ));
    }
    let mut blockers = Vec::new();
    let mut partials = Vec::new();
    let mut customs = Vec::new();
    let mut losses = Vec::new();
    let mut unknowns = Vec::new();
    for subject in &report.subjects {
        for row in &subject.rows {
            if row.blocking {
                blockers.push(format!(
                    "{} -> {}: {} ({})",
                    subject.subject,
                    row.destination,
                    row.dimension.as_str(),
                    row.reason
                ));
            }
            if row.classification == lekalo_core::portability::Classification::Partial {
                partials.push(format!(
                    "{} -> {}: {}",
                    subject.subject, row.destination, row.reason
                ));
            }
            if matches!(
                row.binding,
                Some(
                    lekalo_core::portability::Binding::Custom
                        | lekalo_core::portability::Binding::Foreign
                        | lekalo_core::portability::Binding::External
                )
            ) {
                customs.push(format!(
                    "{} -> {}: {}",
                    subject.subject,
                    row.destination,
                    row.binding.map(|b| b.as_str()).unwrap_or_default()
                ));
            }
            if row.reason == "storage-policy-loss" || row.reason == "implicit-hook-unsupported" {
                losses.push(format!(
                    "{} -> {}: {}",
                    subject.subject, row.destination, row.reason
                ));
            }
            if row.verification == lekalo_core::portability::Verification::Untested
                && row.classification != lekalo_core::portability::Classification::NonApplicable
            {
                unknowns.push(format!(
                    "{} -> {}: untested",
                    subject.subject, row.destination
                ));
            }
        }
    }
    push_bounded_list(&mut lines, "Blockers", &blockers);
    push_bounded_list(&mut lines, "Partial mappings", &partials);
    push_bounded_list(&mut lines, "Custom/foreign/external obligations", &customs);
    push_bounded_list(&mut lines, "Behavior loss", &losses);
    push_bounded_list(&mut lines, "Untested verification states", &unknowns);
    lines.push(format!(
        "Migration surface: {} missing implementations, {} storage reviews, {} affected scenarios, {} affected gates, planning rows {}.",
        report.migration_surface.missing_implementations,
        report.migration_surface.storage_reviews,
        report.migration_surface.affected_scenarios,
        report.migration_surface.affected_gates,
        report.migration_surface.rows.len(),
    ));
    lines.join("\n")
}
