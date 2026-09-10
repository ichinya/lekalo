//! Adapter-driven scanning (issue #42).
//!
//! [`run`] executes `lekalo scan`: it discovers and selects one target
//! adapter through the accepted #27/#28 protocol (safe describe
//! handshake, deterministic selection, strict capability policy), runs
//! the read-only `scan` operation inside the confined sandbox, and
//! translates the returned inventory into a typed observed-scan document
//! that merges through the accepted #39 seam. The core never parses
//! source code: the adapter — the reference node-typescript scanner or
//! any future PHP/Go adapter speaking the same protocol — owns the
//! language-specific extraction, including its stable keys, fingerprints,
//! semantic-id proposals, and candidate sets.
//!
//! Determinism and refusal: the produced document is a pure function of
//! the scanned tree and the adapter response; the scan revision binds
//! the adapter identity and the exact response id. A scan never writes a
//! source file (the operation declares no writes) and never downgrades a
//! user-owned binding (the #39 merge rules).

use serde::Serialize;
use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::result::{DomainResult, Status};
use crate::target_protocol::transport::AdapterCommand;
use crate::target_protocol::{
    discovery::Discovery, selection, wire, CallOutcome, CallRequest, TargetClient, TargetFailure,
};

/// The capability every scanner must declare `full` (issue #28).
const REQUIRED_CAPABILITY: &str = "scan.symbols";

/// One scan request: the target token (validated against the adapter's
/// declared targets), the optional profile, the adapter command, and the
/// transport limits.
pub struct ScanRequest<'a> {
    pub target: &'a str,
    pub profile: Option<&'a str>,
    pub command: AdapterCommand,
    pub limits: crate::target_protocol::transport::TransportLimits,
}

/// The receipt of one completed scan: the #39 merge receipt plus the
/// issue #42 scan facts (target, profile, native test bindings).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScanReceipt {
    pub status: &'static str,
    pub operation: &'static str,
    pub mode: &'static str,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub adapter: String,
    pub revision: String,
    pub symbols: usize,
    pub endpoints: usize,
    pub schemas: usize,
    pub test_bindings: usize,
    pub explicit: usize,
    pub confirmed: usize,
    pub inferred: usize,
    pub stale: usize,
    pub moved: Vec<String>,
    pub staled: Vec<String>,
}

/// Run one scan end to end. Loader and structure failures surface
/// through the observed context; protocol failures map onto their
/// registered `target.*` rules; the merged result uses the accepted
/// #39 merge rules unchanged.
pub fn run(
    ctx: &super::ObservedContext,
    request: &ScanRequest<'_>,
) -> Result<ScanReceipt, DomainResult> {
    let project = ctx
        .model
        .project
        .as_ref()
        .map(|project| project.id.clone())
        .ok_or_else(|| invalid_set("project-missing"))?;
    if !crate::init::detect::valid_target_id(request.target) {
        return Err(DomainResult::usage_error());
    }

    // 1. Safe discovery: the describe handshake only, no project IR.
    let mut client = TargetClient::new(request.limits);
    let discovered = Discovery::run(&mut client, &request.command, &ctx.root)?;

    // 2. Deterministic selection under the strict default policy: the
    // scanner must declare the scan capability `full` and the core IR
    // version (or run a legacy 1.0.0 session, whose IR compatibility the
    // upstream preflight owns). A selection that names no adapter is the
    // registered unsupported refusal, never a guess.
    let required = [REQUIRED_CAPABILITY.to_owned()];
    let report = selection::select(
        std::slice::from_ref(&discovered),
        selection::SelectionRequest {
            required: &required,
            preferred_profile: request.profile,
            policy: selection::SelectionPolicy::default(),
        },
        crate::ir::version::VERSION,
    );
    if report.selected.is_none() {
        let reason = report
            .excluded
            .first()
            .and_then(|excluded| excluded.reasons.first())
            .copied()
            .unwrap_or("capability-unsupported");
        return Err(unsupported_operation(reason));
    }
    if !discovered
        .targets
        .iter()
        .any(|offered| offered == request.target)
    {
        return Err(DomainResult::from(&TargetFailure::CapabilityUnsupported {
            detail: "target",
        }));
    }

    // 3. The read-only scan exchange inside the confined sandbox.
    let fs = crate::project_fs::Fs::open(&ctx.root).map_err(|_| {
        DomainResult::from(&TargetFailure::RequestInvalid {
            detail: "project-root",
        })
    })?;
    let outcome = client.call(
        &request.command,
        CallRequest {
            operation: wire::Operation::Scan,
            target: Some(request.target),
            profile: request.profile,
            profile_resolution: None,
            ir_path: None,
            dry_run: None,
            plan_id: None,
        },
        &ctx.root,
        &fs,
        None,
    )?;

    // 4. Translate the inventory into the typed observed-scan document.
    let entries = outcome
        .response
        .result
        .as_ref()
        .and_then(|result| result.entries.as_deref())
        .unwrap_or(&[]);
    let document = build_document(ctx, &outcome, entries, &project, request)?;

    // 5. Merge through the accepted seam: the wire validation and merge
    // rules are exactly the ones `observe update` uses.
    let bytes = serde_json::to_vec(&document).map_err(|_| invalid_set("document-shape"))?;
    let receipt = super::update_index(ctx, &bytes).map_err(DomainResult::invalid)?;
    let test_bindings = super::load_index(ctx)
        .map_err(DomainResult::invalid)?
        .map(|index| index.test_bindings.len())
        .unwrap_or(0);
    Ok(ScanReceipt {
        status: "valid",
        operation: "scan",
        mode: super::version::MODE,
        project,
        target: Some(request.target.to_owned()),
        profile: request.profile.map(str::to_owned),
        adapter: receipt.adapter,
        revision: receipt.revision,
        symbols: receipt.symbols,
        endpoints: receipt.endpoints,
        schemas: receipt.schemas,
        test_bindings,
        explicit: receipt.explicit,
        confirmed: receipt.confirmed,
        inferred: receipt.inferred,
        stale: receipt.stale,
        moved: receipt.moved,
        staled: receipt.staled,
    })
}

/// The registered invalid set for a refused document, owned by the
/// observed scan classification tags.
fn invalid_set(detail: &'static str) -> DomainResult {
    DomainResult::invalid(super::diagnostic::scan_invalid_set(detail, None))
}

/// The unsupported refusal when no adapter survives selection: the
/// registered `target.capability-unsupported` rule with the stable
/// exclusion reason, projected as the unsupported exit class.
fn unsupported_operation(reason: &'static str) -> DomainResult {
    use crate::diagnostics::types::{token_value, DataObject};
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(reason));
    let set = match crate::diagnostics::normalize::build(
        "target.capability-unsupported",
        None,
        None,
        data,
    ) {
        Ok(item) => DiagnosticSet::try_from_unsorted(vec![item], Status::Unsupported)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    };
    DomainResult::UnsupportedOperation { diagnostics: set }
}

/// The closed member set of one entry detail token (bounded compact
/// JSON): the proposed semantic id `s` (required), the native stable key
/// `n`, the 1-based declaration line `l`, the proposed confidence `q`,
/// and the native test binding `t` (`<test-path>#<name>`).
const DETAIL_KEYS: &[&str] = &["s", "n", "l", "q", "t"];

/// One parsed entry detail.
struct EntryDetail {
    semantic: String,
    native: String,
    line: Option<u64>,
    confidence: String,
    test: Option<String>,
}

/// Parse and validate one entry detail token against the closed shape.
fn parse_detail(entry: &wire::ScanEntry) -> Result<EntryDetail, DomainResult> {
    let Some(detail_text) = &entry.detail else {
        return Err(invalid_set("entry-detail-missing"));
    };
    let value: Json = serde_json::from_str(detail_text).map_err(|_| invalid_set("entry-detail"))?;
    let map = value
        .as_object()
        .ok_or_else(|| invalid_set("entry-detail"))?;
    if map.keys().any(|key| !DETAIL_KEYS.contains(&key.as_str())) {
        return Err(invalid_set("entry-detail"));
    }
    let semantic = string_member(map, "s")?;
    if !crate::trace::id::is_semantic_id(&semantic) {
        return Err(invalid_set("entry-detail"));
    }
    let native = string_member(map, "n")?;
    if native.is_empty() || native.len() > 256 || native.chars().any(|c| c.is_control()) {
        return Err(invalid_set("entry-detail"));
    }
    let line = match map.get("l") {
        None => None,
        Some(value) => {
            let line = value.as_u64().ok_or_else(|| invalid_set("entry-detail"))?;
            if line == 0 || line > 1_000_000 {
                return Err(invalid_set("entry-detail"));
            }
            Some(line)
        }
    };
    let confidence = match map.get("q") {
        None => "medium".to_owned(),
        Some(value) => {
            let confidence = value.as_str().ok_or_else(|| invalid_set("entry-detail"))?;
            if super::types::Confidence::parse(confidence).is_none() {
                return Err(invalid_set("entry-detail"));
            }
            confidence.to_owned()
        }
    };
    let test = match map.get("t") {
        None => None,
        Some(value) => {
            let test = value.as_str().ok_or_else(|| invalid_set("entry-detail"))?;
            if test.is_empty() || test.len() > 256 || test.chars().any(|c| c.is_control()) {
                return Err(invalid_set("entry-detail"));
            }
            Some(test.to_owned())
        }
    };
    Ok(EntryDetail {
        semantic,
        native,
        line,
        confidence,
        test,
    })
}

fn string_member(map: &serde_json::Map<String, Json>, key: &str) -> Result<String, DomainResult> {
    map.get(key)
        .and_then(Json::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid_set("entry-detail"))
}

/// The sha256 of one project file through the confined filesystem; a
/// missing or over-large file is `None` (the binding then carries no
/// freshness evidence, which the audit reports as `unknown`).
fn file_fingerprint(fs: &crate::project_fs::Fs, path: &str) -> Option<String> {
    let (dir, name) = path.rsplit_once('/').unwrap_or((".", path));
    let bytes = fs
        .read_file_opt(dir, name, super::MAX_SOURCE_BYTES)
        .ok()
        .flatten()?;
    Some(format!(
        "sha256:{}",
        crate::versioning::plan::sha256_hex(&bytes)
    ))
}

/// Assemble the observed-scan document from the adapter response. Every
//  entry contributes one candidate for its proposed semantic id; a
//  semantic id proposed by several entries produces ONE binding whose
//  candidate set holds all of them — the core never collapses the set to
//  a first match. Fingerprints are computed here, from the real tree.
fn build_document(
    ctx: &super::ObservedContext,
    outcome: &CallOutcome,
    entries: &[wire::ScanEntry],
    project: &str,
    request: &ScanRequest<'_>,
) -> Result<Json, DomainResult> {
    let fs = crate::project_fs::Fs::open(&ctx.root).map_err(|_| {
        DomainResult::from(&TargetFailure::RequestInvalid {
            detail: "project-root",
        })
    })?;

    // Pass one: parse every entry, keying candidates by semantic id and
    // collecting native test bindings. The entry path is the symbol's
    // source file (the adapter walks only inside its read scopes).
    let mut order: Vec<String> = Vec::new();
    let mut grouped: std::collections::HashMap<
        String,
        Vec<(String, &wire::ScanEntry, EntryDetail)>,
    > = std::collections::HashMap::new();
    let mut tests: Vec<(String, String)> = Vec::new();
    for entry in entries {
        if crate::project_fs::path_violation(&entry.path).is_some() {
            return Err(invalid_set("entry-path"));
        }
        if super::types::SymbolKind::parse(&entry.kind).is_none() {
            return Err(invalid_set("entry-kind"));
        }
        let detail = parse_detail(entry)?;
        if !order.contains(&detail.semantic) {
            order.push(detail.semantic.clone());
        }
        if let Some(test) = &detail.test {
            tests.push((detail.semantic.clone(), test.clone()));
        }
        grouped.entry(detail.semantic.clone()).or_default().push((
            entry.kind.clone(),
            entry,
            detail,
        ));
    }
    order.sort();

    // Pass two: one observed-scan symbol per semantic id. Fingerprints
    // come from the real tree, so the freshness evidence is byte-truth.
    let mut symbols = Vec::with_capacity(order.len());
    for semantic in &order {
        let group = grouped
            .get(semantic)
            .ok_or_else(|| invalid_set("entry-detail"))?;
        let kind = group[0].0.clone();
        let mut candidates = Vec::with_capacity(group.len());
        for (_, entry, detail) in group {
            candidates.push(serde_json::json!({
                "native": detail.native,
                "path": entry.path,
                "line": detail.line,
                "fingerprint": file_fingerprint(&fs, &entry.path),
                "confidence": detail.confidence,
            }));
        }
        candidates.sort_by(|left, right| {
            let key = |value: &Json| (value["native"].as_str().unwrap_or_default().to_owned(),);
            key(left).cmp(&key(right))
        });
        let unambiguous = group.len() == 1;
        let (_, entry, detail) = &group[0];
        let mut symbol = serde_json::Map::new();
        symbol.insert("id".to_owned(), Json::String(semantic.clone()));
        symbol.insert("kind".to_owned(), Json::String(kind));
        symbol.insert(
            "mappingConfidence".to_owned(),
            Json::String(detail.confidence.clone()),
        );
        if unambiguous {
            symbol.insert("stableKey".to_owned(), Json::String(detail.native.clone()));
            let mut location = serde_json::Map::new();
            location.insert("path".to_owned(), Json::String(entry.path.clone()));
            if let Some(line) = detail.line {
                location.insert("line".to_owned(), Json::Number(line.into()));
            }
            symbol.insert("location".to_owned(), Json::Object(location));
            symbol.insert(
                "fingerprint".to_owned(),
                serde_json::json!(file_fingerprint(&fs, &entry.path)),
            );
        }
        symbol.insert("candidates".to_owned(), Json::Array(candidates));
        symbols.push(Json::Object(symbol));
    }

    // Native test bindings: `t` is `<test-path>#<name>`; the test file
    // is the part before the last `#`, fingerprinted from the real tree.
    let mut test_bindings = Vec::new();
    tests.sort();
    tests.dedup();
    for (symbol, binding) in tests {
        let (path, id) = match binding.rsplit_once('#') {
            Some((path, name)) => (path.to_owned(), format!("{path}#{name}")),
            None => (binding.clone(), binding.clone()),
        };
        if crate::project_fs::path_violation(&path).is_some() {
            return Err(invalid_set("test-binding-path"));
        }
        test_bindings.push(serde_json::json!({
            "id": id,
            "symbol": symbol,
            "path": path,
            "fingerprint": file_fingerprint(&fs, &path),
        }));
    }

    // The scan revision binds the adapter identity and the exact
    // response id: one exchange is one revision, deterministically.
    let adapter = &outcome.response.evidence.adapter;
    let revision_payload = (
        adapter.id.as_str(),
        adapter.version.as_str(),
        adapter.digest.as_str(),
        outcome.response.request_id.as_str(),
    );
    let revision_bytes =
        serde_json::to_vec(&revision_payload).map_err(|_| invalid_set("document-shape"))?;
    let revision = format!(
        "sha256:{}",
        crate::versioning::plan::sha256_hex(&revision_bytes)
    );

    let mut document = serde_json::Map::new();
    document.insert(
        "schemaVersion".to_owned(),
        Json::String(super::version::SCAN_SCHEMA_VERSION.to_owned()),
    );
    document.insert(
        "adapter".to_owned(),
        serde_json::json!({
            "id": adapter.id,
            "version": adapter.version,
            "digest": adapter.digest,
        }),
    );
    document.insert("project".to_owned(), Json::String(project.to_owned()));
    document.insert("revision".to_owned(), Json::String(revision));
    document.insert("symbols".to_owned(), Json::Array(symbols));
    document.insert("target".to_owned(), Json::String(request.target.to_owned()));
    if let Some(profile) = request.profile {
        document.insert("profile".to_owned(), Json::String(profile.to_owned()));
    }
    if !test_bindings.is_empty() {
        document.insert("testBindings".to_owned(), Json::Array(test_bindings));
    }
    Ok(Json::Object(document))
}
