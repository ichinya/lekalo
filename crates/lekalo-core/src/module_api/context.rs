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
    // The complete required local closure (r3 review finding 3/codex): a
    // fitting capsule materializes and budgets every local public shape
    // the selected exports' signatures actually reach, not just the
    // selected roots — a selected query's result DTO and its scalar
    // domains are required facts of the same task.
    let mut closure: Vec<&PublicExport> = selected.clone();
    let mut queued: std::collections::BTreeSet<&str> = selected
        .iter()
        .map(|export| export.export.as_str())
        .collect();
    let mut work: Vec<&PublicExport> = selected.clone();
    while let Some(export) = work.pop() {
        for leaf in shape_leaves(export) {
            if let Some(local) = by_id.get(leaf.as_str()) {
                if queued.insert(local.export.as_str()) {
                    closure.push(local);
                    work.push(local);
                }
            }
        }
    }
    closure.sort_by(|left, right| left.export.cmp(&right.export));
    // Required public dependencies of the closure: the transitive
    // reference set (shapes and identity obligations) decides which
    // pins are required.
    let closure_references: std::collections::BTreeSet<String> = closure
        .iter()
        .flat_map(|export| export_references(export))
        .collect();
    let mut required_dependencies: Vec<PublicDependency> = Vec::new();
    for dependency in &bundle.public_dependencies {
        if dependency
            .exports
            .iter()
            .any(|symbol| closure_references.contains(symbol))
        {
            required_dependencies.push((*dependency).clone());
            // A retained dependency pin without the foreign signature is
            // an explicit required gap (r3 review finding 3/codex): the
            // capsule never claims a complete fitting closure while a
            // referenced dependency shape is unavailable.
            gaps.push("foreign-signature-not-materialized".to_owned());
            for symbol in &dependency.exports {
                if closure_references.contains(symbol) {
                    omissions.push(Omission {
                        subject: symbol.clone(),
                        reason: "dependency-signature-not-in-bundle".to_owned(),
                    });
                }
            }
        }
    }
    required_dependencies.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    omissions.sort_by(|left, right| left.subject.cmp(&right.subject));
    omissions.dedup_by(|left, right| left.subject == right.subject);
    // Required facts: signatures, typed errors, effect obligations,
    // capability and version/deprecation obligations, and the query
    // owner facts (r3 review finding 14/codex: the capsule never claims
    // known-empty operation errors from absence).
    let mut facts = Vec::new();
    for export in &closure {
        let signature = signature_text(export);
        let errors = export_errors(export);
        let effects = export
            .effects
            .iter()
            .map(|obligation| {
                format!(
                    "{}|{}|{}",
                    obligation.entity,
                    obligation.operation,
                    obligation.emits.len()
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
    let selected_cost = required_cost; // Protected required facts can never be dropped for fit: an
                                       // over-budget capsule stays explicit.
    let fits = selected_cost <= budget;
    if !fits {
        gaps.push("budget-exceeded".to_owned());
    }
    // Deterministic capsule digest over the exact projection.
    let producer = bundle.producer.clone();
    let producer_digest = bundle.digest.clone();
    let capsule = ContextCapsule {
        schema_version: format!("{}/v{}", wire::CONTEXT_SCHEMA, wire::VERSION),
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

/// Every semantic reference one export carries: the shared reference
/// collector for lock dependency extraction, consumer-impact propagation,
/// and capsule planning.
pub(crate) fn export_references(export: &PublicExport) -> Vec<String> {
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

/// The public *shape* leaves of one export (field/input/payload/result
/// types only): the materialization closure of the capsule. Identity
/// obligations — reads, effect entities, governed operations,
/// invocations — stay identity-only and never drag aggregate shapes
/// into the capsule (r3 review finding 3/codex).
fn shape_leaves(export: &PublicExport) -> Vec<String> {
    let mut leaves = Vec::new();
    let push_type = |expression: &super::wire::TypeExpr, leaves: &mut Vec<String>| {
        leaves.push(expression.leaf().to_owned());
    };
    for field in export
        .fields
        .iter()
        .chain(&export.input)
        .chain(&export.payload)
    {
        push_type(&field.r#type, &mut leaves);
    }
    if let Some(returns) = &export.returns {
        push_type(returns, &mut leaves);
    }
    leaves
}

/// The typed error facts of one export: error-kind exports carry their
/// own identity; exported commands and queries carry their declared
/// public error union (r3 review finding 14/codex — never a silently
/// known-empty union).
fn export_errors(export: &PublicExport) -> Vec<String> {
    if export.kind == "error" {
        return vec![export.export.clone()];
    }
    if matches!(export.kind.as_str(), "command" | "query") {
        return export.errors.clone();
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
                text.push_str(&format!(
                    " -> {} {}",
                    obligation.operation, obligation.entity
                ));
            }
        }
        "query" => {
            if let Some(returns) = &export.returns {
                text.push_str(" -> ");
                text.push_str(&type_text(returns));
            }
            // The public query owner facts ride the signature (r3 review
            // finding 14/codex).
            if export.cardinality.is_some() {
                text.push_str(&format!(
                    " cardinality {} consistency {} pagination {}",
                    export.cardinality.as_deref().unwrap_or_default(),
                    export.consistency.as_deref().unwrap_or_default(),
                    export.pagination.as_deref().unwrap_or_default()
                ));
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
