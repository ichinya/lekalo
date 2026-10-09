//! The consumer snapshot of one producer revision (issue #113).
//!
//! `consumer_snapshot` joins the declared consumer inventory (the
//! attachment's producer-side declarations) with the actually resolved uses
//! (every cross-module reference the IR carries into the producer's
//! exports). Declared zero-use consumers stay in scope so subscriptions and
//! range policies are never lost; resolved-but-undeclared uses stay visible
//! as `declared:false` rows. Edge keys are the original typed reference
//! occurrences, and completeness never overclaims: unknown external callers
//! keep the snapshot explicitly incomplete.

use super::diagnostic::invalid;
use super::resolve::Resolved;
use super::wire::{self, ConsumerRow, ConsumerSnapshot, ConsumerUse, ProducerPin, SnapshotPins};
use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;
use std::collections::BTreeMap;

/// Produce the immutable consumer snapshot of one producer module.
pub fn consumer_snapshot(
    resolved: &Resolved,
    compilation: &Compilation,
    producer: &str,
) -> Result<ConsumerSnapshot, DiagnosticSet> {
    let Some(policy) = resolved.modules.get(producer) else {
        return Err(invalid("unknown-module"));
    };
    let project = &compilation.project;
    let exports: Vec<String> = policy
        .exports
        .iter()
        .map(|entry| entry.export.clone())
        .collect();
    let export_set: std::collections::BTreeSet<&str> = exports.iter().map(String::as_str).collect();
    // Resolve actual uses: every cross-module reference to an export with
    // its original definition occurrence as the edge key.
    let mut resolved_uses: BTreeMap<&str, Vec<ConsumerUse>> = BTreeMap::new();
    for definition in project.definitions.iter() {
        let Some((owner, _)) = definition.id().as_str().split_once('.') else {
            continue;
        };
        if owner == producer {
            continue;
        }
        for reference in super::effects::references_of(definition) {
            let Some((reference_owner, _)) = reference.split_once('.') else {
                continue;
            };
            if reference_owner != producer || !export_set.contains(reference) {
                continue;
            }
            let use_row = ConsumerUse {
                export: reference.to_owned(),
                member: None,
                origin: "resolved".to_owned(),
                sensitivity: "whole-shape".to_owned(),
                transitive: false,
                edges: vec![format!("{}->{}", definition.id().as_str(), reference)],
            };
            resolved_uses.entry(owner).or_default().push(use_row);
        }
    }
    // Declared consumers: the attachment rows of the producer module.
    let mut rows: Vec<ConsumerRow> = Vec::new();
    let mut declared_set: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for consumer in &policy.consumers {
        if !declared_set.insert(consumer.consumer.as_str()) {
            return Err(invalid("duplicate-consumer"));
        }
        let mut uses: Vec<ConsumerUse> = consumer
            .uses
            .iter()
            .map(|export| ConsumerUse {
                export: export.clone(),
                member: None,
                origin: "declared".to_owned(),
                sensitivity: "whole-shape".to_owned(),
                transitive: false,
                edges: vec![],
            })
            .collect();
        // Merge the resolved uses into the declared row.
        if let Some(resolved_rows) = resolved_uses.get(consumer.consumer.as_str()) {
            uses.extend(resolved_rows.iter().cloned());
        }
        uses.sort_by(|left, right| {
            (
                left.export.as_str(),
                left.origin.as_str(),
                left.member.as_deref(),
            )
                .cmp(&(
                    right.export.as_str(),
                    right.origin.as_str(),
                    right.member.as_deref(),
                ))
        });
        uses.dedup_by(|left, right| {
            left.export == right.export
                && left.origin == right.origin
                && left.member == right.member
        });
        rows.push(ConsumerRow {
            consumer: consumer.consumer.clone(),
            declared: true,
            mode: Some(consumer.mode.clone()),
            policy: consumer.policy.clone(),
            uses,
        });
    }
    // Resolved-but-undeclared consumers: visible, never silent.
    for (consumer, uses) in &resolved_uses {
        if declared_set.contains(consumer) {
            continue;
        }
        rows.push(ConsumerRow {
            consumer: (*consumer).to_owned(),
            declared: false,
            mode: None,
            policy: None,
            uses: uses.clone(),
        });
    }
    rows.sort_by(|left, right| left.consumer.cmp(&right.consumer));
    let pins = SnapshotPins {
        project: resolved.project.clone(),
        model_version: wire::MODEL_VERSION.to_owned(),
        ir_digest: format!(
            "sha256:{}",
            crate::digest::sha256_hex(project.to_canonical_json().as_bytes())
        ),
        // The same digest backs the consumer snapshot, the consumer lock,
        // and the context capsule: the producer's certified public bundle
        // digest, never the raw attachment bytes digest (finding 13).
        api_digest: super::public::export_public(resolved, compilation, producer)?.digest,
    };
    let producer_pin = ProducerPin {
        project: resolved.project.clone(),
        module: producer.to_owned(),
    };
    let partial = ConsumerSnapshot {
        schema_version: format!("{}/v{}", wire::CONSUMERS_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::CONSUMERS_IDENTITY, wire::VERSION),
        producer: producer_pin,
        pins,
        consumers: rows,
        // The inventory is exact for named consumers of this revision;
        // unknown external callers keep external completeness unknown, so
        // the snapshot never claims the total consumer set.
        complete: false,
        digest: String::new(),
    };
    let digest = wire::digest(wire::DIGEST_DOMAIN_CONSUMERS, &wire::canonical(&partial));
    Ok(ConsumerSnapshot { digest, ..partial })
}

/// Parse and validate a consumer snapshot document (the diff seam).
pub fn parse_snapshot(bytes: &[u8]) -> Result<ConsumerSnapshot, DiagnosticSet> {
    let snapshot: ConsumerSnapshot = wire::decode(bytes, invalid)?;
    if snapshot.schema_version != format!("{}/v{}", wire::CONSUMERS_SCHEMA, wire::VERSION)
        || snapshot.identity != format!("{}@{}", wire::CONSUMERS_IDENTITY, wire::VERSION)
    {
        return Err(super::diagnostic::unsupported_version(
            &snapshot.schema_version,
        ));
    }
    if !super::version::is_digest(&snapshot.digest) {
        return Err(invalid("snapshot-digest"));
    }
    let mut without_digest = snapshot.clone();
    without_digest.digest = String::new();
    let expected = wire::digest(
        wire::DIGEST_DOMAIN_CONSUMERS,
        &wire::canonical(&without_digest),
    );
    if expected != snapshot.digest {
        return Err(invalid("snapshot-digest-mismatch"));
    }
    Ok(snapshot)
}
