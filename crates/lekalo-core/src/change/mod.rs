//! The semantic change service (issue #73).
//!
//! A target-independent, pure preparation core plus a separately invoked
//! transaction writer. [`prepare`] turns one closed request batch into a
//! sealed [`PreparedChange`]: it resolves every operation against typed
//! semantic identities (never text search), builds and revalidates one
//! candidate project, computes the owner-produced semantic diff, the exact
//! bounded file diff, and the impact and generation projections, and pins
//! every input byte by digest. [`apply_prepared`] is the only writer: one
//! transaction, one commit point, durable idempotency, fail-closed readers
//! during recovery. Preview never writes; CLI sugar and a future MCP tool
//! consume the same request, plan, and receipt types.

pub mod catalog;
pub mod diagnostic;
pub mod edit;
pub mod plan;
pub mod policy;
pub mod receipt;
pub mod request;
pub mod transaction;
pub mod version;

use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use crate::diff;
use crate::ir;
use crate::loader::error::SpanPos;
use crate::loader::frontends::{MapEntry, Node, Scalar, Span, Value};
use crate::loader::{LoadedDocument, NormalizedModel};
use crate::result::DomainResult;
use crate::validator::ValidationProfile;

pub use catalog::{Catalog, CatalogEntry};
pub use diagnostic::ChangeError;
pub use plan::PreparedChange;
pub use policy::Policy;
pub use receipt::{Receipt, ReceiptState};
pub use request::{ChangeRequest, Literal, Operation, RequestInvalid, TypeRef};
pub use version::{
    MAX_FILE_DIFF_BYTES, MAX_LITERAL_DEPTH, MAX_OPERATIONS, MAX_PATCH_LINES, MAX_REQUEST_BYTES,
};

use edit::ByteEdit;
use plan::{
    AffectedEntry, FileDiff, FileEdit, InputPin, ResolvedOperation, SeedEntry, SemanticDiffSummary,
    SubjectWire,
};
use receipt::{FileOutcome, History, HistoryAnchor, HistoryRecord, TransactionState};
use transaction::ReplacementPlan;

/// The maximum type-reference depth, shared with the accepted Model grammar.
pub use crate::ir::MAX_TYPE_LEVELS;

impl From<request::RequestInvalid> for ChangeError {
    fn from(invalid: request::RequestInvalid) -> Self {
        match invalid {
            request::RequestInvalid::OverLimit => ChangeError::RequestInvalid {
                detail: "over-limit",
            },
            request::RequestInvalid::Malformed => ChangeError::RequestInvalid {
                detail: "malformed",
            },
            request::RequestInvalid::UnsupportedVersion => ChangeError::UnsupportedVersion {
                version: String::new(),
            },
            request::RequestInvalid::Shape(detail)
            | request::RequestInvalid::Operation { detail, .. } => {
                ChangeError::RequestInvalid { detail }
            }
        }
    }
}

/// The literal request discriminator carried by a wrong-version refusal.
fn request_version_literal(bytes: &[u8]) -> String {
    serde_json::from_slice::<Json>(bytes)
        .ok()
        .and_then(|value| {
            value
                .get("schemaVersion")
                .and_then(Json::as_str)
                .map(|text| {
                    if text.len() <= 64 {
                        text.to_owned()
                    } else {
                        String::new()
                    }
                })
        })
        .unwrap_or_default()
}

/// The empty-bytes sentinel digest for canonical homes that do not exist
/// yet (the history sidecar before its first commit).
pub(crate) fn absent_digest() -> String {
    format!("sha256:{}", crate::digest::sha256_hex(b""))
}

/// Whether one candidate symbol id satisfies the semantic-id grammar the
/// loader already enforces; request decoding refuses everything else.
pub(crate) fn symbol_id_valid(id: &str) -> bool {
    let segments: Vec<&str> = id.split('.').collect();
    (2..=4).contains(&segments.len()) && segments.iter().all(|segment| field_name_valid(segment))
}

/// Whether one candidate member name satisfies the member grammar.
pub(crate) fn field_name_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    !name.is_empty()
        && name.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// Validate one `sha256:<64hex>` digest literal.
pub(crate) fn expect_digest(text: &str) -> Option<String> {
    let tail = text.strip_prefix("sha256:")?;
    (tail.len() == 64
        && tail
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    .then(|| text.to_owned())
}

/// The options of one pure preparation run.
#[derive(Clone, Debug)]
pub struct PrepareOptions<'a> {
    /// The explicit project selection (`None` discovers upward).
    pub project: Option<&'a str>,
    /// The exact request bytes.
    pub request: &'a [u8],
    /// The optional policy bytes.
    pub policy: Option<&'a [u8]>,
    /// Explicit acknowledgement of reported breaking classes.
    pub allow_breaking: bool,
}

/// The options of one apply run over an already-sealed plan.
#[derive(Clone, Debug)]
pub struct ApplyOptions<'a> {
    /// The durable idempotency key (raw; only its digest is ever stored).
    pub idempotency_key: &'a str,
    /// The expected plan identity; a mismatch refuses before mutation.
    pub expect_plan: Option<&'a str>,
}

/// The options of one full apply run over raw request bytes.
#[derive(Clone, Debug)]
pub struct ApplyRequestOptions<'a> {
    /// The explicit project selection (`None` discovers upward).
    pub project: Option<&'a str>,
    /// The exact request bytes.
    pub request: &'a [u8],
    /// The optional policy bytes.
    pub policy: Option<&'a [u8]>,
    /// Explicit acknowledgement of reported breaking classes.
    pub allow_breaking: bool,
    /// The durable idempotency key.
    pub idempotency_key: &'a str,
    /// The expected plan identity; a mismatch refuses before mutation.
    pub expect_plan: Option<&'a str>,
}

/// Prepare one request batch: pure, read-only, deterministic.
pub fn prepare(options: &PrepareOptions) -> Result<PreparedChange, ChangeError> {
    let request = ChangeRequest::decode(options.request).map_err(|invalid| match invalid {
        RequestInvalid::OverLimit => ChangeError::RequestInvalid {
            detail: "over-limit",
        },
        RequestInvalid::Malformed => ChangeError::RequestInvalid {
            detail: "malformed",
        },
        RequestInvalid::UnsupportedVersion => ChangeError::UnsupportedVersion {
            version: request_version_literal(options.request),
        },
        RequestInvalid::Shape(detail) => ChangeError::RequestInvalid { detail },
        RequestInvalid::Operation { detail, .. } => ChangeError::RequestInvalid { detail },
    })?;
    let policy = match options.policy {
        None => Policy::restrictive(),
        Some(bytes) => Policy::decode(bytes)?,
    };
    let root = resolve_root(options.project)?;
    refuse_while_recovery_pending(&root)?;
    let load = crate::loader::load_validated_root(root.clone()).map_err(ChangeError::LoadFailed)?;
    let project_id = load
        .model
        .project
        .as_ref()
        .map(|project| project.id.clone())
        .ok_or(ChangeError::SubjectConflict {
            subject: "project".to_owned(),
            detail: "project-missing",
        })?;
    let base_compiled = ir::compile(&load.model)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?;

    // Resolve every operation against the typed model; unsupported
    // cataloged operations fail the whole batch before any candidate work.
    let mut resolved_rows = Vec::with_capacity(request.operations.len());
    for operation in &request.operations {
        resolved_rows.push(resolve_operation(&load.model, operation)?);
    }

    // Build the candidate documents with typed tree edits.
    let mut documents = load.documents.clone();
    let mut per_file_edits: Vec<(String, Vec<ByteEdit>)> = Vec::new();
    for (row, operation) in resolved_rows.iter().zip(&request.operations) {
        if row.status != "resolved" {
            return Err(ChangeError::OperationUnsupported {
                operation: operation.operation().to_owned(),
            });
        }
        apply_tree_edit(&mut documents, operation)?;
    }

    // Plan the exact byte edits for every changed document. The edits are
    // planned against the ORIGINAL pinned bytes and spans; the mutated
    // candidate trees above exist only for the semantic comparison.
    let mut changed_documents: Vec<LoadedDocument> = Vec::new();
    for operation in &request.operations {
        let subject = operation
            .subject()
            .ok_or(ChangeError::RequestInvalid {
                detail: "subject-missing",
            })?
            .to_owned();
        let definition = load
            .model
            .definitions
            .iter()
            .find(|definition| definition.id == subject)
            .ok_or(ChangeError::SubjectConflict {
                subject,
                detail: "definition-missing",
            })?;
        let path = definition.path.clone();
        if changed_documents
            .iter()
            .any(|document| document.document.path == path)
        {
            continue;
        }
        let document = load
            .documents
            .iter()
            .find(|document| document.document.path == path)
            .ok_or(ChangeError::SubjectConflict {
                subject: path.clone(),
                detail: "document-missing",
            })?;
        let bytes = document.bytes.clone();
        let tree = &document.document;
        let tree_definition = tree
            .definitions
            .iter()
            .find(|candidate| candidate.id == definition.id)
            .ok_or(ChangeError::SubjectConflict {
                subject: definition.id.clone(),
                detail: "document-definition-missing",
            })?;
        let edits = plan_source_edits(&bytes, tree_definition, &load.model, operation)?;
        if !edits.is_empty() {
            let edited = edits.iter().fold(bytes, |bytes, edit| edit.apply(&bytes));
            let decoded = crate::loader::decode_document_bytes(
                &path,
                crate::loader::DocKind::Symbol,
                &edited,
            )
            .map_err(|diagnostics| {
                ChangeError::LoadFailed(crate::loader::diagnostic::failure(
                    crate::result::Status::Invalid,
                    diagnostics,
                ))
            })?;
            changed_documents.push(LoadedDocument {
                document: decoded,
                bytes: edited,
            });
            per_file_edits.push((path, edits));
        }
    }

    // Rebuild the candidate from the re-decoded prospective bytes and
    // assert compiled-candidate equality with the tree-edited candidate.
    let mut candidate_documents: Vec<LoadedDocument> = documents
        .iter()
        .filter(|document| {
            !changed_documents
                .iter()
                .any(|changed| changed.document.path == document.document.path)
        })
        .cloned()
        .collect();
    candidate_documents.extend(changed_documents.clone());
    let tree_candidate = build_candidate(&load, &documents)?;
    let byte_documents: Vec<crate::loader::Document> = candidate_documents
        .iter()
        .map(|document| document.document.clone())
        .collect();
    let byte_candidate =
        crate::loader::build_normalized_model(load.model.model_version, byte_documents)
            .map_err(ChangeError::LoadFailed)?;
    if ir::compile(&tree_candidate)
        .expect("candidate compiles")
        .project
        .to_canonical_json()
        != ir::compile(&byte_candidate)
            .expect("candidate compiles")
            .project
            .to_canonical_json()
    {
        return Err(ChangeError::SourcePreservation {
            subject: "candidate".to_owned(),
            detail: "reparse-mismatch",
        });
    }

    // Compile and validate the candidate across project boundaries.
    let candidate_compiled = ir::compile(&byte_candidate)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?
        .project;
    let candidate_compilation = ir::Compilation {
        project: candidate_compiled.clone(),
        source_map: ir::SourceMap::new(Vec::new()),
    };
    let profile = ValidationProfile::embedded_default()
        .map_err(|_| ChangeError::RequestInvalid { detail: "profile" })?;
    if let Err(set) = crate::validator::validate(&candidate_compilation, profile) {
        return Err(ChangeError::LoadFailed(DomainResult::invalid(set)));
    }

    // Semantic diff and compatibility against the unchanged base.
    let base_compiled = base_compiled.project;
    let diff = diff::compare(
        &base_compiled,
        &candidate_compiled,
        &diff::DiffRequest::new(),
    )
    .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let semantic_diff = SemanticDiffSummary {
        equal: diff.equal(),
        classification: diff
            .classification()
            .iter()
            .map(|class| class.key().to_owned())
            .collect(),
        changes: diff
            .changes()
            .iter()
            .map(|change| plan::ChangeRecordWire {
                change_id: change.change_id().to_owned(),
                kind: change.kind().key().to_owned(),
                subject: SubjectWire {
                    family: change.subject().family().key().to_owned(),
                    id: change.subject().id().to_owned(),
                    member: change.subject().member().map(str::to_owned),
                },
                reasons: change.reasons().to_vec(),
            })
            .collect(),
        complete: diff.complete(),
        complete_reason: diff.complete_reason().map(str::to_owned),
    };

    // Compatibility gate: breaking classes need policy or explicit
    // acknowledgement. Invalid candidates never reach this line.
    let mut denied_breaking_classes = Vec::new();
    for class in &semantic_diff.classification {
        if policy::BREAKING_CLASSES.contains(&class.as_str())
            && !policy.allows(class)
            && !options.allow_breaking
        {
            denied_breaking_classes.push(class.clone());
        }
    }
    denied_breaking_classes.sort();

    // Impact projection from the typed diff seeds on the matching graphs.
    let impact = project_impact(&base_compiled, &candidate_compiled, &diff);

    // Generation projection: declared targets, never executed here.
    let generation = plan::GenerationProjection {
        executed: false,
        targets: declared_targets(&root)?,
        note: "plan-only; run `lekalo generate --locked` to execute".to_owned(),
    };

    // Complete input inventory with exact byte pins.
    let mut inputs: Vec<InputPin> = load
        .documents
        .iter()
        .map(|document| InputPin {
            path: document.document.path.clone(),
            digest: format!("sha256:{}", crate::digest::sha256_hex(&document.bytes)),
        })
        .collect();
    let history_path = version::HISTORY_LOGICAL_PATH.to_owned();
    let history_bytes =
        std::fs::read(root.join(history_path.replace('/', std::path::MAIN_SEPARATOR_STR)))
            .unwrap_or_default();
    let history_present = !history_bytes.is_empty();
    inputs.push(InputPin {
        path: history_path,
        digest: if history_present {
            format!("sha256:{}", crate::digest::sha256_hex(&history_bytes))
        } else {
            absent_digest()
        },
    });
    inputs.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

    // Optional exact Model precondition.
    let model_digest = format!(
        "sha256:{}",
        crate::digest::sha256_hex(
            inputs
                .iter()
                .map(|pin| format!("{}\u{1f}{}\n", pin.path, pin.digest))
                .collect::<String>()
                .as_bytes()
        )
    );
    if let Some(expected) = &request.expect_model {
        if expected != &model_digest {
            return Err(ChangeError::PreconditionFailed {
                subject: "model".to_owned(),
                detail: "expect-model",
            });
        }
    }

    let file_edits: Vec<FileEdit> = per_file_edits
        .iter()
        .map(|(path, edits)| {
            let original = load
                .documents
                .iter()
                .find(|document| &document.document.path == path)
                .expect("pinned document");
            let before = &original.bytes;
            let after = edits
                .iter()
                .fold(before.clone(), |bytes: Vec<u8>, edit| edit.apply(&bytes));
            FileEdit {
                path: path.clone(),
                before_digest: format!("sha256:{}", crate::digest::sha256_hex(before)),
                after_digest: format!("sha256:{}", crate::digest::sha256_hex(&after)),
            }
        })
        .collect();
    let mut file_diffs = Vec::new();
    let mut diff_complete = true;
    for (path, edits) in &per_file_edits {
        let original = load
            .documents
            .iter()
            .find(|document| &document.document.path == path)
            .expect("pinned document");
        let before =
            std::str::from_utf8(&original.bytes).map_err(|_| ChangeError::SourcePreservation {
                subject: path.clone(),
                detail: "encoding",
            })?;
        let after_bytes = edits
            .iter()
            .fold(original.bytes.clone(), |bytes: Vec<u8>, edit| {
                edit.apply(&bytes)
            });
        let after =
            std::str::from_utf8(&after_bytes).map_err(|_| ChangeError::SourcePreservation {
                subject: path.clone(),
                detail: "encoding",
            })?;
        if before.len() > version::MAX_FILE_DIFF_BYTES || after.len() > version::MAX_FILE_DIFF_BYTES
        {
            diff_complete = false;
            continue;
        }
        let (patch, patch_complete) = plan::compute_patch(before, after);
        if !patch_complete {
            diff_complete = false;
        }
        file_diffs.push(FileDiff {
            path: path.clone(),
            before: before.to_owned(),
            after: after.to_owned(),
            patch,
        });
    }
    file_diffs.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));

    let prepared = PreparedChange {
        // The canonical re-serialization pins the request intent, not its
        // presentation: pretty file JSON and compact sugar bytes of one
        // request carry the same digest and the same plan identity.
        request_bytes: request.to_canonical_json_string().into_bytes(),
        policy,
        operations: resolved_rows,
        inputs,
        file_edits,
        file_diffs,
        semantic_diff,
        impact,
        generation,
        denied_breaking_classes,
        mode: "preview",
        project_id,
        edits: per_file_edits,
        root,
        diff_complete,
    };
    if !prepared.denied_breaking_classes.is_empty() {
        return Err(ChangeError::CompatibilityDenied {
            classes: prepared.denied_breaking_classes.clone(),
        });
    }
    Ok(prepared)
}

/// Apply raw request bytes with ledger-first idempotency (see
/// [`ApplyRequestOptions`]).
pub fn apply_request(options: &ApplyRequestOptions) -> Result<Receipt, ChangeError> {
    if options.idempotency_key.is_empty() || options.idempotency_key.len() > 128 {
        return Err(ChangeError::RequestInvalid {
            detail: "idempotency-key-bounds",
        });
    }
    let root = resolve_root(options.project)?;
    refuse_while_recovery_pending(&root)?;
    // The durable ledger answers before any preparation runs: the Model
    // already carries the committed change, so a fresh resolve would refuse
    // as a duplicate. Replay must still work.
    let key = receipt::key_digest(options.idempotency_key);
    let history_bytes = std::fs::read(
        root.join(version::HISTORY_LOGICAL_PATH.replace('/', std::path::MAIN_SEPARATOR_STR)),
    )
    .unwrap_or_default();
    if !history_bytes.is_empty() {
        let history = History::decode(&history_bytes)?;
        if let Some(record) = history.by_key(&key) {
            // The ledger binds canonical request bytes, so a repeated key
            // presented as pretty file JSON and as compact sugar bytes of
            // one request replays instead of conflicting.
            let request_digest = ChangeRequest::decode(options.request)
                .map(|request| {
                    format!(
                        "sha256:{}",
                        crate::digest::sha256_hex(request.to_canonical_json_string().as_bytes())
                    )
                })
                .map_err(ChangeError::from)?;
            if record.request_digest != request_digest {
                return Err(ChangeError::IdempotencyConflict);
            }
            return replayed_receipt(record);
        }
    }
    let prepared = prepare(&PrepareOptions {
        project: options.project,
        request: options.request,
        policy: options.policy,
        allow_breaking: options.allow_breaking,
    })?;
    apply_prepared(
        &prepared,
        &ApplyOptions {
            idempotency_key: options.idempotency_key,
            expect_plan: options.expect_plan,
        },
    )
}

/// Apply one sealed plan: the only writer of the change service.
pub fn apply_prepared(
    prepared: &PreparedChange,
    options: &ApplyOptions,
) -> Result<Receipt, ChangeError> {
    if options.idempotency_key.is_empty() || options.idempotency_key.len() > 128 {
        return Err(ChangeError::RequestInvalid {
            detail: "idempotency-key-bounds",
        });
    }
    if let Some(expected) = options.expect_plan {
        if expected != prepared.plan_id() {
            return Err(ChangeError::PreconditionFailed {
                subject: "plan".to_owned(),
                detail: "expect-plan",
            });
        }
    }
    let root = prepared.root.clone();
    let _lock = transaction::WriterLock::acquire(&root)?;
    refuse_while_recovery_pending(&root)?;

    // The durable ledger is authoritative: replay first, conflict second.
    let history_logical = version::HISTORY_LOGICAL_PATH;
    let history_file = root.join(history_logical.replace('/', std::path::MAIN_SEPARATOR_STR));
    let history_bytes = std::fs::read(&history_file).unwrap_or_default();
    let mut history = if history_bytes.is_empty() {
        History {
            project: prepared.project_id.clone(),
            next_sequence: 1,
            records: Vec::new(),
        }
    } else {
        History::decode(&history_bytes)?
    };
    let key = receipt::key_digest(options.idempotency_key);
    if let Some(record) = history.by_key(&key) {
        if record.request_digest != prepared.request_digest() {
            return Err(ChangeError::IdempotencyConflict);
        }
        return replayed_receipt(record);
    }

    // Re-verify the complete pinned input set before any mutation (the
    // prepare-to-apply CAS point, including the history sidecar itself).
    for pin in &prepared.inputs {
        let bytes = std::fs::read(root.join(pin.path.replace('/', std::path::MAIN_SEPARATOR_STR)))
            .unwrap_or_default();
        let digest = if bytes.is_empty() && pin.path == history_logical {
            absent_digest()
        } else {
            format!("sha256:{}", crate::digest::sha256_hex(&bytes))
        };
        if digest != pin.digest {
            return Err(ChangeError::PreconditionFailed {
                subject: pin.path.clone(),
                detail: "input-digest",
            });
        }
    }

    // The compatibility gate re-evaluates under the sealed plan's own
    // policy pins: denied classes refuse before any mutation.
    if !prepared.denied_breaking_classes.is_empty() {
        return Err(ChangeError::CompatibilityDenied {
            classes: prepared.denied_breaking_classes.clone(),
        });
    }

    // The source replacement plans, in canonical path order.
    let mut plans: Vec<ReplacementPlan> = prepared
        .edits
        .iter()
        .map(|(path, edits)| {
            let before = std::fs::read(root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)))
                .unwrap_or_default();
            let after = edits
                .iter()
                .fold(before.clone(), |bytes: Vec<u8>, edit| edit.apply(&bytes));
            ReplacementPlan {
                logical: path.clone(),
                before_digest: format!("sha256:{}", crate::digest::sha256_hex(&before)),
                after_digest: format!("sha256:{}", crate::digest::sha256_hex(&after)),
                edits: edits.clone(),
                creation: false,
            }
        })
        .collect();
    let files: Vec<FileOutcome> = plans
        .iter()
        .map(|plan| FileOutcome {
            path: plan.logical.clone(),
            before_digest: plan.before_digest.clone(),
            after_digest: plan.after_digest.clone(),
            outcome: "replaced".to_owned(),
        })
        .collect();

    // The committed (or no-op) receipt and its history record.
    let is_no_op = prepared.is_no_op();
    let state = if is_no_op {
        ReceiptState::NoOp
    } else {
        ReceiptState::Committed
    };
    let sequence = history.next_sequence;
    // Eight hex characters after the `sha256:` prefix: the transaction id
    // must stay a portable directory name.
    let transaction_id = format!("change-{}-{}", &prepared.plan_id()[7..15], sequence);
    let anchor = HistoryAnchor {
        sequence,
        record_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                format!(
                    "{}{}{}{}",
                    key,
                    prepared.request_digest(),
                    prepared.plan_id(),
                    state.as_str()
                )
                .as_bytes()
            )
        ),
    };
    let receipt = receipt::Receipt {
        status: "valid".to_owned(),
        state: state.as_str().to_owned(),
        plan_id: Some(prepared.plan_id()),
        request_digest: Some(prepared.request_digest()),
        key_digest: Some(key.clone()),
        operations: prepared.operations.clone(),
        files,
        history: Some(anchor),
        transaction: Some(receipt::TransactionWire {
            id: transaction_id.clone(),
            state: TransactionState::Committed.as_str().to_owned(),
        }),
        diagnostic_ids: Vec::new(),
        replayed: false,
    };
    let record = HistoryRecord {
        sequence,
        key_digest: key,
        request_digest: prepared.request_digest(),
        plan_id: prepared.plan_id(),
        operations: prepared
            .operations
            .iter()
            .map(|operation| operation.operation.clone())
            .collect(),
        files: prepared
            .edits
            .iter()
            .map(|(path, _)| path.clone())
            .collect(),
        receipt: receipt
            .history_projection()
            .ok_or(ChangeError::HistoryInvalid {
                detail: "projection",
            })?,
    };
    history = history.with_appended(record);
    let final_history_bytes = history.to_json_string().into_bytes();

    // The history sidecar replacement joins the same transaction so the
    // ledger and the sources commit together; source replacements restore
    // before it on ordinary failure.
    plans.push(ReplacementPlan {
        logical: history_logical.to_owned(),
        before_digest: if history_bytes.is_empty() {
            absent_digest()
        } else {
            format!("sha256:{}", crate::digest::sha256_hex(&history_bytes))
        },
        after_digest: format!("sha256:{}", crate::digest::sha256_hex(&final_history_bytes)),
        edits: vec![ByteEdit {
            start: 0,
            end: history_bytes.len(),
            replacement: String::from_utf8(final_history_bytes.clone())
                .map_err(|_| ChangeError::HistoryInvalid { detail: "encoding" })?,
        }],
        creation: history_bytes.is_empty(),
    });

    let mut transaction =
        transaction::Transaction::begin(&root, &transaction_id, &prepared.plan_id(), &plans)?;
    for (index, plan) in plans.iter().enumerate() {
        if let Err(error) = transaction.stage(index, plan) {
            transaction.cleanup_after_restore();
            return Err(error);
        }
    }
    if let Err(error) = transaction.replace_all() {
        transaction.cleanup_after_restore();
        return Err(error);
    }
    transaction.finalize(&final_history_bytes)?;
    Ok(receipt)
}

/// Look up the committed receipt of one idempotency key without mutating.
pub fn lookup_receipt(
    project: Option<&str>,
    idempotency_key: &str,
) -> Result<Option<Receipt>, ChangeError> {
    let root = resolve_root(project)?;
    let history_bytes = std::fs::read(
        root.join(version::HISTORY_LOGICAL_PATH.replace('/', std::path::MAIN_SEPARATOR_STR)),
    )
    .unwrap_or_default();
    if history_bytes.is_empty() {
        return Ok(None);
    }
    let history = History::decode(&history_bytes)?;
    let key = receipt::key_digest(idempotency_key);
    Ok(history.by_key(&key).map(stored_receipt))
}

/// Recover one interrupted transaction explicitly; readers stay blocked
/// until this succeeds and the journal is gone.
pub fn recover_transaction(project: Option<&str>, transaction: &str) -> Result<(), ChangeError> {
    let root = resolve_root(project)?;
    let _lock = transaction::WriterLock::acquire(&root)?;
    transaction::recover(&root, transaction)
}

/// The logical transaction ids currently awaiting recovery.
pub fn pending_transactions(project: Option<&str>) -> Result<Vec<String>, ChangeError> {
    let root = resolve_root(project)?;
    Ok(transaction::pending_transactions(&root))
}

/// Rebuild the replayed receipt from its stored record.
fn replayed_receipt(record: &HistoryRecord) -> Result<Receipt, ChangeError> {
    let mut receipt = stored_receipt(record);
    receipt.state = ReceiptState::Replayed.as_str().to_owned();
    receipt.replayed = true;
    Ok(receipt)
}

/// Parse one stored record receipt back into the typed wire.
fn stored_receipt(record: &HistoryRecord) -> Receipt {
    let value: Json = record.receipt.clone();
    let operations: Vec<ResolvedOperation> = value
        .get("operations")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let files: Vec<FileOutcome> = value
        .get("files")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let state: String = value
        .get("state")
        .and_then(Json::as_str)
        .unwrap_or("committed")
        .to_owned();
    Receipt {
        status: "valid".to_owned(),
        state,
        plan_id: value
            .get("planId")
            .and_then(Json::as_str)
            .map(str::to_owned),
        request_digest: value
            .get("requestDigest")
            .and_then(Json::as_str)
            .map(str::to_owned),
        key_digest: value
            .get("keyDigest")
            .and_then(Json::as_str)
            .map(str::to_owned),
        operations,
        files,
        history: value
            .get("history")
            .and_then(|history| serde_json::from_value(history.clone()).ok()),
        transaction: value
            .get("transaction")
            .and_then(|transaction| serde_json::from_value(transaction.clone()).ok()),
        diagnostic_ids: value
            .get("diagnosticIds")
            .and_then(Json::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        replayed: false,
    }
}

/// Resolve one operation against the typed model: exact identity, exact
/// kind, exact member; no text search of any kind.
fn resolve_operation(
    model: &NormalizedModel,
    operation: &Operation,
) -> Result<ResolvedOperation, ChangeError> {
    let (subject, supported) = match operation {
        Operation::EntityFieldAdd { entity, field } => {
            let definition = find_definition(model, entity)?;
            check_field_kind(definition)?;
            if definition_member_exists(&definition.node, "fields", &field.name) {
                return Err(ChangeError::SubjectConflict {
                    subject: entity.clone(),
                    detail: "field-duplicate",
                });
            }
            if field.default.is_some() {
                return Err(ChangeError::UnsupportedMember {
                    operation: operation.operation().to_owned(),
                    member: "field-default",
                });
            }
            (entity.clone(), true)
        }
        Operation::EntityFieldRemove { entity, name } => {
            let definition = find_definition(model, entity)?;
            check_field_kind(definition)?;
            if !definition_member_exists(&definition.node, "fields", name) {
                return Err(ChangeError::SubjectConflict {
                    subject: name.clone(),
                    detail: "field-missing",
                });
            }
            (entity.clone(), true)
        }
        Operation::EntityFieldRename { entity, name, to } => {
            let definition = find_definition(model, entity)?;
            check_field_kind(definition)?;
            if !definition_member_exists(&definition.node, "fields", name) {
                return Err(ChangeError::SubjectConflict {
                    subject: name.clone(),
                    detail: "field-missing",
                });
            }
            if definition_member_exists(&definition.node, "fields", to) {
                return Err(ChangeError::SubjectConflict {
                    subject: to.clone(),
                    detail: "field-duplicate",
                });
            }
            (entity.clone(), true)
        }
        Operation::EnumCaseAdd { enum_id, case } => {
            let definition = find_definition(model, enum_id)?;
            if definition_kind_of(definition) != "enum" {
                return Err(ChangeError::SubjectConflict {
                    subject: enum_id.clone(),
                    detail: "wrong-kind",
                });
            }
            if definition_member_exists(&definition.node, "values", &case.name) {
                return Err(ChangeError::SubjectConflict {
                    subject: enum_id.clone(),
                    detail: "case-duplicate",
                });
            }
            if case.value.is_some() {
                return Err(ChangeError::UnsupportedMember {
                    operation: operation.operation().to_owned(),
                    member: "case-wire-value",
                });
            }
            (enum_id.clone(), true)
        }
        Operation::Unsupported { subject, .. } => (subject.clone().unwrap_or_default(), false),
    };
    Ok(ResolvedOperation {
        operation: operation.operation().to_owned(),
        subject,
        subject_family: family_for(model, operation),
        status: if supported { "resolved" } else { "unsupported" }.to_owned(),
        detail: if supported {
            None
        } else {
            Some("not-provided".to_owned())
        },
    })
}

/// The wire family of the operation subject.
fn family_for(model: &NormalizedModel, operation: &Operation) -> String {
    match operation {
        Operation::Unsupported { .. } => "none".to_owned(),
        _ => operation
            .subject()
            .and_then(|subject| {
                model
                    .definitions
                    .iter()
                    .find(|definition| definition.id == subject)
            })
            .map(definition_kind_of)
            .unwrap_or_else(|| "none".to_owned()),
    }
}

/// The kind of one model definition as the closed family token.
fn definition_kind_of(definition: &crate::loader::NormalizedDefinition) -> String {
    definition
        .node
        .get("kind")
        .and_then(crate::loader::frontends::Node::as_str)
        .unwrap_or("none")
        .to_owned()
}

/// Find one definition by exact typed identity.
fn find_definition<'model>(
    model: &'model NormalizedModel,
    id: &str,
) -> Result<&'model crate::loader::NormalizedDefinition, ChangeError> {
    model
        .definitions
        .iter()
        .find(|definition| definition.id == id)
        .ok_or(ChangeError::SubjectConflict {
            subject: id.to_owned(),
            detail: "definition-missing",
        })
}

/// Whether the definition accepts field members (entity, value-object).
fn check_field_kind(definition: &crate::loader::NormalizedDefinition) -> Result<(), ChangeError> {
    let kind = definition_kind_of(definition);
    if matches!(kind.as_str(), "entity" | "value-object") {
        Ok(())
    } else {
        Err(ChangeError::SubjectConflict {
            subject: definition.id.clone(),
            detail: "wrong-kind",
        })
    }
}

/// Whether one member sequence already carries the exact member name.
fn definition_member_exists(node: &Node, member: &str, name: &str) -> bool {
    node.get(member)
        .and_then(|sequence| sequence.as_seq())
        .map(|items| {
            items.iter().any(|item| {
                item.get("name").and_then(Node::as_str) == Some(name)
                    || item.get("value").and_then(Node::as_str) == Some(name)
            })
        })
        .unwrap_or(false)
}

/// Apply one typed tree edit to the candidate documents.
fn apply_tree_edit(
    documents: &mut [LoadedDocument],
    operation: &Operation,
) -> Result<(), ChangeError> {
    let subject = match operation {
        Operation::Unsupported { .. } => return Ok(()),
        Operation::EntityFieldAdd { entity, .. }
        | Operation::EntityFieldRemove { entity, .. }
        | Operation::EntityFieldRename { entity, .. }
        | Operation::EnumCaseAdd {
            enum_id: entity, ..
        } => entity.clone(),
    };
    // The declaring document is located through the typed definition table.
    let document = documents
        .iter_mut()
        .find(|document| {
            document
                .document
                .definitions
                .iter()
                .any(|definition| definition.id == subject)
        })
        .ok_or_else(|| ChangeError::SubjectConflict {
            subject: subject.clone(),
            detail: "document-missing",
        })?;
    let definition = document
        .document
        .definitions
        .iter_mut()
        .find(|definition| definition.id == subject)
        .ok_or(ChangeError::SubjectConflict {
            subject,
            detail: "document-definition-missing",
        })?;
    match operation {
        Operation::EntityFieldAdd { field, .. } => {
            push_member_item(&mut definition.node, "fields", field)
        }
        Operation::EnumCaseAdd { case, .. } => push_case_item(&mut definition.node, case),
        Operation::EntityFieldRemove { name, .. } => {
            remove_member_item(&mut definition.node, "fields", name)
        }
        Operation::EntityFieldRename { name, to, .. } => {
            rename_member_item(&mut definition.node, "fields", name, to)
        }
        Operation::Unsupported { .. } => Ok(()),
    }
}

/// The placeholder span of synthesized tree nodes (spans are unused for the
/// candidate's semantic payload; only canonical bytes matter).
fn placeholder_span() -> Span {
    Span::new(
        SpanPos {
            byte: 0,
            line: 1,
            column: 1,
        },
        SpanPos {
            byte: 0,
            line: 1,
            column: 1,
        },
    )
}

/// Push one synthesized field item into the member sequence.
fn push_member_item(
    node: &mut Node,
    member: &str,
    field: &request::FieldDecl,
) -> Result<(), ChangeError> {
    let span = placeholder_span();
    let mut entries = vec![
        MapEntry {
            key: "name".to_owned(),
            key_span: span,
            value: Node::scalar(Scalar::Str(field.name.clone()), span),
        },
        MapEntry {
            key: "type".to_owned(),
            key_span: span,
            value: Node::scalar(Scalar::Str(field.r#type.to_spelling()), span),
        },
    ];
    if field.required {
        entries.push(MapEntry {
            key: "required".to_owned(),
            key_span: span,
            value: Node::scalar(Scalar::Bool(true), span),
        });
    }
    if let Some(description) = &field.description {
        entries.push(MapEntry {
            key: "description".to_owned(),
            key_span: span,
            value: Node::scalar(Scalar::Str(description.clone()), span),
        });
    }
    push_seq_item(
        node,
        member,
        Node {
            span,
            value: Value::Map(entries),
        },
    )
}

/// Push one synthesized enum-case item into the values sequence.
fn push_case_item(node: &mut Node, case: &request::CaseDecl) -> Result<(), ChangeError> {
    let span = placeholder_span();
    let mut entries = vec![MapEntry {
        key: "value".to_owned(),
        key_span: span,
        value: Node::scalar(Scalar::Str(case.name.clone()), span),
    }];
    if let Some(description) = &case.description {
        entries.push(MapEntry {
            key: "description".to_owned(),
            key_span: span,
            value: Node::scalar(Scalar::Str(description.clone()), span),
        });
    }
    push_seq_item(
        node,
        "values",
        Node {
            span,
            value: Value::Map(entries),
        },
    )
}

/// Push one item into the named member sequence of the definition node.
fn push_seq_item(node: &mut Node, member: &str, item: Node) -> Result<(), ChangeError> {
    let sequence = node.get_mut(member).ok_or(ChangeError::SubjectConflict {
        subject: member.to_owned(),
        detail: "member-missing",
    })?;
    match &mut sequence.value {
        Value::Seq(items) => {
            items.push(item);
            Ok(())
        }
        _ => Err(ChangeError::SubjectConflict {
            subject: member.to_owned(),
            detail: "member-not-sequence",
        }),
    }
}

/// Remove the named item from the member sequence.
fn remove_member_item(node: &mut Node, member: &str, name: &str) -> Result<(), ChangeError> {
    let sequence = node.get_mut(member).ok_or(ChangeError::SubjectConflict {
        subject: name.to_owned(),
        detail: "member-missing",
    })?;
    let Value::Seq(items) = &mut sequence.value else {
        return Err(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-not-sequence",
        });
    };
    let position = items
        .iter()
        .position(|item| {
            item.get("name").and_then(Node::as_str) == Some(name)
                || item.get("value").and_then(Node::as_str) == Some(name)
        })
        .ok_or(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-missing",
        })?;
    items.remove(position);
    Ok(())
}

/// Rename the identity scalar of one member item in the tree.
fn rename_member_item(
    node: &mut Node,
    member: &str,
    name: &str,
    to: &str,
) -> Result<(), ChangeError> {
    let sequence = node.get_mut(member).ok_or(ChangeError::SubjectConflict {
        subject: name.to_owned(),
        detail: "member-missing",
    })?;
    let Value::Seq(items) = &mut sequence.value else {
        return Err(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-not-sequence",
        });
    };
    let item = items
        .iter_mut()
        .find(|item| item.get("name").and_then(Node::as_str) == Some(name))
        .ok_or(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-missing",
        })?;
    let entry = item.get_mut("name").ok_or(ChangeError::SubjectConflict {
        subject: name.to_owned(),
        detail: "member-missing",
    })?;
    entry.value = Value::Scalar(Scalar::Str(to.to_owned()));
    Ok(())
}

/// Rebuild the candidate normalized model from the edited documents.
fn build_candidate(
    load: &crate::loader::ProjectLoad,
    documents: &[LoadedDocument],
) -> Result<NormalizedModel, ChangeError> {
    let parsed: Vec<crate::loader::Document> = documents
        .iter()
        .map(|document| document.document.clone())
        .collect();
    crate::loader::build_normalized_model(load.model.model_version, parsed)
        .map_err(ChangeError::LoadFailed)
}

/// Plan the exact byte edits of one operation over the declaring document.
fn plan_source_edits(
    bytes: &[u8],
    definition: &crate::loader::project_docs::Definition,
    _model: &NormalizedModel,
    operation: &Operation,
) -> Result<Vec<ByteEdit>, ChangeError> {
    // A JSON-styled document is decoded by the JSON frontend; the bounded
    // increment plans YAML block edits only and refuses JSON explicitly.
    if std::str::from_utf8(bytes)
        .map(|text| text.trim_start().starts_with('{') || text.trim_start().starts_with('['))
        .unwrap_or(false)
    {
        return Err(ChangeError::SourcePreservation {
            subject: definition.id.clone(),
            detail: "json-unsupported",
        });
    }
    let node = &definition.node;
    let edit = match operation {
        Operation::EntityFieldAdd { field, .. } => {
            let item = edit::field_item_block(
                bytes,
                node,
                "fields",
                &field.name,
                &field.r#type.to_spelling(),
                field.required,
                field.description.as_deref(),
            )?;
            edit::plan_seq_insert(bytes, node, "fields", &item)?
        }
        Operation::EntityFieldRemove { name, .. } => {
            edit::plan_seq_item_remove(bytes, node, "fields", name)?
        }
        Operation::EntityFieldRename { name, to, .. } => {
            edit::plan_seq_item_rename(bytes, node, "fields", name, to)?
        }
        Operation::EnumCaseAdd { case, .. } => {
            let item = edit::case_item_block(
                bytes,
                node,
                "values",
                &case.name,
                case.description.as_deref(),
            )?;
            edit::plan_seq_insert(bytes, node, "values", &item)?
        }
        Operation::Unsupported { .. } => return Ok(Vec::new()),
    };
    Ok(vec![edit])
}

/// The typed impact projection: every diff seed analyzed on its own graph
/// side with bounded reverse reachability.
fn project_impact(
    base: &ir::CompiledProject,
    candidate: &ir::CompiledProject,
    diff: &diff::DiffResult,
) -> plan::ImpactProjection {
    let base_graph = crate::graph::build(base).ok();
    let candidate_graph = crate::graph::build(candidate).ok();
    let mut seeds = Vec::new();
    let mut affected = Vec::new();
    let mut complete = true;
    let mut seen_affected = std::collections::BTreeSet::new();
    for seed in diff.seeds() {
        let subject = seed.subject();
        let side = match seed.side() {
            diff::Side::Base => "base",
            // Candidate-side and both-sides seeds analyze the candidate.
            diff::Side::Candidate | diff::Side::Both => "candidate",
        };
        seeds.push(SeedEntry {
            family: subject.family().key().to_owned(),
            id: subject.id().to_owned(),
            member: subject.member().map(str::to_owned),
            side,
        });
        let graph = match (side, base_graph.as_ref(), candidate_graph.as_ref()) {
            ("base", Some(graph), _) => graph,
            ("candidate", _, Some(graph)) => graph,
            _ => {
                complete = false;
                continue;
            }
        };
        let Some(root) = graph.resolve_id(subject.id()) else {
            complete = false;
            continue;
        };
        let reached = reverse_closure(graph, root);
        if !reached.complete {
            complete = false;
        }
        let via: Vec<String> = reached.ids.into_iter().take(64).collect();
        for id in &via {
            seen_affected.insert((side.to_owned(), id.clone()));
        }
        affected.push(AffectedEntry {
            subject: subject.id().to_owned(),
            subject_family: subject.family().key().to_owned(),
            direction: diff_side(side),
            via,
        });
    }
    // One entry per affected subject per direction, deterministically.
    affected.retain(|entry| {
        seen_affected.contains(&(entry.direction.to_owned(), entry.subject.clone()))
    });
    plan::ImpactProjection {
        seeds,
        affected,
        complete,
    }
}

fn diff_side(side: &str) -> &'static str {
    if side == "base" {
        "base"
    } else {
        "candidate"
    }
}

/// Bounded deterministic reverse closure over the dependency graph.
fn reverse_closure(
    graph: &crate::graph::DependencyGraph,
    root: &crate::graph::NodeId,
) -> ClosureOutcome {
    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut frontier = vec![root.clone()];
    let mut complete = true;
    let filter = crate::graph::EdgeFilter::new();
    while let Some(node) = frontier.pop() {
        if visited.len() >= 256 {
            complete = false;
            break;
        }
        for edge in graph.incoming(&node, &filter) {
            let from = edge.key().from();
            if visited.insert(from.as_str().to_owned()) {
                frontier.push(from.clone());
            }
        }
    }
    visited.remove(root.as_str());
    ClosureOutcome {
        ids: visited,
        complete,
    }
}

struct ClosureOutcome {
    ids: std::collections::BTreeSet<String>,
    complete: bool,
}

/// The declared target stems of the project (generation projection input).
fn declared_targets(root: &Path) -> Result<Vec<String>, ChangeError> {
    let home = root.join("lekalo").join("targets");
    let Ok(entries) = std::fs::read_dir(&home) else {
        return Ok(Vec::new());
    };
    let mut targets = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("yaml") {
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                if targets.len() >= 64 {
                    break;
                }
                targets.push(stem.to_owned());
            }
        }
    }
    targets.sort();
    Ok(targets)
}

/// Resolve the project root through the shared selection rules.
fn resolve_root(project: Option<&str>) -> Result<PathBuf, ChangeError> {
    crate::loader::root_for_selection(&crate::loader::LoadSelection {
        project: project.map(str::to_owned),
    })
    .map_err(ChangeError::LoadFailed)
}

/// Refuse while any change journal awaits recovery (the reader barrier).
fn refuse_while_recovery_pending(root: &Path) -> Result<(), ChangeError> {
    match transaction::pending_transactions(root).first() {
        Some(transaction) => Err(ChangeError::RecoveryRequired {
            transaction: transaction.clone(),
        }),
        None => Ok(()),
    }
}

/// The loader-facing barrier: `Some("change.recovery-required")` while a
/// change journal exists under the project cache.
pub(crate) fn recovery_barrier_code(fs: &crate::project_fs::Fs) -> Option<&'static str> {
    transaction::recovery_barrier_code(fs)
}
