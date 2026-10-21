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
use super::validate::{boundary_verdict, is_boundary_rule};
use super::version::is_digest;
use super::wire::{
    self, EffectObligation, ModulePolicy, ProducerPin, PublicBundle, PublicDependency,
    PublicExport, PublicField, PublicPins, TypeExpr,
};
use crate::diagnostics::DiagnosticSet;
use crate::error_contract::registry::ErrorRegistry;
use crate::error_contract::ErrorId;
use crate::ir::{Compilation, Definition, TypeRef};
use crate::result::Status;
use std::collections::BTreeMap;

/// Derive the certified public bundle of one module (r3 review finding
/// 2/codex: the certification lives in the core public API, never only in
/// the CLI). The unfiltered boundary check must report no exact semantic
/// violation for the module *or for any transitive public dependency of
/// its certified closure*, the required operation-error/query owner
/// evidence must be complete, and only then is the portable bundle
/// emitted. An incomplete or illegal closure refuses even in advisory
/// mode: no uncertified bundle is ever produced.
pub fn export_public(
    resolved: &Resolved,
    compilation: &Compilation,
    module: &str,
) -> Result<PublicBundle, DiagnosticSet> {
    // The unfiltered boundary check owns the whole project; the certified
    // set refuses when the module or any transitive public dependency of
    // its closure owns a boundary violation, with the exact findings.
    let verdict = boundary_verdict(resolved, compilation, false, None)?;
    let mut session = ExportSession::new(resolved, compilation);
    // The certified closure: every transitive public dependency pin, from
    // the edges the session records even when a dep bundle itself fails.
    let scope = certified_scope(&mut session, module);
    let scoped: Vec<_> = verdict
        .evidence
        .iter()
        .filter(|finding| {
            is_boundary_rule(finding.rule)
                && finding
                    .consumer
                    .as_deref()
                    .or(finding.producer.as_deref())
                    .is_some_and(|owner| scope.contains(owner))
        })
        .cloned()
        .collect();
    if !scoped.is_empty() {
        return Err(super::diagnostic::set(&scoped, Status::Invalid)?);
    }
    // Required owner evidence (r3 review finding 14/codex): the declared
    // operation-error/query facts of every module in the certified scope;
    // their absence is a required evidence gap and refuses certification.
    let owner_gaps: Vec<_> = verdict
        .evidence
        .iter()
        .filter(|finding| {
            finding.rule == "module-api.evidence-incomplete"
                && finding.detail == "owner-evidence-missing"
                && scope.contains(finding.subject.as_str())
        })
        .cloned()
        .collect();
    if !owner_gaps.is_empty() {
        return Err(super::diagnostic::set(&owner_gaps, Status::Unavailable)?);
    }
    match session.bundle_checked(module) {
        Ok(bundle) => Ok(bundle),
        Err(ExportFailure::Invalid(set)) => Err(set),
        Err(ExportFailure::Cycle(path)) => {
            // A real cycle refuses with the registered cycle rule and its
            // exact path (r3 review finding 3), never a bare input-invalid
            // set that discards the witness.
            let mut finding = super::diagnostic::Evidence::new(
                "module-api.dependency-cycle",
                module,
                "public-dependency-scc",
            );
            finding.path = path;
            Err(super::diagnostic::set(&[finding], Status::Invalid)?)
        }
    }
}

/// The transitive certified-closure scope of one module: the module plus
/// every public dependency reachable through the session's recorded
/// edges, walked with a bounded worklist even when bundles refuse.
fn certified_scope(
    session: &mut ExportSession,
    module: &str,
) -> std::collections::BTreeSet<String> {
    let mut scope = std::collections::BTreeSet::new();
    scope.insert(module.to_owned());
    let mut work: Vec<String> = vec![module.to_owned()];
    let mut seen: std::collections::BTreeSet<String> = work.iter().cloned().collect();
    while let Some(current) = work.pop() {
        // Best-effort build: it records the module's dependency edges even
        // when the bundle ultimately refuses.
        let _ = session.bundle(&current);
        for dependency in session.deps_of(&current) {
            if seen.insert(dependency.clone()) {
                scope.insert(dependency.clone());
                work.push(dependency);
            }
        }
    }
    scope
}

/// One export failure: a typed invalid set or a real public dependency
/// cycle with its exact active path.
pub enum ExportFailure {
    Invalid(DiagnosticSet),
    Cycle(Vec<String>),
}

/// One memoized export session over one compilation plus attachment
/// (issue #113 review fix: the public export is linear, never exponential
/// and never stack-bound). Each module's bundle is computed at most once,
/// public dependency digests resolve through an explicit worklist with an
/// active-path cycle check, and the compilation digest is canonicalized
/// exactly once per session. Adversarial module graphs therefore hit the
/// recorded node and depth bounds instead of the process stack.
pub struct ExportSession<'a> {
    resolved: &'a Resolved,
    compilation: &'a Compilation,
    bundles: BTreeMap<String, PublicBundle>,
    /// The active dependency path: a real cycle witness, not a memo hit.
    active: Vec<String>,
    ir_digest: String,
    /// Dependency edges recorded even for modules whose own bundle later
    /// fails, so certified-closure scope walks stay complete.
    deps_seen: BTreeMap<String, Vec<String>>,
}

impl<'a> ExportSession<'a> {
    pub fn new(resolved: &'a Resolved, compilation: &'a Compilation) -> Self {
        let ir_digest = format!(
            "sha256:{}",
            crate::digest::sha256_hex(compilation.project.to_canonical_json().as_bytes())
        );
        Self {
            resolved,
            compilation,
            bundles: BTreeMap::new(),
            active: Vec::new(),
            ir_digest,
            deps_seen: BTreeMap::new(),
        }
    }

    /// The recorded public dependency edges of one module (present even
    /// when the module's bundle ultimately refuses).
    pub fn deps_of(&self, module: &str) -> Vec<String> {
        self.deps_seen.get(module).cloned().unwrap_or_default()
    }

    /// The certified bundle of one module, memoized across the session.
    /// A public dependency cycle refuses with the exact active path.
    pub fn bundle(&mut self, module: &str) -> Result<PublicBundle, DiagnosticSet> {
        match self.bundle_checked(module) {
            Ok(bundle) => Ok(bundle),
            Err(ExportFailure::Cycle(_)) => Err(invalid("public-dependency-cycle")),
            Err(ExportFailure::Invalid(set)) => Err(set),
        }
    }

    /// The checked bundle with the typed cycle failure exposed.
    pub fn bundle_checked(&mut self, module: &str) -> Result<PublicBundle, ExportFailure> {
        if let Some(bundle) = self.bundles.get(module) {
            return Ok(bundle.clone());
        }
        if self.active.iter().any(|entered| entered == module) {
            let mut path = self.active.clone();
            path.push(module.to_owned());
            return Err(ExportFailure::Cycle(path));
        }
        if self.active.len() > wire::MAX_DEPENDENCY_NESTING
            || self.bundles.len() > wire::MAX_MODULES
        {
            return Err(ExportFailure::Invalid(invalid("public-closure-bound")));
        }
        self.active.push(module.to_owned());
        let built = self.build(module);
        self.active.pop();
        built
    }

    /// The active public dependency path (a cycle witness), exposed to the
    /// boundary check so the reported cycle is the real one.
    pub fn active_path(&self) -> Vec<String> {
        self.active.clone()
    }

    fn build(&mut self, module: &str) -> Result<PublicBundle, ExportFailure> {
        let Some(policy) = self.resolved.modules.get(module) else {
            return Err(ExportFailure::Invalid(invalid("unknown-module")));
        };
        let compilation = self.compilation;
        let resolved = self.resolved;
        let project = &compilation.project;
        let by_id: BTreeMap<&str, &Definition> = project
            .definitions
            .iter()
            .map(|definition| (definition.id().as_str(), definition))
            .collect();
        let exported: std::collections::BTreeSet<String> = policy
            .exports
            .iter()
            .map(|entry| entry.export.clone())
            .collect();
        let mut exports: Vec<PublicExport> = Vec::new();
        let mut foreign: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in &policy.exports {
            if entry.kind == "error" {
                exports.push(
                    error_export(&entry.export, &resolved.errors)
                        .map_err(ExportFailure::Invalid)?,
                );
                continue;
            }
            let Some(definition) = by_id.get(entry.export.as_str()) else {
                return Err(ExportFailure::Invalid(invalid("unknown-export")));
            };
            project_export(
                definition,
                &by_id,
                module,
                &exported,
                &mut exports,
                &mut foreign,
            )
            .map_err(ExportFailure::Invalid)?;
        }
        // The operation-error and query owner facts (r3 review finding
        // 14/codex): the declared attachment seam wins; the embedded #62
        // registry binding is the fallback owner for operations it covers.
        for export in &mut exports {
            if matches!(export.kind.as_str(), "command" | "query") {
                export.errors = operation_errors_of(policy, &export.export, &resolved.errors);
            }
            if export.kind == "query" {
                if let Some(facts) = policy
                    .query_facts
                    .iter()
                    .find(|facts| facts.query == export.export)
                {
                    export.cardinality = Some(facts.cardinality.clone());
                    export.consistency = Some(facts.consistency.clone());
                    export.pagination = Some(facts.pagination.clone());
                }
            }
            // Accepted alias evidence travels with the export (r3 review
            // finding 10/codex): preserved in the public surface.
            if let Some(entry) = policy
                .exports
                .iter()
                .find(|entry| entry.export == export.export)
            {
                export.alias_of = entry.alias_of.clone();
                export.since = entry.since.clone();
            }
        }
        // Public dependency pins: one exact pin per foreign module.
        let mut public_dependencies: Vec<PublicDependency> = Vec::new();
        for (foreign_module, symbols) in foreign {
            let Some(policy) = resolved.modules.get(&foreign_module) else {
                // A foreign type from an unattached module: no public pin can
                // exist, so the closure is incomplete by construction.
                return Err(ExportFailure::Invalid(invalid(
                    "unattached-public-dependency",
                )));
            };
            for symbol in &symbols {
                if !policy.exports.iter().any(|entry| entry.export == *symbol) {
                    // The foreign type is not exported: the bundle refuses.
                    return Err(ExportFailure::Invalid(invalid("foreign-type-not-exported")));
                }
            }
            public_dependencies.push(PublicDependency {
                module_id: foreign_module.to_owned(),
                digest: String::new(), // filled after recursion below
                api_version: policy.api_version.clone(),
                exports: symbols,
            });
        }
        // Record the edges before any digest resolution so a failure deep
        // in the closure still exposes the dependency graph to scope walks.
        self.deps_seen.insert(
            module.to_owned(),
            public_dependencies
                .iter()
                .map(|dependency| dependency.module_id.clone())
                .collect(),
        );
        // The producer's own digest: canonical public semantics.
        let api_version = policy.api_version.clone();
        let pins = PublicPins {
            model_version: wire::MODEL_VERSION.to_owned(),
            ir_digest: self.ir_digest.clone(),
            source_revision: super::wire::State::Unknown,
        };
        let producer = ProducerPin {
            project: resolved.project.clone(),
            module: module.to_owned(),
        };
        // Resolve public dependency digests through the memoized session: an
        // explicit active path bounds the depth, every cone is computed
        // once, and a real cycle propagates as the typed cycle failure
        // with its exact path (r3 review finding 3) instead of collapsing
        // into an input-invalid set.
        for dependency in &mut public_dependencies {
            dependency.digest = self.bundle_checked(&dependency.module_id)?.digest;
        }
        // Canonical set order (r3 review finding 4/codex): the emitted
        // bundle and its digest are order-insensitive for every set-like
        // collection — exports, dependency pins and their export sets,
        // ports, capabilities, and deprecations. An allowlist reorder is
        // not a public semantic change.
        exports.sort_by(|left, right| left.export.cmp(&right.export));
        public_dependencies.sort_by(|left, right| left.module_id.cmp(&right.module_id));
        for dependency in &mut public_dependencies {
            dependency.exports.sort();
            dependency.exports.dedup();
        }
        let mut ports = policy.ports.clone();
        ports.sort_by(|left, right| left.port.cmp(&right.port));
        for port in &mut ports {
            port.operations.sort();
            port.operations.dedup();
        }
        let mut capabilities = policy.capabilities.clone();
        capabilities.sort_by(|left, right| left.capability.cmp(&right.capability));
        let mut deprecations: Vec<_> = policy
            .deprecations
            .iter()
            .filter(|entry| {
                policy
                    .exports
                    .iter()
                    .any(|export| export.export == entry.export)
            })
            .cloned()
            .collect();
        deprecations.sort_by(|left, right| left.export.cmp(&right.export));
        let partial = PublicBundle {
            schema_version: format!("{}/v{}", wire::PUBLIC_SCHEMA, wire::VERSION),
            identity: format!("{}@{}", wire::PUBLIC_IDENTITY, wire::VERSION),
            producer,
            api_version: api_version.clone(),
            digest: String::new(),
            pins,
            exports,
            public_dependencies,
            ports,
            capabilities,
            deprecations,
        };
        // The digest covers canonical public semantics only (r3 review
        // finding 4/codex): it excludes its own digest member, the integer
        // definition revision (a private edit that only bumps the revision
        // preserves the public identity), and the whole freshness pin
        // block — `irDigest` is whole-project provenance and never part of
        // the public identity. Pins stay in the bundle for freshness
        // checks but outside the digest domain.
        let mut digest_projection = partial.clone();
        digest_projection.digest = String::new();
        digest_projection.pins = PublicPins {
            model_version: String::new(),
            ir_digest: String::new(),
            source_revision: super::wire::State::Unknown,
        };
        for export in &mut digest_projection.exports {
            export.version = 0;
        }
        let digest = wire::digest(
            wire::DIGEST_DOMAIN_PUBLIC,
            &wire::canonical(&digest_projection),
        );
        let bundle = PublicBundle { digest, ..partial };
        // The outgoing public projection is certified against the same
        // published grammar as incoming bundles (r4 review finding
        // 2/codex): the typed admission and semantic closure invariants
        // run on the exact emitted object, so export can never certify a
        // schema-invalid bundle even from a malformed-but-decodable
        // attachment.
        super::admission::admit_bundle(&bundle).map_err(ExportFailure::Invalid)?;
        super::admission::admit_bundle_semantics(&bundle).map_err(ExportFailure::Invalid)?;
        self.bundles.insert(module.to_owned(), bundle.clone());
        Ok(bundle)
    }
}

/// The public error union of one exported operation: the declared
/// attachment seam wins; the embedded #62 registry binding is the
/// fallback owner. Empty means no owner declared a union — the boundary
/// walk records the required evidence gap (r3 review finding 14/codex).
fn operation_errors_of(
    policy: &ModulePolicy,
    operation: &str,
    registry: &ErrorRegistry,
) -> Vec<String> {
    if let Some(declared) = policy
        .operation_errors
        .iter()
        .find(|union| union.operation == operation)
    {
        let mut errors = declared.errors.clone();
        errors.sort();
        errors.dedup();
        return errors;
    }
    if let Some(id) = ErrorId::new(operation) {
        if let Some(binding) = registry.binding(&id) {
            return binding
                .errors()
                .members()
                .iter()
                .map(|member| member.id().as_str().to_owned())
                .collect();
        }
    }
    Vec::new()
}

/// Project one owned definition into the closed export surface.
fn project_export(
    definition: &Definition,
    by_id: &BTreeMap<&str, &Definition>,
    module: &str,
    exported: &std::collections::BTreeSet<String>,
    exports: &mut Vec<PublicExport>,
    foreign: &mut BTreeMap<String, Vec<String>>,
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
            if enum_def.values.iter().any(|v| v.wire_value.is_some()) {
                return Err(super::diagnostic::invalid("enum-wire-contract-unsupported"));
            }
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
            let fields = project_fields(&value_object.fields, by_id, module, exported, foreign)?;
            push_type("value-object", None, vec![], fields, vec![])?;
        }
        Definition::Entity(entity) => {
            let fields = project_fields(&entity.fields, by_id, module, exported, foreign)?;
            let identity = entity
                .identity
                .iter()
                .map(|field| field.as_str().to_owned())
                .collect();
            push_type("entity", None, vec![], fields, identity)?;
        }
        Definition::Command(command) => {
            let input = project_fields(&command.input, by_id, module, exported, foreign)?;
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
            // affected entity and its events are admitted by identity, so
            // consumers can observe the obligation without the aggregate
            // shape ever entering the bundle.
            for obligation in &effects {
                record_public_ref(by_id, module, exported, &obligation.entity, foreign, false)?;
                for event in &obligation.emits {
                    record_public_ref(by_id, module, exported, event, foreign, false)?;
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
            if !query.input.is_empty() {
                return Err(invalid("query-input-contract-unsupported"));
            }
            let returns = query
                .returns
                .as_ref()
                .map(|returns| project_type(returns, by_id, module, exported, foreign))
                .transpose()?
                .map(Box::new)
                .map(|boxed| *boxed);
            let reads = query
                .reads
                .iter()
                .map(|read| {
                    record_public_ref(by_id, module, exported, read.as_str(), foreign, false)?;
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
            let payload = project_fields(&event.payload, by_id, module, exported, foreign)?;
            exports.push(PublicExport {
                export: id.clone(),
                kind: "event".to_owned(),
                version,
                payload,
                ..PublicExport::empty(&id, "event")
            });
        }
        Definition::Effect(effect) => {
            record_public_ref(
                by_id,
                module,
                exported,
                effect.entity.as_str(),
                foreign,
                false,
            )?;
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
                        record_public_ref(by_id, module, exported, event.as_str(), foreign, false)?;
                        Ok(event.as_str().to_owned())
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()?,
                ..PublicExport::empty(&id, "effect")
            });
        }
        Definition::Endpoint(endpoint) => {
            let invokes = endpoint.invokes.as_str().to_owned();
            record_public_ref(by_id, module, exported, &invokes, foreign, false)?;
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
                    record_public_ref(by_id, module, exported, operation.as_str(), foreign, false)?;
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
    exported: &std::collections::BTreeSet<String>,
    foreign: &mut BTreeMap<String, Vec<String>>,
) -> Result<Vec<PublicField>, DiagnosticSet> {
    fields
        .iter()
        .map(|field| {
            if field.default.is_some() {
                return Err(super::diagnostic::invalid(
                    "field-default-contract-unsupported",
                ));
            }
            Ok(PublicField {
                name: field.name.as_str().to_owned(),
                r#type: project_type(&field.r#type, by_id, module, exported, foreign)?,
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
    exported: &std::collections::BTreeSet<String>,
    foreign: &mut BTreeMap<String, Vec<String>>,
) -> Result<TypeExpr, DiagnosticSet> {
    match type_ref {
        TypeRef::Ref(symbol) => {
            record_public_ref(by_id, module, exported, symbol.as_str(), foreign, true)?;
            Ok(TypeExpr::Ref {
                r#ref: symbol.as_str().to_owned(),
            })
        }
        TypeRef::List(inner) => Ok(TypeExpr::List {
            list: Box::new(project_type(inner, by_id, module, exported, foreign)?),
        }),
        TypeRef::Optional(inner) => Ok(TypeExpr::Optional {
            optional: Box::new(project_type(inner, by_id, module, exported, foreign)?),
        }),
    }
}

/// Record one referenced symbol. A public shape reference (`shape: true`:
/// field, input, payload, and result types) owned by the producing module
/// must be exported — a private symbol may never become a portable public
/// contract member. Identity-only references (reads, effect obligations,
/// invocations, policy operations) are admitted without export because
/// they never carry the referenced shape into the bundle.
fn record_public_ref(
    by_id: &BTreeMap<&str, &Definition>,
    module: &str,
    exported: &std::collections::BTreeSet<String>,
    symbol: &str,
    foreign: &mut BTreeMap<String, Vec<String>>,
    shape: bool,
) -> Result<(), DiagnosticSet> {
    let Some(owner) = symbol.split_once('.').map(|(owner, _)| owner) else {
        return Err(invalid("symbol-id"));
    };
    if owner == module {
        if shape && !exported.contains(symbol) {
            // The certified bundle refuses even in advisory mode; the
            // boundary check reports the exact LEK-MODULE-005 path.
            return Err(invalid("nonexported-public-closure"));
        }
        return Ok(());
    }
    let Some(definition) = by_id.get(symbol) else {
        return Err(invalid("unresolved-reference"));
    };
    // A target-specific type can never enter the portable closure silently;
    // the boundary validator reports the exact path (LEK-MODULE-006) while
    // the dependency pin still exists so consumers see the obligation.
    let _ = definition;
    foreign
        .entry(owner.to_owned())
        .or_default()
        .push(symbol.to_owned());
    Ok(())
}

/// Project one typed #62 error export: public stable code, retry policy,
/// and the public payload fields only; private messages and debug payload
/// fields never cross the boundary.
fn error_export(id: &str, registry: &ErrorRegistry) -> Result<PublicExport, DiagnosticSet> {
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
/// The full closed wire contract is enforced before canonicalization and
/// semantic use (r3 review finding 1): required member presence,
/// discriminated export shapes, closed vocabularies, bounds, uniqueness,
/// IDs, versions, the digest domain, and the reference/policy closure —
/// a recomputed checksum never admits a schema-invalid document.
pub fn parse_bundle(bytes: &[u8]) -> Result<PublicBundle, DiagnosticSet> {
    let raw: serde_json::Value = wire::decode(bytes, invalid)?;
    super::admission::admit_bundle_presence(&raw)?;
    let bundle: PublicBundle = serde_json::from_value(raw).map_err(|_| invalid("closed-shape"))?;
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
    super::admission::admit_bundle(&bundle)?;
    // The digest domain excludes its own digest member, the freshness
    // pins, and the integer definition revisions, and is canonicalized
    // over the set order — the exact projection of the export seam.
    let expected = wire::digest(
        wire::DIGEST_DOMAIN_PUBLIC,
        &wire::canonical(&super::admission::digest_projection(&bundle)),
    );
    if expected != bundle.digest {
        return Err(invalid("bundle-digest-mismatch"));
    }
    super::admission::admit_bundle_semantics(&bundle)?;
    Ok(bundle)
}
