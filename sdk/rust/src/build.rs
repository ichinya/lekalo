//! Response builders and the validate-before-write boundary (issue #106).
//! Every builder output is validated against the full closed contract
//! before emission; an invalid success is refused as a build error, and
//! the caller falls back to a valid closed error envelope.

use crate::canonical::sort_write_plan;
use crate::decode::{validate_response_payload, Refusal};
use crate::generated::protocol;
use crate::ids::{is_plan_id, is_sha256_digest};
use crate::json::{canonical, quote, Value};
use crate::paths::{is_capability_id, is_contract_version, is_token};
use crate::validate::VALIDATE_STAGE;

/// A builder refusal: the closed reason token of the failed admission.
#[derive(Clone, Debug)]
pub struct BuildError {
    pub reason: String,
}

impl From<Refusal> for BuildError {
    fn from(refusal: Refusal) -> Self {
        Self {
            reason: refusal.reason,
        }
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sdk-build-error: {}", self.reason)
    }
}

/// Build and validate one adapter identity block.
pub fn build_adapter_identity(id: &str, version: &str, digest: &str) -> Result<Value, BuildError> {
    if !is_token(id) {
        return Err(BuildError {
            reason: "identity".to_owned(),
        });
    }
    if !is_contract_version(version) {
        return Err(BuildError {
            reason: "identity".to_owned(),
        });
    }
    if !is_sha256_digest(digest) {
        return Err(BuildError {
            reason: "identity".to_owned(),
        });
    }
    Ok(Value::Object(vec![
        ("digest".to_owned(), Value::Str(digest.to_owned())),
        ("id".to_owned(), Value::Str(id.to_owned())),
        ("version".to_owned(), Value::Str(version.to_owned())),
    ]))
}

fn bounded_unique(values: Vec<Value>, max: usize, _label: &str) -> Result<Vec<Value>, BuildError> {
    if values.len() > max {
        return Err(BuildError {
            reason: "bounds".to_owned(),
        });
    }
    let mut unique = values.clone();
    unique.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    unique.dedup_by(|a, b| a == b);
    if unique.len() != values.len() {
        return Err(BuildError {
            reason: "bounds".to_owned(),
        });
    }
    Ok(unique)
}

fn string_values(items: &[&str]) -> Vec<Value> {
    items
        .iter()
        .map(|value| Value::Str((*value).to_owned()))
        .collect()
}

/// Build one `describe` capability map. Declared scopes and capability
/// states are honest claims the host polices — describing more than the
/// adapter implements is a refusal, never a capability gain.
#[allow(clippy::too_many_arguments)]
pub fn build_capabilities(
    adapter: &Value,
    protocol_versions: &[&str],
    operations: &[&str],
    transports: &[&str],
    targets: &[&str],
    profiles: &[&str],
    read_scopes: &[&str],
    write_scopes: &[&str],
    ir_versions: &[&str],
    progress: bool,
    capabilities: Option<Vec<(&str, &str)>>,
    constraints: Option<(u64, u64)>,
) -> Result<Value, BuildError> {
    let mut members: Vec<(String, Value)> = vec![
        ("adapter".to_owned(), adapter.clone()),
        (
            "ir_versions".to_owned(),
            Value::Array(bounded_unique(
                string_values(ir_versions),
                protocol::bounds::MAX_IR_VERSIONS as usize,
                "ir_versions",
            )?),
        ),
        (
            "operations".to_owned(),
            Value::Array(bounded_unique(
                string_values(operations),
                protocol::bounds::OPERATIONS_MAX_LINT as usize,
                "operations",
            )?),
        ),
        (
            "profiles".to_owned(),
            Value::Array(bounded_unique(
                string_values(profiles),
                protocol::bounds::PROFILES_MAX as usize,
                "profiles",
            )?),
        ),
        ("progress".to_owned(), Value::Bool(progress)),
        (
            "protocol_versions".to_owned(),
            Value::Array(bounded_unique(
                string_values(protocol_versions),
                protocol::bounds::PROTOCOL_VERSIONS_MAX as usize,
                "protocol_versions",
            )?),
        ),
        (
            "read_scopes".to_owned(),
            Value::Array(bounded_unique(
                string_values(read_scopes),
                protocol::bounds::MAX_SCOPES as usize,
                "read_scopes",
            )?),
        ),
        (
            "targets".to_owned(),
            Value::Array(bounded_unique(
                string_values(targets),
                protocol::bounds::TARGETS_MAX as usize,
                "targets",
            )?),
        ),
        (
            "transports".to_owned(),
            Value::Array(bounded_unique(
                string_values(transports),
                protocol::bounds::TRANSPORTS_MAX as usize,
                "transports",
            )?),
        ),
        (
            "write_scopes".to_owned(),
            Value::Array(bounded_unique(
                string_values(write_scopes),
                protocol::bounds::MAX_SCOPES as usize,
                "write_scopes",
            )?),
        ),
    ];
    if let Some(capabilities) = capabilities {
        let mut sorted = capabilities;
        sorted.sort();
        let mut map = Vec::new();
        for (id, state) in sorted {
            if !is_capability_id(id) {
                return Err(BuildError {
                    reason: "capabilities".to_owned(),
                });
            }
            if !protocol::enums::SUPPORTSTATE.contains(&state) {
                return Err(BuildError {
                    reason: "capabilities".to_owned(),
                });
            }
            map.push((id.to_owned(), Value::Str(state.to_owned())));
        }
        members.push(("capabilities".to_owned(), Value::Object(map)));
    }
    if let Some((max_entries, max_writes)) = constraints {
        members.push((
            "constraints".to_owned(),
            Value::Object(vec![
                ("max_entries".to_owned(), Value::Int(max_entries as i64)),
                ("max_writes".to_owned(), Value::Int(max_writes as i64)),
            ]),
        ));
    }
    Ok(Value::Object(members))
}

fn envelope_base(request: &Value, status: &str) -> Vec<(String, Value)> {
    let evidence_adapter = Value::Object(vec![]);
    vec![
        ("evidence".to_owned(), evidence_adapter),
        (
            "operation".to_owned(),
            request.get("operation").cloned().unwrap_or(Value::Null),
        ),
        (
            "protocol".to_owned(),
            Value::Str(protocol::WIRE_TOKEN.to_owned()),
        ),
        (
            "protocol_version".to_owned(),
            request
                .get("protocol_version")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "request_id".to_owned(),
            request.get("request_id").cloned().unwrap_or(Value::Null),
        ),
        ("status".to_owned(), Value::Str(status.to_owned())),
    ]
}

fn finish_envelope(mut envelope: Value, request: &Value) -> Result<Value, BuildError> {
    if let Some(writes) = envelope.get("writes").cloned() {
        let sorted = sort_write_plan(&writes).ok_or_else(|| BuildError {
            reason: "writes".to_owned(),
        })?;
        let value = Value::Array(
            sorted
                .into_iter()
                .map(|(path, action, digest)| {
                    let mut members = vec![
                        ("action".to_owned(), Value::Str(action)),
                        ("path".to_owned(), Value::Str(path)),
                    ];
                    if let Some(digest) = digest {
                        members.push(("sha256".to_owned(), Value::Str(digest)));
                    }
                    Value::Object(members)
                })
                .collect(),
        );
        if let Value::Object(members) = &mut envelope {
            members.retain(|(key, _)| key != "writes");
            members.push(("writes".to_owned(), value));
        }
    }
    let value = admit_envelope(&envelope).map_err(BuildError::from)?;
    validate_response_payload(request, &value).map_err(BuildError::from)?;
    if let Some(plan) = envelope
        .get("evidence")
        .and_then(|evidence| evidence.get("plan_id"))
    {
        if !plan.as_str().is_some_and(is_plan_id) {
            return Err(BuildError {
                reason: "plan-id".to_owned(),
            });
        }
    }
    Ok(envelope)
}

/// Re-admit an envelope through the response decoder (validate-before-write).
fn admit_envelope(envelope: &Value) -> Result<Value, Refusal> {
    let text = canonical(envelope);
    crate::decode::admit_response(&text)
}

/// Build one success envelope for the request and validate it fully.
#[allow(clippy::too_many_arguments)]
pub fn build_success(
    request: &Value,
    adapter: &Value,
    capabilities: Option<&Value>,
    payload: Option<&Value>,
    writes: Option<&Value>,
    progress: Option<&Value>,
    plan_id_value: Option<&str>,
) -> Result<Value, BuildError> {
    let mut members = envelope_base(request, "ok");
    let mut evidence = vec![("adapter".to_owned(), adapter.clone())];
    if let Some(plan) = plan_id_value {
        evidence.push(("plan_id".to_owned(), Value::Str(plan.to_owned())));
    }
    for (key, value) in &mut members {
        if key == "evidence" {
            *value = Value::Object(evidence.clone());
        }
    }
    // Capabilities ride describe responses only (parity with the host
    // payload rules); other operations must not carry the member.
    if let Some(capabilities) = capabilities {
        if request.get("operation").and_then(Value::as_str) == Some("describe") {
            members.push(("capabilities".to_owned(), capabilities.clone()));
        }
    }
    if let Some(payload) = payload {
        members.push(("result".to_owned(), payload.clone()));
    }
    if let Some(writes) = writes {
        members.push(("writes".to_owned(), writes.clone()));
    }
    if let Some(progress) = progress {
        members.push(("progress".to_owned(), progress.clone()));
    }
    finish_envelope(Value::Object(members), request)
}

/// Build one closed in-envelope error envelope for the request.
pub fn build_error(
    request: &Value,
    adapter: &Value,
    class: &str,
    code: &str,
    message: &str,
    detail: Option<Vec<String>>,
) -> Result<Value, BuildError> {
    if !protocol::enums::ERRORCLASS.contains(&class) {
        return Err(BuildError {
            reason: "error-class".to_owned(),
        });
    }
    if code.is_empty() || code.chars().count() > protocol::bounds::ERROR_CODE_MAX as usize {
        return Err(BuildError {
            reason: "error-code".to_owned(),
        });
    }
    if message.is_empty() || message.chars().count() > protocol::bounds::ERROR_MESSAGE_MAX as usize
    {
        return Err(BuildError {
            reason: "error-message".to_owned(),
        });
    }
    let mut error_members = vec![
        ("class".to_owned(), Value::Str(class.to_owned())),
        ("code".to_owned(), Value::Str(code.to_owned())),
        ("message".to_owned(), Value::Str(message.to_owned())),
    ];
    if let Some(detail) = detail {
        if detail.len() > protocol::bounds::ERROR_DETAILS_MAX as usize {
            return Err(BuildError {
                reason: "error-detail".to_owned(),
            });
        }
        for item in &detail {
            if item.chars().count() > protocol::bounds::ERROR_DETAIL_MAX as usize {
                return Err(BuildError {
                    reason: "error-detail".to_owned(),
                });
            }
        }
        error_members.push((
            "detail".to_owned(),
            Value::Array(detail.into_iter().map(Value::Str).collect()),
        ));
    }
    let mut members = envelope_base(request, "error");
    for (key, value) in &mut members {
        if key == "evidence" {
            *value = Value::Object(vec![("adapter".to_owned(), adapter.clone())]);
        }
    }
    members.push(("error".to_owned(), Value::Object(error_members)));
    finish_envelope(Value::Object(members), request)
}

/// Build one wire `Finding` (validate/verify result row).
pub fn build_finding(code: &str, path: &str, detail: Option<&str>) -> Result<Value, BuildError> {
    if code.is_empty() || code.chars().count() > protocol::bounds::ERROR_CODE_MAX as usize {
        return Err(BuildError {
            reason: "finding-code".to_owned(),
        });
    }
    if let Some(detail) = detail {
        if detail.chars().count() > protocol::bounds::ERROR_DETAIL_MAX as usize {
            return Err(BuildError {
                reason: "finding-detail".to_owned(),
            });
        }
    }
    let mut members = vec![
        ("code".to_owned(), Value::Str(code.to_owned())),
        ("path".to_owned(), Value::Str(path.to_owned())),
    ];
    if let Some(detail) = detail {
        members.push(("detail".to_owned(), Value::Str(detail.to_owned())));
    }
    Ok(Value::Object(members))
}

/// Build one bounded, registry-checked diagnostic record.
pub fn build_diagnostic(
    codes: &[(&str, &str)],
    code: &str,
    message: &str,
    rule: &str,
) -> Result<Value, BuildError> {
    if !codes
        .iter()
        .any(|(_, registry_code)| *registry_code == code)
    {
        return Err(BuildError {
            reason: "diagnostic-code".to_owned(),
        });
    }
    if message.is_empty() || message.chars().count() > protocol::bounds::ERROR_MESSAGE_MAX as usize
    {
        return Err(BuildError {
            reason: "diagnostic-message".to_owned(),
        });
    }
    if rule.chars().count() > protocol::bounds::ERROR_DETAIL_MAX as usize {
        return Err(BuildError {
            reason: "diagnostic-rule".to_owned(),
        });
    }
    Ok(Value::Object(vec![
        ("code".to_owned(), Value::Str(code.to_owned())),
        ("message".to_owned(), Value::Str(message.to_owned())),
        ("rule".to_owned(), Value::Str(rule.to_owned())),
    ]))
}

/// The canonical bytes of one envelope: the exact stdout form.
pub fn response_bytes(envelope: &Value) -> String {
    canonical(envelope)
}

/// Quote helper re-exported for the bundle glue (unused placeholder guard).
#[allow(unused)]
fn _quote_guard(text: &str) -> String {
    quote(text)
}

/// The validate stage marker for wrapper diagnostics (parity constant).
pub const VALIDATE_STAGE_MARKER: &str = VALIDATE_STAGE;
