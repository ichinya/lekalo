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
pub use receipt::{key_digest, Receipt, ReceiptState};
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

/// The exact bytes of the canonical history sidecar: `None` when the file
/// does not exist. Any other read failure is corruption and fails closed
/// (round 2, finding 8): an existing empty ledger is decoded by
/// [`History::decode`] and refused there.
fn read_history_bytes(root: &Path) -> Result<Option<Vec<u8>>, ChangeError> {
    let path = root.join(version::HISTORY_LOGICAL_PATH.replace('/', std::path::MAIN_SEPARATOR_STR));
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ChangeError::HistoryInvalid { detail: "read" }),
    }
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
    /// Explicit acknowledgement of reported breaking classes (part of the
    /// durable execution intent; round 2, finding 10).
    pub allow_breaking: bool,
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
    for (row, operation) in resolved_rows.iter().zip(&request.operations) {
        if row.status != "resolved" {
            return Err(ChangeError::OperationUnsupported {
                operation: operation.operation().to_owned(),
            });
        }
        apply_tree_edit(&mut documents, operation)?;
    }

    // Plan the exact byte edits for every changed document. Every
    // operation in the batch contributes its own edits against the
    // ORIGINAL pinned bytes and spans (the ranges are original-relative
    // and must not overlap); the mutated candidate trees above exist only
    // for the semantic comparison. Batches on one file are legal (review
    // finding 7): the edits accumulate per file, are verified disjoint,
    // and apply in descending-offset order.
    let mut per_file_planned: Vec<(String, Vec<ByteEdit>)> = Vec::new();
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
        if edits.is_empty() {
            continue;
        }
        match per_file_planned
            .iter_mut()
            .find(|(planned, _)| planned == &path)
        {
            Some((_, planned)) => planned.extend(edits),
            None => per_file_planned.push((path, edits)),
        }
    }
    // Verify every per-file edit set is sorted and non-overlapping, then
    // fold in descending-offset order so each range stays original-relative.
    for (path, edits) in per_file_planned.iter_mut() {
        edits.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
        for pair in edits.windows(2) {
            if pair[0].end > pair[1].start {
                return Err(ChangeError::SourcePreservation {
                    subject: path.clone(),
                    detail: "edit-overlap",
                });
            }
        }
    }
    per_file_planned.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut changed_documents: Vec<LoadedDocument> = Vec::new();
    for (path, edits) in &per_file_planned {
        let document = load
            .documents
            .iter()
            .find(|document| &document.document.path == path)
            .ok_or(ChangeError::SubjectConflict {
                subject: path.clone(),
                detail: "document-missing",
            })?;
        let bytes = document.bytes.clone();
        let edited = apply_edits_descending(&bytes, edits);
        let decoded =
            crate::loader::decode_document_bytes(path, crate::loader::DocKind::Symbol, &edited)
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
    }
    let per_file_edits = per_file_planned;

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
    // A candidate that does not compile is request-controlled data: the
    // failure is a typed diagnostic (invalid), never a panic (issue review
    // finding 2 — every outcome is a DomainResult with a registered rule).
    let tree_compiled = ir::compile(&tree_candidate)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?
        .project;
    let byte_compiled = ir::compile(&byte_candidate)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?
        .project;
    if tree_compiled.to_canonical_json() != byte_compiled.to_canonical_json() {
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

    // Compatibility gate: breaking classes need policy, explicit
    // acknowledgement, or (for evidence classes) reviewed evidence.
    // Invalid candidates never reach this line. The acknowledgement flag
    // never authorizes data-loss or storage classes (round 2, finding 15):
    // those need a reviewed policy with migration evidence.
    let mut denied_breaking_classes = Vec::new();
    for class in &semantic_diff.classification {
        if !policy::BREAKING_CLASSES.contains(&class.as_str()) {
            continue;
        }
        let flag_authorizes = options.allow_breaking && !policy::requires_evidence(class);
        if !policy.allows(class) && !flag_authorizes {
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

    // Complete input inventory with exact byte pins. The history sidecar
    // distinguishes absent from empty (round 2, finding 8): a present but
    // empty or unreadable ledger refuses before any mutation.
    let mut inputs: Vec<InputPin> = load
        .documents
        .iter()
        .map(|document| InputPin {
            path: document.document.path.clone(),
            digest: format!("sha256:{}", crate::digest::sha256_hex(&document.bytes)),
        })
        .collect();
    let history_path = version::HISTORY_LOGICAL_PATH.to_owned();
    let history_digest = match read_history_bytes(&root)? {
        None => absent_digest(),
        Some(bytes) => format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
    };
    inputs.push(InputPin {
        path: history_path,
        digest: history_digest,
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
            let after = apply_edits_descending(before, edits);
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
        let after_bytes = apply_edits_descending(&original.bytes, edits);
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
        allow_breaking: options.allow_breaking,
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
    let key = receipt::key_digest(options.idempotency_key);
    // The durable ledger answers before any preparation runs: the Model
    // already carries the committed change, so a fresh resolve would refuse
    // as a duplicate. Replay must still work. Absent (NotFound) means no
    // ledger yet; any other read failure, or an existing empty ledger, is
    // corruption and fails closed (round 2, finding 8).
    if let Some(history_bytes) = read_history_bytes(&root)? {
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
            // The durable execution intent is part of the binding (round 2,
            // finding 10): the same key under a different policy, expected
            // plan, or acknowledgement is a conflict.
            let policy_digest = options.policy.as_ref().map(|bytes| receipt::bytes_digest(bytes));
            let intent = receipt::intent_digest(
                policy_digest.as_deref(),
                options.expect_plan,
                options.allow_breaking,
            );
            if record.intent_digest != intent {
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
            allow_breaking: options.allow_breaking,
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
    // Absent (NotFound) means the first commit creates it; an existing
    // empty or unreadable ledger is a corrupt document and fails closed
    // (round 2, finding 8).
    let history_logical = version::HISTORY_LOGICAL_PATH;
    let history_bytes = read_history_bytes(&root)?;
    let mut history = match &history_bytes {
        None => History {
            project: prepared.project_id.clone(),
            next_sequence: 1,
            records: Vec::new(),
        },
        Some(bytes) => History::decode(bytes)?,
    };
    let key = receipt::key_digest(options.idempotency_key);
    if let Some(record) = history.by_key(&key) {
        if record.request_digest != prepared.request_digest() {
            return Err(ChangeError::IdempotencyConflict);
        }
        // The durable execution intent is part of the binding (round 2,
        // finding 10).
        let intent = receipt::intent_digest(
            prepared.policy.digest.as_deref(),
            options.expect_plan,
            prepared.allow_breaking,
        );
        if record.intent_digest != intent {
            return Err(ChangeError::IdempotencyConflict);
        }
        return replayed_receipt(record);
    }

    // Re-verify the complete pinned input set before any mutation (the
    // prepare-to-apply CAS point, including the history sidecar itself).
    // Absent vs empty stay distinct for the sidecar (round 2, finding 8).
    for pin in &prepared.inputs {
        let digest = if pin.path == history_logical {
            match read_history_bytes(&root)? {
                None => absent_digest(),
                Some(bytes) => format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
            }
        } else {
            match std::fs::read(root.join(pin.path.replace('/', std::path::MAIN_SEPARATOR_STR))) {
                Ok(bytes) => format!("sha256:{}", crate::digest::sha256_hex(&bytes)),
                Err(_) => {
                    return Err(ChangeError::PreconditionFailed {
                        subject: pin.path.clone(),
                        detail: "input-digest",
                    })
                }
            }
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
            let after = apply_edits_descending(&before, edits);
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
    // The record's operation discriminators and logical paths, exactly the
    // fields the record wire and its receipt projection carry.
    let record_operations: Vec<String> = prepared
        .operations
        .iter()
        .map(|operation| operation.operation.clone())
        .collect();
    let record_files: Vec<String> = prepared
        .edits
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    // The chain anchor: the digest over the previous record's digest plus
    // the ENTIRE receipt projection (operations with subjects/statuses,
    // file before/after digests, transaction identity/state, accepted
    // policy/evidence, state, and the history sequence — the anchor's own
    // digest member is excluded from the hash input). The anchor value is
    // embedded in the receipt projection below, so decode recomputes and
    // verifies the whole chain (round 2, finding 7).
    let previous_digest = history
        .records
        .last()
        .map(|record| {
            record
                .receipt
                .get("history")
                .and_then(|anchor| anchor.get("recordDigest"))
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .unwrap_or_else(|| receipt::GENESIS_DIGEST.to_owned());
    // The accepted policy and evidence recorded on the receipt (round 2,
    // finding 15): a data-loss/storage authorization is only recorded
    // when the reviewed policy actually carries the migration evidence.
    let policy_acceptance = receipt::PolicyAcceptance {
        policy_digest: prepared.policy.digest.clone(),
        allow_breaking: prepared.allow_breaking,
        evidence: if prepared
            .semantic_diff
            .classification
            .iter()
            .any(|class| policy::requires_evidence(class))
        {
            prepared.policy.migration_evidence().map(str::to_owned)
        } else {
            None
        },
    };
    let mut receipt = receipt::Receipt {
        status: "valid".to_owned(),
        state: state.as_str().to_owned(),
        plan_id: Some(prepared.plan_id()),
        request_digest: Some(prepared.request_digest()),
        key_digest: Some(key.clone()),
        operations: prepared.operations.clone(),
        files,
        policy: Some(policy_acceptance),
        history: Some(HistoryAnchor {
            sequence,
            record_digest: String::new(),
        }),
        transaction: Some(receipt::TransactionWire {
            id: transaction_id.clone(),
            state: TransactionState::Committed.as_str().to_owned(),
        }),
        diagnostic_ids: Vec::new(),
        replayed: false,
    };
    let projection = receipt
        .history_projection()
        .ok_or(ChangeError::HistoryInvalid {
            detail: "projection",
        })?;
    let record_digest_value = receipt::record_digest(&previous_digest, &projection);
    receipt.history = Some(HistoryAnchor {
        sequence,
        record_digest: record_digest_value,
    });
    let record = HistoryRecord {
        sequence,
        key_digest: key,
        request_digest: prepared.request_digest(),
        plan_id: prepared.plan_id(),
        intent_digest: receipt::intent_digest(
            prepared.policy.digest.as_deref(),
            options.expect_plan,
            prepared.allow_breaking,
        ),
        operations: record_operations,
        files: record_files,
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
    let history_present = history_bytes.is_some();
    plans.push(ReplacementPlan {
        logical: history_logical.to_owned(),
        before_digest: match &history_bytes {
            None => absent_digest(),
            Some(bytes) => format!("sha256:{}", crate::digest::sha256_hex(bytes)),
        },
        after_digest: format!("sha256:{}", crate::digest::sha256_hex(&final_history_bytes)),
        edits: vec![ByteEdit {
            start: 0,
            end: history_bytes.as_ref().map(Vec::len).unwrap_or(0),
            replacement: String::from_utf8(final_history_bytes.clone())
                .map_err(|_| ChangeError::HistoryInvalid { detail: "encoding" })?,
        }],
        creation: !history_present,
    });

    let mut transaction =
        transaction::Transaction::begin(&root, &transaction_id, &prepared.plan_id(), &plans)?;
    for (index, plan) in plans.iter().enumerate() {
        if let Err(error) = transaction.stage(index, plan) {
            transaction.cleanup_after_restore();
            return Err(error);
        }
    }
    transaction.mark_state(transaction::STATE_REPLACING)?;
    if let Err(error) = transaction.replace_all() {
        if matches!(error, ChangeError::RecoveryRequired { .. }) {
            // The rollback itself failed: the journal and every backup stay
            // and readers stay blocked until explicit recovery succeeds
            // (round 2, finding 5) — recovery material is never deleted
            // after an incomplete restoration.
            return Err(error);
        }
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
    // A pending transaction gates the ledger (round 2, finding 9): the
    // history becomes readable while the transaction is durably committed
    // (recovery then only finishes the cleanup), but a staging or
    // replacing journal may still roll the recorded operation back.
    if let Some(transaction) = transaction::pending_transactions(&root).first() {
        if transaction::journal_state(&root, transaction).as_deref()
            != Some(transaction::STATE_COMMITTED)
        {
            return Err(ChangeError::RecoveryRequired {
                transaction: transaction.clone(),
            });
        }
    }
    // Absent reads as "no record"; an existing zero-byte (or
    // whitespace-only) ledger is a corrupt document and fails closed
    // (round 2, finding 8), as is any read failure other than NotFound.
    let history_bytes = match read_history_bytes(&root)? {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let history = History::decode(&history_bytes)?;
    let key = receipt::key_digest(idempotency_key);
    history.by_key(&key).map(stored_receipt).transpose()
}

/// The outcome of one successful recovery: the flat receipt facts of the
/// resolution. `state` is `committed` when the transaction was found
/// durably committed (recovery only finished the cleanup) and
/// `rolled-back` when the originals were restored or an untouched staging
/// transaction was aborted (round 2, findings 4 and 5).
#[derive(Clone, Debug)]
pub struct RecoveryOutcome {
    /// The transaction id that was recovered.
    pub transaction: String,
    /// The durable resolution state (`committed` or `rolled-back`).
    pub state: String,
    /// The bounded per-file outcomes (journal order).
    pub files: Vec<FileOutcome>,
}

/// Recover one interrupted transaction explicitly; readers stay blocked
/// until this succeeds and the journal is gone. A live writer's lock is
/// never stolen: the OS-held lock proves liveness, and only a provably
/// dead owner's artifact is reclaimed (round 2, finding 1).
pub fn recover_transaction(
    project: Option<&str>,
    transaction: &str,
) -> Result<RecoveryOutcome, ChangeError> {
    let root = resolve_root(project)?;
    let _lock = transaction::WriterLock::acquire(&root)?;
    let result = transaction::recover(&root, transaction)?;
    Ok(RecoveryOutcome {
        transaction: transaction.to_owned(),
        state: if result.committed {
            "committed".to_owned()
        } else {
            "rolled-back".to_owned()
        },
        files: result.files,
    })
}

/// The logical transaction ids currently awaiting recovery.
pub fn pending_transactions(project: Option<&str>) -> Result<Vec<String>, ChangeError> {
    let root = resolve_root(project)?;
    Ok(transaction::pending_transactions(&root))
}

/// Test-only live-writer handle: holds the project's change writer lock so
/// tests can prove a live holder denies recovery and mutation (round 2,
/// finding 1). Never used outside `cfg(test)` integration suites.
#[doc(hidden)]
pub struct TestWriterGuard {
    _inner: transaction::WriterLock,
}

/// Test-only: acquire the change writer lock for the selected project.
#[doc(hidden)]
pub fn test_only_acquire_writer_lock(
    project: Option<&str>,
) -> Result<TestWriterGuard, ChangeError> {
    let root = resolve_root(project)?;
    Ok(TestWriterGuard {
        _inner: transaction::WriterLock::acquire(&root)?,
    })
}

/// Rebuild the replayed receipt from its stored record.
fn replayed_receipt(record: &HistoryRecord) -> Result<Receipt, ChangeError> {
    let mut receipt = stored_receipt(record)?;
    receipt.state = ReceiptState::Replayed.as_str().to_owned();
    receipt.replayed = true;
    Ok(receipt)
}

/// Parse one stored record receipt back into the typed wire. Every member
/// is required and strictly typed (no `filter_map(.ok())` drops, no
/// defaulted state — review finding 5). The record already passed
/// [`History::decode`]'s binding and chain verification, so the strict
/// parse is guaranteed to succeed for a decoder-accepted record.
fn stored_receipt(record: &HistoryRecord) -> Result<Receipt, ChangeError> {
    let invalid = || ChangeError::HistoryInvalid {
        detail: "receipt-projection",
    };
    let value: Json = record.receipt.clone();
    let operations: Vec<ResolvedOperation> = value
        .get("operations")
        .and_then(Json::as_array)
        .ok_or_else(invalid)?
        .iter()
        .map(|item| serde_json::from_value(item.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid())?;
    let files: Vec<FileOutcome> = value
        .get("files")
        .and_then(Json::as_array)
        .ok_or_else(invalid)?
        .iter()
        .map(|item| serde_json::from_value(item.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid())?;
    let state: String = value
        .get("state")
        .and_then(Json::as_str)
        .ok_or_else(invalid)?
        .to_owned();
    let history = value
        .get("history")
        .map(|history| serde_json::from_value(history.clone()))
        .transpose()
        .map_err(|_| invalid())?;
    let transaction = value
        .get("transaction")
        .map(|transaction| serde_json::from_value(transaction.clone()))
        .transpose()
        .map_err(|_| invalid())?;
    let policy = value
        .get("policy")
        .map(|policy| serde_json::from_value(policy.clone()))
        .transpose()
        .map_err(|_| invalid())?;
    Ok(Receipt {
        status: "valid".to_owned(),
        state,
        plan_id: Some(record.plan_id.clone()),
        request_digest: Some(record.request_digest.clone()),
        key_digest: Some(record.key_digest.clone()),
        operations,
        files,
        policy,
        history,
        transaction,
        diagnostic_ids: value
            .get("diagnosticIds")
            .and_then(Json::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(invalid)
            })
            .transpose()?
            .unwrap_or_default(),
        replayed: false,
    })
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

/// Apply disjoint original-relative edits in descending-offset order so
/// every range is interpreted against the original bytes. An unsorted set
/// is sorted first; the ranges must already be verified non-overlapping.
fn apply_edits_descending(bytes: &[u8], edits: &[ByteEdit]) -> Vec<u8> {
    let mut sorted: Vec<&ByteEdit> = edits.iter().collect();
    sorted.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
    let mut out = bytes.to_vec();
    for edit in sorted.iter().rev() {
        out = edit.apply(&out);
    }
    out
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
