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
pub(crate) const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

/// The published history schema's record-count bound, shared by both
/// decoding and prospective commit admission (independent of byte size).
pub(crate) const MAX_HISTORY_RECORDS: usize = 4096;

/// The exact member count of one canonical history record.
const RECORD_MEMBERS: usize = 8;

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
#[serde(deny_unknown_fields)]
pub struct FileOutcome {
    pub path: String,
    #[serde(rename = "beforeDigest")]
    pub before_digest: String,
    #[serde(rename = "afterDigest")]
    pub after_digest: String,
    pub outcome: String,
}

/// The accepted policy and evidence recorded on every receipt (round 2,
/// finding 15): which reviewed policy bytes authorized the change, whether
/// explicit acknowledgement was given, and the migration-evidence digest
/// when a data-loss or storage class was authorized. Round 3 (finding 5)
/// adds the expected-plan pin: the full execution intent is reconstructible
/// from stored constituent facts and cross-checked against the record's
/// `intentDigest` on every decode.
#[derive(Clone, Debug, Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct PolicyAcceptance {
    #[serde(rename = "policyDigest")]
    pub policy_digest: Option<String>,
    #[serde(rename = "allowBreaking")]
    pub allow_breaking: bool,
    pub evidence: Option<String>,
    #[serde(rename = "expectPlan")]
    pub expect_plan: Option<String>,
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
    /// The accepted policy/evidence of the run (`null` where no plan
    /// applies, e.g. a recovery receipt).
    pub policy: Option<PolicyAcceptance>,
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
    /// The `sha256:` digest of the execution intent (policy digest, expected
    /// plan, acknowledgement flag) the record was committed under (round 2,
    /// finding 10): same key plus a different policy or expectation is a
    /// conflict, never a silent replay.
    pub intent_digest: String,
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
    /// chain of all earlier records, and no member is defaulted. The entire
    /// document must also satisfy the frozen closed history schema, including
    /// both operation arrays and every embedded operation row (r12).
    pub fn decode(bytes: &[u8]) -> Result<Self, super::diagnostic::ChangeError> {
        // A zero-byte ledger file is a corrupt document, not an empty
        // history (review finding 11): decode fails closed on it.
        if bytes.is_empty() || bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Err(super::diagnostic::ChangeError::HistoryInvalid { detail: "empty" });
        }
        if bytes.len() > MAX_HISTORY_BYTES {
            return Err(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "over-limit",
            });
        }
        let value: Json = super::request::unique_json(bytes).map_err(|_| {
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
        if map.len() != 6 {
            return Err(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "root-members",
            });
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
        if records_value.len() > MAX_HISTORY_RECORDS {
            return Err(super::diagnostic::ChangeError::HistoryInvalid {
                detail: "record-limit",
            });
        }
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
            let intent_digest_field = record_map
                .get("intentDigest")
                .and_then(Json::as_str)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "intent-digest",
                })?
                .to_owned();
            if super::expect_digest(&intent_digest_field).is_none() {
                return Err(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "intent-digest-shape",
                });
            }
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
            verify_receipt_projection(
                &receipt,
                sequence,
                &key_digest,
                &request_digest,
                &plan_id,
                &operations,
                &files,
            )?;
            // Intent/receipt consistency from stored constituent facts
            // (round 3, finding 5): the record's `intentDigest` must be
            // exactly the digest of the policy acceptance the embedded
            // receipt carries (policy digest, expected plan, and
            // acknowledgement flag). A record whose unhashed-looking
            // intent field disagrees with its own receipt is refused —
            // replay authorization can never rest on an unbound field.
            let policy = receipt.get("policy").and_then(Json::as_object).ok_or(
                super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "receipt-policy",
                },
            )?;
            let policy_digest = policy.get("policyDigest").and_then(Json::as_str);
            let expect_plan = policy.get("expectPlan").and_then(Json::as_str);
            let allow_breaking = policy.get("allowBreaking").and_then(Json::as_bool).ok_or(
                super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "receipt-policy",
                },
            )?;
            if intent_digest(policy_digest, expect_plan, allow_breaking) != intent_digest_field {
                return Err(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "intent-consistency",
                });
            }
            let digest = record_digest(&chain, &project, &intent_digest_field, &receipt);
            let anchor = receipt
                .get("history")
                .and_then(|history| history.get("recordDigest"))
                .and_then(Json::as_str)
                .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
                    detail: "record-digest-missing",
                })?;
            if anchor != digest {
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
                intent_digest: intent_digest_field,
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
        // A backward-only chain cannot detect removal of its last row.
        // This independently stored count/tip anchor detects tail truncation
        // even when the editor adjusts nextSequence (r10, finding F2).
        let tip = map
            .get("tip")
            .and_then(Json::as_object)
            .ok_or(super::diagnostic::ChangeError::HistoryInvalid { detail: "tip" })?;
        if tip.len() != 2
            || tip.get("sequence").and_then(Json::as_u64) != Some(records.len() as u64)
            || tip.get("recordDigest").and_then(Json::as_str) != Some(chain.as_str())
        {
            return Err(super::diagnostic::ChangeError::HistoryInvalid { detail: "tip" });
        }
        // Internal consistency is not schema admission: a genuine legacy
        // no-op can have a valid chain but invalid empty operations arrays.
        // Every reader/writer/init path shares this check; no custody or new
        // history may be published by silently adopting or repairing it.
        super::history_schema::validate(&value)?;
        Ok(Self {
            project,
            next_sequence,
            records,
        })
    }

    /// The accepted chain endpoint (the empty sentinel for a new ledger).
    pub(crate) fn tip_digest(&self) -> String {
        self.records
            .last()
            .and_then(|record| record.receipt.get("history"))
            .and_then(|anchor| anchor.get("recordDigest"))
            .and_then(Json::as_str)
            .unwrap_or(GENESIS_DIGEST)
            .to_owned()
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
                    "intentDigest": record.intent_digest,
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
            "tip": {
                "sequence": self.records.len(),
                "recordDigest": self.tip_digest(),
            },
            "records": records,
        });
        serde_json::to_string_pretty(&document).unwrap_or_default()
    }

    /// Encode a prospective commit only if the same reader that will
    /// serve receipt/replay can accept it, with no loss or repair. Capacity
    /// is a registered bounded precondition refusal, not a failed commit:
    /// the writer has not started a transaction and must retain every row.
    pub(crate) fn encode_for_commit(&self) -> Result<Vec<u8>, super::ChangeError> {
        if self.records.len() > MAX_HISTORY_RECORDS {
            return Err(super::ChangeError::PreconditionFailed {
                subject: "history".to_owned(),
                detail: "history-record-limit",
            });
        }
        let bytes = self.to_json_string().into_bytes();
        if bytes.len() > MAX_HISTORY_BYTES {
            return Err(super::ChangeError::PreconditionFailed {
                subject: "history".to_owned(),
                detail: "history-byte-limit",
            });
        }
        if Self::decode(&bytes)? != *self {
            return Err(super::ChangeError::HistoryInvalid {
                detail: "history-round-trip",
            });
        }
        Ok(bytes)
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

/// The canonical per-record digest (round 3, finding 5): binds one history
/// record to the ENTIRE receipt projection it stores — every operation row
/// (subject, family, status, detail), every file outcome (path, before/after
/// digests, outcome), the transaction identity and state, the accepted
/// policy/evidence/expectation, the receipt state, and the history
/// sequence — PLUS the record's own authoritative members the projection
/// does not carry: the project identity and the exact execution-intent
/// digest. Chained to every earlier record through the previous record's
/// digest (the genesis uses the empty-bytes digest). The anchor's own
/// `recordDigest` member is removed before hashing, so the digest is
/// computable on both encode and decode without circularity. Insertion,
/// deletion, reorder, and field tampering — including a lone
/// `intentDigest` edit that recomputes nothing — break the chain and fail
/// decode closed (round 2, finding 7; round 3, finding 5 extends the
/// input beyond the bare receipt). This remains an unkeyed chain: it is
/// tamper evidence against partial edits, not authentication against an
/// attacker who can recompute every hash. Replay additionally requires
/// the transactionally published non-cache custody pin outside this
/// ledger (r10, finding F1); the public chain alone is never authority.
/// Encode ([`super::apply_prepared`])
/// and decode ([`History::decode`]) compute exactly this formula.
pub(crate) fn record_digest(
    previous: &str,
    project: &str,
    intent_digest: &str,
    receipt_projection: &Json,
) -> String {
    let mut projection = receipt_projection.clone();
    if let Some(history) = projection.get_mut("history").and_then(Json::as_object_mut) {
        history.remove("recordDigest");
    }
    let canonical = serde_json::to_string(&projection).unwrap_or_default();
    format!(
        "sha256:{}",
        sha256_hex(
            format!("{previous}\u{1f}{project}\u{1f}{intent_digest}\u{1f}{canonical}").as_bytes()
        )
    )
}

/// The `sha256:` digest of the durable execution intent of one apply run
/// (round 2, finding 10): the exact policy bytes, the expected plan, and
/// the explicit-acknowledgement flag. Same key plus a different intent is
/// a conflict, never a silent replay.
pub(crate) fn intent_digest(
    policy_digest: Option<&str>,
    expect_plan: Option<&str>,
    allow_breaking: bool,
) -> String {
    let canonical = format!(
        "{}\u{1f}{}\u{1f}{}",
        policy_digest.unwrap_or("none"),
        expect_plan.unwrap_or("none"),
        if allow_breaking { "allowed" } else { "default" },
    );
    format!("sha256:{}", sha256_hex(canonical.as_bytes()))
}

/// The bounded `sha256:` digest of raw bytes (the policy-bytes pin).
pub(crate) fn bytes_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// The bounded `sha256:` digest of one raw idempotency key. The raw key
/// never enters diagnostics, receipts, or the history wire.
pub fn key_digest(key: &str) -> String {
    format!("sha256:{}", sha256_hex(key.as_bytes()))
}

/// Whether one serialized JSON object document contains any duplicate key
/// at any nesting level (strict-parser safety assertion for emitted
/// change documents).
pub(crate) fn has_duplicate_keys(document: &str) -> bool {
    fn walk(value: &Json) -> bool {
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
    sequence: u64,
    key_digest: &str,
    request_digest: &str,
    plan_id: &str,
    operations: &[String],
    files: &[String],
) -> Result<&'a str, super::diagnostic::ChangeError> {
    let invalid =
        |detail: &'static str| Err(super::diagnostic::ChangeError::HistoryInvalid { detail });
    let map = receipt
        .as_object()
        .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-shape",
        })?;
    let text = |name: &str| {
        map.get(name)
            .and_then(Json::as_str)
            .ok_or(super::diagnostic::ChangeError::HistoryInvalid {
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
    if !matches!(state, "committed" | "no-op") {
        return invalid("receipt-state");
    }
    let receipt_operations = map.get("operations").and_then(Json::as_array).ok_or(
        super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-operations",
        },
    )?;
    let receipt_operations: Option<Vec<&str>> = receipt_operations
        .iter()
        .map(|value| value.get("operation").and_then(Json::as_str))
        .collect();
    let receipt_operations =
        receipt_operations.ok_or(super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-operations",
        })?;
    if receipt_operations.len() != operations.len()
        || receipt_operations
            .iter()
            .zip(operations.iter())
            .any(|(receipt, record)| receipt != &record.as_str())
    {
        return invalid("receipt-operations-mismatch");
    }
    let receipt_files = map.get("files").and_then(Json::as_array).ok_or(
        super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-files",
        },
    )?;
    // Committed mutation requires a real, nonempty source set; no-op has
    // none. Never let an empty or malformed projection silently authorize
    // mutating replay (r10, finding F1). Every outcome is strictly typed.
    if (state == "committed") == receipt_files.is_empty() {
        return invalid("receipt-state-files");
    }
    let mut seen_paths = std::collections::BTreeSet::new();
    for value in receipt_files {
        let outcome: FileOutcome = serde_json::from_value(value.clone()).map_err(|_| {
            super::diagnostic::ChangeError::HistoryInvalid {
                detail: "receipt-files",
            }
        })?;
        if !seen_paths.insert(outcome.path.clone())
            || outcome.path == super::version::HISTORY_LOGICAL_PATH
            || super::expect_digest(&outcome.before_digest).is_none()
            || super::expect_digest(&outcome.after_digest).is_none()
            || !matches!(
                outcome.outcome.as_str(),
                "replaced" | "moved-from" | "moved-to"
            )
            || (outcome.outcome == "moved-from" && outcome.after_digest != super::absent_digest())
            || (outcome.outcome == "moved-to" && outcome.before_digest != super::absent_digest())
            || (outcome.outcome.starts_with("moved-")
                && !operations
                    .iter()
                    .any(|operation| operation == "module:move"))
            || outcome.before_digest == outcome.after_digest
        {
            return invalid("receipt-files");
        }
    }
    if map
        .get("transaction")
        .and_then(|tx| tx.get("state"))
        .and_then(Json::as_str)
        != Some("committed")
    {
        return invalid("receipt-transaction");
    }
    let receipt_files: Option<Vec<&str>> = receipt_files
        .iter()
        .map(|value| value.get("path").and_then(Json::as_str))
        .collect();
    let receipt_files = receipt_files.ok_or(super::diagnostic::ChangeError::HistoryInvalid {
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
    // The accepted policy/evidence/expectation member must be present and
    // well-formed (round 2, finding 15; round 3, finding 5 adds the
    // expected-plan pin): the closed acceptance object. Its content is
    // hash-bound by the chain and cross-checked against the record's
    // intent digest by the caller.
    match map.get("policy") {
        Some(policy) => {
            let Some(policy) = policy.as_object() else {
                return invalid("receipt-policy");
            };
            if policy.len() != 4
                || !matches!(policy.get("allowBreaking"), Some(Json::Bool(_)))
                || !matches!(
                    policy.get("policyDigest"),
                    None | Some(Json::Null) | Some(Json::String(_))
                )
                || !matches!(
                    policy.get("evidence"),
                    None | Some(Json::Null) | Some(Json::String(_))
                )
                || !matches!(
                    policy.get("expectPlan"),
                    None | Some(Json::Null) | Some(Json::String(_))
                )
            {
                return invalid("receipt-policy");
            }
        }
        None => return invalid("receipt-policy"),
    }
    // The history anchor must be a well-formed sequence + digest pair, and
    // its sequence must equal the record's own sequence (round 2,
    // finding 7: no drifted anchor).
    let history = map.get("history").and_then(Json::as_object).ok_or(
        super::diagnostic::ChangeError::HistoryInvalid {
            detail: "receipt-history",
        },
    )?;
    if history.len() != 2
        || history.get("sequence").and_then(Json::as_u64) != Some(sequence)
        || history.get("recordDigest").and_then(Json::as_str).is_none()
    {
        return invalid("receipt-history");
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::ChangeError;

    #[test]
    fn schema_invalid_operation_arrays_refuse_decode_and_prospective_commit() {
        let bytes = include_bytes!("../../../../tests/fixtures/change/golden/change-history.json");
        let original = History::decode(bytes).unwrap();
        let row = original.records[0].receipt["operations"][0].clone();
        let mut cases = Vec::new();
        for count in [0, super::version::MAX_OPERATIONS + 1] {
            let mut history = original.clone();
            history.records[0].operations = vec!["entity-field:add".to_owned(); count];
            history.records[0].receipt["operations"] = serde_json::json!(vec![row.clone(); count]);
            cases.push(history);
        }
        for operation in [
            "foreign".to_owned(),
            "Entity-field:add".to_owned(),
            "entity_field:add".to_owned(),
            format!("{}:add", "x".repeat(61)),
        ] {
            let mut history = original.clone();
            history.records[0].operations = vec![operation.clone()];
            history.records[0].receipt["operations"][0]["operation"] = serde_json::json!(operation);
            cases.push(history);
        }
        for (member, value) in [
            ("foreign", serde_json::json!(true)),
            ("subject", serde_json::json!(7)),
            ("subject", serde_json::json!("x".repeat(257))),
            ("subjectFamily", serde_json::json!("foreign")),
            ("status", serde_json::json!("committed")),
            ("detail", Json::Null),
            ("detail", serde_json::json!("x".repeat(257))),
        ] {
            let mut history = original.clone();
            history.records[0].receipt["operations"][0][member] = value;
            cases.push(history);
        }
        for missing in ["subject", "subjectFamily", "status"] {
            let mut history = original.clone();
            history.records[0].receipt["operations"][0]
                .as_object_mut()
                .unwrap()
                .remove(missing);
            cases.push(history);
        }
        for (index, mut history) in cases.into_iter().enumerate() {
            // A fully recomputed public chain must not conceal schema-invalid
            // rows. The refusal is schema admission, not hash tampering.
            let record = &mut history.records[0];
            let digest = record_digest(
                GENESIS_DIGEST,
                &history.project,
                &record.intent_digest,
                &record.receipt,
            );
            record.receipt["history"]["recordDigest"] = serde_json::json!(digest);
            assert!(
                matches!(
                    History::decode(history.to_json_string().as_bytes()),
                    Err(ChangeError::HistoryInvalid {
                        detail: "history-schema"
                    })
                ),
                "decode case {index}"
            );
            assert!(
                matches!(
                    history.encode_for_commit(),
                    Err(ChangeError::HistoryInvalid {
                        detail: "history-schema"
                    })
                ),
                "commit case {index}"
            );
        }

        // Both the operation count's inclusive upper bound and JSON Schema's
        // character (not UTF-8 byte) bounds remain valid.
        let mut boundary = original;
        let record = &mut boundary.records[0];
        record.operations = vec!["entity-field:add".to_owned(); super::version::MAX_OPERATIONS];
        let mut row = row;
        row["subject"] = serde_json::json!("é".repeat(256));
        row["detail"] = serde_json::json!("é".repeat(256));
        record.receipt["operations"] = serde_json::json!(vec![row; super::version::MAX_OPERATIONS]);
        record.receipt["history"]["recordDigest"] = serde_json::json!(record_digest(
            GENESIS_DIGEST,
            &boundary.project,
            &record.intent_digest,
            &record.receipt
        ));
        boundary.encode_for_commit().unwrap();
    }

    #[test]
    fn genuine_r10_no_op_history_is_schema_invalid_not_repaired() {
        let bytes =
            include_bytes!("../../../../tests/fixtures/change/legacy/r10-no-op-history.json");
        assert!(matches!(
            History::decode(bytes),
            Err(ChangeError::HistoryInvalid {
                detail: "history-schema"
            })
        ));
    }

    #[test]
    fn history_capacity_is_checked_on_both_ingress_and_prospective_commit() {
        let bytes = include_bytes!("../../../../tests/fixtures/change/golden/change-history.json");
        let history = History::decode(bytes).unwrap();
        let encoded = history.encode_for_commit().unwrap();
        assert_eq!(History::decode(&encoded).unwrap(), history);

        let mut over_records = history.clone();
        over_records.records = vec![history.records[0].clone(); MAX_HISTORY_RECORDS + 1];
        assert!(matches!(
            over_records.encode_for_commit(),
            Err(ChangeError::PreconditionFailed {
                detail: "history-record-limit",
                ..
            })
        ));
        let mut document: Json = serde_json::from_slice(bytes).unwrap();
        document["records"] = serde_json::json!(vec![Json::Null; MAX_HISTORY_RECORDS + 1]);
        assert!(matches!(
            History::decode(&serde_json::to_vec(&document).unwrap()),
            Err(ChangeError::HistoryInvalid {
                detail: "record-limit"
            })
        ));

        let mut over_bytes = history;
        over_bytes.records[0].receipt["operations"][0]["detail"] =
            serde_json::json!("x".repeat(MAX_HISTORY_BYTES));
        assert!(matches!(
            over_bytes.encode_for_commit(),
            Err(ChangeError::PreconditionFailed {
                detail: "history-byte-limit",
                ..
            })
        ));
    }
}
