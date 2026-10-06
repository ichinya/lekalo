//! Bounded consumer context from a public bundle only (issue #113).
//!
//! `plan_consumer_context` decodes the standalone bundle and follows only
//! the selected exports and their required public dependencies. It cannot
//! load the provider's full compilation: a fixture with the entire provider
//! checkout absent must succeed. Required facts are never dropped merely to
//! make `fits:true`; an over-budget capsule preserves `fits:false`, the
//! minimum required cost, and the explicit omission reasons. No raw source,
//! absolute paths, private error text, or storage mapping is emitted.

use super::diagnostic::invalid;
use super::public::parse_bundle;
use super::wire::{
    self, ContextCapsule, EstimatorPin, ExportFacts, Omission, PublicBundle, PublicDependency,
    PublicExport, MAX_CONTEXT_ROOTS, MAX_CONTEXT_TOKENS,
};
use crate::diagnostics::DiagnosticSet;

/// Estimate the cost of one canonical fact: the documented offline
/// estimator (`public-facts-chars4/1`) divides the canonical UTF-8 byte
/// length by four, minimum one token. Deterministic, allocation-free, and
/// identical for equivalent facts.
pub fn estimate(canonical_bytes: &str) -> u64 {
    (canonical_bytes.len() as u64 / 4).max(1)
}

/// Plan the bounded consumer capsule: the selected exports, their required
/// public dependencies, exact estimator pins, fits, minimum required cost,
/// and omissions with reasons.
pub fn plan_consumer_context(
    bundle_bytes: &[u8],
    consumer: &str,
    uses: &[String],
    budget: u64,
) -> Result<ContextCapsule, DiagnosticSet> {
    if budget == 0 || budget > MAX_CONTEXT_TOKENS {
        return Err(invalid("budget-bound"));
    }
    if uses.is_empty() || uses.len() > MAX_CONTEXT_ROOTS {
        return Err(invalid("selection-bound"));
    }
    let mut unique: Vec<String> = uses.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != uses.len() {
        return Err(invalid("duplicate-selection"));
    }
    let bundle: PublicBundle = parse_bundle(bundle_bytes)?;
    let by_id: std::collections::BTreeMap<&str, &PublicExport> = bundle
        .exports
        .iter()
        .map(|export| (export.export.as_str(), export))
        .collect();
    let dependency_by_id: std::collections::BTreeMap<&str, &PublicDependency> = bundle
        .public_dependencies
        .iter()
        .map(|dependency| (dependency.module_id.as_str(), dependency))
        .collect();
    // Every selected id must be an exact export or a pinned dependency
    // export; no short-name guessing.
    let mut selected: Vec<&PublicExport> = Vec::new();
    let mut gaps: Vec<String> = Vec::new();
    let mut omissions: Vec<Omission> = Vec::new();
    for use_id in uses {
        match by_id.get(use_id.as_str()) {
            Some(export) => selected.push(export),
            None => {
                if dependency_by_id
                    .values()
                    .any(|dependency| dependency.exports.contains(use_id))
                {
                    gaps.push("dependency-export-not-materialized".to_owned());
                    omissions.push(Omission {
                        subject: use_id.clone(),
                        reason: "dependency-signature-not-in-bundle".to_owned(),
                    });
                } else {
                    return Err(invalid("unknown-selection"));
                }
            }
        }
    }
    // Required public dependencies of the selected exports: the bundle's
    // bounded dependency set carries the exact pins, so no implicit fetch
    // is needed.
    let mut required_dependencies: Vec<PublicDependency> = Vec::new();
    for export in &selected {
        let references = export_references(export);
        for dependency in &bundle.public_dependencies {
            if references
                .iter()
                .any(|reference| dependency.exports.contains(reference))
                && !required_dependencies
                    .iter()
                    .any(|pinned| pinned.module_id == dependency.module_id)
            {
                required_dependencies.push((*dependency).clone());
            }
        }
    }
    required_dependencies.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    // Required facts: signatures, typed errors, effect obligations,
    // capability and version/deprecation obligations.
    let mut facts = Vec::new();
    for export in &selected {
        let signature = signature_text(export);
        let errors = export_errors(export);
        let effects = export
            .effects
            .iter()
            .map(|obligation| {
                format!(
                    "{}|{}|{}",
                    obligation.entity, obligation.operation, obligation.emits.len()
                )
            })
            .collect();
        let deprecation = bundle
            .deprecations
            .iter()
            .find(|entry| entry.export == export.export)
            .cloned();
        facts.push(ExportFacts {
            export: export.export.clone(),
            kind: export.kind.clone(),
            signature,
            errors,
            effects,
            capabilities: bundle
                .capabilities
                .iter()
                .map(|capability| capability.capability.clone())
                .collect(),
            deprecation,
            api_version: bundle.api_version.clone(),
        });
    }
    // Cost: required facts first, then the selected ordering.
    facts.sort_by(|left, right| left.export.cmp(&right.export));
    let mut required_cost = 0u64;
    for fact in &facts {
        required_cost = required_cost.saturating_add(estimate(&wire::canonical(fact)));
    }
    for dependency in &required_dependencies {
        required_cost = required_cost.saturating_add(estimate(&wire::canonical(dependency)));
    }
    let selected_cost = required_cost;
    // Protected required facts can never be dropped for fit: an
    // over-budget capsule stays explicit.
    let fits = selected_cost <= budget;
    if !fits {
        gaps.push("budget-exceeded".to_owned());
    }
    // Deterministic capsule digest over the exact projection.
    let producer = bundle.producer.clone();
    let producer_digest = bundle.digest.clone();
    let capsule = ContextCapsule {
        schema_version: format!("{}v{}", wire::CONTEXT_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::CONTEXT_IDENTITY, wire::VERSION),
        consumer: consumer.to_owned(),
        producer,
        producer_digest,
        api_version: bundle.api_version.clone(),
        budget,
        estimator: EstimatorPin {
            id: wire::ESTIMATOR_ID.to_owned(),
            recipe: wire::ESTIMATOR_RECIPE.to_owned(),
            version: wire::ESTIMATOR_VERSION.to_owned(),
        },
        fits,
        required_cost,
        selected_cost,
        exports: facts,
        dependencies: required_dependencies,
        omissions,
        gaps: {
            let mut gaps = gaps;
            gaps.sort();
            gaps.dedup();
            gaps
        },
        digest: String::new(),
    };
    let digest = wire::digest(wire::DIGEST_DOMAIN_CONTEXT, &wire::canonical(&capsule));
    Ok(ContextCapsule { digest, ..capsule })
}

/// Every semantic reference one export carries.
fn export_references(export: &PublicExport) -> Vec<String> {
    let mut references: Vec<String> = Vec::new();
    let push_type = |expression: &super::wire::TypeExpr, references: &mut Vec<String>| {
        references.push(expression.leaf().to_owned());
    };
    for field in export
        .fields
        .iter()
        .chain(&export.input)
        .chain(&export.payload)
    {
        push_type(&field.r#type, &mut references);
    }
    for read in &export.reads {
        references.push(read.clone());
    }
    for obligation in &export.effects {
        references.push(obligation.entity.clone());
        references.extend(obligation.emits.iter().cloned());
    }
    if let Some(returns) = &export.returns {
        push_type(returns, &mut references);
    }
    if let Some(entity) = &export.entity {
        references.push(entity.clone());
    }
    references.extend(export.emits.iter().cloned());
    references.extend(export.operations.iter().cloned());
    if let Some(invokes) = &export.invokes {
        references.push(invokes.clone());
    }
    references
}

/// The typed error exports of one export: error-kind exports carry their
/// own identity; operations carry nothing in the current closed surface
/// beyond their kind rows.
fn export_errors(export: &PublicExport) -> Vec<String> {
    if export.kind == "error" {
        return vec![export.export.clone()];
    }
    Vec::new()
}

/// The bounded canonical signature text of one export (no source, no
/// absolute paths, no private message text).
fn signature_text(export: &PublicExport) -> String {
    let mut text = String::new();
    text.push_str(export.kind.as_str());
    match export.kind.as_str() {
        "scalar" => {
            if let Some(base) = &export.base {
                text.push(' ');
                text.push_str(base);
            }
        }
        "enum" => {
            text.push_str(" { ");
            for value in &export.values {
                text.push_str(value);
                text.push(' ');
            }
            text.push('}');
        }
        "entity" | "value-object" => {
            text.push_str(" { ");
            for field in &export.fields {
                text.push_str(field.name.as_str());
                if field.required {
                    text.push('!');
                } else {
                    text.push('?');
                }
                text.push_str(": ");
                text.push_str(&type_text(&field.r#type));
                text.push(' ');
            }
            text.push('}');
            if !export.identity.is_empty() {
                text.push_str(" identity ");
                text.push_str(&export.identity.join(","));
            }
        }
        "command" => {
            text.push_str(" { ");
            for field in &export.input {
                text.push_str(field.name.as_str());
                if field.required {
                    text.push('!');
                } else {
                    text.push('?');
                }
                text.push_str(": ");
                text.push_str(&type_text(&field.r#type));
                text.push(' ');
            }
            text.push('}');
            for obligation in &export.effects {
                text.push_str(&format!(" -> {} {}", obligation.operation, obligation.entity));
            }
        }
        "query" => {
            if let Some(returns) = &export.returns {
                text.push_str(" -> ");
                text.push_str(&type_text(returns));
            }
        }
        "event" => {
            text.push_str(" { ");
            for field in &export.payload {
                text.push_str(field.name.as_str());
                if field.required {
                    text.push('!');
                } else {
                    text.push('?');
                }
                text.push_str(": ");
                text.push_str(&type_text(&field.r#type));
                text.push(' ');
            }
            text.push('}');
        }
        "error" => {
            if let Some(code) = &export.code {
                text.push(' ');
                text.push_str(code);
            }
            text.push_str(" { ");
            for field in &export.payload {
                text.push_str(field.name.as_str());
                if field.required {
                    text.push('!');
                } else {
                    text.push('?');
                }
                text.push_str(": ");
                text.push_str(&type_text(&field.r#type));
                text.push(' ');
            }
            text.push('}');
        }
        _ => {}
    }
    text
}

fn type_text(expression: &super::wire::TypeExpr) -> String {
    match expression {
        super::wire::TypeExpr::Ref { r#ref } => r#ref.clone(),
        super::wire::TypeExpr::List { list } => format!("list<{}>", type_text(list)),
        super::wire::TypeExpr::Optional { optional } => {
            format!("{}?", type_text(optional))
        }
    }
}
