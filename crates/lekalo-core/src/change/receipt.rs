//! Change receipts and durable history (issue #73).
//!
//! Receipts distinguish `prepared`, `committed`, `replayed`, `no-op`,
//! `rejected`, `rolled-back`, and `recovery-required`. These are Change
//! receipt states, never new envelope statuses. The canonical history
//! sidecar at `lekalo/change-history.json` is the durable idempotency
//! ledger: it binds every committed key digest to its request digest and
//! stores the original receipt so replay stays usable after source drift.
//! The raw key never enters any wire surface.

use serde::Serialize;
use serde_json::Value as Json;

use super::plan::ResolvedOperation;
use super::version;
use crate::digest::sha256_hex;

/// Maximum canonical history bytes accepted before decoding.
const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

/// The exact member count of one canonical history record.
const RECORD_MEMBERS: usize = 7;

/// The closed receipt state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptState {
    Prepared,
    Committed,
    Replayed,
    NoOp,
    Rejected,
    RolledBack,
    RecoveryRequired,
}

impl ReceiptState {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Committed => "committed",
            Self::Replayed => "replayed",
            Self::NoOp => "no-op",
            Self::Rejected => "rejected",
            Self::RolledBack => "rolled-back",
            Self::RecoveryRequired => "recovery-required",
        }
    }
}

/// The closed transaction-state vocabulary of a receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionState {
    None,
    Staged,
    Committed,
    RolledBack,
    RecoveryRequired,
}

impl TransactionState {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Staged => "staged",
            Self::Committed => "committed",
            Self::RolledBack => "rolled-back",
            Self::RecoveryRequired => "recovery-required",
        }
    }
}

/// One file outcome of a committed or rolled-back transaction.
#[derive(Clone, Debug, Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct FileOutcome {
    pub path: String,
    #[serde(rename = "beforeDigest")]
    pub before_digest: String,
    #[serde(rename = "afterDigest")]
    pub after_digest: String,
    pub outcome: String,
}

/// The closed wire projection of one receipt.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct Receipt {
    /// The envelope status of the run (`valid`; failure runs carry no
    /// receipt payload). The wire layer emits it exactly once — the
    /// flattened projection skips it (review finding 8: no duplicate keys
    /// in emitted documents).
    #[serde(skip_serializing)]
    pub status: String,
    pub state: String,
    #[serde(rename = "planId")]
    pub plan_id: Option<String>,
    #[serde(rename = "requestDigest")]
    pub request_digest: Option<String>,
    #[serde(rename = "keyDigest")]
    pub key_digest: Option<String>,
    pub operations: Vec<ResolvedOperation>,
    pub files: Vec<FileOutcome>,
    pub history: Option<HistoryAnchor>,
    pub transaction: Option<TransactionWire>,
    #[serde(rename = "diagnosticIds")]
    pub diagnostic_ids: Vec<String>,
    pub replayed: bool,
}

impl Receipt {
    /// The wire bytes (pretty, contract header carried inline).
    pub fn to_json_string(&self) -> String {
        #[derive(Serialize)]
        struct Wire<'a> {
            status: &'a str,
            #[serde(rename = "schemaVersion")]
            schema_version: &'a str,
            identity: &'a str,
            #[serde(flatten)]
            receipt: &'a Receipt,
        }
        let wire = Wire {
            status: "valid",
            schema_version: version::RECEIPT_SCHEMA_VERSION,
            identity: version::RECEIPT_IDENTITY,
            receipt: self,
        };
        let bytes = serde_json::to_string_pretty(&wire).unwrap_or_default();
        debug_assert!(
            !has_duplicate_keys(&bytes),
            "receipt documents must be dup-free JSON"
        );
        bytes
    }

    /// The state of this receipt.
    pub fn state(&self) -> ReceiptState {
        match self.state.as_str() {
            "prepared" => ReceiptState::Prepared,
            "committed" => ReceiptState::Committed,
            "replayed" => ReceiptState::Replayed,
            "no-op" => ReceiptState::NoOp,
            "rejected" => ReceiptState::Rejected,
            "rolled-back" => ReceiptState::RolledBack,
            _ => ReceiptState::RecoveryRequired,
        }
    }

    /// The bounded history projection stored inside one history record:
    /// the exact original receipt wire (with its contract header), never a
    /// relabeled replay.
    pub(crate) fn history_projection(&self) -> Option<Json> {
        if !matches!(self.state.as_str(), "committed" | "no-op") {
            return None;
        }
        serde_json::from_str(&self.to_json_string()).ok()
    }
}

/// The history anchor of a committed receipt.
#[derive(Clone, Debug, Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct HistoryAnchor {
    pub sequence: u64,
    #[serde(rename = "recordDigest")]
    pub record_digest: String,
}

/// The transaction wire value of a receipt.
#[derive(Clone, Debug, Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct TransactionWire {
    pub id: String,
    pub state: String,
}

/// The validated canonical history document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct History {
    /// The project semantic id the history belongs to.
    pub project: String,
    /// The next sequence number (`records.len() + 1`, enforced).
    pub next_sequence: u64,
    /// The committed records in sequence order.
    pub records: Vec<HistoryRecord>,
}

/// One committed record of the canonical history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryRecord {
    pub sequence: u64,
    /// The `sha256:` digest of the idempotency key (never the raw key).
    pub key_digest: String,
    /// The `sha256:` digest of the exact request bytes.
    pub request_digest: String,
    /// The plan identity of the committed change.
    pub plan_id: String,
    /// The operation discriminators in request order.
    pub operations: Vec<String>,
    /// The logical paths of the files the transaction replaced.
    pub files: Vec<String>,
    /// The exact original receipt projection (JSON object).
    pub receipt: Json,
}

impl History {
    /// Parse and validate canonical history bytes. A malformed, unordered,
    /// duplicated, tampered, or internally inconsistent history fails
    /// closed: replay never guesses. Duplicate JSON keys are refused before
    /// typing, every record is bound to its embedded receipt and to the
    /// chain of all earlier records, and no member is defaulted.
    pub fn decode(bytes: &[u8]) -> Result<Self, super::diagnostic::ChangeError> {
        // A zero-byte ledger file is a corrupt document, not an empty
        // history (review finding 11): decode fails closed on it.
        if bytes.is_empty() || bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Err(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "empty",
            });
        }
        if bytes.len() > MAX_HISTORY_BYTES {
            return Err(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "over-limit",
            });
        }
        let value: Json =
            super::request::unique_json(bytes).map_err(|_| {
                super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "json-shape",
                }
            })?;
        let map = value
            .as_object()
            .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "root-not-object",
            })?;
        if map.get("schemaVersion").and_then(Json::as_str) != Some(version::HISTORY_SCHEMA_VERSION)
            || map.get("identity").and_then(Json::as_str) != Some(version::HISTORY_IDENTITY)
        {
            return Err(super::diagnostic::ChangeError::HistoryInvalid { detail: "identity" });
        }
        let project = map
            .get("project")
            .and_then(Json::as_str)
            .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "project" })?
            .to_owned();
        let records_value = map
            .get("records")
            .and_then(Json::as_array)
            .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "records" })?;
        let mut records = Vec::with_capacity(records_value.len());
        let mut seen_keys = std::collections::BTreeSet::new();
        // The chain digest starts at the empty-bytes sentinel; every
        // record must continue it exactly (tamper evidence, review
        // finding 5: insertion, deletion, reorder, and field tampering
        // all break the chain and fail closed).
        let mut chain = super::absent_digest();
        for (index, record_value) in records_value.iter().enumerate() {
            let record_map =
                record_value
                    .as_object()
                    .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                        detail: "record-shape",
                    })?;
            if record_map.len() != RECORD_MEMBERS {
                return Err(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "record-members",
                });
            }
            let sequence = record_map
                .get("sequence")
                .and_then(Json::as_u64)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "sequence" })?;
            if sequence != index as u64 + 1 {
                return Err(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "record-order",
                });
            }
            let key_digest = record_map
                .get("keyDigest")
                .and_then(Json::as_str)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "key-digest",
                })?
                .to_owned();
            if !seen_keys.insert(key_digest.clone()) {
                return Err(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "duplicate-key",
                });
            }
            let request_digest = record_map
                .get("requestDigest")
                .and_then(Json::as_str)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "request-digest",
                })?
                .to_owned();
            let plan_id = record_map
                .get("planId")
                .and_then(Json::as_str)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "plan-id" })?
                .to_owned();
            let operations = record_map
                .get("operations")
                .and_then(Json::as_array)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "operations",
                })?
                .iter()
                .map(|value| value.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "operations",
                })?;
            let files = record_map
                .get("files")
                .and_then(Json::as_array)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "files" })?
                .iter()
                .map(|value| value.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "files" })?;
            let receipt = record_map
                .get("receipt")
                .cloned()
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "receipt" })?;
            // The embedded receipt must agree with the record's own pinned
            // fields and carry its own chain anchor: a forged record with
            // invented plan/files/receipt members is refused here, and
            // decode never defaults a missing state (review finding 5).
            let receipt_state = verify_receipt_projection(
                &receipt,
                &key_digest,
                &request_digest,
                &plan_id,
                &operations,
                &files,
            )?;
            let digest = record_digest(
                &chain,
                sequence,
                &key_digest,
                &request_digest,
                &plan_id,
                &operations,
                &files,
                receipt_state,
            );
            let anchor = receipt
                .get("history")
                .and_then(|history| history.get("recordDigest"))
                .and_then(Json::as_str)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "record-digest-missing",
                })?;
            if anchor != &digest {
                return Err(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "record-digest",
                });
            }
            chain = digest;
            records.push(HistoryRecord {
                sequence,
                key_digest,
                request_digest,
                plan_id,
                operations,
                files,
                receipt,
            });
        }
        let next_sequence = map.get("nextSequence").and_then(Json::as_u64).ok_or(
            super::diagnostic::ChangeError::HistoryInvalid {
                detail: "next-sequence",
            },
        )?;
        if next_sequence != records.len() as u64 + 1 {
            return Err(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "next-sequence-order",
            });
        }
        Ok(Self {
            project,
            next_sequence,
            records,
        })
    }

    /// Serialize the canonical history bytes (sorted keys, compact rows are
    /// not required: the document is canonical pretty JSON).
    pub fn to_json_string(&self) -> String {
        let records: Vec<Json> = self
            .records
            .iter()
            .map(|record| {
                serde_json::json!({
                    "sequence": record.sequence,
                    "keyDigest": record.key_digest,
                    "requestDigest": record.request_digest,
                    "planId": record.plan_id,
                    "operations": record.operations,
                    "files": record.files,
                    "receipt": record.receipt,
                })
            })
            .collect();
        let document = serde_json::json!({
            "schemaVersion": version::HISTORY_SCHEMA_VERSION,
            "identity": version::HISTORY_IDENTITY,
            "project": self.project,
            "nextSequence": self.records.len() as u64 + 1,
            "records": records,
        });
        serde_json::to_string_pretty(&document).unwrap_or_default()
    }

    /// Look up one record by exact key digest.
    pub fn by_key(&self, key_digest: &str) -> Option<&HistoryRecord> {
        self.records
            .iter()
            .find(|record| record.key_digest == key_digest)
    }

    /// The history with one appended record (sequence assigned).
    pub fn with_appended(&self, mut record: HistoryRecord) -> Self {
        record.sequence = self.next_sequence;
        let mut records = self.records.clone();
        records.push(record);
        Self {
            project: self.project.clone(),
            next_sequence: records.len() as u64 + 1,
            records,
        }
    }
}

/// The chain anchor of the first history record (the empty-bytes digest).
pub(crate) const GENESIS_DIGEST: &str =
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The canonical per-record digest: binds one history record to its exact
/// fields, to its embedded receipt's pinned state, and — through the
/// previous record's digest — to every earlier record (chain; the genesis
/// uses the empty-bytes digest). Insertion, deletion, reorder, and field
/// tampering all break the chain and fail decode closed (review finding
/// 5). Encode ([`super::apply_prepared`]) and decode
/// ([`History::decode`]) compute exactly this formula.
pub(crate) fn record_digest(
    previous: &str,
    sequence: u64,
    key_digest: &str,
    request_digest: &str,
    plan_id: &str,
    operations: &[String],
    files: &[String],
    state: &str,
) -> String {
    let canonical = format!(
        "{previous}\u{1f}{sequence}\u{1f}{key_digest}\u{1f}{request_digest}\u{1f}{plan_id}\
         \u{1f}{state}\u{1f}{}\u{1f}{}",
        operations.join("\u{1e}"),
        files.join("\u{1e}"),
    );
    format!("sha256:{}", sha256_hex(canonical.as_bytes()))
}

/// The bounded `sha256:` digest of one raw idempotency key. The raw key
/// never enters diagnostics, receipts, or the history wire.
pub fn key_digest(key: &str) -> String {
    format!("sha256:{}", sha256_hex(key.as_bytes()))
}

/// Whether one serialized JSON object document contains any duplicate key
/// at any nesting level (strict-parser safety assertion for emitted
/// change documents).
pub(crate) fn has_duplicate_keys(document: &str) -> bool {    fn walk(value: &Json) -> bool {
        match value {
            Json::Object(map) => {
                let mut keys = std::collections::BTreeSet::new();
                for key in map.keys() {
                    if !keys.insert(key.clone()) {
                        return true;
                    }
                }
                map.values().any(walk)
            }
            Json::Array(items) => items.iter().any(walk),
            _ => false,
        }
    }
    serde_json::from_str::<Json>(document)
        .map(|value| walk(&value))
        .unwrap_or(true)
}

/// Verify one stored receipt projection against the record's pinned
/// fields (review finding 5). Every identity member is required, strictly
/// typed, and must equal the record's own binding; the receipt state is
/// returned (never defaulted) so the chain digest pins it. A forged
/// record — right key digest, invented plan/receipt — fails here.
fn verify_receipt_projection<'a>(
    receipt: &'a Json,
    key_digest: &str,
    request_digest: &str,
    plan_id: &str,
    operations: &[String],
    files: &[String],
) -> Result<&'a str, super::diagnostic::ChangeError> {
    let invalid = |detail: &'static str| {
        Err(super::diagnostic::ChangeError::HistoryInvalid { detail })
    };
    let map = receipt.as_object().ok_or_else(|| {
        super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-shape",
        }
    })?;
    let text = |name: &str| {
        map.get(name)
            .and_then(Json::as_str)
            .ok_or_else(|| super::diagnostic::ChangeError::HistoryInvalid {
                detail: "receipt-member",
            })
    };
    if map.get("schemaVersion").and_then(Json::as_str) != Some(version::RECEIPT_SCHEMA_VERSION)
        || map.get("identity").and_then(Json::as_str) != Some(version::RECEIPT_IDENTITY)
        || map.get("status").and_then(Json::as_str) != Some("valid")
        || map.get("replayed").and_then(Json::as_bool) != Some(false)
    {
        return invalid("receipt-identity");
    }
    if text("keyDigest")? != key_digest
        || text("requestDigest")? != request_digest
        || text("planId")? != plan_id
    {
        return invalid("receipt-binding");
    }
    let state = text("state")?;
    if !matches!(
        state,
        "committed" | "no-op" | "replayed" | "rejected" | "rolled-back" | "recovery-required"
    ) {
        return invalid("receipt-state");
    }
    let receipt_operations = map
        .get("operations")
        .and_then(Json::as_array)
        .ok_or_else(|| super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-operations",
        })?;
    let receipt_operations: Option<Vec<&str>> = receipt_operations
        .iter()
        .map(|value| value.get("operation").and_then(Json::as_str))
        .collect();
    let receipt_operations = receipt_operations.ok_or_else(|| {
        super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-operations",
        }
    })?;
    if receipt_operations.len() != operations.len()
        || receipt_operations
            .iter()
            .zip(operations.iter())
            .any(|(receipt, record)| receipt != &record.as_str())
    {
        return invalid("receipt-operations-mismatch");
    }
    let receipt_files = map
        .get("files")
        .and_then(Json::as_array)
        .ok_or_else(|| super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-files",
        })?;
    let receipt_files: Option<Vec<&str>> = receipt_files
        .iter()
        .map(|value| value.get("path").and_then(Json::as_str))
        .collect();
    let receipt_files =
        receipt_files.ok_or_else(|| super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-files",
        })?;
    if receipt_files.len() != files.len()
        || receipt_files
            .iter()
            .zip(files.iter())
            .any(|(receipt, record)| receipt != &record.as_str())
    {
        return invalid("receipt-files-mismatch");
    }
    // The history anchor must be a well-formed sequence + digest pair.
    let history = map
        .get("history")
        .and_then(Json::as_object)
        .ok_or_else(|| super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-history",
        })?;
    if history.len() != 2
        || history.get("sequence").and_then(Json::as_u64).is_none()
        || history
            .get("recordDigest")
            .and_then(Json::as_str)
            .is_none()
    {
        return invalid("receipt-history");
    }
    Ok(state)
}
