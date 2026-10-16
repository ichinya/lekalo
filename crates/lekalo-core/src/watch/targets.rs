//! Ephemeral scan/validation over captured bytes, never the live project.
//! Locked entry bytes are checked BEFORE describe executes. Package custody,
//! revocation, manifest consistency and OS confinement stay with their owners.
//! Legacy adapters use a visible complete/full fallback, not a selective claim.
#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::{diagnostics, request::WatchRequest, snapshot::InputSnapshot, StageRow};
use crate::lockfile::{LockState, Lockfile};
use crate::orchestration::{AdapterSupply, Failure};
use crate::project_fs::Fs;
use crate::result::{DomainResult, Status};
use crate::target_protocol::{
    transport::TransportLimits,
    wire::{Operation, SupportState},
    CallRequest, TargetClient,
};

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterPin {
    pub target: String,
    pub id: String,
    pub version: String,
    /// Verified lock/package digest, never the describe self-assertion.
    pub digest: String,
    pub declared_digest: String,
    pub entry_digest: String,
    pub capability_digest: String,
    pub protocol_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Actual module/global requests, including retained partition provenance.
    pub work: Vec<TargetWork>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetWork {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub state: &'static str,
    pub source_revision: String,
    pub selection_digest: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TargetPartition {
    pub work: TargetWork,
    pub status: Status,
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
}
#[derive(Clone, Debug)]
pub(crate) struct TargetState {
    pub pin: AdapterPin,
    pub partitions: std::collections::BTreeMap<String, TargetPartition>,
}

pub(crate) struct SelectionContext<'a> {
    pub affected: &'a super::selection::AffectedSet,
    pub work: &'a [super::WorkUnitWire],
    pub previous: &'a std::collections::BTreeMap<String, TargetState>,
    pub old_symbols: &'a std::collections::BTreeMap<String, Vec<String>>,
    pub symbols: &'a std::collections::BTreeMap<String, Vec<String>>,
    pub changed_paths: Vec<String>,
}

#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
thread_local! {
    static SCAN_ON_SPAWN: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

pub(crate) struct TargetScan {
    pub pins: Vec<AdapterPin>,
    pub rows: Vec<StageRow>,
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
    pub status: Status,
    pub states: std::collections::BTreeMap<String, TargetState>,
}

fn mapped(failure: Failure) -> DomainResult {
    DomainResult::from(&failure)
}
fn unsupported(detail: &'static str) -> DomainResult {
    mapped(Failure::Target(
        crate::target_protocol::TargetFailure::CapabilityUnsupported { detail },
    ))
}
fn read_lock(root: &Path) -> Result<Lockfile, DomainResult> {
    match crate::lockfile::plan::LockService::read_state_at(root)
        .map_err(|f| mapped(Failure::Lock(f)))?
    {
        LockState::Present(lock) => Ok(*lock),
        LockState::Absent => Err(mapped(Failure::Lock(crate::lockfile::LockFailure::Missing))),
    }
}
fn limits(timeout_ms: u64) -> TransportLimits {
    TransportLimits {
        timeout_ms,
        ..TransportLimits::default()
    }
}
fn locked<'a>(
    lock: &'a Lockfile,
    target: &str,
) -> Result<&'a crate::lockfile::ResolvedAdapter, DomainResult> {
    lock.adapters()
        .iter()
        .find(|adapter| adapter.id().as_str() == target)
        .ok_or_else(|| {
            mapped(Failure::AdapterNotLocked {
                adapter: target.to_owned(),
            })
        })
}
fn entry(
    root: &Path,
    pin: &crate::lockfile::ResolvedAdapter,
) -> Result<AdapterSupply, DomainResult> {
    if pin.source().kind() != crate::lockfile::types::SourceKind::Project {
        return Err(unsupported("watch-adapter-source"));
    }
    let path = pin.source().id();
    if crate::project_fs::path_violation(path).is_some() {
        return Err(mapped(Failure::AdapterSupplyRequired));
    }
    let fs = Fs::open(root).map_err(|_| mapped(Failure::AdapterSupplyRequired))?;
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    let bytes = fs
        .read_file_opt(dir, name, crate::target_protocol::version::MAX_FILE_BYTES)
        .map_err(|_| mapped(Failure::AdapterSupplyRequired))?
        .ok_or_else(|| mapped(Failure::AdapterSupplyRequired))?;
    let digest = format!("sha256:{}", crate::digest::sha256_hex(&bytes));
    if !pin
        .artifacts()
        .iter()
        .any(|artifact| artifact.digest().as_str() == digest)
    {
        return Err(mapped(Failure::AdapterDigestMismatch {
            adapter: pin.id().as_str().to_owned(),
        }));
    }
    if let Some(expected) = pin.manifest_digest() {
        let bytes = fs
            .read_file_opt(
                dir,
                "adapter.manifest.json",
                crate::target_protocol::version::MAX_FILE_BYTES,
            )
            .map_err(|_| mapped(Failure::AdapterSupplyRequired))?
            .ok_or_else(|| mapped(Failure::AdapterSupplyRequired))?;
        let manifest = crate::adapter_package::ManifestDocument::from_bytes(&bytes)
            .map_err(|f| mapped(Failure::AdapterPackage(f)))?;
        if manifest.digest().as_str() != expected.as_str() {
            return Err(mapped(Failure::AdapterDigestMismatch {
                adapter: pin.id().as_str().to_owned(),
            }));
        }
    }
    let path = root.join(path);
    let (program, args) = match path.extension().and_then(|e| e.to_str()) {
        Some("js" | "mjs") => (
            PathBuf::from("node"),
            vec![path.to_string_lossy().into_owned()],
        ),
        Some("php") => (
            PathBuf::from("php"),
            vec![path.to_string_lossy().into_owned()],
        ),
        _ => (path, Vec::new()),
    };
    Ok(AdapterSupply {
        command: crate::target_protocol::transport::AdapterCommand { program, args },
        source_id: pin.source().id().to_owned(),
    })
}
fn describe(
    root: &Path,
    pin: &crate::lockfile::ResolvedAdapter,
    timeout_ms: u64,
    cancel: Option<&AtomicBool>,
) -> Result<
    (
        TargetClient,
        AdapterSupply,
        crate::target_protocol::discovery::DiscoveredAdapter,
    ),
    DomainResult,
> {
    let supply = entry(root, pin)?;
    if !TargetClient::bounded_read_only_available() {
        return Err(unsupported("watch-bounded-read-only"));
    }
    let mut client = TargetClient::new(limits(timeout_ms));
    let discovered = crate::orchestration::catalog::discover_watch_cancellable(
        &mut client,
        &supply,
        root,
        cancel,
    )
    .map_err(mapped)?;
    if discovered.adapter.id != pin.id().as_str()
        || discovered.adapter.version != pin.version().to_string()
    {
        return Err(mapped(Failure::AdapterNotLocked {
            adapter: pin.id().as_str().to_owned(),
        }));
    }
    crate::orchestration::catalog::binding_failure(pin, &discovered).map_err(mapped)?;
    Ok((client, supply, discovered))
}
fn captured_root(scope: &str) -> Result<Option<String>, DomainResult> {
    let root = scope
        .strip_suffix("/**")
        .or_else(|| scope.strip_suffix("/*"))
        .unwrap_or(scope);
    // IR is derived ephemerally from this revision. Never capture durable
    // cache or generated output as semantic adapter input / feedback authority.
    if root == ".lekalo/cache/ir" || root.starts_with(".lekalo/cache/ir/") {
        return Ok(None);
    }
    if [".git", ".lekalo/cache", ".lekalo/generated"]
        .iter()
        .any(|home| root == *home || root.starts_with(&format!("{home}/")))
    {
        return Err(unsupported("watch-derived-read-scope"));
    }
    let portable = root.strip_prefix(".lekalo/").unwrap_or(root);
    if crate::project_fs::path_violation(portable).is_some() {
        return Err(unsupported("watch-read-scope"));
    }
    Ok(Some(root.to_owned()))
}
fn feedback_safe(
    discovered: &crate::target_protocol::discovery::DiscoveredAdapter,
) -> Result<(), DomainResult> {
    for scope in &discovered.read_scopes {
        let Some(read) = captured_root(scope)? else {
            continue;
        };
        if ["target", "build", "dist", "coverage", "node_modules"]
            .iter()
            .any(|home| read == *home || read.starts_with(&format!("{home}/")))
        {
            return Err(unsupported("watch-build-read-scope"));
        }
        for write in &discovered.write_scopes {
            let write = write
                .strip_suffix("/**")
                .or_else(|| write.strip_suffix("/*"))
                .unwrap_or(write);
            if read == write
                || read.starts_with(&format!("{write}/"))
                || write.starts_with(&format!("{read}/"))
            {
                // Never silently remove bytes from a declared full read
                // scope, nor subscribe to its generated output as source.
                return Err(unsupported("watch-feedback-scope"));
            }
        }
    }
    Ok(())
}
fn metadata_roots(roots: &mut BTreeSet<String>, source: &str) {
    roots.insert(source.to_owned());
    let parent = source.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    roots.insert(if parent.is_empty() {
        "adapter.manifest.json".to_owned()
    } else {
        format!("{parent}/adapter.manifest.json")
    });
    roots.insert(crate::adapter_package::trust::REVOCATIONS_FILE.to_owned());
    roots.insert(crate::adapter_package::inventory::INVENTORY_FILE.to_owned());
}

/// Bounded, empty-view discovery used only to determine subscription roots.
/// No scan/validate or live Model disclosure occurs in this preparation.
pub fn preflight(
    root: &Path,
    targets: &[String],
    timeout_ms: u64,
) -> Result<BTreeSet<String>, DomainResult> {
    if targets.is_empty() {
        return Ok(BTreeSet::new());
    }
    // Even preparation must not execute a live entry between its checksum
    // check and describe. Resolve roots from one private lock capture, then
    // describe only verified bytes from a second complete captured view.
    let base = super::snapshot::capture(root, false).map_err(|e| e.domain_result())?;
    super::budget::admit(&base).map_err(|e| e.domain_result())?;
    let lock_view = tempfile::tempdir().map_err(|_| mapped(Failure::AdapterSupplyRequired))?;
    super::snapshot::materialize(&base, lock_view.path()).map_err(|e| e.domain_result())?;
    let lock = read_lock(lock_view.path())?;
    let mut roots = BTreeSet::new();
    for target in targets {
        let pin = locked(&lock, target)?;
        if pin.source().kind() != crate::lockfile::types::SourceKind::Project {
            return Err(unsupported("watch-adapter-source"));
        }
        metadata_roots(&mut roots, pin.source().id());
    }
    let policy = super::snapshot::CapturePolicy {
        target_roots: roots.clone(),
        generate_check: false,
    };
    let captured =
        super::snapshot::capture_with_policy(root, &policy).map_err(|e| e.domain_result())?;
    super::budget::admit(&captured).map_err(|e| e.domain_result())?;
    let view = tempfile::tempdir().map_err(|_| mapped(Failure::AdapterSupplyRequired))?;
    super::snapshot::materialize(&captured, view.path()).map_err(|e| e.domain_result())?;
    let lock = read_lock(view.path())?;
    for target in targets {
        let pin = locked(&lock, target)?;
        let (_, _, discovered) = describe(
            view.path(),
            pin,
            timeout_ms,
            Some(super::interrupt::cancellation_flag()),
        )?;
        feedback_safe(&discovered)?;
        for scope in &discovered.read_scopes {
            if let Some(path) = captured_root(scope)? {
                roots.insert(path);
            }
        }
    }
    Ok(roots)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn scan(
    scratch: &Path,
    request: &WatchRequest,
    model: &crate::loader::NormalizedModel,
    compilation: &crate::ir::Compilation,
    snapshot: &InputSnapshot,
    cancel: &AtomicBool,
    selection: &SelectionContext<'_>,
) -> TargetScan {
    let mut result = TargetScan {
        pins: Vec::new(),
        rows: Vec::new(),
        diagnostics: Vec::new(),
        status: Status::Valid,
        states: std::collections::BTreeMap::new(),
    };
    for target in &request.targets {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        // Generated-only saves cannot trigger adapters, even describe.
        // All semantic/lock/entry/target input bytes are unchanged.
        let reused = !request.no_cache
            && selection.changed_paths.is_empty()
            && selection.previous.contains_key(target);
        let execution = if reused {
            let mut retained = selection.previous[target].clone();
            for work in &mut retained.pin.work {
                work.state = "reused";
            }
            for partition in retained.partitions.values_mut() {
                partition.work.state = "reused";
            }
            Ok(retained)
        } else {
            scan_target(
                scratch,
                target,
                request,
                model,
                compilation,
                snapshot,
                cancel,
                selection,
            )
        };
        match execution {
            Ok(state) => {
                for partition in state.partitions.values() {
                    result.status = super::combine_statuses(result.status, partition.status);
                    result
                        .diagnostics
                        .extend(partition.diagnostics.iter().cloned());
                }
                if state.partitions.values().any(|p| p.status != Status::Valid) {
                    result.rows.push(StageRow::unavailable(
                        "target-scan",
                        "adapter-refused",
                        "partial",
                    ));
                } else {
                    result.rows.push(StageRow {
                        reason: Some(
                            if state.pin.work.is_empty() {
                                "adapter-full-scope"
                            } else {
                                "adapter-selected-scope"
                            }
                            .to_owned(),
                        ),
                        state: if reused { "reused" } else { "executed" },
                        ..StageRow::executed("target-scan")
                    });
                }
                result.pins.push(state.pin.clone());
                result.states.insert(target.clone(), state);
            }
            Err(failure) => {
                // Unavailable discovery cannot silently erase untouched
                // accepted diagnostic partitions. Coverage stays partial.
                if let Some(old) = selection.previous.get(target) {
                    result.diagnostics.extend(
                        old.partitions
                            .values()
                            .flat_map(|p| p.diagnostics.iter().cloned()),
                    );
                    result.states.insert(target.clone(), old.clone());
                }
                result.status = super::combine_statuses(result.status, failure.status());
                result
                    .diagnostics
                    .extend(failure.diagnostics().iter().cloned());
                if matches!(failure.status(), Status::Unavailable) {
                    result
                        .diagnostics
                        .push(diagnostics::adapter_recovery(target));
                }
                result.rows.push(StageRow::unavailable(
                    "target-scan",
                    "adapter-refused",
                    "partial",
                ));
            }
        }
    }
    result
}
#[allow(clippy::too_many_arguments)]
fn scan_target(
    scratch: &Path,
    target: &str,
    request: &WatchRequest,
    model: &crate::loader::NormalizedModel,
    compilation: &crate::ir::Compilation,
    snapshot: &InputSnapshot,
    cancel: &AtomicBool,
    selection: &SelectionContext<'_>,
) -> Result<TargetState, DomainResult> {
    let lock = read_lock(scratch)?;
    let pin = locked(&lock, target)?;
    let (mut client, supply, discovered) =
        describe(scratch, pin, request.adapter_timeout_ms, Some(cancel))?;
    feedback_safe(&discovered)?;
    for scope in &discovered.read_scopes {
        if let Some(path) = captured_root(scope)? {
            // Absent optional files are legal only if the host captured that
            // exact selected root; a changed adapter scope must not become an
            // empty successful scan over an incomplete snapshot.
            if !snapshot.selected_roots.contains(&path)
                && path != "lekalo"
                && !path.starts_with("lekalo/")
                && path != "lekalo.lock"
            {
                return Err(unsupported("watch-scope-not-captured"));
            }
        }
    }
    if !discovered.targets.iter().any(|t| t == target)
        || !discovered.ir_compatible(model.model_version.as_str())
    {
        return Err(unsupported("watch-target-or-ir"));
    }
    if !discovered.operations.iter().any(|o| o == "scan")
        || !discovered.operations.iter().any(|o| o == "validate")
        || discovered
            .capability("scan.symbols")
            .map_or(true, |c| c.state != SupportState::Full)
    {
        return Err(unsupported("watch-scan-validation"));
    }
    let prepared = crate::artifacts::check::Prepared::prepare_captured(scratch, model, compilation)
        .map_err(|f| mapped(Failure::Artifact(f)))?;
    if request.locked {
        crate::orchestration::watch_locked_preflight(&prepared, pin, &discovered)?;
    }
    let project = compilation
        .project
        .project
        .as_ref()
        .ok_or_else(|| mapped(Failure::ProjectRefUnresolved))?;
    let evidence = format!(
        "{}/{}.json",
        crate::orchestration::IR_EVIDENCE_DIR,
        project.id.as_str()
    );
    let path = scratch.join(&evidence);
    let parent = path
        .parent()
        .ok_or_else(|| mapped(Failure::AdapterSupplyRequired))?;
    std::fs::create_dir_all(parent).map_err(|_| mapped(Failure::AdapterSupplyRequired))?;
    let ir = compilation.project.to_canonical_json();
    if ir.len() > crate::target_protocol::version::MAX_FILE_BYTES {
        return Err(unsupported("watch-ir-limit"));
    }
    std::fs::write(&path, ir).map_err(|_| mapped(Failure::AdapterSupplyRequired))?;
    let profile = discovered.selected_profile(None);
    let call = |operation, ir_path| CallRequest {
        operation,
        target: Some(target),
        profile: profile.as_deref(),
        profile_resolution: None,
        ir_path,
        dry_run: None,
        plan_id: None,
        native_request: None,
        lint_request: None,
    };
    #[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
    if let Some(callback) = SCAN_ON_SPAWN.with(|slot| slot.borrow_mut().take()) {
        crate::target_protocol::transport::on_next_pump_start(callback);
    }
    let selective = client.describe_outcome().is_some_and(|outcome| {
        outcome.negotiated_version == crate::target_protocol::version::WATCH_VERSION
            && outcome
                .capabilities
                .watch
                .as_ref()
                .is_some_and(|s| s.selection)
    });
    // A legacy fallback has no way to bind successor Model semantics.
    // Refuse before either scan or validate rather than disclose new IR
    // under a predecessor pin or silently down-convert it.
    if model.model_version == crate::loader::ModelVersion::Successor && !selective {
        return Err(unsupported("watch-successor-pins"));
    }
    let input_pins = crate::target_protocol::watch::WatchPins::for_compilation(compilation);
    let mut adapter_pin = AdapterPin {
        target: target.to_owned(),
        id: discovered.adapter.id,
        version: discovered.adapter.version,
        digest: pin.digest().as_str().to_owned(),
        declared_digest: discovered.adapter.digest,
        entry_digest: discovered
            .executable_digest
            .expect("lock binding verified entry digest"),
        capability_digest: discovered.capability_digest,
        protocol_version: discovered.negotiated_version,
        profile: profile.clone(),
        work: Vec::new(),
    };
    let previous = selection.previous.get(target).filter(|old| {
        !request.no_cache
            && old.pin.entry_digest == adapter_pin.entry_digest
            && old.pin.capability_digest == adapter_pin.capability_digest
            && old.pin.digest == adapter_pin.digest
            && !old.pin.work.is_empty()
    });
    let mut partitions = std::collections::BTreeMap::new();
    if !selective {
        let failure = execute_partition(
            &mut client,
            &supply,
            &call,
            scratch,
            model,
            target,
            profile.as_deref(),
            &evidence,
            cancel,
            None,
        )
        .err();
        partitions.insert(
            "*".to_owned(),
            partition(
                failure,
                TargetWork {
                    scope: "full".to_owned(),
                    module: None,
                    state: "executed",
                    source_revision: snapshot.source_revision(),
                    selection_digest: snapshot.source_revision(),
                },
                target,
            ),
        );
    } else {
        let modules: BTreeSet<_> = selection
            .work
            .iter()
            .map(|unit| unit.module.clone())
            .chain(
                previous
                    .into_iter()
                    .flat_map(|old| old.partitions.keys().filter(|key| !key.is_empty()).cloned()),
            )
            .collect();
        if modules.len() > super::request::limits::MAX_TARGET_PARTITIONS {
            return Err(super::snapshot::CaptureError::Limit {
                limit: "target-partitions",
                count: modules.len() as u64,
            }
            .domain_result());
        }
        for module in std::iter::once(String::new()).chain(modules) {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let current =
                module.is_empty() || selection.work.iter().any(|unit| unit.module == module);
            let selected = module.is_empty()
                || selection.affected.scope == "full"
                || selection.affected.modules.contains(&module)
                || selection
                    .work
                    .iter()
                    .any(|unit| unit.module == module && unit.state == "executed");
            if !selected && current {
                if let Some(old) = previous.and_then(|old| old.partitions.get(&module)) {
                    let mut retained = old.clone();
                    retained.work.state = "reused";
                    adapter_pin.work.push(retained.work.clone());
                    partitions.insert(module, retained);
                    continue;
                }
            }
            let symbols: BTreeSet<String> = [selection.old_symbols, selection.symbols]
                .into_iter()
                .flat_map(|index| index.values().flatten())
                .filter(|id| {
                    !module.is_empty() && (*id == &module || id.starts_with(&format!("{module}.")))
                })
                .cloned()
                .collect();
            let selected = crate::target_protocol::watch::WatchSelection {
                source_revision: snapshot.source_revision(),
                pins: input_pins.clone(),
                scope: if module.is_empty() {
                    "global"
                } else {
                    "module"
                }
                .to_owned(),
                module: (!module.is_empty()).then(|| module.clone()),
                symbols: symbols.into_iter().collect(),
                changed_paths: selection.changed_paths.clone(),
            };
            let failure = execute_partition(
                &mut client,
                &supply,
                &call,
                scratch,
                model,
                target,
                profile.as_deref(),
                &evidence,
                cancel,
                Some(&selected),
            )
            .err();
            let row = TargetWork {
                scope: selected.scope.clone(),
                module: selected.module.clone(),
                state: "executed",
                source_revision: selected.source_revision.clone(),
                selection_digest: selected.digest(),
            };
            adapter_pin.work.push(row.clone());
            // Removed module partitions are invalidated, not carried into the
            // new graph/bindings. Their final cleanup exchange still ran.
            if current || failure.is_some() {
                partitions.insert(module, partition(failure, row, target));
            }
        }
    }
    Ok(TargetState {
        pin: adapter_pin,
        partitions,
    })
}

fn partition(failure: Option<DomainResult>, work: TargetWork, target: &str) -> TargetPartition {
    let mut partition = TargetPartition {
        work,
        status: Status::Valid,
        diagnostics: Vec::new(),
    };
    if let Some(failure) = failure {
        partition.status = failure.status();
        partition
            .diagnostics
            .extend(failure.diagnostics().iter().cloned());
        if partition.status == Status::Unavailable {
            partition
                .diagnostics
                .push(diagnostics::adapter_recovery(target));
        }
    }
    partition
}

#[allow(clippy::too_many_arguments)]
fn execute_partition<'a>(
    client: &mut TargetClient,
    supply: &AdapterSupply,
    call: &impl Fn(Operation, Option<&'a str>) -> CallRequest<'a>,
    scratch: &Path,
    model: &crate::loader::NormalizedModel,
    target: &str,
    profile: Option<&str>,
    evidence: &'a str,
    cancel: &AtomicBool,
    selection: Option<&crate::target_protocol::watch::WatchSelection>,
) -> Result<(), DomainResult> {
    let fs = Fs::open(scratch).map_err(|_| mapped(Failure::AdapterSupplyRequired))?;
    let mut invoke = |operation, ir| {
        match selection {
            Some(selection) => client.call_watch(
                &supply.command,
                call(operation, ir),
                scratch,
                Some(cancel),
                selection,
            ),
            None => client.call(
                &supply.command,
                call(operation, ir),
                scratch,
                &fs,
                Some(cancel),
            ),
        }
        .map_err(|f| mapped(Failure::Target(f)))
    };
    let scanned = invoke(Operation::Scan, selection.map(|_| evidence))?;
    crate::observed::scan_service::validate_ephemeral(
        &crate::observed::ObservedContext {
            model: model.clone(),
            root: scratch.to_owned(),
        },
        &scanned,
        target,
        profile,
    )?;
    if let Some(selection) = selection {
        let entries = scanned
            .response
            .result
            .as_ref()
            .and_then(|result| result.entries.as_deref())
            .unwrap_or(&[]);
        for entry in entries {
            let semantic = entry
                .detail
                .as_deref()
                .and_then(|detail| serde_json::from_str::<serde_json::Value>(detail).ok())
                .and_then(|detail| detail.get("s").and_then(|s| s.as_str()).map(str::to_owned));
            if !semantic.as_deref().is_some_and(|id| {
                selection
                    .module
                    .as_ref()
                    .is_some_and(|module| id == module || id.starts_with(&format!("{module}.")))
            }) {
                return Err(mapped(Failure::Target(
                    crate::target_protocol::TargetFailure::ResponseInvalid {
                        detail: crate::target_protocol::wire::ResponseInvalidity::Evidence,
                    },
                )));
            }
        }
    }
    let validated = invoke(Operation::Validate, Some(evidence))?;
    if !validated
        .response
        .result
        .as_ref()
        .and_then(|r| r.ok)
        .unwrap_or(false)
    {
        return Err(mapped(Failure::Target(
            crate::target_protocol::TargetFailure::OperationFailed {
                class: crate::target_protocol::wire::ErrorClass::Invalid,
                code: "adapter-check-failed".to_owned(),
                partial: false,
            },
        )));
    }
    Ok(())
}
