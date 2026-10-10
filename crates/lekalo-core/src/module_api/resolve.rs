//! Attachment decoding and binding to the typed compilation (issue #113).
//!
//! `resolve` is the only admission seam of the family: bytes enter as
//! immutable input, decode fail-closed (bounds, duplicate keys, unknown
//! fields), and bind against the accepted `Compilation`. Every structural
//! violation — unknown module id, export of a foreign or module-private
//! symbol, unknown export kind, contradiction with declared Model imports,
//! malformed versions, intervals, digests, or duplicate selectors — is
//! invalid input with a registered diagnostic. The pure core never reads
//! the filesystem: callers supply exact bytes.

use super::diagnostic::invalid;
use super::version::{is_digest, ApiVersion, ConsumerPolicy};
use super::wire::{
    self, Attachment, ModuleMap, VersionPolicyWire, MAX_EXPORTS_PER_MODULE, MAX_MODULES,
    MAX_USES_PER_CONSUMER,
};
use crate::diagnostics::DiagnosticSet;
use crate::ir::{Compilation, DefinitionKind, Visibility};

/// The resolved, bound attachment: policy per exact module id.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub project: String,
    pub modules: ModuleMap,
    /// The exact attachment bytes digest (domain-prefixed).
    pub digest: String,
}

/// The export kinds the closed grammar admits.
const EXPORT_KINDS: &[(&str, DefinitionKind)] = &[
    ("scalar", DefinitionKind::Scalar),
    ("enum", DefinitionKind::Enum),
    ("value-object", DefinitionKind::ValueObject),
    ("entity", DefinitionKind::Entity),
    ("command", DefinitionKind::Command),
    ("query", DefinitionKind::Query),
    ("policy", DefinitionKind::Policy),
    ("event", DefinitionKind::Event),
    ("effect", DefinitionKind::Effect),
    ("endpoint", DefinitionKind::Endpoint),
];

/// Decode and bind the attachment against the compilation. Every violation
/// is one invalid set; nothing partial is ever returned.
pub fn resolve(bytes: &[u8], compilation: &Compilation) -> Result<Resolved, DiagnosticSet> {
    let attachment: Attachment = wire::decode(bytes, invalid)?;
    if attachment.schema_version != format!("{}/v{}", wire::API_SCHEMA, wire::VERSION)
        || attachment.identity != format!("{}@{}", wire::API_IDENTITY, wire::VERSION)
    {
        return Err(super::diagnostic::unsupported_version(
            &attachment.schema_version,
        ));
    }
    if attachment.modules.len() > MAX_MODULES {
        return Err(invalid("module-bound"));
    }
    let project = &compilation.project;
    if let Some(project_def) = &project.project {
        if attachment.project != project_def.id.as_str() {
            return Err(invalid("project-pin"));
        }
    }
    let mut modules = ModuleMap::new();
    for module in &attachment.modules {
        if modules
            .insert(module.module_id.clone(), module.clone())
            .is_some()
        {
            return Err(invalid("duplicate-module"));
        }
        let Some(definition) = project
            .modules
            .iter()
            .find(|definition| definition.id.as_str() == module.module_id)
        else {
            return Err(invalid("unknown-module"));
        };
        // Attachment imports must agree with the declared Model imports;
        // the attachment adds policy and use detail, never hidden imports.
        for declared in &module.imports {
            if !definition
                .imports
                .iter()
                .any(|import| import.as_str() == declared.module_id)
            {
                return Err(invalid("import-contradiction"));
            }
            if declared.uses.len() > MAX_EXPORTS_PER_MODULE {
                return Err(invalid("use-bound"));
            }
        }
        if module.exports.len() > MAX_EXPORTS_PER_MODULE {
            return Err(invalid("export-bound"));
        }
        let mut seen_symbols: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for entry in &module.exports {
            // The export id is `module.symbol` and the symbol must be owned.
            let Some((owner, symbol)) = entry.export.split_once('.') else {
                return Err(invalid("export-id"));
            };
            if owner != module.module_id {
                return Err(invalid("foreign-export"));
            }
            if !seen_symbols.insert(symbol) {
                return Err(invalid("duplicate-export"));
            }
            // A typed #62 error export references the error registry, not a
            // Model definition; existence and payload exposure are checked
            // at export time.
            if entry.kind == "error" {
                continue;
            }
            let Some(definition) = project
                .definitions
                .iter()
                .find(|definition| definition.id().as_str() == entry.export)
            else {
                return Err(invalid("unknown-export"));
            };
            // Visibility `module` contradicts an export declaration.
            if definition.common().visibility == Some(Visibility::Module) {
                return Err(invalid("module-visibility-export"));
            }
            let Some((_, kind)) = EXPORT_KINDS.iter().find(|(name, _)| *name == entry.kind) else {
                if entry.kind != "error" {
                    return Err(invalid("unknown-kind"));
                }
                // An error export references the typed #62 registry, not a
                // Model definition kind; existence is checked at export.
                continue;
            };
            // The declared kind must agree with the actual definition kind:
            // a mislabeled attachment row is contradictory input, never a
            // silent relabeling of the exported surface.
            if definition.kind() != *kind {
                return Err(invalid("kind-contradiction"));
            }
        }
        // apiVersion, when present, must be canonical stable SemVer.
        if let Some(version) = &module.api_version {
            if ApiVersion::parse(version).is_none() {
                return Err(invalid("api-version"));
            }
        }
        // Consumer declarations parse their closed policies now. Declared
        // zero-use consumers stay in scope (subscriptions and range
        // policies never vanish); the per-consumer use bound is enforced.
        for consumer in &module.consumers {
            if consumer.uses.len() > MAX_USES_PER_CONSUMER {
                return Err(invalid("consumer-use-bound"));
            }
            let mut unique = consumer.uses.clone();
            unique.sort();
            unique.dedup();
            if unique.len() != consumer.uses.len() {
                return Err(invalid("duplicate-use"));
            }
            for use_id in &consumer.uses {
                if !entry_exported(&module.exports, use_id) {
                    return Err(invalid("unknown-use"));
                }
            }
            if !matches!(consumer.mode.as_str(), "source" | "wire") {
                return Err(invalid("consumer-mode"));
            }
            if let Some(policy) = &consumer.policy {
                if ConsumerPolicy::parse(policy).is_none() {
                    return Err(invalid("consumer-policy"));
                }
            }
        }
        for port in &module.ports {
            if port.operations.is_empty() || port.operations.len() > MAX_EXPORTS_PER_MODULE {
                return Err(invalid("port-operations"));
            }
            for operation in &port.operations {
                if !entry_exported(&module.exports, operation) {
                    return Err(invalid("port-operation-not-exported"));
                }
            }
        }
        for capability in &module.capabilities {
            if !matches!(capability.obligation.as_str(), "required" | "optional") {
                return Err(invalid("capability-obligation"));
            }
        }
        for grant in &module.grants {
            if grant.operations.is_empty() || grant.fields.is_empty() {
                return Err(invalid("grant-shape"));
            }
            for operation in &grant.operations {
                if !matches!(operation.as_str(), "create" | "update" | "delete") {
                    return Err(invalid("grant-operation"));
                }
            }
            if grant.fields.len() > 256 {
                return Err(invalid("grant-field-bound"));
            }
        }
        for deprecation in &module.deprecations {
            if !entry_exported(&module.exports, &deprecation.export) {
                return Err(invalid("deprecation-target"));
            }
            if ApiVersion::parse(&deprecation.introduced_in).is_none() {
                return Err(invalid("deprecation-introduced"));
            }
            if let Some(removed) = &deprecation.removed_in {
                let (Some(introduced), Some(removed)) = (
                    ApiVersion::parse(&deprecation.introduced_in),
                    ApiVersion::parse(removed),
                ) else {
                    return Err(invalid("deprecation-window"));
                };
                if removed < introduced {
                    return Err(invalid("deprecation-window"));
                }
            }
        }
    }
    // Cross-module consumer declarations: source-mode consumers must be
    // in-project modules; wire-mode consumers may be external callers.
    for module in modules.values() {
        for consumer in &module.consumers {
            if consumer.mode == "source" && !modules.contains_key(&consumer.consumer) {
                return Err(invalid("unknown-consumer"));
            }
        }
        for grant in &module.grants {
            if !modules.contains_key(&grant.consumer) {
                return Err(invalid("unknown-grant-consumer"));
            }
        }
        // Every declared import use must be an exact public export of the
        // imported module: selected uses are policy facts about the
        // provider's allowlist, never guessed or private symbols.
        for import in &module.imports {
            if import.uses.is_empty() {
                continue;
            }
            let Some(provider) = modules.get(&import.module_id) else {
                return Err(invalid("unknown-import-module"));
            };
            for use_id in &import.uses {
                if !entry_exported(&provider.exports, use_id) {
                    return Err(invalid("unknown-import-use"));
                }
            }
        }
    }
    let project_id = attachment.project.clone();
    let digest = wire::digest(wire::DIGEST_DOMAIN_API, &wire::canonical(&attachment));
    Ok(Resolved {
        project: project_id,
        digest,
        modules,
    })
}

fn entry_exported(exports: &[wire::ExportEntry], id: &str) -> bool {
    exports.iter().any(|entry| entry.export == id)
}

/// Validate one optional lock document's structural pins (used by the lock
/// command before comparison).
pub(crate) fn validate_policy(policy: &VersionPolicyWire) -> Result<(), DiagnosticSet> {
    match policy {
        VersionPolicyWire::ExactDigest { digest } if is_digest(digest) => Ok(()),
        VersionPolicyWire::ExactVersion { version } => ApiVersion::parse(version)
            .map(|_| ())
            .ok_or_else(|| invalid("lock-policy-version")),
        VersionPolicyWire::VersionRange { .. } => ConsumerPolicy::parse(policy)
            .map(|_| ())
            .ok_or_else(|| invalid("lock-policy-interval")),
        _ => Err(invalid("lock-policy-digest")),
    }
}
