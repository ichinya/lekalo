//! Adapter scan wire normalization (issue #39).
//!
//! [`parse_scan`] is the single entry from raw scan bytes to the typed
//! [`ScanDocument`]. It fails closed before any merge: the byte bound,
//! UTF-8, JSON syntax, the closed key sets, identifier/digest/grammar
//! rules, and bounds each return one registered diagnostic with no
//! partial document. The scan is adapter-owned input, so it is never
//! canonicalized here — only the persisted index has canonical bytes.

use std::collections::HashSet;

use super::version;
use crate::loader::ModelVersion;
use serde_json::Value as Json;

use super::diagnostic;
use super::types::{
    AdapterIdentity, CandidateRecord, Confidence, Evidence, FieldEvidence, ReferenceEvidence,
    ScanDocument, ScanEndpoint, ScanSchema, ScanSymbol, ScanTestBinding, SchemaKind,
    SourceLocation, SymbolKind, ValueEvidence,
};

/// The closed top-level member set of a 1.0.0 scan document (the frozen
/// issue #39 base).
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "adapter",
    "project",
    "revision",
    "symbols",
    "endpoints",
    "schemas",
];

/// The closed top-level member set of a 1.1.0 scan document (issue #42):
/// the additive declared target, adapter profile, and native test
/// bindings.
const TOP_LEVEL_KEYS_V11: &[&str] = &[
    "schemaVersion",
    "adapter",
    "project",
    "revision",
    "symbols",
    "endpoints",
    "schemas",
    "target",
    "profile",
    "testBindings",
];

/// The closed adapter member set.
const ADAPTER_KEYS: &[&str] = &["id", "version", "digest"];
/// The closed symbol member set of a 1.0.0 scan document (the frozen
/// issue #39 base).
const SYMBOL_KEYS: &[&str] = &[
    "id",
    "kind",
    "stableKey",
    "location",
    "fingerprint",
    "mappingConfidence",
    "evidence",
];

/// The closed symbol member set of a 1.1.0 scan document (issue #42):
/// the additive per-symbol candidate set.
const SYMBOL_KEYS_V11: &[&str] = &[
    "id",
    "kind",
    "stableKey",
    "location",
    "fingerprint",
    "mappingConfidence",
    "evidence",
    "candidates",
];

/// The closed candidate member set (issue #42).
const CANDIDATE_KEYS: &[&str] = &["native", "path", "line", "fingerprint", "confidence"];

/// The closed native-test-binding member set (issue #42).
const TEST_BINDING_KEYS: &[&str] = &["id", "symbol", "path", "fingerprint", "confidence"];

/// The closed evidence member set.
const EVIDENCE_KEYS: &[&str] = &[
    "signature",
    "references",
    "fields",
    "identityFields",
    "base",
    "values",
];

/// The closed location member set.
const LOCATION_KEYS: &[&str] = &["path", "line"];

/// The closed reference member set.
const REFERENCE_KEYS: &[&str] = &["target", "role", "confidence"];

/// The closed field member set.
const FIELD_KEYS: &[&str] = &["name", "type", "required"];

/// The closed enum-value member set.
const VALUE_KEYS: &[&str] = &["value", "description"];

/// The closed endpoint member set.
const ENDPOINT_KEYS: &[&str] = &["id", "method", "path", "symbol"];

/// The closed endpoint methods (the Model endpoint grammar).
const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];

/// The closed scalar bases (the Model scalar grammar).
const BASES: &[&str] = &[
    "string", "number", "boolean", "date", "datetime", "uuid", "uri",
];

/// The closed schema-record member set.
const SCHEMA_KEYS: &[&str] = &["id", "kind", "symbol", "digest"];

/// Parse and validate one adapter scan document.
pub(super) fn parse_scan(
    bytes: &[u8],
    model_version: ModelVersion,
) -> Result<ScanDocument, crate::diagnostics::DiagnosticSet> {
    if bytes.len() > version::MAX_SCAN_BYTES {
        return Err(diagnostic::scan_limit_set("scan-bytes", bytes.len()));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| diagnostic::scan_invalid_set("invalid-encoding", None))?;
    let value: Json = serde_json::from_str(text)
        .map_err(|_| diagnostic::scan_invalid_set("invalid-json", None))?;
    let map = as_object(&value, "document")?;
    let schema_version = string_member(map, "schemaVersion")?;
    if !version::SCAN_SCHEMA_VERSIONS.contains(&schema_version.as_str()) {
        return Err(diagnostic::scan_invalid_set("schema-version", None));
    }
    // The additive issue #42 members exist only on a 1.1.0 document; the
    // frozen 1.0.0 key sets refuse them exactly as unknown members.
    let extension = schema_version == version::SCAN_SCHEMA_VERSION;
    exact_keys(
        map,
        if extension {
            TOP_LEVEL_KEYS_V11
        } else {
            TOP_LEVEL_KEYS
        },
    )?;
    let adapter_map = as_object(
        map.get("adapter").ok_or_else(|| missing("adapter"))?,
        "adapter",
    )?;
    exact_keys(adapter_map, ADAPTER_KEYS)?;
    let adapter_id = string_member(adapter_map, "id")?;
    if !version::is_adapter_id(&adapter_id) {
        return Err(diagnostic::scan_invalid_set(
            "adapter-id",
            Some(&adapter_id),
        ));
    }
    let adapter_version = string_member(adapter_map, "version")?;
    if !is_contract_version(&adapter_version) {
        return Err(diagnostic::scan_invalid_set("adapter-version", None));
    }
    let adapter_digest = optional_string_member(adapter_map, "digest")?;
    if let Some(digest) = &adapter_digest {
        if !version::is_sha256(digest) {
            return Err(diagnostic::scan_invalid_set("adapter-digest", None));
        }
    }
    let project = string_member(map, "project")?;
    if !model_version.module_id_valid(&project) {
        return Err(diagnostic::scan_invalid_set("project-id", Some(&project)));
    }
    let revision = string_member(map, "revision")?;
    if !version::is_sha256(&revision) {
        return Err(diagnostic::scan_invalid_set("revision", None));
    }

    let symbols_member = map
        .get("symbols")
        .ok_or_else(|| missing("symbols"))?
        .as_array()
        .ok_or_else(|| diagnostic::scan_invalid_set("symbols-array", None))?;
    if symbols_member.len() > version::MAX_SYMBOLS {
        return Err(diagnostic::scan_limit_set("symbols", symbols_member.len()));
    }
    let mut symbols = Vec::with_capacity(symbols_member.len());
    let mut seen_symbols = HashSet::new();
    for entry in symbols_member {
        let symbol = parse_symbol(entry, model_version, extension)?;
        if !seen_symbols.insert(symbol.id.clone()) {
            return Err(diagnostic::scan_invalid_set(
                "duplicate-id",
                Some(&symbol.id),
            ));
        }
        symbols.push(symbol);
    }

    let endpoints_member = optional_array(map.get("endpoints"))?;
    if endpoints_member.len() > version::MAX_ENDPOINTS {
        return Err(diagnostic::scan_limit_set(
            "endpoints",
            endpoints_member.len(),
        ));
    }
    let mut endpoints = Vec::with_capacity(endpoints_member.len());
    let mut seen_endpoints = HashSet::new();
    for entry in endpoints_member {
        let endpoint = parse_endpoint(entry)?;
        if !seen_symbols.contains(&endpoint.symbol) {
            return Err(diagnostic::scan_invalid_set(
                "unresolved-endpoint-symbol",
                Some(&endpoint.symbol),
            ));
        }
        let route = (endpoint.method.clone(), endpoint.path.clone());
        if !seen_endpoints.insert(route) {
            return Err(diagnostic::scan_invalid_set(
                "duplicate-endpoint",
                Some(&endpoint.id),
            ));
        }
        endpoints.push(endpoint);
    }

    let schemas_member = optional_array(map.get("schemas"))?;
    if schemas_member.len() > version::MAX_SCHEMAS {
        return Err(diagnostic::scan_limit_set("schemas", schemas_member.len()));
    }
    let mut schemas = Vec::with_capacity(schemas_member.len());
    let mut seen_schemas = HashSet::new();
    for entry in schemas_member {
        let schema = parse_schema(entry)?;
        if !seen_symbols.contains(&schema.symbol) {
            return Err(diagnostic::scan_invalid_set(
                "unresolved-schema-symbol",
                Some(&schema.symbol),
            ));
        }
        if !seen_schemas.insert(schema.id.clone()) {
            return Err(diagnostic::scan_invalid_set(
                "duplicate-id",
                Some(&schema.id),
            ));
        }
        schemas.push(schema);
    }

    let target = match extension {
        false => None,
        true => match optional_string_member(map, "target")? {
            None => None,
            Some(target) => {
                if !crate::init::detect::valid_target_id(&target) {
                    return Err(diagnostic::scan_invalid_set("scan-target", Some(&target)));
                }
                Some(target)
            }
        },
    };
    let profile = match extension {
        false => None,
        true => match optional_string_member(map, "profile")? {
            None => None,
            Some(profile) => {
                if !crate::target_protocol::scopes::is_token(&profile) {
                    return Err(diagnostic::scan_invalid_set("scan-profile", None));
                }
                Some(profile)
            }
        },
    };
    if profile.is_some() && target.is_none() {
        return Err(diagnostic::scan_invalid_set("profile-without-target", None));
    }
    let test_bindings_member = match extension {
        false => [].as_slice(),
        true => optional_array(map.get("testBindings"))?,
    };
    if test_bindings_member.len() > version::MAX_TEST_BINDINGS {
        return Err(diagnostic::scan_limit_set(
            "test-bindings",
            test_bindings_member.len(),
        ));
    }
    let mut test_bindings = Vec::with_capacity(test_bindings_member.len());
    let mut seen_test_bindings = HashSet::new();
    for entry in test_bindings_member {
        let binding = parse_test_binding(entry)?;
        if !seen_symbols.contains(&binding.symbol) {
            return Err(diagnostic::scan_invalid_set(
                "unresolved-test-symbol",
                Some(&binding.symbol),
            ));
        }
        if !seen_test_bindings.insert(binding.id.clone()) {
            return Err(diagnostic::scan_invalid_set(
                "duplicate-id",
                Some(&binding.id),
            ));
        }
        test_bindings.push(binding);
    }

    Ok(ScanDocument {
        adapter: AdapterIdentity {
            id: adapter_id,
            version: adapter_version,
            digest: adapter_digest,
        },
        project,
        revision,
        symbols,
        endpoints,
        schemas,
        target,
        profile,
        test_bindings,
    })
}

fn parse_symbol(
    value: &Json,
    model_version: ModelVersion,
    extension: bool,
) -> Result<ScanSymbol, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value, "symbol")?;
    exact_keys(
        map,
        if extension {
            SYMBOL_KEYS_V11
        } else {
            SYMBOL_KEYS
        },
    )?;
    let id = string_member(map, "id")?;
    if !crate::trace::id::is_semantic_id(&id) {
        return Err(diagnostic::scan_invalid_set("symbol-id", Some(&id)));
    }
    let kind_text = string_member(map, "kind")?;
    let kind = SymbolKind::parse(&kind_text)
        .ok_or_else(|| diagnostic::scan_invalid_set("symbol-kind", Some(&kind_text)))?;
    let stable_key = optional_string_member(map, "stableKey")?;
    if let Some(key) = &stable_key {
        if key.is_empty() || key.len() > 256 || key.chars().any(|c| c.is_control()) {
            return Err(diagnostic::scan_invalid_set("stable-key", Some(&id)));
        }
    }
    let location = match map.get("location") {
        None | Some(Json::Null) => None,
        Some(value) => {
            let map = as_object(value, "location")?;
            exact_keys(map, LOCATION_KEYS)?;
            let path = string_member(map, "path")?;
            if crate::project_fs::path_violation(&path).is_some() {
                return Err(diagnostic::scan_invalid_set("location-path", Some(&id)));
            }
            let line = match map.get("line") {
                None | Some(Json::Null) => None,
                Some(value) => {
                    let line = value
                        .as_u64()
                        .ok_or_else(|| diagnostic::scan_invalid_set("location-line", Some(&id)))?;
                    if line == 0 || line > 1_000_000 {
                        return Err(diagnostic::scan_invalid_set("location-line", Some(&id)));
                    }
                    Some(line)
                }
            };
            Some(SourceLocation { path, line })
        }
    };
    let fingerprint = optional_string_member(map, "fingerprint")?;
    if let Some(digest) = &fingerprint {
        if !version::is_sha256(digest) {
            return Err(diagnostic::scan_invalid_set("fingerprint", Some(&id)));
        }
    }
    let mapping_text = string_member(map, "mappingConfidence")?;
    let mapping = Confidence::parse(&mapping_text)
        .ok_or_else(|| diagnostic::scan_invalid_set("mapping-confidence", Some(&id)))?;
    let evidence = match map.get("evidence") {
        None | Some(Json::Null) => Evidence::default(),
        Some(value) => parse_evidence(value, &id)?,
    };
    let module = id.split('.').next().unwrap_or_default();
    if !model_version.module_id_valid(module) {
        return Err(diagnostic::scan_invalid_set("symbol-module", Some(&id)));
    }
    let candidates = match extension {
        false => Vec::new(),
        true => {
            let member = optional_array(map.get("candidates"))?;
            if member.len() > version::MAX_CANDIDATES {
                return Err(diagnostic::scan_limit_set("candidates", member.len()));
            }
            let mut candidates = Vec::with_capacity(member.len());
            for entry in member {
                candidates.push(parse_candidate(entry, &id)?);
            }
            candidates.sort_by(|left, right| {
                (&left.confidence, &left.native).cmp(&(&right.confidence, &right.native))
            });
            candidates.dedup();
            candidates
        }
    };
    Ok(ScanSymbol {
        id,
        kind,
        stable_key,
        location,
        fingerprint,
        mapping,
        evidence,
        candidates,
    })
}

/// One native symbol candidate (issue #42).
fn parse_candidate(
    value: &Json,
    id: &str,
) -> Result<CandidateRecord, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value, "candidate")?;
    exact_keys(map, CANDIDATE_KEYS)?;
    let native = string_member(map, "native")?;
    if native.is_empty() || native.len() > 256 || native.chars().any(|c| c.is_control()) {
        return Err(diagnostic::scan_invalid_set("candidate-native", Some(id)));
    }
    let path = string_member(map, "path")?;
    if crate::project_fs::path_violation(&path).is_some() {
        return Err(diagnostic::scan_invalid_set("candidate-path", Some(id)));
    }
    let line = match map.get("line") {
        None | Some(Json::Null) => None,
        Some(value) => {
            let line = value
                .as_u64()
                .ok_or_else(|| diagnostic::scan_invalid_set("candidate-line", Some(id)))?;
            if line == 0 || line > 1_000_000 {
                return Err(diagnostic::scan_invalid_set("candidate-line", Some(id)));
            }
            Some(line)
        }
    };
    let fingerprint = optional_string_member(map, "fingerprint")?;
    if let Some(digest) = &fingerprint {
        if !version::is_sha256(digest) {
            return Err(diagnostic::scan_invalid_set(
                "candidate-fingerprint",
                Some(id),
            ));
        }
    }
    let confidence_text = string_member(map, "confidence")?;
    let confidence = Confidence::parse(&confidence_text)
        .ok_or_else(|| diagnostic::scan_invalid_set("candidate-confidence", Some(id)))?;
    Ok(CandidateRecord {
        native,
        path,
        line,
        fingerprint,
        confidence,
    })
}

/// One native test binding (issue #42).
fn parse_test_binding(value: &Json) -> Result<ScanTestBinding, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value, "testBinding")?;
    exact_keys(map, TEST_BINDING_KEYS)?;
    let id = string_member(map, "id")?;
    if !version::is_external_id(&id) {
        return Err(diagnostic::scan_invalid_set("test-binding-id", Some(&id)));
    }
    let symbol = string_member(map, "symbol")?;
    if !crate::trace::id::is_semantic_id(&symbol) {
        return Err(diagnostic::scan_invalid_set(
            "test-binding-symbol",
            Some(&id),
        ));
    }
    let path = string_member(map, "path")?;
    if crate::project_fs::path_violation(&path).is_some() {
        return Err(diagnostic::scan_invalid_set("test-binding-path", Some(&id)));
    }
    let fingerprint = optional_string_member(map, "fingerprint")?;
    if let Some(digest) = &fingerprint {
        if !version::is_sha256(digest) {
            return Err(diagnostic::scan_invalid_set(
                "test-binding-fingerprint",
                Some(&id),
            ));
        }
    }
    Ok(ScanTestBinding {
        id,
        symbol,
        path,
        fingerprint,
    })
}

fn parse_evidence(value: &Json, id: &str) -> Result<Evidence, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value, "evidence")?;
    exact_keys(map, EVIDENCE_KEYS)?;
    let signature = optional_string_member(map, "signature")?;
    if let Some(digest) = &signature {
        if !version::is_sha256(digest) {
            return Err(diagnostic::scan_invalid_set("evidence-signature", Some(id)));
        }
    }
    let references_member = optional_array(map.get("references"))?;
    if references_member.len() > version::MAX_REFERENCES {
        return Err(diagnostic::scan_limit_set(
            "references",
            references_member.len(),
        ));
    }
    let mut references = Vec::with_capacity(references_member.len());
    for entry in references_member {
        let map = as_object(entry, "reference")?;
        exact_keys(map, REFERENCE_KEYS)?;
        let target = string_member(map, "target")?;
        if !crate::trace::id::is_semantic_id(&target) {
            return Err(diagnostic::scan_invalid_set("reference-target", Some(id)));
        }
        let role_text = string_member(map, "role")?;
        let role = super::types::ReferenceRole::parse(&role_text)
            .ok_or_else(|| diagnostic::scan_invalid_set("reference-role", Some(id)))?;
        let confidence_text = string_member(map, "confidence")?;
        let confidence = Confidence::parse(&confidence_text)
            .ok_or_else(|| diagnostic::scan_invalid_set("reference-confidence", Some(id)))?;
        references.push(ReferenceEvidence {
            target,
            role,
            confidence,
        });
    }
    let fields_member = optional_array(map.get("fields"))?;
    if fields_member.len() > version::MAX_REFERENCES {
        return Err(diagnostic::scan_limit_set("fields", fields_member.len()));
    }
    let fields = parse_fields(fields_member, id)?;
    let identity_member = optional_array(map.get("identityFields"))?;
    if identity_member.len() > version::MAX_REFERENCES {
        return Err(diagnostic::scan_limit_set(
            "identity-fields",
            identity_member.len(),
        ));
    }
    let mut identity_fields = Vec::with_capacity(identity_member.len());
    for entry in identity_member {
        let name = entry
            .as_str()
            .ok_or_else(|| diagnostic::scan_invalid_set("identity-field", Some(id)))?;
        if !is_field_name(name) || !fields.iter().any(|field| field.name == name) {
            return Err(diagnostic::scan_invalid_set("identity-field", Some(id)));
        }
        identity_fields.push(name.to_owned());
    }
    identity_fields.sort();
    let base = optional_string_member(map, "base")?;
    if let Some(base) = &base {
        if !BASES.contains(&base.as_str()) {
            return Err(diagnostic::scan_invalid_set("scalar-base", Some(id)));
        }
    }
    let values_member = optional_array(map.get("values"))?;
    if values_member.len() > version::MAX_VALUES {
        return Err(diagnostic::scan_limit_set("values", values_member.len()));
    }
    let mut values = Vec::with_capacity(values_member.len());
    for entry in values_member {
        let map = as_object(entry, "value")?;
        exact_keys(map, VALUE_KEYS)?;
        let value = string_member(map, "value")?;
        if !is_value_word(&value) {
            return Err(diagnostic::scan_invalid_set("enum-value", Some(id)));
        }
        let description = optional_string_member(map, "description")?;
        if let Some(text) = &description {
            if !is_bounded_prose(text) {
                return Err(diagnostic::scan_invalid_set("value-description", Some(id)));
            }
        }
        values.push(ValueEvidence { value, description });
    }
    Ok(Evidence {
        signature,
        references,
        fields,
        identity_fields,
        base,
        values,
    })
}

fn parse_fields(
    member: &[Json],
    id: &str,
) -> Result<Vec<FieldEvidence>, crate::diagnostics::DiagnosticSet> {
    let mut fields = Vec::with_capacity(member.len());
    for entry in member {
        let map = as_object(entry, "field")?;
        exact_keys(map, FIELD_KEYS)?;
        let name = string_member(map, "name")?;
        if !is_field_name(&name) {
            return Err(diagnostic::scan_invalid_set("field-name", Some(id)));
        }
        let r#type = string_member(map, "type")?;
        if !is_type_expression(&r#type) {
            return Err(diagnostic::scan_invalid_set("field-type", Some(id)));
        }
        let required = map
            .get("required")
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| diagnostic::scan_invalid_set("field-required", Some(id)))
            })
            .transpose()?
            .unwrap_or(false);
        fields.push(FieldEvidence {
            name,
            r#type,
            required,
        });
    }
    Ok(fields)
}

fn parse_endpoint(value: &Json) -> Result<ScanEndpoint, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value, "endpoint")?;
    exact_keys(map, ENDPOINT_KEYS)?;
    let id = string_member(map, "id")?;
    if !crate::trace::id::is_semantic_id(&id) {
        return Err(diagnostic::scan_invalid_set("endpoint-id", Some(&id)));
    }
    let method = string_member(map, "method")?;
    if !METHODS.contains(&method.as_str()) {
        return Err(diagnostic::scan_invalid_set("endpoint-method", Some(&id)));
    }
    let path = string_member(map, "path")?;
    if !is_endpoint_path(&path) {
        return Err(diagnostic::scan_invalid_set("endpoint-path", Some(&id)));
    }
    let symbol = string_member(map, "symbol")?;
    if !crate::trace::id::is_semantic_id(&symbol) {
        return Err(diagnostic::scan_invalid_set("endpoint-symbol", Some(&id)));
    }
    Ok(ScanEndpoint {
        id,
        method,
        path,
        symbol,
    })
}

fn parse_schema(value: &Json) -> Result<ScanSchema, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value, "schema")?;
    exact_keys(map, SCHEMA_KEYS)?;
    let id = string_member(map, "id")?;
    if !crate::trace::id::is_semantic_id(&id) {
        return Err(diagnostic::scan_invalid_set("schema-id", Some(&id)));
    }
    let kind_text = string_member(map, "kind")?;
    let kind = SchemaKind::parse(&kind_text)
        .ok_or_else(|| diagnostic::scan_invalid_set("schema-kind", Some(&id)))?;
    let symbol = string_member(map, "symbol")?;
    if !crate::trace::id::is_semantic_id(&symbol) {
        return Err(diagnostic::scan_invalid_set("schema-symbol", Some(&id)));
    }
    let digest = optional_string_member(map, "digest")?;
    if let Some(digest) = &digest {
        if !version::is_sha256(digest) {
            return Err(diagnostic::scan_invalid_set("schema-digest", Some(&id)));
        }
    }
    Ok(ScanSchema {
        id,
        kind,
        symbol,
        digest,
    })
}

// ---------------------------------------------------------------------------
// Grammar helpers
// ---------------------------------------------------------------------------

/// The canonical `X.Y.Z` contract version grammar.
fn is_contract_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The Model field-name grammar.
fn is_field_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// The closed compact type-expression charset; the loader re-validates
/// every promoted spelling against the full grammar at apply time.
fn is_type_expression(text: &str) -> bool {
    (1..=192).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'?' | b'<' | b'>')
        })
}

/// The endpoint route grammar (`^(/[a-z0-9:_{}-]+)+$`, bounded).
fn is_endpoint_path(text: &str) -> bool {
    (1..=512).contains(&text.len())
        && text.split('/').skip(1).all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b':' | b'_' | b'{' | b'}' | b'-')
                })
        })
        && text.starts_with('/')
}

/// The enum-value word grammar.
fn is_value_word(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// Bounded printable prose for descriptions: at most 256 bytes, no
/// control characters, and no quote/backslash characters that would
/// complicate the canonical YAML emission.
fn is_bounded_prose(text: &str) -> bool {
    (1..=256).contains(&text.len())
        && text
            .chars()
            .all(|character| !character.is_control() && !matches!(character, '"' | '\\' | '\u{7f}'))
}

fn as_object<'a>(
    value: &'a Json,
    _what: &'static str,
) -> Result<&'a serde_json::Map<String, Json>, crate::diagnostics::DiagnosticSet> {
    value
        .as_object()
        .ok_or_else(|| diagnostic::scan_invalid_set("not-an-object", None))
}

fn exact_keys(
    map: &serde_json::Map<String, Json>,
    keys: &[&str],
) -> Result<(), crate::diagnostics::DiagnosticSet> {
    for key in map.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(diagnostic::scan_invalid_set("unknown-key", Some(key)));
        }
    }
    Ok(())
}

fn string_member(
    map: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    match map.get(key) {
        Some(Json::String(text)) => Ok(text.clone()),
        _ => Err(missing(key)),
    }
}

fn optional_string_member(
    map: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Option<String>, crate::diagnostics::DiagnosticSet> {
    match map.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(missing(key)),
    }
}

fn optional_array(member: Option<&Json>) -> Result<&[Json], crate::diagnostics::DiagnosticSet> {
    match member {
        None | Some(Json::Null) => Ok(&[]),
        Some(Json::Array(items)) => Ok(items),
        Some(_) => Err(diagnostic::scan_invalid_set("not-an-array", None)),
    }
}

fn missing(key: &str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::scan_invalid_set("missing-key", Some(key))
}
