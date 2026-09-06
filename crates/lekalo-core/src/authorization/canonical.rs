//! Deterministic canonical serialization of the authorization document
//! (issue #25).
//!
//! Canonical bytes are compact UTF-8 JSON with byte-sorted keys; every
//! repeated collection is stored sorted, so the canonical form is
//! independent of source order and byte-identical across runs. Digests
//! are SHA-256 over the exact canonical bytes.

use super::document::{
    ActorField, Binding, Composition, Document, FieldPermissions, Literal, Mapping, ModelRef,
    Operand, Ownership, Policy, Predicate, Relation, ScopeAnchor, ScopeMap, ScopeSource,
    SymbolDecl,
};
use super::version::{IDENTITY, PROFILE_IDENTITY, SCHEMA_VERSION};
use crate::versioning::plan::sha256_hex;
use serde_json::{json, Map, Value as Json};

fn string_list(values: &[String]) -> Json {
    Json::Array(values.iter().map(|v| Json::String(v.clone())).collect())
}

fn symbol_decl_json(decl: &SymbolDecl) -> Json {
    let mut object = Map::new();
    object.insert("id".to_owned(), Json::String(decl.id.clone()));
    if !decl.implies.is_empty() {
        object.insert("implies".to_owned(), string_list(&decl.implies));
    }
    Json::Object(object)
}

fn actor_field_json(field: ActorField) -> Json {
    Json::String(field.as_str().to_owned())
}

fn scope_source_json(source: &ScopeSource) -> Json {
    let text = match source {
        ScopeSource::Actor(field) => field.as_str().to_owned(),
        ScopeSource::Input(field) => format!("input.{field}"),
        ScopeSource::Resource(field) => format!("resource.{field}"),
        ScopeSource::Scope(dimension) => format!("scope.{}", dimension.as_str()),
    };
    Json::String(text)
}

fn scope_anchor_json(anchor: &ScopeAnchor) -> Json {
    let text = match anchor {
        ScopeAnchor::Resource(field) => format!("resource.{field}"),
        ScopeAnchor::Scope(dimension) => format!("scope.{}", dimension.as_str()),
    };
    Json::String(text)
}

fn binding_json(binding: &Binding) -> Json {
    let relation = match binding.relation {
        Relation::Equality => "equality",
        Relation::Membership => "membership",
        Relation::Ownership => "ownership",
    };
    json!({
        "anchor": scope_anchor_json(&binding.anchor),
        "relation": relation,
        "source": scope_source_json(&binding.source),
    })
}

fn scope_json(scope: &ScopeMap) -> Json {
    let mut object = Map::new();
    if let Some(binding) = &scope.tenant {
        object.insert("tenant".to_owned(), binding_json(binding));
    }
    if let Some(binding) = &scope.workspace {
        object.insert("workspace".to_owned(), binding_json(binding));
    }
    if let Some(binding) = &scope.user {
        object.insert("user".to_owned(), binding_json(binding));
    }
    Json::Object(object)
}

fn literal_json(literal: &Literal) -> Json {
    literal.to_json()
}

fn operand_json(operand: &Operand) -> Json {
    let text = match operand {
        Operand::Actor(field) => field.as_str().to_owned(),
        Operand::Input(field) => format!("input.{field}"),
        Operand::Resource(field) => format!("resource.{field}"),
        Operand::Scope(dimension) => format!("scope.{}", dimension.as_str()),
    };
    Json::String(text)
}

fn predicate_json(predicate: &Predicate) -> Json {
    match predicate {
        Predicate::All(items) => {
            json!({ "all": items.iter().map(predicate_json).collect::<Vec<_>>() })
        }
        Predicate::Any(items) => {
            json!({ "any": items.iter().map(predicate_json).collect::<Vec<_>>() })
        }
        Predicate::Not(inner) => json!({ "not": predicate_json(inner) }),
        Predicate::Equals(left, right) => {
            json!({ "left": operand_json(left), "op": "equals", "right": literal_json(right) })
        }
        Predicate::NotEquals(left, right) => {
            json!({ "left": operand_json(left), "op": "not_equals", "right": literal_json(right) })
        }
        Predicate::In(left, values) => json!({
            "left": operand_json(left),
            "op": "in",
            "values": values.iter().map(literal_json).collect::<Vec<_>>(),
        }),
    }
}

fn fields_json(fields: &FieldPermissions) -> Json {
    let mut object = Map::new();
    if !fields.read.is_empty() {
        object.insert("read".to_owned(), string_list(&fields.read));
    }
    if !fields.write.is_empty() {
        object.insert("write".to_owned(), string_list(&fields.write));
    }
    Json::Object(object)
}

fn ownership_json(ownership: &Ownership) -> Json {
    json!({
        "actor_field": actor_field_json(ownership.actor_field),
        "resource_field": Json::String(ownership.resource_field.clone()),
    })
}

fn model_ref_json(model_ref: &ModelRef) -> Json {
    json!({
        "ir_digest": Json::String(model_ref.ir_digest.clone()),
        "schema_version": Json::String(model_ref.schema_version.clone()),
    })
}

fn policy_json(policy: &Policy) -> Json {
    let mut object = Map::new();
    object.insert("id".to_owned(), Json::String(policy.id.clone()));
    object.insert(
        "actor".to_owned(),
        Json::String(policy.actor.as_str().to_owned()),
    );
    object.insert(
        "decision".to_owned(),
        Json::String(policy.decision.as_str().to_owned()),
    );
    object.insert(
        "error_ref".to_owned(),
        Json::String(policy.error_ref.clone()),
    );
    object.insert("applies_to".to_owned(), string_list(&policy.applies_to));
    if let Some(job) = &policy.job {
        object.insert("job".to_owned(), Json::String(job.clone()));
    }
    let scope = scope_json(&policy.scope);
    if !scope.as_object().expect("scope object").is_empty() {
        object.insert("scope".to_owned(), scope);
    }
    if !policy.capabilities.is_empty() {
        object.insert("capabilities".to_owned(), string_list(&policy.capabilities));
    }
    if !policy.roles.is_empty() {
        object.insert("roles".to_owned(), string_list(&policy.roles));
    }
    if !policy.ownership.is_empty() {
        object.insert(
            "ownership".to_owned(),
            Json::Array(policy.ownership.iter().map(ownership_json).collect()),
        );
    }
    if let Some(conditions) = &policy.conditions {
        object.insert("conditions".to_owned(), predicate_json(conditions));
    }
    if let Some(fields) = &policy.fields {
        let rendered = fields_json(fields);
        if !rendered.as_object().expect("fields object").is_empty() {
            object.insert("fields".to_owned(), rendered);
        }
    }
    if let Some(composition) = &policy.composition {
        object.insert("composition".to_owned(), Json::String(composition.clone()));
    }
    Json::Object(object)
}

fn composition_json(composition: &Composition) -> Json {
    let mut object = Map::new();
    object.insert("id".to_owned(), Json::String(composition.id.clone()));
    if !composition.all_of.is_empty() {
        object.insert("all_of".to_owned(), string_list(&composition.all_of));
    }
    if !composition.any_of.is_empty() {
        object.insert("any_of".to_owned(), string_list(&composition.any_of));
    }
    Json::Object(object)
}

fn mapping_json(mapping: &Mapping) -> Json {
    let mut object = Map::new();
    object.insert("target".to_owned(), Json::String(mapping.target.clone()));
    object.insert(
        "authorization_contract_ref".to_owned(),
        Json::String(IDENTITY.to_owned()),
    );
    object.insert(
        "semantic_policy_ref".to_owned(),
        Json::String(mapping.semantic_policy_ref.clone()),
    );
    object.insert(
        "mapping_state".to_owned(),
        Json::String(mapping.state.as_str().to_owned()),
    );
    if let Some(evidence) = &mapping.evidence_ref {
        object.insert("evidence_ref".to_owned(), Json::String(evidence.clone()));
    }
    object.insert(
        "observed_revision".to_owned(),
        Json::String(mapping.observed_revision.clone()),
    );
    if !mapping.reason_refs.is_empty() {
        object.insert("reason_refs".to_owned(), string_list(&mapping.reason_refs));
    }
    Json::Object(object)
}

impl Document {
    /// The canonical compact JSON value: byte-sorted keys, sorted
    /// collections, no source-order residue.
    pub fn to_json(&self) -> Json {
        let mut object = Map::new();
        object.insert(
            "schema_version".to_owned(),
            Json::String(SCHEMA_VERSION.to_owned()),
        );
        object.insert("identity".to_owned(), Json::String(IDENTITY.to_owned()));
        object.insert("model_ref".to_owned(), model_ref_json(&self.model_ref));
        object.insert(
            "profile_ref".to_owned(),
            Json::String(PROFILE_IDENTITY.to_owned()),
        );
        if !self.capabilities.is_empty() {
            object.insert(
                "capabilities".to_owned(),
                Json::Array(self.capabilities.iter().map(symbol_decl_json).collect()),
            );
        }
        if !self.roles.is_empty() {
            object.insert(
                "roles".to_owned(),
                Json::Array(self.roles.iter().map(symbol_decl_json).collect()),
            );
        }
        object.insert(
            "policies".to_owned(),
            Json::Array(self.policies.iter().map(policy_json).collect()),
        );
        if !self.compositions.is_empty() {
            object.insert(
                "compositions".to_owned(),
                Json::Array(self.compositions.iter().map(composition_json).collect()),
            );
        }
        if !self.mappings.is_empty() {
            object.insert(
                "mappings".to_owned(),
                Json::Array(self.mappings.iter().map(mapping_json).collect()),
            );
        }
        Json::Object(object)
    }

    /// The exact canonical bytes of this document.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(&self.to_json()).expect("canonical document bytes")
    }

    /// The `sha256:`-prefixed digest over the exact canonical bytes.
    pub fn digest_ref(&self) -> String {
        format!("sha256:{}", sha256_hex(self.canonical_json().as_bytes()))
    }
}
