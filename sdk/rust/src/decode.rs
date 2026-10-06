//! Strict wire decoding and semantic validation (issue #106): the
//! hand-maintained codec obligation over the generated vocabulary.
//! Mirrors the host's admission: unknown members, explicit nulls, wrong
//! tokens, unsupported exact versions, and out-of-bounds values are
//! refused before any callback runs, with closed safe reason tokens.

use crate::generated::protocol;
use crate::ids::{is_plan_id, is_request_id, is_sha256_digest};
use crate::json::{read_single_object, ParseReason, Value};
use crate::paths::{is_capability_id, is_contract_version, is_logical_path, is_token};

/// A decode/validate refusal: one closed safe reason token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub stage: &'static str,
    pub reason: String,
}

impl Refusal {
    fn new(stage: &'static str, reason: &str) -> Self {
        Self { stage, reason: reason.to_owned() }
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

/// Decode a response document (structural admission only).
pub fn admit_response(text: &str) -> Result<Value, Refusal> {
    let value = read_single_object(text, protocol::bounds::DEFAULT_MAX_OUTPUT_BYTES as usize)
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
    value.member_names().into_iter().map(str::to_owned).collect()
}

fn explicit_null(object: &Value, key: &str) -> bool {
    matches!(object.get(key), Some(Value::Null))
}

fn is_closed_object(value: &Value, allowed: &[&str]) -> bool {
    if !value.is_object() {
        return false;
    }
    str_members(value).iter().all(|key| allowed.contains(&key.as_str()))
}

fn bounded_text(value: &Value, max: usize) -> bool {
    value.as_str().is_some_and(|text| text.chars().count() <= max)
}

/// Structural admission of one request envelope (closed shape + bounds).
pub fn admit_request_value(value: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("decode", reason));
    let allowed = [
        "dry_run", "ir_path", "limits", "lint_request", "native_request", "operation", "plan_id",
        "profile", "profile_capabilities", "profile_digest", "project_root", "protocol",
        "protocol_version", "request_id", "target",
    ];
    if !is_closed_object(value, &allowed) {
        return refuse(if str_members(value).iter().any(|k| !allowed.contains(&k.as_str())) {
            "unknown-member"
        } else {
            "shape"
        });
    }
    for member in ["protocol", "protocol_version", "operation", "request_id", "project_root"] {
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
    let version = value.get("protocol_version").and_then(Value::as_str).unwrap_or("");
    if !protocol::SUPPORTED_PROTOCOL_VERSIONS.contains(&version) {
        return refuse("version");
    }
    let operation = value.get("operation").and_then(Value::as_str).unwrap_or("");
    if !protocol::LATEST_OPERATIONS.contains(&operation) {
        return refuse("operation");
    }
    if value.get("request_id").and_then(Value::as_str).is_some_and(|value| !is_request_id(value)) {
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
    for (member, (min, max)) in [
        ("timeout_ms", (protocol::bounds::LIMITS_TIMEOUT_MIN, protocol::bounds::LIMITS_TIMEOUT_MAX)),
        ("max_output_bytes", (protocol::bounds::LIMITS_OUTPUT_MIN, protocol::bounds::LIMITS_OUTPUT_MAX)),
    ] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(number) = value.get(member) {
            match number.as_u64() {
                Some(number) if (min..=max).contains(&number) => {}
                _ => return Err(Refusal::new("decode", "bounds")),
            }
        }
    }
    Ok(())
}

fn admit_profile_capabilities(value: &Value) -> Result<(), Refusal> {
    let Some(items) = value.as_array() else {
        return Err(Refusal::new("decode", "bounds"));
    };
    if items.len() > protocol::bounds::MAX_DECLARED_CAPABILITIES as usize {
        return Err(Refusal::new("decode", "bounds"));
    }
    let mut previous: Option<String> = None;
    for capability in items {
        if !is_closed_object(capability, &["id", "support"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !capability.get("id").and_then(Value::as_str).is_some_and(is_capability_id) {
            return Err(Refusal::new("decode", "bounds"));
        }
        let support = capability.get("support").and_then(Value::as_str).unwrap_or("");
        if !protocol::enums::SUPPORTSTATE.contains(&support) {
            return Err(Refusal::new("decode", "shape"));
        }
        let id = capability.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
        if previous.as_deref().is_some_and(|previous| id.as_str() <= previous) {
            return Err(Refusal::new("decode", "bounds"));
        }
        previous = Some(id);
    }
    Ok(())
}

fn admit_content_ref(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["adapter", "digest", "revision"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value.get("digest").and_then(Value::as_str).is_some_and(is_sha256_digest) {
        return Err(Refusal::new("decode", "bounds"));
    }
    for member in ["adapter", "revision"] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(member_value) = value.get(member) {
            if !bounded_text(member_value, 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
    }
    Ok(())
}

fn admit_native_request(value: &Value) -> Result<(), Refusal> {
    let allowed = [
        "capability_snapshot_digest", "changes", "execution_policy_ref", "input_manifest_digest",
        "observed_ref", "scan_ref", "tool_catalog_digest",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    for member in ["input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if !value.get(member).and_then(Value::as_str).is_some_and(is_sha256_digest) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    let changes = value.get("changes").ok_or_else(|| Refusal::new("decode", "shape"))?;
    if !is_closed_object(changes, &["files", "symbols"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    let files = changes.get("files").and_then(Value::as_array).unwrap_or(&[]);
    let symbols = changes.get("symbols").and_then(Value::as_array).unwrap_or(&[]);
    if files.len() > 1024 || symbols.len() > 1024 {
        return Err(Refusal::new("decode", "bounds"));
    }
    let mut previous: Option<String> = None;
    for change in files {
        if !is_closed_object(change, &["after_digest", "before_digest", "change", "path"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !change.get("path").and_then(Value::as_str).is_some_and(is_logical_path) {
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
        let path = change.get("path").and_then(Value::as_str).unwrap_or("").to_owned();
        if previous.as_deref().is_some_and(|previous| path.as_str() <= previous) {
            return Err(Refusal::new("decode", "bounds"));
        }
        previous = Some(path);
    }
    for symbol in symbols {
        if !bounded_text(symbol, protocol::bounds::ERROR_DETAIL_MAX as usize) {
            return Err(Refusal::new("decode", "bounds"));
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
    for member in ["scope", "files", "bindings"] {
        if explicit_null(value, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
    }
    let scope = value.get("scope").and_then(Value::as_array).unwrap_or(&[]);
    let files = value.get("files").and_then(Value::as_array).unwrap_or(&[]);
    let bindings = value.get("bindings").and_then(Value::as_array).unwrap_or(&[]);
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
        let allowed = ["fingerprint", "kind", "native_id", "path", "symbol"];
        if !is_closed_object(binding, &allowed) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !binding.get("path").and_then(Value::as_str).is_some_and(is_logical_path) {
            return Err(Refusal::new("decode", "bounds"));
        }
        for member in ["native_id", "symbol"] {
            if !bounded_text(binding.get(member).unwrap_or(&Value::Null), 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
        let kind = binding.get("kind").and_then(Value::as_str).unwrap_or("");
        if !["command", "entity"].contains(&kind) {
            return Err(Refusal::new("decode", "shape"));
        }
        if !binding.get("fingerprint").and_then(Value::as_str).is_some_and(is_sha256_digest) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    let pins = value.get("pins").unwrap_or(&Value::Null);
    let allowed = ["capabilities", "ir", "model", "observed", "profile", "revision"];
    if !is_closed_object(pins, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    for member in ["model", "ir"] {
        if !pins.get(member).and_then(Value::as_str).is_some_and(is_sha256_digest) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    for member in ["observed", "profile", "capabilities", "revision"] {
        if explicit_null(pins, member) {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(pin) = pins.get(member) {
            if !bounded_text(pin, 128) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Response admission
// ---------------------------------------------------------------------------

fn admit_identity(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["digest", "id", "version"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value.get("id").and_then(Value::as_str).is_some_and(is_token) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !value.get("version").and_then(Value::as_str).is_some_and(is_contract_version) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !value.get("digest").and_then(Value::as_str).is_some_and(is_sha256_digest) {
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
    if !value.get("path").and_then(Value::as_str).is_some_and(is_logical_path) {
        return Err(Refusal::new("decode", "bounds"));
    }
    let action = value.get("action").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::WRITEACTION.contains(&action) {
        return Err(Refusal::new("decode", "shape"));
    }
    if explicit_null(value, "sha256") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(digest) = value.get("sha256") {
        if !digest.as_str().is_some_and(is_sha256_digest) {
            return Err(Refusal::new("decode", "bounds"));
        }
    }
    Ok(())
}

fn admit_scan_entry(value: &Value) -> Result<(), Refusal> {
    if !is_closed_object(value, &["detail", "evidence", "kind", "path"]) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value.get("path").and_then(Value::as_str).is_some_and(is_logical_path) {
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
    if let Some(evidence) = value.get("evidence") {
        if !is_closed_object(evidence, &["references", "signature"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        if explicit_null(evidence, "signature") {
            return Err(Refusal::new("decode", "explicit-null"));
        }
        if let Some(signature) = evidence.get("signature") {
            if !signature.as_str().is_some_and(is_sha256_digest) {
                return Err(Refusal::new("decode", "bounds"));
            }
        }
        let references = evidence.get("references").and_then(Value::as_array).unwrap_or(&[]);
        if references.len() > protocol::bounds::REFERENCES_MAX as usize {
            return Err(Refusal::new("decode", "bounds"));
        }
        for reference in references {
            if !is_closed_object(reference, &["confidence", "role", "target"]) {
                return Err(Refusal::new("decode", "shape"));
            }
            if !bounded_text(reference.get("target").unwrap_or(&Value::Null), 192) {
                return Err(Refusal::new("decode", "bounds"));
            }
            let role = reference.get("role").and_then(Value::as_str).unwrap_or("");
            if !protocol::enums::SCANENTRYREFERENCEROLE.contains(&role) {
                return Err(Refusal::new("decode", "shape"));
            }
            let confidence = reference.get("confidence").and_then(Value::as_str).unwrap_or("");
            if !protocol::enums::SCANENTRYCONFIDENCE.contains(&confidence) {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    Ok(())
}

fn admit_native_plan(value: &Value) -> Result<(), Refusal> {
    let allowed = ["commands", "completeness", "digest", "entries", "kind", "packages", "run_eligibility", "units"];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::NATIVEPLANKIND.contains(&kind) {
        return Err(Refusal::new("decode", "shape"));
    }
    if !value.get("digest").and_then(Value::as_str).is_some_and(is_sha256_digest) {
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

fn admit_result(value: &Value) -> Result<(), Refusal> {
    let allowed = ["bindings", "entries", "findings", "lint_evidence", "native_plan", "ok", "truncated"];
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
            if !finding.get("path").and_then(Value::as_str).is_some_and(is_logical_path) {
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
                if !binding.get(member).and_then(Value::as_str).is_some_and(is_token) {
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
        // The deep LintEvidence subtree is the host's admission duty; the
        // SDK carries it as a closed object and never builds one.
        if !lint.is_object() {
            return Err(Refusal::new("decode", "shape"));
        }
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

fn admit_capabilities(value: &Value) -> Result<(), Refusal> {
    let allowed = [
        "adapter", "capabilities", "constraints", "ir_versions", "operations", "profiles",
        "progress", "protocol_versions", "read_scopes", "targets", "transports", "write_scopes",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", "shape"));
    }
    admit_identity(value.get("adapter").unwrap_or(&Value::Null))?;
    let unique_bounded = |member: &str, max: usize, check: fn(&Value) -> bool, required: bool| -> bool {
        match value.get(member) {
            None => !required,
            Some(Value::Null) => false,
            Some(items) => {
                let Some(items) = items.as_array() else {
                    return false;
                };
                items.len() <= max
                    && items.iter().all(check)
                    && {
                    let mut seen: Vec<String> = items.iter().map(crate::json::canonical).collect();
                    seen.sort();
                    seen.dedup();
                    seen.len() == items.len()
                }
            }
        }
    };
    let is_contract = |value: &Value| value.as_str().is_some_and(is_contract_version);
    let is_token_value = |value: &Value| value.as_str().is_some_and(is_token);
    let is_operation = |value: &Value| value.as_str().is_some_and(|op| protocol::LATEST_OPERATIONS.contains(&op));
    let is_transport = |value: &Value| value.as_str().is_some_and(|t| protocol::enums::TRANSPORT.contains(&t));
    let is_scope_text = |value: &Value| value.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 512);
    if !unique_bounded("protocol_versions", protocol::bounds::PROTOCOL_VERSIONS_MAX as usize, is_contract, true) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("operations", protocol::bounds::OPERATIONS_MAX_LINT as usize, is_operation, true) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("transports", protocol::bounds::TRANSPORTS_MAX as usize, is_transport, true) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("targets", protocol::bounds::TARGETS_MAX as usize, is_token_value, false) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("profiles", protocol::bounds::PROFILES_MAX as usize, is_token_value, false) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("read_scopes", protocol::bounds::MAX_SCOPES as usize, is_scope_text, false) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("write_scopes", protocol::bounds::MAX_SCOPES as usize, is_scope_text, false) {
        return Err(Refusal::new("decode", "bounds"));
    }
    if !unique_bounded("ir_versions", protocol::bounds::MAX_IR_VERSIONS as usize, is_contract, false) {
        return Err(Refusal::new("decode", "bounds"));
    }
    match value.get("progress") {
        Some(progress) if progress.as_bool().is_some() => {}
        _ => return Err(Refusal::new("decode", "shape")),
    }
    if explicit_null(value, "capabilities") {
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
            let support = capability_map.get(name).and_then(Value::as_str).unwrap_or("");
            if !protocol::enums::SUPPORTSTATE.contains(&support) {
                return Err(Refusal::new("decode", "shape"));
            }
        }
    }
    if explicit_null(value, "constraints") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(constraints) = value.get("constraints") {
        if !is_closed_object(constraints, &["max_entries", "max_writes"]) {
            return Err(Refusal::new("decode", "shape"));
        }
        for member in ["max_entries", "max_writes"] {
            if explicit_null(constraints, member) {
                return Err(Refusal::new("decode", "explicit-null"));
            }
            if let Some(number) = constraints.get(member) {
                match number.as_u64() {
                    Some(number) if number <= protocol::bounds::MAX_CONSTRAINT_VALUE => {}
                    _ => return Err(Refusal::new("decode", "bounds")),
                }
            }
        }
    }
    Ok(())
}

/// Structural admission of one response envelope.
pub fn admit_response_value(value: &Value) -> Result<(), Refusal> {
    let allowed = [
        "capabilities", "error", "evidence", "operation", "progress", "protocol",
        "protocol_version", "request_id", "result", "status", "writes",
    ];
    if !is_closed_object(value, &allowed) {
        return Err(Refusal::new("decode", if str_members(value).iter().any(|k| !allowed.contains(&k.as_str())) {
            "unknown-member"
        } else {
            "shape"
        }));
    }
    for member in ["protocol", "protocol_version", "operation", "request_id", "status", "evidence"] {
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
    if !value.get("protocol_version").and_then(Value::as_str).is_some_and(is_contract_version) {
        return Err(Refusal::new("decode", "version"));
    }
    if !value.get("operation").and_then(Value::as_str).is_some_and(|op| protocol::LATEST_OPERATIONS.contains(&op)) {
        return Err(Refusal::new("decode", "operation"));
    }
    if !value.get("request_id").and_then(Value::as_str).is_some_and(is_request_id) {
        return Err(Refusal::new("decode", "request-id"));
    }
    let status = value.get("status").and_then(Value::as_str).unwrap_or("");
    if !protocol::enums::RESPONSESTATUS.contains(&status) {
        return Err(Refusal::new("decode", "shape"));
    }
    admit_evidence(value.get("evidence").unwrap_or(&Value::Null))?;
    if explicit_null(value, "capabilities") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(capabilities) = value.get("capabilities") {
        admit_capabilities(capabilities)?;
    }
    if explicit_null(value, "result") {
        return Err(Refusal::new("decode", "explicit-null"));
    }
    if let Some(result) = value.get("result") {
        admit_result(result)?;
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

fn is_sorted(values: &[Value]) -> bool {
    values.windows(2).all(|window| window[0].as_str() < window[1].as_str())
}

/// Cross-member request semantics (closed refusal tokens).
pub fn validate_request(value: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("validate", reason));
    let invalid = |reason: &str| refuse(reason);
    if value.get("protocol").and_then(Value::as_str) != Some(protocol::WIRE_TOKEN) {
        return invalid("version");
    }
    let operation = value.get("operation").and_then(Value::as_str).unwrap_or("");
    let version = value.get("protocol_version").and_then(Value::as_str).unwrap_or("");
    let has_lint = value.get("lint_request").is_some();
    if (operation == "lint") != has_lint {
        return invalid("lint-request");
    }
    if let Some(lint) = value.get("lint_request") {
        if version != LINT_VERSION || value.get("target").is_none() {
            return invalid("lint-request");
        }
        if ["profile", "profile_digest", "profile_capabilities", "ir_path", "native_request", "dry_run", "plan_id"]
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
        if files.iter().any(|file| !file.as_str().is_some_and(is_logical_path)) {
            return invalid("lint-request");
        }
        let mut keys = std::collections::BTreeSet::new();
        for binding in lint.get("bindings").and_then(Value::as_array).unwrap_or(&[]) {
            let path = binding.get("path").and_then(Value::as_str).unwrap_or("");
            let native_id = binding.get("native_id").and_then(Value::as_str).unwrap_or("");
            let kind = binding.get("kind").and_then(Value::as_str).unwrap_or("");
            if !is_logical_path(path) || !files.iter().any(|file| file.as_str() == Some(path)) {
                return invalid("lint-binding");
            }
            if !binding.get("fingerprint").and_then(Value::as_str).is_some_and(is_sha256_digest) {
                return invalid("lint-binding");
            }
            if !binding.get("symbol").and_then(Value::as_str).is_some_and(|s| !s.is_empty() && s.len() <= 128) {
                return invalid("lint-binding");
            }
            if !keys.insert((path.to_owned(), native_id.to_owned(), kind.to_owned())) {
                return invalid("lint-binding");
            }
        }
        let pins = lint.get("pins").unwrap_or(&Value::Null);
        for pin in ["observed", "profile", "capabilities"] {
            if let Some(pin) = pins.get(pin) {
                if !pin.as_str().is_some_and(is_sha256_digest) {
                    return invalid("lint-pin");
                }
            }
        }
        if let Some(revision) = pins.get("revision") {
            if !revision.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128) {
                return invalid("lint-pin");
            }
        }
    }
    let has_native = value.get("native_request").is_some();
    if (operation == "plan-native") != has_native {
        return invalid("native-request");
    }
    let apply = operation == "clean" || (operation == "generate" && value.get("dry_run").and_then(Value::as_bool) == Some(false));
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
    if operation == "plan-native" && (value.get("dry_run").is_some() || value.get("plan_id").is_some()) {
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
        && ["target", "profile", "profile_digest", "profile_capabilities", "native_request", "ir_path"]
            .iter()
            .any(|member| value.get(member).is_some())
    {
        return invalid("member");
    }
    let profile_resolution = value.get("profile_digest").is_some() || value.get("profile_capabilities").is_some();
    if profile_resolution
        && (value.get("profile_digest").is_none() || value.get("profile_capabilities").is_none() || value.get("profile").is_none())
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
    Ok(())
}

/// Operation payload pairing of one success response (the
/// validate-before-write obligation).
pub fn validate_response_payload(request: &Value, response: &Value) -> Result<(), Refusal> {
    let refuse = |reason: &str| Err(Refusal::new("validate", reason));
    let operation = request.get("operation").and_then(Value::as_str).unwrap_or("");
    if response.get("capabilities").is_some() && operation != "describe" {
        return refuse("member");
    }
    if operation == "describe" && response.get("status").and_then(Value::as_str) == Some("ok") && response.get("capabilities").is_none() {
        return refuse("member");
    }
    let apply = operation == "clean" || (operation == "generate" && request.get("dry_run").and_then(Value::as_bool) == Some(false));
    if apply && response.get("evidence").and_then(|e| e.get("plan_id")).map(Value::as_str) != request.get("plan_id").map(Value::as_str) {
        return refuse("plan-id");
    }
    if response.get("status").and_then(Value::as_str) == Some("error") {
        if response.get("error").is_none() {
            return refuse("error-pairing");
        }
        if response.get("result").is_some() || response.get("writes").is_some() || response.get("progress").is_some() {
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
            } else if entry.get("sha256").is_none() {
                return refuse("write-digest");
            }
        }
    } else if response.get("writes").is_some() {
        return refuse("member");
    }
    Ok(())
}

