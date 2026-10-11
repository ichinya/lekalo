//! Runtime admission of the closed wire contracts (issue #113 r3 review
//! finding 1): the published JSON schemas are the exact admission grammar
//! in both directions, so every input family enforces required member
//! presence (independently of nullable values), discriminated export
//! shapes, closed vocabularies, bounds, uniqueness, IDs, versions, and
//! reference/policy invariants before canonicalization and semantic use.
//! A checksum is never admission validation.

use super::diagnostic::invalid;
use super::wire::{self, PublicBundle, PublicDependency, PublicExport};
use crate::diagnostics::DiagnosticSet;

/// Whether `text` matches the closed `semanticId` grammar
/// (`^[A-Za-z0-9][A-Za-z0-9._:/#-]*$`, length 3..=192).
fn is_semantic_id(text: &str) -> bool {
    (3..=192).contains(&text.len())
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '#' | '-'))
}

/// Whether `text` matches the closed module id grammar
/// (`^[a-z][a-z0-9-]*$`, length 1..=192).
fn is_module_id(text: &str) -> bool {
    (1..=192).contains(&text.len())
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether `text` matches the closed field-name grammar
/// (`^[a-z][a-z0-9_]*$`, length 1..=192).
fn is_field_name(text: &str) -> bool {
    (1..=192).contains(&text.len())
        && text
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether every element of `items` is unique (schema `uniqueItems`).
fn is_unique(items: &[String]) -> bool {
    let mut sorted = items.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted.len() == items.len()
}

/// Require that every key of `keys` is present in the object (a present
/// null satisfies a nullable member; a missing member never does).
fn require(object: &serde_json::Value, keys: &[&str]) -> Result<(), DiagnosticSet> {
    let serde_json::Value::Object(map) = object else {
        return Err(invalid("json-shape"));
    };
    if keys.iter().any(|key| !map.contains_key(*key)) {
        return Err(invalid("required-member-missing"));
    }
    Ok(())
}

/// The per-kind closed member sets of the public export surface (the
/// discriminated `oneOf` branches of the published schema).
fn kind_members(kind: &str) -> Option<&'static [&'static str]> {
    Some(match kind {
        "scalar" | "enum" | "value-object" | "entity" => &[
            "export", "kind", "version", "base", "values", "fields", "identity", "aliasOf", "since",
        ],
        "command" => &[
            "export", "kind", "version", "input", "effects", "errors", "aliasOf", "since",
        ],
        "query" => &[
            "export",
            "kind",
            "version",
            "returns",
            "reads",
            "errors",
            "cardinality",
            "consistency",
            "pagination",
            "aliasOf",
            "since",
        ],
        "event" => &["export", "kind", "version", "payload", "aliasOf", "since"],
        "error" => &[
            "export",
            "kind",
            "version",
            "code",
            "retryable",
            "payload",
            "aliasOf",
            "since",
        ],
        "policy" | "effect" | "endpoint" => &[
            "export",
            "kind",
            "version",
            "operations",
            "invokes",
            "method",
            "path",
            "decision",
            "operation",
            "entity",
            "emits",
            "aliasOf",
            "since",
        ],
        _ => return None,
    })
}

/// The structural admission of one bundle's raw JSON: required member
/// presence at every level of the published closed schema.
pub(crate) fn admit_bundle_presence(raw: &serde_json::Value) -> Result<(), DiagnosticSet> {
    require(
        raw,
        &[
            "schemaVersion",
            "identity",
            "producer",
            "apiVersion",
            "digest",
            "pins",
            "exports",
            "publicDependencies",
            "ports",
            "capabilities",
            "deprecations",
        ],
    )?;
    let serde_json::Value::Object(map) = raw else {
        return Err(invalid("json-shape"));
    };
    require(&map["producer"], &["project", "module"])?;
    require(
        &map["pins"],
        &["modelVersion", "irDigest", "sourceRevision"],
    )?;
    if let Some(revision) = map["pins"]["sourceRevision"].as_object() {
        if revision.get("state").and_then(|state| state.as_str()) == Some("known") {
            require(&map["pins"]["sourceRevision"], &["state", "value"])?;
        }
    }
    for key in [
        "exports",
        "publicDependencies",
        "ports",
        "capabilities",
        "deprecations",
    ] {
        let Some(items) = map[key].as_array() else {
            continue;
        };
        for item in items {
            match key {
                "exports" => {
                    require(item, &["export", "kind", "version"])?;
                    let kind = item["kind"].as_str().unwrap_or_default();
                    let members = kind_members(kind).ok_or_else(|| invalid("export-kind"))?;
                    // The closed per-kind member set: a member outside the
                    // branch's properties is a schema violation even when
                    // the Rust struct could decode it.
                    if let Some(object) = item.as_object() {
                        for member in object.keys() {
                            if !members.contains(&member.as_str()) {
                                return Err(invalid("export-member"));
                            }
                        }
                    }
                    for field_key in ["fields", "input", "payload"] {
                        if let Some(fields) = item[field_key].as_array() {
                            for field in fields {
                                require(field, &["name", "type", "required"])?;
                            }
                        }
                    }
                    if let Some(effects) = item["effects"].as_array() {
                        for obligation in effects {
                            require(obligation, &["entity", "operation", "emits"])?;
                        }
                    }
                }
                "publicDependencies" => {
                    require(item, &["moduleId", "digest", "apiVersion", "exports"])?;
                }
                "ports" => require(item, &["port", "operations"])?,
                "capabilities" => require(item, &["capability", "obligation", "owner", "version"])?,
                _ => require(
                    item,
                    &["export", "introducedIn", "removedIn", "replacement", "hint"],
                )?,
            }
        }
    }
    Ok(())
}

/// The typed closed-grammar admission of one decoded bundle: kinds,
/// vocabularies, bounds, uniqueness, IDs, and versions.
pub(crate) fn admit_bundle(bundle: &PublicBundle) -> Result<(), DiagnosticSet> {
    if !is_module_id(&bundle.producer.module) {
        return Err(invalid("producer-module"));
    }
    if bundle.producer.project.is_empty() || bundle.producer.project.len() > 192 {
        return Err(invalid("producer-project"));
    }
    if let Some(version) = &bundle.api_version {
        if super::version::ApiVersion::parse(version).is_none() {
            return Err(invalid("api-version"));
        }
    }
    if !super::version::is_digest(&bundle.pins.ir_digest) {
        return Err(invalid("pins-ir-digest"));
    }
    if bundle.exports.len() > wire::MAX_EXPORTS_PER_MODULE
        || bundle.public_dependencies.len() > wire::MAX_DEPENDENCY_NESTING
        || bundle.ports.len() > wire::MAX_DEPENDENCY_NESTING
        || bundle.capabilities.len() > wire::MAX_DEPENDENCY_NESTING
        || bundle.deprecations.len() > 1024
    {
        return Err(invalid("bundle-bound"));
    }
    let mut export_ids: Vec<&str> = Vec::new();
    for export in &bundle.exports {
        admit_export(export, &bundle.producer.module)?;
        export_ids.push(export.export.as_str());
    }
    if !is_unique(
        &export_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
    ) {
        return Err(invalid("duplicate-export"));
    }
    let mut dependency_ids: Vec<&str> = Vec::new();
    for dependency in &bundle.public_dependencies {
        if !is_module_id(&dependency.module_id) {
            return Err(invalid("dependency-module"));
        }
        if !super::version::is_digest(&dependency.digest) {
            return Err(invalid("dependency-digest"));
        }
        if let Some(version) = &dependency.api_version {
            if super::version::ApiVersion::parse(version).is_none() {
                return Err(invalid("dependency-api-version"));
            }
        }
        if dependency.exports.is_empty()
            || dependency.exports.len() > wire::MAX_EXPORTS_PER_MODULE
            || !is_unique(&dependency.exports)
            || dependency
                .exports
                .iter()
                .any(|symbol| !is_semantic_id(symbol))
        {
            return Err(invalid("dependency-exports"));
        }
        dependency_ids.push(dependency.module_id.as_str());
    }
    if !is_unique(
        &dependency_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
    ) {
        return Err(invalid("duplicate-dependency"));
    }
    for port in &bundle.ports {
        if !is_semantic_id(&port.port)
            || port.operations.is_empty()
            || port.operations.len() > wire::MAX_EXPORTS_PER_MODULE
            || !is_unique(&port.operations)
            || port
                .operations
                .iter()
                .any(|operation| !is_semantic_id(operation))
        {
            return Err(invalid("port-shape"));
        }
    }
    for capability in &bundle.capabilities {
        let well_formed = (1..=128).contains(&capability.capability.len())
            && capability
                .capability
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_lowercase())
            && capability.capability.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
            })
            && matches!(capability.obligation.as_str(), "required" | "optional");
        if !well_formed {
            return Err(invalid("capability-shape"));
        }
        if let Some(version) = &capability.version {
            if super::version::ApiVersion::parse(version).is_none() {
                return Err(invalid("capability-version"));
            }
        }
    }
    for deprecation in &bundle.deprecations {
        let Some(introduced) = super::version::ApiVersion::parse(&deprecation.introduced_in) else {
            return Err(invalid("deprecation-introduced"));
        };
        if let Some(removed) = &deprecation.removed_in {
            let Some(removed) = super::version::ApiVersion::parse(removed) else {
                return Err(invalid("deprecation-window"));
            };
            if removed < introduced {
                return Err(invalid("deprecation-window"));
            }
        }
        if deprecation.hint.len() > 512 {
            return Err(invalid("deprecation-hint"));
        }
    }
    Ok(())
}

/// The closed grammar of one export row.
fn admit_export(export: &PublicExport, producer: &str) -> Result<(), DiagnosticSet> {
    if !is_semantic_id(&export.export) {
        return Err(invalid("export-id"));
    }
    // The export id is `module.symbol` and the symbol is owned by the
    // producer module.
    let Some((owner, _)) = export.export.split_once('.') else {
        return Err(invalid("export-id"));
    };
    if owner != producer {
        return Err(invalid("foreign-export"));
    }
    if kind_members(&export.kind).is_none() {
        return Err(invalid("export-kind"));
    }
    let fields: &[super::wire::PublicField] = match export.kind.as_str() {
        "command" => &export.input,
        "event" | "error" => &export.payload,
        "scalar" | "enum" | "value-object" | "entity" | "query" | "policy" | "effect"
        | "endpoint" => &export.fields,
        _ => &[],
    };
    if fields.len() > 1024 {
        return Err(invalid("field-bound"));
    }
    for field in fields {
        if !is_field_name(&field.name) {
            return Err(invalid("field-name"));
        }
    }
    if export.values.len() > 1024
        || !is_unique(&export.values)
        || export.values.iter().any(|value| {
            value.is_empty() || value.len() > 192 || value.chars().any(char::is_control)
        })
    {
        return Err(invalid("enum-values"));
    }
    if export.identity.len() > 64 || export.identity.iter().any(|id| !is_field_name(id)) {
        return Err(invalid("identity-members"));
    }
    if let Some(base) = &export.base {
        if !matches!(
            base.as_str(),
            "string" | "number" | "boolean" | "date" | "datetime" | "uuid" | "uri"
        ) {
            return Err(invalid("scalar-base"));
        }
    }
    if let Some(method) = &export.method {
        if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            return Err(invalid("endpoint-method"));
        }
    }
    if let Some(path) = &export.path {
        if !path.starts_with('/') || path.len() > 512 {
            return Err(invalid("endpoint-path"));
        }
    }
    if let Some(decision) = &export.decision {
        if !matches!(decision.as_str(), "allow" | "deny") {
            return Err(invalid("policy-decision"));
        }
    }
    if let Some(operation) = &export.operation {
        if !matches!(operation.as_str(), "create" | "update" | "delete") {
            return Err(invalid("effect-operation"));
        }
    }
    for symbol in export
        .reads
        .iter()
        .chain(export.operations.iter())
        .chain(export.emits.iter())
        .chain(export.errors.iter())
    {
        if !is_semantic_id(symbol) {
            return Err(invalid("export-reference"));
        }
    }
    if !is_unique(&export.reads)
        || !is_unique(&export.operations)
        || !is_unique(&export.emits)
        || !is_unique(&export.errors)
    {
        return Err(invalid("export-reference-duplicate"));
    }
    if export.reads.len() > 1024
        || export.operations.len() > 1024
        || export.emits.len() > 1024
        || export.errors.len() > 1024
    {
        return Err(invalid("export-reference-bound"));
    }
    if export.cardinality.is_some() || export.consistency.is_some() || export.pagination.is_some() {
        // The owner facts travel together: a partial triple is malformed.
        let complete = export.cardinality.is_some()
            && export.consistency.is_some()
            && export.pagination.is_some();
        if !complete {
            return Err(invalid("query-facts"));
        }
        if !matches!(
            export.cardinality.as_deref().unwrap_or_default(),
            "one" | "optional" | "list" | "page" | "stream"
        ) || !matches!(
            export.consistency.as_deref().unwrap_or_default(),
            "strong" | "bounded" | "stale-ok"
        ) || !matches!(
            export.pagination.as_deref().unwrap_or_default(),
            "none" | "offset" | "cursor"
        ) {
            return Err(invalid("query-facts"));
        }
    }
    if let Some(code) = &export.code {
        let well_formed = (1..=128).contains(&code.len())
            && code
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_uppercase())
            && code
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '-');
        if !well_formed {
            return Err(invalid("error-code"));
        }
    }
    Ok(())
}

/// The semantic closure invariants of one decoded, digest-verified
/// bundle: shape references resolve to local exports or pinned foreign
/// symbols, identity references never dangle into unpinned foreign
/// modules, ports and deprecations reference existing exports.
pub(crate) fn admit_bundle_semantics(bundle: &PublicBundle) -> Result<(), DiagnosticSet> {
    let producer = bundle.producer.module.as_str();
    let export_ids: std::collections::BTreeSet<&str> = bundle
        .exports
        .iter()
        .map(|export| export.export.as_str())
        .collect();
    let mut pinned_foreign: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for dependency in &bundle.public_dependencies {
        for symbol in &dependency.exports {
            pinned_foreign.insert(symbol.as_str());
        }
    }
    // Dangling dependency pins: a pinned symbol owned by the producer
    // itself is contradictory input.
    for symbol in &pinned_foreign {
        if symbol
            .split_once('.')
            .is_some_and(|(owner, _)| owner == producer)
        {
            return Err(invalid("dependency-self-pin"));
        }
    }
    for export in &bundle.exports {
        let is_error = export.kind == "error";
        for field in export
            .fields
            .iter()
            .chain(&export.input)
            .chain(&export.payload)
        {
            check_type(
                &field.r#type,
                !is_error,
                producer,
                &export_ids,
                &pinned_foreign,
            )?;
        }
        if let Some(returns) = &export.returns {
            check_type(returns, true, producer, &export_ids, &pinned_foreign)?;
        }
        for read in &export.reads {
            check_ref(read, false, producer, &export_ids, &pinned_foreign)?;
        }
        for obligation in &export.effects {
            check_ref(
                &obligation.entity,
                false,
                producer,
                &export_ids,
                &pinned_foreign,
            )?;
            for event in &obligation.emits {
                check_ref(event, false, producer, &export_ids, &pinned_foreign)?;
            }
        }
        if let Some(entity) = &export.entity {
            check_ref(entity, false, producer, &export_ids, &pinned_foreign)?;
        }
        for event in &export.emits {
            check_ref(event, false, producer, &export_ids, &pinned_foreign)?;
        }
        for operation in &export.operations {
            check_ref(operation, false, producer, &export_ids, &pinned_foreign)?;
        }
        if let Some(invokes) = &export.invokes {
            check_ref(invokes, false, producer, &export_ids, &pinned_foreign)?;
        }
        for error_id in &export.errors {
            check_ref(error_id, false, producer, &export_ids, &pinned_foreign)?;
        }
    }
    for port in &bundle.ports {
        for operation in &port.operations {
            if !export_ids.contains(operation.as_str()) {
                return Err(invalid("port-operation-not-exported"));
            }
        }
    }
    for deprecation in &bundle.deprecations {
        if !export_ids.contains(deprecation.export.as_str()) {
            return Err(invalid("deprecation-target"));
        }
        if let Some(replacement) = &deprecation.replacement {
            if !export_ids.contains(replacement.as_str()) {
                return Err(invalid("replacement-target"));
            }
        }
    }
    Ok(())
}

/// Whether one semantic reference resolves: local shape references must
/// be exports of the producer (error payloads may reference registry-
/// owned local scalars), identity references need only a local or pinned
/// foreign owner, and unpinned foreign references never resolve.
fn check_ref(
    symbol: &str,
    shape: bool,
    producer: &str,
    export_ids: &std::collections::BTreeSet<&str>,
    pinned_foreign: &std::collections::BTreeSet<&str>,
) -> Result<(), DiagnosticSet> {
    let Some((owner, _)) = symbol.split_once('.') else {
        return Err(invalid("export-reference"));
    };
    if owner == producer {
        if shape && !export_ids.contains(symbol) {
            return Err(invalid("nonexported-public-closure"));
        }
        return Ok(());
    }
    if pinned_foreign.contains(symbol) {
        Ok(())
    } else {
        Err(invalid("unresolved-reference"))
    }
}

/// The leaf-reference check over one closed type expression.
fn check_type(
    expression: &super::wire::TypeExpr,
    shape: bool,
    producer: &str,
    export_ids: &std::collections::BTreeSet<&str>,
    pinned_foreign: &std::collections::BTreeSet<&str>,
) -> Result<(), DiagnosticSet> {
    match expression {
        super::wire::TypeExpr::Ref { r#ref } => {
            check_ref(r#ref, shape, producer, export_ids, pinned_foreign)
        }
        super::wire::TypeExpr::List { list } => {
            check_type(list, shape, producer, export_ids, pinned_foreign)
        }
        super::wire::TypeExpr::Optional { optional } => {
            check_type(optional, shape, producer, export_ids, pinned_foreign)
        }
    }
}
/// The canonical digest projection of one bundle (the exact domain the
/// export seam hashes): set order canonicalized, the digest member, the
/// freshness pins, and the integer export revisions excluded.
pub(crate) fn digest_projection(bundle: &PublicBundle) -> PublicBundle {
    let mut projection = bundle.clone();
    let mut exports = bundle.exports.clone();
    exports.sort_by(|left, right| left.export.cmp(&right.export));
    let mut dependencies = bundle.public_dependencies.clone();
    dependencies.sort_by(|left: &PublicDependency, right| left.module_id.cmp(&right.module_id));
    for dependency in &mut dependencies {
        dependency.exports.sort();
        dependency.exports.dedup();
    }
    let mut ports = bundle.ports.clone();
    ports.sort_by(|left, right| left.port.cmp(&right.port));
    for port in &mut ports {
        port.operations.sort();
        port.operations.dedup();
    }
    let mut capabilities = bundle.capabilities.clone();
    capabilities.sort_by(|left, right| left.capability.cmp(&right.capability));
    let mut deprecations = bundle.deprecations.clone();
    deprecations.sort_by(|left, right| left.export.cmp(&right.export));
    for export in &mut exports {
        export.version = 0;
    }
    projection.exports = exports;
    projection.public_dependencies = dependencies;
    projection.ports = ports;
    projection.capabilities = capabilities;
    projection.deprecations = deprecations;
    projection.digest = String::new();
    projection.pins = super::wire::PublicPins {
        model_version: String::new(),
        ir_digest: String::new(),
        source_revision: super::wire::State::Unknown,
    };
    projection
}

/// The structural admission of one attachment's raw JSON: required member
/// presence at every level of the published closed schema.
pub(crate) fn admit_attachment_presence(raw: &serde_json::Value) -> Result<(), DiagnosticSet> {
    require(raw, &["schemaVersion", "identity", "project", "modules"])?;
    let Some(modules) = raw["modules"].as_array() else {
        return Err(invalid("json-shape"));
    };
    for module in modules {
        require(module, &["moduleId", "apiVersion", "exports"])?;
        for key in [
            "exports",
            "imports",
            "consumers",
            "ports",
            "capabilities",
            "grants",
        ] {
            let Some(items) = module[key].as_array() else {
                continue;
            };
            for item in items {
                match key {
                    "exports" => require(item, &["export", "kind", "since", "aliasOf"])?,
                    "imports" => require(item, &["moduleId", "uses"])?,
                    "consumers" => require(item, &["consumer", "uses", "mode", "policy"])?,
                    "ports" => require(item, &["port", "operations"])?,
                    "capabilities" => {
                        require(item, &["capability", "obligation", "owner", "version"])?
                    }
                    _ => require(
                        item,
                        &[
                            "grant",
                            "consumer",
                            "aggregate",
                            "operations",
                            "fields",
                            "rationale",
                        ],
                    )?,
                }
            }
        }
        for key in ["deprecations", "operationErrors", "queryFacts"] {
            let Some(items) = module[key].as_array() else {
                continue;
            };
            for item in items {
                match key {
                    "deprecations" => require(
                        item,
                        &["export", "introducedIn", "removedIn", "replacement", "hint"],
                    )?,
                    "operationErrors" => require(item, &["operation", "errors"])?,
                    _ => require(item, &["query", "cardinality", "consistency", "pagination"])?,
                }
            }
        }
    }
    Ok(())
}

/// The structural admission of one consumer snapshot's raw JSON.
pub(crate) fn admit_snapshot_presence(raw: &serde_json::Value) -> Result<(), DiagnosticSet> {
    require(
        raw,
        &[
            "schemaVersion",
            "identity",
            "producer",
            "pins",
            "consumers",
            "complete",
            "digest",
        ],
    )?;
    require(
        &raw["pins"],
        &["project", "modelVersion", "irDigest", "apiDigest"],
    )?;
    require(&raw["producer"], &["project", "module"])?;
    for consumer in raw["consumers"].as_array().unwrap_or(&Vec::new()) {
        require(
            consumer,
            &["consumer", "declared", "mode", "policy", "uses"],
        )?;
        for use_row in consumer["uses"].as_array().unwrap_or(&Vec::new()) {
            require(
                use_row,
                &[
                    "export",
                    "member",
                    "origin",
                    "sensitivity",
                    "transitive",
                    "edges",
                ],
            )?;
        }
    }
    Ok(())
}

/// The structural admission of one consumer lock's raw JSON.
pub(crate) fn admit_lock_presence(raw: &serde_json::Value) -> Result<(), DiagnosticSet> {
    require(
        raw,
        &[
            "schemaVersion",
            "identity",
            "consumer",
            "producer",
            "apiVersion",
            "digest",
            "policy",
            "publicDependencies",
            "selected",
        ],
    )?;
    require(&raw["producer"], &["project", "module"])?;
    for dependency in raw["publicDependencies"].as_array().unwrap_or(&Vec::new()) {
        require(dependency, &["moduleId", "digest", "apiVersion", "exports"])?;
    }
    Ok(())
}
