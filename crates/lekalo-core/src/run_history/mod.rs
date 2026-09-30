//! The local-only run history and metrics recorder (issue #121).
//!
//! Lekalo-owned evidence custody under `.lekalo/history/**`: pilots and
//! Framework Lift evaluation store reproducible source measurements
//! fully offline — no account, no network, no provider call — with
//! repository/tenant isolation, atomic writes and recovery,
//! configurable retention and deletion, missing metrics kept unknown
//! (never zero), assertions stored separately from metrics, exact #120
//! policy references, and no source snippets, raw prompts, secrets, or
//! absolute paths by default. Deleting a raw record transactionally
//! invalidates every dependent index/claim reference. There is no
//! export: public aggregate payloads are constructed only by issue
//! #102.
//!
//! Module map:
//! - [`version`]: the closed contract identities of the family;
//! - [`clock`]: strict UTC timestamps and the injection seam;
//! - [`limits`]: the closed allocation bounds;
//! - [`value`]: the value-state wrapper (missing means unknown);
//! - [`types`]: the typed wire models;
//! - [`validate`]: fail-closed validation, normalization, and record
//!   construction;
//! - [`path`]: the governed history home and containment checks;
//! - [`store`]: the durable SQLite backend.
//!
//! Every public function consumes the loader's project selection and
//! returns the stable [`DomainResult`] envelope.

use serde_json::json;

use crate::loader::diagnostic::failure;
use crate::loader::error::Diagnostic;
use crate::loader::LoadSelection;
use crate::result::{DomainResult, Status};

pub(crate) mod clock;
pub(crate) mod codes;
pub(crate) mod limits;
pub(crate) mod path;
pub(crate) mod store;
pub(crate) mod types;
pub(crate) mod validate;
pub(crate) mod value;
pub(crate) mod version;

#[cfg(test)]
mod tests;

use store::StoreError;

/// `lekalo history init`: create the governed history home with its
/// generated ignore protection and verified untracked custody.
pub fn init(selection: &LoadSelection, role: Option<&str>) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    match run_init(&root, role) {
        Ok(report) => report,
        Err(error) => failure_from_store(error),
    }
}

fn run_init(root: &std::path::Path, role: Option<&str>) -> Result<DomainResult, StoreError> {
    let store = store::Store::open(root)?;
    if let Some(role) = role {
        store.set_repository_role(role)?;
    }
    let document = store.store_document()?;
    let repository_role = store.repository_role()?;
    let payload = serde_json::to_value(&document).map_err(|_| StoreError::Io)?;
    let json = render_envelope("history-init", &payload);
    let human = format!(
        "history initialized : repository role {repository_role}, {} scope(s)",
        document.tenant_scopes.len()
    );
    Ok(DomainResult::receipt(json, human))
}

/// `lekalo history scope create`: mint one random local tenant scope
/// token; no tenant name or host identity is accepted or derivable.
pub fn scope_create(selection: &LoadSelection) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.scope_create() {
        Ok((scope_id, created_at)) => DomainResult::receipt(
            render_envelope(
                "history-scope-created",
                &json!({
                    "tenantScopeId": scope_id,
                    "createdAt": created_at,
                }),
            ),
            format!("history scope created : {created_at}"),
        ),
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history record --input -`: validate and atomically append
/// one bounded typed harness observation. The recorder's exit proves
/// ingestion only — a recorded operation whose own status is `fail`
/// still ingests validly, and a recorder failure never rewrites the
/// operation outcome.
pub fn record(selection: &LoadSelection, scope_token: &str, input: &[u8]) -> DomainResult {
    if input.len() > limits::MAX_INPUT_BYTES {
        return failure(
            Status::Invalid,
            vec![
                Diagnostic::new(codes::INPUT_INVALID).with_data(data(&[("detail", "input-bound")]))
            ],
        );
    }
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let observation = match validate::parse_observation(input) {
        Ok(observation) => observation,
        Err(violation) => return failure_from_violation(violation),
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.append(scope_token, &observation, input) {
        Ok(receipt) => DomainResult::receipt(
            render_envelope(
                "history-recorded",
                &json!({
                    "runId": receipt.run_id,
                    "recordDigest": receipt.record_digest,
                    "status": receipt.status_outcome.as_str(),
                    "reasonCodes": receipt.reason_codes,
                    "generation": receipt.generation,
                    "pruned": receipt.pruned,
                }),
            ),
            format!(
                "run recorded : {} ({})",
                receipt.run_id,
                receipt.status_outcome.as_str()
            ),
        ),
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history list`: the bounded, sorted, same-scope-only page.
pub fn list(
    selection: &LoadSelection,
    scope_token: &str,
    limit: usize,
    cursor: Option<&str>,
) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.list(scope_token, limit, cursor) {
        Ok((rows, next_cursor)) => {
            let runs: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "runId": row.run_id,
                        "recordedAt": row.recorded_at,
                        "status": row.outcome.as_str(),
                        "operation": parse_operation_kind_name(row.operation_kind),
                        "payloadBytes": row.payload_bytes,
                        "assertions": row.has_assertions,
                    })
                })
                .collect();
            let payload = json!({
                "runs": runs,
                "nextCursor": next_cursor,
            });
            let human = format!("history list : {} run(s)", rows.len());
            DomainResult::receipt(render_envelope("history-list", &payload), human)
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history show`: the sanitized record plus its separately
/// stored assertion set, named apart for the local operator.
pub fn show(selection: &LoadSelection, scope_token: &str, run_id: &str) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.get(scope_token, run_id) {
        Ok((record, assertions)) => {
            let record_value: serde_json::Value =
                serde_json::from_str(&record).unwrap_or(json!({}));
            let payload = json!({
                "record": record_value,
                "assertions": assertions
                    .as_ref()
                    .and_then(|bytes| serde_json::from_str::<serde_json::Value>(bytes).ok()),
            });
            let human = format!("run {run_id} : record and separate assertion set");
            DomainResult::receipt(render_envelope("history-show", &payload), human)
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history retention`: configure the retention bounds
/// store-wide; the strictest applicable bound is enforced per tenant
/// scope on append and explicit prune.
pub fn retention(
    selection: &LoadSelection,
    scope_token: &str,
    max_age_days: u32,
    max_records: u32,
    max_bytes: u64,
) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    // The scope must resolve: retention applies to a live scope.
    if let Err(error) = store.scope_id(scope_token) {
        return failure_from_store(error);
    }
    let bounds = types::Retention {
        max_age_days,
        max_records,
        max_bytes,
    };
    match store.set_retention(&bounds) {
        Ok(()) => {
            let payload = json!({
                "maxAgeDays": bounds.max_age_days,
                "maxRecords": bounds.max_records,
                "maxBytes": bounds.max_bytes,
            });
            DomainResult::receipt(
                render_envelope("history-retention", &payload),
                format!(
                    "retention configured : {} days, {} records, {} bytes",
                    bounds.max_age_days, bounds.max_records, bounds.max_bytes
                ),
            )
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history delete`: dry-run reports the transitively
/// invalidated dependents; `--apply` performs the whole deletion and
/// invalidation in one transaction.
pub fn delete(
    selection: &LoadSelection,
    scope_token: &str,
    run_id: &str,
    dry_run: bool,
    apply: bool,
) -> DomainResult {
    if dry_run == apply {
        return usage();
    }
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.delete(scope_token, run_id, dry_run) {
        Ok(report) => {
            let payload = json!({
                "applied": report.applied,
                "runId": report.run_id,
                "invalidatedDependents": report.invalidated_dependents,
                "generation": report.generation,
            });
            let human = if report.applied {
                format!(
                    "run deleted : {} dependent(s) invalidated",
                    report.invalidated_dependents.len()
                )
            } else {
                format!(
                    "delete dry run : {} dependent(s) would be invalidated",
                    report.invalidated_dependents.len()
                )
            };
            DomainResult::receipt(render_envelope("history-delete", &payload), human)
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history prune`: enforce retention — dry-run lists victims,
/// `--apply` deletes them with dependent invalidation transactionally.
pub fn prune(
    selection: &LoadSelection,
    scope_token: &str,
    dry_run: bool,
    apply: bool,
) -> DomainResult {
    if dry_run == apply {
        return usage();
    }
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.prune(scope_token, dry_run) {
        Ok(report) => {
            let payload = json!({
                "applied": report.applied,
                "deletedRuns": report.deleted_runs,
                "invalidatedDependents": report.invalidated_dependents,
                "generation": report.generation,
            });
            let human = if report.applied {
                format!(
                    "retention applied : {} run(s) deleted, {} dependent(s) invalidated",
                    report.deleted_runs.len(),
                    report.invalidated_dependents.len()
                )
            } else {
                format!(
                    "prune dry run : {} run(s) would be deleted",
                    report.deleted_runs.len()
                )
            };
            DomainResult::receipt(render_envelope("history-prune", &payload), human)
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history clear --apply`: clear one whole scope through the
/// same transactional delete/invalidation path.
pub fn clear(selection: &LoadSelection, scope_token: &str, apply: bool) -> DomainResult {
    if !apply {
        return usage();
    }
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.clear(scope_token) {
        Ok((removed, invalidated)) => {
            let payload = json!({
                "removed": removed,
                "invalidatedDependents": invalidated,
            });
            DomainResult::receipt(
                render_envelope("history-clear", &payload),
                format!("scope cleared : {removed} run(s) removed"),
            )
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history recover`: verify integrity, foreign keys, identity,
/// and every record digest, then rebuild the secondary index only from
/// validated surviving records.
pub fn recover(selection: &LoadSelection) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.recover() {
        Ok(report) => {
            let payload = serde_json::to_value(&report.document).unwrap_or(json!({}));
            let envelope = json!({
                "store": payload,
                "runCount": report.run_count,
                "assertionSetCount": report.assertion_set_count,
                "rebuiltIndexRows": report.rebuilt_index_rows,
                "verifiedDigests": report.verified_digests,
            });
            DomainResult::receipt(
                render_envelope("history-recover", &envelope),
                format!(
                    "recovery verified : {} record(s), index rebuilt",
                    report.run_count
                ),
            )
        }
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history compact`: `VACUUM` after deletes have committed; a
/// failure is reported separately and never undoes logical deletion.
pub fn compact(selection: &LoadSelection) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.compact() {
        Ok(()) => DomainResult::receipt(
            render_envelope("history-compact", &json!({ "compacted": true })),
            "history store compacted".to_owned(),
        ),
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history dependents register`: bind one local dependent
/// reference (index/claim/aggregate-input) to live source records.
pub fn dependent_register(
    selection: &LoadSelection,
    scope_token: &str,
    dependent_id: &str,
    kind: &str,
    run_sources: &[String],
    dependent_sources: &[String],
) -> DomainResult {
    let Some(kind) = types::DependentKind::parse(kind) else {
        return failure(
            Status::Invalid,
            vec![Diagnostic::new(codes::INPUT_INVALID)
                .with_data(data(&[("detail", "dependent-kind")]))],
        );
    };
    if run_sources.is_empty() && dependent_sources.is_empty() {
        return usage();
    }
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let mut store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.register_dependent(
        scope_token,
        dependent_id,
        kind,
        run_sources,
        dependent_sources,
    ) {
        Ok(()) => DomainResult::receipt(
            render_envelope(
                "history-dependent-registered",
                &json!({ "id": dependent_id, "kind": kind.as_str(), "state": "valid" }),
            ),
            format!("dependent registered : {dependent_id}"),
        ),
        Err(error) => failure_from_store(error),
    }
}

/// `lekalo history dependents resolve`: live revalidation of one
/// dependent reference — every bound digest must resolve to live
/// same-scope bytes right now.
pub fn dependent_resolve(
    selection: &LoadSelection,
    scope_token: &str,
    dependent_id: &str,
) -> DomainResult {
    let root = match crate::loader::root_for_selection(selection) {
        Err(result) => return result,
        Ok(root) => root,
    };
    let store = match store::Store::open(&root) {
        Ok(store) => store,
        Err(error) => return failure_from_store(error),
    };
    match store.resolve_dependent(scope_token, dependent_id) {
        Ok(resolution) => {
            let sources: Vec<serde_json::Value> = resolution
                .sources
                .iter()
                .map(|source| {
                    json!({
                        "runId": source.run_id,
                        "recordDigest": source.record_digest,
                        "assertionDigest": source.assertion_digest,
                    })
                })
                .collect();
            let payload = json!({
                "id": resolution.id,
                "kind": resolution.kind.as_str(),
                "state": "valid",
                "generation": resolution.generation,
                "sourceRuns": sources,
            });
            DomainResult::receipt(
                render_envelope("history-dependent-resolved", &payload),
                format!("dependent resolved live : {}", resolution.id),
            )
        }
        Err(error) => failure_from_store(error),
    }
}

// ---------------------------------------------------------------------------
// Failure mapping
// ---------------------------------------------------------------------------

/// Map one store error class onto exactly one stable diagnostic and
/// status. Details are fixed tokens; no rejected value, absolute path,
/// secret, timestamp, or raw SQLite/Git error ever appears.
fn failure_from_store(error: StoreError) -> DomainResult {
    let (status, code, detail) = match &error {
        StoreError::Denied { code, detail } => (Status::Denied, *code, *detail),
        StoreError::Invalid { code, detail } => (Status::Invalid, *code, *detail),
        StoreError::Corrupt(detail) => (Status::Invalid, codes::CORRUPT, *detail),
        StoreError::UnsupportedVersion => {
            return failure(
                Status::UnsupportedVersion,
                vec![Diagnostic::new(codes::VERSION_UNSUPPORTED)],
            );
        }
        StoreError::Busy => (Status::Unavailable, codes::BUSY, "locked"),
        StoreError::Io => (Status::Unavailable, codes::IO, "io"),
        StoreError::Conflict => (Status::Invalid, codes::RUN_CONFLICT, "bytes"),
        StoreError::LimitExceeded => (Status::Invalid, codes::RETENTION_LIMIT, "bound"),
        StoreError::ScopeUnknown => (Status::Denied, codes::SCOPE_MISMATCH, "scope-unknown"),
        StoreError::ParentMissing => (Status::Invalid, codes::INPUT_INVALID, "parent-missing"),
        StoreError::SourceMissing(detail) => (Status::Invalid, codes::SOURCE_MISSING, *detail),
        StoreError::DependentInvalidated => {
            (Status::Invalid, codes::DEPENDENT_INVALIDATED, "invalidated")
        }
        StoreError::Cycle => (Status::Invalid, codes::INPUT_INVALID, "dependency-cycle"),
        StoreError::DependentExists => (Status::Invalid, codes::INPUT_INVALID, "dependent-exists"),
        StoreError::CursorStale => (Status::Invalid, codes::CURSOR_STALE, "cursor"),
        StoreError::PolicyMismatch => (Status::Invalid, codes::POLICY_MISMATCH, "refs"),
    };
    failure(
        status,
        vec![Diagnostic::new(code).with_data(data(&[("detail", detail)]))],
    )
}

/// Map one validation violation onto its stable diagnostic.
fn failure_from_violation(violation: validate::Violation) -> DomainResult {
    if violation.code == codes::VERSION_UNSUPPORTED {
        return failure(
            Status::UnsupportedVersion,
            vec![Diagnostic::new(codes::VERSION_UNSUPPORTED)],
        );
    }
    failure(
        Status::Invalid,
        vec![Diagnostic::new(violation.code).with_data(data(&[("detail", violation.detail)]))],
    )
}

fn data(fields: &[(&str, &str)]) -> serde_json::Value {
    let mut value = serde_json::Map::new();
    for (name, token) in fields {
        value.insert((*name).to_owned(), json!(token));
    }
    serde_json::Value::Object(value)
}

fn usage() -> DomainResult {
    DomainResult::usage_error()
}

/// The fixed success envelope: `status`, `operation`, then the bounded
/// payload — local operator presentation, never a public payload.
fn render_envelope(operation: &str, payload: &serde_json::Value) -> String {
    let value = json!({
        "status": "valid",
        "operation": operation,
        "result": payload,
    });
    crate::privacy::canonical::canonical(&value)
}

fn parse_operation_kind_name(kind: types::OperationKind) -> &'static str {
    match kind {
        types::OperationKind::Scan => "scan",
        types::OperationKind::Verify => "verify",
        types::OperationKind::Context => "context",
        types::OperationKind::Generate => "generate",
        types::OperationKind::Nfr => "nfr",
        types::OperationKind::Scenario => "scenario",
        types::OperationKind::Gate => "gate",
        types::OperationKind::Evaluation => "evaluation",
    }
}
