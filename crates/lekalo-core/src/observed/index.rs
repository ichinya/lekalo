//! The observed index service (issue #39): merging adapter scans,
//! explicit bindings, confirmation, attachments, staleness, and the
//! confined canonical store.
//!
//! Determinism contract: one (index, scan, tree) state produces one
//! receipt and one set of canonical bytes. The index lives at
//! `.lekalo/import/observed/index.json` inside the accepted
//! `lekalo.observed-model-draft` authority home; every mutation rewrites
//! it atomically from a fully recomputed state, and every read verifies
//! the exact canonical re-rendering before the index is used.

use std::collections::HashMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::diagnostics::DiagnosticSet;

use crate::project_fs::Fs;

use super::diagnostic;
use super::types::{
    AdapterIdentity, BindingState, BindingStatus, Confidence, EndpointRecord, HistoryEntry,
    HistoryEvent, ObservedIndex, PromotionReceipt, Provenance, SchemaRecord, SourceLocation,
    SymbolRecord, TestBindingRecord,
};
use super::version;
use super::wire;

/// The maximum source file accepted for fingerprinting; larger sources
/// record `unknown` evidence instead of a truncated digest.
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Receipts
// ---------------------------------------------------------------------------

/// The receipt of `observe update` (a merged adapter scan).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdateReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub project: String,
    pub revision: String,
    pub adapter: String,
    pub symbols: usize,
    pub endpoints: usize,
    pub schemas: usize,
    pub explicit: usize,
    pub confirmed: usize,
    pub inferred: usize,
    pub stale: usize,
    pub promoted: usize,
    pub moved: Vec<String>,
    pub staled: Vec<String>,
}

/// The receipt of `observe bind`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BindReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbol: String,
    pub binding: BindingStatus,
    pub state: BindingState,
}

/// The receipt of `observe confirm`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConfirmReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbol: String,
    pub binding: BindingStatus,
}

/// The receipt of `observe attach`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AttachReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbol: String,
    pub native_tests: Vec<String>,
    pub gates: Vec<String>,
}

/// The receipt of a current `observe check` (no stale binding).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StalenessReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub symbols: usize,
    pub current: usize,
    pub unknown: usize,
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Read the persisted index, if any; a present index must be the exact
/// canonical rendering.
pub fn load_index(ctx: &super::ObservedContext) -> Result<Option<ObservedIndex>, DiagnosticSet> {
    let fs = Fs::open(&ctx.root).map_err(|_| diagnostic::index_io_set("root-unreadable"))?;
    let bytes = match fs.read_file_opt(
        ".lekalo/import/observed",
        "index.json",
        version::MAX_INDEX_BYTES,
    ) {
        Ok(Some(bytes)) => bytes,
        Ok(None) | Err(crate::project_fs::FsErrorKind::NotFound) => return Ok(None),
        Err(_) => return Err(diagnostic::index_io_set("index-unreadable")),
    };
    let index = parse_index(&bytes)?;
    if index.project != model_project(ctx) {
        return Err(diagnostic::index_io_set("project-mismatch"));
    }
    Ok(Some(index))
}

/// Parse and fully validate index bytes: closed wire, canonical
/// re-rendering equality, and bounds.
pub fn parse_index(bytes: &[u8]) -> Result<ObservedIndex, DiagnosticSet> {
    if bytes.len() > version::MAX_INDEX_BYTES {
        return Err(diagnostic::scan_limit_set("index-bytes", bytes.len()));
    }
    let index: ObservedIndex =
        serde_json::from_slice(bytes).map_err(|_| diagnostic::index_io_set("index-malformed"))?;
    if index.schema_version != version::SCHEMA_VERSION
        || index.identity != version::INDEX_IDENTITY
        || index.mode != version::MODE
    {
        return Err(diagnostic::index_io_set("index-identity"));
    }
    let canonical = canonical_bytes(&index)?;
    if canonical.as_bytes() != bytes {
        return Err(diagnostic::index_io_set("index-noncanonical"));
    }
    Ok(index)
}

/// The canonical JSON bytes of one index (compact, fixed key order,
/// sorted records).
pub fn canonical_bytes(index: &ObservedIndex) -> Result<String, DiagnosticSet> {
    let bytes = serde_json::to_string(index)
        .map_err(|_| diagnostic::index_io_set("index-unserializable"))?;
    if bytes.len() > version::MAX_INDEX_BYTES {
        return Err(diagnostic::scan_limit_set("index-bytes", bytes.len()));
    }
    Ok(bytes)
}

/// The `sha256:<hex>` digest of the exact index bytes.
pub fn index_digest(index: &ObservedIndex) -> Result<String, DiagnosticSet> {
    Ok(sha256_hex(canonical_bytes(index)?.as_bytes()))
}

/// Persist the index canonically and atomically.
pub(crate) fn save_index(
    ctx: &super::ObservedContext,
    index: &ObservedIndex,
) -> Result<(), DiagnosticSet> {
    let bytes = canonical_bytes(index)?;
    super::store::write_confined(
        &ctx.root,
        ".lekalo/import/observed",
        "index.json",
        bytes.as_bytes(),
    )
}

/// The one-segment project id of the loaded model.
fn model_project(ctx: &super::ObservedContext) -> String {
    ctx.model
        .project
        .as_ref()
        .map(|project| project.id.clone())
        .unwrap_or_default()
}

/// The declared module ids of the loaded model.
fn model_modules(ctx: &super::ObservedContext) -> Vec<&str> {
    ctx.model
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect()
}
// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Merge one adapter scan into the observed index (or create the index).
///
/// Merge rules, in order: scanned symbols refresh or create their
/// records; a scanned symbol recorded under a stable key survives a
/// source move (the path change is history, never a break); explicit and
/// confirmed statuses survive every scan (a scan never downgrades a
/// user-owned fact); recorded symbols the scan no longer reports become
/// stale; endpoints and schemas are replaced wholesale from the scan.
pub fn update_index(
    ctx: &super::ObservedContext,
    scan_bytes: &[u8],
) -> Result<UpdateReceipt, DiagnosticSet> {
    let scan = wire::parse_scan(scan_bytes, ctx.model.model_version)?;
    let modules = model_modules(ctx);
    for symbol in &scan.symbols {
        let module = ObservedModule::of(&symbol.id);
        if !modules.contains(&module.as_str()) {
            return Err(diagnostic::unknown_module_set(&module));
        }
    }
    let previous = load_index(ctx)?;
    if let Some(existing) = &previous {
        if existing.project != scan.project {
            return Err(diagnostic::scan_invalid_set("project-mismatch", None));
        }
        // The declared target (issue #42) is set once: a later scan
        // naming a different target refuses instead of silently mixing
        // bindings from two targets in one registry.
        if let Some(target) = &scan.target {
            if let Some(existing_target) = &existing.target {
                if existing_target != target {
                    return Err(diagnostic::scan_invalid_set("target-mismatch", None));
                }
            }
        }
    }
    if scan.profile.is_some() && scan.target.is_none() {
        return Err(diagnostic::scan_invalid_set("profile-without-target", None));
    }

    let mut index = ObservedIndex::empty(
        &scan.project,
        AdapterIdentity {
            id: scan.adapter.id.clone(),
            version: scan.adapter.version.clone(),
            digest: scan.adapter.digest.clone(),
        },
        &scan.revision,
    );

    let mut moved: Vec<String> = Vec::new();
    let mut staled: Vec<String> = Vec::new();
    let mut scanned_keys: HashMap<&str, &super::types::ScanSymbol> = HashMap::new();

    for symbol in &scan.symbols {
        scanned_keys.insert(symbol.id.as_str(), symbol);
    }
    // Records the scan no longer reports become stale, never silent.
    if let Some(existing) = &previous {
        for record in &existing.symbols {
            if !scanned_keys.contains_key(record.id.as_str()) {
                let mut stale_record = record.clone();
                stale_record.state = BindingState::Stale;
                stale_record.history.push(HistoryEntry {
                    event: HistoryEvent::Staled,
                    revision: scan.revision.clone(),
                    from: stale_record.location.as_ref().map(|l| l.path.clone()),
                    to: None,
                });
                index.symbols.push(stale_record);
                staled.push(record.id.clone());
            }
        }
    }
    for symbol in &scan.symbols {
        let previous_record = previous.as_ref().and_then(|index| index.symbol(&symbol.id));
        let record = merge_symbol(
            symbol,
            previous_record,
            &scan.adapter,
            &scan.revision,
            &mut moved,
        );
        index.symbols.push(record);
    }
    index.symbols.sort_by(|left, right| left.id.cmp(&right.id));

    index.target = scan.target.clone();
    index.profile = scan.profile.clone();
    index.endpoints = scan
        .endpoints
        .iter()
        .map(|endpoint| EndpointRecord {
            id: endpoint.id.clone(),
            method: endpoint.method.clone(),
            path: endpoint.path.clone(),
            symbol: endpoint.symbol.clone(),
        })
        .collect();
    index
        .endpoints
        .sort_by(|left, right| left.id.cmp(&right.id));
    index.schemas = scan
        .schemas
        .iter()
        .map(|schema| SchemaRecord {
            id: schema.id.clone(),
            kind: schema.kind,
            symbol: schema.symbol.clone(),
            digest: schema.digest.clone(),
        })
        .collect();
    index.schemas.sort_by(|left, right| left.id.cmp(&right.id));
    // Native test bindings (issue #42) are replaced wholesale from the
    // scan; every binding starts current when it carries a fingerprint
    // and unknown when it carries none (missing evidence is unknown,
    // never absence).
    index.test_bindings = scan
        .test_bindings
        .iter()
        .map(|binding| TestBindingRecord {
            id: binding.id.clone(),
            symbol: binding.symbol.clone(),
            path: binding.path.clone(),
            fingerprint: binding.fingerprint.clone(),
            state: if binding.fingerprint.is_some() {
                BindingState::Current
            } else {
                BindingState::Unknown
            },
        })
        .collect();
    index
        .test_bindings
        .sort_by(|left, right| (&left.id, &left.symbol).cmp(&(&right.id, &right.symbol)));

    save_index(ctx, &index)?;
    Ok(receipt_of(
        &index,
        &scan.revision,
        &scan.adapter.id,
        moved,
        staled,
    ))
}

/// Merge one scanned symbol with its previous record.
fn merge_symbol(
    symbol: &super::types::ScanSymbol,
    previous: Option<&SymbolRecord>,
    adapter: &AdapterIdentity,
    revision: &str,
    moved: &mut Vec<String>,
) -> SymbolRecord {
    let provenance = Provenance {
        origin: super::types::Origin::Observed,
        confidence: symbol.mapping,
        adapter: adapter.id.clone(),
        revision: revision.to_owned(),
    };
    let evidence = symbol.evidence.clone();
    let Some(record) = previous else {
        return SymbolRecord {
            id: symbol.id.clone(),
            kind: symbol.kind,
            stable_key: symbol.stable_key.clone(),
            location: symbol.location.clone(),
            fingerprint: symbol.fingerprint.clone(),
            status: BindingStatus::Inferred,
            // An evidence-free record (an ambiguous adapter mapping
            // with candidates only) has nothing to compare: unknown,
            // never current.
            state: if symbol.location.is_none() && symbol.fingerprint.is_none() {
                BindingState::Unknown
            } else {
                BindingState::Current
            },
            promoted: false,
            promotion: None,
            evidence,
            provenance,
            native_tests: Vec::new(),
            gates: Vec::new(),
            history: vec![HistoryEntry {
                event: HistoryEvent::Recorded,
                revision: revision.to_owned(),
                from: None,
                to: symbol.location.as_ref().map(|l| l.path.clone()),
            }],
            candidates: symbol.candidates.clone(),
        };
    };
    let mut next = record.clone();
    let previous_path = next.location.as_ref().map(|l| l.path.clone());
    let next_path = symbol.location.as_ref().map(|l| l.path.clone());
    let path_changed = previous_path != next_path && next_path.is_some();
    // Adapter resolution: the same stable key at a new path survives as a
    // move; without a stable key a path change is a stale break.
    let survived_move = path_changed
        && next.stable_key.is_some()
        && symbol.stable_key.as_deref() == next.stable_key.as_deref();
    if survived_move {
        moved.push(next.id.clone());
        next.history.push(HistoryEntry {
            event: HistoryEvent::Moved,
            revision: revision.to_owned(),
            from: previous_path,
            to: next_path.clone(),
        });
    }
    next.kind = symbol.kind;
    next.evidence = evidence;
    // A scan never downgrades a user-owned binding status, never erases
    // a user-owned fact's recorded location (an ambiguous scan carries
    // no single mapping at all), and never lets a user-owned fact gather
    // candidate sets (issue #42): the adapter's ambiguity material is
    // proposal material only.
    if next.status == BindingStatus::Inferred {
        if symbol.stable_key.is_some() {
            next.stable_key = symbol.stable_key.clone();
        }
        next.location = symbol.location.clone();
        next.fingerprint = symbol.fingerprint.clone();
        next.state = if next.location.is_none() && next.fingerprint.is_none() {
            BindingState::Unknown
        } else {
            BindingState::Current
        };
        next.provenance = provenance;
        next.candidates = symbol.candidates.clone();
    } else {
        // Explicit and confirmed facts refresh only from a concrete
        // mapping (the #39 stable-key move resolution); an ambiguous
        // scan leaves the recorded evidence untouched for the audit to
        // judge.
        if symbol.location.is_some() {
            if symbol.stable_key.is_some() {
                next.stable_key = symbol.stable_key.clone();
            }
            next.location = symbol.location.clone();
            next.fingerprint = symbol.fingerprint.clone();
        }
        next.state = if next.location.is_some() && next.fingerprint.is_some() {
            BindingState::Current
        } else if next.location.is_none() && next.fingerprint.is_none() {
            BindingState::Unknown
        } else {
            next.state
        };
        next.candidates = Vec::new();
    }
    next
}

/// Create or refresh an explicit binding (a user-owned fact).
pub fn bind_explicit(
    ctx: &super::ObservedContext,
    symbol_id: &str,
    stable_key: Option<&str>,
    path: &str,
    line: Option<u64>,
) -> Result<BindReceipt, DiagnosticSet> {
    let mut index = require_index(ctx)?;
    let position = index
        .symbols
        .iter()
        .position(|record| record.id == symbol_id)
        .ok_or_else(|| diagnostic::unknown_symbol_set(symbol_id))?;
    if let Some(key) = stable_key {
        if key.is_empty() || key.len() > 256 || key.chars().any(|c| c.is_control()) {
            return Err(diagnostic::scan_invalid_set("stable-key", Some(symbol_id)));
        }
    }
    if crate::project_fs::path_violation(path).is_some() {
        return Err(diagnostic::scan_invalid_set(
            "location-path",
            Some(symbol_id),
        ));
    }
    let fs = Fs::open(&ctx.root).map_err(|_| diagnostic::index_io_set("root-unreadable"))?;
    let fingerprint = read_fingerprint(&fs, path)?;
    let state = if fingerprint.is_some() {
        BindingState::Current
    } else {
        BindingState::Stale
    };
    let adapter_id = index.adapter.id.clone();
    let revision = index.revision.clone();
    let record = &mut index.symbols[position];
    let previous_path = record.location.as_ref().map(|l| l.path.clone());
    record.status = BindingStatus::Explicit;
    record.state = state;
    if let Some(key) = stable_key {
        record.stable_key = Some(key.to_owned());
    }
    record.location = Some(SourceLocation {
        path: path.to_owned(),
        line,
    });
    record.fingerprint = fingerprint.clone();
    record.evidence.signature = fingerprint;
    record.provenance = Provenance {
        origin: super::types::Origin::Declared,
        confidence: Confidence::Exact,
        adapter: adapter_id,
        revision: revision.clone(),
    };
    record.history.push(HistoryEntry {
        event: HistoryEvent::Bound,
        revision,
        from: previous_path,
        to: Some(path.to_owned()),
    });
    trim_history(record);
    let binding = record.status;
    let bound_state = record.state;
    save_index(ctx, &index)?;
    Ok(BindReceipt {
        status: "valid",
        operation: "bind",
        mode: version::MODE,
        symbol: symbol_id.to_owned(),
        binding,
        state: bound_state,
    })
}

/// Confirm an inferred binding; only inferred bindings confirm.
pub fn confirm_binding(
    ctx: &super::ObservedContext,
    symbol_id: &str,
) -> Result<ConfirmReceipt, DiagnosticSet> {
    let mut index = require_index(ctx)?;
    let position = index
        .symbols
        .iter()
        .position(|record| record.id == symbol_id)
        .ok_or_else(|| diagnostic::unknown_symbol_set(symbol_id))?;
    let revision = index.revision.clone();
    let record = &mut index.symbols[position];
    if record.status != BindingStatus::Inferred {
        return Err(diagnostic::confirm_refused_set(
            symbol_id,
            record.status.key(),
        ));
    }
    record.status = BindingStatus::Confirmed;
    record.provenance.origin = super::types::Origin::Observed;
    record.history.push(HistoryEntry {
        event: HistoryEvent::Confirmed,
        revision,
        from: None,
        to: None,
    });
    trim_history(record);
    let binding = record.status;
    save_index(ctx, &index)?;
    Ok(ConfirmReceipt {
        status: "valid",
        operation: "confirm",
        mode: version::MODE,
        symbol: symbol_id.to_owned(),
        binding,
    })
}

/// Attach native tests and gates to one observed symbol.
pub fn attach(
    ctx: &super::ObservedContext,
    symbol_id: &str,
    native_tests: &[String],
    gates: &[String],
) -> Result<AttachReceipt, DiagnosticSet> {
    let mut index = require_index(ctx)?;
    let position = index
        .symbols
        .iter()
        .position(|record| record.id == symbol_id)
        .ok_or_else(|| diagnostic::unknown_symbol_set(symbol_id))?;
    for id in native_tests.iter().chain(gates.iter()) {
        if !version::is_external_id(id) {
            return Err(diagnostic::scan_invalid_set("attachment-id", Some(id)));
        }
    }
    let record = &mut index.symbols[position];
    let mut tests = record.native_tests.clone();
    for test in native_tests {
        if !tests.contains(test) {
            tests.push(test.clone());
        }
    }
    tests.sort();
    if tests.len() > version::MAX_ATTACHMENTS {
        return Err(diagnostic::scan_limit_set("native-tests", tests.len()));
    }
    let mut attached_gates = record.gates.clone();
    for gate in gates {
        if !attached_gates.contains(gate) {
            attached_gates.push(gate.clone());
        }
    }
    attached_gates.sort();
    if attached_gates.len() > version::MAX_ATTACHMENTS {
        return Err(diagnostic::scan_limit_set("gates", attached_gates.len()));
    }
    if tests != record.native_tests || attached_gates != record.gates {
        record.native_tests = tests.clone();
        record.gates = attached_gates.clone();
        record.history.push(HistoryEntry {
            event: HistoryEvent::Attached,
            revision: index.revision.clone(),
            from: None,
            to: None,
        });
        trim_history(record);
        save_index(ctx, &index)?;
    }
    Ok(AttachReceipt {
        status: "valid",
        operation: "attach",
        mode: version::MODE,
        symbol: symbol_id.to_owned(),
        native_tests: tests,
        gates: attached_gates,
    })
}

/// The staleness gate: re-fingerprint every current binding against the
/// tree. Any stale binding is a fatal registered diagnostic set; a clean
/// index returns the receipt. Bindings without fingerprints are unknown,
/// never stale.
pub fn staleness(ctx: &super::ObservedContext) -> Result<StalenessReceipt, DiagnosticSet> {
    let index = require_index(ctx)?;
    let fs = Fs::open(&ctx.root).map_err(|_| diagnostic::index_io_set("root-unreadable"))?;
    let mut current = 0usize;
    let mut unknown = 0usize;
    let mut failures: Vec<(String, &'static str)> = Vec::new();
    for record in &index.symbols {
        if record.promoted {
            // Promoted symbols are canonical-owned; their observed
            // binding drift is history, not a gate failure.
            continue;
        }
        match (&record.fingerprint, &record.location) {
            (Some(digest), Some(location)) => match read_fingerprint(&fs, &location.path) {
                Ok(Some(actual)) if &actual == digest => current += 1,
                Ok(Some(_)) => failures.push((record.id.clone(), "fingerprint-mismatch")),
                Ok(None) | Err(_) => failures.push((record.id.clone(), "source-missing")),
            },
            _ => unknown += 1,
        }
    }
    if !failures.is_empty() {
        let mut items = Vec::new();
        for (symbol, detail) in failures
            .iter()
            .take(crate::diagnostics::types::limits::DIAGNOSTICS_PER_RESULT)
        {
            match diagnostic::try_stale_binding_item(symbol, detail) {
                Some(item) => items.push(item),
                None => return Err(crate::result::singleton_set("diagnostics.registry-invalid")),
            }
        }
        return Err(crate::diagnostics::DiagnosticSet::try_from_unsorted(
            items,
            crate::result::Status::Invalid,
        )
        .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")));
    }
    Ok(StalenessReceipt {
        status: "valid",
        operation: "check",
        mode: version::MODE,
        symbols: index.symbols.len(),
        current,
        unknown,
    })
}

/// The recorded index; every operation except `update` requires one.
fn require_index(ctx: &super::ObservedContext) -> Result<ObservedIndex, DiagnosticSet> {
    load_index(ctx)?.ok_or_else(|| diagnostic::index_missing_set("observed"))
}

/// The sha256 of one project source file, through the confined fs; a
/// missing file is `None`, an over-large file is unknown (`None`).
pub(crate) fn read_fingerprint(fs: &Fs, path: &str) -> Result<Option<String>, DiagnosticSet> {
    let (dir, name) = split_logical(path);
    match fs.read_file_opt(dir, name, MAX_SOURCE_BYTES) {
        Ok(Some(bytes)) => Ok(Some(sha256_hex(&bytes))),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

/// Split `a/b/c.ts` into (`a/b`, `c.ts`); a bare name reads from the
/// project root.
fn split_logical(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(at) => (&path[..at], &path[at + 1..]),
        None => (".", path),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// Keep the bounded history tail (the oldest entries fall off).
pub(crate) fn trim_history(record: &mut SymbolRecord) {
    if record.history.len() > version::MAX_HISTORY {
        let excess = record.history.len() - version::MAX_HISTORY;
        record.history.drain(..excess);
    }
}

fn receipt_of(
    index: &ObservedIndex,
    revision: &str,
    adapter: &str,
    mut moved: Vec<String>,
    mut staled: Vec<String>,
) -> UpdateReceipt {
    moved.sort();
    moved.dedup();
    staled.sort();
    staled.dedup();
    let count = |status: BindingStatus| {
        index
            .symbols
            .iter()
            .filter(|record| record.status == status)
            .count()
    };
    UpdateReceipt {
        status: "valid",
        operation: "update",
        mode: version::MODE,
        project: index.project.clone(),
        revision: revision.to_owned(),
        adapter: adapter.to_owned(),
        symbols: index.symbols.len(),
        endpoints: index.endpoints.len(),
        schemas: index.schemas.len(),
        explicit: count(BindingStatus::Explicit),
        confirmed: count(BindingStatus::Confirmed),
        inferred: count(BindingStatus::Inferred),
        stale: index
            .symbols
            .iter()
            .filter(|record| record.state == BindingState::Stale)
            .count(),
        promoted: index
            .symbols
            .iter()
            .filter(|record| record.promoted)
            .count(),
        moved,
        staled,
    }
}

/// The module segment of a semantic id.
struct ObservedModule;

impl ObservedModule {
    fn of(id: &str) -> String {
        id.split('.')
            .next()
            .filter(|segment| !segment.is_empty())
            .unwrap_or_default()
            .to_owned()
    }
}

/// Promotion receipts are constructed by the promotion workflow only.
impl SymbolRecord {
    /// Mark this record promoted with its adoption receipt.
    pub(crate) fn mark_promoted(&mut self, receipt: PromotionReceipt) {
        self.promoted = true;
        self.promotion = Some(receipt);
        self.status = BindingStatus::Confirmed;
    }
}
