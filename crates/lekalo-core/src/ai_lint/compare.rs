//! Baselines are strict immutable reports. Revision changes are expected;
//! changes to rules, coverage or selection make the series incomparable.
use super::{diagnostic::failure, input, wire::*};
use crate::result::DomainResult;
use std::collections::BTreeSet;
pub fn validate_report(r: &Report) -> Result<(), DomainResult> {
    input::check_header("report", &r.schema_version, &r.identity)?;
    let bad = || failure("ai-lint.input-invalid", "baseline-arithmetic-or-identity");
    if r.registry_ref != input::registry_ref()
        || !input::sorted(&r.scope)
        || r.scope.is_empty()
        || !input::sorted(&r.findings.iter().map(|f| &f.id).collect::<Vec<_>>())
        || !input::sorted(&r.evidence_refs)
        || !input::sorted(&r.attachment_refs)
        || r.evidence_refs
            .iter()
            .chain(&r.attachment_refs)
            .any(|s| !input::is_digest(s))
        || r.waiver_ref.known().is_some_and(|s| !input::is_digest(s))
        || !input::sorted(
            &r.metrics
                .iter()
                .map(|m| (&m.rule, m.confidence))
                .collect::<Vec<_>>(),
        )
        || !input::sorted(
            &r.coverage
                .iter()
                .map(|c| (&c.rule, &c.target, &c.scope))
                .collect::<Vec<_>>(),
        )
        || !input::sorted(
            &r.depths
                .iter()
                .map(|d| (&d.dimension, &d.target))
                .collect::<Vec<_>>(),
        )
        || !input::is_digest(&r.model_ref)
        || !input::is_digest(&r.ir_ref)
        || !input::is_digest(&r.config_ref)
        || !input::is_digest(&r.profile_ref)
    {
        return Err(bad());
    }
    if r.as_of.known().is_some_and(|s| !input::date(s))
        || !["off", "advisory", "ci"].contains(&r.profile.as_str())
        || !["check", "advisory"].contains(&r.mode.as_str())
        || r.recipes != ["ai-readability/1", "semantic-dependency/1", "native-call/1"]
    {
        return Err(bad());
    }
    let registry = crate::diagnostics::registry::DiagnosticRegistry::embedded().expect("registry");
    for f in &r.findings {
        if !super::RULES.contains(&f.rule_id.as_str())
            || registry
                .entry(&f.rule_id)
                .map_or(true, |e| e.code() != f.code)
            || f.id != input::hash(&(&f.rule_id, &f.subject, &f.target, &f.semantic_symbol))
            || !input::is_digest(&f.condition_digest)
            || f.scope != r.scope
            || f.evidence.is_empty()
            || f.claim == Claim::VerifiedBehavior
            || (f.claim == Claim::PossibleBehavior && f.confidence == Confidence::Exact)
            || (f.confidence <= Confidence::Low && f.severity != "info")
            || !["warning", "info"].contains(&f.severity.as_str())
            || !["active", "waived"].contains(&f.disposition.as_str())
            || ((f.disposition == "waived") != f.waiver.known().is_some())
        {
            return Err(bad());
        }
    }
    let waived = r
        .findings
        .iter()
        .filter(|f| f.disposition == "waived")
        .count() as u64;
    if r.summary.raw != r.findings.len() as u64
        || r.summary.waived != waived
        || r.summary.active != r.summary.raw - waived
        || r.summary.verified_effects != 0
        || r.summary.possible_effects
            != r.findings
                .iter()
                .filter(|f| {
                    ["hidden.observer-write", "hidden.undeclared-effect"]
                        .contains(&f.rule_id.as_str())
                        && f.claim == Claim::PossibleBehavior
                })
                .count() as u64
        || r.summary.ambiguity_sets
            != r.findings
                .iter()
                .filter(|f| f.rule_id == "ambiguity.multiple-resolutions")
                .count() as u64
        || r.summary.diagnostic_projection_truncated != (r.summary.active > 252)
    {
        return Err(bad());
    }
    let mut coverage = BTreeSet::new();
    for c in &r.coverage {
        input::validate_coverage(c)?;
        if !super::RULES.contains(&c.rule.as_str())
            || !r.scope.contains(&c.scope)
            || !coverage.insert((&c.rule, &c.target, &c.scope))
        {
            return Err(bad());
        }
    }
    let mut metrics = BTreeSet::new();
    for m in &r.metrics {
        let rows: Vec<_> = r
            .findings
            .iter()
            .filter(|f| f.rule_id == m.rule && f.confidence == m.confidence)
            .collect();
        let waived = rows.iter().filter(|f| f.disposition == "waived").count() as u64;
        if !super::RULES.contains(&m.rule.as_str())
            || !metrics.insert((&m.rule, m.confidence))
            || m.raw != rows.len() as u64
            || m.waived != waived
            || m.active != m.raw - waived
        {
            return Err(bad());
        }
    }
    if metrics.len() != super::RULES.len() * 5 {
        return Err(bad());
    }
    let mut dimensions = BTreeSet::new();
    for d in &r.depths {
        if !["semantic-dependency", "native-call"].contains(&d.dimension.as_str())
            || !dimensions.insert((&d.dimension, &d.target))
            || d.maximum
                .known()
                .is_some_and(|m| *m > 0 && d.witness.len() != *m as usize + 1)
        {
            return Err(bad());
        }
    }
    let mut waivers = BTreeSet::new();
    for w in &r.waiver_audit {
        if !waivers.insert(&w.id)
            || ![
                "applied",
                "expired",
                "condition-changed",
                "orphan",
                "source-changed",
            ]
            .contains(&w.disposition.as_str())
            || w.finding
                .known()
                .is_some_and(|id| !r.findings.iter().any(|f| &f.id == id))
            || (w.disposition == "applied"
                && !r.findings.iter().any(|f| f.waiver.known() == Some(&w.id)))
        {
            return Err(bad());
        }
    }
    if let Some(c) = r.comparison.known() {
        validate_comparison(c)?;
        let mut content = r.clone();
        content.comparison = State::Unknown;
        if c.candidate_ref != input::hash(&content)
            || c.new_findings
                .iter()
                .chain(&c.retained_findings)
                .any(|id| !r.findings.iter().any(|f| &f.id == id))
        {
            return Err(bad());
        }
    }
    Ok(())
}
fn coverage_signature(r: &Report) -> Vec<(&str, &str, &str, CoverageState)> {
    r.coverage
        .iter()
        .map(|c| {
            (
                c.rule.as_str(),
                c.target.as_str(),
                c.scope.as_str(),
                c.state,
            )
        })
        .collect()
}
pub fn compare(base: &Report, candidate: &Report) -> Comparison {
    let mut reasons = Vec::new();
    if base.registry_ref != candidate.registry_ref {
        reasons.push("registry-changed".into());
    }
    if base.profile_ref != candidate.profile_ref
        || base.config_ref != candidate.config_ref
        || base.recipes != candidate.recipes
    {
        reasons.push("configuration-or-recipe-changed".into());
    }
    if base.scope != candidate.scope {
        reasons.push("selection-changed".into());
    }
    if coverage_signature(base) != coverage_signature(candidate)
        || base
            .coverage
            .iter()
            .chain(&candidate.coverage)
            .any(|c| !matches!(c.state, CoverageState::Complete | CoverageState::Disabled))
    {
        reasons.push("coverage-incomparable".into());
    }
    if base
        .depths
        .iter()
        .map(|d| (&d.dimension, &d.target))
        .collect::<Vec<_>>()
        != candidate
            .depths
            .iter()
            .map(|d| (&d.dimension, &d.target))
            .collect::<Vec<_>>()
        || base
            .depths
            .iter()
            .chain(&candidate.depths)
            .any(|d| d.maximum.known().is_none())
    {
        reasons.push("depth-incomparable".into());
    }
    let comparable = reasons.is_empty();
    let h = input::header("comparison");
    let mut c = Comparison {
        schema_version: h.0,
        identity: h.1,
        baseline_ref: input::hash(base),
        candidate_ref: input::hash(candidate),
        comparable,
        reasons,
        deltas: Vec::new(),
        depth_deltas: Vec::new(),
        uncovered_writer_groups_delta: State::Unknown,
        new_findings: Vec::new(),
        resolved_findings: Vec::new(),
        retained_findings: Vec::new(),
        regression: State::Unknown,
    };
    if comparable {
        let a: BTreeSet<_> = base.findings.iter().map(|f| f.id.clone()).collect();
        let b: BTreeSet<_> = candidate.findings.iter().map(|f| f.id.clone()).collect();
        c.new_findings = b.difference(&a).cloned().collect();
        c.resolved_findings = a.difference(&b).cloned().collect();
        c.retained_findings = a.intersection(&b).cloned().collect();
        for (old, new) in base.metrics.iter().zip(&candidate.metrics) {
            c.deltas.push(Delta {
                rule: new.rule.clone(),
                confidence: new.confidence,
                raw: new.raw as i64 - old.raw as i64,
                active: new.active as i64 - old.active as i64,
                waived: new.waived as i64 - old.waived as i64,
            });
        }
        for (old, new) in base.depths.iter().zip(&candidate.depths) {
            let before = *old.maximum.known().expect("comparable depth");
            let after = *new.maximum.known().expect("comparable depth");
            c.depth_deltas.push(DepthDelta {
                dimension: new.dimension.clone(),
                target: new.target.clone(),
                before,
                after,
                change: after as i64 - before as i64,
                recursive_components: new.recursive_components as i64
                    - old.recursive_components as i64,
            });
        }
        if let Some((old, new)) = base
            .summary
            .uncovered_writer_groups
            .known()
            .zip(candidate.summary.uncovered_writer_groups.known())
        {
            c.uncovered_writer_groups_delta = State::Known(*new as i64 - *old as i64);
        }
        // Waiver changes never turn raw deterioration into improvement.
        c.regression = State::Known(regressed(&c));
    }
    c
}
fn regressed(c: &Comparison) -> bool {
    c.deltas.iter().any(|d| d.raw > 0)
        || c.depth_deltas
            .iter()
            .any(|d| d.change > 0 || d.recursive_components > 0)
        || c.uncovered_writer_groups_delta
            .known()
            .is_some_and(|d| *d > 0)
}
pub fn validate_comparison(c: &Comparison) -> Result<(), DomainResult> {
    input::check_header("comparison", &c.schema_version, &c.identity)?;
    let bad = || failure("ai-lint.input-invalid", "comparison-arithmetic");
    if !input::is_digest(&c.baseline_ref)
        || !input::is_digest(&c.candidate_ref)
        || c.comparable != c.reasons.is_empty()
        || c.comparable != c.regression.known().is_some()
        || !input::sorted(&c.new_findings)
        || !input::sorted(&c.resolved_findings)
        || !input::sorted(&c.retained_findings)
    {
        return Err(bad());
    }
    if c.comparable {
        let mut keys = BTreeSet::new();
        for d in &c.deltas {
            if !super::RULES.contains(&d.rule.as_str())
                || !keys.insert((&d.rule, d.confidence))
                || d.raw != d.active + d.waived
            {
                return Err(bad());
            }
        }
        let mut dimensions = BTreeSet::new();
        for d in &c.depth_deltas {
            if !["semantic-dependency", "native-call"].contains(&d.dimension.as_str())
                || !dimensions.insert((&d.dimension, &d.target))
                || d.change != d.after as i64 - d.before as i64
            {
                return Err(bad());
            }
        }
        if keys.len() != super::RULES.len() * 5 || c.regression != State::Known(regressed(c)) {
            return Err(bad());
        }
    } else if !c.depth_deltas.is_empty()
        || c.uncovered_writer_groups_delta.known().is_some()
        || !c.deltas.is_empty()
        || !c.new_findings.is_empty()
        || !c.resolved_findings.is_empty()
        || !c.retained_findings.is_empty()
    {
        return Err(bad());
    }
    let mut ids = BTreeSet::new();
    for id in c
        .new_findings
        .iter()
        .chain(&c.resolved_findings)
        .chain(&c.retained_findings)
    {
        if !input::is_digest(id) || !ids.insert(id) {
            return Err(bad());
        }
    }
    Ok(())
}
