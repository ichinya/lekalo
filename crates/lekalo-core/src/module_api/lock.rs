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
use super::wire::{self, ConsumerLock, ProducerPin};
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
    // Every transitive public dependency pin the consumer's public view
    // actually carries: the shared reference collector covers field/input/
    // payload/return types, reads, effect obligations, invocations, and
    // governed operations — nothing required is silently dropped from the
    // lock (finding 7).
    let dependencies = bundle
        .public_dependencies
        .iter()
        .filter(|dependency| {
            bundle.exports.iter().any(|export| {
                super::context::export_references(export)
                    .iter()
                    .any(|reference| dependency.exports.contains(reference))
            })
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

/// Parse and validate a consumer lock document.
pub fn parse_lock(bytes: &[u8]) -> Result<ConsumerLock, DiagnosticSet> {
    let raw: serde_json::Value = wire::decode(bytes, super::diagnostic::invalid)?;
    // The closed schema is the admission grammar (r3 review finding 1):
    // required members must be present even when nullable.
    super::admission::admit_lock_presence(&raw)?;
    let lock: ConsumerLock =
        serde_json::from_value(raw).map_err(|_| super::diagnostic::invalid("closed-shape"))?;
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
    // The typed closed-grammar admission (r6 review finding omp F2): the
    // published lock schema is the admission grammar in both directions,
    // like bundles, attachments, and snapshots — a schema-invalid lock is
    // refused input-invalid before any staleness comparison, and a
    // duplicate `moduleId` pin can never mask a stale dependency through
    // last-wins folding.
    super::admission::admit_lock(&lock)?;
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
        match supplied_deps.get(dependency.module_id.as_str()) {
            Some(digest) if *digest == dependency.digest => {}
            Some(_) => {
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
            None => {
                // A required pin absent from the supplied lock is as stale
                // as a changed digest (finding 15: symmetric dep diff).
                let mut finding = Evidence::new(
                    "module-api.lock-stale",
                    &supplied.consumer,
                    "lock-dependency-missing",
                );
                finding.consumer = Some(supplied.consumer.clone());
                finding.producer = Some(current.producer.module.clone());
                finding.path = vec![dependency.module_id.clone()];
                finding.expected = Some(dependency.digest.clone());
                finding.actual = Some("absent".to_owned());
                evidence.push(finding);
            }
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
    // The declared policy: a policy change is a resolution change.
    if current.policy != supplied.policy {
        let mut finding = Evidence::new("module-api.lock-stale", &supplied.consumer, "lock-policy");
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(current.producer.module.clone());
        finding.expected = Some(
            current
                .policy
                .as_ref()
                .map(policy_kind)
                .unwrap_or_else(|| "none".to_owned()),
        );
        finding.actual = Some(
            supplied
                .policy
                .as_ref()
                .map(policy_kind)
                .unwrap_or_else(|| "none".to_owned()),
        );
        evidence.push(finding);
    }
    // The independent API version pin.
    if current.api_version != supplied.api_version {
        let mut finding = Evidence::new(
            "module-api.lock-stale",
            &supplied.consumer,
            "lock-api-version",
        );
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(current.producer.module.clone());
        finding.expected = current.api_version.clone();
        finding.actual = supplied.api_version.clone();
        evidence.push(finding);
    }
    // Symmetric dependency diff: a supplied pin missing from the current
    // resolution is as stale as a missing or changed pin (finding 15).
    let current_deps: std::collections::BTreeSet<&str> = current
        .public_dependencies
        .iter()
        .map(|dependency| dependency.module_id.as_str())
        .collect();
    let mut extra_modules: Vec<String> = supplied
        .public_dependencies
        .iter()
        .filter(|dependency| !current_deps.contains(dependency.module_id.as_str()))
        .map(|dependency| dependency.module_id.clone())
        .collect();
    extra_modules.sort();
    for module_id in extra_modules {
        let mut finding = Evidence::new(
            "module-api.lock-stale",
            &supplied.consumer,
            "lock-dependency-extra",
        );
        finding.consumer = Some(supplied.consumer.clone());
        finding.producer = Some(current.producer.module.clone());
        finding.path = vec![module_id];
        finding.expected = Some("absent".to_owned());
        finding.actual = Some("pinned".to_owned());
        evidence.push(finding);
    }
    evidence
}

/// The stable policy spelling for evidence text.
fn policy_kind(policy: &super::wire::VersionPolicyWire) -> String {
    match policy {
        super::wire::VersionPolicyWire::ExactDigest { .. } => "exact-digest".to_owned(),
        super::wire::VersionPolicyWire::ExactVersion { .. } => "exact-version".to_owned(),
        super::wire::VersionPolicyWire::VersionRange { .. } => "version-range".to_owned(),
    }
}
