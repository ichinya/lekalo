//! Provider-neutral strict chain assessment (issue #35).
//!
//! The external orchestration owner selects the scope, mapping, current input
//! pins and receipts. This service never parses an HLV artifact, resolves a
//! layout, reads Git, runs a provider/test, syncs a requirement or writes a file.
//! A complete trace's reachability is insufficient: every declared chain must
//! prove all five role-specific relations, including the binding branch.

mod diagnostic;
mod json;
mod validate;
pub mod wire;

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::{Diagnostic, DiagnosticSet};
use crate::result::{DomainResult, Status as ResultStatus};
use crate::trace::{Node, NodeKind, Relation, RelationKind, Status, TraceManifest};
pub use wire::*;

/// Parse and assess explicitly supplied evidence against an accepted trace.
pub fn assess(trace: &TraceManifest, bytes: &[u8]) -> Result<AssessmentReport, DiagnosticSet> {
    let mut input = validate::parse(bytes)?;
    let manifest = trace.manifest();
    let trace_digest = trace.digest()?;
    let nodes: BTreeMap<&str, &Node> = manifest
        .nodes
        .iter()
        .map(|n| (n.node_id.as_str(), n))
        .collect();
    let relations: BTreeMap<&str, &Relation> = manifest
        .relations
        .iter()
        .map(|r| (r.relation_id.as_str(), r))
        .collect();
    let receipts: BTreeMap<String, Evidence> = input
        .evidence
        .iter()
        .map(|r| (r.id.clone(), r.clone()))
        .collect();
    let mut findings = Vec::new();
    let current = manifest.source_revision == input.source_revision
        && manifest.model_ref == input.model_ref
        && manifest.project_ref == input.project_ref
        && trace_digest == input.trace_digest;
    if !current {
        finding(
            &mut findings,
            diagnostic::STALE,
            &input.project_ref,
            "manifest-pins",
        );
    }
    if !manifest.gaps.is_empty() {
        finding(
            &mut findings,
            diagnostic::UNCOVERED,
            &input.project_ref,
            "manifest-gaps",
        );
    }
    let mut chains = Vec::new();
    let scope_requirements: BTreeSet<_> = input.scope.requirements.iter().collect();
    let scope_artifacts: BTreeSet<_> = input.scope.artifacts.iter().collect();
    let scope_scenarios: BTreeSet<_> = input.scope.scenarios.iter().collect();
    let mut selected_receipts = BTreeSet::new();
    let mut mappings = BTreeSet::new();
    for chain in &input.chains {
        let before = findings.len();
        if !mappings.insert((
            &chain.occurrence,
            &chain.requirement,
            &chain.symbol,
            &chain.artifact,
            &chain.scenario,
            &chain.test,
            &chain.gate,
            &chain.relation_refs,
        )) {
            finding(
                &mut findings,
                diagnostic::CONFLICT,
                &chain.id,
                "duplicate-chain-mapping",
            );
        }
        for (key, kind) in [
            (&chain.requirement, NodeKind::Requirement),
            (&chain.symbol, NodeKind::Symbol),
            (&chain.artifact, NodeKind::Artifact),
            (&chain.scenario, NodeKind::Scenario),
        ] {
            if !nodes.get(key.as_str()).is_some_and(|n| n.node_kind == kind) {
                finding(
                    &mut findings,
                    diagnostic::UNRESOLVED,
                    &chain.id,
                    kind.as_str(),
                );
            }
        }
        if !scope_requirements.contains(&chain.requirement)
            || !scope_artifacts.contains(&chain.artifact)
            || !scope_scenarios.contains(&chain.scenario)
        {
            finding(
                &mut findings,
                diagnostic::CONFLICT,
                &chain.id,
                "chain-outside-scope",
            );
        }
        let mut required = vec![
            (
                RelationKind::Implements,
                chain.symbol.as_str(),
                chain.requirement.as_str(),
            ),
            (
                RelationKind::Binds,
                chain.symbol.as_str(),
                chain.artifact.as_str(),
            ),
            (
                RelationKind::Covers,
                chain.scenario.as_str(),
                chain.symbol.as_str(),
            ),
        ];
        for (key, kind) in [
            (chain.test.as_ref(), NodeKind::NativeTest),
            (chain.gate.as_ref(), NodeKind::Gate),
        ] {
            match key {
                None => finding(&mut findings, diagnostic::MISSING, &chain.id, kind.as_str()),
                Some(key) if !nodes.get(key.as_str()).is_some_and(|n| n.node_kind == kind) => {
                    finding(
                        &mut findings,
                        diagnostic::UNRESOLVED,
                        &chain.id,
                        kind.as_str(),
                    )
                }
                Some(_) => {}
            }
        }
        if let Some(test) = &chain.test {
            required.push((RelationKind::Verifies, test, &chain.scenario));
            if let Some(gate) = &chain.gate {
                required.push((RelationKind::Evidences, gate, test));
            }
        }
        let mut matched = BTreeSet::new();
        for (kind, from, to) in required {
            let hits: Vec<_> = chain
                .relation_refs
                .iter()
                .filter_map(|key| relations.get(key.as_str()).copied())
                .filter(|r| r.relation_kind == kind && r.from_node == from && r.to_node == to)
                .collect();
            if hits.len() != 1 {
                finding(
                    &mut findings,
                    if hits.len() > 1 {
                        diagnostic::CONFLICT
                    } else {
                        diagnostic::MISSING
                    },
                    &chain.id,
                    kind.as_str(),
                );
            } else {
                let hit = hits[0];
                matched.insert(hit.relation_id.as_str());
                if hit.status != Status::Confirmed {
                    finding(
                        &mut findings,
                        if hit.status == Status::Conflicting {
                            diagnostic::CONFLICT
                        } else {
                            diagnostic::STALE
                        },
                        &chain.id,
                        "relation-unconfirmed",
                    );
                }
            }
        }
        if matched.len() != chain.relation_refs.len() {
            finding(
                &mut findings,
                diagnostic::CONFLICT,
                &chain.id,
                "unrelated-relation-ref",
            );
        }
        if let Some(artifact) = nodes.get(chain.artifact.as_str()) {
            if artifact.revision.as_deref() != Some(&input.source_revision) {
                finding(
                    &mut findings,
                    diagnostic::STALE,
                    &chain.id,
                    "artifact-revision",
                );
            }
        }
        if !current {
            finding(&mut findings, diagnostic::STALE, &chain.id, "manifest-pins");
        }
        let needs_hlv_mapping = input
            .required_providers
            .contains(&crate::trace::ExternalSystem::Hlv)
            || chain
                .evidence_refs
                .iter()
                .filter_map(|id| receipts.get(id))
                .any(|r| r.provider == crate::trace::ExternalSystem::Hlv);
        if needs_hlv_mapping {
            for (role, key) in [
                ("external-test-mapping", chain.test.as_ref()),
                ("external-gate-mapping", chain.gate.as_ref()),
            ] {
                let aliases: Vec<_> = key
                    .and_then(|id| nodes.get(id.as_str()))
                    .map(|node| {
                        node.external_refs
                            .iter()
                            .filter(|r| r.system == crate::trace::ExternalSystem::Hlv)
                            .collect()
                    })
                    .unwrap_or_default();
                if aliases.len() != 1 {
                    finding(
                        &mut findings,
                        if aliases.len() > 1 {
                            diagnostic::CONFLICT
                        } else {
                            diagnostic::MISSING
                        },
                        &chain.id,
                        role,
                    );
                } else if aliases[0].revision.as_deref() != Some(input.source_revision.as_str()) {
                    finding(&mut findings, diagnostic::STALE, &chain.id, role);
                }
            }
        }
        let coverage = coverage_of(&findings[before..]);
        let mut selected = Vec::new();
        for key in &chain.evidence_refs {
            match receipts.get(key) {
                Some(receipt) => {
                    selected.push(receipt);
                    selected_receipts.insert(key);
                }
                None => finding(
                    &mut findings,
                    diagnostic::UNRESOLVED,
                    &chain.id,
                    "receipt-missing",
                ),
            }
        }
        let fresh = |r: &&Evidence| {
            r.source_revision == input.source_revision
                && r.model_ref == input.model_ref
                && r.working_set_digest == input.working_set_digest
        };
        let negotiated = |r: &Evidence| {
            input.provider_pins.iter().any(|p| {
                p.provider == r.provider
                    && p.protocol == r.protocol
                    && r.tool.as_ref() == Some(&p.tool)
            })
        };
        for receipt in &selected {
            if !fresh(receipt) {
                finding(&mut findings, diagnostic::STALE, &chain.id, "receipt-pins");
            }
            if !negotiated(receipt) {
                finding(
                    &mut findings,
                    diagnostic::PROVIDER,
                    &chain.id,
                    "provider-pins-unqualified",
                );
            }
            if receipt.kind == EvidenceKind::Check && receipt.outcome != Outcome::Pass {
                finding(
                    &mut findings,
                    diagnostic::PROVIDER,
                    &chain.id,
                    "provider-check-not-passed",
                );
            }
            if receipt.kind == EvidenceKind::Execution {
                if chain
                    .gate
                    .as_ref()
                    .is_some_and(|id| receipt.gate.as_ref() != Some(id))
                    || chain
                        .test
                        .as_ref()
                        .is_some_and(|id| receipt.test.as_ref() != Some(id))
                    || receipt.artifact.as_ref() != Some(&chain.artifact)
                {
                    finding(
                        &mut findings,
                        diagnostic::CONFLICT,
                        &chain.id,
                        "unrelated-execution-ref",
                    );
                }
                if !input.require_execution && receipt.outcome != Outcome::Pass {
                    finding(
                        &mut findings,
                        diagnostic::EXECUTION,
                        &chain.id,
                        "execution-not-passed",
                    );
                }
            }
        }
        for provider in &input.required_providers {
            let checks: Vec<_> = selected
                .iter()
                .copied()
                .filter(|r| r.provider == *provider && r.kind == EvidenceKind::Check)
                .collect();
            if checks.len() != 1 {
                finding(
                    &mut findings,
                    if checks.len() > 1 {
                        diagnostic::CONFLICT
                    } else {
                        diagnostic::PROVIDER
                    },
                    &chain.id,
                    "provider-check-missing-or-ambiguous",
                );
            } else if !fresh(&checks[0])
                || !negotiated(checks[0])
                || checks[0].outcome != Outcome::Pass
            {
                finding(
                    &mut findings,
                    diagnostic::PROVIDER,
                    &chain.id,
                    "provider-check-not-passed",
                );
            }
        }
        let execution = if !input.require_execution {
            Execution::NotRequested
        } else {
            let executions: Vec<_> = selected
                .iter()
                .copied()
                .filter(|r| {
                    r.kind == EvidenceKind::Execution
                        && r.gate == chain.gate
                        && r.test == chain.test
                        && r.artifact.as_ref() == Some(&chain.artifact)
                })
                .collect();
            if executions.len() != 1 {
                finding(
                    &mut findings,
                    if executions.len() > 1 {
                        diagnostic::CONFLICT
                    } else {
                        diagnostic::EXECUTION
                    },
                    &chain.id,
                    "execution-missing-or-ambiguous",
                );
                Execution::Unverified
            } else {
                let receipt = executions[0];
                let digest_matches = nodes
                    .get(chain.artifact.as_str())
                    .is_some_and(|n| n.content_digest == receipt.artifact_digest)
                    && chain
                        .gate
                        .as_ref()
                        .and_then(|id| nodes.get(id.as_str()))
                        .is_some_and(|n| {
                            n.evidence_digest.as_ref() == Some(&receipt.result_digest)
                        });
                if !current
                    || !fresh(&receipt)
                    || !negotiated(receipt)
                    || !digest_matches
                    || coverage != Coverage::Complete
                {
                    finding(
                        &mut findings,
                        diagnostic::EXECUTION,
                        &chain.id,
                        "execution-binding-unverified",
                    );
                    Execution::Unverified
                } else if receipt.outcome == Outcome::Pass {
                    Execution::Passed
                } else {
                    finding(
                        &mut findings,
                        diagnostic::EXECUTION,
                        &chain.id,
                        "execution-not-passed",
                    );
                    if receipt.outcome == Outcome::Fail {
                        Execution::Failed
                    } else {
                        Execution::Unverified
                    }
                }
            }
        };
        chains.push(ChainReport {
            chain: chain.clone(),
            coverage: coverage_of(&findings[before..]),
            execution,
        });
    }
    for receipt in &input.evidence {
        if !selected_receipts.contains(&receipt.id) {
            finding(
                &mut findings,
                diagnostic::CONFLICT,
                &receipt.id,
                "unreferenced-receipt",
            );
        }
    }
    let covered_requirements: BTreeSet<_> = chains
        .iter()
        .filter(|c| c.coverage == Coverage::Complete)
        .map(|c| &c.chain.requirement)
        .collect();
    let covered_artifacts: BTreeSet<_> = chains
        .iter()
        .filter(|c| c.coverage == Coverage::Complete)
        .map(|c| &c.chain.artifact)
        .collect();
    let covered_scenarios: BTreeSet<_> = chains
        .iter()
        .filter(|c| c.coverage == Coverage::Complete)
        .map(|c| &c.chain.scenario)
        .collect();
    for (scope, kind) in [
        (&input.scope.requirements, NodeKind::Requirement),
        (&input.scope.artifacts, NodeKind::Artifact),
        (&input.scope.scenarios, NodeKind::Scenario),
    ] {
        for id in scope {
            if !nodes.get(id.as_str()).is_some_and(|n| n.node_kind == kind) {
                finding(&mut findings, diagnostic::UNRESOLVED, id, "scope-node");
            }
            let covered = match kind {
                NodeKind::Requirement => covered_requirements.contains(id),
                NodeKind::Artifact => covered_artifacts.contains(id),
                _ => covered_scenarios.contains(id),
            };
            if !covered {
                finding(
                    &mut findings,
                    diagnostic::UNCOVERED,
                    id,
                    "selected-role-uncovered",
                );
            }
        }
    }
    findings
        .sort_by(|a, b| (&a.rule, &a.subject, &a.detail).cmp(&(&b.rule, &b.subject, &b.detail)));
    findings.dedup();
    let mut diagnostics: Vec<Diagnostic> = findings
        .iter()
        .map(|f| f.rule.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|rule| diagnostic::make(rule, "see-assessment-findings", &input.project_ref))
        .collect();
    let verdict = if findings.is_empty() {
        Verdict::Ready
    } else if input.policy == Policy::Required {
        Verdict::Blocked
    } else {
        Verdict::Degraded
    };
    if verdict == Verdict::Blocked {
        diagnostics.push(diagnostic::make(
            diagnostic::DENIED,
            "required-evidence",
            &input.project_ref,
        ));
    }
    let status = if verdict == Verdict::Blocked {
        ResultStatus::Denied
    } else {
        ResultStatus::Valid
    };
    let diagnostics = DiagnosticSet::try_from_unsorted(diagnostics, status)
        .expect("registered assessment diagnostics")
        .as_slice()
        .to_vec();
    let evidence_digest = validate::digest(&input);
    let report = AssessmentReport {
        schema_version: REPORT_SCHEMA,
        identity: REPORT_IDENTITY,
        project_ref: input.project_ref,
        source_revision: input.source_revision,
        model_ref: input.model_ref,
        working_set_digest: input.working_set_digest,
        trace_digest,
        mapping_digest: input.mapping_digest,
        evidence_digest,
        expected_trace_digest: input.trace_digest,
        policy: input.policy,
        required_providers: std::mem::take(&mut input.required_providers),
        require_execution: input.require_execution,
        scope: input.scope,
        coverage: coverage_of(&findings),
        verdict,
        provider_pins: input.provider_pins,
        chains,
        evidence: input.evidence,
        findings,
        diagnostics,
    };
    if report.canonical_json().len() > MAX_BYTES {
        return Err(diagnostic::invalid("report-over-limit"));
    }
    Ok(report)
}

fn finding(findings: &mut Vec<Finding>, rule: &str, subject: &str, detail: &str) {
    findings.push(Finding {
        rule: rule.to_owned(),
        subject: subject.to_owned(),
        detail: detail.to_owned(),
    });
}

fn coverage_of(findings: &[Finding]) -> Coverage {
    if findings.iter().any(|f| f.rule == diagnostic::CONFLICT) {
        Coverage::Conflicting
    } else if findings.iter().any(|f| {
        matches!(
            f.rule.as_str(),
            diagnostic::MISSING
                | diagnostic::UNRESOLVED
                | diagnostic::STALE
                | diagnostic::UNCOVERED
        )
    }) {
        Coverage::Partial
    } else {
        Coverage::Complete
    }
}

impl AssessmentReport {
    /// Compact sorted-key JSON; no clock, host path, or trailing LF.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(&serde_json::to_value(self).expect("typed report serializes"))
            .expect("report JSON serializes")
    }

    /// Optional assessments are informational; required failures deny while
    /// preserving the entire report and every supplied independent receipt.
    pub fn domain_result(&self) -> DomainResult {
        let json = format!(
            "{{\"status\":\"valid\",\"assessment\":{}}}",
            self.canonical_json()
        );
        let human = format!(
            "trace assessment {:?}; {:?} coverage; {} chains; {} findings",
            self.verdict,
            self.coverage,
            self.chains.len(),
            self.findings.len()
        );
        if self.verdict == Verdict::Blocked {
            let set =
                DiagnosticSet::try_from_unsorted(self.diagnostics.clone(), ResultStatus::Denied)
                    .expect("registered denial diagnostics");
            DomainResult::denied_json(json, human, set)
        } else {
            DomainResult::receipt(json, human)
        }
    }
}
