//! Strict wire decoding and semantic validation (issue #106): the
//! hand-maintained codec obligation over the generated vocabulary.
//! Mirrors the host's admission: unknown members, explicit nulls, wrong
//! tokens, unsupported exact versions, and out-of-bounds values are
//! refused before any callback runs, with closed safe reason tokens.

use crate::generated::protocol;
use crate::ids::{is_plan_id, is_request_id, is_sha256_digest};
use crate::json::{canonical, read_single_object, read_single_object_with, ParseReason, Value};
use crate::paths::{
    is_capability_id, is_contract_version, is_logical_path, is_semantic_id, is_token,
};

/// A decode/validate refusal: one closed safe reason token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub stage: &'static str,
    pub reason: String,
}

impl Refusal {
    fn new(stage: &'static str, reason: &str) -> Self {
        Self {
            stage,
            reason: reason.to_owned(),
        }
    }
}

/// Decode a request document and run both admission passes.
pub fn decode_request(text: &str) -> Result<Value, Refusal> {
    let value = read_single_object(text, protocol::bounds::MAX_REQUEST_BYTES as usize)
        .map_err(|reason| Refusal::new("decode", parse_reason(&reason)))?;
    admit_request_value(&value)?;
    validate_request(&value)?;
    Ok(value)
}

/// Decode a request document without the semantic pass.
pub fn admit_request(text: &str) -> Result<Value, Refusal> {
    let value = read_single_object(text, protocol::bounds::MAX_REQUEST_BYTES as usize)
        .map_err(|reason| Refusal::new("decode", parse_reason(&reason)))?;
    admit_request_value(&value)?;
    Ok(value)
}

/// Decode a response document (structural admission only). Duplicate
/// keys are refused everywhere except map-typed member positions:
/// production's `capabilities.capabilities` is a serde `BTreeMap`
/// whose duplicates take last-wins (issue #106 review r5 #9).
pub fn admit_response(text: &str) -> Result<Value, Refusal> {
    let value = read_single_object_with(
        text,
        protocol::bounds::DEFAULT_MAX_OUTPUT_BYTES as usize,
        &|path, previous, next| {
            path == ["capabilities", "capabilities"]
                && [previous, next].iter().all(|v| {
                    v.as_str()
                        .is_some_and(|state| protocol::enums::SUPPORTSTATE.contains(&state))
                })
        },
    )
    .map_err(|reason| Refusal::new("decode", parse_reason(&reason)))?;
    admit_response_value(&value)?;
    Ok(value)
}

fn parse_reason(reason: &ParseReason) -> &str {
    match reason {
        ParseReason::NotJson => "not-json",
        ParseReason::TrailingDocument => "trailing-document",
        ParseReason::DuplicateKey => "duplicate-key",
        ParseReason::Oversize => "oversize",
    }
}

fn str_members(value: &Value) -> Vec<String> {
    value
        .member_names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn explicit_null(object: &Value, key: &str) -> bool {
    matches!(object.get(key), Some(Value::Null))
}

fn is_closed_object(value: &Value, allowed: &[&str]) -> bool {
    if !value.is_object() {
        return false;
    }
    str_members(value)
        .iter()
        .all(|key| allowed.contains(&key.as_str()))
}

fn bounded_text(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|text| text.chars().count() <= max)
}

/// Structural admission of one request envelope (closed shape + bounds).
pub fn admit_request_value(value: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("decode", reason));
    let allowed = [
        "dry_run",
        "ir_path",
        "limits",
        "lint_request",
        "native_request",
        "operation",
        "plan_id",
        "profile",
        "profile_capabilities",
        "profile_digest",
        "project_root",
        "protocol",
        "protocol_version",
        "request_id",
        "target",
    ];
    if !is_closed_object(value, &allowed) {
        return refuse(
            if str_members(value)
                .iter()
                .any(|k| !allowed.contains(&k.as_str()))
            {
                "unknown-member"
            } else {
                "shape"
            },
        );
    }
    for member in [
        "protocol",
        "protocol_version",
        "operation",
        "request_id",
        "project_root",
    ] {
        if value.get(member).is_none() {
            return refuse("shape");
        }
        if explicit_null(value, member) {
            return refuse("explicit-null");
        }
    }
    if value.get("protocol").and_then(Value::as_str) != Some(protocol::WIRE_TOKEN) {
        return refuse("token");
    }
    let version = value
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !protocol::SUPPORTED_PROTOCOL_VERSIONS.contains(&version) {
        return refuse("version");
    }
    let operation = value.get("operation").and_then(Value::as_str).unwrap_or("");
    if !protocol::LATEST_OPERATIONS.contains(&operation) {
        return refuse("operation");
    }
    if !value
        .get("request_id")
        .and_then(Value::as_str)
        .is_some_and(is_request_id)
    {
        return refuse("request-id");
    }
    if value.get("project_root").and_then(Value::as_str) != Some(".") {
        return refuse("grammar");
    }
    for member in ["ir_path", "target", "profile"] {
        if explicit_null(value, member) {
            return refuse("explicit-null");
        }
        if let Some(member_value) = value.get(member) {
            let ok = if member == "ir_path" {
                member_value.as_str().is_some_and(is_logical_path)
            } else {
                member_value.as_str().is_some_and(is_token)
            };
            if !ok {
                return refuse("grammar");
            }
        }
    }
    if explicit_null(value, "profile_digest") {
        return refuse("explicit-null");
    }
    if let Some(digest) = value.get("profile_digest") {
        if !digest.as_str().is_some_and(is_sha256_digest) {
            return refuse("bounds");
        }
    }
    if explicit_null(value, "dry_run") {
        return refuse("explicit-null");
    }
    if let Some(dry_run) = value.get("dry_run") {
        if dry_run.as_bool().is_none() {
            return refuse("shape");
        }
    }
    if explicit_null(value, "plan_id") {
        return refuse("explicit-null");
    }
    if let Some(plan) = value.get("plan_id") {
        if !plan.as_str().is_some_and(is_plan_id) {
            return refuse("bounds");
        }
    }
    if explicit_null(value, "limits") {
        return refuse("explicit-null");
    }
    if let Some(limits) = value.get("limits") {
        admit_limits(limits)?;
    }
    if explicit_null(value, "profile_capabilities") {
        return refuse("explicit-null");
    }
    if let Some(capabilities) = value.get("profile_capabilities") {
        admit_profile_capabilities(capabilities)?;
    }
    if explicit_null(value, "native_request") {
        return refuse("explicit-null");
    }
    if let Some(native) = value.get("native_request") {
        admit_native_request(native)?;
    }
    if explicit_null(value, "lint_request") {
        return refuse("explicit-null");
    }
    if let Some(lint) = value.get("lint_request") {
        admit_lint_request(lint)?;
    }
    Ok(())
}

fn admit_limits(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["max_output_bytes", "timeout_ms"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    // Production requires BOTH members whenever `limits` is present
    // (wire.rs: absent or out-of-range is admission `shape`); the SDK
    // used to admit partial limits objects.
    for (member, (min, max)) in [
        (
            "timeout_ms",
            (
                protocol::bounds::LIMITS_TIMEOUT_MIN,
                protocol::bounds::LIMITS_TIMEOUT_MAX,
            ),
        ),
        (
            "max_output_bytes",
            (
                protocol::bounds::LIMITS_OUTPUT_MIN,
                protocol::bounds::LIMITS_OUTPUT_MAX,
            ),
        ),
    ] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        let Some(number) = value.get(member) else {
            return Err(Refusal::new("decode", "shape"));
        };
        if matches!(number, Value::Float(_)) {
            // A float lexeme where production types `u64` is `shape`.
            return Err(Refusal::new("decode", "shape"));
        }
        match number.as_u64() {
            Some(number) if (min..=max).contains(&number) => {}
            _ => return Err(Refusal::new("decode", "bounds")),
        }
    }
    Ok(())
}

fn admit_profile_capabilities(value: &Value) -> Result<(), Refusal> {
    let Some(items) = value.as_array() else {
        return Err(Refusal::new("decode", "bounds"));
    };
    // Production requires a present, non-empty, strictly-sorted list
    // (wire.rs profile-capabilities: `!capabilities.is_empty()` is part
    // of the sorted-unique verdict) — an empty list is admission
    // `bounds`, never an empty default (issue #106 review r5 #1).
    if items.is_empty() || items.len() > protocol::bounds::MAX_DECLARED_CAPABILITIES as usize {
        return Err(Refusal::new("decode", "bounds"));
    }
    let mut previous: Option<String> = None;
    for capability in items {
        if !is_closed_object(capability, &["id", "support"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !capability
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(is_capability_id)
        {
            return Err(Refusal::new("decode", "bounds"));
        }
        let support = capability
            .get("support")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !protocol::enums::SUPPORTSTATE.contains(&support) {
            return Err(Refusal::new("decode", "shape"));
        }
        let id = capability
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if previous
            .as_deref()
            .is_some_and(|previous| id.as_str() <= previous)
        {
            return Err(Refusal::new("decode", "bounds"));
        }
        previous = Some(id);
    }
    Ok(())
}

/// An optional wire array: absent means the empty default; a present
/// member must be a JSON array (issue #106 review r4 #3). Explicit null
/// keeps its own refusal token, and every other present type — object,
/// string, number, bool — is `bounds`, exactly like production's typed
/// Vec decoder refusing a non-sequence.
fn optional_array<'a>(container: &'a Value, member: &str) -> Result<&'a [Value], Refusal> {
    match container.get(member) {
        None => Ok(&[]),
        Some(Value::Null) => Err(Refusal::new("decode", "explicit-null")),
        Some(items) => items
            .as_array()
            .ok_or_else(|| Refusal::new("decode", "bounds")),
    }
}

/// All-default struct members accepted from a positional JSON sequence
/// (serde `visit_seq` parity, issue #106 review r5 #9): production
/// deserializes every `#[serde(default)]` struct from a seq positionally,
/// so `[]` (and any shorter prefix) decodes with the defaults filling
/// the tail; a longer seq refuses `shape` exactly like serde's
/// invalid-length error.
fn seq_as_members<'a>(
    value: &'a Value,
    fields: &[&str],
) -> Result<std::borrow::Cow<'a, Value>, Refusal> {
    let Some(items) = value.as_array() else {
        return Ok(std::borrow::Cow::Borrowed(value));
    };
    if items.len() > fields.len() {
        return Err(Refusal::new("decode", "shape"));
    }
    let members: Vec<(String, Value)> = fields
        .iter()
        .zip(items.iter())
        .map(|(field, item)| ((*field).to_owned(), item.clone()))
        .collect();
    Ok(std::borrow::Cow::Owned(Value::Object(members)))
}

fn admit_content_ref(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["adapter", "digest", "revision"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value
        .get("digest")
        .and_then(Value::as_str)
        .is_some_and(is_sha256_digest)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    // Revision is an unbounded wire string; the optional adapter identity carries the production adapter
    // token grammar (issue #106 review r4 #2, core wire.rs validate:
    // `!scopes::is_token(adapter)` refuses `native-request`): a
    // present, non-token identity must never reach dispatch.
    if explicit_null(value, "adapter") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(adapter) = value.get("adapter") {
        if !adapter.as_str().is_some_and(is_token) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    if explicit_null(value, "revision") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(member_value) = value.get("revision") {
        // Production types `revision` `Option<String>` with no bound
        // beyond serde's own string admission (issue #106 review r5
        // #9): only the member type is checked.
        if member_value.as_str().is_none() {
            return Err(Refusal::new("decode", "shape"));
        }
    }
    Ok(())
}

fn admit_native_request(value: &Value) -> Result<(), Refusal> {
    let allowed = [
        "capability_snapshot_digest",
        "changes",
        "execution_policy_ref",
        "input_manifest_digest",
        "observed_ref",
        "scan_ref",
        "tool_catalog_digest",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    for member in [
        "input_manifest_digest",
        "tool_catalog_digest",
        "capability_snapshot_digest",
    ] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if !value
            .get(member)
            .and_then(Value::as_str)
            .is_some_and(is_sha256_digest)
        {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    let changes = value
        .get("changes")
        .ok_or_else(|| Refusal::new("decode", "shape"))?;
    // All-default struct: production deserializes `changes` from a
    // positional seq too (`[]` decodes with both lists defaulted),
    // exactly like serde struct-from-seq (issue #106 review r5 #9).
    let changes = seq_as_members(changes, &["files", "symbols"])?;
    let changes = changes.as_ref();
    if !is_closed_object(changes, &["files", "symbols"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    // Optional arrays default ONLY when absent (issue #106 review r4
    // #3): a present member with a non-array JSON type — object, string,
    // number, or explicit null — is admission `bounds`, never an empty
    // default. `unwrap_or(&[])` erased the distinction and admitted
    // `files:{}` into dispatch.
    let files = optional_array(changes, "files")?;
    let symbols = optional_array(changes, "symbols")?;
    if files.len() > 1024 || symbols.len() > 1024 {
        return Err(Refusal::new("decode", "bounds"));
    }
    // Production validates the changed files without any ordering rule
    // (wire.rs native-request: member checks only) — the SDK's old
    // ascending-path requirement refused legal documents (review r5 #9).
    for change in files {
        if !is_closed_object(change, &["after_digest", "before_digest", "change", "path"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !change
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(is_logical_path)
        {
            return Err(Refusal::new("decode", "bounds"));
        }
        let kind = change.get("change").and_then(Value::as_str).unwrap_or("");
        if !protocol::enums::NATIVECHANGEKIND.contains(&kind) {
            return Err(Refusal::new("decode", "shape"));
        }
        for member in ["after_digest", "before_digest"] {
            if explicit_null(change, member) {
                return Err(Refusal::new("decode", "explicit-null"));
            }
            if let Some(digest) = change.get(member) {
                if !digest.as_str().is_some_and(is_sha256_digest) {
                    return Err(Refusal::new("decode", "bounds"));
                }
            }
        }
    }
    // Symbols carry no per-item bound on the wire (only the count is
    // bounded) — empty and long strings are both legal (review r5 #9).
    for symbol in symbols {
        if symbol.as_str().is_none() {
            return Err(Refusal::new("decode", "shape"));
        }
    }
    admit_content_ref(value.get("scan_ref").unwrap_or(&Value::Null))?;
    admit_content_ref(value.get("execution_policy_ref").unwrap_or(&Value::Null))?;
    if explicit_null(value, "observed_ref") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(observed) = value.get("observed_ref") {
        admit_content_ref(observed)?;
    }
    Ok(())
}

fn admit_lint_request(value: &Value) -> Result<(), Refusal> {
    let allowed = ["bindings", "files", "pins", "scope"];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    // Every lint member is required on the wire (production's LintRequest
    // has no serde defaults): a missing member is `shape` exactly like
    // serde's missing-field error — only an explicit null gets its own
    // token (issue #106 review r5 #8).
    for member in ["scope", "files", "bindings", "pins"] {
        if value.get(member).is_none() {
            return Err(Refusal::new("decode", "shape"));
        }
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
    }
    // Optional arrays default ONLY when absent (issue #106 review r4 #3):
    // present non-array values (and explicit nulls) refuse `bounds` /
    // `explicit-null` instead of collapsing to an empty default.
    let scope = optional_array(value, "scope")?;
    let files = optional_array(value, "files")?;
    let bindings = optional_array(value, "bindings")?;
    if scope.is_empty() || scope.len() > 10_000 || files.len() > 4_096 || bindings.len() > 10_000 {
        return Err(Refusal::new("decode", "bounds"));
    }
    for scope_entry in scope {
        if !bounded_text(scope_entry, protocol::bounds::ERROR_DETAIL_MAX as usize) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    for file in files {
        if !file.as_str().is_some_and(is_logical_path) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    for binding in bindings {
        let allowed = ["fingerprint", "kind", "nativeId", "path", "symbol"];
        if !is_closed_object(binding, &allowed) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !binding
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(is_logical_path)
        {
            return Err(Refusal::new("decode", "bounds"));
        }
        for member in ["nativeId", "symbol"] {
            if !bounded_text(binding.get(member).unwrap_or(&Value::Null), 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
        let kind = binding.get("kind").and_then(Value::as_str).unwrap_or("");
        if !["command", "entity"].contains(&kind) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !binding
            .get("fingerprint")
            .and_then(Value::as_str)
            .is_some_and(is_sha256_digest)
        {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    let pins = value.get("pins").unwrap_or(&Value::Null);
    let allowed = [
        "capabilities",
        "ir",
        "model",
        "observed",
        "profile",
        "revision",
    ];
    if !is_closed_object(pins, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    // The pinned 0.6.4 schema carries every pin as a State-tagged value
    // ({state:"known",value}|{state:"unknown"|"withheld"|"unsupported"}).
    for member in allowed {
        if explicit_null(pins, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        admit_state_string(pins.get(member).unwrap_or(&Value::Null))?;
    }
    Ok(())
}

/// One State-tagged string value of the pinned 0.6.4 lint schema.
fn admit_state_string(value: &Value) -> Result<(), Refusal> {
    if !value.is_object() {
        return Err(Refusal::new("decode", "shape"));
    }
    let state = value.get("state").and_then(Value::as_str);
    match state {
        Some("known") => {
            let names = value.member_names();
            if names.len() != 2 || !names.contains(&"value") {
                return Err(Refusal::new("decode", "shape"));
            }
            if !bounded_text(
                value.get("value").unwrap_or(&Value::Null),
                protocol::bounds::ERROR_DETAIL_MAX as usize,
            ) {
                return Err(Refusal::new("decode", "bounds"));
            }
            Ok(())
        }
        Some("unknown" | "withheld" | "unsupported") => {
            if value.member_names().len() != 1 {
                return Err(Refusal::new("decode", "shape"));
            }
            Ok(())
        }
        _ => Err(Refusal::new("decode", "shape")),
    }
}

// ---------------------------------------------------------------------------
// Response admission
// ---------------------------------------------------------------------------

fn admit_identity(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["digest", "id", "version"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(is_token)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !value
        .get("version")
        .and_then(Value::as_str)
        .is_some_and(is_contract_version)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !value
        .get("digest")
        .and_then(Value::as_str)
        .is_some_and(is_sha256_digest)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    Ok(())
}

fn admit_evidence(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["adapter", "plan_id"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    admit_identity(value.get("adapter").unwrap_or(&Value::Null))?;
    if explicit_null(value, "plan_id") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(plan) = value.get("plan_id") {
        if !plan.as_str().is_some_and(is_plan_id) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    Ok(())
}

fn admit_write_entry(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["action", "path", "sha256"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(is_logical_path)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    let action = value.get("action").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::WRITEACTION.contains(&action) {
        return Err(Refusal::new("decode", "shape"));
    }
    if explicit_null(value, "sha256") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    // `sha256` is a free-form `Option<String>` on the wire (production
    // has no digest grammar here — the action/digest pairing is the
    // client's semantic layer): any string is admissible, only the type
    // and the explicit-null refusal remain (issue #106 review r5 #9).
    if let Some(digest) = value.get("sha256") {
        if digest.as_str().is_none() {
            return Err(Refusal::new("decode", "shape"));
        }
    }
    Ok(())
}

fn admit_scan_entry(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["detail", "evidence", "kind", "path"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(is_logical_path)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !bounded_text(value.get("kind").unwrap_or(&Value::Null), 128) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if explicit_null(value, "detail") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(detail) = value.get("detail") {
        if !bounded_text(detail, 128) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    if explicit_null(value, "evidence") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(evidence_member) = value.get("evidence") {
        // All-default evidence: `{}` and the positional `[]` both decode
        // (signature/references default) exactly like production's serde
        // struct-from-seq (issue #106 review r5 #9).
        let evidence = seq_as_members(evidence_member, &["signature", "references"])?;
        if !is_closed_object(&evidence, &["references", "signature"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if explicit_null(&evidence, "signature") {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(signature) = evidence.get("signature") {
            if !signature.as_str().is_some_and(is_sha256_digest) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
        // `references` is a `#[serde(default)] Vec`: absent defaults
        // empty; an explicit null is a typed refusal, never a default.
        if evidence.get("references") == Some(&Value::Null) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        let references = optional_array(&evidence, "references")?;
        if references.len() > protocol::bounds::REFERENCES_MAX as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for reference in references {
            if !is_closed_object(reference, &["confidence", "role", "target"]) {
                return Err(Refusal::new("decode", "shape"));
            }
            // The target carries the manifest-local semantic-id grammar
            // (core trace::id::is_semantic_id, issue #106 review r3 #8):
            // production's response decoder refuses path-style slash
            // targets, so the SDK must too.
            if !reference
                .get("target")
                .and_then(Value::as_str)
                .is_some_and(is_semantic_id)
            {
                return Err(Refusal::new("decode", "bounds"));
            }
            let role = reference.get("role").and_then(Value::as_str).unwrap_or("");
            if !protocol::enums::SCANENTRYREFERENCEROLE.contains(&role) {
                return Err(Refusal::new("decode", "shape"));
            }
            let confidence = reference
                .get("confidence")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !protocol::enums::SCANENTRYCONFIDENCE.contains(&confidence) {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    Ok(())
}

fn admit_native_plan(value: &Value) -> Result<(), Refusal> {
    let allowed = [
        "commands",
        "completeness",
        "digest",
        "entries",
        "kind",
        "packages",
        "run_eligibility",
        "units",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::NATIVEPLANKIND.contains(&kind) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value
        .get("digest")
        .and_then(Value::as_str)
        .is_some_and(is_sha256_digest)
    {
        return Err(Refusal::new("decode", "bounds"));
    }
    for member in ["commands", "entries", "packages", "units"] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(number) = value.get(member) {
            match number.as_u64() {
                Some(number) if number <= protocol::bounds::MAX_CONSTRAINT_VALUE => {}
                _ => return Err(Refusal::new("decode", "bounds")),
            }
        }
    }
    for (member, values) in [
        ("completeness", protocol::enums::NATIVECOMPLETENESS),
        ("run_eligibility", protocol::enums::NATIVERUNELIGIBILITY),
    ] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(spelling) = value.get(member) {
            if !values.contains(&spelling.as_str().unwrap_or("")) {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    Ok(())
}

fn admit_result(member_value: &Value) -> Result<(), Refusal> {
    // All-default operation result: production deserializes it from a
    // positional seq too (`result:[]` is the all-default value), exactly
    // like serde struct-from-seq (issue #106 review r5 #9).
    let value = seq_as_members(
        member_value,
        &[
            "ok",
            "truncated",
            "entries",
            "findings",
            "bindings",
            "native_plan",
            "lint_evidence",
        ],
    )?;
    let value = value.as_ref();
    let allowed = [
        "bindings",
        "entries",
        "findings",
        "lint_evidence",
        "native_plan",
        "ok",
        "truncated",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    for member in ["ok", "truncated"] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(flag) = value.get(member) {
            if flag.as_bool().is_none() {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    if explicit_null(value, "entries") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(entries) = value.get("entries") {
        let Some(items) = entries.as_array() else {
            return Err(Refusal::new("decode", "bounds"));
        };
        if items.len() > protocol::bounds::SCAN_ENTRIES_MAX as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for entry in items {
            admit_scan_entry(entry)?;
        }
    }
    if explicit_null(value, "findings") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(findings) = value.get("findings") {
        let Some(items) = findings.as_array() else {
            return Err(Refusal::new("decode", "bounds"));
        };
        if items.len() > protocol::bounds::FINDINGS_MAX as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for finding in items {
            if !is_closed_object(finding, &["code", "detail", "path"]) {
                return Err(Refusal::new("decode", "shape"));
            }
            if !finding
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(is_logical_path)
            {
                return Err(Refusal::new("decode", "bounds"));
            }
            if !bounded_text(finding.get("code").unwrap_or(&Value::Null), 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
            if let Some(detail) = finding.get("detail") {
                if !bounded_text(detail, 128) {
                    return Err(Refusal::new("decode", "bounds"));
                }
            }
        }
    }
    if explicit_null(value, "bindings") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(bindings) = value.get("bindings") {
        let Some(items) = bindings.as_array() else {
            return Err(Refusal::new("decode", "bounds"));
        };
        if items.len() > protocol::bounds::BINDINGS_MAX as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for binding in items {
            if !is_closed_object(binding, &["module", "profile", "target"]) {
                return Err(Refusal::new("decode", "shape"));
            }
            for member in ["module", "profile", "target"] {
                if !binding
                    .get(member)
                    .and_then(Value::as_str)
                    .is_some_and(is_token)
                {
                    return Err(Refusal::new("decode", "bounds"));
                }
            }
        }
    }
    if explicit_null(value, "native_plan") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(plan) = value.get("native_plan") {
        admit_native_plan(plan)?;
    }
    if explicit_null(value, "lint_evidence") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(lint) = value.get("lint_evidence") {
        // The closed LintEvidence subtree is validated before emission
        // (issue #106 review r2 #3): a shallow object check admitted `{}`
        // as `status:"ok"`; the full pinned schema subtree must hold.
        admit_lint_evidence(lint)?;
    }
    Ok(())
}

/// The bounded string grammar shared by the LintEvidence subtree:
/// 1..=256 chars of `[A-Za-z0-9]` then `[_./:#@\\-]` and alphanumerics.
fn lint_bounded_256(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        let mut chars = text.chars();
        match chars.next() {
            Some(first) if first.is_ascii_alphanumeric() => {}
            _ => return false,
        }
        text.len() <= 256
            && chars.all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '_' | '.' | '/' | ':' | '#' | '@' | '\\' | '-')
            })
    })
}

fn lint_string_array(value: &Value, max: usize, unique: bool) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    items.len() <= max
        && items.iter().all(lint_bounded_256)
        && (!unique || {
            let set: std::collections::BTreeSet<&str> =
                items.iter().filter_map(Value::as_str).collect();
            set.len() == items.len()
        })
}

fn lint_state_string(value: &Value) -> bool {
    if !is_closed_object(value, &["state", "value"]) {
        return false;
    }
    match value.get("state").and_then(Value::as_str) {
        Some("known") => lint_bounded_256(value.get("value").unwrap_or(&Value::Null)),
        Some("unknown" | "withheld" | "unsupported") => value.get("value").is_none(),
        _ => false,
    }
}

fn lint_state_bool(value: &Value) -> bool {
    if !is_closed_object(value, &["state", "value"]) {
        return false;
    }
    match value.get("state").and_then(Value::as_str) {
        Some("known") => value.get("value").and_then(Value::as_bool).is_some(),
        Some("unknown" | "withheld" | "unsupported") => value.get("value").is_none(),
        _ => false,
    }
}

fn lint_state_u64(value: &Value) -> bool {
    if !is_closed_object(value, &["state", "value"]) {
        return false;
    }
    match value.get("state").and_then(Value::as_str) {
        Some("known") => value
            .get("value")
            .and_then(Value::as_u64)
            .is_some_and(|n| n <= 10_000_000),
        Some("unknown" | "withheld" | "unsupported") => value.get("value").is_none(),
        _ => false,
    }
}

fn lint_sha256(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|text| text.len() <= 71 && is_sha256_digest(text))
}

fn lint_pins(value: &Value) -> bool {
    let allowed = [
        "capabilities",
        "ir",
        "model",
        "observed",
        "profile",
        "revision",
    ];
    is_closed_object(value, &allowed)
        && allowed
            .iter()
            .all(|pin| lint_state_string(value.get(pin).unwrap_or(&Value::Null)))
}

fn lint_producer(value: &Value) -> bool {
    let allowed = [
        "artifactDigest",
        "compiler",
        "framework",
        "id",
        "recipe",
        "tool",
        "version",
    ];
    is_closed_object(value, &allowed)
        && ["id", "version", "tool", "recipe"]
            .iter()
            .all(|member| lint_bounded_256(value.get(member).unwrap_or(&Value::Null)))
        && lint_sha256(value.get("artifactDigest").unwrap_or(&Value::Null))
        && lint_state_string(value.get("compiler").unwrap_or(&Value::Null))
        && lint_state_string(value.get("framework").unwrap_or(&Value::Null))
}

fn lint_source(value: &Value) -> bool {
    let allowed = ["bytes", "fingerprint", "id", "path"];
    is_closed_object(value, &allowed)
        && lint_bounded_256(value.get("id").unwrap_or(&Value::Null))
        && value
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| path.len() <= 512 && is_logical_path(path))
        && lint_sha256(value.get("fingerprint").unwrap_or(&Value::Null))
        && value
            .get("bytes")
            .and_then(Value::as_u64)
            .is_some_and(|n| n <= protocol::bounds::MAX_REQUEST_BYTES)
}

fn lint_span(value: &Value) -> bool {
    let allowed = [
        "column",
        "end",
        "endColumn",
        "endLine",
        "id",
        "line",
        "source",
        "start",
    ];
    if !is_closed_object(value, &allowed) {
        return false;
    }
    // Every span member is an unbounded `u64` in production (0 is a
    // legal line/column; the SDK's old min-1 refused legal evidence —
    // review r5 #9).
    let bounded = |member: &str| {
        value
            .get(member)
            .and_then(Value::as_u64)
            .is_some_and(|n| n <= protocol::bounds::MAX_REQUEST_BYTES)
    };
    lint_bounded_256(value.get("id").unwrap_or(&Value::Null))
        && lint_bounded_256(value.get("source").unwrap_or(&Value::Null))
        && bounded("start")
        && bounded("end")
        && bounded("line")
        && bounded("column")
        && bounded("endLine")
        && bounded("endColumn")
}

fn lint_coverage(value: &Value) -> bool {
    let allowed = [
        "eligible",
        "examined",
        "limitations",
        "rule",
        "scope",
        "state",
        "target",
    ];
    is_closed_object(value, &allowed)
        && ["rule", "target", "scope"]
            .iter()
            .all(|member| lint_bounded_256(value.get(member).unwrap_or(&Value::Null)))
        && matches!(
            value.get("state").and_then(Value::as_str),
            Some("complete" | "partial" | "unknown" | "unsupported" | "withheld" | "disabled")
        )
        && lint_state_u64(value.get("eligible").unwrap_or(&Value::Null))
        && lint_state_u64(value.get("examined").unwrap_or(&Value::Null))
        && lint_string_array(
            value.get("limitations").unwrap_or(&Value::Null),
            10_000,
            true,
        )
}

fn lint_route(value: &Value) -> bool {
    let allowed = ["confidence", "currency", "identity", "origin"];
    is_closed_object(value, &allowed)
        && lint_bounded_256(value.get("identity").unwrap_or(&Value::Null))
        && matches!(
            value.get("confidence").and_then(Value::as_str),
            Some("unknown" | "low" | "medium" | "high" | "exact")
        )
        && matches!(
            value.get("currency").and_then(Value::as_str),
            Some("current" | "stale" | "unknown")
        )
        && matches!(
            value.get("origin").and_then(Value::as_str),
            Some("explicit" | "extracted" | "inferred")
        )
}

fn lint_step(value: &Value) -> bool {
    let allowed = ["confidence", "guards", "identity", "locations", "role"];
    is_closed_object(value, &allowed)
        && matches!(
            value.get("role").and_then(Value::as_str),
            Some("binding" | "trigger" | "registration" | "callback" | "effect")
        )
        && lint_bounded_256(value.get("identity").unwrap_or(&Value::Null))
        && lint_string_array(value.get("locations").unwrap_or(&Value::Null), 32, true)
        && lint_string_array(value.get("guards").unwrap_or(&Value::Null), 32, true)
        && matches!(
            value.get("confidence").and_then(Value::as_str),
            Some("unknown" | "low" | "medium" | "high" | "exact")
        )
}

fn lint_record(value: &Value) -> bool {
    let allowed = [
        "activation",
        "binding",
        "candidates",
        "claim",
        "confidence",
        "configuration",
        "currency",
        "field",
        "guards",
        "id",
        "key",
        "kind",
        "locations",
        "mechanism",
        "nativeId",
        "operation",
        "origin",
        "ownership",
        "resource",
        "semanticSymbol",
        "subject",
        "trace",
        "value",
    ];
    if !is_closed_object(value, &allowed) {
        return false;
    }
    let enums_ok = matches!(
        value.get("kind").and_then(Value::as_str),
        Some(
            "binding-candidates"
                | "dispatch"
                | "convention"
                | "effect"
                | "path"
                | "reflection"
                | "string-reference"
                | "default"
                | "native-edge"
                | "field-write"
        )
    ) && matches!(
        value.get("mechanism").and_then(Value::as_str),
        Some("direct" | "observer" | "hook" | "magic")
    ) && matches!(
        value.get("confidence").and_then(Value::as_str),
        Some("unknown" | "low" | "medium" | "high" | "exact")
    ) && matches!(
        value.get("currency").and_then(Value::as_str),
        Some("current" | "stale" | "unknown")
    ) && matches!(
        value.get("origin").and_then(Value::as_str),
        Some("explicit" | "extracted" | "inferred")
    ) && matches!(
        value.get("claim").and_then(Value::as_str),
        Some("structural" | "possible-behavior" | "verified-behavior")
    );
    let strings_ok = ["id", "subject", "nativeId"]
        .iter()
        .all(|member| lint_bounded_256(value.get(member).unwrap_or(&Value::Null)));
    let states_ok = [
        "semanticSymbol",
        "operation",
        "resource",
        "field",
        "value",
        "key",
    ]
    .iter()
    .all(|member| lint_state_string(value.get(member).unwrap_or(&Value::Null)));
    let bools_ok = ["binding", "configuration", "ownership", "trace"]
        .iter()
        .all(|member| lint_state_bool(value.get(member).unwrap_or(&Value::Null)));
    let candidates_ok = value
        .get("candidates")
        .and_then(Value::as_array)
        .is_some_and(|items| items.len() <= 32 && items.iter().all(lint_route));
    let activation_ok = value
        .get("activation")
        .and_then(Value::as_array)
        .is_some_and(|items| items.len() <= 32 && items.iter().all(lint_step));
    let locations_ok = lint_string_array(value.get("locations").unwrap_or(&Value::Null), 32, true);
    let guards_ok = lint_string_array(value.get("guards").unwrap_or(&Value::Null), 32, true);
    enums_ok
        && strings_ok
        && states_ok
        && bools_ok
        && candidates_ok
        && activation_ok
        && locations_ok
        && guards_ok
}

/// The closed `LintEvidence` subtree of a lint result (issue #106 review
/// r2 #3): all twelve members required, const identity/version, closed
/// nested objects — the SDK refuses shallow or malformed evidence instead
/// of emitting it as success.
fn admit_lint_evidence(value: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("decode", reason));
    let allowed = [
        "coverage",
        "identity",
        "inputManifestDigest",
        "limitations",
        "locations",
        "pins",
        "producer",
        "records",
        "schemaVersion",
        "scope",
        "sources",
        "target",
    ];
    if !is_closed_object(value, &allowed) {
        return refuse("shape");
    }
    for member in allowed {
        if value.get(member).is_none() {
            return refuse("shape");
        }
    }
    if value.get("schemaVersion").and_then(Value::as_str) != Some("lekalo/ai-lint-evidence/v0.6.4")
        || value.get("identity").and_then(Value::as_str)
            != Some("dev.lekalo.ai-lint-evidence@0.6.4")
    {
        return refuse("shape");
    }
    if !lint_bounded_256(value.get("target").unwrap_or(&Value::Null))
        || !lint_string_array(value.get("scope").unwrap_or(&Value::Null), 10_000, true)
        || !lint_producer(value.get("producer").unwrap_or(&Value::Null))
        || !lint_pins(value.get("pins").unwrap_or(&Value::Null))
        || !lint_sha256(value.get("inputManifestDigest").unwrap_or(&Value::Null))
    {
        return refuse("bounds");
    }
    let sources = value.get("sources").and_then(Value::as_array);
    let Some(sources) = sources else {
        return refuse("bounds");
    };
    if sources.len() > 10_000 || !sources.iter().all(lint_source) {
        return refuse("bounds");
    }
    let spans = value.get("locations").and_then(Value::as_array);
    let Some(spans) = spans else {
        return refuse("bounds");
    };
    if spans.len() > 32 || !spans.iter().all(lint_span) {
        return refuse("bounds");
    }
    // uniqueItems on the span array
    let span_keys: std::collections::BTreeSet<String> = spans.iter().map(canonical).collect();
    if span_keys.len() != spans.len() {
        return refuse("bounds");
    }
    let coverages = value.get("coverage").and_then(Value::as_array);
    let Some(coverages) = coverages else {
        return refuse("bounds");
    };
    if coverages.len() > 10_000 || !coverages.iter().all(lint_coverage) {
        return refuse("bounds");
    }
    let records = value.get("records").and_then(Value::as_array);
    let Some(records) = records else {
        return refuse("bounds");
    };
    if records.len() > 10_000 || !records.iter().all(lint_record) {
        return refuse("bounds");
    }
    if !lint_string_array(
        value.get("limitations").unwrap_or(&Value::Null),
        10_000,
        true,
    ) {
        return refuse("bounds");
    }
    Ok(())
}

fn admit_progress(value: &Value) -> Result<(), Refusal> {
    let Some(steps) = value.as_array() else {
        return Err(Refusal::new("decode", "bounds"));
    };
    if steps.len() > protocol::bounds::PROGRESS_STEPS_MAX as usize {
        return Err(Refusal::new("decode", "bounds"));
    }
    for step in steps {
        if !is_closed_object(step, &["detail", "state", "step"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !bounded_text(step.get("step").unwrap_or(&Value::Null), 128) {
            return Err(Refusal::new("decode", "bounds"));
        }
        let state = step.get("state").and_then(Value::as_str).unwrap_or("");
        if !protocol::enums::PROGRESSSTATE.contains(&state) {
            return Err(Refusal::new("decode", "shape"));
        }
        if explicit_null(step, "detail") {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(detail) = step.get("detail") {
            if !bounded_text(detail, 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
    }
    Ok(())
}

fn admit_error(value: &Value) -> Result<(), Refusal> {
    let allowed = ["class", "code", "detail", "message", "partial", "retryable"];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    let class = value.get("class").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::ERRORCLASS.contains(&class) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !bounded_text(value.get("code").unwrap_or(&Value::Null), 128) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !bounded_text(value.get("message").unwrap_or(&Value::Null), 256) {
        return Err(Refusal::new("decode", "bounds"));
    }
    for member in ["partial", "retryable"] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(flag) = value.get(member) {
            if flag.as_bool().is_none() {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    if explicit_null(value, "detail") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(details) = value.get("detail") {
        let Some(items) = details.as_array() else {
            return Err(Refusal::new("decode", "bounds"));
        };
        if items.len() > protocol::bounds::ERROR_DETAILS_MAX as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for detail in items {
            if !bounded_text(detail, 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
    }
    Ok(())
}

/// The shared declared-list verdict: present lists must be unique,
/// member-valid, and inside `[min, max]`; absent members pass only when
/// not required. `min = 1` for the required declared lists (production
/// `unique(…, 1, …)`, issue #106 review r5 #1): an empty declaration is
/// `bounds`, never an empty default.
fn unique_bounded(
    value: &Value,
    member: &str,
    min: usize,
    max: usize,
    check: &dyn Fn(&Value) -> bool,
    required: bool,
) -> bool {
    match value.get(member) {
        None => !required,
        Some(Value::Null) => false,
        Some(items) => {
            let Some(items) = items.as_array() else {
                return false;
            };
            items.len() >= min && items.len() <= max && items.iter().all(check) && {
                let mut seen: Vec<String> = items.iter().map(crate::json::canonical).collect();
                seen.sort();
                seen.dedup();
                seen.len() == items.len()
            }
        }
    }
}

fn admit_capabilities(value: &Value, protocol_version: &str) -> Result<(), Refusal> {
    let allowed = [
        "adapter",
        "capabilities",
        "constraints",
        "ir_versions",
        "operations",
        "profiles",
        "progress",
        "protocol_versions",
        "read_scopes",
        "targets",
        "transports",
        "write_scopes",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    admit_identity(value.get("adapter").unwrap_or(&Value::Null))?;
    let is_contract = |value: &Value| value.as_str().is_some_and(is_contract_version);
    let is_token_value = |value: &Value| value.as_str().is_some_and(is_token);
    // Operation set gating mirrors production validate_bounds exactly
    // (issue #106 review r5 #1): `lint` is legal only on a 0.6.4
    // describe, and the closed operation ceiling is 10 there, 9 on
    // every other version.
    let operations_max = if protocol_version == LINT_VERSION {
        protocol::bounds::OPERATIONS_MAX_LINT as usize
    } else {
        protocol::bounds::OPERATIONS_MAX_BASE as usize
    };
    let is_operation = |value: &Value| -> bool {
        value.as_str().is_some_and(|op| {
            protocol::LATEST_OPERATIONS.contains(&op)
                && (protocol_version == LINT_VERSION || op != "lint")
        })
    };
    let is_transport = |value: &Value| {
        value
            .as_str()
            .is_some_and(|t| protocol::enums::TRANSPORT.contains(&t))
    };
    let is_scope_text = |value: &Value| {
        value
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 512)
    };
    if !unique_bounded(
        value,
        "protocol_versions",
        1,
        protocol::bounds::PROTOCOL_VERSIONS_MAX as usize,
        &is_contract,
        true,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(value, "operations", 1, operations_max, &is_operation, true) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(
        value,
        "transports",
        1,
        protocol::bounds::TRANSPORTS_MAX as usize,
        &is_transport,
        true,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(
        value,
        "targets",
        0,
        protocol::bounds::TARGETS_MAX as usize,
        &is_token_value,
        false,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(
        value,
        "profiles",
        0,
        protocol::bounds::PROFILES_MAX as usize,
        &is_token_value,
        false,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(
        value,
        "read_scopes",
        0,
        protocol::bounds::MAX_SCOPES as usize,
        &is_scope_text,
        false,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(
        value,
        "write_scopes",
        0,
        protocol::bounds::MAX_SCOPES as usize,
        &is_scope_text,
        false,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded(
        value,
        "ir_versions",
        0,
        protocol::bounds::MAX_IR_VERSIONS as usize,
        &is_contract,
        false,
    ) {
        return Err(Refusal::new("decode", "bounds"));
    }
    // `progress` is a `#[serde(default)] bool` in production: absent is
    // legal (defaults false); a present non-boolean is a typed refusal
    // (review r5 #9).
    match value.get("progress") {
        None => {}
        Some(progress) if progress.as_bool().is_some() => {}
        _ => return Err(Refusal::new("decode", "shape")),
    }
    if value.get("capabilities") == Some(&Value::Null) {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(capability_map) = value.get("capabilities") {
        if !capability_map.is_object() {
            return Err(Refusal::new("decode", "shape"));
        }
        let names = str_members(capability_map);
        if names.len() > protocol::bounds::MAX_DECLARED_CAPABILITIES as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for name in &names {
            if !is_capability_id(name) {
                return Err(Refusal::new("decode", "bounds"));
            }
            // Registry membership is part of admission (production
            // consults `capability::definition(id)`), not a
            // discovery-time nicety (issue #106 review r5 #1).
            if !crate::generated::capabilities::CAPABILITY_DEFINITIONS
                .iter()
                .any(|definition| definition.id == *name)
            {
                return Err(Refusal::new("decode", "bounds"));
            }
            let support = capability_map
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or("");
            if !protocol::enums::SUPPORTSTATE.contains(&support) {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    // `constraints` is a plain `Option` in production: absent and
    // explicit null are both legal, the members are `1..=MAX` when
    // present, and the all-default struct deserializes from `{}` or a
    // positional `[]` (issue #106 review r5 #1/#9).
    if let Some(constraints_member) = value.get("constraints") {
        if constraints_member != &Value::Null {
            let constraints = seq_as_members(constraints_member, &["max_entries", "max_writes"])?;
            let constraints = constraints.as_ref();
            if !is_closed_object(constraints, &["max_entries", "max_writes"]) {
                return Err(Refusal::new("decode", "shape"));
            }
            for member in ["max_entries", "max_writes"] {
                match constraints.get(member) {
                    None | Some(Value::Null) => {}
                    Some(number) => match number.as_u64() {
                        Some(number)
                            if (1..=protocol::bounds::MAX_CONSTRAINT_VALUE).contains(&number) => {}
                        _ => return Err(Refusal::new("decode", "bounds")),
                    },
                }
            }
        }
    }
    Ok(())
}

/// Structural admission of one response envelope.
pub fn admit_response_value(value: &Value) -> Result<(), Refusal> {
    let allowed = [
        "capabilities",
        "error",
        "evidence",
        "operation",
        "progress",
        "protocol",
        "protocol_version",
        "request_id",
        "result",
        "status",
        "writes",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new(
            "decode",
            if str_members(value)
                .iter()
                .any(|k| !allowed.contains(&k.as_str()))
            {
                "unknown-member"
            } else {
                "shape"
            },
        ));
    }
    for member in [
        "protocol",
        "protocol_version",
        "operation",
        "request_id",
        "status",
        "evidence",
    ] {
        if value.get(member).is_none() {
            return Err(Refusal::new("decode", "shape"));
        }
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
    }
    if value.get("protocol").and_then(Value::as_str) != Some(protocol::WIRE_TOKEN) {
        return Err(Refusal::new("decode", "token"));
    }
    if !value
        .get("protocol_version")
        .and_then(Value::as_str)
        .is_some_and(is_contract_version)
    {
        return Err(Refusal::new("decode", "version"));
    }
    if !value
        .get("operation")
        .and_then(Value::as_str)
        .is_some_and(|op| protocol::LATEST_OPERATIONS.contains(&op))
    {
        return Err(Refusal::new("decode", "operation"));
    }
    if !value
        .get("request_id")
        .and_then(Value::as_str)
        .is_some_and(is_request_id)
    {
        return Err(Refusal::new("decode", "request-id"));
    }
    let status = value.get("status").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::RESPONSESTATUS.contains(&status) {
        return Err(Refusal::new("decode", "shape"));
    }
    admit_evidence(value.get("evidence").unwrap_or(&Value::Null))?;
    let protocol_version = value
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if value.get("capabilities") == Some(&Value::Null) {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(capabilities) = value.get("capabilities") {
        admit_capabilities(capabilities, &protocol_version)?;
    }
    if explicit_null(value, "result") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(result_member) = value.get("result") {
        admit_result(result_member)?;
        // Lint evidence is version-gated exactly like production
        // validate_bounds (issue #106 review r5 #1): the member is legal
        // only on a 0.6.4 `lint` response.
        let carries_lint_evidence = match result_member {
            Value::Object(members) => members.iter().any(|(k, _)| k == "lint_evidence"),
            Value::Array(items) => items.len() >= 7,
            _ => false,
        };
        if carries_lint_evidence
            && (protocol_version != LINT_VERSION
                || value.get("operation").and_then(Value::as_str) != Some("lint"))
        {
            return Err(Refusal::new("decode", "shape"));
        }
    }
    if explicit_null(value, "writes") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(writes) = value.get("writes") {
        let Some(items) = writes.as_array() else {
            return Err(Refusal::new("decode", "bounds"));
        };
        if items.len() > protocol::bounds::MAX_PLAN_ENTRIES as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for entry in items {
            admit_write_entry(entry)?;
        }
    }
    if explicit_null(value, "progress") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(progress) = value.get("progress") {
        admit_progress(progress)?;
    }
    if explicit_null(value, "error") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(error) = value.get("error") {
        admit_error(error)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Semantic validation (mirrors the host's validate_request)
// ---------------------------------------------------------------------------

fn requires_ir(operation: &str) -> bool {
    matches!(operation, "validate" | "generate" | "verify")
}

fn declares_writes(operation: &str) -> bool {
    matches!(operation, "generate" | "clean" | "plan-clean")
}

const LINT_VERSION: &str = "0.6.4";

/// The production ai-lint identifier grammar (issue #106 review r3 #5):
/// core `ai_lint::input::token` gates the lint scope items, binding
/// nativeId/symbol, and the revision pin's known value — non-empty, at
/// most 256 bytes, ASCII alphanumeric or `_.:/#@\\-`, starting
/// alphanumeric. The SDK used to enforce a length bound only.
fn is_wire_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:/#@\\-".contains(&b))
        && value.as_bytes()[0].is_ascii_alphanumeric()
}

fn is_sorted(values: &[Value]) -> bool {
    values
        .windows(2)
        .all(|window| window[0].as_str() < window[1].as_str())
}

/// Cross-member request semantics (closed refusal tokens).
pub fn validate_request(value: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("validate", reason));
    let invalid = |reason: &str| refuse(reason);
    if value.get("protocol").and_then(Value::as_str) != Some(protocol::WIRE_TOKEN) {
        return invalid("version");
    }
    let operation = value.get("operation").and_then(Value::as_str).unwrap_or("");
    let version = value
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or("");
    let has_lint = value.get("lint_request").is_some();
    if (operation == "lint") != has_lint {
        return invalid("lint-request");
    }
    if let Some(lint) = value.get("lint_request") {
        if version != LINT_VERSION || value.get("target").is_none() {
            return invalid("lint-request");
        }
        if [
            "profile",
            "profile_digest",
            "profile_capabilities",
            "ir_path",
            "native_request",
            "dry_run",
            "plan_id",
        ]
        .iter()
        .any(|member| value.get(member).is_some())
        {
            return invalid("lint-request");
        }
        let scope = lint.get("scope").and_then(Value::as_array).unwrap_or(&[]);
        let files = lint.get("files").and_then(Value::as_array).unwrap_or(&[]);
        if !is_sorted(scope) || !is_sorted(files) {
            return invalid("lint-request");
        }
        if scope
            .iter()
            .any(|item| !is_wire_token(item.as_str().unwrap_or("")))
        {
            return invalid("lint-request");
        }
        if files
            .iter()
            .any(|file| !file.as_str().is_some_and(is_logical_path))
        {
            return invalid("lint-request");
        }
        let mut keys = std::collections::BTreeSet::new();
        for binding in lint
            .get("bindings")
            .and_then(Value::as_array)
            .unwrap_or(&[])
        {
            let path = binding.get("path").and_then(Value::as_str).unwrap_or("");
            let native_id = binding
                .get("nativeId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let symbol = binding.get("symbol").and_then(Value::as_str).unwrap_or("");
            let kind = binding.get("kind").and_then(Value::as_str).unwrap_or("");
            if !is_logical_path(path) || !files.iter().any(|file| file.as_str() == Some(path)) {
                return invalid("lint-binding");
            }
            if !binding
                .get("fingerprint")
                .and_then(Value::as_str)
                .is_some_and(is_sha256_digest)
            {
                return invalid("lint-binding");
            }
            if !is_wire_token(native_id) || !is_wire_token(symbol) {
                return invalid("lint-binding");
            }
            if !keys.insert((path.to_owned(), native_id.to_owned(), kind.to_owned())) {
                return invalid("lint-binding");
            }
        }
        let pins = lint.get("pins").unwrap_or(&Value::Null);
        let pin_known = |pin: &Value| -> Option<String> {
            if pin.get("state").and_then(Value::as_str) == Some("known") {
                pin.get("value").and_then(Value::as_str).map(str::to_owned)
            } else {
                None
            }
        };
        for pin in ["model", "ir"] {
            // Required known digests (issue #106 review r2 #2): the
            // production wire decoder refuses unknown and malformed
            // spellings of the model/ir pins; mirror it exactly.
            if !is_sha256_digest(
                &pin_known(pins.get(pin).unwrap_or(&Value::Null)).unwrap_or_default(),
            ) {
                return invalid("lint-pin");
            }
        }
        for pin in ["observed", "profile", "capabilities"] {
            if let Some(value) = pin_known(pins.get(pin).unwrap_or(&Value::Null)) {
                if !is_sha256_digest(&value) {
                    return invalid("lint-pin");
                }
            }
        }
        if let Some(value) = pin_known(pins.get("revision").unwrap_or(&Value::Null)) {
            if !is_wire_token(&value) {
                return invalid("lint-pin");
            }
        }
    }
    let has_native = value.get("native_request").is_some();
    if (operation == "plan-native") != has_native {
        return invalid("native-request");
    }
    let apply = operation == "clean"
        || (operation == "generate"
            && value.get("dry_run").and_then(Value::as_bool) == Some(false));
    let has_plan = value.get("plan_id").is_some();
    if apply != has_plan {
        return invalid("plan-id");
    }
    if let Some(plan) = value.get("plan_id") {
        if !plan.as_str().is_some_and(is_plan_id) {
            return invalid("plan-id");
        }
    }
    if (operation == "generate") != value.get("dry_run").is_some() {
        return invalid("dry-run");
    }
    if operation == "plan-native"
        && (value.get("dry_run").is_some() || value.get("plan_id").is_some())
    {
        return invalid("plan-id");
    }
    if requires_ir(operation) && value.get("ir_path").is_none() {
        return invalid("ir-path");
    }
    if (operation == "generate" || operation == "bind") && value.get("target").is_none() {
        return invalid("target");
    }
    if operation == "bind" && value.get("profile").is_none() {
        return invalid("profile");
    }
    if operation == "describe"
        && [
            "target",
            "profile",
            "profile_digest",
            "profile_capabilities",
            "native_request",
            "ir_path",
        ]
        .iter()
        .any(|member| value.get(member).is_some())
    {
        return invalid("member");
    }
    let profile_resolution =
        value.get("profile_digest").is_some() || value.get("profile_capabilities").is_some();
    if profile_resolution
        && (value.get("profile_digest").is_none()
            || value.get("profile_capabilities").is_none()
            || value.get("profile").is_none())
    {
        return invalid("profile-capabilities");
    }
    if profile_resolution && !protocol::SUPPORTED_PROTOCOL_VERSIONS.contains(&version) {
        return invalid("member");
    }
    Ok(())
}

/// Response identity echo against the exact negotiated session.
pub fn validate_response_identity(
    response: &Value,
    expected: (&str, &str, &str),
) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("validate", reason));
    if response.get("protocol").and_then(Value::as_str) != Some(protocol::WIRE_TOKEN) {
        return refuse("version");
    }
    if response.get("protocol_version").and_then(Value::as_str) != Some(expected.0) {
        return refuse("echo-version");
    }
    if response.get("operation").and_then(Value::as_str) != Some(expected.1) {
        return refuse("echo-operation");
    }
    if response.get("request_id").and_then(Value::as_str) != Some(expected.2) {
        return refuse("echo-request-id");
    }
    // The describe handshake binds the evidence identity to the declared
    // capabilities identity exactly like the production client
    // (`evidence.adapter != capabilities.adapter` → `Evidence`
    // invalidity, issue #106 review r5 #10) — an SDK consumer must
    // never accept a response a host session would refuse.
    if let (Some(evidence_adapter), Some(declared_adapter)) = (
        response.get("evidence").and_then(|e| e.get("adapter")),
        response.get("capabilities").and_then(|c| c.get("adapter")),
    ) {
        if canonical(evidence_adapter) != canonical(declared_adapter) {
            return refuse("identity");
        }
    }
    Ok(())
}

/// Operation payload pairing of one success response (the
/// validate-before-write obligation).
pub fn validate_response_payload(request: &Value, response: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("validate", reason));
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("");
    if response.get("capabilities").is_some() && operation != "describe" {
        return refuse("member");
    }
    if operation == "describe"
        && response.get("status").and_then(Value::as_str) == Some("ok")
        && response.get("capabilities").is_none()
    {
        return refuse("member");
    }
    if operation == "describe" {
        if let Some(caps) = response.get("capabilities") {
            let evidence = response.get("evidence").and_then(|e| e.get("adapter"));
            if evidence.map(canonical) != caps.get("adapter").map(canonical) {
                return refuse("identity");
            }
            let contains = |key: &str, expected: &str| {
                caps.get(key)
                    .and_then(Value::as_array)
                    .is_some_and(|items| items.iter().any(|v| v.as_str() == Some(expected)))
            };
            if !contains("operations", "describe")
                || !contains(
                    "protocol_versions",
                    request
                        .get("protocol_version")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                || response.get("progress").is_some()
            {
                return refuse("member");
            }
        }
    }
    let apply = operation == "clean"
        || (operation == "generate"
            && request.get("dry_run").and_then(Value::as_bool) == Some(false));
    if apply
        && response
            .get("evidence")
            .and_then(|e| e.get("plan_id"))
            .map(Value::as_str)
            != request.get("plan_id").map(Value::as_str)
    {
        return refuse("plan-id");
    }
    if response.get("status").and_then(Value::as_str) == Some("error") {
        if response.get("error").is_none() {
            return refuse("error-pairing");
        }
        if response.get("result").is_some()
            || response.get("writes").is_some()
            || response.get("progress").is_some()
        {
            return refuse("error-pairing");
        }
        return Ok(());
    }
    if response.get("error").is_some() {
        return refuse("error-pairing");
    }
    if let Some(result) = response.get("result") {
        if result.get("lint_evidence").is_some() && operation != "lint" {
            return refuse("member");
        }
        if result.get("entries").is_some() && operation != "scan" {
            return refuse("member");
        }
        if result.get("truncated").is_some() && operation != "scan" {
            return refuse("member");
        }
        if result.get("bindings").is_some() && operation != "bind" {
            return refuse("member");
        }
        if result.get("ok").is_some() && !matches!(operation, "validate" | "verify") {
            return refuse("member");
        }
        if result.get("findings").is_some() && !matches!(operation, "validate" | "verify") {
            return refuse("member");
        }
        if result.get("native_plan").is_some() && operation != "plan-native" {
            return refuse("member");
        }
        if operation == "plan-native" && result.get("entries").is_some() {
            return refuse("member");
        }
    }
    let result = response.get("result");
    let complete = match operation {
        "scan" => result.is_some_and(|r| r.get("entries").is_some()),
        "bind" => result.is_some_and(|r| r.get("bindings").is_some()),
        "validate" | "verify" => result.is_some_and(|r| r.get("ok").is_some()),
        "plan-native" => result.is_some_and(|r| r.get("native_plan").is_some()),
        "lint" => result.is_some_and(|r| r.get("lint_evidence").is_some()),
        "describe" => result.is_none(),
        _ => result.is_none(),
    };
    if !complete {
        return refuse("member");
    }
    if declares_writes(operation) {
        let Some(writes) = response.get("writes").and_then(Value::as_array) else {
            return refuse("writes-missing");
        };
        for entry in writes {
            let action = entry.get("action").and_then(Value::as_str).unwrap_or("");
            if action == "delete" {
                if entry.get("sha256").is_some() {
                    return refuse("write-digest");
                }
            } else if !entry
                .get("sha256")
                .and_then(Value::as_str)
                .is_some_and(is_sha256_digest)
            {
                return refuse("write-digest");
            }
        }
    } else if response.get("writes").is_some() {
        return refuse("member");
    }
    Ok(())
}
