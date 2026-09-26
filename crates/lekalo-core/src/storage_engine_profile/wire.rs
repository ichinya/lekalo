//! Wire normalization of the storage-engine-profile attachment (issue
//! #117).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`StorageEngineProfile`](super::StorageEngineProfile). It fails
//! closed before semantic processing: unknown or missing fields, wrong
//! identities, malformed version spellings, range or wildcard engine
//! versions, unsorted sql-mode lists, capability ids outside the closed
//! vocabulary, partial support without its bounds note, and hostile
//! credential-shaped members each return one typed registered
//! diagnostic and no partial profile.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::{NamespacedId, SemanticId};
use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::id::{
    is_engine_version, is_time_zone, EngineToken, ProfileCapabilityId, SqlModeToken, VariantToken,
};
use super::version;
use super::StorageEngineProfile;
use super::{
    AdapterEvidence, AdapterToken, Capability, EngineIdentity, Evidence, EvidenceKind, Support,
    TestIsolation, TestLifecycle,
};

/// The parsed wire object type.
type WireMap = Map<String, Json>;

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "sourceMapRef",
    "engine",
    "capabilities",
    "testLifecycle",
    "adapters",
];

/// The required top-level members (`sourceMapRef` and `adapters` are
/// optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "engine",
    "capabilities",
    "testLifecycle",
];

/// The closed host/credential-shaped member names the grammar refuses:
/// a profile can never carry connection evidence.
const FORBIDDEN_MEMBERS: &[&str] = &[
    "host",
    "hostname",
    "user",
    "username",
    "password",
    "dsn",
    "url",
    "uri",
    "port",
    "endpoint",
    "connectionString",
];

/// Normalize one wire document into a validated profile, or return the
/// typed rejection set with no partial profile. Pure: no source,
/// network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<StorageEngineProfile, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    let attachment_revision = SemVer::parse(
        object
            .get("attachmentRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("attachment-revision"))?,
    )
    .map_err(|_| diagnostic::input_invalid("attachment-revision"))?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("project-id"))?,
    )
    .map_err(|_| diagnostic::input_invalid("project-id"))?;
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::input_invalid("model-ref"))?,
    )?;
    let ir_digest = digest_member(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?,
        "dev.lekalo.ir@0.2.16",
    )?;
    let source_map_ref = match object.get("sourceMapRef") {
        Some(Json::Null) | None => None,
        Some(value) => Some(
            NamespacedId::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("source-map-ref"))?,
            )
            .map_err(|_| diagnostic::input_invalid("source-map-ref"))?,
        ),
    };
    let engine = engine_identity(
        object
            .get("engine")
            .ok_or_else(|| diagnostic::input_invalid("engine-shape"))?,
    )?;
    let capabilities = capabilities(
        object
            .get("capabilities")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("capabilities-shape"))?,
    )?;
    let test_lifecycle = test_lifecycle(
        object
            .get("testLifecycle")
            .ok_or_else(|| diagnostic::input_invalid("test-lifecycle-shape"))?,
    )?;
    let adapters = match object.get("adapters") {
        Some(Json::Null) | None => Vec::new(),
        Some(value) => adapters(
            value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("adapters-shape"))?,
        )?,
    };
    let profile = StorageEngineProfile::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        source_map_ref,
        engine,
        capabilities,
        test_lifecycle,
        adapters,
    );
    profile.semantic_self_check()?;
    Ok(profile)
}

/// Parse the bound source Model pin.
fn model_pin(json: &Json) -> Result<crate::storage_projection::ModelPin, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("model-ref"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "modelVersion" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let pin_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-version"))?;
    let pin = match pin_version {
        "0.2.16" => crate::scenario::ModelPin::Current,
        _ => return Err(diagnostic::input_invalid("model-version")),
    };
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("model-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("model-digest"))?;
    Ok(crate::storage_projection::ModelPin {
        version: pin,
        digest,
    })
}

/// Parse one `{identity, digest}` contract reference.
fn digest_member(json: &Json, identity: &str) -> Result<Sha256Digest, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("ir-ref"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if object.get("identity").and_then(Json::as_str) != Some(identity) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-digest"))?,
    )
    .map_err(|_| diagnostic::input_invalid("ir-digest"))
}

/// One bounded string member with the declared token bound.
fn bounded_string(object: &WireMap, key: &str, bound: usize) -> Result<String, DiagnosticSet> {
    let text = object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("member-shape"))?;
    if text.is_empty() || text.len() > bound {
        return Err(diagnostic::input_invalid("member-bound"));
    }
    Ok(text.to_owned())
}

/// Parse one bounded evidence record.
fn evidence(json: &Json) -> Result<Evidence, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "kind" | "ref") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
    }
    let kind = EvidenceKind::parse(
        object
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("evidence-kind"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("evidence-kind"))?;
    let reference = bounded_string(object, "ref", version::MAX_EVIDENCE_REF_BYTES)?;
    Ok(Evidence { kind, reference })
}

/// Parse the engine identity block.
fn engine_identity(json: &Json) -> Result<EngineIdentity, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("engine-shape"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "engine"
                | "engineVersion"
                | "variant"
                | "sqlMode"
                | "defaultStorageEngine"
                | "charset"
                | "collation"
                | "timeZone"
                | "evidence"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
    }
    let engine = EngineToken::parse(
        object
            .get("engine")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("engine-token"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("engine-token"))?;
    let engine_version = bounded_string(object, "engineVersion", 16)?;
    if !is_engine_version(&engine_version) {
        // A range (`8.0-8.4`) or a wildcard (`8.0.x`) is a refusal,
        // never a claim: exact engine/version evidence is mandatory.
        return Err(diagnostic::input_invalid("engine-version-range"));
    }
    let variant = VariantToken::parse(
        object
            .get("variant")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("variant-token"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("variant-token"))?;
    // The sql-mode member is mandatory: the mode is always declared,
    // never implicit. An empty list declares the empty server mode.
    let sql_mode_array = object
        .get("sqlMode")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("sql-mode-missing"))?;
    if sql_mode_array.len() > version::MAX_SQL_MODE_TOKENS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut sql_mode = Vec::with_capacity(sql_mode_array.len());
    for entry in sql_mode_array {
        let token = SqlModeToken::parse(
            entry
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("sql-mode-token"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("sql-mode-token"))?;
        sql_mode.push(token);
    }
    sql_mode.sort();
    sql_mode.dedup();
    let default_storage_engine =
        bounded_string(object, "defaultStorageEngine", version::MAX_TOKEN_BYTES)?;
    let charset = bounded_string(object, "charset", version::MAX_TOKEN_BYTES)?;
    let collation = bounded_string(object, "collation", version::MAX_TOKEN_BYTES)?;
    let time_zone = bounded_string(object, "timeZone", 16)?;
    if !is_time_zone(&time_zone) {
        return Err(diagnostic::input_invalid("time-zone"));
    }
    let evidence = evidence(
        object
            .get("evidence")
            .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?,
    )?;
    Ok(EngineIdentity {
        engine,
        engine_version,
        variant,
        sql_mode,
        default_storage_engine,
        charset,
        collation,
        time_zone,
        evidence,
    })
}

/// Parse one capability record.
fn capability(json: &Json) -> Result<Capability, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
    for key in object.keys() {
        if !matches!(key.as_str(), "support" | "bounds" | "evidence") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
    }
    let support = Support::parse(
        object
            .get("support")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("support-token"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("support-token"))?;
    let bounds = match object.get("bounds") {
        Some(Json::Null) | None => None,
        Some(value) => {
            let bounds = value
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("bounds-shape"))?;
            if bounds.is_empty() || bounds.len() > version::MAX_EVIDENCE_REF_BYTES {
                return Err(diagnostic::input_invalid("bounds-shape"));
            }
            Some(bounds.to_owned())
        }
    };
    if support == Support::Partial && bounds.is_none() {
        // Partial support is honest only with its bounded gap note.
        return Err(diagnostic::input_invalid("partial-without-bounds"));
    }
    let evidence = evidence(
        object
            .get("evidence")
            .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?,
    )?;
    Ok(Capability {
        support,
        bounds,
        evidence,
    })
}

/// Parse the closed capability map; canonical order is the id.
fn capabilities(map: &WireMap) -> Result<Vec<(ProfileCapabilityId, Capability)>, DiagnosticSet> {
    if map.len() > version::MAX_CAPABILITIES {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(map.len());
    for (key, value) in map {
        let Some(id) = ProfileCapabilityId::parse(key) else {
            return Err(diagnostic::input_invalid("capability-id"));
        };
        parsed.push((id, capability(value)?));
    }
    parsed.sort_by(|left, right| left.0.key().cmp(right.0.key()));
    if parsed
        .windows(2)
        .any(|window| window[0].0.key() == window[1].0.key())
    {
        return Err(diagnostic::input_invalid("duplicate-capability"));
    }
    Ok(parsed)
}

/// Parse the test lifecycle block.
fn test_lifecycle(json: &Json) -> Result<TestLifecycle, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("test-lifecycle-shape"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "create" | "drop" | "isolation" | "production" | "testSchemaPrefix" | "evidence"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
        if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("credential-member"));
        }
    }
    // The production token is the closed `forbidden` constant a strict
    // consumer can gate on; no other spelling exists.
    if object.get("production").and_then(Json::as_str) != Some("forbidden") {
        return Err(diagnostic::input_invalid("production-forbidden"));
    }
    let isolation = TestIsolation::parse(
        object
            .get("isolation")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("isolation-token"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("isolation-token"))?;
    let test_schema_prefix = bounded_string(
        object,
        "testSchemaPrefix",
        version::MAX_TEST_SCHEMA_PREFIX_BYTES,
    )?;
    if !test_schema_prefix
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        || !test_schema_prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(diagnostic::input_invalid("test-schema-prefix"));
    }
    let create = capability(
        object
            .get("create")
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?,
    )?;
    let drop = capability(
        object
            .get("drop")
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?,
    )?;
    let evidence = evidence(
        object
            .get("evidence")
            .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?,
    )?;
    Ok(TestLifecycle {
        create,
        drop,
        isolation,
        test_schema_prefix,
        evidence,
    })
}

/// Parse the adapter evidence list.
fn adapters(array: &[Json]) -> Result<Vec<AdapterEvidence>, DiagnosticSet> {
    if array.len() > version::MAX_ADAPTERS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("adapter-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "name" | "dialect" | "evidence") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
            if FORBIDDEN_MEMBERS.contains(&key.as_str()) {
                return Err(diagnostic::input_invalid("credential-member"));
            }
        }
        let name = AdapterToken::parse(
            object
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("adapter-token"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("adapter-token"))?;
        let dialect = bounded_string(object, "dialect", version::MAX_EVIDENCE_REF_BYTES)?;
        let evidence = evidence(
            object
                .get("evidence")
                .ok_or_else(|| diagnostic::input_invalid("evidence-shape"))?,
        )?;
        parsed.push(AdapterEvidence {
            name,
            dialect,
            evidence,
        });
    }
    parsed.sort_by(|left, right| left.name.key().cmp(right.name.key()));
    if parsed
        .windows(2)
        .any(|window| window[0].name.key() == window[1].name.key())
    {
        return Err(diagnostic::input_invalid("duplicate-adapter"));
    }
    Ok(parsed)
}
