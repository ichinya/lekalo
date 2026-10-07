//! Consumer lock resolution and staleness checks (issue #113).
//!
//! The module consumer lock pins the selected public digest, the optional
//! independent API version, and every transitive public dependency digest.
//! `--check` detects dependency changes even when the top-level version
//! string is unchanged; private layout/body/storage changes preserve the
//! resolved public identity. Checks never rewrite a stale lock into a
//! passing one and are entirely independent of the adapter `lekalo.lock`.

use super::diagnostic::Evidence;

use super::resolve::Resolved;
use super::wire::{self, ConsumerLock, ProducerPin, PublicBundle};
use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;

/// Build the candidate consumer lock payload for one consumer of one
/// producer module. Nothing is ever written: the caller persists stdout.
pub fn resolve_lock(
    resolved: &Resolved,
    compilation: &Compilation,
    producer: &str,
    consumer: &str,
) -> Result<ConsumerLock, DiagnosticSet> {
    let Some(policy) = resolved.modules.get(producer) else {
        return Err(super::diagnostic::invalid("unknown-module"));
    };
    let Some(declaration) = policy
        .consumers
        .iter()
        .find(|declaration| declaration.consumer == consumer)
    else {
        return Err(super::diagnostic::invalid("unknown-consumer"));
    };
    let bundle = super::validate::bundle(resolved, compilation, producer)?;
    let dependencies = bundle
        .public_dependencies
        .iter()
        .filter(|dependency| {
            bundle.exports.iter().any(|export| {
                export
                    .fields
                    .iter()
                    .any(|field| type_uses(&field.r#type, &dependency.exports))
            }) || uses_export(&bundle, &dependency.exports)
        })
        .cloned()
        .collect();
    Ok(ConsumerLock {
        schema_version: format!("{}/v{}", wire::LOCK_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::LOCK_IDENTITY, wire::VERSION),
        consumer: consumer.to_owned(),
        producer: ProducerPin {
            project: resolved.project.clone(),
            module: producer.to_owned(),
        },
        api_version: bundle.api_version.clone(),
        digest: bundle.digest.clone(),
        policy: declaration.policy.clone(),
        public_dependencies: dependencies,
        selected: declaration.uses.clone(),
    })
}

fn uses_export(bundle: &PublicBundle, exports: &[String]) -> bool {
    bundle
        .exports
        .iter()
        .any(|export| export.reads.iter().any(|read| exports.contains(read)))
        || bundle
            .exports
            .iter()
            .any(|export| export.emits.iter().any(|event| exports.contains(event)))
        || bundle.exports.iter().any(|export| {
            export
                .operations
                .iter()
                .any(|operation| exports.contains(operation))
        })
        || bundle
            .exports
            .iter()
            .any(|export| exports.contains(&export.export))
            && false
}

fn type_uses(expression: &super::wire::TypeExpr, exports: &[String]) -> bool {
    match expression {
        super::wire::TypeExpr::Ref { r#ref } => exports.contains(r#ref),
        super::wire::TypeExpr::List { list } => type_uses(list, exports),
        super::wire::TypeExpr::Optional { optional } => type_uses(optional, exports),
    }
}

/// Parse and validate a consumer lock document.
pub fn parse_lock(bytes: &[u8]) -> Result<ConsumerLock, DiagnosticSet> {
    let lock: ConsumerLock = super::wire::decode(bytes, super::diagnostic::invalid)?;
    if lock.schema_version != format!("{}/v{}", wire::LOCK_SCHEMA, wire::VERSION)
        || lock.identity != format!("{}@{}", wire::LOCK_IDENTITY, wire::VERSION)
    {
        return Err(super::diagnostic::unsupported_version(&lock.schema_version));
    }
    if !super::version::is_digest(&lock.digest) {
        return Err(super::diagnostic::invalid("lock-digest"));
    }
    if let Some(policy) = &lock.policy {
        super::resolve::validate_policy(policy)?;
    }
    Ok(lock)
}

/// Compare the supplied lock against the current resolution. The returned
/// evidence carries the expected/actual pins; empty means fresh.
pub fn check_lock(current: &ConsumerLock, supplied: &ConsumerLock) -> Vec<Evidence> {
    let mut evidence = Vec::new();
    if current.producer != supplied.producer || current.consumer != supplied.consumer {
        let mut finding =
            Evidence::new("module-api.lock-stale", &supplied.consumer, "lock-identity");
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(current.producer.module.clone());
        finding.expected = Some(format!(
            "{}::{}",
            current.producer.project, current.producer.module
        ));
        finding.actual = Some(format!(
            "{}::{}",
            supplied.producer.project, supplied.producer.module
        ));
        evidence.push(finding);
        return evidence;
    }
    if current.digest != supplied.digest {
        let mut finding = Evidence::new("module-api.lock-stale", &supplied.consumer, "lock-digest");
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(current.producer.module.clone());
        finding.path = vec!["version-range".to_owned()];
        finding.expected = Some(current.digest.clone());
        finding.actual = Some(supplied.digest.clone());
        evidence.push(finding);
    }
    // Public dependency changes count even when the top-level version
    // string is unchanged.
    let supplied_deps: std::collections::BTreeMap<&str, &str> = supplied
        .public_dependencies
        .iter()
        .map(|dependency| (dependency.module_id.as_str(), dependency.digest.as_str()))
        .collect();
    for dependency in &current.public_dependencies {
        if supplied_deps
            .get(dependency.module_id.as_str())
            .is_some_and(|digest| *digest != dependency.digest)
        {
            let mut finding = Evidence::new(
                "module-api.lock-stale",
                &supplied.consumer,
                "lock-dependency-digest",
            );
            finding.consumer = Some(supplied.consumer.clone());
            finding.producer = Some(current.producer.module.clone());
            finding.path = vec![dependency.module_id.clone()];
            finding.expected = Some(dependency.digest.clone());
            finding.actual = supplied_deps
                .get(dependency.module_id.as_str())
                .map(|digest| (*digest).to_owned());
            evidence.push(finding);
        }
    }
    // The selected public ids: a changed selection is a resolution change.
    if current.selected != supplied.selected {
        let mut finding = Evidence::new(
            "module-api.lock-stale",
            &supplied.consumer,
            "lock-selection",
        );
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(current.producer.module.clone());
        finding.expected = Some(current.selected.join(","));
        finding.actual = Some(supplied.selected.join(","));
        evidence.push(finding);
    }
    evidence
}

/// Freshness verdict for one bundle against its own lock (the lock seam of
/// `module api lock --check` without consumer context).
pub fn bundle_lock_evidence(bundle: &PublicBundle, supplied: &ConsumerLock) -> Vec<Evidence> {
    let mut evidence = Vec::new();
    if bundle.digest != supplied.digest {
        let mut finding = Evidence::new("module-api.lock-stale", &supplied.consumer, "lock-digest");
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(bundle.producer.module.clone());
        finding.expected = Some(bundle.digest.clone());
        finding.actual = Some(supplied.digest.clone());
        evidence.push(finding);
    }
    let supplied_deps: std::collections::BTreeMap<&str, &str> = supplied
        .public_dependencies
        .iter()
        .map(|dependency| (dependency.module_id.as_str(), dependency.digest.as_str()))
        .collect();
    for dependency in &bundle.public_dependencies {
        if supplied_deps
            .get(dependency.module_id.as_str())
            .is_some_and(|digest| *digest != dependency.digest)
        {
            let mut finding = Evidence::new(
                "module-api.lock-stale",
                &supplied.consumer,
                "lock-dependency-digest",
            );
            finding.consumer = Some(supplied.consumer.clone());
            finding.producer = Some(bundle.producer.module.clone());
            finding.path = vec![dependency.module_id.clone()];
            finding.expected = Some(dependency.digest.clone());
            finding.actual = supplied_deps
                .get(dependency.module_id.as_str())
                .map(|digest| (*digest).to_owned());
            evidence.push(finding);
        }
    }
    evidence
}
