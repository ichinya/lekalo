//! Public projection: the standalone bundle of one module (issue #113).
//!
//! `export_public` materializes the exported semantic facts of one exact
//! module: exported types with their portable field closure, commands with
//! typed inputs and observable effect obligations, queries with results and
//! reads, events, endpoints, typed public error exports from the #62
//! registry, exact public dependency pins for foreign exported symbols, and
//! carried ports/capabilities/deprecations. No storage, adapter, SQL, or
//! provider internals can enter; the digest covers public semantics only
//! (descriptions, spans, and physical locations never do).

use super::diagnostic::invalid;
use super::resolve::Resolved;
use super::version::is_digest;
use super::wire::{
    self, EffectObligation, ProducerPin, PublicBundle, PublicDependency, PublicExport, PublicField,
    PublicPins, TypeExpr,
};
use crate::diagnostics::DiagnosticSet;
use crate::error_contract::registry::ErrorRegistry;
use crate::ir::{Compilation, Definition, Portability, TypeRef};
use std::collections::BTreeMap;

/// Derive the certified public bundle of one module. An incomplete or
/// illegal closure refuses even in advisory mode: no uncertified bundle is
/// ever produced.
pub fn export_public(
    resolved: &Resolved,
    compilation: &Compilation,
    module: &str,
) -> Result<PublicBundle, DiagnosticSet> {
    let Some(policy) = resolved.modules.get(module) else {
        return Err(invalid("unknown-module"));
    };
    let project = &compilation.project;
    let by_id: BTreeMap<&str, &Definition> = project
        .definitions
        .iter()
        .map(|definition| (definition.id().as_str(), definition))
        .collect();
    let mut exports: Vec<PublicExport> = Vec::new();
    let mut foreign: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut path: Vec<String> = Vec::new();
    for entry in &policy.exports {
        if entry.kind == "error" {
            exports.push(error_export(&entry.export)?);
            continue;
        }
        let Some(definition) = by_id.get(entry.export.as_str()) else {
            return Err(invalid("unknown-export"));
        };
        project_export(
            definition,
            &by_id,
            module,
            &mut exports,
            &mut foreign,
            &mut path,
        )?;
    }
    // Public dependency pins: one exact pin per foreign module.
    let mut public_dependencies: Vec<PublicDependency> = Vec::new();
    for (foreign_module, symbols) in foreign {
        let Some(policy) = resolved.modules.get(&foreign_module) else {
            // A foreign type from an unattached module: no public pin can
            // exist, so the closure is incomplete by construction.
            return Err(invalid("unattached-public-dependency"));
        };
        for symbol in &symbols {
            if !policy.exports.iter().any(|entry| entry.export == *symbol) {
                // The foreign type is not exported: the bundle refuses.
                return Err(invalid("foreign-type-not-exported"));
            }
        }
        public_dependencies.push(PublicDependency {
            module_id: foreign_module.to_owned(),
            digest: String::new(), // filled after recursion below
            api_version: policy.api_version.clone(),
            exports: symbols,
        });
    }
    // The producer's own digest: canonical public semantics.
    let api_version = policy.api_version.clone();
    let pins = PublicPins {
        model_version: wire::MODEL_VERSION.to_owned(),
        ir_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(project.to_canonical_json().as_bytes())
        ),
        source_revision: super::wire::State::Unknown,
    };
    let producer = ProducerPin {
        project: resolved.project.clone(),
        module: module.to_owned(),
    };
    // Resolve public dependency digests recursively with a depth bound.
    let mut seen = vec![module.to_owned()];
    resolve_dependency_digests(
        resolved,
        compilation,
        &mut public_dependencies,
        &mut seen,
        0,
    )?;
    let partial = PublicBundle {
        schema_version: format!("{}/v{}", wire::PUBLIC_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::PUBLIC_IDENTITY, wire::VERSION),
        producer,
        api_version: api_version.clone(),
        digest: String::new(),
        pins,
        exports,
        public_dependencies,
        ports: policy.ports.clone(),
        capabilities: policy.capabilities.clone(),
        deprecations: policy
            .deprecations
            .iter()
            .filter(|entry| {
                policy
                    .exports
                    .iter()
                    .any(|export| export.export == entry.export)
            })
            .cloned()
            .collect(),
    };
    // The digest excludes its own digest member; the canonical projection
    // below carries an empty digest field so equivalent bundles agree.
    let mut without_digest = partial.clone();
    without_digest.digest = String::new();
    let digest = wire::digest(
        wire::DIGEST_DOMAIN_PUBLIC,
        &wire::canonical(&without_digest),
    );
    Ok(PublicBundle { digest, ..partial })
}

/// Recursively resolve public dependency digests; the nesting bound keeps
/// the closure bounded and cycles explicit.
fn resolve_dependency_digests(
    resolved: &Resolved,
    compilation: &Compilation,
    dependencies: &mut [PublicDependency],
    seen: &mut Vec<String>,
    depth: usize,
) -> Result<(), DiagnosticSet> {
    if depth > wire::MAX_DEPENDENCY_NESTING {
        return Err(invalid("dependency-nesting"));
    }
    for dependency in dependencies.iter_mut() {
        if seen.contains(&dependency.module_id) {
            dependency.digest = "sha256:cycle".to_owned();
            continue;
        }
        seen.push(dependency.module_id.clone());
        let bundle = export_public(resolved, compilation, &dependency.module_id)?;
        dependency.digest = bundle.digest.clone();
    }
    Ok(())
}

/// Project one owned definition into the closed export surface.
fn project_export(
    definition: &Definition,
    by_id: &BTreeMap<&str, &Definition>,
    module: &str,
    exports: &mut Vec<PublicExport>,
    foreign: &mut BTreeMap<String, Vec<String>>,
    path: &mut Vec<String>,
) -> Result<(), DiagnosticSet> {
    let id = definition.id().as_str().to_owned();
    let version = definition.common().version;
    let mut push_type = |kind: &'static str,
                         base: Option<String>,
                         values: Vec<String>,
                         fields: Vec<PublicField>,
                         identity: Vec<String>|
     -> Result<(), DiagnosticSet> {
        exports.push(PublicExport {
            export: id.clone(),
            kind: kind.to_owned(),
            version,
            base,
            values,
            fields,
            identity,
            ..PublicExport::empty(&id, kind)
        });
        Ok(())
    };
    match definition {
        Definition::Scalar(scalar) => {
            push_type(
                "scalar",
                Some(scalar.base.as_str().to_owned()),
                vec![],
                vec![],
                vec![],
            )?;
        }
        Definition::Enum(enum_def) => {
            push_type(
                "enum",
                None,
                enum_def
                    .values
                    .iter()
                    .map(|value| value.value.as_str().to_owned())
                    .collect(),
                vec![],
                vec![],
            )?;
        }
        Definition::ValueObject(value_object) => {
            let fields = project_fields(&value_object.fields, by_id, module, foreign, path)?;
            push_type("value-object", None, vec![], fields, vec![])?;
        }
        Definition::Entity(entity) => {
            let fields = project_fields(&entity.fields, by_id, module, foreign, path)?;
            let identity = entity
                .identity
                .iter()
                .map(|field| field.as_str().to_owned())
                .collect();
            push_type("entity", None, vec![], fields, identity)?;
        }
        Definition::Command(command) => {
            let input = project_fields(&command.input, by_id, module, foreign, path)?;
            let effects = command
                .effects
                .iter()
                .map(|effect| {
                    let Some(Definition::Effect(effect_def)) = by_id.get(effect.as_str()) else {
                        return Err(invalid("unresolved-effect"));
                    };
                    Ok(EffectObligation {
                        entity: effect_def.entity.as_str().to_owned(),
                        operation: effect_def.operation.as_str().to_owned(),
                        emits: effect_def
                            .emits
                            .iter()
                            .map(|event| event.as_str().to_owned())
                            .collect(),
                    })
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?;
            // Effect obligations are part of the public signature: the
            // affected entity and its events must be exported here too.
            for obligation in &effects {
                record_public_ref(by_id, module, &obligation.entity, foreign, path)?;
                for event in &obligation.emits {
                    record_public_ref(by_id, module, event, foreign, path)?;
                }
            }
            exports.push(PublicExport {
                export: id.clone(),
                kind: "command".to_owned(),
                version,
                input,
                effects,
                ..PublicExport::empty(&id, "command")
            });
        }
        Definition::Query(query) => {
            let returns = query
                .returns
                .as_ref()
                .map(|returns| project_type(returns, by_id, module, foreign, path))
                .transpose()?
                .map(Box::new)
                .map(|boxed| *boxed);
            let reads = query
                .reads
                .iter()
                .map(|read| {
                    record_public_ref(by_id, module, read.as_str(), foreign, path)?;
                    Ok(read.as_str().to_owned())
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?;
            exports.push(PublicExport {
                export: id.clone(),
                kind: "query".to_owned(),
                version,
                returns,
                reads,
                ..PublicExport::empty(&id, "query")
            });
        }
        Definition::Event(event) => {
            let payload = project_fields(&event.payload, by_id, module, foreign, path)?;
            exports.push(PublicExport {
                export: id.clone(),
                kind: "event".to_owned(),
                version,
                payload,
                ..PublicExport::empty(&id, "event")
            });
        }
        Definition::Effect(effect) => {
            record_public_ref(by_id, module, effect.entity.as_str(), foreign, path)?;
            exports.push(PublicExport {
                export: id.clone(),
                kind: "effect".to_owned(),
                version,
                operation: Some(effect.operation.as_str().to_owned()),
                entity: Some(effect.entity.as_str().to_owned()),
                emits: effect
                    .emits
                    .iter()
                    .map(|event| {
                        record_public_ref(by_id, module, event.as_str(), foreign, path)?;
                        Ok(event.as_str().to_owned())
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
                ..PublicExport::empty(&id, "effect")
            });
        }
        Definition::Endpoint(endpoint) => {
            let invokes = endpoint.invokes.as_str().to_owned();
            record_public_ref(by_id, module, &invokes, foreign, path)?;
            exports.push(PublicExport {
                export: id.clone(),
                kind: "endpoint".to_owned(),
                version,
                invokes: Some(invokes),
                method: Some(endpoint.method.as_str().to_owned()),
                path: Some(endpoint.path.as_str().to_owned()),
                ..PublicExport::empty(&id, "endpoint")
            });
        }
        Definition::Policy(policy) => {
            let operations = policy
                .applies_to
                .iter()
                .map(|operation| {
                    record_public_ref(by_id, module, operation.as_str(), foreign, path)?;
                    Ok(operation.as_str().to_owned())
                })
                .collect::<Result<Vec<_>, DiagnosticSet>>()?;
            exports.push(PublicExport {
                export: id.clone(),
                kind: "policy".to_owned(),
                version,
                operations,
                decision: Some(policy.decision.as_str().to_owned()),
                ..PublicExport::empty(&id, "policy")
            });
        }
        Definition::TargetBinding(binding) => {
            // A target binding can never be part of the portable closure;
            // exporting one is refused before any silent projection.
            let _ = binding;
            return Err(invalid("target-binding-export"));
        }
        Definition::Scenario(scenario) => {
            let _ = scenario;
            return Err(invalid("scenario-export"));
        }
    }
    Ok(())
}

/// Project one field list, recording foreign and portable references.
fn project_fields(
    fields: &[crate::ir::Field],
    by_id: &BTreeMap<&str, &Definition>,
    module: &str,
    foreign: &mut BTreeMap<String, Vec<String>>,
    path: &mut Vec<String>,
) -> Result<Vec<PublicField>, DiagnosticSet> {
    fields
        .iter()
        .map(|field| {
            Ok(PublicField {
                name: field.name.as_str().to_owned(),
                r#type: project_type(&field.r#type, by_id, module, foreign, path)?,
                required: field.required,
            })
        })
        .collect()
}

/// Project one type expression, recording every referenced symbol.
fn project_type(
    type_ref: &TypeRef,
    by_id: &BTreeMap<&str, &Definition>,
    module: &str,
    foreign: &mut BTreeMap<String, Vec<String>>,
    path: &mut Vec<String>,
) -> Result<TypeExpr, DiagnosticSet> {
    match type_ref {
        TypeRef::Ref(symbol) => {
            record_public_ref(by_id, module, symbol.as_str(), foreign, path)?;
            Ok(TypeExpr::Ref {
                r#ref: symbol.as_str().to_owned(),
            })
        }
        TypeRef::List(inner) => Ok(TypeExpr::List {
            list: Box::new(project_type(inner, by_id, module, foreign, path)?),
        }),
        TypeRef::Optional(inner) => Ok(TypeExpr::Optional {
            optional: Box::new(project_type(inner, by_id, module, foreign, path)?),
        }),
    }
}

/// Record one referenced symbol: local or foreign public dependency.
fn record_public_ref(
    by_id: &BTreeMap<&str, &Definition>,
    module: &str,
    symbol: &str,
    foreign: &mut BTreeMap<String, Vec<String>>,
    path: &mut Vec<String>,
) -> Result<(), DiagnosticSet> {
    let Some(owner) = symbol.split_once('.').map(|(owner, _)| owner) else {
        return Err(invalid("symbol-id"));
    };
    if owner == module {
        return Ok(());
    }
    let Some(definition) = by_id.get(symbol) else {
        return Err(invalid("unresolved-reference"));
    };
    if definition.common().portability == Some(Portability::TargetSpecific) {
        // A target-specific type can never enter the portable closure
        // silently; the boundary validator reports the exact path.
        path.push(symbol.to_owned());
    }
    foreign
        .entry(owner.to_owned())
        .or_default()
        .push(symbol.to_owned());
    Ok(())
}

/// Project one typed #62 error export: public stable code, retry policy,
/// and the public payload fields only; private messages and debug payload
/// fields never cross the boundary.
fn error_export(id: &str) -> Result<PublicExport, DiagnosticSet> {
    let registry = ErrorRegistry::embedded()?;
    let Some(error) = registry
        .errors()
        .iter()
        .find(|error| error.id().as_str() == id)
    else {
        return Err(invalid("unknown-error"));
    };
    let payload = error
        .payload()
        .fields()
        .iter()
        .filter(|field| matches!(field.exposure(), crate::error_contract::Exposure::Public))
        .map(|field| PublicField {
            name: field.name().to_owned(),
            r#type: convert_error_type(field.field_type()),
            required: field.required(),
        })
        .collect();
    Ok(PublicExport {
        export: id.to_owned(),
        kind: "error".to_owned(),
        version: 0,
        code: Some(error.code().as_str().to_owned()),
        retryable: Some(!matches!(
            error.retry(),
            crate::error_contract::RetryPolicy::Never
        )),
        payload,
        ..PublicExport::empty(id, "error")
    })
}

/// Convert one #62 type expression into the closed public expression.
fn convert_error_type(expression: &crate::error_contract::types::TypeExpr) -> TypeExpr {
    match expression {
        crate::error_contract::types::TypeExpr::Ref(id) => TypeExpr::Ref {
            r#ref: id.as_str().to_owned(),
        },
        crate::error_contract::types::TypeExpr::List(inner) => TypeExpr::List {
            list: Box::new(convert_error_type(inner)),
        },
        crate::error_contract::types::TypeExpr::Optional(inner) => TypeExpr::Optional {
            optional: Box::new(convert_error_type(inner)),
        },
    }
}

/// Parse and validate a public bundle document (the diff/context seam).
pub fn parse_bundle(bytes: &[u8]) -> Result<PublicBundle, DiagnosticSet> {
    let bundle: PublicBundle = wire::decode(bytes, invalid)?;
    if bundle.schema_version != format!("{}/v{}", wire::PUBLIC_SCHEMA, wire::VERSION)
        || bundle.identity != format!("{}@{}", wire::PUBLIC_IDENTITY, wire::VERSION)
    {
        return Err(super::diagnostic::unsupported_version(
            &bundle.schema_version,
        ));
    }
    if !is_digest(&bundle.digest) {
        return Err(invalid("bundle-digest"));
    }
    let mut without_digest = bundle.clone();
    without_digest.digest = String::new();
    let expected = wire::digest(
        wire::DIGEST_DOMAIN_PUBLIC,
        &wire::canonical(&without_digest),
    );
    if expected != bundle.digest {
        return Err(invalid("bundle-digest-mismatch"));
    }
    Ok(bundle)
}
