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
    /// receipt payload).
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
        serde_json::to_string_pretty(&wire).unwrap_or_default()
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
    /// or duplicated history fails closed: replay never guesses.
    pub fn decode(bytes: &[u8]) -> Result<Self, super::diagnostic::ChangeError> {
        let value: Json = serde_json::from_slice(bytes).map_err(|_| {
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
        for (index, record_value) in records_value.iter().enumerate() {
            let record_map =
                record_value
                    .as_object()
                    .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                        detail: "record-shape",
                    })?;
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

/// The bounded `sha256:` digest of one raw idempotency key. The raw key
/// never enters diagnostics, receipts, or the history wire.
pub fn key_digest(key: &str) -> String {
    format!("sha256:{}", sha256_hex(key.as_bytes()))
}
