//! Wire normalization of the storage-engine attachment (issue #69).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`StorageEngineAttachment`](super::StorageEngineAttachment). It
//! fails closed before semantic processing: unknown or missing fields,
//! wrong identities, malformed identifiers, digests, bounds, and
//! incoherent policies each return one typed registered diagnostic and
//! no partial attachment. Semantic rules (extension-list coherence,
//! RLS coherence, connection honesty) live in the attachment's
//! semantic self-check.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};
use crate::scenario::id::SemanticId;
use serde_json::{Map, Value as Json};

use super::diagnostic;
use super::id::{ConnectionName, Engine, ExtensionName, ScopeName, SessionVariable, VersionPin};
use super::{
    ArrayPolicy, Cleanup, Concurrency, Enforcement, EnumPolicy as EnumPolicyWire, Extension,
    Introspection, Isolation, JsonPolicy, PaginationPolicy, Policies, Provision,
    StorageEngineAttachment, Tenancy, TestLifecycle, TimePolicy,
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
    "projectionRef",
    "engine",
    "engineVersion",
    "policies",
    "tenancy",
    "concurrency",
    "introspection",
    "testLifecycle",
    "extensions",
];

/// The required top-level members (tenancy, concurrency, introspection,
/// testLifecycle, and extensions are optional).
const REQUIRED_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "projectionRef",
    "engine",
    "engineVersion",
    "policies",
];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub(crate) fn from_value(json: &Json) -> Result<StorageEngineAttachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    for required in REQUIRED_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::input_invalid("missing-field"));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(super::version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(super::version::IDENTITY) {
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
    let projection_ref = Sha256Digest::parse(
        object
            .get("projectionRef")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("projection-ref"))?,
    )
    .map_err(|_| diagnostic::input_invalid("projection-ref"))?;
    let engine = Engine::parse(
        object
            .get("engine")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("engine"))?,
    )
    .map_err(|_| diagnostic::input_invalid("engine"))?;
    let engine_version = VersionPin::parse(
        object
            .get("engineVersion")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("engine-version"))?,
    )
    .map_err(|_| diagnostic::input_invalid("engine-version"))?;
    let policies = policies(
        object
            .get("policies")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("policies-shape"))?,
    )?;
    let tenancy = match object.get("tenancy") {
        Some(Json::Null) | None => None,
        Some(value) => {
            Some(tenancy(value.as_object().ok_or_else(|| {
                diagnostic::input_invalid("tenancy-shape")
            })?)?)
        }
    };
    let concurrency = match object.get("concurrency") {
        Some(Json::Null) | None => None,
        Some(value) => {
            Some(concurrency(value.as_object().ok_or_else(|| {
                diagnostic::input_invalid("concurrency-shape")
            })?)?)
        }
    };
    let introspection = match object.get("introspection") {
        Some(Json::Null) | None => None,
        Some(value) => {
            Some(introspection(value.as_object().ok_or_else(|| {
                diagnostic::input_invalid("introspection-shape")
            })?)?)
        }
    };
    let test_lifecycle = match object.get("testLifecycle") {
        Some(Json::Null) | None => None,
        Some(value) => {
            Some(test_lifecycle(value.as_object().ok_or_else(|| {
                diagnostic::input_invalid("lifecycle-shape")
            })?)?)
        }
    };
    let extensions = match object.get("extensions") {
        Some(Json::Null) | None => Vec::new(),
        Some(value) => extensions(
            value
                .as_array()
                .ok_or_else(|| diagnostic::input_invalid("extensions-shape"))?,
        )?,
    };
    let attachment = StorageEngineAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_digest,
        projection_ref,
        engine,
        engine_version,
        policies,
        tenancy,
        concurrency,
        introspection,
        test_lifecycle,
        extensions,
    );
    attachment.semantic_self_check()?;
    Ok(attachment)
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

/// One bounded string member.
fn string_member<'a>(object: &'a WireMap, key: &str) -> Result<&'a str, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("member-shape"))
}

/// Parse the closed SQL-shaping policies.
fn policies(object: &WireMap) -> Result<Policies, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "identifierQuote" | "json" | "enum" | "array" | "time" | "pagination"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if string_member(object, "identifierQuote")? != "always" {
        return Err(diagnostic::input_invalid("identifier-quote"));
    }
    let json = JsonPolicy::parse(string_member(object, "json")?)
        .map_err(|_| diagnostic::input_invalid("json-policy"))?;
    let enum_policy = EnumPolicyWire::parse(string_member(object, "enum")?)
        .map_err(|_| diagnostic::input_invalid("enum-policy"))?;
    let array = ArrayPolicy::parse(string_member(object, "array")?)
        .map_err(|_| diagnostic::input_invalid("array-policy"))?;
    let time = object
        .get("time")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("time-shape"))?;
    for key in time.keys() {
        if !matches!(key.as_str(), "instant" | "local") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if string_member(time, "instant")? != "timestamptz" {
        return Err(diagnostic::input_invalid("time-policy"));
    }
    let local_allowed = match string_member(time, "local")? {
        "forbidden" => false,
        "allowed" => true,
        _ => return Err(diagnostic::input_invalid("time-policy")),
    };
    let pagination = object
        .get("pagination")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("pagination-shape"))?;
    for key in pagination.keys() {
        if !matches!(key.as_str(), "offset" | "cursor") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let offset_allowed = match string_member(pagination, "offset")? {
        "allowed" => true,
        "forbidden" => false,
        _ => return Err(diagnostic::input_invalid("pagination-policy")),
    };
    let cursor_keyset = match string_member(pagination, "cursor")? {
        "keyset" => true,
        "forbidden" => false,
        _ => return Err(diagnostic::input_invalid("pagination-policy")),
    };
    Ok(Policies {
        identifier_quote_always: true,
        json,
        enum_policy,
        array,
        time: TimePolicy {
            instant_timestamptz: true,
            local_allowed,
        },
        pagination: PaginationPolicy {
            offset_allowed,
            cursor_keyset,
        },
    })
}

/// Parse the declared tenancy enforcement.
fn tenancy(object: &WireMap) -> Result<Tenancy, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(key.as_str(), "enforcement" | "rls") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let enforcement = Enforcement::parse(string_member(object, "enforcement")?)
        .map_err(|_| diagnostic::input_invalid("enforcement"))?;
    let rls = match object.get("rls") {
        Some(Json::Null) | None => None,
        Some(value) => {
            let rls = value
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("rls-shape"))?;
            for key in rls.keys() {
                if !matches!(key.as_str(), "sessionVariable" | "force") {
                    return Err(diagnostic::input_invalid("unknown-field"));
                }
            }
            let session_variable = SessionVariable::parse(string_member(rls, "sessionVariable")?)
                .map_err(|_| diagnostic::input_invalid("session-variable"))?;
            let force = rls
                .get("force")
                .and_then(Json::as_bool)
                .ok_or_else(|| diagnostic::input_invalid("rls-shape"))?;
            Some(super::RlsPolicy {
                session_variable,
                force,
            })
        }
    };
    Ok(Tenancy { enforcement, rls })
}

/// Parse the declared optimistic-versioning policy.
fn concurrency(object: &WireMap) -> Result<Concurrency, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(key.as_str(), "versioning" | "waitPolicy") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let version_column = match string_member(object, "versioning")? {
        "none" => false,
        "version_column" => true,
        _ => return Err(diagnostic::input_invalid("versioning")),
    };
    let wait = match string_member(object, "waitPolicy")? {
        "wait" => true,
        "no_wait" => false,
        _ => return Err(diagnostic::input_invalid("wait-policy")),
    };
    Ok(Concurrency {
        version_column,
        wait,
    })
}

/// Parse the declared checked-mode introspection.
fn introspection(object: &WireMap) -> Result<Introspection, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(key.as_str(), "mode" | "scopes" | "connection") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if string_member(object, "mode")? != "checked" {
        return Err(diagnostic::input_invalid("introspection-mode"));
    }
    let scopes = object
        .get("scopes")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("scopes-shape"))?;
    if scopes.is_empty() || scopes.len() > super::version::MAX_SCOPES {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(scopes.len());
    for entry in scopes {
        parsed.push(
            ScopeName::parse(
                entry
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("scopes-shape"))?,
            )
            .map_err(|_| diagnostic::input_invalid("scope-name"))?,
        );
    }
    parsed.sort();
    parsed.dedup();
    let connection = ConnectionName::parse(string_member(
        object
            .get("connection")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("connection-shape"))?,
        "name",
    )?)
    .map_err(|_| diagnostic::input_invalid("connection-name"))?;
    Ok(Introspection {
        scopes: parsed,
        connection,
    })
}

/// Parse the declared test-database lifecycle.
fn test_lifecycle(object: &WireMap) -> Result<TestLifecycle, DiagnosticSet> {
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "isolation" | "provision" | "cleanup" | "production" | "connection"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let isolation = Isolation::parse(string_member(object, "isolation")?)
        .map_err(|_| diagnostic::input_invalid("isolation"))?;
    let provision = Provision::parse(string_member(object, "provision")?)
        .map_err(|_| diagnostic::input_invalid("provision"))?;
    let cleanup = Cleanup::parse(string_member(object, "cleanup")?)
        .map_err(|_| diagnostic::input_invalid("cleanup"))?;
    if string_member(object, "production")? != "forbidden" {
        return Err(diagnostic::input_invalid("lifecycle-production"));
    }
    let connection = ConnectionName::parse(string_member(
        object
            .get("connection")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("connection-shape"))?,
        "name",
    )?)
    .map_err(|_| diagnostic::input_invalid("connection-name"))?;
    Ok(TestLifecycle {
        isolation,
        provision,
        cleanup,
        connection,
    })
}

/// Parse the closed extension allow-list; canonical order is name.
fn extensions(array: &[Json]) -> Result<Vec<Extension>, DiagnosticSet> {
    if array.len() > super::version::MAX_EXTENSIONS {
        return Err(diagnostic::input_invalid("bound-exceeded"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let extension = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("extension-shape"))?;
        for key in extension.keys() {
            if !matches!(key.as_str(), "name" | "state") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let name = ExtensionName::parse(string_member(extension, "name")?)
            .map_err(|_| diagnostic::input_invalid("extension-name"))?;
        let required = match string_member(extension, "state")? {
            "allowed" => false,
            "required" => true,
            _ => return Err(diagnostic::input_invalid("extension-state")),
        };
        parsed.push(Extension { name, required });
    }
    parsed.sort_by(|left, right| left.name.cmp(&right.name));
    if parsed
        .windows(2)
        .any(|window| window[0].name == window[1].name)
    {
        return Err(diagnostic::input_invalid("duplicate-extension"));
    }
    Ok(parsed)
}
