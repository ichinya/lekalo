//! The closed wire types of the target protocol (issue #27).
//!
//! Request envelopes are built here (and serialized to compact canonical
//! JSON: sorted keys, no whitespace — `serde_json`'s `BTreeMap` ordering);
//! response envelopes are parsed and semantically validated here. Unknown
//! members are rejected on both sides: the protocol is closed. Wire names
//! are snake_case exactly as the issue specifies; the schema artifact
//! `contracts/target-protocol.schema.v1.0.0.json` mirrors every bound.

use serde::{Deserialize, Serialize};

use super::scopes;
use super::version::{MAX_PLAN_ENTRIES, PROTOCOL_TOKEN, REQUEST_ID_PREFIX, VERSION};

/// The closed v1 operation set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    Describe,
    Scan,
    Bind,
    Validate,
    Generate,
    Verify,
    #[serde(rename = "clean")]
    Clean,
    #[serde(rename = "plan-clean")]
    PlanClean,
}

impl Operation {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Describe => "describe",
            Self::Scan => "scan",
            Self::Bind => "bind",
            Self::Validate => "validate",
            Self::Generate => "generate",
            Self::Verify => "verify",
            Self::Clean => "clean",
            Self::PlanClean => "plan-clean",
        }
    }

    /// Operations that consume the compiled IR and therefore require
    /// `ir_path` on the request.
    pub fn requires_ir(self) -> bool {
        matches!(self, Self::Validate | Self::Generate | Self::Verify)
    }

    /// Operations that mutate the project. Each requires a prior successful
    /// planning exchange (`generate --dry-run` or `plan-clean`) whose plan
    /// identifier the apply echoes.
    pub fn is_destructive_apply(self) -> bool {
        matches!(self, Self::Clean)
    }

    /// Operations whose response declares writes (plans or applied writes).
    pub fn declares_writes(self) -> bool {
        matches!(self, Self::Generate | Self::Clean | Self::PlanClean)
    }
}

/// The closed transport set: request delivery over stdin or a bounded
/// temporary file whose path the core appends as
/// `--lekalo-request-file <PATH>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Stdin,
    File,
}

/// The core-declared operation limits, mirrored into the request envelope so
/// the adapter can honor the same deadline and response bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    #[serde(skip_serializing_if = "Self::absent")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Self::absent")]
    pub max_output_bytes: Option<u64>,
}

impl Limits {
    fn absent(value: &Option<u64>) -> bool {
        value.is_none()
    }
}

/// One request envelope. `request_id` is filled in by the client from the
/// canonical bytes of the remaining members; optional members are omitted
/// from the wire while absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub protocol: String,
    pub protocol_version: String,
    pub operation: Operation,
    pub request_id: String,
    pub project_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ir_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

/// The adapter identity every response binds its evidence to.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterIdentity {
    pub id: String,
    pub version: String,
    pub digest: String,
}

/// The capability map returned by `describe`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub adapter: AdapterIdentity,
    pub protocol_versions: Vec<String>,
    pub operations: Vec<Operation>,
    pub transports: Vec<Transport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_scopes: Vec<String>,
    #[serde(default)]
    pub progress: bool,
}

/// One declared or observed write: the logical path, the action, and — for
/// `create` and `replace` — the SHA-256 of the exact resulting bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteEntry {
    pub path: String,
    pub action: WriteAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// The closed write action set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteAction {
    Create,
    Replace,
    Delete,
}

/// The evidence binding of a response: which adapter produced it and — for
/// planned or applied writes — which plan identity it honors.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub adapter: AdapterIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

/// One optional structured progress step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressStep {
    pub step: String,
    pub state: ProgressState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The closed progress state set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressState {
    Started,
    Done,
    Failed,
}

/// One adapter finding (validate/verify results).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub path: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// One scan entry (scan results).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanEntry {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// One binding proposal (bind results).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub module: String,
    pub target: String,
    pub profile: String,
}

/// The closed operation result payload; members are interpreted per
/// operation and every member is optional so one closed shape serves all
/// eight operations without a per-operation wire family.
#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<ScanEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings: Option<Vec<Finding>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bindings: Option<Vec<Binding>>,
}

/// The closed adapter error class set of an in-envelope operation error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorClass {
    Invalid,
    Unsupported,
    Infrastructure,
    Conflict,
}

impl ErrorClass {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::Unsupported => "unsupported",
            Self::Infrastructure => "infrastructure",
            Self::Conflict => "conflict",
        }
    }
}

/// An in-envelope operation error reported by the adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationError {
    pub class: ErrorClass,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Vec<String>>,
}

/// The closed response status set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Ok,
    Error,
}

/// One response envelope. Parsed with closed shapes; semantic validation
/// happens in [`validate_response`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseEnvelope {
    pub protocol: String,
    pub protocol_version: String,
    pub operation: Operation,
    pub request_id: String,
    pub status: ResponseStatus,
    pub evidence: Evidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Capabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OperationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes: Option<Vec<WriteEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<Vec<ProgressStep>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OperationError>,
}

/// Why a parsed response was refused before its payload could be used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseInvalidity {
    /// The envelope is not JSON.
    NotJson,
    /// The envelope violates its shape or bounds.
    Shape,
    /// The echoed request id differs from the request.
    RequestId,
    /// The echoed operation differs from the request.
    Operation,
    /// The evidence binds a different adapter than the handshake.
    Evidence,
    /// A response member is present that the operation forbids.
    UnexpectedMember,
    /// The writes member is missing for an operation that must declare it.
    WritesMissing,
    /// A write entry's digest member contradicts its action.
    WriteDigest,
    /// `status: "error"` without a well-formed error member, or the
    /// inverse pairing.
    ErrorPairing,
}

/// The combined rejection of one response identity check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseRejection {
    /// The response speaks a different protocol line or version.
    Protocol(ProtocolMismatch),
    /// The response is well-formed JSON but semantically malformed.
    Invalid(ResponseInvalidity),
}

/// Validate the identity members every response must carry. Protocol
/// token/version deviations are protocol mismatches (`unsupported` before
/// any generation); echo mismatches are malformed responses.
pub fn validate_response_identity(
    response: &ResponseEnvelope,
    expected_request_id: &str,
    expected_operation: Operation,
) -> Result<(), ResponseRejection> {
    if response.protocol != PROTOCOL_TOKEN {
        return Err(ResponseRejection::Protocol(ProtocolMismatch::Token));
    }
    if response.protocol_version != VERSION {
        return Err(ResponseRejection::Protocol(ProtocolMismatch::Version));
    }
    if response.operation != expected_operation {
        return Err(ResponseRejection::Invalid(ResponseInvalidity::Operation));
    }
    if response.request_id != expected_request_id {
        return Err(ResponseRejection::Invalid(ResponseInvalidity::RequestId));
    }
    Ok(())
}
impl ResponseInvalidity {
    /// The bounded wire token carried in diagnostic data.
    pub fn detail(self) -> &'static str {
        match self {
            Self::NotJson => "json",
            Self::Shape => "shape",
            Self::RequestId => "request-id",
            Self::Operation => "operation",
            Self::Evidence => "evidence",
            Self::UnexpectedMember => "member",
            Self::WritesMissing => "writes-missing",
            Self::WriteDigest => "write-digest",
            Self::ErrorPairing => "error-pairing",
        }
    }
}

/// Why a protocol identity was refused: the closed mismatch taxonomy that
/// maps onto `unsupported` before any generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolMismatch {
    /// The `protocol` token is not `lekalo.target/v1`.
    Token,
    /// The echoed `protocol_version` is not the negotiated one.
    Version,
    /// The negotiated version is absent from the adapter's declared set.
    Negotiation,
}

impl ProtocolMismatch {
    /// The bounded wire token carried in diagnostic data.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::Version => "version",
            Self::Negotiation => "negotiation",
        }
    }
}

/// Compute the deterministic request identifier over the canonical JSON of
/// the envelope with its `request_id` member absent: sorted keys, compact,
/// no timestamps, no absolute paths, no host data.
pub fn request_id(envelope: &RequestEnvelope) -> String {
    let mut value = serde_json::to_value(envelope).expect("envelope serializes");
    value
        .as_object_mut()
        .expect("envelope is an object")
        .remove("request_id");
    let bytes = serde_json::to_string(&value).expect("canonical value serializes");
    format!(
        "{}{}",
        REQUEST_ID_PREFIX,
        super::plan::sha256_hex(bytes.as_bytes())
    )
}

/// Compute the deterministic plan identifier over the canonical JSON of the
/// ordered plan entries.
pub fn plan_id(entries: &[WriteEntry]) -> String {
    let value = serde_json::to_value(entries).expect("entries serialize");
    let bytes = serde_json::to_string(&value).expect("canonical value serializes");
    format!(
        "{}{}",
        super::version::PLAN_ID_PREFIX,
        super::plan::sha256_hex(bytes.as_bytes())
    )
}

/// Validate write entries semantically: canonical order, digest/action
/// pairing, scope coverage, protected homes.
pub fn validate_writes(
    entries: &[WriteEntry],
    write_scopes: &[String],
) -> Result<(), super::TargetFailure> {
    if entries.len() > MAX_PLAN_ENTRIES {
        return Err(super::TargetFailure::plan_mismatch(None, "plan-limit"));
    }
    let mut sorted: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != entries.len()
        || entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>() != sorted
    {
        return Err(super::TargetFailure::plan_mismatch(None, "unsorted"));
    }
    for entry in entries {
        if !scopes::is_logical_path(&entry.path) {
            return Err(super::TargetFailure::plan_mismatch(
                Some(entry.path.clone()),
                "path",
            ));
        }
        if let Some(home) = scopes::protected_home(&entry.path) {
            return Err(super::TargetFailure::protected_path(&entry.path, home));
        }
        let covered = write_scopes
            .iter()
            .any(|scope| scopes::scope_covers(scope, &entry.path));
        if !covered {
            return Err(super::TargetFailure::scope_violation(
                Some(entry.path.clone()),
                "uncovered",
            ));
        }
        match (&entry.action, &entry.sha256) {
            (WriteAction::Delete, Some(_)) => {
                return Err(super::TargetFailure::plan_mismatch(
                    Some(entry.path.clone()),
                    "digest",
                ));
            }
            (WriteAction::Delete, None) => {}
            (_, None) => {
                return Err(super::TargetFailure::plan_mismatch(
                    Some(entry.path.clone()),
                    "digest",
                ));
            }
            (_, Some(digest)) => {
                if !is_sha256_digest(digest) {
                    return Err(super::TargetFailure::plan_mismatch(
                        Some(entry.path.clone()),
                        "digest",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Whether one string is the canonical `sha256:<64 lowercase hex>` spelling.
pub fn is_sha256_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> RequestEnvelope {
        RequestEnvelope {
            protocol: PROTOCOL_TOKEN.to_owned(),
            protocol_version: VERSION.to_owned(),
            operation: Operation::Describe,
            request_id: String::new(),
            project_root: ".".to_owned(),
            ir_path: None,
            target: None,
            profile: None,
            dry_run: None,
            limits: None,
            plan_id: None,
        }
    }

    #[test]
    fn request_ids_are_deterministic_and_content_bound() {
        let mut a = base_request();
        let mut b = base_request();
        assert_eq!(request_id(&a), request_id(&b));
        b.operation = Operation::Generate;
        assert_ne!(request_id(&a), request_id(&b));
        a.ir_path = Some(".lekalo/ir/planner.json".to_owned());
        assert_ne!(request_id(&a), request_id(&b));
        assert!(request_id(&a).starts_with(REQUEST_ID_PREFIX));
        assert_eq!(request_id(&a).len(), 68);
    }

    #[test]
    fn request_ids_ignore_the_identifier_member() {
        let mut a = base_request();
        a.request_id = "req-deadbeef".to_owned();
        let b = base_request();
        assert_eq!(request_id(&a), request_id(&b));
    }

    #[test]
    fn plan_ids_are_deterministic_and_order_sensitive() {
        let entry = |path: &str, action: WriteAction, digest: Option<&str>| WriteEntry {
            path: path.to_owned(),
            action,
            sha256: digest.map(str::to_owned),
        };
        let a = vec![entry("a.ts", WriteAction::Create, Some("sha256:00"))];
        let b = vec![entry("a.ts", WriteAction::Create, Some("sha256:00"))];
        let c = vec![entry("b.ts", WriteAction::Create, Some("sha256:00"))];
        assert_eq!(plan_id(&a), plan_id(&b));
        assert_ne!(plan_id(&a), plan_id(&c));
    }

    #[test]
    fn digest_spelling_is_exact() {
        let good = format!("sha256:{}", "a".repeat(64));
        assert!(is_sha256_digest(&good));
        assert!(!is_sha256_digest("sha256:ZZ"));
        assert!(!is_sha256_digest("00"));
        assert!(!is_sha256_digest(&format!("sha256:{}", "a".repeat(63))));
    }

    #[test]
    fn operation_wire_names_round_trip() {
        for operation in [
            Operation::Describe,
            Operation::Scan,
            Operation::Bind,
            Operation::Validate,
            Operation::Generate,
            Operation::Verify,
            Operation::Clean,
            Operation::PlanClean,
        ] {
            let text = serde_json::to_string(&operation).expect("serializes");
            let back: Operation = serde_json::from_str(&text).expect("parses");
            assert_eq!(back, operation);
            assert_eq!(back.as_str(), text.trim_matches('"'));
        }
        assert_eq!(
            serde_json::to_string(&Operation::PlanClean).unwrap(),
            "\"plan-clean\""
        );
    }
}
