//! Canonical serialization of the storage-engine attachment (issue
//! #69).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys,
//! no BOM, and no trailing LF; absent optional members are dropped
//! entirely. Set-like collections (scopes, extensions) normalize to
//! unsigned UTF-8-byte order while policy members keep their closed
//! spellings. The output is path-independent and byte-identical for
//! value-equal attachments.

use crate::diagnostics::DiagnosticSet;

use super::{StorageEngineAttachment, TestLifecycle};

/// The canonical attachment payload bytes, or the typed over-bound
/// refusal.
pub fn attachment_bytes(attachment: &StorageEngineAttachment) -> Result<String, DiagnosticSet> {
    let bytes = attachment_payload(attachment);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The full canonical attachment payload.
fn attachment_payload(attachment: &StorageEngineAttachment) -> String {
    object(vec![
        (
            "schemaVersion",
            Some(string(super::version::SCHEMA_VERSION)),
        ),
        ("identity", Some(string(super::version::IDENTITY))),
        (
            "attachmentRevision",
            Some(string(attachment.attachment_revision().as_str())),
        ),
        ("projectId", Some(string(attachment.project_id().as_str()))),
        ("modelRef", Some(model_ref_payload(attachment.model_ref()))),
        (
            "irRef",
            Some(digest_payload(
                "dev.lekalo.ir@0.2.16",
                attachment.ir_digest().as_str(),
            )),
        ),
        (
            "projectionRef",
            Some(string(attachment.projection_ref().as_str())),
        ),
        ("engine", Some(string(attachment.engine().key()))),
        (
            "engineVersion",
            Some(string(attachment.engine_version().as_str())),
        ),
        ("policies", Some(policies_payload(attachment.policies()))),
        ("tenancy", attachment.tenancy().map(tenancy_payload)),
        (
            "concurrency",
            attachment.concurrency().map(concurrency_payload),
        ),
        (
            "introspection",
            attachment.introspection().map(introspection_payload),
        ),
        (
            "testLifecycle",
            attachment.test_lifecycle().map(lifecycle_payload),
        ),
        (
            "extensions",
            optional_array(
                &attachment
                    .extensions()
                    .iter()
                    .map(extension_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical Model pin.
fn model_ref_payload(pin: &super::super::storage_projection::ModelPin) -> String {
    object(vec![
        ("modelVersion", Some(string(pin.version().as_str()))),
        ("digest", Some(string(pin.digest().as_str()))),
    ])
}

/// One canonical `{identity, digest}` contract reference.
fn digest_payload(identity: &str, digest: &str) -> String {
    object(vec![
        ("identity", Some(string(identity))),
        ("digest", Some(string(digest))),
    ])
}

/// One canonical policies member.
fn policies_payload(policies: &super::Policies) -> String {
    object(vec![
        (
            "identifierQuote",
            Some(string(if policies.identifier_quote_always() {
                "always"
            } else {
                "never"
            })),
        ),
        ("json", Some(string(policies.json().key()))),
        ("enum", Some(string(policies.enum_policy().key()))),
        ("array", Some(string(policies.array().key()))),
        (
            "time",
            Some(object(vec![
                (
                    "instant",
                    Some(string(if policies.time().instant_timestamptz() {
                        "timestamptz"
                    } else {
                        "timestamp"
                    })),
                ),
                (
                    "local",
                    Some(string(if policies.time().local_allowed() {
                        "allowed"
                    } else {
                        "forbidden"
                    })),
                ),
            ])),
        ),
        (
            "pagination",
            Some(object(vec![
                (
                    "offset",
                    Some(string(if policies.pagination().offset_allowed() {
                        "allowed"
                    } else {
                        "forbidden"
                    })),
                ),
                (
                    "cursor",
                    Some(string(if policies.pagination().cursor_keyset() {
                        "keyset"
                    } else {
                        "forbidden"
                    })),
                ),
            ])),
        ),
    ])
}

/// One canonical tenancy member.
fn tenancy_payload(tenancy: &super::Tenancy) -> String {
    object(vec![
        ("enforcement", Some(string(tenancy.enforcement().key()))),
        ("rls", tenancy.rls().map(rls_payload)),
    ])
}

/// One canonical RLS policy.
fn rls_payload(rls: &super::RlsPolicy) -> String {
    object(vec![
        (
            "sessionVariable",
            Some(string(rls.session_variable().as_str())),
        ),
        ("force", Some(flag(rls.force()))),
    ])
}

/// One canonical concurrency member.
fn concurrency_payload(concurrency: &super::Concurrency) -> String {
    object(vec![
        (
            "versioning",
            Some(string(if concurrency.version_column() {
                "version_column"
            } else {
                "none"
            })),
        ),
        (
            "waitPolicy",
            Some(string(if concurrency.wait() {
                "wait"
            } else {
                "no_wait"
            })),
        ),
    ])
}

/// One canonical introspection member.
fn introspection_payload(introspection: &super::Introspection) -> String {
    object(vec![
        ("mode", Some(string("checked"))),
        (
            "scopes",
            Some(array(
                &introspection
                    .scopes()
                    .iter()
                    .map(|scope| string(scope.as_str()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "connection",
            Some(object(vec![(
                "name",
                Some(string(introspection.connection().as_str())),
            )])),
        ),
    ])
}

/// One canonical test lifecycle.
fn lifecycle_payload(lifecycle: &TestLifecycle) -> String {
    object(vec![
        ("isolation", Some(string(lifecycle.isolation().key()))),
        ("provision", Some(string(lifecycle.provision().key()))),
        ("cleanup", Some(string(lifecycle.cleanup().key()))),
        ("production", Some(string("forbidden"))),
        (
            "connection",
            Some(object(vec![(
                "name",
                Some(string(lifecycle.connection().as_str())),
            )])),
        ),
    ])
}

/// One canonical extension.
fn extension_payload(extension: &super::Extension) -> String {
    object(vec![
        ("name", Some(string(extension.name().as_str()))),
        (
            "state",
            Some(string(if extension.required() {
                "required"
            } else {
                "allowed"
            })),
        ),
    ])
}

/// One canonical JSON string value.
pub(crate) fn string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// One canonical JSON boolean.
pub(crate) fn flag(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

/// One canonical JSON array.
pub(crate) fn array(members: &[String]) -> String {
    format!("[{}]", members.join(","))
}

/// One canonical JSON array, or nothing when empty.
pub(crate) fn optional_array(members: &[String]) -> Option<String> {
    if members.is_empty() {
        None
    } else {
        Some(array(members))
    }
}

/// One canonical JSON object with byte-sorted keys; `None` members are
/// dropped entirely.
pub(crate) fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut present: Vec<(&str, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
    present.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let body: Vec<String> = present
        .iter()
        .map(|(key, value)| format!("{}:{}", string(key), value))
        .collect();
    format!("{{{}}}", body.join(","))
}
