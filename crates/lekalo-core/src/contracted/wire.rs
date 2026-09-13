//! Adapter declaration wire normalization (issue #40).
//!
//! [`parse_declaration`] is the single entry from raw declaration bytes
//! to the typed [`DeclarationDocument`]. It fails closed before any
//! merge: the byte bound, UTF-8, JSON syntax, the closed key sets,
//! identifier/digest/path grammars, and bounds each return one
//! registered diagnostic with no partial document. The declaration is
//! adapter-owned input, so it is never canonicalized here — only the
//! persisted registry has canonical bytes.

use std::collections::HashSet;

use serde_json::Value as Json;

use super::diagnostic;
use super::types::{
    AdapterIdentity, DeclarationDocument, DeclaredEffect, DeclaredSymbol, SignatureEvidence,
    SignatureField, SourceLocation, SupportArtifact, SupportKind, SupportLifecycle, SymbolKind,
};
use super::version;

/// The closed top-level member set of a declaration document.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "adapter",
    "project",
    "revision",
    "symbols",
    "artifacts",
];

/// The closed adapter member set.
const ADAPTER_KEYS: &[&str] = &["id", "version", "digest"];

/// The closed symbol member set.
const SYMBOL_KEYS: &[&str] = &[
    "id",
    "kind",
    "source",
    "fingerprint",
    "signature",
    "effects",
];

/// The closed source member set.
const SOURCE_KEYS: &[&str] = &["path", "line"];

/// The closed signature member set.
const SIGNATURE_KEYS: &[&str] = &["inputs", "output", "reads"];

/// The closed signature-input member set.
const INPUT_KEYS: &[&str] = &["name", "type", "required"];

/// The closed effect member set.
const EFFECT_KEYS: &[&str] = &["kind", "subject"];

/// The closed support-artifact member set.
const ARTIFACT_KEYS: &[&str] = &["symbol", "kind", "path", "lifecycle", "digest"];

/// The closed effect-kind keys (the #14 effect-kind wire words).
const EFFECT_KINDS: &[&str] = &[
    "read",
    "create",
    "update",
    "delete",
    "write-field",
    "emit-event",
    "enqueue-job",
    "external-call",
    "cache-read",
    "cache-write",
    "cache-invalidate",
    "publish-output",
    "audit-log",
    "transaction-boundary",
];

/// Parse and validate one declaration document.
pub(super) fn parse_declaration(
    bytes: &[u8],
) -> Result<DeclarationDocument, crate::diagnostics::DiagnosticSet> {
    if bytes.len() > version::MAX_REGISTRY_BYTES {
        return Err(diagnostic::declaration_limit_set(
            "declaration-bytes",
            bytes.len(),
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| diagnostic::declaration_invalid_set("invalid-encoding", None))?;
    let value: Json = serde_json::from_str(text)
        .map_err(|_| diagnostic::declaration_invalid_set("invalid-json", None))?;
    let map = as_object(&value)?;
    exact_keys(map, TOP_LEVEL_KEYS)?;
    let schema_version = string_member(map, "schemaVersion")?;
    if schema_version != super::DECLARATION_SCHEMA_VERSION {
        return Err(diagnostic::declaration_invalid_set("schema-version", None));
    }
    let adapter_map = as_object(map.get("adapter").ok_or_else(|| missing("adapter"))?)?;
    exact_keys(adapter_map, ADAPTER_KEYS)?;
    let adapter_id = string_member(adapter_map, "id")?;
    if !version::is_adapter_id(&adapter_id) {
        return Err(diagnostic::declaration_invalid_set(
            "adapter-id",
            Some(&adapter_id),
        ));
    }
    let adapter_version = string_member(adapter_map, "version")?;
    if !is_contract_version(&adapter_version) {
        return Err(diagnostic::declaration_invalid_set("adapter-version", None));
    }
    let adapter_digest = optional_string_member(adapter_map, "digest")?;
    if let Some(digest) = &adapter_digest {
        if !version::is_sha256(digest) {
            return Err(diagnostic::declaration_invalid_set("adapter-digest", None));
        }
    }
    let project = string_member(map, "project")?;
    if project.is_empty() || project.len() > 64 {
        return Err(diagnostic::declaration_invalid_set(
            "project-id",
            Some(&project),
        ));
    }
    let revision = string_member(map, "revision")?;
    if !version::is_sha256(&revision) {
        return Err(diagnostic::declaration_invalid_set("revision", None));
    }

    let symbols_member = map
        .get("symbols")
        .ok_or_else(|| missing("symbols"))?
        .as_array()
        .ok_or_else(|| diagnostic::declaration_invalid_set("symbols-array", None))?;
    if symbols_member.len() > version::MAX_SYMBOLS {
        return Err(diagnostic::declaration_limit_set(
            "symbols",
            symbols_member.len(),
        ));
    }
    let mut symbols = Vec::with_capacity(symbols_member.len());
    let mut seen_symbols = HashSet::new();
    for entry in symbols_member {
        let symbol = parse_symbol(entry)?;
        if !seen_symbols.insert(symbol.id.clone()) {
            return Err(diagnostic::declaration_invalid_set(
                "duplicate-id",
                Some(&symbol.id),
            ));
        }
        symbols.push(symbol);
    }

    let artifacts_member = optional_array(map.get("artifacts"))?;
    if artifacts_member.len() > version::MAX_SUPPORT_ARTIFACTS {
        return Err(diagnostic::declaration_limit_set(
            "artifacts",
            artifacts_member.len(),
        ));
    }
    let mut artifacts = Vec::with_capacity(artifacts_member.len());
    let mut seen_artifacts = HashSet::new();
    for entry in artifacts_member {
        let artifact = parse_artifact(entry)?;
        if !seen_symbols.contains(&artifact.symbol) {
            return Err(diagnostic::declaration_invalid_set(
                "unresolved-artifact-symbol",
                Some(&artifact.symbol),
            ));
        }
        if !seen_artifacts.insert(artifact.path.clone()) {
            return Err(diagnostic::declaration_invalid_set(
                "duplicate-path",
                Some(&artifact.path),
            ));
        }
        artifacts.push(artifact);
    }

    Ok(DeclarationDocument {
        adapter: AdapterIdentity {
            id: adapter_id,
            version: adapter_version,
            digest: adapter_digest,
        },
        project,
        revision,
        symbols,
        artifacts,
    })
}

fn parse_symbol(value: &Json) -> Result<DeclaredSymbol, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value)?;
    exact_keys(map, SYMBOL_KEYS)?;
    let id = string_member(map, "id")?;
    if !crate::trace::id::is_semantic_id(&id) {
        return Err(diagnostic::declaration_invalid_set("symbol-id", Some(&id)));
    }
    let kind_text = string_member(map, "kind")?;
    let kind = SymbolKind::parse(&kind_text)
        .ok_or_else(|| diagnostic::declaration_invalid_set("symbol-kind", Some(&kind_text)))?;
    let source_map = as_object(map.get("source").ok_or_else(|| missing("source"))?)?;
    exact_keys(source_map, SOURCE_KEYS)?;
    let path = string_member(source_map, "path")?;
    if crate::project_fs::path_violation(&path).is_some() || path.starts_with("lekalo/") {
        return Err(diagnostic::declaration_invalid_set(
            "source-path",
            Some(&path),
        ));
    }
    let line = match source_map.get("line") {
        None | Some(Json::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .filter(|line| (1..=1_000_000).contains(line))
                .ok_or_else(|| diagnostic::declaration_invalid_set("source-line", Some(&id)))?,
        ),
    };
    let fingerprint = optional_string_member(map, "fingerprint")?;
    if let Some(digest) = &fingerprint {
        if !version::is_sha256(digest) {
            return Err(diagnostic::declaration_invalid_set(
                "fingerprint",
                Some(&id),
            ));
        }
    }
    let signature = match map.get("signature") {
        None | Some(Json::Null) => None,
        Some(value) => {
            let map = as_object(value)?;
            exact_keys(map, SIGNATURE_KEYS)?;
            let inputs_member = optional_array(map.get("inputs"))?;
            if inputs_member.len() > version::MAX_ATTACHMENTS {
                return Err(diagnostic::declaration_limit_set(
                    "inputs",
                    inputs_member.len(),
                ));
            }
            let inputs = signature_inputs(inputs_member, &id)?;
            let output = optional_string_member(map, "output")?;
            if let Some(output) = &output {
                if !is_type_expression(output) {
                    return Err(diagnostic::declaration_invalid_set(
                        "output-type",
                        Some(&id),
                    ));
                }
            }
            let reads_member = optional_array(map.get("reads"))?;
            if reads_member.len() > version::MAX_ATTACHMENTS {
                return Err(diagnostic::declaration_limit_set(
                    "reads",
                    reads_member.len(),
                ));
            }
            let mut reads = Vec::with_capacity(reads_member.len());
            for entry in reads_member {
                let read = entry
                    .as_str()
                    .filter(|read| crate::trace::id::is_semantic_id(read))
                    .ok_or_else(|| {
                        diagnostic::declaration_invalid_set("read-reference", Some(&id))
                    })?
                    .to_owned();
                reads.push(read);
            }
            reads.sort();
            reads.dedup();
            Some(SignatureEvidence {
                inputs,
                output,
                reads,
            })
        }
    };
    let effects_member = optional_array(map.get("effects"))?;
    if effects_member.len() > version::MAX_ATTACHMENTS {
        return Err(diagnostic::declaration_limit_set(
            "effects",
            effects_member.len(),
        ));
    }
    let mut effects = Vec::with_capacity(effects_member.len());
    for entry in effects_member {
        let effect_map = as_object(entry)?;
        exact_keys(effect_map, EFFECT_KEYS)?;
        let kind = string_member(effect_map, "kind")?;
        if !EFFECT_KINDS.contains(&kind.as_str()) {
            return Err(diagnostic::declaration_invalid_set(
                "effect-kind",
                Some(&kind),
            ));
        }
        let subject = string_member(effect_map, "subject")?;
        if !crate::trace::id::is_semantic_id(&subject) && !is_entity_field_subject(&subject) {
            return Err(diagnostic::declaration_invalid_set(
                "effect-subject",
                Some(&subject),
            ));
        }
        effects.push(DeclaredEffect { kind, subject });
    }
    effects.sort();
    effects.dedup();
    Ok(DeclaredSymbol {
        id,
        kind,
        source: SourceLocation { path, line },
        fingerprint,
        signature,
        effects,
    })
}

fn signature_inputs(
    member: &[Json],
    id: &str,
) -> Result<Vec<SignatureField>, crate::diagnostics::DiagnosticSet> {
    let mut inputs = Vec::with_capacity(member.len());
    for entry in member {
        let input_map = as_object(entry)?;
        exact_keys(input_map, INPUT_KEYS)?;
        let name = string_member(input_map, "name")?;
        if !is_field_name(&name) {
            return Err(diagnostic::declaration_invalid_set("input-name", Some(id)));
        }
        let r#type = string_member(input_map, "type")?;
        if !is_type_expression(&r#type) {
            return Err(diagnostic::declaration_invalid_set("input-type", Some(id)));
        }
        let required = match input_map.get("required") {
            None | Some(Json::Null) => false,
            Some(Json::Bool(value)) => *value,
            Some(_) => {
                return Err(diagnostic::declaration_invalid_set(
                    "input-required",
                    Some(id),
                ))
            }
        };
        inputs.push(SignatureField {
            name,
            r#type,
            required,
        });
    }
    Ok(inputs)
}

fn parse_artifact(value: &Json) -> Result<SupportArtifact, crate::diagnostics::DiagnosticSet> {
    let map = as_object(value)?;
    exact_keys(map, ARTIFACT_KEYS)?;
    let symbol = string_member(map, "symbol")?;
    if !crate::trace::id::is_semantic_id(&symbol) {
        return Err(diagnostic::declaration_invalid_set(
            "artifact-symbol",
            Some(&symbol),
        ));
    }
    let kind_text = string_member(map, "kind")?;
    let kind = SupportKind::parse(&kind_text)
        .ok_or_else(|| diagnostic::declaration_invalid_set("artifact-kind", Some(&kind_text)))?;
    let path = string_member(map, "path")?;
    if !version::is_support_path(&path) {
        return Err(diagnostic::declaration_invalid_set(
            "artifact-path",
            Some(&path),
        ));
    }
    let lifecycle_text = string_member(map, "lifecycle")?;
    let lifecycle = SupportLifecycle::parse(&lifecycle_text).ok_or_else(|| {
        diagnostic::declaration_invalid_set("artifact-lifecycle", Some(&lifecycle_text))
    })?;
    let digest = optional_string_member(map, "digest")?;
    if let Some(digest) = &digest {
        if !version::is_sha256(digest) {
            return Err(diagnostic::declaration_invalid_set(
                "artifact-digest",
                Some(&path),
            ));
        }
    }
    Ok(SupportArtifact {
        symbol,
        kind,
        path,
        lifecycle,
        digest,
    })
}

/// `entity.field` subjects of field-scoped effect kinds.
fn is_entity_field_subject(text: &str) -> bool {
    match text.split_once('.') {
        Some((entity, field)) => crate::trace::id::is_semantic_id(entity) && is_field_name(field),
        None => false,
    }
}

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

/// The closed compact type-expression charset; the conformance engine
/// compares against canonical renderings of the typed IR.
fn is_type_expression(text: &str) -> bool {
    (1..=192).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'?' | b'(' | b')')
        })
}

fn as_object(
    value: &Json,
) -> Result<&serde_json::Map<String, Json>, crate::diagnostics::DiagnosticSet> {
    value
        .as_object()
        .ok_or_else(|| diagnostic::declaration_invalid_set("not-an-object", None))
}

fn exact_keys(
    map: &serde_json::Map<String, Json>,
    keys: &[&str],
) -> Result<(), crate::diagnostics::DiagnosticSet> {
    for key in map.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(diagnostic::declaration_invalid_set(
                "unknown-key",
                Some(key),
            ));
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
        Some(_) => Err(diagnostic::declaration_invalid_set("not-an-array", None)),
    }
}

fn missing(key: &str) -> crate::diagnostics::DiagnosticSet {
    diagnostic::declaration_invalid_set("missing-key", Some(key))
}
