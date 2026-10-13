//! Issue #95 `lekalo watch`: the incremental read-only dev loop.
//!
//! The service executes immutable captured snapshots (the scheduler and
//! affected-selection planner remain pure/offline):
//! every stage of one run consumes the exact same captured bytes
//! (materialized into a bounded scratch root outside the project).
//! Differential tests cover the implemented core-validation cases;
//! complete stage/command equivalence remains required for issue #95. The scheduler owns coalescing and cancellation; the
//! host owns subscriptions, timing, and delivery. Watch never writes
//! the project, never starts destructive or production commands,
//! never generates, and never updates the lock; adapter work that
//! cannot be proven read-only is reported explicitly unavailable.

pub mod changes;
pub mod compile;
pub mod diagnostics;
pub mod events;
pub mod interrupt;
pub mod request;
pub mod scheduler;
pub mod selection;
pub mod snapshot;
#[cfg(test)]
mod tests;
pub mod version;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::diagnostics::Diagnostic;
use crate::loader::NormalizedModel;
use crate::result::Status;
use crate::watch::changes::{diff, ChangeBatch, ChangeKind};
use crate::watch::request::WatchRequest;
use crate::watch::selection::{compute, AffectedSet, ModuleIndex};
use crate::watch::snapshot::{materialize, CaptureError, FileRecord, InputSnapshot};
use crate::watch::version::{RESULT_IDENTITY, RESULT_SCHEMA_VERSION, WATCH_REGISTRY_VERSION};

pub use events::WatchEvent;
pub use request::WatchBackend;

/// One stage row of the result: what ran, what was reused, and the
/// explicit coverage claim.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StageRow {
    /// `validate`, `impact`, `generate-check`, `target-scan`, or
    /// `lock-preflight`.
    pub stage: &'static str,
    /// `executed`, `reused`, `not-requested`, or `unavailable`.
    pub state: &'static str,
    /// `complete`, `partial`, or `unsupported`.
    pub coverage: &'static str,
    /// The closed refusal reason, present when state is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl StageRow {
    fn not_requested(stage: &'static str) -> Self {
        Self {
            stage,
            state: "not-requested",
            coverage: "complete",
            reason: None,
        }
    }

    fn executed(stage: &'static str) -> Self {
        Self {
            stage,
            state: "executed",
            coverage: "complete",
            reason: None,
        }
    }

    fn unavailable(stage: &'static str, reason: &str, coverage: &'static str) -> Self {
        Self {
            stage,
            state: "unavailable",
            coverage,
            reason: Some(reason.to_owned()),
        }
    }
}

/// One changed-input row of the wire result.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ChangedRowWire {
    pub path: String,
    /// `created`, `modified`, or `deleted`.
    pub kind: &'static str,
    #[serde(rename = "oldSha256", skip_serializing_if = "Option::is_none")]
    pub old_sha256: Option<String>,
    #[serde(rename = "newSha256", skip_serializing_if = "Option::is_none")]
    pub new_sha256: Option<String>,
    #[serde(rename = "renameOf", skip_serializing_if = "Option::is_none")]
    pub rename_of: Option<String>,
    /// `semantic` or `generated`.
    pub class: &'static str,
}

/// The affected selection of the wire result.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AffectedWire {
    /// `selective` or `full`.
    pub scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    pub modules: Vec<String>,
}

/// One validation work unit: the module, its content fingerprint, and
/// whether this run executed it or reused the validated partition.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WorkUnitWire {
    pub module: String,
    pub sha256: String,
    /// `executed` or `reused`.
    pub state: &'static str,
}

/// The closed per-run result document (`lekalo/watch-result/v0.8.0`).
#[derive(Clone, Debug, serde::Serialize)]
pub struct WatchResult {
    #[serde(rename = "schemaVersion")]
    pub schema_version: &'static str,
    pub identity: &'static str,
    /// The aggregate run status.
    #[serde(serialize_with = "serialize_status")]
    pub status: Status,
    /// Always `replace`: a settled result is the whole effective
    /// diagnostic set, not only additions.
    #[serde(rename = "diagnosticsMode")]
    pub diagnostics_mode: &'static str,
    #[serde(rename = "sourceRevision")]
    pub source_revision: String,
    #[serde(
        rename = "previousSourceRevision",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_source_revision: Option<String>,
    /// The whole replaced diagnostic set in normalized order.
    pub diagnostics: Vec<Diagnostic>,
    pub stages: Vec<StageRow>,
    #[serde(rename = "changedInputs")]
    pub changed_inputs: Vec<ChangedRowWire>,
    pub affected: AffectedWire,
    /// The selective-work evidence: per-module fingerprints.
    pub work: Vec<WorkUnitWire>,
    pub counts: ResultCounts,
    /// Dependency-linked capsule entries a consumer must invalidate:
    /// symbols removed by this revision plus the symbols declared by
    /// the changed documents. Bounded.
    #[serde(rename = "capsuleInvalidations")]
    pub capsule_invalidations: Vec<String>,
    pub pins: ResultPins,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift: Option<DriftSummary>,
    #[serde(rename = "pipelineCounts", skip_serializing_if = "Option::is_none")]
    pub pipeline_counts: Option<compile::PipelineCounts>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ResultCounts {
    pub diagnostics: usize,
    #[serde(rename = "executedUnits")]
    pub executed_units: usize,
    #[serde(rename = "reusedUnits")]
    pub reused_units: usize,
    #[serde(rename = "changedFiles")]
    pub changed_files: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ResultPins {
    #[serde(rename = "modelVersion", skip_serializing_if = "Option::is_none")]
    pub model_version: Option<&'static str>,
    #[serde(rename = "registryVersion")]
    pub registry_version: &'static str,
}

/// The drift summary of one generate-check execution.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftSummary {
    pub verdict: &'static str,
    pub clean: usize,
    pub stale: usize,
    pub manual_drift: usize,
    pub missing: usize,
    pub orphan: usize,
    pub reported: usize,
    pub findings: usize,
}

impl DriftSummary {
    fn of(receipt: &crate::artifacts::CheckReceipt) -> Self {
        Self {
            verdict: receipt.verdict,
            clean: receipt.counts.clean,
            stale: receipt.counts.stale,
            manual_drift: receipt.counts.manual_drift,
            missing: receipt.counts.missing,
            orphan: receipt.counts.orphan,
            reported: receipt.counts.reported,
            findings: receipt.findings.len(),
        }
    }
}

/// The accepted (committed) state of the last settled revision.
#[derive(Clone, Debug, Default)]
pub struct AcceptedState {
    pub revision: String,
    /// Digest-only view of the accepted semantic inputs.
    pub files: BTreeMap<String, (String, u64)>,
    pub generated: BTreeMap<String, (String, u64)>,
    pub module_index: ModuleIndex,
    pub symbols_by_path: BTreeMap<String, Vec<String>>,
    /// Module validation fingerprints of the accepted revision.
    pub fingerprints: BTreeMap<String, String>,
    /// Retained module-owned diagnostic partitions.
    pub partitions: BTreeMap<String, Vec<Diagnostic>>,
    /// The project-level (global) obligations partition and its
    /// fingerprint.
    pub global_partition: Vec<Diagnostic>,
    pub global_fingerprint: Option<String>,
    pub drift_fingerprint: Option<String>,
    pub drift: Option<DriftSummary>,
    pub(crate) documents: BTreeMap<String, (String, crate::loader::project_docs::Document)>,
    pub(crate) fragments: compile::Fragments,
    pub(crate) graph_fragments: crate::graph::build::GraphReuse,
}

/// The facts one run executes under.
pub struct RunContext<'a> {
    pub run_id: u64,
    pub generation: u64,
    pub full_rescan: bool,
    /// The cooperative stale-run flag; checked between bounded units.
    pub cancel: &'a AtomicBool,
}

/// The terminal outcome of one run.
pub struct RunOutcome {
    pub run_id: u64,
    /// True when the run observed its own cancellation and never
    /// committed; the host emits `run-cancelled` instead.
    pub stale: bool,
    pub source_revision: String,
    pub status: Status,
    pub result: WatchResult,
    /// The state to accept when the host commits this run.
    pub staged: AcceptedState,
}

fn serialize_status<S: serde::Serializer>(
    status: &Status,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(status.as_str())
}

fn sha256_of(parts: &[&[u8]]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
        hasher.update(b"\0");
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn digest_records(map: &BTreeMap<String, (String, u64)>) -> BTreeMap<String, FileRecord> {
    map.iter()
        .map(|(path, (sha256, len))| {
            (
                path.clone(),
                FileRecord {
                    sha256: sha256.clone(),
                    len: *len,
                    bytes: None,
                },
            )
        })
        .collect()
}

fn symbols_by_path(compilation: &crate::ir::Compilation) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in compilation.source_map.entries() {
        let Some(symbol) = &entry.semantic_id else {
            continue;
        };
        map.entry(entry.path.clone())
            .or_default()
            .insert(symbol.clone());
    }
    map.into_iter()
        .map(|(path, symbols)| (path, symbols.into_iter().collect()))
        .collect()
}

fn module_fingerprint(
    index: &ModuleIndex,
    files: &BTreeMap<String, (String, u64)>,
    module: &str,
) -> String {
    let mut rows: Vec<String> = Vec::new();
    let push_path = |path: &str, rows: &mut Vec<String>| match files.get(path) {
        Some((sha256, len)) => rows.push(format!("{path}\0{sha256}\0{len}")),
        None => rows.push(format!("{path}\0absent")),
    };
    if let Some(owned) = index.owned_paths.get(module) {
        for path in owned {
            push_path(path, &mut rows);
        }
    }
    for import in index.import_closure(module) {
        rows.push(format!("import:{import}"));
        if let Some(owned) = index.owned_paths.get(&import) {
            for path in owned {
                push_path(path, &mut rows);
            }
        }
    }
    rows.sort();
    rows.dedup();
    let joined = rows.join("\n").into_bytes();
    sha256_of(&[
        b"watch-module-fingerprint",
        module.as_bytes(),
        &joined,
        crate::validator::PROFILE_VERSION.as_bytes(),
        WATCH_REGISTRY_VERSION.as_bytes(),
    ])
}

fn owner_of_diagnostic(diagnostic: &Diagnostic) -> &str {
    diagnostic
        .symbol
        .as_deref()
        .and_then(|symbol| symbol.split('.').next())
        .unwrap_or("")
}

fn normalize_diagnostics(diagnostics: Vec<Diagnostic>, status: Status) -> Vec<Diagnostic> {
    if diagnostics.len() > crate::validator::MAX_DIAGNOSTICS {
        return vec![crate::watch::diagnostics::resource_limit(
            "result-diagnostics",
            diagnostics.len() as u64,
        )];
    }
    use crate::diagnostics::DiagnosticSet;
    // Rows originate in admitted component results (or registered watch
    // lifecycle producers). Their statuses may differ; the complete
    // replacement must preserve each row's own registry admission, not
    // erase valid findings because another component has a worse class.
    let mut sets = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        let mut admitted = None;
        for component_status in [
            status,
            Status::Valid,
            Status::Invalid,
            Status::Denied,
            Status::Unsupported,
            Status::Unavailable,
            Status::UnsupportedVersion,
        ] {
            if let Ok(set) =
                DiagnosticSet::try_from_unsorted(vec![diagnostic.clone()], component_status)
            {
                admitted = Some(set);
                break;
            }
        }
        let Some(set) = admitted else {
            return crate::validator::registry_invariant_failure()
                .as_slice()
                .to_vec();
        };
        sets.push(set);
    }
    DiagnosticSet::try_union(sets)
        .map(|set| set.as_slice().to_vec())
        .unwrap_or_else(|_| {
            crate::validator::registry_invariant_failure()
                .as_slice()
                .to_vec()
        })
}

fn status_of(diagnostics: &[Diagnostic], denied: bool) -> Status {
    if denied
        || diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "watch.resource-limit")
    {
        return Status::Denied;
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == crate::diagnostics::types::Severity::Error)
    {
        return Status::Invalid;
    }
    Status::Valid
}

/// The watch service: pure revision execution over captured snapshots.
pub struct WatchService {
    request: WatchRequest,
    profile: crate::validator::ValidationProfile,
    accepted: AcceptedState,
}

impl WatchService {
    /// Build the service; the embedded profile is a developer-owned
    /// contract, so a parse failure fails closed.
    pub fn new(request: WatchRequest) -> Result<Self, crate::result::DomainResult> {
        request.validate()?;
        let profile = if request.strict {
            crate::validator::ValidationProfile::embedded_strict()
        } else {
            crate::validator::ValidationProfile::embedded_default()
        }
        .map_err(|_| {
            crate::result::DomainResult::invalid(crate::validator::registry_invariant_failure())
        })?;
        Ok(Self {
            request,
            profile: profile.clone(),
            accepted: AcceptedState::default(),
        })
    }

    /// The request this service executes.
    pub fn request(&self) -> &WatchRequest {
        &self.request
    }

    /// The accepted revision, if any run committed yet.
    pub fn accepted(&self) -> Option<&AcceptedState> {
        if self.accepted.revision.is_empty() {
            None
        } else {
            Some(&self.accepted)
        }
    }

    /// Commit a staged run outcome.
    pub fn commit(&mut self, staged: AcceptedState) {
        if !staged.revision.is_empty() {
            self.accepted = staged;
        }
    }

    /// Execute one revision: materialize, compile, select, run the
    /// requested stages, and assemble the whole replacement result.
    /// The outcome's `stale` flag is true when the run observed its own
    /// cancellation and must never commit or emit `run-completed`.
    pub fn run_revision(
        &mut self,
        snapshot: &InputSnapshot,
        scratch: &Path,
        context: &RunContext<'_>,
    ) -> RunOutcome {
        let source_revision = snapshot.source_revision();
        let previous_revision =
            (!self.accepted.revision.is_empty()).then(|| self.accepted.revision.clone());
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }
        let files: BTreeMap<String, (String, u64)> = snapshot
            .files
            .iter()
            .map(|(path, record)| (path.clone(), (record.sha256.clone(), record.len)))
            .collect();
        let generated: BTreeMap<String, (String, u64)> = snapshot
            .generated
            .iter()
            .map(|(path, record)| (path.clone(), (record.sha256.clone(), record.len)))
            .collect();
        let baseline = self.accepted.revision.is_empty();
        let previous_snapshot = InputSnapshot {
            files: digest_records(&self.accepted.files),
            generated: digest_records(&self.accepted.generated),
            directories: BTreeSet::new(),
        };
        let empty = InputSnapshot::default();
        let reference = if baseline { &empty } else { &previous_snapshot };
        let changes = diff(reference, snapshot);

        // Materialize the captured bytes and compile over them only.
        if let Err(error) = materialize(snapshot, scratch) {
            return self.capture_refused_outcome(
                context,
                source_revision,
                previous_revision,
                changes,
                error,
                files,
                generated,
            );
        }
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }
        let mut decoded_documents = 0;
        let mut reused_documents = 0;
        let loaded = {
            let mut reuse =
                |path: &str, kind: crate::loader::project_docs::DocKind, bytes: &[u8]| {
                    let digest = format!("sha256:{}", crate::digest::sha256_hex(bytes));
                    match self
                        .accepted
                        .documents
                        .get(path)
                        .filter(|_| !self.request.no_cache)
                    {
                        Some((old_digest, document))
                            if old_digest == &digest
                                && document.path == path
                                && document.kind == kind =>
                        {
                            reused_documents += 1;
                            Some(document.clone())
                        }
                        _ => {
                            decoded_documents += 1;
                            None
                        }
                    }
                };
            crate::loader::load_validated_root_reusing(
                scratch.to_owned(),
                false,
                None,
                Some(&mut reuse),
            )
        };
        let compiled = loaded.and_then(|loaded| {
            let empty = compile::Fragments::default();
            let previous = if self.request.no_cache {
                &empty
            } else {
                &self.accepted.fragments
            };
            let (compilation, fragments, mut counts) =
                compile::Fragments::compile(&loaded.model, previous)
                    .map_err(|failure| failure.into_result())?;
            counts.decoded_documents = decoded_documents;
            counts.reused_documents = reused_documents;
            let documents = loaded
                .documents
                .into_iter()
                .map(|input| {
                    let digest = format!("sha256:{}", crate::digest::sha256_hex(&input.bytes));
                    (input.document.path.clone(), (digest, input.document))
                })
                .collect();
            Ok((loaded.model, compilation, fragments, documents, counts))
        });
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }
        let (model, compilation, fragments, documents, mut pipeline_counts) = match compiled {
            Ok(pair) => pair,
            Err(result) => {
                return self.parse_invalid_outcome(
                    context,
                    source_revision,
                    previous_revision,
                    changes,
                    result.status(),
                    result.diagnostics().to_vec(),
                    files,
                    generated,
                );
            }
        };

        let empty_graph = crate::graph::build::GraphReuse::default();
        let previous_graph = if self.request.no_cache {
            &empty_graph
        } else {
            &self.accepted.graph_fragments
        };
        let graph_result = crate::graph::build::build_reusing(&compilation.project, previous_graph);
        let (graph_result, graph_fragments) = match graph_result {
            Ok((graph, fragments, work)) => {
                pipeline_counts.graph_cycle_passes = 1;
                pipeline_counts.emitted_graph_units = work.emitted_units;
                pipeline_counts.reused_graph_units = work.reused_units;
                pipeline_counts.assembled_graph_nodes = graph.nodes().len();
                pipeline_counts.assembled_graph_edges = graph.edges().len();
                (Ok(graph), fragments)
            }
            Err(set) => (Err(set), crate::graph::build::GraphReuse::default()),
        };
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }
        let module_index = ModuleIndex::from_graph(&compilation, graph_result.as_ref().ok());
        let symbols = symbols_by_path(&compilation);
        let mut affected = compute(&changes, self.accepted_optional_index(), &module_index);
        if context.full_rescan && !baseline {
            affected = AffectedSet::full("recovery");
        }
        if self.request.no_cache {
            affected = AffectedSet::full("no-cache");
        }

        let mut stages: Vec<StageRow> = Vec::new();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut denied = false;
        let mut unsupported_version = false;
        let mut work: Vec<WorkUnitWire> = Vec::new();
        let mut executed_units = 0usize;
        let mut reused_units = 0usize;
        let mut fingerprints: BTreeMap<String, String> = BTreeMap::new();
        let mut partitions: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
        let mut global_partition: Vec<Diagnostic> = Vec::new();
        let mut global_fingerprint: Option<String> = None;
        let model_version: Option<&'static str> = Some(model.model_version.as_str());

        if self.request.stages.validate {
            // Per-module fingerprints decide executed vs reused; a full
            // scope executes everything.
            let modules: BTreeSet<String> = module_index
                .owned_paths
                .keys()
                .chain(module_index.imports.keys())
                .cloned()
                .collect();
            let full = affected.scope == "full";
            let mut selected_modules = BTreeSet::new();
            for module in &modules {
                let fingerprint = module_fingerprint(&module_index, &files, module);
                fingerprints.insert(module.clone(), fingerprint.clone());
                let unchanged = self
                    .accepted
                    .fingerprints
                    .get(module)
                    .is_some_and(|old| *old == fingerprint);
                let in_affected =
                    affected.scope == "selective" && affected.modules.contains(module);
                if !full && !in_affected && unchanged {
                    work.push(WorkUnitWire {
                        module: module.clone(),
                        sha256: fingerprint,
                        state: "reused",
                    });
                    reused_units += 1;
                } else {
                    selected_modules.insert(module.clone());
                    work.push(WorkUnitWire {
                        module: module.clone(),
                        sha256: fingerprint,
                        state: "executed",
                    });
                    executed_units += 1;
                }
            }
            // Selection occurs BEFORE each semantic rule executes; full
            // IR remains available only for target resolution. Recursion
            // is a mandatory, separately retained global obligation.
            let mut fresh_diagnostics = Vec::new();
            let mut visited_symbols = BTreeSet::new();
            for module in selected_modules {
                if context.cancel.load(Ordering::SeqCst) {
                    return stale_outcome(context, source_revision, previous_revision);
                }
                let unit = crate::validator::validate_selected(
                    &compilation,
                    &self.profile,
                    BTreeSet::from([module]),
                    false,
                );
                fresh_diagnostics.extend(unit.diagnostics);
                visited_symbols.extend(unit.visited_symbols);
            }
            let global = crate::validator::validate_selected(
                &compilation,
                &self.profile,
                BTreeSet::new(),
                true,
            );
            fresh_diagnostics.extend(global.diagnostics);
            let visited_modules: BTreeSet<String> = visited_symbols
                .iter()
                .filter_map(|id| id.split('.').next())
                .map(str::to_owned)
                .collect();
            debug_assert!(visited_modules.iter().all(|module| work
                .iter()
                .any(|unit| unit.module == *module && unit.state == "executed")));
            for diagnostic in fresh_diagnostics {
                let owner = if diagnostic.id() == "semantic.type-recursion" {
                    ""
                } else {
                    owner_of_diagnostic(&diagnostic)
                };
                partitions
                    .entry(owner.to_owned())
                    .or_default()
                    .push(diagnostic);
            }
            for unit in &work {
                let partition = if unit.state == "reused" {
                    self.accepted
                        .partitions
                        .get(&unit.module)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    partitions.get(&unit.module).cloned().unwrap_or_default()
                };
                diagnostics.extend(partition.clone());
                partitions.insert(unit.module.clone(), partition);
            }
            // Project-level obligations rerun whenever the source
            // revision changed at all (the whole model feeds subject
            // resolution and authorization).
            let fingerprint = sha256_of(&[
                b"watch-global",
                source_revision.as_bytes(),
                crate::validator::PROFILE_VERSION.as_bytes(),
                WATCH_REGISTRY_VERSION.as_bytes(),
            ]);
            global_fingerprint = Some(fingerprint.clone());
            let global_unchanged = self
                .accepted
                .global_fingerprint
                .as_ref()
                .is_some_and(|old| *old == fingerprint)
                && !baseline;
            global_partition = if global_unchanged {
                self.accepted.global_partition.clone()
            } else {
                partitions.remove("").unwrap_or_default()
            };
            diagnostics.extend(global_partition.clone());
            let (aggregate, aggregate_denied) =
                self.aggregate_validate(scratch, &compilation, &model, diagnostics);
            diagnostics = aggregate;
            denied |= aggregate_denied;
            let reason = if full {
                affected.reason.map(str::to_owned)
            } else {
                None
            };
            stages.push(StageRow {
                reason,
                ..StageRow::executed("validate")
            });
        } else {
            stages.push(StageRow::not_requested("validate"));
        }
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }

        // Impact stage.
        let mut impact = None;
        if self.request.stages.impact {
            let (row, extra, extra_denied, impact_result) = self.impact_stage(
                &changes,
                &symbols,
                &compilation,
                &graph_result,
                &source_revision,
            );
            denied = denied || extra_denied;
            impact = impact_result;
            stages.push(row);
            diagnostics.extend(extra);
        } else {
            stages.push(StageRow::not_requested("impact"));
        }
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }

        // Generate-check drift stage (read-only manifest check over the
        // captured generated tree).
        let mut staged_drift = self.accepted.drift.clone();
        let mut staged_drift_fingerprint = self.accepted.drift_fingerprint.clone();
        if self.request.stages.generate_check {
            let fingerprint = sha256_of(&[
                b"watch-drift",
                source_revision.as_bytes(),
                snapshot.generated_digest().as_bytes(),
            ]);
            if self
                .accepted
                .drift_fingerprint
                .as_ref()
                .is_some_and(|old| *old == fingerprint)
                && self.accepted.drift.is_some()
                && !self.request.no_cache
            {
                stages.push(StageRow {
                    stage: "generate-check",
                    state: "reused",
                    coverage: "complete",
                    reason: None,
                });
            } else {
                match crate::artifacts::GenerateService::check_captured(
                    scratch,
                    &model,
                    &compilation,
                ) {
                    Ok(receipt) => {
                        staged_drift = Some(DriftSummary::of(&receipt));
                        staged_drift_fingerprint = Some(fingerprint);
                        stages.push(StageRow::executed("generate-check"));
                    }
                    Err(failure) => {
                        let reason: &'static str = match &failure {
                            crate::artifacts::ArtifactFailure::Loader(_) => "loader-refused",
                            crate::artifacts::ArtifactFailure::Lock(_) => "lock-refused",
                            _ => "artifact-check-refused",
                        };
                        // A failed current check must never publish the
                        // previous revision's successful drift receipt.
                        staged_drift = None;
                        staged_drift_fingerprint = None;
                        let result = crate::DomainResult::from(&failure);
                        diagnostics.extend(result.diagnostics().to_vec());
                        denied |= result.status() == Status::Denied;
                        unsupported_version |= result.status() == Status::UnsupportedVersion;
                        stages.push(StageRow::unavailable("generate-check", reason, "partial"));
                    }
                }
            }
        } else {
            stages.push(StageRow::not_requested("generate-check"));
        }
        if context.cancel.load(Ordering::SeqCst) {
            return stale_outcome(context, source_revision, previous_revision);
        }

        // Lock preflight (--locked) and the explicit target stage.
        if self.request.locked {
            match crate::orchestration::locked_check_captured(scratch, &model, &compilation) {
                Ok(()) => stages.push(StageRow::executed("lock-preflight")),
                Err(result) => {
                    diagnostics.extend(result.diagnostics().to_vec());
                    unsupported_version |= result.status() == Status::UnsupportedVersion;
                    if matches!(result, crate::result::DomainResult::Denied { .. }) {
                        denied = true;
                    }
                    stages.push(StageRow::unavailable(
                        "lock-preflight",
                        "lock-refused",
                        "partial",
                    ));
                }
            }
        }
        if !self.request.targets.is_empty() {
            if !snapshot.files.contains_key("lekalo.lock") {
                diagnostics.push(crate::watch::diagnostics::selective_unavailable(
                    "target-scan",
                    "lock-missing",
                ));
                stages.push(StageRow::unavailable(
                    "target-scan",
                    "lock-missing",
                    "unsupported",
                ));
            } else {
                // TODO(#95, required increment 4): implement negotiated,
                // ephemeral snapshot-bound scan/validation. The accepted
                // scan service persists state and must not be launched
                // here. Until that seam is implemented and tested, this
                // component is explicitly unavailable, not completed.
                diagnostics.push(crate::watch::diagnostics::selective_unavailable(
                    "target-scan",
                    "read-only-scan",
                ));
                stages.push(StageRow::unavailable(
                    "target-scan",
                    "read-only-scan",
                    "unsupported",
                ));
            }
        }

        // Conservative dependency-linked invalidation includes both old
        // and new symbols of every affected/reverse-imported module.
        // A limit is an observable refusal, never silent truncation.
        let mut capsule_set = BTreeSet::new();
        for row in changes.semantic_rows() {
            for index in [&self.accepted.symbols_by_path, &symbols] {
                if let Some(owned) = index.get(&row.path) {
                    capsule_set.extend(owned.iter().filter(|id| id.contains('.')).cloned());
                }
            }
        }
        for index in [&self.accepted.symbols_by_path, &symbols] {
            capsule_set.extend(
                index
                    .values()
                    .flatten()
                    .filter(|id| {
                        id.contains('.')
                            && (affected.scope == "full"
                                || affected
                                    .modules
                                    .contains(id.split('.').next().unwrap_or("")))
                    })
                    .cloned(),
            );
        }
        let capsule = if capsule_set.len() > 128 {
            diagnostics.push(crate::watch::diagnostics::resource_limit(
                "capsule-invalidations",
                capsule_set.len() as u64,
            ));
            denied = true;
            Vec::new()
        } else {
            capsule_set.into_iter().collect()
        };

        // Module scope retention: module-owned diagnostics plus every
        // error anywhere (the accepted `validate --module` semantics).
        if let Some(scope) = &self.request.module {
            diagnostics.retain(|diagnostic| {
                diagnostic.severity() == crate::diagnostics::types::Severity::Error
                    || owner_of_diagnostic(diagnostic) == scope
            });
        }
        let diagnostics = {
            let status = status_of(&diagnostics, denied);
            let status = if unsupported_version && status != Status::Denied {
                Status::UnsupportedVersion
            } else {
                status
            };
            normalize_diagnostics(diagnostics, status)
        };
        let status = status_of(&diagnostics, denied);
        let status = if unsupported_version && status != Status::Denied {
            Status::UnsupportedVersion
        } else {
            status
        };
        let status =
            if status == Status::Valid && stages.iter().any(|stage| stage.state == "unavailable") {
                Status::Unavailable
            } else {
                status
            };
        let result = WatchResult {
            schema_version: RESULT_SCHEMA_VERSION,
            identity: RESULT_IDENTITY,
            status,
            diagnostics_mode: "replace",
            source_revision: source_revision.clone(),
            previous_source_revision: previous_revision.clone(),
            diagnostics: diagnostics.clone(),
            stages,
            changed_inputs: changes
                .rows
                .iter()
                .map(|row| ChangedRowWire {
                    path: row.path.clone(),
                    kind: row.kind.as_str(),
                    old_sha256: row.old_sha256.clone(),
                    new_sha256: row.new_sha256.clone(),
                    rename_of: row.rename_of.clone(),
                    class: row.class,
                })
                .collect(),
            affected: AffectedWire {
                scope: affected.scope,
                reason: affected.reason,
                modules: affected.modules.iter().cloned().collect(),
            },
            work: work.clone(),
            counts: ResultCounts {
                diagnostics: diagnostics.len(),
                executed_units,
                reused_units,
                changed_files: changes.len(),
            },
            capsule_invalidations: capsule,
            impact,
            drift: staged_drift.clone(),
            pipeline_counts: Some(pipeline_counts),
            pins: ResultPins {
                model_version,
                registry_version: WATCH_REGISTRY_VERSION,
            },
        };
        RunOutcome {
            run_id: context.run_id,
            stale: false,
            source_revision: source_revision.clone(),
            status,
            result,
            staged: AcceptedState {
                revision: source_revision,
                files,
                generated,
                module_index,
                symbols_by_path: symbols,
                fingerprints,
                partitions,
                global_partition,
                global_fingerprint,
                drift_fingerprint: staged_drift_fingerprint,
                drift: staged_drift,
                documents,
                fragments,
                graph_fragments,
            },
        }
    }

    fn accepted_optional_index(&self) -> Option<&ModuleIndex> {
        if self.accepted.revision.is_empty() {
            None
        } else {
            Some(&self.accepted.module_index)
        }
    }

    /// The full `lekalo validate` aggregate over the captured tree:
    /// the semantic validator plus the classification, authorization,
    /// and transport reviews, exactly as the clean command runs them.
    /// Returns the diagnostic set and whether the aggregate denied.
    fn aggregate_validate(
        &self,
        root: &Path,
        compilation: &crate::ir::Compilation,
        model: &NormalizedModel,
        mut diagnostics: Vec<Diagnostic>,
    ) -> (Vec<Diagnostic>, bool) {
        if diagnostics
            .iter()
            .any(|item| item.severity() == crate::diagnostics::types::Severity::Error)
        {
            return (diagnostics, false);
        }
        match crate::classification::discover(root) {
            Err(set) => return (set.as_slice().to_vec(), false),
            Ok(Some((attachment, policy))) => {
                let model_json = &crate::loader::canonical_model_bytes(model);
                if let Err(set) = crate::classification::validate_custody(
                    &attachment,
                    &policy,
                    &compilation.project,
                    model_json,
                ) {
                    return (set.as_slice().to_vec(), false);
                }
                match crate::classification::validate_subjects(&attachment, compilation) {
                    Err(set) => return (set.as_slice().to_vec(), false),
                    Ok(resolution) => {
                        match crate::classification::validate_policy_and_grants(
                            &attachment,
                            &policy,
                            &resolution,
                            &compilation.project,
                        ) {
                            Err(set) => return (set.as_slice().to_vec(), false),
                            Ok(review) => {
                                if self.request.strict && review.invalid {
                                    return (
                                        crate::classification::findings_set(&review)
                                            .as_slice()
                                            .to_vec(),
                                        false,
                                    );
                                }
                                diagnostics.extend(
                                    crate::classification::findings_set(&review)
                                        .as_slice()
                                        .to_vec(),
                                );
                            }
                        }
                    }
                }
            }
            Ok(None) => {}
        }
        let mut denied = false;
        let authorization = crate::authorization::parse::read_document(root).map(|document| {
            crate::authorization::review::review(
                compilation,
                document.as_ref(),
                self.request.strict,
            )
        });
        match authorization {
            Ok(crate::authorization::Review::Ok) => {}
            Ok(crate::authorization::Review::Invalid(set)) => {
                diagnostics.extend(set.as_slice().to_vec());
            }
            Ok(crate::authorization::Review::Denied(set)) => {
                diagnostics.extend(set.as_slice().to_vec());
                denied = true;
            }
            Err(set) => return (set.as_slice().to_vec(), false),
        }
        match crate::transport_http::read_document(root) {
            Err(set) => diagnostics.extend(set.as_slice().to_vec()),
            Ok(Some(attachment)) => {
                let context = crate::transport_http::ValidationContext::new(&compilation.project);
                if let Err(set) = crate::transport_http::validate(&attachment, &context) {
                    diagnostics.extend(set.as_slice().to_vec());
                }
                if let Err(set) = crate::openapi::render(
                    &attachment,
                    &context,
                    &crate::openapi::RenderConfig::new(),
                ) {
                    diagnostics.extend(set.as_slice().to_vec());
                }
            }
            Ok(None) => {}
        }
        (diagnostics, denied)
    }

    /// One impact stage pass. Returns the stage row, the extra domain
    /// diagnostics, and whether the stage denied.
    #[allow(clippy::type_complexity)]
    fn impact_stage(
        &self,
        changes: &ChangeBatch,
        symbols: &BTreeMap<String, Vec<String>>,
        compilation: &crate::ir::Compilation,
        graph_result: &Result<crate::graph::DependencyGraph, crate::diagnostics::DiagnosticSet>,
        candidate_revision: &str,
    ) -> (StageRow, Vec<Diagnostic>, bool, Option<serde_json::Value>) {
        use crate::impact::input::{
            ChangedInput, ChangedInputSet, ChangedMode, EntryEvidence, FileChange,
        };
        let mut entries = Vec::new();
        for row in changes.semantic_rows() {
            let symbols_for_path = match row.kind {
                ChangeKind::Deleted | ChangeKind::Modified => self
                    .accepted
                    .symbols_by_path
                    .get(&row.path)
                    .cloned()
                    .or_else(|| symbols.get(&row.path).cloned())
                    .unwrap_or_default(),
                ChangeKind::Created => symbols.get(&row.path).cloned().unwrap_or_default(),
            };
            let file_change = match row.kind {
                ChangeKind::Created => FileChange::Added,
                ChangeKind::Modified => FileChange::Modified,
                ChangeKind::Deleted => FileChange::Deleted,
            };
            let file_change = if row.rename_of.is_some() {
                FileChange::Renamed
            } else {
                file_change
            };
            let evidence = if symbols_for_path.is_empty() {
                EntryEvidence::Unknown
            } else {
                EntryEvidence::Canonical
            };
            match ChangedInput::new(
                symbols_for_path,
                Vec::new(),
                Some((row.path.as_str(), row.rename_of.as_deref())),
                file_change,
                evidence,
            ) {
                Ok(entry) => entries.push(entry),
                Err(set) => {
                    return (
                        StageRow::unavailable("impact", "changed-input-invalid", "partial"),
                        set.as_slice().to_vec(),
                        false,
                        None,
                    )
                }
            }
        }
        if entries.is_empty() {
            return (StageRow::executed("impact"), Vec::new(), false, None);
        }
        let set = ChangedInputSet::from_entries(
            ChangedMode::Worktree,
            self.accepted
                .revision
                .strip_prefix("sha256:")
                .map(str::to_owned),
            Some(
                candidate_revision
                    .strip_prefix("sha256:")
                    .unwrap_or(candidate_revision)
                    .to_owned(),
            ),
            entries,
        );
        let set = match set {
            Ok(set) => set,
            Err(set) => {
                return (
                    StageRow::unavailable("impact", "changed-set-invalid", "partial"),
                    set.as_slice().to_vec(),
                    false,
                    None,
                )
            }
        };
        let graph = match graph_result {
            Ok(graph) => graph,
            Err(set) => {
                return (
                    StageRow::unavailable("impact", "graph-refused", "partial"),
                    set.as_slice().to_vec(),
                    false,
                    None,
                )
            }
        };
        let effects = match crate::effects::build(&compilation.project) {
            Ok(effects) => effects,
            Err(set) => {
                return (
                    StageRow::unavailable("impact", "effects-refused", "partial"),
                    set.as_slice().to_vec(),
                    false,
                    None,
                )
            }
        };
        let request = crate::impact::ImpactRequest::for_changed();
        match crate::impact::analyze(
            &compilation.project,
            graph,
            &effects,
            &request,
            Some(&set),
            None,
        ) {
            Ok(result) => match result.to_canonical_json() {
                Ok(json) => (
                    StageRow::executed("impact"),
                    Vec::new(),
                    false,
                    Some(serde_json::from_str(&json).expect("impact canonical JSON")),
                ),
                Err(set) => (
                    StageRow::unavailable("impact", "impact-limit", "partial"),
                    set.as_slice().to_vec(),
                    true,
                    None,
                ),
            },
            Err(crate::impact::ImpactFailure::Invalid(set)) => (
                StageRow::unavailable("impact", "impact-refused", "partial"),
                set.as_slice().to_vec(),
                false,
                None,
            ),
            Err(crate::impact::ImpactFailure::Denied(set)) => (
                StageRow::unavailable("impact", "impact-denied", "partial"),
                set.as_slice().to_vec(),
                true,
                None,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_refused_outcome(
        &self,
        context: &RunContext<'_>,
        source_revision: String,
        previous_revision: Option<String>,
        changes: ChangeBatch,
        error: CaptureError,
        files: BTreeMap<String, (String, u64)>,
        generated: BTreeMap<String, (String, u64)>,
    ) -> RunOutcome {
        let (reason, diagnostic, denied) = match &error {
            CaptureError::Limit { limit, count } => (
                "capture-limit",
                crate::watch::diagnostics::resource_limit(limit, *count),
                true,
            ),
            CaptureError::Refused { code, .. } => (
                "capture-refused",
                crate::watch::diagnostics::selective_unavailable("validate", code),
                false,
            ),
            _ => (
                "capture-failed",
                crate::watch::diagnostics::selective_unavailable("validate", "capture-failed"),
                false,
            ),
        };
        let _ = (context, diagnostic, denied);
        let failure = error.domain_result();
        let diagnostics = failure.diagnostics().to_vec();
        let status = failure.status();
        let stages = vec![
            StageRow::unavailable("validate", reason, "partial"),
            StageRow::not_requested("impact"),
            StageRow::not_requested("generate-check"),
        ];
        let result = WatchResult {
            schema_version: RESULT_SCHEMA_VERSION,
            identity: RESULT_IDENTITY,
            status,
            diagnostics_mode: "replace",
            source_revision: source_revision.clone(),
            previous_source_revision: previous_revision,
            diagnostics: diagnostics.clone(),
            stages,
            changed_inputs: changed_wire(&changes),
            affected: AffectedWire {
                scope: "full",
                reason: Some("recovery"),
                modules: Vec::new(),
            },
            work: Vec::new(),
            counts: ResultCounts {
                diagnostics: diagnostics.len(),
                executed_units: 0,
                reused_units: 0,
                changed_files: changes.len(),
            },
            capsule_invalidations: Vec::new(),
            impact: None,
            drift: None,
            pipeline_counts: None,
            pins: ResultPins {
                model_version: None,
                registry_version: WATCH_REGISTRY_VERSION,
            },
        };
        RunOutcome {
            run_id: context.run_id,
            stale: false,
            source_revision,
            status,
            result,
            staged: AcceptedState {
                revision: String::new(),
                files,
                generated,
                ..AcceptedState::default()
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_invalid_outcome(
        &self,
        context: &RunContext<'_>,
        source_revision: String,
        previous_revision: Option<String>,
        changes: ChangeBatch,
        input_status: Status,
        mut diagnostics: Vec<Diagnostic>,
        files: BTreeMap<String, (String, u64)>,
        generated: BTreeMap<String, (String, u64)>,
    ) -> RunOutcome {
        let stages = [
            ("validate", self.request.stages.validate),
            ("impact", self.request.stages.impact),
            ("generate-check", self.request.stages.generate_check),
        ]
        .into_iter()
        .map(|(stage, selected)| {
            if selected {
                StageRow::unavailable(stage, "parse-invalid", "partial")
            } else {
                StageRow::not_requested(stage)
            }
        })
        .collect();
        // Broken current documents cannot erase still-effective findings
        // from other accepted units. The loader owns the terminal status.
        diagnostics.extend(self.accepted.partitions.values().flatten().cloned());
        diagnostics.extend(self.accepted.global_partition.iter().cloned());
        let diagnostics = normalize_diagnostics(diagnostics, input_status);
        let status = input_status;
        let result = WatchResult {
            schema_version: RESULT_SCHEMA_VERSION,
            identity: RESULT_IDENTITY,
            status,
            diagnostics_mode: "replace",
            source_revision: source_revision.clone(),
            previous_source_revision: previous_revision,
            diagnostics: diagnostics.clone(),
            stages,
            changed_inputs: changed_wire(&changes),
            affected: AffectedWire {
                scope: "full",
                reason: Some("parse-invalid"),
                modules: Vec::new(),
            },
            work: Vec::new(),
            counts: ResultCounts {
                diagnostics: diagnostics.len(),
                executed_units: 0,
                reused_units: 0,
                changed_files: changes.len(),
            },
            capsule_invalidations: Vec::new(),
            impact: None,
            drift: None,
            pipeline_counts: None,
            pins: ResultPins {
                model_version: None,
                registry_version: WATCH_REGISTRY_VERSION,
            },
        };
        RunOutcome {
            run_id: context.run_id,
            stale: false,
            source_revision,
            status,
            result,
            // A parse-invalid revision never becomes the accepted
            // semantic baseline; the previous accepted state stays.
            staged: AcceptedState {
                revision: String::new(),
                files,
                generated,
                ..AcceptedState::default()
            },
        }
    }
}

fn changed_wire(changes: &ChangeBatch) -> Vec<ChangedRowWire> {
    changes
        .rows
        .iter()
        .map(|row| ChangedRowWire {
            path: row.path.clone(),
            kind: row.kind.as_str(),
            old_sha256: row.old_sha256.clone(),
            new_sha256: row.new_sha256.clone(),
            rename_of: row.rename_of.clone(),
            class: row.class,
        })
        .collect()
}

fn stale_outcome(
    context: &RunContext<'_>,
    source_revision: String,
    previous_revision: Option<String>,
) -> RunOutcome {
    RunOutcome {
        run_id: context.run_id,
        stale: true,
        source_revision,
        status: Status::Valid,
        result: WatchResult {
            schema_version: RESULT_SCHEMA_VERSION,
            identity: RESULT_IDENTITY,
            status: Status::Valid,
            diagnostics_mode: "replace",
            source_revision: String::new(),
            previous_source_revision: previous_revision,
            diagnostics: Vec::new(),
            stages: Vec::new(),
            changed_inputs: Vec::new(),
            affected: AffectedWire {
                scope: "full",
                reason: None,
                modules: Vec::new(),
            },
            work: Vec::new(),
            counts: ResultCounts {
                diagnostics: 0,
                executed_units: 0,
                reused_units: 0,
                changed_files: 0,
            },
            capsule_invalidations: Vec::new(),
            impact: None,
            drift: None,
            pipeline_counts: None,
            pins: ResultPins {
                model_version: None,
                registry_version: WATCH_REGISTRY_VERSION,
            },
        },
        staged: AcceptedState::default(),
    }
}

/// Structural validation of one decoded result object (the closed
/// `watch-result` shape as the Rust decoder checks it): consts, enums,
/// and bounded members. The full schema authority is the Ajv gate.
pub fn result_value_is_wellformed(value: &serde_json::Value) -> bool {
    static VALIDATOR: std::sync::LazyLock<jsonschema::Validator> = std::sync::LazyLock::new(|| {
        let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../contracts/watch-result.schema.v0.8.0.json"
        ))
        .expect("watch result schema JSON");
        jsonschema::validator_for(&schema).expect("offline watch result schema")
    });
    if !VALIDATOR.is_valid(value) {
        return false;
    }
    let counts = &value["counts"];
    if events::counter(&counts["diagnostics"])
        != value["diagnostics"]
            .as_array()
            .map(|rows| rows.len() as u64)
        || events::counter(&counts["changedFiles"])
            != value["changedInputs"]
                .as_array()
                .map(|rows| rows.len() as u64)
        || events::counter(&counts["executedUnits"])
            != value["work"]
                .as_array()
                .map(|rows| rows.iter().filter(|row| row["state"] == "executed").count() as u64)
        || events::counter(&counts["reusedUnits"])
            != value["work"]
                .as_array()
                .map(|rows| rows.iter().filter(|row| row["state"] == "reused").count() as u64)
    {
        return false;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    for key in object.keys() {
        if ![
            "schemaVersion",
            "identity",
            "status",
            "diagnosticsMode",
            "sourceRevision",
            "previousSourceRevision",
            "diagnostics",
            "stages",
            "changedInputs",
            "affected",
            "work",
            "counts",
            "capsuleInvalidations",
            "pins",
            "impact",
            "drift",
            "pipelineCounts",
        ]
        .contains(&key.as_str())
        {
            return false;
        }
    }
    object.get("schemaVersion").and_then(|value| value.as_str()) == Some(RESULT_SCHEMA_VERSION)
        && object.get("identity").and_then(|value| value.as_str()) == Some(RESULT_IDENTITY)
        && object
            .get("diagnosticsMode")
            .and_then(|value| value.as_str())
            == Some("replace")
        && object
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| {
                [
                    "valid",
                    "invalid",
                    "denied",
                    "unsupported",
                    "unavailable",
                    "unsupported-version",
                ]
                .contains(&status)
            })
        && object
            .get("sourceRevision")
            .and_then(|value| value.as_str())
            .is_some_and(|revision| revision.starts_with("sha256:") && revision.len() == 7 + 64)
        && object
            .get("diagnostics")
            .and_then(|value| value.as_array())
            .is_some_and(|rows| rows.len() <= 512)
        && object
            .get("stages")
            .and_then(|value| value.as_array())
            .is_some_and(|rows| rows.len() <= 16)
        && object
            .get("changedInputs")
            .and_then(|value| value.as_array())
            .is_some_and(|rows| rows.len() <= 4096)
        && object
            .get("work")
            .and_then(|value| value.as_array())
            .is_some_and(|rows| rows.len() <= 4096)
        && object
            .get("capsuleInvalidations")
            .and_then(|value| value.as_array())
            .is_some_and(|rows| rows.len() <= 128)
        && object
            .get("pins")
            .and_then(|value| value.as_object())
            .is_some()
        && object
            .get("counts")
            .and_then(|value| value.as_object())
            .is_some()
}
