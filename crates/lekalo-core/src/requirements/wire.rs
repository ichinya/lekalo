//! Wire normalization of the requirements attachment (issue #36).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`RequirementsAttachment`](super::RequirementsAttachment). It fails
//! closed before any semantic processing: unknown or missing fields,
//! wrong identities, malformed identifiers, digests, roots, bounds, and
//! duplicate declarations each return one typed registered diagnostic
//! and no partial attachment. Namespace nesting and per-provider tree
//! validity are resolution-time concerns.

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use crate::project_fs;
use crate::scenario::id::SemanticId;
use serde_json::Value as Json;

use super::diagnostic;
use super::version;
use super::{
    ModelRef, ProviderDecl, ProviderKind, Relation, RequirementLink, RequirementsAttachment,
};

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "providers",
    "references",
];

/// The provider declaration member set.
const PROVIDER_KEYS: &[&str] = &["source", "kind", "root"];

/// The reference member set.
const REFERENCE_KEYS: &[&str] = &["symbol", "relation", "source", "requirement", "revision"];

/// The Model pin member set.
const MODEL_REF_KEYS: &[&str] = &["modelVersion", "digest"];

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no model,
/// filesystem, cache, report, network, process, or target access of any
/// kind.
pub(crate) fn from_value(json: &Json) -> Result<RequirementsAttachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("top-level-shape", None))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::document_invalid("unknown-field", Some(key)));
        }
    }
    for required in TOP_LEVEL_KEYS {
        if !object.contains_key(*required) {
            return Err(diagnostic::document_invalid(
                "missing-field",
                Some(required),
            ));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::document_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::document_invalid("contract-identity", None));
    }
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("project-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("project-id", None))?;
    let model_ref = model_ref(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?,
    )?;
    let providers = providers(
        object
            .get("providers")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("provider-list", None))?,
    )?;
    let references = references(
        object
            .get("references")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("reference-list", None))?,
    )?;
    if providers.len() > version::MAX_PROVIDERS {
        return Err(diagnostic::export_limit(
            "providers",
            &format!("max={}", version::MAX_PROVIDERS),
        ));
    }
    if references.len() > version::MAX_REFERENCES {
        return Err(diagnostic::export_limit(
            "references",
            &format!("max={}", version::MAX_REFERENCES),
        ));
    }
    // Namespace uniqueness and containment: two providers may never
    // share a source id, and no root may lexically contain another (the
    // walk would resolve one tree twice under different namespaces).
    let mut sorted_providers = providers.clone();
    sorted_providers.sort();
    for index in 0..sorted_providers.len() {
        for rest in sorted_providers.iter().skip(index + 1) {
            let left = &sorted_providers[index];
            if left.source == rest.source {
                return Err(diagnostic::document_invalid(
                    "duplicate-source",
                    Some(&left.source),
                ));
            }
            if is_contained_root(&left.root, &rest.root) {
                return Err(diagnostic::document_invalid(
                    "nested-provider-root",
                    Some(&rest.source),
                ));
            }
        }
    }
    // References are a set: the same (symbol, relation, source,
    // requirement) tuple twice is one declaration said twice.
    let mut sorted_references = references.clone();
    sorted_references.sort();
    for pair in sorted_references.windows(2) {
        if pair[0].symbol == pair[1].symbol
            && pair[0].relation == pair[1].relation
            && pair[0].source == pair[1].source
            && pair[0].requirement == pair[1].requirement
        {
            return Err(diagnostic::document_invalid(
                "duplicate-reference",
                Some(&pair[0].requirement),
            ));
        }
    }
    Ok(RequirementsAttachment::assemble(
        project_id,
        model_ref,
        sorted_providers,
        sorted_references,
    ))
}

/// Whether two project-relative logical roots are equal or one contains
/// the other at a segment boundary.
fn is_contained_root(first: &str, second: &str) -> bool {
    if first == second {
        return true;
    }
    let (outer, inner) = if first.len() < second.len() {
        (first, second)
    } else {
        (second, first)
    };
    inner
        .strip_prefix(outer)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some()
}

/// Parse the bound source Model pin.
fn model_ref(json: &Json) -> Result<ModelRef, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?;
    for key in object.keys() {
        if !MODEL_REF_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::document_invalid("unknown-field", Some(key)));
        }
    }
    let model_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("model-version", None))?;
    if model_version != "0.1.0" && model_version != "1.0.0" {
        return Err(diagnostic::document_invalid("model-version", None));
    }
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("model-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("model-digest", None))?;
    Ok(ModelRef {
        model_version: model_version.to_owned(),
        digest,
    })
}

/// Parse every provider declaration.
fn providers(json: &[Json]) -> Result<Vec<ProviderDecl>, DiagnosticSet> {
    let mut parsed = Vec::new();
    for provider in json {
        let object = provider
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("provider-shape", None))?;
        for key in object.keys() {
            if !PROVIDER_KEYS.contains(&key.as_str()) {
                return Err(diagnostic::document_invalid("unknown-field", Some(key)));
            }
        }
        for required in PROVIDER_KEYS {
            if !object.contains_key(*required) {
                return Err(diagnostic::document_invalid(
                    "missing-field",
                    Some(required),
                ));
            }
        }
        let source = object
            .get("source")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("provider-source", None))?;
        if !super::is_source_id(source) {
            return Err(diagnostic::document_invalid(
                "provider-source",
                Some(source),
            ));
        }
        let kind = ProviderKind::parse(
            object
                .get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("provider-kind", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("provider-kind", None))?;
        let root = object
            .get("root")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("provider-root", None))?;
        if project_fs::path_violation(root).is_some() || !root.is_ascii() {
            return Err(diagnostic::document_invalid("provider-root", Some(root)));
        }
        parsed.push(ProviderDecl {
            source: source.to_owned(),
            kind,
            root: root.to_owned(),
        });
    }
    Ok(parsed)
}

/// Parse every requirement reference.
fn references(json: &[Json]) -> Result<Vec<RequirementLink>, DiagnosticSet> {
    let mut parsed = Vec::new();
    for link in json {
        let object = link
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("reference-shape", None))?;
        for key in object.keys() {
            if !REFERENCE_KEYS.contains(&key.as_str()) {
                return Err(diagnostic::document_invalid("unknown-field", Some(key)));
            }
        }
        for required in REFERENCE_KEYS {
            if !object.contains_key(*required) {
                return Err(diagnostic::document_invalid(
                    "missing-field",
                    Some(required),
                ));
            }
        }
        let symbol = object
            .get("symbol")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("reference-symbol", None))?;
        if !crate::trace::id::is_semantic_id(symbol) {
            return Err(diagnostic::document_invalid(
                "reference-symbol",
                Some(symbol),
            ));
        }
        let relation = Relation::parse(
            object
                .get("relation")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("reference-relation", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("reference-relation", None))?;
        let source = object
            .get("source")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("reference-source", None))?;
        if !super::is_source_id(source) {
            return Err(diagnostic::document_invalid(
                "reference-source",
                Some(source),
            ));
        }
        let requirement = object
            .get("requirement")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("reference-requirement", None))?;
        if !super::is_requirement_id(requirement) {
            return Err(diagnostic::document_invalid(
                "reference-requirement",
                Some(requirement),
            ));
        }
        let revision = Sha256Digest::parse(
            object
                .get("revision")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("reference-revision", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("reference-revision", None))?;
        parsed.push(RequirementLink {
            symbol: symbol.to_owned(),
            relation,
            source: source.to_owned(),
            requirement: requirement.to_owned(),
            revision,
        });
    }
    Ok(parsed)
}
