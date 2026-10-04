//! Immutable baseline validation, exact comparable deltas, and opt-in denial.
use super::{
    diagnostic,
    wire::{self, Report, State},
    Profile,
};
use crate::diagnostics::DiagnosticSet;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Delta {
    pub subject: String,
    pub metric: String,
    pub state: String,
    pub base: State<u64>,
    pub candidate: State<u64>,
    pub absolute: State<i64>,
    pub relative: State<super::profile::Rational>,
    pub verdict: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comparison {
    pub schema_version: String,
    pub identity: String,
    pub metric_version: String,
    pub baseline_digest: String,
    pub candidate_digest: String,
    pub comparable: bool,
    pub rows: Vec<Delta>,
    pub regressions: u64,
    pub reasons: Vec<String>,
}
fn sorted(values: &[String]) -> bool {
    values.windows(2).all(|w| w[0] < w[1])
        && values
            .iter()
            .all(|v| !v.is_empty() && v.len() <= 1024 && !v.chars().any(char::is_control))
}
fn count_matches(metric: &State<u64>, n: usize) -> bool {
    match metric.value() {
        Some(v) => *v == n as u64,
        None => true,
    }
}

pub fn parse_baseline(bytes: &[u8]) -> Result<Report, DiagnosticSet> {
    let report: Report = wire::decode(bytes)?;
    validate(&report)?;
    Ok(report)
}
/// Scalars are checked against their exact unique sets; paths must be ordered,
/// occurrence-safe canonical witnesses. No numeric baseline bag is accepted.
pub fn validate(r: &Report) -> Result<(), DiagnosticSet> {
    let bad = || diagnostic::invalid("baseline-invariant");
    if r.schema_version != format!("lekalo/coupling-report/v{}", wire::VERSION)
        || r.identity != format!("dev.lekalo.coupling-report@{}", wire::VERSION)
        || r.metric_version != wire::METRIC_VERSION
        || r.recipe != wire::RECIPE
    {
        return Err(diagnostic::unsupported("baseline-version-or-recipe"));
    }
    r.profile.validate()?;
    if r.subjects.len() > wire::MAX_SUBJECTS
        || r.witnesses.len() > 50_000
        || r.cycles.len() > 100_000
        || r.findings.len() > 20_000
        || r.suggestions.len() > 16
    {
        return Err(bad());
    }
    // Validate the closed metric vocabulary before any indexing, including
    // aggregate replay which reads metrics from other rows.
    if r.subjects.iter().any(|s| {
        s.metrics.len() != wire::METRICS.len()
            || wire::METRICS.iter().any(|m| !s.metrics.contains_key(*m))
            || s.metrics
                .values()
                .any(|v| v.value().is_some_and(|n| *n > 9_007_199_254_740_991))
    }) {
        return Err(bad());
    }
    let pin = &r.provenance;
    let projection = super::projection::Projection::from_snapshot(&r.projection)?;
    if pin.projection_digest != wire::digest(&r.projection) {
        return Err(bad());
    }
    let edge_map: BTreeMap<_, _> = projection.edges.iter().map(|e| (&e.key, e)).collect();
    let writers = projection.declared_writers();
    let effect_keys: BTreeSet<_> = writers
        .values()
        .flat_map(|ops| ops.values().flatten())
        .collect();
    let replay_cycles = projection.cycles();
    if replay_cycles != r.cycles {
        return Err(bad());
    }
    let mut replay_work = projection.edges.len();
    if [
        &pin.inputs.ir_digest,
        &pin.inputs.graph_digest,
        &pin.inputs.effect_digest,
        &pin.inputs.semantic_digest,
        &pin.measurement_digest,
        &pin.profile_digest,
    ]
    .iter()
    .any(|d| !wire::is_digest(d))
        || pin.profile_digest != wire::digest(&r.profile)
        || pin.measurement_digest
            != wire::digest(
                &serde_json::json!({"measurement":r.profile.measurement,"domainGroups":r.profile.domain_groups,"classification":"project-visible-contracts/1"}),
            )
    {
        return Err(bad());
    }
    if !matches!(
        r.scope.kind.as_str(),
        "symbol" | "module" | "all" | "changed"
    ) || !sorted(&r.scope.roots)
        || !sorted(
            &r.subjects
                .iter()
                .map(|s| s.subject.clone())
                .collect::<Vec<_>>(),
        )
    {
        return Err(bad());
    }
    let mut witness_map = BTreeMap::new();
    for w in &r.witnesses {
        if witness_map.insert(&w.id, w).is_some()
            || w.edges.len() > 256
            || !sorted(&w.evidence_refs)
        {
            return Err(bad());
        }
        let mut identity = w.clone();
        identity.id.clear();
        if wire::digest(&identity) != w.id {
            return Err(bad());
        }
        if !matches!(w.direction.as_str(), "reverse" | "forward" | "effects")
            || w.confidence != "canonical"
        {
            return Err(bad());
        }
        let mut node = w.root.as_str();
        for e in &w.edges {
            if edge_map.get(&e.key).copied() != Some(e) {
                return Err(bad());
            }
            if e.key != format!("{}|{}|{}|{}", e.from, e.relation, e.to, e.occurrence)
                || e.confidence != "canonical"
                || !matches!(
                    e.role.as_str(),
                    "entity-field"
                        | "value-object-field"
                        | "event-payload"
                        | "command-effect"
                        | "effect-entity"
                        | "command-input"
                        | "query-returns"
                        | "query-reads"
                        | "endpoint-invokes"
                        | "effect-emits"
                )
            {
                return Err(bad());
            }
            if w.direction == "reverse" {
                if e.to != node {
                    return Err(bad());
                }
                node = &e.from;
            } else {
                if e.from != node {
                    return Err(bad());
                }
                node = &e.to;
            }
        }
        if node != w.subject
            || w.direction == "effects" && (w.evidence_refs.is_empty() || !w.edges.is_empty())
        {
            return Err(bad());
        }
        if w.direction == "effects" && w.evidence_refs.iter().any(|k| !effect_keys.contains(k)) {
            return Err(bad());
        }
    }
    let mut union_public = BTreeSet::new();
    let mut union_internal = BTreeSet::new();
    let mut union_support = BTreeSet::new();
    let mut union_unclassified = BTreeSet::new();
    for s in &r.subjects {
        let partitions = [
            &s.impact.public_contracts,
            &s.impact.internal_symbols,
            &s.impact.supporting_symbols,
            &s.impact.unclassified_symbols,
        ];
        let all: BTreeSet<_> = s.impact.ids().collect();
        let owned: BTreeSet<_> = all.iter().map(|id| (*id).clone()).collect();
        if projection.partition(&owned) != s.impact {
            return Err(bad());
        }
        let root = s.subject.split('#').next().unwrap_or_default();
        if s.measurement_basis != "aggregate-declared"
            && !root.starts_with("type:")
            && !count_matches(&s.metrics["sharedAbstractionRadius"], 0)
        {
            return Err(bad());
        }
        if s.measurement_basis == "exact-declared" {
            let incoming = projection
                .incoming
                .get(root)
                .into_iter()
                .flatten()
                .map(|i| projection.edges[*i].from.clone())
                .filter(|id| id != root)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let outgoing = projection
                .outgoing
                .get(root)
                .into_iter()
                .flatten()
                .map(|i| projection.edges[*i].to.clone())
                .filter(|id| id != root)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if incoming != s.fan_in
                || outgoing != s.fan_out
                || projection.nodes.get(root).map(|n| n.module.as_str()) != Some(s.module.as_str())
            {
                return Err(bad());
            }
            if s.metrics["semanticSymbolsAffected"].value().is_some() {
                let walk = projection.walk(root, true, &mut replay_work);
                if !walk.complete || walk.paths.keys().cloned().collect::<BTreeSet<_>>() != owned {
                    return Err(bad());
                }
            }
            if s.metrics["publicTargetExposure"].value().is_some() {
                let walk = projection.walk(root, false, &mut replay_work);
                let expected = if projection.nodes[root].class == "public" {
                    walk.paths
                        .keys()
                        .filter(|id| *id != root && projection.nodes[*id].target_specific)
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                };
                if !walk.complete || expected != s.target_exposure {
                    return Err(bad());
                }
            }
            let expected_resources = writers
                .iter()
                .filter(|(resource, ops)| {
                    root.split_once(':').is_some_and(|(_, id)| *resource == id)
                        || ops.keys().any(|op| owned.contains(op))
                })
                .map(|(r, _)| r.clone())
                .collect::<Vec<_>>();
            let expected_shared = expected_resources
                .iter()
                .filter(|r| writers[*r].len() > 1)
                .cloned()
                .collect::<Vec<_>>();
            if expected_resources != s.resources || expected_shared != s.shared_resources {
                return Err(bad());
            }
            if root.starts_with("type:")
                && s.metrics["sharedAbstractionRadius"]
                    .value()
                    .is_some_and(|n| {
                        *n != s.impact.public_contracts.len().saturating_sub(usize::from(
                            s.impact.public_contracts.iter().any(|id| id == root),
                        )) as u64
                    })
            {
                return Err(bad());
            }
        }
        if s.measurement_basis == "owner-conservative" {
            let member = s.subject.split_once("#field.").ok_or_else(bad)?.1;
            let expected = projection
                .fields
                .get(root)
                .and_then(|fs| fs.iter().find(|(f, _)| f == member))
                .ok_or_else(bad)?
                .1
                .iter()
                .filter(|id| *id != root)
                .cloned()
                .collect::<Vec<_>>();
            if expected != s.fan_out {
                return Err(bad());
            }
        }
        if s.measurement_basis != "aggregate-declared" {
            let own_module = projection
                .nodes
                .get(root)
                .map(|n| n.module.as_str())
                .ok_or_else(bad)?;
            if !count_matches(
                &s.metrics["fanInModules"],
                projection
                    .modules(s.fan_in.iter())
                    .iter()
                    .filter(|m| m.as_str() != own_module)
                    .count(),
            ) || !count_matches(
                &s.metrics["fanOutModules"],
                projection
                    .modules(s.fan_out.iter())
                    .iter()
                    .filter(|m| m.as_str() != own_module)
                    .count(),
            ) {
                return Err(bad());
            }
        }
        if s.measurement_basis == "aggregate-declared" {
            if s.subject != format!("module:{}", s.module)
                && !(s.module.is_empty()
                    && s.subject == format!("project:{}", r.provenance.inputs.project))
            {
                return Err(bad());
            }
            let basic_owners: BTreeSet<_> = r
                .subjects
                .iter()
                .filter(|row| row.measurement_basis == "exact-declared")
                .map(|row| row.subject.as_str())
                .collect();
            let selected = r
                .subjects
                .iter()
                .filter(|row| {
                    row.measurement_basis != "aggregate-declared"
                        && (s.module.is_empty() || row.module == s.module)
                        && !(row.measurement_basis == "owner-conservative"
                            && basic_owners
                                .contains(row.subject.split('#').next().unwrap_or_default()))
                })
                .collect::<Vec<_>>();
            if selected.is_empty() {
                return Err(bad());
            }
            let roots: BTreeSet<_> = selected
                .iter()
                .map(|row| row.subject.split('#').next().unwrap_or_default().to_owned())
                .collect();
            let ids: BTreeSet<_> = selected
                .iter()
                .flat_map(|row| row.impact.ids().cloned())
                .collect();
            let incoming = selected
                .iter()
                .flat_map(|row| row.fan_in.iter().cloned())
                .filter(|id| !roots.contains(id))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let outgoing = selected
                .iter()
                .flat_map(|row| row.fan_out.iter().cloned())
                .filter(|id| !roots.contains(id))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if projection.partition(&ids) != s.impact
                || incoming != s.fan_in
                || outgoing != s.fan_out
            {
                return Err(bad());
            }
            let own_modules = projection.modules(roots.iter());
            if !count_matches(
                &s.metrics["fanInModules"],
                projection
                    .modules(incoming.iter())
                    .iter()
                    .filter(|m| !own_modules.contains(m))
                    .count(),
            ) || !count_matches(
                &s.metrics["fanOutModules"],
                projection
                    .modules(outgoing.iter())
                    .iter()
                    .filter(|m| !own_modules.contains(m))
                    .count(),
            ) {
                return Err(bad());
            }
            for metric in [
                "fanInSymbols",
                "fanOutSymbols",
                "fanInModules",
                "fanOutModules",
                "publicContractsAffected",
                "internalSymbolsAffected",
                "semanticSymbolsAffected",
                "modulesAffected",
                "affectedArtifacts",
                "affectedTests",
                "affectedTargets",
                "requiredChecks",
                "sharedMutableResources",
            ] {
                if s.metrics[metric].value().is_some()
                    && selected
                        .iter()
                        .any(|row| row.metrics[metric].value().is_none())
                {
                    return Err(bad());
                }
            }
        }
        if projection.modules(owned.iter()) != s.affected_modules {
            return Err(bad());
        }
        let cycle_count = replay_cycles
            .iter()
            .filter(|c| {
                if c.series == "symbols" {
                    c.members.iter().any(|m| owned.contains(m))
                } else {
                    c.modules.iter().any(|m| s.affected_modules.contains(m))
                }
            })
            .count();
        if !count_matches(&s.metrics["crossModuleCycles"], cycle_count) {
            return Err(bad());
        }
        if partitions.iter().any(|p| !sorted(p))
            || all.len() != partitions.iter().map(|p| p.len()).sum::<usize>()
            || s.measurement_basis != "aggregate-declared"
                && !all.contains(&s.subject.split('#').next().unwrap_or_default().to_owned())
            || s.metrics.len() != wire::METRICS.len()
            || wire::METRICS.iter().any(|m| !s.metrics.contains_key(*m))
        {
            return Err(bad());
        }
        for values in [
            &s.possible,
            &s.fan_in,
            &s.fan_out,
            &s.affected_modules,
            &s.artifacts,
            &s.tests,
            &s.targets,
            &s.checks,
            &s.resources,
            &s.shared_resources,
            &s.target_exposure,
            &s.replica_obligations,
            &s.import_edge_keys,
            &s.transaction_groups,
            &s.witness_refs,
            &s.gaps,
        ] {
            if !sorted(values) {
                return Err(bad());
            }
        }
        if s.possible.iter().any(|v| all.contains(v))
            || s.lower_bounds
                .keys()
                .any(|m| !wire::METRICS.contains(&m.as_str()))
            || s.lower_bounds
                .iter()
                .any(|(m, v)| s.metrics[m].value().is_some() || *v > 50_000)
        {
            return Err(bad());
        }
        if s.measurement_basis == "owner-conservative"
            && (!s.lower_bounds.is_empty()
                || s.metrics["publicContractsAffected"].value().is_some()
                || s.metrics["fanInSymbols"].value().is_some())
        {
            return Err(bad());
        }
        if !matches!(
            s.measurement_basis.as_str(),
            "exact-declared" | "owner-conservative" | "aggregate-declared"
        ) {
            return Err(bad());
        }
        for (metric, n) in [
            ("publicContractsAffected", s.impact.public_contracts.len()),
            ("internalSymbolsAffected", s.impact.internal_symbols.len()),
            ("semanticSymbolsAffected", all.len()),
            ("modulesAffected", s.affected_modules.len()),
            ("fanInSymbols", s.fan_in.len()),
            ("fanOutSymbols", s.fan_out.len()),
            ("affectedArtifacts", s.artifacts.len()),
            ("affectedTests", s.tests.len()),
            ("affectedTargets", s.targets.len()),
            ("requiredChecks", s.checks.len()),
            ("sharedMutableResources", s.shared_resources.len()),
            ("publicTargetExposure", s.target_exposure.len()),
            ("duplicationDivergence", s.replica_obligations.len()),
        ] {
            if !count_matches(&s.metrics[metric], n) {
                return Err(bad());
            }
            if s.lower_bounds.get(metric).is_some_and(|v| *v != n as u64) {
                return Err(bad());
            }
        }
        for id in &s.witness_refs {
            if !witness_map.contains_key(id) {
                return Err(bad());
            }
        }
        if s.transaction_scopes
            .iter()
            .map(|t| t.group.clone())
            .collect::<Vec<_>>()
            != s.transaction_groups
        {
            return Err(bad());
        }
        for scope in &s.transaction_scopes {
            let classified = !r.profile.domain_groups.is_empty()
                && scope.modules.iter().all(|m| {
                    r.profile
                        .domain_groups
                        .iter()
                        .any(|g| g.modules.contains(m))
                });
            let domains = r
                .profile
                .domain_groups
                .iter()
                .filter(|g| g.modules.iter().any(|m| scope.modules.contains(m)))
                .map(|g| g.id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if !sorted(&scope.modules)
                || !sorted(&scope.domain_groups)
                || scope.classified != classified
                || scope.domain_groups != domains
            {
                return Err(bad());
            }
        }
        if let Some(value) = s.metrics["transactionSpread"].value() {
            if r.profile.domain_groups.is_empty()
                || s.transaction_scopes.iter().any(|t| !t.classified)
                || *value
                    != s.transaction_scopes
                        .iter()
                        .filter(|t| t.domain_groups.len() > 1)
                        .count() as u64
            {
                return Err(bad());
            }
        }
        for id in s.impact.ids().chain(&s.possible) {
            if !s.witness_refs.iter().any(|key| {
                witness_map[key].subject == *id && witness_map[key].direction == "reverse"
            }) {
                return Err(bad());
            }
        }
        union_public.extend(s.impact.public_contracts.iter().cloned());
        union_internal.extend(s.impact.internal_symbols.iter().cloned());
        union_support.extend(s.impact.supporting_symbols.iter().cloned());
        union_unclassified.extend(s.impact.unclassified_symbols.iter().cloned());
    }
    if r.summary
        != (wire::Partition {
            public_contracts: union_public.into_iter().collect(),
            internal_symbols: union_internal.into_iter().collect(),
            supporting_symbols: union_support.into_iter().collect(),
            unclassified_symbols: union_unclassified.into_iter().collect(),
        })
    {
        return Err(bad());
    }
    if r.planning.public_contracts != r.summary.public_contracts
        || r.planning.internal_symbols != r.summary.internal_symbols
        || r.planning.review_overlap != r.summary.public_contracts
    {
        return Err(bad());
    }
    let counts: BTreeMap<_, _> = ["type", "entity", "operation", "event", "endpoint"]
        .into_iter()
        .map(|kind| {
            (
                kind.to_owned(),
                r.summary
                    .public_contracts
                    .iter()
                    .filter(|id| id.starts_with(&format!("{kind}:")))
                    .count() as u64,
            )
        })
        .collect();
    if r.public_contract_counts != counts || r.complete != r.planning.evidence_gaps.is_empty() {
        return Err(bad());
    }
    if !matches!(
        r.provenance.inputs.source_revision,
        State::Known { .. } | State::Unknown
    ) || r
        .provenance
        .inputs
        .source_revision
        .value()
        .is_some_and(|s| !crate::trace::id::is_revision(s))
        || !matches!(
            r.provenance.inputs.model_digest,
            State::Known { .. } | State::Unknown
        )
        || r.provenance
            .inputs
            .model_digest
            .value()
            .is_some_and(|s| !wire::is_digest(s))
    {
        return Err(bad());
    }
    for s in &r.suggestions {
        if s.applied
            || s.witness_refs.is_empty()
            || s.witness_refs
                .iter()
                .any(|id| !witness_map.contains_key(id))
        {
            return Err(bad());
        }
    }
    Ok(())
}

pub fn compare(
    base: &Report,
    candidate: &Report,
    baseline_bytes: &[u8],
) -> Result<Comparison, DiagnosticSet> {
    validate(base)?;
    validate(candidate)?;
    let mut reasons = vec![];
    if base.metric_version != candidate.metric_version
        || base.recipe != candidate.recipe
        || base.provenance.measurement_digest != candidate.provenance.measurement_digest
    {
        reasons.push("measurement-recipe".into());
    }
    if base.scope.kind != candidate.scope.kind
        || base.scope.selector != candidate.scope.selector
        || (base.scope.kind == "changed" && base.scope.roots != candidate.scope.roots)
    {
        reasons.push("scope-roots".into());
    }
    if base.provenance.inputs.project != candidate.provenance.inputs.project
        || base.provenance.inputs.model_version != candidate.provenance.inputs.model_version
    {
        reasons.push("project-contract".into());
    }
    if base.coverage != candidate.coverage {
        reasons.push("evidence-capabilities".into());
    }
    let comparable = reasons.is_empty();
    let old: BTreeMap<_, _> = base.subjects.iter().map(|s| (&s.subject, s)).collect();
    let new: BTreeMap<_, _> = candidate.subjects.iter().map(|s| (&s.subject, s)).collect();
    let ids: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    let mut rows = vec![];
    let mut regressions = 0;
    for id in ids {
        for metric in wire::METRICS {
            let (a, b) = (old.get(id), new.get(id));
            let av = a
                .map(|s| s.metrics[*metric].clone())
                .unwrap_or(State::Unknown);
            let bv = b
                .map(|s| s.metrics[*metric].clone())
                .unwrap_or(State::Unknown);
            let mut row = Delta {
                subject: id.clone(),
                metric: (*metric).into(),
                state: if a.is_none() {
                    "added"
                } else if b.is_none() {
                    "removed"
                } else {
                    "incomparable"
                }
                .into(),
                base: av.clone(),
                candidate: bv.clone(),
                absolute: State::Unknown,
                relative: State::Unknown,
                verdict: "indeterminate".into(),
                reason: if a.is_none() {
                    "subject-added"
                } else if b.is_none() {
                    "subject-removed"
                } else if !comparable {
                    "measurement-mismatch"
                } else {
                    "unavailable-evidence"
                }
                .into(),
            };
            if comparable && a.is_some() && b.is_some() {
                if let (Some(a), Some(b)) = (av.value(), bv.value()) {
                    let delta = i64::try_from(*b as i128 - *a as i128)
                        .map_err(|_| diagnostic::invalid("delta-overflow"))?;
                    row.state = "comparable".into();
                    row.absolute = State::known(delta);
                    row.verdict = "unchanged-or-allowed".into();
                    row.reason = if *a == 0 {
                        "zero-baseline"
                    } else if delta < 0 {
                        "decrease"
                    } else {
                        "none"
                    }
                    .into();
                    if *a > 0 && delta >= 0 {
                        row.relative = State::known(super::profile::Rational {
                            numerator: delta as u64,
                            denominator: *a,
                        });
                    }
                    let limits = candidate.profile.limits(new[id].module.as_str());
                    if let Some(limit) = limits
                        .regression_limits
                        .iter()
                        .find(|l| l.metric == *metric)
                    {
                        let absolute = delta > 0 && delta as u64 > limit.absolute_increase;
                        let relative = if let (
                            State::Known { value: actual },
                            State::Known { value: allowed },
                        ) = (&row.relative, &limit.relative_increase)
                        {
                            (actual.numerator as u128) * (allowed.denominator as u128)
                                > (allowed.numerator as u128) * (actual.denominator as u128)
                        } else {
                            false
                        };
                        if absolute || relative {
                            row.verdict = "regression".into();
                            regressions += 1;
                        }
                    }
                }
            }
            rows.push(row);
            if rows.len() > 20_000 {
                return Err(diagnostic::invalid("comparison-row-limit"));
            }
        }
    }
    Ok(Comparison {
        schema_version: format!("lekalo/coupling-comparison/v{}", wire::VERSION),
        identity: format!("dev.lekalo.coupling-comparison@{}", wire::VERSION),
        metric_version: wire::METRIC_VERSION.into(),
        baseline_digest: format!("sha256:{}", crate::digest::sha256_hex(baseline_bytes)),
        candidate_digest: wire::digest(candidate),
        comparable,
        rows,
        regressions,
        reasons,
    })
}

/// Strictness is selected only by this independent profile. A reviewed baseline
/// pin is mandatory; missing baselines never establish a fresh clean trend.
pub fn denial(profile: &Profile, report: &Report, comparison: Option<&Comparison>) -> Vec<String> {
    if profile.gate.mode != "strict" {
        return vec![];
    }
    let mut reasons = vec![];
    if comparison.is_none() {
        reasons.push("baseline-required".into());
    }
    if let Some(c) = comparison {
        if profile.gate.baseline_ready_ref.value() != Some(&c.baseline_digest) {
            reasons.push("baseline-review-pin".into());
        }
        if !c.comparable {
            reasons.push("baseline-incomparable".into());
        }
        let required_unknown = c.rows.iter().any(|row| {
            let module = report
                .subjects
                .iter()
                .find(|s| s.subject == row.subject)
                .map(|s| s.module.as_str())
                .unwrap_or_default();
            profile
                .limits(module)
                .regression_limits
                .iter()
                .any(|l| l.metric == row.metric)
                && row.state != "comparable"
        });
        if required_unknown {
            reasons.push("required-metric-incomparable".into());
        }
        if profile
            .gate
            .fail_on
            .iter()
            .any(|v| v == "baseline-regression")
            && c.regressions > 0
        {
            reasons.push("baseline-regression".into());
        }
    }
    if profile
        .gate
        .fail_on
        .iter()
        .any(|v| v == "required-incomplete")
        && (!report.complete
            || report
                .findings
                .iter()
                .any(|f| f.rule == "coupling.evidence-incomplete"))
    {
        reasons.push("required-incomplete".into());
    }
    if profile
        .gate
        .fail_on
        .iter()
        .any(|v| v == "threshold-exceeded")
        && report
            .findings
            .iter()
            .any(|f| f.detail == "configured-threshold-exceeded" && f.centrality.value().is_none())
    {
        reasons.push("threshold-exceeded".into());
    }
    reasons
}
