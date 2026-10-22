//! Read-only scoped analysis (issue #136, increment I1): one selected
//! use case, module, or symbol becomes a minimal reviewable starter plan
//! with per-fact provenance and confidence, explicit unknown and
//! ambiguous facts, and the exact input pins — without a single write to
//! application sources, the canonical Model, or any cache/report home.
//!
//! The analysis composes the accepted seams: the adapter-driven observed
//! index, the canonical Model, the lock state, and the declared
//! target/profile. It is derived evidence, never ownership authority.

use std::collections::BTreeSet;

use super::diagnostic;
use super::version;
use super::wire::{
    self, Ambiguity, AnalysisBinding, AnalysisContract, AnalysisDefinition, AnalysisDocument,
    Discrepancy, Gap, ModuleProposal, Pins, Scope, SourcePin,
};
use crate::observed::types::{BindingState, BindingStatus, ObservedIndex, SymbolRecord};
use crate::result::DomainResult;

/// One loaded project context for the adoption seam.
pub struct AdoptionContext {
    pub model: crate::loader::NormalizedModel,
    pub root: std::path::PathBuf,
}

/// Load one project selection for the adoption seam through the accepted
/// loader snapshot.
pub fn context(selection: &crate::loader::LoadSelection) -> Result<AdoptionContext, DomainResult> {
    crate::loader::load_with_snapshot(selection).map(|loaded| AdoptionContext {
        model: loaded.model,
        root: loaded.root,
    })
}

/// Resolve the project root WITHOUT a Model load, for the recovery
/// surfaces (`adoption recovery` view and `adoption recovery --restore`).
/// The r8 reader barrier fails Model loads closed while an installation
/// journal is stranded — the very state these surfaces exist to resolve
/// — so they must not depend on a full load. Every structure boundary
/// (selection grammar, root discovery, directory containment) stays
/// armed exactly as in [`context`]; only the Model read is deferred, and
/// the recovery path itself authenticates its journal before any effect.
pub fn recovery_root(
    selection: &crate::loader::LoadSelection,
) -> Result<std::path::PathBuf, DomainResult> {
    crate::loader::root_for_selection(selection)
}

/// The outcome type of every adoption service.
pub type Outcome<T> = Result<T, DomainResult>;

/// The requested analysis scope: exactly one of the three selectors.
#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisScope {
    /// One stable observed symbol id (`taskboard.complete_task`).
    Symbol(String),
    /// One exact module id, without wildcards.
    Module(String),
    /// One explicitly reviewed use-case scope record.
    UseCase(String),
}

impl AnalysisScope {
    /// The closed wire scope of the produced documents.
    pub fn wire(&self) -> Scope {
        let (kind, id) = match self {
            Self::Symbol(id) => ("symbol", id),
            Self::Module(id) => ("module", id),
            Self::UseCase(id) => ("use-case", id),
        };
        Scope {
            kind: kind.to_owned(),
            id: id.to_owned(),
        }
    }
}

/// One authored use-case scope record: an explicitly reviewed linkage of
/// one use-case id to its root symbol. Closed member set.
#[derive(Clone, Debug, PartialEq)]
pub struct UseCaseRecord {
    pub id: String,
    pub symbol: String,
}

/// Load and validate one use-case scope record.
pub fn use_case_record(root: &std::path::Path, id: &str) -> Outcome<UseCaseRecord> {
    if !version::is_scope_id(id) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "use-case",
            "invalid-id",
        )));
    }
    let path = root.join(version::USE_CASES_DIR).join(format!("{id}.json"));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let candidates = available_use_cases(root)?;
            return Err(DomainResult::denied_json(
                serde_json::json!({
                    "scope": "use-case",
                    "requested": id,
                    "candidates": candidates,
                })
                .to_string(),
                format!(
                    "adoption use-case {id} is unknown; define {} first",
                    use_case_path(id)
                ),
                diagnostic::scope_refused_set(id, "use-case-unknown"),
            ));
        }
    };
    let value = wire::unique_json(&bytes).map_err(|error| DomainResult::invalid(error.set()))?;
    let object = value.as_object().ok_or_else(|| {
        DomainResult::invalid(diagnostic::input_invalid_set("use-case", "closed-shape"))
    })?;
    let keys: Vec<&String> = object.keys().collect();
    if keys != ["id", "symbol"] {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "use-case",
            "closed-shape",
        )));
    }
    let record_id = object["id"]
        .as_str()
        .ok_or_else(|| DomainResult::invalid(diagnostic::input_invalid_set("use-case", "id")))?;
    let symbol = object["symbol"].as_str().ok_or_else(|| {
        DomainResult::invalid(diagnostic::input_invalid_set("use-case", "symbol"))
    })?;
    if record_id != id || !version::is_scope_id(record_id) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "use-case",
            "id-mismatch",
        )));
    }
    if !version::is_semantic_id(symbol) {
        return Err(DomainResult::invalid(diagnostic::input_invalid_set(
            "use-case", "symbol",
        )));
    }
    Ok(UseCaseRecord {
        id: record_id.to_owned(),
        symbol: symbol.to_owned(),
    })
}

fn available_use_cases(root: &std::path::Path) -> Outcome<Vec<String>> {
    let mut candidates = Vec::new();
    let dir = root.join(version::USE_CASES_DIR);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if let Some(id) = name.strip_suffix(".json") {
                candidates.push(id.to_owned());
            }
        }
    }
    candidates.sort();
    Ok(candidates)
}

fn use_case_path(id: &str) -> String {
    format!("{}/{}.json", version::USE_CASES_DIR, id)
}

/// Resolve the scope into its root symbol and the full in-scope record
/// set. Module scope is exact; a use case binds exactly one root symbol.
pub(crate) fn resolve_scope(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    scope: &AnalysisScope,
) -> Outcome<(String, Vec<String>)> {
    match scope {
        AnalysisScope::Symbol(symbol) => {
            if !version::is_semantic_id(symbol) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "symbol",
                    "invalid-id",
                )));
            }
            let known = index.is_some_and(|index| index.symbol(symbol).is_some());
            let canonical = ctx
                .model
                .definitions
                .iter()
                .any(|definition| definition.id == *symbol);
            if !known && !canonical {
                return Err(DomainResult::denied_json(
                    serde_json::json!({ "scope": "symbol", "requested": symbol }).to_string(),
                    format!("adoption symbol {symbol} is unknown"),
                    diagnostic::scope_refused_set(symbol, "symbol-unknown"),
                ));
            }
            Ok((symbol.clone(), vec![symbol.clone()]))
        }
        AnalysisScope::Module(module) => {
            if !version::is_module_id(module) {
                return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                    "module",
                    "invalid-id",
                )));
            }
            let mut ids: Vec<String> = index
                .map(|index| {
                    index
                        .symbols
                        .iter()
                        .filter(|record| {
                            ObservedIndex::module_of(&record.id) == Some(module.as_str())
                        })
                        .map(|record| record.id.clone())
                        .collect()
                })
                .unwrap_or_default();
            for definition in &ctx.model.definitions {
                if crate::observed::types::ObservedIndex::module_of(&definition.id)
                    == Some(module.as_str())
                    && !ids.contains(&definition.id)
                {
                    ids.push(definition.id.clone());
                }
            }
            if ids.is_empty() {
                return Err(DomainResult::denied_json(
                    serde_json::json!({ "scope": "module", "requested": module }).to_string(),
                    format!("adoption module {module} has no observed or canonical symbols"),
                    diagnostic::scope_refused_set(module, "module-empty"),
                ));
            }
            ids.sort();
            let root = ids.first().cloned().unwrap_or_default();
            Ok((root, ids))
        }
        AnalysisScope::UseCase(id) => {
            let record = use_case_record(&ctx.root, id)?;
            Ok((record.symbol.clone(), vec![record.symbol]))
        }
    }
}

/// Build the exact pin set of one analysis or plan: the canonical Model
/// bytes, the lock bytes (absent is a typed `None`), the observed index
/// digest, the adapter identity, and the hashed source files.
pub(crate) fn compute_pins(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    source_paths: &BTreeSet<String>,
) -> Outcome<Pins> {
    let model_bytes = crate::loader::canonical_model_bytes(&ctx.model);
    let model = format!(
        "sha256:{}",
        crate::digest::sha256_hex(model_bytes.as_bytes())
    );
    let lock = match crate::lockfile::plan::LockService::read_state_at(&ctx.root) {
        Ok(crate::lockfile::LockState::Present(lock)) => Some(
            crate::lockfile::types::Lockfile::digest(&lock)
                .as_str()
                .to_owned(),
        ),
        Ok(crate::lockfile::LockState::Absent) => None,
        Err(failure) => return Err(DomainResult::from(&failure)),
    };
    let observed_index = match index {
        Some(index) => {
            Some(crate::observed::index::index_digest(index).map_err(DomainResult::invalid)?)
        }
        None => None,
    };
    let adapter = index.as_ref().map(|index| super::wire::AdapterPin {
        id: index.adapter.id.clone(),
        version: index.adapter.version.clone(),
        digest: index.adapter.digest.clone(),
    });
    let mut sources = Vec::new();
    for path in source_paths.iter().take(version::MAX_SOURCE_FILES) {
        let bytes = std::fs::read(ctx.root.join(path));
        if let Ok(bytes) = bytes {
            sources.push(SourcePin {
                path: path.clone(),
                sha256: format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
            });
        }
    }
    Ok(Pins {
        model,
        lock,
        observed_index,
        adapter,
        sources,
    })
}

/// The source files one scope's evidence names, plus the whole source
/// directory of the scope's module when a native root is recorded.
pub(crate) fn scope_source_paths(
    index: Option<&ObservedIndex>,
    records: &[SymbolRecord],
) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for record in records {
        if let Some(location) = &record.location {
            paths.insert(location.path.clone());
        }
        for candidate in &record.candidates {
            paths.insert(candidate.path.clone());
        }
    }
    if let Some(index) = index {
        for endpoint in &index.endpoints {
            paths.insert(endpoint.path.clone());
        }
        for binding in &index.test_bindings {
            paths.insert(binding.path.clone());
        }
    }
    paths.retain(|path| version::is_rel_path(path));
    paths
}

/// Produce the read-only analysis document for exactly one scope.
pub fn analyze(
    ctx: &AdoptionContext,
    index: Option<&ObservedIndex>,
    scope: &AnalysisScope,
    target: Option<&str>,
    profile: Option<&str>,
) -> Outcome<AnalysisDocument> {
    // A declared target/profile must match the producing scan exactly;
    // the analysis never scans, so a mismatch means stale evidence.
    if let (Some(index), Some(target)) = (index, target) {
        if index.target.as_deref() != Some(target) {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "target",
                "target-mismatch",
            )));
        }
    }
    if let (Some(index), Some(profile)) = (index, profile) {
        if index.profile.as_deref() != Some(profile) {
            return Err(DomainResult::invalid(diagnostic::input_invalid_set(
                "profile",
                "profile-mismatch",
            )));
        }
    }
    let (root_symbol, ids) = resolve_scope(ctx, index, scope)?;
    let records: Vec<SymbolRecord> = ids
        .iter()
        .filter_map(|id| index.and_then(|index| index.symbol(id)).cloned())
        .collect();
    let canonical_ids: BTreeSet<&str> = ctx
        .model
        .definitions
        .iter()
        .map(|definition| definition.id.as_str())
        .collect();

    // Module proposal: the scope's module; proposed when the canonical
    // skeleton does not declare it yet.
    let module_id = ObservedIndex::module_of(&root_symbol)
        .unwrap_or_default()
        .to_owned();
    let proposed = !ctx
        .model
        .modules
        .iter()
        .any(|module| module.id == module_id);
    let native_root = records
        .first()
        .and_then(|record| record.location.as_ref())
        .map(|location| {
            location
                .path
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_owned())
                .unwrap_or_else(|| location.path.clone())
        });
    let module = ModuleProposal {
        id: module_id,
        proposed,
        native_root,
    };

    let mut definitions = Vec::new();
    let mut bindings = Vec::new();
    let mut contracts = Vec::new();
    let mut unknowns: Vec<Gap> = Vec::new();
    let mut ambiguities = Vec::new();
    let mut discrepancies = Vec::new();
    if index.is_none() {
        unknowns.push(Gap {
            subject: "observed-index".to_owned(),
            detail: "no-scan-evidence".to_owned(),
        });
    }
    for record in &records {
        let status = if canonical_ids.contains(record.id.as_str()) {
            "confirmed"
        } else {
            "proposed"
        };
        let confidence = confidence_of(record).to_owned();
        definitions.push(AnalysisDefinition {
            id: record.id.clone(),
            kind: record.kind.key().to_owned(),
            status: status.to_owned(),
            confidence: confidence.to_owned(),
            provenance: super::wire::Provenance {
                source: "observed-index".to_owned(),
                native: record.stable_key.clone(),
                path: record.location.as_ref().map(|l| l.path.clone()),
                line: record.location.as_ref().and_then(|l| l.line),
            },
        });
        if record.status == BindingStatus::Inferred {
            unknowns.push(Gap {
                subject: record.id.clone(),
                detail: "binding-inferred".to_owned(),
            });
        }
        if record.state == BindingState::Stale {
            discrepancies.push(Discrepancy {
                symbol: record.id.clone(),
                kind: "binding".to_owned(),
                detail: "binding-stale".to_owned(),
            });
        }
        if record.candidates.len() > 1 {
            ambiguities.push(Ambiguity {
                symbol: record.id.clone(),
                candidates: record
                    .candidates
                    .iter()
                    .map(|candidate| candidate.native.clone())
                    .take(version::MAX_CANDIDATES)
                    .collect(),
            });
        }
        if canonical_ids.contains(record.id.as_str()) && record.state == BindingState::Stale {
            discrepancies.push(Discrepancy {
                symbol: record.id.clone(),
                kind: "evidence".to_owned(),
                detail: "confirmed-definition-stale-evidence".to_owned(),
            });
        }
        // Re-fingerprint the recorded location against the real bytes:
        // drifted evidence is reported, never silently refreshed.
        if let (Some(fingerprint), Some(location)) = (&record.fingerprint, &record.location) {
            if let Ok(bytes) = std::fs::read(ctx.root.join(&location.path)) {
                let actual = crate::digest::sha256_hex(&bytes);
                if fingerprint != &actual && fingerprint != &format!("sha256:{actual}") {
                    discrepancies.push(Discrepancy {
                        symbol: record.id.clone(),
                        kind: "evidence".to_owned(),
                        detail: "fingerprint-drift".to_owned(),
                    });
                }
            }
        }
        contracts.push(AnalysisContract {
            symbol: record.id.clone(),
            tests: record.native_tests.clone(),
            gates: record.gates.clone(),
        });
        if record.native_tests.is_empty() {
            unknowns.push(Gap {
                subject: record.id.clone(),
                detail: "no-native-test".to_owned(),
            });
        }
    }
    if index.is_some() {
        if let Ok(receipt) = crate::observed::bindings::propose(&ctx_as_observed(ctx)) {
            for proposal in &receipt.proposals {
                if !ids.contains(&proposal.symbol) {
                    continue;
                }
                if let Some(candidate) = proposal.candidates.first() {
                    bindings.push(AnalysisBinding {
                        proposal: proposal.proposal.clone(),
                        symbol: proposal.symbol.clone(),
                        native: candidate.native.clone(),
                        confidence: candidate.confidence.clone(),
                        state: "inferred".to_owned(),
                    });
                }
            }
        }
    }
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    bindings.sort_by(|left, right| left.proposal.cmp(&right.proposal));
    contracts.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    unknowns
        .sort_by(|left, right| (&left.subject, &left.detail).cmp(&(&right.subject, &right.detail)));
    unknowns.dedup();
    ambiguities.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    discrepancies.sort_by(|left, right| {
        (&left.symbol, &left.kind, &left.detail).cmp(&(&right.symbol, &right.kind, &right.detail))
    });
    discrepancies.dedup();

    let pins = compute_pins(ctx, index, &scope_source_paths(index, &records))?;
    let project = ctx
        .model
        .project
        .as_ref()
        .map(|project| project.id.clone())
        .unwrap_or_else(|| module.id.clone());
    Ok(AnalysisDocument {
        schema_version: version::ANALYSIS_SCHEMA_VERSION.to_owned(),
        identity: version::ANALYSIS_IDENTITY.to_owned(),
        project,
        scope: scope.wire(),
        target: target.map(str::to_owned),
        profile: profile.map(str::to_owned),
        pins,
        module,
        definitions,
        bindings,
        contracts,
        unknowns,
        ambiguities,
        discrepancies,
    })
}

/// The content-address digest of one analysis document.
pub fn analysis_digest(document: &AnalysisDocument) -> String {
    let value = serde_json::to_value(document).expect("analysis serializes");
    wire::digest("adoption-analysis", &value)
}

/// Decode a saved analysis file and recompute its pinned digest; the
/// file bytes are never trusted without the recomputed content address.
pub fn load_analysis(bytes: &[u8]) -> Outcome<(AnalysisDocument, String)> {
    let document = wire::analysis_of_bytes(bytes).map_err(|error| error.domain_result())?;
    let digest = analysis_digest(&document);
    Ok((document, digest))
}

/// The per-fact confidence of one definition: confirmed bindings are
/// exact facts, explicit bindings high, inferred low, unknown records
/// stay unknown. Confidence never becomes a runtime contract.
fn confidence_of(record: &SymbolRecord) -> &'static str {
    if record.status == BindingStatus::Confirmed {
        "exact"
    } else if record.state == BindingState::Unknown {
        "unknown"
    } else if record.status == BindingStatus::Explicit {
        "high"
    } else {
        "low"
    }
}

fn ctx_as_observed(ctx: &AdoptionContext) -> crate::observed::ObservedContext {
    crate::observed::ObservedContext {
        model: clone_model(&ctx.model),
        root: ctx.root.clone(),
    }
}

fn clone_model(model: &crate::loader::NormalizedModel) -> crate::loader::NormalizedModel {
    // NormalizedModel is Clone; the adoption context never mutates it.
    model.clone()
}
