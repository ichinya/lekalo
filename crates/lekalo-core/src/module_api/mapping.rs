//! Mapping conformance: the in-process/service-port implementation
//! criterion (issue #113, r4 review finding 10/codex).
//!
//! The boundary must work identically for an in-process consumer and an
//! extracted-service consumer. `check_mapping_conformance` compares two
//! **independently authored** mapping-evidence documents — one per
//! implementation, each stating per public operation the exact IDs,
//! input/result shapes, error unions, effect obligations, read/write
//! meaning, and capability demands it observes — against one pinned
//! public digest and against each other. Synthetic mapping evidence is
//! the research contract's local proof level; deployment is not claimed.
//! Every divergence is a typed row, never a silent normalization, and an
//! unproven conformance surfaces as required evidence gaps so AC7 stays
//! uncovered until both implementations match.

use super::diagnostic::{invalid, Evidence};
use super::public::parse_bundle;
use super::wire::{
    self, MappingConformance, MappingDivergence, MappingEvidence, MappingObservation, PublicBundle,
};
use crate::diagnostics::DiagnosticSet;

/// Parse and validate one mapping-evidence document: the closed wire
/// contract (presence, ids, vocabularies, bounds) plus the digest seal —
/// a resealed checksum never admits a malformed row.
pub fn parse_mapping_evidence(bytes: &[u8]) -> Result<MappingEvidence, DiagnosticSet> {
    let raw: serde_json::Value = wire::decode(bytes, invalid)?;
    admit_mapping_presence(&raw)?;
    let evidence: MappingEvidence =
        serde_json::from_value(raw).map_err(|_| invalid("closed-shape"))?;
    if evidence.schema_version != format!("{}/v{}", wire::MAPPING_SCHEMA, wire::VERSION)
        || evidence.identity != format!("{}@{}", wire::MAPPING_IDENTITY, wire::VERSION)
    {
        return Err(super::diagnostic::unsupported_version(
            &evidence.schema_version,
        ));
    }
    if !super::version::is_digest(&evidence.digest) {
        return Err(invalid("mapping-digest"));
    }
    admit_mapping(&evidence)?;
    let mut without_digest = evidence.clone();
    without_digest.digest = String::new();
    let expected = wire::digest(
        wire::DIGEST_DOMAIN_MAPPING,
        &wire::canonical(&without_digest),
    );
    if expected != evidence.digest {
        return Err(invalid("mapping-digest-mismatch"));
    }
    Ok(evidence)
}

/// The structural admission of one mapping-evidence document's raw JSON.
fn admit_mapping_presence(raw: &serde_json::Value) -> Result<(), DiagnosticSet> {
    let serde_json::Value::Object(map) = raw else {
        return Err(invalid("json-shape"));
    };
    for key in [
        "schemaVersion",
        "identity",
        "implementation",
        "port",
        "producer",
        "publicDigest",
        "observations",
        "unsupported",
        "digest",
    ] {
        if !map.contains_key(key) {
            return Err(invalid("required-member-missing"));
        }
    }
    if !map["producer"].is_object() {
        return Err(invalid("json-shape"));
    }
    for key in ["project", "module"] {
        if !map["producer"]
            .as_object()
            .is_some_and(|producer| producer.contains_key(key))
        {
            return Err(invalid("required-member-missing"));
        }
    }
    for observation in map["observations"].as_array().unwrap_or(&Vec::new()) {
        let serde_json::Value::Object(row) = observation else {
            return Err(invalid("json-shape"));
        };
        for key in [
            "operation",
            "kind",
            "inputs",
            "results",
            "errors",
            "effects",
            "access",
            "capabilities",
        ] {
            if !row.contains_key(key) {
                return Err(invalid("required-member-missing"));
            }
        }
    }
    Ok(())
}

/// The typed closed grammar of one mapping-evidence document.
fn admit_mapping(evidence: &MappingEvidence) -> Result<(), DiagnosticSet> {
    if !matches!(
        evidence.implementation.as_str(),
        "in-process" | "extracted-service"
    ) {
        return Err(invalid("mapping-implementation"));
    }
    if !super::admission::is_semantic_id(&evidence.port) {
        return Err(invalid("mapping-port"));
    }
    if !super::admission::is_project_id(&evidence.producer.project)
        || !super::admission::is_module_id(&evidence.producer.module)
    {
        return Err(invalid("mapping-producer"));
    }
    if !super::version::is_digest(&evidence.public_digest) {
        return Err(invalid("mapping-public-digest"));
    }
    if evidence.observations.len() > wire::MAX_EXPORTS_PER_MODULE
        || evidence.unsupported.len() > wire::MAX_DEPENDENCY_NESTING
    {
        return Err(invalid("mapping-bound"));
    }
    let mut operations: Vec<&str> = Vec::new();
    for observation in &evidence.observations {
        admit_observation(observation)?;
        operations.push(observation.operation.as_str());
    }
    if !operations
        .iter()
        .all(|id| operations.iter().filter(|kept| *kept == id).count() == 1)
    {
        return Err(invalid("duplicate-observation"));
    }
    let mut unsupported = evidence.unsupported.clone();
    unsupported.sort();
    unsupported.dedup();
    if unsupported.len() != evidence.unsupported.len() {
        return Err(invalid("duplicate-unsupported"));
    }
    for capability in &evidence.unsupported {
        if !super::admission::is_capability_id(capability) {
            return Err(invalid("mapping-unsupported"));
        }
    }
    Ok(())
}

/// The typed closed grammar of one observation row.
fn admit_observation(observation: &MappingObservation) -> Result<(), DiagnosticSet> {
    if !super::admission::is_semantic_id(&observation.operation) {
        return Err(invalid("mapping-operation"));
    }
    if !matches!(observation.kind.as_str(), "command" | "query") {
        return Err(invalid("mapping-kind"));
    }
    if !matches!(observation.access.as_str(), "read" | "write") {
        return Err(invalid("mapping-access"));
    }
    if observation.inputs.len() > 1024
        || observation.errors.len() > 1024
        || observation.effects.len() > wire::MAX_DEPENDENCY_NESTING
        || observation.capabilities.len() > wire::MAX_DEPENDENCY_NESTING
    {
        return Err(invalid("mapping-observation-bound"));
    }
    for error_id in &observation.errors {
        if !super::admission::is_semantic_id(error_id) {
            return Err(invalid("mapping-error-id"));
        }
    }
    for capability in &observation.capabilities {
        if !super::admission::is_capability_id(capability) {
            return Err(invalid("mapping-capability"));
        }
    }
    Ok(())
}

/// Check the conformance of the two independently authored mapping
/// evidence documents against one public bundle. Malformed input fails
/// invalid; every semantic divergence is a typed row plus a required
/// evidence gap, so an unproven conformance is explicit.
pub fn check_mapping_conformance(
    bundle_bytes: &[u8],
    in_process_bytes: &[u8],
    service_bytes: &[u8],
) -> Result<(MappingConformance, Vec<Evidence>), DiagnosticSet> {
    let bundle: PublicBundle = parse_bundle(bundle_bytes)?;
    let in_process = parse_mapping_evidence(in_process_bytes)?;
    let service = parse_mapping_evidence(service_bytes)?;
    if in_process.implementation == service.implementation {
        return Err(invalid("mapping-implementation-pair"));
    }
    let mut divergences: Vec<MappingDivergence> = Vec::new();
    for evidence in [&in_process, &service] {
        bind_and_observe(&bundle, evidence, &mut divergences);
    }
    // Cross-implementation equality: the two independent statements must
    // agree on every shared aspect of every operation — IDs, inputs and
    // results, error unions, effects, and read/write meaning.
    let local = observations_by_operation(&in_process);
    let remote = observations_by_operation(&service);
    for operation in local.keys().copied().chain(remote.keys().copied()) {
        match (local.get(operation), remote.get(operation)) {
            (Some(left), Some(right)) => {
                let mut aspects: Vec<(&str, String, String)> = Vec::new();
                if left.kind != right.kind {
                    aspects.push(("kind", left.kind.clone(), right.kind.clone()));
                }
                if left.inputs != right.inputs {
                    aspects.push((
                        "inputs",
                        type_list_text(&left.inputs),
                        type_list_text(&right.inputs),
                    ));
                }
                if left.results != right.results {
                    aspects.push((
                        "results",
                        left.results
                            .as_ref()
                            .map(wire::canonical)
                            .unwrap_or_default(),
                        right
                            .results
                            .as_ref()
                            .map(wire::canonical)
                            .unwrap_or_default(),
                    ));
                }
                if left.errors != right.errors {
                    aspects.push(("errors", left.errors.join(","), right.errors.join(",")));
                }
                if left.effects != right.effects {
                    aspects.push((
                        "effects",
                        wire::canonical(&left.effects),
                        wire::canonical(&right.effects),
                    ));
                }
                if left.access != right.access {
                    aspects.push(("access", left.access.clone(), right.access.clone()));
                }
                for (aspect, expected, actual) in aspects {
                    divergences.push(MappingDivergence {
                        implementation: "in-process|extracted-service".to_owned(),
                        operation: operation.to_owned(),
                        aspect: aspect.to_owned(),
                        expected: Some(expected),
                        actual: Some(actual),
                    });
                }
            }
            _ => {
                divergences.push(MappingDivergence {
                    implementation: if local.contains_key(operation) {
                        "extracted-service".to_owned()
                    } else {
                        "in-process".to_owned()
                    },
                    operation: operation.to_owned(),
                    aspect: "membership".to_owned(),
                    expected: Some("observed".to_owned()),
                    actual: None,
                });
            }
        }
    }
    let mut unsupported: Vec<String> = in_process
        .unsupported
        .iter()
        .chain(service.unsupported.iter())
        .cloned()
        .collect();
    unsupported.sort();
    unsupported.dedup();
    let conformance = MappingConformance {
        schema_version: format!("{}/v{}", wire::MAPPING_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::MAPPING_IDENTITY, wire::VERSION),
        producer: bundle.producer.clone(),
        public_digest: bundle.digest.clone(),
        in_process: in_process.port.clone(),
        service: service.port.clone(),
        conforms: divergences.is_empty(),
        observations: local.len().max(remote.len()) as u64,
        unsupported,
        divergences: divergences.clone(),
    };
    let mut evidence = Vec::new();
    for divergence in &divergences {
        let mut gap = Evidence::new(
            "module-api.evidence-incomplete",
            &conformance.producer.module,
            "mapping-divergence",
        );
        gap.export = Some(divergence.operation.clone());
        gap.path = vec![divergence.implementation.clone(), divergence.aspect.clone()];
        gap.expected = divergence.expected.clone();
        gap.actual = divergence.actual.clone();
        gap.gaps = vec!["mapping-conformance-unproven".to_owned()];
        evidence.push(gap);
    }
    Ok((conformance, evidence))
}

/// Bind one evidence document to the bundle and observe its rows: same
/// producer and public digest, a real port of exported operations, exact
/// coverage of that port, and per-operation facts equal to the certified
/// public semantics.
fn bind_and_observe(
    bundle: &PublicBundle,
    evidence: &MappingEvidence,
    divergences: &mut Vec<MappingDivergence>,
) {
    let implementation = evidence.implementation.clone();
    let mut divergence =
        |operation: &str, aspect: &str, expected: Option<String>, actual: Option<String>| {
            divergences.push(MappingDivergence {
                implementation: implementation.clone(),
                operation: operation.to_owned(),
                aspect: aspect.to_owned(),
                expected,
                actual,
            })
        };
    if evidence.producer != bundle.producer || evidence.public_digest != bundle.digest {
        divergence(
            &evidence.port,
            "digest-binding",
            Some(bundle.digest.clone()),
            Some(evidence.public_digest.clone()),
        );
        return;
    }
    let Some(port) = bundle
        .ports
        .iter()
        .find(|port| port.port == evidence.port)
        .cloned()
    else {
        divergence(
            &evidence.port,
            "port",
            Some("exported-port".to_owned()),
            None,
        );
        return;
    };
    let observed: std::collections::BTreeSet<&str> = evidence
        .observations
        .iter()
        .map(|observation| observation.operation.as_str())
        .collect();
    let declared: std::collections::BTreeSet<&str> =
        port.operations.iter().map(String::as_str).collect();
    if observed != declared {
        for operation in declared.difference(&observed) {
            divergence(operation, "membership", Some("observed".to_owned()), None);
        }
        for operation in observed.difference(&declared) {
            divergence(operation, "membership", None, Some("not-mapped".to_owned()));
        }
    }
    let capability_ids: std::collections::BTreeSet<&str> = bundle
        .capabilities
        .iter()
        .map(|capability| capability.capability.as_str())
        .collect();
    // An implementation that cannot provide a required capability of the
    // mapped surface diverges from the certified contract.
    for capability in &evidence.unsupported {
        if let Some(required) = bundle
            .capabilities
            .iter()
            .find(|kept| &kept.capability == capability && kept.obligation == "required")
        {
            divergence(
                &evidence.port,
                "required-capability-unsupported",
                Some(required.obligation.clone()),
                Some(capability.clone()),
            );
        }
    }
    for observation in &evidence.observations {
        let Some(export) = bundle
            .exports
            .iter()
            .find(|export| export.export == observation.operation)
            .cloned()
        else {
            divergence(
                &observation.operation,
                "operation",
                Some("exported".to_owned()),
                None,
            );
            continue;
        };
        if export.kind != observation.kind {
            divergence(
                &observation.operation,
                "kind",
                Some(export.kind.clone()),
                Some(observation.kind.clone()),
            );
            continue;
        }
        match observation.kind.as_str() {
            "command" => {
                if export.input != observation.inputs {
                    divergence(
                        &observation.operation,
                        "inputs",
                        Some(type_list_text(&export.input)),
                        Some(type_list_text(&observation.inputs)),
                    );
                }
                if export.effects != observation.effects {
                    divergence(
                        &observation.operation,
                        "effects",
                        Some(wire::canonical(&export.effects)),
                        Some(wire::canonical(&observation.effects)),
                    );
                }
                if observation.access != "write" {
                    divergence(
                        &observation.operation,
                        "access",
                        Some("write".to_owned()),
                        Some(observation.access.clone()),
                    );
                }
            }
            "query" => {
                if export.returns != observation.results {
                    divergence(
                        &observation.operation,
                        "results",
                        export.returns.as_ref().map(wire::canonical),
                        observation.results.as_ref().map(wire::canonical),
                    );
                }
                if !observation.effects.is_empty() {
                    divergence(
                        &observation.operation,
                        "effects",
                        Some(String::new()),
                        Some(wire::canonical(&observation.effects)),
                    );
                }
                if observation.access != "read" {
                    divergence(
                        &observation.operation,
                        "access",
                        Some("read".to_owned()),
                        Some(observation.access.clone()),
                    );
                }
            }
            _ => {}
        }
        if export.errors != observation.errors {
            divergence(
                &observation.operation,
                "errors",
                Some(export.errors.join(",")),
                Some(observation.errors.join(",")),
            );
        }
        for capability in &observation.capabilities {
            if !capability_ids.contains(capability.as_str()) {
                divergence(
                    &observation.operation,
                    "capability",
                    Some("declared-capability".to_owned()),
                    Some(capability.clone()),
                );
            }
        }
    }
}

/// The bounded text of one observed input list.
fn type_list_text(fields: &[super::wire::PublicField]) -> String {
    wire::canonical(&fields)
}

/// The observation rows of one evidence document keyed by operation.
fn observations_by_operation(
    evidence: &MappingEvidence,
) -> std::collections::BTreeMap<&str, &MappingObservation> {
    evidence
        .observations
        .iter()
        .map(|observation| (observation.operation.as_str(), observation))
        .collect()
}
