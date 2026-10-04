use std::collections::BTreeSet;

use serde::Serialize;

use super::{diagnostic, json, wire::*};
use crate::diagnostics::DiagnosticSet;
use crate::trace::id;

pub(super) fn digest<T: Serialize>(value: &T) -> String {
    let canonical =
        serde_json::to_string(&serde_json::to_value(value).expect("typed evidence serializes"))
            .expect("canonical JSON serializes");
    format!("sha256:{}", crate::digest::sha256_hex(canonical.as_bytes()))
}

/// Stable mapping identity excludes provider outcomes and policy. Scope and
/// chain arrays are normalized before hashing; ordered source scenario steps
/// never enter this document and are never reordered.
pub(super) fn mapping_digest(input: &EvidenceDocument) -> String {
    digest(&(
        "lekalo/trace-validation-evidence/v0.6.4/mapping",
        &input.scope,
        &input.chains,
    ))
}

fn unique<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

fn refs(values: &[String], max: usize, check: fn(&str) -> bool) -> bool {
    values.len() <= max && values.iter().all(|value| check(value)) && unique(values.iter())
}

fn pin(pin: &Pin) -> bool {
    id::is_adapter_id(&pin.id)
        && id::is_contract_version(&pin.version)
        && pin.version.len() <= 32
        && id::is_digest(&pin.digest)
}

fn model(reference: &crate::trace::ContractRef) -> bool {
    reference.schema_version == "0.2.16" && id::is_digest(&reference.digest)
}

fn code(text: &str) -> bool {
    (1..=128).contains(&text.len()) && id::is_semantic_id(text)
}

pub(super) fn parse(bytes: &[u8]) -> Result<EvidenceDocument, DiagnosticSet> {
    if bytes.len() > MAX_BYTES {
        return Err(diagnostic::invalid("document-over-limit"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| diagnostic::invalid("encoding"))?;
    let value = json::parse(text)?;
    let mut input: EvidenceDocument =
        serde_json::from_value(value).map_err(|_| diagnostic::invalid("closed-wire"))?;
    if input.schema_version != EVIDENCE_SCHEMA || input.identity != EVIDENCE_IDENTITY {
        return Err(diagnostic::invalid("contract-identity"));
    }
    if !id::is_semantic_id(&input.project_ref)
        || !id::is_revision(&input.source_revision)
        || !model(&input.model_ref)
        || !id::is_digest(&input.working_set_digest)
        || !id::is_digest(&input.trace_digest)
        || !id::is_digest(&input.mapping_digest)
    {
        return Err(diagnostic::invalid("input-pins"));
    }
    let scope = &input.scope;
    if scope.requirements.is_empty()
        || scope.artifacts.is_empty()
        || scope.scenarios.is_empty()
        || scope.requirements.len() + scope.artifacts.len() + scope.scenarios.len() > MAX_ROWS
        || !refs(&scope.requirements, MAX_ROWS, id::is_node_id)
        || !refs(&scope.artifacts, MAX_ROWS, id::is_node_id)
        || !refs(&scope.scenarios, MAX_ROWS, id::is_node_id)
        || input.chains.len() > MAX_ROWS
        || input.evidence.len() > MAX_EVIDENCE
        || !unique(input.required_providers.iter().map(|p| p.as_str()))
        || input.provider_pins.len() > 5
        || !unique(input.provider_pins.iter().map(|p| p.provider.as_str()))
        || input
            .provider_pins
            .iter()
            .any(|p| !pin(&p.tool) || !pin(&p.protocol))
    {
        return Err(diagnostic::invalid("scope-or-bound"));
    }
    for chain in &input.chains {
        if !id::is_node_id(&chain.id)
            || !id::is_occurrence(&chain.occurrence)
            || [
                &chain.requirement,
                &chain.symbol,
                &chain.artifact,
                &chain.scenario,
            ]
            .iter()
            .any(|s| !id::is_node_id(s))
            || chain.test.as_ref().is_some_and(|s| !id::is_node_id(s))
            || chain.gate.as_ref().is_some_and(|s| !id::is_node_id(s))
            || !refs(&chain.relation_refs, 5, id::is_digest)
            || !refs(&chain.evidence_refs, 16, id::is_node_id)
        {
            return Err(diagnostic::invalid("chain-shape"));
        }
    }
    if !unique(input.chains.iter().map(|c| &c.id)) || !unique(input.evidence.iter().map(|r| &r.id))
    {
        return Err(diagnostic::invalid("duplicate-identity"));
    }
    let mut diagnostics = 0;
    for receipt in &mut input.evidence {
        diagnostics += receipt.diagnostics.len();
        if !id::is_node_id(&receipt.id)
            || !id::is_revision(&receipt.source_revision)
            || !model(&receipt.model_ref)
            || !id::is_digest(&receipt.working_set_digest)
            || !id::is_digest(&receipt.result_digest)
            || !pin(&receipt.protocol)
            || receipt.tool.as_ref().is_some_and(|p| !pin(p))
            || (matches!(
                receipt.outcome,
                Outcome::Pass | Outcome::Warn | Outcome::Fail
            ) && receipt.tool.is_none())
            || receipt.diagnostics.len() > 128
            || diagnostics > MAX_EVIDENCE
            || receipt
                .diagnostics
                .iter()
                .any(|d| !code(&d.code) || !id::is_node_id(&d.subject))
        {
            return Err(diagnostic::invalid("receipt-shape"));
        }
        let has_error = receipt
            .diagnostics
            .iter()
            .any(|d| d.severity == OriginalSeverity::Error);
        let has_warning = receipt
            .diagnostics
            .iter()
            .any(|d| d.severity == OriginalSeverity::Warning);
        if (receipt.outcome == Outcome::Pass && (has_error || has_warning))
            || (receipt.outcome == Outcome::Warn && has_error)
        {
            return Err(diagnostic::invalid("contradictory-receipt"));
        }
        match receipt.kind {
            EvidenceKind::Check => {
                if receipt.gate.is_some()
                    || receipt.test.is_some()
                    || receipt.artifact.is_some()
                    || receipt.artifact_digest.is_some()
                {
                    return Err(diagnostic::invalid("check-execution-fields"));
                }
            }
            EvidenceKind::Execution => {
                if [&receipt.gate, &receipt.test, &receipt.artifact]
                    .iter()
                    .any(|v| !v.as_ref().is_some_and(|s| id::is_node_id(s)))
                    || !receipt
                        .artifact_digest
                        .as_ref()
                        .is_some_and(|s| id::is_digest(s))
                {
                    return Err(diagnostic::invalid("execution-fields"));
                }
            }
        }
        receipt.diagnostics.sort_by(|a, b| {
            (&a.code, &a.subject, severity_rank(a.severity)).cmp(&(
                &b.code,
                &b.subject,
                severity_rank(b.severity),
            ))
        });
    }
    input.scope.requirements.sort();
    input.scope.artifacts.sort();
    input.scope.scenarios.sort();
    input.required_providers.sort_by_key(|p| p.as_str());
    input.provider_pins.sort_by_key(|p| p.provider.as_str());
    for chain in &mut input.chains {
        chain.relation_refs.sort();
        chain.evidence_refs.sort();
    }
    input.chains.sort_by(|a, b| a.id.cmp(&b.id));
    input.evidence.sort_by(|a, b| a.id.cmp(&b.id));
    if input.mapping_digest != mapping_digest(&input) {
        return Err(diagnostic::invalid("mapping-digest"));
    }
    Ok(input)
}

fn severity_rank(severity: OriginalSeverity) -> u8 {
    match severity {
        OriginalSeverity::Error => 0,
        OriginalSeverity::Warning => 1,
        OriginalSeverity::Info => 2,
    }
}
