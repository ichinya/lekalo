//! Canonical serialization of the storage-engine-profile attachment
//! (issue #117).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted object keys,
//! no BOM, and no trailing LF; absent optional members are dropped
//! entirely. Set-like collections (sql mode, capabilities, adapters)
//! normalize to unsigned UTF-8-byte order. The output is
//! path-independent and byte-identical for value-equal profiles.

use crate::diagnostics::DiagnosticSet;

use super::StorageEngineProfile;

/// The canonical attachment payload bytes, or the typed over-bound
/// refusal.
pub fn attachment_bytes(profile: &StorageEngineProfile) -> Result<String, DiagnosticSet> {
    let bytes = payload(profile);
    if bytes.len() > super::version::MAX_CANONICAL_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The full canonical profile payload.
fn payload(profile: &StorageEngineProfile) -> String {
    object(vec![
        (
            "schemaVersion",
            Some(string(super::version::SCHEMA_VERSION)),
        ),
        ("identity", Some(string(super::version::IDENTITY))),
        (
            "attachmentRevision",
            Some(string(profile.attachment_revision().as_str())),
        ),
        ("projectId", Some(string(profile.project_id().as_str()))),
        ("modelRef", Some(model_ref_payload(profile.model_ref()))),
        (
            "irRef",
            Some(digest_payload(
                "dev.lekalo.ir@0.2.16",
                profile.ir_digest().as_str(),
            )),
        ),
        (
            "sourceMapRef",
            profile
                .source_map_ref()
                .map(|reference| string(reference.as_str())),
        ),
        ("engine", Some(engine_payload(profile.engine()))),
        (
            "capabilities",
            Some(object(
                profile
                    .capabilities()
                    .iter()
                    .map(|(id, capability)| (id.key(), Some(capability_payload(capability))))
                    .collect::<Vec<(&str, Option<String>)>>(),
            )),
        ),
        (
            "testLifecycle",
            Some(test_lifecycle_payload(profile.test_lifecycle())),
        ),
        (
            "adapters",
            optional_array(
                &profile
                    .adapters()
                    .iter()
                    .map(adapter_payload)
                    .collect::<Vec<String>>(),
            ),
        ),
    ])
}

/// One canonical Model pin.
fn model_ref_payload(pin: &crate::storage_projection::ModelPin) -> String {
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

/// One canonical engine identity block.
fn engine_payload(engine: &super::EngineIdentity) -> String {
    object(vec![
        ("engine", Some(string(engine.engine().key()))),
        ("engineVersion", Some(string(engine.engine_version()))),
        ("variant", Some(string(engine.variant().key()))),
        (
            "sqlMode",
            Some(array(
                &engine
                    .sql_mode()
                    .iter()
                    .map(|token| string(token.key()))
                    .collect::<Vec<String>>(),
            )),
        ),
        (
            "defaultStorageEngine",
            Some(string(engine.default_storage_engine())),
        ),
        ("charset", Some(string(engine.charset()))),
        ("collation", Some(string(engine.collation()))),
        ("timeZone", Some(string(engine.time_zone()))),
        ("evidence", Some(evidence_payload(engine.evidence()))),
    ])
}

/// One canonical capability record.
fn capability_payload(capability: &super::Capability) -> String {
    object(vec![
        ("support", Some(string(capability.support().key()))),
        ("bounds", capability.bounds().map(string)),
        ("evidence", Some(evidence_payload(capability.evidence()))),
    ])
}

/// One canonical test lifecycle block.
fn test_lifecycle_payload(lifecycle: &super::TestLifecycle) -> String {
    object(vec![
        ("create", Some(capability_payload(lifecycle.create()))),
        ("drop", Some(capability_payload(lifecycle.drop()))),
        ("isolation", Some(string(lifecycle.isolation().key()))),
        ("production", Some(string("forbidden"))),
        (
            "testSchemaPrefix",
            Some(string(lifecycle.test_schema_prefix())),
        ),
        ("evidence", Some(evidence_payload(lifecycle.evidence()))),
    ])
}

/// One canonical adapter evidence record.
fn adapter_payload(adapter: &super::AdapterEvidence) -> String {
    object(vec![
        ("name", Some(string(adapter.name().key()))),
        ("dialect", Some(string(adapter.dialect()))),
        ("evidence", Some(evidence_payload(adapter.evidence()))),
    ])
}

/// One canonical evidence record.
fn evidence_payload(evidence: &super::Evidence) -> String {
    object(vec![
        ("kind", Some(string(evidence.kind().key()))),
        ("ref", Some(string(evidence.reference()))),
    ])
}

/// One canonical JSON string.
fn string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// One canonical JSON array.
fn array(members: &[String]) -> String {
    format!("[{}]", members.join(","))
}

/// One canonical JSON array, or nothing when empty.
fn optional_array(members: &[String]) -> Option<String> {
    if members.is_empty() {
        None
    } else {
        Some(array(members))
    }
}

/// One canonical JSON object with byte-sorted members.
fn object(members: Vec<(&str, Option<String>)>) -> String {
    let mut sorted: Vec<(String, String)> = members
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    sorted.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let body: Vec<String> = sorted
        .into_iter()
        .map(|(key, value)| format!("{}:{value}", string(&key)))
        .collect();
    format!("{{{}}}", body.join(","))
}
